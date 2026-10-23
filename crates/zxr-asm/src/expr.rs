//! Assembler expressions. They are trees because a label may not have a value
//! yet when it is used; evaluation is retried when the memory is dumped.

use std::cell::RefCell;
use std::rc::Rc;
use zxr_core::Session;

pub type Label = Rc<RefCell<LabelData>>;

#[derive(Clone, Debug)]
pub enum LabelValue {
    Int(i128),
    /// `NAME EQU expression`
    Expr(Expr),
}

#[derive(Debug)]
pub struct LabelData {
    /// The name as referenced or declared; a temporary label keeps its
    /// direction suffix here (`1F`).
    pub raw: String,
    pub lineno: u32,
    /// The file `lineno` is a line of.
    pub filename: Rc<str>,
    /// The source line it was declared on or generated for, for debug data.
    pub source: Option<(Rc<str>, u32)>,
    pub value: Option<LabelValue>,
    pub local: bool,
    pub namespace: Option<String>,
    /// Declared at an address rather than with `EQU`.
    pub is_address: bool,
    /// The output segment (code bank) it was declared in; 0 is resident.
    pub segment: i64,
}

impl LabelData {
    pub fn new(raw: &str, lineno: u32, value: Option<LabelValue>, local: bool, namespace: Option<String>, is_address: bool) -> Label {
        Rc::new(RefCell::new(LabelData { raw: raw.to_string(), lineno, filename: Rc::from(""), source: None, value, local, namespace, is_address, segment: 0 }))
    }

    pub fn defined(&self) -> bool {
        self.value.is_some()
    }

    pub fn is_temporary(&self) -> bool {
        self.raw.chars().next().is_some_and(|c| c.is_ascii_digit())
    }

    /// Which way a temporary label reference looks: -1 back, 1 forward.
    pub fn direction(&self) -> i8 {
        match self.raw.chars().last() {
            Some('B') if self.is_temporary() => -1,
            Some('F') if self.is_temporary() => 1,
            _ => 0,
        }
    }

    pub fn name(&self) -> String {
        if self.is_temporary() { self.raw.trim_matches(['B', 'F']).to_string() } else { self.raw.clone() }
    }
}

#[derive(Debug)]
pub enum Item {
    Int(i128),
    Label(Label),
    /// An operator: `+ - * / ^ % & | ~ << >>`.
    Op(&'static str),
}

#[derive(Debug)]
pub struct ExprNode {
    pub item: Item,
    pub lineno: u32,
    pub children: Vec<Expr>,
}

pub type Expr = Rc<ExprNode>;

pub fn int(value: i128, lineno: u32) -> Expr {
    Rc::new(ExprNode { item: Item::Int(value), lineno, children: Vec::new() })
}

pub fn label(label: Label, lineno: u32) -> Expr {
    Rc::new(ExprNode { item: Item::Label(label), lineno, children: Vec::new() })
}

pub fn op(operator: &'static str, lineno: u32, children: Vec<Expr>) -> Expr {
    Rc::new(ExprNode { item: Item::Op(operator), lineno, children })
}

/// The operator a token spells.
pub fn operator(text: &str) -> &'static str {
    match text {
        "+" => "+",
        "-" => "-",
        "*" => "*",
        "/" => "/",
        "^" => "^",
        "%" => "%",
        "&" => "&",
        "|" => "|",
        "~" => "~",
        "<<" => "<<",
        ">>" => ">>",
        other => panic!("not an operator: {other}"),
    }
}

/// Python's `//` and `%`: the quotient rounds down and the remainder takes
/// the sign of the divisor.
fn floor_div(a: i128, b: i128) -> Option<i128> {
    let q = a.checked_div(b)?;
    Some(if a % b != 0 && ((a < 0) != (b < 0)) { q - 1 } else { q })
}

fn python_mod(a: i128, b: i128) -> Option<i128> {
    let r = a.checked_rem(b)?;
    Some(if r != 0 && ((r < 0) != (b < 0)) { r + b } else { r })
}

thread_local! {
    /// How many labels deep an evaluation is, following one EQU to the next.
    static DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// More EQUs in a chain than any program has: beyond it the labels are
/// defined in terms of each other.
const MAX_DEPTH: u32 = 512;

/// Evaluates an expression. A label with no value yet makes the result
/// `None`; with `strict` that is also reported as an error.
pub fn eval(expr: &Expr, strict: bool, sess: &mut Session) -> Option<i128> {
    match &expr.item {
        Item::Int(value) => Some(*value),
        Item::Label(label) => {
            let value = label.borrow().value.clone();
            match value {
                Some(LabelValue::Int(value)) => Some(value),
                Some(LabelValue::Expr(inner)) => {
                    let depth = DEPTH.get();
                    if depth >= MAX_DEPTH {
                        // The Python assembler recurses until it dies.
                        if strict {
                            let name = label.borrow().name();
                            sess.error(expr.lineno, &format!("Label '{name}' is defined in terms of itself"), None);
                        }
                        return None;
                    }
                    DEPTH.set(depth + 1);
                    let value = eval(&inner, strict, sess);
                    DEPTH.set(depth);
                    value
                }
                None => {
                    if strict {
                        let name = label.borrow().name();
                        sess.error(expr.lineno, &format!("Undefined label '{name}'"), None);
                    }
                    None
                }
            }
        }
        Item::Op(operator) => {
            if expr.children.len() == 1 && matches!(*operator, "-" | "+") {
                let value = eval(&expr.children[0], strict, sess)?;
                return if *operator == "-" { value.checked_neg() } else { Some(value) };
            }
            // Both sides are evaluated before either is checked.
            let left = eval(expr.children.first()?, strict, sess);
            let right = eval(expr.children.get(1)?, strict, sess);
            let (a, b) = (left?, right?);
            let value = match *operator {
                "-" => a.checked_sub(b),
                "+" => a.checked_add(b),
                "*" => a.checked_mul(b),
                "/" | "%" if b == 0 => {
                    sess.error(expr.lineno, "Division by 0", None);
                    return None;
                }
                "/" => floor_div(a, b),
                "%" => python_mod(a, b),
                "^" => u32::try_from(b).ok().and_then(|e| a.checked_pow(e)),
                "&" => Some(a & b),
                "|" => Some(a | b),
                "~" => Some(a ^ b),
                "<<" => u32::try_from(b).ok().and_then(|n| a.checked_shl(n)).filter(|r| r >> b == a),
                ">>" => u32::try_from(b).ok().map(|n| if n >= 127 { if a < 0 { -1 } else { 0 } } else { a >> n }),
                _ => return None,
            };
            // Python's integers have no limit. These have 127 bits, and a
            // result that does not fit is said so, not taken for 0.
            if value.is_none() && strict {
                sess.error(expr.lineno, "Arithmetic overflow in expression", None);
            }
            value
        }
    }
}

/// Every label a list of expressions mentions, in the order the Python
/// assembler visits them: a stack, so the last argument comes first.
pub fn collect_labels(args: &[Expr]) -> Vec<Label> {
    let mut out = Vec::new();
    let mut pending: Vec<&Expr> = args.iter().collect();
    while let Some(node) = pending.pop() {
        if let Item::Label(label) = &node.item {
            out.push(Rc::clone(label));
        }
        pending.extend(node.children.iter());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn python_integer_division() {
        assert_eq!(floor_div(7, 2), Some(3));
        assert_eq!(floor_div(-7, 2), Some(-4));
        assert_eq!(floor_div(7, -2), Some(-4));
        assert_eq!(python_mod(-7, 2), Some(1));
        assert_eq!(python_mod(7, -2), Some(-1));
        assert_eq!(python_mod(6, 3), Some(0));
    }

    #[test]
    fn evaluates() {
        let mut sess = Session::default();
        let e = op("-", 1, vec![op("*", 1, vec![int(3, 1), int(4, 1)]), int(2, 1)]);
        assert_eq!(eval(&e, true, &mut sess), Some(10));
        let undefined = label(LabelData::new(".x", 1, None, false, None, false), 1);
        assert_eq!(eval(&op("+", 1, vec![int(1, 1), undefined]), false, &mut sess), None);
        assert_eq!(sess.has_errors, 0);
    }
}
