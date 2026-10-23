//! The fork's passes for module-level asm in code banks:
//! `src/zxbc/codebank_asm.py`, and `validate_codebank_options` of
//! `src/api/codebank_options.py`.
//!
//! `CODEBANK n` is not lexically scoped: it is a flag read where a SUB or
//! FUNCTION header and a global DIM are parsed. The parser marks every `asm`
//! block written at module level while the flag is set; the pass here decides
//! which of them are really compiled into the bank, and which labels move
//! with them. That is positional (`my_table:` on one line, `asm` on the
//! next), so it can only be done on the finished statement list.

use crate::ast::*;
use crate::compiler::Compiler;
use crate::pyval::crash;
use std::collections::HashSet;

fn where_(bank: i64) -> String {
    if bank != 0 { format!("CODEBANK {bank}") } else { "the resident program".to_string() }
}

/// `"%04X" % n`.
fn hex4(n: i128) -> String {
    if n < 0 { format!("-{:03X}", -n) } else { format!("{n:04X}") }
}

impl Compiler {
    fn present(node: Option<NodeId>) -> NodeId {
        node.unwrap_or_else(|| crash("AttributeError: 'NoneType' object has no attribute 'token'"))
    }

    /// `node.bank` of an ASM node.
    fn asm_bank(&self, node: NodeId) -> i64 {
        match self.node(node).kind {
            Kind::Asm { bank, .. } => bank,
            _ => 0,
        }
    }

    /// `codebank_asm.bind_module_level_asm(ast)`: decides which module-level
    /// `asm` blocks are compiled into a code bank. Only a direct child of
    /// the top-level statement list qualifies; a block inside an IF or a FOR
    /// is code in a control-flow position and stays where it was written.
    pub fn bind_module_level_asm(&mut self, ast: NodeId) {
        if self.token(ast) != "BLOCK" {
            return;
        }
        let mut diverted: HashSet<NodeId> = HashSet::new();
        // Labels not yet claimed by a block.
        let mut pending: Vec<NodeId> = Vec::new();
        for node in self.children(ast).to_vec() {
            let node = Self::present(node);
            match self.token(node) {
                "LABEL" => {
                    // A BASIC line number is a control-flow target, never a
                    // name for a data block.
                    if self.id(node).label().is_some_and(|l| l.is_line_number) {
                        pending.clear();
                    } else {
                        pending.push(node);
                    }
                }
                // Put in by --enable-break. It names nothing.
                "CHKBREAK" => {}
                "ASM" if self.asm_bank(node) != 0 => {
                    let bank = self.asm_bank(node);
                    for label in pending.drain(..) {
                        self.id_mut(label).label_mut().unwrap().bank = bank;
                    }
                    diverted.insert(node);
                }
                _ => pending.clear(), // anything else ends the run
            }
        }
        self.clear_nested(ast, &diverted);
    }

    /// `_clear_nested`: un-marks every `asm` block that is not at the top
    /// level. The parser marks by the option alone, so a block inside
    /// `IF ... END IF` written in a CODEBANK block is marked too.
    fn clear_nested(&mut self, ast: NodeId, diverted: &HashSet<NodeId>) {
        for node in self.walk_routine(ast) {
            if !diverted.contains(&node) {
                if let Kind::Asm { bank, .. } = &mut self.node_mut(node).kind {
                    *bank = 0;
                }
            }
        }
    }

    /// `_walk`: every node under `root`, stopping at a routine boundary. A
    /// function body is a child of its FUNCTION identifier, so a walk
    /// without that guard would go into every routine.
    fn walk_routine(&self, root: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack = vec![Some(root)];
        let mut seen: HashSet<NodeId> = HashSet::new();
        while let Some(node) = stack.pop() {
            let node = Self::present(node);
            if seen.contains(&node) || matches!(self.token(node), "FUNCDECL" | "FUNCTION") {
                continue;
            }
            seen.insert(node);
            out.push(node);
            stack.extend(self.children(node).iter().copied());
        }
        out
    }

    /// Whether the Python class of the node has a `lineno`.
    fn has_lineno(&self, node: NodeId) -> bool {
        !matches!(self.node(node).kind, Kind::Nop | Kind::Block | Kind::VarDecl | Kind::ArrayDecl | Kind::Bound { .. } | Kind::BoundList | Kind::ParamList { .. } | Kind::ArgList)
    }

    /// `codebank_asm.check_label_bank_access(ast)`: an error for each
    /// reference to a bank-local label from outside its bank. The assembler
    /// is what guarantees the rule; this puts a BASIC line number on the
    /// common case. It cannot be done while parsing, as label banks are only
    /// assigned by the pass above.
    pub fn check_label_bank_access(&mut self, ast: NodeId) {
        let mut stack: Vec<(NodeId, i64)> = vec![(ast, 0)];
        let mut seen: HashSet<NodeId> = HashSet::new();
        while let Some((node, bank)) = stack.pop() {
            if !seen.insert(node) {
                continue;
            }
            if matches!(&self.node(node).kind, Kind::Unary { operator } if operator == "FARADDRESS") {
                continue; // FARPTR: naming a bank-local symbol is the point
            }
            for child in self.children(node).to_vec() {
                let child = Self::present(child);
                match self.token(child) {
                    "LABEL" => {
                        // A label whose parent is a BLOCK is a definition.
                        // Anywhere else it is a reference: an argument of
                        // GOTO, GOSUB or ON GOTO, or the operand of `@lbl`.
                        if self.token(node) != "BLOCK" {
                            let lineno = if self.has_lineno(node) { self.lineno(node) } else { self.lineno(child) };
                            self.check_label(child, lineno, bank);
                        }
                        // Never descend into a label.
                    }
                    "FUNCDECL" => {
                        // The routine is reached through its entry, with
                        // its own bank in scope.
                        let entry = self.entry(child);
                        stack.push((entry, self.bank_of(entry)));
                    }
                    // Reached through a call: not the definition.
                    "FUNCTION" => {}
                    _ => stack.push((child, bank)),
                }
            }
        }
    }

    /// `_check_label`.
    fn check_label(&mut self, label: NodeId, lineno: u32, bank: i64) {
        let target = self.bank_of(label);
        if target == 0 || target == bank {
            return;
        }
        let msg = format!(
            "'{}' lives in CODEBANK {target} and cannot be reached from {}. Bank-local data is only addressable while its own bank is paged in",
            self.id(label).name,
            where_(bank)
        );
        let fname = self.id(label).filename.clone();
        self.error_at(lineno, &msg, Some(&fname));
    }

    /// `_addressed_symbols`: every symbol an AT address names, as (bank,
    /// name). The test for an array is against a token no node has, so an
    /// array is walked into instead (and holds nothing but its bounds).
    fn addressed_symbols(&self, addr: NodeId) -> Vec<(i64, String)> {
        let mut out = Vec::new();
        let mut stack = vec![Some(addr)];
        let mut seen: HashSet<NodeId> = HashSet::new();
        while let Some(node) = stack.pop() {
            let Some(node) = node else { continue };
            if !seen.insert(node) {
                continue;
            }
            if matches!(self.token(node), "LABEL" | "VAR") {
                out.push((self.bank_of(node), self.id(node).name.clone()));
                continue; // never descend into a symbol's own definition
            }
            stack.extend(self.children(node).iter().copied());
        }
        out
    }

    /// `codebank_asm.check_at_address_banks(data_ast)`: an error for a
    /// `DIM ... AT addr` whose address lies in another bank than the
    /// declaration. The descriptor of such an array holds the address, and
    /// has to be mapped together with what it points at.
    pub fn check_at_address_banks(&mut self, data_ast: NodeId) {
        for node in self.children(data_ast).to_vec() {
            let node = Self::present(node);
            if !matches!(self.node(node).kind, Kind::VarDecl | Kind::ArrayDecl | Kind::FuncDecl | Kind::Call(_) | Kind::FuncCall(_) | Kind::ArrayAccess(_) | Kind::ArrayLoad(_)) {
                continue;
            }
            let Some(entry) = self.child(node, 0) else { continue };
            let Kind::Id(data) = &self.node(entry).kind else { continue };
            let Some(addr) = data.addr else { continue };
            let bank = data.bank();
            for (target, name) in self.addressed_symbols(addr) {
                if target == 0 || target == bank {
                    continue;
                }
                let msg = format!(
                    "'{}' is declared in {} but its AT address '{name}' lives in CODEBANK {target}. The two must be in the same bank, because the array descriptor holding that address is only mapped with it",
                    self.id(entry).name,
                    where_(bank)
                );
                let (lineno, fname) = (self.lineno(entry), self.id(entry).filename.clone());
                self.error_at(lineno, &msg, Some(&fname));
                break;
            }
        }
    }

    /// `codebank_options.validate_codebank_options()`: checks the geometry
    /// of the code window, reporting the first problem. Done after parsing,
    /// so that a `#pragma` has had its say.
    pub fn validate_codebank_options(&mut self) -> bool {
        let size = self.opts.int("codewindowsize");
        let window = self.opts.int("codewindow");
        if size != 8192 && size != 16384 {
            self.error(0, &format!("Invalid code window size {size}. It must be 8192 (one 8K MMU slot) or 16384 (two)"));
            return false;
        }
        if window.rem_euclid(8192) != 0 {
            let msg = format!("The code window address ${} is not on an 8K boundary. It is paged by whole MMU slots, so it must be a multiple of 8192", hex4(window));
            self.error(0, &msg);
            return false;
        }
        if window + size > 0x10000 {
            let msg = format!("The code window (${}-${}) runs past the end of the address space", hex4(window), hex4(window + size - 1));
            self.error(0, &msg);
            return false;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::OptValue;
    use zxr_core::Session;

    fn compiler() -> Compiler {
        let mut sess = Session::default();
        sess.stderr = Box::new(std::io::sink());
        Compiler::new(sess)
    }

    #[test]
    fn labels_follow_the_asm_block_they_name() {
        let mut c = compiler();
        let table = c.declare_label("table", 1).unwrap();
        let line = c.declare_label("100", 2).unwrap();
        let other = c.declare_label("other", 3).unwrap();
        let banked = c.new_asm("db 1", 2, false, 3);
        let nested = c.new_asm("nop", 5, false, 3);
        let inner = c.make_block(&[Some(nested)]);
        let cond = c.new_number(crate::types::Num::Int(1), 5, None);
        let if_ = c.new_sentence(5, "IF", &[Some(cond), Some(inner)], false);
        let print = c.new_sentence(6, "PRINT", &[], false);
        let late = c.new_asm("db 2", 7, false, 3);
        // table: asm / 100 other: IF .. asm .. / PRINT / asm
        let ast = c.make_block(&[Some(table), Some(banked), Some(line), Some(other), Some(if_), Some(print), Some(late)]);
        c.bind_module_level_asm(ast);
        assert_eq!(c.bank_of(table), 3);
        assert_eq!(c.bank_of(line), 0);
        assert_eq!(c.bank_of(other), 0);
        assert_eq!(c.asm_bank(banked), 3);
        assert_eq!(c.asm_bank(nested), 0);
        assert_eq!(c.asm_bank(late), 3);

        // A GOTO to the banked label from the resident program is an error.
        let goto = c.new_sentence(9, "GOTO", &[Some(table)], false);
        c.append_child(ast, Some(goto));
        c.check_label_bank_access(ast);
        assert_eq!(c.sess.has_errors, 1);
    }

    #[test]
    fn code_window_geometry() {
        let mut c = compiler();
        assert!(c.validate_codebank_options());
        c.set_option("codewindowsize", OptValue::Int(4096)).unwrap();
        assert!(!c.validate_codebank_options());
        c.set_option("codewindowsize", OptValue::Int(16384)).unwrap();
        c.set_option("codewindow", OptValue::Int(0xE000)).unwrap();
        assert!(!c.validate_codebank_options());
        c.set_option("codewindow", OptValue::Int(0x6100)).unwrap();
        assert!(!c.validate_codebank_options());
        assert_eq!(hex4(0x6100), "6100");
        assert_eq!(hex4(255), "00FF");
    }
}
