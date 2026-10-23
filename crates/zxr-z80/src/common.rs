//! What the backend's modules share: the state of one compilation and the
//! helpers of `backend/common.py`.
//!
//! The Python backend keeps its state in module globals (`REQUIRES`, `INITS`,
//! `ASMS`, `AT_END`, the two `FLAG_`s, the temporary-label counter of
//! `api/tmp_labels.py` and the registries of `backend/codebank.py`). Here it
//! all lives in a [`Common`], which every quad handler receives.
//!
//! Where a Python function raises an exception no caller catches, the port
//! panics with a message; the command line turns that into a one-line error.

use crate::codebank::CodeBank;
use crate::runtime;
use std::collections::{BTreeMap, BTreeSet};
use zxr_ir::Quad;

/// Modules that, if included, need `__MEM_INIT` called at start-up.
pub const MEMINITS: [&str; 8] = ["mem/alloc.asm", "loadstr.asm", "storestr2.asm", "storestr.asm", "strcat.asm", "strcpy.asm", "string.asm", "strslice.asm"];

/// `global_.CORE_NAMESPACE`.
pub const NAMESPACE: &str = ".core";
/// `global_.LABELS_NAMESPACE`.
pub const LABELS_NAMESPACE: &str = ".LABEL";

// Labels for the program's start and exit.
pub const START_LABEL: &str = ".core.__START_PROGRAM";
pub const END_LABEL: &str = ".core.__END_PROGRAM";
pub const CALL_BACK: &str = ".core.__CALL_BACK__";
pub const MAIN_LABEL: &str = ".core.__MAIN_PROGRAM__";
pub const DATA_LABEL: &str = ".core.ZXBASIC_USER_DATA";
pub const DATA_END_LABEL: &str = ".core.ZXBASIC_USER_DATA_END";

/// Index register (base pointer).
pub const IDX_REG: &str = "ix";

/// The options the backend reads. They are the values in force once the
/// source has been parsed, so a `#pragma` has had its say.
#[derive(Clone, Debug)]
pub struct Options {
    pub architecture: String,
    pub org: i64,
    pub heap_size: i64,
    pub heap_address: Option<i64>,
    pub heap_start_label: String,
    pub heap_size_label: String,
    pub headerless: bool,
    pub autorun: bool,
    pub optimization_level: i64,
    /// `size`, `speed` or `auto`.
    pub opt_strategy: String,
    pub codewindow: i64,
    pub codewindowsize: i64,
    pub codebankbase: i64,
    pub codebankpages: String,
    pub codebankdepth: i64,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            architecture: "zx48k".to_string(),
            org: 32768,
            heap_size: 4768,
            heap_address: None,
            heap_start_label: format!("{NAMESPACE}.ZXBASIC_MEM_HEAP"),
            heap_size_label: format!("{NAMESPACE}.ZXBASIC_HEAP_SIZE"),
            headerless: false,
            autorun: false,
            optimization_level: 0,
            opt_strategy: "auto".to_string(),
            codewindow: 0x6000,
            codewindowsize: 8192,
            codebankbase: 30,
            codebankpages: String::new(),
            codebankdepth: 16,
        }
    }
}

/// `api/tmp_labels.py`: generated labels `.LABEL.__LABELn`. The front end
/// draws from the same counter, so it is handed back and forth with it.
#[derive(Default, Debug)]
pub struct TmpLabels {
    pub counter: u64,
    /// Every label generated so far (`TMP_LABELS`).
    pub labels: BTreeSet<String>,
}

impl TmpLabels {
    /// Generates a new unique temporary label, like `.LABEL.__LABEL1`.
    pub fn tmp_label(&mut self) -> String {
        let result = format!("{LABELS_NAMESPACE}.__LABEL{}", self.counter);
        self.labels.insert(result.clone());
        self.counter += 1;
        result
    }

    /// Moves the counter on to `counter`, as if the labels in between had
    /// been generated here: what the front end did while it had the counter.
    pub fn advance_to(&mut self, counter: u64) {
        while self.counter < counter {
            self.tmp_label();
        }
    }
}

/// The state of one compilation, as far as the backend goes.
#[derive(Default, Debug)]
pub struct Common {
    pub options: Options,
    /// Whether the FunctionExit scheme has been already used or not.
    pub flag_use_function_exit: bool,
    /// Whether an `end` has already been emitted or not.
    pub flag_end_emitted: bool,
    /// Appended at the end of code emission (useful for lvard, for example).
    pub at_end: Vec<String>,
    /// ASM blocks entered by the user (these won't be optimised), by the
    /// `##ASMn` token standing for each in the instruction stream.
    pub asms: BTreeMap<String, Vec<String>>,
    /// ASM blocks counter.
    pub asmcount: u64,
    /// Required libraries (included once).
    pub requires: BTreeSet<String>,
    /// Routines called automatically at start.
    pub inits: BTreeSet<String>,
    pub tmp_labels: TmpLabels,
    pub codebank: CodeBank,
}

impl Common {
    pub fn new(options: Options) -> Common {
        Common { options, ..Common::default() }
    }

    /// `tmp_labels.tmp_label()`.
    pub fn tmp_label(&mut self) -> String {
        self.tmp_labels.tmp_label()
    }

    /// The `call` of a runtime routine; the library file holding it is
    /// noted as required.
    pub fn runtime_call(&mut self, label: &str) -> String {
        assert!(runtime::RUNTIME_LABELS.binary_search(&label).is_ok(), "Invalid runtime label '{label}'");
        if let Some(module) = runtime::required_module(label) {
            self.requires.insert(module.to_string());
        }
        format!("call {label}")
    }

    /// Returns a new unique ASM block id.
    pub fn new_asmid(&mut self) -> String {
        let result = format!("##ASM{}", self.asmcount);
        self.asmcount += 1;
        result
    }

    pub fn normalize_boolean(&mut self) -> Vec<String> {
        if self.options.opt_strategy == "size" {
            return vec![self.runtime_call(runtime::NORMALIZE_BOOLEAN)];
        }
        strs(&[
            "sub 1",    // Carry if A = 0
            "sbc a, a", // 0xFF if A was 0, 0 otherwise
            "inc a",    // 0 if A was 0, 1 otherwise
        ])
    }

    // ------------------------------------------------------------------
    // Typecast conversions
    // ------------------------------------------------------------------

    /// The instruction sequence for converting from the given type to byte.
    pub fn to_byte(&mut self, stype: &str) -> Vec<String> {
        let mut output = Vec::new();
        if stype == "bool" {
            return self.normalize_boolean();
        }
        if stype == "i8" || stype == "u8" {
            return output;
        }
        if is_int_type(stype) {
            output.push("ld a, l".to_string());
        } else if stype == "f16" {
            output.push("ld a, e".to_string());
        } else if stype == "f" {
            // Converts C ED LH to byte
            output.push(self.runtime_call(runtime::FTOU32REG));
            output.push("ld a, l".to_string());
        }
        output
    }

    /// The instruction sequence for converting the given type stored in
    /// DE,HL to word (unsigned) HL.
    pub fn to_word(&mut self, stype: &str) -> Vec<String> {
        let mut output = Vec::new();
        if stype == "bool" {
            output.extend(self.normalize_boolean());
        }
        if stype == "bool" || stype == "u8" {
            // Byte to word
            output.push("ld l, a".to_string());
            output.push("ld h, 0".to_string());
        } else if stype == "i8" {
            // Signed byte to word
            output.extend(strs(&["ld l, a", "add a, a", "sbc a, a", "ld h, a"]));
        } else if stype == "f16" {
            // Must MOVE HL into DE
            output.push("ex de, hl".to_string());
        } else if stype == "f" {
            output.push(self.runtime_call(runtime::FTOU32REG));
        }
        output
    }

    /// The instruction sequence for converting the given type stored in
    /// DE,HL to long (DE, HL).
    pub fn to_long(&mut self, stype: &str) -> Vec<String> {
        let mut output = Vec::new();
        if stype == "bool" {
            output = self.normalize_boolean();
            output.extend(strs(&["ld l, a", "ld h, 0", "ld e, h", "ld d, h"]));
            return output;
        }
        if stype == "i8" || stype == "u8" {
            // Byte to word
            output = self.to_word(stype);
            output.push("ld e, h".to_string());
            output.push("ld d, h".to_string());
        } else if stype == "i16" {
            // Signed byte or fixed to word
            output.extend(strs(&["ld a, h", "add a, a", "sbc a, a", "ld e, a", "ld d, a"]));
        } else if stype == "f16" {
            output.extend(strs(&[
                "ex de, hl",
                "ld de, 0",
                // Copies the highest bit (sign) to DE
                "ld a, h",
                "add a, a",
                "sbc a, a",
                "ld e, a",
                "ld d, a",
            ]));
        } else if stype == "u32" || stype == "i32" {
            return Vec::new();
        } else if stype == "u16" {
            output.push("ld de, 0".to_string());
        } else if stype == "f" {
            output.push(self.runtime_call(runtime::FTOU32REG));
        } else {
            panic!("type conversion from {stype} to long is undefined");
        }
        output
    }

    /// The instruction sequence for converting the given type stored in
    /// DE,HL to fixed DE,HL.
    pub fn to_fixed(&mut self, stype: &str) -> Vec<String> {
        if stype == "bool" || is_int_type(stype) {
            let mut output = self.to_word(stype);
            output.extend(strs(&[
                "ex de, hl",
                "ld hl, 0", // 'Truncate' the fixed point
            ]));
            return output;
        }
        if stype == "f" {
            return vec![self.runtime_call(runtime::FTOF16REG)];
        }
        panic!("type conversion from {stype} to fixed");
    }

    /// The instruction sequence for converting the given type stored in
    /// DE,HL to float.
    pub fn to_float(&mut self, stype: &str) -> Vec<String> {
        let mut output = Vec::new();
        if stype == "f" {
            return output; // Nothing to do
        }
        if stype == "f16" {
            output.push(self.runtime_call(runtime::F16TOFREG));
            return output;
        }
        if stype == "bool" {
            output.extend(self.normalize_boolean());
        }
        // If we reach this point, it's an integer type
        if stype == "bool" || stype == "u8" {
            // The ZX Spectrum ROM FP-Calc already returns 0 or 1 for Booleans
            output.push(self.runtime_call(runtime::U8TOFREG));
        } else if stype == "i8" {
            output.push(self.runtime_call(runtime::I8TOFREG));
        } else if matches!(stype, "i16" | "i32" | "u16" | "u32") {
            if stype == "i16" || stype == "u16" {
                output.extend(self.to_long(stype));
            }
            if stype == "i16" || stype == "i32" {
                output.push(self.runtime_call(runtime::I32TOFREG));
            } else {
                output.push(self.runtime_call(runtime::U32TOFREG));
            }
        } else {
            panic!("type conversion from {stype} to float is undefined");
        }
        output
    }
}

/// A list of instructions from literals.
pub fn strs(lines: &[&str]) -> Vec<String> {
    lines.iter().map(|s| s.to_string()).collect()
}

/// A quad the way the Python code indexes it: `ins[0]` is the instruction
/// and `ins[1]` its first operand.
pub trait QuadExt {
    /// `ins[i]`. An operand that is not there is an `IndexError` in Python.
    fn at(&self, i: usize) -> &str;
}

impl QuadExt for Quad {
    fn at(&self, i: usize) -> &str {
        if i == 0 {
            return &self.op;
        }
        match self.args.get(i - 1) {
            Some(arg) => arg,
            None => panic!("IndexError: quad {self} has no operand {i}"),
        }
    }
}

/// `op[0]`: the first character of an operand. Python raises `IndexError`
/// on an empty one.
pub fn first(op: &str) -> char {
    op.chars().next().unwrap_or_else(|| panic!("IndexError: string index out of range"))
}

/// Internal data types with their size in bytes (`YY_TYPES`). A string has
/// none, and asking for it is a `KeyError`.
pub fn yy_type_size(stype: &str) -> i64 {
    match stype {
        "bool" | "u8" | "i8" => 1,
        "u16" | "i16" => 2,
        "u32" | "i32" | "f16" => 4,
        "f" => 5,
        other => panic!("KeyError: '{other}'"),
    }
}

/// `math.log(x) / math.log(2)`.
pub fn log2(x: f64) -> f64 {
    x.ln() / 2f64.ln()
}

/// Whether x is an exact power of 2, the way the Python code works it out:
/// through floating-point logarithms.
pub fn is_2n(x: i128) -> bool {
    if x < 1 {
        return false;
    }
    let n = log2(x as f64);
    n == n.trunc()
}

// ------------------------------------------------------------------
// Operands checking
// ------------------------------------------------------------------

/// Python's `str.strip()` with no argument, for the ASCII blanks it takes.
fn py_strip(text: &str) -> &str {
    text.trim_matches(|c: char| matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0b' | '\x0c' | '\x1c'..='\x1f' | '\u{85}' | '\u{a0}') || c.is_whitespace())
}

/// Drops the underscores Python allows between two digits; `None` if one is
/// anywhere else.
fn strip_underscores(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'_' {
            let digit_before = i > 0 && bytes[i - 1].is_ascii_digit();
            let digit_after = bytes.get(i + 1).is_some_and(u8::is_ascii_digit);
            if !digit_before || !digit_after {
                return None;
            }
        } else {
            out.push(b as char);
        }
    }
    Some(out)
}

/// `int(op)` of a string: blanks around it, a sign, decimal digits with
/// single underscores between them. `None` where Python raises `ValueError`.
///
/// Python integers have no upper bound; here anything outside 128 bits is
/// not a number (see docs/quirks.md).
pub fn parse_int(op: &str) -> Option<i128> {
    let text = py_strip(op);
    if !text.is_ascii() {
        return None;
    }
    let (negative, digits) = match text.as_bytes().first()? {
        b'-' => (true, &text[1..]),
        b'+' => (false, &text[1..]),
        _ => (false, text),
    };
    if digits.is_empty() || !digits.as_bytes()[0].is_ascii_digit() {
        return None;
    }
    let digits = strip_underscores(digits)?;
    if !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let value: i128 = digits.parse().ok()?;
    Some(if negative { -value } else { value })
}

/// `float(op)` of a string: blanks around it, a sign, `inf`, `infinity` or
/// `nan` in any case, or a decimal number with an optional exponent and
/// underscores between digits. `None` where Python raises `ValueError`.
pub fn parse_float(op: &str) -> Option<f64> {
    let text = py_strip(op);
    if text.is_empty() || !text.is_ascii() {
        return None;
    }
    let body = text.strip_prefix(['+', '-']).unwrap_or(text);
    let lower = body.to_ascii_lowercase();
    if matches!(lower.as_str(), "inf" | "infinity" | "nan") {
        let value = if lower == "nan" { f64::NAN } else { f64::INFINITY };
        return Some(if text.starts_with('-') { -value } else { value });
    }
    // Only digits, one dot and an exponent from here on.
    let cleaned = strip_underscores(body)?;
    let bytes = cleaned.as_bytes();
    let mut i = 0;
    let digits = |i: &mut usize| {
        let start = *i;
        while *i < bytes.len() && bytes[*i].is_ascii_digit() {
            *i += 1;
        }
        *i - start
    };
    let whole = digits(&mut i);
    let mut frac = 0;
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        frac = digits(&mut i);
    }
    if whole + frac == 0 {
        return None;
    }
    if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
        i += 1;
        if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
            i += 1;
        }
        if digits(&mut i) == 0 {
            return None;
        }
    }
    if i != bytes.len() {
        return None;
    }
    let value: f64 = cleaned.parse().ok()?;
    Some(if text.starts_with('-') { -value } else { value })
}

/// Whether the given operand contains an integer number.
pub fn is_int(op: &str) -> bool {
    parse_int(op).is_some()
}

/// `int(op)`, where the Python code has already checked `is_int(op)`.
pub fn int(op: &str) -> i128 {
    parse_int(op).unwrap_or_else(|| panic!("ValueError: invalid literal for int() with base 10: '{op}'"))
}

/// Whether the given operand contains a floating point number.
pub fn is_float(op: &str) -> bool {
    parse_float(op).is_some()
}

/// `float(op)`, where the Python code has already checked `is_float(op)`.
pub fn float(op: &str) -> f64 {
    parse_float(op).unwrap_or_else(|| panic!("ValueError: could not convert string to float: '{op}'"))
}

/// `int(x)` of a float: towards zero. Python raises on infinities and NaN.
pub fn float_to_int(x: f64) -> i128 {
    if x.is_nan() {
        panic!("ValueError: cannot convert float NaN to integer");
    }
    if x.is_infinite() {
        panic!("OverflowError: cannot convert float infinity to integer");
    }
    x.trunc() as i128
}

/// Receives two operands. If neither is an integer, returns `None`.
/// Otherwise returns `(other, n)`, where the second is always the integer
/// one (they are swapped if need be). Not for non-commutative operations.
pub fn _int_ops(op1: &str, op2: &str) -> Option<(String, i128)> {
    if let Some(n) = parse_int(op1) {
        return Some((op2.to_string(), n));
    }
    if let Some(n) = parse_int(op2) {
        return Some((op1.to_string(), n));
    }
    None
}

/// One of the two values `_f_ops` returns: an operand as it came, or the
/// number it holds.
#[derive(Clone, Debug, PartialEq)]
pub enum FOp {
    Str(String),
    Num(f64),
}

impl FOp {
    /// The operand's text; a number prints as Python's `str()` prints it.
    pub fn text(&self) -> String {
        match self {
            FOp::Str(s) => s.clone(),
            FOp::Num(x) => zxr_core::pynum::float_repr(*x),
        }
    }

    pub fn num(&self) -> Option<f64> {
        match self {
            FOp::Str(_) => None,
            FOp::Num(x) => Some(*x),
        }
    }
}

/// Receives two operands. If neither is a number, returns `None`. Otherwise
/// returns the pair with the second being the number (swapped if need be),
/// unless `swap` is false (sub and div are not commutative), in which case a
/// numeric first operand stays first.
pub fn _f_ops(op1: &str, op2: &str, swap: bool) -> Option<(FOp, FOp)> {
    if let Some(x) = parse_float(op1) {
        if swap {
            return Some((FOp::Str(op2.to_string()), FOp::Num(x)));
        }
        return Some((FOp::Num(x), FOp::Str(op2.to_string())));
    }
    if let Some(x) = parse_float(op2) {
        return Some((FOp::Str(op1.to_string()), FOp::Num(x)));
    }
    None
}

/// Whether a given type is integer.
pub fn is_int_type(stype: &str) -> bool {
    matches!(first(stype), 'u' | 'i')
}

/// `RE_HEXA`: `^[0-9A-F]+$`.
pub fn is_hexa(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit() || (b'A'..=b'F').contains(&b))
}

/// Python's `x[a:b]` over characters, with negative positions counted from
/// the end.
pub fn py_slice(text: &str, start: i64, end: i64) -> String {
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len() as i64;
    let clamp = |i: i64| if i < 0 { (i + n).max(0) } else { i.min(n) } as usize;
    let (a, b) = (clamp(start), clamp(end));
    if a >= b { String::new() } else { chars[a..b].iter().collect() }
}

/// A list of bytes/words in hexadecimal, or literals (starting with #).
/// Numeric values with more than 2 digits are a WORD (2 bytes):
/// `01` is 01h, `001` is 1, 0 (0001h). A literal starts with `#` (1 byte) or
/// `##` (2 bytes): `#label + 1` is `(label + 1) & 0xFF`.
pub fn get_bytes(elements: &[String]) -> Vec<String> {
    let mut output = Vec::new();
    for x in elements {
        if let Some(rest) = x.strip_prefix("##") {
            // 2-byte literal
            output.push(format!("({rest}) & 0xFF"));
            output.push(format!("(({rest}) >> 8) & 0xFF"));
            continue;
        }
        if let Some(rest) = x.strip_prefix('#') {
            // 1-byte literal
            output.push(format!("({rest}) & 0xFF"));
            continue;
        }
        // must be a hex number
        assert!(is_hexa(x), "expected an hex number, got \"{x}\"");
        let hex = |s: &str| i64::from_str_radix(s, 16).unwrap_or_else(|_| panic!("ValueError: invalid literal for int() with base 16: '{s}'"));
        output.push(format!("{:02X}", hex(&py_slice(x, -2, i64::MAX))));
        if x.chars().count() > 2 {
            output.push(format!("{:02X}", hex(&py_slice(x, -4, -2))));
        }
    }
    output
}

/// The number of bytes `get_bytes` gives.
pub fn get_bytes_size(elements: &[String]) -> usize {
    get_bytes(elements).len()
}

/// Reads a list operand: the Python backend receives lists as their `repr`
/// (`['00', "it's"]`, `[3, 2.5]`) and recovers them with `eval`. `None` if
/// the text is not a list of strings and numbers. A number is kept as its
/// text.
pub fn parse_list(text: &str) -> Option<Vec<String>> {
    let chars: Vec<char> = text.trim().chars().collect();
    if chars.first() != Some(&'[') || chars.last() != Some(&']') {
        return None;
    }
    let mut items = Vec::new();
    let mut i = 1;
    let end = chars.len() - 1;
    loop {
        while i < end && chars[i].is_whitespace() {
            i += 1;
        }
        if i >= end {
            break;
        }
        let quote = chars[i];
        if quote != '\'' && quote != '"' {
            // A number. The list was written with `repr`, so its text is
            // already what `str()` of the value read back would be.
            let from = i;
            while i < end && chars[i] != ',' {
                i += 1;
            }
            let item: String = chars[from..i].iter().collect();
            let item = item.trim();
            if parse_float(item).is_none() {
                return None;
            }
            items.push(item.to_string());
            if i < end {
                i += 1;
            }
            continue;
        }
        i += 1;
        let mut item = String::new();
        loop {
            if i >= end {
                return None;
            }
            let c = chars[i];
            i += 1;
            if c == quote {
                break;
            }
            if c != '\\' {
                item.push(c);
                continue;
            }
            let e = *chars.get(i)?;
            i += 1;
            let hex = |digits: usize, i: &mut usize| -> Option<char> {
                let text: String = chars.get(*i..*i + digits)?.iter().collect();
                *i += digits;
                char::from_u32(u32::from_str_radix(&text, 16).ok()?)
            };
            match e {
                'n' => item.push('\n'),
                'r' => item.push('\r'),
                't' => item.push('\t'),
                'a' => item.push('\x07'),
                'b' => item.push('\x08'),
                'f' => item.push('\x0c'),
                'v' => item.push('\x0b'),
                '0' => item.push('\0'),
                'x' => item.push(hex(2, &mut i)?),
                'u' => item.push(hex(4, &mut i)?),
                'U' => item.push(hex(8, &mut i)?),
                '\\' | '\'' | '"' => item.push(e),
                other => {
                    item.push('\\');
                    item.push(other);
                }
            }
        }
        items.push(item);
        while i < end && chars[i].is_whitespace() {
            i += 1;
        }
        if i < end {
            if chars[i] != ',' {
                return None;
            }
            i += 1;
        }
    }
    Some(items)
}

/// `eval(text)` where the Python code expects a list of strings.
pub fn eval_list(text: &str) -> Vec<String> {
    parse_list(text).unwrap_or_else(|| panic!("SyntaxError: cannot read the list {text}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn int_and_float_parse_as_python_does() {
        // Checked against python3: int(x), float(x).
        for (text, value) in [("5", Some(5)), ("+5", Some(5)), (" -5 ", Some(-5)), ("1_0", Some(10)), ("007", Some(7)), ("\t12\n", Some(12))] {
            assert_eq!(parse_int(text), value, "{text:?}");
        }
        for text in ["", " ", "1e3", "0x10", "1__0", "_1", "1_", "1.0", "+", "- 5", "inf", "5a", "#5", "*5"] {
            assert_eq!(parse_int(text), None, "{text:?}");
        }
        for (text, value) in [
            ("5", 5.0),
            ("+5", 5.0),
            (" 1e3 ", 1000.0),
            ("1_0.5", 10.5),
            (".5", 0.5),
            ("5.", 5.0),
            ("-1.5E-2", -0.015),
            ("inf", f64::INFINITY),
            ("-Infinity", f64::NEG_INFINITY),
            ("1_0e1_0", 1e11),
        ] {
            assert_eq!(parse_float(text), Some(value), "{text:?}");
        }
        assert!(parse_float("nan").unwrap().is_nan());
        for text in ["", ".", "e5", "1e", "0x10", "1__0", "1_.5", "1._5", "in", "infinit", "1 2", "--1", "+-1", "1e+", "_1", "1_"] {
            assert_eq!(parse_float(text), None, "{text:?}");
        }
    }

    #[test]
    fn powers_of_two() {
        // [n for n in range(1, 70000) if is_2n(n)] in Python is the 17 powers.
        let found: Vec<i128> = (1..70000).filter(|&n| is_2n(n)).collect();
        assert_eq!(found, (0..17).map(|k| 1i128 << k).collect::<Vec<_>>());
        assert!(!is_2n(0) && !is_2n(-4));
        // [k for k in range(127) if not is_2n(2**k)]: the logarithm is not exact.
        let inexact: Vec<u32> = (0..127).filter(|&k| !is_2n(1i128 << k)).collect();
        assert_eq!(inexact, [29, 31, 39, 47, 51, 55, 58, 59, 62, 78, 93, 94, 95, 101, 102, 109, 110, 116, 117, 118, 124, 125]);
    }

    #[test]
    fn bytes_of_hex_items() {
        let list = |items: &[&str]| items.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(get_bytes(&list(&["01", "001", "1234", "#a + 1", "##(b)", "ABCDE"])), list(&["01", "01", "00", "34", "12", "(a + 1) & 0xFF", "((b)) & 0xFF", "(((b)) >> 8) & 0xFF", "DE", "BC"]));
    }

    #[test]
    fn lists_read_back() {
        assert_eq!(parse_list("[]"), Some(vec![]));
        assert_eq!(parse_list("['00', \"it's\", 'a\\\\b', 'x\\'y']"), Some(vec!["00".to_string(), "it's".to_string(), "a\\b".to_string(), "x'y".to_string()]));
        assert_eq!(parse_list("['a' 'b']"), None);
        assert_eq!(parse_list("5"), None);
        assert_eq!(parse_list("[3, -2.5, 'x']"), Some(vec!["3".to_string(), "-2.5".to_string(), "x".to_string()]));
    }
}
