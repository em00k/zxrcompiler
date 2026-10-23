//! The assembler's lexer. Rules are in `grammar/zxbasm.lex`.

use crate::grammar::next::term;
use std::rc::Rc;
use zxr_core::Session;
use zxr_lex::{Lexer, Spec, Step};

include!(concat!(env!("OUT_DIR"), "/lex.rs"));
const TEXT: &str = include_str!("../../../grammar/zxbasm.lex");

#[derive(Clone, Debug)]
pub enum TokenValue {
    Text(String),
    Int(i128),
}

pub struct Token {
    pub kind: u16,
    pub value: TokenValue,
    pub lineno: u32,
    pub lexpos: usize,
}

pub struct AsmLexer {
    lex: Lexer,
    kinds: Vec<Option<u16>>,
    /// Whether the Z80N mnemonics and CODEBANK are keywords.
    zxnext: bool,
    /// `-b`: parentheses only group; brackets are the memory indirection.
    force_brackets: bool,
    /// Marks in the order met, each with where it begins.
    marks: Vec<(usize, Mark)>,
}

fn parse_digits(text: &str, radix: u32) -> i128 {
    i128::from_str_radix(&text.replace('_', ""), radix).unwrap_or(i128::MAX)
}

/// The terminal a word spells, if it is a keyword. Looked up in lower case,
/// in the order the Python lexer consults its tables.
fn keyword(word: &str, zxnext: bool) -> Option<u16> {
    let terminal = |name: &str| crate::grammar::next::TABLES.terminal_names.iter().position(|t| *t == name).map(|i| i as u16);
    const INSTRUCTIONS: &str = " adc add and bit call ccf cp cpd cpdr cpi cpir cpl daa dec di djnz ei ex exx halt im in inc ind indr ini inir jp jr ld ldd lddr ldi ldir neg nop or otdr otir out outd outi pop push res ret reti retn rl rla rlc rlca rld rr rra rrc rrca rrd rst sbc scf set sla sll sra srl sub xor ";
    const REGS8: &str = " a b c d e h l i r ixh ixl iyh iyl ";
    const FLAGS: &str = " z nz nc po pe p m ";
    const NEXT: &str = " ldix ldws ldirx lddx lddrx ldpirx outinb mul swapnib mirror nextreg pixeldn pixelad setae test bsla bsra bsrl bsrf brlc codebank ";
    const REGS16: &str = " af bc de hl ix iy sp ";
    let has = |list: &str| list.contains(&format!(" {word} "));
    let pseudo = match word {
        "align" => Some("ALIGN"),
        "org" => Some("ORG"),
        "defb" | "defm" | "db" => Some("DEFB"),
        "defs" | "ds" => Some("DEFS"),
        "defw" | "dw" => Some("DEFW"),
        "equ" => Some("EQU"),
        "proc" => Some("PROC"),
        "endp" => Some("ENDP"),
        "local" => Some("LOCAL"),
        "end" => Some("END"),
        "incbin" => Some("INCBIN"),
        "namespace" => Some("NAMESPACE"),
        _ => None,
    };
    if has(INSTRUCTIONS) {
        terminal(&word.to_uppercase())
    } else if let Some(name) = pseudo {
        terminal(name)
    } else if has(REGS8) || has(FLAGS) || (zxnext && has(NEXT)) || has(REGS16) {
        terminal(&word.to_uppercase())
    } else {
        None
    }
}

/// A comment that begins so is the compiler's: the number after it names
/// the source line the code below was generated for, in a table handed to
/// the assembler; 0 is none.
pub const PIN: &str = ";@sld ";

/// Where the text after a point came from, for debug data.
#[derive(Clone, Debug)]
pub enum Mark {
    /// Source text of this file, since a `#line`: each line is its own.
    Track(Rc<str>),
    /// Code generated for the source line of that number in the table.
    Pin(u32),
}

impl AsmLexer {
    pub fn new(input: &str, zxnext: bool, force_brackets: bool) -> AsmLexer {
        let spec = Rc::new(Spec::parse(TEXT, &FUNCTIONS).expect("assembler lexer rules"));
        let kinds = spec.tokens.iter().map(|name| crate::grammar::next::TABLES.terminal_names.iter().position(|t| t == name).map(|i| i as u16)).collect();
        let mut lex = Lexer::new(spec);
        lex.input(Rc::new(input.chars().collect()));
        AsmLexer { lex, kinds, zxnext, force_brackets, marks: Vec::new() }
    }

    /// What says where the text at `lexpos` came from: the last mark
    /// before it.
    pub fn mark_at(&self, lexpos: usize) -> Option<&Mark> {
        let after = self.marks.partition_point(|(pos, _)| *pos <= lexpos);
        after.checked_sub(1).map(|i| &self.marks[i].1)
    }

    pub fn lineno(&self) -> u32 {
        self.lex.lineno
    }

    pub fn lexpos(&self) -> usize {
        self.lex.pos
    }

    pub fn token(&mut self, sess: &mut Session) -> Option<Token> {
        loop {
            let lineno = self.lex.lineno;
            match self.lex.step() {
                Step::End => return None,
                Step::NoMatch { pos } => {
                    let message = format!("zxbasm: scanning error, illegal character {:?}\n", self.lex.data[pos]);
                    sess.write_stderr(&message);
                    zxr_core::exit(1);
                }
                Step::Rule { function, token, found } => {
                    let text = self.lex.text(&found);
                    let mut tok = Token { kind: self.kinds[token as usize].unwrap_or(u16::MAX), value: TokenValue::Text(text.clone()), lineno, lexpos: found.start };
                    if self.action(sess, FUNCS[function as usize], &mut tok, &text, &found) {
                        debug_assert_ne!(tok.kind, u16::MAX, "token type is not a grammar terminal");
                        return Some(tok);
                    }
                }
            }
        }
    }

    fn illegal(&self, sess: &mut Session, text: &str) {
        let c = text.chars().next().unwrap_or(' ');
        sess.error(self.lex.lineno, &format!("illegal character '{c}'"), None);
    }

    fn action(&mut self, sess: &mut Session, func: Func, tok: &mut Token, text: &str, found: &zxr_re::Match) -> bool {
        use Func::*;
        let int = |tok: &mut Token, value: i128| {
            tok.kind = term::INTEGER;
            tok.value = TokenValue::Int(value);
        };
        match func {
            t_INITIAL_preproc_skip => false,
            t_COMMENT => {
                // What the compiler says of the code it generated.
                if let Some(origin) = text.strip_prefix(PIN).and_then(|n| n.trim().parse().ok()) {
                    self.marks.push((tok.lexpos, Mark::Pin(origin)));
                }
                false
            }
            t_CHAR => {
                int(tok, text.chars().nth(1).unwrap() as i128);
                true
            }
            t_HEXA => {
                let digits = if let Some(rest) = text.strip_prefix("0x") {
                    rest
                } else if let Some(rest) = text.strip_prefix('$') {
                    rest
                } else {
                    &text[..text.len() - 1]
                };
                int(tok, parse_digits(digits, 16));
                true
            }
            t_BIN => {
                let digits = text.strip_prefix('%').unwrap_or_else(|| &text[2..]);
                int(tok, parse_digits(digits, 2));
                true
            }
            t_INITIAL_TMPLABEL => {
                tok.kind = term::ID;
                tok.value = TokenValue::Text(text.to_uppercase());
                true
            }
            t_INITIAL_preproc_INTEGER => {
                int(tok, parse_digits(text, 10));
                true
            }
            t_INITIAL_ID => {
                // Keywords are upper-cased; any other name keeps its spelling.
                match keyword(&text.to_lowercase(), self.zxnext) {
                    Some(kind) => {
                        tok.kind = kind;
                        tok.value = TokenValue::Text(text.to_uppercase());
                    }
                    None => tok.kind = term::ID,
                }
                true
            }
            t_asm_PREPROCLINE => {
                let data = Rc::clone(&self.lex.data);
                if let Some(name) = found.group(&data, 2).filter(|n| !n.is_empty()) {
                    sess.filename = name;
                }
                self.lex.lineno = found.group(&data, 1).and_then(|n| n.parse().ok()).unwrap_or(0);
                self.marks.push((tok.lexpos, Mark::Track(Rc::from(sess.filename.as_str()))));
                false
            }
            t_preproc_ID => {
                tok.kind = if text.eq_ignore_ascii_case("init") { term::_INIT } else { term::ID };
                true
            }
            t_COMMA | t_ADDR | t_LB | t_RB | t_LSHIFT | t_RSHIFT | t_BAND | t_BOR | t_BXOR | t_PLUS | t_MINUS | t_MUL | t_DIV | t_MOD | t_POW | t_APO | t_CO => true,
            t_LP => {
                if self.force_brackets {
                    tok.kind = term::LPP;
                }
                true
            }
            t_RP => {
                if self.force_brackets {
                    tok.kind = term::RPP;
                }
                true
            }
            t_INITIAL_preproc_STRING => {
                let n = text.chars().count();
                let inner: String = text.chars().skip(1).take(n - 2).collect();
                tok.value = TokenValue::Text(inner.replace("\"\"", "\""));
                true
            }
            t_INITIAL_preproc_CONTINUE => {
                self.lex.lineno += 1;
                false
            }
            t_INITIAL_preproc_NEWLINE => {
                self.lex.lineno += 1;
                self.lex.begin("INITIAL");
                true
            }
            t_INITIAL_SHARP => {
                if tok.lexpos == 0 || self.lex.data[tok.lexpos - 1] == '\n' {
                    self.lex.begin("preproc");
                } else {
                    self.illegal(sess, text);
                }
                false
            }
            t_INITIAL_preproc_ERROR => {
                self.illegal(sess, text);
                false
            }
        }
    }
}
