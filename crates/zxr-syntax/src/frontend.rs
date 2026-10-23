//! The front end's part of `main()` in `src/zxbc/zxbc.py`: from the parse to
//! the end of translation, in the order the Python compiler runs things.

use crate::compiler::Compiler;
use zxr_ir::Quad;

/// How far to go, and what to hand back.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stop {
    /// `--stage ast`: the tree as parsed.
    Ast,
    /// `--stage ast2`: the tree after the passes.
    AstPasses,
    /// `--stage ir`: the quads of every pass and the state the backend reads.
    Ir,
    /// `-f ir`: what the Python compiler writes for that format.
    EmitBackend,
    /// `--parse-only`.
    ParseOnly,
    /// Go on to the backend.
    Translated,
}

pub enum Outcome {
    /// The compile is over, with this exit status.
    Exit(i32),
    /// The text of the stage asked for.
    Text(String),
    /// The main body is translated and in `Compiler::memory`.
    Translated,
}

fn quads_text(memory: &[Quad]) -> String {
    let mut text = String::new();
    for quad in memory {
        text.push_str(&quad.to_string());
        text.push('\n');
    }
    text
}

impl Compiler {
    fn errors(&self) -> bool {
        self.sess.has_errors > 0
    }

    /// Compiles preprocessed source as far as `stop` says.
    pub fn run_front_end(&mut self, source: &str, stop: Stop) -> Outcome {
        self.parse(source);
        if self.errors() {
            return Outcome::Exit(1);
        }
        let (Some(ast), Some(data_ast)) = (self.ast, self.data_ast) else { crate::pyval::crash("AttributeError: 'NoneType' object has no attribute 'token'") };
        if stop == Stop::Ast {
            return Outcome::Text(self.dump_ast());
        }

        // Unreachable code removal.
        self.unreachable_code_visitor(ast);
        // ZX Next: which module-level asm blocks are compiled into a code
        // bank. After unreachable-code removal, which strips the CHKBREAK
        // that --enable-break puts between a label and the block it names,
        // and before the optimizer, which may delete statements.
        self.bind_module_level_asm(ast);
        self.check_label_bank_access(ast);
        self.check_at_address_banks(data_ast);
        // Checked here because `#pragma codewindowsize` is applied while
        // the source is parsed.
        self.validate_codebank_options();
        if self.errors() {
            return Outcome::Exit(1);
        }

        self.function_graph_visitor(ast);
        self.mark_asm_uses(ast);
        self.optimizer_visitor(ast);
        if stop == Stop::AstPasses {
            return Outcome::Text(self.dump_ast());
        }

        // Intermediate code: the main body, then the functions.
        self.translate_program(ast);
        if self.data_is_used {
            let data_functions = self.data_functions.clone();
            self.functions.extend(data_functions);
        }
        self.translate_functions();
        if self.errors() {
            return Outcome::Exit(1);
        }
        if stop == Stop::ParseOnly {
            return Outcome::Exit(self.sess.has_errors as i32);
        }

        self.emit_data_blocks();
        self.emit_strings();
        self.emit_jump_tables();
        self.ic_inline(";; --- end of user code ---");
        if self.errors() {
            return Outcome::Exit(1);
        }

        match stop {
            Stop::EmitBackend => {
                let mut text = quads_text(&self.memory);
                self.memory.clear();
                self.translate_vars(data_ast, 0);
                text.push_str(&quads_text(&self.memory));
                Outcome::Text(text)
            }
            Stop::Ir => {
                // The variable passes as a full compile makes them, but with
                // no backend run in between. See docs/seams.md.
                let mut text = quads_text(&self.memory);
                self.memory.clear();
                self.variable_visitor(data_ast);
                self.translate_vars(data_ast, 0);
                if self.errors() {
                    return Outcome::Exit(1);
                }
                text.push_str("--- vars\n");
                text.push_str(&quads_text(&self.memory));
                // emit_banked_vars(): one more pass for each bank that has
                // a declaration, lowest first.
                let mut banks: Vec<i64> = Vec::new();
                for &child in self.children(data_ast) {
                    let entry = child.and_then(|c| self.child(c, 0));
                    let bank = self.bank_of_var(entry);
                    if bank != 0 && !banks.contains(&bank) {
                        banks.push(bank);
                    }
                }
                banks.sort();
                for bank in banks {
                    self.memory.clear();
                    self.translate_vars(data_ast, bank);
                    text.push_str(&format!("--- vars bank {bank}\n"));
                    text.push_str(&quads_text(&self.memory));
                }
                text.push_str("--- state\n");
                text.push_str(&self.dump_backend_state());
                Outcome::Text(text)
            }
            _ => Outcome::Translated,
        }
    }
}
