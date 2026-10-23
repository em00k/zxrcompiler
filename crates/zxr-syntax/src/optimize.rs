//! The passes over the tree: `src/api/optimize.py`.
//!
//! The Python visitors are generators run by a loop with an explicit stack
//! (`NodeVisitor.visit`): a method yields a child wrapped in `ToVisit` to
//! have it visited and receives what the visit made of it; it yields anything
//! else to return it. What a method "returns" is the value of the last thing
//! it yielded. Here the same is plain recursion: `visit_node` is the visit of
//! a `ToVisit`, and each `visit_*` method returns its result.
//!
//! A visit can give back another node than the one visited, or none, and the
//! caller writes that into the child slot. Three of the four passes visit a
//! node only once (`UniqueVisitor`); a second visit gives the node back as
//! it is.

use crate::ast::*;
use crate::compiler::*;
use crate::pyval::{self, crash};
use crate::types::{BasicType, Num, Ty};
use std::collections::HashSet;
use std::rc::Rc;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pass {
    Unreachable,
    FunctionGraph,
    Optimizer,
    Variable,
}

struct Visitor {
    pass: Pass,
    /// `UniqueVisitor.visited`. Only asked whether a node is in it.
    visited: HashSet<NodeId>,
    /// `VariableVisitor._visited`.
    var_visited: HashSet<NodeId>,
    /// `VariableVisitor._original_variable` and `_parent_variable`.
    original_variable: Option<NodeId>,
    parent_variable: Option<NodeId>,
}

impl Visitor {
    fn new(pass: Pass) -> Visitor {
        Visitor { pass, visited: HashSet::new(), var_visited: HashSet::new(), original_variable: None, parent_variable: None }
    }
}

/// The walk of `NodeVisitor.filter_inorder`, which the Python code consumes
/// lazily: each node is handed over before the next one is looked for.
struct FilterWalk {
    visited: HashSet<Option<NodeId>>,
    stack: Vec<Option<NodeId>>,
}

type NodeTest = fn(&Compiler, NodeId) -> bool;

fn is_call(c: &Compiler, x: NodeId) -> bool {
    matches!(c.token(x), "CALL" | "FUNCCALL")
}

fn is_funccall_or_builtin(c: &Compiler, x: NodeId) -> bool {
    matches!(c.token(x), "FUNCCALL" | "BUILTIN")
}

fn is_funccall(c: &Compiler, x: NodeId) -> bool {
    c.token(x) == "FUNCCALL"
}

fn any_node(_: &Compiler, _: NodeId) -> bool {
    true
}

fn not_function(c: &Compiler, x: NodeId) -> bool {
    c.token(x) != "FUNCTION"
}

impl Compiler {
    /// `UnreachableCodeVisitor().visit(ast)`.
    pub fn unreachable_code_visitor(&mut self, ast: NodeId) {
        let mut v = Visitor::new(Pass::Unreachable);
        self.visit_node(&mut v, Some(ast));
    }

    /// `FunctionGraphVisitor().visit(ast)`.
    pub fn function_graph_visitor(&mut self, ast: NodeId) {
        let mut v = Visitor::new(Pass::FunctionGraph);
        self.visit_node(&mut v, Some(ast));
    }

    /// `OptimizerVisitor().visit(ast)`.
    pub fn optimizer_visitor(&mut self, ast: NodeId) {
        let mut v = Visitor::new(Pass::Optimizer);
        self.visit(&mut v, Some(ast));
    }

    /// `VariableVisitor().visit(data_ast)`.
    pub fn variable_visitor(&mut self, data_ast: NodeId) {
        let mut v = Visitor::new(Pass::Variable);
        self.visit_node(&mut v, Some(data_ast));
    }

    // ------------------------------------------------------------------
    // GenericVisitor, UniqueVisitor
    // ------------------------------------------------------------------

    /// `GenericVisitor.NOP`.
    fn nop(&mut self) -> NodeId {
        if let Some(nop) = self.visitor_nop {
            return nop;
        }
        let nop = self.new_nop();
        self.visitor_nop = Some(nop);
        nop
    }

    /// `visitor.visit(node)`. The optimizer does nothing below `-O1`.
    fn visit(&mut self, v: &mut Visitor, node: Option<NodeId>) -> Option<NodeId> {
        if v.pass == Pass::Optimizer && self.o_level() < 1 {
            return node;
        }
        self.visit_node(v, node)
    }

    /// `_visit(ToVisit(node))`: the method for the node's token, or the
    /// generic visit.
    fn visit_node(&mut self, v: &mut Visitor, node: Option<NodeId>) -> Option<NodeId> {
        let node = node?;
        if v.pass != Pass::Variable && !v.visited.insert(node) {
            return Some(node);
        }
        enum M {
            Generic,
            Function,
            Block,
            GraphCall,
            GraphFuncDecl,
            Goto,
            Binary,
            Builtin,
            ConstExpr,
            Call,
            FuncDecl,
            Let,
            LetArray,
            LetSubstr,
            Return,
            Unary,
            If,
            While,
            For,
            VarDecl,
        }
        let method = match (v.pass, self.token(node)) {
            (Pass::Unreachable, "FUNCTION") => M::Function,
            (Pass::Unreachable, "BLOCK") => M::Block,
            (Pass::FunctionGraph, "FUNCCALL" | "CALL") => M::GraphCall,
            (Pass::FunctionGraph, "FUNCDECL") => M::GraphFuncDecl,
            (Pass::FunctionGraph, "GOTO" | "GOSUB") => M::Goto,
            (Pass::Optimizer, "BINARY") => M::Binary,
            (Pass::Optimizer, "BUILTIN") => M::Builtin,
            (Pass::Optimizer, "CONSTEXPR") => M::ConstExpr,
            (Pass::Optimizer, "FUNCCALL" | "CALL") => M::Call,
            (Pass::Optimizer, "FUNCDECL") => M::FuncDecl,
            (Pass::Optimizer, "LET") => M::Let,
            (Pass::Optimizer, "LETARRAY") => M::LetArray,
            (Pass::Optimizer, "LETSUBSTR") => M::LetSubstr,
            (Pass::Optimizer, "RETURN") => M::Return,
            (Pass::Optimizer, "UNARY") => M::Unary,
            (Pass::Optimizer, "IF") => M::If,
            (Pass::Optimizer, "WHILE") => M::While,
            (Pass::Optimizer, "FOR") => M::For,
            (Pass::Variable, "VARDECL") => M::VarDecl,
            _ => M::Generic,
        };
        match method {
            M::Generic => self.generic_visit(v, node),
            M::Function => self.unreachable_visit_function(v, node),
            M::Block => self.unreachable_visit_block(v, node),
            M::GraphCall => {
                self.set_children_as_accessed(v, node);
                Some(node)
            }
            M::GraphFuncDecl => self.graph_visit_funcdecl(v, node),
            M::Goto => self.graph_visit_goto(node),
            M::Binary => self.visit_binary(v, node),
            M::Builtin => self.visit_builtin(v, node),
            M::ConstExpr => self.visit_constexpr(node),
            M::Call => self.visit_call(v, node),
            M::FuncDecl => self.visit_funcdecl(v, node),
            M::Let => self.visit_let(v, node),
            M::LetArray => self.visit_letarray(v, node),
            M::LetSubstr => self.visit_letsubstr(v, node),
            M::Return => self.visit_return(v, node),
            M::Unary => self.visit_unary(v, node),
            M::If => self.visit_if(v, node),
            M::While => self.visit_while(v, node),
            M::For => self.visit_for(v, node),
            M::VarDecl => self.visit_vardecl(v, node),
        }
    }

    /// `generic_visit` of each visitor: every child is replaced by what its
    /// visit gives.
    fn generic_visit(&mut self, v: &mut Visitor, node: NodeId) -> Option<NodeId> {
        if v.pass == Pass::Variable {
            // A node met a second time yields nothing, and the result of a
            // generator that yields nothing is None.
            if !v.var_visited.insert(node) {
                return None;
            }
            for i in 0..self.children(node).len() {
                let child = self.children(node)[i];
                let result = self.visit_node(v, child);
                self.set_child(node, i, result);
            }
            return Some(node);
        }
        // The list is walked as it stands at each step.
        let mut i = 0;
        while i < self.children(node).len() {
            let child = self.children(node)[i];
            let result = self.visit_node(v, child);
            if i >= self.children(node).len() {
                crash("IndexError: list assignment index out of range");
            }
            self.set_child(node, i, result);
            i += 1;
        }
        Some(node)
    }

    /// `NodeVisitor.filter_inorder(node, filter_func, child_selector)`, one
    /// step: the next node under `node` that passes `filter`, visited. Only
    /// the children of nodes that pass `selector` are looked at.
    fn filter_next(&mut self, v: &mut Visitor, walk: &mut FilterWalk, filter: NodeTest, selector: NodeTest) -> Option<Option<NodeId>> {
        while let Some(node) = walk.stack.pop() {
            if !walk.visited.insert(node) {
                continue;
            }
            let node = node.unwrap_or_else(|| crash("AttributeError: 'NoneType' object has no attribute 'token'"));
            let found = if filter(self, node) { Some(self.visit(v, Some(node))) } else { None };
            if selector(self, node) {
                walk.stack.extend(self.children(node).iter().rev().copied());
            }
            if found.is_some() {
                return found;
            }
        }
        None
    }

    fn filter_walk(node: Option<NodeId>) -> FilterWalk {
        FilterWalk { visited: HashSet::new(), stack: vec![node] }
    }

    /// `node.filename`, of the nodes that have one.
    fn node_filename(&self, node: NodeId) -> Rc<str> {
        match &self.node(node).kind {
            Kind::Id(data) => data.filename.clone(),
            Kind::Sentence { filename, .. } | Kind::Asm { filename, .. } | Kind::Argument { filename, .. } => filename.clone(),
            Kind::Call(c) | Kind::FuncCall(c) | Kind::ArrayAccess(c) | Kind::ArrayLoad(c) => c.filename.clone(),
            _ => crash(&format!("AttributeError: '{}' object has no attribute 'filename'", self.token(node))),
        }
    }

    /// `node.get_parent(symbols.FUNCDECL)`: the nearest FUNCDECL going up
    /// through the parents. If the parents go round in a circle, the node
    /// where that was noticed.
    fn get_parent_funcdecl(&self, node: NodeId) -> Option<NodeId> {
        let mut visited = HashSet::new();
        let mut parent = self.node(node).parent;
        while let Some(p) = parent {
            if !visited.insert(p) {
                break;
            }
            if matches!(self.node(p).kind, Kind::FuncDecl) {
                return Some(p);
            }
            parent = self.node(p).parent;
        }
        parent
    }

    // ------------------------------------------------------------------
    // UnreachableCodeVisitor
    // ------------------------------------------------------------------

    fn unreachable_visit_function(&mut self, v: &mut Visitor, node: NodeId) -> Option<NodeId> {
        let body = self.body(node);
        let last = self.children(body).last().copied();
        let ends_well = |c: &Compiler| match last {
            None => false,
            Some(last) => {
                let last = last.unwrap_or_else(|| crash("AttributeError: 'NoneType' object has no attribute 'token'"));
                matches!(c.token(last), "CHKBREAK" | "LABEL" | "RETURN")
            }
        };
        if self.class_(node) == Class::Function && self.token(body) == "BLOCK" && !ends_well(self) {
            // A function must always return a value. For a string one a
            // sentinel is added, which is removed if a RETURN is found.
            let lineno = match last {
                None => self.lineno(node),
                Some(last) => self.lineno(last.unwrap()),
            };
            let (name, filename) = (self.id(node).name.clone(), self.id(node).filename.clone());
            self.warning_function_should_return_a_value(lineno, &name, Some(&filename));
            let is_string = self.ty(node).is_some_and(|t| t == BasicType::String);
            if is_string && self.id(node).func().is_some_and(|f| f.convention != Convention::Fastcall) {
                let asm = self.new_node(Kind::Asm { asm: "\nld hl, 0\n".to_string(), filename, is_sentinel: true, bank: 0 }, lineno, &[]);
                self.block_append(body, &[Some(asm)]);
            }
        }
        self.generic_visit(v, node)
    }

    fn block_child(&self, block: NodeId, i: usize) -> NodeId {
        self.children(block)[i].unwrap_or_else(|| crash("AttributeError: 'NoneType' object has no attribute 'token'"))
    }

    fn unreachable_visit_block(&mut self, v: &mut Visitor, node: NodeId) -> Option<NodeId> {
        if !matches!(self.node(node).kind, Kind::Block) {
            crash("TypeError: object of type 'SymbolSENTENCE' has no len()");
        }
        // Remove the CHKBREAK after a label.
        let mut i = 0;
        while i + 1 < self.children(node).len() {
            let child = self.block_child(node, i);
            if matches!(self.token(child), "LABEL" | "RETURN") && self.token(self.block_child(node, i + 1)) == "CHKBREAK" {
                self.pop_child(node, i + 1);
                continue;
            }
            i += 1;
        }

        let mut warning_emitted = false;
        let mut i = 0;
        while i < self.children(node).len() {
            let sentence = self.block_child(node, i);
            if self.is_ender(sentence) {
                let mut j = i + 1;
                while j < self.children(node).len() {
                    let next = self.block_child(node, j);
                    if self.is_label(&[next]) {
                        break;
                    }
                    if self.token(next) == "FUNCDECL" {
                        j += 1;
                        continue;
                    }
                    if self.token(next) == "END" {
                        // Sentinel instructions can be freely removed.
                        let Kind::Sentence { is_sentinel, .. } = self.node(next).kind else { crash("AttributeError: object has no attribute 'is_sentinel'") };
                        if is_sentinel {
                            self.pop_child(node, j);
                            continue;
                        }
                    }
                    if self.token(next) == "ASM" {
                        break; // the user's asm is always left there
                    }
                    if !warning_emitted && self.o_level() > 0 {
                        warning_emitted = true;
                        let (lineno, filename) = (self.lineno(next), self.node_filename(next));
                        self.warning_unreachable_code(lineno, Some(&filename));
                        if self.o_level() < 2 {
                            break;
                        }
                    }
                    self.pop_child(node, j);
                }
            }
            i += 1;
        }

        if self.o_level() >= 1 && self.is_null(&[Some(node)]) {
            return Some(self.nop());
        }
        self.generic_visit(v, node)
    }

    // ------------------------------------------------------------------
    // FunctionGraphVisitor
    // ------------------------------------------------------------------

    /// `_get_calls_from_children`, with what the callers do to each call.
    fn mark_calls_from_children(&mut self, v: &mut Visitor, node: NodeId) {
        let mut walk = Self::filter_walk(Some(node));
        let mut calls = Vec::new();
        while let Some(symbol) = self.filter_next(v, &mut walk, is_call, any_node) {
            calls.push(symbol);
        }
        for symbol in calls {
            let symbol = symbol.unwrap_or_else(|| crash("AttributeError: 'NoneType' object has no attribute 'entry'"));
            let entry = self.entry(symbol);
            self.set_accessed(entry, true);
        }
    }

    /// The routines a routine calls are in use, as the function graph
    /// marks them for one that is.
    pub(crate) fn mark_routine_calls(&mut self, decl: NodeId) {
        let mut v = Visitor::new(Pass::FunctionGraph);
        self.mark_calls_from_children(&mut v, decl);
    }

    fn set_children_as_accessed(&mut self, v: &mut Visitor, node: NodeId) {
        if self.get_parent_funcdecl(node).is_none() {
            // Global scope.
            self.mark_calls_from_children(v, node);
        }
    }

    fn graph_visit_funcdecl(&mut self, v: &mut Visitor, node: NodeId) -> Option<NodeId> {
        if self.accessed(self.entry(node)) {
            self.mark_calls_from_children(v, node);
        }
        Some(node)
    }

    fn graph_visit_goto(&mut self, node: NodeId) -> Option<NodeId> {
        if self.get_parent_funcdecl(node).is_none() {
            let label = self.children(node).first().copied().unwrap_or_else(|| crash("IndexError: list index out of range"));
            let label = label.unwrap_or_else(|| crash("AttributeError: 'NoneType' object has no attribute 'accessed'"));
            // On anything but an identifier the assignment just sets an
            // attribute nobody reads.
            if self.is_id(label) {
                self.set_accessed(label, true);
            }
        }
        Some(node)
    }

    // ------------------------------------------------------------------
    // OptimizerVisitor
    // ------------------------------------------------------------------

    fn some(node: Option<NodeId>, attr: &str) -> NodeId {
        node.unwrap_or_else(|| crash(&format!("AttributeError: 'NoneType' object has no attribute '{attr}'")))
    }

    fn operator(&self, node: NodeId) -> String {
        match &self.node(node).kind {
            Kind::Binary { operator, .. } | Kind::Unary { operator } => operator.clone(),
            _ => crash(&format!("AttributeError: '{}' object has no attribute 'operator'", self.token(node))),
        }
    }

    fn binary_func(&self, node: NodeId) -> Option<Fold2> {
        match &self.node(node).kind {
            Kind::Binary { func, .. } => *func,
            _ => crash(&format!("AttributeError: '{}' object has no attribute 'func'", self.token(node))),
        }
    }

    /// `chk.is_number(node)` of a child that may be missing.
    fn opt_is_number(&self, node: Option<NodeId>) -> bool {
        node.is_some_and(|n| self.is_number(&[n]))
    }

    fn visit_address(&mut self, node: NodeId) -> Option<NodeId> {
        let operand = Self::some(self.child(node, 0), "token");
        let lineno = self.lineno(node);
        if self.token(operand) != "ARRAYACCESS" {
            if !self.is_dynamic(&[operand]) {
                return Some(self.new_constexpr(node, lineno));
            }
        } else if let Some(offset) = self.array_offset(operand) {
            // A constant access: for a global the offset is added now.
            if self.scope(operand) == Scope::Global {
                let entry = self.entry(operand);
                let ptr = Ty::new(PTR_TYPE);
                let address = self.new_unary("ADDRESS", entry, lineno, Some(ptr));
                let number = self.new_number(offset, self.lineno(operand), Some(ptr));
                return self.make_binary_node("PLUS", Some(address), Some(number), lineno, Some(pyval::fold::add), None);
            }
        }
        Some(node)
    }

    fn visit_binary(&mut self, v: &mut Visitor, node: NodeId) -> Option<NodeId> {
        // This might turn constants into numbers.
        let mut node = Self::some(self.generic_visit(v, node), "operator");
        let operator = self.operator(node);
        let left = |c: &Compiler, n: NodeId| Self::some(c.child(n, 0), "token");
        let right = |c: &Compiler, n: NodeId| Self::some(c.child(n, 1), "token");
        if self.o_level() > 1 && matches!(operator.as_str(), "PLUS" | "MUL") {
            if self.opt_is_number(self.child(node, 0)) && !self.opt_is_number(self.child(node, 1)) {
                let (l, r) = (self.child(node, 0), self.child(node, 1));
                self.set_child(node, 0, r);
                self.set_child(node, 1, l);
                node = Self::some(self.generic_visit(v, node), "left");
            }

            let l = left(self, node);
            if self.token(l) == "BINARY" && self.operator(l) == operator && self.is_number(&[right(self, node)]) {
                let (ll, lr) = (left(self, l), right(self, l));
                let picked = if self.is_number(&[lr]) {
                    Some((ll, lr))
                } else if self.is_number(&[ll]) {
                    Some((lr, ll))
                } else {
                    None
                };
                if let Some((new_left, number)) = picked {
                    let (lineno, func) = (self.lineno(node), self.binary_func(node));
                    let new_right = self.make_binary_node(&operator, Some(number), self.child(node, 1), lineno, func, None);
                    self.set_child(node, 0, Some(new_left));
                    let new_right = new_right.unwrap_or_else(|| crash("AssertionError: isinstance(value, Symbol)"));
                    self.set_child(node, 1, Some(new_right));
                }
            }

            let (l, r) = (left(self, node), right(self, node));
            if self.token(l) == "BINARY"
                && self.token(r) == "BINARY"
                && operator == self.operator(l)
                && operator == self.operator(r)
                && self.opt_is_number(self.child(l, 1))
                && self.opt_is_number(self.child(r, 1))
            {
                let new_left = self.make_binary_node(&operator, self.child(l, 0), self.child(r, 0), self.lineno(l), self.binary_func(l), None);
                let new_right = self.make_binary_node(&operator, self.child(l, 1), self.child(r, 1), self.lineno(r), self.binary_func(r), None);
                let (lineno, func) = (self.lineno(node), self.binary_func(node));
                node = Self::some(self.make_binary_node(&operator, new_left, new_right, lineno, func, None), "operator");
            }
        }
        // Retry folding.
        let (operator, lineno, func, ty) = (self.operator(node), self.lineno(node), self.binary_func(node), self.ty(node));
        let (l, r) = (self.child(node, 0), self.child(node, 1));
        self.make_binary_node(&operator, l, r, lineno, func, ty)
    }

    fn visit_builtin(&mut self, v: &mut Visitor, node: NodeId) -> Option<NodeId> {
        // `hasattr(self, "visit_" + node.fname)`: of the names a builtin has,
        // only CHR is a method.
        let is_chr = matches!(&self.node(node).kind, Kind::Builtin { fname, .. } if fname == "CHR");
        if is_chr { self.visit_chr(v, node) } else { self.generic_visit(v, node) }
    }

    fn visit_chr(&mut self, v: &mut Visitor, node: NodeId) -> Option<NodeId> {
        let node = Self::some(self.generic_visit(v, node), "operand");
        let operand = self.child(node, 0).unwrap_or_else(|| crash("TypeError: 'NoneType' object is not iterable"));
        if !matches!(self.node(operand).kind, Kind::ArgList | Kind::Block | Kind::BoundList | Kind::ParamList { .. }) {
            crash(&format!("TypeError: '{}' object is not iterable", self.token(operand)));
        }
        let args: Vec<Option<NodeId>> = self.children(operand).to_vec();
        let mut values = Vec::new();
        for arg in &args {
            let arg = Self::some(*arg, "value");
            let value = self.child(arg, 0).unwrap_or_else(|| crash("AssertionError"));
            values.push(value);
        }
        // all() stops at the first that is not static.
        if values.iter().all(|&value| self.is_static(&[value])) {
            let mut text = String::new();
            for value in values {
                let code = match self.value(value).num() {
                    Num::Int(i) => i & 0xFF,
                    Num::Float(_) => crash("TypeError: unsupported operand type(s) for &: 'float' and 'int'"),
                };
                text.push(char::from_u32(code as u32).unwrap());
            }
            let lineno = self.lineno(node);
            return Some(self.new_string(&text, lineno));
        }
        Some(node)
    }

    fn visit_constexpr(&mut self, node: NodeId) -> Option<NodeId> {
        let expr = Self::some(self.child(node, 0), "token");
        if self.is_number(&[expr]) || self.is_const(&[expr]) { Some(expr) } else { Some(node) }
    }

    /// `visit_FUNCCALL` and `visit_CALL`. The entry is not visited, which
    /// would never end for a recursive function.
    fn visit_call(&mut self, v: &mut Visitor, node: NodeId) -> Option<NodeId> {
        let args = Self::some(self.child(node, 1), "children");
        let args = self.generic_visit(v, args);
        let args = args.unwrap_or_else(|| crash("AssertionError: isinstance(value, SymbolARGLIST)"));
        self.set_child(node, 1, Some(args));
        let entry = self.entry(node);
        if self.id(entry).func().is_none() {
            crash("AttributeError: 'SymbolRef' object has no attribute 'params'");
        }
        let params = self.params(entry);
        self.check_if_any_arg_is_an_array_and_needs_lbound_or_ubound(params, args);
        Some(node)
    }

    fn visit_funcdecl(&mut self, v: &mut Visitor, node: NodeId) -> Option<NodeId> {
        let entry = self.entry(node);
        if self.o_level() > 1 && !self.accessed(entry) {
            let (lineno, name, filename) = (self.lineno(entry), self.id(entry).name.clone(), self.id(entry).filename.clone());
            self.warning_func_is_never_called(lineno, &name, Some(&filename));
            return Some(self.nop());
        }
        if self.o_level() > 1 {
            let locals_size = self.id(entry).func().map_or_else(|| crash("AttributeError: 'SymbolRef' object has no attribute 'locals_size'"), |f| f.locals_size);
            if self.get_funcdecl_params_size(node) == 0 && locals_size == 0 {
                self.id_mut(entry).func_mut().unwrap().convention = Convention::Fastcall;
            }
        }
        // The second child, the parameter list, is replaced by what the
        // visit of the entry gives: the entry.
        let result = self.visit_node(v, Some(entry));
        if self.children(node).len() < 2 {
            crash("IndexError: list assignment index out of range");
        }
        self.set_child(node, 1, result);
        Some(node)
    }

    /// The statements kept of an expression whose value is dropped: its
    /// function calls, as CALLs, and the builtins that have an effect.
    fn calls_to_keep(&mut self, v: &mut Visitor, expr: Option<NodeId>, filename_of_entry: bool, filename: Option<Rc<str>>) -> Vec<NodeId> {
        let mut walk = Self::filter_walk(expr);
        let mut nodes = Vec::new();
        while let Some(x) = self.filter_next(v, &mut walk, is_funccall_or_builtin, not_function) {
            let x = Self::some(x, "token");
            if self.token(x) == "FUNCCALL" {
                let (entry, args, lineno) = (self.entry(x), Self::some(self.child(x, 1), "args"), self.lineno(x));
                let filename = if filename_of_entry { self.node_filename(entry) } else { filename.clone().unwrap() };
                nodes.push(self.new_call_statement(entry, args, lineno, filename));
                continue;
            }
            // `getattr(x, "fname")`: the visit may have given something that
            // is not a builtin any more (CHR of constants is a STRING).
            let Kind::Builtin { fname, .. } = &self.node(x).kind else {
                let class = match self.node(x).kind {
                    Kind::Str { .. } => "SymbolSTRING".to_string(),
                    _ => format!("Symbol{}", self.token(x)),
                };
                crash(&format!("AttributeError: '{class}' object has no attribute 'fname'"))
            };
            if matches!(fname.as_str(), "IN" | "RND" | "USR") {
                nodes.push(x);
            }
        }
        for &node in &nodes {
            if let Kind::Builtin { discard_result, .. } = &mut self.node_mut(node).kind {
                *discard_result = true;
            }
        }
        nodes
    }

    /// `symbols.BLOCK(*nodes)`.
    fn new_block_of(&mut self, nodes: &[NodeId]) -> NodeId {
        let children: Vec<Option<NodeId>> = nodes.iter().map(|&n| Some(n)).collect();
        self.new_node(Kind::Block, 0, &children)
    }

    fn lvalue_id(&self, node: Option<NodeId>) -> NodeId {
        let lvalue = Self::some(node, "accessed");
        if !self.is_id(lvalue) {
            crash(&format!("AttributeError: '{}' object has no attribute 'accessed'", self.token(lvalue)));
        }
        lvalue
    }

    fn visit_let(&mut self, v: &mut Visitor, node: NodeId) -> Option<NodeId> {
        let lvalue = self.lvalue_id(self.children(node).first().copied().flatten());
        if self.o_level() > 1 && !self.accessed(lvalue) {
            let (lineno, name, filename) = (self.lineno(lvalue), self.id(lvalue).name.clone(), self.id(lvalue).filename.clone());
            self.warning_not_used(lineno, &name, "Variable", Some(&filename));
            let expr = *self.children(node).get(1).unwrap_or_else(|| crash("IndexError: list index out of range"));
            let nodes = self.calls_to_keep(v, expr, false, Some(filename));
            return Some(self.new_block_of(&nodes));
        }
        self.generic_visit(v, node)
    }

    fn visit_letarray(&mut self, v: &mut Visitor, node: NodeId) -> Option<NodeId> {
        let access = Self::some(self.children(node).first().copied().flatten(), "entry");
        let lvalue = self.entry(access);
        if self.o_level() > 1 && !self.accessed(lvalue) {
            let (lineno, name, filename) = (self.lineno(lvalue), self.id(lvalue).name.clone(), self.id(lvalue).filename.clone());
            self.warning_not_used(lineno, &name, "Variable", Some(&filename));
            let expr = *self.children(node).get(1).unwrap_or_else(|| crash("IndexError: list index out of range"));
            let mut walk = Self::filter_walk(expr);
            let mut nodes = Vec::new();
            while let Some(x) = self.filter_next(v, &mut walk, is_funccall, not_function) {
                let x = Self::some(x, "entry");
                let (entry, args, line) = (self.entry(x), Self::some(self.child(x, 1), "args"), self.lineno(x));
                nodes.push(self.new_call_statement(entry, args, line, filename.clone()));
            }
            return Some(self.new_block_of(&nodes));
        }
        self.generic_visit(v, node)
    }

    fn visit_letsubstr(&mut self, v: &mut Visitor, node: NodeId) -> Option<NodeId> {
        let lvalue = self.lvalue_id(self.children(node).first().copied().flatten());
        if self.o_level() > 1 && !self.accessed(lvalue) {
            let (lineno, name) = (self.lineno(lvalue), self.id(lvalue).name.clone());
            self.warning_not_used(lineno, &name, "Variable", None);
            return Some(self.nop());
        }
        self.generic_visit(v, node)
    }

    /// Only the second child is visited: the first is the function being
    /// returned from.
    fn visit_return(&mut self, v: &mut Visitor, node: NodeId) -> Option<NodeId> {
        if self.children(node).len() == 2 {
            let result = self.visit_node(v, self.children(node)[1]);
            self.set_child(node, 1, result);
        }
        Some(node)
    }

    fn visit_unary(&mut self, v: &mut Visitor, node: NodeId) -> Option<NodeId> {
        if self.operator(node) == "ADDRESS" { self.visit_address(node) } else { self.generic_visit(v, node) }
    }

    /// `chk.is_block_accessed` of what a visit gave.
    fn opt_is_block_accessed(&self, block: Option<NodeId>) -> bool {
        let block = block.unwrap_or_else(|| crash("AssertionError: all(isinstance(x, symbols.SYMBOL) for x in symbols_)"));
        self.is_block_accessed(block)
    }

    fn visit_if(&mut self, v: &mut Visitor, node: NodeId) -> Option<NodeId> {
        let n = self.children(node).len();
        if n < 2 {
            crash("IndexError: list index out of range");
        }
        let expr = self.visit_node(v, self.children(node)[0]);
        let then = self.visit_node(v, self.children(node)[1]);
        let else_ = if self.children(node).len() == 3 { self.visit_node(v, self.children(node)[2]) } else { Some(self.nop()) };

        if self.o_level() >= 1 {
            if self.is_null(&[then, else_]) {
                let lineno = self.lineno(node);
                self.warning_empty_if(lineno);
                // The IF goes, but not the calls in its condition: they
                // still have to run.
                let nodes = self.calls_to_keep(v, expr, true, None);
                return Some(if nodes.is_empty() { self.nop() } else { self.new_block_of(&nodes) });
            }
            let block_accessed = self.opt_is_block_accessed(then) || self.opt_is_block_accessed(else_);
            if !block_accessed && self.opt_is_number(expr) {
                // A constant condition.
                let value = self.value(expr.unwrap());
                return if pyval::truthy(&value) { then } else { else_ };
            }
            if self.is_null(&[else_]) && self.children(node).len() == 3 {
                // The empty ELSE goes. What the visits of the condition and
                // of the THEN part gave is not written back here.
                self.pop_child(node, 2);
                return Some(node);
            }
        }
        let results = [expr, then, else_];
        for i in 0..self.children(node).len() {
            if i >= 3 {
                crash("IndexError: tuple index out of range");
            }
            self.set_child(node, i, results[i]);
        }
        Some(node)
    }

    fn visit_while(&mut self, v: &mut Visitor, node: NodeId) -> Option<NodeId> {
        let node = Self::some(self.generic_visit(v, node), "children");
        if self.children(node).len() < 2 {
            crash("IndexError: list index out of range");
        }
        let (expr, body) = (self.children(node)[0], self.children(node)[1]);
        if self.o_level() >= 1 && self.opt_is_number(expr) {
            let value = self.value(expr.unwrap());
            if !pyval::truthy(&value) && !self.opt_is_block_accessed(body) {
                return Some(self.nop());
            }
        }
        Some(node)
    }

    fn visit_for(&mut self, v: &mut Visitor, node: NodeId) -> Option<NodeId> {
        let node = Self::some(self.generic_visit(v, node), "children");
        if self.children(node).len() < 5 {
            crash("IndexError: list index out of range");
        }
        let c = self.children(node).to_vec();
        let (from, to, step, body) = (c[1], c[2], c[3], c[4]);
        if self.o_level() > 0 && self.opt_is_number(from) && self.opt_is_number(to) && self.opt_is_number(step) && !self.opt_is_block_accessed(body) {
            let (from, to, step) = (self.value(from.unwrap()), self.value(to.unwrap()), self.value(step.unwrap()));
            let zero = Val::int(0);
            if pyval::less(&to, &from) && pyval::less(&zero, &step) {
                return Some(self.nop());
            }
            if pyval::less(&from, &to) && pyval::less(&step, &zero) {
                return Some(self.nop());
            }
        }
        Some(node)
    }

    /// For each argument that is an array passed ByRef, takes over whether
    /// its bounds are asked for, which changes the size of a local array's
    /// descriptor, and so the offsets of the locals.
    fn check_if_any_arg_is_an_array_and_needs_lbound_or_ubound(&mut self, params: NodeId, args: NodeId) {
        let args = self.children(args).to_vec();
        let params = self.children(params).to_vec();
        for (arg, param) in args.into_iter().zip(params) {
            let param = Self::some(param, "byref");
            let Some(param_ref) = self.id(param).var() else { crash("AttributeError: 'SymbolRef' object has no attribute 'byref'") };
            if !param_ref.byref || self.class_(param) != Class::Array {
                continue;
            }
            let arg = Self::some(arg, "value");
            let value = Self::some(self.child(arg, 0), "ref");
            let bounds_used = match &self.node(value).kind {
                Kind::Id(data) => match &data.r.kind {
                    RefKind::Array(r) => r.lbound_used && r.ubound_used,
                    _ => crash("AttributeError: 'VarRef' object has no attribute 'lbound_used'"),
                },
                _ => crash(&format!("AttributeError: '{}' object has no attribute 'ref'", self.token(value))),
            };
            if bounds_used {
                continue;
            }
            self.update_bound_status(value);
        }
    }

    fn update_bound_status(&mut self, arg: NodeId) {
        let array_ref = |c: &Compiler, n: NodeId| -> (bool, bool) {
            match &c.node(n).kind {
                Kind::Id(data) => match &data.r.kind {
                    RefKind::Array(r) => (r.lbound_used, r.ubound_used),
                    _ => crash("AttributeError: 'VarRef' object has no attribute 'lbound_used'"),
                },
                _ => crash(&format!("AttributeError: '{}' object has no attribute 'ref'", c.token(n))),
            }
        };
        let (old_lbound, old_ubound) = array_ref(self, arg);
        let (mut lbound, mut ubound) = (old_lbound, old_ubound);
        let requires: Vec<NodeId> = self.node(arg).requires.iter().map(|&(p, _)| p).collect();
        for p in requires {
            // `a or b` does not look at b once a is true.
            if !lbound {
                lbound = array_ref(self, p).0;
            }
            if !ubound {
                ubound = array_ref(self, p).1;
            }
        }
        let r = self.id_mut(arg).var_mut().unwrap();
        r.lbound_used = lbound;
        r.ubound_used = ubound;
        if old_lbound != lbound || old_ubound != ubound {
            let data = self.id(arg);
            if data.scope == Scope::Global {
                return;
            }
            if data.scope == Scope::Local && !data.var().unwrap().byref {
                let scope = data.scope_ref.unwrap_or_else(|| crash("AttributeError: 'NoneType' object has no attribute 'owner'"));
                let owner = self.scope_data(scope).owner.unwrap_or_else(|| crash("AttributeError: 'NoneType' object has no attribute 'locals_size'"));
                let size = self.compute_offsets(scope);
                let entry = self.entry(owner);
                self.id_mut(entry).func_mut().unwrap().locals_size = size;
            }
        }
    }

    // ------------------------------------------------------------------
    // VariableVisitor
    // ------------------------------------------------------------------

    /// `get_var_dependencies`: what a variable's `AT` address is made of,
    /// as (the variable it was found under, the symbol). The Python result
    /// is a set; this is in the order found.
    fn get_var_dependencies(&mut self, v: &mut Visitor, var_entry: NodeId) -> Vec<(Option<NodeId>, NodeId)> {
        fn visit_var(c: &Compiler, v: &mut Visitor, visited: &mut HashSet<Option<NodeId>>, result: &mut Vec<(Option<NodeId>, NodeId)>, entry: Option<NodeId>) {
            if !visited.insert(entry) {
                return;
            }
            let entry = entry.unwrap_or_else(|| crash("AttributeError: 'NoneType' object has no attribute 'token'"));
            if c.token(entry) != "VAR" {
                for &child in c.children(entry) {
                    visit_var(c, v, visited, result, child);
                    let child = child.unwrap_or_else(|| crash("AttributeError: 'NoneType' object has no attribute 'token'"));
                    if matches!(c.token(child), "FUNCTION" | "LABEL" | "VAR" | "VARARRAY") {
                        let dependency = (v.parent_variable, child);
                        if !result.contains(&dependency) {
                            result.push(dependency);
                        }
                    }
                }
                return;
            }
            v.parent_variable = Some(entry);
            if let Some(addr) = c.id(entry).addr {
                visit_var(c, v, visited, result, Some(addr));
            }
        }
        let mut visited = HashSet::new();
        let mut result = Vec::new();
        visit_var(self, v, &mut visited, &mut result, Some(var_entry));
        result
    }

    /// Looks for a variable placed `AT` an address that depends on itself.
    fn visit_vardecl(&mut self, v: &mut Visitor, node: NodeId) -> Option<NodeId> {
        v.var_visited.clear();
        let original = self.entry(node);
        v.original_variable = Some(original);
        for (parent, dependency) in self.get_var_dependencies(v, original) {
            if dependency == original {
                let parent = parent.map_or("None".to_string(), |p| self.str_of(p));
                let msg = format!("Circular dependency between '{}' and '{parent}'", self.id(original).name);
                let lineno = self.lineno(original);
                self.error(lineno, &msg);
                break;
            }
        }
        v.var_visited.clear();
        v.original_variable = None;
        v.parent_variable = None;
        Some(node)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::OptValue;
    use zxr_core::Session;

    fn compiler(level: i128) -> Compiler {
        let mut sess = Session::default();
        sess.stderr = Box::new(std::io::sink());
        let mut c = Compiler::new(sess);
        c.set_option("optimization_level", OptValue::Int(level)).unwrap();
        c
    }

    fn number(c: &mut Compiler, n: i128) -> NodeId {
        c.new_number(Num::Int(n), 1, None)
    }

    fn tokens(c: &Compiler, block: NodeId) -> Vec<String> {
        c.children(block).iter().map(|x| c.token(x.unwrap()).to_string()).collect()
    }

    /// GOTO a: PRINT: PRINT: a: PRINT
    fn block_with_dead_code(c: &mut Compiler) -> NodeId {
        let label = c.access_label("a", 1, None).unwrap();
        let goto = c.new_sentence(1, "GOTO", &[Some(label)], false);
        let p1 = c.new_sentence(2, "PRINT", &[], false);
        let p2 = c.new_sentence(3, "PRINT", &[], false);
        let p3 = c.new_sentence(5, "PRINT", &[], false);
        let end = c.new_sentence(6, "END", &[], true);
        c.make_block(&[Some(goto), Some(p1), Some(p2), Some(label), Some(p3), Some(end)])
    }

    #[test]
    fn unreachable_code_is_removed_at_every_level_but_one() {
        // At -O1 the warning is given and the code left; at -O0 and from
        // -O2 on it goes (without a warning at -O0).
        for (level, expected, warnings) in [(0, vec!["GOTO", "LABEL", "PRINT", "END"], 0), (1, vec!["GOTO", "PRINT", "PRINT", "LABEL", "PRINT", "END"], 1), (2, vec!["GOTO", "LABEL", "PRINT", "END"], 1)] {
            let mut c = compiler(level);
            let block = block_with_dead_code(&mut c);
            c.unreachable_code_visitor(block);
            assert_eq!(tokens(&c, block), expected, "-O{level}");
            assert_eq!(c.sess.has_warnings, warnings, "-O{level}");
        }
    }

    #[test]
    fn optimizer_refolds_and_reorders() {
        // (x + 1) + 2 becomes x + 3 at -O2.
        let mut c = compiler(2);
        let x = c.access_var("x", 1, None, Some(Ty::new(BasicType::UByte))).unwrap();
        let one = number(&mut c, 1);
        let two = number(&mut c, 2);
        let inner = c.make_binary_node("PLUS", Some(x), Some(one), 1, Some(pyval::fold::add), None).unwrap();
        let outer = c.make_binary_node("PLUS", Some(inner), Some(two), 1, Some(pyval::fold::add), None).unwrap();
        let print = c.new_sentence(1, "PRINT", &[Some(outer)], false);
        c.optimizer_visitor(print);
        let result = c.child(print, 0).unwrap();
        assert_eq!(c.token(result), "BINARY");
        assert_eq!(c.child(result, 0), Some(x));
        let folded = c.child(result, 1).unwrap();
        assert_eq!(c.t(folded), "3");
    }

    #[test]
    fn optimizer_drops_constant_branches() {
        let mut c = compiler(1);
        let cond = number(&mut c, 0);
        let p1 = c.new_sentence(1, "PRINT", &[], false);
        let then = c.make_block(&[Some(p1)]);
        let p2 = c.new_sentence(2, "CLS", &[], false);
        let else_ = c.make_block(&[Some(p2)]);
        let if_ = c.new_sentence(1, "IF", &[Some(cond), Some(then), Some(else_)], false);
        let block = c.make_block(&[Some(if_)]);
        c.optimizer_visitor(block);
        assert_eq!(c.child(block, 0), Some(else_));
        // Below -O1 nothing is touched.
        let mut c = compiler(0);
        let cond = number(&mut c, 0);
        let then = c.new_nop();
        let if_ = c.new_sentence(1, "IF", &[Some(cond), Some(then)], false);
        let block = c.make_block(&[Some(if_)]);
        c.optimizer_visitor(block);
        assert_eq!(c.child(block, 0), Some(if_));
    }
}
