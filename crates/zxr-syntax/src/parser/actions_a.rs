//! Grammar actions: the program, declarations and statements (`p_start` to
//! `p_load_data` in `zxbparser.py`).

use super::*;

/// A nested Python list of nodes as the tree keeps it.
fn const_vector(v: &V) -> ConstVector {
    match v {
        V::Node(n) => ConstVector::Elem(*n),
        V::List(items) => ConstVector::List(items.iter().map(const_vector).collect()),
        other => crash(&format!("TypeError: a constant vector was expected, not {other:?}")),
    }
}

/// `len(x)` of a list; `None` has no length.
fn py_len(v: &V) -> usize {
    match v {
        V::List(items) => items.len(),
        _ => crash("TypeError: object of type 'NoneType' has no len()"),
    }
}

impl Parser<'_> {
    /// `make_typecast(TYPE.<basic>, node, lineno)`.
    fn cast(&mut self, basic: BasicType, node: Option<NodeId>, lineno: u32) -> Option<NodeId> {
        self.c.make_typecast(Ty::new(basic), node, lineno)
    }

    /// `make_typecast(type_, node, lineno)` where the type is another
    /// node's and may be missing, which the Python assert rejects once it
    /// gets that far.
    fn cast_to(&mut self, type_: Option<Ty>, node: Option<NodeId>, lineno: u32) -> Option<NodeId> {
        let node = node?;
        self.c.ty(node)?;
        let type_ = type_.unwrap_or_else(|| crash("AssertionError: isinstance(type_, sym.TYPE)"));
        self.c.typecast(type_, Some(node), lineno)
    }

    /// `make_number(value, lineno=lineno)`.
    fn number(&mut self, value: i128, lineno: u32) -> NodeId {
        self.c.make_number(Num::Int(value), lineno, None)
    }

    /// `make_sentence(lineno, keyword, *args)` as the value of the rule.
    fn sentence(&mut self, lineno: u32, keyword: &str, args: &[Option<NodeId>]) -> V {
        V::Node(self.c.make_sentence(lineno, keyword, args))
    }

    /// `is_number(node)`, which is false for `None`.
    fn is_number(&self, node: Option<NodeId>) -> bool {
        node.is_some_and(|n| self.c.is_number(&[n]))
    }

    /// `bool(node.value)`.
    fn truth(&mut self, node: NodeId) -> bool {
        let value = self.c.value(node);
        pyval::truthy(&value)
    }

    /// `node.type_ != TYPE.string`, for a node that must be there.
    fn not_string(&self, node: Option<NodeId>) -> bool {
        let node = node.unwrap_or_else(|| crash("AttributeError: 'NoneType' object has no attribute 'type_'"));
        self.c.ty(node).is_none_or(|t| t != BasicType::String)
    }

    /// `str(node.type_)`.
    fn type_name(&self, node: Option<NodeId>) -> String {
        node.and_then(|n| self.c.ty(n)).map_or("None".to_string(), |t| t.to_string())
    }

    /// `check_and_make_label(lbl, lineno)` for the value of an ID or a
    /// NUMBER token. A number written in hexadecimal or binary is an `int`,
    /// which the Python code takes for a name and then fails to index.
    fn label_of(&mut self, lbl: &V, lineno: u32) -> Option<NodeId> {
        match lbl {
            V::Text(name) => self.c.check_and_make_label(name, lineno),
            V::Float(f) => self.c.check_and_make_label_number(*f, lineno),
            _ => crash("TypeError: 'int' object is not subscriptable"),
        }
    }

    /// `common_type(a, b)` of two types.
    fn common_ty(a: Option<Ty>, b: Option<Ty>) -> Ty {
        let (Some(a), Some(b)) = (a, b) else { crash("AttributeError: 'NoneType' object has no attribute 'is_basic'") };
        if a == b { a } else { Ty::new(crate::types::common_type(a.basic, b.basic)) }
    }

    fn push_loop(&mut self, kind: LoopType, lineno: u32, var: Option<String>) {
        self.c.loops.push(LoopInfo { kind, lineno, var });
    }

    fn pop_loop(&mut self) {
        if self.c.loops.pop().is_none() {
            crash("IndexError: pop from empty list");
        }
    }

    /// `_delayed_init`: a `DIM x = value` that cannot be storage becomes a
    /// LET. For a variable in a code bank the LET goes into a SUB of its
    /// own, bound to that bank, and a call to it takes the LET's place.
    fn delayed_init(&mut self, lineno: u32, entry: Option<NodeId>, sentence: NodeId) -> NodeId {
        let c = &mut *self.c;
        let Some(entry) = entry else { return sentence };
        if !c.function_level.is_empty() {
            return sentence; // a local lives on the stack, and is resident
        }
        let bank = c.bank_of(entry);
        if bank == 0 {
            return sentence;
        }
        let name = format!("__BANKINIT__{}", c.functions_bankinit.len());
        let Some(funcdecl) = c.make_func_declaration(&name, lineno, Class::Sub, None) else { return sentence };
        let func = c.entry(funcdecl);
        {
            let f = c.id_mut(func).func_mut().unwrap();
            f.convention = Convention::Stdcall;
            f.bank = bank;
        }
        c.enter_scope(&name);
        let params = c.make_param_list(None, &[]);
        c.append_child(funcdecl, Some(params));
        c.node_mut(funcdecl).params_size = Some(0);
        let scope = c.current_scope();
        c.id_mut(func).func_mut().unwrap().local_symbol_table = Some(scope);
        let locals_size = c.leave_scope(true);
        c.id_mut(func).func_mut().unwrap().locals_size = locals_size;
        c.scope_data_mut(scope).owner = Some(funcdecl);
        let body = c.make_block(&[Some(sentence)]);
        c.set_body(func, Some(body));
        c.id_mut(func).func_mut().unwrap().forwarded = false;
        c.functions_bankinit.push(func);
        let args = c.make_arg_list(None, &[]);
        let call = c.make_sub_call(&name, lineno, args);
        c.make_block(&[Some(funcdecl), call])
    }

    /// `check_bound` of `p_arr_decl_initialized`: the shape of a constant
    /// vector against the bounds of the array.
    fn check_bound(&mut self, lineno: u32, boundlist: &[NodeId], remaining: &V) -> bool {
        let rows = match remaining {
            V::List(rows) => Some(rows),
            _ => None,
        };
        let Some((&first, rest)) = boundlist.split_first() else {
            let Some(rows) = rows else { return true };
            self.c.error(lineno, &format!("Unexpected extra vector dimensions. It should be {}", rows.len()));
            return false;
        };
        let Some(rows) = rows else {
            self.c.error(lineno, &format!("Mismatched vector size. Missing {} extra dimension(s)", boundlist.len()));
            return false;
        };
        let count = self.c.bound_count(first);
        if rows.len() as i128 != count {
            self.c.error(lineno, &format!("Mismatched vector size. Expected {count} elements, got {}.", rows.len()));
            return false;
        }
        rows.iter().all(|row| self.check_bound(lineno, rest, row))
    }

    /// The length of what a SAVE or LOAD ... DATA names: `entry.memsize` of
    /// an array, `entry.type_.size` of anything else.
    fn data_length(&mut self, entry: NodeId, lineno: u32) -> NodeId {
        let size = if self.c.class_(entry) == Class::Array {
            self.c.memsize(entry)
        } else {
            self.c.ty(entry).unwrap_or_else(|| crash("AttributeError: 'NoneType' object has no attribute 'size'")).size()
        };
        self.number(size as i128, lineno)
    }

    /// The action of `rule`, if it is one of this file's.
    pub(super) fn reduce_a(&mut self, rule: Rule, p: &mut P) -> Result<Option<V>, SyntaxError> {
        use BasicType::*;
        Ok(Some(match rule {
            Rule::p_start => {
                self.c.make_label(ZXBASIC_USER_DATA, 0);
                self.c.make_label(ZXBASIC_USER_DATA_LEN, 0);
                if self.c.print_is_used {
                    self.c.define("___PRINT_IS_USED___", "1");
                }
                if self.lexer.in_state() {
                    // An asm block was never closed. p_error is handed the
                    // production, whose `lineno` is a method, and formatting
                    // the message with it raises.
                    crash("TypeError: %i format: a real number is required, not method");
                }
                let ast = p.node(1);
                let lineno = self.lexer.lineno();
                let zero = self.number(0, lineno);
                let end = self.c.make_sentinel(lineno, "END", &[Some(zero)]);
                if !self.c.is_null(&[ast]) {
                    let ast = ast.unwrap();
                    if matches!(self.c.node(ast).kind, Kind::Block | Kind::Nop) {
                        let last = self.c.children(ast).last().copied().flatten().unwrap_or_else(|| crash("IndexError: list index out of range"));
                        if !self.c.is_ender(last) {
                            self.c.append_child(ast, Some(end));
                        }
                    }
                    self.c.ast = Some(ast);
                } else {
                    self.c.ast = Some(end);
                }
                let ast = self.c.ast.unwrap();
                self.c.check_labels();
                self.c.check_classes();
                if self.c.sess.has_errors > 0 {
                    return Ok(Some(p.take(1)));
                }
                if !self.c.check_pending_labels(ast) {
                    return Ok(Some(p.take(1)));
                }
                if !self.c.check_pending_calls() {
                    return Ok(Some(p.take(1)));
                }
                // The declarations of the globals: a sentence whose keyword
                // is BLOCK, not a block.
                let data_ast = self.c.make_sentence(lineno, "BLOCK", &[]);
                for var in self.c.vars_() {
                    let decl = self.c.make_var_declaration(var);
                    self.c.append_child(data_ast, Some(decl));
                }
                for var in self.c.arrays() {
                    let decl = self.c.make_array_declaration(var);
                    self.c.append_child(data_ast, Some(decl));
                }
                self.c.data_ast = Some(data_ast);
                p.take(1)
            }
            Rule::p_program_program_line => {
                let brk = self.c.make_break(p.lineno(1), p.node(1));
                V::Node(self.c.make_block(&[p.node(1), brk]))
            }
            Rule::p_program => {
                let brk = self.c.make_break(p.lineno(2), p.node(2));
                V::Node(self.c.make_block(&[p.node(1), p.node(2), brk]))
            }
            Rule::p_program_line => {
                if p.len() == 2 {
                    V::Node(self.c.new_nop())
                } else {
                    p.take(1)
                }
            }
            Rule::p_co_statements_co => {
                if p.len() == 3 {
                    p.take(1)
                } else {
                    V::Node(self.c.new_nop())
                }
            }
            Rule::p_co_statements => V::Node(self.c.make_block(&[p.node(1), p.node(2)])),
            Rule::p_statements_co => p.take(1),
            Rule::p_statements_statement => {
                if p.len() == 2 {
                    V::Node(self.c.make_block(&[p.node(1)]))
                } else {
                    V::Node(self.c.make_block(&[p.node(1), p.node(2)]))
                }
            }
            Rule::p_var_decls => p.take(1),
            Rule::p_label => {
                // A line number is a label too: `str(id_)`.
                let id = p.v(1).python_str();
                V::opt(self.c.make_label(&id, p.lineno(1)))
            }
            Rule::p_program_line_label | Rule::p_label_line_co => {
                if p.len() == 3 {
                    V::Node(self.c.make_block(&[p.node(1), p.node(2)]))
                } else {
                    p.take(1)
                }
            }
            Rule::p_label_line_label_line_co => p.take(1),
            Rule::p_program_line_co => {
                if p.len() == 2 {
                    p.take(1)
                } else {
                    V::Node(self.c.make_block(&[p.node(1), p.node(2)]))
                }
            }
            Rule::p_var_decl => {
                let type_ = p.v(3).ty();
                for vardata in p.v(2).list().to_vec() {
                    let V::Id(name, lineno) = vardata else { unreachable!() };
                    let type_ = type_.unwrap_or_else(|| crash("AssertionError: isinstance(type_, symbols.TYPEREF)"));
                    self.c.declare_variable(&name, lineno, type_, None, Class::Var);
                }
                V::None // the declarations are made at the end of parsing
            }
            Rule::p_var_decl_at => {
                let (Some(type_), Some(expr)) = (p.v(3).ty(), p.node(5)) else { return Ok(Some(V::None)) };
                if p.v(2).is_none() {
                    return Ok(Some(V::None));
                }
                let idlist = p.v(2).list();
                if idlist.len() != 1 {
                    self.c.error(p.lineno(1), "Only one variable at a time can be declared this way");
                    return Ok(Some(V::None));
                }
                let V::Id(name, lineno) = idlist[0].clone() else { unreachable!() };
                let Some(entry) = self.c.declare_variable(&name, lineno, type_, None, Class::Var) else { return Ok(Some(V::None)) };
                if self.c.token(expr) == "CONSTEXPR" {
                    let inner = self.c.child(expr, 0);
                    self.c.id_mut(entry).addr = inner;
                } else if !self.c.is_static(&[expr]) {
                    self.c.syntax_error_address_must_be_constant(p.lineno(4));
                } else {
                    let addr = self.c.make_typecast(Ty::new(PTR_TYPE), Some(expr), p.lineno(4));
                    self.c.id_mut(entry).addr = addr;
                    self.c.mark_entry_as_accessed(entry);
                    if self.c.id(entry).scope == Scope::Local {
                        let name = self.c.id(entry).name.clone();
                        self.c.make_static(&name);
                    }
                }
                V::None
            }
            Rule::p_var_decl_ini => {
                let keyword = p.text(1).to_string();
                let idlist = p.v(2).list().to_vec();
                let mut typedef = p.v(3).ty();
                if idlist.len() != 1 {
                    self.c.error(p.lineno(1), "Initialized variables must be declared one by one.");
                    return Ok(Some(V::None));
                }
                let Some(mut expr) = p.node(5) else { return Ok(Some(V::None)) };
                let V::Id(name, id_lineno) = idlist[0].clone() else { unreachable!() };
                if self.c.is_static(&[expr]) && matches!(self.c.node(expr).kind, Kind::Unary { .. }) {
                    expr = self.c.make_constexpr(p.lineno(4), expr); // delayed constant evaluation
                }
                let declared = typedef.unwrap_or_else(|| crash("AttributeError: 'NoneType' object has no attribute 'implicit'"));
                if declared.implicit {
                    let expr_ty = self.c.ty(expr).unwrap_or_else(|| crash("AssertionError: isinstance(type_, SymbolTYPE)"));
                    typedef = Some(Ty::reference(expr_ty.basic, self.lexer.lineno(), true));
                }
                let typedef = typedef.unwrap();
                let value = self.c.make_typecast(typedef, Some(expr), p.lineno(4));
                let mut defval = None;
                if self.c.is_static(&[expr]) {
                    // A conversion that failed leaves no value to ask.
                    let value = value.unwrap_or_else(|| crash("AttributeError: 'NoneType' object has no attribute 'type_'"));
                    if self.c.ty(value).is_none_or(|t| t != BasicType::String) {
                        defval = Some(value);
                    }
                }
                if keyword == "DIM" {
                    self.c.declare_variable(&name, id_lineno, typedef, defval, Class::Var);
                } else {
                    // keyword == "CONST"
                    if defval.is_none() {
                        let value = value.unwrap_or_else(|| crash("AttributeError: 'NoneType' object has no attribute 'token'"));
                        if !self.c.is_static_str(&[value]) {
                            self.c.syntax_error_not_constant(p.lineno(4));
                            return Ok(Some(V::None));
                        }
                        defval = Some(value);
                    }
                    self.c.declare_const(&name, id_lineno, typedef, defval);
                }
                if defval.is_none() {
                    // A delayed initialization.
                    let entry = self.c.access_var(&name, p.lineno(1), None, None);
                    let let_ = self.c.make_sentence(p.lineno(1), "LET", &[entry, value]);
                    V::Node(self.delayed_init(p.lineno(1), entry, let_))
                } else {
                    V::None
                }
            }
            Rule::p_singleid => V::Id(p.text(1).to_string(), p.lineno(1)),
            Rule::p_idlist_id => V::List(vec![p.take(1)]),
            Rule::p_idlist_idlist_id => {
                let id = p.take(3);
                let mut list = p.take(1).into_list();
                list.push(id);
                V::List(list)
            }
            Rule::p_arr_decl => V::None,
            Rule::p_arr_decl_attr => {
                let Some(mut expr) = p.node(3) else { return Ok(Some(V::None)) };
                if p.v(1).is_none() {
                    return Ok(Some(V::None));
                }
                let V::Id(name, id_lineno) = p.v(1).clone() else { unreachable!() };
                if self.c.token(expr) == "CONSTEXPR" {
                    expr = self.c.child(expr, 0).unwrap();
                    if matches!(&self.c.node(expr).kind, Kind::Unary { operator } if operator == "ADDRESS") {
                        // Must be an ID.
                        let operand = self.c.child(expr, 0).unwrap();
                        if self.c.token(operand) == "ARRAYACCESS" {
                            if self.c.array_offset(operand).is_none() {
                                // The message is given the line of a fourth
                                // symbol, and the rule has three.
                                crash("IndexError: list index out of range");
                            }
                        } else {
                            if !matches!(self.c.token(operand), "ID" | "VAR" | "LABEL") {
                                self.c.error(p.lineno(3), "Only addresses of identifiers are allowed");
                                return Ok(Some(V::None));
                            }
                            self.c.id_mut(operand).has_address = Some(true);
                        }
                    }
                } else if !self.c.is_static(&[expr]) {
                    self.c.syntax_error_address_must_be_constant(p.lineno(3));
                    return Ok(Some(V::None));
                }
                let arr_entry = self.c.access_array(&name, id_lineno, None, None).unwrap_or_else(|| crash("AttributeError: 'NoneType' object has no attribute 'addr'"));
                let addr = self.c.make_typecast(Ty::new(PTR_TYPE), Some(expr), p.lineno(2));
                self.c.id_mut(arr_entry).addr = addr;
                if self.c.id(arr_entry).scope == Scope::Local {
                    let name = self.c.id(arr_entry).name.clone();
                    self.c.make_static(&name);
                }
                p.take(1)
            }
            Rule::p_decl_arr => {
                let idlist = p.v(2).list().to_vec();
                if idlist.len() != 1 {
                    self.c.error(p.lineno(1), "Array declaration only allows one variable name at a time");
                } else {
                    let V::Id(name, lineno) = idlist[0].clone() else { unreachable!() };
                    let type_ = p.v(6).ty().unwrap_or_else(|| crash("AssertionError: isinstance(type_, symbols.TYPEREF)"));
                    let bounds = p.node(4).unwrap_or_else(|| crash("AssertionError: isinstance(bounds, symbols.BOUNDLIST)"));
                    self.c.declare_array(&name, lineno, type_, bounds, None, None);
                }
                idlist[0].clone()
            }
            Rule::p_arr_decl_initialized => {
                let (Some(bounds), Some(type_)) = (p.node(4), p.v(6).ty()) else { return Ok(Some(V::None)) };
                if p.v(8).is_none() {
                    return Ok(Some(V::None));
                }
                let vector = p.take(8);
                let boundlist: Vec<NodeId> = self.c.children(bounds).iter().flatten().copied().collect();
                if !self.check_bound(p.lineno(8), &boundlist, &vector) {
                    return Ok(Some(V::None));
                }
                let V::Id(name, lineno) = p.v(2).list()[0].clone() else { unreachable!() };
                if let Some(entry) = self.c.declare_array(&name, lineno, type_, bounds, None, None) {
                    self.c.id_mut(entry).var_mut().unwrap().default_vector = Some(const_vector(&vector));
                }
                let Some(entry) = self.c.get_entry(&name, None, None) else { return Ok(Some(V::None)) };
                if type_ == BasicType::String || self.c.ty(entry).is_some_and(|t| t == BasicType::String) {
                    self.c.syntax_error_cannot_initialize_array_of_type(p.lineno(1), "string");
                }
                V::None
            }
            Rule::p_bound_list => V::Node(self.c.make_bound_list(p.node(1), &[])),
            Rule::p_bound_list_bound => V::Node(self.c.make_bound_list(p.node(1), &[p.node(3)])),
            Rule::p_bound => {
                let base = self.c.opts.int("array_base");
                let lower = self.number(base, p.lineno(1));
                let upper = p.node(1).unwrap_or_else(|| crash("AssertionError: all(isinstance(x, symbols.SYMBOL) for x in p)"));
                V::opt(self.c.make_bound(lower, upper, self.lexer.lineno()))
            }
            Rule::p_bound_to_bound => {
                let (Some(lower), Some(upper)) = (p.node(1), p.node(3)) else { crash("AssertionError: all(isinstance(x, symbols.SYMBOL) for x in p)") };
                V::opt(self.c.make_bound(lower, upper, p.lineno(2)))
            }
            Rule::p_const_vector => p.take(2),
            Rule::p_const_vector_elem_list => {
                let Some(expr) = p.node(1) else { return Ok(Some(V::None)) };
                if !self.c.is_static(&[expr]) {
                    if matches!(self.c.node(expr).kind, Kind::Unary { .. }) {
                        V::List(vec![V::Node(self.c.make_constexpr(p.lineno(1), expr))])
                    } else {
                        self.c.syntax_error_not_constant(self.lexer.lineno());
                        V::None
                    }
                } else {
                    V::List(vec![V::Node(expr)])
                }
            }
            Rule::p_const_vector_elem_list_list => {
                let Some(expr) = p.node(3) else { return Ok(Some(V::None)) };
                if p.v(1).is_none() {
                    return Ok(Some(V::None));
                }
                let tmp = if !self.c.is_static(&[expr]) {
                    if matches!(self.c.node(expr).kind, Kind::Unary { .. }) {
                        self.c.make_constexpr(p.lineno(2), expr)
                    } else {
                        self.c.syntax_error_not_constant(p.lineno(2));
                        return Ok(Some(V::None));
                    }
                } else {
                    expr
                };
                let mut list = p.take(1).into_list();
                list.push(V::Node(tmp));
                V::List(list)
            }
            Rule::p_const_vector_list => V::List(vec![p.take(1)]),
            Rule::p_const_vector_vector_list => {
                let first = match p.v(1) {
                    V::List(rows) => &rows[0],
                    _ => crash("TypeError: 'NoneType' object is not subscriptable"),
                };
                if py_len(p.v(3)) != py_len(first) {
                    self.c.error(p.lineno(2), "All rows must have the same number of elements");
                    return Ok(Some(V::None));
                }
                let row = p.take(3);
                let mut rows = p.take(1).into_list();
                rows.push(row);
                V::List(rows)
            }
            Rule::p_staement_func_decl => p.take(1),
            Rule::p_statement_border => {
                let a = self.cast(UByte, p.node(2), p.lineno(1));
                self.sentence(p.lineno(1), "BORDER", &[a])
            }
            Rule::p_statement_plot => {
                let a = self.cast(UByte, p.node(2), p.lineno(3));
                let b = self.cast(UByte, p.node(4), p.lineno(3));
                self.sentence(p.lineno(1), "PLOT", &[a, b])
            }
            Rule::p_statement_plot_attr => {
                let a = self.cast(UByte, p.node(3), p.lineno(4));
                let b = self.cast(UByte, p.node(5), p.lineno(4));
                self.sentence(p.lineno(1), "PLOT", &[a, b, p.node(2)])
            }
            Rule::p_statement_draw3 => {
                let a = self.cast(Integer, p.node(2), p.lineno(3));
                let b = self.cast(Integer, p.node(4), p.lineno(5));
                let c = self.cast(Float, p.node(6), p.lineno(5));
                self.sentence(p.lineno(1), "DRAW3", &[a, b, c])
            }
            Rule::p_statement_draw3_attr => {
                let a = self.cast(Integer, p.node(3), p.lineno(4));
                let b = self.cast(Integer, p.node(5), p.lineno(6));
                let c = self.cast(Float, p.node(7), p.lineno(6));
                self.sentence(p.lineno(1), "DRAW3", &[a, b, c, p.node(2)])
            }
            Rule::p_statement_draw => {
                let a = self.cast(Integer, p.node(2), p.lineno(3));
                let b = self.cast(Integer, p.node(4), p.lineno(3));
                self.sentence(p.lineno(1), "DRAW", &[a, b])
            }
            Rule::p_statement_draw_attr => {
                let a = self.cast(Integer, p.node(3), p.lineno(4));
                let b = self.cast(Integer, p.node(5), p.lineno(4));
                self.sentence(p.lineno(1), "DRAW", &[a, b, p.node(2)])
            }
            Rule::p_statement_circle => {
                let a = self.cast(Byte, p.node(2), p.lineno(3));
                let b = self.cast(Byte, p.node(4), p.lineno(5));
                let c = self.cast(Byte, p.node(6), p.lineno(5));
                self.sentence(p.lineno(1), "CIRCLE", &[a, b, c])
            }
            Rule::p_statement_circle_attr => {
                let a = self.cast(Byte, p.node(3), p.lineno(4));
                let b = self.cast(Byte, p.node(5), p.lineno(6));
                let c = self.cast(Byte, p.node(7), p.lineno(6));
                self.sentence(p.lineno(1), "CIRCLE", &[a, b, c, p.node(2)])
            }
            Rule::p_statement_cls => self.sentence(p.lineno(1), "CLS", &[]),
            Rule::p_statement_asm => V::Node(self.c.make_asm_sentence(p.text(1), p.lineno(1), false)),
            Rule::p_statement_randomize => {
                let zero = self.c.make_number(Num::Int(0), p.lineno(1), Some(Ty::new(ULong)));
                self.sentence(p.lineno(1), "RANDOMIZE", &[Some(zero)])
            }
            Rule::p_statement_randomize_expr => {
                let a = self.cast(ULong, p.node(2), p.lineno(1));
                self.sentence(p.lineno(1), "RANDOMIZE", &[a])
            }
            Rule::p_statement_beep => {
                let a = self.cast(Float, p.node(2), p.lineno(1));
                let b = self.cast(Float, p.node(4), p.lineno(3));
                self.sentence(p.lineno(1), "BEEP", &[a, b])
            }
            Rule::p_statement_call => {
                let name = p.text(1).to_string();
                if p.len() > 2 && p.v(2).is_none() {
                    V::None
                } else if p.len() == 2 {
                    let entry = self.c.get_entry(&name, None, None);
                    if entry.is_some_and(|e| matches!(self.c.class_(e), Class::Label | Class::Unknown)) {
                        V::opt(self.c.make_label(&name, p.lineno(1)))
                    } else {
                        let args = self.c.make_arg_list(None, &[]);
                        V::opt(self.c.make_sub_call(&name, p.lineno(1), args))
                    }
                } else {
                    let args = p.node(2).unwrap();
                    V::opt(self.c.make_sub_call(&name, p.lineno(1), args))
                }
            }
            Rule::p_assignment => {
                self.c.let_assignment = false; // no longer in a LET
                let name = p.text(1).to_string();
                let lineno = p.lineno(1);
                let Some(expr) = p.node(2) else { return Ok(Some(V::None)) };
                // The Python code first asks whether the expression is a VAR
                // of unknown class, which no VAR is.
                let q1class = if self.c.token(expr) == "VAR" { self.c.class_(expr) } else { Class::Unknown };
                let default_type = self.c.ty(expr);
                // HINT: None only if the variable was not declared with DIM and --strict is in use
                let Some(variable) = self.c.access_id(&name, lineno, None, default_type, q1class, false) else { return Ok(Some(V::None)) };
                if self.c.class_(variable) == Class::Unknown {
                    self.c.to_var(variable, None); // the variable is implicit
                }
                let class = self.c.class_(variable);
                if !matches!(class, Class::Var | Class::Array) {
                    let name = self.c.name(variable).to_string();
                    self.c.syntax_error_cannot_assign_not_a_var(lineno, &name);
                    return Ok(Some(V::None));
                }
                if class == Class::Var && q1class == Class::Array {
                    self.c.error(lineno, "Cannot assign an array to an scalar variable");
                    return Ok(Some(V::None));
                }
                let value = self.cast_to(self.c.ty(variable), Some(expr), lineno);
                self.sentence(lineno, "LET", &[Some(variable), value])
            }
            Rule::p_lexpr => {
                self.c.let_assignment = true; // about to start a LET sentence
                let i = if p.text(1) == "LET" { 2 } else { 1 };
                let name = p.text(i).to_string();
                self.c.access_id(&name, p.lineno(i), None, None, Class::Unknown, false);
                V::Text(name)
            }
            Rule::p_array_copy => {
                let (i, j) = if p.text(1) == "LET" { (2, 4) } else { (1, 3) };
                let (array_id1, array_id2) = (p.text(i).to_string(), p.text(j).to_string());
                let (l1, l2) = (p.lineno(i), p.lineno(j));
                let larray = self.c.access_id(&array_id1, l1, None, None, Class::Unknown, false);
                let rarray = self.c.access_id(&array_id2, l2, None, None, Class::Unknown, false);
                let (Some(larray), Some(rarray)) = (larray, rarray) else { return Ok(Some(V::None)) };
                if self.c.ty(larray) != self.c.ty(rarray) {
                    self.c.error(l1, "Arrays must have the same element type");
                    return Ok(Some(V::None));
                }
                if self.c.memsize(larray) != self.c.memsize(rarray) {
                    self.c.error(l1, &format!("Arrays '{array_id1}' and '{array_id2}' must have the same size"));
                    return Ok(Some(V::None));
                }
                if self.c.count(larray) != self.c.count(rarray) {
                    let msg = format!("Arrays '{}' and '{}' don't have the same number of dimensions", self.c.name(larray), self.c.name(rarray));
                    self.c.warning(l1, &msg);
                } else {
                    let b1: Vec<NodeId> = self.c.children(self.c.bounds(larray)).iter().flatten().copied().collect();
                    let b2: Vec<NodeId> = self.c.children(self.c.bounds(rarray)).iter().flatten().copied().collect();
                    if b1.iter().zip(&b2).any(|(&x, &y)| self.c.bound_count(x) != self.c.bound_count(y)) {
                        self.c.warning(l1, &format!("Arrays '{array_id1}' and '{array_id2}' don't have the same dimensions"));
                    }
                }
                // Array copy
                self.c.mark_entry_as_accessed(larray);
                self.c.mark_entry_as_accessed(rarray);
                self.sentence(p.lineno(1), "ARRAYCOPY", &[Some(larray), Some(rarray)])
            }
            Rule::p_arr_assignment => {
                let i = if p.text(1).to_uppercase() == "LET" { 2 } else { 1 };
                let id = p.text(i).to_string();
                let (Some(arg_list), Some(expr)) = (p.node(i + 1), p.node(i + 3)) else { return Ok(Some(V::None)) };
                let lineno = p.lineno(i);
                let Some(entry) = self.c.access_call(&id, lineno, None, None) else { return Ok(Some(V::None)) };
                if self.c.ty(entry).is_some_and(|t| t == BasicType::String) {
                    let variable = self.c.access_array(&id, lineno, None, None).unwrap_or_else(|| crash("AttributeError: 'NoneType' object has no attribute 'ref'"));
                    // No bounds means they are not known (a ByRef parameter).
                    let bounds = self.c.children(self.c.bounds(variable)).len();
                    if bounds != 0 && bounds + 1 == self.c.children(arg_list).len() {
                        let last = self.c.children(arg_list).len() - 1;
                        let arg = self.c.pop_child(arg_list, last).unwrap();
                        let ss = self.c.child(arg, 0);
                        return Ok(Some(V::opt(self.c.make_array_substr_assign(lineno, &id, Some(arg_list), Some((ss, ss)), Some(expr)))));
                    }
                }
                let Some(arr) = self.c.make_array_access(&id, lineno, arg_list) else { return Ok(Some(V::None)) };
                let value = self.cast_to(self.c.ty(arr), Some(expr), lineno);
                if self.c.id(entry).addr.is_some() {
                    self.c.mark_entry_as_accessed(entry);
                }
                self.sentence(p.lineno(1), "LETARRAY", &[Some(arr), value])
            }
            Rule::p_substr_assignment_no_let => {
                // Only a substring assignment, `a$(i + 3) = "."`: an array
                // would have come as ARRAY_ID.
                let name = p.text(1).to_string();
                let Some(entry) = self.c.access_call(&name, p.lineno(1), None, None) else { return Ok(Some(V::None)) };
                if self.c.class_(entry) == Class::Unknown {
                    let entry_name = self.c.name(entry).to_string();
                    self.c.warning_uninitalized_string_var(p.lineno(1), &entry_name);
                    self.c.to_var(entry, None);
                }
                if self.not_string(p.node(6)) {
                    let type_ = self.type_name(p.node(6));
                    self.c.syntax_error_expected_string(p.lineno(5), &type_);
                }
                let lineno = p.lineno(2);
                let index_type = Ty::new(STR_INDEX_TYPE);
                let base = self.c.make_number(Num::Int(self.c.opts.int("string_base")), lineno, Some(index_type));
                let substr = self.c.make_typecast(index_type, p.node(3), lineno);
                let low = self.c.make_binary(lineno, "MINUS", substr, Some(base), Some(pyval::fold::sub), None);
                let high = self.c.make_binary(lineno, "MINUS", substr, Some(base), Some(pyval::fold::sub), None);
                self.sentence(p.lineno(1), "LETSUBSTR", &[Some(entry), low, high, p.node(6)])
            }
            Rule::p_substr_assignment => {
                let (Some(args), Some(expr)) = (p.node(3), p.node(5)) else { return Ok(Some(V::None)) };
                let name = p.text(2).to_string();
                let lineno = p.lineno(2);
                let Some(entry) = self.c.access_call(&name, lineno, None, None) else { return Ok(Some(V::None)) };
                if self.c.class_(entry) == Class::Unknown {
                    self.c.to_var(entry, None);
                }
                if self.c.class_(entry) != Class::Var {
                    self.c.syntax_error_cannot_assign_not_a_var(lineno, &name);
                    return Ok(Some(V::None));
                }
                if self.not_string(Some(entry)) {
                    let type_ = self.type_name(Some(entry));
                    self.c.syntax_error_expected_string(lineno, &type_);
                    return Ok(Some(V::None));
                }
                if self.not_string(Some(expr)) {
                    let type_ = self.type_name(Some(expr));
                    self.c.syntax_error_expected_string(p.lineno(4), &type_);
                    return Ok(Some(V::None));
                }
                let arg_list: Vec<NodeId> = self.c.children(args).iter().flatten().copied().collect();
                if arg_list.len() > 1 {
                    self.c.error(lineno, "Accessing string with too many indexes. Expected only one.");
                    return Ok(Some(V::None));
                }
                let index_type = Ty::new(STR_INDEX_TYPE);
                let substr = if arg_list.len() == 1 {
                    let index = self.c.child(arg_list[0], 0);
                    (self.c.make_typecast(index_type, index, lineno), self.c.make_typecast(index_type, index, lineno))
                } else {
                    let low = self.number(MIN_STRSLICE_IDX, lineno);
                    let low = self.c.make_typecast(index_type, Some(low), lineno);
                    let high = self.number(MAX_STRSLICE_IDX, lineno);
                    (low, self.c.make_typecast(index_type, Some(high), lineno))
                };
                let base = self.c.make_number(Num::Int(self.c.opts.int("string_base")), lineno, Some(index_type));
                let low = self.c.make_binary(lineno, "MINUS", substr.0, Some(base), Some(pyval::fold::sub), None);
                let high = self.c.make_binary(lineno, "MINUS", substr.1, Some(base), Some(pyval::fold::sub), None);
                self.sentence(p.lineno(1), "LETSUBSTR", &[Some(entry), low, high, Some(expr)])
            }
            Rule::p_str_assign => {
                let (q, r, s, lineno) = if p.text(1).to_uppercase() != "LET" { (1, 4, 2, p.lineno(3)) } else { (2, 5, 3, p.lineno(4)) };
                if p.v(q).is_none() || p.v(s).is_none() {
                    return Ok(Some(V::None));
                }
                let name = p.text(q).to_string();
                let r = p.node(r);
                if self.not_string(r) {
                    let type_ = self.type_name(r);
                    self.c.syntax_error_expected_string(lineno, &type_);
                }
                let Some(entry) = self.c.access_var(&name, lineno, None, Some(Ty::reference(BasicType::String, lineno, false))) else { return Ok(Some(V::None)) };
                let (s0, s1) = (p.v(s).list()[0].node(), p.v(s).list()[1].node());
                self.sentence(p.lineno(1), "LETSUBSTR", &[Some(entry), s0, s1, r])
            }
            Rule::p_goto => {
                let label = p.v(2).clone();
                match self.label_of(&label, p.lineno(2)) {
                    Some(entry) => {
                        let keyword = p.text(1).to_uppercase();
                        self.sentence(p.lineno(1), &keyword, &[Some(entry)])
                    }
                    None => V::None,
                }
            }
            Rule::p_go => {
                let mut word = p.text(1).to_string();
                if word == "GO" {
                    word.push_str(p.text(2));
                }
                if word == "GOSUB" && !self.c.function_level.is_empty() {
                    self.c.error(p.lineno(1), "GOSUB not allowed within SUB or FUNCTION");
                }
                V::Text(word)
            }
            // region [IF sentence]
            Rule::p_if_sentence => {
                let cond = p.node(1);
                let (stat, endif) = if p.len() == 6 {
                    (Some(self.c.make_block(&[p.node(3), p.node(4)])), p.node(5))
                } else if p.len() == 5 {
                    (p.node(3), p.node(4))
                } else {
                    (Some(self.c.new_nop()), p.node(3))
                };
                let block = self.c.make_block(&[stat, endif]);
                self.sentence(p.lineno(2), "IF", &[cond, Some(block)])
            }
            Rule::p_endif => {
                if matches!(p.v(1), V::Text(t) if t == "END" || t == "ENDIF") {
                    V::Node(self.c.new_nop())
                } else {
                    p.take(1)
                }
            }
            Rule::p_statement_if => p.take(1),
            Rule::p_statement_if_then_endif => {
                let block = self.c.make_block(&[p.node(2), p.node(3)]);
                self.sentence(p.lineno(1), "IF", &[p.node(1), Some(block)])
            }
            Rule::p_single_line_if => self.sentence(p.lineno(1), "IF", &[p.node(1), p.node(2)]),
            Rule::p_if_elseif => {
                let cond = p.node(1);
                let (stats, eliflist) = if p.len() == 5 { (p.node(3), p.node(4)) } else { (Some(self.c.new_nop()), p.node(3)) };
                self.sentence(p.lineno(2), "IF", &[cond, stats, eliflist])
            }
            Rule::p_elseif_part => {
                if matches!(p.v(1), V::Text(t) if t == "ELSEIF") {
                    let label = self.c.new_nop(); // no label
                    V::List(vec![V::Node(label), p.take(2)])
                } else {
                    V::List(vec![p.take(1), p.take(3)])
                }
            }
            Rule::p_elseif_list => {
                let (label, cond) = (p.v(1).list()[0].node(), p.v(1).list()[1].node());
                let mut then = p.node(2);
                let else_ = match p.v(3) {
                    // An else part.
                    V::List(parts) => {
                        let parts: Vec<Option<NodeId>> = parts.iter().map(V::node).collect();
                        Some(self.c.make_block(&parts))
                    }
                    endif => {
                        let endif = endif.node();
                        then = Some(self.c.make_block(&[then, endif]));
                        None
                    }
                };
                let if_ = self.c.make_sentence(p.lineno(1), "IF", &[cond, then, else_]);
                V::Node(self.c.make_block(&[label, Some(if_)]))
            }
            Rule::p_elseif_elseiflist => {
                let (label, cond) = (p.v(1).list()[0].node(), p.v(1).list()[1].node());
                let if_ = self.c.make_sentence(p.lineno(1), "IF", &[cond, p.node(2), p.node(3)]);
                V::Node(self.c.make_block(&[label, Some(if_)]))
            }
            Rule::p_else_part_endif => {
                if matches!(p.v(2), V::Text(t) if t == "\n") {
                    if p.len() == 4 {
                        let nop = self.c.new_nop();
                        V::List(vec![V::Node(nop), p.take(3)])
                    } else if p.len() == 6 {
                        V::List(vec![p.take(3), p.take(4), p.take(5)])
                    } else {
                        V::List(vec![p.take(3), p.take(4)])
                    }
                } else {
                    V::List(vec![p.take(2), p.take(3)])
                }
            }
            Rule::p_else_part => {
                let nop = self.c.new_nop();
                V::List(vec![p.take(2), V::Node(nop)])
            }
            Rule::p_else_part_is_inline => p.take(1),
            Rule::p_else_part_label => {
                let block = self.c.make_block(&[p.node(1), p.node(3)]);
                V::List(vec![V::Node(block), p.take(4)])
            }
            Rule::p_if_then_part => {
                let Some(expr) = p.node(2) else { return Ok(Some(V::None)) };
                if self.c.is_number(&[expr]) {
                    let cond = self.truth(expr);
                    self.c.warning_condition_is_always(p.lineno(1), cond);
                }
                V::Node(expr)
            }
            Rule::p_if_inline => {
                let if_ = p.node(1).unwrap_or_else(|| crash("AttributeError: 'NoneType' object has no attribute 'append_child'"));
                let parts = p.v(2).list();
                let block = self.c.make_block(&[parts[0].node(), parts[1].node()]);
                self.c.append_child(if_, Some(block));
                V::Node(if_)
            }
            Rule::p_if_else => {
                let parts = p.v(4).list();
                let block = self.c.make_block(&[parts[0].node(), parts[1].node()]);
                self.sentence(p.lineno(2), "IF", &[p.node(1), p.node(3), Some(block)])
            }
            Rule::p_then => V::None,
            // region [FOR sentence]
            Rule::p_for_sentence => {
                // A FOR with an error in it is left on the loop stack.
                let Some(for_) = p.node(1) else { return Ok(Some(V::None)) };
                if self.c.is_null(&[Some(for_)]) {
                    return Ok(Some(V::Node(for_)));
                }
                let block = self.c.make_block(&[p.node(2), p.node(3)]);
                self.c.append_child(for_, Some(block));
                self.pop_loop();
                V::Node(for_)
            }
            Rule::p_next => {
                if matches!(p.v(1), V::Text(t) if t == "NEXT") {
                    V::Node(self.c.new_nop())
                } else {
                    p.take(1)
                }
            }
            Rule::p_next1 => {
                let (p1, p3) = if matches!(p.v(1), V::Text(t) if t == "NEXT") { (V::Node(self.c.new_nop()), p.text(2).to_string()) } else { (p.take(1), p.text(3).to_string()) };
                let var = self.c.loops.last().unwrap_or_else(|| crash("IndexError: list index out of range")).var.clone();
                if var.as_deref() != Some(p3.as_str()) {
                    self.c.syntax_error_wrong_for_var(p.lineno(2), var.as_deref().unwrap_or("None"), &p3);
                    V::Node(self.c.new_nop())
                } else {
                    p1
                }
            }
            Rule::p_for_sentence_start => {
                let name = p.text(2).to_string();
                self.push_loop(LoopType::For, p.lineno(1), Some(name.clone()));
                let (Some(from), Some(to), Some(step)) = (p.node(4), p.node(6), p.node(7)) else { return Ok(Some(V::None)) };
                if self.c.is_number(&[from, to, step]) {
                    let (a, b, s) = (self.c.num_value(from), self.c.num_value(to), self.c.num_value(step));
                    if a != b && s.is_zero() {
                        self.c.warning(p.lineno(5), "STEP value is 0 and FOR might loop forever");
                    }
                    if a > b && s > Num::Int(0) {
                        self.c.warning(p.lineno(5), "FOR start value is greater than end. This FOR loop is useless");
                    }
                    if a < b && s < Num::Int(0) {
                        self.c.warning(p.lineno(2), "FOR start value is lower than end. This FOR loop is useless");
                    }
                }
                let id_type = Self::common_ty(Some(Self::common_ty(self.c.ty(from), self.c.ty(to))), self.c.ty(step));
                let Some(variable) = self.c.access_var(&name, p.lineno(2), None, Some(id_type)) else { return Ok(Some(V::None)) };
                self.c.mark_entry_as_accessed(variable);
                let var_ty = self.c.ty(variable);
                let expr1 = self.cast_to(var_ty, Some(from), p.lineno(3));
                let expr2 = self.cast_to(var_ty, Some(to), p.lineno(5));
                let expr3 = self.cast_to(var_ty, Some(step), self.lexer.lineno());
                self.sentence(p.lineno(1), "FOR", &[Some(variable), expr1, expr2, expr3])
            }
            Rule::p_step => V::Node(self.number(1, self.lexer.lineno())),
            Rule::p_step_expr => p.take(2),
            // endregion
            Rule::p_end => {
                let q = if p.len() == 2 { Some(self.number(0, p.lineno(1))) } else { p.node(2) };
                self.sentence(p.lineno(1), "END", &[q])
            }
            Rule::p_error_raise => {
                let q = self.number(1, p.lineno(2));
                let code = self.cast(UByte, p.node(2), p.lineno(1));
                let r = self.c.make_binary(p.lineno(1), "MINUS", code, Some(q), Some(pyval::fold::sub), None);
                self.sentence(p.lineno(1), "ERROR", &[r])
            }
            Rule::p_stop_raise => {
                let q = if p.len() == 2 { Some(self.number(9, p.lineno(1))) } else { p.node(2) };
                let z = self.number(1, p.lineno(1));
                let code = self.cast(UByte, q, p.lineno(1));
                let r = self.c.make_binary(p.lineno(1), "MINUS", code, Some(z), Some(pyval::fold::sub), None);
                self.sentence(p.lineno(1), "STOP", &[r])
            }
            Rule::p_loop | Rule::p_label_end_while => {
                // LOOP, WEND or END WHILE with no label before it.
                if matches!(p.v(1), V::Text(_)) {
                    V::None
                } else {
                    p.take(1)
                }
            }
            Rule::p_do_loop => {
                let q = if p.len() == 4 { Some(self.c.make_block(&[p.node(2), p.node(3)])) } else { p.node(2) };
                if matches!(p.v(1), V::Text(t) if t == "DO") {
                    self.push_loop(LoopType::Do, p.lineno(1), None);
                }
                if q.is_none() {
                    self.c.warning(p.lineno(1), "Infinite empty loop");
                }
                // An infinite loop and no warnings
                let sentence = self.sentence(p.lineno(1), "DO_LOOP", &[q]);
                self.pop_loop();
                sentence
            }
            Rule::p_do_loop_until | Rule::p_do_loop_while => {
                let (q, r) = if p.len() == 6 { (Some(self.c.make_block(&[p.node(2), p.node(3)])), p.node(5)) } else { (p.node(2), p.node(4)) };
                if matches!(p.v(1), V::Text(t) if t == "DO") {
                    self.push_loop(LoopType::Do, p.lineno(1), None);
                }
                let keyword = if rule == Rule::p_do_loop_until { "DO_UNTIL" } else { "DO_WHILE" };
                let sentence = self.sentence(p.lineno(1), keyword, &[r, q]);
                self.pop_loop();
                if self.is_number(r) {
                    let cond = self.truth(r.unwrap());
                    self.c.warning_condition_is_always(p.lineno(3), cond);
                }
                if q.is_none() {
                    self.c.warning_empty_loop(p.lineno(3));
                }
                sentence
            }
            Rule::p_data => {
                let current = self.c.data_ptr_current.clone();
                let label = self.c.make_label(&current, p.lineno(1));
                let Some(args) = p.node(2) else { return Ok(Some(V::None)) };
                if !self.c.function_level.is_empty() {
                    self.c.error(p.lineno(1), "DATA not allowed within Functions nor Subs");
                    return Ok(Some(V::None));
                }
                let mut datas = Vec::new();
                for d in self.c.children(args).to_vec().into_iter().flatten() {
                    let value = self.c.child(d, 0).unwrap_or_else(|| crash("AssertionError: all(isinstance(x, symbols.SYMBOL) for x in p)"));
                    if self.c.is_static(&[value]) {
                        datas.push(d);
                        continue;
                    }
                    // Not a constant: a function that returns the value.
                    let new_lbl = format!("__DATA__FUNCPTR__{}", self.c.data_functions.len());
                    let value_ty = self.c.ty(value);
                    let Some(entry) = self.c.make_func_declaration(&new_lbl, p.lineno(1), Class::Function, value_ty) else { continue };
                    let func = self.c.entry(entry);
                    self.c.id_mut(func).func_mut().unwrap().convention = Convention::Fastcall;
                    self.c.enter_scope(&new_lbl);
                    let scope = self.c.current_scope();
                    self.c.id_mut(func).func_mut().unwrap().local_symbol_table = Some(scope);
                    let locals_size = self.c.leave_scope(true);
                    self.c.id_mut(func).func_mut().unwrap().locals_size = locals_size;
                    self.c.data_functions.push(func);
                    let sent = self.c.make_sentence(p.lineno(1), "RETURN", &[Some(func), Some(value)]);
                    let body = self.c.make_block(&[Some(sent)]);
                    self.c.set_body(func, Some(body));
                    datas.push(entry);
                }
                let label = label.unwrap_or_else(|| crash("AttributeError: 'NoneType' object has no attribute 'token'"));
                self.c.datas.push(DataRef { label, datas });
                self.c.data_ptr_current = self.c.current_data_label();
                V::None
            }
            Rule::p_restore => {
                let lbl = if p.len() == 2 {
                    None
                } else {
                    let label = p.v(2).clone();
                    self.label_of(&label, p.lineno(1))
                };
                self.sentence(p.lineno(1), "RESTORE", &[lbl])
            }
            Rule::p_read => {
                self.c.data_is_used = true;
                let Some(args) = p.node(2) else { return Ok(Some(V::None)) };
                let mut reads = Vec::new();
                for arg in self.c.children(args).to_vec().into_iter().flatten() {
                    let Some(entry) = self.c.child(arg, 0) else { return Ok(Some(V::None)) };
                    if self.c.token(entry) == "VARARRAY" {
                        let msg = format!("Cannot read '{}'. It's an array", self.c.name(entry));
                        self.c.error(p.lineno(1), &msg);
                        return Ok(Some(V::None));
                    }
                    if self.c.is_id(entry) {
                        if self.c.class_(entry) != Class::Var {
                            let name = self.c.name(entry).to_string();
                            self.c.syntax_error_cannot_assign_not_a_var(p.lineno(2), &name);
                            return Ok(Some(V::None));
                        }
                        self.c.mark_entry_as_accessed(entry);
                        if self.c.ty(entry).is_some_and(|t| t == BasicType::Unknown) {
                            self.c.set_ty(entry, Some(Ty::new(DEFAULT_TYPE)));
                            // The name given to the warning is the whole
                            // argument list, which prints as `(ARGUMENT)`.
                            let shown = self.c.str_of(args);
                            self.c.warning_implicit_type(p.lineno(2), &shown, Some(DEFAULT_TYPE.name()));
                        }
                        reads.push(Some(self.c.make_sentence(p.lineno(1), "READ", &[Some(entry)])));
                        continue;
                    }
                    if matches!(self.c.node(entry).kind, Kind::ArrayLoad(_)) {
                        let (array, arglist, lineno) = (self.c.entry(entry), self.c.child(entry, 1).unwrap(), self.c.lineno(entry));
                        let filename = self.c.filename();
                        let access = self.c.new_array_access(false, array, arglist, lineno, filename);
                        reads.push(Some(self.c.make_sentence(p.lineno(1), "READ", &[Some(access)])));
                        continue;
                    }
                    self.c.error(p.lineno(1), "Syntax error. Can only read a variable or an array element");
                    return Ok(Some(V::None));
                }
                V::Node(self.c.make_block(&reads))
            }
            Rule::p_do_while_loop | Rule::p_do_until_loop => {
                let r = p.node(1);
                let q = if matches!(p.v(2), V::Text(t) if t == "LOOP") { None } else { p.node(2) };
                let sentence = if rule == Rule::p_do_while_loop { self.sentence(p.lineno(1), "WHILE_DO", &[r, q]) } else { self.sentence(p.lineno(2), "UNTIL_DO", &[r, q]) };
                self.pop_loop();
                if self.is_number(r) {
                    let cond = self.truth(r.unwrap());
                    self.c.warning_condition_is_always(p.lineno(2), cond);
                }
                sentence
            }
            Rule::p_do_while_start | Rule::p_do_until_start => {
                self.push_loop(LoopType::Do, p.lineno(1), None);
                p.take(3)
            }
            Rule::p_do_start => {
                self.push_loop(LoopType::Do, p.lineno(1), None);
                V::None
            }
            Rule::p_while_sentence => {
                self.pop_loop();
                let q = self.c.make_block(&[p.node(2), p.node(3)]);
                let cond = p.node(1);
                if self.is_number(cond) {
                    let value = self.truth(cond.unwrap());
                    self.c.warning_condition_is_always(p.lineno(1), value);
                }
                self.sentence(p.lineno(1), "WHILE", &[cond, Some(q)])
            }
            Rule::p_while_start => {
                self.push_loop(LoopType::While, p.lineno(1), None);
                let cond = p.node(2);
                if self.is_number(cond) && !self.truth(cond.unwrap()) {
                    self.c.warning_condition_is_always(p.lineno(1), false);
                }
                p.take(2)
            }
            Rule::p_exit | Rule::p_continue => {
                let q = p.text(2).to_string();
                let word = if rule == Rule::p_exit { "EXIT" } else { "CONTINUE" };
                let sentence = self.sentence(p.lineno(1), &format!("{word}_{q}"), &[]);
                if !self.c.loops.iter().any(|l| l.kind.name() == q) {
                    self.c.error(p.lineno(1), &format!("Syntax Error: {word} {q} out of loop"));
                }
                sentence
            }
            Rule::p_print_sentence => {
                self.c.print_is_used = true;
                p.take(2)
            }
            Rule::p_print_elem_expr => {
                let expr = p.node(1);
                if expr.is_some_and(|e| self.c.ty(e).is_some_and(|t| t == BasicType::Boolean)) {
                    V::opt(self.cast(UByte, expr, p.lineno(1)))
                } else {
                    p.take(1)
                }
            }
            Rule::p_print_list_expr => {
                if let V::Text(word) = p.v(1) {
                    let keyword = format!("{word}_TMP");
                    let value = self.cast(UByte, p.node(2), p.lineno(1));
                    self.sentence(p.lineno(1), &keyword, &[value])
                } else {
                    p.take(1)
                }
            }
            Rule::p_attr_list => p.take(1),
            Rule::p_attr_list_list => V::Node(self.c.make_block(&[p.node(1), p.node(2)])),
            Rule::p_attr => {
                // Drawing commands take a list of these. BOLD and ITALIC
                // mean nothing to them and are left out, so that `DRAW
                // BOLD 1; ...` is a syntax error.
                let keyword = format!("{}_TMP", p.text(1));
                let value = self.cast(UByte, p.node(2), p.lineno(1));
                self.sentence(p.lineno(1), &keyword, &[value])
            }
            Rule::p_print_list_epsilon => V::None,
            Rule::p_print_list_elem => {
                let print = self.c.make_sentence(self.lexer.lineno(), "PRINT", &[p.node(1)]);
                self.c.node_mut(print).eol = true;
                V::Node(print)
            }
            Rule::p_print_list | Rule::p_print_list_comma => {
                let print = p.node(1).unwrap_or_else(|| crash("AttributeError: 'NoneType' object has no attribute 'eol'"));
                let elem = p.node(3);
                self.c.node_mut(print).eol = elem.is_some();
                if rule == Rule::p_print_list_comma {
                    let comma = self.c.make_sentence(p.lineno(2), "PRINT_COMMA", &[]);
                    self.c.append_child(print, Some(comma));
                }
                if elem.is_some() {
                    self.c.append_child(print, elem);
                }
                V::Node(print)
            }
            Rule::p_print_list_at => {
                let a = self.cast(UByte, p.node(2), p.lineno(1));
                let b = self.cast(UByte, p.node(4), p.lineno(3));
                self.sentence(p.lineno(1), "PRINT_AT", &[a, b])
            }
            Rule::p_print_list_tab => {
                let a = self.cast(UByte, p.node(2), p.lineno(1));
                self.sentence(p.lineno(1), "PRINT_TAB", &[a])
            }
            Rule::p_on_goto => {
                let expr = self.cast(UByte, p.node(2), p.lineno(1));
                let mut args = vec![expr];
                args.extend(p.v(4).list().iter().map(V::node));
                let keyword = format!("ON_{}", p.text(3));
                self.sentence(p.lineno(1), &keyword, &args)
            }
            Rule::p_label_list => {
                let label = p.v(1).clone();
                V::List(vec![V::opt(self.label_of(&label, p.lineno(1)))])
            }
            Rule::p_label_list_list => {
                let label = p.v(3).clone();
                let entry = self.label_of(&label, p.lineno(3));
                let mut list = p.take(1).into_list();
                list.push(V::opt(entry));
                V::List(list)
            }
            Rule::p_return => {
                // Outside a function this is the return of a GOSUB.
                let Some(&func) = self.c.function_level.last() else { return Ok(Some(self.sentence(p.lineno(1), "RETURN", &[]))) };
                if self.c.class_(func) != Class::Sub {
                    self.c.error(p.lineno(1), "Syntax Error: Function must RETURN a value.");
                    return Ok(Some(V::None));
                }
                self.sentence(p.lineno(1), "RETURN", &[Some(func)])
            }
            Rule::p_return_expr => {
                let Some(&func) = self.c.function_level.last() else {
                    self.c.error(p.lineno(1), "Syntax Error: Returning value out of FUNCTION");
                    return Ok(Some(V::None));
                };
                let class = self.c.class_(func);
                if class == Class::Unknown {
                    return Ok(Some(V::None)); // the function was not declared correctly
                }
                if class != Class::Function {
                    self.c.error(p.lineno(1), "Syntax Error: SUBs cannot return a value");
                    return Ok(Some(V::None));
                }
                // No type: there was an error in the function's declaration.
                let Some(func_ty) = self.c.ty(func) else { return Ok(Some(V::None)) };
                let expr = p.node(2);
                let numeric = expr.is_some_and(|e| self.c.is_numeric(&[e]));
                if numeric && func_ty == BasicType::String {
                    self.c.error(p.lineno(2), "Type Error: Function must return a string, not a numeric value");
                    return Ok(Some(V::None));
                }
                if !numeric && func_ty != BasicType::String {
                    self.c.error(p.lineno(2), "Type Error: Function must return a numeric value, not a string");
                    return Ok(Some(V::None));
                }
                let value = self.c.make_typecast(func_ty, expr, p.lineno(1));
                self.sentence(p.lineno(1), "RETURN", &[Some(func), value])
            }
            Rule::p_pause => {
                let a = self.cast(UInteger, p.node(2), p.lineno(1));
                self.sentence(p.lineno(1), "PAUSE", &[a])
            }
            Rule::p_poke => {
                // With the parentheses the second symbol is the `(` token.
                let i = if matches!(p.v(2), V::Text(_)) { 3 } else { 2 };
                if p.v(i).is_none() || p.v(i + 2).is_none() {
                    return Ok(Some(V::None));
                }
                let a = self.cast(UInteger, p.node(i), p.lineno(i + 1));
                let b = self.cast(UByte, p.node(i + 2), p.lineno(i + 1));
                self.sentence(p.lineno(1), "POKE", &[a, b])
            }
            Rule::p_poke2 => {
                let i = if matches!(p.v(2), V::Text(_)) { 3 } else { 2 };
                if p.v(i + 1).is_none() || p.v(i + 3).is_none() {
                    return Ok(Some(V::None));
                }
                let a = self.cast(UInteger, p.node(i + 1), p.lineno(i + 2));
                let b = self.cast_to(p.v(i).ty(), p.node(i + 3), p.lineno(i + 3));
                self.sentence(p.lineno(1), "POKE", &[a, b])
            }
            Rule::p_poke3 => {
                let i = if matches!(p.v(2), V::Text(_)) { 3 } else { 2 };
                if p.v(i + 2).is_none() || p.v(i + 4).is_none() {
                    return Ok(Some(V::None));
                }
                let a = self.cast(UInteger, p.node(i + 2), p.lineno(i + 3));
                // Without brackets the rule has six symbols and the Python
                // asks for the line of a seventh: `IndexError`.
                let b = self.cast_to(p.v(i).ty(), p.node(i + 4), p.lineno((i + 5).min(p.len() - 1)));
                self.sentence(p.lineno(1), "POKE", &[a, b])
            }
            Rule::p_out => {
                let a = self.cast(UInteger, p.node(2), p.lineno(3));
                let b = self.cast(UByte, p.node(4), p.lineno(4));
                self.sentence(p.lineno(1), "OUT", &[a, b])
            }
            Rule::p_simple_instruction => {
                let keyword = p.text(1).to_string();
                let a = self.cast(UByte, p.node(2), p.lineno(1));
                self.sentence(p.lineno(1), &keyword, &[a])
            }
            Rule::p_save_code => {
                let expr = p.node(2);
                if self.not_string(expr) {
                    let type_ = self.type_name(expr);
                    self.c.syntax_error_expected_string(p.lineno(1), &type_);
                }
                let (start, length) = if p.len() == 4 {
                    let id = p.text(3);
                    if !matches!(id.to_uppercase().as_str(), "SCREEN" | "SCREEN$") {
                        let msg = format!("Unexpected \"{id}\" ID. Expected \"SCREEN$\" instead");
                        self.c.error(p.lineno(3), &msg);
                        return Ok(Some(V::None));
                    }
                    // The ZX Spectrum screen: its start and its length.
                    (Some(self.number(16384, p.lineno(1))), Some(self.number(6912, p.lineno(1))))
                } else {
                    (self.cast(UInteger, p.node(4), p.lineno(4)), self.cast(UInteger, p.node(6), p.lineno(6)))
                };
                let keyword = p.text(1).to_string();
                self.sentence(p.lineno(1), &keyword, &[expr, start, length])
            }
            Rule::p_save_data | Rule::p_load_data => {
                let expr = p.node(2);
                if self.not_string(expr) {
                    let type_ = self.type_name(expr);
                    self.c.syntax_error_expected_string(p.lineno(1), &type_);
                }
                let address = Some(Ty::new(UInteger));
                let (start, length) = if p.len() != 4 {
                    let name = p.text(4).to_string();
                    let Some(entry) = self.c.access_id(&name, p.lineno(4), None, None, Class::Unknown, false) else { return Ok(Some(V::None)) };
                    self.c.mark_entry_as_accessed(entry);
                    let start = self.c.make_unary(p.lineno(4), "ADDRESS", Some(entry), None, address);
                    (start, Some(self.data_length(entry, p.lineno(4))))
                } else {
                    let global = self.c.global_scope();
                    let access = self.c.access_label(ZXBASIC_USER_DATA, p.lineno(3), Some(global));
                    let start = self.c.make_unary(p.lineno(3), "ADDRESS", access, None, address);
                    let access = self.c.access_label(ZXBASIC_USER_DATA_LEN, p.lineno(3), Some(global));
                    (start, self.c.make_unary(p.lineno(3), "ADDRESS", access, None, address))
                };
                let keyword = p.text(1).to_string();
                // SAVE is given the line of its keyword, LOAD and VERIFY
                // that of the DATA keyword.
                let lineno = if rule == Rule::p_save_data { p.lineno(1) } else { p.lineno(3) };
                self.sentence(lineno, &keyword, &[expr, start, length])
            }
            Rule::p_load_or_verify => p.take(1),
            Rule::p_load_code => {
                let expr = p.node(2);
                if self.not_string(expr) {
                    let type_ = self.type_name(expr);
                    self.c.syntax_error_expected_string(p.lineno(3), &type_);
                }
                let (start, length) = if p.len() == 4 {
                    let id = p.text(3);
                    let upper = id.to_uppercase();
                    if !matches!(upper.as_str(), "SCREEN" | "SCREEN$" | "CODE") {
                        let msg = format!("Unexpected \"{id}\" ID. Expected \"SCREEN$\" instead");
                        self.c.error(p.lineno(3), &msg);
                        return Ok(Some(V::None));
                    }
                    if upper == "CODE" {
                        // LOAD "..." CODE
                        (Some(self.number(0, p.lineno(3))), Some(self.number(0, p.lineno(3))))
                    } else {
                        // SCREEN$
                        (Some(self.number(16384, p.lineno(3))), Some(self.number(6912, p.lineno(3))))
                    }
                } else {
                    let start = self.cast(UInteger, p.node(4), p.lineno(3));
                    let length = if p.len() == 5 { Some(self.number(0, p.lineno(3))) } else { self.cast(UInteger, p.node(6), p.lineno(5)) };
                    (start, length)
                };
                let keyword = p.text(1).to_string();
                self.sentence(p.lineno(3), &keyword, &[expr, start, length])
            }
            _ => return Ok(None),
        }))
    }
}
