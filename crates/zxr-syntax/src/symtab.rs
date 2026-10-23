//! The symbol table: `src/api/symboltable/scope.py` and `symboltable.py`.
//!
//! Scopes outlive the function they belong to (the FUNCTION symbol keeps its
//! `local_symbol_table`), so they live in an arena of their own and the table
//! is a stack of indices into it.

use crate::ast::*;
use crate::compiler::*;
use crate::ordmap::OrdMap;
use crate::pyval::crash;
use crate::types::{BasicType, Ty};

/// `Scope`: the symbols of one function, or the global ones.
#[derive(Clone, Debug, Default)]
pub struct ScopeData {
    pub symbols: OrdMap<NodeId>,
    /// Lower-cased names of the symbols declared case-insensitive.
    pub caseins: OrdMap<NodeId>,
    pub namespace: String,
    /// The FUNCDECL owning the scope.
    pub owner: Option<NodeId>,
    pub parent_scope: Option<ScopeId>,
    pub parent_namespace: Option<String>,
}

/// `DEPRECATED_SUFFIXES` and `SUFFIX_TYPE`.
pub fn suffix_type(id: &str) -> Option<(char, BasicType)> {
    match id.chars().last() {
        Some('$') => Some(('$', BasicType::String)),
        Some('%') => Some(('%', BasicType::Integer)),
        Some('&') => Some(('&', BasicType::Long)),
        _ => None,
    }
}

/// What the lexer asks of the symbol table for a name (`t_ID`): whether it is
/// declared at all and, if so, whether as an array. A name with no entry may
/// be a label; one with an entry is `ARRAY_ID` or `ID`. The entry is tested
/// for truth, and the type `unknown` is the one entry that is false.
pub fn lexer_lookup(nodes: &[Node], scopes: &[ScopeData], table: &[ScopeId], id: &str) -> Option<bool> {
    let id = strip_suffix(id);
    let lower = id.to_lowercase();
    let entry = table.iter().rev().find_map(|&s| {
        let data = &scopes[s.0 as usize];
        data.symbols.get(id).or_else(|| data.caseins.get(&lower)).copied()
    })?;
    match &nodes[entry.0 as usize].kind {
        Kind::Id(data) => Some(data.class() == Class::Array),
        Kind::Type(BasicType::Unknown) => None,
        _ => Some(false),
    }
}

fn strip_suffix(id: &str) -> &str {
    if suffix_type(id).is_some() { &id[..id.len() - 1] } else { id }
}

impl Compiler {
    /// `SymbolTable.__init__`: the global scope, holding the basic types.
    pub(crate) fn init_symbol_table(&mut self) {
        self.scopes.push(ScopeData::default());
        self.table.push(ScopeId(0));
        self.namespaces.insert("", ScopeId(0));
        // `for type_ in TYPE.types` walks a set of IntEnum members, which
        // CPython keeps in value order.
        for basic in BasicType::ALL {
            let node = self.new_node(Kind::Type(basic), 0, &[]);
            self.scope_set(ScopeId(0), basic.name(), node);
            self.basic_types.push(node);
        }
    }

    /// `SYMBOL_TABLE.basic_types[type_]` as a type.
    pub fn basic_type(&self, basic: BasicType) -> Ty {
        Ty::new(basic)
    }

    pub fn current_scope(&self) -> ScopeId {
        *self.table.last().unwrap()
    }

    pub fn global_scope(&self) -> ScopeId {
        self.table[0]
    }

    pub fn scope_data(&self, scope: ScopeId) -> &ScopeData {
        &self.scopes[scope.0 as usize]
    }

    pub fn scope_data_mut(&mut self, scope: ScopeId) -> &mut ScopeData {
        &mut self.scopes[scope.0 as usize]
    }

    /// `scope[key]`.
    pub fn scope_get(&self, scope: ScopeId, key: &str) -> Option<NodeId> {
        let data = self.scope_data(scope);
        data.symbols.get(key).or_else(|| data.caseins.get(&key.to_lowercase())).copied()
    }

    /// `scope[key] = value`.
    pub fn scope_set(&mut self, scope: ScopeId, key: &str, value: NodeId) {
        let caseins = match &self.node(value).kind {
            Kind::Id(data) => data.caseins,
            // A type takes the option as it was when the type was made:
            // before any option is read, so never.
            _ => false,
        };
        let data = self.scope_data_mut(scope);
        data.symbols.insert(key, value);
        if caseins {
            data.caseins.insert(&key.to_lowercase(), value);
        }
    }

    /// `del scope[key]`. The case-insensitive table is keyed by the lowered
    /// name and Python pops the name as given, so an entry stays behind there
    /// unless the name is already lower case.
    pub fn scope_del(&mut self, scope: ScopeId, key: &str) {
        let data = self.scope_data_mut(scope);
        data.symbols.remove(key);
        data.caseins.remove(key);
    }

    /// `scope.values(filter_by_opt)`: above `-O1` only the symbols accessed.
    pub fn scope_values(&self, scope: ScopeId, filter_by_opt: bool) -> Vec<NodeId> {
        let filter = filter_by_opt && self.o_level() > 1;
        self.scope_data(scope).symbols.values().copied().filter(|&v| !filter || self.accessed(v)).collect()
    }

    /// `scope.keys(filter_by_opt)`.
    pub fn scope_keys(&self, scope: ScopeId, filter_by_opt: bool) -> Vec<String> {
        let filter = filter_by_opt && self.o_level() > 1;
        self.scope_data(scope).symbols.iter().filter(|&(_, &v)| !filter || self.accessed(v)).map(|(k, _)| k.to_string()).collect()
    }

    /// `SymbolTable.get_entry`.
    pub fn get_entry(&mut self, id: &str, scope: Option<ScopeId>, lineno: Option<u32>) -> Option<NodeId> {
        if id.is_empty() {
            crash("IndexError: string index out of range");
        }
        let suffix = suffix_type(id);
        let id = strip_suffix(id);
        let entry = match scope {
            Some(scope) => self.scope_get(scope, id),
            None => self.table.iter().rev().find_map(|&s| self.scope_get(s, id)),
        };
        if let (Some(entry), Some((suffix, basic)), Some(lineno)) = (entry, suffix, lineno) {
            if self.is_id(entry) {
                if let Some(ty) = self.node(entry).ty {
                    if ty != basic {
                        if suffix == '$' {
                            self.error(lineno, "Invalid $ suffix (or sigil) for numeric variable");
                        } else {
                            self.error(lineno, &format!("Invalid {suffix} suffix (or sigil) for {} variable", ty.name()));
                        }
                    }
                }
            }
        }
        entry
    }

    /// `SymbolTable.get_existing_entry`.
    pub fn get_existing_entry(&mut self, id: &str, scope: Option<ScopeId>, lineno: Option<u32>) -> NodeId {
        self.get_entry(id, scope, lineno).unwrap_or_else(|| crash("AssertionError: result is not None"))
    }

    /// `SymbolTable.declare`: enters `entry` in the current scope under the
    /// name, unless the name is taken there.
    pub fn declare(&mut self, id: &str, lineno: u32, entry: NodeId) -> Option<NodeId> {
        let id2 = strip_suffix(id);
        let mut ty = self.node(entry).ty;
        if let Some((_, basic)) = suffix_type(id) {
            // The suffix decides the type.
            let suffix_ty = Ty::reference(basic, lineno, false);
            if let Some(entry_ty) = ty {
                if !entry_ty.implicit && suffix_ty != entry_ty {
                    self.error(lineno, &format!("expected type {} for '{id}', got {}", suffix_ty.name(), entry_ty.name()));
                }
            }
            ty = Some(suffix_ty);
        }
        let current = self.current_scope();
        if self.scope_get(current, id2).is_some() {
            return None;
        }
        self.id_mut(entry).caseins = self.opts.bool("case_insensitive");
        self.scope_set(current, id2, entry);
        let mangled = Self::make_child_namespace(&self.current_namespace, id2);
        let is_global = current == self.global_scope() || self.class_(entry) == Class::Label;
        let data = self.id_mut(entry);
        data.name = id2.to_string();
        data.mangled = mangled;
        data.scope_ref = Some(current);
        data.scope = if is_global { Scope::Global } else { Scope::Local };
        self.node_mut(entry).ty = ty;
        Some(entry)
    }

    /// `SymbolTable.declare_safe`.
    pub fn declare_safe(&mut self, id: &str, lineno: u32, entry: NodeId) -> NodeId {
        self.declare(id, lineno, entry).unwrap_or_else(|| crash("AssertionError: result is not None"))
    }

    /// `SymbolTable.make_child_namespace`.
    pub fn make_child_namespace(parent_namespace: &str, child_namespace: &str) -> String {
        if parent_namespace.is_empty() {
            format!("{MANGLE_CHR}{child_namespace}")
        } else {
            format!("{parent_namespace}{NAMESPACE_SEPARATOR}{child_namespace}")
        }
    }

    // ------------------------------------------------------------------
    // Symbol Table Checks
    // ------------------------------------------------------------------

    /// `SymbolTable.check_is_declared`. `classname` is the word used for the
    /// thing in the message.
    pub fn check_is_declared(&mut self, id: &str, lineno: u32, classname: &str, scope: Option<ScopeId>, show_error: bool) -> bool {
        let result = self.get_entry(id, scope, Some(lineno));
        if let Some(entry) = result {
            if !self.is_id(entry) {
                return true; // a type
            }
            if self.id(entry).declared {
                return true;
            }
        }
        if show_error {
            let fname = result.map(|e| self.id(e).filename.clone());
            self.error_at(lineno, &format!("Undeclared {classname} \"{id}\""), fname.as_deref());
        }
        false
    }

    /// `SymbolTable.check_is_undeclared`.
    pub fn check_is_undeclared(&mut self, id: &str, lineno: u32, classname: &str, scope: Option<ScopeId>, show_error: bool) -> bool {
        let result = self.get_entry(id, scope, Some(lineno));
        match result {
            Some(entry) if self.is_id(entry) && self.id(entry).declared => {}
            // A type has no `declared` attribute.
            Some(entry) if !self.is_id(entry) => crash("AttributeError: 'SymbolBASICTYPE' object has no attribute 'declared'"),
            _ => return true,
        }
        let scope = scope.unwrap_or_else(|| self.current_scope());
        if show_error {
            // Looked up under the name as written, suffix and all.
            let previous = self.scope_get(scope, id).unwrap_or_else(|| crash("AttributeError: 'NoneType' object has no attribute 'filename'"));
            let msg = format!("Duplicated {classname} \"{id}\" (previous one at {}:{})", self.id(previous).filename, self.lineno(previous));
            self.error(lineno, &msg);
        }
        false
    }

    /// `SymbolTable.check_class`: the name is free, of unknown class, or of
    /// the class given.
    pub fn check_class_id(&mut self, id: &str, class: Class, lineno: u32, scope: Option<ScopeId>, show_error: bool) -> bool {
        let Some(entry) = self.get_entry(id, scope, Some(lineno)) else { return true };
        let entry_class = self.class_(entry);
        if entry_class == Class::Unknown || entry_class == class {
            return true;
        }
        if show_error {
            self.check_class(entry, class, lineno);
        }
        false
    }

    // ------------------------------------------------------------------
    // Scope Management
    // ------------------------------------------------------------------

    /// `SymbolTable.enter_scope`.
    pub fn enter_scope(&mut self, namespace: &str) {
        self.current_namespace = Self::make_child_namespace(&self.current_namespace, namespace);
        let parent = self.current_scope();
        let scope = ScopeId(self.scopes.len() as u32);
        self.scopes.push(ScopeData {
            namespace: self.current_namespace.clone(),
            parent_scope: Some(parent),
            parent_namespace: Some(self.scope_data(parent).namespace.clone()),
            ..ScopeData::default()
        });
        self.table.push(scope);
        let namespace = self.current_namespace.clone();
        self.namespaces.insert(&namespace, scope);
        let saved = std::mem::take(&mut self.loops);
        self.meta_loops.push(saved);
    }

    /// `entry_size` of `compute_offsets`: what a local takes on the stack.
    fn entry_size(&self, entry: NodeId) -> i64 {
        let Kind::Id(data) = &self.node(entry).kind else { crash("AttributeError: 'SymbolBASICTYPE' object has no attribute 'scope'") };
        if data.scope == Scope::Global || data.addr.is_some() || data.class() == Class::Const {
            return 0;
        }
        if data.class() != Class::Array { self.size(entry) } else { self.memsize(entry) }
    }

    /// `SymbolTable.compute_offsets`: lays the locals out, smallest first.
    pub fn compute_offsets(&mut self, scope: ScopeId) -> i64 {
        let mut entries = self.scope_values(scope, true);
        // sorted() is stable, and so is this.
        entries.sort_by_key(|&e| self.entry_size(e));
        let mut offset = 0;
        for entry in entries {
            let (class, scope) = (self.class_(entry), self.id(entry).scope);
            if matches!(class, Class::Function | Class::Label | Class::Type) {
                continue;
            }
            if class == Class::Var && scope == Scope::Local {
                offset += self.entry_size(entry);
                self.id_mut(entry).r.offset = Some(offset);
            }
            if class == Class::Array && scope == Scope::Local {
                offset += self.entry_size(entry);
                self.id_mut(entry).r.offset = Some(offset);
            }
        }
        offset
    }

    /// `SymbolTable.leave_scope`: ends a function body. Returns the size of
    /// its locals.
    pub fn leave_scope(&mut self, show_warnings: bool) -> i64 {
        let current = self.current_scope();
        for v in self.scope_values(current, false) {
            if !self.accessed(v) && self.id(v).scope == Scope::Parameter {
                // Parameters are always there, used or not.
                self.set_accessed(v, true);
                // A ByRef one may be how a value is returned.
                let byref = self.id(v).var().is_some_and(|r| r.byref);
                if show_warnings && !byref {
                    let (lineno, name, fname) = (self.lineno(v), self.id(v).name.clone(), self.id(v).filename.clone());
                    self.warning_not_used(lineno, &name, "Parameter", Some(&fname));
                }
            }
        }
        for entry in self.scope_values(current, true) {
            if self.class_(entry) == Class::Unknown {
                let name = self.id(entry).name.clone();
                self.move_to_global_scope(&name);
            }
        }
        let offset = self.compute_offsets(current);
        self.current_namespace = self.scope_data(current).parent_namespace.clone().unwrap_or_default();
        self.table.pop();
        self.loops = self.meta_loops.pop().unwrap_or_else(|| crash("IndexError: pop from empty list"));
        offset
    }

    /// `SymbolTable.move_to_global_scope`.
    pub fn move_to_global_scope(&mut self, id: &str) {
        let current = self.current_scope();
        if self.table.len() > 1 && self.scope_data(current).symbols.contains_key(id) {
            let symbol = self.scope_get(current, id).unwrap();
            let global = self.global_scope();
            let mangled = Self::make_child_namespace(&self.scope_data(global).namespace, id);
            let is_label = self.class_(symbol) == Class::Label;
            let data = self.id_mut(symbol);
            data.r.offset = None;
            data.scope = Scope::Global;
            if !is_label {
                data.mangled = mangled;
            }
            self.scope_set(global, id, symbol);
            self.scope_del(current, id);
        }
    }

    /// `SymbolTable.make_static`: a local that lives with the globals.
    pub fn make_static(&mut self, id: &str) {
        let current = self.current_scope();
        let entry = self.scope_get(current, id).unwrap_or_else(|| crash("AssertionError: entry is not None"));
        self.id_mut(entry).scope = Scope::Global;
        let mangled = self.id(entry).mangled.clone();
        let global = self.global_scope();
        self.scope_set(global, &mangled, entry);
    }

    // ------------------------------------------------------------------
    // Identifier Declaration (e.g DIM, FUNCTION, SUB, etc.)
    // ------------------------------------------------------------------

    /// `SymbolTable.access_id`: the entry of a name, declared implicitly if
    /// it is new. `default_class` is the class a new one gets.
    pub fn access_id(&mut self, id: &str, lineno: u32, scope: Option<ScopeId>, default_type: Option<Ty>, default_class: Class, ignore_explicit_flag: bool) -> Option<NodeId> {
        if !ignore_explicit_flag {
            self.check_is_declared_explicit(lineno, id, "variable");
        }
        let Some(result) = self.get_entry(id, scope, Some(lineno)) else {
            let default_type = default_type.unwrap_or_else(|| Ty::reference(DEFAULT_IMPLICIT_TYPE, lineno, true));
            let entry = self.new_id(id, lineno, None, Some(default_type), default_class);
            return Some(self.declare_safe(id, lineno, entry));
        };
        // Declared already. If its type is still open and one is given, that
        // becomes its type.
        if let Some(mut default_type) = default_type {
            if self.ty(result).is_some_and(|t| t == BasicType::Unknown) {
                if default_type == BasicType::Boolean {
                    default_type = Ty::new(BasicType::UByte);
                }
                self.node_mut(result).ty = Some(default_type);
                self.warning_implicit_type(lineno, id, Some(default_type.name()));
            }
        }
        self.check_codebank_access(Some(result), lineno);
        Some(result)
    }

    /// `SymbolTable.access_var`.
    pub fn access_var(&mut self, id: &str, lineno: u32, scope: Option<ScopeId>, default_type: Option<Ty>) -> Option<NodeId> {
        let result = self.access_id(id, lineno, scope, default_type, Class::Var, false)?;
        if !self.check_class_id(id, Class::Var, lineno, scope, true) {
            return None;
        }
        if self.class_(result) == Class::Unknown {
            self.to_var(result, None);
        }
        if self.class_(result) != Class::Var {
            crash("AssertionError: result.class_ == CLASS.var");
        }
        Some(result)
    }

    /// `SymbolTable.access_array`.
    pub fn access_array(&mut self, id: &str, lineno: u32, scope: Option<ScopeId>, default_type: Option<Ty>) -> Option<NodeId> {
        if !self.check_is_declared(id, lineno, "array", scope, true) {
            return None;
        }
        if !self.check_class_id(id, Class::Array, lineno, scope, true) {
            return None;
        }
        self.access_id(id, lineno, scope, default_type, Class::Unknown, false)
    }

    /// `SymbolTable.access_func`.
    pub fn access_func(&mut self, id: &str, lineno: u32, scope: Option<ScopeId>, default_type: Option<Ty>) -> Option<NodeId> {
        let Some(result) = self.get_entry(id, scope, Some(lineno)) else {
            // DEFAULT_IMPLICIT_TYPE is TYPE.unknown.
            let default_type = default_type.unwrap_or_else(|| Ty::reference(BasicType::Unknown, lineno, true));
            return self.declare_func(id, lineno, Some(default_type), Class::Unknown);
        };
        let class = self.class_(result);
        if !matches!(class, Class::Function | Class::Sub | Class::Unknown) {
            self.syntax_error_unexpected_class(lineno, id, class, Class::Function);
            return None;
        }
        Some(result)
    }

    /// `SymbolTable.access_call`: something that may be followed by
    /// arguments, be it a function, an array or a string to slice.
    pub fn access_call(&mut self, id: &str, lineno: u32, scope: Option<ScopeId>, type_: Option<Ty>) -> Option<NodeId> {
        let Some(entry) = self.access_id(id, lineno, scope, type_, Class::Unknown, false) else {
            return self.access_func(id, lineno, None, None);
        };
        if !self.is_id(entry) {
            crash("AttributeError: 'SymbolBASICTYPE' object has no attribute 'callable'");
        }
        if !self.id(entry).r.callable && self.ty(entry).is_none_or(|t| t != BasicType::String) {
            self.syntax_error_not_array_nor_func(lineno, id);
            return None;
        }
        Some(entry)
    }

    /// `SymbolTable.access_label`.
    pub fn access_label(&mut self, id: &str, lineno: u32, scope: Option<ScopeId>) -> Option<NodeId> {
        let result = match self.get_entry(id, scope, Some(lineno)) {
            None => {
                // declare_label() gives None only for a duplicate, which a
                // name that is not there cannot be.
                let result = self.declare_label(id, lineno).unwrap_or_else(|| crash("AttributeError: 'NoneType' object has no attribute 'declared'"));
                self.id_mut(result).declared = false;
                result
            }
            Some(result) => {
                if !self.check_class_id(id, Class::Label, lineno, scope, true) {
                    return None;
                }
                result
            }
        };
        if self.class_(result) != Class::Label {
            self.to_label(result); // an undeclared label used in advance
        }
        Some(result)
    }

    /// `SymbolTable.declare_variable`.
    pub fn declare_variable(&mut self, id: &str, lineno: u32, type_: Ty, default_value: Option<NodeId>, class: Class) -> Option<NodeId> {
        let current = self.current_scope();
        if !self.check_is_undeclared(id, lineno, "identifier", Some(current), false) {
            let entry = self.get_existing_entry(id, None, Some(lineno));
            let (fname, line) = (self.id(entry).filename.clone(), self.lineno(entry));
            if self.id(entry).scope == Scope::Parameter {
                self.error(lineno, &format!("Variable '{id}' already declared as a parameter at {fname}:{line}"));
            } else {
                self.error(lineno, &format!("Variable '{id}' already declared at {fname}:{line}"));
            }
            return None;
        }
        if !self.check_class_id(id, class, lineno, Some(current), true) {
            return None;
        }
        let entry = match self.get_entry(id, Some(current), Some(lineno)) {
            Some(entry) => entry,
            None => {
                let new = self.new_id(id, lineno, None, Some(type_), Class::Unknown);
                self.declare_safe(id, lineno, new)
            }
        };
        if self.class_(entry) == Class::Unknown {
            match class {
                Class::Var => {
                    self.to_var(entry, default_value);
                }
                Class::Const => {
                    let value = default_value.unwrap_or_else(|| crash("AttributeError: 'NoneType' object has no attribute 'token'"));
                    self.to_const(entry, value);
                }
                _ => {}
            }
        }
        if self.class_(entry) != class {
            crash("AssertionError: entry.class_ == class_");
        }
        let entry_ty = match self.ty(entry) {
            Some(t) if t != BasicType::Unknown => t,
            _ => {
                self.node_mut(entry).ty = Some(type_);
                type_
            }
        };
        if entry_ty != type_ && !type_.implicit {
            self.error(lineno, &format!("'{id}' suffix is for type '{entry_ty}' but it was declared as '{type_}'"));
            return None;
        }
        self.id_mut(entry).declared = true;
        if entry_ty.implicit && entry_ty != BasicType::Unknown {
            self.warning_implicit_type(lineno, id, Some(entry_ty.name()));
        }
        if let Some(value) = default_value {
            if self.ty(value).is_none_or(|t| t != entry_ty) {
                if self.is_number(&[value]) {
                    self.typecast(entry_ty, Some(value), lineno)?;
                } else {
                    let value_ty = self.ty(value).map_or("None".to_string(), |t| t.to_string());
                    self.error(lineno, &format!("Variable '{id}' declared as '{entry_ty}' but initialized with a '{value_ty}' value"));
                    return None;
                }
            }
        }
        self.bind_to_codebank(Some(entry));
        Some(entry)
    }

    /// `SymbolTable._bind_to_codebank`: a global declared inside a CODEBANK
    /// block is stored in that bank.
    pub fn bind_to_codebank(&mut self, entry: Option<NodeId>) {
        let Some(entry) = entry else { return };
        if !matches!(self.class_(entry), Class::Var | Class::Array) {
            return;
        }
        if self.current_scope() == self.global_scope() {
            let bank = self.opts.current_codebank();
            self.id_mut(entry).var_mut().unwrap().bank = bank;
        }
    }

    /// `SymbolTable._codebank_in_scope`: the bank the statement being parsed
    /// is compiled into.
    pub fn codebank_in_scope(&self) -> i64 {
        match self.function_level.last() {
            Some(&func) => self.bank_of(func),
            None => self.opts.current_codebank(),
        }
    }

    /// `SymbolTable._check_codebank_access`: a bank-local variable can only
    /// be reached from its own bank.
    pub fn check_codebank_access(&mut self, entry: Option<NodeId>, lineno: u32) {
        if self.far_access {
            return; // FARPTR: naming a bank-local symbol is the point
        }
        let Some(entry) = entry else { return };
        if !matches!(self.class_(entry), Class::Var | Class::Array) {
            return;
        }
        let bank = self.bank_of(entry);
        let here = self.codebank_in_scope();
        if bank == 0 || bank == here {
            return;
        }
        let place = if here != 0 { format!("CODEBANK {here}") } else { "the resident program".to_string() };
        let msg = format!(
            "'{}' lives in CODEBANK {bank} and cannot be reached from {place}. Bank-local data is only addressable while its own bank is paged in",
            self.id(entry).name
        );
        self.error(lineno, &msg);
    }

    /// `SymbolTable.declare_const`.
    pub fn declare_const(&mut self, id: &str, lineno: u32, type_: Ty, default_value: Option<NodeId>) -> Option<NodeId> {
        let current = self.current_scope();
        if !self.check_is_undeclared(id, lineno, "identifier", Some(current), false) {
            let entry = self.get_entry(id, None, Some(lineno)).unwrap_or_else(|| crash("AttributeError: 'NoneType' object has no attribute 'scope'"));
            let (fname, line) = (self.id(entry).filename.clone(), self.lineno(entry));
            if self.id(entry).scope == Scope::Parameter {
                self.error(lineno, &format!("Constant '{id}' already declared as a parameter at {fname}:{line}"));
            } else {
                self.error(lineno, &format!("Constant '{id}' already declared at {fname}:{line}"));
            }
            return None;
        }
        self.declare_variable(id, lineno, type_, default_value, Class::Const)
    }

    /// `SymbolTable.declare_label`. Labels, line numbers among them, are
    /// always global.
    pub fn declare_label(&mut self, id: &str, lineno: u32) -> Option<NodeId> {
        if !self.check_is_undeclared(id, lineno, "label", None, false) {
            let e = self.get_existing_entry(id, None, Some(lineno));
            let msg = format!("Label '{id}' already used at {}:{}", self.id(e).filename, self.lineno(e));
            self.error(lineno, &msg);
            return Some(e);
        }
        // Unreachable in the Python code as well: what the check above lets
        // through is not declared.
        let (current, global) = (self.current_scope(), self.global_scope());
        let entry = self.get_entry(id, Some(current), Some(lineno)).or_else(|| self.get_entry(id, Some(global), Some(lineno))).or_else(|| {
            let new = self.new_id(id, lineno, None, None, Class::Label);
            self.declare(id, lineno, new)
        })?;
        if self.token(entry) != "LABEL" {
            self.to_label(entry);
        }
        if id.starts_with(NAMESPACE_SEPARATOR) {
            // A label starting with a dot is a root-global one, taken as it is.
            self.id_mut(entry).mangled = id.to_string();
        } else {
            // Done again because labels are always global: it resets the
            // namespace, and takes another temporary.
            self.to_label(entry);
        }
        if !self.function_level.is_empty() {
            let owners = self.function_level.clone();
            self.set_scope_owner(entry, owners);
        }
        self.move_to_global_scope(id);
        self.id_mut(entry).declared = true;
        self.node_mut(entry).ty = Some(Ty::new(PTR_TYPE));
        Some(entry)
    }

    /// `SymbolTable.declare_param`.
    pub fn declare_param(&mut self, id: &str, lineno: u32, type_: Option<Ty>, is_array: bool, default_value: Option<NodeId>) -> Option<NodeId> {
        let current = self.current_scope();
        if !self.check_is_undeclared(id, lineno, "parameter", Some(current), true) {
            return None;
        }
        let entry = if is_array {
            if default_value.is_some() {
                self.syntax_error_cannot_define_default_array_argument(lineno);
                return None;
            }
            let new = self.new_id(id, lineno, None, type_, Class::Unknown);
            let entry = self.declare_safe(id, lineno, new);
            let bounds = self.new_node(Kind::BoundList, 0, &[]);
            self.to_vararray(entry, bounds)
        } else {
            let new = self.new_id(id, lineno, None, type_, Class::Unknown);
            let entry = self.declare_safe(id, lineno, new);
            self.to_var(entry, default_value)
        };
        let data = self.id_mut(entry);
        data.scope = Scope::Parameter;
        data.declared = true;
        let entry_ty = self.ty(entry).unwrap_or_else(|| crash("AttributeError: 'NoneType' object has no attribute 'implicit'"));
        if entry_ty.implicit {
            // The type as declared is what the message shows.
            let shown = type_.map_or("None".to_string(), |t| t.to_string());
            self.warning_implicit_type(lineno, id, Some(&shown));
        }
        Some(entry)
    }

    /// `SymbolTable.declare_array`. `addr` is where `DIM ... AT` puts it.
    pub fn declare_array(&mut self, id: &str, lineno: u32, type_: Ty, bounds: NodeId, default_value: Option<NodeId>, addr: Option<NodeId>) -> Option<NodeId> {
        let mut type_ = type_;
        // More cells than the machine has bytes. The Python compiler builds
        // the storage item by item, and for a large enough array never ends.
        let cells = self.children(bounds).iter().flatten().try_fold(1i128, |cells, &bound| match self.node(bound).kind {
            Kind::Bound { lower, upper } => upper.checked_sub(lower)?.checked_add(1).and_then(|n| cells.checked_mul(n)),
            _ => Some(cells),
        });
        if cells.and_then(|cells| cells.checked_mul(type_.size() as i128)).is_none_or(|bytes| bytes > 65536) {
            self.error(lineno, &format!("Array '{id}' is too large: it does not fit in 64K"));
            return None;
        }
        let current = self.current_scope();
        if !self.check_class_id(id, Class::Array, lineno, Some(current), true) {
            return None;
        }
        let entry = match self.get_entry(id, Some(current), Some(lineno)) {
            Some(entry) => entry,
            None => {
                let new = self.new_id(id, lineno, None, Some(type_), Class::Unknown);
                self.declare_safe(id, lineno, new)
            }
        };
        if !self.id(entry).declared {
            if self.id(entry).r.callable {
                let msg = format!("Array '{id}' must be declared before use. First used at line {}", self.lineno(entry));
                self.error(lineno, &msg);
                return None;
            }
        } else {
            let line = self.lineno(entry);
            if self.id(entry).scope == Scope::Parameter {
                self.error(lineno, &format!("variable '{id}' already declared as a parameter at line {line}"));
            } else {
                self.error(lineno, &format!("variable '{id}' already declared at line {line}"));
            }
            return None;
        }
        let entry_ty = self.ty(entry);
        if entry_ty.is_none_or(|t| t != BasicType::Unknown && t != type_) {
            if !type_.implicit {
                let shown = entry_ty.map_or("None".to_string(), |t| t.to_string());
                let msg = format!("Array suffix for '{}' is for type '{shown}' but declared as '{type_}'", self.id(entry).name);
                self.error(lineno, &msg);
                return None;
            }
            type_ = entry_ty.unwrap_or_else(|| crash("AttributeError: 'NoneType' object has no attribute 'implicit'"));
        }
        if type_.implicit {
            self.warning_implicit_type(lineno, id, Some(type_.name()));
        }
        if self.class_(entry) != Class::Array {
            self.to_vararray(entry, bounds);
        }
        let is_global = current == self.global_scope();
        let data = self.id_mut(entry);
        data.declared = true;
        data.scope = if is_global { Scope::Global } else { Scope::Local };
        data.var_mut().unwrap().default_value = default_value;
        data.addr = addr;
        self.node_mut(entry).ty = Some(type_);
        self.bind_to_codebank(Some(entry));
        Some(entry)
    }

    /// `SymbolTable.declare_func`.
    pub fn declare_func(&mut self, id: &str, lineno: u32, type_: Option<Ty>, class: Class) -> Option<NodeId> {
        if !self.check_class_id(id, class, lineno, None, true) {
            return None;
        }
        let entry = match self.get_entry(id, None, Some(lineno)) {
            Some(entry) => {
                if !self.is_id(entry) {
                    crash("AttributeError: 'SymbolBASICTYPE' object has no attribute 'declared'");
                }
                if self.id(entry).declared {
                    let Some(func) = self.id(entry).func() else { crash("AssertionError: entry.token == \"FUNCTION\"") };
                    if !func.forwarded {
                        let msg = format!("Duplicate function name '{id}', previously defined at {}", self.lineno(entry));
                        self.error(lineno, &msg);
                        return None;
                    }
                }
                if let Some((_, basic)) = suffix_type(id) {
                    if self.ty(entry).is_none_or(|t| t != basic) {
                        self.syntax_error_func_type_mismatch(lineno, entry);
                    }
                }
                if self.class_(entry) == Class::Unknown {
                    // A function or sub used in advance. One only called so
                    // far already has a function ref of unknown class, and
                    // gets a new one (and new, empty parameters and body).
                    self.to_function(entry, None, class);
                }
                // Mangled as a child of the scope the definition is in.
                let mangled = format!("{}_{}", self.current_namespace, self.id(entry).name);
                self.id_mut(entry).mangled = mangled;
                entry
            }
            None => {
                let new = self.new_id(id, lineno, None, type_, Class::Unknown);
                self.to_function(new, None, class);
                self.declare(id, lineno, new)?
            }
        };
        if self.token(entry) != "FUNCTION" {
            crash("AssertionError: entry.token == \"FUNCTION\"");
        }
        let func = self.id_mut(entry).func_mut().unwrap();
        if !func.forwarded {
            func.params_size = 0;
        }
        func.locals_size = 0;
        Some(entry)
    }

    /// `SymbolTable.check_labels`: every label used has been declared.
    pub fn check_labels(&mut self) {
        for entry in self.labels_in_scope() {
            let (name, lineno) = (self.id(entry).name.clone(), self.lineno(entry));
            self.check_is_declared(&name, lineno, "label", None, true);
        }
    }

    /// `SymbolTable.check_classes`. It looks for a class of `None`, which no
    /// entry has, so it reports nothing.
    pub fn check_classes(&mut self) {}

    /// `SymbolTable.vars_`: the variables of the current scope.
    pub fn vars_(&self) -> Vec<NodeId> {
        self.scope_values(self.current_scope(), false).into_iter().filter(|&x| self.class_(x) == Class::Var).collect()
    }

    /// `SymbolTable.labels`.
    pub fn labels_in_scope(&self) -> Vec<NodeId> {
        self.scope_values(self.current_scope(), false).into_iter().filter(|&x| self.class_(x) == Class::Label).collect()
    }

    /// `SymbolTable.arrays`. Unlike `vars_`, filtered by what was accessed
    /// above `-O1`.
    pub fn arrays(&self) -> Vec<NodeId> {
        self.scope_values(self.current_scope(), true).into_iter().filter(|&x| self.class_(x) == Class::Array).collect()
    }

    /// `SymbolTable.functions`.
    pub fn functions_in_scope(&self) -> Vec<NodeId> {
        self.scope_values(self.current_scope(), true).into_iter().filter(|&x| matches!(self.class_(x), Class::Function | Class::Sub)).collect()
    }
}
