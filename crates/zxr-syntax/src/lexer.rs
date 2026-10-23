//! The BASIC lexer. Rules are in `grammar/zxbasic.lex`, keywords in
//! `grammar/zxbasic.keywords`.
//!
//! Three things make it more than a table of expressions: an `asm ... end
//! asm` block comes out as a single token; a name or number first on its
//! line may be a label; and a name the program has declared as an array is a
//! different token from any other name, which the parser relies on.

use crate::grammar::term;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::OnceLock;
use zxr_core::Session;
use zxr_lalr::runtime::UNKNOWN_TERMINAL;
use zxr_lex::{Lexer, Spec, Step};
use zxr_re::Match;

include!(concat!(env!("OUT_DIR"), "/lex.rs"));
include!(concat!(env!("OUT_DIR"), "/keywords.rs"));
const TEXT: &str = include_str!("../../../grammar/zxbasic.lex");

#[derive(Clone, Debug, PartialEq)]
pub enum TokenValue {
    Text(String),
    Int(i128),
    Float(f64),
}

#[derive(Clone, Debug)]
pub struct Token {
    pub kind: u16,
    /// The token type as the Python lexer names it.
    pub name: &'static str,
    pub value: TokenValue,
    pub lineno: u32,
    pub lexpos: usize,
}

fn keywords() -> &'static HashMap<&'static str, (&'static str, u16)> {
    static MAP: OnceLock<HashMap<&'static str, (&'static str, u16)>> = OnceLock::new();
    MAP.get_or_init(|| KEYWORDS.iter().map(|&(word, token, id)| (word, (token, id))).collect())
}

pub struct BasicLexer {
    lex: Lexer,
    /// Grammar terminal of each token type the rules name.
    kinds: Vec<(Option<u16>, &'static str)>,
    /// Text of the asm block being read, and the line it started on.
    asm: String,
    asm_lineno: u32,
    in_asm: bool,
    /// False on a continuation line, where nothing is a label.
    labels_allowed: bool,
    string: String,
    comment_level: i32,
}

impl BasicLexer {
    pub fn new(input: &str) -> BasicLexer {
        let spec = Rc::new(Spec::parse(TEXT, &FUNCTIONS).expect("BASIC lexer rules"));
        let names = crate::grammar::TABLES.terminal_names;
        let kinds = spec
            .tokens
            .iter()
            .map(|name| match names.iter().position(|t| t == name) {
                Some(i) => (Some(i as u16), names[i]),
                None => (None, ""),
            })
            .collect();
        let mut lex = Lexer::new(spec);
        lex.input(Rc::new(input.chars().collect()));
        BasicLexer { lex, kinds, asm: String::new(), asm_lineno: 0, in_asm: false, labels_allowed: true, string: String::new(), comment_level: 0 }
    }

    pub fn lineno(&self) -> u32 {
        self.lex.lineno
    }

    pub fn lexpos(&self) -> usize {
        self.lex.pos
    }

    /// `zxblex.IN_STATE`: inside an `asm` block that has not been closed.
    pub fn in_state(&self) -> bool {
        self.in_asm
    }

    /// The next token. `lookup` says whether a name is in the symbol table
    /// and, if it is, whether as an array.
    pub fn token(&mut self, sess: &mut Session, lookup: &dyn Fn(&str) -> Option<bool>) -> Option<Token> {
        loop {
            let lineno = self.lex.lineno;
            match self.lex.step() {
                Step::End => return None,
                Step::NoMatch { pos } => {
                    let message = format!("zxbc: scanning error, illegal character {:?}\n", self.lex.data[pos]);
                    sess.write_stderr(&message);
                    zxr_core::exit(1);
                }
                Step::Rule { function, token, found } => {
                    let text = self.lex.text(&found);
                    let (kind, name) = self.kinds[token as usize];
                    let mut tok = Token { kind: kind.unwrap_or(UNKNOWN_TERMINAL), name, value: TokenValue::Text(text.clone()), lineno, lexpos: found.start };
                    if self.action(sess, lookup, FUNCS[function as usize], &mut tok, &text, &found) {
                        return Some(tok);
                    }
                }
            }
        }
    }

    /// A literal in base 2, 8 or 16. One too long for 127 bits is an error,
    /// not the largest number there is.
    fn literal(sess: &mut Session, digits: &str, radix: u32, text: &str, lineno: u32) -> i128 {
        i128::from_str_radix(digits, radix).unwrap_or_else(|_| {
            sess.error(lineno, &format!("Number '{text}' is too large"), None);
            0
        })
    }

    fn at_line_start(&self, pos: usize) -> bool {
        pos == 0 || self.lex.data[pos - 1] == '\n'
    }

    /// Whether a token is a label: first on its line (blanks aside), and
    /// either a number or a name followed by a colon. Index arithmetic as in
    /// the Python lexer, which among other things never sees a label at the
    /// very start of the input.
    fn is_label(&self, lexpos: usize, number: bool, len: usize) -> bool {
        if !self.labels_allowed {
            return false;
        }
        let data = &self.lex.data;
        let mut c = lexpos as i64 - 1;
        while c > 0 && matches!(data[c as usize], ' ' | '\t') {
            c -= 1;
        }
        let mut i = lexpos as i64;
        while i > 0 {
            if data.get(i as usize) == Some(&'\n') {
                break;
            }
            i -= 1;
        }
        if c != i {
            return false;
        }
        if number {
            return true;
        }
        let mut i = lexpos + len;
        while i < data.len() {
            match data[i] {
                ':' => return true,
                ' ' | '\t' => i += 1,
                _ => break,
            }
        }
        false
    }

    fn set(tok: &mut Token, kind: u16) {
        tok.kind = kind;
        tok.name = crate::grammar::TABLES.terminal_names[kind as usize];
    }

    fn line_marker(&mut self, sess: &mut Session, found: &Match) {
        let data = Rc::clone(&self.lex.data);
        self.lex.lineno = found.group(&data, 1).and_then(|n| n.parse().ok()).unwrap_or(0);
        if let Some(name) = found.group(&data, 2).filter(|n| !n.is_empty()) {
            sess.filename = name;
        }
    }

    fn action(&mut self, sess: &mut Session, lookup: &dyn Fn(&str) -> Option<bool>, func: Func, tok: &mut Token, text: &str, found: &Match) -> bool {
        use Func::*;
        let nth = |n: usize| text.chars().nth(n).unwrap();
        let flag = |c: char| if matches!(c, '1' | 'i') { '\u{1}' } else { '\0' };
        let digit = |c: char| char::from_u32(c.to_digit(10).unwrap()).unwrap();
        match func {
            t_INITIAL_bin_comment_beginBlockComment => {
                self.comment_level += 1;
                self.lex.push_state("comment");
                false
            }
            t_comment_endBlockComment => {
                self.comment_level -= 1;
                self.lex.pop_state();
                false
            }
            t_comment_NEWLINE => {
                self.lex.lineno += 1;
                false
            }
            t_comment_charComment | t_rem | t_asm_comment | t_INITIAL_bin_preproc_SEPARATOR => false,
            t_PLUS | t_MINUS | t_MUL | t_DIV | t_LP | t_RP | t_WEQ | t_CO | t_SC | t_COMMA | t_GE | t_LE | t_NE | t_RIGHTARROW | t_EQ | t_SHL | t_LT | t_SHR | t_GT | t_POW | t_BXOR | t_BAND
            | t_BOR | t_BNOT | t_LBRACE | t_RBRACE | t_ADDRESSOF | t_preproc_INTEGER | t_preproc_LP | t_preproc_RP | t_preproc_EQ => true,
            t_initial_PREPROC_LINE => {
                self.line_marker(sess, found);
                false
            }
            t_INITIAL_SHARP => {
                if self.at_line_start(tok.lexpos) {
                    self.lex.begin("preproc");
                } else {
                    sess.error(tok.lineno, "illegal character '#'", None);
                }
                false
            }
            t_str => {
                self.lex.begin("string");
                false
            }
            t_string_SLASH => {
                self.string.push('\\');
                false
            }
            t_string_COPYRIGHT => {
                self.string.push('\u{7f}');
                false
            }
            t_string_NGRAPH => {
                // Block graphics: two characters, each one of `space ' . :`.
                let high = |c| match c {
                    '\'' => 2,
                    '.' => 8,
                    ':' => 10,
                    _ => 0,
                };
                let low = |c| match c {
                    '\'' => 1,
                    '.' => 4,
                    ':' => 5,
                    _ => 0,
                };
                self.string.push(char::from_u32(128 + high(nth(1)) + low(nth(2))).unwrap());
                false
            }
            t_string_UDG => {
                self.string.push(char::from_u32(79 + nth(1).to_ascii_uppercase() as u32).unwrap());
                false
            }
            t_string_CODE => {
                self.string.push(char::from_u32(text[2..].parse().unwrap()).unwrap());
                false
            }
            t_string_PAPERCODE => {
                self.string.extend(['\u{11}', digit(nth(3))]);
                false
            }
            t_string_INKCODE => {
                self.string.extend(['\u{10}', digit(nth(3))]);
                false
            }
            t_string_FLASH => {
                self.string.extend(['\u{12}', flag(nth(3))]);
                false
            }
            t_string_BRIGHT => {
                self.string.extend(['\u{13}', flag(nth(3))]);
                false
            }
            t_string_INVERSE => {
                self.string.extend(['\u{14}', flag(nth(3))]);
                false
            }
            t_string_ITALIC => {
                self.string.extend(['\u{f}', flag(nth(3))]);
                false
            }
            t_string_BOLD => {
                self.string.extend(['\u{e}', flag(nth(3))]);
                false
            }
            t_string_DQUOTE => {
                self.string.push('"');
                false
            }
            t_string_STRC => {
                self.lex.begin("INITIAL");
                tok.value = TokenValue::Text(std::mem::take(&mut self.string));
                true
            }
            t_string_CHAR => {
                self.string.push_str(text);
                false
            }
            t_asm => {
                self.lex.begin("asm");
                self.asm.clear();
                self.asm_lineno = self.lex.lineno;
                self.in_asm = true;
                false
            }
            t_asm_ASM => {
                self.lex.begin("INITIAL");
                tok.value = TokenValue::Text(std::mem::take(&mut self.asm));
                tok.lineno = self.asm_lineno;
                self.in_asm = false;
                true
            }
            t_asm_STRING | t_asm_next => {
                self.asm.push_str(text);
                false
            }
            t_asm_PREPROCLINE => {
                self.asm.push_str(text);
                self.line_marker(sess, found);
                false
            }
            t_asm_NEWLINE => {
                self.asm.push_str(text);
                self.lex.lineno += 1;
                false
            }
            t_EmptyRem => {
                self.lex.begin("INITIAL");
                self.lex.lineno += 1;
                tok.value = TokenValue::Text("\n".to_string());
                Self::set(tok, term::NEWLINE);
                true
            }
            t_preproc_ID => {
                let kind = match text.to_lowercase().as_str() {
                    "line" => term::_LINE,
                    "init" => term::_INIT,
                    "require" => term::_REQUIRE,
                    "pragma" => term::_PRAGMA,
                    "push" => term::_PUSH,
                    "pop" => term::_POP,
                    _ => term::ID,
                };
                Self::set(tok, kind);
                true
            }
            t_preproc_NEWLINE => {
                self.lex.begin("INITIAL");
                self.lex.lineno += 1;
                true
            }
            t_preproc_STRING => {
                let n = text.chars().count();
                tok.value = TokenValue::Text(text.chars().skip(1).take(n - 2).collect());
                true
            }
            t_HEXA | t_OCTAL | t_BIN => {
                let (digits, radix) = match func {
                    t_HEXA => (text.strip_prefix('$').or_else(|| text.strip_prefix("0x")).unwrap_or_else(|| &text[..text.len() - 1]), 16),
                    t_OCTAL => (&text[..text.len() - 1], 8),
                    _ => (text.strip_prefix('%').unwrap_or_else(|| &text[..text.len() - 1]), 2),
                };
                tok.value = TokenValue::Int(Self::literal(sess, digits, radix, &text, tok.lineno));
                Self::set(tok, term::NUMBER);
                true
            }
            t_bin_NUMBER => {
                tok.value = TokenValue::Int(Self::literal(sess, text, 2, text, tok.lineno));
                self.lex.begin("INITIAL");
                true
            }
            t_NUMBER => {
                let value: f64 = text.parse().unwrap_or(f64::INFINITY);
                tok.value = TokenValue::Float(value);
                if value.fract() == 0.0 && value.is_finite() && self.is_label(tok.lexpos, true, 0) {
                    tok.value = TokenValue::Int(value as i128);
                    Self::set(tok, term::LABEL);
                }
                true
            }
            t_INITIAL_bin_LineContinue => {
                self.lex.lineno += 1;
                self.labels_allowed = false;
                false
            }
            t_ID => {
                match keywords().get(text.to_lowercase().as_str()) {
                    Some(&(name, kind)) => {
                        // `BIN` is not a token: what follows is a binary number.
                        if name == "BIN" {
                            self.lex.begin("bin");
                            return false;
                        }
                        tok.value = TokenValue::Text(name.to_string());
                        Self::set(tok, kind);
                    }
                    None => {
                        // A name the symbol table knows is never a label.
                        match lookup(text) {
                            Some(true) => Self::set(tok, term::ARRAY_ID),
                            Some(false) => {}
                            None => {
                                if self.is_label(tok.lexpos, false, text.chars().count()) {
                                    Self::set(tok, term::LABEL);
                                }
                            }
                        }
                    }
                }
                true
            }
            t_bin_ZERO => {
                // No digits after BIN: the value is zero, and this character
                // is read again as ordinary input.
                self.lex.begin("INITIAL");
                tok.value = TokenValue::Int(0);
                Self::set(tok, term::NUMBER);
                self.lex.pos -= 1;
                true
            }
            t_INITIAL_bin_NEWLINE => {
                self.lex.lineno += 1;
                tok.value = TokenValue::Text("\n".to_string());
                self.labels_allowed = true;
                true
            }
            t_INITIAL_bin_string_asm_preproc_comment_ERROR => {
                sess.error(tok.lineno, &format!("ignoring illegal character '{}'", nth(0)), None);
                tok.name = "ERROR";
                true
            }
        }
    }
}
