//! `peephole/evaluator.py`: the expressions of a rule's IF and DEFINE
//! sections.
//!
//! An expression arrives from the parser as nested lists ([`Tree`]) and is
//! evaluated against the variables a pattern bound. Values are text for the
//! most part; a comparison gives `""` or `"True"`, a parenthesised list
//! gives a list.
//!
//! Three functions depend on what the caller knows about the code around
//! the match: `GVAL`, `FLAGVAL` and `IS_REQUIRED`. Up to -O2 they are stubs;
//! the flow optimiser supplies its own through [`Context`].

use crate::asm::py_strip;
use crate::helpers::{self, TmpVals};
use crate::memcell::MemCell;
use crate::pattern::svar_prefix;
use std::collections::BTreeMap;

/// What the parser gives: a token, or a list of them.
#[derive(Clone, Debug, PartialEq)]
pub enum Tree {
    Str(String),
    List(Vec<Tree>),
}

impl Tree {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Tree::Str(s) => Some(s),
            Tree::List(_) => None,
        }
    }

    /// Python's `repr()`.
    pub fn repr(&self) -> String {
        match self {
            Tree::Str(s) => zxr_core::pynum::str_repr(s),
            Tree::List(items) => format!("[{}]", items.iter().map(Tree::repr).collect::<Vec<_>>().join(", ")),
        }
    }
}

/// What an expression evaluates to.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Str(String),
    Bool(bool),
    List(Vec<Value>),
    None,
}

impl Value {
    pub fn str(text: &str) -> Value {
        Value::Str(text.to_string())
    }

    pub fn truthy(&self) -> bool {
        match self {
            Value::Str(s) => !s.is_empty(),
            Value::Bool(b) => *b,
            Value::List(items) => !items.is_empty(),
            Value::None => false,
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Str(_) => "str",
            Value::Bool(_) => "bool",
            Value::List(_) => "list",
            Value::None => "NoneType",
        }
    }

    fn repr(&self) -> String {
        match self {
            Value::Str(s) => zxr_core::pynum::str_repr(s),
            other => other.py_str(),
        }
    }

    /// Python's `str()`.
    pub fn py_str(&self) -> String {
        match self {
            Value::Str(s) => s.clone(),
            Value::Bool(true) => "True".to_string(),
            Value::Bool(false) => "False".to_string(),
            Value::List(items) => format!("[{}]", items.iter().map(Value::repr).collect::<Vec<_>>().join(", ")),
            Value::None => "None".to_string(),
        }
    }

    /// The text of a value where the Python code calls a `str` method on
    /// it; anything else is an `AttributeError`.
    fn text(&self) -> &str {
        match self {
            Value::Str(s) => s,
            other => panic!("AttributeError: '{}' object is not a string", other.type_name()),
        }
    }
}

/// What the rules' functions need to know from outside.
pub trait Context {
    /// Whether a line stands for a parked block of inline assembly
    /// (`line in ASMS`).
    fn is_asm_block(&self, line: &str) -> bool;
    /// The counter behind "unknown" values.
    fn tmp_vals(&mut self) -> &mut TmpVals;
    /// `GVAL(x)`: the value a register or memory cell is known to hold.
    fn gval(&mut self, _x: &Value) -> Value {
        Value::Str(self.tmp_vals().new_tmp_val()) // To be updated in the O3 optimizer
    }
    /// `FLAGVAL(x)`: the value a flag is known to have.
    fn flagval(&mut self, _x: &Value) -> Value {
        Value::Str(self.tmp_vals().new_tmp_val())
    }
    /// `IS_REQUIRED(x)`: whether a register is used after the match.
    fn is_required(&mut self, _x: &Value) -> Value {
        Value::Bool(true) // by default always required
    }
}

/// The context of the peephole pass up to -O2: nothing is known.
pub struct BasicContext<'a> {
    pub is_asm: &'a dyn Fn(&str) -> bool,
    pub tmp: TmpVals,
}

impl Context for BasicContext<'_> {
    fn is_asm_block(&self, line: &str) -> bool {
        (self.is_asm)(line)
    }

    fn tmp_vals(&mut self) -> &mut TmpVals {
        &mut self.tmp
    }
}

/// Unary functions and operators, by name (`UNARY`).
pub const UNARY: [&str; 20] =
    ["!", "IS_ASM", "IS_INDIR", "IS_REG16", "IS_REG8", "IS_LABEL", "IS_IMMED", "LEN", "INSTR", "HIREG", "LOREG", "HIVAL", "LOVAL", "GVAL", "IS_REQUIRED", "CTEST", "NEEDS", "FLAGVAL", "OP1", "OP2"];

/// Binary operators (`BINARY`).
pub const BINARY: [&str; 11] = ["==", "+", "<>", "&&", "||", "IN", ",", ".+", ".-", ".*", "./"];

pub fn is_unary(name: &str) -> bool {
    UNARY.contains(&name)
}

pub fn is_binary(name: &str) -> bool {
    BINARY.contains(&name)
}

/// `RE_IXIY_IDX`: `^\([ \t]*i[xy][ \t]*[-+][ \t]*.+\)$`.
fn is_ixiy_idx(x: &str) -> bool {
    let x = x.strip_suffix('\n').unwrap_or(x);
    let blank = |s: &'_ str| -> usize { s.len() - s.trim_start_matches([' ', '\t']).len() };
    let Some(s) = x.strip_prefix('(') else { return false };
    let s = &s[blank(s)..];
    if !(s.starts_with("ix") || s.starts_with("iy")) {
        return false;
    }
    let s = &s[2..];
    let s = &s[blank(s)..];
    if !(s.starts_with('-') || s.starts_with('+')) {
        return false;
    }
    // `[ \t]*.+\)$`: at least one character, none of them a line end, then
    // the closing parenthesis.
    let rest = &s[1..];
    rest.chars().count() >= 2 && rest.ends_with(')') && !rest.contains('\n')
}

/// `utils.parse_int`: an integer in decimal, or hexadecimal (0xnnn, $nnnn or
/// nnnnh), or binary (%nnn or nnnb). `None` if it is not one.
pub fn parse_int(num: &str) -> Option<i128> {
    let mut num = py_strip(num).to_uppercase();
    if num.is_empty() {
        return None;
    }
    let first = num.chars().next().unwrap();
    let last = num.chars().last().unwrap();

    let mut base = 10;
    if num.starts_with("0X") {
        base = 16;
        num = num[2..].to_string();
        // `int("0X_1", 16)` is allowed, `int("0X", 16)` is not.
        num = num.strip_prefix('_').map(String::from).unwrap_or(num);
    } else if last == 'H' {
        if !first.is_ascii_digit() {
            return None;
        }
        base = 16;
        num.pop();
    } else if first == '$' {
        base = 16;
        num.remove(0);
    } else if first == '%' {
        base = 2;
        num.remove(0);
    } else if last == 'B' {
        if first != '0' && first != '1' {
            return None;
        }
        base = 2;
        num.pop();
    }

    let text = py_strip(&num);
    let (negative, body) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    if body.is_empty() || body.starts_with('_') || body.ends_with('_') || body.contains("__") {
        return None;
    }
    let value = i128::from_str_radix(&body.replace('_', ""), base).ok()?;
    Some(if negative { -value } else { value })
}

/// Emulates a number that can be also None: `Number(x)`.
fn number(value: &Value) -> Option<i128> {
    parse_int(&value.py_str())
}

fn number_str(value: Option<i128>) -> String {
    value.map(|v| v.to_string()).unwrap_or_default()
}

/// If a value is false converts it to "", otherwise to its text.
fn normalize(value: Value) -> Value {
    if !value.truthy() {
        return Value::Str(String::new());
    }
    Value::Str(value.py_str())
}

fn unary(oper: &str, x: Value, ctx: &mut dyn Context) -> Value {
    let lower = |x: &Value| py_strip(x.text()).to_lowercase();
    let from_table = |x: &Value, table: &[(&str, &str)]| Value::str(table.iter().find(|(k, _)| *k == lower(x)).map_or("", |(_, v)| v));
    // `str.split()`: on runs of blanks, with none at the ends.
    let words = |x: &str| x.split(|c: char| c.is_whitespace() || ('\x1c'..='\x1f').contains(&c)).filter(|w| !w.is_empty()).map(String::from).collect::<Vec<_>>();
    match oper {
        "!" => Value::Bool(!x.truthy()),
        "IS_ASM" => Value::Bool(x.text().starts_with("##ASM")),
        "IS_INDIR" => Value::Bool(is_ixiy_idx(x.text())),
        "IS_REG16" => Value::Bool(matches!(lower(&x).as_str(), "af" | "bc" | "de" | "hl" | "ix" | "iy")),
        "IS_REG8" => Value::Bool(matches!(lower(&x).as_str(), "a" | "b" | "c" | "d" | "e" | "h" | "l" | "ixh" | "ixl" | "iyh" | "iyl")),
        "IS_LABEL" => Value::Bool(py_strip(x.text()).ends_with(':')),
        "IS_IMMED" => Value::Bool(!py_strip(x.text()).starts_with('(')),
        "LEN" => Value::Str(words(x.text()).len().to_string()),
        "INSTR" => Value::Str(words(x.text()).into_iter().next().unwrap_or_else(|| panic!("IndexError: list index out of range"))),
        "HIREG" => from_table(&x, &[("af", "a"), ("bc", "b"), ("de", "d"), ("hl", "h"), ("ix", "ixh"), ("iy", "iyh")]),
        "LOREG" => from_table(&x, &[("af", "f"), ("bc", "c"), ("de", "e"), ("hl", "l"), ("ix", "ixl"), ("iy", "iyl")]),
        "HIVAL" => match &x {
            Value::None => Value::Str(ctx.tmp_vals().new_tmp_val()),
            x => Value::Str(helpers::HI16_val(x.text(), ctx.tmp_vals())),
        },
        "LOVAL" => match &x {
            Value::None => Value::Str(ctx.tmp_vals().new_tmp_val()),
            x => Value::Str(helpers::LO16_val(x.text(), ctx.tmp_vals())),
        },
        "GVAL" => ctx.gval(&x),
        "IS_REQUIRED" => ctx.is_required(&x),
        // condition test, if any. E.g. retz returns 'z'
        "CTEST" => MemCell::new(x.text(), 1).condition_flag().map_or(Value::None, Value::str),
        "NEEDS" => {
            let Value::List(args) = &x else { panic!("TypeError: NEEDS takes an instruction and a list of registers") };
            let (instr, regs) = match args.as_slice() {
                [instr, regs, ..] => (instr.text(), regs),
                _ => panic!("IndexError: list index out of range"),
            };
            let regs: Vec<String> = match regs {
                Value::List(items) => items.iter().map(|r| r.text().to_string()).collect(),
                other => vec![other.text().to_string()],
            };
            let cell = MemCell::new(instr, 1);
            let in_asms = ctx.is_asm_block(cell.code());
            Value::Bool(cell.needs(&regs, in_asms))
        }
        "FLAGVAL" => ctx.flagval(&x),
        // 1st and 2nd operand of an instruction, or ""
        "OP1" | "OP2" => {
            let parts = words(&py_strip(x.text()).replacen(',', " ", 1));
            Value::Str(parts.get(if oper == "OP1" { 1 } else { 2 }).cloned().unwrap_or_default())
        }
        other => panic!("ValueError: Invalid unary operator '{other}'"),
    }
}

/// An expression ready to be evaluated: the Python `Evaluator`.
#[derive(Clone, Debug, PartialEq)]
pub struct Evaluator {
    expression: Expr,
    /// The expression as the parser gave it, as Python's `str()` shows it.
    pub str_: String,
}

#[derive(Clone, Debug, PartialEq)]
enum Expr {
    /// An empty expression: always true.
    True,
    /// A token: a variable, or text standing for itself.
    Atom(String),
    /// A single element that is itself a list, which is returned as it is.
    Raw(Tree),
    Unary(String, Box<Evaluator>),
    Binary(Box<Evaluator>, String, Box<Evaluator>),
    /// A list: the elements, without the commas between them.
    List(Vec<Evaluator>),
}

fn raw_value(tree: &Tree) -> Value {
    match tree {
        Tree::Str(s) => Value::Str(s.clone()),
        Tree::List(items) => Value::List(items.iter().map(raw_value).collect()),
    }
}

impl Evaluator {
    /// Prepares an expression (an AST in nested lists from the parser) for
    /// its later execution. An `Err` is Python's `ValueError`.
    pub fn new(expression: &Tree) -> Result<Evaluator, String> {
        let str_ = match expression {
            Tree::Str(s) => s.clone(),
            list => list.repr(),
        };
        let items: Vec<Tree> = match expression {
            Tree::Str(_) => vec![expression.clone()],
            Tree::List(items) => items.clone(),
        };

        let expression = if items.is_empty() {
            Expr::True
        } else if items.len() == 1 {
            match &items[0] {
                Tree::Str(s) => Expr::Atom(s.clone()),
                list => Expr::Raw(list.clone()),
            }
        } else if items.len() == 2 {
            match items[0].as_str() {
                Some(op) if is_unary(op) => Expr::Unary(op.to_string(), Box::new(Evaluator::new(&items[1])?)),
                _ => return Err(format!("Invalid operator '{}'", items[0].repr())),
            }
        } else if items.len() == 3 && items[1].as_str() != Some(",") {
            match items[1].as_str() {
                Some(op) if is_binary(op) => Expr::Binary(Box::new(Evaluator::new(&items[0])?), op.to_string(), Box::new(Evaluator::new(&items[2])?)),
                _ => return Err(format!("Invalid operator '{}'", items[1].repr())),
            }
        } else {
            // It's a list
            if items.len() % 2 == 0 || items.iter().skip(1).step_by(2).any(|x| x.as_str() != Some(",")) {
                return Err(format!("Invalid expression {}", Tree::List(items).repr()));
            }
            Expr::List(items.iter().step_by(2).map(Evaluator::new).collect::<Result<_, _>>()?)
        };

        Ok(Evaluator { expression, str_ })
    }

    pub fn eval(&self, vars: &BTreeMap<String, Value>, ctx: &mut dyn Context) -> Value {
        match &self.expression {
            Expr::True => Value::Bool(true),
            Expr::Raw(tree) => raw_value(tree),
            Expr::Atom(val) => {
                if val == "$" {
                    return Value::Str(val.clone());
                }

                if svar_prefix(val).is_none() {
                    return Value::Str(val.clone());
                }

                match vars.get(val) {
                    None => panic!("UnboundVarError: Unbound variable '{val}'"),
                    Some(value) => value.clone(),
                }
            }
            Expr::Unary(oper, operand) => {
                let operand = operand.eval(vars, ctx);
                normalize(unary(oper, operand, ctx))
            }
            Expr::Binary(left, oper, right) => {
                // Do lazy evaluation
                let result = match oper.as_str() {
                    "==" => Value::Bool(left.eval(vars, ctx) == right.eval(vars, ctx)),
                    "<>" => Value::Bool(left.eval(vars, ctx) != right.eval(vars, ctx)),
                    "+" => match (left.eval(vars, ctx), right.eval(vars, ctx)) {
                        (Value::Str(a), Value::Str(b)) => Value::Str(a + &b),
                        (Value::List(mut a), Value::List(b)) => {
                            a.extend(b);
                            Value::List(a)
                        }
                        (a, b) => panic!("TypeError: unsupported operand type(s) for +: '{}' and '{}'", a.type_name(), b.type_name()),
                    },
                    "&&" => {
                        let x = left.eval(vars, ctx);
                        if x.truthy() { right.eval(vars, ctx) } else { x }
                    }
                    "||" => {
                        let x = left.eval(vars, ctx);
                        if x.truthy() { x } else { right.eval(vars, ctx) }
                    }
                    "IN" => {
                        let x = left.eval(vars, ctx);
                        match right.eval(vars, ctx) {
                            Value::List(items) => Value::Bool(items.contains(&x)),
                            Value::Str(text) => Value::Bool(text.contains(x.text())),
                            other => panic!("TypeError: argument of type '{}' is not iterable", other.type_name()),
                        }
                    }
                    ".+" | ".-" | ".*" | "./" => {
                        let (a, b) = (number(&left.eval(vars, ctx)), number(&right.eval(vars, ctx)));
                        let both = a.zip(b);
                        Value::Str(match oper.as_str() {
                            ".+" => number_str(both.and_then(|(a, b)| a.checked_add(b))),
                            ".-" => number_str(both.and_then(|(a, b)| a.checked_sub(b))),
                            ".*" => number_str(both.and_then(|(a, b)| a.checked_mul(b))),
                            // The quotient is a float, whose text `Number`
                            // cannot read back: a division gives "", or
                            // "None" when dividing by zero.
                            _ => match both {
                                Some((_, 0)) => "None".to_string(),
                                _ => String::new(),
                            },
                        })
                    }
                    other => panic!("ValueError: Invalid binary operator '{other}'"),
                };
                normalize(result)
            }
            Expr::List(items) => Value::List(items.iter().map(|x| x.eval(vars, ctx)).collect()),
        }
    }
}
