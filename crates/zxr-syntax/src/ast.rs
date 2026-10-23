//! The syntax tree: `src/ast/tree.py` and the classes of `src/symbols/`.
//!
//! The Python tree is made of objects that refer to each other freely: a
//! symbol-table entry is the child of every expression that names it, and its
//! `parent` is whichever node took it last. Nodes here live in one arena and
//! are named by index, which keeps exactly that sharing.
//!
//! A Python node's class is its `token` (`SymbolBINARY` is `BINARY`), except
//! that a sentence's token is its keyword and an identifier's follows what the
//! identifier has turned out to be.

use crate::types::{BasicType, Num, Ty};
use std::rc::Rc;

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct NodeId(pub u32);

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct ScopeId(pub u32);

/// `CLASS` in `src/api/constants.py`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Class {
    Unknown,
    Var,
    Array,
    Function,
    Label,
    Const,
    Sub,
    Type,
}

impl Class {
    /// `CLASS.to_string`.
    pub fn name(self) -> &'static str {
        match self {
            Class::Unknown => "unknown",
            Class::Var => "var",
            Class::Array => "array",
            Class::Function => "function",
            Class::Label => "label",
            Class::Const => "const",
            Class::Sub => "sub",
            Class::Type => "type",
        }
    }
}

/// `SCOPE`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scope {
    Global,
    Local,
    Parameter,
}

impl Scope {
    pub fn name(self) -> &'static str {
        match self {
            Scope::Global => "global",
            Scope::Local => "local",
            Scope::Parameter => "parameter",
        }
    }
}

/// `CONVENTION`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Convention {
    Unknown,
    Fastcall,
    Stdcall,
}

impl Convention {
    pub fn name(self) -> &'static str {
        match self {
            Convention::Unknown => "unknown",
            Convention::Fastcall => "__fastcall__",
            Convention::Stdcall => "__stdcall__",
        }
    }
}

/// A Python value a constant holds or a folding function works on. A bool is
/// kept as the integer it is.
#[derive(Clone, Debug, PartialEq)]
pub enum Val {
    Num(Num),
    Str(String),
}

impl Val {
    pub fn int(i: i128) -> Val {
        Val::Num(Num::Int(i))
    }

    pub fn float(f: f64) -> Val {
        Val::Num(Num::Float(f))
    }

    pub fn bool(b: bool) -> Val {
        Val::Num(Num::Int(b as i128))
    }

    /// The number, where the Python code would have one.
    pub fn num(&self) -> Num {
        match self {
            Val::Num(n) => *n,
            Val::Str(_) => crate::pyval::crash("TypeError: a number was expected, not a str"),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Val::Str(s) => s,
            Val::Num(_) => crate::pyval::crash("TypeError: a str was expected, not a number"),
        }
    }
}

/// The `func` of `make_unary` and `make_builtin`: folds one constant.
pub type Fold1 = fn(&Val) -> Val;
/// The `func` of `make_binary`: folds two constants.
pub type Fold2 = fn(&Val, &Val) -> Val;

/// The initial value of an array, `{{1, 2}, {3, 4}}`: a Python list of lists
/// whose leaves are the nodes of the elements.
#[derive(Clone, Debug, PartialEq)]
pub enum ConstVector {
    Elem(NodeId),
    List(Vec<ConstVector>),
}

/// What `VarRef` and `ArrayRef` hold; an array has the last three as well.
#[derive(Clone, Debug, Default)]
pub struct VarRef {
    /// The initial value of a scalar.
    pub default_value: Option<NodeId>,
    /// `default_value` of an array, which is a nested list and not a node.
    pub default_vector: Option<ConstVector>,
    pub byref: bool,
    pub alias: Option<NodeId>,
    /// ZX Next code bank holding the variable; 0 is the resident program.
    pub bank: i64,
    pub lbound_used: bool,
    pub ubound_used: bool,
    /// Whether the array is ever indexed with something not constant.
    pub is_dynamically_accessed: bool,
}

#[derive(Clone, Debug)]
pub struct FuncRef {
    /// `CLASS.function`, `CLASS.sub`, or unknown for one only called so far.
    pub class: Class,
    pub bank: i64,
    /// Declared `PRIVATE`: a banked routine with no resident trampoline,
    /// reachable only from its own bank. Not in the Python compiler.
    pub private: bool,
    pub local_symbol_table: Option<ScopeId>,
    pub convention: Convention,
    /// Declared with DECLARE and not defined yet.
    pub forwarded: bool,
    pub params_size: i64,
    pub locals_size: i64,
}

#[derive(Clone, Debug)]
pub struct LabelRef {
    pub bank: i64,
    /// The nested functions the label was declared in.
    pub scope_owner: Vec<NodeId>,
    pub is_line_number: bool,
}

/// The `ref` object of an identifier, swapped as its class becomes known.
#[derive(Clone, Debug)]
pub enum RefKind {
    /// `SymbolRef`: not known yet.
    Unknown,
    Var(VarRef),
    /// `ConstRef`, with the node of its value.
    Const(NodeId),
    Label(LabelRef),
    Func(FuncRef),
    Array(VarRef),
}

#[derive(Clone, Debug)]
pub struct Ref {
    /// `_t`: the temporary taken when the ref was made.
    pub t: String,
    pub callable: bool,
    pub accessed: bool,
    /// Offset from the frame pointer of a local or a parameter.
    pub offset: Option<i64>,
    /// Whether a FARPTR names the symbol.
    pub has_faraddress: bool,
    pub kind: RefKind,
}

/// `SymbolID`.
#[derive(Clone, Debug)]
pub struct IdData {
    /// The name without a `$`, `%` or `&` suffix once declared.
    pub name: String,
    pub original_name: String,
    pub filename: Rc<str>,
    pub mangled: String,
    /// Explicitly declared (DIM, FUNCTION, a label definition).
    pub declared: bool,
    pub caseins: bool,
    pub scope: Scope,
    pub scope_ref: Option<ScopeId>,
    /// `DIM x AT expr`: the expression of the address.
    pub addr: Option<NodeId>,
    pub has_address: Option<bool>,
    pub r: Ref,
}

/// `SymbolCALL` and its subclasses.
#[derive(Clone, Debug)]
pub struct CallData {
    pub filename: Rc<str>,
    /// `SymbolARRAYACCESS.offset` is a cached property: computed once, on
    /// first use.
    pub offset: Option<Option<Num>>,
}

#[derive(Clone, Debug)]
pub enum Kind {
    Nop,
    Block,
    Asm { asm: String, filename: Rc<str>, is_sentinel: bool, bank: i64 },
    Sentence { keyword: String, filename: Rc<str>, is_sentinel: bool },
    VarDecl,
    ArrayDecl,
    FuncDecl,
    Bound { lower: i128, upper: i128 },
    BoundList,
    ParamList { size: i64 },
    ArgList,
    Argument { filename: Rc<str>, byref: bool, name: Option<String> },
    ConstExpr,
    Number { value: Num },
    Str { value: String },
    Unary { operator: String },
    Binary { operator: String, func: Option<Fold2> },
    TypeCast,
    Builtin { fname: String, discard_result: bool },
    Call(CallData),
    FuncCall(CallData),
    ArrayAccess(CallData),
    ArrayLoad(CallData),
    StrSlice,
    Id(Box<IdData>),
    /// `SymbolBASICTYPE`, as the symbol table holds it under the type's name.
    Type(BasicType),
}

#[derive(Clone, Debug)]
pub struct Node {
    pub kind: Kind,
    /// A child may be missing, as in the Python `ChildrenList`.
    pub children: Vec<Option<NodeId>>,
    /// The node that took this one as a child last.
    pub parent: Option<NodeId>,
    pub lineno: u32,
    /// `_t`: the temporary or operand text, taken on first use by most nodes.
    pub t: Option<String>,
    /// The type the node stores, for the classes that store one. Read types
    /// with `Compiler::ty`, which knows which classes derive theirs.
    pub ty: Option<Ty>,
    /// `Symbol._required_by` and `_requires`: counters, in insertion order.
    pub required_by: Vec<(NodeId, u32)>,
    pub requires: Vec<(NodeId, u32)>,
    /// `funcdecl.params_size`: an attribute the parser sets on a FUNCDECL
    /// node itself (the function's ref has one of its own, which stays 0).
    /// `None` where it was never set, and reading it would raise.
    pub params_size: Option<i64>,
    /// `eol`, an attribute the parser gives a PRINT sentence: whether the
    /// line ends after the last item.
    pub eol: bool,
}

impl Node {
    pub fn new(kind: Kind, lineno: u32) -> Node {
        Node { kind, children: Vec::new(), parent: None, lineno, t: None, ty: None, required_by: Vec::new(), requires: Vec::new(), params_size: None, eol: false }
    }

    pub fn id(&self) -> &IdData {
        match &self.kind {
            Kind::Id(data) => data,
            other => crate::pyval::crash(&format!("AttributeError: a {other:?} node is not an identifier")),
        }
    }

    pub fn id_mut(&mut self) -> &mut IdData {
        match &mut self.kind {
            Kind::Id(data) => data,
            other => crate::pyval::crash(&format!("AttributeError: a {other:?} node is not an identifier")),
        }
    }

    pub fn is_id(&self) -> bool {
        matches!(self.kind, Kind::Id(_))
    }

    pub fn call(&self) -> &CallData {
        match &self.kind {
            Kind::Call(c) | Kind::FuncCall(c) | Kind::ArrayAccess(c) | Kind::ArrayLoad(c) => c,
            other => crate::pyval::crash(&format!("AttributeError: a {other:?} node is not a call")),
        }
    }

    pub fn call_mut(&mut self) -> &mut CallData {
        match &mut self.kind {
            Kind::Call(c) | Kind::FuncCall(c) | Kind::ArrayAccess(c) | Kind::ArrayLoad(c) => c,
            other => crate::pyval::crash(&format!("AttributeError: a {other:?} node is not a call")),
        }
    }
}

impl IdData {
    /// The var or array part of the ref, for the code that reads `byref`,
    /// `bank`, `default_value` and so on from either.
    pub fn var(&self) -> Option<&VarRef> {
        match &self.r.kind {
            RefKind::Var(v) | RefKind::Array(v) => Some(v),
            _ => None,
        }
    }

    pub fn var_mut(&mut self) -> Option<&mut VarRef> {
        match &mut self.r.kind {
            RefKind::Var(v) | RefKind::Array(v) => Some(v),
            _ => None,
        }
    }

    pub fn func(&self) -> Option<&FuncRef> {
        match &self.r.kind {
            RefKind::Func(f) => Some(f),
            _ => None,
        }
    }

    pub fn func_mut(&mut self) -> Option<&mut FuncRef> {
        match &mut self.r.kind {
            RefKind::Func(f) => Some(f),
            _ => None,
        }
    }

    pub fn label(&self) -> Option<&LabelRef> {
        match &self.r.kind {
            RefKind::Label(l) => Some(l),
            _ => None,
        }
    }

    pub fn label_mut(&mut self) -> Option<&mut LabelRef> {
        match &mut self.r.kind {
            RefKind::Label(l) => Some(l),
            _ => None,
        }
    }

    /// `getattr(ref, "bank", 0)`.
    pub fn bank(&self) -> i64 {
        match &self.r.kind {
            RefKind::Var(v) | RefKind::Array(v) => v.bank,
            RefKind::Func(f) => f.bank,
            RefKind::Label(l) => l.bank,
            RefKind::Unknown | RefKind::Const(_) => 0,
        }
    }

    pub fn class(&self) -> Class {
        match &self.r.kind {
            RefKind::Unknown => Class::Unknown,
            RefKind::Var(_) => Class::Var,
            RefKind::Const(_) => Class::Const,
            RefKind::Label(_) => Class::Label,
            RefKind::Func(f) => f.class,
            RefKind::Array(_) => Class::Array,
        }
    }

    pub fn token(&self) -> &'static str {
        match &self.r.kind {
            RefKind::Unknown => "ID",
            RefKind::Var(_) => "VAR",
            RefKind::Const(_) => "CONST",
            RefKind::Label(_) => "LABEL",
            RefKind::Func(_) => "FUNCTION",
            RefKind::Array(_) => "VARARRAY",
        }
    }
}
