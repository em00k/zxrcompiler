//! `backend/_8bit.py`: implementation of 8bit (u8, i8) operations.

use crate::common::{first, int, is_2n, is_int, Common, QuadExt, _int_ops};
use crate::runtime;
use zxr_ir::Quad;

/// `op[1:]`: the operand without its first character.
fn rest(op: &str) -> &str {
    let mut chars = op.chars();
    chars.next();
    chars.as_str()
}

/// `output.append(text)`.
fn put(output: &mut Vec<String>, text: &str) {
    output.push(text.to_string());
}

/// Implementation of 8bit (u8, i8) operations.
pub struct Bits8;

impl Bits8 {
    /// Returns the operator converted to 8 bit unsigned integer (byte).
    /// For signed ones, it returns the 8bit C2 (Two Complement)
    pub fn int8(op: i128) -> i128 {
        op & 0xFF
    }

    /// Returns pop sequence for 8 bits operands
    /// 1st operand in H, 2nd operand in A (accumulator)
    ///
    /// For some operations (like comparisons), you can swap
    /// operands extraction by setting reversed = True
    pub fn get_oper(op1: &str, op2: Option<&str>, reversed: bool) -> Vec<String> {
        let mut output = Vec::new();

        let (op1, op2) = match op2 {
            Some(op2) if reversed => (op2, Some(op1)),
            _ => (op1, op2),
        };

        let mut op = op1;
        let indirect = first(op) == '*';
        if indirect {
            op = rest(op);
        }

        let immediate = first(op) == '#';
        if immediate {
            op = rest(op);
        }

        if is_int(op) {
            let op_ = int(op);

            if indirect {
                output.push(format!("ld a, ({op_})"));
            } else if op_ == 0 {
                put(&mut output, "xor a");
            } else {
                output.push(format!("ld a, {}", Self::int8(op_)));
            }
        } else if immediate {
            if indirect {
                output.push(format!("ld a, ({op})"));
            } else {
                output.push(format!("ld a, {op}"));
            }
        } else if first(op) == '_' {
            if indirect {
                let idx = if reversed { "bc" } else { "hl" };
                output.push(format!("ld {idx}, ({op})")); // can't use HL
                output.push(format!("ld a, ({idx})"));
            } else {
                output.push(format!("ld a, ({op})"));
            }
        } else {
            // The Python code tests `immediate` again here, where it can
            // only be false: that branch is never taken.
            if indirect {
                let idx = if reversed { "bc" } else { "hl" };
                output.push(format!("pop {idx}"));
                output.push(format!("ld a, ({idx})"));
            } else {
                put(&mut output, "pop af");
            }
        }

        let Some(op2) = op2 else {
            return output;
        };

        let mut tmp = Vec::new();
        if !reversed {
            tmp = std::mem::take(&mut output);
        }

        let mut op = op2;
        let indirect = first(op) == '*';
        if indirect {
            op = rest(op);
        }

        let immediate = first(op) == '#';
        if immediate {
            op = rest(op);
        }

        if is_int(op) {
            let op_ = int(op);

            if indirect {
                output.push(format!("ld hl, ({op_} - 1)"));
            } else {
                output.push(format!("ld h, {}", Self::int8(op_)));
            }
        } else {
            if immediate {
                if indirect {
                    output.push(format!("ld hl, {op}"));
                    put(&mut output, "ld h, (hl)");
                } else {
                    output.push(format!("ld h, {op}"));
                }
            } else if first(op) == '_' {
                if indirect {
                    output.push(format!("ld hl, ({op})"));
                    put(&mut output, "ld h, (hl)"); // TODO: Is this ever executed?
                } else {
                    output.push(format!("ld hl, ({op} - 1)"));
                }
            } else {
                put(&mut output, "pop hl");
            }

            // Yes: an indirect operand that is not a number gets a second
            // `ld h, (hl)` after the one above. Reproduced as it stands.
            if indirect {
                put(&mut output, "ld h, (hl)");
            }
        }

        if !reversed {
            output.extend(tmp);
        }

        output
    }

    /// Pops last 2 bytes from the stack and adds them.
    /// Then push the result onto the stack.
    ///
    /// Optimizations:
    ///   * If any of the operands is ZERO,
    ///     then do NOTHING: A + 0 = 0 + A = A
    ///
    ///   * If any of the operands is 1, then
    ///     INC is used
    ///
    ///   * If any of the operands is -1 (255), then
    ///     DEC is used
    pub fn add8(_c: &mut Common, ins: &Quad) -> Vec<String> {
        let (mut op1, mut op2) = (ins.at(2), ins.at(3));
        if let Some((op1, op2)) = _int_ops(op1, op2) {
            let mut output = Self::get_oper(&op1, None, false);
            if op2 == 0 {
                // Nothing to add: A + 0 = A
                put(&mut output, "push af");
                return output;
            }

            let op2 = Self::int8(op2);

            if op2 == 1 {
                // Adding 1 is just an inc
                put(&mut output, "inc a");
                put(&mut output, "push af");
                return output;
            }

            if op2 == 0xFF {
                // Adding 255 is just a dec
                put(&mut output, "dec a");
                put(&mut output, "push af");
                return output;
            }

            // The zero test above is made before the reduction to a byte:
            // adding 256 is `add a, 0`.
            output.push(format!("add a, {}", Self::int8(op2)));
            put(&mut output, "push af");
            return output;
        }

        if first(op2) == '_' {
            // stack optimization
            (op1, op2) = (op2, op1);
        }

        let mut output = Self::get_oper(op1, Some(op2), false);
        put(&mut output, "add a, h");
        put(&mut output, "push af");

        output
    }

    /// Pops last 2 bytes from the stack and subtract them.
    /// Then push the result onto the stack. Top-1 of the stack is
    /// subtracted Top
    /// _sub8 t1, a, b === t1 <-- a - b
    ///
    /// Optimizations:
    ///   * If 2nd op is ZERO,
    ///     then do NOTHING: A - 0 = A
    ///
    ///   * If 1st operand is 0, then
    ///     just do a NEG
    ///
    ///   * If any of the operands is 1, then
    ///     DEC is used
    ///
    ///   * If any of the operands is -1 (255), then
    ///     INC is used
    pub fn sub8(_c: &mut Common, ins: &Quad) -> Vec<String> {
        let (mut op1, mut op2) = (ins.at(2), ins.at(3));
        if is_int(op2) {
            // 2nd operand
            let op2 = Self::int8(int(op2));
            let mut output = Self::get_oper(op1, None, false);

            if op2 == 0 {
                put(&mut output, "push af");
                return output; // A - 0 = A
            }

            let op2 = Self::int8(op2);

            if op2 == 1 {
                // A - 1 == DEC A
                put(&mut output, "dec a");
                put(&mut output, "push af");
                return output;
            }

            if op2 == 0xFF {
                // A - (-1) == INC A
                put(&mut output, "inc a");
                put(&mut output, "push af");
                return output;
            }

            output.push(format!("sub {op2}"));
            put(&mut output, "push af");
            return output;
        }

        if is_int(op1) {
            // 1st operand is numeric?
            if Self::int8(int(op1)) == 0 {
                // 0 - A = -A ==> NEG A
                let mut output = Self::get_oper(op2, None, false);
                put(&mut output, "neg");
                put(&mut output, "push af");
                return output;
            }
        }

        // At this point, even if 1st operand is numeric, proceed
        // normally

        let rev;
        if first(op2) == '_' {
            // Optimization when 2nd operand is an id
            rev = true;
            (op1, op2) = (op2, op1);
        } else {
            rev = false;
        }

        let mut output = Self::get_oper(op1, Some(op2), rev);
        put(&mut output, "sub h");
        put(&mut output, "push af");

        output
    }

    /// Multiplies 2 las values from the stack.
    ///
    /// Optimizations:
    ///   * If any of the ops is ZERO,
    ///     then do A = 0 ==> XOR A, cause A * 0 = 0 * A = 0
    ///
    ///   * If any ot the ops is ONE, do NOTHING
    ///     A * 1 = 1 * A = A
    pub fn mul8(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (mut op1, mut op2) = (ins.at(2), ins.at(3));
        let mut output;
        if let Some((op1, op2)) = _int_ops(op1, op2) {
            output = Self::get_oper(&op1, None, false);

            if op2 == 0 {
                put(&mut output, "xor a");
                put(&mut output, "push af");
                return output;
            }

            if op2 == 1 {
                // A * 1 = 1 * A = A
                put(&mut output, "push af");
                return output;
            }

            if op2 == 2 {
                // A * 2 == A SLA 1
                put(&mut output, "add a, a");
                put(&mut output, "push af");
                return output;
            }

            if op2 == 4 {
                // A * 4 == A SLA 2
                put(&mut output, "add a, a");
                put(&mut output, "add a, a");
                put(&mut output, "push af");
                return output;
            }

            output.push(format!("ld h, {}", Self::int8(op2)));
        } else {
            if first(op2) == '_' {
                // stack optimization
                (op1, op2) = (op2, op1);
            }

            output = Self::get_oper(op1, Some(op2), false);
        }

        output.push(c.runtime_call(runtime::MUL8_FAST)); // Immediate
        put(&mut output, "push af");
        output
    }

    /// Divides 2 8bit unsigned integers. The result is pushed onto the stack.
    ///
    /// Optimizations:
    ///   * If 2nd op is 1 then
    ///     do nothing
    ///
    ///   * If 2nd op is 2 then
    ///     Shift Right Logical
    pub fn divu8(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (mut op1, mut op2) = (ins.at(2), ins.at(3));
        let mut output;
        if is_int(op2) {
            let op2 = Self::int8(int(op2));

            output = Self::get_oper(op1, None, false);
            if op2 == 1 {
                put(&mut output, "push af");
                return output;
            }

            if op2 == 2 {
                put(&mut output, "srl a");
                put(&mut output, "push af");
                return output;
            }

            output.push(format!("ld h, {}", Self::int8(op2)));
        } else {
            let rev;
            if first(op2) == '_' {
                // Optimization when 2nd operand is an id
                if is_int(op1) && int(op1) == 0 {
                    output = Vec::new(); // Optimization: Discard previous op if not from the stack
                    put(&mut output, "xor a");
                    put(&mut output, "push af");
                    return output;
                }

                rev = true;
                (op1, op2) = (op2, op1);
            } else {
                rev = false;
            }

            output = Self::get_oper(op1, Some(op2), rev);
        }

        output.push(c.runtime_call(runtime::DIVU8_FAST));
        put(&mut output, "push af");
        output
    }

    /// Divides 2 8bit signed integers. The result is pushed onto the stack.
    ///
    /// Optimizations:
    ///   * If 2nd op is 1 then
    ///     do nothing
    ///
    ///   * If 2nd op is 2 then
    ///     Shift Right Arithmetic
    pub fn divi8(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (mut op1, mut op2) = (ins.at(2), ins.at(3));
        let mut output;
        if is_int(op2) {
            let op2 = int(op2) & 0xFF;
            output = Self::get_oper(op1, None, false);

            if op2 == 1 {
                put(&mut output, "push af");
                return output;
            }

            // The operand has just been reduced to 0..255, so it is never
            // -1 and the `neg` is never emitted. Reproduced as it stands.
            if op2 == -1 {
                put(&mut output, "neg");
                put(&mut output, "push af");
                return output;
            }

            if op2 == 2 {
                put(&mut output, "sra a");
                put(&mut output, "push af");
                return output;
            }

            output.push(format!("ld h, {}", Self::int8(op2)));
        } else {
            let rev;
            if first(op2) == '_' {
                // Optimization when 2nd operand is an id
                if is_int(op1) && int(op1) == 0 {
                    output = Vec::new(); // Optimization: Discard previous op if not from the stack
                    put(&mut output, "xor a");
                    put(&mut output, "push af");
                    return output;
                }

                rev = true;
                (op1, op2) = (op2, op1);
            } else {
                rev = false;
            }

            output = Self::get_oper(op1, Some(op2), rev);
        }

        output.push(c.runtime_call(runtime::DIVI8_FAST));
        put(&mut output, "push af");
        output
    }

    /// Reminder of div. 2 8bit unsigned integers. The result is pushed onto the stack.
    ///
    /// Optimizations:
    ///   * If 2nd operands is 1 then
    ///     returns 0
    ///
    ///   * If 2nd operand = 2^n => do AND (2^n - 1)
    pub fn modu8(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (mut op1, mut op2) = (ins.at(2), ins.at(3));
        let mut output;
        if is_int(op2) {
            let op2 = Self::int8(int(op2));

            output = Self::get_oper(op1, None, false);
            if op2 == 1 {
                if first(op1) == '_' {
                    output = Vec::new(); // Optimization: Discard previous op if not from the stack
                }

                put(&mut output, "xor a");
                put(&mut output, "push af");
                return output;
            }

            if is_2n(op2) {
                output.push(format!("and {}", op2 - 1));
                put(&mut output, "push af");
                return output;
            }

            output.push(format!("ld h, {}", Self::int8(op2)));
        } else {
            let rev;
            if first(op2) == '_' {
                // Optimization when 2nd operand is an id
                if is_int(op1) && int(op1) == 0 {
                    output = Vec::new(); // Optimization: Discard previous op if not from the stack
                    put(&mut output, "xor a");
                    put(&mut output, "push af");
                    return output;
                }

                rev = true;
                (op1, op2) = (op2, op1);
            } else {
                rev = false;
            }

            output = Self::get_oper(op1, Some(op2), rev);
        }

        output.push(c.runtime_call(runtime::MODU8_FAST));
        put(&mut output, "push af");
        output
    }

    /// Reminder of div. 2 8bit unsigned integers. The result is pushed onto the stack.
    ///
    /// Optimizations:
    ///   * If 2nd operands is 1 then
    ///     returns 0
    ///
    ///   * If 2nd operand = 2^n => do AND (2^n - 1)
    pub fn modi8(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (mut op1, mut op2) = (ins.at(2), ins.at(3));
        let mut output;
        if is_int(op2) {
            let op2 = Self::int8(int(op2));

            output = Self::get_oper(op1, None, false);
            if op2 == 1 {
                if first(op1) == '_' {
                    output = Vec::new(); // Optimization: Discard previous op if not from the stack
                }

                put(&mut output, "xor a");
                put(&mut output, "push af");
                return output;
            }

            if is_2n(op2) {
                output.push(format!("and {}", op2 - 1));
                put(&mut output, "push af");
                return output;
            }

            output.push(format!("ld h, {}", Self::int8(op2)));
        } else {
            let rev;
            if first(op2) == '_' {
                // Optimization when 2nd operand is an id
                if is_int(op1) && int(op1) == 0 {
                    output = Vec::new(); // Optimization: Discard previous op if not from the stack
                    put(&mut output, "xor a");
                    put(&mut output, "push af");
                    return output;
                }

                rev = true;
                (op1, op2) = (op2, op1);
            } else {
                rev = false;
            }

            output = Self::get_oper(op1, Some(op2), rev);
        }

        output.push(c.runtime_call(runtime::MODI8_FAST));
        put(&mut output, "push af");
        output
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand < 2nd operand (top of the stack).
    /// Pushes 0 if False, 1 if True.
    ///
    /// 8 bit unsigned version
    pub fn ltu8(_c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output = Self::get_oper(ins.at(2), Some(ins.at(3)), false);
        put(&mut output, "cp h");
        put(&mut output, "sbc a, a");
        put(&mut output, "push af");

        output
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand < 2nd operand (top of the stack).
    /// Pushes 0 if False, 1 if True.
    ///
    /// 8 bit signed version
    pub fn lti8(c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output = Vec::new();
        output.extend(Self::get_oper(ins.at(2), Some(ins.at(3)), false));
        output.push(c.runtime_call(runtime::LTI8));
        put(&mut output, "push af");

        output
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand > 2nd operand (top of the stack).
    /// Pushes 0 if False, 1 if True.
    ///
    /// 8 bit unsigned version
    pub fn gtu8(_c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output = Self::get_oper(ins.at(2), Some(ins.at(3)), true);
        put(&mut output, "cp h");
        put(&mut output, "sbc a, a");
        put(&mut output, "push af");

        output
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand > 2nd operand (top of the stack).
    /// Pushes 0 if False, 1 if True.
    ///
    /// 8 bit signed version
    pub fn gti8(c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output = Self::get_oper(ins.at(2), Some(ins.at(3)), true);
        output.push(c.runtime_call(runtime::LTI8));
        put(&mut output, "push af");

        output
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand == 2nd operand (top of the stack).
    /// Pushes 0 if False, 1 if True.
    ///
    /// 8 bit un/signed version
    pub fn eq8(_c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output;
        if is_int(ins.at(3)) {
            output = Self::get_oper(ins.at(2), None, false);
            let n = Self::int8(int(ins.at(3)));
            if n != 0 {
                if n == 1 {
                    put(&mut output, "dec a");
                } else {
                    output.push(format!("sub {n}"));
                }
            }
        } else {
            output = Self::get_oper(ins.at(2), Some(ins.at(3)), false);
            put(&mut output, "sub h");
        }

        put(&mut output, "sub 1"); // Sets Carry only if 0
        put(&mut output, "sbc a, a");
        put(&mut output, "push af");

        output
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand <= 2nd operand (top of the stack).
    /// Pushes 0 if False, 1 if True.
    ///
    /// 8 bit unsigned version
    pub fn leu8(_c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output = Self::get_oper(ins.at(2), Some(ins.at(3)), true);
        put(&mut output, "sub h"); // Carry if H > A
        put(&mut output, "ccf"); // Negates => Carry if H <= A
        put(&mut output, "sbc a, a");
        put(&mut output, "push af");

        output
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand <= 2nd operand (top of the stack).
    /// Pushes 0 if False, 1 if True.
    ///
    /// 8 bit signed version
    pub fn lei8(c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output = Self::get_oper(ins.at(2), Some(ins.at(3)), false);
        output.push(c.runtime_call(runtime::LEI8));
        put(&mut output, "push af");

        output
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand >= 2nd operand (top of the stack).
    /// Pushes 0 if False, 1 if True.
    ///
    /// 8 bit unsigned version
    pub fn geu8(_c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output;
        if is_int(ins.at(3)) {
            output = Self::get_oper(ins.at(2), None, false);
            let n = Self::int8(int(ins.at(3)));
            if n != 0 {
                output.push(format!("sub {n}"));
            } else {
                put(&mut output, "cp a");
            }
        } else {
            output = Self::get_oper(ins.at(2), Some(ins.at(3)), false);
            put(&mut output, "sub h");
        }

        put(&mut output, "ccf");
        put(&mut output, "sbc a, a");
        put(&mut output, "push af");

        output
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand >= 2nd operand (top of the stack).
    /// Pushes 0 if False, 1 if True.
    ///
    /// 8 bit signed version
    pub fn gei8(c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output = Self::get_oper(ins.at(2), Some(ins.at(3)), true);
        output.push(c.runtime_call(runtime::LEI8));
        put(&mut output, "push af");

        output
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand != 2nd operand (top of the stack).
    /// Pushes 0 if False, 1 if True.
    ///
    /// 8 bit un/signed version
    pub fn ne8(_c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output;
        if is_int(ins.at(3)) {
            output = Self::get_oper(ins.at(2), None, false);
            let n = Self::int8(int(ins.at(3)));
            if n != 0 {
                if n == 1 {
                    put(&mut output, "dec a");
                } else {
                    output.push(format!("sub {}", Self::int8(int(ins.at(3)))));
                }
            }
        } else {
            output = Self::get_oper(ins.at(2), Some(ins.at(3)), false);
            put(&mut output, "sub h");
        }

        put(&mut output, "push af");

        output
    }

    /// Pops top 2 operands out of the stack, and checks
    /// if 1st operand OR (logical) 2nd operand (top of the stack),
    /// pushes 0 if False, not 0 if True.
    ///
    /// 8 bit un/signed version
    pub fn or8(_c: &mut Common, ins: &Quad) -> Vec<String> {
        let (op1, op2) = (ins.at(2), ins.at(3));
        if let Some((op1, op2)) = _int_ops(op1, op2) {
            let mut output = Self::get_oper(&op1, None, false);
            if op2 == 0 {
                // X or False = X
                put(&mut output, "push af");
                return output;
            }

            // X or True = True
            put(&mut output, "ld a, 1"); // True
            put(&mut output, "push af");
            return output;
        }

        let mut output = Self::get_oper(op1, Some(op2), false);
        put(&mut output, "or h");
        put(&mut output, "push af");

        output
    }

    /// pops top 2 operands out of the stack, and does
    /// OR (bitwise) with 1st and 2nd operand (top of the stack),
    /// pushes result.
    ///
    /// 8 bit un/signed version
    pub fn bor8(_c: &mut Common, ins: &Quad) -> Vec<String> {
        let (op1, op2) = (ins.at(2), ins.at(3));
        if let Some((op1, op2)) = _int_ops(op1, op2) {
            let mut output = Self::get_oper(&op1, None, false);
            if op2 == 0 {
                // X | 0 = X
                put(&mut output, "push af");
                return output;
            }

            if op2 == 0xFF {
                // X | 0xFF = 0xFF
                put(&mut output, "ld a, 0FFh");
                put(&mut output, "push af");
                return output;
            }

            // The operands go back to what they were (`tuple(ins[2:])`
            // again) and that output is dropped for the general case.
        }

        let mut output = Self::get_oper(op1, Some(op2), false);
        put(&mut output, "or h");
        put(&mut output, "push af");

        output
    }

    /// Pops top 2 operands out of the stack, and checks
    /// if 1st operand AND (logical) 2nd operand (top of the stack),
    /// pushes 0 if False, not 0 if True.
    ///
    /// 8 bit un/signed version
    pub fn and8(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (op1, op2) = (ins.at(2), ins.at(3));
        if let Some((op1, op2)) = _int_ops(op1, op2) {
            let mut output = Self::get_oper(&op1, None, false); // Pops the stack (if applicable)
            if op2 != 0 {
                // X and True = X
                put(&mut output, "push af");
                return output;
            }

            // False and X = False
            put(&mut output, "xor a");
            put(&mut output, "push af");
            return output;
        }

        let mut output = Self::get_oper(op1, Some(op2), false);

        let lbl = c.tmp_label();
        put(&mut output, "or a");
        output.push(format!("jr z, {lbl}"));
        put(&mut output, "ld a, h");
        output.push(format!("{lbl}:"));
        put(&mut output, "push af");

        output
    }

    /// Pops top 2 operands out of the stack, and does
    /// 1st AND (bitwise) 2nd operand (top of the stack),
    /// pushes the result.
    ///
    /// 8 bit un/signed version
    pub fn band8(_c: &mut Common, ins: &Quad) -> Vec<String> {
        let (op1, op2) = (ins.at(2), ins.at(3));
        if let Some((op1, op2)) = _int_ops(op1, op2) {
            let mut output = Self::get_oper(&op1, None, false);
            if op2 == 0xFF {
                // X & 0xFF = X
                put(&mut output, "push af");
                return output;
            }

            if op2 == 0 {
                // X and 0 = 0
                put(&mut output, "xor a");
                put(&mut output, "push af");
                return output;
            }

            // The operands go back to what they were (`tuple(ins[2:])`
            // again) and that output is dropped for the general case.
        }

        let mut output = Self::get_oper(op1, Some(op2), false);
        put(&mut output, "and h");
        put(&mut output, "push af");

        output
    }

    /// Pops top 2 operands out of the stack, and checks
    /// if 1st operand XOR (logical) 2nd operand (top of the stack),
    /// pushes 0 if False, 1 if True.
    ///
    /// 8 bit un/signed version
    pub fn xor8(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (op1, op2) = (ins.at(2), ins.at(3));
        if let Some((op1, op2)) = _int_ops(op1, op2) {
            let mut output = Self::get_oper(&op1, None, false); // True or X = not X
            if op2 == 0 {
                // False xor X = X
                put(&mut output, "push af");
                return output;
            }

            put(&mut output, "sub 1");
            put(&mut output, "sbc a, a");
            put(&mut output, "push af");
            return output;
        }

        let mut output = Self::get_oper(op1, Some(op2), false);
        output.push(c.runtime_call(runtime::XOR8));
        put(&mut output, "push af");

        output
    }

    /// Pops top 2 operands out of the stack, and does
    /// 1st operand XOR (bitwise) 2nd operand (top of the stack),
    /// pushes the result
    ///
    /// 8 bit un/signed version
    pub fn bxor8(_c: &mut Common, ins: &Quad) -> Vec<String> {
        let (op1, op2) = (ins.at(2), ins.at(3));
        if let Some((op1, op2)) = _int_ops(op1, op2) {
            let mut output = Self::get_oper(&op1, None, false);
            if op2 == 0 {
                // 0 xor X = X
                put(&mut output, "push af");
                return output;
            }

            if op2 == 0xFF {
                // X xor 0xFF = ~X
                put(&mut output, "cpl");
                put(&mut output, "push af");
                return output;
            }

            // The operands go back to what they were (`tuple(ins[2:])`
            // again) and that output is dropped for the general case.
        }

        let mut output = Self::get_oper(op1, Some(op2), false);
        put(&mut output, "xor h");
        put(&mut output, "push af");

        output
    }

    /// Negates (Logical NOT) top of the stack (8 bits in AF)
    pub fn not8(_c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output = Self::get_oper(ins.at(2), None, false);
        put(&mut output, "sub 1"); // Gives carry only if A = 0
        put(&mut output, "sbc a, a"); // Gives FF only if Carry else 0
        put(&mut output, "push af");

        output
    }

    /// Negates (BITWISE NOT) top of the stack (8 bits in AF)
    pub fn bnot8(_c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output = Self::get_oper(ins.at(2), None, false);
        put(&mut output, "cpl"); // Gives carry only if A = 0
        put(&mut output, "push af");

        output
    }

    /// Negates top of the stack (8 bits in AF)
    pub fn neg8(_c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output = Self::get_oper(ins.at(2), None, false);
        put(&mut output, "neg");
        put(&mut output, "push af");

        output
    }

    /// Absolute value of top of the stack (8 bits in AF)
    pub fn abs8(c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output = Self::get_oper(ins.at(2), None, false);
        output.push(c.runtime_call(runtime::ABS8));
        put(&mut output, "push af");
        output
    }

    /// Shift 8bit unsigned integer to the right. The result is pushed onto the stack.
    ///
    /// Optimizations:
    ///   * If 1nd or 2nd op is 0 then
    ///     do nothing
    ///
    ///   * If 2nd op is < 4 then
    ///     unroll loop
    pub fn shru8(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (op1, op2) = (ins.at(2), ins.at(3));

        if is_int(op2) {
            let op2 = Self::int8(int(op2));

            let mut output = Self::get_oper(op1, None, false);
            if op2 == 0 {
                put(&mut output, "push af");
                return output;
            }

            if op2 < 4 {
                for _ in 0..op2 {
                    put(&mut output, "srl a");
                }
                put(&mut output, "push af");
                return output;
            }

            let label = c.tmp_label();
            output.push(format!("ld b, {}", Self::int8(op2)));
            output.push(format!("{label}:"));
            put(&mut output, "srl a");
            output.push(format!("djnz {label}"));
            put(&mut output, "push af");
            return output;
        }

        if is_int(op1) && int(op1) == 0 {
            let mut output = Self::get_oper(op2, None, false);
            put(&mut output, "xor a");
            put(&mut output, "push af");
            return output;
        }

        let mut output = Self::get_oper(op1, Some(op2), true);
        let label = c.tmp_label();
        let label2 = c.tmp_label();
        put(&mut output, "or a");
        put(&mut output, "ld b, a");
        put(&mut output, "ld a, h");
        output.push(format!("jr z, {label2}"));
        output.push(format!("{label}:"));
        put(&mut output, "srl a");
        output.push(format!("djnz {label}"));
        output.push(format!("{label2}:"));
        put(&mut output, "push af");
        output
    }

    /// Shift 8bit signed integer to the right. The result is pushed onto the stack.
    ///
    /// Optimizations:
    ///   * If 1nd or 2nd op is 0 then
    ///     do nothing
    ///
    ///   * If 2nd op is < 4 then
    ///     unroll loop
    pub fn shri8(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (op1, op2) = (ins.at(2), ins.at(3));

        if is_int(op2) {
            let op2 = Self::int8(int(op2));

            let mut output = Self::get_oper(op1, None, false);
            if op2 == 0 {
                put(&mut output, "push af");
                return output;
            }

            if op2 < 4 {
                for _ in 0..op2 {
                    put(&mut output, "sra a");
                }
                put(&mut output, "push af");
                return output;
            }

            let label = c.tmp_label();
            output.push(format!("ld b, {}", Self::int8(op2)));
            output.push(format!("{label}:"));
            put(&mut output, "sra a");
            output.push(format!("djnz {label}"));
            put(&mut output, "push af");
            return output;
        }

        if is_int(op1) && int(op1) == 0 {
            let mut output = Self::get_oper(op2, None, false);
            put(&mut output, "xor a");
            put(&mut output, "push af");
            return output;
        }

        let mut output = Self::get_oper(op1, Some(op2), true);
        let label = c.tmp_label();
        let label2 = c.tmp_label();
        put(&mut output, "or a");
        put(&mut output, "ld b, a");
        put(&mut output, "ld a, h");
        output.push(format!("jr z, {label2}"));
        output.push(format!("{label}:"));
        put(&mut output, "sra a");
        output.push(format!("djnz {label}"));
        output.push(format!("{label2}:"));
        put(&mut output, "push af");
        output
    }

    /// Shift 8bit (un)signed integer to the left. The result is pushed onto the stack.
    ///
    /// Optimizations:
    ///   * If 1nd or 2nd op is 0 then
    ///     do nothing
    ///
    ///   * If 2nd op is < 4 then
    ///     unroll loop
    pub fn shl8(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (op1, op2) = (ins.at(2), ins.at(3));
        if is_int(op2) {
            let op2 = Self::int8(int(op2));

            let mut output = Self::get_oper(op1, None, false);
            if op2 == 0 {
                put(&mut output, "push af");
                return output;
            }

            // The loop is unrolled up to 5 here, not 3 as the docstring says.
            if op2 < 6 {
                for _ in 0..op2 {
                    put(&mut output, "add a, a");
                }
                put(&mut output, "push af");
                return output;
            }

            let label = c.tmp_label();
            output.push(format!("ld b, {}", Self::int8(op2)));
            output.push(format!("{label}:"));
            put(&mut output, "add a, a");
            output.push(format!("djnz {label}"));
            put(&mut output, "push af");
            return output;
        }

        if is_int(op1) && int(op1) == 0 {
            let mut output = Self::get_oper(op2, None, false);
            put(&mut output, "xor a");
            put(&mut output, "push af");
            return output;
        }

        let mut output = Self::get_oper(op1, Some(op2), true);
        let label = c.tmp_label();
        let label2 = c.tmp_label();
        put(&mut output, "or a");
        put(&mut output, "ld b, a");
        put(&mut output, "ld a, h");
        output.push(format!("jr z, {label2}"));
        output.push(format!("{label}:"));
        put(&mut output, "add a, a");
        output.push(format!("djnz {label}"));
        output.push(format!("{label2}:"));
        put(&mut output, "push af");
        output
    }

    /// Loads an 8 bit value from a memory address
    /// If 2nd arg. start with '*', it is always treated as
    /// an indirect value.
    pub fn load8(_c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output = Self::get_oper(ins.at(2), None, false);
        put(&mut output, "push af");
        output
    }

    /// Stores 2nd operand content into address of 1st operand.
    /// store8 a, x =>  a = x
    /// Use '*' for indirect store on 1st operand.
    pub fn store8(_c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output = Self::get_oper(ins.at(2), None, false);

        let mut op = ins.at(1);

        let indirect = first(op) == '*';
        if indirect {
            op = rest(op);
        }

        let immediate = first(op) == '#';
        if immediate {
            op = rest(op);
        }

        if is_int(op) || matches!(first(op), '_' | '.') {
            let op = if is_int(op) { (int(op) & 0xFFFF).to_string() } else { op.to_string() };

            if immediate {
                if indirect {
                    output.push(format!("ld ({op}), a"));
                } else {
                    // ???
                    output.push(format!("ld ({op}), a"));
                }
            } else if indirect {
                output.push(format!("ld hl, ({op})"));
                put(&mut output, "ld (hl), a");
            } else {
                output.push(format!("ld ({op}), a"));
            }
        } else {
            if immediate {
                if indirect {
                    // A label not starting with _
                    output.push(format!("ld hl, ({op})"));
                    put(&mut output, "ld (hl), a");
                } else {
                    output.push(format!("ld ({op}), a"));
                }
                return output;
            }
            put(&mut output, "pop hl");

            if indirect {
                put(&mut output, "ld e, (hl)");
                put(&mut output, "inc hl");
                put(&mut output, "ld d, (hl)");
                put(&mut output, "ld (de), a");
            } else {
                put(&mut output, "ld (hl), a");
            }
        }

        output
    }

    /// Jumps if top of the stack (8bit) is 0 to arg(1)
    pub fn jzero8(_c: &mut Common, ins: &Quad) -> Vec<String> {
        let value = ins.at(1);
        if is_int(value) {
            if int(value) == 0 {
                return vec![format!("jp {}", ins.at(2))]; // Always true
            }
            return Vec::new();
        }

        let mut output = Self::get_oper(value, None, false);
        put(&mut output, "or a");
        output.push(format!("jp z, {}", ins.at(2)));
        output
    }

    /// Jumps if top of the stack (8bit) is != 0 to arg(1)
    pub fn jnzero8(_c: &mut Common, ins: &Quad) -> Vec<String> {
        let value = ins.at(1);
        if is_int(value) {
            if int(value) != 0 {
                return vec![format!("jp {}", ins.at(2))]; // Always true
            }
            return Vec::new();
        }

        let mut output = Self::get_oper(value, None, false);
        put(&mut output, "or a");
        output.push(format!("jp nz, {}", ins.at(2)));
        output
    }

    /// Jumps if top of the stack (8bit) is >= 0 to arg(1)
    /// Always TRUE for unsigned
    pub fn jgezerou8(_c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output = Vec::new();
        let value = ins.at(1);
        if !is_int(value) {
            output = Self::get_oper(value, None, false);
        }

        output.push(format!("jp {}", ins.at(2)));
        output
    }

    /// Jumps if top of the stack (8bit) is >= 0 to arg(1)
    pub fn jgezeroi8(_c: &mut Common, ins: &Quad) -> Vec<String> {
        let value = ins.at(1);
        if is_int(value) {
            if int(value) >= 0 {
                return vec![format!("jp {}", ins.at(2))]; // Always true
            }
            return Vec::new();
        }

        let mut output = Self::get_oper(value, None, false);
        put(&mut output, "add a, a"); // Puts sign into carry
        output.push(format!("jp nc, {}", ins.at(2)));
        output
    }

    /// Returns from a procedure / function an 8bits value
    pub fn ret8(_c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output = Self::get_oper(ins.at(1), None, false);
        put(&mut output, "#pragma opt require a");
        output.push(format!("jp {}", ins.at(2)));
        output
    }

    /// Pushes 8bit param into the stack
    pub fn param8(_c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output = Self::get_oper(ins.at(1), None, false);
        put(&mut output, "push af");
        output
    }

    /// Passes a byte as a __FASTCALL__ parameter.
    /// This is done by popping out of the stack for a
    /// value, or by loading it from memory (indirect)
    /// or directly (immediate)
    pub fn fparam8(_c: &mut Common, ins: &Quad) -> Vec<String> {
        Self::get_oper(ins.at(1), None, false)
    }
}
