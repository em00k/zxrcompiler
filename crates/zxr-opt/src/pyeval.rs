//! A small evaluator standing in for Python's `eval(text, {}, {})` where the
//! optimiser asks "is this operand a number, and which one?".
//!
//! It understands what an assembler operand that Python can evaluate to a
//! number looks like: integer and float literals in Python's spellings,
//! `True` and `False`, parentheses, the arithmetic, bitwise and comparison
//! operators, `not`, `and`, `or`. Anything else (a name, a string, a call,
//! a syntax error, an integer too large for 128 bits) is "not a number",
//! which is also what happens in Python when `eval` raises or gives
//! something that is not an `int` or a `float`.

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Num {
    Int(i128),
    Float(f64),
}

impl Num {
    fn as_f64(self) -> f64 {
        match self {
            Num::Int(i) => i as f64,
            Num::Float(f) => f,
        }
    }

    fn truthy(self) -> bool {
        match self {
            Num::Int(i) => i != 0,
            Num::Float(f) => f != 0.0,
        }
    }

    /// Python's `int(x)`.
    pub fn to_int(self) -> Option<i128> {
        match self {
            Num::Int(i) => Some(i),
            Num::Float(f) if f.is_finite() => Some(f.trunc() as i128),
            Num::Float(_) => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(Num),
    Op(&'static str),
    Name(String),
}

const OPS: [&str; 26] = ["**", "//", "<<", ">>", "<=", ">=", "==", "!=", "+", "-", "*", "/", "%", "&", "|", "^", "~", "<", ">", "(", ")", "not", "and", "or", "True", "False"];

fn number(chars: &[char], i: &mut usize) -> Option<Num> {
    let start = *i;
    let text = |a: usize, b: usize| chars[a..b].iter().filter(|&&c| c != '_').collect::<String>();
    // Underscores are allowed between digits only.
    let digits = |i: &mut usize, ok: &dyn Fn(char) -> bool| -> bool {
        let from = *i;
        while *i < chars.len() && (ok(chars[*i]) || (chars[*i] == '_' && *i > from && chars.get(*i + 1).is_some_and(|&c| ok(c)))) {
            *i += 1;
        }
        *i > from
    };
    if chars[*i] == '0' && chars.get(*i + 1).is_some_and(|c| "xXoObB".contains(*c)) {
        let radix = match chars[*i + 1].to_ascii_lowercase() {
            'x' => 16,
            'o' => 8,
            _ => 2,
        };
        *i += 2;
        if chars.get(*i) == Some(&'_') {
            *i += 1;
        }
        let from = *i;
        if !digits(i, &|c| c.is_digit(radix)) {
            return None;
        }
        return i128::from_str_radix(&text(from, *i), radix).ok().map(Num::Int);
    }
    let dec = |c: char| c.is_ascii_digit();
    let whole = digits(i, &dec);
    let mut is_float = false;
    if *i < chars.len() && chars[*i] == '.' {
        is_float = true;
        *i += 1;
        let frac = digits(i, &dec);
        if !whole && !frac {
            return None;
        }
    }
    if *i < chars.len() && (chars[*i] == 'e' || chars[*i] == 'E') {
        let mark = *i;
        *i += 1;
        if *i < chars.len() && (chars[*i] == '+' || chars[*i] == '-') {
            *i += 1;
        }
        if digits(i, &dec) {
            is_float = true;
        } else {
            *i = mark;
        }
    }
    let body = text(start, *i);
    if is_float {
        return body.parse().ok().map(Num::Float);
    }
    // A decimal integer cannot start with a zero unless it is all zeros.
    if body.len() > 1 && body.starts_with('0') && body.bytes().any(|b| b != b'0') {
        return None;
    }
    body.parse().ok().map(Num::Int)
}

fn tokens(text: &str) -> Option<Vec<Tok>> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == ' ' || c == '\t' || c == '\x0c' {
            i += 1;
        } else if c.is_ascii_digit() || (c == '.' && chars.get(i + 1).is_some_and(char::is_ascii_digit)) {
            out.push(Tok::Num(number(&chars, &mut i)?));
            // `1x` and `10h` are not Python.
            if chars.get(i).is_some_and(|&c| c.is_alphanumeric() || c == '_' || c == '.') {
                return None;
            }
        } else if c.is_alphabetic() || c == '_' {
            let from = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[from..i].iter().collect();
            match OPS.iter().find(|op| **op == word) {
                Some(op) => out.push(Tok::Op(op)),
                None => out.push(Tok::Name(word)),
            }
        } else {
            let rest: String = chars[i..chars.len().min(i + 2)].iter().collect();
            let op = OPS.iter().find(|op| !op.as_bytes()[0].is_ascii_alphabetic() && rest.starts_with(**op))?;
            out.push(Tok::Op(op));
            i += op.len();
        }
    }
    Some(out)
}

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
}

type R = Option<Num>;

fn int_op(a: Num, b: Num, f: impl Fn(i128, i128) -> Option<i128>) -> R {
    match (a, b) {
        (Num::Int(x), Num::Int(y)) => f(x, y).map(Num::Int),
        // A TypeError in Python.
        _ => None,
    }
}

fn arith(a: Num, b: Num, int: impl Fn(i128, i128) -> Option<i128>, float: impl Fn(f64, f64) -> Option<f64>) -> R {
    match (a, b) {
        (Num::Int(x), Num::Int(y)) => int(x, y).map(Num::Int),
        _ => float(a.as_f64(), b.as_f64()).map(Num::Float),
    }
}

impl Parser {
    fn peek(&self) -> Option<&'static str> {
        match self.toks.get(self.pos) {
            Some(Tok::Op(op)) => Some(op),
            _ => None,
        }
    }

    fn eat(&mut self, op: &str) -> bool {
        if self.peek() == Some(op) {
            self.pos += 1;
            return true;
        }
        false
    }

    // Lowest first, as in Python's grammar: or, and, not, comparison, |, ^,
    // &, shifts, sums, terms, unary, power.
    fn or_(&mut self) -> R {
        let mut left = self.and_()?;
        while self.eat("or") {
            // The right-hand side is not evaluated when the left decides,
            // but it has to parse. (In Python a name there would then be no
            // error; here it is, which makes such an operand "not a number".)
            let right = self.and_();
            if !left.truthy() {
                left = right?;
            }
        }
        Some(left)
    }

    fn and_(&mut self) -> R {
        let mut left = self.not_()?;
        while self.eat("and") {
            let right = self.not_();
            if left.truthy() {
                left = right?;
            }
        }
        Some(left)
    }

    fn not_(&mut self) -> R {
        if self.eat("not") {
            let value = self.not_()?;
            return Some(Num::Int(!value.truthy() as i128));
        }
        self.comparison()
    }

    fn comparison(&mut self) -> R {
        let mut left = self.bit_or()?;
        let mut result: Option<bool> = None;
        while let Some(op) = self.peek().filter(|op| ["<", ">", "<=", ">=", "==", "!="].contains(op)) {
            self.pos += 1;
            let right = self.bit_or()?;
            let (a, b) = (left.as_f64(), right.as_f64());
            let exact = match (left, right) {
                (Num::Int(x), Num::Int(y)) => Some(x.cmp(&y)),
                _ => a.partial_cmp(&b),
            };
            use std::cmp::Ordering::*;
            let holds = match op {
                "<" => exact == Some(Less),
                ">" => exact == Some(Greater),
                "<=" => matches!(exact, Some(Less | Equal)),
                ">=" => matches!(exact, Some(Greater | Equal)),
                "==" => exact == Some(Equal),
                _ => exact != Some(Equal),
            };
            // A chain `a < b < c` holds when every link does.
            result = Some(result.unwrap_or(true) && holds);
            left = right;
        }
        Some(match result {
            Some(b) => Num::Int(b as i128),
            None => left,
        })
    }

    fn bit_or(&mut self) -> R {
        let mut left = self.bit_xor()?;
        while self.eat("|") {
            let right = self.bit_xor()?;
            left = int_op(left, right, |a, b| Some(a | b))?;
        }
        Some(left)
    }

    fn bit_xor(&mut self) -> R {
        let mut left = self.bit_and()?;
        while self.eat("^") {
            let right = self.bit_and()?;
            left = int_op(left, right, |a, b| Some(a ^ b))?;
        }
        Some(left)
    }

    fn bit_and(&mut self) -> R {
        let mut left = self.shift()?;
        while self.eat("&") {
            let right = self.shift()?;
            left = int_op(left, right, |a, b| Some(a & b))?;
        }
        Some(left)
    }

    fn shift(&mut self) -> R {
        let mut left = self.sum()?;
        loop {
            if self.eat("<<") {
                let right = self.sum()?;
                left = int_op(left, right, |a, b| if (0..100).contains(&b) { a.checked_mul(1i128.checked_shl(b as u32)?) } else { None })?;
            } else if self.eat(">>") {
                let right = self.sum()?;
                left = int_op(left, right, |a, b| if b < 0 { None } else { Some(if b > 127 { if a < 0 { -1 } else { 0 } } else { a >> b }) })?;
            } else {
                return Some(left);
            }
        }
    }

    fn sum(&mut self) -> R {
        let mut left = self.term()?;
        loop {
            if self.eat("+") {
                let right = self.term()?;
                left = arith(left, right, |a, b| a.checked_add(b), |a, b| Some(a + b))?;
            } else if self.eat("-") {
                let right = self.term()?;
                left = arith(left, right, |a, b| a.checked_sub(b), |a, b| Some(a - b))?;
            } else {
                return Some(left);
            }
        }
    }

    fn term(&mut self) -> R {
        let mut left = self.unary()?;
        loop {
            if self.eat("*") {
                let right = self.unary()?;
                left = arith(left, right, |a, b| a.checked_mul(b), |a, b| Some(a * b))?;
            } else if self.eat("//") {
                let right = self.unary()?;
                left = arith(left, right, |a, b| if b == 0 { None } else { Some(a.div_euclid(b) - if (a.rem_euclid(b) != 0) && (b < 0) { 1 } else { 0 }) }, |a, b| if b == 0.0 { None } else { Some((a / b).floor()) })?;
            } else if self.eat("/") {
                let right = self.unary()?;
                // True division always gives a float.
                if right.as_f64() == 0.0 {
                    return None;
                }
                left = Num::Float(left.as_f64() / right.as_f64());
            } else if self.eat("%") {
                let right = self.unary()?;
                left = arith(
                    left,
                    right,
                    |a, b| if b == 0 { None } else { Some(((a % b) + b) % b) },
                    |a, b| if b == 0.0 { None } else { Some(a - b * (a / b).floor()) },
                )?;
            } else {
                return Some(left);
            }
        }
    }

    fn unary(&mut self) -> R {
        if self.eat("-") {
            return match self.unary()? {
                Num::Int(i) => i.checked_neg().map(Num::Int),
                Num::Float(f) => Some(Num::Float(-f)),
            };
        }
        if self.eat("+") {
            return self.unary();
        }
        if self.eat("~") {
            return match self.unary()? {
                Num::Int(i) => Some(Num::Int(!i)),
                Num::Float(_) => None,
            };
        }
        self.power()
    }

    fn power(&mut self) -> R {
        let base = self.atom()?;
        if self.eat("**") {
            // The exponent binds as a unary expression: 2 ** -1.
            let exp = self.unary()?;
            return match (base, exp) {
                (Num::Int(b), Num::Int(e)) if e >= 0 => u32::try_from(e).ok().and_then(|e| b.checked_pow(e)).map(Num::Int),
                _ => {
                    let value = base.as_f64().powf(exp.as_f64());
                    // 0 ** -1 is a ZeroDivisionError, a negative base with a
                    // fractional exponent gives a complex number.
                    if value.is_finite() { Some(Num::Float(value)) } else { None }
                }
            };
        }
        Some(base)
    }

    fn atom(&mut self) -> R {
        match self.toks.get(self.pos).cloned() {
            Some(Tok::Num(n)) => {
                self.pos += 1;
                Some(n)
            }
            Some(Tok::Op("(")) => {
                self.pos += 1;
                let value = self.or_()?;
                if !self.eat(")") {
                    return None;
                }
                Some(value)
            }
            Some(Tok::Op("True")) => {
                self.pos += 1;
                Some(Num::Int(1))
            }
            Some(Tok::Op("False")) => {
                self.pos += 1;
                Some(Num::Int(0))
            }
            _ => None,
        }
    }
}

/// The number `eval(text, {}, {})` gives in Python, if it gives one.
pub fn eval_number(text: &str) -> Option<Num> {
    let toks = tokens(text.trim_matches([' ', '\t', '\x0c']))?;
    if toks.is_empty() {
        return None;
    }
    let mut parser = Parser { toks, pos: 0 };
    let value = parser.or_()?;
    if parser.pos != parser.toks.len() {
        return None;
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::{eval_number, Num};

    #[test]
    fn numbers_python_evaluates() {
        for (text, value) in [
            ("5", 5),
            (" 12 ", 12),
            ("0x1F", 31),
            ("0b101", 5),
            ("0o17", 15),
            ("1_000", 1000),
            ("-3", -3),
            ("1 + 2 * 3", 7),
            ("(1 + 2) * 3", 9),
            ("7 // 2", 3),
            ("-7 // 2", -4),
            ("-7 % 3", 2),
            ("1 << 4", 16),
            ("0xFF & 15 | 64", 79),
            ("~0", -1),
            ("2 ** 10", 1024),
            ("True", 1),
            ("1 < 2", 1),
            ("not 5", 0),
            ("0 or 7", 7),
            ("00", 0),
        ] {
            assert_eq!(eval_number(text), Some(Num::Int(value)), "{text:?}");
        }
        for (text, value) in [("2.5", 2.5), ("1e3", 1000.0), ("4 / 2", 2.0), (".5", 0.5), ("5.", 5.0), ("1 + 0.5", 1.5)] {
            assert_eq!(eval_number(text), Some(Num::Float(value)), "{text:?}");
        }
        for text in ["", "a", "hl", "_x + 1", "10h", "$10", "%101", "0FFh", "1 +", "(1", "1 / 0", "007", "'a'", ".LABEL.x", "1,2", "af'", "x or 1", "1 or x", "1 2", "1__0", "0x", "abs(1)", "(ix+4)"] {
            assert_eq!(eval_number(text), None, "{text:?}");
        }
    }
}
