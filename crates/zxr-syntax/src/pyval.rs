//! Python's arithmetic on the values constants are folded with, and the
//! folding functions the grammar actions hand to `make_binary`, `make_unary`
//! and `make_builtin` (lambdas in the Python source).
//!
//! Integers are `i128`, which stands in for Python's unbounded `int`. Where a
//! result does not fit, or where Python would raise, the compile ends the way
//! a Python traceback ends it: see [`crash`].

use crate::ast::Val;
use crate::types::Num;

/// Stands in for an uncaught Python exception: the oracle prints a traceback
/// and exits with status 1. One line is printed here instead.
pub fn crash(what: &str) -> ! {
    eprintln!("zxbc: internal error: {what}");
    zxr_core::exit(1)
}

fn overflow() -> ! {
    crash("integer constant too large (more than 127 bits)")
}

fn both_int(a: Num, b: Num) -> Option<(i128, i128)> {
    match (a, b) {
        (Num::Int(a), Num::Int(b)) => Some((a, b)),
        _ => None,
    }
}

fn int_only(a: Num, b: Num, op: &str) -> (i128, i128) {
    both_int(a, b).unwrap_or_else(|| crash(&format!("TypeError: unsupported operand type(s) for {op}: a float")))
}

/// A float result, which Python refuses when it overflows (`math` functions
/// and `**`; plain `+ - *` give `inf`).
fn finite(x: f64, inputs_finite: bool) -> f64 {
    if x.is_infinite() && inputs_finite {
        crash("OverflowError: (34, 'Numerical result out of range')");
    }
    x
}

pub fn add(a: Num, b: Num) -> Num {
    match both_int(a, b) {
        Some((a, b)) => Num::Int(a.checked_add(b).unwrap_or_else(|| overflow())),
        None => Num::Float(a.as_f64() + b.as_f64()),
    }
}

pub fn sub(a: Num, b: Num) -> Num {
    match both_int(a, b) {
        Some((a, b)) => Num::Int(a.checked_sub(b).unwrap_or_else(|| overflow())),
        None => Num::Float(a.as_f64() - b.as_f64()),
    }
}

pub fn mul(a: Num, b: Num) -> Num {
    match both_int(a, b) {
        Some((a, b)) => Num::Int(a.checked_mul(b).unwrap_or_else(|| overflow())),
        None => Num::Float(a.as_f64() * b.as_f64()),
    }
}

/// `a / b`: true division, always a float.
pub fn truediv(a: Num, b: Num) -> Num {
    if b.is_zero() {
        crash(if both_int(a, b).is_some() { "ZeroDivisionError: division by zero" } else { "ZeroDivisionError: float division by zero" });
    }
    Num::Float(a.as_f64() / b.as_f64())
}

/// `a % b`: the result takes the sign of the divisor.
pub fn modulo(a: Num, b: Num) -> Num {
    if b.is_zero() {
        crash(if both_int(a, b).is_some() { "ZeroDivisionError: integer modulo by zero" } else { "ZeroDivisionError: float modulo" });
    }
    match both_int(a, b) {
        Some((a, b)) => {
            let r = a % b;
            Num::Int(if r != 0 && (r < 0) != (b < 0) { r + b } else { r })
        }
        None => {
            let (x, y) = (a.as_f64(), b.as_f64());
            let mut r = x % y;
            if r != 0.0 {
                if (y < 0.0) != (r < 0.0) {
                    r += y;
                }
            } else {
                r = 0.0f64.copysign(y);
            }
            Num::Float(r)
        }
    }
}

/// `a ** b`.
pub fn pow(a: Num, b: Num) -> Num {
    if let Some((x, y)) = both_int(a, b) {
        if y >= 0 {
            let exp = u32::try_from(y).unwrap_or_else(|_| overflow());
            return Num::Int(x.checked_pow(exp).unwrap_or_else(|| overflow()));
        }
        if x == 0 {
            crash("ZeroDivisionError: 0.0 cannot be raised to a negative power");
        }
    }
    let (x, y) = (a.as_f64(), b.as_f64());
    if x == 0.0 && y < 0.0 {
        crash("ZeroDivisionError: 0.0 cannot be raised to a negative power");
    }
    if x < 0.0 && y.is_finite() && y.fract() != 0.0 {
        // Python gives a complex number, which no constant can hold.
        crash("AssertionError: a negative number raised to a fractional power is complex");
    }
    Num::Float(finite(x.powf(y), x.is_finite() && y.is_finite()))
}

pub fn shl(a: Num, b: Num) -> Num {
    let (a, b) = int_only(a, b, "<<");
    if b < 0 {
        crash("ValueError: negative shift count");
    }
    if a == 0 {
        return Num::Int(0);
    }
    let shift = u32::try_from(b).ok().filter(|&s| s < 127).unwrap_or_else(|| overflow());
    let result = a << shift;
    if result >> shift != a {
        overflow();
    }
    Num::Int(result)
}

pub fn shr(a: Num, b: Num) -> Num {
    let (a, b) = int_only(a, b, ">>");
    if b < 0 {
        crash("ValueError: negative shift count");
    }
    // An arithmetic shift, as Python's: -1 >> n is -1.
    Num::Int(if b >= 127 { if a < 0 { -1 } else { 0 } } else { a >> b })
}

pub fn bitand(a: Num, b: Num) -> Num {
    let (a, b) = int_only(a, b, "&");
    Num::Int(a & b)
}

pub fn bitor(a: Num, b: Num) -> Num {
    let (a, b) = int_only(a, b, "|");
    Num::Int(a | b)
}

pub fn bitxor(a: Num, b: Num) -> Num {
    let (a, b) = int_only(a, b, "^");
    Num::Int(a ^ b)
}

pub fn neg(a: Num) -> Num {
    match a {
        Num::Int(i) => Num::Int(i.checked_neg().unwrap_or_else(|| overflow())),
        Num::Float(f) => Num::Float(-f),
    }
}

pub fn invert(a: Num) -> Num {
    match a {
        Num::Int(i) => Num::Int(!i),
        Num::Float(_) => crash("TypeError: bad operand type for unary ~: 'float'"),
    }
}

pub fn truthy(v: &Val) -> bool {
    match v {
        Val::Num(n) => !n.is_zero(),
        Val::Str(s) => !s.is_empty(),
    }
}

/// `a == b` for two Python values; a number never equals a string.
pub fn equal(a: &Val, b: &Val) -> bool {
    match (a, b) {
        (Val::Num(a), Val::Num(b)) => a == b,
        (Val::Str(a), Val::Str(b)) => a == b,
        _ => false,
    }
}

/// `a < b`. Strings compare by code point, as `str` does.
pub fn less(a: &Val, b: &Val) -> bool {
    match (a, b) {
        (Val::Num(a), Val::Num(b)) => a < b,
        (Val::Str(a), Val::Str(b)) => a.chars().lt(b.chars()),
        _ => crash("TypeError: '<' not supported between instances of 'str' and a number"),
    }
}

/// The folding functions, named after the operation of the lambda they stand
/// for. Each is a `Fold1` or a `Fold2`.
pub mod fold {
    use super::*;

    fn n(v: &Val) -> Num {
        v.num()
    }

    /// `lambda x, y: x + y`, which also joins strings.
    pub fn add(x: &Val, y: &Val) -> Val {
        match (x, y) {
            (Val::Str(a), Val::Str(b)) => Val::Str(format!("{a}{b}")),
            _ => Val::Num(super::add(n(x), n(y))),
        }
    }

    pub fn sub(x: &Val, y: &Val) -> Val {
        Val::Num(super::sub(n(x), n(y)))
    }

    pub fn mul(x: &Val, y: &Val) -> Val {
        Val::Num(super::mul(n(x), n(y)))
    }

    pub fn div(x: &Val, y: &Val) -> Val {
        Val::Num(super::truediv(n(x), n(y)))
    }

    pub fn modulo(x: &Val, y: &Val) -> Val {
        Val::Num(super::modulo(n(x), n(y)))
    }

    pub fn pow(x: &Val, y: &Val) -> Val {
        Val::Num(super::pow(n(x), n(y)))
    }

    pub fn shl(x: &Val, y: &Val) -> Val {
        Val::Num(super::shl(n(x), n(y)))
    }

    pub fn shr(x: &Val, y: &Val) -> Val {
        Val::Num(super::shr(n(x), n(y)))
    }

    pub fn eq(x: &Val, y: &Val) -> Val {
        Val::bool(equal(x, y))
    }

    pub fn ne(x: &Val, y: &Val) -> Val {
        Val::bool(!equal(x, y))
    }

    pub fn lt(x: &Val, y: &Val) -> Val {
        Val::bool(less(x, y))
    }

    pub fn le(x: &Val, y: &Val) -> Val {
        Val::bool(!less(y, x))
    }

    pub fn gt(x: &Val, y: &Val) -> Val {
        Val::bool(less(y, x))
    }

    pub fn ge(x: &Val, y: &Val) -> Val {
        Val::bool(!less(x, y))
    }

    /// `lambda x, y: x or y`: the first operand if it is true, else the second.
    pub fn or(x: &Val, y: &Val) -> Val {
        if truthy(x) { x.clone() } else { y.clone() }
    }

    /// `lambda x, y: x and y`.
    pub fn and(x: &Val, y: &Val) -> Val {
        if truthy(x) { y.clone() } else { x.clone() }
    }

    /// `lambda x, y: (x and not y) or (not x and y)`. Python's `and` and `or`
    /// give back an operand, so the result is not always a bool:
    /// `x and not y` is `x` itself when `x` is false.
    pub fn xor(x: &Val, y: &Val) -> Val {
        let first = if truthy(x) { Val::bool(!truthy(y)) } else { x.clone() };
        if truthy(&first) {
            return first;
        }
        if !truthy(x) { y.clone() } else { Val::bool(false) }
    }

    pub fn bor(x: &Val, y: &Val) -> Val {
        Val::Num(super::bitor(n(x), n(y)))
    }

    pub fn band(x: &Val, y: &Val) -> Val {
        Val::Num(super::bitand(n(x), n(y)))
    }

    pub fn bxor(x: &Val, y: &Val) -> Val {
        Val::Num(super::bitxor(n(x), n(y)))
    }

    pub fn neg(x: &Val) -> Val {
        Val::Num(super::neg(n(x)))
    }

    pub fn not(x: &Val) -> Val {
        Val::bool(!truthy(x))
    }

    pub fn bnot(x: &Val) -> Val {
        Val::Num(super::invert(n(x)))
    }

    /// `asc` of `p_code`: the code of the first character, or 0.
    pub fn asc(x: &Val) -> Val {
        Val::int(x.as_str().chars().next().map_or(0, |c| c as i128))
    }

    /// `lambda x: x < 0 and -1 or x > 0 and 1 or 0`.
    pub fn sgn(x: &Val) -> Val {
        let zero = Val::int(0);
        Val::int(if less(x, &zero) { -1 } else if less(&zero, x) { 1 } else { 0 })
    }

    /// `lambda x: x if x >= 0 else -x`.
    pub fn abs(x: &Val) -> Val {
        if less(x, &Val::int(0)) { Val::Num(super::neg(n(x))) } else { x.clone() }
    }

    fn math(x: &Val, f: fn(f64) -> f64, domain: fn(f64) -> bool) -> Val {
        let x = n(x).as_f64();
        if !domain(x) {
            crash("ValueError: math domain error");
        }
        let r = f(x);
        if r.is_infinite() && x.is_finite() {
            crash("OverflowError: math range error");
        }
        Val::float(r)
    }

    /// `math.sin`, and so on: the C library's functions, as in Python.
    pub fn sin(x: &Val) -> Val {
        math(x, f64::sin, f64::is_finite)
    }

    pub fn cos(x: &Val) -> Val {
        math(x, f64::cos, f64::is_finite)
    }

    pub fn tan(x: &Val) -> Val {
        math(x, f64::tan, f64::is_finite)
    }

    pub fn asn(x: &Val) -> Val {
        math(x, f64::asin, |x| (-1.0..=1.0).contains(&x))
    }

    pub fn acs(x: &Val) -> Val {
        math(x, f64::acos, |x| (-1.0..=1.0).contains(&x))
    }

    pub fn atn(x: &Val) -> Val {
        math(x, f64::atan, |_| true)
    }

    /// `lambda y: math.log(y, math.exp(1))`: the logarithm of y over the
    /// logarithm of the base, which is how `math.log` works it out.
    pub fn ln(x: &Val) -> Val {
        math(x, |y| y.ln() / 1f64.exp().ln(), |y| y > 0.0)
    }

    pub fn exp(x: &Val) -> Val {
        math(x, f64::exp, |_| true)
    }

    pub fn sqr(x: &Val) -> Val {
        math(x, f64::sqrt, |x| x >= 0.0)
    }
}

/// What `float(eval(s, {}, {}))` gives for the text of a `VAL("...")` with
/// a constant argument, or `None` where Python raises. Only numbers and
/// arithmetic are evaluated: literals, parentheses, unary `+ - ~` and the
/// binary `+ - * / // % ** << >> & | ^`. Anything else Python would accept
/// (names of built-ins, comparisons, strings, conditionals) comes out as
/// `None` here; see docs/quirks.md.
pub fn eval_to_float(text: &str) -> Option<f64> {
    let chars: Vec<char> = text.chars().collect();
    let mut e = Eval { s: &chars, i: 0 };
    e.blanks();
    let value = e.or()?;
    // A trailing line end is allowed.
    while matches!(e.peek(), Some(' ' | '\t' | '\n' | '\r' | '\x0c')) {
        e.i += 1;
    }
    if e.i != chars.len() {
        return None;
    }
    Some(value.as_f64())
}

struct Eval<'a> {
    s: &'a [char],
    i: usize,
}

impl Eval<'_> {
    fn peek(&self) -> Option<char> {
        self.s.get(self.i).copied()
    }

    fn blanks(&mut self) {
        while matches!(self.peek(), Some(' ' | '\t' | '\x0c')) {
            self.i += 1;
        }
    }

    /// Takes `op` if it is next, and not the start of a longer operator.
    fn eat(&mut self, op: &str) -> bool {
        let n = op.chars().count();
        if self.i + n > self.s.len() || !self.s[self.i..self.i + n].iter().copied().eq(op.chars()) {
            return false;
        }
        let next = self.s.get(self.i + n).copied();
        let longer = match op {
            "*" => next == Some('*'),
            "/" => next == Some('/'),
            "<" | ">" | "|" | "&" | "^" | "+" | "-" | "%" => false,
            _ => false,
        };
        if longer || next == Some('=') {
            return false;
        }
        self.i += n;
        self.blanks();
        true
    }

    fn ints(a: Num, b: Num) -> Option<(i128, i128)> {
        both_int(a, b)
    }

    fn or(&mut self) -> Option<Num> {
        let mut a = self.xor()?;
        while self.eat("|") {
            let (x, y) = Self::ints(a, self.xor()?)?;
            a = Num::Int(x | y);
        }
        Some(a)
    }

    fn xor(&mut self) -> Option<Num> {
        let mut a = self.and()?;
        while self.eat("^") {
            let (x, y) = Self::ints(a, self.and()?)?;
            a = Num::Int(x ^ y);
        }
        Some(a)
    }

    fn and(&mut self) -> Option<Num> {
        let mut a = self.shift()?;
        while self.eat("&") {
            let (x, y) = Self::ints(a, self.shift()?)?;
            a = Num::Int(x & y);
        }
        Some(a)
    }

    fn shift(&mut self) -> Option<Num> {
        let mut a = self.sum()?;
        loop {
            if self.eat("<<") {
                let (x, y) = Self::ints(a, self.sum()?)?;
                let shift = u32::try_from(y).ok().filter(|&s| s < 127)?;
                let r = x.checked_shl(shift).filter(|r| r >> shift == x)?;
                a = Num::Int(r);
            } else if self.eat(">>") {
                let (x, y) = Self::ints(a, self.sum()?)?;
                if y < 0 {
                    return None;
                }
                a = Num::Int(if y >= 127 { if x < 0 { -1 } else { 0 } } else { x >> y });
            } else {
                return Some(a);
            }
        }
    }

    fn sum(&mut self) -> Option<Num> {
        let mut a = self.term()?;
        loop {
            if self.eat("+") {
                let b = self.term()?;
                a = match both_int(a, b) {
                    Some((x, y)) => Num::Int(x.checked_add(y)?),
                    None => Num::Float(a.as_f64() + b.as_f64()),
                };
            } else if self.eat("-") {
                let b = self.term()?;
                a = match both_int(a, b) {
                    Some((x, y)) => Num::Int(x.checked_sub(y)?),
                    None => Num::Float(a.as_f64() - b.as_f64()),
                };
            } else {
                return Some(a);
            }
        }
    }

    fn term(&mut self) -> Option<Num> {
        let mut a = self.factor()?;
        loop {
            if self.eat("//") {
                let b = self.factor()?;
                if b.is_zero() {
                    return None;
                }
                a = match both_int(a, b) {
                    Some((x, y)) => Num::Int(x.checked_div_euclid(y).map(|q| if y < 0 && x.rem_euclid(y) != 0 { q - 1 } else { q })?),
                    None => Num::Float((a.as_f64() / b.as_f64()).floor()),
                };
            } else if self.eat("*") {
                let b = self.factor()?;
                a = match both_int(a, b) {
                    Some((x, y)) => Num::Int(x.checked_mul(y)?),
                    None => Num::Float(a.as_f64() * b.as_f64()),
                };
            } else if self.eat("/") {
                let b = self.factor()?;
                if b.is_zero() {
                    return None;
                }
                a = Num::Float(a.as_f64() / b.as_f64());
            } else if self.eat("%") {
                let b = self.factor()?;
                if b.is_zero() {
                    return None;
                }
                a = modulo(a, b);
            } else {
                return Some(a);
            }
        }
    }

    fn factor(&mut self) -> Option<Num> {
        if self.eat("+") {
            return self.factor();
        }
        if self.eat("-") {
            return Some(match self.factor()? {
                Num::Int(i) => Num::Int(i.checked_neg()?),
                Num::Float(f) => Num::Float(-f),
            });
        }
        if self.eat("~") {
            return match self.factor()? {
                Num::Int(i) => Some(Num::Int(!i)),
                Num::Float(_) => None,
            };
        }
        self.power()
    }

    /// `atom ** factor`: binds tighter than a sign on its left, and takes
    /// one on its right.
    fn power(&mut self) -> Option<Num> {
        let base = self.atom()?;
        if !self.eat("**") {
            return Some(base);
        }
        let exp = self.factor()?;
        if let Some((x, y)) = both_int(base, exp) {
            if y >= 0 {
                return Some(Num::Int(x.checked_pow(u32::try_from(y).ok()?)?));
            }
            if x == 0 {
                return None;
            }
        }
        let (x, y) = (base.as_f64(), exp.as_f64());
        if x == 0.0 && y < 0.0 {
            return None;
        }
        if x < 0.0 && y.is_finite() && y.fract() != 0.0 {
            return None; // complex
        }
        let r = x.powf(y);
        if r.is_infinite() && x.is_finite() && y.is_finite() {
            return None; // OverflowError
        }
        Some(Num::Float(r))
    }

    fn atom(&mut self) -> Option<Num> {
        if self.peek() == Some('(') {
            self.i += 1;
            // Inside parentheses a line end is a blank too.
            let skip = |e: &mut Self| {
                while matches!(e.peek(), Some(' ' | '\t' | '\n' | '\r' | '\x0c')) {
                    e.i += 1;
                }
            };
            skip(self);
            let value = self.or()?;
            skip(self);
            if self.peek() != Some(')') {
                return None;
            }
            self.i += 1;
            self.blanks();
            return Some(value);
        }
        let start = self.i;
        while self.peek().is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.') {
            // An exponent's sign belongs to the literal.
            if matches!(self.peek(), Some('e' | 'E')) && matches!(self.s.get(self.i + 1), Some('+' | '-')) && !Self::is_prefixed(&self.s[start..self.i]) {
                self.i += 1;
            }
            self.i += 1;
        }
        let text: String = self.s[start..self.i].iter().collect();
        self.blanks();
        Self::literal(&text)
    }

    fn is_prefixed(chars: &[char]) -> bool {
        chars.len() >= 2 && chars[0] == '0' && matches!(chars[1], 'x' | 'X' | 'o' | 'O' | 'b' | 'B')
    }

    /// A Python numeric literal. Underscores go between digits only.
    fn literal(text: &str) -> Option<Num> {
        let digits_ok = |d: &str, radix: u32| !d.is_empty() && !d.starts_with('_') && !d.ends_with('_') && !d.contains("__") && d.chars().all(|c| c == '_' || c.is_digit(radix));
        let lower = text.to_ascii_lowercase();
        for (prefix, radix) in [("0x", 16), ("0o", 8), ("0b", 2)] {
            if let Some(d) = lower.strip_prefix(prefix) {
                // One underscore may follow the prefix.
                let d = d.strip_prefix('_').unwrap_or(d);
                if !digits_ok(d, radix) {
                    return None;
                }
                return i128::from_str_radix(&d.replace('_', ""), radix).ok().map(Num::Int);
            }
        }
        if digits_ok(&lower, 10) {
            let d = lower.replace('_', "");
            // A decimal integer has no leading zero, unless it is all zeros.
            if d.len() > 1 && d.starts_with('0') && d.chars().any(|c| c != '0') {
                return None;
            }
            return d.parse::<i128>().ok().map(Num::Int);
        }
        // A float: digits, a point, an exponent.
        let (mantissa, exponent) = match lower.split_once('e') {
            Some((m, e)) => (m, Some(e)),
            None => (lower.as_str(), None),
        };
        let (whole, frac) = match mantissa.split_once('.') {
            Some((w, f)) => (w, Some(f)),
            None => (mantissa, None),
        };
        if frac.is_none() && exponent.is_none() {
            return None;
        }
        if whole.is_empty() && frac.is_none_or(str::is_empty) {
            return None;
        }
        if !whole.is_empty() && !digits_ok(whole, 10) || frac.is_some_and(|f| !f.is_empty() && !digits_ok(f, 10)) {
            return None;
        }
        if let Some(e) = exponent {
            let e = e.strip_prefix(['+', '-']).unwrap_or(e);
            if !digits_ok(e, 10) {
                return None;
            }
        }
        lower.replace('_', "").parse::<f64>().ok().map(Num::Float)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn i(x: i128) -> Num {
        Num::Int(x)
    }

    fn f(x: f64) -> Num {
        Num::Float(x)
    }

    // Expected values are what CPython 3 prints.
    #[test]
    fn python_modulo_and_power() {
        assert_eq!(modulo(i(-7), i(3)), i(2));
        assert_eq!(modulo(i(7), i(-3)), i(-2));
        assert_eq!(modulo(f(-7.5), i(2)), f(0.5));
        assert_eq!(modulo(f(7.5), i(-2)), f(-0.5));
        assert_eq!(pow(i(2), i(10)), i(1024));
        assert_eq!(pow(i(2), i(-1)), f(0.5));
        assert_eq!(pow(f(2.0), i(3)), f(8.0));
        assert_eq!(truediv(i(7), i(2)), f(3.5));
        assert_eq!(shr(i(-1), i(3)), i(-1));
        assert_eq!(shl(i(3), i(4)), i(48));
        assert_eq!(invert(i(5)), i(-6));
    }

    // Expected values are what CPython 3 prints.
    #[test]
    fn val_evaluates_arithmetic() {
        for (text, expected) in [
            ("1+2*3", 7.0),
            ("2**-1", 0.5),
            ("-2**2", -4.0),
            ("7//2", 3.0),
            ("-7//2", -4.0),
            ("7%-3", -2.0),
            ("1_000", 1000.0),
            ("0x1F", 31.0),
            ("1e3", 1000.0),
            (".5", 0.5),
            ("5.", 5.0),
            ("(1+2)*3", 9.0),
            ("~5", -6.0),
            ("1<<4", 16.0),
            ("6/4", 1.5),
            ("  3", 3.0),
            ("010.5", 10.5),
            ("1.5e-3 ", 0.0015),
            ("2**0.5", 1.4142135623730951),
        ] {
            assert_eq!(eval_to_float(text), Some(expected), "{text}");
        }
        for text in ["", "007", "1/0", "abc", "1 +", "(1", "1j", "1__0", "2 3", "0x", "1.5.2", "(-8)**0.5", "1.0<<2"] {
            assert_eq!(eval_to_float(text), None, "{text}");
        }
    }

    #[test]
    fn math_folds_match_python() {
        use fold::*;
        let f = |v: Val| v.num().as_f64();
        assert_eq!(f(sin(&Val::float(1.0))), 0.8414709848078965);
        assert_eq!(f(ln(&Val::float(10.0))), 2.302585092994046);
        assert_eq!(f(exp(&Val::float(2.5))), 12.182493960703473);
        assert_eq!(f(atn(&Val::float(0.3))), 0.2914567944778671);
        assert_eq!(f(sqr(&Val::float(2.0))), 1.4142135623730951);
        assert_eq!(sgn(&Val::float(-0.5)), Val::int(-1));
        assert_eq!(sgn(&Val::int(0)), Val::int(0));
        assert_eq!(abs(&Val::int(-3)), Val::int(3));
        assert_eq!(asc(&Val::Str("A".into())), Val::int(65));
        assert_eq!(asc(&Val::Str(String::new())), Val::int(0));
    }

    #[test]
    fn boolean_folds_return_operands() {
        use fold::*;
        assert_eq!(or(&Val::int(0), &Val::int(7)), Val::int(7));
        assert_eq!(or(&Val::int(3), &Val::int(7)), Val::int(3));
        assert_eq!(and(&Val::int(3), &Val::int(7)), Val::int(7));
        assert_eq!(and(&Val::int(0), &Val::int(7)), Val::int(0));
        // (x and not y) or (not x and y)
        assert_eq!(xor(&Val::int(0), &Val::int(7)), Val::int(7));
        assert_eq!(xor(&Val::int(3), &Val::int(0)), Val::int(1));
        assert_eq!(xor(&Val::int(3), &Val::int(7)), Val::int(0));
        assert_eq!(xor(&Val::int(0), &Val::int(0)), Val::int(0));
        assert_eq!(xor(&Val::float(0.0), &Val::float(0.0)), Val::float(0.0));
    }
}
