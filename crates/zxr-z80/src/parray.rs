//! `backend/_parray.py`: the quads that load from and store into the
//! elements of an array that is a parameter or a local variable.

use crate::common::{first, parse_float, parse_int, Common, QuadExt, IDX_REG};
use crate::bits32::Bits32;
use crate::f16::Fixed16;
use crate::float::Float;
use crate::fp;
use crate::runtime;
use zxr_ir::Quad;

/// Generic element array-address stack-ptr loading.
/// Emits output code for setting IX at the right location.
/// bytes = Number of bytes to load:
///     1 => 8 bit value
///     2 => 16 bit value / string
///     4 => 32 bit value / f16 value
///     5 => 40 bit value
///
/// (Python's `_paddr(offset)`; the quad handler of that name is in
/// `pload.rs`.)
fn paddr(c: &mut Common, offset: &str) -> Vec<String> {
    let mut output = Vec::new();

    let mut offset = offset;
    let indirect = first(offset) == '*';
    if indirect {
        offset = &offset[1..];
    }

    // An offset that is not a number is a ValueError nobody catches: the
    // handlers call this before their `try`.
    let mut i = crate::common::int(offset);
    if i >= 0 {
        i += 4; // Return Address + "push IX"
    }

    output.push(format!("push {IDX_REG}"));
    output.push("pop hl".to_string());
    output.push(format!("ld de, {i}"));
    output.push("add hl, de".to_string());

    if indirect {
        output.push(c.runtime_call(runtime::ARRAY_PTR));
    } else {
        output.push(c.runtime_call(runtime::ARRAY));
    }

    output
}

/// The start of the `_pastore*` handlers: the value operand without its
/// leading '*', and whether it had one.
fn split_indirect(value: &str) -> (&str, bool) {
    if first(value) == '*' {
        (&value[1..], true)
    } else {
        (value, false)
    }
}

/// Loads address of an array element into the stack
pub fn _paaddr(c: &mut Common, ins: &Quad) -> Vec<String> {
    let mut output = paddr(c, ins.at(2));
    output.push("push hl".to_string());

    output
}

/// Loads an 8 bit value from a memory address
/// If 2nd arg. start with '*', it is always treated as
/// an indirect value.
pub fn _paload8(c: &mut Common, ins: &Quad) -> Vec<String> {
    let mut output = paddr(c, ins.at(2));
    output.push("ld a, (hl)".to_string());
    output.push("push af".to_string());

    output
}

/// Loads a 16 bit value from a memory address
/// If 2nd arg. start with '*', it is always treated as
/// an indirect value.
pub fn _paload16(c: &mut Common, ins: &Quad) -> Vec<String> {
    let mut output = paddr(c, ins.at(2));

    output.push("ld e, (hl)".to_string());
    output.push("inc hl".to_string());
    output.push("ld d, (hl)".to_string());
    output.push("ex de, hl".to_string());
    output.push("push hl".to_string());

    output
}

/// Loads a 32 bit value from a memory address
/// If 2nd arg. start with '*', it is always treated as
/// an indirect value.
pub fn _paload32(c: &mut Common, ins: &Quad) -> Vec<String> {
    let mut output = paddr(c, ins.at(2));

    output.push(c.runtime_call(runtime::ILOAD32));
    output.push("push de".to_string());
    output.push("push hl".to_string());

    output
}

/// Loads a floating point value from a memory address.
pub fn _paloadf(c: &mut Common, ins: &Quad) -> Vec<String> {
    let mut output = paddr(c, ins.at(2));

    output.push(c.runtime_call(runtime::LOADF));
    output.extend(Float::fpush());

    output
}

/// Loads a string value from a memory address.
pub fn _paloadstr(c: &mut Common, ins: &Quad) -> Vec<String> {
    let mut output = paddr(c, ins.at(2));

    output.push(c.runtime_call(runtime::ILOADSTR));
    output.push("push hl".to_string());

    output
}

/// Stores 2º operand content into address of 1st operand.
/// 1st operand is an array element. Dimensions are pushed into the
/// stack.
/// Use '*' for indirect store on 1st operand (A pointer to an array)
pub fn _pastore8(c: &mut Common, ins: &Quad) -> Vec<String> {
    let mut output = paddr(c, ins.at(1));

    let (value, indirect) = split_indirect(ins.at(2));

    match parse_int(value) {
        Some(value) => {
            let mut value = value & 0xFFFF;
            if indirect {
                output.push(format!("ld a, ({value})"));
                output.push("ld (hl), a".to_string());
            } else {
                value &= 0xFF;
                output.push(format!("ld (hl), {value}"));
            }
        }
        // An immediate that is not a number is a constant all the same
        // (`#(.LABEL._x) & 0xFF`); the Python takes it from the stack, where
        // nothing put it.
        None if first(value) == '#' => {
            let value = &value[1..];
            output.push(if indirect { format!("ld a, ({value})") } else { format!("ld a, {value}") });
            output.push("ld (hl), a".to_string());
        }
        // except ValueError
        None => {
            output.push("pop af".to_string());
            output.push("ld (hl), a".to_string());
        }
    }

    output
}

/// Stores 2º operand content into address of 1st operand.
/// store16 a, x =>  *(&a) = x
/// Use '*' for indirect store on 1st operand.
pub fn _pastore16(c: &mut Common, ins: &Quad) -> Vec<String> {
    let mut output = paddr(c, ins.at(1));

    let (value, indirect) = split_indirect(ins.at(2));

    match parse_int(value) {
        Some(value) => {
            let value = value & 0xFFFF;
            output.push(format!("ld de, {value}"));
            if indirect {
                output.push(c.runtime_call(runtime::LOAD_DE_DE));
            }
        }
        // As in `_pastore8`.
        None if first(value) == '#' => {
            let value = &value[1..];
            output.push(if indirect { format!("ld de, ({value})") } else { format!("ld de, {value}") });
        }
        // except ValueError
        None => {
            output.push("pop de".to_string());
        }
    }

    output.push("ld (hl), e".to_string());
    output.push("inc hl".to_string());
    output.push("ld (hl), d".to_string());

    output
}

/// Stores 2º operand content into address of 1st operand.
/// store16 a, x =>  *(&a) = x
pub fn _pastore32(c: &mut Common, ins: &Quad) -> Vec<String> {
    let mut output = paddr(c, ins.at(1));

    let (value, indirect) = split_indirect(ins.at(2));

    match parse_int(value) {
        Some(value) => {
            let value = value & 0xFFFFFFFF; // Immediate?
            if indirect {
                output.push("push hl".to_string());
                output.push(format!("ld hl, {}", value & 0xFFFF));
                output.push(c.runtime_call(runtime::ILOAD32));
                output.push("ld b, h".to_string());
                output.push("ld c, l".to_string()); // BC = Lower 16 bits
                output.push("pop hl".to_string());
            } else {
                output.push(format!("ld de, {}", value >> 16));
                output.push(format!("ld bc, {}", value & 0xFFFF));
            }
        }
        // As in `_pastore8`; loaded as `_astore32` loads it.
        None if first(value) == '#' && !indirect => output.extend(Bits32::get_oper(c, value, None, false, true)),
        // except ValueError
        None => {
            output.push("pop bc".to_string());
            output.push("pop de".to_string());
        }
    }

    output.push(c.runtime_call(runtime::STORE32));
    output
}

/// Stores 2º operand content into address of 1st operand.
/// storef16 a, x =>  *(&a) = x
pub fn _pastoref16(c: &mut Common, ins: &Quad) -> Vec<String> {
    let mut output = paddr(c, ins.at(1));

    let (value, indirect) = split_indirect(ins.at(2));

    // The Python `try` block, up to the point where it can raise ValueError:
    // `Some` is what it goes on with, `None` is the `except`.
    let immediate = if indirect {
        // Python reads `int(ins[2])` here, the operand with its '*' still
        // on, which is never a number: an indirect value always ends up in
        // the `except` branch and is taken from the stack.
        parse_int(ins.at(2)).map(|value| (value, 0))
    } else {
        // `Fixed16.f16(value)` starts with `float(value)`. A NaN gets past
        // that and raises ValueError too, at f16's `int(op)`.
        parse_float(value).filter(|x| !x.is_nan()).map(Fixed16::f16)
    };

    match immediate {
        Some((value, _)) if indirect => {
            output.push("push hl".to_string());
            output.push(format!("ld hl, {}", value & 0xFFFF));
            output.push(c.runtime_call(runtime::ILOAD32));
            output.push("ld b, h".to_string());
            output.push("ld c, l".to_string()); // BC = Lower 16 bits
            output.push("pop hl".to_string());
        }
        Some((de, hl)) => {
            output.push(format!("ld de, {de}"));
            output.push(format!("ld bc, {hl}"));
        }
        // except ValueError
        None => {
            output.push("pop bc".to_string());
            output.push("pop de".to_string());
        }
    }

    output.push(c.runtime_call(runtime::STORE32));
    output
}

/// Stores a floating point value into a memory address.
pub fn _pastoref(c: &mut Common, ins: &Quad) -> Vec<String> {
    let mut output = paddr(c, ins.at(1));

    let (value, indirect) = split_indirect(ins.at(2));

    // Whether the `try` block got to its end; otherwise ValueError was
    // raised before anything was emitted.
    let mut done = false;
    if indirect {
        if let Some(value) = parse_int(value) {
            let value = value & 0xFFFF; // Immediate?
            output.push("push hl".to_string());
            output.push(format!("ld hl, {value}"));
            output.push(c.runtime_call(runtime::ILOADF));
            output.push("ld a, c".to_string());
            output.push("ld b, h".to_string());
            output.push("ld c, l".to_string()); // BC = Lower 16 bits, A = Exp
            output.push("pop hl".to_string()); // Recovers pointer
            done = true;
        }
    } else if let Some(value) = parse_float(value) {
        // Immediate?
        // A NaN raises ValueError inside `fp.immediate_float` (at an
        // `int(nan)`), which the `except` here catches as well.
        if !value.is_nan() {
            let (c_, de, hl) = fp::immediate_float(value);
            output.push(format!("ld a, {c_}"));
            output.push(format!("ld de, {de}"));
            output.push(format!("ld bc, {hl}"));
            done = true;
        }
    }

    // except ValueError
    if !done {
        output.push("pop bc".to_string());
        output.push("pop de".to_string());
        output.push("ex (sp), hl".to_string()); // Preserve HL for STOREF
        output.push("ld a, l".to_string());
        output.push("pop hl".to_string());
    }

    output.push(c.runtime_call(runtime::STOREF));
    output
}

/// Stores a string value into a memory address.
/// It copies the content of the 2nd operand (string), into 1st, reallocating
/// dynamic memory for the 1st str. These instructions DO ALLOW
/// immediate strings for the 2nd parameter, starting with '#'.
pub fn _pastorestr(c: &mut Common, ins: &Quad) -> Vec<String> {
    let mut output = paddr(c, ins.at(1));
    let mut value = ins.at(2);

    let indirect = first(value) == '*';
    if indirect {
        value = &value[1..];
    }

    let immediate = first(value) == '#';
    if immediate {
        value = &value[1..];
    }

    // Python drops the first character of a temporal ("t1" becomes "1"),
    // whatever it is, and looks at the next one below.
    let temporal = !immediate && first(value) != '$';
    if temporal {
        value = &value[first(value).len_utf8()..];
    }

    if first(value) == '_' {
        if indirect {
            if immediate {
                output.push(format!("ld de, ({value})"));
            } else {
                output.push(format!("ld de, ({value})"));
                output.push(c.runtime_call(runtime::LOAD_DE_DE));
            }
        } else if immediate {
            output.push(format!("ld de, {value}"));
        } else {
            output.push(format!("ld de, ({value})"));
        }
    } else {
        if immediate {
            output.push(format!("ld de, {value}"));
        } else {
            output.push("pop de".to_string());
        }

        if indirect {
            output.push(c.runtime_call(runtime::LOAD_DE_DE));
        }
    }

    if !temporal {
        output.push(c.runtime_call(runtime::STORE_STR));
    } else {
        // A value already on dynamic memory
        output.push(c.runtime_call(runtime::STORE_STR2));
    }

    output
}
