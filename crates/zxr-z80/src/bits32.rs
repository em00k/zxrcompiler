//! `backend/_32bit.py`: 32 bits operands. Implementation of 32bit (u32, i32)
//! operations.

use crate::bits8::Bits8;
use crate::common::{first, int, is_int, Common, QuadExt, _int_ops};
use crate::runtime;
use zxr_ir::Quad;

/// `op[1:]`: the operand without its first character.
fn rest(op: &str) -> &str {
    let mut chars = op.chars();
    chars.next();
    chars.as_str()
}

/// `op1, op2 = ins.args[1:]`: the 2nd and 3rd operands. The unpacking is
/// strict: Python raises `ValueError` unless there are exactly two.
fn args2(ins: &Quad) -> (&str, &str) {
    match ins.args.len() {
        3 => (&ins.args[1], &ins.args[2]),
        n if n < 3 => panic!("ValueError: not enough values to unpack (expected 2, got {})", n.saturating_sub(1)),
        _ => panic!("ValueError: too many values to unpack (expected 2)"),
    }
}

pub struct Bits32;

impl Bits32 {
    /// Returns a 32 bit operand converted to 32 bits unsigned int.
    /// Negative numbers are returned in 2 complement.
    ///
    /// The result is returned in a tuple (DE, HL) => High16, Low16
    pub fn int32(op: i128) -> (i128, i128) {
        let result = op & 0xFFFF_FFFF;
        (result >> 16, result & 0xFFFF)
    }

    /// Returns pop sequence for 32 bits operands
    /// 1st operand in HLDE, 2nd operand remains in the stack
    ///
    /// Now it does support operands inversion calling __SWAP32.
    ///
    /// However, if 1st operand is integer (immediate) or indirect, the stack
    /// will be rearranged, so it contains a 32 bit pushed parameter value for
    /// the subroutine to be called.
    ///
    /// If preserveHL is True, then BC will be used instead of HL for lower
    /// part for the 1st operand.
    pub fn get_oper(c: &mut Common, op1: &str, op2: Option<&str>, reversed: bool, preserve_hl: bool) -> Vec<String> {
        let mut output: Vec<String> = Vec::new();
        let mut op = op2.unwrap_or(op1);
        let mut int1 = false; // whether op1 (2nd operand) is integer

        let indirect = first(op) == '*';
        if indirect {
            op = rest(op);
        }

        let immediate = first(op) == '#';
        if immediate {
            op = rest(op);
        }

        let hl = if !preserve_hl { "hl" } else { "bc" };

        if is_int(op) {
            int1 = true;
            if indirect {
                // The operand goes out as it was written (` 7 `, `+5`), not
                // as the number it holds.
                if immediate {
                    output.push(format!("ld hl, {op}"));
                } else {
                    output.push(format!("ld hl, ({op})"));
                }

                output.push(c.runtime_call(runtime::ILOAD32));

                if preserve_hl {
                    output.push("ld b, h".to_string());
                    output.push("ld c, l".to_string());
                }
            } else {
                let (de, hl_value) = Bits32::int32(int(op));
                output.push(format!("ld de, {de}"));
                output.push(format!("ld {hl}, {hl_value}"));
            }
        } else {
            if first(op) == '_' {
                if immediate {
                    output.push(format!("ld {hl}, {op}"));
                } else {
                    output.push(format!("ld {hl}, ({op})"));
                }
            } else if immediate {
                output.push(format!("ld {hl}, ({op}) & 0xFFFF"));
            } else {
                output.push(format!("pop {hl}"));
            }

            if indirect {
                output.push(c.runtime_call(runtime::ILOAD32));

                if preserve_hl {
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

            let indirect = first(op) == '*';
            if indirect {
                op = rest(op);
            }

            let immediate = first(op) == '#';
            if immediate {
                op = rest(op);
            }

            if is_int(op) {
                let op = int(op);

                if indirect {
                    output.push("exx".to_string());
                    if immediate {
                        output.push(format!("ld hl, {}", op & 0xFFFF));
                    } else {
                        output.push(format!("ld hl, ({})", op & 0xFFFF));
                    }

                    output.push(c.runtime_call(runtime::ILOAD32)); // TODO: Is this ever used
                    output.push("push de".to_string());
                    output.push("push hl".to_string());
                    output.push("exx".to_string());
                } else {
                    let (de, hl_value) = Bits32::int32(op);
                    output.push(format!("ld bc, {de}"));
                    output.push("push bc".to_string());
                    output.push(format!("ld bc, {hl_value}"));
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

                output.push(c.runtime_call(runtime::ILOAD32)); // TODO: Is this ever used
                output.push("push de".to_string());
                output.push("push hl".to_string());
                output.push("exx".to_string());
            } else if immediate {
                output.push(format!("ld bc, ({op}) >> 16"));
                output.push("push bc".to_string());
                output.push(format!("ld bc, ({op}) & 0xFFFF"));
                output.push("push bc".to_string());
            } else if first(op) == '_' {
                // an address
                //
                // The Python code tests `op1[0] == "_"`, and here `op` is
                // `op1` untouched (neither indirect nor immediate), so the
                // test always holds and the `else` below is never taken.
                // Kept as it stands.
                if int1 || first(op1) == '_' {
                    // If previous op was integer, we can use hl in advance
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

    /// Outputs the assembler sequence for a binary (2 args) function that
    /// outputs a boolean (byte) value.
    ///
    /// `ins`: The Quad intermediate code.
    /// `label`: The Label of the ASM routine that computes the result.
    /// `commutative`: If true, the operands may be reversed in the stack for
    /// efficiency.
    pub fn bool_binary(c: &mut Common, ins: &Quad, label: &str, commutative: bool) -> Vec<String> {
        let (op1, op2) = args2(ins);
        let rev = commutative && first(op1) != 't' && !is_int(op1) && first(op2) == 't';

        let (mut op1, mut op2) = (op1.to_string(), op2.to_string());
        if commutative {
            if let Some((o1, o2)) = _int_ops(&op1, &op2) {
                // Op2 is always integer
                op1 = o1;
                op2 = o2.to_string();
            }
        }

        let mut output = Bits32::get_oper(c, &op1, Some(&op2), rev, false);
        output.push(c.runtime_call(label));
        output.push("push af".to_string());
        output
    }

    pub fn unary(c: &mut Common, ins: &Quad, label: &str) -> Vec<String> {
        let mut output = Bits32::get_oper(c, ins.at(2), None, false, false);
        output.push(c.runtime_call(label));
        output.push("push de".to_string());
        output.push("push hl".to_string());
        output
    }

    pub fn to_bool() -> Vec<String> {
        // 0 if not Carry, -1 if Carry  # 0 if not Carry (false), 1 if Carry (true),
        vec!["sbc a, a".to_string(), "neg".to_string(), "push af".to_string()]
    }

    // -----------------------------------------------------
    //               Arithmetic operations
    // -----------------------------------------------------

    /// Pops last 2 bytes from the stack and adds them.
    /// Then push the result onto the stack.
    ///
    /// Optimizations:
    ///   * If any of the operands is ZERO,
    ///     then do NOTHING: A + 0 = 0 + A = A
    pub fn add32(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (mut op1, mut op2) = args2(ins);

        if let Some((o1, o2)) = _int_ops(op1, op2) {
            if o2 == 0 {
                // A + 0 = 0 + A = A => Do Nothing
                let mut output = Bits32::get_oper(c, &o1, None, false, false);
                output.push("push de".to_string());
                output.push("push hl".to_string());
                return output;
            }
        }

        if first(op1) == '_' && first(op2) != '_' {
            std::mem::swap(&mut op1, &mut op2); // swap them
        }

        if first(op2) == '_' {
            let mut output = Bits32::get_oper(c, op1, None, false, false);
            output.push(format!("ld bc, ({op2})"));
            output.push("add hl, bc".to_string());
            output.push("ex de, hl".to_string());
            output.push(format!("ld bc, ({op2} + 2)"));
            output.push("adc hl, bc".to_string());
            output.push("push hl".to_string());
            output.push("push de".to_string());
            return output;
        }

        let mut output = Bits32::get_oper(c, op1, Some(op2), false, false);
        output.push("pop bc".to_string());
        output.push("add hl, bc".to_string());
        output.push("ex de, hl".to_string());
        output.push("pop bc".to_string());
        output.push("adc hl, bc".to_string());
        output.push("push hl".to_string()); // High and low parts are reversed
        output.push("push de".to_string());

        output
    }

    /// Pops last 2 dwords from the stack and subtract them.
    /// Then push the result onto the stack.
    /// NOTE: The operation is TOP[0] = TOP[-1] - TOP[0]
    ///
    /// If TOP[0] is 0, nothing is done
    pub fn sub32(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (op1, op2) = args2(ins);

        if is_int(op2) && int(op2) == 0 {
            // A - 0 = A => Do Nothing
            let mut output = Bits32::get_oper(c, op1, None, false, false);
            output.push("push de".to_string());
            output.push("push hl".to_string());
            return output;
        }

        let rev = first(op1) != 't' && !is_int(op1) && first(op2) == 't';

        let mut output = Bits32::get_oper(c, op1, Some(op2), rev, false);
        output.push(c.runtime_call(runtime::SUB32));
        output.push("push de".to_string());
        output.push("push hl".to_string());
        output
    }

    /// Multiplies two last 32bit values on top of the stack and
    /// and returns the value on top of the stack
    ///
    /// Optimizations done:
    ///
    ///   * If any operand is 1, do nothing
    ///   * If any operand is 0, push 0
    pub fn mul32(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (op1, op2) = args2(ins);
        let (mut op1, mut op2) = (op1.to_string(), op2.to_string());

        if let Some((o1, o2)) = _int_ops(&op1, &op2) {
            // This sequence is thrown away unless the number is 0 or 1, but
            // working it out may note a runtime routine as required: the
            // call stays.
            let mut output = Bits32::get_oper(c, &o1, None, false, false);

            if o2 == 1 {
                output.push("push de".to_string());
                output.push("push hl".to_string());
                return output; // A * 1 = Nothing
            }

            if o2 == 0 {
                output.push("ld hl, 0".to_string());
                output.push("push hl".to_string());
                output.push("push hl".to_string());
                return output;
            }

            op1 = o1;
            op2 = o2.to_string(); // convert it back to str
        }

        let mut output = Bits32::get_oper(c, &op1, Some(&op2), false, false);
        output.push(c.runtime_call(runtime::MUL32)); // Immediate
        output.push("push de".to_string());
        output.push("push hl".to_string());
        output
    }

    /// Divides 2 32bit unsigned integers. The result is pushed onto the stack.
    ///
    /// Optimizations:
    ///
    ///  * If 2nd operand is 1, do nothing
    pub fn divu32(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (op1, op2) = args2(ins);

        if is_int(op2) && int(op2) == 1 {
            let mut output = Bits32::get_oper(c, op1, None, false, false);
            output.push("push de".to_string());
            output.push("push hl".to_string());
            return output;
        }

        let rev = is_int(op1) || first(op1) == 't' || first(op2) != 't';
        let mut output = Bits32::get_oper(c, op1, Some(op2), rev, false);
        output.push(c.runtime_call(runtime::DIVU32));
        output.push("push de".to_string());
        output.push("push hl".to_string());
        output
    }

    /// Divides 2 32bit signed integers. The result is pushed onto the stack.
    ///
    /// Optimizations:
    ///
    ///  * If 2nd operand is 1, do nothing
    ///  * If 2nd operand is -1, do NEG32
    pub fn divi32(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (op1, op2) = args2(ins);

        if is_int(op2) {
            if int(op2) == 1 {
                let mut output = Bits32::get_oper(c, op1, None, false, false);
                output.push("push de".to_string());
                output.push("push hl".to_string());
                return output;
            }

            if int(op2) == -1 {
                return Bits32::neg32(c, ins);
            }
        }

        let rev = is_int(op1) || first(op1) == 't' || first(op2) != 't';
        let mut output = Bits32::get_oper(c, op1, Some(op2), rev, false);
        output.push(c.runtime_call(runtime::DIVI32));
        output.push("push de".to_string());
        output.push("push hl".to_string());
        output
    }

    /// Reminder of div. 2 32bit unsigned integers. The result is pushed onto
    /// the stack.
    ///
    /// Optimizations:
    ///
    ///  * If 2nd op is 1. Returns 0
    pub fn modu32(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (op1, op2) = args2(ins);

        if is_int(op2) && int(op2) == 1 {
            let mut output = Bits32::get_oper(c, op1, None, false, false);
            output.push("ld hl, 0".to_string());
            output.push("push hl".to_string());
            output.push("push hl".to_string());
            return output;
        }

        let rev = is_int(op1) || first(op1) == 't' || first(op2) != 't';
        let mut output = Bits32::get_oper(c, op1, Some(op2), rev, false);
        output.push(c.runtime_call(runtime::MODU32));
        output.push("push de".to_string());
        output.push("push hl".to_string());
        output
    }

    /// Reminder of div. 2 32bit signed integers. The result is pushed onto
    /// the stack.
    ///
    /// Optimizations:
    ///
    ///  * If 2nd op is 1. Returns 0
    pub fn modi32(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (op1, op2) = args2(ins);

        if is_int(op2) && int(op2) == 1 {
            let mut output = Bits32::get_oper(c, op1, None, false, false);
            output.push("ld hl, 0".to_string());
            output.push("push hl".to_string());
            output.push("push hl".to_string());
            return output;
        }

        let rev = is_int(op1) || first(op1) == 't' || first(op2) != 't';
        let mut output = Bits32::get_oper(c, op1, Some(op2), rev, false);
        output.push(c.runtime_call(runtime::MODI32));
        output.push("push de".to_string());
        output.push("push hl".to_string());
        output
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand < 2nd operand (top of the stack).
    /// Pushes 0 if False, 1 if True.
    ///
    /// 32 bit unsigned version
    pub fn ltu32(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (op1, op2) = args2(ins);
        let rev = first(op1) != 't' && !is_int(op1) && first(op2) == 't';
        let mut output = Bits32::get_oper(c, op1, Some(op2), rev, false);
        output.push(c.runtime_call(runtime::SUB32));
        output.push("sbc a, a".to_string());
        output.push("push af".to_string());
        output
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand < 2nd operand (top of the stack).
    /// Pushes 0 if False, 1 if True.
    ///
    /// 32 bit signed version
    pub fn lti32(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (op1, op2) = args2(ins);
        let rev = first(op1) != 't' && !is_int(op1) && first(op2) == 't';
        let mut output = Bits32::get_oper(c, op1, Some(op2), rev, false);
        output.push(c.runtime_call(runtime::LTI32)); // Checks A <= B ?
        output.push("push af".to_string());
        output
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand > 2nd operand (top of the stack).
    /// Pushes 0 if False, 1 if True.
    ///
    /// 32 bit unsigned version
    pub fn gtu32(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (op1, op2) = args2(ins);
        let rev = first(op1) != 't' && !is_int(op1) && first(op2) == 't';
        let mut output = Bits32::get_oper(c, op1, Some(op2), rev, false);
        output.push("pop bc".to_string());
        output.push("or a".to_string());
        output.push("sbc hl, bc".to_string());
        output.push("ex de, hl".to_string());
        output.push("pop de".to_string());
        output.push("sbc hl, de".to_string());
        output.push("sbc a, a".to_string());
        output.push("push af".to_string());
        output
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand > 2nd operand (top of the stack).
    /// Pushes 0 if False, 1 if True.
    ///
    /// 32 bit signed version
    pub fn gti32(c: &mut Common, ins: &Quad) -> Vec<String> {
        // TODO: Refact this as a call to _lei32() + pop af + ...
        let (op1, op2) = args2(ins);
        let rev = first(op1) != 't' && !is_int(op1) && first(op2) == 't';
        let mut output = Bits32::get_oper(c, op1, Some(op2), rev, false);
        output.push(c.runtime_call(runtime::LEI32)); // Checks A <= B ?
        output.push("sub 1".to_string()); // Carry if A = 0 (False)
        output.push("sbc a, a".to_string()); // Negates => A > B ?
        output.push("push af".to_string());
        output
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand <= 2nd operand (top of the stack).
    /// Pushes 0 if False, 1 if True.
    ///
    /// 32 bit unsigned version
    pub fn leu32(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (op1, op2) = args2(ins);
        let rev = first(op1) != 't' && !is_int(op1) && first(op2) == 't';
        let mut output = Bits32::get_oper(c, op1, Some(op2), rev, false);
        output.push("pop bc".to_string());
        output.push("or a".to_string());
        output.push("sbc hl, bc".to_string());
        output.push("ex de, hl".to_string());
        output.push("pop de".to_string());
        output.push("sbc hl, de".to_string()); // Carry if A > B
        output.push("ccf".to_string()); // Negates result => Carry if A <= B
        output.push("sbc a, a".to_string());
        output.push("push af".to_string());
        output
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand <= 2nd operand (top of the stack).
    /// Pushes 0 if False, 1 if True.
    ///
    /// 32 bit signed version
    pub fn lei32(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (op1, op2) = args2(ins);
        let rev = first(op1) != 't' && !is_int(op1) && first(op2) == 't';
        let mut output = Bits32::get_oper(c, op1, Some(op2), rev, false);
        output.push(c.runtime_call(runtime::LEI32)); // Checks A <= B ?
        output.push("push af".to_string());
        output
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand >= 2nd operand (top of the stack).
    /// Pushes 0 if False, 1 if True.
    ///
    /// 32 bit unsigned version
    pub fn geu32(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (op1, op2) = args2(ins);
        let rev = first(op1) != 't' && !is_int(op1) && first(op2) == 't';
        let mut output = Bits32::get_oper(c, op1, Some(op2), rev, false);
        output.push(c.runtime_call(runtime::SUB32)); // Carry if A < B
        output.push("ccf".to_string()); // Negates result => Carry if A >= B
        output.push("sbc a, a".to_string());
        output.push("push af".to_string());
        output
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand >= 2nd operand (top of the stack).
    /// Pushes 0 if False, 1 if True.
    ///
    /// 32 bit signed version
    pub fn gei32(c: &mut Common, ins: &Quad) -> Vec<String> {
        // TODO: Refact this as negated Boolean
        let (op1, op2) = args2(ins);
        let rev = first(op1) != 't' && !is_int(op1) && first(op2) == 't';
        let mut output = Bits32::get_oper(c, op1, Some(op2), rev, false);
        output.push(c.runtime_call(runtime::LTI32)); // A = (a < b)
        output.push("sub 1".to_string()); // Carry if !(a < b)
        output.push("sbc a, a".to_string()); // A = !(a < b) = (a >= b)
        output.push("push af".to_string());
        output
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand == 2nd operand (top of the stack).
    /// Pushes 0 if False, 1 if True.
    ///
    /// 32 bit un/signed version
    pub fn eq32(c: &mut Common, ins: &Quad) -> Vec<String> {
        Bits32::bool_binary(c, ins, runtime::EQ32, false)
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand != 2nd operand (top of the stack).
    /// Pushes 0 if False, 1 if True.
    ///
    /// 32 bit un/signed version
    pub fn ne32(c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output = Bits32::eq32(c, ins);
        output.pop(); // Compare 32 bits, but do not push result
        output.push("sub 1".to_string()); // Carry if A = 0 (False)
        output.push("sbc a, a".to_string()); // Negates => !(A == B)
        output.push("push af".to_string());
        output
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand OR (Logical) 2nd operand (top of the stack).
    /// Pushes 0 if False, 1 if True.
    ///
    /// 32 bit un/signed version
    pub fn or32(c: &mut Common, ins: &Quad) -> Vec<String> {
        Bits32::bool_binary(c, ins, runtime::OR32, false)
    }

    /// Pops top 2 operands out of the stack, and checks
    /// if the 1st operand OR (Bitwise) 2nd operand (top of the stack).
    /// Pushes result DE (high) HL (low)
    ///
    /// 32 bit un/signed version
    pub fn bor32(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (op1, op2) = args2(ins);
        let mut output = Bits32::get_oper(c, op1, Some(op2), false, false);
        output.push(c.runtime_call(runtime::BOR32));
        output.push("push de".to_string());
        output.push("push hl".to_string());
        output
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand XOR (Logical) 2nd operand (top of the stack).
    /// Pushes 0 if False, 1 if True.
    ///
    /// 32 bit un/signed version
    pub fn xor32(c: &mut Common, ins: &Quad) -> Vec<String> {
        Bits32::bool_binary(c, ins, runtime::XOR32, false)
    }

    /// Pops top 2 operands out of the stack, and checks
    /// if the 1st operand XOR (Bitwise) 2nd operand (top of the stack).
    /// Pushes result DE (high) HL (low)
    ///
    /// 32 bit un/signed version
    pub fn bxor32(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (op1, op2) = args2(ins);
        let mut output = Bits32::get_oper(c, op1, Some(op2), false, false);
        output.push(c.runtime_call(runtime::BXOR32));
        output.push("push de".to_string());
        output.push("push hl".to_string());
        output
    }

    /// Compares & pops top 2 operands out of the stack, and checks
    /// if the 1st operand AND (Logical) 2nd operand (top of the stack).
    /// Pushes 0 if False, 1 if True.
    ///
    /// 32 bit un/signed version
    pub fn and32(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (op1, op2) = args2(ins);

        if let Some((op1, op2)) = _int_ops(op1, op2) {
            if op2 == 0 {
                // X and False = False  # TODO: Move this to the optimizer
                let mut output = if first(&op1) == 't' {
                    // a temporary term (stack)
                    Bits32::get_oper(c, &op1, None, false, false) // Remove op1 from the stack
                } else {
                    Vec::new()
                };
                output.push("xor a".to_string());
                output.push("push af".to_string());
                return output;
            }

            // For X and TRUE = X we do nothing as we have to convert it to boolean
            // which is a rather expensive instruction
        }

        Bits32::bool_binary(c, ins, runtime::AND32, true)
    }

    /// Pops top 2 operands out of the stack, and checks
    /// if the 1st operand AND (Bitwise) 2nd operand (top of the stack).
    /// Pushes result DE (high) HL (low)
    ///
    /// 32 bit un/signed version
    pub fn band32(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (op1, op2) = args2(ins);
        let mut output = Bits32::get_oper(c, op1, Some(op2), false, false);
        output.push(c.runtime_call(runtime::BAND32));
        output.push("push de".to_string());
        output.push("push hl".to_string());
        output
    }

    /// Negates top (Logical NOT) of the stack (32 bits in DEHL)
    pub fn not32(c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output = Bits32::get_oper(c, ins.at(2), None, false, false);
        output.push(c.runtime_call(runtime::NOT32));
        output.push("push af".to_string());
        output
    }

    /// Negates top (Bitwise NOT) of the stack (32 bits in DEHL)
    pub fn bnot32(c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output = Bits32::get_oper(c, ins.at(2), None, false, false);
        output.push(c.runtime_call(runtime::BNOT32));
        output.push("push de".to_string());
        output.push("push hl".to_string());
        output
    }

    /// Negates top of the stack (32 bits in DEHL)
    pub fn neg32(c: &mut Common, ins: &Quad) -> Vec<String> {
        Bits32::unary(c, ins, runtime::NEG32)
    }

    /// Absolute value of top of the stack (32 bits in DEHL)
    pub fn abs32(c: &mut Common, ins: &Quad) -> Vec<String> {
        Bits32::unary(c, ins, runtime::ABS32)
    }

    /// Logical Right shift 32bit unsigned integers.
    /// The result is pushed onto the stack.
    ///
    /// Optimizations:
    ///
    ///  * If 2nd operand is 0, do nothing
    pub fn shru32(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (op1, op2) = args2(ins);

        if is_int(op2) {
            let mut output = Bits32::get_oper(c, op1, None, false, false);

            if int(op2) == 0 {
                output.push("push de".to_string());
                output.push("push hl".to_string());
                return output;
            }

            if int(op2) > 1 {
                let label = c.tmp_label();
                // The count goes out as it was written, not as a number.
                output.push(format!("ld b, {op2}"));
                output.push(format!("{label}:"));
                output.push(c.runtime_call(runtime::SHRL32));
                output.push(format!("djnz {label}"));
            } else {
                output.push(c.runtime_call(runtime::SHRL32));
            }

            output.push("push de".to_string());
            output.push("push hl".to_string());
            return output;
        }

        let mut output = Bits8::get_oper(op2, None, false);
        output.push("ld b, a".to_string());
        output.extend(Bits32::get_oper(c, op1, None, false, false));
        let label = c.tmp_label();
        let label2 = c.tmp_label();
        output.push("or a".to_string());
        output.push(format!("jr z, {label2}"));
        output.push(format!("{label}:"));
        output.push(c.runtime_call(runtime::SHRL32));
        output.push(format!("djnz {label}"));
        output.push(format!("{label2}:"));
        output.push("push de".to_string());
        output.push("push hl".to_string());
        output
    }

    /// Logical Right shift 32bit unsigned integers.
    /// The result is pushed onto the stack.
    ///
    /// Optimizations:
    ///
    ///  * If 2nd operand is 0, do nothing
    pub fn shri32(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (op1, op2) = args2(ins);

        if is_int(op2) {
            let mut output = Bits32::get_oper(c, op1, None, false, false);

            if int(op2) == 0 {
                output.push("push de".to_string());
                output.push("push hl".to_string());
                return output;
            }

            if int(op2) > 1 {
                let label = c.tmp_label();
                // The count goes out as it was written, not as a number.
                output.push(format!("ld b, {op2}"));
                output.push(format!("{label}:"));
                output.push(c.runtime_call(runtime::SHRA32));
                output.push(format!("djnz {label}"));
            } else {
                output.push(c.runtime_call(runtime::SHRA32));
            }

            output.push("push de".to_string());
            output.push("push hl".to_string());
            return output;
        }

        let mut output = Bits8::get_oper(op2, None, false);
        output.push("ld b, a".to_string());
        output.extend(Bits32::get_oper(c, op1, None, false, false));
        let label = c.tmp_label();
        let label2 = c.tmp_label();
        output.push("or a".to_string());
        output.push(format!("jr z, {label2}"));
        output.push(format!("{label}:"));
        output.push(c.runtime_call(runtime::SHRA32));
        output.push(format!("djnz {label}"));
        output.push(format!("{label2}:"));
        output.push("push de".to_string());
        output.push("push hl".to_string());
        output
    }

    /// Logical Left shift 32bit unsigned integers.
    /// The result is pushed onto the stack.
    ///
    /// Optimizations:
    ///
    ///  * If 2nd operand is 0, do nothing
    pub fn shl32(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (op1, op2) = args2(ins);

        if is_int(op2) {
            let mut output = Bits32::get_oper(c, op1, None, false, false);

            if int(op2) == 0 {
                output.push("push de".to_string());
                output.push("push hl".to_string());
                return output;
            }

            if int(op2) > 1 {
                let label = c.tmp_label();
                // The count goes out as it was written, not as a number.
                output.push(format!("ld b, {op2}"));
                output.push(format!("{label}:"));
                output.push(c.runtime_call(runtime::SHL32));
                output.push(format!("djnz {label}"));
            } else {
                output.push(c.runtime_call(runtime::SHL32));
            }

            output.push("push de".to_string());
            output.push("push hl".to_string());
            return output;
        }

        let mut output = Bits8::get_oper(op2, None, false);
        output.push("ld b, a".to_string());
        output.extend(Bits32::get_oper(c, op1, None, false, false));
        let label = c.tmp_label();
        let label2 = c.tmp_label();
        output.push("or a".to_string());
        output.push(format!("jr z, {label2}"));
        output.push(format!("{label}:"));
        output.push(c.runtime_call(runtime::SHL32));
        output.push(format!("djnz {label}"));
        output.push(format!("{label2}:"));
        output.push("push de".to_string());
        output.push("push hl".to_string());
        output
    }

    /// Load a 32 bit value from a memory address
    /// If 2nd arg. start with '*', it is always treated as
    /// an indirect value.
    pub fn load32(c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output = Bits32::get_oper(c, ins.at(2), None, false, false);
        output.push("push de".to_string());
        output.push("push hl".to_string());
        output
    }

    /// Stores 2nd operand content into address of 1st operand.
    /// store16 a, x =>  *(&a) = x
    pub fn store32(c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut op = ins.at(1);

        let indirect = first(op) == '*';
        if indirect {
            op = rest(op);
        }

        let immediate = first(op) == '#'; // Might make no sense here?
        if immediate {
            op = rest(op);
        }

        // `op[0] in "_."` is looked at before `immediate`: an operand that
        // is just `#` is an IndexError.
        if is_int(op) || matches!(first(op), '_' | '.') || immediate {
            let mut output = Bits32::get_oper(c, ins.at(2), None, false, indirect);

            let op = if is_int(op) { (int(op) & 0xFFFF).to_string() } else { op.to_string() };

            if indirect {
                output.push(format!("ld hl, ({op})"));
                output.push(c.runtime_call(runtime::STORE32)); // TODO: is this ever used?
                return output;
            }

            output.push(format!("ld ({op}), hl"));
            output.push(format!("ld ({op} + 2), de"));

            return output;
        }

        let mut output = Bits32::get_oper(c, ins.at(2), None, false, true);
        output.push("pop hl".to_string());

        if indirect {
            output.push(c.runtime_call(runtime::ISTORE32)); // TODO: is this ever used?

            return output;
        }

        output.push(c.runtime_call(runtime::STORE32));
        output
    }

    /// Jumps if top of the stack (32bit) is 0 to arg(1)
    pub fn jzero32(c: &mut Common, ins: &Quad) -> Vec<String> {
        let value = ins.at(1);
        if is_int(value) {
            if int(value) == 0 {
                return vec![format!("jp {}", ins.at(2))]; // Always true
            }
            return Vec::new();
        }

        let mut output = Bits32::get_oper(c, value, None, false, false);
        output.push("ld a, h".to_string());
        output.push("or l".to_string());
        output.push("or e".to_string());
        output.push("or d".to_string());
        output.push(format!("jp z, {}", ins.at(2)));
        output
    }

    /// Jumps if top of the stack (23bit) is >= 0 to arg(1)
    /// Always TRUE for unsigned
    pub fn jgezerou32(c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output = Vec::new();
        let value = ins.at(1);
        if !is_int(value) {
            output = Bits32::get_oper(c, value, None, false, false);
        }

        output.push(format!("jp {}", ins.at(2)));
        output
    }

    /// Jumps if top of the stack (32bit) is >= 0 to arg(1)
    pub fn jgezeroi32(c: &mut Common, ins: &Quad) -> Vec<String> {
        let value = ins.at(1);
        if is_int(value) {
            if int(value) >= 0 {
                return vec![format!("jp {}", ins.at(2))]; // Always true
            }
            return Vec::new();
        }

        let mut output = Bits32::get_oper(c, value, None, false, false);
        output.push("ld a, d".to_string());
        output.push("add a, a".to_string()); // Puts sign into carry
        output.push(format!("jp nc, {}", ins.at(2)));
        output
    }

    /// Returns from a procedure / function a 32bits value (even Fixed point)
    pub fn ret32(c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output = Bits32::get_oper(c, ins.at(1), None, false, false);
        output.push("#pragma opt require hl,de".to_string());
        output.push(format!("jp {}", ins.at(2)));
        output
    }

    /// Pushes 32bit param into the stack
    pub fn param32(c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output = Bits32::get_oper(c, ins.at(1), None, false, false);
        output.push("push de".to_string());
        output.push("push hl".to_string());
        output
    }

    /// Passes a dword as a __FASTCALL__ parameter.
    /// This is done by popping out of the stack for a
    /// value, or by loading it from memory (indirect)
    /// or directly (immediate)
    pub fn fparam32(c: &mut Common, ins: &Quad) -> Vec<String> {
        Bits32::get_oper(c, ins.at(1), None, false, false)
    }

    /// Jumps if top of the stack (32bit) is !=0 to arg(1)
    pub fn jnzero32(c: &mut Common, ins: &Quad) -> Vec<String> {
        let value = ins.at(1);
        if is_int(value) {
            if int(value) != 0 {
                return vec![format!("jp {}", ins.at(2))]; // Always true
            }
            return Vec::new();
        }

        let mut output = Bits32::get_oper(c, value, None, false, false);
        output.push("ld a, h".to_string());
        output.push("or l".to_string());
        output.push("or e".to_string());
        output.push("or d".to_string());
        output.push(format!("jp nz, {}", ins.at(2)));
        output
    }
}
