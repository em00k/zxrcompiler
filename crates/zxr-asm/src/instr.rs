//! One assembled instruction or data directive.

use crate::expr::{self, Expr, Label};
use std::rc::Rc;
use std::collections::HashMap;
use std::sync::OnceLock;
use zxr_core::Session;

pub struct Opcode {
    pub key: &'static str,
    pub tstates: u16,
    pub size: u8,
    /// Hex bytes separated by spaces; `XX` is an operand byte.
    pub encoding: &'static str,
}

include!(concat!(env!("OUT_DIR"), "/opcodes.rs"));

fn opcode(mnemonic: &str) -> Option<&'static Opcode> {
    static INDEX: OnceLock<HashMap<&'static str, &'static Opcode>> = OnceLock::new();
    INDEX.get_or_init(|| OPCODES.iter().map(|o| (o.key, o)).collect()).get(mnemonic).copied()
}

/// The widths in bytes of the operands in an instruction pattern: each run
/// of `N` standing as a word of its own (`LD (IX+N),NN` has 1 and 2).
fn operand_widths(pattern: &str) -> Vec<usize> {
    let chars: Vec<char> = pattern.chars().collect();
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let mut widths = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == 'N' && (i == 0 || !word(chars[i - 1])) {
            let start = i;
            while i < chars.len() && chars[i] == 'N' {
                i += 1;
            }
            if i == chars.len() || !word(chars[i]) {
                widths.push(i - start);
            }
        } else {
            i += 1;
        }
    }
    widths
}

#[derive(Clone, Debug)]
pub enum Arg {
    Expr(Expr),
    Int(i128),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Opcode,
    Defb,
    Defs,
    Defw,
}

pub struct Asm {
    pub lineno: u32,
    /// The file it was read from; set when it is added to the memory.
    pub filename: Rc<str>,
    /// The source line it was written on or generated for, for debug data;
    /// `None` until the parser has said, then perhaps no line at all.
    pub source: Option<Option<(Rc<str>, u32)>>,
    /// The instruction pattern, e.g. `LD A,N`, or the directive name.
    pub asm: String,
    pub kind: Kind,
    opcode: Option<&'static Opcode>,
    widths: Vec<usize>,
    pub args: Vec<Arg>,
    /// Operand values once known; `None` for one that could not be had.
    values: Vec<Option<i128>>,
    /// Raw bytes of an INCBIN.
    blob: Option<Vec<u8>>,
    /// Whether the operands still wait for a label.
    pub pending: bool,
    pub label_refs: Vec<Label>,
}

fn exprs_of(args: &[Arg]) -> Vec<Expr> {
    args.iter().filter_map(|a| if let Arg::Expr(e) = a { Some(e.clone()) } else { None }).collect()
}

impl Asm {
    /// An instruction. `None` (with an error reported) if the pattern is not
    /// one the processor has.
    pub fn instruction(sess: &mut Session, lineno: u32, asm: &str, args: Vec<Arg>) -> Option<Asm> {
        let Some(op) = opcode(&asm.to_uppercase()) else {
            sess.error(lineno, &format!("Invalid mnemonic '{asm}'"), None);
            return None;
        };
        let mut pending = false;
        for arg in &args {
            if let Arg::Expr(e) = arg {
                pending |= expr::eval(e, false, sess).is_none();
            }
        }
        let label_refs = expr::collect_labels(&exprs_of(&args));
        let mut this = Asm { lineno, filename: Rc::from(""), source: None, asm: asm.to_string(), kind: Kind::Opcode, opcode: Some(op), widths: operand_widths(asm), args, values: Vec::new(), blob: None, pending, label_refs };
        if !pending {
            this.resolve(sess);
        }
        Some(this)
    }

    /// `DEFB`, `DEFS` or `DEFW`. Their values are always worked out when the
    /// memory is dumped.
    pub fn data(lineno: u32, kind: Kind, args: Vec<Expr>) -> Asm {
        let name = match kind {
            Kind::Defb => "DEFB",
            Kind::Defs => "DEFS",
            _ => "DEFW",
        };
        let label_refs = expr::collect_labels(&args);
        Asm { lineno, filename: Rc::from(""), source: None, asm: name.to_string(), kind, opcode: None, widths: Vec::new(), args: args.into_iter().map(Arg::Expr).collect(), values: Vec::new(), blob: None, pending: true, label_refs }
    }

    /// The bytes of an included binary file, as a `DEFB`.
    pub fn blob(lineno: u32, bytes: Vec<u8>) -> Asm {
        Asm { lineno, filename: Rc::from(""), source: None, asm: "DEFB".to_string(), kind: Kind::Defb, opcode: None, widths: Vec::new(), args: Vec::new(), values: Vec::new(), blob: Some(bytes), pending: true, label_refs: Vec::new() }
    }

    fn arg_count(&self) -> usize {
        match (&self.blob, self.kind) {
            (Some(bytes), _) => bytes.len(),
            (None, Kind::Opcode) => self.widths.len(),
            (None, _) => self.args.len(),
        }
    }

    fn is_relative_jump(&self) -> bool {
        matches!(self.asm.split(' ').next(), Some("JR" | "DJNZ"))
    }

    /// Works out the operand values, reporting what is wrong with them. The
    /// Python `argval()`.
    pub fn resolve(&mut self, sess: &mut Session) {
        self.pending = false;
        if sess.has_errors > 0 {
            self.values = vec![None];
            return;
        }
        if let Some(bytes) = &self.blob {
            self.values = bytes.iter().map(|&b| Some(b as i128)).collect();
            return;
        }
        let values: Vec<Option<i128>> = self
            .args
            .iter()
            .map(|a| match a {
                Arg::Expr(e) => expr::eval(e, true, sess),
                Arg::Int(v) => Some(*v),
            })
            .collect();
        if self.kind != Kind::Opcode {
            if self.kind == Kind::Defb && values.iter().flatten().any(|&v| v > 255) {
                sess.warning_coded("200", self.lineno, "Value will be truncated", None);
            }
            self.values = values;
            return;
        }
        if sess.has_errors > 0 {
            self.values = vec![None];
            return;
        }
        if self.is_relative_jump() && values.first().copied().flatten().is_some_and(|v| !(-128..=127).contains(&v)) {
            sess.error(self.lineno, "Relative jump out of range", None);
            self.values = vec![None];
            return;
        }
        self.values = values;
    }

    /// The bytes this occupies. While operands are pending they read as zero,
    /// so that the size is right and labels after it land where they should.
    /// The count of a DEFS, as a size. No memory the assembler knows of holds
    /// more than this, and a count beyond it is refused here, before the
    /// bytes are asked for.
    fn defs_count(&self, count: i128, sess: &mut Session) -> usize {
        const MOST: i128 = 1 << 24;
        if count > MOST {
            sess.error(self.lineno, &format!("DEFS count {count} is out of range"), None);
            return 0;
        }
        usize::try_from(count).unwrap_or(0)
    }

    pub fn bytes(&mut self, sess: &mut Session) -> Vec<u8> {
        match self.kind {
            Kind::Opcode => {
                let op = self.opcode.unwrap();
                // The Python code evaluates the operands again here.
                let usable = !self.pending && sess.has_errors == 0;
                if usable && self.is_relative_jump() && self.values.first().copied().flatten().is_some_and(|v| !(-128..=127).contains(&v)) {
                    sess.error(self.lineno, "Relative jump out of range", None);
                }
                let mut out = Vec::with_capacity(op.size as usize);
                let mut parts = op.encoding.split(' ');
                let mut arg = 0;
                while let Some(part) = parts.next() {
                    if part == "XX" {
                        let width = self.widths.get(arg).copied().unwrap_or(1);
                        for _ in 1..width {
                            parts.next();
                        }
                        let value = if usable { self.values.get(arg).copied().flatten().unwrap_or(0) } else { 0 };
                        out.extend((0..width).map(|i| (value >> (8 * i)) as u8));
                        arg += 1;
                    } else {
                        out.push(u8::from_str_radix(part, 16).expect("opcode table"));
                    }
                }
                out
            }
            Kind::Defb => {
                if self.pending {
                    return vec![0; self.arg_count()];
                }
                if self.blob.is_none() && sess.has_errors == 0 && self.values.iter().flatten().any(|&v| v > 255) {
                    sess.warning_coded("200", self.lineno, "Value will be truncated", None);
                }
                self.values.iter().map(|v| v.unwrap_or(0) as u8).collect()
            }
            Kind::Defs => {
                if self.pending {
                    // The count must be known at once; the fill may wait.
                    let count = match self.args.first() {
                        Some(Arg::Expr(e)) => expr::eval(e, true, sess),
                        Some(Arg::Int(v)) => Some(*v),
                        None => None,
                    };
                    return vec![0; self.defs_count(count.unwrap_or(0), sess)];
                }
                let count = self.values.first().copied().flatten().unwrap_or(0);
                let fill = self.values.get(1).copied().flatten().unwrap_or(0);
                if fill > 255 {
                    sess.warning_coded("200", self.lineno, "Value will be truncated", None);
                }
                vec![fill as u8; self.defs_count(count, sess)]
            }
            Kind::Defw => {
                if self.pending {
                    return vec![0; 2 * self.arg_count()];
                }
                self.values.iter().flat_map(|v| (v.unwrap_or(0) as u16).to_le_bytes()).collect()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operand_widths_follow_runs_of_n() {
        assert_eq!(operand_widths("LD A,N"), [1]);
        assert_eq!(operand_widths("LD (IX+N),N"), [1, 1]);
        assert_eq!(operand_widths("JP NC,NN"), [2]);
        assert_eq!(operand_widths("IN A,(N)"), [1]);
        assert_eq!(operand_widths("NOP"), [] as [usize; 0]);
        assert_eq!(operand_widths("NEXTREG N,A"), [1]);
    }

    #[test]
    fn encodes() {
        let mut sess = Session::default();
        let mut i = Asm::instruction(&mut sess, 1, "LD (IX+N),N", vec![Arg::Int(-1), Arg::Int(0x1234)]).unwrap();
        assert_eq!(i.bytes(&mut sess), [0xDD, 0x36, 0xFF, 0x34]);
        let mut i = Asm::instruction(&mut sess, 1, "JP NN", vec![Arg::Int(0x8001)]).unwrap();
        assert_eq!(i.bytes(&mut sess), [0xC3, 0x01, 0x80]);
        assert!(Asm::instruction(&mut sess, 1, "LD Q,N", vec![]).is_none());
    }
}
