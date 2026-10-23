//! `optimizer/memcell.py`: what one instruction needs and what it destroys.
//! The peephole rules ask through `NEEDS` and `CTEST`; the flow optimiser
//! builds its liveness analysis on the same answers.

use crate::asm::Asm;
use crate::helpers::{self, single_registers};
use std::collections::BTreeSet;

/// Class describing a memory address. It just contains the addr (memory
/// array index), and the instruction.
#[derive(Clone, Debug)]
pub struct MemCell {
    pub addr: usize,
    pub asm: Asm,
}

fn set(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|x| x.to_string()).collect()
}

impl MemCell {
    pub fn new(instr: &str, addr: usize) -> MemCell {
        MemCell { addr, asm: Asm::new(instr) }
    }

    pub fn code(&self) -> &str {
        &self.asm.asm
    }

    /// Returns whether the current addr contains a label.
    pub fn is_label(&self) -> bool {
        self.asm.is_label
    }

    /// Returns if this instruction is a BLOCK ender
    pub fn is_ender(&self) -> bool {
        helpers::BLOCK_ENDERS.contains(&self.inst())
    }

    /// Returns just the asm instruction in lower case. E.g. 'ld', 'jp',
    /// 'pop'. For a label, its name.
    pub fn inst(&self) -> &str {
        if self.is_label() {
            let asm = &self.asm.asm;
            return &asm[..asm.len() - 1];
        }
        &self.asm.inst
    }

    /// Returns the flag this instruction uses or None. E.g. 'c' for Carry,
    /// 'nz' for not-zero, etc.
    pub fn condition_flag(&self) -> Option<&str> {
        self.asm.cond.as_deref()
    }

    /// Returns a list of operands (i.e. register) this mnemonic uses
    pub fn opers(&self) -> &[String] {
        &self.asm.oper
    }

    pub fn branch_arg(&self) -> Option<&str> {
        if !matches!(self.asm.inst.as_str(), "jr" | "jp" | "call" | "rst" | "djnz") {
            return None;
        }
        self.asm.asm.split_whitespace().last()
    }

    /// Returns which single registers (including f, flag) this instruction
    /// changes. `in_asms` says whether the line is a parked block of inline
    /// assembly (`self.code in ASMS`), which destroys everything.
    ///
    /// Registers are: a, b, c, d, e, i, h, l, ixh, ixl, iyh, iyl, r
    ///
    /// LD a, X => Destroys a
    /// LD a, a => Destroys nothing
    /// INC a => Destroys a, f
    /// POP af => Destroys a, f, sp
    /// PUSH af => Destroys sp
    /// ret => Destroys SP
    pub fn destroys(&self, in_asms: bool) -> BTreeSet<String> {
        destroys_of(self.inst(), self.opers(), in_asms)
    }

    /// Returns the registers, operands, etc. required by an instruction.
    pub fn requires(&self, in_asms: bool) -> BTreeSet<String> {
        requires_of(self.inst(), self.opers(), self.code(), self.condition_flag().is_some(), in_asms)
    }

    /// Returns if this instruction affects any of the registers in reglist.
    pub fn affects<S: AsRef<str>>(&self, reglist: &[S], in_asms: bool) -> bool {
        let reglist = single_registers(reglist);
        // An empty register name is false in Python's `any(x for x in ...)`.
        self.destroys(in_asms).iter().any(|x| !x.is_empty() && reglist.contains(x))
    }

    /// Returns if this instruction need any of the registers in reglist.
    pub fn needs<S: AsRef<str>>(&self, reglist: &[S], in_asms: bool) -> bool {
        let reglist = single_registers(reglist);
        self.requires(in_asms).iter().any(|x| !x.is_empty() && reglist.contains(x))
    }
}

/// `MemCell.destroys`, given the instruction (or the label's name) and its
/// operands. The flow optimiser keeps both cached past a change of the
/// cell's text, as the Python `cached_property` does.
pub fn destroys_of(i: &str, o: &[String], in_asms: bool) -> BTreeSet<String> {
    if in_asms {
        return set(&helpers::ALL_REGS);
    }

    let mut res = BTreeSet::new();
    let oper = |n: usize| -> &str { o.get(n).map(String::as_str).unwrap_or_else(|| panic!("IndexError: list index out of range")) };
    let first = || single_registers(&o[..o.len().min(1)]);

    if matches!(i, "push" | "ret" | "call" | "rst" | "reti" | "retn") {
        return set(&["sp"]);
    }

    if i == "pop" {
        // `res.update("sp", ...)` adds the *letters* of "sp".
        res.extend(set(&["s", "p"]));
        res.extend(first());
    } else if matches!(i, "ldir" | "lddr" | "ldd" | "ldi") {
        res.extend(set(&["b", "c", "d", "e", "h", "l", "f"]));
    } else if matches!(i, "otir" | "otdr" | "oti" | "otd" | "inir" | "indr" | "ini" | "ind") {
        res.extend(set(&["h", "l", "b"]));
    } else if matches!(i, "cpir" | "cpi" | "cpdr" | "cpd") {
        res.extend(set(&["h", "l", "b", "c", "f"]));
    } else if matches!(i, "ld" | "in") {
        res.extend(first());
    } else if matches!(i, "inc" | "dec") {
        res.insert("f".to_string());
        res.extend(first());
    } else if i == "exx" {
        res.extend(set(&["b", "c", "d", "e", "h", "l"]));
    } else if i == "ex" {
        res.extend(single_registers(&[oper(0)]));
        res.extend(single_registers(&[oper(1)]));
    } else if matches!(i, "ccf" | "scf" | "bit" | "cp") {
        res.insert("f".to_string());
    } else if matches!(i, "or" | "and") {
        res.insert("f".to_string());
        if oper(0) != "a" {
            res.insert("a".to_string());
        }
    } else if matches!(i, "xor" | "add" | "adc" | "sub" | "sbc") {
        if o.len() > 1 {
            res.extend(single_registers(&[oper(0)]));
        } else {
            res.insert("a".to_string());
        }
        res.insert("f".to_string());
    } else if matches!(i, "neg" | "cpl" | "daa" | "rra" | "rla" | "rrca" | "rlca" | "rrd" | "rld") {
        res.extend(set(&["a", "f"]));
    } else if i == "djnz" {
        res.extend(set(&["b", "f"]));
    } else if matches!(i, "rr" | "rl" | "rrc" | "rlc" | "srl" | "sra" | "sll" | "sla") {
        res.extend(single_registers(&[oper(0)]));
        res.insert("f".to_string());
    } else if matches!(i, "set" | "res") {
        res.extend(single_registers(&[oper(1)]));
    }

    res
}

/// `MemCell.requires`, likewise.
pub fn requires_of(i: &str, opers: &[String], code: &str, has_condition: bool, in_asms: bool) -> BTreeSet<String> {
    if in_asms {
        return set(&helpers::ALL_REGS);
    }

    if i == "#pragma" {
        let tmp: Vec<&str> = code.split(' ').skip(1).collect();
        if tmp.first().copied().unwrap_or_else(|| panic!("IndexError: list index out of range")) != "opt" {
            return BTreeSet::new();
        }
        if tmp.get(1).copied().unwrap_or_else(|| panic!("IndexError: list index out of range")) == "require" {
            return tmp[2..].iter().flat_map(|x| single_registers(&[x.trim_matches([',', ' ', '\t', '\r'])])).collect();
        }

        return BTreeSet::new();
    }

    let mut result: BTreeSet<String> = BTreeSet::new();
    let o: Vec<String> = opers.iter().map(|x| x.to_lowercase()).collect();
    let oper = |n: usize| -> &str { o.get(n).map(String::as_str).unwrap_or_else(|| panic!("IndexError: list index out of range")) };
    // `o[0][0] != "("`, an IndexError on an empty operand.
    let not_paren = |n: usize| oper(n).chars().next().unwrap_or_else(|| panic!("IndexError: string index out of range")) != '(';
    let add = |result: &mut BTreeSet<String>, items: &[&str]| result.extend(items.iter().map(|x| x.to_string()));

    if matches!(i, "ret" | "pop" | "push") {
        add(&mut result, &["sp"]);
    }

    if has_condition || matches!(i, "sbc" | "adc") {
        add(&mut result, &["f"]);
    }

    for op in &o {
        if op.contains("(hl)") {
            add(&mut result, &["h", "l"]);
        }

        if op.contains("(de)") {
            add(&mut result, &["d", "e"]);
        }

        if op.contains("(bc)") {
            add(&mut result, &["b", "c"]);
        }

        if op.contains("(sp)") {
            add(&mut result, &["sp"]);
        }

        if op.contains("(ix") {
            add(&mut result, &["ixh", "ixl"]);
        }

        if op.contains("(iy") {
            add(&mut result, &["iyh", "iyl"]);
        }
    }

    if i == "ccf" {
        add(&mut result, &["f"]);
    } else if matches!(i, "rra" | "rla" | "rrca" | "rlca") {
        add(&mut result, &["a", "f"]);
    } else if matches!(i, "xor" | "cp") {
        // XOR A, and CP A don't need the a register
        if oper(0) != "a" {
            add(&mut result, &["a"]);

            if not_paren(0) && !helpers::is_number(oper(0)) {
                result.extend(single_registers(&o));
            }
        }
    } else if matches!(i, "or" | "and") {
        // AND A, and OR A do need the a register to compute Z flag
        add(&mut result, &["a"]);

        if not_paren(0) && !helpers::is_number(oper(0)) {
            result.extend(single_registers(&o));
        }
    } else if matches!(i, "adc" | "sbc" | "add" | "sub") {
        if o.len() == 1 {
            if !matches!(i, "sub" | "sbc") || oper(0) != "a" {
                // sbc a and sub a dont' need the a register
                add(&mut result, &["a"]);
            }

            if not_paren(0) && !helpers::is_number(oper(0)) {
                result.extend(single_registers(&o));
            }
        } else if oper(0) != oper(1) || matches!(i, "add" | "adc") {
            // sub HL, HL or sub X, X don't need the X register(s)
            result.extend(single_registers(&o));
        }

        if matches!(i, "adc" | "sbc") {
            add(&mut result, &["f"]);
        }
    } else if matches!(i, "daa" | "rld" | "rrd" | "neg" | "cpl") {
        add(&mut result, &["a"]);
    } else if matches!(i, "rl" | "rr" | "rlc" | "rrc") {
        result.extend(single_registers(&o));
        add(&mut result, &["f"]);
    } else if matches!(i, "sla" | "sll" | "sra" | "srl" | "inc" | "dec") {
        result.extend(single_registers(&o));
    } else if i == "djnz" {
        add(&mut result, &["b"]);
    } else if matches!(i, "ldir" | "lddr" | "ldi" | "ldd") {
        add(&mut result, &["b", "c", "d", "e", "h", "l"]);
    } else if matches!(i, "cpi" | "cpd" | "cpir" | "cpdr") {
        add(&mut result, &["a", "b", "c", "h", "l"]);
    } else if i == "ld" && !helpers::is_number(oper(1)) {
        result.extend(single_registers(&[oper(1)]));
    } else if i == "ex" {
        if oper(0) == "de" {
            add(&mut result, &["d", "e", "h", "l"]);
        } else if oper(1) == "(sp)" {
            add(&mut result, &["h", "l"]); // sp already included
        } else {
            add(&mut result, &["a", "f", "a'", "f'"]);
        }
    } else if i == "exx" {
        add(&mut result, &["b", "c", "d", "e", "h", "l"]);
    } else if i == "push" {
        result.extend(single_registers(&o));
    } else if matches!(i, "bit" | "set" | "res") {
        result.extend(single_registers(&[oper(1)]));
    } else if i == "out" {
        result.insert(oper(1).to_string());
        if oper(0) == "c" {
            add(&mut result, &["b", "c"]);
        }
    } else if i == "in" {
        if oper(1) == "c" {
            add(&mut result, &["b", "c"]);
        }
    } else if i == "im" {
        add(&mut result, &["i"]);
    }

    result
}
