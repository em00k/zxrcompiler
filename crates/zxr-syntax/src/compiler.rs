//! One compilation's state: what the Python compiler keeps in the module
//! globals of `src/api/global_.py`, `opcodestemps.py`, `tmp_labels.py`,
//! `string_labels.py` and `zxbparser.py`, together with the tree's arena and
//! the properties of the classes in `src/symbols/`.
//!
//! Every Python function that reads those globals is a method of
//! [`Compiler`] here, under the same name where one exists.

use crate::ast::*;
use crate::options::{OptError, OptValue, Options};
use crate::ordmap::OrdMap;
use crate::pyval::crash;
use crate::symtab::ScopeData;
use crate::types::{BasicType, Ty};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::rc::Rc;
use zxr_core::Session;
use zxr_ir::Quad;

/// `LoopType`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LoopType {
    Do,
    For,
    While,
}

impl LoopType {
    pub fn name(self) -> &'static str {
        match self {
            LoopType::Do => "DO",
            LoopType::For => "FOR",
            LoopType::While => "WHILE",
        }
    }
}

/// `LoopInfo`: a loop the parser is inside.
#[derive(Clone, Debug)]
pub struct LoopInfo {
    pub kind: LoopType,
    pub lineno: u32,
    /// The variable of a FOR.
    pub var: Option<String>,
}

/// `DataRef`: a DATA line, with the label that names it.
#[derive(Clone, Debug)]
pub struct DataRef {
    pub label: NodeId,
    pub datas: Vec<NodeId>,
}

// The architecture constants `src/arch/z80/__init__.py` sets in `global_`.
pub const PARAM_ALIGN: i64 = 2;
pub const BOUND_TYPE: BasicType = BasicType::UInteger;
pub const SIZE_TYPE: BasicType = BasicType::UByte;
pub const PTR_TYPE: BasicType = BasicType::UInteger;
pub const STR_INDEX_TYPE: BasicType = BasicType::UInteger;
pub const MIN_STRSLICE_IDX: i128 = 0;
pub const MAX_STRSLICE_IDX: i128 = 65534;
pub const DEFAULT_TYPE: BasicType = BasicType::Float;
pub const DEFAULT_IMPLICIT_TYPE: BasicType = BasicType::Unknown;
pub const CORE_NAMESPACE: &str = ".core";
pub const DATAS_NAMESPACE: &str = ".DATA";
pub const LABELS_NAMESPACE: &str = ".LABEL";
pub const ZXBASIC_USER_DATA: &str = ".core.ZXBASIC_USER_DATA";
pub const ZXBASIC_USER_DATA_LEN: &str = ".core.ZXBASIC_USER_DATA_LEN";
pub const MANGLE_CHR: &str = "_";
pub const NAMESPACE_SEPARATOR: &str = ".";
pub const ARRAY_DATA_PREFIX: &str = "__DATA__";
pub const FAR_BODY_SUFFIX: &str = ".__far";
pub const FAR_ADDR_SUFFIX: &str = ".__faraddr";

/// A call of a routine or an `@` of a name, noted while parsing.
#[derive(Clone, Debug)]
pub struct Use {
    pub entry: NodeId,
    pub lineno: u32,
    /// The code bank it was written in, 0 for the resident program.
    pub bank: i64,
    pub filename: Rc<str>,
}

pub struct Compiler {
    pub sess: Session,
    /// Debug data is wanted: each quad is given the source line of the
    /// statement it is generated for.
    pub debug_lines: bool,
    /// The source lines so named, the first being number 1.
    pub origins: Vec<(Rc<str>, u32)>,
    origin_numbers: HashMap<(Rc<str>, u32), u32>,
    /// The line of the statement being translated; 0 is none.
    pub origin: u32,
    /// `OPTIONS`.
    pub opts: Options,
    /// `OPTIONS.__DEFINES`: macros the compile defines for the second
    /// preprocessor pass, in order.
    pub defines: Vec<(String, String)>,

    // --- the tree ---
    pub nodes: Vec<Node>,
    cur_fname: Rc<str>,

    // --- SymbolTable ---
    pub scopes: Vec<ScopeData>,
    /// `SymbolTable.table`: the open scopes, global first.
    pub table: Vec<ScopeId>,
    pub current_namespace: String,
    /// `SymbolTable.namespaces`.
    pub namespaces: OrdMap<ScopeId>,
    /// `SymbolTable.basic_types`, indexed by `BasicType as usize`.
    pub basic_types: Vec<NodeId>,
    /// Set while a FARPTR is being parsed.
    pub far_access: bool,

    // --- counters ---
    /// `opcodestemps._COUNTER`.
    pub temp_counter: u32,
    /// `tmp_labels.LABEL_COUNTER`.
    pub label_counter: u32,
    /// `tmp_labels.TMP_LABELS`, in the order they were made.
    pub tmp_labels: Vec<String>,
    /// `string_labels.STRING_LABELS`: constant string to its label.
    pub string_labels: OrdMap<String>,

    // --- global_ ---
    pub loops: Vec<LoopInfo>,
    pub meta_loops: Vec<Vec<LoopInfo>>,
    /// The functions being defined, innermost last; empty at module level.
    pub function_level: Vec<NodeId>,
    /// Calls made before the function was defined, checked at the end.
    pub function_calls: Vec<NodeId>,
    /// Functions waiting to be translated.
    pub functions: Vec<NodeId>,
    pub functions_bankinit: Vec<NodeId>,
    /// `#init` routines. A set in Python; kept in the order they were added.
    pub inits: Vec<String>,
    pub datas: Vec<DataRef>,
    /// A set in Python; kept in the order they were added.
    pub data_labels_required: Vec<String>,
    pub data_labels: OrdMap<String>,
    pub data_ptr_current: String,
    pub data_is_used: bool,
    pub data_functions: Vec<NodeId>,

    // --- zxbparser ---
    pub labels: OrdMap<NodeId>,
    pub let_assignment: bool,
    pub print_is_used: bool,
    pub last_brk_linenum: u32,
    /// Line of a `PRIVATE` the parser has read and the header of the SUB or
    /// FUNCTION after it has not yet claimed.
    pub pending_private: Option<u32>,
    /// Each `@name` and each call: whether the name is a private routine,
    /// or could be one, is known only once everything has been read.
    pub address_uses: Vec<Use>,
    pub call_uses: Vec<Use>,
    pub ast: Option<NodeId>,
    pub data_ast: Option<NodeId>,

    // --- what the translators leave for the backend ---
    /// `backend.MEMORY`: the quads of the pass being translated.
    pub memory: Vec<Quad>,
    /// `backend.common.REQUIRES`: library files the code needs. A set.
    pub requires: BTreeSet<String>,
    /// `backend.common.INITS`: routines to call at start-up. A set.
    pub backend_inits: BTreeSet<String>,
    /// `backend.common.FLAG_use_function_exit`.
    pub flag_use_function_exit: bool,
    /// `backend.common.FLAG_end_emitted`.
    pub flag_end_emitted: bool,
    /// `backend.common.AT_END`: assembly lines for the very end of the file.
    pub at_end: Vec<String>,
    /// `codebank.FAR_TRAMPOLINES`: trampoline label to the banked body's.
    pub far_trampolines: OrdMap<String>,
    /// `codebank.BANKS_USED`: banks that hold a routine.
    pub banks_used: BTreeSet<i64>,
    /// `codebank.BANKS_WITH_DATA`.
    pub banks_with_data: BTreeSet<i64>,
    /// `codebank.DIVERTED`: module-level asm blocks and labels to replay
    /// inside a bank, by bank in the order the banks were first seen.
    pub diverted: Vec<(i64, Vec<NodeId>)>,
    /// `codebank._DIVERTED_SEEN`.
    pub diverted_seen: BTreeSet<NodeId>,
    /// `codebank._NO_ROUTINE_WARNED`.
    pub no_routine_warned: BTreeSet<i64>,
    /// The program's asm reads the far-call runtime's state, as the far
    /// memory routines (`farmem.asm`) do: the runtime is needed, and a bank
    /// of nothing but data is reachable after all.
    pub far_memory_used: bool,

    // --- the passes over the tree ---
    /// `GenericVisitor.NOP`: the one node every pass returns for a node it
    /// has erased. Made the first time it is needed.
    pub visitor_nop: Option<NodeId>,
    /// `funcdecl.params_size`: an attribute the grammar actions set on the
    /// FUNCDECL node itself, apart from the one of the function's ref.
    pub funcdecl_params_size: std::collections::BTreeMap<NodeId, i64>,
    /// The state of the translator classes.
    pub tr: crate::translate::State,
}

impl Compiler {
    /// `config.init()`, `zxbparser.init()` and the rest of what `main()` sets
    /// up before the options are parsed.
    pub fn new(sess: Session) -> Compiler {
        let cur_fname: Rc<str> = Rc::from(sess.filename.as_str());
        let mut c = Compiler {
            sess,
            opts: Options::new(),
            defines: Vec::new(),
            nodes: Vec::new(),
            cur_fname,
            scopes: Vec::new(),
            table: Vec::new(),
            current_namespace: String::new(),
            namespaces: OrdMap::new(),
            basic_types: Vec::new(),
            far_access: false,
            temp_counter: 0,
            label_counter: 0,
            tmp_labels: Vec::new(),
            string_labels: OrdMap::new(),
            loops: Vec::new(),
            meta_loops: Vec::new(),
            function_level: Vec::new(),
            function_calls: Vec::new(),
            functions: Vec::new(),
            functions_bankinit: Vec::new(),
            inits: Vec::new(),
            datas: Vec::new(),
            data_labels_required: Vec::new(),
            data_labels: OrdMap::new(),
            data_ptr_current: String::new(),
            data_is_used: false,
            data_functions: Vec::new(),
            labels: OrdMap::new(),
            let_assignment: false,
            print_is_used: false,
            last_brk_linenum: 0,
            pending_private: None,
            address_uses: Vec::new(),
            call_uses: Vec::new(),
            ast: None,
            data_ast: None,
            memory: Vec::new(),
            requires: BTreeSet::new(),
            backend_inits: BTreeSet::new(),
            flag_use_function_exit: false,
            flag_end_emitted: false,
            at_end: Vec::new(),
            far_trampolines: OrdMap::new(),
            banks_used: BTreeSet::new(),
            banks_with_data: BTreeSet::new(),
            diverted: Vec::new(),
            diverted_seen: BTreeSet::new(),
            no_routine_warned: BTreeSet::new(),
            far_memory_used: false,
            debug_lines: false,
            origins: Vec::new(),
            origin_numbers: HashMap::new(),
            origin: 0,
            visitor_nop: None,
            funcdecl_params_size: std::collections::BTreeMap::new(),
            tr: crate::translate::State::default(),
        };
        c.init_symbol_table();
        c.data_ptr_current = c.current_data_label();
        c
    }

    // ------------------------------------------------------------------
    // Options
    // ------------------------------------------------------------------

    /// `OPTIONS.optimization_level`.
    pub fn o_level(&self) -> i128 {
        self.opts.int("optimization_level")
    }

    /// Copies the options the diagnostics read into the session, after
    /// anything that may have changed them.
    pub fn sync_session_options(&mut self) {
        self.sess.options.max_syntax_errors = self.opts.int("max_syntax_errors").clamp(0, u32::MAX as i128) as u32;
        self.sess.options.expected_warnings = self.opts.int("expected_warnings").clamp(0, u32::MAX as i128) as u32;
        self.sess.options.hide_warning_codes = self.opts.bool("hide_warning_codes");
    }

    pub fn set_option(&mut self, name: &str, value: OptValue) -> Result<(), OptError> {
        let result = self.opts.set(name, value);
        self.sync_session_options();
        result
    }

    /// `OPTIONS.__DEFINES[name] = value`.
    pub fn define(&mut self, name: &str, value: &str) {
        match self.defines.iter_mut().find(|(n, _)| n == name) {
            Some(entry) => entry.1 = value.to_string(),
            None => self.defines.push((name.to_string(), value.to_string())),
        }
    }

    // ------------------------------------------------------------------
    // Counters and labels
    // ------------------------------------------------------------------

    /// `optemps.new_t()`.
    pub fn new_t(&mut self) -> String {
        let t = format!("t{}", self.temp_counter);
        self.temp_counter += 1;
        t
    }

    /// `tmp_labels.tmp_label()`.
    pub fn tmp_label(&mut self) -> String {
        let label = format!("{LABELS_NAMESPACE}.__LABEL{}", self.label_counter);
        self.tmp_labels.push(label.clone());
        self.label_counter += 1;
        label
    }

    /// `string_labels.add_string_label()`: the label of a constant string,
    /// made on first use.
    pub fn add_string_label(&mut self, string: &str) -> String {
        if let Some(label) = self.string_labels.get(string) {
            return label.clone();
        }
        let label = self.tmp_label();
        self.string_labels.insert(string, label.clone());
        label
    }

    /// `utils.current_data_label()`.
    pub fn current_data_label(&self) -> String {
        format!("{DATAS_NAMESPACE}.__DATA__{}", self.datas.len())
    }

    /// `gl.FILENAME`, shared between the nodes made while it is current.
    pub fn filename(&mut self) -> Rc<str> {
        if *self.cur_fname != *self.sess.filename {
            self.cur_fname = Rc::from(self.sess.filename.as_str());
        }
        Rc::clone(&self.cur_fname)
    }

    // ------------------------------------------------------------------
    // errmsg
    // ------------------------------------------------------------------

    pub fn error(&mut self, lineno: u32, msg: &str) {
        self.sess.error(lineno, msg, None);
    }

    /// `error(lineno, msg, fname=fname)`.
    pub fn error_at(&mut self, lineno: u32, msg: &str, fname: Option<&str>) {
        self.sess.error(lineno, msg, fname);
    }

    /// A warning with no code.
    pub fn warning(&mut self, lineno: u32, msg: &str) {
        self.sess.warning(lineno, msg, None);
    }

    fn coded(&mut self, code: &str, lineno: u32, msg: &str, fname: Option<&str>) {
        self.sess.warning_coded(code, lineno, msg, fname);
    }

    /// `warning_implicit_type`. In strict mode it is an error instead, but
    /// only if the warning is enabled, as the check sits inside it.
    pub fn warning_implicit_type(&mut self, lineno: u32, id: &str, type_: Option<&str>) {
        if !self.sess.warnings.get("100").copied().unwrap_or(true) {
            return;
        }
        if self.opts.bool("strict") {
            self.syntax_error_undeclared_type(lineno, id);
            return;
        }
        let type_ = type_.unwrap_or(DEFAULT_TYPE.name());
        self.coded("100", lineno, &format!("Using default implicit type '{type_}' for '{id}'"), None);
    }

    pub fn warning_uninitalized_string_var(&mut self, lineno: u32, id: &str) {
        self.coded("101", lineno, &format!("Accessing uninitialized string variable '{id}'"), None);
    }

    pub fn warning_condition_is_always(&mut self, lineno: u32, cond: bool) {
        self.coded("110", lineno, &format!("Condition is always {}", if cond { "True" } else { "False" }), None);
    }

    pub fn warning_conversion_lose_digits(&mut self, lineno: u32) {
        self.coded("120", lineno, "Conversion may lose significant digits", None);
    }

    pub fn warning_empty_loop(&mut self, lineno: u32) {
        self.coded("130", lineno, "Empty loop", None);
    }

    pub fn warning_empty_if(&mut self, lineno: u32) {
        self.coded("140", lineno, "Useless empty IF ignored", None);
    }

    pub fn warning_not_used(&mut self, lineno: u32, id: &str, kind: &str, fname: Option<&str>) {
        if self.o_level() > 0 {
            self.coded("150", lineno, &format!("{kind} '{id}' is never used"), fname);
        }
    }

    pub fn warning_fastcall_with_n_parameters(&mut self, lineno: u32, kind: &str, id: &str, num_params: usize) {
        self.coded("160", lineno, &format!("{kind} '{id}' declared as FASTCALL with {num_params} parameters"), None);
    }

    pub fn warning_func_is_never_called(&mut self, lineno: u32, func_name: &str, fname: Option<&str>) {
        self.coded("170", lineno, &format!("Function '{func_name}' is never called and has been ignored"), fname);
    }

    pub fn warning_unreachable_code(&mut self, lineno: u32, fname: Option<&str>) {
        self.coded("180", lineno, "Unreachable code", fname);
    }

    pub fn warning_function_should_return_a_value(&mut self, lineno: u32, func_name: &str, fname: Option<&str>) {
        self.coded("190", lineno, &format!("Function '{func_name}' should return a value"), fname);
    }

    pub fn warning_value_will_be_truncated(&mut self, lineno: u32, fname: Option<&str>) {
        self.coded("200", lineno, "Value will be truncated", fname);
    }

    pub fn warning_ignoring_unknown_pragma(&mut self, lineno: u32, pragma_name: &str) {
        self.coded("300", lineno, &format!("Ignoring unknown pragma '{pragma_name}'"), None);
    }

    pub fn warning_byref_leaves_codebank(&mut self, lineno: u32, id: &str, bank: i64, callee: &str, fname: Option<&str>) {
        let msg = format!(
            "'{id}' lives in CODEBANK {bank} but is passed ByRef to '{callee}', which does not. The callee gets a plain address that stops being valid as soon as the bank is paged out"
        );
        self.coded("900", lineno, &msg, fname);
    }

    pub fn warning_codebank_has_no_routines(&mut self, lineno: u32, bank: i64, fname: Option<&str>) {
        let msg = format!("CODEBANK {bank} holds data but no SUB or FUNCTION, so nothing ever pages it in. Bank-local data is only reachable from routines in the same bank");
        self.coded("910", lineno, &msg, fname);
    }

    pub fn warning_could_be_private(&mut self, lineno: u32, id: &str, bank: i64, fname: Option<&str>) {
        let msg = format!("'{id}' is only called from its own CODEBANK {bank}. Declared PRIVATE it would be called directly, with no trampoline");
        self.coded("930", lineno, &msg, fname);
    }

    pub fn warning_fastcall_without_frame(&mut self, lineno: u32, kind: &str, id: &str, what: &str) {
        let msg = format!("FASTCALL {kind} '{id}' has no stack frame, but its BASIC code uses {what}, which would be read from the caller's frame. Take the argument from A or HL in an ASM block, or drop FASTCALL");
        self.coded("940", lineno, &msg, None);
    }

    pub fn warning_local_under_asm(&mut self, lineno: u32, local: &str, routine: &str) {
        let msg = format!("Local variable '{local}' of '{routine}' is not used by BASIC code and is removed at -O2 and above, which moves the other locals; the asm in '{routine}' addresses locals through IX");
        self.coded("950", lineno, &msg, None);
    }

    /// Whether asm in these statements has an operand `(ix-n)`: a local
    /// variable, by where it is in the stack frame.
    pub fn asm_addresses_locals(&self, body: NodeId) -> bool {
        let mut pending = vec![body];
        while let Some(node) = pending.pop() {
            match &self.node(node).kind {
                Kind::Asm { asm, .. } => {
                    let found = asm.lines().any(|line| {
                        let code: String = line.split(';').next().unwrap_or("").chars().filter(|c| !c.is_whitespace()).collect();
                        code.to_ascii_lowercase().contains("(ix-")
                    });
                    if found {
                        return true;
                    }
                }
                // The routines it calls are not part of it.
                Kind::Id(_) => continue,
                _ => {}
            }
            pending.extend(self.children(node).iter().flatten());
        }
        false
    }

    /// Where a routine is called or has its address taken.
    fn note_use(&self, entry: NodeId, lineno: u32, bank: i64) -> Use {
        Use { entry, lineno, bank, filename: self.sess.filename.as_str().into() }
    }

    /// The code bank a statement being parsed will run in: its routine's,
    /// and the resident program for module-level code wherever it stands.
    pub fn code_bank_here(&self) -> i64 {
        self.function_level.last().map_or(0, |&f| self.bank_of(f))
    }

    pub fn note_call(&mut self, entry: NodeId, lineno: u32) {
        let u = self.note_use(entry, lineno, self.code_bank_here());
        self.call_uses.push(u);
    }

    pub fn note_address(&mut self, entry: NodeId, lineno: u32) {
        // A DIM at module level in a bank keeps its initialiser in the bank.
        let u = self.note_use(entry, lineno, self.codebank_in_scope());
        self.address_uses.push(u);
    }

    /// Whether a routine was declared `PRIVATE`.
    pub fn is_private(&self, func: NodeId) -> bool {
        matches!(&self.node(func).kind, Kind::Id(data) if data.func().is_some_and(|f| f.private))
    }

    /// The number of a source line in the table of those that code is
    /// generated for.
    pub fn origin_of(&mut self, filename: &Rc<str>, lineno: u32) -> u32 {
        if let Some(&number) = self.origin_numbers.get(&(Rc::clone(filename), lineno)) {
            return number;
        }
        self.origins.push((Rc::clone(filename), lineno));
        let number = self.origins.len() as u32;
        self.origin_numbers.insert((Rc::clone(filename), lineno), number);
        number
    }

    /// A global variable that the program's asm names is in use, though no
    /// BASIC statement reads it: without this it is removed above `-O1` and
    /// the asm is left with an undefined label. Run once it is known which
    /// routines are called, so that asm which is itself dropped keeps
    /// nothing, and before anything is removed.
    pub fn mark_asm_uses(&mut self, ast: NodeId) {
        if self.sess.oracle_diagnostics {
            return;
        }
        self.mark_asm_routines(ast);
        let mut globals: HashMap<String, NodeId> = HashMap::new();
        for entry in self.scope_values(self.global_scope(), false) {
            if matches!(self.class_(entry), Class::Var | Class::Array) && !self.accessed(entry) {
                globals.insert(self.id(entry).mangled.clone(), entry);
            }
        }
        if globals.is_empty() {
            return;
        }
        let (mut named, mut defined) = (HashSet::new(), HashSet::new());
        let (mut seen, mut pending) = (HashSet::new(), vec![ast]);
        while let Some(node) = pending.pop() {
            if !seen.insert(node) {
                continue;
            }
            match &self.node(node).kind {
                Kind::Asm { asm, .. } => asm_names(asm, &mut named, &mut defined),
                // A routine nobody calls is not compiled above -O1.
                Kind::Id(_) if matches!(self.class_(node), Class::Function | Class::Sub) && self.o_level() > 1 && !self.accessed(node) => continue,
                _ => {}
            }
            pending.extend(self.children(node).iter().flatten());
        }
        let mut keep = HashSet::new();
        for name in named {
            // `_buf.__DATA__` names `_buf`.
            let mut end = name.len();
            loop {
                // Asm that declares the label itself has its own.
                if let Some(&entry) = globals.get(&name[..end]).filter(|_| !defined.contains(&name[..end])) {
                    keep.insert(entry);
                }
                match name[..end].rfind('.') {
                    Some(dot) if dot > 0 => end = dot,
                    _ => break,
                }
            }
        }
        // In the order they were declared.
        for entry in self.scope_values(self.global_scope(), false) {
            if !keep.contains(&entry) {
                continue;
            }
            self.set_accessed(entry, true);
            // An array nothing had used by the end of the source was left
            // out of the declarations above -O1.
            if self.class_(entry) == Class::Array && self.o_level() > 1 {
                if let Some(data_ast) = self.data_ast {
                    let decl = self.make_array_declaration(entry);
                    self.append_child(data_ast, Some(decl));
                }
            }
        }
    }

    /// A SUB or FUNCTION that only asm names (`call ._Foo`, `jp ._Foo`, a
    /// `dw ._Foo` table) is in use, and so is what it calls: without this
    /// it is removed above `-O1` and the asm is left with an undefined
    /// label. Asm in the main program and in routines in use counts.
    fn mark_asm_routines(&mut self, ast: NodeId) {
        let mut decls = Vec::new();
        let (mut seen, mut pending) = (HashSet::new(), vec![ast]);
        while let Some(node) = pending.pop() {
            if !seen.insert(node) {
                continue;
            }
            if self.token(node) == "FUNCDECL" {
                decls.push(node);
            }
            pending.extend(self.children(node).iter().flatten());
        }
        let by_name: HashMap<String, NodeId> = decls.iter().map(|&d| (self.id(self.entry(d)).mangled.clone(), d)).collect();
        if by_name.is_empty() {
            return;
        }
        let mut roots: Vec<NodeId> = decls.iter().copied().filter(|&d| self.accessed(self.entry(d))).collect();
        roots.push(ast);
        let (mut scanned, mut fresh) = (HashSet::new(), HashSet::new());
        while let Some(root) = roots.pop() {
            if !scanned.insert(root) {
                continue;
            }
            let was: Vec<bool> = decls.iter().map(|&d| self.accessed(self.entry(d))).collect();
            if fresh.contains(&root) {
                self.mark_routine_calls(root);
            }
            let (mut named, mut defined) = (HashSet::new(), HashSet::new());
            let (mut seen, mut pending) = (HashSet::new(), vec![root]);
            while let Some(node) = pending.pop() {
                if !seen.insert(node) || (node != root && self.token(node) == "FUNCDECL") {
                    continue;
                }
                if let Kind::Asm { asm, .. } = &self.node(node).kind {
                    asm_names(asm, &mut named, &mut defined);
                }
                pending.extend(self.children(node).iter().flatten());
            }
            for name in named {
                if let Some(&decl) = by_name.get(&name).filter(|_| !defined.contains(&name)) {
                    self.set_accessed(self.entry(decl), true);
                }
            }
            for (i, &decl) in decls.iter().enumerate() {
                if !was[i] && self.accessed(self.entry(decl)) {
                    fresh.insert(decl);
                    roots.push(decl);
                }
            }
        }
    }

    /// For debug data: every global variable as (name in BASIC, label of
    /// its storage, file, line). An array's storage is its first cell.
    pub fn debug_variables(&self) -> Vec<(String, String, Rc<str>, u32)> {
        let mut out = Vec::new();
        for entry in self.scope_values(self.global_scope(), false) {
            let label = match self.class_(entry) {
                Class::Var => self.id(entry).mangled.clone(),
                Class::Array => self.data_label(entry),
                _ => continue,
            };
            let id = self.id(entry);
            out.push((id.name.clone(), label, Rc::clone(&id.filename), self.lineno(entry)));
        }
        out
    }

    /// Run when the whole program has been read. A private routine has no
    /// trampoline, so nothing outside its bank can call it or do anything
    /// with its address; and a routine that is public for nothing is
    /// pointed out.
    pub fn check_private_uses(&mut self) {
        let (calls, addresses) = (std::mem::take(&mut self.call_uses), std::mem::take(&mut self.address_uses));
        let mut faults: Vec<(&Use, &str)> = calls.iter().map(|u| (u, "called")).chain(addresses.iter().map(|u| (u, "addressed"))).filter(|(u, _)| self.is_private(u.entry) && self.bank_of(u.entry) != u.bank).collect();
        faults.sort_by_key(|(u, _)| u.lineno);
        for (u, what) in faults {
            let place = if u.bank != 0 { format!("CODEBANK {}", u.bank) } else { "the resident program".to_string() };
            let msg = format!("'{}' is PRIVATE to CODEBANK {} and cannot be {what} from {place}", self.id(u.entry).name, self.bank_of(u.entry));
            self.error_at(u.lineno, &msg, Some(&u.filename));
        }

        let mut candidates: Vec<NodeId> = Vec::new();
        for u in &calls {
            let bank = self.bank_of(u.entry);
            let defined = self.id(u.entry).func().is_some_and(|f| !f.forwarded && !f.private && f.local_symbol_table.is_some());
            if bank == 0 || !defined || candidates.contains(&u.entry) {
                continue;
            }
            // An address may go anywhere, and a FARPTR is meant to.
            if calls.iter().any(|c| c.entry == u.entry && c.bank != bank) || addresses.iter().any(|a| a.entry == u.entry) || self.id(u.entry).r.has_faraddress {
                continue;
            }
            candidates.push(u.entry);
        }
        candidates.sort_by_key(|&f| self.lineno(f));
        for f in candidates {
            let (lineno, name, bank, fname) = (self.lineno(f), self.id(f).name.clone(), self.bank_of(f), self.id(f).filename.clone());
            self.warning_could_be_private(lineno, &name, bank, Some(&fname));
        }
    }

    pub fn warning_addressof_array(&mut self, lineno: u32, id: &str, subscripts: &str, fname: Option<&str>) {
        let msg = format!("'@{id}' is the address of the array descriptor, not its data. Since 1.18.3 write '@{id}({subscripts})' for the address of the first element");
        self.coded("920", lineno, &msg, fname);
    }

    pub fn syntax_error_expected_string(&mut self, lineno: u32, type_: &str) {
        self.error(lineno, &format!("Expected a 'string' type expression, got '{type_}' instead"));
    }

    pub fn syntax_error_wrong_for_var(&mut self, lineno: u32, x: &str, y: &str) {
        self.error(lineno, &format!("FOR variable should be '{x}' instead of '{y}'"));
    }

    pub fn syntax_error_not_constant(&mut self, lineno: u32) {
        self.error(lineno, "Initializer expression is not constant.");
    }

    pub fn syntax_error_not_array_nor_func(&mut self, lineno: u32, varname: &str) {
        self.error(lineno, &format!("'{varname}' is neither an array nor a function."));
    }

    pub fn syntax_error_not_an_array(&mut self, lineno: u32, varname: &str) {
        self.error(lineno, &format!("'{varname}' is not an array (or has not been declared yet)"));
    }

    pub fn syntax_error_func_type_mismatch(&mut self, lineno: u32, entry: NodeId) {
        let msg = format!("Function '{}' (previously declared at {}) type mismatch", self.name(entry), self.lineno(entry));
        self.error(lineno, &msg);
    }

    pub fn syntax_error_parameter_mismatch(&mut self, lineno: u32, entry: NodeId) {
        let msg = format!("Function '{}' (previously declared at {}) parameter mismatch", self.name(entry), self.lineno(entry));
        self.error(lineno, &msg);
    }

    pub fn syntax_error_cant_convert_to_type(&mut self, lineno: u32, expr_str: &str, type_: &str) {
        self.error(lineno, &format!("Cant convert '{expr_str}' to type {type_}"));
    }

    pub fn syntax_error_is_a_sub_not_a_func(&mut self, lineno: u32, name: &str) {
        self.error(lineno, &format!("'{name}' is a SUB not a FUNCTION"));
    }

    pub fn syntax_error_undeclared_type(&mut self, lineno: u32, id: &str) {
        self.error(lineno, &format!("strict mode: missing type declaration for '{id}'"));
    }

    pub fn syntax_error_cannot_assign_not_a_var(&mut self, lineno: u32, id: &str) {
        self.error(lineno, &format!("Cannot assign a value to '{id}'. It's not a variable"));
    }

    pub fn syntax_error_address_must_be_constant(&mut self, lineno: u32) {
        self.error(lineno, "Address must be a numeric constant expression");
    }

    pub fn syntax_error_cannot_pass_array_by_value(&mut self, lineno: u32, id: &str) {
        self.error(lineno, &format!("Array parameter '{id}' must be passed ByRef"));
    }

    pub fn syntax_error_no_data_defined(&mut self, lineno: u32) {
        self.error(lineno, "No DATA defined");
    }

    pub fn syntax_error_cannot_initialize_array_of_type(&mut self, lineno: u32, type_name: &str) {
        self.error(lineno, &format!("Cannot initialize array of type {type_name}"));
    }

    pub fn syntax_error_cannot_define_default_array_argument(&mut self, lineno: u32) {
        self.error(lineno, "Cannot define default array argument");
    }

    pub fn syntax_error_unexpected_class(&mut self, lineno: u32, id_name: &str, wrong_class: Class, good_class: Class) {
        let article = |c: Class| if "aeiou".contains(&c.name()[..1]) { "n" } else { "" };
        let msg = format!("'{id_name}' is a{} {}, not a{} {}", article(wrong_class), wrong_class.name().to_uppercase(), article(good_class), good_class.name().to_uppercase());
        self.error(lineno, &msg);
    }

    pub fn syntax_error_already_declared(&mut self, lineno: u32, id_name: &str, as_class: Class, at_lineno: u32) {
        self.error(lineno, &format!("'{id_name}' already declared as {} at {at_lineno}", as_class.name()));
    }

    pub fn syntax_error_mandatory_param_after_optional(&mut self, lineno: u32, param1: &str, param2: &str) {
        self.error(lineno, &format!("Can't declare mandatory param '{param2}' after optional param '{param1}'"));
    }

    pub fn syntax_error_for_without_next(&mut self, lineno: u32) {
        self.error(lineno, "FOR without NEXT");
    }

    pub fn syntax_error_loop_not_closed(&mut self, lineno: u32, loop_type: &str) {
        self.error(lineno, &format!("{loop_type} loop not closed"));
    }

    // ------------------------------------------------------------------
    // The tree: src/ast/tree.py
    // ------------------------------------------------------------------

    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id.0 as usize]
    }

    pub fn node_mut(&mut self, id: NodeId) -> &mut Node {
        &mut self.nodes[id.0 as usize]
    }

    /// A new node with the given children, each of which gets it as parent.
    pub fn new_node(&mut self, kind: Kind, lineno: u32, children: &[Option<NodeId>]) -> NodeId {
        let id = NodeId(self.nodes.len() as u32);
        self.nodes.push(Node::new(kind, lineno));
        for &child in children {
            self.append_child(id, child);
        }
        id
    }

    pub fn children(&self, id: NodeId) -> &[Option<NodeId>] {
        &self.node(id).children
    }

    pub fn child(&self, id: NodeId, n: usize) -> Option<NodeId> {
        self.node(id).children.get(n).copied().flatten()
    }

    /// `children.append(value)`.
    pub fn append_child(&mut self, id: NodeId, child: Option<NodeId>) {
        if let Some(c) = child {
            self.node_mut(c).parent = Some(id);
        }
        self.node_mut(id).children.push(child);
    }

    /// `children.insert(pos, value)`.
    pub fn insert_child(&mut self, id: NodeId, pos: usize, child: NodeId) {
        self.node_mut(child).parent = Some(id);
        self.node_mut(id).children.insert(pos, Some(child));
    }

    /// `children[n] = value`. The old child keeps its parent, as in Python.
    pub fn set_child(&mut self, id: NodeId, n: usize, child: Option<NodeId>) {
        if let Some(c) = child {
            self.node_mut(c).parent = Some(id);
        }
        self.node_mut(id).children[n] = child;
    }

    /// `children.pop(pos)`: the child removed loses its parent.
    pub fn pop_child(&mut self, id: NodeId, pos: usize) -> Option<NodeId> {
        let child = self.node_mut(id).children.remove(pos);
        if let Some(c) = child {
            self.node_mut(c).parent = None;
        }
        child
    }

    /// `node.children = value`: the old children are popped one by one, and
    /// so lose their parent, before the new ones are appended.
    pub fn set_children(&mut self, id: NodeId, children: &[Option<NodeId>]) {
        while let Some(child) = self.node_mut(id).children.pop() {
            if let Some(c) = child {
                self.node_mut(c).parent = None;
            }
        }
        for &child in children {
            self.append_child(id, child);
        }
    }

    // ------------------------------------------------------------------
    // Symbol: dependencies between symbols
    // ------------------------------------------------------------------

    /// `counter.update([key])`. The counters are dicts keyed by node, and a
    /// STRING hashes and compares by its text, so two STRING nodes with the
    /// same text are one key (the first one met).
    fn bump(&mut self, owner: NodeId, required_by: bool, key: NodeId) {
        let text = match &self.node(key).kind {
            Kind::Str { value } => Some(value.clone()),
            _ => None,
        };
        let counter = if required_by { &self.node(owner).required_by } else { &self.node(owner).requires };
        let found = counter.iter().position(|&(k, _)| k == key || text.as_ref().is_some_and(|t| matches!(&self.node(k).kind, Kind::Str { value } if value == t)));
        let node = self.node_mut(owner);
        let counter = if required_by { &mut node.required_by } else { &mut node.requires };
        match found {
            Some(i) => counter[i].1 += 1,
            None => counter.push((key, 1)),
        }
    }

    /// `Symbol.mark_as_required_by`.
    pub fn mark_as_required_by(&mut self, this: NodeId, other: NodeId) {
        if this == other {
            return;
        }
        self.bump(this, true, other);
        self.bump(other, false, this);
        let dependents: Vec<NodeId> = self.node(other).required_by.iter().map(|&(k, _)| k).collect();
        for sym in dependents {
            self.add_required_symbol(sym, this);
        }
    }

    /// `Symbol.add_required_symbol`.
    pub fn add_required_symbol(&mut self, this: NodeId, other: NodeId) {
        if this == other {
            return;
        }
        self.bump(this, false, other);
        self.bump(other, true, this);
        let required: Vec<NodeId> = self.node(other).requires.iter().map(|&(k, _)| k).collect();
        for sym in required {
            self.mark_as_required_by(sym, this);
        }
    }

    /// `Symbol.is_needed`.
    pub fn is_needed(&self, id: NodeId) -> bool {
        !self.node(id).required_by.is_empty()
    }

    // ------------------------------------------------------------------
    // Properties shared by the symbol classes
    // ------------------------------------------------------------------

    /// `node.token`.
    pub fn token(&self, id: NodeId) -> &str {
        match &self.node(id).kind {
            Kind::Nop => "NOP",
            Kind::Block => "BLOCK",
            Kind::Asm { .. } => "ASM",
            Kind::Sentence { keyword, .. } => keyword,
            Kind::VarDecl => "VARDECL",
            Kind::ArrayDecl => "ARRAYDECL",
            Kind::FuncDecl => "FUNCDECL",
            Kind::Bound { .. } => "BOUND",
            Kind::BoundList => "BOUNDLIST",
            Kind::ParamList { .. } => "PARAMLIST",
            Kind::ArgList => "ARGLIST",
            Kind::Argument { .. } => "ARGUMENT",
            Kind::ConstExpr => "CONSTEXPR",
            Kind::Number { .. } => "NUMBER",
            Kind::Str { .. } => "STRING",
            Kind::Unary { .. } => "UNARY",
            Kind::Binary { .. } => "BINARY",
            Kind::TypeCast => "TYPECAST",
            Kind::Builtin { .. } => "BUILTIN",
            Kind::Call(_) => "CALL",
            Kind::FuncCall(_) => "FUNCCALL",
            Kind::ArrayAccess(_) => "ARRAYACCESS",
            Kind::ArrayLoad(_) => "ARRAYLOAD",
            Kind::StrSlice => "STRSLICE",
            Kind::Id(data) => data.token(),
            Kind::Type(_) => "BASICTYPE",
        }
    }

    pub fn lineno(&self, id: NodeId) -> u32 {
        self.node(id).lineno
    }

    pub fn is_id(&self, id: NodeId) -> bool {
        self.node(id).is_id()
    }

    pub fn id(&self, id: NodeId) -> &IdData {
        self.node(id).id()
    }

    pub fn id_mut(&mut self, id: NodeId) -> &mut IdData {
        self.node_mut(id).id_mut()
    }

    /// `node.name` of an identifier.
    pub fn name(&self, id: NodeId) -> &str {
        match &self.node(id).kind {
            Kind::Id(data) => &data.name,
            Kind::Type(basic) => basic.name(),
            Kind::ArrayDecl | Kind::FuncDecl => self.name(self.entry(id)),
            _ => crash(&format!("AttributeError: '{}' object has no attribute 'name'", self.token(id))),
        }
    }

    /// `node.class_`. A node whose class has none is unknown, which is what
    /// the one caller that asks such a node (`SymbolARGUMENT.class_`) gets.
    pub fn class_(&self, id: NodeId) -> Class {
        match &self.node(id).kind {
            Kind::Id(data) => data.class(),
            Kind::Number { .. } | Kind::Str { .. } => Class::Const,
            Kind::Type(_) => Class::Type,
            Kind::Argument { .. } => self.child(id, 0).map_or(Class::Unknown, |v| self.class_(v)),
            _ => Class::Unknown,
        }
    }

    /// `node.scope`, for an identifier or an array access.
    pub fn scope(&self, id: NodeId) -> Scope {
        match &self.node(id).kind {
            Kind::Id(data) => data.scope,
            Kind::ArrayAccess(_) | Kind::ArrayLoad(_) => self.scope(self.entry(id)),
            _ => crash(&format!("AttributeError: '{}' object has no attribute 'scope'", self.token(id))),
        }
    }

    /// `node.entry`: the identifier a declaration or a call is about.
    pub fn entry(&self, id: NodeId) -> NodeId {
        match &self.node(id).kind {
            Kind::VarDecl | Kind::ArrayDecl | Kind::FuncDecl | Kind::Call(_) | Kind::FuncCall(_) | Kind::ArrayAccess(_) | Kind::ArrayLoad(_) => {
                self.child(id, 0).unwrap_or_else(|| crash("AttributeError: 'NoneType' entry"))
            }
            Kind::Id(data) => crash(&format!("AttributeError: '{}' object has no attribute 'entry'", if data.var().is_some() { "VarRef" } else { "SymbolRef" })),
            _ => crash(&format!("AttributeError: '{}' object has no attribute 'entry'", self.token(id))),
        }
    }

    /// `node.type_`. `None` also where the Python class has no such attribute.
    pub fn ty(&self, id: NodeId) -> Option<Ty> {
        let node = self.node(id);
        match &node.kind {
            Kind::Number { .. } | Kind::Str { .. } | Kind::Binary { .. } | Kind::TypeCast | Kind::StrSlice | Kind::Id(_) => node.ty,
            Kind::Unary { .. } | Kind::Builtin { .. } => node.ty.or_else(|| self.child(id, 0).and_then(|operand| self.ty(operand))),
            Kind::ConstExpr | Kind::Argument { .. } => self.child(id, 0).and_then(|c| self.ty(c)),
            Kind::VarDecl | Kind::ArrayDecl | Kind::FuncDecl | Kind::Call(_) | Kind::FuncCall(_) | Kind::ArrayAccess(_) | Kind::ArrayLoad(_) => {
                self.child(id, 0).and_then(|c| self.ty(c))
            }
            Kind::Type(basic) => Some(Ty::new(*basic)),
            _ => None,
        }
    }

    /// The type of a node that has one.
    pub fn ty_of(&self, id: NodeId) -> Ty {
        self.ty(id).unwrap_or_else(|| crash(&format!("AttributeError: '{}' node has no type", self.token(id))))
    }

    pub fn set_ty(&mut self, id: NodeId, ty: Option<Ty>) {
        self.node_mut(id).ty = ty;
    }

    /// `symbol.accessed`.
    pub fn accessed(&self, id: NodeId) -> bool {
        match &self.node(id).kind {
            Kind::Id(data) => data.r.accessed,
            _ => false,
        }
    }

    /// `symbol.accessed = value`. Using a label uses the functions it was
    /// declared in.
    pub fn set_accessed(&mut self, id: NodeId, value: bool) {
        let data = self.id_mut(id);
        data.r.accessed = value;
        if value {
            if let RefKind::Label(label) = &data.r.kind {
                for owner in label.scope_owner.clone() {
                    self.set_accessed(owner, true);
                }
            }
        }
    }

    /// `label.ref.scope_owner = entries`.
    pub fn set_scope_owner(&mut self, id: NodeId, entries: Vec<NodeId>) {
        if let Some(label) = self.id_mut(id).label_mut() {
            label.scope_owner = entries;
        }
        // "if true, refresh scope_owners"
        let accessed = self.accessed(id);
        self.set_accessed(id, accessed);
    }

    /// `node.value` of a constant: a number, a string, or for a CONST whose
    /// value is an expression, its text.
    pub fn value(&mut self, id: NodeId) -> Val {
        match &self.node(id).kind {
            Kind::Number { value } => Val::Num(*value),
            Kind::Str { value } => Val::Str(value.clone()),
            Kind::Id(data) => match data.r.kind {
                RefKind::Const(v) => {
                    if matches!(self.token(v), "NUMBER" | "CONST" | "STRING") {
                        self.value(v)
                    } else {
                        Val::Str(self.t(id))
                    }
                }
                _ => crash(&format!("AttributeError: '{}' object has no attribute 'value'", data.token())),
            },
            _ => crash(&format!("AttributeError: '{}' object has no attribute 'value'", self.token(id))),
        }
    }

    /// `node.value` where the Python code goes on to use it as a number.
    pub fn num_value(&mut self, id: NodeId) -> crate::types::Num {
        self.value(id).num()
    }

    /// `node.t`: the operand text of a node in the intermediate code. Most
    /// nodes take a new temporary the first time they are asked.
    pub fn t(&mut self, id: NodeId) -> String {
        match &self.node(id).kind {
            Kind::Number { value } => value.to_string(),
            Kind::ConstExpr => {
                let text = self.traverse_const(id);
                format!("#{}", text.as_deref().unwrap_or("None"))
            }
            Kind::Argument { byref, .. } => {
                let byref = *byref;
                let value = self.child(id, 0).unwrap_or_else(|| crash("AttributeError: 'NoneType' object has no attribute 't'"));
                let t = self.t(value);
                if byref || !self.ty_of(id).is_dynamic() {
                    return t;
                }
                if self.token(value) == "VAR" && self.scope(value) != Scope::Global {
                    // Without the '$' prefix.
                    return t.chars().skip(1).collect();
                }
                t
            }
            Kind::Id(data) => {
                let dynamic = self.node(id).ty.is_some_and(|ty| ty.is_dynamic());
                match &data.r.kind {
                    RefKind::Unknown => data.r.t.clone(),
                    RefKind::Const(value) => {
                        let value = *value;
                        self.t(value)
                    }
                    RefKind::Label(_) | RefKind::Func(_) => data.mangled.clone(),
                    RefKind::Var(_) | RefKind::Array(_) if data.scope == Scope::Global => {
                        if matches!(data.r.kind, RefKind::Array(_)) {
                            self.data_label(id)
                        } else {
                            data.mangled.clone()
                        }
                    }
                    // Local strings, and string parameters, carry a '$'.
                    RefKind::Var(_) | RefKind::Array(_) => {
                        if dynamic {
                            format!("${}", data.r.t)
                        } else {
                            data.r.t.clone()
                        }
                    }
                }
            }
            _ => {
                if let Some(t) = &self.node(id).t {
                    return t.clone();
                }
                let t = self.new_t();
                self.node_mut(id).t = Some(t.clone());
                t
            }
        }
    }

    /// `str(node)`.
    pub fn str_of(&self, id: NodeId) -> String {
        let join = |c: &Compiler| c.children(id).iter().map(|x| x.map_or("None".to_string(), |x| c.str_of(x))).collect::<Vec<_>>().join(", ");
        let kid = |n: usize| self.child(id, n).map_or("None".to_string(), |x| self.str_of(x));
        match &self.node(id).kind {
            Kind::Id(data) => data.name.clone(),
            Kind::Type(basic) => basic.name().to_string(),
            Kind::Number { value } => value.to_string(),
            Kind::Str { value } => value.clone(),
            Kind::ConstExpr => kid(0),
            Kind::Unary { operator } => format!("{operator}({})", kid(0)),
            Kind::Binary { operator, .. } => format!("{} {operator} {}", kid(0), kid(1)),
            Kind::Bound { lower, upper } => {
                if *lower == 0 {
                    format!("({upper})")
                } else {
                    format!("({lower} TO {upper})")
                }
            }
            Kind::BoundList | Kind::ArgList => format!("({})", join(self)),
            Kind::ArrayDecl => {
                let entry = self.entry(id);
                format!("{}({})", self.name(entry), self.child(entry, 0).map_or(String::new(), |b| self.str_of(b)))
            }
            _ => self.token(id).to_string(),
        }
    }

    // ------------------------------------------------------------------
    // SymbolID and its refs
    // ------------------------------------------------------------------

    /// `SymbolID(name, lineno, filename, type_, class_)`.
    pub fn new_id(&mut self, name: &str, lineno: u32, filename: Option<Rc<str>>, ty: Option<Ty>, class: Class) -> NodeId {
        let filename = filename.unwrap_or_else(|| self.filename());
        let data = IdData {
            name: name.to_string(),
            original_name: name.to_string(),
            filename,
            mangled: format!("{MANGLE_CHR}{name}"),
            declared: false,
            caseins: self.opts.bool("case_insensitive"),
            scope: Scope::Global,
            scope_ref: None,
            addr: None,
            has_address: None,
            r: self.new_ref(RefKind::Unknown),
        };
        let id = self.new_node(Kind::Id(Box::new(data)), lineno, &[]);
        self.node_mut(id).ty = ty;
        match class {
            Class::Var => {
                self.to_var(id, None);
            }
            Class::Label => {
                self.to_label(id);
            }
            Class::Const | Class::Unknown => {}
            _ => crash("AssertionError: class_ in (CLASS.const, CLASS.label, CLASS.var, CLASS.unknown)"),
        }
        id
    }

    /// A new ref object. Each takes a temporary, used or not.
    fn new_ref(&mut self, kind: RefKind) -> Ref {
        Ref { t: self.new_t(), callable: false, accessed: false, offset: None, has_faraddress: false, kind }
    }

    fn check_has_address(&mut self, id: NodeId, address: bool) {
        let data = self.id_mut(id);
        if data.has_address.is_some_and(|h| h != address) {
            crash("AssertionError: identifier with and without an address");
        }
        data.has_address = Some(address);
    }

    /// `SymbolID.to_var`.
    pub fn to_var(&mut self, id: NodeId, default_value: Option<NodeId>) -> NodeId {
        let class = self.class_(id);
        if class != Class::Unknown && class != Class::Var {
            crash("AssertionError: to_var() on an identifier of another class");
        }
        self.check_has_address(id, true);
        if class == Class::Var {
            self.id_mut(id).var_mut().unwrap().default_value = default_value;
            return id;
        }
        let accessed = self.id(id).r.accessed;
        let mut r = self.new_ref(RefKind::Var(VarRef { default_value, ..VarRef::default() }));
        r.accessed = accessed;
        self.id_mut(id).r = r;
        id
    }

    /// `SymbolID.to_const`.
    pub fn to_const(&mut self, id: NodeId, default_value: NodeId) -> NodeId {
        if self.class_(id) != Class::Unknown {
            crash("AssertionError: to_const() on an identifier of another class");
        }
        self.check_has_address(id, false);
        if !matches!(self.token(default_value), "CONSTEXPR" | "NUMBER" | "CONST" | "STRING") {
            crash("AssertionError: default_value.token in (\"CONSTEXPR\", \"NUMBER\", \"CONST\", \"STRING\")");
        }
        let r = self.new_ref(RefKind::Const(default_value));
        self.id_mut(id).r = r;
        id
    }

    /// `SymbolID.to_label`.
    pub fn to_label(&mut self, id: NodeId) -> NodeId {
        if !matches!(self.class_(id), Class::Unknown | Class::Label) {
            crash("AssertionError: to_label() on an identifier of another class");
        }
        self.check_has_address(id, true);
        let accessed = self.id(id).r.accessed;
        let name = self.id(id).name.clone();
        // str.isdecimal(): not empty, and digits only.
        let is_line_number = !name.is_empty() && name.chars().all(|c| c.is_ascii_digit());
        let r = self.new_ref(RefKind::Label(LabelRef { bank: 0, scope_owner: Vec::new(), is_line_number }));
        let data = self.id_mut(id);
        data.mangled = format!("{LABELS_NAMESPACE}.{MANGLE_CHR}{name}");
        data.r = r;
        self.set_accessed(id, accessed);
        id
    }

    /// `SymbolID.to_function`.
    pub fn to_function(&mut self, id: NodeId, lineno: Option<u32>, class: Class) -> NodeId {
        if self.class_(id) != Class::Unknown {
            crash("AssertionError: to_function() on an identifier of another class");
        }
        self.check_has_address(id, true);
        let accessed = self.id(id).r.accessed;
        let mut r = self.new_ref(RefKind::Func(FuncRef {
            class,
            bank: 0,
            private: false,
            local_symbol_table: None,
            convention: Convention::Unknown,
            forwarded: false,
            params_size: 0,
            locals_size: 0,
        }));
        r.callable = true;
        self.id_mut(id).r = r;
        let params = self.new_node(Kind::ParamList { size: 0 }, 0, &[]);
        let body = self.new_node(Kind::Block, 0, &[]);
        self.set_children(id, &[Some(params), Some(body)]);
        if let Some(lineno) = lineno {
            self.node_mut(id).lineno = lineno;
        }
        self.set_accessed(id, accessed);
        id
    }

    /// `SymbolID.to_vararray`.
    pub fn to_vararray(&mut self, id: NodeId, bounds: NodeId) -> NodeId {
        if self.class_(id) != Class::Unknown {
            crash("AssertionError: to_vararray() on an identifier of another class");
        }
        self.check_has_address(id, true);
        let accessed = self.id(id).r.accessed;
        let mut r = self.new_ref(RefKind::Array(VarRef::default()));
        r.callable = true;
        self.id_mut(id).r = r;
        self.set_children(id, &[Some(bounds)]);
        self.set_accessed(id, accessed);
        id
    }

    /// `funcdecl.params_size = value`.
    pub fn set_funcdecl_params_size(&mut self, funcdecl: NodeId, value: i64) {
        self.funcdecl_params_size.insert(funcdecl, value);
    }

    /// `funcdecl.params_size`. Where the grammar action did not record it,
    /// it is what the action would have recorded: the size of the parameter
    /// list it appended as the FUNCDECL's second child.
    pub fn get_funcdecl_params_size(&self, funcdecl: NodeId) -> i64 {
        if let Some(&size) = self.funcdecl_params_size.get(&funcdecl) {
            return size;
        }
        match self.child(funcdecl, 1).map(|list| &self.node(list).kind) {
            Some(Kind::ParamList { size }) => *size,
            _ => crash("AttributeError: 'SymbolFUNCDECL' object has no attribute 'params_size'"),
        }
    }

    /// `FuncRef.params`.
    pub fn params(&self, func: NodeId) -> NodeId {
        self.child(func, 0).unwrap_or_else(|| crash("AttributeError: function without a parameter list"))
    }

    /// `FuncRef.body`.
    pub fn body(&self, func: NodeId) -> NodeId {
        self.child(func, 1).unwrap_or_else(|| crash("AttributeError: function without a body"))
    }

    /// `FuncRef.body = value`; no value is an empty block.
    pub fn set_body(&mut self, func: NodeId, body: Option<NodeId>) {
        let body = body.unwrap_or_else(|| self.new_node(Kind::Block, 0, &[]));
        self.set_child(func, 1, Some(body));
    }

    /// `ArrayRef.bounds`.
    pub fn bounds(&self, array: NodeId) -> NodeId {
        let array = if self.is_id(array) { array } else { self.entry(array) };
        self.child(array, 0).unwrap_or_else(|| crash("AttributeError: array without bounds"))
    }

    /// `SymbolBOUND.count`.
    pub fn bound_count(&self, bound: NodeId) -> i128 {
        match self.node(bound).kind {
            Kind::Bound { lower, upper } => upper - lower + 1,
            _ => crash("AttributeError: not a BOUND"),
        }
    }

    /// `ArrayRef.count`: the number of cells. `functools.reduce` of nothing
    /// raises, which a parameter array (no bounds) would do.
    pub fn count(&self, array: NodeId) -> i128 {
        let bounds = self.bounds(array);
        let counts: Vec<i128> = self.children(bounds).iter().flatten().map(|&b| self.bound_count(b)).collect();
        if counts.is_empty() {
            crash("TypeError: reduce() of empty iterable with no initial value");
        }
        counts.into_iter().product()
    }

    /// `ArrayRef.memsize`: the size of the array's descriptor.
    pub fn memsize(&self, array: NodeId) -> i64 {
        let array = if self.is_id(array) { array } else { self.entry(array) };
        let ubound_used = self.id(array).var().is_some_and(|v| v.ubound_used);
        PTR_TYPE.size() * (3 + ubound_used as i64)
    }

    /// `ArrayRef.data_label`.
    pub fn data_label(&self, array: NodeId) -> String {
        format!("{}.{ARRAY_DATA_PREFIX}", self.id(array).mangled)
    }

    /// `ArrayRef.data_ptr_label`.
    pub fn data_ptr_label(&self, array: NodeId) -> String {
        format!("{}.__PTR__", self.data_label(array))
    }

    /// `ArrayRef.is_zero_based`.
    pub fn is_zero_based(&self, array: NodeId) -> bool {
        let bounds = self.bounds(array);
        self.children(bounds).iter().flatten().all(|&b| matches!(self.node(b).kind, Kind::Bound { lower: 0, .. }))
    }

    /// `node.size`: the bytes a value of the node's type takes, or for an
    /// identifier what its storage takes.
    pub fn size(&self, id: NodeId) -> i64 {
        let type_size = |c: &Compiler| c.ty(id).map_or(0, |t| t.size());
        match &self.node(id).kind {
            Kind::Id(data) => match &data.r.kind {
                RefKind::Var(v) => match self.node(id).ty {
                    None => 0,
                    Some(_) if data.scope == Scope::Parameter && v.byref => PTR_TYPE.size(),
                    // Parameters are pushed as whole words.
                    Some(t) if data.scope == Scope::Parameter => t.size() + t.size() % PARAM_ALIGN,
                    Some(t) => t.size(),
                },
                RefKind::Array(_) => {
                    if data.scope != Scope::Parameter {
                        self.count(id) as i64 * self.ty_of(id).size()
                    } else {
                        PTR_TYPE.size()
                    }
                }
                RefKind::Func(_) => type_size(self),
                _ => crash(&format!("AttributeError: '{}' object has no attribute 'size'", data.token())),
            },
            Kind::ArrayDecl => self.ty_of(id).size() * self.count(self.entry(id)) as i64,
            Kind::VarDecl => self.size(self.entry(id)),
            Kind::ParamList { size } => *size,
            Kind::Type(basic) => basic.size(),
            _ => type_size(self),
        }
    }

    /// `getattr(entry.ref, "bank", 0) or 0`.
    pub fn bank_of(&self, id: NodeId) -> i64 {
        match &self.node(id).kind {
            Kind::Id(data) => data.bank(),
            _ => 0,
        }
    }

    /// `codebank.bank_of_var`: the bank holding a global's storage. A scalar
    /// `DIM x AT addr` has none of its own.
    pub fn bank_of_var(&self, entry: Option<NodeId>) -> i64 {
        let Some(entry) = entry else { return 0 };
        let Kind::Id(data) = &self.node(entry).kind else { return 0 };
        if data.addr.is_some() && data.class() != Class::Array {
            return 0;
        }
        data.bank()
    }

    /// `codebank.far_alias`: the label a FARPTR names.
    pub fn far_alias(&self, entry: NodeId, name: &str) -> String {
        if self.bank_of_var(Some(entry)) != 0 { format!("{name}{FAR_ADDR_SUFFIX}") } else { name.to_string() }
    }

    // ------------------------------------------------------------------
    // TranslatorVisitor.traverse_const, which SymbolCONSTEXPR.t needs
    // ------------------------------------------------------------------

    /// The assembler expression of a constant expression, or `None` after
    /// reporting why there is none.
    pub fn traverse_const(&mut self, node: NodeId) -> Option<String> {
        // What Python prints for a part that came out as None.
        let show = |part: Option<String>| part.unwrap_or_else(|| "None".to_string());
        match self.token(node) {
            "NUMBER" | "CONST" => return Some(self.t(node)),
            "UNARY" => {
                let Kind::Unary { operator } = &self.node(node).kind else { unreachable!() };
                let operator = operator.clone();
                let operand = self.child(node, 0).unwrap_or_else(|| crash("AttributeError: UNARY without an operand"));
                return match operator.as_str() {
                    "MINUS" => {
                        let inner = self.traverse_const(operand);
                        // " -" + None raises TypeError.
                        Some(format!(" -{}", inner.unwrap_or_else(|| crash("TypeError: can only concatenate str (not \"NoneType\") to str"))))
                    }
                    "FARADDRESS" => Some(self.traverse_far_address(operand)),
                    "ADDRESS" => {
                        let is_global = match &self.node(operand).kind {
                            Kind::Id(data) => data.scope == Scope::Global,
                            Kind::ArrayAccess(_) | Kind::ArrayLoad(_) => self.scope(operand) == Scope::Global,
                            _ => crash(&format!("AttributeError: '{}' object has no attribute 'scope'", self.token(operand))),
                        };
                        if is_global || matches!(self.token(operand), "LABEL" | "FUNCTION") {
                            self.traverse_const(operand)
                        } else {
                            let lineno = self.lineno(operand);
                            self.syntax_error_not_constant(lineno);
                            None
                        }
                    }
                    other => crash(&format!("InvalidOperatorError: Invalid operator \"{other}\"")),
                };
            }
            "BINARY" => {
                let Kind::Binary { operator, .. } = &self.node(node).kind else { unreachable!() };
                // The Python table has SHL and SHR the wrong way round, so
                // that `@a << 1` halves the address. Not copied.
                let mid = match operator.as_str() {
                    "PLUS" => "+",
                    "MINUS" => "-",
                    "MUL" => "*",
                    "DIV" => "/",
                    "MOD" => "%",
                    "POW" => "^",
                    "SHL" => "<<",
                    "SHR" => ">>",
                    other => crash(&format!("InvalidOperatorError: Invalid operator \"{other}\"")),
                };
                let (left, right) = (self.child(node, 0).unwrap(), self.child(node, 1).unwrap());
                let left = self.traverse_const(left);
                let right = self.traverse_const(right);
                return Some(format!("({}) {mid} ({})", show(left), show(right)));
            }
            "TYPECAST" => {
                let ty = self.ty_of(node);
                let operand = self.child(node, 0).unwrap();
                use BasicType::*;
                let (open, close) = match ty.basic {
                    Byte | UByte => ("(", ") & 0xFF"),
                    Integer | UInteger => ("(", ") & 0xFFFF"),
                    Long | ULong => ("(", ") & 0xFFFFFFFF"),
                    Fixed => ("((", ") & 0xFFFF) << 16"),
                    _ => {
                        let (lineno, text) = (self.lineno(node), self.str_of(operand));
                        self.syntax_error_cant_convert_to_type(lineno, &text, ty.name());
                        return None;
                    }
                };
                let inner = self.traverse_const(operand);
                return Some(format!("{open}{}{close}", inner.unwrap_or_else(|| crash("TypeError: can only concatenate str (not \"NoneType\") to str"))));
            }
            "VAR" | "LABEL" | "FUNCTION" => return Some(self.t(node)),
            "CONSTEXPR" => {
                let expr = self.child(node, 0).unwrap();
                return self.traverse_const(expr);
            }
            "ARRAYACCESS" => {
                let entry = self.entry(node);
                let offset = self.array_offset(node);
                return Some(format!("({} + {})", self.data_label(entry), offset.map_or("None".to_string(), |o| o.to_string())));
            }
            _ => {}
        }
        if let Kind::Id(data) = &self.node(node).kind {
            if matches!(data.token(), "ID" | "VARARRAY") && data.has_address == Some(true) && data.scope == Scope::Global {
                return Some(data.mangled.clone());
            }
        }
        let t = self.t(node);
        crash(&format!("InvalidCONSTexpr: Invalid CONST expression: {}|{t}", self.token(node)))
    }

    /// `TranslatorVisitor.traverse_far_address`: `FARPTR x` as an assembler
    /// expression, `(bank << 16) | address`.
    pub fn traverse_far_address(&mut self, node: NodeId) -> String {
        let (entry, base) = if self.token(node) == "ARRAYACCESS" {
            let entry = self.entry(node);
            let offset = self.array_offset(node);
            let label = self.data_label(entry);
            (entry, format!("{} + {}", self.far_alias(entry, &label), offset.map_or("None".to_string(), |o| o.to_string())))
        } else {
            let name = if self.class_(node) == Class::Array { self.data_label(node) } else { self.t(node) };
            (node, self.far_alias(node, &name))
        };
        let bank = self.bank_of_var(Some(entry));
        if bank == 0 { format!("({base})") } else { format!("({base}) + {}", bank * 0x10000) }
    }
}

/// The identifiers a block of asm mentions, and those it declares as labels
/// (`name:` or `name EQU`), each without the dot of the global namespace.
fn asm_names(asm: &str, named: &mut HashSet<String>, defined: &mut HashSet<String>) {
    for line in asm.lines() {
        let text = line.as_bytes();
        let mut last: Option<String> = None;
        let mut i = 0;
        while i < text.len() {
            let c = text[i];
            if c == b';' {
                break;
            }
            if c == b'"' {
                i += 1;
                while i < text.len() && text[i] != b'"' {
                    i += 1;
                }
                i += 1;
                continue;
            }
            // A character: 'x'. An apostrophe on its own is AF'.
            if c == b'\'' && text.get(i + 2) == Some(&b'\'') {
                i += 3;
                continue;
            }
            if c.is_ascii_alphabetic() || c == b'_' || c == b'.' {
                let start = i;
                while i < text.len() && (text[i].is_ascii_alphanumeric() || text[i] == b'_' || text[i] == b'.') {
                    i += 1;
                }
                let word = line[start..i].strip_prefix('.').unwrap_or(&line[start..i]).to_string();
                if word.eq_ignore_ascii_case("equ") {
                    defined.extend(last.take());
                }
                named.insert(word.clone());
                last = Some(word);
                continue;
            }
            if c == b':' {
                defined.extend(last.take());
            } else if !c.is_ascii_whitespace() {
                last = None;
            }
            i += 1;
        }
    }
}
