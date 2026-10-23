//! `optimizer/helpers.py`: registers, numbers and "unknown" values. This is
//! the part of it the peephole rules reach; the flow optimiser's own
//! helpers (`simplify_arg` and the like) belong with it.

use crate::pyeval::eval_number;

/// All 'single' registers (even f FLAG one). SP is not decomposable so it's
/// 'single' already
pub const ALL_REGS: [&str; 15] = ["a", "b", "c", "d", "e", "f", "h", "l", "ixh", "ixl", "iyh", "iyl", "r", "i", "sp"];

/// The set of all registers as they can appear in any instruction as operands
pub const REGS_OPER_SET: [&str; 21] = ["a", "b", "c", "d", "e", "h", "l", "bc", "de", "hl", "sp", "ix", "iy", "ixh", "ixl", "iyh", "iyl", "af", "af'", "i", "r"];

/// Instructions that marks the end of a basic block (any branching instruction)
pub const BLOCK_ENDERS: [&str; 8] = ["jr", "jp", "call", "ret", "reti", "retn", "djnz", "rst"];
pub const UNKNOWN_PREFIX: &str = "*UNKNOWN_";
/// Label for end program
pub const END_PROGRAM_LABEL: &str = "__END_PROGRAM";
/// Hi/Low separator
pub const HL_SEP: &str = "|";

/// The counter behind `new_tmp_val()` (`_RAND_COUNT`).
#[derive(Default, Debug, Clone)]
pub struct TmpVals {
    pub count: u64,
}

impl TmpVals {
    /// Generates an 8-bit unknown value
    pub fn new_tmp_val(&mut self) -> String {
        self.count += 1;
        format!("{UNKNOWN_PREFIX}{}", self.count)
    }

    /// Generates an unknown 16-bit tmp value concatenating two 8-it unknown ones
    pub fn new_tmp_val16(&mut self) -> String {
        let high = self.new_tmp_val();
        format!("{high}{HL_SEP}{}", self.new_tmp_val())
    }
}

/// Given a str, generates an unknown 16 bit value (always the same for the same label)
pub fn new_tmp_val16_from_label(label: &str) -> String {
    format!("{UNKNOWN_PREFIX}H_{label}{HL_SEP}{UNKNOWN_PREFIX}L_{label}")
}

pub fn is_unknown(x: &str) -> bool {
    let xx: Vec<&str> = x.split(HL_SEP).collect();
    if xx.len() > 2 {
        return false;
    }
    xx.iter().any(|x| x.starts_with(UNKNOWN_PREFIX))
}

pub fn is_unknown8(x: &str) -> bool {
    is_unknown(x) && x.split(HL_SEP).count() == 1
}

pub fn is_unknown16(x: &str) -> bool {
    is_unknown(x) && x.split(HL_SEP).count() == 2
}

/// Returns if a given string is a memory access, that is if it matches the
/// form (...)
pub fn is_mem_access(arg: &str) -> bool {
    let arg = crate::asm::py_strip(arg);
    assert!(!arg.is_empty(), "IndexError: string index out of range");
    arg.starts_with('(') && arg.ends_with(')')
}

/// `RE_NUMBER`: `^([-+]?[0-9]+|$[A-Fa-f0-9]+|[0-9][A-Fa-f0-9]*[Hh]|%[01]+|[01]+[bB])$`.
/// The second alternative begins with an unescaped `$`, so it never matches.
fn re_number(x: &str) -> bool {
    // `$` also matches before a final line end.
    let x = x.strip_suffix('\n').unwrap_or(x);
    let b = x.as_bytes();
    if b.is_empty() {
        return false;
    }
    let digits = b.strip_prefix(b"-").or_else(|| b.strip_prefix(b"+")).unwrap_or(b);
    if !digits.is_empty() && digits.iter().all(u8::is_ascii_digit) {
        return true;
    }
    if b[0].is_ascii_digit() && b.len() > 1 && matches!(b[b.len() - 1], b'h' | b'H') && b[1..b.len() - 1].iter().all(u8::is_ascii_hexdigit) {
        return true;
    }
    if b[0] == b'%' && b.len() > 1 && b[1..].iter().all(|c| matches!(c, b'0' | b'1')) {
        return true;
    }
    b.len() > 1 && matches!(b[b.len() - 1], b'b' | b'B') && b[..b.len() - 1].iter().all(|c| matches!(c, b'0' | b'1'))
}

/// Returns whether X is a numeric representation. The Python code asks
/// `eval` first and a regular expression second.
pub fn is_number(x: &str) -> bool {
    if x.is_empty() {
        return false;
    }
    let x = crate::asm::py_strip(x);
    if is_mem_access(x) {
        return false;
    }
    // Plain decimal digits with no leading zero are a number however they
    // are looked at; by far the commonest case, settled without the evaluator.
    if !x.is_empty() && x.len() < 19 && x.bytes().all(|b| b.is_ascii_digit()) && (x.len() == 1 || !x.starts_with('0')) {
        return true;
    }
    eval_number(x).is_some() || re_number(x)
}

pub fn is_label(x: &str) -> bool {
    x.starts_with('.') || x.starts_with('_') || x.is_empty()
}

/// Python's `int(text, base)`; a `ValueError` where it is not a number.
fn int_base(text: &str, base: u32) -> i128 {
    let t = crate::asm::py_strip(text);
    let (negative, body) = match t.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    let ok = !body.is_empty() && !body.starts_with('_') && !body.ends_with('_') && !body.contains("__");
    let value = if ok { i128::from_str_radix(&body.replace('_', ""), base).ok() } else { None };
    let value = value.unwrap_or_else(|| panic!("ValueError: invalid literal for int() with base {base}: '{text}'"));
    if negative { -value } else { value }
}

/// If x is a string representation of a number (hexa, binary), returns it
/// numeric value. Otherwise returns None
pub fn valnum(x: &str) -> Option<i128> {
    if !is_number(x) {
        return None;
    }

    let last = x.chars().last().unwrap();
    if let Some(rest) = x.strip_prefix('%') {
        return Some(int_base(rest, 2));
    }

    if last == 'b' || last == 'B' {
        return Some(int_base(&x[..x.len() - 1], 2));
    }

    if let Some(rest) = x.strip_prefix('$') {
        return Some(int_base(rest, 16));
    }

    if last == 'h' || last == 'H' {
        return Some(int_base(&x[..x.len() - 1], 16));
    }

    let value = eval_number(x).unwrap_or_else(|| panic!("SyntaxError: cannot evaluate '{x}'"));
    Some(value.to_int().unwrap_or_else(|| panic!("OverflowError: cannot convert float infinity to integer")))
}

/// True if x is a register in a str.
pub fn is_register(x: &str) -> bool {
    REGS_OPER_SET.contains(&x.to_lowercase().as_str())
}

/// Returns whether the given string x is a "normal" 8 bit register.
pub fn is_8bit_normal_register(x: &str) -> bool {
    matches!(x.to_lowercase().as_str(), "a" | "b" | "c" | "d" | "e" | "i" | "h" | "l")
}

/// Returns whether the given string x one of the undocumented IX, IY 8 bit registers.
pub fn is_8bit_idx_register(x: &str) -> bool {
    matches!(x.to_lowercase().as_str(), "ixh" | "ixl" | "iyh" | "iyl")
}

/// Returns whether the given string x is an 8 bit register that can be used
/// as an instruction operand, the undocumented ixh, ixl, etc. included, but
/// not h', f'.
pub fn is_8bit_oper_register(x: &str) -> bool {
    is_8bit_normal_register(x) || is_8bit_idx_register(x)
}

/// Returns whether the given string x is a "normal" 16 bit register.
pub fn is_16bit_normal_register(x: &str) -> bool {
    matches!(x.to_lowercase().as_str(), "bc" | "de" | "hl")
}

/// Returns whether the given string x is a indexable (i.e. IX + n) 16 bit register.
pub fn is_16bit_idx_register(x: &str) -> bool {
    matches!(x.to_lowercase().as_str(), "ix" | "iy")
}

/// A 16bit register that can be decomposed into a high H16 and low L16 part
pub fn is_16bit_composed_register(x: &str) -> bool {
    matches!(x.to_lowercase().as_str(), "af" | "af'" | "bc" | "de" | "hl" | "ix" | "iy")
}

/// Returns whether the given string x is a 16 bit register.
pub fn is_16bit_oper_register(x: &str) -> bool {
    matches!(x.to_lowercase().as_str(), "af" | "af'" | "bc" | "de" | "hl" | "ix" | "iy" | "sp")
}

/// Given a 16-bit register, returns the low 8 bit register of it.
#[allow(non_snake_case)]
pub fn LO16(x: &str) -> String {
    let x = x.to_lowercase();
    assert!(is_16bit_oper_register(&x), "'{x}' is not a 16bit register");
    assert!(x != "sp", "'sp' register cannot be split into two 8 bit registers");

    if is_16bit_idx_register(&x) {
        return format!("{x}l");
    }

    format!("{}{}", &x[1..2], if x.contains('\'') { "'" } else { "" })
}

/// Given a 16-bit register, returns the high 8 bit register of it.
#[allow(non_snake_case)]
pub fn HI16(x: &str) -> String {
    let x = x.to_lowercase();
    assert!(is_16bit_oper_register(&x), "'{x}' is not a 16bit register");
    assert!(x != "sp", "'sp' register cannot be split into two 8 bit registers");

    if is_16bit_idx_register(&x) {
        return format!("{x}h");
    }

    format!("{}{}", &x[..1], if x.contains('\'') { "'" } else { "" })
}

/// Given registers like ['a', 'bc', "af'", 'h', 'hl'] returns a sorted list
/// of single registers: ['a', "a'", "f'", 'b', 'c', 'h', 'l']. Non register
/// parameters (like numbers) are ignored. SP is returned as is; an 8 bit
/// register is returned as it was spelt.
pub fn single_registers<S: AsRef<str>>(op: &[S]) -> Vec<String> {
    let mut result = std::collections::BTreeSet::new();

    for x in op {
        let x = x.as_ref();
        if is_8bit_oper_register(x) || matches!(x.to_lowercase().as_str(), "f" | "sp") {
            result.insert(x.to_string());
        } else if !is_16bit_oper_register(x) {
            continue;
        } else {
            // Must be a 16bit reg or we have an internal error!
            result.insert(LO16(x));
            result.insert(HI16(x));
        }
    }

    result.into_iter().collect()
}

/// Given an x value, it must be unknown, or an integer. Then returns it
/// lower part.
#[allow(non_snake_case)]
pub fn LO16_val(x: &str, tmp: &mut TmpVals) -> String {
    if valnum(x).is_some() {
        // `int(x)` of the text itself, which is stricter than valnum().
        return (py_int(x) & 0xFF).to_string();
    }

    if !is_unknown(x) {
        return tmp.new_tmp_val();
    }

    x.split(HL_SEP).last().unwrap().to_string()
}

/// Given an x value, it must be unknown, or an integer. Then returns it
/// upper part. It it's an unknown8, return 0, because it's considered an 8
/// bit value.
#[allow(non_snake_case)]
pub fn HI16_val(x: &str, tmp: &mut TmpVals) -> String {
    if valnum(x).is_some() {
        return ((py_int(x) >> 8) & 0xFF).to_string();
    }

    if !is_unknown(x) {
        return tmp.new_tmp_val();
    }

    let text = format!("0{HL_SEP}{x}");
    let parts: Vec<&str> = text.split(HL_SEP).collect();
    parts[parts.len() - 2].to_string()
}

/// `int(text)`, raising as Python does.
pub fn py_int(text: &str) -> i128 {
    int_base(text, 10)
}

/// `int(text)` where the Python code catches the `ValueError`.
pub fn try_py_int(text: &str) -> Option<i128> {
    let t = crate::asm::py_strip(text);
    let (negative, body) = match t.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    if body.is_empty() || body.starts_with('_') || body.ends_with('_') || body.contains("__") {
        return None;
    }
    let value = body.replace('_', "").parse::<i128>().ok()?;
    Some(if negative { -value } else { value })
}

/// Like `valnum`, but a `ValueError` where it is not a number.
pub fn to_int(x: &str) -> i128 {
    valnum(x).unwrap_or_else(|| panic!("ValueError: Invalid number: {x}"))
}

pub fn get_orig_label_from_unknown16(x: &str) -> Option<String> {
    if !is_unknown16(x) {
        return None;
    }

    let h = get_H_from_unknown_value(x);
    let l = get_L_from_unknown_value(x);
    let hi = h.strip_prefix(&format!("{UNKNOWN_PREFIX}H_"))?;
    let lo = l.strip_prefix(&format!("{UNKNOWN_PREFIX}L_"))?;
    if hi != lo { None } else { Some(hi.to_string()) }
}

/// Given a 16bit *UNKNOWN value, returns it's lower part, which is the same
/// 2nd part, after splitting by HL_SEP. If the value is a composed one
/// (xxxH | yyyL) returns yyyL.
#[allow(non_snake_case)]
pub fn get_L_from_unknown_value(tmp_val: &str) -> String {
    assert!(is_unknown(tmp_val), "AssertionError: Malformed unknown value '{tmp_val}'");
    tmp_val.split(HL_SEP).last().unwrap().to_string()
}

/// Given a 16bit *UNKNOWN value, returns it's higher part, which is the
/// same 1st part, after splitting by HL_SEP.
#[allow(non_snake_case)]
pub fn get_H_from_unknown_value(tmp_val: &str) -> String {
    assert!(is_unknown(tmp_val), "AssertionError: Malformed unknown value '{tmp_val}'");
    tmp_val.split(HL_SEP).next().unwrap().to_string()
}

/// Given an asm operand, if it can be evaluated to a single number it will
/// be done so. Memory addresses will preserve their parenthesis. If the
/// string can not be simplified, it will be returned as is (stripped).
///
/// eg.:
///     0       -> 0
///     (0)     -> (0)
///     0 + 3   -> 3
///     (3 + 1) -> (4)
///     (a - 1) -> (a - 1)
///     b - 5   -> b - 5
///
/// Python asks `eval`; see `pyeval`. Where the value would be a `bool`
/// (`True`, a comparison, `not`) Python prints `True` or `False`; such an
/// operand is left as it is here.
pub fn simplify_arg(arg: &str) -> String {
    let arg = crate::asm::py_strip(arg);
    let boolish = arg.contains(['<', '>', '=', '!']) || arg.contains("True") || arg.contains("False") || arg.contains("not");
    let result = if boolish { None } else { eval_number(arg) }.map(|tmp| match tmp {
        crate::pyeval::Num::Int(i) => i.to_string(),
        crate::pyeval::Num::Float(f) => zxr_core::pynum::float_repr(f),
    });

    let Some(result) = result else { return arg.to_string() };

    if !is_mem_access(arg) {
        return result;
    }

    format!("({result})")
}

/// Given an asm instruction try to simplify its args.
pub fn simplify_asm_args(asm: &str) -> String {
    let Some((head, tail)) = asm.split_once(' ') else { return asm.to_string() };
    let args: Vec<String> = tail.splitn(2, ',').map(simplify_arg).collect();
    format!("{head} {}", args.join(", "))
}

/// Given an argument x, returns None if it's not an index operation
/// "ix/iy + n". Otherwise return a tuple (reg, oper, offset). It's case
/// insensitive and the register is always returned in lowercase. The
/// parenthesis must NOT be included.
///
/// `RE_IDX`: `^([iI][xXyY])[ ]*([-+])[ \t]*(.*)$`
pub fn idx_args(x: &str) -> Option<(String, String, String)> {
    let b = x.as_bytes();
    if b.len() < 2 || !matches!(b[0], b'i' | b'I') || !matches!(b[1], b'x' | b'X' | b'y' | b'Y') {
        return None;
    }
    let rest = x[2..].trim_start_matches(' ');
    let sign = rest.chars().next().filter(|c| *c == '-' || *c == '+')?;
    let args = rest[1..].trim_start_matches([' ', '\t']);
    // `.*$`: no line end but a last one, which is left out.
    let args = args.strip_suffix('\n').unwrap_or(args);
    if args.contains('\n') {
        return None;
    }
    Some((x[..2].to_lowercase(), sign.to_string(), args.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers() {
        // helpers.is_number(x) in the Python compiler.
        for x in ["5", "-5", "0x10", "10h", "0FFh", "%101", "101b", "1+2", "2.5", " 7 ", "True"] {
            assert!(is_number(x), "{x}");
        }
        for x in ["", "a", "hl", "(5)", "$10", "_x", ".LABEL.x", "FFh", "af'", "(ix+4)", "_a + 1"] {
            assert!(!is_number(x), "{x}");
        }
        assert_eq!(valnum("10h"), Some(16));
        assert_eq!(valnum("%101"), Some(5));
        assert_eq!(valnum("1+2"), Some(3));
    }

    #[test]
    fn registers() {
        assert_eq!(single_registers(&["a", "bc", "af'", "h", "hl", "5", "sp", "ix"]), ["a", "a'", "b", "c", "f'", "h", "ixh", "ixl", "l", "sp"]);
    }
}

/// `tests/arch/zx48k/optimizer/test_helpers.py` and the `simplify_asm_args`
/// case of `test_optimizer_asm.py`. Where the Python functions take `None`
/// the evaluator makes the unknown value itself.
#[cfg(test)]
mod flow_tests {
    use super::*;

    #[test]
    fn new_tmp_val() {
        let mut tmp = TmpVals::default();
        let (a, b) = (tmp.new_tmp_val(), tmp.new_tmp_val());
        assert_ne!(a, b, "Values must be different");
        assert!(is_unknown(&a) && is_unknown(&b));
    }

    #[test]
    fn unknowns() {
        let mut tmp = TmpVals::default();
        assert!(is_unknown(UNKNOWN_PREFIX));
        assert!(is_unknown(&format!("{UNKNOWN_PREFIX}a0")));
        assert!(is_unknown(&format!("{UNKNOWN_PREFIX}0")));
        assert!(is_unknown(&format!("{UNKNOWN_PREFIX}000|{UNKNOWN_PREFIX}001")));
        assert!(!is_unknown16(&tmp.new_tmp_val()));
        assert!(is_unknown16(&tmp.new_tmp_val16()));
        let half = format!("{}|3", tmp.new_tmp_val());
        assert!(is_unknown16(&half));
    }

    #[test]
    fn hl_unknowns() {
        let mut tmp = TmpVals::default();
        let val = tmp.new_tmp_val16();
        assert!(is_unknown(&val));
        let (a, b) = val.split_once('|').unwrap();
        assert!(is_unknown(a) && is_unknown(b));
        assert_eq!(a, get_H_from_unknown_value(&val));
        assert_eq!(b, get_L_from_unknown_value(&val));
    }

    #[test]
    fn l16_val() {
        let mut tmp = TmpVals::default();
        let tmp8 = tmp.new_tmp_val();
        assert_eq!(LO16_val(&tmp8, &mut tmp), tmp8);
        assert_eq!(LO16_val("255", &mut tmp), "255");
        assert_eq!(LO16_val("256", &mut tmp), "0");
        let tmp16 = tmp.new_tmp_val16();
        assert_eq!(LO16_val(&tmp16, &mut tmp), tmp16.split(HL_SEP).nth(1).unwrap());
        assert!(is_unknown8(&LO16_val(&tmp16, &mut tmp)));
        assert!(is_unknown8(&LO16_val("_unknown", &mut tmp))); // An unknown expression
    }

    #[test]
    fn h16_val() {
        let mut tmp = TmpVals::default();
        let tmp8 = tmp.new_tmp_val();
        assert_eq!(HI16_val(&tmp8, &mut tmp), "0");
        assert_eq!(HI16_val("255", &mut tmp), "0");
        assert_eq!(HI16_val("256", &mut tmp), "1");
        let tmp16 = tmp.new_tmp_val16();
        assert_eq!(HI16_val(&tmp16, &mut tmp), tmp16.split(HL_SEP).next().unwrap());
        assert!(is_unknown8(&HI16_val(&tmp16, &mut tmp)));
        assert!(is_unknown8(&HI16_val("_unknown", &mut tmp))); // An unknown expression
    }

    #[test]
    fn single_registers_with_flags() {
        assert_eq!(single_registers(&["af"]), ["a", "f"]);
        assert_eq!(single_registers(&["f", "sp"]), ["f", "sp"]);
    }

    #[test]
    fn simplify() {
        assert_eq!(simplify_asm_args("ld a, (126 - 1)"), "ld a, (125)");
        assert_eq!(simplify_asm_args("ld hl, (30 + 40)"), "ld hl, (70)");
        assert_eq!(simplify_asm_args("ld hl, ((30) + (40))"), "ld hl, (70)");
        assert_eq!(simplify_asm_args("ld de, (30) + (40)"), "ld de, (70)");
        // As Python's eval and str have it.
        assert_eq!(simplify_asm_args("ld a,0x10"), "ld a, 16");
        assert_eq!(simplify_asm_args("ld a, 1/2"), "ld a, 0.5");
        assert_eq!(simplify_asm_args("ld a, 4 / 2"), "ld a, 2.0");
        assert_eq!(simplify_asm_args("ld a, 010"), "ld a, 010");
        assert_eq!(simplify_asm_args("ld a, 10h"), "ld a, 10h");
        assert_eq!(simplify_asm_args("ld hl, (_a + 1)"), "ld hl, (_a + 1)");
        assert_eq!(simplify_asm_args("jp nz,  .LABEL.x"), "jp nz, .LABEL.x");
        assert_eq!(simplify_asm_args("ld (ix+0),  2 * 3"), "ld (ix+0), 6");
        assert_eq!(simplify_asm_args("ret"), "ret");
        assert_eq!(idx_args("IY - Something + 4"), Some(("iy".to_string(), "-".to_string(), "Something + 4".to_string())));
        assert_eq!(idx_args("ix + 3"), Some(("ix".to_string(), "+".to_string(), "3".to_string())));
        assert_eq!(idx_args("(ix + 3)"), None);
    }
}
