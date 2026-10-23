//! The parser: `src/zxbc/zxbparser.py`.
//!
//! The grammar is `grammar/zxbasic.y`, and as in the preprocessor and the
//! assembler each arm of the reduction is the body of the Python function of
//! that name. The actions are in two files, split where the Python source
//! goes from statements to expressions; this one holds what they share: the
//! value a grammar symbol carries, the `make_*` helpers from the top of the
//! Python module, and the driver.

mod actions_a;
mod actions_b;

use crate::ast::*;
use crate::compiler::*;
use crate::grammar::{term, Rule, RULES, TABLES};
use crate::lexer::{BasicLexer, TokenValue};
use crate::pyval::{self, crash};
use crate::types::{BasicType, Num, Ty};
use zxr_lalr::runtime::UNKNOWN_TERMINAL;
use zxr_lalr::{Driver, OnError, ParseOptions, Symbol, SyntaxError};

/// What a grammar symbol carries: the `p[n]` of a Python action.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum V {
    /// `None`: nothing, or what an action leaves after an error.
    #[default]
    None,
    /// A node of the tree.
    Node(NodeId),
    /// The text of a token, or any other string.
    Text(String),
    /// The value of a NUMBER token that came from digits in another base,
    /// or of a line-number LABEL.
    Int(i128),
    /// The value of a NUMBER token.
    Float(f64),
    /// `Id(name, lineno)`.
    Id(String, u32),
    /// A type reference, as `make_type` returns it.
    Type(Ty),
    Bool(bool),
    /// A Python list or tuple.
    List(Vec<V>),
}

impl V {
    pub fn is_none(&self) -> bool {
        matches!(self, V::None)
    }

    /// The node, or `None` where the Python value is `None`.
    pub fn node(&self) -> Option<NodeId> {
        match self {
            V::Node(n) => Some(*n),
            V::None => None,
            other => crash(&format!("TypeError: a node was expected, not {other:?}")),
        }
    }

    pub fn opt(node: Option<NodeId>) -> V {
        node.map_or(V::None, V::Node)
    }

    pub fn text(&self) -> &str {
        match self {
            V::Text(s) => s,
            other => crash(&format!("TypeError: a str was expected, not {other:?}")),
        }
    }

    pub fn ty(&self) -> Option<Ty> {
        match self {
            V::Type(t) => Some(*t),
            V::None => None,
            other => crash(&format!("TypeError: a type was expected, not {other:?}")),
        }
    }

    pub fn list(&self) -> &[V] {
        match self {
            V::List(items) => items,
            other => crash(&format!("TypeError: a list was expected, not {other:?}")),
        }
    }

    pub fn into_list(self) -> Vec<V> {
        match self {
            V::List(items) => items,
            other => crash(&format!("TypeError: a list was expected, not {other:?}")),
        }
    }

    /// `'%s' % value`, as `p_error` shows a token's value.
    pub fn python_str(&self) -> String {
        match self {
            V::None => "None".to_string(),
            V::Text(s) => s.clone(),
            V::Int(i) => i.to_string(),
            V::Float(f) => zxr_core::pynum::float_repr(*f),
            V::Bool(true) => "True".to_string(),
            V::Bool(false) => "False".to_string(),
            other => format!("{other:?}"),
        }
    }
}

pub type Sym = Symbol<V>;

/// The `p` of a Python action: `p[1]` is `p.v(1)`, and `len(p)` counts the
/// left-hand side as PLY does.
pub struct P<'r> {
    pub rhs: &'r mut [Sym],
}

impl P<'_> {
    /// `len(p)`.
    pub fn len(&self) -> usize {
        self.rhs.len() + 1
    }

    /// `p.lineno(n)`.
    pub fn lineno(&self, n: usize) -> u32 {
        self.rhs[n - 1].lineno
    }

    /// `p[n]`.
    pub fn v(&self, n: usize) -> &V {
        &self.rhs[n - 1].value
    }

    /// `p[n]`, moved out; what is left behind is `None`.
    pub fn take(&mut self, n: usize) -> V {
        std::mem::take(&mut self.rhs[n - 1].value)
    }

    /// `p[n]` where it is a node or `None`.
    pub fn node(&self, n: usize) -> Option<NodeId> {
        self.v(n).node()
    }

    /// `p[n]` where it is the text of a token.
    pub fn text(&self, n: usize) -> &str {
        self.v(n).text()
    }
}

pub struct Parser<'a> {
    pub c: &'a mut Compiler,
    pub lexer: BasicLexer,
    /// A token read ahead while deciding what a `private` was.
    pub ahead: Option<crate::lexer::Token>,
}

impl Compiler {
    /// `zxbparser.parser.parse(input, lexer=zxblex.lexer, tracking=True)`:
    /// parses preprocessed source, leaving the program in `self.ast` and the
    /// declarations of its globals in `self.data_ast`.
    pub fn parse(&mut self, input: &str) {
        // TYPE … END TYPE is lowered to plain BASIC first (records.rs); a
        // program without one is read as it is.
        let errors = self.sess.has_errors;
        let lowered = crate::records::lower(&mut self.sess, input, self.opts.bool("case_insensitive"), self.opts.int("array_base") as i64);
        if self.sess.has_errors > errors {
            // What is left of a wrong record is not worth the parser's
            // messages about it.
            return;
        }
        if let (Some(text), Some(path)) = (&lowered, std::env::var_os("ZXR_DUMP_RECORDS")) {
            if let Err(e) = std::fs::write(&path, text) {
                eprintln!("zxbc: ZXR_DUMP_RECORDS: {}: {e}", path.to_string_lossy());
            }
        }
        let input = lowered.as_deref().unwrap_or(input);
        let mut parser = Parser { c: self, lexer: BasicLexer::new(input), ahead: None };
        zxr_lalr::parse(&TABLES, &mut parser, ParseOptions { tracking: true, defaulted_states: true });
        self.check_private_uses();
    }

    // ------------------------------------------------------------------
    // The helpers at the top of zxbparser.py
    // ------------------------------------------------------------------

    /// `_TYPE(type_)`.
    pub fn type_(&self, basic: BasicType) -> Ty {
        Ty::new(basic)
    }

    /// `mark_entry_as_accessed`: marks the entry as needed, unless it is a
    /// function named from inside another.
    pub fn mark_entry_as_accessed(&mut self, entry: NodeId) {
        if !self.function_level.is_empty() && self.token(entry) == "FUNCTION" {
            return;
        }
        self.set_accessed(entry, true);
    }

    /// `convert_to_function`.
    pub fn convert_to_function(&mut self, entry: NodeId, class: Class, lineno: u32) {
        if self.check_class(entry, class, lineno) {
            if self.class_(entry) == class {
                return;
            }
            self.to_function(entry, Some(lineno), class);
        }
    }

    /// `make_number`.
    pub fn make_number(&mut self, value: Num, lineno: u32, type_: Option<Ty>) -> NodeId {
        self.new_number(value, lineno, type_)
    }

    /// `make_typecast`.
    pub fn make_typecast(&mut self, type_: Ty, node: Option<NodeId>, lineno: u32) -> Option<NodeId> {
        let node = node?;
        self.ty(node)?; // a syntax or semantic error earlier on
        self.typecast(type_, Some(node), lineno)
    }

    /// `make_binary`.
    pub fn make_binary(&mut self, lineno: u32, operator: &str, left: Option<NodeId>, right: Option<NodeId>, func: Option<Fold2>, type_: Option<Ty>) -> Option<NodeId> {
        self.make_binary_node(operator, left, right, lineno, func, type_)
    }

    /// `make_unary`.
    pub fn make_unary(&mut self, lineno: u32, operator: &str, operand: Option<NodeId>, func: Option<Fold1>, type_: Option<Ty>) -> Option<NodeId> {
        let operand = operand?;
        Some(self.make_unary_node(lineno, operator, operand, func, type_))
    }

    /// `make_builtin`. The operands are a node, several, or none.
    pub fn make_builtin(&mut self, lineno: u32, fname: &str, operands: &[NodeId], func: Option<Fold1>, type_: Option<Ty>) -> NodeId {
        self.make_builtin_node(lineno, fname, func, type_, operands)
    }

    /// `make_constexpr`.
    pub fn make_constexpr(&mut self, lineno: u32, expr: NodeId) -> NodeId {
        self.new_constexpr(expr, lineno)
    }

    /// `make_strslice`.
    pub fn make_strslice(&mut self, lineno: u32, s: Option<NodeId>, lower: Option<NodeId>, upper: Option<NodeId>) -> Option<NodeId> {
        self.make_strslice_node(lineno, s, lower, upper)
    }

    /// `make_sentence`.
    pub fn make_sentence(&mut self, lineno: u32, sentence: &str, args: &[Option<NodeId>]) -> NodeId {
        self.new_sentence(lineno, sentence, args, false)
    }

    /// `make_sentence(..., sentinel=True)`.
    pub fn make_sentinel(&mut self, lineno: u32, sentence: &str, args: &[Option<NodeId>]) -> NodeId {
        self.new_sentence(lineno, sentence, args, true)
    }

    fn codebank_name(bank: i64) -> String {
        if bank != 0 { format!("CODEBANK {bank}") } else { "the resident program".to_string() }
    }

    /// `_check_asm_codebank_balance`: an asm block that switches the
    /// assembler to a code bank with `CODEBANK n` must switch back. Textual:
    /// each line is matched against `^\s*CODEBANK\b\s*(.*?)\s*$`, any case.
    fn check_asm_codebank_balance(&mut self, asm: &str, lineno: u32, enclosing: i64) {
        let mut current = enclosing;
        for line in asm.split('\n') {
            let rest = line.trim_start();
            let Some(head) = rest.get(..8) else { continue };
            if !head.eq_ignore_ascii_case("codebank") {
                continue;
            }
            let rest = &rest[8..];
            if rest.chars().next().is_some_and(|c| c.is_alphanumeric() || c == '_') {
                continue;
            }
            let operand = rest.trim();
            if operand.is_empty() || !operand.chars().all(|c| c.is_ascii_digit()) {
                return; // not a plain number: it cannot be followed
            }
            current = operand.parse().unwrap_or(i64::MAX);
        }
        if current == enclosing {
            return;
        }
        let msg = format!(
            "This 'asm' block is assembled into {} but its 'CODEBANK' directives leave the assembler in {}. It must switch back before it ends",
            Self::codebank_name(enclosing),
            Self::codebank_name(current)
        );
        self.error(lineno, &msg);
    }

    /// `make_asm_sentence`. A block at module level inside a CODEBANK block
    /// is compiled into that bank; whether it really is diverted is settled
    /// after parsing (`codebank_asm`).
    pub fn make_asm_sentence(&mut self, asm: &str, lineno: u32, sentinel: bool) -> NodeId {
        let enclosing = self.opts.current_codebank();
        self.check_asm_codebank_balance(asm, lineno, enclosing);
        let bank = if sentinel || !self.function_level.is_empty() { 0 } else { enclosing };
        self.new_asm(asm, lineno, sentinel, bank)
    }

    /// `make_var_declaration`.
    pub fn make_var_declaration(&mut self, entry: NodeId) -> NodeId {
        self.new_vardecl(entry)
    }

    /// `make_array_declaration`.
    pub fn make_array_declaration(&mut self, entry: NodeId) -> NodeId {
        self.new_arraydecl(entry)
    }

    /// `make_arg_list`.
    pub fn make_arg_list(&mut self, node: Option<NodeId>, args: &[NodeId]) -> NodeId {
        self.arglist_make_node(node, args)
    }

    /// `make_argument`.
    pub fn make_argument(&mut self, expr: Option<NodeId>, lineno: u32, byref: Option<bool>, name: Option<String>) -> Option<NodeId> {
        let expr = expr?; // there was a syntax or semantic error
        let byref = byref.unwrap_or_else(|| self.opts.bool("default_byref"));
        Some(self.new_argument(expr, lineno, Some(byref), name))
    }

    /// `make_param_list`.
    pub fn make_param_list(&mut self, node: Option<NodeId>, args: &[Option<NodeId>]) -> NodeId {
        self.paramlist_make_node(node, args)
    }

    /// `make_array_access`: `A(x1, x2, ..., xn)` as an ARRAYACCESS, which
    /// gives the address of the element.
    pub fn make_array_access(&mut self, id: &str, lineno: u32, arglist: NodeId) -> Option<NodeId> {
        for arg in self.children(arglist).to_vec().into_iter().flatten() {
            let value = self.make_typecast(Ty::new(BOUND_TYPE), self.child(arg, 0), self.lineno(arg))?;
            self.set_child(arg, 0, Some(value));
        }
        self.make_array_access_node(false, id, arglist, lineno)
    }

    /// `make_array_substr_assign`: `a$(i)(x TO y) = expr`.
    pub fn make_array_substr_assign(&mut self, lineno: u32, id: &str, arg_list: Option<NodeId>, substr: Option<(Option<NodeId>, Option<NodeId>)>, expr: Option<NodeId>) -> Option<NodeId> {
        let (Some(arg_list), Some(substr), Some(expr)) = (arg_list, substr, expr) else { return None };
        let entry = self.access_call(id, lineno, None, None)?;
        if self.ty(entry).is_none_or(|t| t != BasicType::String) {
            self.error(lineno, &format!("Array '{id}' is not of type String"));
            return None;
        }
        let arr = self.make_array_access(id, lineno, arg_list)?;
        let arr_ty = self.ty_of(arr);
        let expr = self.make_typecast(arr_ty, Some(expr), lineno)?;
        let str_idx_type = Ty::new(STR_INDEX_TYPE);
        let s0 = self.make_typecast(str_idx_type, substr.0, lineno)?;
        let s1 = self.make_typecast(str_idx_type, substr.1, lineno)?;
        let string_base = self.opts.int("string_base");
        if string_base != 0 {
            let base = self.make_number(Num::Int(string_base), lineno, Some(str_idx_type));
            // A None here would make make_sentence drop the argument.
            let low = self.make_binary(lineno, "MINUS", Some(s0), Some(base), Some(pyval::fold::sub), None);
            let high = self.make_binary(lineno, "MINUS", Some(s1), Some(base), Some(pyval::fold::sub), None);
            return Some(self.make_sentence(lineno, "LETARRAYSUBSTR", &[Some(arr), low, high, Some(expr)]));
        }
        Some(self.make_sentence(lineno, "LETARRAYSUBSTR", &[Some(arr), Some(s0), Some(s1), Some(expr)]))
    }

    /// `make_call`: an identifier followed by arguments, `a(4)`. It is a
    /// function call if `a` is a function, an array access if it is an
    /// array, and a string slice if it is a string. An undeclared `a` is
    /// taken for a function defined further on.
    pub fn make_call(&mut self, id: &str, lineno: u32, args: Option<NodeId>) -> Option<NodeId> {
        let args = args?;
        let mut entry = self.access_call(id, lineno, None, None)?;
        let arg_list: Vec<NodeId> = self.children(args).iter().flatten().copied().collect();
        if self.class_(entry) == Class::Unknown && self.ty(entry).is_some_and(|t| t == BasicType::String) && arg_list.len() == 1 && self.is_numeric(&[arg_list[0]]) {
            entry = self.to_var(entry, None); // a scalar variable: a$(expr)
        }
        if self.class_(entry) == Class::Array {
            // An array declared already.
            let arr = self.make_array_access_node(true, id, args, lineno)?;
            if let Some(offset) = self.array_offset(arr) {
                let number = self.make_number(offset, lineno, None);
                let offset = self.make_typecast(Ty::new(BasicType::UInteger), Some(number), lineno);
                self.append_child(arr, offset);
            }
            return Some(arr);
        }
        if matches!(self.class_(entry), Class::Var | Class::Const) {
            // A string variable declared or used already.
            if arg_list.len() > 1 {
                self.syntax_error_not_array_nor_func(lineno, id);
                return None;
            }
            if self.class_(entry) == Class::Var {
                entry = self.access_var(id, lineno, None, None)?;
            }
            if arg_list.len() == 1 {
                let index = self.child(arg_list[0], 0);
                if self.class_(entry) == Class::Var {
                    return self.make_strslice(lineno, Some(entry), index, index);
                }
                let value = self.value(entry);
                let string = self.new_string(value.as_str(), lineno);
                return self.make_strslice(lineno, Some(string), index, index);
            }
            self.mark_entry_as_accessed(entry);
            return Some(entry);
        }
        self.make_func_call(id, lineno, args)
    }

    /// `make_param_decl`.
    pub fn make_param_decl(&mut self, id: &str, lineno: u32, typedef: Option<Ty>, is_array: bool, default_value: Option<NodeId>) -> Option<NodeId> {
        self.declare_param(id, lineno, typedef, is_array, default_value)
    }

    /// `make_type`: a use of the type of that name, as in `DIM a AS Integer`.
    pub fn make_type(&mut self, typename: &str, lineno: u32, implicit: bool) -> Option<Ty> {
        if !self.check_is_declared(typename, lineno, "type", None, true) {
            return None;
        }
        let entry = self.get_entry(typename, None, None)?;
        match self.node(entry).kind {
            Kind::Type(basic) => Some(Ty::reference(basic, lineno, implicit)),
            _ => crash("AssertionError: isinstance(type_, SymbolTYPE)"),
        }
    }

    /// `make_bound_list`.
    pub fn make_bound_list(&mut self, node: Option<NodeId>, args: &[Option<NodeId>]) -> NodeId {
        self.boundlist_make_node(node, args)
    }

    /// `make_label`: declares a label. A line number is one too.
    pub fn make_label(&mut self, id: &str, lineno: u32) -> Option<NodeId> {
        let entry = self.declare_label(id, lineno);
        if entry.is_some() {
            // The label points at the current DATA block.
            let current = self.data_ptr_current.clone();
            self.data_labels.insert(id, current);
        }
        entry
    }

    /// `make_break`: with `--enable-break`, a check of the BREAK key for
    /// this line, unless the line has one already.
    pub fn make_break(&mut self, lineno: u32, p: Option<NodeId>) -> Option<NodeId> {
        if !self.opts.bool("enable_break") || lineno == self.last_brk_linenum || self.is_null(&[p]) {
            return None;
        }
        self.last_brk_linenum = lineno;
        let number = self.make_number(Num::Int(lineno as i128), lineno, Some(Ty::new(BasicType::UInteger)));
        Some(self.make_sentence(lineno, "CHKBREAK", &[Some(number)]))
    }
}

impl Parser<'_> {
    fn lex(&mut self) -> Option<crate::lexer::Token> {
        let Compiler { sess, nodes, scopes, table, .. } = &mut *self.c;
        self.lexer.token(sess, &|name| crate::symtab::lexer_lookup(nodes, scopes, table, name))
    }
}

impl Driver<V> for Parser<'_> {
    fn token(&mut self) -> Option<Sym> {
        let mut token = match self.ahead.take() {
            Some(token) => token,
            None => self.lex()?,
        };
        // `PRIVATE` is not a keyword, so that the grammar stays the Python
        // compiler's and the word stays free as a name: it means something
        // only as the word before SUB or FUNCTION, and there it is dropped
        // and left for the header's action to find.
        if token.kind == term::ID && matches!(&token.value, TokenValue::Text(s) if s.eq_ignore_ascii_case("private")) {
            match self.lex() {
                Some(next) if matches!(next.kind, term::SUB | term::FUNCTION) => {
                    self.c.pending_private = Some(token.lineno);
                    token = next;
                }
                next => self.ahead = next,
            }
        }
        let value = match token.value {
            TokenValue::Text(s) => V::Text(s),
            TokenValue::Int(i) => V::Int(i),
            TokenValue::Float(f) => V::Float(f),
        };
        Some(Symbol { kind: token.kind, value, lineno: token.lineno, lexpos: token.lexpos, ..Symbol::default() })
    }

    fn reduce(&mut self, production: usize, rhs: &mut [Sym]) -> Result<V, SyntaxError> {
        let rule = RULES[production];
        let mut p = P { rhs };
        if let Some(value) = self.reduce_a(rule, &mut p)? {
            return Ok(value);
        }
        if let Some(value) = self.reduce_b(rule, &mut p)? {
            return Ok(value);
        }
        match rule {
            Rule::start_ => Ok(p.take(1)),
            other => crash(&format!("no action for the grammar rule {other:?}")),
        }
    }

    /// `p_error`.
    fn syntax_error(&mut self, token: Option<&Sym>) -> OnError<V> {
        if let Some(token) = token {
            let msg = if token.kind != term::NEWLINE {
                let name = if token.kind == UNKNOWN_TERMINAL { "ERROR" } else { TABLES.terminal_names[token.kind as usize] };
                format!("Syntax Error. Unexpected token '{}' <{name}>", token.value.python_str())
            } else {
                "Unexpected end of line".to_string()
            };
            self.c.error(token.lineno, &msg);
            return OnError::Recover;
        }
        // Try to give some hints: a loop that was not closed.
        if let Some(loop_info) = self.c.loops.last().cloned() {
            if loop_info.kind == LoopType::For {
                self.c.syntax_error_for_without_next(loop_info.lineno);
            } else {
                self.c.syntax_error_loop_not_closed(loop_info.lineno, loop_info.kind.name());
            }
        }
        // After earlier errors this end of file is their doing.
        if self.c.sess.has_errors > 0 {
            return OnError::Recover;
        }
        let lineno = self.lexer.lineno();
        self.c.error(lineno, "Unexpected end of file");
        OnError::Recover
    }

    fn position(&self) -> (u32, usize) {
        (self.lexer.lineno(), self.lexer.lexpos())
    }
}
