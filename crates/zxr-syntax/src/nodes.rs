//! The constructors of the node classes in `src/symbols/`: `__init__` and
//! `make_node` of each. Constant folding happens here, as the tree is built.

use crate::ast::*;
use crate::compiler::*;
use crate::pyval::{self, crash};
use crate::types::{BasicType, Num, Ty};
use std::rc::Rc;

/// `utils.eval_to_num`: the Python code evaluates the text as an expression.
/// The texts it is given are operands of the intermediate code, and of those
/// only a number evaluates to one.
pub fn eval_to_num(expr: &str) -> Option<Num> {
    let text = expr.trim();
    if text.is_empty() || !text.chars().all(|c| c.is_ascii_digit() || matches!(c, '-' | '+' | '.' | 'e' | 'E')) {
        return None;
    }
    if let Ok(i) = text.parse::<i128>() {
        return Some(Num::Int(i));
    }
    text.parse::<f64>().ok().map(Num::Float)
}

impl Compiler {
    // ------------------------------------------------------------------
    // NOP, BLOCK, SENTENCE, ASM
    // ------------------------------------------------------------------

    /// `SymbolNOP()`.
    pub fn new_nop(&mut self) -> NodeId {
        self.new_node(Kind::Nop, 0, &[])
    }

    /// `SymbolBLOCK.make_node(*args)`: a block of the arguments, with nested
    /// blocks flattened and empty ones dropped.
    pub fn make_block(&mut self, args: &[Option<NodeId>]) -> NodeId {
        let block = self.new_node(Kind::Block, 0, &[]);
        self.block_append(block, args);
        block
    }

    /// `SymbolBLOCK.append(*args)`.
    pub fn block_append(&mut self, block: NodeId, args: &[Option<NodeId>]) {
        for &arg in args {
            if self.is_null(&[arg]) {
                continue;
            }
            let arg = arg.unwrap();
            if self.token(arg) == "BLOCK" {
                let children = self.children(arg).to_vec();
                self.block_append(block, &children);
            } else {
                self.append_child(block, Some(arg));
            }
        }
    }

    /// `SymbolSENTENCE(lineno, filename, keyword, *args)`: arguments that are
    /// `None` are left out.
    pub fn new_sentence(&mut self, lineno: u32, keyword: &str, args: &[Option<NodeId>], is_sentinel: bool) -> NodeId {
        let filename = self.filename();
        let args: Vec<Option<NodeId>> = args.iter().copied().filter(Option::is_some).collect();
        self.new_node(Kind::Sentence { keyword: keyword.to_string(), filename, is_sentinel }, lineno, &args)
    }

    /// `SymbolASM(asm, lineno, filename, is_sentinel, bank)`.
    pub fn new_asm(&mut self, asm: &str, lineno: u32, is_sentinel: bool, bank: i64) -> NodeId {
        let filename = self.filename();
        self.new_node(Kind::Asm { asm: asm.to_string(), filename, is_sentinel, bank }, lineno, &[])
    }

    // ------------------------------------------------------------------
    // NUMBER, STRING, CONSTEXPR
    // ------------------------------------------------------------------

    /// `SymbolNUMBER(value, lineno, type_)`: a whole value becomes an integer,
    /// and with no type given the value decides it.
    pub fn new_number(&mut self, value: Num, lineno: u32, ty: Option<Ty>) -> NodeId {
        if let Num::Float(f) = value {
            if f.is_nan() {
                crash("ValueError: cannot convert float NaN to integer");
            }
            if f.is_infinite() {
                crash("OverflowError: cannot convert float infinity to integer");
            }
        }
        let value = value.normalised();
        let id = self.new_node(Kind::Number { value }, lineno, &[]);
        self.node_mut(id).ty = Some(ty.unwrap_or_else(|| Ty::new(value.literal_type())));
        id
    }

    /// `number.value = value`: assigned as it is, a whole float staying a
    /// float (and printing as `2.0`).
    pub fn set_number_value(&mut self, id: NodeId, new: Num) {
        match &mut self.node_mut(id).kind {
            Kind::Number { value } => *value = new,
            _ => crash("AttributeError: can't set attribute 'value'"),
        }
    }

    /// `SymbolSTRING(value, lineno)`.
    pub fn new_string(&mut self, value: &str, lineno: u32) -> NodeId {
        let id = self.new_node(Kind::Str { value: value.to_string() }, lineno, &[]);
        let node = self.node_mut(id);
        node.ty = Some(Ty::new(BasicType::String));
        node.t = Some(value.to_string());
        id
    }

    /// `SymbolCONSTEXPR(expr, lineno)`. It takes a temporary it never uses.
    pub fn new_constexpr(&mut self, expr: NodeId, lineno: u32) -> NodeId {
        let id = self.new_node(Kind::ConstExpr, lineno, &[Some(expr)]);
        let t = self.new_t();
        self.node_mut(id).t = Some(t);
        id
    }

    // ------------------------------------------------------------------
    // TYPECAST
    // ------------------------------------------------------------------

    /// `SymbolTYPECAST.make_node(new_type, node, lineno)`: the node converted
    /// to the type; the node itself where there is nothing to convert, and a
    /// constant converted in place. `None` after an error.
    pub fn typecast(&mut self, new_type: Ty, node: Option<NodeId>, lineno: u32) -> Option<NodeId> {
        let node = node?;
        let mut new_type = new_type;
        let node_ty = self.ty(node).unwrap_or_else(|| crash("AssertionError: Invalid operand 'None'"));
        if new_type == node_ty {
            return Some(node);
        }
        if self.token(node) == "VARARRAY" {
            if new_type.size() == node_ty.size() && new_type != BasicType::String && node_ty != BasicType::String {
                return Some(node);
            }
            let name = self.id(node).name.clone();
            self.error(lineno, &format!("Array {name} type does not match parameter type"));
            return None;
        }
        // Typecasting, at the moment, only for numbers.
        if node_ty == BasicType::String {
            self.error(lineno, "Cannot convert string to a value. Use VAL() function");
            return None;
        }
        if new_type == BasicType::String {
            self.error(lineno, "Cannot convert value to string. Use STR() function");
            return None;
        }
        // A constant expression is converted inside.
        if self.is_constexpr(&[node]) {
            let expr = self.child(node, 0).unwrap();
            let cast = self.new_node(Kind::TypeCast, lineno, &[Some(expr)]);
            self.node_mut(cast).ty = Some(new_type);
            self.set_children(node, &[Some(cast)]);
            return Some(node);
        }
        if !self.is_number(&[node]) && !self.is_const(&[node]) {
            let cast = self.new_node(Kind::TypeCast, lineno, &[Some(node)]);
            self.node_mut(cast).ty = Some(new_type);
            return Some(cast);
        }
        // A number: convert it directly.
        let node = if self.is_const(&[node]) {
            let value = self.num_value(node);
            let (line, ty) = (self.lineno(node), self.ty(node));
            self.new_number(value, line, ty)
        } else {
            node
        };
        let value = self.num_value(node);
        if new_type == BasicType::Boolean {
            self.set_number_value(node, Num::Int(!value.is_zero() as i128));
            new_type = Ty::new(BasicType::UByte); // UByte is what is used outside
        } else if !new_type.basic.is_integral() {
            self.set_number_value(node, Num::Float(value.as_f64()));
        } else {
            // The ZX Spectrum ROM truncates towards minus infinity, and so
            // does floor().
            if let Num::Float(f) = value {
                if !f.is_finite() {
                    crash("OverflowError: cannot convert float infinity to integer");
                }
            }
            let bits = 8 * new_type.size() as u32;
            let modulus: i128 = 1 << bits;
            let new_val = value.floor() & (modulus - 1);
            let line = self.lineno(node);
            if value >= Num::Int(0) && value != Num::Int(new_val) {
                self.warning_conversion_lose_digits(line);
                self.set_number_value(node, Num::Int(new_val));
            } else if value < Num::Int(0) && pyval::add(Num::Int(modulus), value) != Num::Int(new_val) {
                // A positive value coerced to a negative one.
                self.warning_conversion_lose_digits(line);
                self.set_number_value(node, Num::Int(new_val - modulus));
            }
        }
        self.node_mut(node).ty = Some(new_type);
        Some(node)
    }

    // ------------------------------------------------------------------
    // BINARY, UNARY, BUILTIN
    // ------------------------------------------------------------------

    /// `SymbolBINARY(operator, left, right, lineno, type_, func)`.
    pub fn new_binary(&mut self, operator: &str, left: NodeId, right: NodeId, lineno: u32, ty: Option<Ty>, func: Option<Fold2>) -> NodeId {
        let ty = ty.or_else(|| self.common_type(left, right));
        let id = self.new_node(Kind::Binary { operator: operator.to_string(), func }, lineno, &[Some(left), Some(right)]);
        self.node_mut(id).ty = ty;
        id
    }

    /// `SymbolBINARY.make_node(operator, left, right, lineno, func, type_)`:
    /// a node for `left operator right`, folded if both sides are constant.
    /// `func` is what folds them; `type_` forces the type of the result.
    pub fn make_binary_node(&mut self, operator: &str, left: Option<NodeId>, right: Option<NodeId>, lineno: u32, func: Option<Fold2>, type_: Option<Ty>) -> Option<NodeId> {
        let (Some(mut a), Some(mut b)) = (left, right) else { return None };
        if matches!(operator, "BAND" | "BOR" | "BXOR" | "AND" | "OR" | "XOR" | "MINUS" | "MULT" | "DIV" | "SHL" | "SHR") && !self.is_numeric(&[a, b]) {
            self.error(lineno, &format!("Operator {operator} cannot be used with strings"));
            return None;
        }
        if !matches!(operator, "AND" | "OR" | "XOR") {
            // Operators that are not boolean take numbers: a boolean operand
            // becomes 0 or 1.
            if self.ty(a).is_some_and(|t| t == BasicType::Boolean) {
                a = self.typecast(Ty::new(BasicType::UByte), Some(a), lineno).unwrap();
            }
            if self.ty(b).is_some_and(|t| t == BasicType::Boolean) {
                b = self.typecast(Ty::new(BasicType::UByte), Some(b), lineno).unwrap();
            }
        }
        let (a_ty, b_ty) = (self.ty(a), self.ty(b));
        let mut c_type = self.common_type(a, b).unwrap_or_else(|| crash("AttributeError: 'NoneType' object has no attribute 'final'"));
        if c_type.basic.is_numeric() {
            // There is a common type for a and b.
            let constant = |c: &Compiler, x: NodeId| c.is_const(&[x]) || c.is_number(&[x]);
            if self.is_numeric(&[a, b]) && constant(self, a) && constant(self, b) {
                if let Some(func) = func {
                    let a = self.typecast(c_type, Some(a), lineno).unwrap();
                    let b = self.typecast(c_type, Some(b), lineno).unwrap();
                    let (x, y) = (self.value(a), self.value(b));
                    return Some(self.new_number(func(&x, &y).num(), lineno, type_));
                }
            }
            // Only what an assembler expression can say stays for the
            // assembler. The Python compiler wraps a comparison or a logical
            // or bitwise operator of two addresses too, and then has no way
            // of writing it out: `@a < 16384` ends it with InvalidOperatorError.
            // Here those are computed when the program runs.
            let assembles = matches!(operator, "PLUS" | "MINUS" | "MUL" | "DIV" | "MOD" | "POW" | "SHL" | "SHR");
            if assembles && self.is_static(&[a, b]) {
                let a = self.typecast(c_type, Some(a), lineno).unwrap_or_else(|| crash("AssertionError: None is not Symbol"));
                let b = self.typecast(c_type, Some(b), lineno).unwrap_or_else(|| crash("AssertionError: None is not Symbol"));
                let binary = self.new_binary(operator, a, b, lineno, type_, func);
                return Some(self.new_constexpr(binary, lineno));
            }
        }
        if self.is_string(&[a, b]) {
            if let Some(func) = func {
                // Two constant strings.
                if operator == "PLUS" {
                    let (x, y) = (self.value(a), self.value(b));
                    let joined = func(&x, &y);
                    return Some(self.new_string(joined.as_str(), lineno));
                }
                // Comparing them reads `a.text`, which no node has.
                crash("AttributeError: 'SymbolSTRING' object has no attribute 'text'");
            }
        }
        if matches!(operator, "BNOT" | "BAND" | "BOR" | "BXOR") && c_type.basic.is_decimal() {
            c_type = Ty::new(BasicType::Long);
        }
        if a_ty != b_ty && (a_ty.is_some_and(|t| t == BasicType::String) || b_ty.is_some_and(|t| t == BasicType::String)) {
            // This gives an error that goes by the first operand.
            c_type = a_ty.unwrap();
        }
        let (mut a, mut b) = (Some(a), Some(b));
        if !matches!(operator, "SHR" | "SHL") {
            a = self.typecast(c_type, a, lineno);
            b = self.typecast(c_type, b, lineno);
        }
        let (Some(a), Some(b)) = (a, b) else { return None };
        let type_ = type_.unwrap_or_else(|| if matches!(operator, "LT" | "GT" | "EQ" | "LE" | "GE" | "NE" | "AND" | "OR" | "XOR") { Ty::new(BasicType::Boolean) } else { c_type });
        Some(self.new_binary(operator, a, b, lineno, Some(type_), func))
    }

    /// `SymbolUNARY(oper, operand, lineno, type_)`.
    pub fn new_unary(&mut self, operator: &str, operand: NodeId, lineno: u32, ty: Option<Ty>) -> NodeId {
        let id = self.new_node(Kind::Unary { operator: operator.to_string() }, lineno, &[Some(operand)]);
        self.node_mut(id).ty = ty;
        id
    }

    /// `SymbolUNARY.make_node(lineno, operator, operand, func, type_)`.
    /// `type_` is the type of the result where it is not the operand's.
    pub fn make_unary_node(&mut self, lineno: u32, operator: &str, operand: NodeId, func: Option<Fold1>, type_: Option<Ty>) -> NodeId {
        let mut operand = operand;
        if let Some(func) = func {
            if self.is_number(&[operand]) {
                let value = self.value(operand);
                return self.new_number(func(&value).num(), lineno, None);
            }
            if self.is_string(&[operand]) {
                crash("AttributeError: 'SymbolSTRING' object has no attribute 'text'");
            }
        }
        let mut type_ = type_.or_else(|| self.ty(operand));
        if operator == "MINUS" {
            let ty = type_.unwrap_or_else(|| crash("AttributeError: 'NoneType' object has no attribute 'is_signed'"));
            if !ty.basic.is_signed() {
                let signed = Ty::new(ty.basic.to_signed());
                type_ = Some(signed);
                // A string cannot be converted; the error is reported and
                // then the node is built without an operand, which fails.
                operand = self.typecast(signed, Some(operand), lineno).unwrap_or_else(|| crash("AssertionError: None is not Symbol"));
            }
        } else if operator == "NOT" {
            type_ = Some(Ty::new(BasicType::Boolean));
        }
        self.new_unary(operator, operand, lineno, type_)
    }

    /// `SymbolBUILTIN.make_node(lineno, fname, func, type_, *operands)`.
    pub fn make_builtin_node(&mut self, lineno: u32, fname: &str, func: Option<Fold1>, type_: Option<Ty>, operands: &[NodeId]) -> NodeId {
        if let (Some(func), [operand]) = (func, operands) {
            if self.is_number(&[*operand]) || self.is_string(&[*operand]) {
                let value = self.value(*operand);
                return self.new_number(func(&value).num(), lineno, type_);
            }
        }
        let operands: Vec<Option<NodeId>> = operands.iter().map(|&o| Some(o)).collect();
        let id = self.new_node(Kind::Builtin { fname: fname.to_string(), discard_result: false }, lineno, &operands);
        self.node_mut(id).ty = type_;
        id
    }

    // ------------------------------------------------------------------
    // STRSLICE
    // ------------------------------------------------------------------

    /// `SymbolSTRSLICE.make_node(lineno, s, lower, upper)`: `s(lower TO
    /// upper)`. A constant string with constant bounds is sliced now.
    pub fn make_strslice_node(&mut self, lineno: u32, s: Option<NodeId>, lower: Option<NodeId>, upper: Option<NodeId>) -> Option<NodeId> {
        let (Some(s), Some(lower), Some(upper)) = (s, lower, upper) else { return None };
        if !self.check_type(lineno, Ty::new(BasicType::String), s) {
            return None;
        }
        let index_type = Ty::new(STR_INDEX_TYPE);
        let base = self.new_number(Num::Int(self.opts.int("string_base")), lineno, None);
        let lower = self.make_binary_node("MINUS", Some(lower), Some(base), lineno, Some(pyval::fold::sub), None);
        let lower = self.typecast(index_type, lower, lineno);
        let upper = self.make_binary_node("MINUS", Some(upper), Some(base), lineno, Some(pyval::fold::sub), None);
        let upper = self.typecast(index_type, upper, lineno);
        let (Some(lower), Some(upper)) = (lower, upper) else { return None };
        let (mut lo, mut up) = (None, None);
        if self.is_number(&[lower]) {
            let mut value = self.num_value(lower);
            if value < Num::Int(MIN_STRSLICE_IDX) {
                value = Num::Int(MIN_STRSLICE_IDX);
                self.set_number_value(lower, value);
            }
            lo = Some(value);
        }
        if self.is_number(&[upper]) {
            let mut value = self.num_value(upper);
            if value > Num::Int(MAX_STRSLICE_IDX) {
                value = Num::Int(MAX_STRSLICE_IDX);
                self.set_number_value(upper, value);
            }
            up = Some(value);
        }
        if let (Some(lo), Some(up)) = (lo, up) {
            if lo > up {
                return Some(self.new_string("", lineno));
            }
            if matches!(self.token(s), "STRING" | "CONST") {
                // A constant string: slice it now.
                let (Num::Int(lo), Num::Int(up)) = (lo, up) else { crash("TypeError: slice indices must be integers or None or have an __index__ method") };
                let value = self.value(s);
                let sliced: String = value.as_str().chars().skip(lo as usize).take((up + 1 - lo) as usize).collect();
                return Some(self.new_string(&sliced, lineno));
            }
            // a$(0 TO the end) is a$
            if lo == Num::Int(MIN_STRSLICE_IDX) && up == Num::Int(MAX_STRSLICE_IDX) {
                return Some(s);
            }
        }
        let id = self.new_node(Kind::StrSlice, lineno, &[Some(s), Some(lower), Some(upper)]);
        self.node_mut(id).ty = Some(Ty::new(BasicType::String));
        Some(id)
    }

    // ------------------------------------------------------------------
    // ARGUMENT, ARGLIST, PARAMLIST, BOUND, BOUNDLIST
    // ------------------------------------------------------------------

    /// `SymbolARGUMENT(value, lineno, byref, filename, name)`.
    pub fn new_argument(&mut self, value: NodeId, lineno: u32, byref: Option<bool>, name: Option<String>) -> NodeId {
        let filename = self.filename();
        let id = self.new_node(Kind::Argument { filename, byref: false, name }, lineno, &[Some(value)]);
        let byref = byref.unwrap_or_else(|| self.opts.bool("default_byref"));
        self.set_arg_byref(id, byref);
        id
    }

    /// `argument.byref = value`. Only a variable or an array element can be
    /// passed by reference.
    pub fn set_arg_byref(&mut self, arg: NodeId, value: bool) {
        if value {
            let ok = self.child(arg, 0).is_some_and(|v| matches!(self.token(v), "VAR" | "VARARRAY" | "ARRAYLOAD"));
            if !ok {
                crash("AssertionError: self.value.token in (\"VAR\", \"VARARRAY\", \"ARRAYLOAD\")");
            }
        }
        if let Kind::Argument { byref, .. } = &mut self.node_mut(arg).kind {
            *byref = value;
        }
    }

    pub fn arg_byref(&self, arg: NodeId) -> bool {
        matches!(self.node(arg).kind, Kind::Argument { byref: true, .. })
    }

    /// `argument.value`.
    pub fn arg_value(&self, arg: NodeId) -> Option<NodeId> {
        self.child(arg, 0)
    }

    /// `SymbolARGUMENT.typecast(type_)`: converts the argument's value.
    /// False, with the value gone, if it cannot be.
    pub fn argument_typecast(&mut self, arg: NodeId, type_: Ty) -> bool {
        let value = self.child(arg, 0);
        let lineno = self.lineno(arg);
        let cast = self.typecast(type_, value, lineno);
        self.set_child(arg, 0, cast);
        cast.is_some()
    }

    /// `SymbolARGLIST.make_node(node, *args)`.
    pub fn arglist_make_node(&mut self, node: Option<NodeId>, args: &[NodeId]) -> NodeId {
        let node = match node {
            None => self.new_node(Kind::ArgList, 0, &[]),
            Some(node) if matches!(self.node(node).kind, Kind::Argument { .. }) => {
                let list = self.new_node(Kind::ArgList, 0, &[]);
                self.append_child(list, Some(node));
                list
            }
            Some(node) => node,
        };
        for &arg in args {
            self.append_child(node, Some(arg));
        }
        node
    }

    /// `SymbolPARAMLIST.append_child(param)`: the parameter gets the next
    /// offset in the list.
    pub fn paramlist_append(&mut self, list: NodeId, param: NodeId) {
        self.append_child(list, Some(param));
        if self.id(param).r.offset.is_none() {
            let Kind::ParamList { size } = self.node(list).kind else { crash("AttributeError: not a PARAMLIST") };
            self.id_mut(param).r.offset = Some(size);
            let param_size = self.size(param);
            if let Kind::ParamList { size } = &mut self.node_mut(list).kind {
                *size += param_size;
            }
        }
    }

    /// `SymbolPARAMLIST.make_node(node, *params)`.
    pub fn paramlist_make_node(&mut self, node: Option<NodeId>, params: &[Option<NodeId>]) -> NodeId {
        let mut params = params.to_vec();
        let node = match node {
            None => self.new_node(Kind::ParamList { size: 0 }, 0, &[]),
            Some(node) if !matches!(self.node(node).kind, Kind::ParamList { .. }) => {
                params.insert(0, Some(node));
                self.new_node(Kind::ParamList { size: 0 }, 0, &[])
            }
            Some(node) => node,
        };
        for param in params.into_iter().flatten() {
            self.paramlist_append(node, param);
        }
        node
    }

    /// `SymbolBOUND.make_node(lower, upper, lineno)`.
    pub fn make_bound(&mut self, lower: NodeId, upper: NodeId, lineno: u32) -> Option<NodeId> {
        if !self.is_static(&[lower, upper]) {
            self.error(lineno, "Array bounds must be constants");
            return None;
        }
        let lower_t = self.t(lower);
        let Some(lower_value) = eval_to_num(&lower_t) else {
            self.error(lineno, "Unknown lower bound for array dimension");
            return None;
        };
        let upper_t = self.t(upper);
        let Some(upper_value) = eval_to_num(&upper_t) else {
            self.error(lineno, "Unknown upper bound for array dimension");
            return None;
        };
        if lower_value < Num::Int(0) {
            self.error(lineno, "Array bounds must be greater than 0");
            return None;
        }
        if lower_value > upper_value {
            self.error(lineno, "Lower array bound must be less or equal to upper one");
            return None;
        }
        let (Num::Int(lower), Num::Int(upper)) = (lower_value, upper_value) else { crash("AssertionError: isinstance(lower, int)") };
        Some(self.new_node(Kind::Bound { lower, upper }, 0, &[]))
    }

    /// `SymbolBOUNDLIST.make_node(node, *args)`.
    pub fn boundlist_make_node(&mut self, node: Option<NodeId>, args: &[Option<NodeId>]) -> NodeId {
        let mut args = args.to_vec();
        let node = match node {
            None => self.new_node(Kind::BoundList, 0, &[]),
            Some(node) if !matches!(self.node(node).kind, Kind::BoundList) => {
                args.insert(0, Some(node));
                self.new_node(Kind::BoundList, 0, &[])
            }
            Some(node) => node,
        };
        for arg in args.into_iter().flatten() {
            self.append_child(node, Some(arg));
        }
        node
    }

    // ------------------------------------------------------------------
    // VARDECL, ARRAYDECL, FUNCDECL
    // ------------------------------------------------------------------

    /// `SymbolVARDECL(entry)`.
    pub fn new_vardecl(&mut self, entry: NodeId) -> NodeId {
        self.new_node(Kind::VarDecl, 0, &[Some(entry)])
    }

    /// `SymbolARRAYDECL(entry)`.
    pub fn new_arraydecl(&mut self, entry: NodeId) -> NodeId {
        self.new_node(Kind::ArrayDecl, 0, &[Some(entry)])
    }

    /// `SymbolFUNCDECL(entry, lineno)`.
    pub fn new_funcdecl(&mut self, entry: NodeId, lineno: u32) -> NodeId {
        if self.token(entry) != "FUNCTION" {
            crash("AssertionError: isinstance(value, SymbolID) and value.token == \"FUNCTION\"");
        }
        self.new_node(Kind::FuncDecl, lineno, &[Some(entry)])
    }

    /// `SymbolFUNCDECL.make_node(func_name, lineno, class_, type_)`.
    pub fn make_func_declaration(&mut self, func_name: &str, lineno: u32, class: Class, type_: Option<Ty>) -> Option<NodeId> {
        let entry = self.declare_func(func_name, lineno, type_, class)?;
        self.id_mut(entry).declared = true;
        Some(self.new_funcdecl(entry, lineno))
    }

    // ------------------------------------------------------------------
    // CALL, FUNCCALL, ARRAYACCESS, ARRAYLOAD
    // ------------------------------------------------------------------

    /// `SymbolCALL(entry, arglist, lineno, filename)`. `kind` picks the class:
    /// `Kind::Call` or `Kind::FuncCall`.
    fn new_call(&mut self, kind: fn(CallData) -> Kind, entry: NodeId, arglist: NodeId, lineno: u32, filename: Rc<str>) -> NodeId {
        let id = self.new_node(kind(CallData { filename, offset: None }), lineno, &[Some(entry), Some(arglist)]);
        if self.id(entry).func().is_some() {
            // Each argument depends on its parameter.
            let args: Vec<NodeId> = self.children(arglist).iter().flatten().copied().collect();
            let params: Vec<NodeId> = self.children(self.params(entry)).iter().flatten().copied().collect();
            for (arg, param) in args.into_iter().zip(params) {
                let Some(value) = self.child(arg, 0) else { continue };
                self.add_required_symbol(value, param);
                if self.is_id(value) && self.class_(value) == Class::Array && self.class_(param) == Class::Array && self.id(param).var().is_some_and(|v| v.is_dynamically_accessed) {
                    self.id_mut(value).var_mut().unwrap().is_dynamically_accessed = true;
                }
            }
        }
        id
    }

    /// `SymbolCALL(entry, arglist, lineno, filename)`, as the passes make
    /// one to keep a call whose result is no longer wanted.
    pub fn new_call_statement(&mut self, entry: NodeId, arglist: NodeId, lineno: u32, filename: Rc<str>) -> NodeId {
        if self.token(entry) != "FUNCTION" {
            crash("AssertionError: value.token == \"FUNCTION\"");
        }
        self.new_call(Kind::Call, entry, arglist, lineno, filename)
    }

    fn make_call_node(&mut self, kind: fn(CallData) -> Kind, id: &str, params: NodeId, lineno: u32) -> Option<NodeId> {
        let filename = self.filename();
        let mut entry = self.access_func(id, lineno, None, None)?;
        if !self.id(entry).r.callable && self.ty(entry).is_none_or(|t| t != BasicType::String) {
            self.syntax_error_not_array_nor_func(lineno, id);
            return None;
        }
        let forwarded = match self.id(entry).func() {
            Some(func) => func.forwarded,
            None if self.id(entry).declared => crash("AttributeError: 'SymbolRef' object has no attribute 'forwarded'"),
            None => false,
        };
        if self.id(entry).declared && !forwarded {
            self.check_call_arguments(lineno, id, params, &filename);
        } else {
            // Not defined yet. All functions go to the global scope.
            if self.token(entry) != "FUNCTION" {
                entry = self.to_function(entry, Some(lineno), Class::Function);
            }
            self.move_to_global_scope(id);
            self.note_call(entry, lineno);
            let result = self.new_call(kind, entry, params, lineno, filename);
            self.function_calls.push(result);
            return Some(result);
        }
        if self.token(entry) != "FUNCTION" {
            crash("AssertionError: value.token == \"FUNCTION\"");
        }
        self.note_call(entry, lineno);
        Some(self.new_call(kind, entry, params, lineno, filename))
    }

    /// `SymbolCALL.make_node(id_, params, lineno, filename)`: a call whose
    /// result is not used.
    pub fn make_sub_call(&mut self, id: &str, lineno: u32, params: NodeId) -> Option<NodeId> {
        self.make_call_node(Kind::Call, id, params, lineno)
    }

    /// `SymbolFUNCCALL.make_node`: a call inside an expression.
    pub fn make_func_call(&mut self, id: &str, lineno: u32, params: NodeId) -> Option<NodeId> {
        self.make_call_node(Kind::FuncCall, id, params, lineno)
    }

    /// `SymbolARRAYACCESS(entry, arglist, lineno, filename)`.
    pub fn new_array_access(&mut self, load: bool, entry: NodeId, arglist: NodeId, lineno: u32, filename: Rc<str>) -> NodeId {
        if self.token(entry) != "VARARRAY" {
            crash("AssertionError: isinstance(value, SymbolID) and value.token == \"VARARRAY\"");
        }
        let data = CallData { filename, offset: None };
        let id = self.new_node(if load { Kind::ArrayLoad(data) } else { Kind::ArrayAccess(data) }, lineno, &[Some(entry), Some(arglist)]);
        let all_bound_type = self.children(arglist).iter().flatten().all(|&x| self.ty(x).is_some_and(|t| t == BOUND_TYPE));
        if !all_bound_type {
            crash("AssertionError: Invalid type for array index");
        }
        self.id_mut(entry).var_mut().unwrap().is_dynamically_accessed = true;
        id
    }

    /// `SymbolARRAYACCESS.make_node(id_, arglist, lineno, filename)`, or
    /// `SymbolARRAYLOAD.make_node` with `load`: `A(x1, x2, ..., xn)`.
    pub fn make_array_access_node(&mut self, load: bool, id: &str, arglist: NodeId, lineno: u32) -> Option<NodeId> {
        let filename = self.filename();
        let variable = self.access_array(id, lineno, None, None)?;
        let btype = Ty::new(BOUND_TYPE);
        let args: Vec<NodeId> = self.children(arglist).iter().flatten().copied().collect();
        if self.id(variable).scope != Scope::Parameter {
            let bounds: Vec<NodeId> = self.children(self.bounds(variable)).iter().flatten().copied().collect();
            if bounds.len() != args.len() {
                let msg = format!("Array '{}' has {} dimensions, not {}", self.id(variable).name, bounds.len(), args.len());
                self.error(lineno, &msg);
                return None;
            }
            // A constant subscript is checked against the bounds.
            for (&arg, &bound) in args.iter().zip(&bounds) {
                let value = self.child(arg, 0).unwrap_or_else(|| crash("AttributeError: 'NoneType' object has no attribute 'token'"));
                if self.is_number(&[value]) || self.is_const(&[value]) {
                    let val = self.num_value(value);
                    let Kind::Bound { lower, upper } = self.node(bound).kind else { unreachable!() };
                    if val < Num::Int(lower) || val > Num::Int(upper) {
                        self.warning(lineno, &format!("Array '{id}' subscript out of range"));
                    }
                }
                let cast = self.typecast(btype, Some(value), lineno);
                self.set_child(arg, 0, cast);
            }
        } else {
            for &arg in &args {
                let value = self.child(arg, 0).unwrap_or_else(|| crash("AttributeError: 'NoneType' object has no attribute 'lineno'"));
                let cast = self.typecast(btype, Some(value), self.lineno(value));
                self.set_child(arg, 0, cast);
            }
        }
        Some(self.new_array_access(load, variable, arglist, lineno, filename))
    }

    /// `SymbolARRAYACCESS.offset`: for an access with constant subscripts,
    /// the offset in bytes of the element from the start of the array's
    /// data. Worked out once, the first time it is asked for.
    pub fn array_offset(&mut self, access: NodeId) -> Option<Num> {
        if let Some(cached) = self.node(access).call().offset {
            return cached;
        }
        let offset = self.compute_array_offset(access);
        self.node_mut(access).call_mut().offset = Some(offset);
        offset
    }

    fn compute_array_offset(&mut self, access: NodeId) -> Option<Num> {
        let entry = self.entry(access);
        if self.id(entry).scope == Scope::Parameter {
            return None;
        }
        let arglist = self.child(access, 1).unwrap();
        let args: Vec<NodeId> = self.children(arglist).iter().flatten().copied().collect();
        let bounds: Vec<NodeId> = self.children(self.bounds(entry)).iter().flatten().copied().collect();
        let mut offset = Num::Int(0);
        for (arg, bound) in args.into_iter().zip(bounds) {
            let tmp = self.child(arg, 0).unwrap_or_else(|| crash("AttributeError: 'NoneType' object has no attribute 'token'"));
            if !(self.is_number(&[tmp]) || self.is_const(&[tmp])) {
                return None;
            }
            let Kind::Bound { lower, .. } = self.node(bound).kind else { unreachable!() };
            let value = self.num_value(tmp);
            offset = pyval::add(pyval::mul(offset, Num::Int(self.bound_count(bound))), pyval::sub(value, Num::Int(lower)));
        }
        Some(pyval::mul(offset, Num::Int(self.ty_of(access).size() as i128)))
    }

    /// `SymbolARRAYACCESS.copy_from(other)`: the same access, as a new
    /// ARRAYACCESS node.
    pub fn array_access_copy_from(&mut self, other: NodeId) -> NodeId {
        let (entry, arglist) = (self.entry(other), self.child(other, 1).unwrap());
        let (lineno, filename) = (self.lineno(other), self.node(other).call().filename.clone());
        self.new_array_access(false, entry, arglist, lineno, filename)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zxr_core::Session;

    fn compiler() -> Compiler {
        let mut sess = Session::default();
        sess.stderr = Box::new(std::io::sink());
        Compiler::new(sess)
    }

    fn cast(basic: BasicType, value: Num) -> (String, BasicType) {
        let mut c = compiler();
        let n = c.new_number(value, 1, None);
        let n = c.typecast(Ty::new(basic), Some(n), 1).unwrap();
        (c.t(n), c.ty_of(n).basic)
    }

    // Each expected value is what the Python classes give.
    #[test]
    fn typecast_of_numbers() {
        use BasicType::*;
        assert_eq!(cast(Float, Num::Int(5)), ("5.0".to_string(), Float));
        assert_eq!(cast(UByte, Num::Int(256)), ("0".to_string(), UByte));
        assert_eq!(cast(UByte, Num::Int(-1)), ("-1".to_string(), UByte));
        assert_eq!(cast(Byte, Num::Int(200)), ("200".to_string(), Byte));
        assert_eq!(cast(UByte, Num::Float(3.7)), ("3".to_string(), UByte));
        assert_eq!(cast(UByte, Num::Float(-1.5)), ("-2".to_string(), UByte));
        assert_eq!(cast(Integer, Num::Int(65535)), ("65535".to_string(), Integer));
        assert_eq!(cast(UInteger, Num::Int(65536)), ("0".to_string(), UInteger));
        assert_eq!(cast(Boolean, Num::Int(7)), ("1".to_string(), UByte));
        assert_eq!(cast(Fixed, Num::Int(2)), ("2.0".to_string(), Fixed));
    }

    #[test]
    fn folding_and_temporaries() {
        let mut c = compiler();
        let a = c.new_number(Num::Int(2), 1, None);
        let b = c.new_number(Num::Float(1.5), 1, None);
        let sum = c.make_binary_node("PLUS", Some(a), Some(b), 1, Some(pyval::fold::add), None).unwrap();
        assert_eq!(c.t(sum), "3.5");
        assert_eq!(c.ty_of(sum).basic, BasicType::Fixed);
        // An identifier takes one temporary for its first ref and one more
        // when it becomes a variable.
        let before = c.temp_counter;
        let v = c.access_var("x", 1, None, None).unwrap();
        assert_eq!(c.temp_counter, before + 2);
        assert_eq!(c.t(v), "_x");
        let expr = c.make_binary_node("PLUS", Some(v), Some(sum), 1, Some(pyval::fold::add), None).unwrap();
        assert_eq!(c.token(expr), "BINARY");
        assert_eq!(c.t(expr), format!("t{}", before + 2));
    }
}
