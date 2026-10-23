//! `backend/generic.py`: the quads that are not tied to a data type. Labels,
//! jumps, calls, function entry and exit, variable storage, type casts and
//! inline assembly.

use crate::bits16::Bits16;
use crate::bits32::Bits32;
use crate::bits8::Bits8;
use crate::common::{eval_list, first, get_bytes_size, int, is_hexa, parse_int, strs, yy_type_size, Common, QuadExt, CALL_BACK, END_LABEL, IDX_REG};
use crate::f16::Fixed16;
use crate::float::Float;
use crate::runtime;
use std::sync::OnceLock;
use zxr_ir::Quad;
use zxr_re::{Flags, Regex};

/// Label RegExp
fn re_label() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^[ \t]*[a-zA-Z_][_a-zA-Z\d]*:", Flags::default()).unwrap())
}

/// `InvalidICError`: an exception no caller catches.
fn invalid_ic(ins: &Quad, msg: String) -> ! {
    let _ = ins;
    panic!("InvalidICError: {msg}")
}

/// Returns nothing. (Ignored nop)
pub fn _nop(_c: &mut Common, _ins: &Quad) -> Vec<String> {
    Vec::new()
}

/// Outputs an origin of code.
pub fn _org(_c: &mut Common, ins: &Quad) -> Vec<String> {
    vec![format!("org {}", ins.at(1))]
}

/// Exchange ALL registers. If the processor does not support this, it must
/// be implemented saving registers in a memory location.
pub fn _exchg(_c: &mut Common, _ins: &Quad) -> Vec<String> {
    strs(&["ex af, af'", "exx"])
}

/// Outputs the ending sequence
pub fn _end(c: &mut Common, ins: &Quad) -> Vec<String> {
    let mut output = Bits16::get_oper(ins.at(1), None, false);
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
    output.extend(strs(&["ld sp, hl", "exx", "pop hl", "exx", "pop iy", "pop ix", "ei", "ret"]));
    output
}

/// Defines a Label.
pub fn _label(_c: &mut Common, ins: &Quad) -> Vec<String> {
    vec![format!("{}:", ins.at(1))]
}

/// Defines a Label with a value.
pub fn _deflabel(_c: &mut Common, ins: &Quad) -> Vec<String> {
    vec![format!("{} EQU {}", ins.at(1), ins.at(2))]
}

/// Defines a data item (binary). It's just a constant expression to be
/// converted do binary data "as is".
///
/// 1st parameter is the type-size (u8 or i8 for byte, u16 or i16 for word,
/// etc). 2nd parameter is the list of expressions. All of them will be
/// converted to the type required.
pub fn _data(_c: &mut Common, ins: &Quad) -> Vec<String> {
    let mut output = Vec::new();
    let t = ins.at(1);
    let mut q = eval_list(ins.at(2));

    let size = match t {
        "i8" | "u8" => "B",
        "i16" | "u16" => "W",
        "i32" | "u32" => {
            // The Python code loops over `ins[2]`, the operand as text, and
            // not over the list read from it: each *character* of the list's
            // repr becomes an expression. Reproduced as it stands.
            let mut z = Vec::new();
            for expr in ins.at(2).chars() {
                z.push(format!("({expr}) & 0xFFFF"));
                z.push(format!("({expr}) >> 16"));
            }
            q = z;
            "W"
        }
        "str" => {
            q = q.iter().map(|x| format!("\"{}\"", x.replace('"', "\"\""))).collect();
            "B"
        }
        "f" => {
            for x in &q {
                let (c, de, hl) = Float::float(crate::common::float(x));
                output.push(format!("DEFB {c}"));
                output.push(format!("DEFW {de}, {hl}"));
            }
            return output;
        }
        _ => invalid_ic(ins, format!("Unimplemented data size {t} for {}", ins.at(2))),
    };

    for x in &q {
        output.push(format!("DEF{size} {x}"));
    }

    output
}

/// Defines a memory variable.
pub fn _var(_c: &mut Common, ins: &Quad) -> Vec<String> {
    let count = (int(ins.at(2)) - 1).max(0) as usize;
    vec![format!("{}:", ins.at(1)), format!("DEFB {}00", "00, ".repeat(count))]
}

/// Defines a memory space with a default CONSTANT expression.
/// 1st parameter is the var name, 2nd parameter is the type-size (u8 or i8
/// for byte, u16 or i16 for word, etc), 3rd parameter is the list of
/// expressions. All of them will be converted to the type required.
pub fn _varx(_c: &mut Common, ins: &Quad) -> Vec<String> {
    varx(ins.at(1), ins.at(2), ins.at(3), ins)
}

fn varx(name: &str, stype: &str, list: &str, ins: &Quad) -> Vec<String> {
    let mut output = vec![format!("{name}:")];
    let mut q = eval_list(list);

    let size = match stype {
        "i8" | "u8" => "B",
        "i16" | "u16" => "W",
        "i32" | "u32" => {
            let mut z = Vec::new();
            for expr in &q {
                z.push(format!("({expr}) & 0xFFFF"));
                z.push(format!("({expr}) >> 16"));
            }
            q = z;
            "W"
        }
        _ => invalid_ic(ins, format!("Unimplemented vard size: {stype}")),
    };

    for x in &q {
        output.push(format!("DEF{size} {x}"));
    }

    output
}

/// Defines a memory space with a default set of bytes/words in hexadecimal
/// (starting with a hex number) or literals (starting with #).
/// Numeric values with more than 2 digits represents a WORD (2 bytes) value.
/// E.g. '01' => 01h, '001' => 1, 0 bytes (0001h)
/// Literal values starts with # (1 byte) or ## (2 bytes)
/// E.g. '#label + 1' => (label + 1) & 0xFF
///      '##(label + 1)' => (label + 1) & 0xFFFF
pub fn _vard(_c: &mut Common, ins: &Quad) -> Vec<String> {
    vard(ins.at(1), ins.at(2))
}

fn vard(name: &str, list: &str) -> Vec<String> {
    let mut output = vec![format!("{name}:")];

    let q = eval_list(list);

    for x in &q {
        if first(x) == '#' {
            // literal?
            let size_t = if x.chars().nth(1).unwrap_or_else(|| panic!("IndexError: string index out of range")) == '#' { "W" } else { "B" };
            output.push(format!("DEF{size_t} {}", x.trim_start_matches('#')));
            continue;
        }

        // must be an hex number
        let mut x = x.to_uppercase();
        assert!(is_hexa(&x), "expected an hex number, got \"{x}\"");
        let size_t = if x.chars().count() <= 2 { "B" } else { "W" };
        if first(&x) > '9' {
            // Not a number?
            x = format!("0{x}");
        }
        output.push(format!("DEF{size_t} {x}h"));
    }

    output
}

/// Defines a local variable. 1st param is offset of the local variable.
/// 2nd param is the type a list of bytes in hexadecimal.
pub fn _lvarx(c: &mut Common, ins: &Quad) -> Vec<String> {
    let mut output = Vec::new();

    let l = eval_list(ins.at(3)); // List of bytes to push
    let label = c.tmp_label();
    let offset = int(ins.at(1));
    let at_end = varx(&label, ins.at(2), ins.at(3), ins);
    c.at_end.extend(at_end);

    output.push(format!("push {IDX_REG}"));
    output.push("pop hl".to_string());
    output.push(format!("ld bc, {}", -offset));
    output.push("add hl, bc".to_string());
    output.push("ex de, hl".to_string());
    output.push(format!("ld hl, {label}"));
    output.push(format!("ld bc, {}", l.len() as i64 * yy_type_size(ins.at(2))));
    output.push("ldir".to_string());

    output
}

/// Defines a local variable. 1st param is offset of the local variable.
/// 2nd param is a list of bytes in hexadecimal.
pub fn _lvard(c: &mut Common, ins: &Quad) -> Vec<String> {
    let mut output = Vec::new();

    let label = c.tmp_label();
    let offset = int(ins.at(1));
    c.at_end.extend(vard(&label, ins.at(2)));

    output.push(format!("push {IDX_REG}"));
    output.push("pop hl".to_string());
    output.push(format!("ld bc, {}", -offset));
    output.push("add hl, bc".to_string());
    output.push("ex de, hl".to_string());
    output.push(format!("ld hl, {label}"));
    output.push(format!("ld bc, {}", get_bytes_size(&eval_list(ins.at(2)))));
    output.push("ldir".to_string());

    output
}

/// Defines a local array.
/// - 1st param is offset of the local variable.
/// - 2nd param is a list of bytes in hexadecimal corresponding to the index table
/// - 3rd param is the size of elements in byte
/// - 4th param is a list (might be empty) of byte to initialize the array with
/// - 5th param is a list (might be empty or 2 elements) of [lbound, ubound] labels.
pub fn _larrd(c: &mut Common, ins: &Quad) -> Vec<String> {
    let mut output = Vec::new();

    let label = c.tmp_label();
    let offset = int(ins.at(1));
    let elements_size = ins.at(3);
    c.at_end.extend(vard(&label, ins.at(2)));

    let bounds = eval_list(ins.at(5));
    if bounds.len() != 0 && bounds.len() != 2 {
        invalid_ic(ins, format!("Bounds list length must be 0 or 2, not {}", ins.at(5)));
    }

    let have_bounds = !bounds.is_empty() && bounds.iter().any(|x| x != "0");
    if have_bounds {
        output.push(format!("ld hl, {}", bounds[1])); // UBOUND Table PTR
        output.push("push hl".to_string());
        output.push(format!("ld hl, {}", bounds[0])); // LBOUND Table PTR
        output.push("push hl".to_string());
    }

    let must_initialize = ins.at(4) != "[]";
    if must_initialize {
        let label2 = c.tmp_label();
        c.at_end.extend(vard(&label2, ins.at(4)));
        output.push(format!("ld hl, {label2}"));
        output.push("push hl".to_string());
    }

    output.push(format!("ld hl, {}", -offset));
    output.push(format!("ld de, {label}"));
    output.push(format!("ld bc, {elements_size}"));

    if must_initialize {
        if !have_bounds {
            output.push(c.runtime_call(runtime::ALLOC_INITIALIZED_LOCAL_ARRAY));
        } else {
            output.push(c.runtime_call(runtime::ALLOC_INITIALIZED_LOCAL_ARRAY_WITH_BOUNDS));
        }
    } else if !have_bounds {
        output.push(c.runtime_call(runtime::ALLOC_LOCAL_ARRAY));
    } else {
        output.push(c.runtime_call(runtime::ALLOC_LOCAL_ARRAY_WITH_BOUNDS));
    }

    output
}

/// Translates OUT to asm.
pub fn _out(_c: &mut Common, ins: &Quad) -> Vec<String> {
    let mut output = Bits8::get_oper(ins.at(2), None, false);
    output.extend(Bits16::get_oper(ins.at(1), None, false));
    output.extend(strs(&["ld b, h", "ld c, l", "out (c), a"]));

    output
}

/// Translates IN to asm.
pub fn _in(_c: &mut Common, ins: &Quad) -> Vec<String> {
    let mut output = Bits16::get_oper(ins.at(1), None, false);
    output.extend(strs(&["ld b, h", "ld c, l", "in a, (c)", "push af"]));

    output
}

/// Convert data from typeA to typeB (only numeric data types)
pub fn _cast(c: &mut Common, ins: &Quad) -> Vec<String> {
    // Signed and unsigned types are the same in the Z80
    let ta = ins.at(2); // From TypeA
    let tb = ins.at(3); // To TypeB

    let s_b = yy_type_size(tb); // Type sizes
    let mut xs_b = s_b;

    if matches!(ta, "u8" | "i8") && tb == "bool" {
        // bytes are booleans already (0 = False, not 0 = True)
        let mut output = Bits8::get_oper(ins.at(4), None, false);
        output.push("push af".to_string());
        return output;
    }

    let mut output = Vec::new();
    match ta {
        "u8" | "i8" | "bool" => output.extend(Bits8::get_oper(ins.at(4), None, false)),
        "u16" | "i16" => output.extend(Bits16::get_oper(ins.at(4), None, false)),
        "u32" | "i32" => output.extend(Bits32::get_oper(c, ins.at(4), None, false, false)),
        "f16" => output.extend(Fixed16::get_oper(c, ins.at(4), None, false, false)),
        "f" => output.extend(Float::get_oper(c, ins.at(4), None)),
        _ => panic!("Internal error: invalid typecast from {ta} to {tb}"),
    }

    match tb {
        "u8" | "i8" => output.extend(c.to_byte(ta)), // It was a byte
        "u16" | "i16" => output.extend(c.to_word(ta)),
        "u32" | "i32" => output.extend(c.to_long(ta)),
        "f16" => output.extend(c.to_fixed(ta)),
        "f" => output.extend(c.to_float(ta)),
        _ => panic!("Internal error: invalid typecast from {ta} to {tb}"),
    }

    xs_b += s_b % 2; // make it even (round up)

    if xs_b > 4 {
        output.extend(Float::fpush());
    } else {
        if xs_b > 2 {
            output.push("push de".to_string()); // Fixed or 32 bit Integer
        }

        if s_b > 1 {
            output.push("push hl".to_string()); // 16 bit Integer
        } else {
            output.push("push af".to_string()); // 8 bit Integer
        }
    }

    output
}

// ------------------- FLOW CONTROL instructions -------------------

/// Jump to a label
pub fn _jump(_c: &mut Common, ins: &Quad) -> Vec<String> {
    vec![format!("jp {}", ins.at(1))]
}

/// Returns from a procedure / function
pub fn _ret(_c: &mut Common, ins: &Quad) -> Vec<String> {
    vec![format!("jp {}", ins.at(1))]
}

/// Calls a function XXXX (or address XXXX)
/// 2nd parameter contains size of the returning result if any, and will be
/// pushed onto the stack.
pub fn _call(_c: &mut Common, ins: &Quad) -> Vec<String> {
    let mut output = vec![format!("call {}", ins.at(1))];

    // A size that is not a number is a ValueError the Python code swallows.
    if let Some(val) = parse_int(ins.at(2)) {
        if val == 1 {
            output.push("push af".to_string()); // Byte
        } else if val > 4 {
            output.extend(Float::fpush());
        } else {
            if val > 2 {
                output.push("push de".to_string());
            }
            if val > 1 {
                output.push("push hl".to_string());
            }
        }
    }

    output
}

/// Enter function sequence for doing a function start.
/// ins[1] contains size (in bytes) of local variables.
/// Use '__fastcall__' as 1st parameter to prepare a fastcall function (no
/// local variables).
pub fn _enter(_c: &mut Common, ins: &Quad) -> Vec<String> {
    let mut output = Vec::new();

    if ins.at(1) == "__fastcall__" {
        return output;
    }

    output.push(format!("push {IDX_REG}"));
    output.push(format!("ld {IDX_REG}, 0"));
    output.push(format!("add {IDX_REG}, sp"));

    let size_bytes = int(ins.at(1));

    if size_bytes != 0 {
        if size_bytes < 7 {
            output.push("ld hl, 0".to_string());
            for _ in 0..(size_bytes >> 1).max(0) {
                output.push("push hl".to_string());
            }

            if size_bytes.rem_euclid(2) != 0 {
                // odd?
                output.push("push hl".to_string());
                output.push("inc sp".to_string());
            }
        } else {
            output.push(format!("ld hl, -{size_bytes}")); // "Pushes nn bytes"
            output.push("add hl, sp".to_string());
            output.push("ld sp, hl".to_string());
            output.push("ld (hl), 0".to_string());
            output.push(format!("ld bc, {}", size_bytes - 1));
            output.push("ld d, h".to_string());
            output.push("ld e, l".to_string());
            output.push("inc de".to_string());
            output.push("ldir".to_string()); // Clear with ZEROs
        }
    }

    output
}

/// Return from a function popping N bytes from the stack.
/// Use '__fastcall__' as 1st parameter, to just return.
pub fn _leave(c: &mut Common, ins: &Quad) -> Vec<String> {
    let mut output = Vec::new();

    if ins.at(1) == "__fastcall__" {
        output.push("ret".to_string());
        return output;
    }

    let nbytes = int(ins.at(1)); // Number of bytes to pop (params size)

    if nbytes == 0 {
        output.push(format!("ld sp, {IDX_REG}"));
        output.push(format!("pop {IDX_REG}"));
        output.push("ret".to_string());

        return output;
    }

    if nbytes == 1 {
        output.push(format!("ld sp, {IDX_REG}"));
        output.push(format!("pop {IDX_REG}"));
        output.push("inc sp".to_string()); // "Pops" 1 byte
        output.push("ret".to_string());

        return output;
    }

    if nbytes <= 11 {
        // Number of bytes it worth the hassle to "pop" off the stack
        output.push(format!("ld sp, {IDX_REG}"));
        output.push(format!("pop {IDX_REG}"));
        output.push("exx".to_string());
        output.push("pop hl".to_string());
        for _ in 0..((nbytes >> 1) - 1).max(0) {
            output.push("pop bc".to_string()); // Removes (n * 2  - 2) bytes form the stack
        }

        if nbytes & 1 != 0 {
            // Odd?
            output.push("inc sp".to_string()); // "Pops" 1 byte (This should never happens, since params are always even-sized)
        }

        output.push("ex (sp), hl".to_string()); // Place back return address
        output.push("exx".to_string());
        output.push("ret".to_string());

        return output;
    }

    if !c.flag_use_function_exit {
        c.flag_use_function_exit = true; // Use standard exit
        output.push("exx".to_string());
        output.push(format!("ld hl, {nbytes}"));
        output.push("__EXIT_FUNCTION:".to_string());
        output.push(format!("ld sp, {IDX_REG}"));
        output.push(format!("pop {IDX_REG}"));
        output.extend(strs(&["pop de", "add hl, sp", "ld sp, hl", "push de", "exx", "ret"]));
    } else {
        output.push("exx".to_string());
        output.push(format!("ld hl, {nbytes}"));
        output.push("jp __EXIT_FUNCTION".to_string());
    }

    output
}

/// Copies a block of memory from param 2 addr to param 1 addr.
pub fn _memcopy(c: &mut Common, ins: &Quad) -> Vec<String> {
    let mut output = Bits16::get_oper(ins.at(3), None, false);
    output.push("ld b, h".to_string());
    output.push("ld c, l".to_string());
    // With two operands it may call the runtime, which has to be noted.
    output.extend(Bits16::get_oper_c(c, ins.at(1), Some(ins.at(2)), true));
    output.push("ldir".to_string()); // ***

    output
}

/// Inline code. The text does not go in the instruction stream: its lines
/// are tabulated, parked under a `##ASMn` key, and that one token is what
/// the stream carries, so the peephole optimiser sees the block as opaque.
pub fn _inline(c: &mut Common, ins: &Quad) -> Vec<String> {
    let mut tmp: Vec<String> = ins.at(1).split('\n').map(|x| x.trim_matches([' ', '\t', '\r']).to_string()).collect(); // Split lines

    for line in tmp.iter_mut() {
        if line.is_empty() || line.starts_with(';') {
            // a comment
            continue;
        }

        if line.starts_with('#') {
            // A preprocessor directive
            continue;
        }

        if re_label().match_str(line).is_none() {
            *line = format!("\t{line}");
        }
        // It starts with a label. Do not tabulate
    }

    // `str.split` never gives an empty list, so there is always a block.
    let asmlabel = c.new_asmid();
    c.asms.insert(asmlabel.clone(), tmp);
    vec![asmlabel]
}
