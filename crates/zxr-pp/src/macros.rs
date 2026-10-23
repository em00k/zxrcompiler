//! Macro definitions and their expansion.
//!
//! Expansion is lazy, as in the Python preprocessor: source text is parsed
//! into [`Node`]s when read and evaluated when the line is emitted. A macro
//! call evaluates its body against a private copy of the table, in which the
//! arguments are bound and the macros being expanded are marked, so that a
//! macro naming itself comes out as plain text instead of recursing.

use std::collections::HashMap;
use std::rc::Rc;

/// An expandable piece of source text.
#[derive(Debug)]
pub enum Node {
    /// An identifier, possibly with an argument list. The identifier may
    /// itself be the result of another node, as in `A ## B (x)`.
    Call { fname: String, lineno: u32, id: IdRef, args: Option<ArgList> },
    /// `left ## right`
    Concat { lineno: u32, left: Rc<Node>, right: Rc<Node> },
    /// `# inner`
    Stringize { lineno: u32, inner: Rc<Node> },
}

impl Node {
    pub fn lineno(&self) -> u32 {
        match self {
            Node::Call { lineno, .. } | Node::Concat { lineno, .. } | Node::Stringize { lineno, .. } => *lineno,
        }
    }
}

#[derive(Debug)]
pub enum IdRef {
    Name(String),
    Node(Rc<Node>),
}

#[derive(Debug, Default)]
pub struct ArgList {
    pub args: Vec<Arg>,
    pub start_lineno: u32,
    pub end_lineno: u32,
}

#[derive(Debug, Default)]
pub struct Arg {
    pub tokens: Vec<ArgTok>,
}

#[derive(Debug)]
pub enum ArgTok {
    Text(String),
    Macro(Rc<Node>),
    /// A parenthesised list inside an argument.
    List(ArgList),
}

/// One element of a macro body or of a source line.
#[derive(Clone, Debug)]
pub enum DefTok {
    Text(String),
    Macro(Rc<Node>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Builtin {
    AbsFile,
    BaseFile,
    File,
    Line,
}

pub const BUILTINS: [(&str, Builtin); 4] =
    [("__ABS_FILE__", Builtin::AbsFile), ("__BASE_FILE__", Builtin::BaseFile), ("__FILE__", Builtin::File), ("__LINE__", Builtin::Line)];

#[derive(Debug)]
pub struct Definition {
    pub name: String,
    /// Parameter names; `None` for an object-like macro.
    pub params: Option<Vec<String>>,
    pub value: Vec<DefTok>,
    pub lineno: u32,
    pub fname: String,
    pub builtin: Option<Builtin>,
}

/// An error raised during expansion, reported against a line.
#[derive(Debug)]
pub struct MacroError {
    pub message: String,
    pub lineno: u32,
}

/// The table of definitions. Copying one is cheap: the definitions are
/// shared, and a copy records only what was bound or marked since.
#[derive(Clone, Default)]
pub struct Table {
    base: Rc<HashMap<String, Rc<Definition>>>,
    /// Bindings made in this copy, newest last; they shadow `base`.
    bound: Vec<(String, Rc<Definition>)>,
    /// Macros whose expansion is in progress in this copy.
    evaluating: Vec<String>,
}

impl Table {
    pub fn clear(&mut self) {
        *self = Table::default();
    }

    fn get(&self, key: &str) -> Option<&Rc<Definition>> {
        let key = key.trim();
        self.bound.iter().rev().find(|(k, _)| k == key).map(|(_, d)| d).or_else(|| self.base.get(key))
    }

    pub fn lookup(&self, key: &str) -> Option<Rc<Definition>> {
        self.get(key).cloned()
    }

    pub fn defined(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    /// Defines or replaces a macro in the shared part of the table.
    pub fn insert(&mut self, definition: Definition) {
        self.evaluating.retain(|k| *k != definition.name);
        Rc::make_mut(&mut self.base).insert(definition.name.clone(), Rc::new(definition));
    }

    pub fn undef(&mut self, key: &str) {
        if self.defined(key) {
            Rc::make_mut(&mut self.base).remove(key);
        }
    }

    /// Binds a macro argument in this copy only.
    fn bind(&mut self, name: &str, lineno: u32, value: String) {
        self.evaluating.retain(|k| k != name);
        let definition = Definition { name: name.to_string(), params: None, value: vec![DefTok::Text(value)], lineno, fname: String::new(), builtin: None };
        self.bound.push((name.to_string(), Rc::new(definition)));
    }
}

fn internal(message: &str, lineno: u32) -> MacroError {
    MacroError { message: format!("internal preprocessor error: {message}"), lineno }
}

/// The text of an argument list as it appeared: `(a,b)`.
fn arglist_text(list: &ArgList, implicit: &Table) -> Result<String, MacroError> {
    let parts: Vec<String> = list.args.iter().map(|a| eval_arg(a, implicit)).collect::<Result<_, _>>()?;
    Ok(format!("({})", parts.join(",")))
}

fn eval_arg(arg: &Arg, implicit: &Table) -> Result<String, MacroError> {
    let mut out = String::new();
    for token in &arg.tokens {
        match token {
            ArgTok::Text(text) => out.push_str(text),
            ArgTok::Macro(node) => out.push_str(&eval(node, None, implicit)?),
            ArgTok::List(list) => out.push_str(&arglist_text(list, implicit)?),
        }
    }
    Ok(out)
}

/// Evaluates a node. `explicit` is the table handed down by a macro body
/// being expanded; `implicit` is the table the node belongs to, used where
/// the Python code calls a node without passing one.
pub fn eval(node: &Node, explicit: Option<&Table>, implicit: &Table) -> Result<String, MacroError> {
    match node {
        Node::Concat { left, right, .. } => {
            let (l, r) = (eval(left, explicit, implicit)?, eval(right, explicit, implicit)?);
            Ok(format!("{}{}", l.trim_end(), r.trim_start()))
        }
        Node::Stringize { inner, .. } => Ok(format!("\"{}\"", eval(inner, explicit, implicit)?.replace('"', "\"\""))),
        Node::Call { fname, lineno, id, args } => {
            let id = match id {
                IdRef::Name(name) => name.clone(),
                IdRef::Node(inner) => eval(inner, None, implicit)?,
            };
            call(&id, args.as_ref(), fname, *lineno, explicit, implicit)
        }
    }
}

/// A call of the macro named `id`, already resolved to text.
fn call(id: &str, args: Option<&ArgList>, fname: &str, lineno: u32, explicit: Option<&Table>, implicit: &Table) -> Result<String, MacroError> {
    let source = explicit.unwrap_or(implicit);
    let Some(definition) = source.lookup(id) else {
        // Not a macro: the text goes through as written.
        let mut out = id.to_string();
        if let Some(args) = args {
            out.push_str(&arglist_text(args, implicit)?);
        }
        return Ok(out);
    };
    let mut table = source.clone();
    if definition.params.is_some() && args.is_none() {
        return Ok(id.to_string());
    }
    let mut values: Vec<String> = Vec::new();
    if let Some(args) = args {
        for arg in &args.args {
            values.push(eval_arg(arg, implicit)?.trim().to_string());
        }
    }
    let Some(params) = &definition.params else {
        let mut out = expand(&mut table, id, (fname, lineno))?;
        if args.is_some() {
            out.push_str(&format!("({})", values.join(", ")));
        }
        return Ok(out);
    };
    if values.len() != params.len() {
        return Err(MacroError { message: format!("Macro \"{id}\" expected {} params, got {}", params.len(), values.len()), lineno });
    }
    for (param, value) in params.iter().zip(values) {
        table.bind(param, lineno, value);
    }
    expand(&mut table, id, (fname, lineno))
}

/// Expands the body of the macro `key` in `table`. `site` is where the
/// expansion was asked for, which is what `__LINE__` and `__FILE__` report.
fn expand(table: &mut Table, key: &str, site: (&str, u32)) -> Result<String, MacroError> {
    let key = key.trim();
    let definition = table.lookup(key).ok_or_else(|| internal(&format!("'{key}' is not a macro"), site.1))?;
    if let Some(builtin) = definition.builtin {
        return Ok(match builtin {
            Builtin::AbsFile => format!("\"{}\"", zxr_core::paths::absolute(site.0)),
            Builtin::BaseFile => format!("\"{}\"", zxr_core::paths::basename(site.0)),
            Builtin::File => format!("\"{}\"", site.0),
            Builtin::Line => site.1.to_string(),
        });
    }
    if table.evaluating.iter().any(|k| k == key) {
        // Already being expanded further up: emit the call as text.
        let mut out = definition.name.clone();
        if let Some(params) = &definition.params {
            let parts: Vec<String> = params.iter().map(|p| expand(table, p, (&definition.fname, definition.lineno))).collect::<Result<_, _>>()?;
            out.push_str(&format!("({})", parts.join(", ")));
        }
        return Ok(out);
    }
    table.evaluating.push(key.to_string());
    let mut out = String::new();
    for token in &definition.value {
        match token {
            DefTok::Text(text) => out.push_str(text),
            DefTok::Macro(node) => match &**node {
                Node::Call { fname, lineno, id, args } => {
                    let mut id = match id {
                        IdRef::Name(name) => name.clone(),
                        IdRef::Node(inner) => eval(inner, Some(table), table)?,
                    };
                    // An object-like macro in the callee position is expanded
                    // first, on behalf of the outer call.
                    if table.lookup(&id).is_some_and(|d| d.params.is_none()) {
                        id = expand(table, &id, site)?;
                    }
                    out.push_str(&call(&id, args.as_ref(), fname, *lineno, Some(table), table)?);
                }
                other => out.push_str(&eval(other, Some(table), table)?),
            },
        }
    }
    table.evaluating.retain(|k| k != key);
    Ok(out)
}
