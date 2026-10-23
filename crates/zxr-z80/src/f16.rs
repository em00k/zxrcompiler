//! `backend/_f16.py`: the quads working on fixed point (16.16) operands.

use crate::bits32::Bits32;
use crate::common::{_f_ops, first, float, float_to_int, is_float, Common, QuadExt};
use crate::float::Float;
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

// -----------------------------------------------------
// Fixed Point (16.16) bits operands
// -----------------------------------------------------
pub struct Fixed16;

impl Fixed16 {
    /// Returns a floating point operand converted to 32 bits unsigned int.
    /// Negative numbers are returned in 2 complement.
    /// The result is returned in a tuple (DE, HL) => High16 (Int part), Low16 (Decimal part)
    pub fn f16(op: f64) -> (i128, i128) {
        let mut op = op;

        let negative = op < 0.0;
        if negative {
            op = -op;
        }

        // `DE = int(op)`, then `op - DE` with DE turned back into the same
        // float: the integer part of op, exactly.
        let mut de = int_low16(op);
        let mut hl = float_to_int((op - op.trunc()) * 65536.0) & 0xFFFF;

        if negative {
            // Do C2
            de ^= 0xFFFF;
            hl ^= 0xFFFF;

            let dehl = ((de << 16) | hl) + 1;
            hl = dehl & 0xFFFF;
            de = (dehl >> 16) & 0xFFFF;
        }

        (de, hl)
    }

    /// Returns pop sequence for 32 bits operands
    /// 1st operand in HLDE, 2nd operand remains in the stack
    ///
    /// Now it does support operands inversion calling __SWAP32.
    ///
    /// However, if 1st operand is integer (immediate) or indirect, the stack
    /// will be rearranged, so it contains a 32 bit pushed parameter value for the
    /// subroutine to be called.
    ///
    /// If use_bc is True, then BC will be used instead of HL for lower part
    /// for the 1st operand.
    pub fn get_oper(c: &mut Common, op1: &str, op2: Option<&str>, use_bc: bool, reversed: bool) -> Vec<String> {
        let mut output: Vec<String> = Vec::new();

        let mut op = op2.unwrap_or(op1);

        let mut float1 = false; // whether op1 (2nd operand) is float

        let mut indirect = first(op) == '*';
        if indirect {
            op = &op[1..];
        }

        let mut immediate = first(op) == '#';
        if immediate {
            op = &op[1..];
        }

        let hl = if !use_bc { "hl" } else { "bc" };

        if is_float(op) {
            float1 = true;
            let op = float(op);

            if indirect {
                let op = int_low16(op);
                if immediate {
                    output.push(format!("ld hl, {op}"));
                } else {
                    output.push(format!("ld hl, ({op})"));
                }

                output.push(c.runtime_call(runtime::ILOAD32));
                if use_bc {
                    output.push("ld b, h".to_string());
                    output.push("ld c, l".to_string());
                }
            } else {
                let (de, hl_) = Self::f16(op);
                output.push(format!("ld de, {de}"));
                output.push(format!("ld {hl}, {hl_}"));
            }
        } else {
            if first(op) == '_' {
                if immediate {
                    output.push(format!("ld {hl}, {op}"));
                } else {
                    output.push(format!("ld {hl}, ({op})"));
                }
            } else if immediate {
                // A constant address (`@a(0) + 2`), as Bits32::get_oper has
                // it since nb12; the Python code pops one nobody pushed.
                output.push(format!("ld {hl}, ({op}) & 0xFFFF"));
            } else {
                output.push(format!("pop {hl}"));
            }

            if indirect {
                output.push(c.runtime_call(runtime::ILOAD32));
                if use_bc {
                    output.push("ld b, h".to_string());
                    output.push("ld c, l".to_string());
                }
            } else if first(op) == '_' {
                output.push(format!("ld de, ({op} + 2)"));
            } else if immediate {
                output.push(format!("ld de, ({op}) >> 16"));
            } else {
                output.push("pop de".to_string());
            }
        }

        if op2.is_some() {
            let mut op = op1;

            indirect = first(op) == '*';
            if indirect {
                op = &op[1..];
            }

            immediate = first(op) == '#';
            if immediate {
                op = &op[1..];
            }

            if is_float(op) {
                let op = float(op);

                if indirect {
                    let op = int_low16(op);
                    output.push("exx".to_string());
                    if immediate {
                        output.push(format!("ld hl, {op}"));
                    } else {
                        output.push(format!("ld hl, ({op})"));
                    }

                    output.push(c.runtime_call(runtime::ILOAD32)); // TODO: Check if this is ever used
                    output.push("push de".to_string());
                    output.push("push hl".to_string());
                    output.push("exx".to_string());
                } else {
                    let (de, hl_) = Self::f16(op);
                    output.push(format!("ld bc, {de}"));
                    output.push("push bc".to_string());
                    output.push(format!("ld bc, {hl_}"));
                    output.push("push bc".to_string());
                }
            } else if indirect {
                output.push("exx".to_string()); // uses alternate set to put it on the stack
                if first(op) == '_' {
                    if immediate {
                        output.push(format!("ld hl, {op}"));
                    } else {
                        output.push(format!("ld hl, ({op})"));
                    }
                } else if immediate {
                    // A constant address; the Python code pops one nobody
                    // pushed.
                    output.push(format!("ld hl, ({op}) & 0xFFFF"));
                } else {
                    output.push("pop hl".to_string()); // Pointers are only 16 bits ***
                }

                output.push(c.runtime_call(runtime::ILOAD32)); // TODO: Check if this is ever used
                output.push("push de".to_string());
                output.push("push hl".to_string());
                output.push("exx".to_string());
            } else if first(op) == '_' {
                // an address
                //
                // The Python code tests `op1[0]`, the operand as it came, so
                // an immediate one (`#_a`) only gets here through float1.
                if float1 || first(op1) == '_' {
                    // If previous op was constant, we can use hl in advance
                    let tmp = output;
                    output = Vec::new();
                    output.push(format!("ld hl, ({op} + 2)"));
                    output.push("push hl".to_string());
                    output.push(format!("ld hl, ({op})"));
                    output.push("push hl".to_string());
                    output.extend(tmp);
                } else {
                    output.push(format!("ld bc, ({op} + 2)"));
                    output.push("push bc".to_string());
                    output.push(format!("ld bc, ({op})"));
                    output.push("push bc".to_string());
                }
            } else {
                // 2nd operand remains in the stack
            }
        }

        if op2.is_some() && reversed {
            output.push(c.runtime_call(runtime::SWAP32));
        }

        output
    }

    /// If any of the operands within the ins(truction) are numeric,
    /// convert them to its 32bit representation, otherwise leave them
    /// as they are.
    pub fn f16_to_32bit(ins: &Quad) -> Quad {
        let mut quads = ins.clone();
        for instruction in quads.args.iter_mut() {
            if is_float(instruction) {
                let (de, hl) = Self::f16(float(instruction));
                *instruction = ((de << 16) | hl).to_string();
            }
        }

        quads
    }

    pub fn f16_binary(c: &mut Common, ins: &Quad, label: &str, reversible: bool) -> Vec<String> {
        let (op1, op2) = (ins.at(2), ins.at(3));
        let rev = reversible && !is_float(op1) && first(op1) != 't' && first(op2) == 't';
        let mut output = Self::get_oper(c, op1, Some(op2), false, rev);
        output.push(c.runtime_call(label));
        output.push("push de".to_string());
        output.push("push hl".to_string());
        output
    }

    /// Pops last 2 bytes from the stack and adds them.
    /// Then push the result onto the stack.
    ///
    /// Optimizations:
    ///   * If any of the operands is ZERO,
    ///     then do NOTHING: A + 0 = 0 + A = A
    pub fn addf16(c: &mut Common, ins: &Quad) -> Vec<String> {
        Bits32::add32(c, &Self::f16_to_32bit(ins))
    }

    /// Pops last 2 dwords from the stack and subtract them.
    /// Then push the result onto the stack.
    /// NOTE: The operation is TOP[0] = TOP[-1] - TOP[0]
    ///
    /// If TOP[0] is 0, nothing is done
    pub fn subf16(c: &mut Common, ins: &Quad) -> Vec<String> {
        Bits32::sub32(c, &Self::f16_to_32bit(ins))
    }

    /// Multiplies 2 32bit (16.16) fixed point numbers. The result is pushed onto the stack.
    pub fn mulf16(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (op1, op2) = (ins.at(2), ins.at(3));

        // TODO: move this to the optimizer
        if let Some((op1, op2)) = _f_ops(op1, op2, true) {
            let op1 = op1.text();
            let op2 = op2.num();

            if op2 == Some(1.0) {
                // A * 1 => A
                let mut output = Self::get_oper(c, &op1, None, false, false);
                output.push("push de".to_string());
                output.push("push hl".to_string());
                return output;
            }

            if op2 == Some(-1.0) {
                // The quad goes as it came, not through f16_to_32bit.
                return Bits32::neg32(c, ins);
            }

            // The Python code builds this before knowing whether it is
            // wanted: when it is not, the list is dropped, but what building
            // it did (a runtime call noted as required) stays.
            let mut output = Self::get_oper(c, &op1, None, false, false);
            if op2 == Some(0.0) {
                output.push("ld hl, 0".to_string());
                output.push("ld e, h".to_string());
                output.push("ld d, l".to_string());
                output.push("push de".to_string());
                output.push("push hl".to_string());
                return output;
            }
        }

        Self::f16_binary(c, ins, runtime::MULF16, false)
    }

    /// Divides 2 32bit (16.16) fixed point numbers. The result is pushed onto the stack.
    ///
    /// Optimizations:
    ///
    ///  * If 2nd operand is 1, do nothing
    ///  * If 2nd operand is -1, do NEG32
    pub fn divf16(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (op1, op2) = (ins.at(2), ins.at(3));

        if is_float(op2) {
            if float(op2) == 1.0 {
                // TODO move this to the optimizer
                let mut output = Self::get_oper(c, op1, None, false, false);
                output.push("push de".to_string());
                output.push("push hl".to_string());
                return output;
            }

            if float(op2) == -1.0 {
                // The Python code negates a float here, not a fixed point
                // number. Reproduced as it stands.
                return Float::negf(c, ins);
            }
        }

        Self::f16_binary(c, ins, runtime::DIVF16, true)
    }

    /// Reminder of div. 2 32bit (16.16) fixed point numbers. The result is pushed onto the stack.
    /// Optimizations:
    ///  * If 2nd op is 1. Returns 0
    pub fn modf16(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (op1, op2) = (ins.at(2), ins.at(3));

        if is_float(op2) && float(op2) == 1.0 {
            // TODO move this to the optimizer
            let mut output = Self::get_oper(c, op1, None, false, false);
            output.push("ld hl, 0".to_string());
            output.push("push hl".to_string());
            output.push("push hl".to_string());
            return output;
        }

        Self::f16_binary(c, ins, runtime::MODF16, true)
    }

    /// Negates (arithmetic) top of the stack (Fixed point in DE.HL)
    ///
    /// Fixed point signed version
    pub fn negf16(c: &mut Common, ins: &Quad) -> Vec<String> {
        Bits32::neg32(c, &Self::f16_to_32bit(ins))
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand < 2nd operand (top of the stack).
    /// Pushes 0 if False, 1 if True.
    ///
    /// Fixed point signed version
    pub fn ltf16(c: &mut Common, ins: &Quad) -> Vec<String> {
        Bits32::lti32(c, &Self::f16_to_32bit(ins))
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand > 2nd operand (top of the stack).
    /// Pushes 0 if False, 1 if True.
    ///
    /// Fixed point signed version
    pub fn gtf16(c: &mut Common, ins: &Quad) -> Vec<String> {
        Bits32::gti32(c, &Self::f16_to_32bit(ins))
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand <= 2nd operand (top of the stack).
    /// Pushes 0 if False, 1 if True.
    ///
    /// Fixed point signed version
    pub fn lef16(c: &mut Common, ins: &Quad) -> Vec<String> {
        Bits32::lei32(c, &Self::f16_to_32bit(ins))
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand >= 2nd operand (top of the stack).
    /// Pushes 0 if False, 1 if True.
    ///
    /// Fixed point signed version
    pub fn gef16(c: &mut Common, ins: &Quad) -> Vec<String> {
        Bits32::gei32(c, &Self::f16_to_32bit(ins))
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand == 2nd operand (top of the stack).
    /// Pushes 0 if False, 1 if True.
    ///
    /// Fixed point signed version
    pub fn eqf16(c: &mut Common, ins: &Quad) -> Vec<String> {
        Bits32::eq32(c, &Self::f16_to_32bit(ins))
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand != 2nd operand (top of the stack).
    /// Pushes 0 if False, 1 if True.
    ///
    /// Fixed point signed version
    pub fn nef16(c: &mut Common, ins: &Quad) -> Vec<String> {
        Bits32::ne32(c, &Self::f16_to_32bit(ins))
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand OR (Logical) 2nd operand (top of the stack).
    /// Pushes 0 if False, 1 if True.
    ///
    /// Fixed point signed version
    pub fn orf16(c: &mut Common, ins: &Quad) -> Vec<String> {
        Bits32::or32(c, &Self::f16_to_32bit(ins))
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand XOR (Logical) 2nd operand (top of the stack).
    /// Pushes 0 if False, 1 if True.
    ///
    /// Fixed point signed version
    pub fn xorf16(c: &mut Common, ins: &Quad) -> Vec<String> {
        Bits32::xor32(c, &Self::f16_to_32bit(ins))
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand AND (Logical) 2nd operand (top of the stack).
    /// Pushes 0 if False, 1 if True.
    ///
    /// Fixed point signed version
    pub fn andf16(c: &mut Common, ins: &Quad) -> Vec<String> {
        Bits32::and32(c, &Self::f16_to_32bit(ins))
    }

    /// Negates top of the stack (Fixed point in DE.HL)
    ///
    /// Fixed point signed version
    pub fn notf16(c: &mut Common, ins: &Quad) -> Vec<String> {
        Bits32::not32(c, &Self::f16_to_32bit(ins))
    }

    /// Absolute value of top of the stack (Fixed point in DE.HL)
    ///
    /// Fixed point signed version
    pub fn absf16(c: &mut Common, ins: &Quad) -> Vec<String> {
        Bits32::abs32(c, &Self::f16_to_32bit(ins))
    }

    /// Load a 32 bit (16.16) fixed point value from a memory address
    /// If 2nd arg. start with '*', it is always treated as
    /// an indirect value.
    pub fn loadf16(c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output = Self::get_oper(c, ins.at(2), None, false, false);
        output.push("push de".to_string());
        output.push("push hl".to_string());
        output
    }

    /// Stores 2º operand content into address of 1st operand.
    /// store16 a, x =>  *(&a) = x
    pub fn storef16(c: &mut Common, ins: &Quad) -> Vec<String> {
        let value = ins.at(2);
        if is_float(value) {
            let val = float(ins.at(2)); // Immediate?
            let (de, hl) = Self::f16(val);
            let mut q = ins.clone();
            q.args[1] = ((de << 16) | hl).to_string();
            return Bits32::store32(c, &q);
        }

        Bits32::store32(c, ins)
    }

    /// Jumps if top of the stack (32bit) is 0 to arg(1)
    /// (For Fixed point 16.16 bit values)
    pub fn jzerof16(c: &mut Common, ins: &Quad) -> Vec<String> {
        let value = ins.at(1);
        if is_float(value) {
            if float(value) == 0.0 {
                return vec![format!("jp {}", ins.at(2))]; // Always true
            }
            return Vec::new();
        }

        let mut output = Self::get_oper(c, value, None, false, false);
        output.push("ld a, h".to_string());
        output.push("or l".to_string());
        output.push("or e".to_string());
        output.push("or d".to_string());
        output.push(format!("jp z, {}", ins.at(2)));
        output
    }

    /// Jumps if top of the stack (32bit) is !=0 to arg(1)
    /// Fixed Point (16.16 bit) values.
    pub fn jnzerof16(c: &mut Common, ins: &Quad) -> Vec<String> {
        let value = ins.at(1);
        if is_float(value) {
            if float(value) != 0.0 {
                return vec![format!("jp {}", ins.at(2))]; // Always true
            }
            return Vec::new();
        }

        let mut output = Self::get_oper(c, value, None, false, false);
        output.push("ld a, h".to_string());
        output.push("or l".to_string());
        output.push("or e".to_string());
        output.push("or d".to_string());
        output.push(format!("jp nz, {}", ins.at(2)));
        output
    }

    /// Jumps if top of the stack (32bit, fixed point) is >= 0 to arg(1)
    pub fn jgezerof16(c: &mut Common, ins: &Quad) -> Vec<String> {
        let value = ins.at(1);
        if is_float(value) && float(value) >= 0.0 {
            return vec![format!("jp {}", ins.at(2))]; // Always true
        }

        // A negative number is loaded and tested like anything else.
        let mut output = Self::get_oper(c, value, None, false, false);
        output.push("ld a, d".to_string());
        output.push("add a, a".to_string()); // Puts sign into carry
        output.push(format!("jp nc, {}", ins.at(2)));
        output
    }

    /// Returns from a procedure / function a Fixed Point (32bits) value
    pub fn retf16(c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output = Self::get_oper(c, ins.at(1), None, false, false);
        output.push("#pragma opt require hl,de".to_string());
        output.push(format!("jp {}", ins.at(2)));
        output
    }

    /// Pushes 32bit fixed point param into the stack
    pub fn paramf16(c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output = Self::get_oper(c, ins.at(1), None, false, false);
        output.push("push de".to_string());
        output.push("push hl".to_string());
        output
    }

    /// Passes a 16.16 fixed point as a __FASTCALL__ parameter.
    /// This is done by popping out of the stack for a
    /// value, or by loading it from memory (indirect)
    /// or directly (immediate)
    pub fn fparamf16(c: &mut Common, ins: &Quad) -> Vec<String> {
        Self::get_oper(c, ins.at(1), None, false, false)
    }
}

#[cfg(test)]
mod tests {
    use super::Fixed16;

    #[test]
    fn f16_matches_python() {
        // [Fixed16.f16(x) for x in ...] in the Python compiler.
        assert_eq!(Fixed16::f16(0.0), (0, 0));
        assert_eq!(Fixed16::f16(-0.0), (0, 0));
        assert_eq!(Fixed16::f16(2.5), (2, 32768));
        assert_eq!(Fixed16::f16(-1.5), (65534, 32768));
        assert_eq!(Fixed16::f16(-1.0), (65535, 0));
        assert_eq!(Fixed16::f16(65536.5), (0, 32768));
        assert_eq!(Fixed16::f16(-32768.25), (32767, 49152));
        assert_eq!(Fixed16::f16(1e10), (58368, 0));
        assert_eq!(Fixed16::f16(1e300), (0, 0));
    }
}
