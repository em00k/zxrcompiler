//! `backend/_float.py`: the quads working on floating point (40 bits)
//! operands.

use crate::common::{_f_ops, first, float, float_to_int, is_float, strs, Common, QuadExt};
use crate::runtime;
use zxr_ir::Quad;

/// `int(op) & 0xFFFF` of a float. Python integers have no upper bound, and
/// a float too large for 128 bits is a multiple of 65536 anyway.
fn int_low16(op: f64) -> i128 {
    let n = float_to_int(op); // Raises on infinities and NaN, as Python does
    if op.abs() >= 1e30 {
        return 0;
    }
    n & 0xFFFF
}

/// `int(op)` of a string, of any length: whether it is negative, and its
/// digits without leading zeros. `None` where Python raises `ValueError`.
///
/// `common::parse_int` stops at 128 bits; the two places here that take an
/// integer only print it or keep its low 16 bits, which needs no such limit.
fn big_int(op: &str) -> Option<(bool, String)> {
    let text = op.trim_matches(|c: char| matches!(c, '\x1c'..='\x1f') || c.is_whitespace());
    if !text.is_ascii() {
        return None;
    }
    let (negative, digits) = match text.as_bytes().first()? {
        b'-' => (true, &text[1..]),
        b'+' => (false, &text[1..]),
        _ => (false, text),
    };
    let bytes = digits.as_bytes();
    let mut out = String::new();
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'_' {
            // Only single underscores between two digits
            let digit_before = i > 0 && bytes[i - 1].is_ascii_digit();
            let digit_after = bytes.get(i + 1).is_some_and(u8::is_ascii_digit);
            if !digit_before || !digit_after {
                return None;
            }
        } else if b.is_ascii_digit() {
            out.push(b as char);
        } else {
            return None;
        }
    }
    if out.is_empty() {
        return None;
    }
    let out = out.trim_start_matches('0');
    if out.is_empty() {
        return Some((false, "0".to_string()));
    }
    Some((negative, out.to_string()))
}

/// `int(op) & 0xFFFF` of what `big_int` read.
fn big_int_low16((negative, digits): &(bool, String)) -> u32 {
    let low = digits.bytes().fold(0u32, |r, b| (r * 10 + u32::from(b - b'0')) & 0xFFFF);
    if *negative {
        (0x10000 - low) & 0xFFFF
    } else {
        low
    }
}

pub struct Float;

impl Float {
    /// Returns a floating point operand converted to 5 byte (40 bits) unsigned int.
    /// The result is returned in a tuple (C, DE, HL) => Exp, mantissa =>High16 (Int part), Low16 (Decimal part)
    pub fn float(op: f64) -> (String, String, String) {
        crate::fp::immediate_float(op)
    }

    /// Returns the pop sequence of a float
    pub fn fpop() -> Vec<String> {
        strs(&["pop af", "pop de", "pop bc"])
    }

    /// Returns the push sequence of a float
    pub fn fpush() -> Vec<String> {
        strs(&["push bc", "push de", "push af"])
    }

    /// Returns pop sequence for floating point operands
    /// 1st operand in A DE BC, 2nd operand remains in the stack
    ///
    /// Unlike 8bit and 16bit version, this does not support
    /// operands inversion. Since many of the instructions are implemented
    /// as functions, they must support this.
    ///
    /// However, if 1st operand is a number (immediate) or indirect, the stack
    /// will be rearranged, so it contains a 48 bit pushed parameter value for the
    /// subroutine to be called.
    pub fn get_oper(c: &mut Common, op1: &str, op2: Option<&str>) -> Vec<String> {
        let mut output: Vec<String> = Vec::new();
        let mut op = op2.unwrap_or(op1);

        let indirect = first(op) == '*';
        if indirect {
            op = &op[1..];
        }

        if is_float(op) {
            let op = float(op);

            if indirect {
                let op = int_low16(op);
                output.push(format!("ld hl, ({op})"));
                output.push(c.runtime_call(runtime::ILOADF));
            } else {
                let (a, de, bc) = Self::float(op);
                output.push(format!("ld a, {a}"));
                output.push(format!("ld de, {de}"));
                output.push(format!("ld bc, {bc}"));
            }
        } else if indirect {
            if first(op) == '_' {
                output.push(format!("ld hl, ({op})"));
            } else if let Some(address) = op.strip_prefix('#') {
                // A constant address (`@a(0) + 2`); the Python code pops one
                // nobody pushed.
                output.push(format!("ld hl, {address}"));
            } else {
                output.push("pop hl".to_string());
            }

            output.push(c.runtime_call(runtime::LOADF));
        } else if first(op) == '_' {
            output.push(format!("ld a, ({op})"));
            output.push(format!("ld de, ({op} + 1)"));
            output.push(format!("ld bc, ({op} + 3)"));
        } else {
            output.extend(Self::fpop());
        }

        if let Some(op2) = op2 {
            let mut op = op1;
            if is_float(op) {
                // A float must be in the stack. Let's push it
                let (a, de, bc) = Self::float(float(op));
                output.push(format!("ld hl, {bc}"));
                output.push("push hl".to_string());
                output.push(format!("ld hl, {de}"));
                output.push("push hl".to_string());
                output.push(format!("ld h, {a}"));
                output.push("push hl".to_string());
            } else if first(op) == '*' {
                // Indirect
                op = &op[1..];
                output.push("exx".to_string()); // uses alternate set to put it on the stack
                output.push("ex af, af'".to_string());
                if let Some((negative, digits)) = big_int(op) {
                    // TODO: it will fail
                    let sign = if negative { "-" } else { "" };
                    output.push(format!("ld hl, {sign}{digits}"));
                } else if first(op) == '_' {
                    output.push(format!("ld hl, ({op})"));
                } else if let Some(address) = op.strip_prefix('#') {
                    output.push(format!("ld hl, {address}"));
                } else {
                    output.push("pop hl".to_string());
                }

                output.push(c.runtime_call(runtime::ILOADF));
                output.extend(Self::fpush());
                output.push("ex af, af'".to_string());
                output.push("exx".to_string());
            } else if first(op) == '_' {
                if is_float(op2) {
                    let tmp = output;
                    output = Vec::new();
                    output.push(format!("ld hl, {op} + 4"));
                    output.push(c.runtime_call(runtime::FP_PUSH_REV));
                    output.extend(tmp);
                } else {
                    output.push(format!("ld hl, {op} + 4"));
                    output.push(c.runtime_call(runtime::FP_PUSH_REV));
                }
            } else {
                // Else do nothing, and leave the op onto the stack
            }
        }

        output
    }

    // -----------------------------------------------------
    //               Arithmetic operations
    // -----------------------------------------------------

    pub fn float_binary(c: &mut Common, ins: &Quad, label: &str) -> Vec<String> {
        assert!(runtime::RUNTIME_LABELS.binary_search(&label).is_ok());

        let (op1, op2) = (ins.at(2), ins.at(3));
        let mut output = Self::get_oper(c, op1, Some(op2));
        output.push(c.runtime_call(label));
        output.extend(Self::fpush());
        output
    }

    /// Add 2 float values. The result is pushed onto the stack.
    pub fn addf(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (op1, op2) = (ins.at(2), ins.at(3));

        // TODO: This should be done in the optimizer
        if let Some((opa, opb)) = _f_ops(op1, op2, true) {
            if opb.num() == Some(0.0) {
                // A + 0 => A
                let mut output = Self::get_oper(c, &opa.text(), None);
                output.extend(Self::fpush());
                return output;
            }
        }

        Self::float_binary(c, ins, runtime::ADDF)
    }

    /// Subtract 2 float values. The result is pushed onto the stack.
    pub fn subf(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (op1, op2) = (ins.at(2), ins.at(3));

        // TODO: This should be done in the optimizer
        if is_float(op2) && float(op2) == 0.0 {
            // Nothing to do: A - 0 = A
            let mut output = Self::get_oper(c, op1, None);
            output.extend(Self::fpush());
            return output;
        }

        Self::float_binary(c, ins, runtime::SUBF)
    }

    /// Multiply 2 float values. The result is pushed onto the stack.
    pub fn mulf(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (op1, op2) = (ins.at(2), ins.at(3));

        // TODO: This should be done in the optimizer
        if let Some((opa, opb)) = _f_ops(op1, op2, true) {
            if opb.num() == Some(1.0) {
                // A * 1 => A
                let mut output = Self::get_oper(c, &opa.text(), None);
                output.extend(Self::fpush());
                return output;
            }
        }

        Self::float_binary(c, ins, runtime::MULF)
    }

    /// Divide 2 float values. The result is pushed onto the stack.
    pub fn divf(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (op1, op2) = (ins.at(2), ins.at(3));

        // TODO: This should be done in the optimizer
        if is_float(op2) && float(op2) == 1.0 {
            // Nothing to do. A / 1 = A
            let mut output = Self::get_oper(c, op1, None);
            output.extend(Self::fpush());
            return output;
        }

        Self::float_binary(c, ins, runtime::DIVF)
    }

    /// Reminder of div. 2 float values. The result is pushed onto the stack.
    pub fn modf(c: &mut Common, ins: &Quad) -> Vec<String> {
        Self::float_binary(c, ins, runtime::MODF)
    }

    /// Exponentiation of 2 float values. The result is pushed onto the stack.
    pub fn powf(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (op1, op2) = (ins.at(2), ins.at(3));

        // TODO: This should be done in the optimizer
        if is_float(op2) && float(op2) == 1.0 {
            // Nothing to do. A ^ 1 = A
            let mut output = Self::get_oper(c, op1, None);
            output.extend(Self::fpush());
            return output;
        }

        Self::float_binary(c, ins, runtime::POWF)
    }

    pub fn bool_binary(c: &mut Common, ins: &Quad, label: &str) -> Vec<String> {
        assert!(runtime::RUNTIME_LABELS.binary_search(&label).is_ok());
        let (op1, op2) = (ins.at(2), ins.at(3));
        let mut output = Self::get_oper(c, op1, Some(op2));
        output.push(c.runtime_call(label));
        output.push("push af".to_string());
        output
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand < 2nd operand (top of the stack).
    /// Pushes 0 if False, 1 if True.
    ///
    /// Floating Point version
    pub fn ltf(c: &mut Common, ins: &Quad) -> Vec<String> {
        Self::bool_binary(c, ins, runtime::LTF)
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand > 2nd operand (top of the stack).
    /// Pushes 0 if False, 1 if True.
    ///
    /// Floating Point version
    pub fn gtf(c: &mut Common, ins: &Quad) -> Vec<String> {
        Self::bool_binary(c, ins, runtime::GTF)
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand <= 2nd operand (top of the stack).
    /// Pushes 0 if False, 1 if True.
    ///
    /// Floating Point version
    pub fn lef(c: &mut Common, ins: &Quad) -> Vec<String> {
        Self::bool_binary(c, ins, runtime::LEF)
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand >= 2nd operand (top of the stack).
    /// Pushes 0 if False, 1 if True.
    ///
    /// Floating Point version
    pub fn gef(c: &mut Common, ins: &Quad) -> Vec<String> {
        Self::bool_binary(c, ins, runtime::GEF)
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand == 2nd operand (top of the stack).
    /// Pushes 0 if False, 1 if True.
    ///
    /// Floating Point version
    pub fn eqf(c: &mut Common, ins: &Quad) -> Vec<String> {
        Self::bool_binary(c, ins, runtime::EQF)
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand != 2nd operand (top of the stack).
    /// Pushes 0 if False, 1 if True.
    ///
    /// Floating Point version
    pub fn nef(c: &mut Common, ins: &Quad) -> Vec<String> {
        Self::bool_binary(c, ins, runtime::NEF)
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand || 2nd operand (top of the stack).
    /// Pushes 0 if False, 1 if True.
    ///
    /// Floating Point version
    pub fn orf(c: &mut Common, ins: &Quad) -> Vec<String> {
        Self::bool_binary(c, ins, runtime::ORF)
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand ~~ 2nd operand (top of the stack).
    /// Pushes 0 if False, 1 if True.
    ///
    /// Floating Point version
    pub fn xorf(c: &mut Common, ins: &Quad) -> Vec<String> {
        Self::bool_binary(c, ins, runtime::XORF)
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand && 2nd operand (top of the stack).
    /// Pushes 0 if False, 1 if True.
    ///
    /// Floating Point version
    pub fn andf(c: &mut Common, ins: &Quad) -> Vec<String> {
        Self::bool_binary(c, ins, runtime::ANDF)
    }

    /// Negates top of the stack (48 bits)
    pub fn notf(c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output = Self::get_oper(c, ins.at(2), None);
        output.push(c.runtime_call(runtime::NOTF));
        output.push("push af".to_string());
        output
    }

    /// Changes sign of top of the stack (48 bits)
    pub fn negf(c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output = Self::get_oper(c, ins.at(2), None);
        output.push(c.runtime_call(runtime::NEGF));
        output.extend(Self::fpush());
        output
    }

    /// Absolute value of top of the stack (48 bits)
    pub fn absf(c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output = Self::get_oper(c, ins.at(2), None);
        output.push("res 7, e".to_string()); // Just resets the sign bit!
        output.extend(Self::fpush());
        output
    }

    /// Loads a floating point value from a memory address.
    /// If 2nd arg. start with '*', it is always treated as
    /// an indirect value.
    pub fn loadf(c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output = Self::get_oper(c, ins.at(2), None);
        output.extend(Self::fpush());
        output
    }

    /// Stores a floating point value into a memory address.
    pub fn storef(c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output = Self::get_oper(c, ins.at(2), None);

        let mut op = ins.at(1);

        let indirect = first(op) == '*';
        if indirect {
            op = &op[1..];
        }

        let immediate = first(op) == '#'; // Might make no sense here?
        if immediate {
            op = &op[1..];
        }

        let number = big_int(op);
        // Any immediate is a constant address, `@a(0) + 2` as well as `_a`;
        // the Python code pops one nobody pushed.
        if number.is_some() || matches!(first(op), '_' | '.') || immediate {
            let op = match &number {
                Some(n) => big_int_low16(n).to_string(),
                None => op.to_string(),
            };

            if indirect {
                output.push(format!("ld hl, ({op})"));
            } else {
                output.push(format!("ld hl, {op}"));
            }
        } else {
            output.push("pop hl".to_string());
            if indirect {
                output.push(c.runtime_call(runtime::ISTOREF));
                // TODO: Check if this is ever used
                return output;
            }
        }

        output.push(c.runtime_call(runtime::STOREF));

        output
    }

    /// Jumps if top of the stack (40bit, float) is 0 to arg(1)
    pub fn jzerof(c: &mut Common, ins: &Quad) -> Vec<String> {
        let value = ins.at(1);
        if is_float(value) {
            if float(value) == 0.0 {
                return vec![format!("jp {}", ins.at(2))]; // Always true
            }
            return Vec::new();
        }

        let mut output = Self::get_oper(c, value, None);
        output.push("ld a, c".to_string());
        output.push("or l".to_string());
        output.push("or h".to_string());
        output.push("or e".to_string());
        output.push("or d".to_string());
        output.push(format!("jp z, {}", ins.at(2)));
        output
    }

    /// Jumps if top of the stack (40bit, float) is != 0 to arg(1)
    pub fn jnzerof(c: &mut Common, ins: &Quad) -> Vec<String> {
        let value = ins.at(1);
        if is_float(value) {
            if float(value) != 0.0 {
                return vec![format!("jp {}", ins.at(2))]; // Always true
            }
            return Vec::new();
        }

        let mut output = Self::get_oper(c, value, None);
        output.push("ld a, c".to_string());
        output.push("or l".to_string());
        output.push("or h".to_string());
        output.push("or e".to_string());
        output.push("or d".to_string());
        output.push(format!("jp nz, {}", ins.at(2)));
        output
    }

    /// Jumps if top of the stack (40bit, float) is >= 0 to arg(1)
    pub fn jgezerof(c: &mut Common, ins: &Quad) -> Vec<String> {
        let value = ins.at(1);
        if is_float(value) && float(value) >= 0.0 {
            return vec![format!("jp {}", ins.at(2))]; // Always true
        }

        // A negative number is loaded and tested like anything else.
        let mut output = Self::get_oper(c, value, None);
        output.push("ld a, e".to_string()); // Take sign from mantissa (bit 7)
        output.push("add a, a".to_string()); // Puts sign into carry
        output.push(format!("jp nc, {}", ins.at(2)));
        output
    }

    /// Returns from a procedure / function a Floating Point (40bits) value
    pub fn retf(c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output = Self::get_oper(c, ins.at(1), None);
        output.push("#pragma opt require a,bc,de".to_string());
        output.push(format!("jp {}", ins.at(2)));
        output
    }

    /// Pushes 40bit (float) param into the stack
    pub fn paramf(c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output = Self::get_oper(c, ins.at(1), None);
        output.extend(Self::fpush());
        output
    }

    /// Passes a floating point as a __FASTCALL__ parameter.
    /// This is done by popping out of the stack for a
    /// value, or by loading it from memory (indirect)
    /// or directly (immediate)
    pub fn fparamf(c: &mut Common, ins: &Quad) -> Vec<String> {
        Self::get_oper(c, ins.at(1), None)
    }
}
