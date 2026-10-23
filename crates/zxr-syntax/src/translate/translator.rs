//! `translator.py`: the Translator, which turns the statements and
//! expressions of the main body, and through `FunctionTranslator` those of
//! every function, into intermediate code.
//!
//! The order of the calls inside each method is the order of the Python
//! source, argument by argument: reading a node's `t` may take a new
//! temporary, and the numbers of the temporaries are part of the output.

use super::inst::tsuffix;
use super::{get_dehl, runtime};
use crate::ast::*;
use crate::compiler::*;
use crate::pyval::{self, crash};
use crate::types::{BasicType, Num};

impl Compiler {
    /// `_visit(node)` of `Translator` and `FunctionTranslator`: the method
    /// named after the node's token.
    pub(super) fn translator_visit(&mut self, node: NodeId, token: &str) {
        match token {
            "BLOCK" => self.tr_visit_block(node),
            "NOP" => {}
            "CLS" => self.runtime_call(runtime::CLS, 0),
            // visit_NUMBER gives back the value and does nothing else.
            "NUMBER" => {}
            "STRING" => self.tr_visit_string(node),
            "END" => {
                let code = self.kid(node, 0);
                self.tr_visit(Some(code));
                let t = self.t(code);
                self.ic_end(&t);
            }
            "ERROR" => {
                // Raises an error.
                let code = self.kid(node, 0);
                self.tr_visit(Some(code));
                let t = self.t(code);
                self.ic_fparam(BasicType::UByte, &t);
                self.runtime_call(runtime::ERROR, 0);
            }
            "STOP" => {
                // Returns to BASIC with an error code.
                let code = self.kid(node, 0);
                self.tr_visit(Some(code));
                let t = self.t(code);
                self.ic_fparam(BasicType::UByte, &t);
                self.runtime_call(runtime::STOP, 0);
                self.ic_end("0");
            }
            "LET" => self.tr_visit_let(node),
            "POKE" => self.tr_visit_poke(node),
            "RANDOMIZE" => {
                let seed = self.kid(node, 0);
                self.tr_visit(Some(seed));
                let (ty, t) = (self.tyb(seed), self.t(seed));
                self.ic_fparam(ty, &t);
                self.runtime_call(runtime::RANDOMIZE, 0);
            }
            "LABEL" => {
                // ZX Next: a label naming a diverted asm block moves into
                // the bank with it.
                if self.collect_if_diverted(node) {
                    return;
                }
                let mangled = self.id(node).mangled.clone();
                self.ic_label(&mangled);
            }
            "CONST" => {
                let RefKind::Const(symbol) = self.id(node).r.kind else { unreachable!() };
                self.tr_visit(Some(symbol));
            }
            "VAR" => self.tr_visit_var(node),
            "CONSTEXPR" => {
                // `yield node.t`: the text is worked out, which is where an
                // expression that is not constant after all is reported.
                self.t(node);
            }
            "VARARRAY" => {}
            "UNARY" => self.tr_visit_unary(node),
            "BUILTIN" => self.tr_visit_builtin(node),
            "BINARY" => self.tr_visit_binary(node),
            "TYPECAST" => {
                let operand = self.kid(node, 0);
                self.tr_visit(Some(operand));
                let t = self.t(node);
                let (from, to) = (self.tyb(operand), self.tyb(node));
                let operand_t = self.t(operand);
                self.ic_cast(&t, from, to, &operand_t);
            }
            "FUNCDECL" => {
                // The function is translated after the main code. A nested
                // one joins the list FunctionTranslator is working through,
                // which is the same list.
                let entry = self.entry(node);
                self.functions.push(entry);
            }
            "FUNCTION" if self.tr.mode == super::Mode::Function => self.tr_visit_function(node),
            "CALL" => self.tr_visit_call(node),
            "ARGLIST" => {
                // In reverse order.
                for i in (0..self.children(node).len()).rev() {
                    let arg = self.children(node)[i];
                    self.tr_visit(arg);
                }
            }
            "ARGUMENT" => self.tr_visit_argument(node),
            "ARRAYLOAD" => self.tr_visit_arrayload(node),
            "ARRAYCOPY" => self.tr_visit_arraycopy(node),
            "LETARRAY" => self.tr_visit_letarray(node),
            "LETSUBSTR" => self.tr_visit_letsubstr(node),
            "LETARRAYSUBSTR" => self.tr_visit_letarraysubstr(node),
            "ARRAYACCESS" => {
                let arglist = self.child(node, 1);
                self.tr_visit(arglist);
            }
            "STRSLICE" => self.tr_visit_strslice(node),
            "FUNCCALL" => self.tr_visit_funccall(node),
            "RESTORE" => self.tr_visit_restore(node),
            "READ" => self.tr_visit_read(node),
            "DO_LOOP" => self.tr_visit_do_loop(node),
            "DO_UNTIL" | "UNTIL_DO" => self.tr_visit_until_do(node, token),
            "DO_WHILE" | "WHILE_DO" => self.tr_visit_do_while(node, token),
            "EXIT_DO" => self.jump_out("DO", false),
            "EXIT_WHILE" => self.jump_out("WHILE", false),
            "EXIT_FOR" => self.jump_out("FOR", false),
            "CONTINUE_DO" => self.jump_out("DO", true),
            "CONTINUE_WHILE" => self.jump_out("WHILE", true),
            "CONTINUE_FOR" => self.jump_out("FOR", true),
            "FOR" => self.tr_visit_for(node),
            "GOTO" => {
                let label = self.id(self.kid(node, 0)).mangled.clone();
                self.ic_jump(&label);
            }
            "GOSUB" => {
                let label = self.id(self.kid(node, 0)).mangled.clone();
                self.ic_call(&label, 0);
            }
            "ON_GOTO" => self.tr_visit_on_goto(node, runtime::ON_GOTO),
            "ON_GOSUB" => self.tr_visit_on_goto(node, runtime::ON_GOSUB),
            "CHKBREAK" => {
                // PREV_TOKEN is never set, so the check is always made.
                self.ic_inline("push hl");
                let t = self.t(self.kid(node, 0));
                self.ic_fparam(PTR_TYPE, &t);
                self.runtime_call(runtime::CHECK_BREAK, 0);
            }
            "IF" => self.tr_visit_if(node),
            "RETURN" => self.tr_visit_return(node),
            "WHILE" => self.tr_visit_while(node),
            "PLOT" => self.tr_visit_draw(node, 2, runtime::PLOT),
            "DRAW" => self.tr_visit_draw(node, 2, runtime::DRAW),
            "DRAW3" => self.tr_visit_draw(node, 3, runtime::DRAW3),
            "CIRCLE" => self.tr_visit_draw(node, 3, runtime::CIRCLE),
            "OUT" => {
                let (port, value) = (self.kid(node, 0), self.kid(node, 1));
                self.tr_visit(Some(port));
                self.tr_visit(Some(value));
                let (t1, t2) = (self.t(port), self.t(value));
                self.ic_out(&t1, &t2);
            }
            "PRINT" => self.tr_visit_print(node),
            "PRINT_AT" => {
                let (row, column) = (self.kid(node, 0), self.kid(node, 1));
                self.tr_visit(Some(row));
                let t = self.t(row);
                self.ic_param(BasicType::UByte, &t);
                self.tr_visit(Some(column));
                let t = self.t(column);
                self.ic_fparam(BasicType::UByte, &t);
                self.runtime_call(runtime::PRINT_AT, 0); // a procedure call: the result is dropped
            }
            "PRINT_TAB" => {
                let column = self.kid(node, 0);
                self.tr_visit(Some(column));
                let t = self.t(column);
                self.ic_fparam(BasicType::UByte, &t);
                self.runtime_call(runtime::PRINT_TAB, 0);
            }
            "PRINT_COMMA" => self.runtime_call(runtime::PRINT_COMMA, 0),
            "LOAD" | "VERIFY" => self.tr_visit_load(node, token),
            "SAVE" => {
                self.tr_visit_load_args(node);
                self.runtime_call(runtime::SAVE_CODE, 0);
            }
            "BORDER" => {
                let colour = self.kid(node, 0);
                self.tr_visit(Some(colour));
                let t = self.t(colour);
                self.ic_fparam(BasicType::UByte, &t);
                self.runtime_call(runtime::BORDER, 0);
            }
            "BEEP" => self.tr_visit_beep(node),
            "PAUSE" => {
                let time = self.kid(node, 0);
                self.tr_visit(Some(time));
                let (ty, t) = (self.tyb(time), self.t(time));
                self.ic_fparam(ty, &t);
                self.runtime_call(runtime::PAUSE, 0);
            }
            "INK" => self.tr_visit_attr_sentence(node, runtime::INK),
            "PAPER" => self.tr_visit_attr_sentence(node, runtime::PAPER),
            "BRIGHT" => self.tr_visit_attr_sentence(node, runtime::BRIGHT),
            "FLASH" => self.tr_visit_attr_sentence(node, runtime::FLASH),
            "INVERSE" => self.tr_visit_attr_sentence(node, runtime::INVERSE),
            "OVER" => self.tr_visit_attr_sentence(node, runtime::OVER),
            "BOLD" => self.tr_visit_attr_sentence(node, runtime::BOLD),
            "ITALIC" => self.tr_visit_attr_sentence(node, runtime::ITALIC),
            "ASM" => {
                // ZX Next: a module-level block inside a CODEBANK block
                // belongs in that bank. It is parked now and replayed inside
                // the bank's fence.
                if self.collect_if_diverted(node) {
                    return;
                }
                self.emit_asm_node(node);
            }
            other => self.no_visit_method(other),
        }
    }

    fn tr_visit_string(&mut self, node: NodeId) {
        let Kind::Str { value } = &self.node(node).kind else { unreachable!() };
        let value = value.clone();
        let label = self.add_string_label(&value);
        self.node_mut(node).t = Some(format!("#{label}"));
    }

    fn tr_visit_let(&mut self, node: NodeId) {
        let (var, expr) = (self.kid(node, 0), self.kid(node, 1));
        if self.token(var) != "VAR" {
            crash("AssertionError: node.children[0].token == \"VAR\"");
        }
        if self.o_level() < 2 || self.accessed(var) || self.token(expr) == "CONSTEXPR" {
            self.tr_visit(Some(expr));
        }
        self.emit_let_left_part(node, None);
    }

    fn tr_visit_poke(&mut self, node: NodeId) {
        let (ch0, ch1) = (self.kid(node, 0), self.kid(node, 1));
        self.tr_visit(Some(ch0));
        self.tr_visit(Some(ch1));
        let ty = self.tyb(ch1);
        let indirect = self.token(ch0) == "VAR" && self.class_(ch0) != Class::Const && self.scope(ch0) == Scope::Global;
        let address = self.t(ch0);
        let value = self.t(ch1);
        if indirect {
            self.ic_store(ty, &format!("*{address}"), &value);
        } else {
            self.ic_store(ty, &address, &value);
        }
    }

    fn tr_visit_var(&mut self, node: NodeId) {
        let scope = self.id(node).scope;
        let t = self.t(node);
        if t == self.id(node).mangled && scope == Scope::Global {
            return;
        }
        let p = if self.byref(node) { "*" } else { "" }; // the indirection prefix
        let ty = self.tyb(node);
        match scope {
            Scope::Parameter => {
                let t = self.t(node);
                self.ic_pload(ty, &t, &format!("{p}{}", self.offset_str(node)));
            }
            Scope::Local => {
                let offset = self.offset(node);
                let t = self.t(node);
                self.ic_pload(ty, &t, &format!("{p}{}", -offset));
            }
            Scope::Global => {}
        }
    }

    fn tr_visit_binary(&mut self, node: NodeId) {
        let (left, right) = (self.kid(node, 0), self.kid(node, 1));
        self.tr_visit(Some(left));
        self.tr_visit(Some(right));
        let Kind::Binary { operator, .. } = &self.node(node).kind else { unreachable!() };
        let ins = match operator.as_str() {
            "PLUS" => "add".to_string(),
            "MINUS" => "sub".to_string(),
            other => other.to_lowercase(),
        };
        let ty = self.tyb(left);
        // The node's own temporary is taken before those of its operands.
        let t = self.t(node);
        let (t1, t2) = (self.t(left), self.t(right));
        match ins.as_str() {
            "add" => self.ic_add(ty, &t, &t1, &t2),
            "sub" => self.ic_sub(ty, &t, &t1, &t2),
            "mul" => self.ic_mul(ty, &t, &t1, &t2),
            "div" => self.ic_div(ty, &t, &t1, &t2),
            "or" => self.ic_or(ty, &t, &t1, &t2),
            "xor" => self.ic_xor(ty, &t, &t1, &t2),
            "and" => self.ic_and(ty, &t, &t1, &t2),
            "eq" => self.ic_eq(ty, &t, &t1, &t2),
            "ne" => self.ic_ne(ty, &t, &t1, &t2),
            "gt" => self.ic_gt(ty, &t, &t1, &t2),
            "lt" => self.ic_lt(ty, &t, &t1, &t2),
            "le" => self.ic_le(ty, &t, &t1, &t2),
            "ge" => self.ic_ge(ty, &t, &t1, &t2),
            "band" => self.ic_band(ty, &t, &t1, &t2),
            "bor" => self.ic_bor(ty, &t, &t1, &t2),
            "bxor" => self.ic_bxor(ty, &t, &t1, &t2),
            "shl" => self.ic_shl(ty, &t, &t1, &t2),
            "shr" => self.ic_shr(ty, &t, &t1, &t2),
            "pow" => self.ic_pow(ty, &t, &t1, &t2),
            "mod" => self.ic_mod(ty, &t, &t1, &t2),
            other => crash(&format!("KeyError: '{other}'")),
        }
    }

    /// `_check_byref_codebank`: warns when a bank-local variable is passed
    /// ByRef out of its own bank. The callee gets a bare address in the code
    /// window, which only means anything while the variable's bank is paged
    /// in.
    fn check_byref_codebank(&mut self, node: NodeId) {
        let callee = self.entry(node);
        let callee_bank = self.bank_of_func(callee);
        let args = self.kid(node, 1);
        for arg in self.children(args).to_vec().into_iter().flatten() {
            if !self.arg_byref(arg) {
                continue;
            }
            // getattr(value, "entry", value)
            let entry = self.child(arg, 0).map(|value| match self.node(value).kind {
                Kind::VarDecl | Kind::ArrayDecl | Kind::FuncDecl | Kind::Call(_) | Kind::FuncCall(_) | Kind::ArrayAccess(_) | Kind::ArrayLoad(_) => self.entry(value),
                _ => value,
            });
            let bank = self.bank_of_var(entry);
            if bank != 0 && bank != callee_bank {
                let (lineno, name, callee_name) = (self.lineno(node), self.name(entry.unwrap()).to_string(), self.id(callee).name.clone());
                self.warning_byref_leaves_codebank(lineno, &name, bank, &callee_name, None);
            }
        }
    }

    /// The fastcall convention's first argument goes in registers.
    fn fastcall_first_param(&mut self, node: NodeId) {
        let entry = self.entry(node);
        let convention = match self.id(entry).func() {
            Some(func) => func.convention,
            None => crash("AttributeError: 'VarRef' object has no attribute 'convention'"),
        };
        if convention == Convention::Fastcall {
            let args = self.kid(node, 1);
            if !self.children(args).is_empty() {
                let ty = self.tyb(self.kid(args, 0));
                let t = self.new_t();
                self.ic_fparam(ty, &t);
            }
        }
    }

    fn tr_visit_call(&mut self, node: NodeId) {
        self.check_byref_codebank(node);
        let args = self.child(node, 1);
        self.tr_visit(args);
        self.fastcall_first_param(node);
        let entry = self.entry(node);
        let mangled = self.id(entry).mangled.clone();
        self.ic_call(&mangled, 0); // a procedure call: 0 drops the result
        if self.class_(entry) == Class::Function && self.ty(entry).is_some_and(|t| t == BasicType::String) {
            // The string the function returned is freed.
            self.runtime_call(runtime::MEM_FREE, 0);
        }
    }

    fn tr_visit_funccall(&mut self, node: NodeId) {
        self.check_byref_codebank(node);
        let args = self.child(node, 1);
        self.tr_visit(args);
        self.fastcall_first_param(node);
        let entry = self.entry(node);
        let mangled = self.id(entry).mangled.clone();
        let size = self.ty(entry).map_or(0, |t| t.size());
        self.ic_call(&mangled, size);
    }

    fn tr_visit_argument(&mut self, node: NodeId) {
        let value = self.child(node, 0).unwrap_or_else(|| crash("AttributeError: 'NoneType' object has no attribute 'token'"));
        if !self.arg_byref(node) {
            let ty = self.ty_of(node);
            if self.token(value) == "VAR" && ty.is_dynamic() && self.t(value).starts_with('$') {
                // A local string: it is duplicated in the heap.
                let scope = self.scope(value);
                let t = self.t(node);
                match scope {
                    Scope::Local => self.ic_pload(ty.basic, &t, &(-self.offset(value)).to_string()),
                    Scope::Parameter => self.ic_pload(ty.basic, &t, &self.offset_str(value)),
                    Scope::Global => crash("AssertionError: node.value.scope in (SCOPE.local, SCOPE.parameter)"),
                }
            } else {
                self.tr_visit(Some(value));
            }
            // The value may have been replaced by the visit.
            let ty = self.tyb(node);
            let t = self.t(node);
            self.ic_param(ty, &t);
            return;
        }

        // A ByRef argument.
        if !matches!(self.token(value), "ARRAYLOAD" | "ARRAYACCESS") {
            let scope = self.scope(value);
            let own = self.t(node);
            let t = if own.starts_with('_') { self.new_t() } else { own };
            match scope {
                Scope::Global => {
                    let mangled = self.id(value).mangled.clone();
                    self.ic_load(BasicType::UInteger, &t, &format!("#{mangled}"));
                }
                Scope::Parameter => {
                    // A function passing one of its parameters on.
                    if !self.byref(value) {
                        // It is like a local variable.
                        let ty = self.tyb(node);
                        let offset = if matches!(ty, BasicType::Byte | BasicType::UByte) { 1 } else { 0 };
                        self.ic_paddr(&(self.offset(value) + offset).to_string(), &t);
                    } else {
                        self.ic_pload(PTR_TYPE, &t, &self.offset_str(value));
                    }
                }
                Scope::Local => self.ic_paddr(&(-self.offset(value)).to_string(), &t),
            }
            self.ic_param(BasicType::UInteger, &t);
            return;
        }

        // The address of @array(...) must be computed.
        let ptr = self.type_(PTR_TYPE);
        let offset = if self.o_level() > 1 && self.scope(value) == Scope::Global { self.array_offset(value) } else { None };
        let new_value = if let Some(offset) = offset {
            // A global: the offset is known.
            let (entry, line) = (self.entry(value), self.lineno(value));
            let address = self.new_unary("ADDRESS", entry, line, Some(ptr));
            let number = self.new_number(offset, line, Some(ptr));
            let lineno = self.lineno(node);
            self.make_binary_node("PLUS", Some(address), Some(number), lineno, Some(pyval::fold::add), None)
        } else {
            let copy = self.array_access_copy_from(value);
            self.set_child(node, 0, Some(copy));
            let lineno = self.lineno(node);
            Some(self.new_unary("ADDRESS", copy, lineno, Some(ptr)))
        };
        self.set_child(node, 0, new_value);
        self.tr_visit(new_value);
    }

    fn tr_visit_arrayload(&mut self, node: NodeId) {
        let entry = self.entry(node);
        let scope = self.id(entry).scope;
        let ty = self.tyb(node);
        match self.array_offset(node) {
            None => {
                let args = self.child(node, 1);
                self.tr_visit(args);
                match scope {
                    Scope::Global => {
                        let (t, mangled) = (self.t(entry), self.id(entry).mangled.clone());
                        self.ic_aload(ty, &t, &mangled);
                    }
                    Scope::Parameter => {
                        let t = self.t(node);
                        self.ic_paload(ty, &t, &format!("*{}", self.offset_str(entry)));
                    }
                    Scope::Local => {
                        let t = self.t(node);
                        self.ic_paload(ty, &t, &(-self.offset(entry)).to_string());
                    }
                }
            }
            Some(offset) => match scope {
                Scope::Global => {
                    let t = self.t(entry);
                    self.ic_load(ty, &t, &format!("{t} + {}", Self::percent_i(offset)));
                }
                Scope::Parameter => {
                    let t = self.t(node);
                    let address = pyval::sub(Num::Int(self.offset(entry) as i128), offset);
                    self.ic_pload(ty, &t, &address.to_string());
                }
                Scope::Local => {
                    let (t1, t2, t3) = (self.new_t(), self.new_t(), self.new_t());
                    self.ic_pload(PTR_TYPE, &t1, &(-self.offset(entry) + PTR_TYPE.size()).to_string());
                    self.ic_add(PTR_TYPE, &t2, &t1, &offset.to_string());
                    self.ic_load(ty, &t3, &format!("*${t2}"));
                }
            },
        }
    }

    fn emit_arraycopy_child(&mut self, child: NodeId) -> String {
        if self.token(child) != "VARARRAY" {
            crash("AssertionError: child.token == \"VARARRAY\"");
        }
        match self.id(child).scope {
            Scope::Global => format!("#{}", self.data_label(child)),
            Scope::Parameter => {
                let t = self.new_t();
                self.ic_pload(PTR_TYPE, &t, &(self.offset(child) - PTR_TYPE.size()).to_string());
                t
            }
            Scope::Local => {
                let t = self.new_t();
                self.ic_pload(PTR_TYPE, &t, &(-(self.offset(child) - PTR_TYPE.size())).to_string());
                t
            }
        }
    }

    fn tr_visit_arraycopy(&mut self, node: NodeId) {
        let (t_source, t_dest) = (self.kid(node, 1), self.kid(node, 0));
        let t = self.new_t();
        if self.ty_of(t_source) != BasicType::String {
            let t1 = self.emit_arraycopy_child(t_dest);
            let t2 = self.emit_arraycopy_child(t_source);
            self.ic_load(BOUND_TYPE, &t, &self.size(t_source).to_string());
            self.ic_memcopy(&t1, &t2, &t);
        } else {
            let t2 = self.emit_arraycopy_child(t_dest);
            if self.scope(t_dest) == Scope::Global {
                let tmp = self.new_t();
                self.ic_load(PTR_TYPE, &tmp, &t2);
            }
            let t1 = self.emit_arraycopy_child(t_source);
            if self.scope(t_source) == Scope::Global {
                let tmp = self.new_t();
                self.ic_load(PTR_TYPE, &tmp, &t1);
            }
            self.ic_load(BOUND_TYPE, &t, &self.count(t_source).to_string());
            self.runtime_call(runtime::STR_ARRAYCOPY, 0);
        }
    }

    fn tr_visit_letarray(&mut self, node: NodeId) {
        let arr = self.kid(node, 0); // the array access
        let entry = self.entry(arr);
        if self.o_level() > 1 && !self.accessed(entry) {
            return;
        }
        let scope = self.id(entry).scope;
        let expr = self.kid(node, 1); // the right expression
        let ty = self.tyb(arr);
        match self.array_offset(arr) {
            None => {
                self.tr_visit(Some(expr));
                self.tr_visit(Some(arr));
                match scope {
                    Scope::Global => {
                        let (mangled, t) = (self.id(entry).mangled.clone(), self.t(expr));
                        self.ic_astore(ty, &mangled, &t);
                    }
                    Scope::Parameter => {
                        // Arrays are always passed ByRef.
                        let (address, t) = (format!("*{}", self.offset_str(entry)), self.t(expr));
                        self.ic_pastore(ty, &address, &t);
                    }
                    Scope::Local => {
                        let (address, t) = ((-self.offset(entry)).to_string(), self.t(expr));
                        self.ic_pastore(ty, &address, &t);
                    }
                }
            }
            Some(offset) => {
                let name = self.data_label(entry);
                match scope {
                    Scope::Global => {
                        self.tr_visit(Some(expr));
                        let t = self.t(expr);
                        self.ic_store(ty, &format!("{name} + {}", Self::percent_i(offset)), &t);
                    }
                    Scope::Local => {
                        let (t1, t2) = (self.new_t(), self.new_t());
                        self.ic_pload(PTR_TYPE, &t1, &(-(self.offset(entry) - PTR_TYPE.size())).to_string());
                        self.ic_add(PTR_TYPE, &t2, &t1, &offset.to_string());
                        self.tr_visit(Some(expr));
                        let t = self.t(expr);
                        if ty == BasicType::String {
                            self.ic_store(ty, &format!("*{t2}"), &t);
                        } else {
                            self.ic_store(ty, &t2, &t);
                        }
                    }
                    Scope::Parameter => {
                        crash(&format!("InternalError: Invalid scope {} for variable '{}'", "SCOPE.parameter", self.id(entry).name));
                    }
                }
            }
        }
    }

    /// The value and the two bounds of an assignment to a slice: what
    /// LETSUBSTR and LETARRAYSUBSTR have in common.
    fn substr_params(&mut self, node: NodeId) {
        // The string assigned.
        let expr = self.kid(node, 3);
        self.tr_visit(Some(expr));
        if self.is_temporary_value(expr) {
            let t = self.t(expr);
            self.ic_param(BasicType::String, &t);
            self.ic_param(BasicType::UByte, "1");
        } else {
            let t = self.t(expr);
            self.ic_param(PTR_TYPE, &t);
            self.ic_param(BasicType::UByte, "0");
        }
        for n in [1, 2] {
            let bound = self.kid(node, n);
            self.tr_visit(Some(bound));
            let t = self.t(bound);
            self.ic_param(PTR_TYPE, &t);
        }
    }

    /// `LET X$(a TO b) = Y$`.
    fn tr_visit_letsubstr(&mut self, node: NodeId) {
        self.substr_params(node);
        let str_var = self.kid(node, 0);
        match self.scope(str_var) {
            Scope::Global => {
                let t = self.t(str_var);
                self.ic_fparam(PTR_TYPE, &t);
            }
            Scope::Local => {
                let t = self.t(str_var);
                self.ic_pload(PTR_TYPE, &t, &(-self.offset(str_var)).to_string());
                let t = self.t(str_var);
                self.ic_fparam(PTR_TYPE, &t);
            }
            Scope::Parameter => {
                let t = self.t(str_var);
                self.ic_pload(PTR_TYPE, &t, &self.offset_str(str_var));
                let t = self.t(str_var);
                if self.byref(str_var) {
                    self.ic_fparam(PTR_TYPE, &format!("*{t}"));
                } else {
                    self.ic_fparam(PTR_TYPE, &t);
                }
            }
        }
        self.runtime_call(runtime::LETSUBSTR, 0);
    }

    fn tr_visit_letarraysubstr(&mut self, node: NodeId) {
        let access = self.kid(node, 0);
        let entry = self.entry(access);
        if self.o_level() > 1 && !self.accessed(entry) {
            return;
        }
        self.substr_params(node);
        let scope = self.id(entry).scope;
        // The address of the element.
        match self.array_offset(access) {
            None => {
                self.tr_visit(Some(access));
                let t = self.t(access);
                match scope {
                    Scope::Global => {
                        let mangled = self.id(entry).mangled.clone();
                        self.ic_aload(PTR_TYPE, &t, &mangled);
                    }
                    Scope::Parameter => self.ic_paload(PTR_TYPE, &t, &self.offset_str(entry)),
                    Scope::Local => self.ic_paload(PTR_TYPE, &t, &(-self.offset(entry)).to_string()),
                }
            }
            Some(offset) => match scope {
                Scope::Global => {
                    let (t, mangled) = (self.t(entry), self.id(entry).mangled.clone());
                    self.ic_load(PTR_TYPE, &t, &format!("{mangled}.__DATA__ + {}", Self::percent_i(offset)));
                }
                Scope::Parameter => {
                    let t = self.t(access);
                    let address = pyval::sub(Num::Int(self.offset(entry) as i128), offset);
                    self.ic_pload(PTR_TYPE, &t, &address.to_string());
                }
                Scope::Local => {
                    let t = self.t(access);
                    let address = pyval::neg(pyval::sub(Num::Int(self.offset(entry) as i128), offset));
                    self.ic_pload(PTR_TYPE, &t, &address.to_string());
                }
            },
        }
        let t = self.t(access);
        self.ic_fparam(PTR_TYPE, &t);
        self.runtime_call(runtime::LETSUBSTR, 0);
    }

    fn tr_visit_strslice(&mut self, node: NodeId) {
        let (string, lower, upper) = (self.kid(node, 0), self.kid(node, 1), self.kid(node, 2));
        self.tr_visit(Some(string));
        if self.token(string) == "STRING" || self.token(string) == "VAR" && self.scope(string) == Scope::Global {
            let t = self.t(string);
            self.ic_param(PTR_TYPE, &t);
        }
        // Now the indexes of the slice.
        for bound in [lower, upper] {
            self.tr_visit(Some(bound));
            let (ty, t) = (self.tyb(bound), self.t(bound));
            self.ic_param(ty, &t);
        }
        if self.token(string) == "VAR" && self.id(string).mangled.starts_with('_') || self.token(string) == "STRING" {
            self.ic_fparam(BasicType::UByte, "0");
        } else {
            // What is not a variable must be freed.
            self.ic_fparam(BasicType::UByte, "1");
        }
        self.runtime_call(runtime::STRSLICE, PTR_TYPE.size());
    }

    fn tr_visit_restore(&mut self, node: NodeId) {
        if !self.data_is_used {
            return; // with no READ, everything to do with DATA is left out
        }
        let (lbl, ty) = if self.children(node).is_empty() {
            // No label.
            let Some(first) = self.datas.first() else {
                let lineno = self.lineno(node);
                self.syntax_error_no_data_defined(lineno);
                return;
            };
            (self.id(first.label).name.clone(), PTR_TYPE)
        } else {
            let label = self.kid(node, 0);
            let name = self.name(label).to_string();
            let Some(lbl) = self.data_labels.get(&name) else { crash(&format!("KeyError: '{name}'")) };
            (lbl.clone(), self.tyb(label))
        };
        if !self.data_labels_required.contains(&lbl) {
            self.data_labels_required.push(lbl.clone());
        }
        self.ic_fparam(ty, &format!("#{lbl}"));
        self.runtime_call(runtime::RESTORE, 0);
    }

    fn tr_visit_read(&mut self, node: NodeId) {
        if self.datas.is_empty() {
            let lineno = self.lineno(node);
            self.syntax_error_no_data_defined(lineno);
            return;
        }
        let target = self.kid(node, 0);
        let ty = self.ty_of(target);
        self.ic_fparam(BasicType::UByte, &format!("#{}", super::data_type_code(tsuffix(ty.basic))));
        self.runtime_call(runtime::READ, ty.size());
        if !matches!(self.node(target).kind, Kind::ArrayAccess(_) | Kind::ArrayLoad(_)) {
            let t = self.new_t();
            self.emit_var_assign(target, &t);
            return;
        }
        let arr = target;
        let entry = self.entry(arr);
        let t = self.new_t();
        let scope = self.id(entry).scope;
        match self.array_offset(arr) {
            None => {
                self.tr_visit(Some(arr));
                match scope {
                    Scope::Global => {
                        let mangled = self.id(entry).mangled.clone();
                        self.ic_astore(ty.basic, &mangled, &t);
                    }
                    Scope::Parameter => self.ic_pastore(ty.basic, &self.offset_str(entry), &t),
                    Scope::Local => self.ic_pastore(ty.basic, &(-self.offset(entry)).to_string(), &t),
                }
            }
            Some(offset) => {
                let name = self.id(entry).mangled.clone();
                match scope {
                    Scope::Global => self.ic_store(ty.basic, &format!("{name} + {}", Self::percent_i(offset)), &t),
                    Scope::Parameter => {
                        let address = pyval::sub(Num::Int(self.offset(entry) as i128), offset);
                        self.ic_pstore(ty.basic, &address.to_string(), &t);
                    }
                    Scope::Local => {
                        let address = pyval::neg(pyval::sub(Num::Int(self.offset(entry) as i128), offset));
                        self.ic_pstore(ty.basic, &address.to_string(), &t);
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // Control flow: FOR, IF, WHILE, DO UNTIL...
    // ------------------------------------------------------------------

    fn tr_visit_do_loop(&mut self, node: NodeId) {
        let loop_label = self.tmp_label();
        let end_loop = self.tmp_label();
        // The labels EXIT and CONTINUE jump to.
        self.tr.loops.push(("DO", end_loop.clone(), loop_label.clone()));
        self.ic_label(&loop_label);
        if !self.children(node).is_empty() {
            let body = self.children(node)[0];
            self.tr_visit(body);
        }
        self.ic_jump(&loop_label);
        self.ic_label(&end_loop);
        self.tr.loops.pop();
    }

    /// `visit_DO_WHILE`, which `WHILE_DO` shares.
    fn tr_visit_do_while(&mut self, node: NodeId, token: &str) {
        let loop_label = self.tmp_label();
        let end_loop = self.tmp_label();
        let continue_loop = self.tmp_label();
        if token == "WHILE_DO" {
            self.ic_jump(&continue_loop);
        }
        self.ic_label(&loop_label);
        self.tr.loops.push(("DO", end_loop.clone(), continue_loop.clone()));
        if self.children(node).len() > 1 {
            let body = self.children(node)[1];
            self.tr_visit(body);
        }
        self.ic_label(&continue_loop);
        let condition = self.kid(node, 0);
        self.tr_visit(Some(condition));
        let (ty, t) = (self.tyb(condition), self.t(condition));
        self.ic_jnzero(ty, &t, &loop_label);
        self.ic_label(&end_loop);
        self.tr.loops.pop();
    }

    /// `visit_UNTIL_DO`, which `DO_UNTIL` shares.
    fn tr_visit_until_do(&mut self, node: NodeId, token: &str) {
        let loop_label = self.tmp_label();
        let end_loop = self.tmp_label();
        let continue_loop = self.tmp_label();
        if token == "UNTIL_DO" {
            self.ic_jump(&continue_loop);
        }
        self.ic_label(&loop_label);
        self.tr.loops.push(("DO", end_loop.clone(), continue_loop.clone()));
        if self.children(node).len() > 1 {
            let body = self.children(node)[1];
            self.tr_visit(body);
        }
        self.ic_label(&continue_loop);
        let condition = self.kid(node, 0);
        self.tr_visit(Some(condition));
        let (ty, t) = (self.tyb(condition), self.t(condition));
        self.ic_jzero(ty, &t, &loop_label);
        self.ic_label(&end_loop);
        self.tr.loops.pop();
    }

    fn tr_visit_while(&mut self, node: NodeId) {
        let loop_label = self.tmp_label();
        let end_loop = self.tmp_label();
        self.tr.loops.push(("WHILE", end_loop.clone(), loop_label.clone()));
        self.ic_label(&loop_label);
        let condition = self.kid(node, 0);
        self.tr_visit(Some(condition));
        let (ty, t) = (self.tyb(condition), self.t(condition));
        self.ic_jzero(ty, &t, &end_loop);
        if self.children(node).len() > 1 {
            let body = self.children(node)[1];
            self.tr_visit(body);
        }
        self.ic_jump(&loop_label);
        self.ic_label(&end_loop);
        self.tr.loops.pop();
    }

    /// `visit_EXIT_*` and `visit_CONTINUE_*`: `loop_exit_label` or
    /// `loop_cont_label` of the innermost loop of the kind.
    fn jump_out(&mut self, loop_type: &str, to_continue: bool) {
        let Some(found) = self.tr.loops.iter().rev().find(|l| l.0 == loop_type) else {
            crash(&format!("InvalidLoopError: Invalid loop type error (not found?) \"{loop_type}\""));
        };
        let label = if to_continue { found.2.clone() } else { found.1.clone() };
        self.ic_jump(&label);
    }

    /// `_for_wraps_around`: whether the exit test of a FOR loop can never
    /// fire, because adding STEP to the iterator wraps around its type before
    /// it can pass the limit, as in `FOR n AS UBYTE = 0 TO 255`. Only for a
    /// constant limit and step. With it comes the guard: the constant the
    /// iterator must be compared against before it is incremented, or
    /// nothing if the loop can never run more than once.
    fn for_wraps_around(&mut self, node: NodeId) -> (bool, Option<Num>) {
        let (limit, step) = (self.kid(node, 2), self.kid(node, 3));
        if !self.is_number(&[limit, step]) {
            return (false, None);
        }
        let (limit, step) = (self.num_value(limit), self.num_value(step));
        if step.is_zero() {
            return (false, None);
        }
        let ty = self.tyb(self.kid(node, 0));
        if !ty.is_integral() {
            return (false, None); // it does not wrap
        }
        let bits = 8 * ty.size() as u32;
        let (lo, hi): (i128, i128) = if ty.is_unsigned() { (0, (1 << bits) - 1) } else { (-(1 << (bits - 1)), (1 << (bits - 1)) - 1) };
        let within = |n: Num| Num::Int(lo) <= n && n <= Num::Int(hi);
        if !within(pyval::add(limit, step)) {
            // The variable can never pass the limit: it is tested against
            // limit - step instead, which cannot wrap.
            let guard = pyval::sub(limit, step);
            return (true, within(guard).then_some(guard));
        }
        (false, None)
    }

    fn tr_visit_for(&mut self, node: NodeId) {
        let loop_label_start = self.tmp_label();
        let loop_label_gt = self.tmp_label();
        let end_loop = self.tmp_label();
        let loop_body = self.tmp_label();
        let loop_continue = self.tmp_label();
        let (var, start, limit, step, body) = (self.kid(node, 0), self.kid(node, 1), self.kid(node, 2), self.kid(node, 3), self.child(node, 4));
        let ty = self.tyb(var);
        self.tr.loops.push(("FOR", end_loop.clone(), loop_continue.clone()));

        let (wraps, guard) = self.for_wraps_around(node);
        if wraps {
            self.emit_wrap_safe_for(node, guard, &end_loop, &loop_body, &loop_continue);
            self.tr.loops.pop();
            return;
        }

        self.tr_visit(Some(start)); // the starting value
        self.emit_let_left_part(node, None); // stored in the iterator variable
        self.ic_jump(&loop_label_start);

        // The statements of the body.
        self.ic_label(&loop_body);
        if self.children(node).len() <= 4 {
            crash("IndexError: list index out of range");
        }
        self.tr_visit(body);

        // CONTINUE comes here.
        self.ic_label(&loop_continue);

        // VAR = VAR + STEP
        self.tr_visit(Some(var));
        self.tr_visit(Some(step));
        let t = self.new_t();
        let (t1, t2) = (self.t(var), self.t(step));
        self.ic_add(ty, &t, &t1, &t2);
        self.emit_let_left_part(node, Some(&t));

        // The loop starts here, with the condition. `is_unsigned` is given
        // the step's type where it takes nodes, so it is never true.
        self.ic_label(&loop_label_start);
        let direct = self.is_number(&[step]);
        if !direct {
            self.tr_visit(Some(step));
            let t = self.t(step);
            self.ic_jgezero(ty, &t, &loop_label_gt);
        }
        if !direct || self.num_value(step) < Num::Int(0) {
            // For negative steps: is var < limit?
            self.tr_visit(Some(var));
            self.tr_visit(Some(limit));
            let t = self.t(node);
            let (t1, t2) = (self.t(var), self.t(limit));
            self.ic_lt(ty, &t, &t1, &t2);
            let t = self.t(node);
            self.ic_jzero(BasicType::UByte, &t, &loop_body);
        }
        if !direct {
            self.ic_jump(&end_loop);
            self.ic_label(&loop_label_gt);
        }
        if !direct || self.num_value(step) >= Num::Int(0) {
            // For positive steps: is var > limit?
            self.tr_visit(Some(var));
            self.tr_visit(Some(limit));
            let t = self.t(node);
            let (t1, t2) = (self.t(var), self.t(limit));
            self.ic_gt(ty, &t, &t1, &t2);
            let t = self.t(node);
            self.ic_jzero(BasicType::UByte, &t, &loop_body);
        }
        self.ic_label(&end_loop);
        self.tr.loops.pop();
    }

    /// `_emit_wrap_safe_for`: a FOR loop whose iterator would wrap around
    /// before it could pass the limit. The exit test is made ahead of the
    /// increment, against `guard` (limit - step), so that neither can
    /// overflow.
    fn emit_wrap_safe_for(&mut self, node: NodeId, guard: Option<Num>, end_loop: &str, loop_body: &str, loop_continue: &str) {
        let (var, start, limit, step, body) = (self.kid(node, 0), self.kid(node, 1), self.kid(node, 2), self.kid(node, 3), self.child(node, 4));
        let ty = self.ty_of(var);
        let ascending = self.num_value(step) > Num::Int(0);
        let compare = |c: &mut Compiler, t: &str, t1: &str, t2: &str| if ascending { c.ic_gt(ty.basic, t, t1, t2) } else { c.ic_lt(ty.basic, t, t1, t2) };

        self.tr_visit(Some(start)); // the starting value
        self.emit_let_left_part(node, None); // stored in the iterator variable

        // The loop is skipped if it starts out past the limit.
        self.tr_visit(Some(var));
        self.tr_visit(Some(limit));
        let t = self.new_t();
        let (t1, t2) = (self.t(var), self.t(limit));
        compare(self, &t, &t1, &t2);
        self.ic_jnzero(BasicType::UByte, &t, end_loop);

        // The statements of the body.
        self.ic_label(loop_body);
        if self.children(node).len() <= 4 {
            crash("IndexError: list index out of range");
        }
        self.tr_visit(body);

        // CONTINUE comes here.
        self.ic_label(loop_continue);
        let Some(guard) = guard else {
            // limit - step cannot be represented, so there is no second
            // iteration.
            self.ic_label(end_loop);
            return;
        };

        // Out before the increment, so that the iterator never wraps.
        let lineno = self.lineno(node);
        let guard = self.new_number(guard, lineno, Some(ty));
        self.tr_visit(Some(var));
        self.tr_visit(Some(guard));
        let t = self.new_t();
        let (t1, t2) = (self.t(var), self.t(guard));
        compare(self, &t, &t1, &t2);
        self.ic_jnzero(BasicType::UByte, &t, end_loop);

        // VAR = VAR + STEP
        self.tr_visit(Some(var));
        self.tr_visit(Some(step));
        let t = self.new_t();
        let (t1, t2) = (self.t(var), self.t(step));
        self.ic_add(ty.basic, &t, &t1, &t2);
        self.emit_let_left_part(node, Some(&t));
        self.ic_jump(loop_body);
        self.ic_label(end_loop);
    }

    /// `visit_ON_GOTO` and `visit_ON_GOSUB`.
    fn tr_visit_on_goto(&mut self, node: NodeId, routine: &str) {
        let table_label = self.tmp_label();
        self.ic_param(PTR_TYPE, &format!("#{table_label}"));
        let index = self.kid(node, 0);
        self.tr_visit(Some(index));
        let (ty, t) = (self.tyb(index), self.t(index));
        self.ic_fparam(ty, &t);
        self.runtime_call(routine, 0);
        let addresses: Vec<NodeId> = self.children(node)[1..].iter().flatten().copied().collect();
        self.tr.jump_tables.push((table_label, addresses));
    }

    fn tr_visit_if(&mut self, node: NodeId) {
        let n = self.children(node).len();
        if !(1 < n && n < 4) {
            crash(&format!("AssertionError: IF nodes: {n}"));
        }
        let condition = self.kid(node, 0);
        self.tr_visit(Some(condition));
        let if_label_else = self.tmp_label();
        let if_label_endif = self.tmp_label();
        let (ty, t) = (self.tyb(condition), self.t(condition));
        self.ic_jzero(ty, &t, if n == 3 { &if_label_else } else { &if_label_endif });
        let then = self.children(node)[1];
        self.tr_visit(then);
        if n == 3 {
            self.ic_jump(&if_label_endif);
            self.ic_label(&if_label_else);
            let otherwise = self.children(node)[2];
            self.tr_visit(otherwise);
        }
        self.ic_label(&if_label_endif);
    }

    fn tr_visit_return(&mut self, node: NodeId) {
        match self.children(node).len() {
            2 => {
                // Something to return.
                let (func, value) = (self.kid(node, 0), self.kid(node, 1));
                self.tr_visit(Some(value));
                let (ty, t) = (self.tyb(value), self.t(value));
                self.ic_ret(ty, &t, &format!("{}__leave", self.id(func).mangled));
            }
            1 => {
                let func = self.kid(node, 0);
                self.ic_return(&format!("{}__leave", self.id(func).mangled));
            }
            _ => self.ic_leave("__fastcall__"),
        }
    }

    // ------------------------------------------------------------------
    // Drawing primitives: PLOT, DRAW, DRAW3, CIRCLE
    // ------------------------------------------------------------------

    /// `visit_PLOT`, `visit_DRAW`, `visit_DRAW3` and `visit_CIRCLE`: `n`
    /// coordinates, the last of them in registers, after any attributes.
    fn tr_visit_draw(&mut self, node: NodeId, n: usize, routine: &str) {
        self.norm_attr();
        let attrs = self.check_attr(node, n);
        self.tr_visit(attrs);
        for i in 0..n {
            let arg = self.kid(node, i);
            self.tr_visit(Some(arg));
            let (ty, t) = (self.tyb(arg), self.t(arg));
            if i + 1 < n {
                self.ic_param(ty, &t);
            } else {
                self.ic_fparam(ty, &t);
            }
        }
        self.runtime_call(routine, 0);
    }

    // ------------------------------------------------------------------
    // PRINT, LOAD, SAVE and the other I/O statements
    // ------------------------------------------------------------------

    fn tr_visit_print(&mut self, node: NodeId) {
        self.norm_attr();
        let mut n = 0;
        while n < self.children(node).len() {
            let i = self.kid(node, n);
            n += 1;
            self.tr_visit(Some(i));
            // The subcommands (AT, TAB, INK...) print nothing themselves.
            let token = self.token(i);
            if matches!(token, "PRINT_TAB" | "PRINT_AT" | "PRINT_COMMA") || super::ATTR_TMP.contains(&token) {
                continue;
            }
            let (ty, t) = (self.tyb(i), self.t(i));
            self.ic_fparam(ty, &t);
            let label = match tsuffix(ty) {
                "bool" | "u8" => runtime::PRINTU8,
                "i8" => runtime::PRINTI8,
                "i16" => runtime::PRINTI16,
                "u16" => runtime::PRINTU16,
                "i32" => runtime::PRINTI32,
                "u32" => runtime::PRINTU32,
                "f16" => runtime::PRINTF16,
                "f" => runtime::PRINTF,
                _ => runtime::PRINTSTR,
            };
            self.runtime_call(label, 0);
        }
        if self.print_eol(node) {
            self.runtime_call(runtime::PRINT_EOL, 0);
        }
    }

    /// The name, address and length of LOAD, VERIFY and SAVE.
    fn tr_visit_load_args(&mut self, node: NodeId) {
        for (n, ty) in [(0, BasicType::String), (1, PTR_TYPE), (2, PTR_TYPE)] {
            let arg = self.kid(node, n);
            self.tr_visit(Some(arg));
            let t = self.t(arg);
            self.ic_param(ty, &t);
        }
    }

    /// `visit_LOAD`, which `VERIFY` shares.
    fn tr_visit_load(&mut self, node: NodeId, token: &str) {
        self.tr_visit_load_args(node);
        self.ic_param(BasicType::UByte, if token == "LOAD" { "1" } else { "0" });
        self.runtime_call(runtime::LOAD_CODE, 0);
    }

    fn tr_visit_beep(&mut self, node: NodeId) {
        let (duration, pitch) = (self.kid(node, 0), self.kid(node, 1));
        if self.token(duration) == "NUMBER" && self.token(pitch) == "NUMBER" {
            // BEEP <const>, <const>
            let (d, p) = (self.num_value(duration).as_f64(), self.num_value(pitch).as_f64());
            let (de, hl) = get_dehl(d, p);
            self.ic_param(BasicType::UInteger, &hl.to_string());
            self.ic_fparam(BasicType::UInteger, &de.to_string());
            self.runtime_call(runtime::BEEPER, 0); // a procedure call: the result is dropped
        } else {
            self.tr_visit(Some(pitch));
            let t = self.t(pitch);
            self.ic_param(BasicType::Float, &t);
            self.tr_visit(Some(duration));
            let t = self.t(duration);
            self.ic_fparam(BasicType::Float, &t);
            self.runtime_call(runtime::BEEP, 0);
        }
    }

    // ------------------------------------------------------------------
    // The attribute sentences: INK, PAPER, BRIGHT, FLASH, INVERSE, OVER,
    // ITALIC, BOLD
    // ------------------------------------------------------------------

    /// `visit_ATTR_sentence`.
    fn tr_visit_attr_sentence(&mut self, node: NodeId, routine: &str) {
        let value = self.kid(node, 0);
        self.tr_visit(Some(value));
        let t = self.t(value);
        self.ic_fparam(BasicType::UByte, &t);
        self.runtime_call(routine, 0);
    }

    // ------------------------------------------------------------------
    // Other sentences, and helpers
    // ------------------------------------------------------------------

    /// `emit_asm_node`: an inline asm block, between the `#line` markers it
    /// needs. Apart from `visit_ASM` so that FunctionTranslator can replay a
    /// block diverted into a code bank.
    pub(super) fn emit_asm_node(&mut self, node: NodeId) {
        let Kind::Asm { asm, filename, .. } = &self.node(node).kind else { crash("AttributeError: object has no attribute 'asm'") };
        let (asm, filename) = (asm.clone(), filename.clone());
        let lineno = self.lineno(node);
        self.ic_inline(&format!("#line {lineno} \"{filename}\""));
        self.ic_inline(&asm);
        self.ic_inline(&format!("#line {} \"{filename}\"", lineno as usize + asm.split('\n').count()));
    }

    /// `emit_var_assign`: stores a value in a variable. `t` is the value: a
    /// label, a constant, a temporary...
    pub(super) fn emit_var_assign(&mut self, var: NodeId, t: &str) {
        let p = if self.byref(var) { "*" } else { "" }; // the indirection prefix
        if self.o_level() > 1 && !self.accessed(var) {
            return;
        }
        let ty = self.tyb(var);
        match self.id(var).scope {
            Scope::Global => {
                let mangled = self.id(var).mangled.clone();
                self.ic_store(ty, &mangled, t);
            }
            Scope::Parameter => self.ic_pstore(ty, &format!("{p}{}", self.offset_str(var)), t),
            Scope::Local => self.ic_pstore(ty, &format!("{p}{}", -self.offset(var)), t),
        }
    }

    /// `emit_let_left_part`: the assignment of a LET or a FOR. With no `t`,
    /// the value is the expression's.
    pub(super) fn emit_let_left_part(&mut self, node: NodeId, t: Option<&str>) {
        let (var, expr) = (self.kid(node, 0), self.kid(node, 1));
        let t = match t {
            Some(t) => t.to_string(),
            None => self.t(expr),
        };
        self.emit_var_assign(var, &t);
    }

    /// `has_control_chars`: whether the node is a string that may hold
    /// attribute control characters.
    pub fn has_control_chars(&self, i: NodeId) -> bool {
        if self.ty(i).is_none_or(|t| t != BasicType::String) {
            return false;
        }
        match &self.node(i).kind {
            Kind::Id(data) if data.token() == "VAR" => true, // there is no telling what a variable holds
            Kind::Str { value } => value.chars().any(|c| 15 < c as u32 && (c as u32) < 22),
            _ => self.children(i).iter().flatten().any(|&j| self.has_control_chars(j)),
        }
    }
}
