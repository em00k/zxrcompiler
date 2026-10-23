//! `optimizer/asm.py`: an assembly instruction as the optimisers see it,
//! taken apart with string operations rather than parsed.

use crate::helpers::single_registers;

include!(concat!(env!("OUT_DIR"), "/z80instr.rs"));

/// Python's `str.strip()` with no argument.
pub fn py_strip(text: &str) -> &str {
    text.trim_matches(|c: char| c.is_whitespace() || ('\x1c'..='\x1f').contains(&c))
}

/// `RE_INDIR16.match`: `[ \t]*\([ \t]*([dD][eE]|[hH][lL])[ \t]*\)[ \t]*` at
/// the start of the text.
fn is_indir16(text: &str) -> bool {
    let blank = |s: &str| s.trim_start_matches([' ', '\t']).len();
    let s = &text[text.len() - blank(text)..];
    let Some(s) = s.strip_prefix('(') else { return false };
    let s = &s[s.len() - blank(s)..];
    let Some(reg) = s.get(..2) else { return false };
    if !reg.eq_ignore_ascii_case("de") && !reg.eq_ignore_ascii_case("hl") {
        return false;
    }
    let s = &s[2..];
    s[s.len() - blank(s)..].starts_with(')')
}

/// `RE_OUTC.match`: `[ \t]*\([ \t]*[cC]\)` at the start of the text.
fn is_outc(text: &str) -> bool {
    let s = text.trim_start_matches([' ', '\t']);
    let Some(s) = s.strip_prefix('(') else { return false };
    let s = s.trim_start_matches([' ', '\t']);
    s.starts_with("c)") || s.starts_with("C)")
}

/// Defines an asm instruction
#[derive(Clone, Debug, PartialEq)]
pub struct Asm {
    pub inst: String,
    pub oper: Vec<String>,
    pub asm: String,
    pub cond: Option<String>,
    pub is_label: bool,
}

impl Asm {
    pub fn new(asm: &str) -> Asm {
        let asm = py_strip(asm);
        assert!(!asm.is_empty(), "AssertionError: Empty instruction ''");
        let inst = Asm::instruction(asm);
        let rest = asm.split_once(' ').map_or("", |(_, rest)| rest);
        let text = py_strip(&format!("{inst} {rest}")).to_string();
        let is_label = inst.ends_with(':');
        Asm { oper: Asm::opers(asm), asm: text, cond: Asm::condition(asm), inst, is_label }
    }

    pub fn instruction(asm: &str) -> String {
        let tmp = asm.trim_matches([' ', '\t', '\n']).split(' ').next().unwrap_or("");
        if Z80INSTR.binary_search(&tmp.to_uppercase().as_str()).is_ok() { tmp.to_lowercase() } else { tmp.to_string() }
    }

    /// Returns operands of an ASM instruction. Even "indirect" operands,
    /// like SP if RET or CALL is used.
    pub fn opers(inst: &str) -> Vec<String> {
        let padded = format!("{} ", inst.trim_matches([' ', '\t', '\n']));
        let (car, cdr) = padded.split_once(' ').unwrap();
        let i = car.to_lowercase(); // Instruction
        let i = i.as_str();
        let mut op: Vec<String> = cdr.split(',').map(|x| py_strip(x).to_string()).collect();
        let s = |x: &str| x.to_string();

        if matches!(i, "call" | "jp" | "jr") && op.len() > 1 {
            op.remove(0);
            op.push(s("f"));
        } else if i == "djnz" {
            op.push(s("b"));
        } else if matches!(i, "push" | "pop" | "call") {
            op.push(s("sp")); // Sp is also affected by push, pop and call
        } else if matches!(i, "or" | "and" | "xor" | "neg" | "cpl" | "rrca" | "rlca") {
            op.push(s("a"));
        } else if matches!(i, "rra" | "rla") {
            op.push(s("a"));
            op.push(s("f"));
        } else if matches!(i, "rr" | "rl") {
            op.push(s("f"));
        } else if matches!(i, "adc" | "sbc") {
            if op.len() == 1 {
                op.insert(0, s("f"));
                op.insert(0, s("a"));
            }
        } else if matches!(i, "add" | "sub") {
            if op.len() == 1 {
                op.insert(0, s("a"));
            }
        } else if matches!(i, "ldd" | "ldi" | "lddr" | "ldir") {
            op = vec![s("hl"), s("de"), s("bc")];
        } else if matches!(i, "cpd" | "cpi" | "cpdr" | "cpir") {
            op = vec![s("a"), s("hl"), s("bc")];
        } else if i == "exx" {
            op = ["*", "bc", "de", "hl", "b", "c", "d", "e", "h", "l"].iter().map(|x| s(x)).collect();
        } else if matches!(i, "ret" | "reti" | "retn") {
            op.push(s("sp"));
        } else if i == "out" {
            if !op.is_empty() && is_outc(&op[0]) {
                op[0] = s("c");
            } else {
                op.remove(0);
            }
        } else if i == "in" {
            if op.len() > 1 && is_outc(&op[1]) {
                op[1] = s("c");
            } else {
                assert!(op.len() > 1, "IndexError: pop index out of range");
                op.remove(1);
            }
        }

        for o in op.iter_mut() {
            if is_indir16(o) {
                // '  (  dE )  ' => '(de)'
                let stripped: Vec<char> = py_strip(o).chars().collect();
                let inner: String = if stripped.len() >= 2 { stripped[1..stripped.len() - 1].iter().collect() } else { String::new() };
                *o = format!("({})", py_strip(&inner).to_lowercase());
            }
        }

        op
    }

    /// Returns the flag this instruction uses or None. E.g. 'c' for Carry,
    /// 'nz' for not-zero, etc. That is the condition required for this
    /// instruction to execute. For example: ADC A, 0 does NOT have a
    /// condition flag (it always execute) whilst RET C does. DJNZ has
    /// condition flag NZ
    pub fn condition(asm: &str) -> Option<String> {
        let i = Asm::instruction(asm);

        if !matches!(i.as_str(), "call" | "jp" | "jr" | "ret" | "djnz") {
            return None; // This instruction always execute
        }

        if i == "ret" {
            let words: Vec<String> = asm.split(' ').filter(|x| !x.is_empty()).map(str::to_lowercase).collect();
            return words.get(1).cloned();
        }

        if i == "djnz" {
            return Some("nz".to_string());
        }

        let head = py_strip(asm.split(',').next().unwrap_or(""));
        let words: Vec<String> = head.split(' ').filter(|x| !x.is_empty()).map(str::to_lowercase).collect();
        if words.len() > 1 && matches!(words[1].as_str(), "c" | "nc" | "z" | "nz" | "po" | "pe" | "p" | "m") {
            return Some(words[1].clone());
        }

        None
    }

    /// Returns which 8-bit registers (and SP for INC SP, DEC SP, etc.) are
    /// used by an asm instruction to return a result.
    pub fn result(asm: &str) -> Vec<String> {
        let ins = Asm::instruction(asm);
        let ins = ins.as_str();
        let op = Asm::opers(asm);
        let v = |items: &[&str]| items.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        let first = || op.first().cloned().unwrap_or_else(|| panic!("IndexError: list index out of range"));

        if matches!(ins, "or" | "and") && op == ["a"] {
            return v(&["f"]);
        }

        if matches!(ins, "xor" | "or" | "and" | "neg" | "cpl" | "daa" | "rld" | "rrd" | "rra" | "rla" | "rrca" | "rlca") {
            return v(&["a", "f"]);
        }

        if matches!(ins, "bit" | "cp" | "scf" | "ccf") {
            return v(&["f"]);
        }

        if matches!(ins, "sub" | "add" | "sbc" | "adc") {
            if op.len() == 1 {
                return v(&["a", "f"]);
            }
            let mut r = single_registers(&[first()]);
            r.push("f".to_string());
            return r;
        }

        if ins == "djnz" {
            return v(&["b", "f"]);
        }

        if matches!(ins, "ldir" | "ldi" | "lddr" | "ldd") {
            return v(&["f", "b", "c", "d", "e", "h", "l"]);
        }

        if matches!(ins, "cpi" | "cpir" | "cpd" | "cpdr") {
            return v(&["f", "b", "c", "h", "l"]);
        }

        if matches!(ins, "pop" | "ld") {
            return single_registers(&[first()]);
        }

        if matches!(ins, "inc" | "dec" | "sbc" | "rr" | "rl" | "rrc" | "rlc") {
            let mut r = v(&["f"]);
            r.extend(single_registers(&[first()]));
            return r;
        }

        if matches!(ins, "set" | "res") {
            return single_registers(&[op.get(1).cloned().unwrap_or_else(|| panic!("IndexError: list index out of range"))]);
        }

        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::Asm;

    #[test]
    fn operands() {
        // Asm.opers(x) in the Python compiler.
        let t = |asm: &str, want: &[&str]| assert_eq!(Asm::opers(asm), want, "{asm}");
        t("ld a, (hl)", &["a", "(hl)"]);
        t("ld a, ( HL )", &["a", "(hl)"]);
        t("jp nz, .LABEL.x", &[".LABEL.x", "f"]);
        t("jp .LABEL.x", &[".LABEL.x"]);
        t("call .core.__X", &[".core.__X", "sp"]);
        t("ret", &["", "sp"]);
        t("push af", &["af", "sp"]);
        t("sbc a", &["a", "f", "a"]);
        t("add a, 5", &["a", "5"]);
        t("sub 5", &["a", "5"]);
        t("out (c), a", &["c", "a"]);
        t("out (254), a", &["a"]);
        t("in a, (c)", &["a", "c"]);
        t("in a, (254)", &["a"]);
        t("ldir", &["hl", "de", "bc"]);
        t("xor a", &["a", "a"]);
        t(".LABEL.x:", &[""]);
        t("##ASM3", &[""]);
    }

    /// `tests/arch/zx48k/optimizer/test_optimizer_asm.py`. (`Asm.bytes`,
    /// which only the debug trace shows, is not ported.)
    #[test]
    fn spaces() {
        assert_eq!(Asm::new("   nop  \t\n").asm, "nop");
    }

    #[test]
    #[should_panic(expected = "Empty instruction")]
    fn raises_error_on_empty_instruction() {
        Asm::new("  \t \n");
    }
}
