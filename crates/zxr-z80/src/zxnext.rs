//! The ZX Spectrum Next's own versions of three quads
//! (`arch/zxnext/backend`): 8-bit multiplication with the Z80N `mul d, e`,
//! and an ending sequence to match its shorter prologue.

use crate::common::{first, strs, Common, QuadExt, _int_ops, CALL_BACK, END_LABEL};
use zxr_ir::Quad;

pub struct Bits8;

impl Bits8 {
    /// Multiplies 2 las values from the stack.
    ///
    /// Optimizations:
    ///   * If any of the ops is ZERO,
    ///     then do A = 0 ==> XOR A, cause A * 0 = 0 * A = 0
    ///
    ///   * If any ot the ops is ONE, do NOTHING
    ///     A * 1 = 1 * A = A
    pub fn mul8(_c: &mut Common, ins: &Quad) -> Vec<String> {
        use crate::bits8::Bits8 as Z80Bits8;

        let (op1, op2) = (ins.at(2), ins.at(3));
        let mut output;
        if let Some((op1, op2)) = _int_ops(op1, op2) {
            output = Z80Bits8::get_oper(&op1, None, false);

            if op2 == 0 {
                output.push("xor a".to_string());
                output.push("push af".to_string());
                return output;
            }

            if op2 == 1 {
                // A * 1 = 1 * A = A
                output.push("push af".to_string());
                return output;
            }

            if op2 == 2 {
                // A * 2 == A SLA 1
                output.push("add a, a".to_string());
                output.push("push af".to_string());
                return output;
            }

            if op2 == 4 {
                // A * 4 == A SLA 2
                output.push("add a, a".to_string());
                output.push("add a, a".to_string());
                output.push("push af".to_string());
                return output;
            }

            output.push(format!("ld h, {}", Z80Bits8::int8(op2)));
        } else {
            let (mut op1, mut op2) = (op1, op2);
            if first(op2) == '_' {
                // stack optimization
                std::mem::swap(&mut op1, &mut op2);
            }

            output = Z80Bits8::get_oper(op1, Some(op2), false);
        }

        output.extend(strs(&[
            "ld d, h", // Immediate
            "ld e, a",
            "mul d, e",
            "ld a, e",
            "push af",
        ]));
        output
    }
}

/// Outputs the ending sequence
pub fn _end(c: &mut Common, ins: &Quad) -> Vec<String> {
    let mut output = crate::bits16::Bits16::get_oper(ins.at(1), None, false);
    output.push("ld b, h".to_string());
    output.push("ld c, l".to_string());

    if c.flag_end_emitted {
        output.push(format!("jp {END_LABEL}"));
        return output;
    }

    c.flag_end_emitted = true;

    output.push(format!("{END_LABEL}:"));
    if c.options.headerless {
        output.push("ret".to_string());
        return output;
    }

    output.push("di".to_string());
    output.push(format!("ld hl, ({CALL_BACK})"));
    output.extend(strs(&["ld sp, hl", "pop iy", "ei", "ret"]));
    output
}
