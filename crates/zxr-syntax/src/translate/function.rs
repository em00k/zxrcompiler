//! `function_translator.py`: the FunctionTranslator, which translates the
//! bodies of the functions the main body left in `gl.FUNCTIONS`, and on the
//! ZX Next sorts them into their code banks.

use super::inst::Item;
use super::{runtime, LabelledData};
use crate::ast::*;
use crate::compiler::*;
use crate::pyval::crash;
use crate::types::BasicType;

impl Compiler {
    /// `_local_array_load`.
    fn local_array_load(&mut self, scope: Scope, local_var: NodeId) {
        let t2 = self.new_t();
        match scope {
            Scope::Parameter => self.ic_pload(PTR_TYPE, &t2, &(self.offset(local_var) - PTR_TYPE.size()).to_string()),
            Scope::Local => self.ic_pload(PTR_TYPE, &t2, &(-(self.offset(local_var) - PTR_TYPE.size())).to_string()),
            Scope::Global => {}
        }
        self.ic_fparam(PTR_TYPE, &t2);
    }

    /// `drain(queue)` of `start()`, on `self.functions`: translates the
    /// resident functions and sets the banked ones aside.
    fn drain_functions(&mut self, banked: &mut Vec<(i64, Vec<NodeId>)>) {
        while !self.functions.is_empty() {
            let f = self.functions.remove(0);
            let bank = self.bank_of_func(f);
            if bank != 0 {
                Self::bank_queue(banked, bank).push(f);
                continue;
            }
            self.tr_visit(Some(f));
        }
    }

    /// `banked[bank]` of a `defaultdict(list)`.
    fn bank_queue(banked: &mut Vec<(i64, Vec<NodeId>)>, bank: i64) -> &mut Vec<NodeId> {
        let index = match banked.iter().position(|(b, _)| *b == bank) {
            Some(index) => index,
            None => {
                banked.push((bank, Vec::new()));
                banked.len() - 1
            }
        };
        &mut banked[index].1
    }

    /// `FunctionTranslator.start()`: translates every pending function,
    /// resident ones first. Banked routines follow bank by bank, each group
    /// fenced by a `CODEBANK n` assembler directive, and the run of banks is
    /// closed with `CODEBANK 0`, so that what is emitted afterwards (deferred
    /// bound tables, DATA blocks, strings, the runtime library) lands in the
    /// resident program again.
    ///
    /// A module-level asm block diverted into a bank is replayed inside that
    /// bank's fence, ahead of its routines.
    pub(super) fn function_translator_start(&mut self) {
        let mut banked: Vec<(i64, Vec<NodeId>)> = Vec::new();
        self.drain_functions(&mut banked);

        // Module-level asm blocks the Translator diverted into a bank. A
        // bank may hold nothing else, and still needs a fence and an entry
        // in __CODE_BANK_TABLE, like a bank holding only variables.
        let diverted_banks: Vec<i64> = self.diverted.iter().map(|(bank, _)| *bank).collect();
        self.banks_with_data.extend(diverted_banks);
        // The Python compiler leaves the far-call runtime out when no bank
        // holds a routine, and then FarPeek and the rest of farmem.asm do not
        // assemble: they read the bank the program is running in, which the
        // runtime keeps. Here the runtime comes in when the asm asks for it.
        if banked.is_empty() {
            self.far_memory_used = self.nodes.iter().any(|node| matches!(&node.kind, Kind::Asm { asm, .. } if asm.contains("__FAR_CUR_BANK")));
            if self.far_memory_used {
                self.require_far_runtime();
            }
        }
        if banked.is_empty() && self.diverted.is_empty() {
            return;
        }
        if !banked.is_empty() {
            // A bank holding only diverted asm has no routine to call, so it
            // must not bring in the far-call runtime.
            self.prepare_far_calls(&banked);
        }

        let mut emitted: Vec<i64> = Vec::new();
        loop {
            // Worked out again each time round: visiting a bank can discover
            // a nested FUNCDECL of a bank not seen yet.
            let mut todo: Vec<i64> = banked.iter().map(|(bank, _)| *bank).chain(self.diverted.iter().map(|(bank, _)| *bank)).filter(|bank| !emitted.contains(bank)).collect();
            todo.sort();
            let Some(&bank) = todo.first() else { break };
            emitted.push(bank);

            self.ic_inline(&format!("CODEBANK {bank}"));
            let nodes: Vec<NodeId> = self.diverted.iter().find(|(b, _)| *b == bank).map(|(_, nodes)| nodes.clone()).unwrap_or_default();
            for node in nodes {
                if self.token(node) == "LABEL" {
                    let mangled = self.id(node).mangled.clone();
                    self.ic_label(&mangled);
                    // A FARPTR naming this label needs a name the resident
                    // program may hold; see far_alias.
                    if self.id(node).r.has_faraddress {
                        self.ic_label(&format!("{mangled}{FAR_ADDR_SUFFIX}"));
                    }
                } else {
                    self.emit_asm_node(node);
                }
            }

            // banked.pop(bank, [])
            let mut queue: Vec<NodeId> = match banked.iter().position(|(b, _)| *b == bank) {
                Some(index) => banked.remove(index).1,
                None => Vec::new(),
            };
            while !queue.is_empty() {
                let f = queue.remove(0);
                self.tr_visit(Some(f));
                // The visit may have discovered nested FUNCDECLs. What is
                // for this bank joins the queue; what is for another bank
                // goes there, and resident ones wait until the banks close.
                while !self.functions.is_empty() {
                    let g = self.functions.remove(0);
                    let g_bank = self.bank_of_func(g);
                    if g_bank == bank {
                        queue.push(g);
                    } else if g_bank != 0 {
                        Self::bank_queue(&mut banked, g_bank).push(g);
                    } else {
                        self.tr.deferred_resident.push(g);
                    }
                }
            }
        }

        self.ic_inline("CODEBANK 0");

        let mut diverted = self.diverted.clone();
        diverted.sort_by_key(|(bank, _)| *bank);
        for (bank, nodes) in diverted {
            let first = nodes[0];
            let filename = match &self.node(first).kind {
                Kind::Asm { filename, .. } => Some(filename.clone()),
                Kind::Id(data) => Some(data.filename.clone()),
                _ => None,
            };
            self.warn_if_no_routines(bank, self.lineno(first), filename.as_deref());
        }

        // drain(self._deferred_resident). Nested functions found now go to
        // self.functions, which nothing looks at again.
        while !self.tr.deferred_resident.is_empty() {
            let f = self.tr.deferred_resident.remove(0);
            let bank = self.bank_of_func(f);
            if bank != 0 {
                Self::bank_queue(&mut banked, bank).push(f);
                continue;
            }
            self.tr_visit(Some(f));
        }

        for bound_table in std::mem::take(&mut self.tr.deferred_bound_tables) {
            let data: Vec<Item> = bound_table.data.into_iter().map(Item::Str).collect();
            self.ic_vard(&bound_table.label, &data);
        }
    }

    /// `_prepare_far_calls`: the resident trampolines, and the far-call
    /// runtime they need.
    fn prepare_far_calls(&mut self, banked: &[(i64, Vec<NodeId>)]) {
        let mut banked = banked.to_vec();
        banked.sort_by_key(|(bank, _)| *bank);
        for (bank, functions) in banked {
            for f in functions {
                let mangled = self.id(f).mangled.clone();
                if self.backend_inits.contains(&mangled) {
                    let (lineno, name) = (self.lineno(f), self.id(f).name.clone());
                    let msg = format!("'{name}' is used as an #init routine and cannot live in a CODEBANK, because it would run before the far-call runtime is initialised");
                    self.error(lineno, &msg);
                    continue;
                }
                self.banks_used.insert(bank);
                if self.is_private(f) {
                    continue; // called only from its own bank, directly
                }
                let body = Self::banked_label(&mangled);
                self.far_trampolines.insert(&mangled, body.clone());
                self.ic_inline(&format!("{mangled}:\ncall {}\nDEFB {bank}\nDEFW {body}", runtime::FAR_CALL));
            }
        }
        if !self.is_banking_active() {
            return;
        }
        self.require_far_runtime();
    }

    /// The far-call runtime, and what it needs of the program.
    fn require_far_runtime(&mut self) {
        let module = runtime::required_module(runtime::FAR_CALL).unwrap_or_else(|| crash("KeyError: '.core.__FAR_CALL'"));
        self.requires.insert(module.to_string());
        self.backend_inits.insert(runtime::FAR_INIT.to_string());

        // __EXIT_FUNCTION is emitted inline inside the first function whose
        // parameter block is over 11 bytes, and every later one jumps to it.
        // Were that first function banked, the shared label would end up
        // inside a bank. It is forced into the resident epilogue instead.
        if !self.flag_use_function_exit {
            self.flag_use_function_exit = true;
            self.at_end.extend(["__EXIT_FUNCTION:", "ld sp, ix", "pop ix", "pop de", "add hl, sp", "ld sp, hl", "push de", "exx", "ret"].map(String::from));
        }
    }

    /// `local_symbol_table.values()` of a function.
    fn local_symbols(&self, func: NodeId) -> Vec<NodeId> {
        match self.id(func).func().and_then(|f| f.local_symbol_table) {
            Some(scope) => self.scope_values(scope, true),
            None => crash("AttributeError: 'NoneType' object has no attribute 'values'"),
        }
    }

    /// `local_var.default_value`, for whatever kind of symbol a local is.
    fn local_default_value(&self, local_var: NodeId) -> Option<NodeId> {
        match &self.id(local_var).r.kind {
            RefKind::Var(v) | RefKind::Array(v) => v.default_value,
            _ => crash(&format!("AttributeError: '{}' object has no attribute 'default_value'", self.token(local_var))),
        }
    }

    /// `visit_FUNCTION`: one function, from its label to its `leave`.
    pub(super) fn tr_visit_function(&mut self, node: NodeId) {
        let mut bound_tables: Vec<LabelledData> = Vec::new();
        let bank = self.bank_of_func(node);
        let mangled = self.id(node).mangled.clone();
        let func = self.id(node).func().unwrap().clone();

        // The body of a banked routine lives under a label of its own; the
        // ordinary one is the resident trampoline's, so that a call needs
        // nothing special.
        // A private one has no trampoline, and keeps its name.
        self.ic_label(&if bank != 0 && !func.private { Self::banked_label(&mangled) } else { mangled.clone() });
        if func.convention == Convention::Fastcall {
            self.ic_enter("__fastcall__");
        } else {
            self.ic_enter(&func.locals_size.to_string());
        }

        for local_var in self.local_symbols(node) {
            if !self.accessed(local_var) {
                // This should never happen, as values() is filtered already.
                let (lineno, name) = (self.lineno(local_var), self.name(local_var).to_string());
                self.warning_not_used(lineno, &name, "Variable", None);
            }
            let (class, scope) = (self.class_(local_var), self.scope(local_var));
            if class == Class::Const || scope == Scope::Parameter {
                continue;
            }
            if class == Class::Array && scope == Scope::Local {
                let var_mangled = self.id(local_var).mangled.clone();
                let lbound_label = format!("{var_mangled}.__LBOUND__");
                let ubound_label = format!("{var_mangled}.__UBOUND__");
                let array = self.id(local_var).var().unwrap().clone();
                let array_check = self.opts.bool("array_check");
                let lbound_needed = !self.is_zero_based(local_var) && (array.is_dynamically_accessed || array.lbound_used || array_check);
                let ubound_needed = array.ubound_used || array_check;
                let bound_ptrs: Vec<Item> = vec![if lbound_needed { lbound_label.clone() } else { "0".to_string() }.into(), if ubound_needed { ubound_label.clone() } else { "0".to_string() }.into()];
                if lbound_needed || ubound_needed {
                    self.define("__ZXB_USE_LOCAL_ARRAY_WITH_BOUNDS__", "");
                }
                let bounds: Vec<(i128, i128)> = self
                    .children(self.bounds(local_var))
                    .iter()
                    .flatten()
                    .map(|&b| match self.node(b).kind {
                        Kind::Bound { lower, upper } => (lower, upper),
                        _ => crash("AttributeError: not a BOUND"),
                    })
                    .collect();
                if lbound_needed {
                    bound_tables.push(LabelledData { label: lbound_label, data: bounds.iter().map(|b| format!("{:04X}", b.0)).collect() });
                }
                if ubound_needed {
                    bound_tables.push(LabelledData { label: ubound_label, data: bounds.iter().map(|b| format!("{:04X}", b.1)).collect() });
                }
                // The number of dimensions less one, then the size of each
                // dimension but the first: two bytes each, of which the high
                // one is always written as zero.
                let mut l: Vec<i128> = vec![bounds.len() as i128 - 1];
                l.extend(bounds.iter().skip(1).map(|b| b.1 - b.0 + 1));
                let mut q: Vec<Item> = Vec::new();
                for x in l {
                    q.push(format!("{:02X}", x & 0xFF).into());
                    q.push(format!("{:02X}", (x & 0xFF) >> 8).into());
                }
                let ty = self.ty_of(local_var);
                q.push(format!("{:02X}", ty.size()).into());
                let mut r: Vec<Item> = Vec::new();
                if let Some(init) = self.array_init(local_var) {
                    r.extend(self.array_default_value(ty, &init));
                }
                // Sets up the array and its bounds.
                let (offset, size) = (self.offset_str(local_var), self.size(local_var));
                self.ic_larrd(&offset, &q, &size.to_string(), &r, &bound_ptrs);
            } else if self.token(local_var) != "FUNCTION" {
                // Local variables start as 0, so for 0 there is nothing to do.
                let Some(default_value) = self.local_default_value(local_var) else { continue };
                // `default_value != 0`: a number compares by value, a string
                // cannot be compared with a number at all, and any other
                // node is simply not 0.
                match &self.node(default_value).kind {
                    Kind::Number { value } if value.is_zero() => continue,
                    Kind::Str { .. } => crash("AssertionError: isinstance(other, SymbolSTRING)"),
                    _ => {}
                }
                let ty = self.ty_of(local_var);
                let offset = self.offset_str(local_var);
                if self.token(default_value) == "CONSTEXPR" {
                    let text = self.traverse_const(default_value);
                    self.ic_lvarx(ty.basic, &offset, &[text.into()]);
                } else {
                    let q = self.default_value(ty, default_value);
                    self.ic_lvard(&offset, &q);
                }
            }
        }

        // `for i in node.ref.body`: the statements, by index, as the body is
        // when each is reached.
        let body = self.body(node);
        let mut i = 0;
        while i < self.children(body).len() {
            let statement = self.children(body)[i];
            self.tr_visit(statement);
            i += 1;
        }

        self.ic_label(&format!("{mangled}__leave"));

        // Now any local string is freed.
        let mut preserve_hl = false;
        if func.convention == Convention::Stdcall {
            for local_var in self.local_symbols(node) {
                let scope = self.scope(local_var);
                let class = self.class_(local_var);
                let is_string = self.ty_of(local_var) == BasicType::String;
                let owned = |c: &Compiler| scope == Scope::Local || (scope == Scope::Parameter && !c.byref(local_var));
                if is_string {
                    if class == Class::Const || self.token(local_var) == "FUNCTION" {
                        continue;
                    }
                    if class != Class::Array {
                        // A string: just free it.
                        if owned(self) {
                            if !preserve_hl {
                                preserve_hl = true;
                                self.ic_exchg();
                            }
                            let offset = if scope == Scope::Local { -self.offset(local_var) } else { self.offset(local_var) };
                            let t = self.t(local_var);
                            self.ic_fpload(BasicType::String, &t, &offset.to_string());
                            self.runtime_call(runtime::MEM_FREE, 0);
                        }
                    } else if owned(self) {
                        // An array of strings is freed, unless it came ByRef.
                        if !preserve_hl {
                            preserve_hl = true;
                            self.ic_exchg();
                        }
                        self.ic_param(BOUND_TYPE, &self.count(local_var).to_string());
                        self.local_array_load(scope, local_var);
                        self.runtime_call(runtime::ARRAYSTR_FREE_MEM, 0);
                    }
                }
                if class == Class::Array && !is_string && owned(self) {
                    if !preserve_hl {
                        preserve_hl = true;
                        self.ic_exchg();
                    }
                    self.local_array_load(scope, local_var);
                    self.runtime_call(runtime::MEM_FREE, 0);
                }
            }
        }
        if preserve_hl {
            self.ic_exchg();
        }

        // The convention is read again: it does not change during the visit.
        if func.convention == Convention::Fastcall {
            self.ic_leave(func.convention.name());
        } else {
            let params_size = self.size(self.params(node));
            self.ic_leave(&params_size.to_string());
        }

        if bank != 0 {
            // These tables come after the leave, so in a banked routine they
            // would land inside the bank, and a ByRef array passed to a
            // routine of another bank would have LBOUND()/UBOUND() read a
            // table that is not paged in. They are kept resident.
            self.tr.deferred_bound_tables.extend(bound_tables);
        } else {
            for bound_table in bound_tables {
                let data: Vec<Item> = bound_table.data.into_iter().map(Item::Str).collect();
                self.ic_vard(&bound_table.label, &data);
            }
        }
    }
}
