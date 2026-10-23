//! `src/api/check.py`: the checks the grammar actions and the node
//! constructors make, which report an error when they fail.

use crate::ast::*;
use crate::compiler::*;
use crate::pyval::crash;
use crate::types::{BasicType, Ty};
use std::rc::Rc;

impl Compiler {
    /// `check_type`, with the one type every caller gives.
    pub fn check_type(&mut self, lineno: u32, expected: Ty, arg: NodeId) -> bool {
        let actual = self.ty(arg);
        if actual.is_some_and(|t| t == expected) {
            return true;
        }
        let actual = actual.map_or("None".to_string(), |t| t.to_string());
        self.error(lineno, &format!("Wrong expression type '{actual}'. Expected '{expected}'"));
        false
    }

    /// `check_is_declared_explicit`: under `--explicit`, a name must have
    /// been declared.
    pub fn check_is_declared_explicit(&mut self, lineno: u32, id: &str, classname: &str) -> bool {
        if !self.opts.bool("explicit") {
            return true;
        }
        self.check_is_declared(id, lineno, classname, None, true)
    }

    /// `check_is_callable`.
    pub fn check_is_callable(&mut self, lineno: u32, id: &str) -> bool {
        let Some(entry) = self.get_entry(id, None, None) else { return false };
        let class = self.class_(entry);
        if !matches!(class, Class::Function | Class::Sub) {
            self.syntax_error_unexpected_class(lineno, id, class, Class::Function);
            return false;
        }
        true
    }

    /// `check_type_is_explicit`.
    pub fn check_type_is_explicit(&mut self, lineno: u32, id: &str, type_: Ty) {
        if type_.implicit && self.opts.bool("strict") {
            self.syntax_error_undeclared_type(lineno, id);
        }
    }

    fn arg_name(&self, arg: NodeId) -> Option<String> {
        match &self.node(arg).kind {
            Kind::Argument { name, .. } => name.clone(),
            _ => crash("AttributeError: not an ARGUMENT"),
        }
    }

    fn set_arg_name(&mut self, arg: NodeId, value: Option<String>) {
        if let Kind::Argument { name, .. } = &mut self.node_mut(arg).kind {
            *name = value;
        }
    }

    /// `check_call_arguments`: holds the arguments of a call against the
    /// function's parameters, filling in defaults and converting types.
    /// Python returns `None` from one path and `False` from the others; both
    /// are false.
    pub fn check_call_arguments(&mut self, lineno: u32, id: &str, args: NodeId, filename: &Rc<str>) -> bool {
        if !self.check_is_declared(id, lineno, "function", None, true) {
            return false;
        }
        if !self.check_is_callable(lineno, id) {
            return false;
        }
        let entry = self.get_entry(id, None, None).unwrap();
        let params: Vec<NodeId> = self.children(self.params(entry)).iter().flatten().copied().collect();
        let param_names: Vec<String> = params.iter().map(|&p| self.id(p).name.clone()).collect();
        let arg_list = |c: &Compiler| -> Vec<NodeId> { c.children(args).iter().flatten().copied().collect() };

        for arg in arg_list(self) {
            if let Some(name) = self.arg_name(arg) {
                if !param_names.contains(&name) {
                    self.error_at(lineno, &format!("Unexpected argument '{name}'"), Some(filename));
                    return false;
                }
            }
        }

        // The arguments given, by the name of the parameter each is for.
        let mut named_args: Vec<(String, NodeId)> = Vec::new();
        let set = |named: &mut Vec<(String, NodeId)>, name: String, arg: NodeId| match named.iter_mut().find(|(n, _)| *n == name) {
            Some(entry) => entry.1 = arg,
            None => named.push((name, arg)),
        };
        let mut last_arg_name: Option<String> = None;
        for (arg, &param) in arg_list(self).into_iter().zip(&params) {
            let name = self.arg_name(arg);
            if let (Some(last), None) = (&last_arg_name, &name) {
                self.error_at(lineno, &format!("Positional argument cannot go after keyword argument '{last}'"), Some(filename));
                return false;
            }
            let name = match name {
                Some(name) => {
                    last_arg_name = Some(name.clone());
                    name
                }
                None => {
                    let name = self.id(param).name.clone();
                    self.set_arg_name(arg, Some(name.clone()));
                    name
                }
            };
            set(&mut named_args, name, arg);
        }

        if named_args.len() < params.len() {
            // Try filling in the parameters that have a default.
            for &param in &params {
                let name = self.id(param).name.clone();
                if named_args.iter().any(|(n, _)| *n == name) {
                    continue;
                }
                let Some(default_value) = self.id(param).var().and_then(|v| v.default_value) else { break };
                let arg = self.new_argument(default_value, lineno, Some(false), Some(name.clone()));
                self.arglist_make_node(Some(args), &[arg]);
                set(&mut named_args, name, arg);
            }
        }

        for arg in arg_list(self) {
            if self.arg_name(arg).is_none() {
                self.error_at(lineno, &format!("Too many arguments for Function '{id}'"), Some(filename));
                return false;
            }
        }

        if named_args.len() != params.len() {
            let c = if params.len() != 1 { "s" } else { "" };
            let msg = format!("Function '{id}' takes {} parameter{c}, not {}", params.len(), arg_list(self).len());
            self.error_at(lineno, &msg, Some(filename));
            return false;
        }

        for &param in &params {
            let name = self.id(param).name.clone();
            let arg = named_args.iter().find(|(n, _)| *n == name).unwrap().1;
            let arg_class = self.class_(arg);
            let arg_filename = match &self.node(arg).kind {
                Kind::Argument { filename, .. } => filename.clone(),
                _ => unreachable!(),
            };
            if matches!(arg_class, Class::Var | Class::Array) && self.class_(param) != arg_class {
                let value = self.child(arg, 0).map_or("None".to_string(), |v| self.str_of(v));
                self.error_at(lineno, &format!("Invalid argument '{value}'"), Some(&arg_filename));
                return false;
            }
            let param_ty = self.ty(param).unwrap_or_else(|| crash("AssertionError: isinstance(new_type, SymbolTYPE)"));
            if !self.argument_typecast(arg, param_ty) {
                return false;
            }
            if self.id(param).var().is_some_and(|v| v.byref) {
                let value = self.child(arg, 0).unwrap();
                if !(self.is_id(value) || self.token(value) == "ARRAYLOAD") {
                    self.error_at(lineno, "Expected a variable name, not an expression (parameter By Reference)", Some(&arg_filename));
                    return false;
                }
                if !matches!(self.class_(arg), Class::Var | Class::Array | Class::Unknown) {
                    self.error(lineno, "Expected a variable or array name (parameter By Reference)");
                    return false;
                }
                self.set_arg_byref(arg, true);
            }
            if let Some(value) = self.child(arg, 0) {
                self.add_required_symbol(value, param);
            }
        }

        if self.id(entry).func().is_some_and(|f| f.forwarded) {
            // Declared, never implemented.
            let msg = format!("{} '{}' declared but not implemented", self.class_(entry).name(), self.id(entry).name);
            let fname = self.id(entry).filename.clone();
            self.error_at(lineno, &msg, Some(&fname));
            return false;
        }
        true
    }

    /// `check_pending_calls`: the calls made before their function was
    /// defined. Once one fails the rest are not looked at.
    pub fn check_pending_calls(&mut self) -> bool {
        let mut result = true;
        for call in self.function_calls.clone() {
            if !result {
                break;
            }
            let entry = self.entry(call);
            let name = self.id(entry).original_name.clone();
            let args = self.child(call, 1).unwrap();
            let filename = self.node(call).call().filename.clone();
            result = self.check_call_arguments(self.lineno(call), &name, args, &filename);
        }
        result
    }

    /// `check_pending_labels`: every identifier still of no class must be a
    /// label by now. The walk is over the whole tree, each node once.
    pub fn check_pending_labels(&mut self, ast: NodeId) -> bool {
        let mut result = true;
        let mut visited = vec![false; self.nodes.len()];
        let mut pending = vec![Some(ast)];
        while let Some(node) = pending.pop() {
            let Some(node) = node else { continue };
            if visited[node.0 as usize] {
                continue;
            }
            visited[node.0 as usize] = true;
            pending.extend(self.children(node).iter().copied());
            if !matches!(self.token(node), "ID" | "LABEL") {
                continue;
            }
            let name = self.id(node).name.clone();
            let tmp = self.get_entry(&name, None, None);
            if tmp.is_none_or(|t| self.class_(t) == Class::Unknown) {
                let lineno = self.lineno(node);
                self.error(lineno, &format!("Undeclared identifier \"{name}\""));
            } else if tmp.is_some_and(|t| self.class_(t) != Class::Label) {
                crash("AssertionError: tmp.class_ == CLASS.label");
            }
            result = result && tmp.is_some();
        }
        result
    }

    /// `check_and_make_label`, for a label that is a name.
    pub fn check_and_make_label(&mut self, lbl: &str, lineno: u32) -> Option<NodeId> {
        self.access_label(lbl, lineno, None)
    }

    /// `check_and_make_label`, for a line number as the lexer gives it.
    pub fn check_and_make_label_number(&mut self, lbl: f64, lineno: u32) -> Option<NodeId> {
        if lbl.fract() != 0.0 || !lbl.is_finite() {
            self.error(lineno, "Line numbers must be integers.");
            return None;
        }
        self.access_label(&format!("{}", lbl as i128), lineno, None)
    }

    /// `is_null`: nothing, a NOP, or a block of nothing but those.
    pub fn is_null(&self, symbols: &[Option<NodeId>]) -> bool {
        symbols.iter().flatten().all(|&sym| match self.node(sym).kind {
            Kind::Nop => true,
            Kind::Block => self.is_null(self.children(sym)),
            _ => false,
        })
    }

    /// `is_SYMBOL`.
    pub fn is_symbol(&self, token: &str, symbols: &[NodeId]) -> bool {
        symbols.iter().all(|&sym| self.token(sym) == token)
    }

    pub fn is_label(&self, p: &[NodeId]) -> bool {
        self.is_symbol("LABEL", p)
    }

    /// `is_string`: a STRING, or a CONST of string type.
    pub fn is_string(&self, p: &[NodeId]) -> bool {
        p.iter().all(|&x| self.token(x) == "STRING" || self.token(x) == "CONST" && self.ty(x).is_some_and(|t| t == BasicType::String))
    }

    /// `is_const`: a constant of the program, as in `CONST a = 5`.
    pub fn is_const(&self, p: &[NodeId]) -> bool {
        self.is_symbol("CONST", p)
    }

    /// `is_CONST`: a constant expression. Not to be confused with the above.
    pub fn is_constexpr(&self, p: &[NodeId]) -> bool {
        self.is_symbol("CONSTEXPR", p)
    }

    fn is_static_unary(&self, x: NodeId) -> bool {
        let Kind::Unary { operator } = &self.node(x).kind else { return false };
        if operator != "ADDRESS" {
            return false;
        }
        let Some(operand) = self.child(x, 0) else { return false };
        match self.token(operand) {
            "LABEL" => true,
            "ID" | "VAR" => self.id(operand).scope == Scope::Global,
            "ARRAYACCESS" => {
                // Python asks only whether the subscripts are constant, so
                // `@local(1) + 1` was folded as a constant and then ended in
                // "Initializer expression is not constant".
                if !self.sess.oracle_diagnostics && self.scope(operand) != Scope::Global {
                    return false;
                }
                let args = self.child(operand, 1).unwrap();
                let values: Vec<NodeId> = self.children(args).iter().flatten().filter_map(|&arg| self.child(arg, 0)).collect();
                self.is_static(&values)
            }
            _ => false,
        }
    }

    /// `is_static`: a value known at compile time that does not change.
    pub fn is_static(&self, p: &[NodeId]) -> bool {
        p.iter().all(|&x| self.is_constexpr(&[x]) || self.is_number(&[x]) || self.is_const(&[x]) || self.is_static_unary(x))
    }

    /// `is_number`: a NUMBER, or a CONST of numeric type.
    pub fn is_number(&self, p: &[NodeId]) -> bool {
        p.iter().all(|&i| matches!(self.token(i), "NUMBER" | "CONST") && self.ty(i).is_some_and(|t| t.basic.is_numeric()))
    }

    pub fn is_static_str(&self, p: &[NodeId]) -> bool {
        self.is_symbol("STRING", p)
    }

    pub fn is_var(&self, p: &[NodeId]) -> bool {
        self.is_symbol("VAR", p)
    }

    pub fn is_unsigned(&self, p: &[NodeId]) -> bool {
        p.iter().all(|&i| self.ty(i).is_some_and(|t| t.basic.is_unsigned()))
    }

    pub fn is_signed(&self, p: &[NodeId]) -> bool {
        p.iter().all(|&i| self.ty(i).is_some_and(|t| t.basic.is_signed()))
    }

    pub fn is_numeric(&self, p: &[NodeId]) -> bool {
        p.iter().all(|&i| self.ty(i).is_some_and(|t| t.basic.is_numeric()))
    }

    pub fn is_type(&self, type_: BasicType, p: &[NodeId]) -> bool {
        p.iter().all(|&i| self.ty(i).is_some_and(|t| t == type_))
    }

    /// `is_dynamic`. As written it asks each argument for `is_basic`, which
    /// no node has, and the exception that follows makes the answer false; but
    /// only an argument of global scope is asked. So it comes down to: every
    /// argument has a scope, and none is global.
    pub fn is_dynamic(&self, p: &[NodeId]) -> bool {
        for &i in p {
            let scope = match &self.node(i).kind {
                Kind::Id(data) => data.scope,
                Kind::ArrayAccess(_) | Kind::ArrayLoad(_) => self.scope(i),
                _ => return false,
            };
            if scope == Scope::Global {
                return false;
            }
        }
        true
    }

    pub fn is_callable(&self, p: &[NodeId]) -> bool {
        self.is_symbol("FUNCTION", p)
    }

    /// `is_block_accessed`: whether a block holds a label that is used.
    pub fn is_block_accessed(&self, block: NodeId) -> bool {
        if self.is_label(&[block]) && self.accessed(block) {
            return true;
        }
        self.children(block).iter().any(|&child| {
            let child = child.unwrap_or_else(|| crash("AttributeError: 'NoneType' object has no attribute 'token'"));
            !self.is_callable(&[child]) && self.is_block_accessed(child)
        })
    }

    /// `is_temporary_value`: whether a node's value is a temporary copy in
    /// the heap rather than a variable.
    pub fn is_temporary_value(&mut self, node: NodeId) -> bool {
        if matches!(self.token(node), "STRING" | "VAR") {
            return false;
        }
        let t = self.t(node);
        !matches!(t.chars().next(), Some('_' | '#'))
    }

    /// `common_type` of two nodes.
    pub fn common_type(&self, a: NodeId, b: NodeId) -> Option<Ty> {
        let (a, b) = (self.ty(a)?, self.ty(b)?);
        if a == b {
            return Some(a);
        }
        Some(Ty::new(crate::types::common_type(a.basic, b.basic)))
    }

    /// `is_ender`: a statement after which the next one is not reached.
    pub fn is_ender(&self, node: NodeId) -> bool {
        matches!(self.token(node), "END" | "ERROR" | "CONTINUE_DO" | "CONTINUE_FOR" | "CONTINUE_WHILE" | "EXIT_DO" | "EXIT_FOR" | "EXIT_WHILE" | "GOTO" | "RETURN" | "STOP")
    }

    /// `check_class`: the node is of the class given, or of none yet.
    pub fn check_class(&mut self, node: NodeId, class: Class, lineno: u32) -> bool {
        let node_class = self.class_(node);
        if node_class == Class::Unknown || node_class == class {
            return true;
        }
        let name = self.name(node).to_string();
        self.syntax_error_unexpected_class(lineno, &name, node_class, class);
        false
    }
}
