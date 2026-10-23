//! `var_translator.py`: the VarTranslator, which emits the storage of the
//! global variables and arrays.
//!
//! It emits only what belongs to one code bank, so the whole of `data_ast`
//! is walked once for each bank. Bank 0 is the resident variable area, which
//! is all an ordinary compilation has; a ZX Next program using CODEBANK gets
//! one more pass for each bank that holds data.

use super::inst::Item;
use crate::ast::*;
use crate::compiler::*;
use crate::pyval::crash;

impl Compiler {
    /// `_visit(node)` of `VarTranslator`.
    pub(super) fn var_visit(&mut self, node: NodeId, token: &str) {
        match token {
            "BLOCK" => self.tr_visit_block(node),
            "LABEL" => {
                if self.tr.bank != 0 {
                    return; // labels are always resident
                }
                let mangled = self.id(node).mangled.clone();
                self.ic_label(&mangled);
                crash("AttributeError: 'LabelRef' object has no attribute 'aliased_by'");
            }
            "VARDECL" => self.tr_visit_vardecl(node),
            "ARRAYDECL" => self.tr_visit_arraydecl(node),
            other => self.no_visit_method(other),
        }
    }

    /// `_should_emit`: whether the declaration belongs to this pass and
    /// survives it. Both are settled before anything is emitted, so that a
    /// pass neither warns twice about an unused variable nor takes a label
    /// for an array it is not going to emit. A bank is recorded as holding
    /// data only once something has survived for it.
    fn should_emit(&mut self, entry: NodeId) -> bool {
        if self.bank_of_var(Some(entry)) != self.tr.bank {
            return false;
        }
        if !self.accessed(entry) {
            let (lineno, name, fname) = (self.lineno(entry), self.id(entry).name.clone(), self.id(entry).filename.clone());
            self.warning_not_used(lineno, &name, "Variable", Some(&fname));
            if self.o_level() > 1 {
                return false; // unused variables are not compiled
            }
        }
        if self.tr.bank != 0 {
            self.banks_with_data.insert(self.tr.bank);
        }
        self.emit_line_marker(entry);
        true
    }

    /// `_emit_line_marker`: attributes the storage about to be emitted to
    /// its BASIC declaration. Only when banking is active, which is the only
    /// time the assembler has anything to say about it. The marker names the
    /// line before the declaration, as `#line N` makes the next line N+1.
    fn emit_line_marker(&mut self, entry: NodeId) {
        if !self.is_banking_active() {
            return;
        }
        let marker = format!("#line {} \"{}\"", self.lineno(entry).saturating_sub(1), self.id(entry).filename);
        self.ic_inline(&marker);
    }

    /// `_far_alias`: the alias label to lay down beside the entry's storage:
    /// only for bank-local storage that a FARPTR names.
    fn var_far_alias(&self, entry: NodeId) -> Option<String> {
        if self.tr.bank == 0 || !self.id(entry).r.has_faraddress {
            return None;
        }
        let name = if self.class_(entry) == Class::Array { self.data_label(entry) } else { self.id(entry).mangled.clone() };
        Some(format!("{name}{FAR_ADDR_SUFFIX}"))
    }

    /// `visit_VARDECL`.
    fn tr_visit_vardecl(&mut self, node: NodeId) {
        let entry = self.entry(node);
        if !self.should_emit(entry) {
            return;
        }
        let alias = self.var_far_alias(entry);
        let mangled = self.id(entry).mangled.clone();
        if let Some(addr) = self.id(entry).addr {
            let addr = self.traverse_const(addr).unwrap_or_else(|| "None".to_string());
            self.ic_deflabel(&mangled, &addr);
            if let Some(alias) = alias {
                self.ic_deflabel(&alias, &addr);
            }
            return;
        }
        if let Some(alias) = alias {
            self.ic_label(&alias);
        }
        let default_value = match &self.id(entry).r.kind {
            RefKind::Var(v) | RefKind::Array(v) => v.default_value,
            // A constant's value is not a default value: it has none.
            _ => crash(&format!("AttributeError: '{}' object has no attribute 'default_value'", self.token(entry))),
        };
        match default_value {
            None => {
                let size = self.size(entry);
                self.ic_var(&mangled, &size.to_string());
            }
            Some(value) => {
                let ty = self.ty_of(node);
                if self.token(value) == "CONSTEXPR" {
                    let text = self.traverse_const(value);
                    self.ic_varx(&mangled, ty.basic, &[text.into()]);
                } else {
                    let bytes = self.default_value(ty, value);
                    self.ic_vard(&mangled, &bytes);
                }
            }
        }
    }

    /// `visit_ARRAYDECL`.
    fn tr_visit_arraydecl(&mut self, node: NodeId) {
        let entry = self.entry(node);
        let init = self.array_init(entry);
        let addr = self.id(entry).addr;
        if init.is_some() && addr.is_some() {
            crash("AssertionError: Cannot use address and default_value at once");
        }
        if !self.should_emit(entry) {
            return;
        }
        let mangled = self.id(entry).mangled.clone();
        let lbound_label = format!("{mangled}.__LBOUND__");
        let ubound_label = format!("{mangled}.__UBOUND__");
        let array = self.id(entry).var().unwrap().clone();
        // NULL by default.
        let mut bound_ptrs = ["0".to_string(), "0".to_string()];
        if !self.is_zero_based(entry) && (array.is_dynamically_accessed || array.lbound_used) {
            bound_ptrs[0] = lbound_label.clone();
        }
        if array.ubound_used || self.opts.bool("array_check") {
            bound_ptrs[1] = ubound_label.clone();
        }
        let bound_items: Vec<Item> = bound_ptrs.iter().map(|b| Item::Str(b.clone())).collect();

        let data_label = self.data_label(entry);
        let idx_table_label = self.tmp_label();
        let bounds: Vec<(i128, i128)> = self
            .children(self.bounds(entry))
            .iter()
            .flatten()
            .map(|&b| match self.node(b).kind {
                Kind::Bound { lower, upper } => (lower, upper),
                _ => crash("AttributeError: not a BOUND"),
            })
            .collect();
        let ty = self.ty_of(node);
        // The number of dimensions less one, the size of each but the
        // first, and the size of an element.
        let mut l: Vec<Item> = vec![format!("{:04X}", bounds.len() as i128 - 1).into()];
        l.extend(bounds.iter().skip(1).map(|b| Item::Str(format!("{:04X}", b.1 - b.0 + 1))));
        l.push(format!("{:02X}", ty.size()).into());

        let mut arr_data: Vec<Item> = Vec::new();
        let alias = self.var_far_alias(entry);
        if let Some(addr) = addr {
            let addr = self.traverse_const(addr).unwrap_or_else(|| "None".to_string());
            self.ic_deflabel(&data_label, &addr);
            if let Some(alias) = &alias {
                self.ic_deflabel(alias, &addr);
            }
        } else if let Some(init) = &init {
            arr_data = self.array_default_value(ty, init);
        } else {
            arr_data = vec![Item::Str("00".to_string()); self.size(node) as usize];
        }

        self.ic_varx(&mangled, PTR_TYPE, &[idx_table_label.clone().into()]);

        let data_ptr_label = self.data_ptr_label(entry);
        if let Some(addr) = addr {
            let text = self.traverse_const(addr);
            self.ic_varx(&data_ptr_label, PTR_TYPE, &[text.into()]);
            self.ic_data(PTR_TYPE, &bound_items);
        } else {
            self.ic_varx(&data_ptr_label, PTR_TYPE, &[data_label.clone().into()]);
            self.ic_data(PTR_TYPE, &bound_items);
            if let Some(alias) = &alias {
                self.ic_label(alias);
            }
            self.ic_vard(&data_label, &arr_data);
        }

        self.ic_vard(&idx_table_label, &l);
        if bound_ptrs[0] != "0" {
            let l: Vec<Item> = bounds.iter().map(|b| Item::Str(format!("{:04X}", b.0))).collect();
            self.ic_vard(&lbound_label, &l);
        }
        if bound_ptrs[1] != "0" {
            let l: Vec<Item> = bounds.iter().map(|b| Item::Str(format!("{:04X}", b.1))).collect();
            self.ic_vard(&ubound_label, &l);
        }
    }
}
