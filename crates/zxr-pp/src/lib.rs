//! The preprocessor.
//!
//! This follows the Python `zxbpp` rule for rule: the grammar is
//! `grammar/zxbpp.y`, and each arm of [`Preprocessor::reduce`] is the body of
//! the Python function of the same name. Output is built by the grammar
//! actions, and several of them act on the lexer (an `#include` switches
//! files when its rule is reduced), so the order in which things happen is
//! part of the behaviour.

pub mod lexer;
pub mod macros;
mod runtime_fixes;

#[allow(clippy::all)]
pub mod grammar {
    include!(concat!(env!("OUT_DIR"), "/grammar.rs"));
}

use grammar::{term, Rule};
use lexer::PpLexer;
use macros::{Arg, ArgList, ArgTok, DefTok, Definition, IdRef, Node, Table};
use std::collections::HashMap;
use std::rc::Rc;
use zxr_core::{paths, Session};
use zxr_lalr::{Driver, OnError, ParseOptions, Symbol, SyntaxError};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Basic,
    Asm,
}

/// What a grammar symbol carries.
#[derive(Default, Debug)]
pub enum Value {
    #[default]
    None,
    Text(String),
    /// Pieces of output.
    Lines(Vec<String>),
    /// The tokens of a source line or macro body.
    Defs(Vec<DefTok>),
    Macro(Rc<Node>),
    /// Macro parameters; `None` when there are none or they were invalid.
    Params(Option<Vec<String>>),
    Args(ArgList),
    Arg(Arg),
    Bool(bool),
    /// An include modifier: `Some(arch)` or `Some("")`, or `None` if it was invalid.
    Modifier(Option<String>),
}

type Sym = Symbol<Value>;

fn text(sym: &mut Sym) -> String {
    match std::mem::take(&mut sym.value) {
        Value::Text(s) => s,
        other => panic!("expected text, found {other:?}"),
    }
}

fn lines(sym: &mut Sym) -> Vec<String> {
    match std::mem::take(&mut sym.value) {
        Value::Lines(l) => l,
        other => panic!("expected output lines, found {other:?}"),
    }
}

fn node(sym: &mut Sym) -> Rc<Node> {
    match std::mem::take(&mut sym.value) {
        Value::Macro(n) => n,
        other => panic!("expected a macro call, found {other:?}"),
    }
}

fn arglist(sym: &mut Sym) -> ArgList {
    match std::mem::take(&mut sym.value) {
        Value::Args(a) => a,
        other => panic!("expected an argument list, found {other:?}"),
    }
}

fn is_digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn to_bool(expr: &str) -> bool {
    if is_digits(expr) { expr.parse::<u128>().map_or(true, |n| n != 0) } else { !expr.is_empty() }
}

fn to_int(expr: &str) -> u128 {
    if is_digits(expr) { expr.parse().unwrap_or(u128::MAX) } else { 0 }
}

fn remove_spaces(x: String) -> String {
    if x.is_empty() {
        return x;
    }
    let stripped = x.trim_matches([' ', '\t']);
    if stripped.is_empty() { " ".to_string() } else { stripped.to_string() }
}

/// Where include files are looked for.
#[derive(Clone, Debug, Default)]
pub struct Resolver {
    /// Directory of the file being read.
    pub current_dir: String,
    pub lib_root: String,
    pub architecture: String,
    /// Extra directories, `:` separated.
    pub include_path: String,
}

impl Resolver {
    fn include_dirs(&self, arch: &str) -> Vec<String> {
        let arch = if arch.is_empty() { self.architecture.as_str() } else { arch };
        if !zxr_core::ARCHITECTURES.contains(&arch) {
            return Vec::new();
        }
        let base = paths::join(&paths::join(&self.lib_root, "arch"), arch);
        vec![paths::join(&base, "stdlib"), paths::join(&base, "runtime")]
    }

    /// Finds `fname`, reporting an error and returning an empty string if it
    /// is nowhere. `local_first` looks in `current_dir` before the library.
    pub fn search(&self, sess: &mut Session, fname: &str, lineno: u32, local_first: bool, arch: &str) -> String {
        let fname = paths::sanitize(fname);
        let mut dirs: Vec<String> = if local_first { vec![self.current_dir.clone()] } else { Vec::new() };
        // An unknown architecture falls back to the default directories.
        let known = self.include_dirs(arch);
        dirs.extend(if known.is_empty() { self.include_dirs("") } else { known });
        if !self.include_path.is_empty() {
            dirs.extend(self.include_path.split(':').map(String::from));
        }
        if fname.starts_with('/') {
            if std::path::Path::new(&fname).is_file() {
                return fname;
            }
        } else {
            for dir in dirs {
                let path = paths::absolute(&paths::sanitize(&paths::join(&dir, &fname)));
                if std::path::Path::new(&path).exists() {
                    return path;
                }
            }
        }
        sess.error(lineno, &format!("file '{fname}' not found"), None);
        String::new()
    }
}

struct Included {
    once: bool,
    /// Where it was included from: (file, line).
    parents: Vec<(String, u32)>,
}

/// A preprocessor between two passes, without its session.
pub struct Suspended {
    lexer: PpLexer,
    pub output: String,
    table: Table,
    current_dir: String,
    lib_root: String,
    enabled: bool,
    included: HashMap<String, Included>,
    ifdefs: Vec<bool>,
    current_file: Vec<String>,
}

pub struct Preprocessor<'s> {
    pub sess: &'s mut Session,
    lexer: PpLexer,
    /// The preprocessed text.
    pub output: String,
    table: Table,
    current_dir: String,
    /// Directory holding `arch/<name>/{stdlib,runtime}`.
    lib_root: String,
    enabled: bool,
    included: HashMap<String, Included>,
    /// The enabled state outside each open `#if`.
    ifdefs: Vec<bool>,
    /// Files being read, innermost last, as named in `#line` markers.
    current_file: Vec<String>,
}

impl<'s> Preprocessor<'s> {
    pub fn new(sess: &'s mut Session, mode: Mode, lib_root: &str) -> Preprocessor<'s> {
        let mut pp = Preprocessor {
            sess,
            lexer: PpLexer::new(mode),
            output: String::new(),
            table: Table::default(),
            current_dir: String::new(),
            lib_root: lib_root.to_string(),
            enabled: true,
            included: HashMap::new(),
            ifdefs: Vec::new(),
            current_file: Vec::new(),
        };
        pp.reset_table();
        pp
    }

    /// Lets go of the session, keeping everything else: the files included
    /// once, the macro table, the lexer. The compiler does this between the
    /// preprocessing of the BASIC source and that of the generated assembly,
    /// where the Python one simply leaves the module's globals alone.
    pub fn suspend(self) -> Suspended {
        Suspended {
            lexer: self.lexer,
            output: self.output,
            table: self.table,
            current_dir: self.current_dir,
            lib_root: self.lib_root,
            enabled: self.enabled,
            included: self.included,
            ifdefs: self.ifdefs,
            current_file: self.current_file,
        }
    }

    /// Takes up where [`Preprocessor::suspend`] left off.
    pub fn resume(sess: &'s mut Session, state: Suspended) -> Preprocessor<'s> {
        Preprocessor {
            sess,
            lexer: state.lexer,
            output: state.output,
            table: state.table,
            current_dir: state.current_dir,
            lib_root: state.lib_root,
            enabled: state.enabled,
            included: state.included,
            ifdefs: state.ifdefs,
            current_file: state.current_file,
        }
    }

    /// Switches between the BASIC and ASM lexers, keeping everything else.
    pub fn set_mode(&mut self, mode: Mode) {
        self.lexer = PpLexer::new(mode);
    }

    /// Empties the macro table down to the command-line and built-in macros.
    pub fn reset_table(&mut self) {
        self.table.clear();
        let fname = self.current_file.last().cloned().unwrap_or_else(|| self.sess.program.clone());
        for (name, value) in self.sess.options.defines.clone() {
            self.define(&name, None, vec![DefTok::Text(value)], 0, &fname);
        }
        for (name, builtin) in macros::BUILTINS {
            self.table.insert(Definition { name: name.to_string(), params: None, value: Vec::new(), lineno: 0, fname: String::new(), builtin: Some(builtin) });
        }
    }

    fn file(&self) -> String {
        self.current_file.last().cloned().unwrap_or_default()
    }

    /// Finds an include file. `local_first` looks beside the including file
    /// before the library directories.
    fn search_filename(&mut self, fname: &str, lineno: u32, local_first: bool, arch: &str) -> String {
        self.resolver().search(self.sess, fname, lineno, local_first, arch)
    }

    /// The include search as it stands now, for whoever needs to find files
    /// after preprocessing (the assembler's INCBIN).
    pub fn resolver(&self) -> Resolver {
        Resolver {
            current_dir: self.current_dir.clone(),
            lib_root: self.lib_root.clone(),
            architecture: self.sess.options.architecture.clone(),
            include_path: self.sess.options.include_path.clone(),
        }
    }

    fn include_file(&mut self, filename: &str, lineno: u32, local_first: bool, arch: &str) -> String {
        let abs = self.search_filename(filename, lineno, local_first, arch);
        let filename = paths::relative_to_cwd(&abs);
        match self.included.get(&abs) {
            None => {
                self.included.insert(abs.clone(), Included { once: false, parents: Vec::new() });
            }
            Some(info) if info.once => {
                // Already read, and marked to be read once: an empty file.
                self.lexer.next_token = Some(term::_ENDFILE_);
                return String::new();
            }
            Some(_) => {}
        }
        if let Some(parent) = self.current_file.last() {
            self.included.get_mut(&abs).unwrap().parents.push((parent.clone(), lineno));
        }
        self.current_file.push(filename.clone());
        self.current_dir = paths::dirname(&filename);
        self.lexer.include(self.sess, &filename)
    }

    fn include_once(&mut self, filename: &str, lineno: u32, local_first: bool, arch: &str) -> String {
        let abs = self.search_filename(filename, lineno, local_first, arch);
        let Some(info) = self.included.get(&abs) else {
            return self.include_file(filename, lineno, local_first, arch);
        };
        if info.parents.len() > 1 {
            let (parent, line) = info.parents[0].clone();
            self.sess.warning(line, &format!("file '{filename}' already included more than once, in file '{parent}' at line {line}"), None);
        }
        self.lexer.next_token = Some(term::_ENDFILE_);
        String::new()
    }

    fn define(&mut self, name: &str, params: Option<Vec<String>>, value: Vec<DefTok>, lineno: u32, fname: &str) {
        if let Some(previous) = self.table.lookup(name) {
            if previous.fname.is_empty() && previous.lineno == 0 {
                self.sess.warning_coded("500", lineno, &format!("builtin macro \"{}\" redefined", previous.name), Some(fname));
            } else {
                let message = format!("\"{}\" redefined (previous definition at {}:{})", previous.name, previous.fname, previous.lineno);
                self.sess.warning_coded("510", previous.lineno, &message, Some(&previous.fname));
            }
        }
        self.table.insert(Definition { name: name.to_string(), params, value, lineno, fname: fname.to_string(), builtin: None });
    }

    /// Expands the macros of one source line into text.
    fn expand_macros(&mut self, defs: Vec<DefTok>, lineno: u32) -> Option<String> {
        let mut out = String::new();
        for def in defs {
            match def {
                DefTok::Text(text) => out.push_str(&text),
                DefTok::Macro(node) => match macros::eval(&node, None, &self.table) {
                    Ok(text) => out.push_str(&remove_spaces(text)),
                    Err(error) => {
                        self.sess.error(error.lineno, &error.message, None);
                        return None;
                    }
                },
            }
        }
        if out.contains('\n') {
            out.push_str(&format!("\n#line {}", lineno + 1));
        }
        out.push('\n');
        Some(out)
    }

    /// Evaluates a macro call where the Python code has no error handling.
    fn eval_or_die(&mut self, node: &Node) -> String {
        match macros::eval(node, None, &self.table) {
            Ok(text) => text,
            Err(error) => {
                self.sess.write_stderr(&format!("zxbpp: {}\n", error.message));
                zxr_core::exit(1)
            }
        }
    }

    fn parse(&mut self) {
        zxr_lalr::parse(&grammar::TABLES, self, ParseOptions { tracking: false, defaulted_states: false });
    }

    /// Preprocesses a string, appending to [`Preprocessor::output`].
    pub fn filter(&mut self, input: &str, filename: &str, state: &str) {
        let previous = self.current_dir.clone();
        self.current_file.push(filename.to_string());
        self.current_dir = paths::dirname(filename);
        self.lexer.input(input, filename);
        self.lexer.begin(state);
        self.parse();
        self.current_file.pop();
        self.current_dir = previous;
    }

    /// Preprocesses a file. Returns the number of errors so far.
    pub fn run(&mut self, input_file: Option<&str>) -> u32 {
        self.enabled = true;
        self.output.clear();
        let main = input_file.map_or_else(|| self.sess.filename.clone(), String::from);
        self.current_file.push(main);
        self.current_dir = paths::dirname(&self.file());

        if self.sess.options.sinclair {
            let included = self.search_filename("sinclair.bas", 0, false, "");
            if included.is_empty() {
                return 0;
            }
            let marker = self.include_once(&included, 0, false, "");
            self.output.push_str(&marker);
            if !self.output.is_empty() && !self.output.ends_with('\n') {
                self.output.push('\n');
            }
            self.parse();
            self.current_file.pop();
            self.current_dir = paths::dirname(&self.file());
        }

        let name = self.file();
        let previous = std::mem::replace(&mut self.sess.filename, name.clone());
        let marker = self.lexer.include(self.sess, &name);
        self.output.push_str(&marker);
        if !self.output.is_empty() && !self.output.ends_with('\n') {
            self.output.push('\n');
        }
        self.parse();
        self.current_file.pop();
        self.sess.filename = previous;
        self.sess.has_errors
    }

    /// The Python code adds a string to a list here and dies.
    fn include_without_file(&mut self) -> ! {
        self.sess.write_stderr("zxbpp: an #include that was not carried out is followed by text\n");
        zxr_core::exit(1)
    }

    fn finish_include(&mut self, rhs: &mut [Sym]) -> Value {
        let Value::Text(marker) = std::mem::take(&mut rhs[0].value) else { self.include_without_file() };
        let mut out = vec![marker + &text(&mut rhs[1])];
        out.extend(lines(&mut rhs[2]));
        out.push(text(&mut rhs[3]));
        self.current_file.pop();
        self.current_dir = paths::dirname(&self.file());
        Value::Lines(out)
    }

    fn line_marker(&self, lineno: u32) -> String {
        format!("#line {} \"{}\"", lineno, self.file())
    }

    fn arch_of(modifier: &Value) -> Option<String> {
        match modifier {
            Value::Modifier(m) => m.clone(),
            _ => None,
        }
    }
}

impl Driver<Value> for Preprocessor<'_> {
    fn token(&mut self) -> Option<Sym> {
        let token = self.lexer.token(self.sess)?;
        Some(Symbol { kind: token.kind, value: Value::Text(token.value), lineno: token.lineno, lexpos: token.lexpos, ..Symbol::default() })
    }

    fn syntax_error(&mut self, token: Option<&Sym>) -> OnError<Value> {
        match token {
            Some(token) => {
                let file = self.file();
                let message = match token.kind {
                    term::NEWLINE => "Syntax error. Unexpected end of line".to_string(),
                    term::_ENDFILE_ => "Syntax error. Unexpected end of file".to_string(),
                    kind => {
                        let Value::Text(value) = &token.value else { unreachable!() };
                        let shown: String = value.chars().map(|c| if c < ' ' { format!("|{:#x}|", c as u32) } else { c.to_string() }).collect();
                        format!("Syntax error. Unexpected token '{shown}' [{}]", grammar::TABLES.terminal_names[kind as usize])
                    }
                };
                self.sess.error(token.lineno, &message, Some(&file));
            }
            None => self.sess.write_stderr("General syntax error at preprocessor (unexpected End of File?)"),
        }
        self.sess.has_errors += 1;
        OnError::Recover
    }

    fn reduce(&mut self, production: usize, rhs: &mut [Sym]) -> Result<Value, SyntaxError> {
        let lineno = |rhs: &[Sym], n: usize| rhs[n - 1].lineno;
        Ok(match grammar::RULES[production] {
            Rule::start_ => Value::None,
            Rule::p_start => {
                self.output.push_str(&lines(&mut rhs[0]).concat());
                Value::None
            }
            Rule::p_program | Rule::p_token | Rule::p_def | Rule::p_def_macrocall | Rule::p_arg_argstring => std::mem::take(&mut rhs[0].value),
            Rule::p_program_tokenstring => {
                let Value::Defs(defs) = std::mem::take(&mut rhs[0].value) else { unreachable!() };
                Value::Lines(self.expand_macros(defs, lineno(rhs, 2)).into_iter().collect())
            }
            Rule::p_program_tokenstring_2 => {
                let mut out = lines(&mut rhs[0]);
                out.push(text(&mut rhs[1]));
                Value::Lines(out)
            }
            Rule::p_program_char => {
                let mut out = lines(&mut rhs[0]);
                out.extend(lines(&mut rhs[1]));
                Value::Lines(out)
            }
            Rule::p_program_newline => {
                let Value::Defs(defs) = std::mem::take(&mut rhs[1].value) else { unreachable!() };
                match self.expand_macros(defs, lineno(rhs, 3)) {
                    // An expansion error throws away what came before it too.
                    None => Value::Lines(Vec::new()),
                    Some(line) => {
                        let mut out = lines(&mut rhs[0]);
                        out.push(line);
                        Value::Lines(out)
                    }
                }
            }
            Rule::p_program_newline_2 => {
                let mut out = lines(&mut rhs[0]);
                out.push(format!("{}\n", self.line_marker(lineno(rhs, 3) + 1)));
                Value::Lines(out)
            }
            Rule::p_include_file | Rule::p_include_once_ok => self.finish_include(rhs),
            Rule::p_include_file_empty | Rule::p_include_once_empty => Value::Lines(vec![text(&mut rhs[1])]),
            Rule::p_include_fname => match Self::arch_of(&rhs[1].value) {
                None => Value::Lines(Vec::new()),
                Some(arch) => {
                    let filename = text(&mut rhs[2]);
                    if self.enabled {
                        Value::Text(self.include_file(&filename, lineno(rhs, 3), false, &arch))
                    } else {
                        self.lexer.next_token = Some(term::_ENDFILE_);
                        Value::Lines(Vec::new())
                    }
                }
            },
            Rule::p_include_macro => match Self::arch_of(&rhs[1].value) {
                None => Value::Lines(Vec::new()),
                Some(arch) => {
                    let expr = text(&mut rhs[2]);
                    let chars: Vec<char> = expr.chars().collect();
                    let quoted = chars.len() >= 2 && chars[0] == '"' && chars[chars.len() - 1] == '"' && {
                        // "([^"]|"")*": any quotes inside come in pairs.
                        let inner: String = chars[1..chars.len() - 1].iter().collect();
                        !inner.replace("\"\"", "").contains('"')
                    };
                    let angled = chars.len() >= 3 && chars[0] == '<' && chars[chars.len() - 1] == '>' && !chars[1..chars.len() - 1].contains(&'>');
                    if !quoted && !angled {
                        self.sess.error(lineno(rhs, 1), &format!("invalid filename {expr}"), None);
                        Value::Lines(Vec::new())
                    } else if self.enabled {
                        let inner: String = chars[1..chars.len() - 1].iter().collect();
                        Value::Text(self.include_file(&inner, lineno(rhs, 3), quoted, &arch))
                    } else {
                        self.lexer.next_token = Some(term::_ENDFILE_);
                        Value::Lines(Vec::new())
                    }
                }
            },
            Rule::p_include_once | Rule::p_include_once_fname => match Self::arch_of(&rhs[2].value) {
                None => Value::Lines(Vec::new()),
                Some(arch) => {
                    let quoted = rhs[3].kind == term::STRING;
                    let mut name = text(&mut rhs[3]);
                    if quoted {
                        let n = name.chars().count();
                        name = name.chars().skip(1).take(n.saturating_sub(2)).collect();
                    }
                    let marker = if self.enabled { self.include_once(&name, lineno(rhs, 4), quoted, &arch) } else { String::new() };
                    if marker.is_empty() {
                        self.lexer.next_token = Some(term::_ENDFILE_);
                    }
                    if self.enabled { Value::Text(marker) } else { Value::Lines(Vec::new()) }
                }
            },
            Rule::p_include_modifier => {
                if rhs.is_empty() {
                    Value::Modifier(Some(String::new()))
                } else {
                    let modifier = text(&mut rhs[1]);
                    let value = text(&mut rhs[3]);
                    if modifier == "arch" {
                        Value::Modifier(Some(value))
                    } else {
                        self.sess.error(lineno(rhs, 1), &format!("unknown modifier {modifier}"), None);
                        Value::Modifier(None)
                    }
                }
            }
            Rule::p_line => {
                let (a, b, c) = (text(&mut rhs[0]), text(&mut rhs[1]), text(&mut rhs[2]));
                Value::Lines(if self.enabled { vec![format!("#{a} {b}{c}")] } else { Vec::new() })
            }
            Rule::p_line_file => {
                let (a, b, c, d) = (text(&mut rhs[0]), text(&mut rhs[1]), text(&mut rhs[2]), text(&mut rhs[3]));
                Value::Lines(if self.enabled { vec![format!("#{a} {b} \"{c}\"{d}")] } else { Vec::new() })
            }
            Rule::p_require_file => Value::Lines(vec![format!("#{} {}\n", text(&mut rhs[0]), paths::sanitize(&text(&mut rhs[1])))]),
            Rule::p_init => Value::Lines(vec![format!("#{} \"{}\"\n", text(&mut rhs[0]), text(&mut rhs[1]))]),
            Rule::p_init_str => Value::Lines(vec![format!("#{} {}\n", text(&mut rhs[0]), text(&mut rhs[1]))]),
            Rule::p_undef => {
                if self.enabled {
                    self.table.undef(&text(&mut rhs[1]));
                }
                Value::Lines(Vec::new())
            }
            Rule::p_errormsg => {
                if self.enabled {
                    self.sess.error(lineno(rhs, 1), &text(&mut rhs[1]), None);
                }
                Value::Lines(Vec::new())
            }
            Rule::p_warningmsg => {
                if self.enabled {
                    self.sess.warning(lineno(rhs, 1), &text(&mut rhs[1]), None);
                }
                Value::Lines(Vec::new())
            }
            Rule::p_define => {
                let name = text(&mut rhs[1]);
                let Value::Params(params) = std::mem::take(&mut rhs[2].value) else { unreachable!() };
                let Value::Defs(mut defs) = std::mem::take(&mut rhs[3].value) else { unreachable!() };
                if self.enabled {
                    if let Some(first) = defs.first_mut() {
                        // Blank space after the name is dropped, but only if it is a
                        // single space, a single tab, or a space then a tab.
                        match first {
                            DefTok::Text(t) if " \t".contains(t.as_str()) => *t = t.trim_start_matches([' ', '\t']).to_string(),
                            _ => {
                                let file = self.lexer.current_file().map(String::from);
                                self.sess.warning_coded("520", lineno(rhs, 1), "missing whitespace after macro name", file.as_deref());
                            }
                        }
                    }
                    let file = self.file();
                    self.define(&name, params, defs, lineno(rhs, 2), &file);
                }
                Value::Lines(Vec::new())
            }
            Rule::p_define_params_epsilon => Value::Params(None),
            Rule::p_define_params_empty => Value::Params(Some(vec![String::new()])),
            Rule::p_define_params_paramlist => {
                let Value::Params(Some(names)) = std::mem::take(&mut rhs[1].value) else { unreachable!() };
                match names.iter().enumerate().find(|(i, n)| names[i + 1..].contains(n)) {
                    Some((_, name)) => {
                        self.sess.error(lineno(rhs, 3), &format!("Duplicated name parameter \"{name}\""), None);
                        Value::Params(None)
                    }
                    None => Value::Params(Some(names)),
                }
            }
            Rule::p_paramlist_single => Value::Params(Some(vec![text(&mut rhs[0])])),
            Rule::p_paramlist_paramlist => {
                let Value::Params(Some(mut names)) = std::mem::take(&mut rhs[0].value) else { unreachable!() };
                names.push(text(&mut rhs[2]));
                Value::Params(Some(names))
            }
            Rule::p_pragma_id => Value::Lines(vec![format!("#{} {}", text(&mut rhs[0]), text(&mut rhs[1]))]),
            Rule::p_pragma_id_expr => Value::Lines(vec![format!("#{} {} {} {}", text(&mut rhs[0]), text(&mut rhs[1]), text(&mut rhs[2]), text(&mut rhs[3]))]),
            Rule::p_pragma_id_string => {
                let string = text(&mut rhs[3]);
                let n = string.chars().count();
                let inner: String = string.chars().skip(1).take(n.saturating_sub(2)).collect();
                Value::Lines(vec![format!("#{} {} {} {}", text(&mut rhs[0]), text(&mut rhs[1]), text(&mut rhs[2]), inner)])
            }
            Rule::p_pragma_push => {
                Value::Lines(vec![format!("#{} {}{}{}{}", text(&mut rhs[0]), text(&mut rhs[1]), text(&mut rhs[2]), text(&mut rhs[3]), text(&mut rhs[4]))])
            }
            Rule::p_pragma_once => {
                let abs = paths::absolute(&self.file());
                self.included.entry(abs).or_insert(Included { once: false, parents: Vec::new() }).once = true;
                Value::Lines(Vec::new())
            }
            Rule::p_ifdef => {
                let mut out = Vec::new();
                if self.enabled {
                    out.push(text(&mut rhs[1]));
                    out.extend(lines(&mut rhs[2]));
                }
                out.push(self.line_marker(lineno(rhs, 4) + 1));
                self.enabled = self.ifdefs.pop().expect("#endif without #if");
                Value::Lines(out)
            }
            Rule::p_ifdef_else => {
                self.enabled = self.ifdefs.pop().expect("#endif without #if");
                let mut out = Vec::new();
                if self.enabled {
                    out.extend(lines(&mut rhs[0]));
                    out.extend(lines(&mut rhs[1]));
                }
                out.push(self.line_marker(lineno(rhs, 3) + 1));
                Value::Lines(out)
            }
            Rule::p_ifdef_else_a => {
                let mut out = Vec::new();
                if *self.ifdefs.last().expect("#else without #if") {
                    let Value::Bool(taken) = rhs[0].value else { unreachable!() };
                    if taken {
                        out.push(text(&mut rhs[1]));
                        out.extend(lines(&mut rhs[2]));
                    }
                    self.enabled = !taken;
                }
                Value::Lines(out)
            }
            Rule::p_ifdef_else_b => {
                let mut out = Vec::new();
                if self.enabled {
                    out.push(format!("{}{}", self.line_marker(lineno(rhs, 1) + 1), text(&mut rhs[1])));
                    out.extend(lines(&mut rhs[2]));
                }
                Value::Lines(out)
            }
            Rule::p_if_header | Rule::p_ifn_header | Rule::p_if_expr_header => {
                self.ifdefs.push(self.enabled);
                if self.enabled {
                    let operand = text(&mut rhs[1]);
                    self.enabled = match grammar::RULES[production] {
                        Rule::p_if_header => self.table.defined(&operand),
                        Rule::p_ifn_header => !self.table.defined(&operand),
                        _ if is_digits(&operand) => to_bool(&operand),
                        _ => self.table.defined(&operand),
                    };
                }
                Value::Bool(self.enabled)
            }
            Rule::p_expr => {
                let call = node(&mut rhs[0]);
                Value::Text(self.eval_or_die(&call).trim().to_string())
            }
            Rule::p_expr_val | Rule::p_expr_str => std::mem::take(&mut rhs[0].value),
            Rule::p_exprand | Rule::p_expror | Rule::p_exprne | Rule::p_expreq | Rule::p_exprlt | Rule::p_exprle | Rule::p_exprgt | Rule::p_exprge => {
                let (a, b) = (text(&mut rhs[0]), text(&mut rhs[2]));
                let result = match grammar::RULES[production] {
                    Rule::p_exprand => to_bool(&a) && to_bool(&b),
                    Rule::p_expror => to_bool(&a) || to_bool(&b),
                    Rule::p_exprne => a != b,
                    Rule::p_expreq => a == b,
                    Rule::p_exprlt => to_int(&a) < to_int(&b),
                    Rule::p_exprle => to_int(&a) <= to_int(&b),
                    Rule::p_exprgt => to_int(&a) > to_int(&b),
                    _ => to_int(&a) >= to_int(&b),
                };
                Value::Text(if result { "1" } else { "0" }.to_string())
            }
            Rule::p_expr_par => std::mem::take(&mut rhs[1].value),
            Rule::p_defs_list_eps => Value::Defs(Vec::new()),
            Rule::p_defs_list => {
                let Value::Defs(mut defs) = std::mem::take(&mut rhs[0].value) else { unreachable!() };
                defs.push(match std::mem::take(&mut rhs[1].value) {
                    Value::Text(t) => DefTok::Text(t),
                    Value::Macro(n) => DefTok::Macro(n),
                    other => panic!("unexpected line element {other:?}"),
                });
                Value::Defs(defs)
            }
            Rule::p_macrocall => {
                let fname = self.lexer.current_file().unwrap_or_default().to_string();
                Value::Macro(Rc::new(Node::Call { fname, lineno: lineno(rhs, 1), id: IdRef::Name(text(&mut rhs[0])), args: None }))
            }
            Rule::p_macrocall_args => {
                let fname = self.lexer.current_file().unwrap_or_default().to_string();
                let callee = node(&mut rhs[0]);
                let args = arglist(&mut rhs[1]);
                Value::Macro(Rc::new(Node::Call { fname, lineno: args.end_lineno, id: IdRef::Node(callee), args: Some(args) }))
            }
            Rule::p_macrocall_paste => {
                let (left, right) = (node(&mut rhs[0]), node(&mut rhs[2]));
                Value::Macro(Rc::new(Node::Concat { lineno: left.lineno(), left, right }))
            }
            Rule::p_macrocall_stringizing => {
                let inner = node(&mut rhs[1]);
                Value::Macro(Rc::new(Node::Stringize { lineno: inner.lineno(), inner }))
            }
            Rule::p_args => {
                let mut list = arglist(&mut rhs[1]);
                list.start_lineno = rhs[0].lineno;
                list.end_lineno = rhs[2].lineno;
                Value::Args(list)
            }
            Rule::p_arglist | Rule::p_arglist_arg => {
                let (mut list, arg_at) = if rhs.len() == 3 { (arglist(&mut rhs[0]), 2) } else { (ArgList::default(), 0) };
                let Value::Arg(arg) = std::mem::take(&mut rhs[arg_at].value) else { unreachable!() };
                list.args.push(arg);
                Value::Args(list)
            }
            Rule::p_arg_eps => Value::Arg(Arg::default()),
            Rule::p_argstring => Value::Arg(Arg { tokens: vec![arg_token(std::mem::take(&mut rhs[0].value))] }),
            Rule::p_argstring_argslist => Value::Arg(Arg { tokens: vec![ArgTok::List(arglist(&mut rhs[1]))] }),
            Rule::p_argstring_token | Rule::p_argstring_argstring => {
                let Value::Arg(mut arg) = std::mem::take(&mut rhs[0].value) else { unreachable!() };
                let added = if rhs.len() == 4 { ArgTok::List(arglist(&mut rhs[2])) } else { arg_token(std::mem::take(&mut rhs[1].value)) };
                arg.tokens.push(added);
                Value::Arg(arg)
            }
        })
    }
}

fn arg_token(value: Value) -> ArgTok {
    match value {
        Value::Text(t) => ArgTok::Text(t),
        Value::Macro(n) => ArgTok::Macro(n),
        other => panic!("unexpected argument element {other:?}"),
    }
}

/// Finds the directory that holds `arch/<name>/{stdlib,runtime}`.
///
/// `ZXR_LIB` names it outright. Otherwise it is looked for beside the
/// executable and in the directories above it, as `lib` or `src/lib`.
pub fn find_lib_root() -> String {
    if let Ok(dir) = std::env::var("ZXR_LIB") {
        return paths::absolute(&dir);
    }
    if let Ok(exe) = std::env::current_exe() {
        let mut dir = exe.parent();
        while let Some(d) = dir {
            for candidate in ["lib", "src/lib"] {
                let path = d.join(candidate);
                if path.join("arch").is_dir() {
                    return paths::absolute(&path.to_string_lossy());
                }
            }
            dir = d.parent();
        }
    }
    "lib".to_string()
}
