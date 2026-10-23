//! `backend/_str.py`: the quads working with strings. A string is a 16 bit
//! pointer to a block of dynamic memory; temporary ones (those coming from
//! the stack) must be freed once used.
//!
//! The Python class is `String`; it is `Str` here so as not to hide the
//! standard type.

use crate::common::{first, Common, QuadExt};
use crate::runtime;
use zxr_ir::Quad;

pub struct Str;

impl Str {
    /// Returns pop sequence for 16 bits operands
    /// 1st operand in HL, 2nd operand in DE
    ///
    /// You can swap operators extraction order
    /// by setting reversed to True.
    ///
    /// If no_exaf = True => No bits flags in A' will be used.
    ///                     This saves two bytes.
    ///
    /// The Python function returns `(tmp1, output)` when there is no second
    /// operand and `(tmp1, tmp2, output)` when there is; here it is always
    /// the latter, with `tmp2` false if there is no second operand.
    pub fn get_oper(c: &mut Common, op1: &str, op2: Option<&str>, reversed: bool, no_exaf: bool) -> (bool, bool, Vec<String>) {
        let mut output = Vec::new();

        let (mut op1, mut op2) = (op1, op2);
        if let (Some(second), true) = (op2, reversed) {
            op2 = Some(op1);
            op1 = second;
        }

        let mut tmp2 = false;
        if let Some(op2) = op2 {
            let mut val = op2;
            let indirect;
            if first(val) == '*' {
                indirect = true;
                val = &val[1..];
            } else {
                indirect = false;
            }

            if first(val) == '_' {
                // Direct
                output.push(format!("ld de, ({val})"));
            } else if first(val) == '#' {
                // Direct
                output.push(format!("ld de, {}", &val[1..]));
            } else if first(val) == '$' {
                // Direct in the stack
                output.push("pop de".to_string());
            } else {
                output.push("pop de".to_string());
                tmp2 = true;
            }

            if indirect {
                output.push(c.runtime_call(runtime::LOAD_DE_DE)); // TODO: Is this ever used??
            }
        }

        let mut tmp = Vec::new();
        if reversed {
            tmp = std::mem::take(&mut output);
        }

        let mut val = op1;
        let mut tmp1 = false;
        let indirect;
        if first(val) == '*' {
            indirect = true;
            val = &val[1..];
        } else {
            indirect = false;
        }

        if first(val) == '_' {
            // Direct
            output.push(format!("ld hl, ({val})"));
        } else if first(val) == '#' {
            // Immediate
            output.push(format!("ld hl, {}", &val[1..]));
        } else if first(val) == '$' {
            // Direct in the stack
            output.push("pop hl".to_string());
        } else {
            output.push("pop hl".to_string());
            tmp1 = true;
        }

        if indirect {
            output.push("ld c, (hl)".to_string());
            output.push("inc hl".to_string());
            output.push("ld h, (hl)".to_string());
            output.push("ld l, c".to_string());
        }

        if reversed {
            output.extend(tmp);
        }

        if !no_exaf {
            if tmp1 && tmp2 {
                output.push("ld a, 3".to_string()); // Marks both strings to be freed
            } else if tmp1 {
                output.push("ld a, 1".to_string()); // Marks str1 to be freed
            } else if tmp2 {
                output.push("ld a, 2".to_string()); // Marks str2 to be freed
            } else {
                output.push("xor a".to_string()); // Marks no string to be freed
            }
        }

        (tmp1, tmp2, output)
    }

    /// Outputs a FREEMEM sequence for 1 or 2 ops
    pub fn free_sequence(c: &mut Common, tmp1: bool, tmp2: bool) -> Vec<String> {
        if !tmp1 && !tmp2 {
            return Vec::new();
        }

        let mut output = Vec::new();
        if tmp1 && tmp2 {
            output.push("pop de".to_string());
            output.push("ex (sp), hl".to_string());
            output.push("push de".to_string());
            output.push(c.runtime_call(runtime::MEM_FREE));
            output.push("pop hl".to_string());
            output.push(c.runtime_call(runtime::MEM_FREE));
        } else {
            output.push("ex (sp), hl".to_string());
            output.push(c.runtime_call(runtime::MEM_FREE));
        }

        output.push("pop hl".to_string());
        output
    }

    /// Adds 2 string values. The result is pushed onto the stack.
    /// Note: This instruction does admit direct strings (as labels).
    pub fn addstr(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (tmp1, tmp2, mut output) = Self::get_oper(c, ins.at(2), Some(ins.at(3)), false, true);

        if tmp1 {
            output.push("push hl".to_string());
        }

        if tmp2 {
            output.push("push de".to_string());
        }

        output.push(c.runtime_call(runtime::ADDSTR));
        output.extend(Self::free_sequence(c, tmp1, tmp2));
        output.push("push hl".to_string());
        output
    }

    /// The six comparisons: they only differ in the routine called.
    fn compare(c: &mut Common, ins: &Quad, label: &str) -> Vec<String> {
        let (_, _, mut output) = Self::get_oper(c, ins.at(2), Some(ins.at(3)), false, false);
        output.push(c.runtime_call(label));
        output.push("push af".to_string());
        output
    }

    /// Compares & pops top 2 strings out of the stack.
    /// Temporal values are freed from memory. (a$ < b$)
    pub fn ltstr(c: &mut Common, ins: &Quad) -> Vec<String> {
        Self::compare(c, ins, runtime::STRLT)
    }

    /// Compares & pops top 2 strings out of the stack.
    /// Temporal values are freed from memory. (a$ > b$)
    pub fn gtstr(c: &mut Common, ins: &Quad) -> Vec<String> {
        Self::compare(c, ins, runtime::STRGT)
    }

    /// Compares & pops top 2 strings out of the stack.
    /// Temporal values are freed from memory. (a$ <= b$)
    pub fn lestr(c: &mut Common, ins: &Quad) -> Vec<String> {
        Self::compare(c, ins, runtime::STRLE)
    }

    /// Compares & pops top 2 strings out of the stack.
    /// Temporal values are freed from memory. (a$ >= b$)
    pub fn gestr(c: &mut Common, ins: &Quad) -> Vec<String> {
        Self::compare(c, ins, runtime::STRGE)
    }

    /// Compares & pops top 2 strings out of the stack.
    /// Temporal values are freed from memory. (a$ == b$)
    pub fn eqstr(c: &mut Common, ins: &Quad) -> Vec<String> {
        Self::compare(c, ins, runtime::STREQ)
    }

    /// Compares & pops top 2 strings out of the stack.
    /// Temporal values are freed from memory. (a$ != b$)
    pub fn nestr(c: &mut Common, ins: &Quad) -> Vec<String> {
        Self::compare(c, ins, runtime::STRNE)
    }

    /// Returns string length
    pub fn lenstr(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (tmp1, _, mut output) = Self::get_oper(c, ins.at(2), None, false, true);
        if tmp1 {
            output.push("push hl".to_string());
        }

        output.push(c.runtime_call(runtime::STRLEN));
        output.extend(Self::free_sequence(c, tmp1, false));
        output.push("push hl".to_string());
        output
    }

    /// Loads a string value from a memory address.
    pub fn loadstr(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (temporal, _, mut output) = Self::get_oper(c, ins.at(2), None, false, true);

        if !temporal {
            output.push(c.runtime_call(runtime::LOADSTR));
        }

        output.push("push hl".to_string());
        output
    }

    /// Stores a string value into a memory address.
    /// It copies content of 2nd operand (string), into 1st, reallocating
    /// dynamic memory for the 1st str. These instruction DOES ALLOW
    /// immediate strings for the 2nd parameter, starting with '#'.
    ///
    /// Must prepend '#' (immediate sigil) to 1st operand, as we need
    /// the & address of the destination.
    pub fn storestr(c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut op1 = ins.at(1).to_string();
        let indirect = first(&op1) == '*';
        if indirect {
            op1 = op1[1..].to_string();
        }

        let immediate = first(&op1) == '#';
        if immediate && !indirect {
            panic!("InvalidICError: storestr does not allow immediate destination: {ins}");
        }

        if !indirect {
            op1 = format!("#{op1}");
        }

        let (_, tmp2, mut output) = Self::get_oper(c, &op1, Some(ins.at(2)), false, true);

        if !tmp2 {
            output.push(c.runtime_call(runtime::STORE_STR));
        } else {
            output.push(c.runtime_call(runtime::STORE_STR2));
        }

        output
    }

    /// The body `jzerostr` and `jnzerostr` share, up to the jump.
    fn jump_on_len(c: &mut Common, ins: &Quad) -> Vec<String> {
        let mut output = Vec::new();
        let mut disposable = false; // True if string must be freed from memory

        if first(ins.at(1)) == '_' {
            // Variable?
            // The Python formats `ins[1][0]`, the first character, not the
            // operand: the line is always `ld hl, (_)`.
            output.push(format!("ld hl, ({})", first(ins.at(1))));
        } else {
            output.push("pop hl".to_string());
            output.push("push hl".to_string()); // Saves it for later
            disposable = true;
        }

        output.push(c.runtime_call(runtime::STRLEN));

        if disposable {
            output.push("ex (sp), hl".to_string());
            output.push(c.runtime_call(runtime::MEM_FREE));
            output.push("pop hl".to_string());
        }

        output.push("ld a, h".to_string());
        output.push("or l".to_string());
        output
    }

    /// Jumps if top of the stack contains a NULL pointer
    /// or its len is Zero
    pub fn jzerostr(c: &mut Common, ins: &Quad) -> Vec<String> {
        // TODO: Check if this is ever used?
        let mut output = Self::jump_on_len(c, ins);
        output.push(format!("jp z, {}", ins.at(2)));
        output
    }

    /// Jumps if top of the stack contains a string with
    /// at less 1 char
    pub fn jnzerostr(c: &mut Common, ins: &Quad) -> Vec<String> {
        // TODO: Check if this is ever used?
        let mut output = Self::jump_on_len(c, ins);
        output.push(format!("jp nz, {}", ins.at(2)));
        output
    }

    /// Returns from a procedure / function a string pointer (16bits) value
    pub fn retstr(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (tmp, _, mut output) = Self::get_oper(c, ins.at(1), None, false, true);

        if !tmp {
            output.push(c.runtime_call(runtime::LOADSTR));
        }

        output.push("#pragma opt require hl".to_string());
        output.push(format!("jp {}", ins.at(2)));
        output
    }

    /// Pushes an 16 bit unsigned value, which points
    /// to a string. For indirect values, it will push
    /// the pointer to the pointer :-)
    pub fn paramstr(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (_, _, mut output) = Self::get_oper(c, ins.at(1), None, false, false);
        output.pop(); // Remove a register flag (useless here)
        let tmp = matches!(first(ins.at(1)), '#' | '_'); // Determine if the string must be duplicated

        if tmp {
            output.push(c.runtime_call(runtime::LOADSTR)); // Must be duplicated
        }

        output.push("push hl".to_string());
        output
    }

    /// Passes a string ptr as a __FASTCALL__ parameter.
    /// This is done by popping out of the stack for a
    /// value, or by loading it from memory (indirect)
    /// or directly (immediate) --prefixed with '#'--
    pub fn fparamstr(c: &mut Common, ins: &Quad) -> Vec<String> {
        let (_, _, output) = Self::get_oper(c, ins.at(1), None, false, false);

        output
    }
}
