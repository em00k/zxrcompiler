//! The text of the intermediate seams, as `tools/oracle_seam.py` writes it
//! from the Python compiler. The formats are described in `docs/seams.md`;
//! each function here has a counterpart of the same name there, and the two
//! must be changed together.
//!
//! Nothing here takes a temporary or a label: a node's `t` is shown as it
//! stands, taken or not.

use crate::ast::*;
use crate::compiler::Compiler;
use crate::options::OptValue;
use std::collections::BTreeSet;
use std::fmt::Write;

/// Text on one line: printable ASCII as it is, the rest escaped.
fn esc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            ' '..='~' => out.push(c),
            c => write!(out, "\\u{{{:x}}}", c as u32).unwrap(),
        }
    }
    out
}

/// One record of the state dump: fields separated by tabs.
fn record(fields: &[&str]) -> String {
    let fields: Vec<String> = fields.iter().map(|f| f.replace('\\', "\\\\").replace('\t', "\\t").replace('\n', "\\n").replace('\r', "\\r")).collect();
    format!("{}\n", fields.join("\t"))
}

/// `str()` of a Python bool.
fn py(value: bool) -> &'static str {
    if value { "True" } else { "False" }
}

/// `str()` of an optional value.
fn opt<T: ToString>(value: Option<T>) -> String {
    value.map_or("None".to_string(), |v| v.to_string())
}

/// `repr()` of a namespace, escaped.
fn namespace(name: &str) -> String {
    esc(&zxr_core::pynum::str_repr(name))
}

struct Dumper<'a> {
    c: &'a Compiler,
    out: String,
    /// The functions whose body has been written.
    bodies: BTreeSet<NodeId>,
}

impl Dumper<'_> {
    fn line(&mut self, depth: usize, text: &str) {
        for _ in 0..depth {
            self.out.push_str("  ");
        }
        self.out.push_str(text);
        self.out.push('\n');
    }

    /// The name of the Python class of a node.
    fn class_name(&self, node: NodeId) -> &'static str {
        match &self.c.node(node).kind {
            Kind::Nop => "SymbolNOP",
            Kind::Block => "SymbolBLOCK",
            Kind::Asm { .. } => "SymbolASM",
            Kind::Sentence { .. } => "SymbolSENTENCE",
            Kind::VarDecl => "SymbolVARDECL",
            Kind::ArrayDecl => "SymbolARRAYDECL",
            Kind::FuncDecl => "SymbolFUNCDECL",
            Kind::Bound { .. } => "SymbolBOUND",
            Kind::BoundList => "SymbolBOUNDLIST",
            Kind::ParamList { .. } => "SymbolPARAMLIST",
            Kind::ArgList => "SymbolARGLIST",
            Kind::Argument { .. } => "SymbolARGUMENT",
            Kind::ConstExpr => "SymbolCONSTEXPR",
            Kind::Number { .. } => "SymbolNUMBER",
            Kind::Str { .. } => "SymbolSTRING",
            Kind::Unary { .. } => "SymbolUNARY",
            Kind::Binary { .. } => "SymbolBINARY",
            Kind::TypeCast => "SymbolTYPECAST",
            Kind::Builtin { .. } => "SymbolBUILTIN",
            Kind::Call(_) => "SymbolCALL",
            Kind::FuncCall(_) => "SymbolFUNCCALL",
            Kind::ArrayAccess(_) => "SymbolARRAYACCESS",
            Kind::ArrayLoad(_) => "SymbolARRAYLOAD",
            Kind::StrSlice => "SymbolSTRSLICE",
            Kind::Id(_) => "SymbolID",
            Kind::Type(_) => "SymbolBASICTYPE",
        }
    }

    /// A type: its name, with `?` after one that was assumed, not declared.
    fn type_text(ty: Option<crate::types::Ty>) -> String {
        match ty {
            None => "None".to_string(),
            Some(t) => format!("{}{}", t.name(), if t.implicit { "?" } else { "" }),
        }
    }

    /// Names a node in one word: an identifier by its mangled name.
    fn who(&self, node: Option<NodeId>) -> String {
        match node {
            None => "None".to_string(),
            Some(n) => match &self.c.node(n).kind {
                Kind::Id(data) => data.mangled.clone(),
                _ => self.c.token(n).to_string(),
            },
        }
    }

    fn counter_text(&self, counter: &[(NodeId, u32)]) -> String {
        counter.iter().map(|&(sym, n)| format!("{}:{n}", self.who(Some(sym)))).collect::<Vec<_>>().join(",")
    }

    fn id_ref(&self, node: NodeId) -> String {
        let data = self.c.id(node);
        format!("{} SymbolID {} {} {}", data.token(), esc(&data.name), esc(&data.mangled), data.scope.name())
    }

    fn node_text(&self, id: NodeId) -> String {
        let node = self.c.node(id);
        let mut parts: Vec<String> = vec![self.c.token(id).to_string(), self.class_name(id).to_string()];
        let with_lineno = !matches!(
            node.kind,
            Kind::Nop | Kind::Block | Kind::VarDecl | Kind::ArrayDecl | Kind::Bound { .. } | Kind::BoundList | Kind::ParamList { .. } | Kind::ArgList | Kind::Id(_) | Kind::Type(_)
        );
        if with_lineno {
            parts.push(format!("L{}", node.lineno));
        }
        if matches!(node.kind, Kind::Number { .. } | Kind::Str { .. } | Kind::Binary { .. } | Kind::TypeCast | Kind::StrSlice | Kind::Unary { .. } | Kind::Builtin { .. }) {
            parts.push(format!("type={}", Self::type_text(node.ty)));
        }
        match &node.kind {
            Kind::Asm { asm, filename, is_sentinel, bank } => {
                parts.extend([format!("file={}", esc(filename)), format!("sentinel={}", py(*is_sentinel)), format!("bank={bank}"), format!("asm={}", esc(asm))]);
            }
            Kind::Sentence { filename, is_sentinel, .. } => {
                parts.extend([format!("file={}", esc(filename)), format!("sentinel={}", py(*is_sentinel))]);
            }
            Kind::Bound { lower, upper } => parts.extend([format!("lower={lower}"), format!("upper={upper}")]),
            Kind::ParamList { size } => parts.push(format!("size={size}")),
            Kind::Argument { filename, byref, name } => {
                parts.extend([format!("file={}", esc(filename)), format!("byref={}", py(*byref)), format!("name={}", name.as_deref().map_or("None".to_string(), esc))]);
            }
            Kind::Number { value } => parts.push(format!("value={value}")),
            Kind::Str { value } => parts.push(format!("value={}", esc(value))),
            Kind::Unary { operator } => parts.push(format!("op={operator}")),
            Kind::Binary { operator, func } => parts.extend([format!("op={operator}"), format!("func={}", func.is_some() as u8)]),
            Kind::Builtin { fname, discard_result } => parts.extend([format!("fname={}", esc(fname)), format!("discard={}", py(*discard_result))]),
            Kind::Call(call) | Kind::FuncCall(call) | Kind::ArrayAccess(call) | Kind::ArrayLoad(call) => {
                parts.push(format!("file={}", esc(&call.filename)));
                parts.push(match call.offset {
                    None => "offset=unset".to_string(),
                    Some(offset) => format!("offset={}", opt(offset)),
                });
            }
            _ => {}
        }
        match (&node.kind, &node.t) {
            (Kind::Str { .. }, t) => parts.push(format!("t={}", esc(t.as_deref().unwrap_or("None")))),
            (_, Some(t)) => parts.push(format!("t={}", esc(t))),
            _ => {}
        }
        if !node.requires.is_empty() {
            parts.push(format!("requires={}", self.counter_text(&node.requires)));
        }
        if !node.required_by.is_empty() {
            parts.push(format!("required_by={}", self.counter_text(&node.required_by)));
        }
        parts.join(" ")
    }

    /// A node and what is under it. An identifier is a reference, one line;
    /// the parameters and body of a function are written under its FUNCDECL,
    /// once.
    fn tree(&mut self, node: Option<NodeId>, depth: usize) {
        let Some(node) = node else {
            self.line(depth, "None");
            return;
        };
        if self.c.is_id(node) {
            let text = self.id_ref(node);
            self.line(depth, &text);
            return;
        }
        let text = self.node_text(node);
        self.line(depth, &text);
        if matches!(self.c.node(node).kind, Kind::FuncDecl) {
            match self.c.child(node, 0) {
                Some(entry) => {
                    let text = self.id_ref(entry);
                    self.line(depth + 1, &text);
                    self.function(entry, depth + 2);
                }
                None => self.line(depth + 1, "None"),
            }
            return;
        }
        for child in self.c.children(node).to_vec() {
            self.tree(child, depth + 1);
        }
    }

    fn function(&mut self, entry: NodeId, depth: usize) {
        if !self.bodies.insert(entry) {
            return;
        }
        for child in self.c.children(entry).to_vec() {
            self.tree(child, depth);
        }
    }

    /// The initial value of a variable. For an array the Python compiler
    /// keeps nested lists of nodes, written as `[` ... `]` around the items
    /// of each list; however the port comes to hold them, this is where they
    /// are written.
    fn default_vector(&mut self, value: &ConstVector, depth: usize) {
        match value {
            ConstVector::Elem(node) => self.tree(Some(*node), depth),
            ConstVector::List(items) => {
                self.line(depth, "[");
                for item in items {
                    self.default_vector(item, depth + 1);
                }
                self.line(depth, "]");
            }
        }
    }

    fn entry(&mut self, key: &str, node: NodeId) {
        let c = self.c;
        let Kind::Id(data) = &c.node(node).kind else {
            let text = format!("type {} {} {}", esc(key), self.class_name(node), esc(c.name(node)));
            self.line(1, &text);
            return;
        };
        let r = &data.r;
        let text = format!(
            "id {} token={} class={} name={} orig={} mangled={} scope={} type={} L{} file={} declared={} accessed={} caseins={} has_address={} in={} t={} callable={} offset={} faraddr={}",
            esc(key),
            data.token(),
            data.class().name(),
            esc(&data.name),
            esc(&data.original_name),
            esc(&data.mangled),
            data.scope.name(),
            Self::type_text(c.node(node).ty),
            c.node(node).lineno,
            esc(&data.filename),
            py(data.declared),
            py(r.accessed),
            py(data.caseins),
            data.has_address.map_or("None", py),
            data.scope_ref.map_or("-".to_string(), |s| namespace(&c.scope_data(s).namespace)),
            r.t,
            py(r.callable),
            opt(r.offset),
            py(r.has_faraddress),
        );
        self.line(1, &text);
        if !c.node(node).requires.is_empty() {
            let text = format!("requires {}", self.counter_text(&c.node(node).requires));
            self.line(2, &text);
        }
        if !c.node(node).required_by.is_empty() {
            let text = format!("required_by {}", self.counter_text(&c.node(node).required_by));
            self.line(2, &text);
        }
        if let Some(addr) = data.addr {
            self.line(2, "addr:");
            self.tree(Some(addr), 3);
        }
        match &r.kind {
            RefKind::Unknown => {}
            RefKind::Var(v) | RefKind::Array(v) => {
                let text = format!("var byref={} alias={} bank={}", py(v.byref), self.who(v.alias), v.bank);
                self.line(2, &text);
                if matches!(r.kind, RefKind::Array(_)) {
                    let text = format!("array lbound_used={} ubound_used={} dynamic={}", py(v.lbound_used), py(v.ubound_used), py(v.is_dynamically_accessed));
                    self.line(2, &text);
                    self.line(2, "bounds:");
                    for child in c.children(node).to_vec() {
                        self.tree(child, 3);
                    }
                }
                if let Some(value) = v.default_value {
                    self.line(2, "default_value:");
                    self.tree(Some(value), 3);
                } else if let Some(vector) = &v.default_vector {
                    self.line(2, "default_value:");
                    self.default_vector(vector, 3);
                }
            }
            RefKind::Const(value) => {
                self.line(2, "value:");
                self.tree(Some(*value), 3);
            }
            RefKind::Label(label) => {
                let owners: Vec<String> = label.scope_owner.iter().map(|&o| esc(&c.id(o).mangled)).collect();
                let text = format!("label bank={} line_number={} owners={}", label.bank, py(label.is_line_number), owners.join(","));
                self.line(2, &text);
            }
            RefKind::Func(func) => {
                let text = format!(
                    "func bank={} convention={} forwarded={} params_size={} locals_size={} locals={}",
                    func.bank,
                    func.convention.name(),
                    py(func.forwarded),
                    func.params_size,
                    func.locals_size,
                    func.local_symbol_table.map_or("-".to_string(), |s| namespace(&c.scope_data(s).namespace)),
                );
                self.line(2, &text);
                if self.bodies.contains(&node) {
                    self.line(2, "body: above");
                } else {
                    self.line(2, "body:");
                    self.function(node, 3);
                }
            }
        }
    }

    fn scopes(&mut self) {
        let c = self.c;
        for (name, &scope) in c.namespaces.iter() {
            let data = c.scope_data(scope);
            let owner = data.owner.and_then(|o| c.child(o, 0)).map_or("-".to_string(), |e| esc(&c.id(e).mangled));
            let text = format!("scope {} parent={} owner={owner}", namespace(name), data.parent_namespace.as_deref().map_or("-".to_string(), namespace));
            self.line(0, &text);
            for (key, &node) in data.symbols.iter() {
                self.entry(key, node);
            }
            for (key, &node) in data.caseins.iter() {
                let text = format!("caseins {} {}", esc(key), self.who(Some(node)));
                self.line(1, &text);
            }
        }
    }
}

impl Compiler {
    fn option_text(value: &OptValue) -> String {
        esc(&value.to_string())
    }

    /// The options, in the order the Python compiler registers them, and the
    /// macros it has defined. `output_filename` and `project_filename` hold
    /// paths that differ from run to run and are left out.
    fn options_lines(&self) -> String {
        let mut out = String::new();
        for option in &self.opts.list {
            if matches!(option.name, "output_filename" | "project_filename") {
                continue;
            }
            writeln!(out, "option {}={}", option.name, Self::option_text(&option.value)).unwrap();
        }
        for (name, value) in &self.defines {
            writeln!(out, "define {}={}", esc(name), esc(value)).unwrap();
        }
        out
    }

    /// The `ast` and `ast2` stages: the tree, the symbol table and the
    /// globals the parser fills.
    pub fn dump_ast(&mut self) -> String {
        let mut d = Dumper { c: self, out: String::new(), bodies: BTreeSet::new() };
        d.line(0, "--- ast");
        d.tree(d.c.ast, 0);
        d.line(0, "--- data_ast");
        d.tree(d.c.data_ast, 0);
        d.line(0, "--- datas");
        for data in d.c.datas.clone() {
            let text = format!("data {}", esc(&d.c.id(data.label).mangled));
            d.line(0, &text);
            for item in data.datas {
                d.tree(Some(item), 1);
            }
        }
        d.line(0, "--- symbols");
        d.scopes();
        d.line(0, "--- globals");
        let mut out = d.out;
        let mangled = |n: NodeId| esc(&self.id(n).mangled);
        writeln!(out, "temporaries {}", self.temp_counter).unwrap();
        writeln!(out, "tmp_labels {}", self.label_counter).unwrap();
        for (name, target) in self.data_labels.iter() {
            writeln!(out, "data_label {} {}", esc(name), esc(target)).unwrap();
        }
        let mut required = self.data_labels_required.clone();
        required.sort();
        required.dedup();
        for name in &required {
            writeln!(out, "data_label_required {}", esc(name)).unwrap();
        }
        writeln!(out, "data_ptr_current {}", esc(&self.data_ptr_current)).unwrap();
        writeln!(out, "data_is_used {}", py(self.data_is_used)).unwrap();
        for &func in &self.data_functions {
            writeln!(out, "data_function {}", mangled(func)).unwrap();
        }
        let mut inits = self.inits.clone();
        inits.sort();
        inits.dedup();
        for name in &inits {
            writeln!(out, "init {}", esc(name)).unwrap();
        }
        for name in &self.backend_inits {
            writeln!(out, "backend_init {}", esc(name)).unwrap();
        }
        for name in &self.requires {
            writeln!(out, "require {}", esc(name)).unwrap();
        }
        for &call in &self.function_calls {
            let entry = self.child(call, 0).map_or("None".to_string(), mangled);
            writeln!(out, "function_call {entry} L{}", self.lineno(call)).unwrap();
        }
        for &func in &self.functions {
            writeln!(out, "function {}", mangled(func)).unwrap();
        }
        for &func in &self.functions_bankinit {
            writeln!(out, "function_bankinit {}", mangled(func)).unwrap();
        }
        writeln!(out, "print_is_used {}", py(self.print_is_used)).unwrap();
        writeln!(out, "warnings {}", self.sess.has_warnings).unwrap();
        out.push_str(&self.options_lines());
        out
    }

    /// The state the backend reads besides the quads, for the `ir` stage:
    /// records in the vocabulary of `tools/oracle_backend_in.py`.
    pub fn dump_backend_state(&mut self) -> String {
        const STATE_OPTIONS: [&str; 24] = [
            "architecture",
            "org",
            "heap_size",
            "heap_address",
            "heap_start_label",
            "heap_size_label",
            "headerless",
            "autorun",
            "optimization_level",
            "opt_strategy",
            "memory_check",
            "array_check",
            "enable_break",
            "zxnext",
            "force_asm_brackets",
            "use_basic_loader",
            "output_file_type",
            "codebank",
            "codewindow",
            "codewindowsize",
            "codebankbase",
            "codebankpages",
            "codebankdepth",
            "input_filename",
        ];
        let mut out = String::new();
        for name in STATE_OPTIONS {
            out.push_str(&record(&["option", name, &self.opts.get(name).to_string()]));
        }
        for (name, value) in &self.defines {
            out.push_str(&record(&["define", name, value]));
        }
        for name in &self.requires {
            out.push_str(&record(&["require", name]));
        }
        for name in &self.backend_inits {
            out.push_str(&record(&["init", name]));
        }
        out.push_str(&record(&["flag", "use_function_exit", &(self.flag_use_function_exit as u8).to_string()]));
        out.push_str(&record(&["flag", "end_emitted", &(self.flag_end_emitted as u8).to_string()]));
        for line in &self.at_end {
            out.push_str(&record(&["at_end", line]));
        }
        for (label, body) in self.far_trampolines.iter() {
            out.push_str(&record(&["far_trampoline", label, body]));
        }
        for bank in &self.banks_used {
            out.push_str(&record(&["bank_used", &bank.to_string()]));
        }
        for bank in &self.no_routine_warned {
            out.push_str(&record(&["bank_warned", &bank.to_string()]));
        }
        // The first declaration in each bank, as emit_banked_vars() finds it.
        let mut declared: Vec<(i64, NodeId)> = Vec::new();
        if let Some(data_ast) = self.data_ast {
            for &child in self.children(data_ast) {
                let Some(entry) = child.filter(|&c| matches!(self.node(c).kind, Kind::VarDecl | Kind::ArrayDecl)).and_then(|c| self.child(c, 0)) else { continue };
                let bank = self.bank_of_var(Some(entry));
                if bank != 0 && !declared.iter().any(|&(b, _)| b == bank) {
                    declared.push((bank, entry));
                }
            }
        }
        declared.sort_by_key(|&(bank, _)| bank);
        for (bank, entry) in declared {
            out.push_str(&record(&["bank_decl", &bank.to_string(), &self.lineno(entry).to_string(), &self.id(entry).filename]));
        }
        let banks: Vec<String> = self.banks_with_data.iter().map(|b| b.to_string()).collect();
        out.push_str(&record(&["banks_with_data", &banks.join(",")]));
        out.push_str(&record(&["counter", "tmp_label", &self.label_counter.to_string()]));
        out.push_str(&record(&["counter", "temporary", &self.temp_counter.to_string()]));
        for (text, label) in self.string_labels.iter() {
            out.push_str(&record(&["string_label", label, text]));
        }
        let mut required = self.data_labels_required.clone();
        required.sort();
        required.dedup();
        for name in &required {
            out.push_str(&record(&["data_label_required", name]));
        }
        out.push_str(&record(&["warnings", &self.sess.has_warnings.to_string()]));
        out
    }
}

#[cfg(test)]
mod tests {
    use crate::compiler::Compiler;
    use crate::options::OptValue;
    use crate::types::Num;
    use zxr_core::Session;

    /// What `tools/oracle_stage.py <root> ast` writes for a program of one empty
    /// line compiled as `empty.bas --arch zx48k -O1`, less the options.
    const EMPTY: &str = "--- ast
END SymbolSENTENCE L2 file=empty.bas sentinel=True
  NUMBER SymbolNUMBER L2 type=ubyte value=0
--- data_ast
BLOCK SymbolSENTENCE L2 file=empty.bas sentinel=False
--- datas
--- symbols
scope '' parent=- owner=-
  type unknown SymbolBASICTYPE unknown
  type byte SymbolBASICTYPE byte
  type ubyte SymbolBASICTYPE ubyte
  type integer SymbolBASICTYPE integer
  type uinteger SymbolBASICTYPE uinteger
  type long SymbolBASICTYPE long
  type ulong SymbolBASICTYPE ulong
  type fixed SymbolBASICTYPE fixed
  type float SymbolBASICTYPE float
  type string SymbolBASICTYPE string
  type boolean SymbolBASICTYPE boolean
  id .core.ZXBASIC_USER_DATA token=LABEL class=label name=.core.ZXBASIC_USER_DATA orig=.core.ZXBASIC_USER_DATA mangled=.core.ZXBASIC_USER_DATA scope=global type=uinteger L0 file=empty.bas declared=True accessed=False caseins=False has_address=True in='' t=t1 callable=False offset=None faraddr=False
    label bank=0 line_number=False owners=
  id .core.ZXBASIC_USER_DATA_LEN token=LABEL class=label name=.core.ZXBASIC_USER_DATA_LEN orig=.core.ZXBASIC_USER_DATA_LEN mangled=.core.ZXBASIC_USER_DATA_LEN scope=global type=uinteger L0 file=empty.bas declared=True accessed=False caseins=False has_address=True in='' t=t3 callable=False offset=None faraddr=False
    label bank=0 line_number=False owners=
--- globals
temporaries 4
tmp_labels 0
data_label .core.ZXBASIC_USER_DATA .DATA.__DATA__0
data_label .core.ZXBASIC_USER_DATA_LEN .DATA.__DATA__0
data_ptr_current .DATA.__DATA__0
data_is_used False
print_is_used False
warnings 0
option input_filename=empty.bas
";

    #[test]
    fn empty_program_dumps_as_the_oracle_does() {
        let mut sess = Session::default();
        sess.filename = "empty.bas".to_string();
        let mut c = Compiler::new(sess);
        c.set_option("input_filename", OptValue::Str("empty.bas".into())).unwrap();
        // What p_start does for a program with no statements.
        c.make_label(crate::compiler::ZXBASIC_USER_DATA, 0);
        c.make_label(crate::compiler::ZXBASIC_USER_DATA_LEN, 0);
        let zero = c.make_number(Num::Int(0), 2, None);
        c.ast = Some(c.make_sentinel(2, "END", &[Some(zero)]));
        c.data_ast = Some(c.make_sentence(2, "BLOCK", &[]));
        let dump = c.dump_ast();
        let upto = dump.find("option stderr_filename").unwrap();
        assert_eq!(&dump[..upto], EMPTY);
    }
}
