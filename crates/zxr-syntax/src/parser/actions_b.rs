//! Grammar actions: expressions, calls, function declarations, types, `#pragma` lines and the built-in functions (`p_numbertype` to `p_abs` in `zxbparser.py`).

use super::*;
use crate::options::{OptError, OptValue};
use crate::pyval::fold;

/// `p[0] = None; return`.
macro_rules! none {
    () => {
        return Ok(Some(V::None))
    };
}

fn convention_of(v: &V) -> Convention {
    match v.text() {
        "__fastcall__" => Convention::Fastcall,
        _ => Convention::Stdcall,
    }
}

/// An `Id(name, lineno)`, as `p_singleid` makes it.
fn id_of(v: &V) -> (String, u32) {
    match v {
        V::Id(name, lineno) => (name.clone(), *lineno),
        other => crash(&format!("AttributeError: {other:?} has no attribute 'name'")),
    }
}

/// The two ends of a `substr`: a tuple of two nodes (and sometimes a line
/// number, which nothing reads).
fn substr_of(v: &V) -> (Option<NodeId>, Option<NodeId>) {
    match v {
        V::List(items) if items.len() >= 2 => (items[0].node(), items[1].node()),
        _ => crash("TypeError: 'NoneType' object is not subscriptable"),
    }
}

impl Parser<'_> {
    /// `p[n].type_`, where `p[n]` being `None` raises.
    fn ty_or_crash(&self, node: Option<NodeId>) -> Option<Ty> {
        let node = node.unwrap_or_else(|| crash("AttributeError: 'NoneType' object has no attribute 'type_'"));
        self.c.ty(node)
    }

    /// `TYPE.to_string(type_)`.
    fn ty_name(ty: Option<Ty>) -> &'static str {
        ty.unwrap_or_else(|| crash("AssertionError: isinstance(t, SymbolTYPE)")).name()
    }

    fn is_string_type(ty: Option<Ty>) -> bool {
        ty.is_some_and(|t| t == BasicType::String)
    }

    /// `make_binary(p.lineno(2), operator, p[1], p[3], func)`.
    fn binary(&mut self, p: &P, operator: &str, func: Fold2) -> V {
        V::opt(self.c.make_binary(p.lineno(2), operator, p.node(1), p.node(3), Some(func), None))
    }

    /// `make_builtin(lineno, fname, operand, func, type_)` with what may be
    /// one operand: a `None` there makes a BUILTIN with no operands at all.
    fn builtin(&mut self, lineno: u32, fname: &str, operand: Option<NodeId>, func: Option<Fold1>, type_: Option<Ty>) -> V {
        let operands: Vec<NodeId> = operand.into_iter().collect();
        V::Node(self.c.make_builtin(lineno, fname, &operands, func, type_))
    }

    /// `_make_faraddress`: the FARADDRESS node behind `FARPTR x`. Its value
    /// is `(bank << 16) | address`; the bank is only settled after parsing,
    /// so the node survives to the translation. `operand` is the node the
    /// address hangs off when it is not the entry itself: an ARRAYACCESS.
    fn make_faraddress(&mut self, lineno: u32, entry: Option<NodeId>, operand: Option<NodeId>) -> Option<NodeId> {
        let entry = entry?;
        let c = &mut *self.c;
        if matches!(c.class_(entry), Class::Function | Class::Sub) {
            let msg = format!(
                "Cannot take FARPTR of '{}'. A banked SUB or FUNCTION is called through its resident trampoline, which is what its ordinary name already is",
                c.name(entry)
            );
            c.error(lineno, &msg);
            return None;
        }
        let data = c.id_mut(entry);
        data.r.has_faraddress = true;
        data.has_address = Some(true);
        c.mark_entry_as_accessed(entry);
        let result = c.make_unary(lineno, "FARADDRESS", Some(operand.unwrap_or(entry)), None, Some(Ty::new(BasicType::ULong)))?;
        Some(c.make_constexpr(lineno, result))
    }

    /// The `i` of the `LET ARRAY_ID ...` rules: where the array's name is.
    fn let_index(p: &P) -> usize {
        if matches!(p.v(1), V::Text(t) if t.to_uppercase() == "LET") { 2 } else { 1 }
    }

    /// `OPTIONS[...]` raised something no action catches.
    fn option_crash(error: OptError) -> ! {
        let class = match error {
            OptError::Undefined(_) => "UndefinedOptionError",
            OptError::InvalidValue { .. } => "InvalidValueError",
            OptError::StackUnderflow(_) => "OptionStackUnderflowError",
        };
        crash(&format!("{class}: {error}"))
    }

    /// The action of `rule`, if it is one of this file's.
    pub(super) fn reduce_b(&mut self, rule: Rule, p: &mut P) -> Result<Option<V>, SyntaxError> {
        // `p.lexer.lineno`.
        let lexer_lineno = self.lexer.lineno();
        Ok(Some(match rule {
            Rule::p_numbertype => {
                let name = p.text(1).to_lowercase();
                self.c.make_type(&name, p.lineno(1), false).map_or(V::None, V::Type)
            }
            Rule::p_expr_plus_expr => self.binary(p, "PLUS", fold::add),
            Rule::p_expr_minus_expr => self.binary(p, "MINUS", fold::sub),
            Rule::p_expr_mul_expr => self.binary(p, "MUL", fold::mul),
            Rule::p_expr_div_expr => self.binary(p, "DIV", fold::div),
            Rule::p_expr_mod_expr => self.binary(p, "MOD", fold::modulo),
            Rule::p_expr_pow_expr => {
                let float = Ty::new(BasicType::Float);
                let left = self.c.make_typecast(float, p.node(1), p.lineno(2));
                let right = self.c.make_typecast(float, p.node(3), lexer_lineno);
                V::opt(self.c.make_binary(p.lineno(2), "POW", left, right, Some(fold::pow), None))
            }
            Rule::p_expr_shl_expr | Rule::p_expr_shr_expr => {
                let (Some(left), Some(right)) = (p.node(1), p.node(3)) else { none!() };
                let (operator, func): (&str, Fold2) = if rule == Rule::p_expr_shl_expr { ("SHL", fold::shl) } else { ("SHR", fold::shr) };
                let lineno = p.lineno(2);
                let mut left_opt = Some(left);
                if self.c.ty(left).is_some_and(|t| matches!(t.basic, BasicType::Float | BasicType::Fixed)) {
                    left_opt = self.c.make_typecast(Ty::new(BasicType::ULong), Some(left), lineno);
                }
                let right = self.c.make_typecast(Ty::new(BasicType::UByte), Some(right), lineno);
                V::opt(self.c.make_binary(lineno, operator, left_opt, right, Some(func), None))
            }
            Rule::p_minus_expr => V::opt(self.c.make_unary(p.lineno(1), "MINUS", p.node(2), Some(fold::neg), None)),
            Rule::p_expr_EQ_expr => self.binary(p, "EQ", fold::eq),
            Rule::p_expr_LT_expr => self.binary(p, "LT", fold::lt),
            Rule::p_expr_LE_expr => self.binary(p, "LE", fold::le),
            Rule::p_expr_GT_expr => self.binary(p, "GT", fold::gt),
            Rule::p_expr_GE_expr => self.binary(p, "GE", fold::ge),
            Rule::p_expr_NE_expr => self.binary(p, "NE", fold::ne),
            Rule::p_expr_OR_expr => self.binary(p, "OR", fold::or),
            Rule::p_expr_BOR_expr => self.binary(p, "BOR", fold::bor),
            Rule::p_expr_XOR_expr => self.binary(p, "XOR", fold::xor),
            Rule::p_expr_BXOR_expr => self.binary(p, "BXOR", fold::bxor),
            Rule::p_expr_AND_expr => self.binary(p, "AND", fold::and),
            Rule::p_expr_BAND_expr => self.binary(p, "BAND", fold::band),
            Rule::p_NOT_expr => V::opt(self.c.make_unary(p.lineno(1), "NOT", p.node(2), Some(fold::not), None)),
            Rule::p_BNOT_expr => V::opt(self.c.make_unary(p.lineno(1), "BNOT", p.node(2), Some(fold::bnot), None)),
            Rule::p_lp_expr_rp => p.take(2),
            Rule::p_cast => {
                // make_typecast() looks at the node before the type.
                let (Some(node), lineno) = (p.node(5), p.lineno(6)) else { none!() };
                if self.c.ty(node).is_none() {
                    none!();
                }
                let type_ = p.v(3).ty().unwrap_or_else(|| crash("AssertionError: isinstance(type_, sym.TYPE)"));
                V::opt(self.c.make_typecast(type_, Some(node), lineno))
            }
            Rule::p_number_expr => {
                let value = match p.v(1) {
                    V::Float(f) => Num::Float(*f),
                    V::Int(i) => Num::Int(*i),
                    other => crash(&format!("AssertionError: isinstance(value, (float, int)): {other:?}")),
                };
                V::Node(self.c.make_number(value, p.lineno(1), None))
            }
            Rule::p_expr_PI => V::Node(self.c.make_number(Num::Float(std::f64::consts::PI), p.lineno(1), Some(Ty::new(BasicType::Float)))),
            Rule::p_expr_string | Rule::p_string_lprp | Rule::p_expr_bexpr | Rule::p_expr_funccall => p.take(1),
            Rule::p_string_func_call | Rule::p_string_substr => {
                let (lower, upper) = substr_of(p.v(2));
                V::opt(self.c.make_strslice(p.lineno(1), p.node(1), lower, upper))
            }
            Rule::p_string_func_call_single => V::opt(self.c.make_strslice(p.lineno(1), p.node(1), p.node(3), p.node(3))),
            Rule::p_string_str => V::Node(self.c.new_string(p.text(1), p.lineno(1))),
            Rule::p_string_lp_expr_rp => V::opt(self.c.make_strslice(p.lineno(2), p.node(1), p.node(3), p.node(3))),
            Rule::p_expr_id_substr => {
                let name = p.text(1).to_string();
                let lineno = p.lineno(1);
                let c = &mut *self.c;
                let entry = c.get_entry(&name, None, None);
                if let Some(entry) = entry {
                    if Self::is_string_type(c.ty(entry)) && c.token(entry) == "CONST" {
                        let (lower, upper) = substr_of(p.v(2));
                        return Ok(Some(V::opt(c.make_strslice(lineno, Some(entry), lower, upper))));
                    }
                }
                let Some(entry) = c.access_var(&name, lineno, None, Some(Ty::reference(BasicType::String, lineno, false))) else { none!() };
                c.mark_entry_as_accessed(entry);
                let (lower, upper) = substr_of(p.v(2));
                V::opt(c.make_strslice(lineno, Some(entry), lower, upper))
            }
            Rule::p_string_expr_lp => {
                let ty = self.ty_or_crash(p.node(2));
                if !Self::is_string_type(ty) {
                    let msg = format!("Expected a string type expression. Got {} type instead", Self::ty_name(ty));
                    self.c.error(lexer_lineno, &msg);
                    V::None
                } else {
                    let (lower, upper) = substr_of(p.v(4));
                    V::opt(self.c.make_strslice(lexer_lineno, p.node(2), lower, upper))
                }
            }
            Rule::p_subind_str => {
                let uinteger = Ty::new(BasicType::UInteger);
                let lower = self.c.make_typecast(uinteger, p.node(2), p.lineno(1));
                let upper = self.c.make_typecast(uinteger, p.node(4), p.lineno(3));
                V::List(vec![V::opt(lower), V::opt(upper)])
            }
            Rule::p_subind_strTO => {
                let uinteger = Ty::new(BasicType::UInteger);
                let zero = self.c.make_number(Num::Int(0), p.lineno(2), None);
                let lower = self.c.make_typecast(uinteger, Some(zero), p.lineno(1));
                let upper = self.c.make_typecast(uinteger, p.node(3), p.lineno(2));
                V::List(vec![V::opt(lower), V::opt(upper)])
            }
            Rule::p_subind_TOstr => {
                let uinteger = Ty::new(BasicType::UInteger);
                let lower = self.c.make_typecast(uinteger, p.node(2), p.lineno(1));
                let max = self.c.make_number(Num::Int(MAX_STRSLICE_IDX), p.lineno(4), None);
                let upper = self.c.make_typecast(uinteger, Some(max), p.lineno(4));
                // A tuple of three: the line of the TO rides along, unread.
                V::List(vec![V::opt(lower), V::opt(upper), V::Int(p.lineno(3) as i128)])
            }
            Rule::p_subind_TO => {
                let uinteger = Ty::new(BasicType::UInteger);
                let zero = self.c.make_number(Num::Int(0), p.lineno(2), None);
                let lower = self.c.make_typecast(uinteger, Some(zero), p.lineno(1));
                let max = self.c.make_number(Num::Int(MAX_STRSLICE_IDX), p.lineno(3), None);
                let upper = self.c.make_typecast(uinteger, Some(max), p.lineno(2));
                V::List(vec![V::opt(lower), V::opt(upper)])
            }
            Rule::p_id_expr => {
                let name = p.text(1).to_string();
                let lineno = p.lineno(1);
                let c = &mut *self.c;
                let Some(entry) = c.access_id(&name, lineno, None, None, Class::Var, false) else { none!() };
                c.mark_entry_as_accessed(entry);
                if c.ty(entry).is_some_and(|t| t == BasicType::Unknown) {
                    // The basic type itself, not a reference to it.
                    c.set_ty(entry, Some(Ty::new(DEFAULT_TYPE)));
                    c.warning_implicit_type(lineno, &name, Some(DEFAULT_TYPE.name()));
                }
                match c.class_(entry) {
                    // This should never happen now.
                    Class::Array => {
                        if !c.let_assignment {
                            c.error(lineno, &format!("Variable '{name}' is an array and cannot be used in this context"));
                            V::None
                        } else {
                            V::Node(entry)
                        }
                    }
                    // A function call with no arguments.
                    Class::Function => {
                        let args = c.make_arg_list(None, &[]);
                        V::opt(c.make_call(&name, lineno, Some(args)))
                    }
                    // Forbidden for subs.
                    Class::Sub => {
                        c.syntax_error_is_a_sub_not_a_func(lineno, &name);
                        V::None
                    }
                    _ => V::Node(entry),
                }
            }
            Rule::p_addr_of_id => {
                let (name, id_lineno) = id_of(p.v(2));
                let lineno = p.lineno(1);
                let c = &mut *self.c;
                // For the @ operator the explicit flag is ignored.
                let Some(entry) = c.access_id(&name, id_lineno, None, None, Class::Unknown, true) else { none!() };
                // `@arr` is the address of the descriptor since 1.18.3. Say
                // so, naming the replacement with the array's real arity.
                if c.class_(entry) == Class::Array && c.id(entry).scope == Scope::Global {
                    let dims = c.children(c.bounds(entry)).len();
                    let (entry_name, subscripts) = (c.id(entry).name.clone(), vec!["0"; dims].join(","));
                    c.warning_addressof_array(lineno, &entry_name, &subscripts, None);
                }
                c.id_mut(entry).has_address = Some(true);
                c.mark_entry_as_accessed(entry);
                c.note_address(entry, lineno);
                let result = c.make_unary(lineno, "ADDRESS", Some(entry), None, Some(Ty::new(PTR_TYPE))).unwrap();
                if c.is_dynamic(&[entry]) { V::Node(result) } else { V::Node(c.make_constexpr(lineno, result)) }
            }
            Rule::p_farptr_of_id => {
                let (name, id_lineno) = id_of(p.v(2));
                let lineno = p.lineno(1);
                // with SYMBOL_TABLE.far_access():
                let previous = std::mem::replace(&mut self.c.far_access, true);
                let entry = self.c.access_id(&name, id_lineno, None, None, Class::Unknown, true);
                self.c.far_access = previous;
                match entry {
                    Some(entry) if !self.c.is_dynamic(&[entry]) => V::opt(self.make_faraddress(lineno, Some(entry), None)),
                    Some(entry) => {
                        let msg = format!("Cannot take FARPTR of '{}'. Only a global has a fixed address", self.c.name(entry));
                        self.c.error(lineno, &msg);
                        V::None
                    }
                    None => V::None,
                }
            }
            Rule::p_farptr_of_array_element => {
                let Some(args) = p.node(3) else { none!() };
                let name = p.text(2).to_string();
                let previous = std::mem::replace(&mut self.c.far_access, true);
                let result = self.c.make_array_access(&name, p.lineno(2), args);
                self.c.far_access = previous;
                let Some(result) = result else { none!() };
                if self.c.array_offset(result).is_none() {
                    let msg = format!("Cannot take FARPTR of '{name}(...)' with a variable subscript. The address must be known at compile time");
                    self.c.error(p.lineno(1), &msg);
                    none!();
                }
                let entry = self.c.entry(result);
                V::opt(self.make_faraddress(p.lineno(1), Some(entry), Some(result)))
            }
            Rule::p_err_farptr_undefined_arr | Rule::p_err_undefined_arr_access => {
                let msg = format!("Undeclared array \"{}\"", p.text(2));
                self.c.error(p.lineno(2), &msg);
                V::None
            }
            Rule::p_idcall_expr => {
                // This can be a function call or a string index.
                let Some(args) = p.node(2) else { none!() };
                let name = p.text(1).to_string();
                let lineno = p.lineno(1);
                let c = &mut *self.c;
                let Some(result) = c.make_call(&name, lineno, Some(args)) else { none!() };
                if matches!(c.token(result), "STRSLICE" | "ID" | "STRING") || c.token(result) == "CONST" && Self::is_string_type(c.ty(result)) {
                    let entry = c.access_call(&name, lineno, None, None).unwrap_or_else(|| crash("AssertionError: isinstance(entry, sym.ID)"));
                    c.mark_entry_as_accessed(entry);
                    return Ok(Some(V::Node(result)));
                }
                // A string variable with no arguments comes back as the VAR
                // itself, which has no `entry`: the Python compiler dies here.
                let entry = c.entry(result);
                c.convert_to_function(entry, Class::Function, lineno);
                let entry = c.entry(result);
                c.mark_entry_as_accessed(entry);
                V::Node(result)
            }
            Rule::p_array_eq_error => {
                let msg = format!("Invalid assignment. Variable {}() is an array", p.text(2));
                self.c.error(p.lineno(4), &msg);
                V::None
            }
            Rule::p_arr_access_expr => {
                // This is an array access.
                let name = p.text(1).to_string();
                let lineno = p.lineno(1);
                let c = &mut *self.c;
                let Some(result) = c.make_call(&name, lineno, p.node(2)) else { none!() };
                let entry = c.access_call(&name, lineno, None, None).unwrap_or_else(|| crash("AssertionError: isinstance(entry, sym.ID)"));
                c.mark_entry_as_accessed(entry);
                V::Node(result)
            }
            Rule::p_let_arr_substr => {
                let i = Self::let_index(p);
                let name = p.text(i).to_string();
                let substr = if p.v(i + 2).is_none() { None } else { Some(substr_of(p.v(i + 2))) };
                V::opt(self.c.make_array_substr_assign(p.lineno(i), &name, p.node(i + 1), substr, p.node(i + 4)))
            }
            Rule::p_let_arr_substr_single => {
                let i = Self::let_index(p);
                let name = p.text(i).to_string();
                let substr = Some((p.node(i + 3), p.node(i + 3)));
                V::opt(self.c.make_array_substr_assign(p.lineno(i), &name, p.node(i + 1), substr, p.node(i + 6)))
            }
            Rule::p_let_arr_substr_in_args | Rule::p_let_arr_substr_in_args4 => {
                let i = Self::let_index(p);
                let name = p.text(i).to_string();
                let arg_list = p.node(i + 2).unwrap_or_else(|| crash("AttributeError: 'NoneType' object has no attribute 'children'"));
                // The last argument is the start of the slice.
                let count = self.c.children(arg_list).len();
                if count == 0 {
                    crash("IndexError: pop from empty list");
                }
                let last = self.c.pop_child(arg_list, count - 1).unwrap_or_else(|| crash("AttributeError: 'NoneType' object has no attribute 'value'"));
                let lower = self.c.arg_value(last);
                let (upper, expr) = if rule == Rule::p_let_arr_substr_in_args {
                    (Some(self.c.make_number(Num::Int(MAX_STRSLICE_IDX), p.lineno(i + 3), None)), p.node(i + 6))
                } else {
                    (p.node(i + 4), p.node(i + 7))
                };
                V::opt(self.c.make_array_substr_assign(p.lineno(i), &name, Some(arg_list), Some((lower, upper)), expr))
            }
            Rule::p_let_arr_substr_in_args2 => {
                let i = Self::let_index(p);
                let name = p.text(i).to_string();
                let zero = self.c.make_number(Num::Int(0), p.lineno(i + 4), None);
                let substr = Some((Some(zero), p.node(i + 5)));
                V::opt(self.c.make_array_substr_assign(p.lineno(i), &name, p.node(i + 2), substr, p.node(i + 8)))
            }
            Rule::p_let_arr_substr_in_args3 => {
                let i = Self::let_index(p);
                let name = p.text(i).to_string();
                let zero = self.c.make_number(Num::Int(0), p.lineno(i + 4), None);
                let max = self.c.make_number(Num::Int(MAX_STRSLICE_IDX), p.lineno(i + 3), None);
                let substr = Some((Some(zero), Some(max)));
                V::opt(self.c.make_array_substr_assign(p.lineno(i), &name, p.node(i + 2), substr, p.node(i + 7)))
            }
            Rule::p_addr_of_array_element => {
                let Some(args) = p.node(3) else { none!() };
                let name = p.text(2).to_string();
                let c = &mut *self.c;
                let Some(result) = c.make_array_access(&name, p.lineno(2), args) else { none!() };
                let entry = c.entry(result);
                c.mark_entry_as_accessed(entry);
                V::opt(c.make_unary(p.lineno(1), "ADDRESS", Some(result), None, Some(Ty::new(PTR_TYPE))))
            }
            Rule::p_bexpr_func => {
                let name = p.text(1).to_string();
                let lineno = p.lineno(1);
                let c = &mut *self.c;
                let argument = c.make_argument(p.node(2), p.lineno(2), None, None);
                let args = c.make_arg_list(argument, &[]);
                let Some(result) = c.make_call(&name, lineno, Some(args)) else { none!() };
                if matches!(c.token(result), "STRSLICE" | "VAR" | "STRING") {
                    let entry = c.access_call(&name, lineno, None, None).unwrap_or_else(|| crash("AssertionError: isinstance(entry, sym.ID)"));
                    c.mark_entry_as_accessed(entry);
                    return Ok(Some(V::Node(result)));
                }
                let entry = c.entry(result);
                c.convert_to_function(entry, Class::Function, lineno);
                let entry = c.entry(result);
                c.mark_entry_as_accessed(entry);
                V::Node(result)
            }
            Rule::p_arg_list => V::Node(self.c.make_arg_list(None, &[])),
            Rule::p_arg_list_arg => p.take(2),
            Rule::p_arguments => match p.node(1) {
                None => V::None,
                Some(arg) => V::Node(self.c.make_arg_list(Some(arg), &[])),
            },
            Rule::p_arguments_argument => match (p.node(1), p.node(3)) {
                (Some(list), Some(arg)) => V::Node(self.c.make_arg_list(Some(list), &[arg])),
                _ => V::None,
            },
            Rule::p_argument => V::opt(self.c.make_argument(p.node(1), p.lineno(1), None, None)),
            Rule::p_named_argument => {
                let name = p.text(1).to_string();
                V::opt(self.c.make_argument(p.node(3), p.lineno(1), None, Some(name)))
            }
            Rule::p_argument_array => {
                let name = p.text(1).to_string();
                let lineno = p.lineno(1);
                let Some(entry) = self.c.access_array(&name, lineno, None, None) else { none!() };
                self.c.mark_entry_as_accessed(entry);
                V::opt(self.c.make_argument(Some(entry), lineno, None, None))
            }
            Rule::p_funcdecl => {
                let Some(decl) = p.node(1) else { none!() };
                let c = &mut *self.c;
                let entry = c.entry(decl);
                let scope = c.current_scope();
                c.id_mut(entry).func_mut().unwrap().local_symbol_table = Some(scope);
                // Read now: leaving the scope reports the unused ones and
                // marks them.
                let used_param = c.children(c.params(entry)).iter().flatten().copied().find(|&param| c.accessed(param)).map(|param| c.id(param).name.clone());
                let mut idle_locals: Vec<(u32, String)> = c
                    .scope_values(c.current_scope(), false)
                    .into_iter()
                    .filter(|&v| matches!(c.class_(v), Class::Var | Class::Array) && c.id(v).scope == Scope::Local && !c.accessed(v))
                    .map(|v| (c.lineno(v), c.id(v).name.clone()))
                    .collect();
                idle_locals.sort();
                let locals_size = c.leave_scope(true);
                c.id_mut(entry).func_mut().unwrap().locals_size = locals_size;
                c.function_level.pop();
                let body = p.node(2);
                if body.is_some_and(|b| !matches!(c.node(b).kind, Kind::Block | Kind::Nop)) {
                    crash("AssertionError: isinstance(value, SymbolBLOCK)");
                }
                let entry = c.entry(decl);
                if c.id(entry).func().is_some_and(|f| f.convention == Convention::Fastcall) {
                    let what = match used_param {
                        Some(name) => Some(format!("parameter '{name}'")),
                        None if locals_size != 0 => Some("local variables".to_string()),
                        None => None,
                    };
                    if let Some(what) = what {
                        let kind = if c.class_(entry) == Class::Sub { "SUB" } else { "FUNCTION" };
                        let name = c.id(entry).name.clone();
                        c.warning_fastcall_without_frame(c.lineno(decl), kind, &name, &what);
                    }
                }
                // Unused locals are removed above -O1 and the rest move up:
                // asm that reaches them through IX then reaches others.
                if !idle_locals.is_empty() && body.is_some_and(|b| c.asm_addresses_locals(b)) {
                    let name = c.id(entry).name.clone();
                    for (lineno, local) in idle_locals {
                        c.warning_local_under_asm(lineno, &local, &name);
                    }
                }
                // A DECLARE may stand outside the bank; the definition may not.
                if c.id(entry).func().is_some_and(|f| f.private && f.bank == 0) {
                    let msg = format!("'{}' cannot be PRIVATE: it is not in a CODEBANK", c.name(decl));
                    c.error(c.lineno(decl), &msg);
                }
                c.set_body(entry, body);
                c.scope_data_mut(scope).owner = Some(decl);
                c.id_mut(entry).func_mut().unwrap().forwarded = false;
                V::Node(decl)
            }
            Rule::p_funcdeclforward => {
                let c = &mut *self.c;
                let Some(decl) = p.node(2) else {
                    c.function_level.pop();
                    none!()
                };
                let entry = c.entry(decl);
                if c.id(entry).func().is_some_and(|f| f.forwarded) {
                    let msg = format!("duplicated declaration for function '{}'", c.name(decl));
                    c.error(p.lineno(1), &msg);
                }
                c.id_mut(entry).func_mut().unwrap().forwarded = true;
                c.leave_scope(false);
                if c.function_level.pop().is_none() {
                    crash("IndexError: pop from empty list");
                }
                V::None
            }
            Rule::p_function_header => p.take(1),
            Rule::p_function_header_error | Rule::p_param_decl_errpr => V::None,
            Rule::p_function_header_pre => {
                let (Some(decl), Some(params)) = (p.node(1), p.node(2)) else { none!() };
                let c = &mut *self.c;
                let entry = c.entry(decl);
                let forwarded = c.id(entry).func().is_some_and(|f| f.forwarded);
                c.append_child(decl, Some(params));
                let Kind::ParamList { size } = c.node(params).kind else { crash("AttributeError: object has no attribute 'size'") };
                c.node_mut(decl).params_size = Some(size);
                let lineno = p.lineno(3);
                let previoustype_ = c.ty(decl);
                let typedef = p.v(3).ty().unwrap_or_else(|| crash("AttributeError: 'NoneType' object has no attribute 'implicit'"));
                if !typedef.implicit || c.ty(entry).is_none_or(|t| t == BasicType::Unknown) {
                    c.set_ty(entry, Some(typedef));
                    if typedef.implicit && c.class_(entry) == Class::Function {
                        let name = c.id(entry).name.clone();
                        c.warning_implicit_type(typedef.lineno, &name, Some(typedef.name()));
                    }
                }
                if forwarded && previoustype_ != c.ty(decl) {
                    c.syntax_error_func_type_mismatch(lineno, entry);
                    none!();
                }
                let new_params: Vec<Option<NodeId>> = c.children(params).to_vec();
                if forwarded {
                    // Declared in advance: the parameters must be the same.
                    let old_params: Vec<Option<NodeId>> = c.children(c.params(entry)).to_vec();
                    if old_params.len() != new_params.len() {
                        c.syntax_error_parameter_mismatch(lineno, entry);
                        none!();
                    }
                    for (a, b) in old_params.into_iter().zip(new_params.iter().copied()) {
                        let (a, b) = (a.unwrap(), b.unwrap());
                        if c.id(a).name != c.id(b).name {
                            let msg = format!("Parameter '{}' in function '{}' has been renamed to '{}'", c.id(a).name, c.name(decl), c.id(b).name);
                            c.warning(lineno, &msg);
                        }
                        let byref = |c: &Compiler, x: NodeId| c.id(x).var().is_some_and(|v| v.byref);
                        if c.ty(a) != c.ty(b) || byref(c, a) != byref(c, b) {
                            c.syntax_error_parameter_mismatch(lineno, entry);
                            none!();
                        }
                    }
                }
                c.set_child(entry, 0, Some(params));
                let level = *c.function_level.last().unwrap_or_else(|| crash("IndexError: list index out of range"));
                let level_class = c.class_(level);
                if level_class == Class::Sub && !typedef.implicit {
                    c.error(lineno, "SUBs cannot have a return type definition");
                    none!();
                }
                if level_class == Class::Function {
                    let name = c.id(entry).name.clone();
                    c.check_type_is_explicit(c.lineno(decl), &name, typedef);
                }
                if c.id(entry).func().is_some_and(|f| f.convention == Convention::Fastcall) && new_params.len() > 1 {
                    let class_ = if level_class == Class::Sub { "SUB" } else { "FUNCTION" };
                    let name = c.id(entry).name.clone();
                    c.warning_fastcall_with_n_parameters(lineno, class_, &name, new_params.len());
                }
                V::Node(decl)
            }
            Rule::p_function_error => {
                self.c.error(p.lineno(3), "Unexpected token 'END'. Expected 'END FUNCTION' or 'END SUB' instead.");
                V::None
            }
            Rule::p_function_def => {
                let convention = convention_of(p.v(2));
                let name = p.text(3).to_string();
                // Must be 'function' or 'sub'.
                let class_ = if p.text(1) == "SUB" { Class::Sub } else { Class::Function };
                let c = &mut *self.c;
                let private = c.pending_private.take();
                let decl = c.make_func_declaration(&name, p.lineno(3), class_, None);
                c.enter_scope(&name);
                let entry = c.get_entry(&name, None, None).unwrap_or_else(|| crash("AttributeError: 'NoneType' object has no attribute 'class_'"));
                c.function_level.push(entry);
                if matches!(c.class_(entry), Class::Function | Class::Sub) {
                    // Was correctly declared. ZX Next: the routine is bound
                    // to the code bank in effect at its header; a `#pragma
                    // codebank` inside the body cannot alter it.
                    let bank = c.opts.current_codebank();
                    let func = c.id_mut(entry).func_mut().unwrap();
                    let declared = func.forwarded.then_some(func.private);
                    func.convention = convention;
                    func.bank = bank;
                    func.private = private.is_some();
                    if declared.is_some_and(|d| d != private.is_some()) {
                        let (was, is) = if private.is_some() { ("without", "with") } else { ("with", "without") };
                        c.error(p.lineno(3), &format!("'{name}' was declared {was} PRIVATE and is defined {is} it"));
                    }
                }
                V::opt(decl)
            }
            Rule::p_convention => V::Text(Convention::Stdcall.name().to_string()),
            Rule::p_convention2 => V::Text(Convention::Fastcall.name().to_string()),
            Rule::p_param_decl_none => V::Node(self.c.make_param_list(None, &[])),
            Rule::p_param_decl => p.take(2),
            Rule::p_param_decl_list => V::Node(self.c.make_param_list(p.node(1), &[])),
            Rule::p_param_decl_list2 => {
                let c = &mut *self.c;
                let default_value = |c: &Compiler, param: NodeId| c.id(param).var().and_then(|v| v.default_value);
                if let (Some(list), Some(param)) = (p.node(1), p.node(3)) {
                    // No errors in parsing.
                    let last = c.children(list).last().copied().flatten().unwrap_or_else(|| crash("IndexError: list index out of range"));
                    if default_value(c, param).is_none() && default_value(c, last).is_some() {
                        let (last_name, name) = (c.id(last).name.clone(), c.id(param).name.clone());
                        c.syntax_error_mandatory_param_after_optional(c.lineno(param), &last_name, &name);
                    }
                }
                V::Node(c.make_param_list(p.node(1), &[p.node(3)]))
            }
            Rule::p_param_byref_definition => {
                if let Some(param) = p.node(2) {
                    self.c.id_mut(param).var_mut().unwrap().byref = true;
                }
                p.take(2)
            }
            Rule::p_param_byval_definition => {
                let Some(param) = p.node(2) else { none!() };
                if self.c.class_(param) == Class::Array {
                    let name = self.c.id(param).name.clone();
                    self.c.syntax_error_cannot_pass_array_by_value(p.lineno(1), &name);
                    none!();
                }
                self.c.id_mut(param).var_mut().unwrap().byref = false;
                V::Node(param)
            }
            Rule::p_param_definition => {
                let Some(param) = p.node(1) else { none!() };
                let byref = self.c.class_(param) == Class::Array || self.c.opts.bool("default_byref");
                self.c.id_mut(param).var_mut().unwrap().byref = byref;
                V::Node(param)
            }
            Rule::p_param_def_array => {
                let Some(typeref) = p.v(4).ty() else { none!() };
                let (name, lineno) = id_of(p.v(1));
                self.c.check_type_is_explicit(lineno, &name, typeref);
                V::opt(self.c.make_param_decl(&name, lineno, Some(typeref), true, None))
            }
            Rule::p_param_def_type => {
                let (name, lineno) = id_of(p.v(1));
                let typedef = p.v(2).ty();
                if let Some(typedef) = typedef {
                    self.c.check_type_is_explicit(lineno, &name, typedef);
                }
                let default_value = match p.node(3) {
                    Some(value) if self.c.ty(value).is_some() => {
                        let typedef = typedef.unwrap_or_else(|| crash("AssertionError: isinstance(type_, sym.TYPE)"));
                        self.c.make_typecast(typedef, Some(value), lineno)
                    }
                    _ => None,
                };
                V::opt(self.c.make_param_decl(&name, lineno, typedef, false, default_value))
            }
            Rule::p_param_def_default_arg_value => {
                if p.len() == 1 {
                    none!();
                }
                p.take(2)
            }
            Rule::p_function_body => {
                let c = &mut *self.c;
                let Some(&level) = c.function_level.last() else {
                    if p.len() < 4 {
                        crash("IndexError: list index out of range");
                    }
                    let msg = format!("Unexpected token 'END {}'. No Function or Sub has been defined.", p.v(2).python_str());
                    c.error(p.lineno(3), &msg);
                    none!()
                };
                let a = c.class_(level);
                if !matches!(a, Class::Sub | Class::Function) {
                    // This function or sub was not correctly declared.
                    none!();
                }
                let only_end = matches!(p.v(1), V::Text(t) if t == "END");
                let i = if only_end { 2 } else { 3 };
                let b = p.text(i).to_lowercase();
                if a.name() != b {
                    let msg = format!("Unexpected token 'END {}'. Should be 'END {}'", b.to_uppercase(), a.name().to_uppercase());
                    c.error(p.lineno(i), &msg);
                    V::None
                } else if only_end {
                    V::Node(c.make_block(&[]))
                } else {
                    p.take(1)
                }
            }
            Rule::p_type_def_empty => {
                // Epsilon. Defaults to float.
                self.c.make_type(DEFAULT_TYPE.name(), lexer_lineno, true).map_or(V::None, V::Type)
            }
            Rule::p_type_def => {
                let name = p.text(2).to_string();
                self.c.make_type(&name, p.lineno(2), false).map_or(V::None, V::Type)
            }
            Rule::p_type => V::Text(p.text(1).to_lowercase()),

            // Preprocessor lines: #init, #require and #pragma are handled
            // here and not in the lexer.
            Rule::p_preproc_line_init => {
                let name = p.text(2).to_string();
                if !self.c.inits.contains(&name) {
                    self.c.inits.push(name);
                }
                V::None
            }
            Rule::p_preproc_line_require => {
                self.c.requires.insert(p.text(2).to_string());
                V::None
            }
            Rule::p_preproc_line_pragma_option => {
                // The value is text whichever token it came as; the option
                // converts it to its own type. A value it cannot take raises
                // InvalidValueError, which nothing catches.
                let (name, value) = (p.text(2).to_string(), p.v(4).python_str());
                match self.c.set_option(&name, OptValue::Str(value)) {
                    Ok(()) => {}
                    Err(OptError::Undefined(_)) => self.c.warning_ignoring_unknown_pragma(p.lineno(2), &name),
                    Err(error) => Self::option_crash(error),
                }
                V::None
            }
            Rule::p_codebank_begin => {
                // ZX Next: every SUB or FUNCTION declared from here on goes
                // into code bank N, until the matching END CODEBANK.
                let c = &mut *self.c;
                if !c.function_level.is_empty() {
                    c.error(p.lineno(1), "CODEBANK cannot be used inside a SUB or FUNCTION");
                    none!();
                }
                let bank = match p.v(2) {
                    V::Float(f) => Num::Float(*f),
                    V::Int(i) => Num::Int(*i),
                    // `CODEBANK INTEGER`: the keyword, of which int() makes nothing.
                    other => crash(&format!("ValueError: invalid literal for int() with base 10: '{}'", other.python_str())),
                };
                let whole = match bank {
                    Num::Int(i) => Some(i),
                    Num::Float(f) if f.is_finite() && f.fract() == 0.0 => Some(f as i128),
                    Num::Float(f) if !f.is_finite() => crash("OverflowError: cannot convert float infinity to integer"),
                    Num::Float(_) => None,
                };
                match whole {
                    Some(n) if n >= 0 => {
                        if let Err(error) = c.opts.push("codebank", Some(OptValue::Int(n))) {
                            Self::option_crash(error);
                        }
                        c.sync_session_options();
                    }
                    _ => {
                        let msg = format!("Invalid CODEBANK number '{bank}'. Must be a whole number, 0 or greater");
                        c.error(p.lineno(1), &msg);
                    }
                }
                V::None
            }
            Rule::p_codebank_end => {
                match self.c.opts.pop("codebank") {
                    Ok(_) => {}
                    Err(OptError::StackUnderflow(_)) => self.c.error(p.lineno(1), "END CODEBANK without a matching CODEBANK"),
                    Err(error) => Self::option_crash(error),
                }
                self.c.sync_session_options();
                V::None
            }
            Rule::p_preproc_pragma_push | Rule::p_preproc_pragma_pop => {
                let name = p.text(4).to_string();
                let result = if rule == Rule::p_preproc_pragma_push { self.c.opts.push(&name, None) } else { self.c.opts.pop(&name).map(|_| ()) };
                match result {
                    Ok(()) => {}
                    Err(OptError::Undefined(_)) => self.c.warning_ignoring_unknown_pragma(p.lineno(4), &name),
                    Err(error) => Self::option_crash(error),
                }
                self.c.sync_session_options();
                V::None
            }

            // Internal BASIC functions. These are implemented in the
            // translator as a call to an asm routine.
            Rule::p_expr_usr => {
                let lineno = p.lineno(1);
                let uinteger = Ty::new(BasicType::UInteger);
                if Self::is_string_type(self.ty_or_crash(p.node(2))) {
                    self.builtin(lineno, "USR_STR", p.node(2), None, Some(uinteger))
                } else {
                    let operand = self.c.make_typecast(uinteger, p.node(2), lineno);
                    self.builtin(lineno, "USR", operand, None, Some(uinteger))
                }
            }
            Rule::p_expr_rnd => self.builtin(p.lineno(1), "RND", None, None, Some(Ty::new(BasicType::Float))),
            Rule::p_expr_peek | Rule::p_expr_in => {
                let lineno = p.lineno(1);
                let fname = if rule == Rule::p_expr_peek { "PEEK" } else { "IN" };
                let operand = self.c.make_typecast(Ty::new(BasicType::UInteger), p.node(2), lineno);
                self.builtin(lineno, fname, operand, None, Some(Ty::new(BasicType::UByte)))
            }
            Rule::p_expr_peektype_ => {
                let operand = self.c.make_typecast(Ty::new(BasicType::UInteger), p.node(5), p.lineno(4));
                self.builtin(p.lineno(1), "PEEK", operand, None, p.v(3).ty())
            }
            Rule::p_expr_lbound => {
                let name = p.text(3).to_string();
                let c = &mut *self.c;
                let Some(entry) = c.access_array(&name, p.lineno(3), None, None) else { none!() };
                c.mark_entry_as_accessed(entry);
                let uinteger = Ty::new(BasicType::UInteger);
                if c.id(entry).scope == Scope::Parameter {
                    let num = c.make_number(Num::Int(0), p.lineno(3), Some(uinteger));
                    let fname = p.text(1).to_string();
                    V::Node(c.make_builtin(p.lineno(1), &fname, &[entry, num], None, Some(uinteger)))
                } else {
                    let dims = c.children(c.bounds(entry)).len();
                    V::Node(c.make_number(Num::Int(dims as i128), p.lineno(3), Some(uinteger)))
                }
            }
            Rule::p_expr_lbound_expr => {
                let Some(expr) = p.node(5) else { none!() };
                let name = p.text(3).to_string();
                let fname = p.text(1).to_string();
                let c = &mut *self.c;
                let Some(entry) = c.access_array(&name, p.lineno(3), None, None) else { none!() };
                c.mark_entry_as_accessed(entry);
                let uinteger = Ty::new(BasicType::UInteger);
                let Some(num) = c.make_typecast(uinteger, Some(expr), p.lineno(6)) else { none!() };
                if c.is_number(&[num]) && matches!(c.id(entry).scope, Scope::Local | Scope::Global) {
                    // Try constant propagation.
                    let val = c.num_value(num);
                    let bounds: Vec<NodeId> = c.children(c.bounds(entry)).iter().flatten().copied().collect();
                    if val < Num::Int(0) || val > Num::Int(bounds.len() as i128) {
                        c.error(p.lineno(6), "Dimension out of range");
                        none!();
                    }
                    let value = if val.is_zero() {
                        bounds.len() as i128 // 0 => the number of dimensions
                    } else {
                        let Num::Int(dim) = val else { crash("TypeError: list indices must be integers or slices, not float") };
                        let Kind::Bound { lower, upper } = c.node(bounds[dim as usize - 1]).kind else { unreachable!() };
                        if fname == "LBOUND" { lower } else { upper }
                    };
                    return Ok(Some(V::Node(c.make_number(Num::Int(value), p.lineno(3), Some(uinteger)))));
                }
                let var = c.id_mut(entry).var_mut().unwrap();
                if fname == "LBOUND" {
                    var.lbound_used = true;
                } else {
                    var.ubound_used = true;
                }
                V::Node(c.make_builtin(p.lineno(1), &fname, &[entry, num], None, Some(uinteger)))
            }
            Rule::p_len => {
                let Some(arg) = p.node(2) else { none!() };
                let lineno = p.lineno(1);
                // (The Python code first asks whether the argument is a VAR
                // of class array, which nothing is.)
                let ty = self.c.ty(arg);
                if !Self::is_string_type(ty) {
                    self.c.syntax_error_expected_string(lineno, Self::ty_name(ty));
                    V::None
                } else if self.c.is_string(&[arg]) {
                    // A constant string: fold it.
                    let value = self.c.value(arg);
                    let len = value.as_str().chars().count() as i128;
                    V::Node(self.c.make_number(Num::Int(len), lineno, None))
                } else {
                    self.builtin(lineno, "LEN", Some(arg), None, Some(Ty::new(BasicType::UInteger)))
                }
            }
            Rule::p_sizeof => {
                // `TYPE` in the Python module is the class of the symbol
                // types, which has no `to_type`: every SIZEOF ends here.
                crash("AttributeError: type object 'Type' has no attribute 'to_type'")
            }
            Rule::p_str => {
                let lineno = p.lineno(1);
                match p.node(2) {
                    Some(arg) if self.c.is_number(&[arg]) => {
                        // A constant is converted to a string directly.
                        let text = match self.c.value(arg) {
                            Val::Num(n) => n.to_string(),
                            Val::Str(s) => s,
                        };
                        V::Node(self.c.new_string(&text, lineno))
                    }
                    arg => {
                        let operand = self.c.make_typecast(Ty::new(BasicType::Float), arg, lineno);
                        self.builtin(lineno, "STR", operand, None, Some(Ty::new(BasicType::String)))
                    }
                }
            }
            Rule::p_inkey => self.builtin(p.lineno(1), "INKEY", None, None, Some(Ty::new(BasicType::String))),
            Rule::p_chr_one => {
                let lineno = p.lineno(1);
                let c = &mut *self.c;
                let argument = c.make_argument(p.node(2), lineno, None, None);
                let arg_list = c.make_arg_list(argument, &[]);
                let first = c.child(arg_list, 0).unwrap_or_else(|| crash("IndexError: list index out of range"));
                let value = c.make_typecast(Ty::new(BasicType::UByte), c.arg_value(first), lineno);
                c.set_child(first, 0, value);
                V::Node(c.make_builtin(lineno, "CHR", &[arg_list], None, Some(Ty::new(BasicType::String))))
            }
            Rule::p_chr => {
                let lineno = p.lineno(1);
                let c = &mut *self.c;
                let arg_list = p.node(2).unwrap_or_else(|| crash("TypeError: object of type 'NoneType' has no len()"));
                let args: Vec<Option<NodeId>> = c.children(arg_list).to_vec();
                if args.is_empty() {
                    c.error(lineno, "CHR$ function need at less 1 parameter");
                    none!();
                }
                // Every argument becomes an unsigned byte.
                for arg in args {
                    let arg = arg.unwrap();
                    let value = c.make_typecast(Ty::new(BasicType::UByte), c.arg_value(arg), lineno);
                    c.set_child(arg, 0, value);
                }
                V::Node(c.make_builtin(lineno, "CHR", &[arg_list], None, Some(Ty::new(BasicType::String))))
            }
            Rule::p_val => {
                let lineno = p.lineno(1);
                let ty = self.ty_or_crash(p.node(2));
                let arg = p.node(2).unwrap();
                if !Self::is_string_type(ty) {
                    self.c.syntax_error_expected_string(lineno, Self::ty_name(ty));
                    V::None
                } else if self.c.is_string(&[arg]) {
                    // make_builtin() folds a constant with `val`, which
                    // evaluates the text as a Python expression; one that
                    // fails is 0, with a warning.
                    let text = match self.c.value(arg) {
                        Val::Str(s) => s,
                        Val::Num(_) => crash("TypeError: eval() arg 1 must be a string, bytes or code object"),
                    };
                    let x = match pyval::eval_to_float(&text) {
                        Some(x) => Num::Float(x),
                        None => {
                            self.c.warning(lineno, &format!("Invalid string numeric constant '{text}' evaluated as 0"));
                            Num::Int(0)
                        }
                    };
                    V::Node(self.c.new_number(x, lineno, Some(Ty::new(BasicType::Float))))
                } else {
                    self.builtin(lineno, "VAL", Some(arg), None, Some(Ty::new(BasicType::Float)))
                }
            }
            Rule::p_code => {
                let Some(arg) = p.node(2) else { none!() };
                let lineno = p.lineno(1);
                let ty = self.c.ty(arg);
                if !Self::is_string_type(ty) {
                    self.c.syntax_error_expected_string(lineno, Self::ty_name(ty));
                    V::None
                } else {
                    self.builtin(lineno, "CODE", Some(arg), Some(fold::asc), Some(Ty::new(BasicType::UByte)))
                }
            }
            Rule::p_sgn => {
                let lineno = p.lineno(1);
                if Self::is_string_type(self.ty_or_crash(p.node(2))) {
                    self.c.error(lineno, "Expected a numeric expression, got TYPE.string instead");
                    V::None
                } else {
                    let arg = p.node(2).unwrap();
                    if self.c.is_unsigned(&[arg]) && !self.c.is_number(&[arg]) {
                        self.c.warning(lineno, "Sign of unsigned value is always 0 or 1");
                    }
                    self.builtin(lineno, "SGN", Some(arg), Some(fold::sgn), Some(Ty::new(BasicType::Byte)))
                }
            }
            // Trigonometric and LN, EXP, SQR.
            Rule::p_expr_trig => {
                let lineno = p.lineno(1);
                let fname = p.text(1).to_string();
                let float = Ty::new(BasicType::Float);
                let operand = self.c.make_typecast(float, p.node(2), lineno);
                let func: Fold1 = match fname.as_str() {
                    "SIN" => fold::sin,
                    "COS" => fold::cos,
                    "TAN" => fold::tan,
                    "ASN" => fold::asn,
                    "ACS" => fold::acs,
                    "ATN" => fold::atn,
                    "LN" => fold::ln,
                    "EXP" => fold::exp,
                    "SQR" => fold::sqr,
                    other => crash(&format!("KeyError: '{other}'")),
                };
                self.builtin(lineno, &fname, operand, Some(func), Some(float))
            }
            Rule::p_math_fn => p.take(1),
            // Other important functions.
            Rule::p_expr_int => V::opt(self.c.make_typecast(Ty::new(BasicType::Long), p.node(2), p.lineno(1))),
            Rule::p_abs => {
                let lineno = p.lineno(1);
                match p.node(2) {
                    Some(arg) if self.c.is_unsigned(&[arg]) => {
                        self.c.warning(lineno, "Redundant operation ABS for unsigned value");
                        V::Node(arg)
                    }
                    arg => self.builtin(lineno, "ABS", arg, Some(fold::abs), None),
                }
            }
            _ => return Ok(None),
        }))
    }
}
