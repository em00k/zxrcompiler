//! `backend/_array.py`: the quads that load from and store into the elements
//! of a global array. The indexes are already on the stack; the runtime's
//! `__ARRAY` routine turns them and the array's address into the address of
//! the element.

use crate::bits32::Bits32;
use crate::common::{first, int, is_int, parse_int, Common, QuadExt};
use crate::f16::Fixed16;
use crate::float::Float;
use crate::runtime;
use zxr_ir::Quad;

/// Common subroutine for emitting array address
fn _addr(c: &mut Common, value: &str) -> Vec<String> {
    let mut output = Vec::new();
    let mut indirect = false;

    // In Python all of this is inside a `try`, but only `ValueError` (the
    // `int()` below) is caught: the `IndexError` of an empty operand is not.
    let mut value = value;
    if first(value) == '*' {
        indirect = true;
        value = &value[1..];
    }

    match parse_int(value) {
        Some(n) => {
            let value = n & 0xFFFF;
            if indirect {
                output.push(format!("ld hl, ({value})"));
            } else {
                output.push(format!("ld hl, {value}"));
            }
        }
        // except ValueError: `value` has already lost its `*` here.
        None => {
            if first(value) == '_' {
                output.push(format!("ld hl, {value}"));
                if indirect {
                    output.push("ld c, (hl)".to_string());
                    output.push("inc hl".to_string());
                    output.push("ld h, (hl)".to_string());
                    output.push("ld l, c".to_string());
                }
            } else {
                output.push("pop hl".to_string());
                if indirect {
                    output.push("ld c, (hl)".to_string());
                    output.push("inc hl".to_string());
                    output.push("ld h, (hl)".to_string());
                    output.push("ld l, c".to_string());
                }
            }
        }
    }

    output.push(c.runtime_call(runtime::ARRAY));

    output
}

/// Loads the address of an array element
/// into the stack.
pub fn _aaddr(c: &mut Common, ins: &Quad) -> Vec<String> {
    let mut output = _addr(c, ins.at(2));
    output.push("push hl".to_string());

    output
}

/// Loads an 8 bit value from a memory address
/// If 2nd arg. start with '*', it is always treated as
/// an indirect value.
pub fn _aload8(c: &mut Common, ins: &Quad) -> Vec<String> {
    let mut output = _addr(c, ins.at(2));
    output.push("ld a, (hl)".to_string());
    output.push("push af".to_string());

    output
}

/// Loads a 16 bit value from a memory address
/// If 2nd arg. start with '*', it is always treated as
/// an indirect value.
pub fn _aload16(c: &mut Common, ins: &Quad) -> Vec<String> {
    let mut output = _addr(c, ins.at(2));

    output.push("ld e, (hl)".to_string());
    output.push("inc hl".to_string());
    output.push("ld d, (hl)".to_string());
    output.push("ex de, hl".to_string());
    output.push("push hl".to_string());

    output
}

/// Load a 32 bit value from a memory address
/// If 2nd arg. start with '*', it is always treated as
/// an indirect value.
pub fn _aload32(c: &mut Common, ins: &Quad) -> Vec<String> {
    let mut output = _addr(c, ins.at(2));

    output.push(c.runtime_call(runtime::ILOAD32));
    output.push("push de".to_string());
    output.push("push hl".to_string());

    output
}

/// Loads a floating point value from a memory address.
/// If 2nd arg. start with '*', it is always treated as
/// an indirect value.
pub fn _aloadf(c: &mut Common, ins: &Quad) -> Vec<String> {
    let mut output = _addr(c, ins.at(2));
    output.push(c.runtime_call(runtime::LOADF));
    output.extend(Float::fpush());

    output
}

/// Loads a string value from a memory address.
pub fn _aloadstr(c: &mut Common, ins: &Quad) -> Vec<String> {
    let mut output = _addr(c, ins.at(2));

    output.push(c.runtime_call(runtime::ILOADSTR));
    output.push("push hl".to_string());

    output
}

/// Stores 2º operand content into address of 1st operand.
/// 1st operand is an array element. Dimensions are pushed into the
/// stack.
/// Use '*' for indirect store on 1st operand (A pointer to an array)
pub fn _astore8(c: &mut Common, ins: &Quad) -> Vec<String> {
    let mut output = _addr(c, ins.at(1));
    let mut op = ins.at(2);

    let indirect = first(op) == '*';
    if indirect {
        op = &op[1..];
    }

    let immediate = first(op) == '#';
    if immediate {
        op = &op[1..];
    }

    if is_int(op) {
        if indirect {
            if immediate {
                let op = int(op) & 0xFFFF; // Truncate to 16bit pointer
                output.push(format!("ld a, ({op})"));
            } else {
                // The operand as written, not the number it holds.
                output.push(format!("ld de, ({op})"));
                output.push("ld a, (de)".to_string());
            }
        } else {
            let op = int(op) & 0xFF; // Truncate to byte
            output.push(format!("ld (hl), {op}"));
            return output;
        }
    } else if first(op) == '_' || immediate {
        // An immediate is a constant, `.LABEL._name` or `(.LABEL._name) &
        // 0xFF` as well as `_name`; the Python tests for `_` alone and pops
        // a value nobody pushed.
        if indirect {
            if immediate {
                output.push(format!("ld a, ({op})")); // Redundant: *#_id == _id
            } else {
                output.push(format!("ld de, ({op})")); // *_id
                output.push("ld a, (de)".to_string());
            }
        } else if immediate {
            output.push(format!("ld a, {op}")); // #_id
        } else {
            output.push(format!("ld a, ({op})")); // _id
        }
    } else {
        output.push("pop af".to_string()); // tn
    }

    output.push("ld (hl), a".to_string());

    output
}

/// Stores 2º operand content into address of 1st operand.
/// store16 a, x =>  *(&a) = x
/// Use '*' for indirect store on 1st operand.
pub fn _astore16(c: &mut Common, ins: &Quad) -> Vec<String> {
    let mut output = _addr(c, ins.at(1));
    let mut op = ins.at(2);

    let indirect = first(op) == '*';
    if indirect {
        op = &op[1..];
    }

    let immediate = first(op) == '#';
    if immediate {
        op = &op[1..];
    }

    if is_int(op) {
        let op = int(op) & 0xFFFF; // Truncate to 16bit pointer

        if indirect {
            if immediate {
                output.push(format!("ld de, ({op})"));
            } else {
                output.push(format!("ld de, ({op})"));
                output.push(c.runtime_call(runtime::LOAD_DE_DE)); // TODO: Check if this is ever used
            }
        } else {
            let h = op >> 8;
            let l = op & 0xFF;
            output.push(format!("ld (hl), {l}"));
            output.push("inc hl".to_string());
            output.push(format!("ld (hl), {h}"));
            return output;
        }
    } else if first(op) == '_' || immediate {
        // As in `_astore8`.
        if indirect {
            if immediate {
                output.push(format!("ld de, ({op})")); // redundant: *#_id == _id
            } else {
                output.push(format!("ld de, ({op})")); // *_id
                output.push(c.runtime_call(runtime::LOAD_DE_DE)); // TODO: Check if this is ever used
            }
        } else if immediate {
            output.push(format!("ld de, {op}"));
        } else {
            output.push(format!("ld de, ({op})"));
        }
    } else {
        output.push("pop de".to_string());
    }

    output.push("ld (hl), e".to_string());
    output.push("inc hl".to_string());
    output.push("ld (hl), d".to_string());

    output
}

/// Stores 2º operand content into address of 1st operand.
/// store16 a, x =>  *(&a) = x
pub fn _astore32(c: &mut Common, ins: &Quad) -> Vec<String> {
    let mut output = _addr(c, ins.at(1));

    let mut value = ins.at(2);
    let indirect;
    if first(value) == '*' {
        value = &value[1..];
        indirect = true;
    } else {
        indirect = false;
    }

    match parse_int(value) {
        Some(n) => {
            let value = n & 0xFFFFFFFF; // Immediate?
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
        // except ValueError: the operand goes on without its `*`, so the
        // indirection is lost.
        None => output.extend(Bits32::get_oper(c, value, None, false, true)),
    }

    output.push(c.runtime_call(runtime::STORE32));

    output
}

/// Stores 2º operand content into address of 1st operand.
/// storef16 a, x =>  *(&a) = x
pub fn _astoref16(c: &mut Common, ins: &Quad) -> Vec<String> {
    let mut output = _addr(c, ins.at(1));

    let mut value = ins.at(2);
    let indirect;
    if first(value) == '*' {
        value = &value[1..];
        indirect = true;
    } else {
        indirect = false;
    }

    if indirect {
        output.push("push hl".to_string());
        output.extend(Fixed16::get_oper(c, value, None, true, false));
        output.push("pop hl".to_string()); // TODO: Check if this is ever used
        c.requires.insert("iload32.asm".to_string()); // ?? Nonsense
    } else {
        output.extend(Fixed16::get_oper(c, value, None, true, false));
    }

    output.push(c.runtime_call(runtime::STORE32)); // TODO: Check if this is ever used

    output
}

/// Stores a floating point value into a memory address.
pub fn _astoref(c: &mut Common, ins: &Quad) -> Vec<String> {
    let mut output = _addr(c, ins.at(1));

    // The Python strips the `*` into `value`, and then never uses `value`:
    // it is `ins[2]`, with its `*`, that goes to `Float.get_oper`.
    let value = ins.at(2);
    let indirect = first(value) == '*';

    if indirect {
        output.push("push hl".to_string());
        output.extend(Float::get_oper(c, ins.at(2), None));
        output.push("pop hl".to_string());
    } else {
        output.extend(Float::get_oper(c, ins.at(2), None));
    }

    output.push(c.runtime_call(runtime::STOREF));
    output
}

/// Stores a string value into a memory address.
/// It copies the content of the 2nd operand (string), into 1st, reallocating
/// dynamic memory for the 1st str. These instructions DO ALLOW
/// immediate strings for the 2nd parameter, starting with '#'.
pub fn _astorestr(c: &mut Common, ins: &Quad) -> Vec<String> {
    let mut output = _addr(c, ins.at(1));
    let mut op = ins.at(2);

    let indirect = first(op) == '*';
    if indirect {
        op = &op[1..];
    }

    let immediate = first(op) == '#';
    if immediate {
        op = &op[1..];
    }

    let mut temporal = first(op) != '$';
    if !temporal {
        op = &op[1..];
    }

    if is_int(op) {
        let op = int(op) & 0xFFFF;
        if indirect {
            if immediate {
                // *#<addr> = ld hl, (number)
                output.push(format!("ld de, ({op})"));
            } else {
                output.push(format!("ld de, ({op})"));
                output.push(c.runtime_call(runtime::LOAD_DE_DE)); // TODO: Check if this is ever used
            }
        } else {
            // Integer does not make sense here (unless it's a ptr)
            panic!("InvalidICError: {ins}");
        }
    } else if matches!(first(op), '.' | '_') {
        // an identifier
        temporal = false; // Global var is not a temporary string

        if indirect {
            if immediate {
                // *#_id = _id
                output.push(format!("ld de, ({op})"));
            } else {
                // *_id
                output.push(format!("ld de, ({op})"));
                output.push(c.runtime_call(runtime::LOAD_DE_DE)); // TODO: Check if this is ever used
            }
        } else if immediate {
            output.push(format!("ld de, {op}"));
        } else {
            output.push(format!("ld de, ({op})"));
        }
    } else {
        // tn
        output.push("pop de".to_string());

        if indirect {
            output.push(c.runtime_call(runtime::LOAD_DE_DE)); // TODO: Check if this is ever used
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
