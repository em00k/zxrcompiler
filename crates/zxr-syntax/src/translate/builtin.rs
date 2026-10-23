//! `unary_op_translator.py` and `builtin_translator.py`: the sub-visitors
//! the Translator hands UNARY and BUILTIN nodes to. A `yield` inside them
//! goes back to the translator that called, so their operands are visited by
//! it.

use super::inst::tsuffix;
use super::runtime;
use crate::ast::*;
use crate::compiler::*;
use crate::pyval::crash;
use crate::types::BasicType;

impl Compiler {
    /// `Translator.visit_UNARY`, and the `UnaryOpTranslator` method it picks
    /// by the operator: `-a`, `NOT a`, `bNOT a` or `@a`.
    pub(super) fn tr_visit_unary(&mut self, node: NodeId) {
        let Kind::Unary { operator } = &self.node(node).kind else { unreachable!() };
        let operator = operator.clone();
        let operand = self.kid(node, 0);
        match operator.as_str() {
            "MINUS" => {
                self.tr_visit(Some(operand));
                let ty = self.tyb(node);
                let (t, t1) = (self.t(node), self.t(operand));
                self.ic_neg(ty, &t, &t1);
            }
            "NOT" => {
                self.tr_visit(Some(operand));
                let ty = self.tyb(operand);
                let (t, t1) = (self.t(node), self.t(operand));
                self.ic_not(ty, &t, &t1);
            }
            "BNOT" => {
                self.tr_visit(Some(operand));
                let ty = self.tyb(operand);
                let (t, t1) = (self.t(node), self.t(operand));
                self.ic_bnot(ty, &t, &t1);
            }
            "ADDRESS" => self.tr_visit_address(node, operand),
            // The sub-visitor is asked for `visit_<operator>` and it has the
            // methods of every translator, so these two names are found. An
            // operator is never either.
            "BLOCK" => self.tr_visit_block(node),
            other => crash(&format!("InvalidOperatorError: Invalid operator \"{other}\"")),
        }
    }

    /// `UnaryOpTranslator.visit_ADDRESS`.
    fn tr_visit_address(&mut self, node: NodeId, operand: NodeId) {
        let scope = self.scope(operand);
        if self.token(operand) == "ARRAYACCESS" {
            self.tr_visit(Some(operand));
            // The address of an array element.
            let entry = self.entry(operand);
            let t = self.t(node);
            match scope {
                Scope::Global => {
                    let mangled = self.id(entry).mangled.clone();
                    self.ic_aaddr(&t, &mangled);
                }
                Scope::Parameter => self.ic_paaddr(&t, &format!("*{}", self.offset_str(entry))),
                Scope::Local => self.ic_paaddr(&t, &(-self.offset(entry)).to_string()),
            }
            return;
        }
        // A scalar variable.
        match scope {
            Scope::Global => {
                let ty = self.tyb(node);
                let (t, t1) = (self.t(node), self.t(operand));
                self.ic_load(ty, &t, &format!("#{t1}"));
            }
            Scope::Parameter => {
                let address = self.offset(operand) + self.ty_of(operand).size() % 2;
                let t = self.t(node);
                self.ic_paddr(&address.to_string(), &t);
            }
            Scope::Local => {
                let address = -self.offset(operand);
                let t = self.t(node);
                self.ic_paddr(&address.to_string(), &t);
            }
        }
    }

    /// `Translator.visit_BUILTIN`, and the `BuiltinTranslator` method it
    /// picks by the function's name: `LEN(a$)`, `SIN(x)` and so on.
    pub(super) fn tr_visit_builtin(&mut self, node: NodeId) {
        let operand = self.child(node, 0);
        self.tr_visit(operand);
        let Kind::Builtin { fname, discard_result } = &self.node(node).kind else { unreachable!() };
        let (fname, discard_result) = (fname.clone(), *discard_result);
        // `node.operand`, where the method goes on to use it.
        let op = |c: &Compiler| operand.unwrap_or_else(|| crash(&format!("AttributeError: 'NoneType' object has no attribute 't' ({})", c.token(node))));
        match fname.as_str() {
            // The string functions.
            "INKEY" => self.runtime_call(runtime::INKEY, BasicType::String.size()),
            "IN" => {
                let t = self.t(self.kid(node, 0));
                self.ic_in(&t);
            }
            "CODE" | "VAL" => {
                let operand = op(self);
                let t = self.t(operand);
                self.ic_fparam(PTR_TYPE, &t);
                // What is not a variable must be freed.
                let free = !matches!(self.token(operand), "STRING" | "VAR") && self.t(operand) != "_";
                self.ic_fparam(BasicType::UByte, if free { "1" } else { "0" });
                if fname == "CODE" {
                    self.runtime_call(runtime::ASC, BasicType::UByte.size()); // a char code is expected
                } else {
                    self.runtime_call(runtime::VAL, self.ty_of(node).size());
                }
            }
            "CHR" => {
                // The number of arguments.
                let count = self.children(op(self)).len();
                self.ic_fparam(STR_INDEX_TYPE, &count.to_string());
                self.runtime_call(runtime::CHR, self.size(node));
            }
            "STR" => {
                let t = self.t(self.kid(node, 0));
                self.ic_fparam(BasicType::Float, &t);
                self.runtime_call(runtime::STR_FAST, self.ty_of(node).size());
            }
            "LEN" => {
                let (t, t1) = (self.t(node), self.t(op(self)));
                self.ic_lenstr(&t, &t1);
            }
            "ABS" => {
                let operand = self.kid(node, 0);
                let ty = self.tyb(operand);
                let (t, t1) = (self.t(node), self.t(operand));
                self.ic_abs(ty, &t, &t1);
            }
            // A function of no parameters.
            "RND" => self.runtime_call(runtime::RND, BasicType::Float.size()),
            "PEEK" => {
                let operand = op(self);
                let ty = self.tyb(node);
                let t = self.t(node);
                // A constant address is immediate whether it was written as a
                // number or as the name of a CONST; one whose value is a
                // link-time expression already carries its own "#".
                let t1 = self.t(operand);
                let constant = matches!(self.token(operand), "NUMBER" | "CONST") && !t1.starts_with('#');
                let prefix = if constant { "*#" } else { "*" };
                self.ic_load(ty, &t, &format!("{prefix}{t1}"));
            }
            // The mathematical functions.
            "SIN" | "COS" | "TAN" | "ASN" | "ACS" | "ATN" | "EXP" | "LN" | "SQR" => {
                let operand = op(self);
                let (ty, t) = (self.tyb(operand), self.t(operand));
                self.ic_fparam(ty, &t);
                let label = match fname.as_str() {
                    "SIN" => runtime::SIN,
                    "COS" => runtime::COS,
                    "TAN" => runtime::TAN,
                    "ASN" => runtime::ASN,
                    "ACS" => runtime::ACS,
                    "ATN" => runtime::ATN,
                    "EXP" => runtime::EXP,
                    "LN" => runtime::LN,
                    _ => runtime::SQR,
                };
                self.runtime_call(label, self.size(node));
            }
            "SGN" => {
                let operand = op(self);
                let ty = self.tyb(operand);
                let s = tsuffix(ty);
                let t = self.t(operand);
                self.ic_fparam(ty, &t);
                let label = match s {
                    "i8" => runtime::SGNI8,
                    "u8" => runtime::SGNU8,
                    "i16" => runtime::SGNI16,
                    "u16" => runtime::SGNU16,
                    "i32" => runtime::SGNI32,
                    "u32" => runtime::SGNU32,
                    "f16" => runtime::SGNF16,
                    "f" => runtime::SGNF,
                    other => crash(&format!("KeyError: '{other}'")),
                };
                self.runtime_call(label, self.size(node));
            }
            "LBOUND" | "UBOUND" => {
                let dimension = self.kid(node, 1);
                self.tr_visit(Some(dimension));
                let t = self.t(dimension);
                self.ic_param(BOUND_TYPE, &t);
                let entry = self.kid(node, 0);
                match self.scope(entry) {
                    Scope::Global => {
                        let mangled = self.id(entry).mangled.clone();
                        self.ic_fparam(PTR_TYPE, &format!("#{mangled}"));
                    }
                    Scope::Parameter => {
                        let t = self.t(entry);
                        self.ic_pload(PTR_TYPE, &t, &self.offset_str(entry));
                        let t1 = self.new_t();
                        self.ic_fparam(PTR_TYPE, &t1);
                    }
                    Scope::Local => {
                        let address = -self.offset(entry);
                        let t = self.t(entry);
                        self.ic_paddr(&address.to_string(), &t);
                        let t1 = self.new_t();
                        self.ic_fparam(PTR_TYPE, &t1);
                    }
                }
                self.runtime_call(if fname == "LBOUND" { runtime::LBOUND } else { runtime::UBOUND }, BOUND_TYPE.size());
            }
            "USR_STR" => {
                let t = self.t(self.kid(node, 0));
                self.ic_fparam(BasicType::String, &t);
                self.runtime_call(runtime::USR_STR, self.ty_of(node).size());
            }
            // A machine code call from BASIC.
            "USR" => {
                let t = self.t(self.kid(node, 0));
                self.ic_fparam(PTR_TYPE, &t);
                self.runtime_call(runtime::USR, self.ty_of(node).size());
            }
            "BLOCK" => self.tr_visit_block(node),
            other => crash(&format!("InvalidBuiltinFunctionError: Invalid BUILTIN function '{other}'")),
        }
        if discard_result {
            let ty = self.tyb(node);
            let t = self.new_t();
            self.ic_fparam(ty, &t);
        }
    }
}
