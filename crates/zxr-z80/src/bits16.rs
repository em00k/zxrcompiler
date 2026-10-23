//! `backend/_16bit.py`: implementation of 16bit operations.

use crate::bits8::Bits8;
use crate::common::{first, float_to_int, int, is_2n, is_int, log2, parse_int, Common, QuadExt, _int_ops};
use crate::runtime;
use zxr_ir::Quad;

/// `[line] * n`: a negative count gives the empty list.
fn repeat(line: &str, n: i128) -> Vec<String> {
    (0..n.max(0)).map(|_| line.to_string()).collect()
}

/// Implementation of 16bit operations.
pub struct Bits16;

impl Bits16 {
    /// Returns a 16 bit operand converted to 16 bits unsigned int.
    /// Negative numbers are returned in 2 complement.
    pub fn int16(op: i128) -> i128 {
        op & 0xFFFF
    }

    /// Returns pop sequence for 16 bits operands
    /// 1st operand in HL, 2nd operand in DE
    ///
    /// For subtraction, division, etc. you can swap operators extraction
    /// order by setting reversed to True
    ///
    /// The Python method calls `runtime_call(RuntimeLabel.LOAD_DE_DE)` when
    /// the 2nd operand is an indirect one that is neither a number nor an
    /// immediate, which notes `lddede.asm` as required. This signature has
    /// no [`Common`] to note it in, so here the call is emitted and nothing
    /// is noted: with two operands, use [`Bits16::get_oper_c`], which does
    /// both. With one operand the two are the same.
    pub fn get_oper(op1: &str, op2: Option<&str>, reversed: bool) -> Vec<String> {
        Self::oper(op1, op2, reversed, |label| format!("call {label}"))
    }

    /// [`Bits16::get_oper`] with the side effect of the Python method: the
    /// runtime routine it may call is noted as required.
    pub fn get_oper_c(c: &mut Common, op1: &str, op2: Option<&str>, reversed: bool) -> Vec<String> {
        Self::oper(op1, op2, reversed, |label| c.runtime_call(label))
    }

    fn oper(op1: &str, op2: Option<&str>, reversed: bool, mut runtime_call: impl FnMut(&str) -> String) -> Vec<String> {
        let mut output = Vec::new();

        let (op1, op2) = match op2 {
            Some(second) if reversed => (second, Some(op1)),
            _ => (op1, op2),
        };

        let mut op = op1;
        let indirect = first(op) == '*';
        if indirect {
            op = &op[1..];
        }

        let immediate = first(op) == '#';
        if immediate {
            op = &op[1..];
        }

        if is_int(op) {
            let op = int(op);

            if indirect {
                output.push(format!("ld hl, ({op})"));
            } else {
                output.push(format!("ld hl, {}", Self::int16(op)));
            }
        } else if immediate {
            if indirect {
                output.push(format!("ld hl, ({op})"));
            } else {
                output.push(format!("ld hl, {op}"));
            }
        } else {
            if first(op) == '_' {
                output.push(format!("ld hl, ({op})"));
            } else {
                output.push("pop hl".to_string());
            }

            if indirect {
                output.push("ld a, (hl)".to_string());
                output.push("inc hl".to_string());
                output.push("ld h, (hl)".to_string());
                output.push("ld l, a".to_string());
            }
        }

        let Some(op2) = op2 else {
            return output;
        };

        let mut tmp = Vec::new();
        if !reversed {
            tmp = output;
            output = Vec::new();
        }

        let mut op = op2;
        let indirect = first(op) == '*';
        if indirect {
            op = &op[1..];
        }

        let immediate = first(op) == '#';
        if immediate {
            op = &op[1..];
        }

        if is_int(op) {
            let op = int(op);

            if indirect {
                output.push(format!("ld de, ({op})"));
            } else {
                output.push(format!("ld de, {}", Self::int16(op)));
            }
        } else if immediate {
            // An indirect immediate (`*#x`) loads the address all the same:
            // the Python code does not tell them apart for the 2nd operand.
            output.push(format!("ld de, {op}"));
        } else {
            if first(op) == '_' {
                output.push(format!("ld de, ({op})"));
            } else {
                output.push("pop de".to_string());
            }

            if indirect {
                output.push(runtime_call(runtime::LOAD_DE_DE)); // DE = (DE)
            }
        }

        if !reversed {
            output.extend(tmp);
        }

        output
    }

    // -----------------------------------------------------
    //               Arithmetic operations
    // -----------------------------------------------------

    /// Pops last 2 bytes from the stack and adds them.
    /// Then push the result onto the stack.
    ///
    ///
    /// Optimizations:
    ///   * If any of the operands is ZERO,
    ///     then do NOTHING: A + 0 = 0 + A = A
    ///
    ///   * If any of the operands is < 4, then
    ///     INC is used
    ///
    ///   * If any of the operands is > (65531) (-4), then
    ///     DEC is used
    pub fn add16(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (mut op1, mut op2) = (ins.at(2), ins.at(3));
        if let Some((op1, op2)) = _int_ops(op1, op2) {
            let op2 = Self::int16(op2);
            let mut output = Self::get_oper(&op1, None, false);

            if op2 == 0 {
                output.push("push hl".to_string());
                return output; // ADD HL, 0 => NOTHING
            }

            if op2 < 4 {
                output.extend(repeat("inc hl", op2)); // ADD HL, 2 ==> inc hl; inc hl
                output.push("push hl".to_string());
                return output;
            }

            if op2 > 65531 {
                // (between -4 and 0)
                output.extend(repeat("dec hl", 0x10000 - op2));
                output.push("push hl".to_string());
                return output;
            }

            output.push(format!("ld de, {op2}"));
            output.push("add hl, de".to_string());
            output.push("push hl".to_string());
            return output;
        }

        if first(op2) == '_' {
            // stack optimization
            std::mem::swap(&mut op1, &mut op2);
        }

        let mut output = Self::get_oper_c(c, op1, Some(op2), false);
        output.push("add hl, de".to_string());
        output.push("push hl".to_string());
        output
    }

    /// Pops last 2 words from the stack and subtract them.
    /// Then push the result onto the stack. Top of the stack is
    /// subtracted Top -1
    ///
    /// Optimizations:
    ///   * If 2nd op is ZERO,
    ///     then do NOTHING: A - 0 = A
    ///
    ///   * If any of the operands is < 4, then
    ///     DEC is used
    ///
    ///   * If any of the operands is > 65531 (-4..-1), then
    ///     INC is used
    pub fn sub16(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (mut op1, mut op2) = (ins.at(2), ins.at(3));

        if is_int(op2) {
            let op = Self::int16(int(op2));
            let mut output = Self::get_oper(op1, None, false);

            if op == 0 {
                output.push("push hl".to_string());
                return output;
            }

            if op < 4 {
                output.extend(repeat("dec hl", op));
                output.push("push hl".to_string());
                return output;
            }

            if op > 65531 {
                output.extend(repeat("inc hl", 0x10000 - op));
                output.push("push hl".to_string());
                return output;
            }

            output.push(format!("ld de, -{op}"));
            output.push("add hl, de".to_string());
            output.push("push hl".to_string());
            return output;
        }

        let rev;
        if first(op2) == '_' {
            // Optimization when 2nd operand is an id
            rev = true;
            std::mem::swap(&mut op1, &mut op2);
        } else {
            rev = false;
        }

        let mut output = Self::get_oper_c(c, op1, Some(op2), rev);
        output.push("or a".to_string());
        output.push("sbc hl, de".to_string());
        output.push("push hl".to_string());
        output
    }

    /// Multiplies tow last 16bit values on top of the stack and
    /// and returns the value on top of the stack
    ///
    /// Optimizations:
    ///   * If any of the ops is ZERO,
    ///     then do A = 0 ==> XOR A, cause A * 0 = 0 * A = 0
    ///
    ///   * If any ot the ops is ONE, do NOTHING
    ///     A * 1 = 1 * A = A
    ///
    ///   * If B is 2^n and B < 16 => Shift Right n
    pub fn mul16(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (mut op1, mut op2) = (ins.at(2), ins.at(3));
        let mut output;
        // If any of the operands is constant
        if let Some((op1, op2)) = _int_ops(op1, op2) {
            // put the constant one the 2nd
            output = Self::get_oper(&op1, None, false);

            // The constant is compared and printed as it came, not brought
            // to 16 bits: -1 is not 0xFFFF here, and `ld de, -1` is emitted.
            if op2 == 0 {
                // A * 0 = 0 * A = 0
                if matches!(first(&op1), '_' | '$') {
                    output = Vec::new(); // Optimization: Discard previous op if not from the stack
                }
                output.push("ld hl, 0".to_string());
                output.push("push hl".to_string());
                return output;
            }

            if op2 == 1 {
                // A * 1 = 1 * A == A => Do nothing
                output.push("push hl".to_string());
                return output;
            }

            if op2 == 0xFFFF {
                // This is the same as (-1)
                output.push(c.runtime_call(runtime::NEGHL));
                output.push("push hl".to_string());
                return output;
            }

            if is_2n(op2) && log2(op2 as f64) < 4.0 {
                output.extend(repeat("add hl, hl", float_to_int(log2(op2 as f64))));
                output.push("push hl".to_string());
                return output;
            }

            output.push(format!("ld de, {op2}"));
        } else {
            if first(op2) == '_' {
                // stack optimization
                std::mem::swap(&mut op1, &mut op2);
            }

            output = Self::get_oper_c(c, op1, Some(op2), false);
        }

        output.push(c.runtime_call(runtime::MUL16_FAST)); // Immediate
        output.push("push hl".to_string());
        output
    }

    /// Divides 2 16bit unsigned integers. The result is pushed onto the stack.
    ///
    /// Optimizations:
    ///   * If 2nd op is 1 then
    ///     do nothing
    ///
    ///   * If 2nd op is 2 then
    ///     Shift Right Logical
    pub fn divu16(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (mut op1, mut op2) = (ins.at(2), ins.at(3));
        if is_int(op1) && int(op1) == 0 {
            // 0 / A = 0
            let mut output = if matches!(first(op2), '_' | '$') {
                Vec::new() // Optimization: Discard previous op if not from the stack
            } else {
                Self::get_oper(op2, None, false) // Normalize stack
            };

            output.push("ld hl, 0".to_string());
            output.push("push hl".to_string());
            return output;
        }

        let mut output;
        if is_int(op2) {
            let op = Self::int16(int(op2));
            output = Self::get_oper(op1, None, false);

            // The Python code tests `op2 == 0` here, with op2 still the
            // operand's text: never true, so a division by the constant 0
            // goes on to the runtime routine like any other.

            if op == 1 {
                output.push("push hl".to_string());
                return output;
            }

            if op == 2 {
                output.push("srl h".to_string());
                output.push("rr l".to_string());
                output.push("push hl".to_string());
                return output;
            }

            output.push(format!("ld de, {op}"));
        } else {
            let rev;
            if first(op2) == '_' {
                // Optimization when 2nd operand is an id
                rev = true;
                std::mem::swap(&mut op1, &mut op2);
            } else {
                rev = false;
            }
            output = Self::get_oper_c(c, op1, Some(op2), rev);
        }

        output.push(c.runtime_call(runtime::DIVU16));
        output.push("push hl".to_string());
        output
    }

    /// Divides 2 16bit signed integers. The result is pushed onto the stack.
    ///
    /// Optimizations:
    ///   * If 2nd op is 1 then
    ///     do nothing
    ///
    ///   * If 2nd op is -1 then
    ///     do NEG
    ///
    ///   * If 2nd op is 2 then
    ///     Shift Right Arithmetic
    pub fn divi16(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (mut op1, mut op2) = (ins.at(2), ins.at(3));
        if is_int(op1) && int(op1) == 0 {
            // 0 / A = 0
            let mut output = if matches!(first(op2), '_' | '$') {
                Vec::new() // Optimization: Discard previous op if not from the stack
            } else {
                Self::get_oper(op2, None, false) // Normalize stack
            };

            output.push("ld hl, 0".to_string());
            output.push("push hl".to_string());
            return output;
        }

        let mut output;
        if is_int(op2) {
            let op = Self::int16(int(op2));
            output = Self::get_oper(op1, None, false);

            if op == 1 {
                output.push("push hl".to_string());
                return output;
            }

            // Never true: op has been brought to 0..65535, so a division by
            // -1 is one by 65535 for the runtime routine. As in the Python.
            if op == -1 {
                output.push(c.runtime_call(runtime::NEGHL)); // TODO: Is this ever used?
                output.push("push hl".to_string());
                return output;
            }

            if op == 2 {
                output.push("sra h".to_string());
                output.push("rr l".to_string());
                output.push("push hl".to_string());
                return output;
            }

            output.push(format!("ld de, {op}"));
        } else {
            let rev;
            if first(op2) == '_' {
                // Optimization when 2nd operand is an id
                rev = true;
                std::mem::swap(&mut op1, &mut op2);
            } else {
                rev = false;
            }
            output = Self::get_oper_c(c, op1, Some(op2), rev);
        }

        output.push(c.runtime_call(runtime::DIVI16));
        output.push("push hl".to_string());
        output
    }

    /// Reminder of div. 2 16bit unsigned integers. The result is pushed onto the stack.
    ///
    /// Optimizations:
    ///  * If 2nd operand is 1 => Return 0
    ///  * If 2nd operand = 2^n => do AND (2^n - 1)
    pub fn modu16(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (op1, op2) = (ins.at(2), ins.at(3));

        let mut output;
        if is_int(op2) {
            let op2 = Self::int16(int(op2));
            output = Self::get_oper(op1, None, false);

            if op2 == 1 {
                // The Python code looks at `op2[0]` here, and op2 is by now
                // a number: a TypeError. (modi16 looks at op1.)
                panic!("TypeError: 'int' object is not subscriptable");
            }

            if is_2n(op2) {
                let k = op2 - 1;
                if op2 > 255 {
                    // only affects H
                    output.push("ld a, h".to_string());
                    output.push(format!("and {}", k >> 8));
                    output.push("ld h, a".to_string());
                } else {
                    output.push("ld h, 0".to_string()); // High part goes 0
                    output.push("ld a, l".to_string());
                    // `% 0xFF`, not `& 0xFF`: the same for the k there are.
                    output.push(format!("and {}", k.rem_euclid(0xFF)));
                    output.push("ld l, a".to_string());
                }

                output.push("push hl".to_string());
                return output;
            }

            output.push(format!("ld de, {op2}"));
        } else {
            output = Self::get_oper_c(c, op1, Some(op2), false);
        }

        output.push(c.runtime_call(runtime::MODU16));
        output.push("push hl".to_string());
        output
    }

    /// Reminder of div 2 16bit signed integers. The result is pushed onto the stack.
    ///
    /// Optimizations:
    ///  * If 2nd operand is 1 => Return 0
    ///  * If 2nd operand = 2^n => do AND (2^n - 1)
    pub fn modi16(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (op1, op2) = (ins.at(2), ins.at(3));

        let mut output;
        if is_int(op2) {
            let op2 = Self::int16(int(op2));
            output = Self::get_oper(op1, None, false);

            if op2 == 1 {
                // The whole operand is compared, not its first character:
                // true only for an operand that is just "_" or "$".
                if op1 == "_" || op1 == "$" {
                    output = Vec::new(); // Optimization: Discard previous op if not from the stack
                }

                output.push("ld hl, 0".to_string());
                output.push("push hl".to_string());
                return output;
            }

            if is_2n(op2) {
                let k = op2 - 1;
                if op2 > 255 {
                    // only affects H
                    output.push("ld a, h".to_string());
                    output.push(format!("and {}", k >> 8));
                    output.push("ld h, a".to_string());
                } else {
                    output.push("ld h, 0".to_string()); // High part goes 0
                    output.push("ld a, l".to_string());
                    // `% 0xFF`, not `& 0xFF`: the same for the k there are.
                    output.push(format!("and {}", k.rem_euclid(0xFF)));
                    output.push("ld l, a".to_string());
                }

                output.push("push hl".to_string());
                return output;
            }

            output.push(format!("ld de, {op2}"));
        } else {
            output = Self::get_oper_c(c, op1, Some(op2), false);
        }

        output.push(c.runtime_call(runtime::MODI16));
        output.push("push hl".to_string());
        output
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand < 2nd operand (top of the stack).
    /// Pushes 0 if False, 1 if True.
    ///
    /// 16 bit unsigned version
    pub fn ltu16(c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output = Self::get_oper_c(c, ins.at(2), Some(ins.at(3)), false);
        output.push("or a".to_string());
        output.push("sbc hl, de".to_string());
        output.push("sbc a, a".to_string());
        output.push("push af".to_string());
        output
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand < 2nd operand (top of the stack).
    /// Pushes 0 if False, 1 if True.
    ///
    /// 16 bit signed version
    pub fn lti16(c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output = Self::get_oper_c(c, ins.at(2), Some(ins.at(3)), false);
        output.push(c.runtime_call(runtime::LTI16));
        output.push("push af".to_string());
        output
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand > 2nd operand (top of the stack).
    /// Pushes 0 if False, 1 if True.
    ///
    /// 16 bit unsigned version
    pub fn gtu16(c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output = Self::get_oper_c(c, ins.at(2), Some(ins.at(3)), true);
        output.push("or a".to_string());
        output.push("sbc hl, de".to_string());
        output.push("sbc a, a".to_string());
        output.push("push af".to_string());
        output
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand > 2nd operand (top of the stack).
    /// Pushes 0 if False, 1 if True.
    ///
    /// 16 bit signed version
    pub fn gti16(c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output = Self::get_oper_c(c, ins.at(2), Some(ins.at(3)), true);
        output.push(c.runtime_call(runtime::LTI16));
        output.push("push af".to_string());
        output
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand <= 2nd operand (top of the stack).
    /// Pushes 0 if False, 1 if True.
    ///
    /// 16 bit unsigned version
    pub fn leu16(c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output = Self::get_oper_c(c, ins.at(2), Some(ins.at(3)), true);
        output.push("or a".to_string());
        output.push("sbc hl, de".to_string()); // Carry if A > B
        output.push("ccf".to_string()); // Negates the result => Carry if A <= B
        output.push("sbc a, a".to_string());
        output.push("push af".to_string());
        output
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand <= 2nd operand (top of the stack).
    /// Pushes 0 if False, 1 if True.
    ///
    /// 16 bit signed version
    pub fn lei16(c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output = Self::get_oper_c(c, ins.at(2), Some(ins.at(3)), false);
        output.push(c.runtime_call(runtime::LEI16));
        output.push("push af".to_string());
        output
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand >= 2nd operand (top of the stack).
    /// Pushes 0 if False, 1 if True.
    ///
    /// 16 bit unsigned version
    pub fn geu16(c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output = Self::get_oper_c(c, ins.at(2), Some(ins.at(3)), false);
        output.push("or a".to_string());
        output.push("sbc hl, de".to_string());
        output.push("ccf".to_string());
        output.push("sbc a, a".to_string());
        output.push("push af".to_string());
        output
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand >= 2nd operand (top of the stack).
    /// Pushes 0 if False, 1 if True.
    ///
    /// 16 bit signed version
    pub fn gei16(c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output = Self::get_oper_c(c, ins.at(2), Some(ins.at(3)), true);
        output.push(c.runtime_call(runtime::LEI16));
        output.push("push af".to_string());
        output
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand == 2nd operand (top of the stack).
    /// Pushes 0 if False, 1 if True.
    ///
    /// 16 bit un/signed version
    pub fn eq16(c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output = Self::get_oper_c(c, ins.at(2), Some(ins.at(3)), false);
        output.push(c.runtime_call(runtime::EQ16));
        output.push("push af".to_string());

        output
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand != 2nd operand (top of the stack).
    /// Pushes 0 if False, 1 if True.
    ///
    /// 16 bit un/signed version
    pub fn ne16(c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output = Self::get_oper_c(c, ins.at(2), Some(ins.at(3)), false);
        output.push("or a".to_string()); // Resets carry flag
        output.push("sbc hl, de".to_string());
        output.push("ld a, h".to_string());
        output.push("or l".to_string());
        output.push("push af".to_string());

        output
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand OR (logical) 2nd operand (top of the stack),
    /// pushes 0 if False, 1 if True.
    ///
    /// 16 bit un/signed version
    ///
    /// Optimizations:
    ///
    /// If any of the operators are constants: Returns either 0 or
    /// the other operand
    pub fn or16(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (op1, op2) = (ins.at(2), ins.at(3));

        if let Some((op1, op2)) = _int_ops(op1, op2) {
            if op2 == 0 {
                let mut output = Self::get_oper(&op1, None, false);
                output.push("ld a, h".to_string());
                output.push("or l".to_string()); // Convert x to Boolean
                output.push("push af".to_string());
                return output; // X or False = X
            }

            let mut output = Self::get_oper(&op1, None, false);
            output.push("ld a, 0FFh".to_string()); // X or True = True
            output.push("push af".to_string());
            return output;
        }

        let mut output = Self::get_oper_c(c, ins.at(2), Some(ins.at(3)), false);
        output.push("ld a, h".to_string());
        output.push("or l".to_string());
        output.push("or d".to_string());
        output.push("or e".to_string());
        output.push("push af".to_string());
        output
    }

    /// Pops top 2 operands out of the stack, and performs
    /// 1st operand OR (bitwise) 2nd operand (top of the stack),
    /// pushes result (16 bit in HL).
    ///
    /// 16 bit un/signed version
    ///
    /// Optimizations:
    ///
    /// If any of the operators are constants: Returns either 0 or
    /// the other operand
    pub fn bor16(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (mut op1, mut op2) = (ins.at(2).to_string(), ins.at(3).to_string());

        if let Some((other, n)) = _int_ops(&op1, &op2) {
            let mut output = Self::get_oper(&other, None, false);
            if n == 0 {
                // X | 0 = X
                output.push("push hl".to_string());
                return output;
            }

            if n == 0xFFFF {
                // X & 0xFFFF = 0xFFFF
                output.push("ld hl, 0FFFFh".to_string());
                output.push("push hl".to_string());
                return output;
            }

            // The operands stay as `_int_ops` left them: the constant goes
            // 2nd, as the number it is (`+5` is 5 from here on).
            (op1, op2) = (other, n.to_string());
        }

        let mut output = Self::get_oper_c(c, &op1, Some(&op2), false);
        output.push(c.runtime_call(runtime::BOR16));
        output.push("push hl".to_string());
        output
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand XOR (logical) 2nd operand (top of the stack),
    /// pushes 0 if False, 1 if True.
    ///
    /// 16 bit un/signed version
    ///
    /// Optimizations:
    ///
    /// If any of the operators are constants: Returns either 0 or
    /// the other operand
    pub fn xor16(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (op1, op2) = (ins.at(2), ins.at(3));

        if let Some((op1, op2)) = _int_ops(op1, op2) {
            let mut output = Self::get_oper(&op1, None, false);

            if op2 == 0 {
                // X xor False = X
                output.push("ld a, h".to_string());
                output.push("or l".to_string());
                output.push("push af".to_string());
                return output;
            }

            // X xor True = NOT X
            output.push("ld a, h".to_string());
            output.push("or l".to_string());
            output.push("sub 1".to_string());
            output.push("sbc a, a".to_string());
            output.push("push af".to_string());
            return output;
        }

        let mut output = Self::get_oper_c(c, ins.at(2), Some(ins.at(3)), false);
        output.push(c.runtime_call(runtime::XOR16));
        output.push("push af".to_string());
        output
    }

    /// Pops top 2 operands out of the stack, and performs
    /// 1st operand XOR (bitwise) 2nd operand (top of the stack),
    /// pushes result (16 bit in HL).
    ///
    /// 16 bit un/signed version
    ///
    /// Optimizations:
    ///
    /// If any of the operators are constants: Returns either 0 or
    /// the other operand
    pub fn bxor16(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (mut op1, mut op2) = (ins.at(2).to_string(), ins.at(3).to_string());

        if let Some((other, n)) = _int_ops(&op1, &op2) {
            let mut output = Self::get_oper(&other, None, false);
            if n == 0 {
                // X ^ 0 = X
                output.push("push hl".to_string());
                return output;
            }

            // The operands stay as `_int_ops` left them: the constant goes
            // 2nd, as the number it is.
            (op1, op2) = (other, n.to_string());
        }

        let mut output = Self::get_oper_c(c, &op1, Some(&op2), false);
        output.push(c.runtime_call(runtime::BXOR16));
        output.push("push hl".to_string());
        output
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand AND (logical) 2nd operand (top of the stack),
    /// pushes 0 if False, 1 if True.
    ///
    /// 16 bit un/signed version
    ///
    /// Optimizations:
    ///
    /// If any of the operators are constants: Returns either 0 or
    /// the other operand
    pub fn and16(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (op1, op2) = (ins.at(2), ins.at(3));

        if let Some((op1, op2)) = _int_ops(op1, op2) {
            let mut output = Self::get_oper(&op1, None, false);
            if op2 != 0 {
                output.push("ld a, h".to_string());
                output.push("or l".to_string());
                output.push("push af".to_string());
                return output; // X and True = X
            }

            let mut output = Self::get_oper(&op1, None, false);
            output.push("xor a".to_string()); // X and False = False
            output.push("push af".to_string());
            return output;
        }

        let mut output = Self::get_oper_c(c, op1, Some(op2), false);
        output.push(c.runtime_call(runtime::AND16));
        output.push("push af".to_string());
        output
    }

    /// Pops top 2 operands out of the stack, and performs
    /// 1st operand AND (bitwise) 2nd operand (top of the stack),
    /// pushes result (16 bit in HL).
    ///
    /// 16 bit un/signed version
    ///
    /// Optimizations:
    ///
    /// If any of the operators are constants: Returns either 0 or
    /// the other operand
    pub fn band16(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (mut op1, mut op2) = (ins.at(2).to_string(), ins.at(3).to_string());

        if let Some((other, n)) = _int_ops(&op1, &op2) {
            if n == 0xFFFF {
                // X & 0xFFFF = X
                // Nothing at all is emitted, not even for an operand that
                // has to be loaded and pushed. As in the Python.
                return Vec::new();
            }

            if n == 0 {
                // X & 0 = 0
                let mut output = Self::get_oper(&other, None, false);
                output.push("ld hl, 0".to_string());
                output.push("push hl".to_string());
                return output;
            }

            // The operands stay as `_int_ops` left them: the constant goes
            // 2nd, as the number it is.
            (op1, op2) = (other, n.to_string());
        }

        let mut output = Self::get_oper_c(c, &op1, Some(&op2), false);
        output.push(c.runtime_call(runtime::BAND16));
        output.push("push hl".to_string());
        output
    }

    /// Negates top (Logical NOT) of the stack (16 bits in HL)
    pub fn not16(_c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output = Self::get_oper(ins.at(2), None, false);
        output.push("ld a, h".to_string());
        output.push("or l".to_string());
        output.push("sub 1".to_string());
        output.push("sbc a, a".to_string());
        output.push("push af".to_string());
        output
    }

    /// Negates top (Bitwise NOT) of the stack (16 bits in HL)
    pub fn bnot16(c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output = Self::get_oper(ins.at(2), None, false);
        output.push(c.runtime_call(runtime::BNOT16));
        output.push("push hl".to_string());
        output
    }

    /// Negates top of the stack (16 bits in HL)
    pub fn neg16(c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output = Self::get_oper(ins.at(2), None, false);
        output.push(c.runtime_call(runtime::NEGHL));
        output.push("push hl".to_string());
        output
    }

    /// Absolute value of top of the stack (16 bits in HL)
    pub fn abs16(c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output = Self::get_oper(ins.at(2), None, false);
        output.push(c.runtime_call(runtime::ABS16));
        output.push("push hl".to_string());
        output
    }

    /// Logical right shift 16bit unsigned integer.
    /// The result is pushed onto the stack.
    ///
    /// Optimizations:
    ///   * If 2nd op is 0 then
    ///     do nothing
    ///
    ///   * If 2nd op is 1
    ///     Shift Right Arithmetic
    pub fn shru16(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (op1, op2) = (ins.at(2), ins.at(3));
        let label = c.tmp_label();
        let label2 = c.tmp_label();

        let mut output;
        if is_int(op2) {
            let op = Self::int16(int(op2));
            if op == 0 {
                return Vec::new();
            }

            output = Self::get_oper(op1, None, false);
            if op == 1 {
                output.push("srl h".to_string());
                output.push("rr l".to_string());
                output.push("push hl".to_string());
                return output;
            }

            output.push(format!("ld b, {op}"));
        } else {
            output = Bits8::get_oper(op2, None, false);
            output.push("ld b, a".to_string());
            output.extend(Self::get_oper(op1, None, false));
            output.push("or a".to_string());
            output.push(format!("jr z, {label2}"));
        }

        output.push(format!("{label}:"));
        output.push("srl h".to_string());
        output.push("rr l".to_string());
        output.push(format!("djnz {label}"));
        output.push(format!("{label2}:"));
        output.push("push hl".to_string());
        output
    }

    /// Arithmetical right shift 16bit signed integer.
    /// The result is pushed onto the stack.
    ///
    /// Optimizations:
    ///   * If 2nd op is 0 then
    ///     do nothing
    ///
    ///   * If 2nd op is 1
    ///     Shift Right Arithmetic
    pub fn shri16(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (op1, op2) = (ins.at(2), ins.at(3));
        let label = c.tmp_label();
        let label2 = c.tmp_label();

        let mut output;
        if is_int(op2) {
            let op = Self::int16(int(op2));
            if op == 0 {
                return Vec::new();
            }

            output = Self::get_oper(op1, None, false);
            if op == 1 {
                // `srl h`, a logical shift, where the loop below does
                // `sra h`: as in the Python.
                output.push("srl h".to_string());
                output.push("rr l".to_string());
                output.push("push hl".to_string());
                return output;
            }

            output.push(format!("ld b, {op}"));
        } else {
            output = Bits8::get_oper(op2, None, false);
            output.push("ld b, a".to_string());
            output.extend(Self::get_oper(op1, None, false));
            output.push("or a".to_string());
            output.push(format!("jr z, {label2}"));
        }

        output.push(format!("{label}:"));
        output.push("sra h".to_string());
        output.push("rr l".to_string());
        output.push(format!("djnz {label}"));
        output.push(format!("{label2}:"));
        output.push("push hl".to_string());
        output
    }

    /// Logical/aritmetical left shift 16bit (un)signed integer.
    /// The result is pushed onto the stack.
    ///
    /// Optimizations:
    ///   * If 2nd op is 0 then
    ///     do nothing
    ///
    ///   * If 2nd op is lower than 6
    ///     unroll lop
    pub fn shl16(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (op1, op2) = (ins.at(2), ins.at(3));
        let label = c.tmp_label();
        let label2 = c.tmp_label();

        let mut output;
        if is_int(op2) {
            let op = Self::int16(int(op2));
            if op == 0 {
                return Vec::new();
            }

            output = Self::get_oper(op1, None, false);
            if op < 6 {
                output.extend(repeat("add hl, hl", op));
                output.push("push hl".to_string());
                return output;
            }

            output.push(format!("ld b, {op}"));
        } else {
            output = Bits8::get_oper(op2, None, false);
            output.push("ld b, a".to_string());
            output.extend(Self::get_oper(op1, None, false));
            output.push("or a".to_string());
            output.push(format!("jr z, {label2}"));
        }

        output.push(format!("{label}:"));
        output.push("add hl, hl".to_string());
        output.push(format!("djnz {label}"));
        output.push(format!("{label2}:"));
        output.push("push hl".to_string());
        output
    }

    /// Loads a 16 bit value from a memory address
    /// If 2nd arg. start with '*', it is always treated as
    /// an indirect value.
    pub fn load16(_c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output = Self::get_oper(ins.at(2), None, false);
        output.push("push hl".to_string());
        output
    }

    /// Stores 2nd operand content into address of 1st operand.
    /// store16 a, x =>  *(&a) = x
    /// Use '*' for indirect store on 1st operand.
    pub fn store16(_c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output = Self::get_oper(ins.at(2), None, false);

        let mut value = ins.at(1);
        let mut indirect = false;

        // An empty operand is an IndexError, which the `except ValueError`
        // of the Python code does not catch.
        if first(value) == '*' {
            indirect = true;
            value = &value[1..];
        }

        if let Some(number) = parse_int(value) {
            let value = number & 0xFFFF;
            if indirect {
                output.push("ex de, hl".to_string());
                output.push(format!("ld hl, ({value})"));
                output.push("ld (hl), e".to_string());
                output.push("inc hl".to_string());
                output.push("ld (hl), d".to_string());
            } else {
                output.push(format!("ld ({value}), hl"));
            }
        } else if matches!(first(value), '_' | '.') {
            if indirect {
                output.push("ex de, hl".to_string());
                output.push(format!("ld hl, ({value})"));
                output.push("ld (hl), e".to_string());
                output.push("inc hl".to_string());
                output.push("ld (hl), d".to_string());
            } else {
                output.push(format!("ld ({value}), hl"));
            }
        } else if first(value) == '#' {
            let value = &value[1..];
            if indirect {
                output.push("ex de, hl".to_string());
                output.push(format!("ld hl, ({value})"));
                output.push("ld (hl), e".to_string());
                output.push("inc hl".to_string());
                output.push("ld (hl), d".to_string());
            } else {
                output.push(format!("ld ({value}), hl"));
            }
        } else {
            output.push("ex de, hl".to_string());
            if indirect {
                output.push("pop hl".to_string());
                output.push("ld a, (hl)".to_string());
                output.push("inc hl".to_string());
                output.push("ld h, (hl)".to_string());
                output.push("ld l, a".to_string());
            } else {
                output.push("pop hl".to_string());
            }

            output.push("ld (hl), e".to_string());
            output.push("inc hl".to_string());
            output.push("ld (hl), d".to_string());
        }

        output
    }

    /// Jumps if top of the stack (16bit) is 0 to arg(1)
    pub fn jzero16(_c: &mut Common, ins: &Quad) -> Vec<String> {
        let value = ins.at(1);
        if is_int(value) {
            if int(value) == 0 {
                return vec![format!("jp {}", ins.at(2))]; // Always true
            }
            return Vec::new();
        }

        let mut output = Self::get_oper(value, None, false);
        output.push("ld a, h".to_string());
        output.push("or l".to_string());
        output.push(format!("jp z, {}", ins.at(2)));
        output
    }

    /// Jumps if top of the stack (16bit) is >= 0 to arg(1)
    /// Always TRUE for unsigned
    pub fn jgezerou16(_c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output = Vec::new();
        let value = ins.at(1);
        if !is_int(value) {
            output = Self::get_oper(value, None, false);
        }

        output.push(format!("jp {}", ins.at(2)));
        output
    }

    /// Jumps if top of the stack (16bit) is >= 0 to arg(1)
    pub fn jgezeroi16(_c: &mut Common, ins: &Quad) -> Vec<String> {
        let value = ins.at(1);
        if is_int(value) {
            if int(value) >= 0 {
                return vec![format!("jp {}", ins.at(2))]; // Always true
            }
            return Vec::new();
        }

        let mut output = Self::get_oper(value, None, false);
        output.push("add hl, hl".to_string()); // Puts sign into carry
        output.push(format!("jp nc, {}", ins.at(2)));
        output
    }

    /// Returns from a procedure / function a 16bits value
    pub fn ret16(_c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output = Self::get_oper(ins.at(1), None, false);
        output.push("#pragma opt require hl".to_string());
        output.push(format!("jp {}", ins.at(2)));
        output
    }

    /// Pushes 16bit param into the stack
    pub fn param16(_c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output = Self::get_oper(ins.at(1), None, false);
        output.push("push hl".to_string());
        output
    }

    /// Passes a word as a __FASTCALL__ parameter.
    /// This is done by popping out of the stack for a
    /// value, or by loading it from memory (indirect)
    /// or directly (immediate)
    pub fn fparam16(_c: &mut Common, ins: &Quad) -> Vec<String> {
        Self::get_oper(ins.at(1), None, false)
    }

    /// Jumps if top of the stack (16bit) is != 0 to arg(1)
    pub fn jnzero16(_c: &mut Common, ins: &Quad) -> Vec<String> {
        let value = ins.at(1);
        if is_int(value) {
            if int(value) != 0 {
                return vec![format!("jp {}", ins.at(2))]; // Always true
            }
            return Vec::new();
        }

        let mut output = Self::get_oper(value, None, false);
        output.push("ld a, h".to_string());
        output.push("or l".to_string());
        output.push(format!("jp nz, {}", ins.at(2)));
        output
    }
}
