//! `backend/_pload.py`: the quads that load from and store into parameters
//! and local variables, which live at an offset from the index register.

use crate::bits16::Bits16;
use crate::bits32::Bits32;
use crate::bits8::Bits8;
use crate::common::{first, int, is_int, Common, QuadExt, IDX_REG};
use crate::f16::Fixed16;
use crate::float::Float;
use crate::runtime;
use zxr_ir::Quad;

/// `"%+i" % i`: a decimal number with its sign always there (`+5`, `-3`, `+0`).
fn signed(i: i128) -> String {
    format!("{i:+}")
}

/// The offset operand of the `_pstore*` handlers: whether it is indirect
/// (starts with '*') and its number, with the 4 bytes of the return address
/// and the "push IX" added if it is a parameter.
fn offset_of(offset: &str) -> (bool, i128) {
    let mut offset = offset;
    let indirect = first(offset) == '*';
    if indirect {
        offset = &offset[1..];
    }

    let mut i = int(offset);
    if i >= 0 {
        i += 4; // Return Address + "push IX"
    }

    (indirect, i)
}

/// Returns code sequence which points to
/// local variable or parameter (HL)
pub fn _paddr(_c: &mut Common, ins: &Quad) -> Vec<String> {
    let mut output = Vec::new();

    let mut oper = ins.at(1);
    let indirect = first(oper) == '*';
    if indirect {
        oper = &oper[1..];
    }

    let mut i = int(oper);
    if i >= 0 {
        i += 4; // Return Address + "push IX"
    }

    output.push(format!("push {IDX_REG}"));
    output.push("pop hl".to_string());
    output.push(format!("ld de, {i}"));
    output.push("add hl, de".to_string());

    if indirect {
        output.push("ld e, (hl)".to_string());
        output.push("inc hl".to_string());
        output.push("ld h, (hl)".to_string());
        output.push("ld l, e".to_string());
    }

    output.push("push hl".to_string());
    output
}

/// Generic parameter loading.
/// Emits output code for loading at (IX + offset).
/// size = Number of bytes to load:
///     1 => 8 bit value   # A register
///     2 => 16 bit value / string  # HL register
///     4 => 32 bit value / f16 value  # DE (HI), HL (LO) register
///     5 => 40 bit value / float value  # A (exp) BC (HI), DE (LO) mantissa (as ZX Spectrum ROM)
///
/// (Python's `_pload(offset, size)`.)
fn pload(c: &mut Common, offset: &str, size: i128) -> Vec<String> {
    let mut output = Vec::new();

    let mut offset = offset;
    let indirect = first(offset) == '*';
    if indirect {
        offset = &offset[1..];
    }

    let mut i = int(offset);
    if i >= 0 {
        // If it is a parameter, round up to even bytes
        i += 4 + (if !indirect { size % 2 } else { 0 }); // Return Address + "push IX"
    }

    let ix_changed = (indirect || size < 5) && (i.abs() + size) > 127; // Offset > 127 bytes. Need to change IX
    if ix_changed {
        // more than 1 byte
        output.push(format!("push {IDX_REG}"));
        output.push(format!("ld de, {i}"));
        output.push(format!("add {IDX_REG}, de"));
        i = 0;
    } else if size == 5 {
        // For floating point numbers we always use DE as IX offset
        output.push(format!("push {IDX_REG}"));
        output.push("pop hl".to_string());
        output.push(format!("ld de, {i}"));
        output.push("add hl, de".to_string());
        i = 0;
    }

    if indirect {
        output.push(format!("ld h, ({IDX_REG}{})", signed(i + 1)));
        output.push(format!("ld l, ({IDX_REG}{})", signed(i)));

        if size == 1 {
            output.push("ld a, (hl)".to_string());
        } else if size == 2 {
            output.push("ld c, (hl)".to_string());
            output.push("inc hl".to_string());
            output.push("ld h, (hl)".to_string());
            output.push("ld l, c".to_string());
        } else if size == 4 {
            output.push(c.runtime_call(runtime::ILOAD32));
        } else {
            // Floating point
            output.push(c.runtime_call(runtime::ILOADF));
        }
    } else if size == 1 {
        output.push(format!("ld a, ({IDX_REG}{})", signed(i)));
    } else if size <= 4 {
        // 16/32bit integer, low part
        output.push(format!("ld l, ({IDX_REG}{})", signed(i)));
        output.push(format!("ld h, ({IDX_REG}{})", signed(i + 1)));

        if size > 2 {
            // 32 bit integer, high part
            output.push(format!("ld e, ({IDX_REG}{})", signed(i + 2)));
            output.push(format!("ld d, ({IDX_REG}{})", signed(i + 3)));
        }
    } else {
        // Floating point
        output.push(c.runtime_call(runtime::PLOADF));
    }

    if ix_changed {
        output.push(format!("pop {IDX_REG}"));
    }

    output
}

/// Loads from stack pointer (SP) + X, being
/// X 2st parameter.
///
/// 2st operand must be a SIGNED integer.
/// 1nd operand cannot be an immediate nor an address, but
/// can be an indirect (*) parameter, for function 'ByRef' implementation.
pub fn _pload8(c: &mut Common, ins: &Quad) -> Vec<String> {
    let mut output = pload(c, ins.at(2), 1);
    output.push("push af".to_string());
    output
}

/// Loads from stack pointer (SP) + X, being
/// X 2st parameter.
///
/// 1st operand must be a SIGNED integer.
/// 2nd operand cannot be an immediate nor an address.
pub fn _pload16(c: &mut Common, ins: &Quad) -> Vec<String> {
    let mut output = pload(c, ins.at(2), 2);
    output.push("push hl".to_string());
    output
}

/// Loads from stack pointer (SP) + X, being
/// X 2st parameter.
///
/// 1st operand must be a SIGNED integer.
/// 2nd operand cannot be an immediate nor an address.
pub fn _pload32(c: &mut Common, ins: &Quad) -> Vec<String> {
    let mut output = pload(c, ins.at(2), 4);
    output.push("push de".to_string());
    output.push("push hl".to_string());
    output
}

/// Loads from stack pointer (SP) + X, being
/// X 2st parameter.
///
/// 1st operand must be a SIGNED integer.
pub fn _ploadf(c: &mut Common, ins: &Quad) -> Vec<String> {
    let mut output = pload(c, ins.at(2), 5);
    output.extend(Float::fpush());
    output
}

/// Loads from stack pointer (SP) + X, being
/// X 2st parameter.
///
/// 1st operand must be a SIGNED integer.
/// 2nd operand cannot be an immediate nor an address.
pub fn _ploadstr(c: &mut Common, ins: &Quad) -> Vec<String> {
    let mut output = pload(c, ins.at(2), 2);
    if first(ins.at(1)) != '$' {
        output.push(c.runtime_call(runtime::LOADSTR));
    }

    output.push("push hl".to_string());
    output
}

/// Loads from stack pointer (SP) + X, being
/// X 2st parameter.
///
/// 1st operand must be a SIGNED integer.
/// Unlike ploadstr, this version does not push the result
/// back into the stack.
pub fn _fploadstr(c: &mut Common, ins: &Quad) -> Vec<String> {
    let mut output = pload(c, ins.at(2), 2);
    if first(ins.at(1)) != '$' {
        output.push(c.runtime_call(runtime::LOADSTR));
    }

    output
}

/// Stores 2nd parameter at stack pointer (SP) + X, being
/// X 1st parameter.
///
/// 1st operand must be a SIGNED integer.
pub fn _pstore8(_c: &mut Common, ins: &Quad) -> Vec<String> {
    let value = ins.at(2);
    let mut offset = ins.at(1);
    let indirect = first(offset) == '*';
    let mut size = 0;
    if indirect {
        offset = &offset[1..];
        size = 1;
    }

    let mut i = int(offset);
    if i >= 0 {
        i += 4; // Return Address + "push IX"
        if !indirect {
            i += 1; // F flag ignored
        }
    }

    let mut output = if is_int(value) { Vec::new() } else { Bits8::get_oper(value, None, false) };

    let ix_changed = !(-128 + size <= i && i <= 127 - size); // Offset > 127 bytes. Need to change IX
    if ix_changed {
        // more than 1 byte
        output.push(format!("push {IDX_REG}"));
        output.push("pop hl".to_string());
        output.push(format!("ld de, {i}"));
        output.push("add hl, de".to_string());
    }

    if indirect {
        if ix_changed {
            output.push("ld c, (hl)".to_string());
            output.push("inc hl".to_string());
            output.push("ld h, (hl)".to_string());
            output.push("ld l, c".to_string());
        } else {
            output.push(format!("ld h, ({IDX_REG}{})", signed(i + 1)));
            output.push(format!("ld l, ({IDX_REG}{})", signed(i)));
        }

        if is_int(value) {
            output.push(format!("ld (hl), {}", Bits8::int8(int(value))));
        } else {
            output.push("ld (hl), a".to_string());
        }

        return output;
    }

    // direct store
    if ix_changed {
        if is_int(value) {
            output.push(format!("ld (hl), {}", Bits8::int8(int(value))));
        } else {
            output.push("ld (hl), a".to_string());
        }

        return output;
    }

    if is_int(value) {
        output.push(format!("ld ({IDX_REG}{}), {}", signed(i), Bits8::int8(int(value))));
    } else {
        output.push(format!("ld ({IDX_REG}{}), a", signed(i)));
    }

    output
}

/// Stores 2nd parameter at stack pointer (SP) + X, being
/// X 1st parameter.
///
/// 1st operand must be a SIGNED integer.
pub fn _pstore16(c: &mut Common, ins: &Quad) -> Vec<String> {
    let value = ins.at(2);
    let size = 1;
    let (indirect, i) = offset_of(ins.at(1));

    let mut output = if is_int(value) { Vec::new() } else { Bits16::get_oper(value, None, false) };

    let ix_changed = !(-128 + size <= i && i <= 127 - size); // Offset > 127 bytes. Need to change IX

    if indirect {
        if is_int(value) {
            output.push(format!("ld hl, {}", Bits16::int16(int(value))));
        }

        output.push(format!("ld bc, {i}"));
        output.push(c.runtime_call(runtime::PISTORE16));
        return output;
    }

    // direct store
    if ix_changed {
        // more than 1 byte
        if !is_int(value) {
            output.push("ex de, hl".to_string());
        }

        output.push(format!("push {IDX_REG}"));
        output.push("pop hl".to_string());
        output.push(format!("ld bc, {i}"));
        output.push("add hl, bc".to_string());

        if is_int(value) {
            let v = Bits16::int16(int(value));
            output.push(format!("ld (hl), {}", v & 0xFF));
            output.push("inc hl".to_string());
            output.push(format!("ld (hl), {}", v >> 8));
            return output;
        }
        output.push("ld (hl), e".to_string());
        output.push("inc hl".to_string());
        output.push("ld (hl), d".to_string());
        return output;
    }

    if is_int(value) {
        let v = Bits16::int16(int(value));
        output.push(format!("ld ({IDX_REG}{}), {}", signed(i), v & 0xFF));
        output.push(format!("ld ({IDX_REG}{}), {}", signed(i + 1), v >> 8));
    } else {
        output.push(format!("ld ({IDX_REG}{}), l", signed(i)));
        output.push(format!("ld ({IDX_REG}{}), h", signed(i + 1)));
    }

    output
}

/// Stores 2nd parameter at stack pointer (SP) + X, being
/// X 1st parameter.
///
/// 1st operand must be a SIGNED integer.
pub fn _pstore32(c: &mut Common, ins: &Quad) -> Vec<String> {
    let value = ins.at(2);
    let (indirect, i) = offset_of(ins.at(1));

    let mut output = Bits32::get_oper(c, value, None, false, false);

    if indirect {
        output.push(format!("ld bc, {i}"));
        output.push(c.runtime_call(runtime::PISTORE32));
        return output;
    }

    // direct store
    output.push(format!("ld bc, {i}"));
    output.push(c.runtime_call(runtime::PSTORE32));

    output
}

/// Stores 2nd parameter at stack pointer (SP) + X, being
/// X 1st parameter.
///
/// 1st operand must be a SIGNED integer.
pub fn _pstoref16(c: &mut Common, ins: &Quad) -> Vec<String> {
    let value = ins.at(2);
    let (indirect, i) = offset_of(ins.at(1));

    let mut output = Fixed16::get_oper(c, value, None, false, false);

    if indirect {
        output.push(format!("ld bc, {i}"));
        output.push(c.runtime_call(runtime::PISTORE32));
        return output;
    }

    // direct store
    output.push(format!("ld bc, {i}"));
    output.push(c.runtime_call(runtime::PSTORE32));

    output
}

/// Stores 2nd parameter at stack pointer (SP) + X, being
/// X 1st parameter.
///
/// 1st operand must be a SIGNED integer.
pub fn _pstoref(c: &mut Common, ins: &Quad) -> Vec<String> {
    let value = ins.at(2);
    let (indirect, i) = offset_of(ins.at(1));

    let mut output = Float::get_oper(c, value, None);

    if indirect {
        output.push(format!("ld hl, {i}"));
        output.push(c.runtime_call(runtime::PISTOREF));
        return output;
    }

    // direct store
    output.push(format!("ld hl, {i}"));
    output.push(c.runtime_call(runtime::PSTOREF));

    output
}

/// Stores 2nd parameter at stack pointer (SP) + X, being
/// X 1st parameter.
///
/// 1st operand must be a SIGNED integer.
///
/// Note: This procedure proceeds as _pstore16, since STRINGS are 16bit pointers.
pub fn _pstorestr(c: &mut Common, ins: &Quad) -> Vec<String> {
    let mut output = Vec::new();
    let mut temporal = false;

    // 2nd operand first, because must go into the stack
    let mut value = ins.at(2);

    let indirect;
    if first(value) == '*' {
        value = &value[1..];
        indirect = true;
    } else {
        indirect = false;
    }

    if first(value) == '_' {
        output.push(format!("ld de, ({value})"));

        if indirect {
            output.push(c.runtime_call(runtime::LOAD_DE_DE));
        }
    } else if first(value) == '#' {
        output.push(format!("ld de, {}", &value[1..]));
    } else {
        output.push("pop de".to_string());
        temporal = first(value) != '$';
        if indirect {
            output.push(c.runtime_call(runtime::LOAD_DE_DE));
        }
    }

    // Now 1st operand
    let (indirect, i) = offset_of(ins.at(1));

    output.push(format!("ld bc, {i}"));

    if !temporal {
        if indirect {
            output.push(c.runtime_call(runtime::PISTORE_STR));
        } else {
            output.push(c.runtime_call(runtime::PSTORE_STR));
        }
    } else if indirect {
        output.push(c.runtime_call(runtime::PISTORE_STR2));
    } else {
        output.push(c.runtime_call(runtime::PSTORE_STR2));
    }

    output
}
