//! The preprocessor's two lexers and the stack of files being read.
//!
//! The rules live in `grammar/zxbpp_basic.lex` and `grammar/zxbpp_asm.lex`;
//! this file holds what each rule does once it has matched.

use crate::grammar::term;
use crate::Mode;
use std::rc::Rc;
use zxr_core::Session;
use zxr_lex::{Lexer, Spec, Step};
use zxr_re::Match;

pub mod basic {
    include!(concat!(env!("OUT_DIR"), "/lex_basic.rs"));
    pub const TEXT: &str = include_str!("../../../grammar/zxbpp_basic.lex");
}

pub mod asm {
    include!(concat!(env!("OUT_DIR"), "/lex_asm.rs"));
    pub const TEXT: &str = include_str!("../../../grammar/zxbpp_asm.lex");
}

pub const STDIN: &str = "(stdin)";

#[derive(Clone, Debug)]
pub struct Token {
    pub kind: u16,
    pub value: String,
    pub lineno: u32,
    pub lexpos: usize,
}

struct FileState {
    filename: String,
    /// The lexer and input that were current when this file was opened.
    lex: Option<Lexer>,
    input: Rc<Vec<char>>,
}

pub struct PpLexer {
    mode: Mode,
    spec: Rc<Spec>,
    /// Grammar terminal of each token type the rules name, if it is one.
    kinds: Vec<Option<u16>>,
    lex: Option<Lexer>,
    files: Vec<FileState>,
    input: Rc<Vec<char>>,
    /// If set, returned once as an empty token of this kind.
    pub next_token: Option<u16>,
    expecting_directive: bool,
    comment_level: i32,
}

/// `#include`, `#define` and the rest, by lower-case name.
fn directive(name: &str) -> Option<u16> {
    Some(match name.to_lowercase().as_str() {
        "include" => term::INCLUDE,
        "once" => term::ONCE,
        "define" => term::DEFINE,
        "undef" => term::UNDEF,
        "if" => term::IF,
        "ifdef" => term::IFDEF,
        "ifndef" => term::IFNDEF,
        "else" => term::ELSE,
        "elif" => term::ELIF,
        "endif" => term::ENDIF,
        "init" => term::INIT,
        "line" => term::LINE,
        "require" => term::REQUIRE,
        "pragma" => term::PRAGMA,
        "error" => term::ERROR,
        "warning" => term::WARNING,
        _ => return None,
    })
}

/// The lexer state a directive switches to, if any.
fn directive_state(kind: u16) -> Option<&'static str> {
    Some(match kind {
        term::DEFINE => "define",
        term::ERROR | term::WARNING => "msg",
        term::IF => "if",
        term::LINE => "line",
        term::PRAGMA => "pragma",
        _ => return None,
    })
}

/// Files open at once, one inside another, before a recursive inclusion is
/// taken to be endless.
const MAX_INCLUDE_DEPTH: usize = 200;

impl PpLexer {
    pub fn new(mode: Mode) -> PpLexer {
        let spec = match mode {
            Mode::Basic => Spec::parse(basic::TEXT, &basic::FUNCTIONS),
            Mode::Asm => Spec::parse(asm::TEXT, &asm::FUNCTIONS),
        }
        .expect("preprocessor lexer rules");
        let kinds = spec.tokens.iter().map(|name| crate::grammar::TABLES.terminal_names.iter().position(|t| t == name).map(|i| i as u16)).collect();
        PpLexer { mode, spec: Rc::new(spec), kinds, lex: None, files: Vec::new(), input: Rc::new(Vec::new()), next_token: None, expecting_directive: false, comment_level: 0 }
    }

    pub fn lineno(&self) -> u32 {
        self.lex.as_ref().map_or(0, |l| l.lineno)
    }

    pub fn current_file(&self) -> Option<&str> {
        self.files.last().map(|f| f.filename.as_str())
    }

    fn line_marker(&self, suffix: &str) -> String {
        format!("#line {} \"{}\"{suffix}", self.lineno(), self.current_file().unwrap_or("None"))
    }

    fn error(&self, sess: &mut Session, message: &str) {
        sess.error(self.lineno(), message, None);
    }

    /// Starts reading `filename`; what is being read now resumes afterwards.
    /// Returns the `#line` marker that opens the file.
    pub fn include(&mut self, sess: &mut Session, filename: &str) -> String {
        if filename != STDIN && self.files.iter().any(|f| f.filename == filename) {
            sess.warning(self.lineno(), "Recursive inclusion", None);
            // A file that includes itself with nothing to stop it never
            // ends; the Python preprocessor goes on until memory does.
            if self.files.len() >= MAX_INCLUDE_DEPTH {
                sess.error(self.lineno(), &format!("Recursive inclusion of '{filename}' does not end"), None);
                zxr_core::exit(1);
            }
        }
        self.files.push(FileState { filename: filename.to_string(), lex: self.lex.take(), input: Rc::clone(&self.input) });
        // A file opened from another starts in the state its parent is in.
        let mut lex = match &self.files.last().unwrap().lex {
            Some(parent) => parent.clone(),
            None => Lexer::new(Rc::clone(&self.spec)),
        };
        lex.lineno = 1;
        self.lex = Some(lex);
        let result = self.line_marker("");

        let mut text = match std::fs::read(filename) {
            Ok(bytes) => match zxr_core::text::decode(&bytes) {
                Some(text) => text,
                None => {
                    sess.filename = filename.to_string();
                    sess.error(1, &format!("Invalid file encoding. Use one of: {}", zxr_core::text::ENCODINGS), None);
                    String::new()
                }
            },
            Err(_) => "\n".to_string(),
        };
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        if !sess.oracle_runtime {
            if let Some(right) = crate::runtime_fixes::fixed(filename, &text) {
                text = right.to_string();
            }
        }
        self.input = Rc::new(text.chars().collect());
        self.lex.as_mut().unwrap().input(Rc::clone(&self.input));
        result
    }

    /// Ends the current file. Returns the `_ENDFILE_` token that tells the
    /// parser so, or `None` when it was the outermost one.
    fn include_end(&mut self) -> Option<Token> {
        let (old_lineno, old_lexpos) = self.lex.as_ref().map_or((0, 0), |l| (l.lineno, l.pos));
        let state = self.files.pop()?;
        self.lex = state.lex;
        self.input = state.input;
        if self.files.is_empty() {
            return None;
        }
        Some(Token { kind: term::_ENDFILE_, value: self.line_marker("\n"), lineno: old_lineno, lexpos: old_lexpos })
    }

    /// Reads from a string instead of a file, with a fresh lexer.
    pub fn input(&mut self, text: &str, filename: &str) {
        self.files.push(FileState { filename: filename.to_string(), lex: self.lex.take(), input: Rc::clone(&self.input) });
        self.input = Rc::new(text.chars().collect());
        let mut lex = Lexer::new(Rc::clone(&self.spec));
        lex.input(Rc::clone(&self.input));
        self.lex = Some(lex);
    }

    /// Sets the state of the lexer just created by [`PpLexer::input`].
    pub fn begin(&mut self, state: &str) {
        if let Some(lex) = self.lex.as_mut() {
            lex.begin(state);
        }
    }

    pub fn token(&mut self, sess: &mut Session) -> Option<Token> {
        if let Some(kind) = self.next_token.take() {
            let (lineno, lexpos) = self.lex.as_ref().map_or((0, 0), |l| (l.lineno, l.pos));
            return Some(Token { kind, value: String::new(), lineno, lexpos });
        }
        while self.lex.is_some() {
            if let Some(token) = self.lex_token(sess) {
                return Some(token);
            }
            if let Some(token) = self.include_end() {
                return Some(token);
            }
        }
        None
    }

    fn lex_token(&mut self, sess: &mut Session) -> Option<Token> {
        loop {
            let lex = self.lex.as_mut().unwrap();
            let lineno = lex.lineno;
            match lex.step() {
                Step::End => return None,
                Step::NoMatch { pos } => {
                    // PLY raises LexError here and the Python compiler dies
                    // with a traceback.
                    let message = format!("zxbpp: scanning error, illegal character {:?} in state '{}'\n", lex.data[pos], lex.state_name());
                    sess.write_stderr(&message);
                    zxr_core::exit(1);
                }
                Step::Rule { function, token, found } => {
                    let value = lex.text(&found);
                    let mut tok = Token { kind: self.kinds[token as usize].unwrap_or(u16::MAX), value, lineno, lexpos: found.start };
                    let keep = match self.mode {
                        Mode::Basic => self.basic_action(sess, basic::FUNCS[function as usize], &mut tok, &found),
                        Mode::Asm => self.asm_action(sess, asm::FUNCS[function as usize], &mut tok),
                    };
                    if keep {
                        debug_assert_ne!(tok.kind, u16::MAX, "token type is not a grammar terminal");
                        return Some(tok);
                    }
                }
            }
        }
    }

    /// Whether the token starts in the first column of its line.
    fn at_line_start(&self, tok: &Token) -> bool {
        tok.lexpos == 0 || self.input[tok.lexpos - 1] == '\n'
    }

    fn lex_mut(&mut self) -> &mut Lexer {
        self.lex.as_mut().unwrap()
    }

    fn drop_first_char(tok: &mut Token) {
        tok.value = tok.value.chars().skip(1).collect();
    }

    fn unquote(tok: &mut Token) {
        let n = tok.value.chars().count();
        tok.value = tok.value.chars().skip(1).take(n.saturating_sub(2)).collect();
    }

    fn prepro_id(&mut self, sess: &mut Session, tok: &mut Token, complain: bool) {
        tok.kind = directive(&tok.value).unwrap_or(term::ID);
        if let Some(state) = directive_state(tok.kind) {
            self.lex_mut().begin(state);
        } else if complain && tok.kind == term::ID && self.expecting_directive {
            self.error(sess, &format!("invalid directive #{}", tok.value));
        }
        self.expecting_directive = false;
    }

    fn msg_text(&mut self, tok: &mut Token) {
        self.lex_mut().lineno += 1;
        self.lex_mut().begin("INITIAL");
        tok.value = tok.value.trim().to_string();
    }

    fn illegal(&mut self, sess: &mut Session, tok: &Token) {
        let c = tok.value.chars().next().unwrap_or(' ');
        self.error(sess, &format!("illegal preprocessor character '{c}'"));
    }

    /// Runs the action of a BASIC-mode rule. Returns whether the token is kept.
    fn basic_action(&mut self, sess: &mut Session, func: basic::Func, tok: &mut Token, found: &Match) -> bool {
        use basic::Func::*;
        match func {
            t_INITIAL_COMMENT => {
                self.lex_mut().push_state("singlecomment");
                false
            }
            t_INITIAL_asmBegin => {
                tok.kind = term::TOKEN;
                self.lex_mut().begin("asm");
                true
            }
            t_asm_asmEnd => {
                tok.kind = term::TOKEN;
                self.lex_mut().begin("INITIAL");
                true
            }
            t_asm_CONTINUE | t_defexpr_CONTINUE => {
                self.lex_mut().lineno += 1;
                Self::drop_first_char(tok);
                true
            }
            t_asm_COMMENT | t_singlecomment_comment_Skip | t_line_prepro_pragma_defargs_define_if_skip => false,
            t_asm_PREPROCLINE => {
                let data = Rc::clone(&self.input);
                let line: u32 = found.group(&data, 1).and_then(|n| n.parse().ok()).unwrap_or(0);
                self.lex_mut().lineno = line;
                if let Some(name) = found.group(&data, 2).filter(|n| !n.is_empty()) {
                    sess.filename = name;
                }
                false
            }
            t_asm_ID | t_INITIAL_ID | t_asm_TOKEN | t_defargs_if_ID | t_defargs_LP => true,
            t_asm_CHAR => {
                tok.kind = term::TOKEN;
                true
            }
            t_INITIAL_CONTINUE | t_prepro_define_pragma_defargs_defargsopt_CONTINUE | t_INITIAL_asm_NEWLINE | t_comment_NEWLINE => {
                self.lex_mut().lineno += 1;
                true
            }
            t_line_singlecomment_prepro_define_defargs_defargsopt_defexpr_pragma_if_NEWLINE => {
                self.lex_mut().lineno += 1;
                self.lex_mut().pop_state();
                true
            }
            t_prepro_define_defargs_defargsopt_defexpr_pragma_COMMENT => {
                self.lex_mut().begin("singlecomment");
                false
            }
            t_prepro_LB | t_prepro_RB | t_prepro_CO => true,
            t_INITIAL_comment_beginBlock => {
                self.comment_level += 1;
                self.lex_mut().begin("comment");
                false
            }
            t_comment_endBlock => {
                self.comment_level -= 1;
                if self.comment_level == 0 {
                    self.lex_mut().begin("INITIAL");
                }
                false
            }
            t_msg_TEXT => {
                self.msg_text(tok);
                true
            }
            t_singlecomment_CONTINUE => {
                self.lex_mut().lineno += 1;
                Self::drop_first_char(tok);
                self.lex_mut().pop_state();
                true
            }
            t_if_EQ | t_if_NE | t_if_GE | t_if_GT | t_if_LE | t_if_LT | t_if_AND | t_if_OR => true,
            t_prepro_ID => {
                self.prepro_id(sess, tok, true);
                true
            }
            t_pragma_LP | t_pragma_RP | t_INITIAL_asm_defexpr_if_LLP | t_INITIAL_asm_defexpr_if_RRP => true,
            t_pragma_ID => {
                match tok.value.to_uppercase().as_str() {
                    "ONCE" => tok.kind = term::ONCE,
                    "PUSH" => tok.kind = term::PUSH,
                    "POP" => tok.kind = term::POP,
                    _ => {}
                }
                true
            }
            t_defargsopt_LP => {
                self.lex_mut().begin("defargs");
                true
            }
            t_defargsopt_TOKEN | t_defargsopt_STRING | t_defargs_RP | t_defargsopt_defexpr_ID => {
                self.lex_mut().begin("defexpr");
                true
            }
            t_defargsopt_SEPARATOR => {
                self.lex_mut().begin("defexpr");
                false
            }
            t_define_ID => {
                self.lex_mut().begin("defargsopt");
                true
            }
            t_prepro_pragma_INTEGER | t_INITIAL_pragma_prepro_defexpr_asm_if_STRING | t_pragma_EQ | t_INITIAL_asm_defexpr_defargs_prepro_COMMA => true,
            t_line_INTEGER => {
                self.lex_mut().lineno = tok.value.parse().unwrap_or(0);
                true
            }
            t_line_STRING => {
                Self::unquote(tok);
                if let Some(file) = self.files.last_mut() {
                    file.filename = tok.value.clone();
                }
                true
            }
            t_INITIAL_asm_sharp => {
                if self.at_line_start(tok) {
                    self.lex_mut().push_state("prepro");
                    self.expecting_directive = true;
                    false
                } else {
                    tok.kind = term::TOKEN;
                    true
                }
            }
            t_INITIAL_defexpr_TOKEN | t_defexpr_PASTE | t_defexpr_STRINGIZING | t_INITIAL_defexpr_asm_SEPARATOR | t_INITIAL_defexpr_asm_if_NUMBER => true,
            t_INITIAL_defexpr_asm_HEXA | t_INITIAL_defexpr_asm_BIN => {
                tok.kind = term::NUMBER;
                true
            }
            t_prepro_FILENAME => {
                Self::unquote(tok);
                true
            }
            t_INITIAL_line_defargs_defargsopt_prepro_define_defexpr_pragma_comment_singlecomment_ANY | t_asm_asmcomment_if_ANY => {
                self.illegal(sess, tok);
                false
            }
        }
    }

    /// Runs the action of an ASM-mode rule. Returns whether the token is kept.
    fn asm_action(&mut self, sess: &mut Session, func: asm::Func, tok: &mut Token) -> bool {
        use asm::Func::*;
        match func {
            t_INITIAL_asmcomment_CONTINUE | t_INITIAL_NEWLINE | t_defexpr_CONTINUE => {
                self.lex_mut().lineno += 1;
                true
            }
            t_INITIAL_COMMENT => {
                self.lex_mut().push_state("asmcomment");
                tok.kind = term::TOKEN;
                tok.value = ";".to_string();
                true
            }
            t_asmcomment_TOKEN | t_INITIAL_ID | t_INITIAL_TOKEN | t_defargs_ID | t_defargs_LP => true,
            t_INITIAL_CHAR => {
                tok.kind = term::TOKEN;
                true
            }
            t_line_singlecomment_asmcomment_prepro_define_defargs_defargsopt_defexpr_pragma_if_NEWLINE => {
                self.lex_mut().lineno += 1;
                self.lex_mut().pop_state();
                true
            }
            t_prepro_define_defargs_defargsopt_defexpr_pragma_COMMENT => {
                self.lex_mut().begin("singlecomment");
                false
            }
            t_prepro_define_pragma_defargs_defargsopt_CONTINUE => {
                self.lex_mut().lineno += 1;
                Self::drop_first_char(tok);
                tok.kind = term::NEWLINE;
                true
            }
            t_prepro_LB | t_prepro_RB | t_prepro_CO => true,
            t_singlecomment_comment_Skip | t_line_prepro_pragma_defargs_define_skip_if => false,
            t_if_EQ | t_if_NE | t_if_GE | t_if_GT | t_if_LE | t_if_LT | t_if_AND | t_if_OR => true,
            t_prepro_ID => {
                self.prepro_id(sess, tok, false);
                true
            }
            t_msg_TEXT => {
                self.msg_text(tok);
                true
            }
            t_pragma_LP | t_pragma_RP | t_defexpr_LLP | t_defexpr_RRP | t_defexpr_STRINGIZING | t_defexpr_PASTE => true,
            t_pragma_ID => {
                match tok.value.to_uppercase().as_str() {
                    "PUSH" => tok.kind = term::PUSH,
                    "POP" => tok.kind = term::POP,
                    _ => {}
                }
                true
            }
            t_defargsopt_LP => {
                self.lex_mut().begin("defargs");
                true
            }
            t_defargsopt_TOKEN | t_defargsopt_STRING | t_defargs_RP | t_defargsopt_defexpr_ID => {
                self.lex_mut().begin("defexpr");
                true
            }
            t_defargs_defargsopt_CONTINUE => {
                self.lex_mut().lineno += 1;
                false
            }
            t_defargsopt_SEPARATOR => {
                self.lex_mut().begin("defexpr");
                false
            }
            t_define_ID => {
                self.lex_mut().begin("defargsopt");
                true
            }
            t_prepro_pragma_INTEGER | t_INITIAL_prepro_pragma_defexpr_STRING | t_pragma_EQ | t_defexpr_defargs_prepro_COMMA => true,
            t_line_INTEGER => {
                self.lex_mut().lineno = tok.value.parse().unwrap_or(0);
                true
            }
            t_line_STRING => {
                Self::unquote(tok);
                if let Some(file) = self.files.last_mut() {
                    file.filename = tok.value.clone();
                }
                true
            }
            t_INIIAL_sharp => {
                if self.at_line_start(tok) {
                    self.lex_mut().push_state("prepro");
                    false
                } else {
                    tok.kind = term::TOKEN;
                    true
                }
            }
            t_defexpr_TOKEN | t_defexpr_INITIAL_SEPARATOR | t_defexpr_INITIAL_NUMBER => true,
            t_defexpr_INITIAL_HEXA | t_defexpr_INITIAL_BIN => {
                tok.kind = term::NUMBER;
                true
            }
            t_prepro_FILENAME => {
                Self::unquote(tok);
                true
            }
            t_line_defargs_defargsopt_prepro_define_defexpr_pragma_singlecomment_INITIAL_asmcomment_ANY => {
                self.illegal(sess, tok);
                false
            }
        }
    }
}
