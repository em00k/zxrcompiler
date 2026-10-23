//! `optimizer/memcell.py` as the flow optimiser uses it: one instruction of
//! a basic block.
//!
//! In Python `inst`, `opers`, `branch_arg`, `destroys` and `requires` are
//! `cached_property`s, and assigning a new instruction to a cell (`cell.asm
//! = ...`: jump threading does, and the renaming of LOCAL labels) does not
//! forget them. A cell whose jump has been redirected to `(hl)`, say, still
//! answers with the operands, and so the register needs, of the jump it was
//! when it was first asked. That is reproduced: the caches here are filled
//! on first use and never cleared.

use crate::asm::Asm;
use crate::helpers;
use crate::memcell::{destroys_of, requires_of};
use std::cell::OnceCell;
use std::collections::BTreeSet;
use std::sync::OnceLock;
use zxr_re::{Flags, Regex};

/// What a cell has to know about the compile.
pub struct Env<'a> {
    /// `line in ASMS`: whether a line stands for a parked block of inline
    /// assembly.
    pub is_asm: &'a dyn Fn(&str) -> bool,
    /// `FAR_TRAMPOLINES.get(label)`: the body a banked routine's resident
    /// trampoline calls.
    pub far_body: &'a dyn Fn(&str) -> Option<String>,
    /// The architecture is the ZX Spectrum Next: its `MemCell`, `CPUState`
    /// and `BasicBlock` are used, and its lexer's keywords.
    pub zxnext: bool,
    /// `OPTIONS.optimization_level`
    pub level: i64,
}

#[derive(Clone, Debug)]
pub struct Cell {
    pub addr: usize,
    pub asm: Asm,
    inst: OnceCell<String>,
    opers: OnceCell<Vec<String>>,
    branch_arg: OnceCell<Option<String>>,
    destroys: OnceCell<BTreeSet<String>>,
    requires: OnceCell<BTreeSet<String>>,
}

impl Cell {
    pub fn new(instr: &str, addr: usize) -> Cell {
        Cell { addr, asm: Asm::new(instr), inst: OnceCell::new(), opers: OnceCell::new(), branch_arg: OnceCell::new(), destroys: OnceCell::new(), requires: OnceCell::new() }
    }

    /// `cell.asm = text`. What has been asked of the cell before stays as
    /// it was answered.
    pub fn set_asm(&mut self, text: &str) {
        self.asm = Asm::new(text);
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
        self.inst.get_or_init(|| if self.asm.is_label { self.asm.asm[..self.asm.asm.len() - 1].to_string() } else { self.asm.inst.clone() })
    }

    /// Returns the flag this instruction uses or None.
    pub fn condition_flag(&self) -> Option<&str> {
        self.asm.cond.as_deref()
    }

    /// Returns a list of operands (i.e. register) this mnemonic uses
    pub fn opers(&self) -> &[String] {
        self.opers.get_or_init(|| self.asm.oper.clone())
    }

    pub fn branch_arg(&self) -> Option<&str> {
        self.branch_arg
            .get_or_init(|| {
                if !matches!(self.asm.inst.as_str(), "jr" | "jp" | "call" | "rst" | "djnz") {
                    return None;
                }
                Some(py_split(&self.asm.asm).last().unwrap_or_else(|| panic!("IndexError: list index out of range")).to_string())
            })
            .as_deref()
    }

    /// Returns which single registers (including f, flag) this instruction
    /// changes.
    pub fn destroys(&self, env: &Env) -> &BTreeSet<String> {
        self.destroys.get_or_init(|| {
            if env.zxnext {
                if self.is_label() {
                    return BTreeSet::new();
                }

                if self.inst() == "mul" {
                    return ["d", "e"].iter().map(|x| x.to_string()).collect();
                }
            }
            destroys_of(self.inst(), self.opers(), (env.is_asm)(self.code()))
        })
    }

    /// Returns the registers, operands, etc. required by an instruction.
    pub fn requires(&self, env: &Env) -> &BTreeSet<String> {
        self.requires.get_or_init(|| {
            if env.zxnext && self.inst() == "mul" {
                return ["d", "e"].iter().map(|x| x.to_string()).collect();
            }
            requires_of(self.inst(), self.opers(), self.code(), self.condition_flag().is_some(), (env.is_asm)(self.code()))
        })
    }

    /// Returns a list of required labels for this instruction: the
    /// identifiers the assembler's lexer finds in it.
    pub fn used_labels(&self, env: &Env) -> Vec<String> {
        let tmp = self.code();
        if tmp.is_empty() || tmp.starts_with('#') || tmp.starts_with(';') {
            return Vec::new();
        }

        asm_identifiers(tmp, env.zxnext)
    }

    /// Replaces old label with a new one
    pub fn replace_label(&mut self, old_label: &str, new_label: &str) {
        if old_label == new_label {
            return;
        }

        // The label is used as a regular expression, as it is in Python.
        let re = Regex::new(&format!(r"\b{old_label}\b"), Flags::NONE).unwrap_or_else(|e| panic!("re.error: {}", e.0));
        let chars: Vec<char> = self.code().chars().collect();
        let mut out = String::new();
        let mut pos = 0;
        while pos <= chars.len() {
            match re.search(&chars, pos) {
                Some(m) => {
                    out.extend(&chars[pos..m.start]);
                    out.push_str(new_label);
                    if m.end == m.start {
                        if let Some(c) = chars.get(m.end) {
                            out.push(*c);
                        }
                        pos = m.end + 1;
                    } else {
                        pos = m.end;
                    }
                }
                None => {
                    out.extend(&chars[pos.min(chars.len())..]);
                    break;
                }
            }
        }
        self.set_asm(&out);
    }
}

/// `str.split()`
fn py_split(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| c.is_whitespace() || ('\x1c'..='\x1f').contains(&c)).filter(|w| !w.is_empty())
}

/// `RE_ID_OR_NUMBER.findall`: `[0-9]+|[a-zA-Z_][a-zA-Z_0-9]*`
pub fn ids_or_numbers(text: &str) -> Vec<&str> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let from = i;
        if b[i].is_ascii_digit() {
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            out.push(&text[from..i]);
        } else if b[i].is_ascii_alphabetic() || b[i] == b'_' {
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            out.push(&text[from..i]);
        } else {
            i += 1;
        }
    }
    out
}

/// The identifiers of an instruction as `asmlex` would give them: what is
/// left of its ID tokens once mnemonics, pseudo-operations, registers and
/// flags are put aside. The lexer's rules are tried in its order
/// (`grammar/zxbasm.lex`); only those that can swallow something that looks
/// like an identifier are spelt out, every other token being one character
/// that cannot start one.
fn asm_identifiers(text: &str, zxnext: bool) -> Vec<String> {
    static RULES: OnceLock<Vec<(Regex, bool)>> = OnceLock::new();
    let rules = RULES.get_or_init(|| {
        [
            (r"[ \t]+", false),
            (r"'.'", false),
            (r"([0-9](_?[0-9a-fA-F])*[hH])|(\$[0-9a-fA-F](_?[0-9a-fA-F])*)|(0x[0-9a-fA-F](_?[0-9a-fA-F])*)", false),
            (r"(%[01](_?[01])*)|(0[bB](_?[01])+)", false),
            (r"[0-9]+[FfBb]", false),
            (r"[0-9](_?\d)*", false),
            (r"[._a-zA-Z][._a-zA-Z0-9]*", true),
            (r#""(""|[^"])*""#, false),
            (r";.*", false),
        ]
        .into_iter()
        .map(|(pattern, id)| (Regex::new(pattern, Flags::NONE).unwrap(), id))
        .collect()
    });
    const WORDS: &str = " adc add and bit call ccf cp cpd cpdr cpi cpir cpl daa dec di djnz ei ex exx halt im in inc ind indr ini inir jp jr ld ldd lddr ldi ldir neg nop or otdr otir out outd outi pop push res ret reti retn rl rla rlc rlca rld rr rra rrc rrca rrd rst sbc scf set sla sll sra srl sub xor \
        align org defb defm db defs ds defw dw equ proc endp local end incbin namespace \
        a b c d e h l i r ixh ixl iyh iyl z nz nc po pe p m af bc de hl ix iy sp ";
    const NEXT: &str = " ldix ldws ldirx lddx lddrx ldpirx outinb mul swapnib mirror nextreg pixeldn pixelad setae test bsla bsra bsrl bsrf brlc codebank ";

    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut pos = 0;
    'scan: while pos < chars.len() {
        for (re, id) in rules {
            if let Some(m) = re.match_at(&chars, pos).filter(|m| m.end > pos) {
                if *id {
                    let word: String = chars[pos..m.end].iter().collect();
                    let key = format!(" {} ", word.to_lowercase());
                    if !WORDS.contains(&key) && !(zxnext && NEXT.contains(&key)) {
                        out.push(word);
                    }
                }
                pos = m.end;
                continue 'scan;
            }
        }
        pos += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers() {
        assert_eq!(asm_identifiers("ld hl, (_a + 2)", false), ["_a"]);
        assert_eq!(asm_identifiers("jp nz, .LABEL.x ; note", false), [".LABEL.x"]);
        assert_eq!(asm_identifiers("ld a, 0FFh", false), Vec::<String>::new());
        assert_eq!(asm_identifiers("mul d, e", false), ["mul"]);
        assert_eq!(asm_identifiers("mul d, e", true), Vec::<String>::new());
        assert_eq!(ids_or_numbers("(_k - 1)"), ["_k", "1"]);
    }
}
