//! The emitters of intermediate code: `translator_inst_visitor.py`. One
//! `ic_*` method for each instruction, named as in Python. Operands are
//! text, as a quad holds them: what `str()` gives for the Python argument.

use crate::compiler::Compiler;
use crate::pyval::crash;
use crate::types::BasicType;
use zxr_ir::Quad;

/// `TranslatorInstVisitor.TSUFFIX`: the suffix of the typed instructions.
pub fn tsuffix(type_: BasicType) -> &'static str {
    match type_ {
        BasicType::Byte => "i8",
        BasicType::UByte => "u8",
        BasicType::Integer => "i16",
        BasicType::UInteger => "u16",
        BasicType::Long => "i32",
        BasicType::ULong => "u32",
        BasicType::Fixed => "f16",
        BasicType::Float => "f",
        BasicType::String => "str",
        BasicType::Boolean => "bool",
        BasicType::Unknown => crash("KeyError: <TYPE.unknown: 0>"),
    }
}

/// `_no_bool`: the suffix, with a boolean taken as a byte.
pub fn no_bool(type_: BasicType) -> &'static str {
    match tsuffix(type_) {
        "bool" => "u8",
        other => other,
    }
}

/// An element of a Python list given as an operand.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Str(String),
    Int(i128),
    None,
}

impl From<String> for Item {
    fn from(s: String) -> Item {
        Item::Str(s)
    }
}

impl From<&str> for Item {
    fn from(s: &str) -> Item {
        Item::Str(s.to_string())
    }
}

impl From<Option<String>> for Item {
    fn from(s: Option<String>) -> Item {
        s.map_or(Item::None, Item::Str)
    }
}

/// `str(list)`: the list as Python prints it.
pub fn list_str(items: &[Item]) -> String {
    let parts: Vec<String> = items
        .iter()
        .map(|item| match item {
            Item::Str(s) => zxr_core::pynum::str_repr(s),
            Item::Int(i) => i.to_string(),
            Item::None => "None".to_string(),
        })
        .collect();
    format!("[{}]", parts.join(", "))
}

/// A list of strings as an operand.
pub fn strs(items: &[String]) -> String {
    let items: Vec<Item> = items.iter().map(|s| Item::Str(s.clone())).collect();
    list_str(&items)
}

impl Compiler {
    /// `emit(*args)`: appends a quad to the backend's memory.
    pub fn emit(&mut self, op: &str, args: &[&str]) {
        let mut quad = Quad::new(op, args);
        quad.origin = self.origin;
        self.memory.push(quad);
    }

    pub fn ic_add(&mut self, type_: BasicType, t: &str, t1: &str, t2: &str) {
        self.emit(&format!("add{}", tsuffix(type_)), &[t, t1, t2]);
    }

    pub fn ic_band(&mut self, type_: BasicType, t: &str, t1: &str, t2: &str) {
        self.emit(&format!("band{}", tsuffix(type_)), &[t, t1, t2]);
    }

    pub fn ic_bor(&mut self, type_: BasicType, t: &str, t1: &str, t2: &str) {
        self.emit(&format!("bor{}", tsuffix(type_)), &[t, t1, t2]);
    }

    pub fn ic_bxor(&mut self, type_: BasicType, t: &str, t1: &str, t2: &str) {
        self.emit(&format!("bxor{}", tsuffix(type_)), &[t, t1, t2]);
    }

    pub fn ic_div(&mut self, type_: BasicType, t: &str, t1: &str, t2: &str) {
        self.emit(&format!("div{}", tsuffix(type_)), &[t, t1, t2]);
    }

    pub fn ic_eq(&mut self, type_: BasicType, t: &str, t1: &str, t2: &str) {
        self.emit(&format!("eq{}", tsuffix(type_)), &[t, t1, t2]);
    }

    pub fn ic_ge(&mut self, type_: BasicType, t: &str, t1: &str, t2: &str) {
        self.emit(&format!("ge{}", tsuffix(type_)), &[t, t1, t2]);
    }

    pub fn ic_gt(&mut self, type_: BasicType, t: &str, t1: &str, t2: &str) {
        self.emit(&format!("gt{}", tsuffix(type_)), &[t, t1, t2]);
    }

    pub fn ic_le(&mut self, type_: BasicType, t: &str, t1: &str, t2: &str) {
        self.emit(&format!("le{}", tsuffix(type_)), &[t, t1, t2]);
    }

    pub fn ic_lt(&mut self, type_: BasicType, t: &str, t1: &str, t2: &str) {
        self.emit(&format!("lt{}", tsuffix(type_)), &[t, t1, t2]);
    }

    pub fn ic_mod(&mut self, type_: BasicType, t: &str, t1: &str, t2: &str) {
        self.emit(&format!("mod{}", tsuffix(type_)), &[t, t1, t2]);
    }

    pub fn ic_mul(&mut self, type_: BasicType, t: &str, t1: &str, t2: &str) {
        self.emit(&format!("mul{}", tsuffix(type_)), &[t, t1, t2]);
    }

    pub fn ic_ne(&mut self, type_: BasicType, t: &str, t1: &str, t2: &str) {
        self.emit(&format!("ne{}", tsuffix(type_)), &[t, t1, t2]);
    }

    pub fn ic_pow(&mut self, type_: BasicType, t: &str, t1: &str, t2: &str) {
        self.emit(&format!("pow{}", tsuffix(type_)), &[t, t1, t2]);
    }

    pub fn ic_shl(&mut self, type_: BasicType, t: &str, t1: &str, t2: &str) {
        self.emit(&format!("shl{}", tsuffix(type_)), &[t, t1, t2]);
    }

    pub fn ic_shr(&mut self, type_: BasicType, t: &str, t1: &str, t2: &str) {
        self.emit(&format!("shr{}", tsuffix(type_)), &[t, t1, t2]);
    }

    pub fn ic_sub(&mut self, type_: BasicType, t: &str, t1: &str, t2: &str) {
        self.emit(&format!("sub{}", tsuffix(type_)), &[t, t1, t2]);
    }

    pub fn ic_and(&mut self, type_: BasicType, t: &str, t1: &str, t2: &str) {
        self.emit(&format!("and{}", no_bool(type_)), &[t, t1, t2]);
    }

    pub fn ic_or(&mut self, type_: BasicType, t: &str, t1: &str, t2: &str) {
        self.emit(&format!("or{}", no_bool(type_)), &[t, t1, t2]);
    }

    pub fn ic_xor(&mut self, type_: BasicType, t: &str, t1: &str, t2: &str) {
        self.emit(&format!("xor{}", no_bool(type_)), &[t, t1, t2]);
    }

    pub fn ic_abs(&mut self, type_: BasicType, t1: &str, t2: &str) {
        self.emit(&format!("abs{}", tsuffix(type_)), &[t1, t2]);
    }

    pub fn ic_bnot(&mut self, type_: BasicType, t1: &str, t2: &str) {
        self.emit(&format!("bnot{}", tsuffix(type_)), &[t1, t2]);
    }

    pub fn ic_neg(&mut self, type_: BasicType, t1: &str, t2: &str) {
        self.emit(&format!("neg{}", tsuffix(type_)), &[t1, t2]);
    }

    pub fn ic_not(&mut self, type_: BasicType, t1: &str, t2: &str) {
        self.emit(&format!("not{}", no_bool(type_)), &[t1, t2]);
    }

    pub fn ic_aload(&mut self, type_: BasicType, t1: &str, t2: &str) {
        self.emit(&format!("aload{}", tsuffix(type_)), &[t1, t2]);
    }

    pub fn ic_astore(&mut self, type_: BasicType, t1: &str, t2: &str) {
        self.emit(&format!("astore{}", tsuffix(type_)), &[t1, t2]);
    }

    pub fn ic_fpload(&mut self, type_: BasicType, t1: &str, t2: &str) {
        self.emit(&format!("fpload{}", tsuffix(type_)), &[t1, t2]);
    }

    pub fn ic_load(&mut self, type_: BasicType, t1: &str, t2: &str) {
        self.emit(&format!("load{}", tsuffix(type_)), &[t1, t2]);
    }

    pub fn ic_paload(&mut self, type_: BasicType, t1: &str, t2: &str) {
        self.emit(&format!("paload{}", tsuffix(type_)), &[t1, t2]);
    }

    pub fn ic_pastore(&mut self, type_: BasicType, t1: &str, t2: &str) {
        self.emit(&format!("pastore{}", tsuffix(type_)), &[t1, t2]);
    }

    pub fn ic_pload(&mut self, type_: BasicType, t1: &str, t2: &str) {
        self.emit(&format!("pload{}", tsuffix(type_)), &[t1, t2]);
    }

    pub fn ic_pstore(&mut self, type_: BasicType, t1: &str, t2: &str) {
        self.emit(&format!("pstore{}", tsuffix(type_)), &[t1, t2]);
    }

    pub fn ic_store(&mut self, type_: BasicType, t1: &str, t2: &str) {
        self.emit(&format!("store{}", tsuffix(type_)), &[t1, t2]);
    }

    pub fn ic_ret(&mut self, type_: BasicType, t1: &str, t2: &str) {
        self.emit(&format!("ret{}", tsuffix(type_)), &[t1, t2]);
    }

    pub fn ic_jgezero(&mut self, type_: BasicType, t: &str, label: &str) {
        self.emit(&format!("jgezero{}", no_bool(type_)), &[t, label]);
    }

    pub fn ic_jnzero(&mut self, type_: BasicType, t: &str, label: &str) {
        self.emit(&format!("jnzero{}", no_bool(type_)), &[t, label]);
    }

    pub fn ic_jzero(&mut self, type_: BasicType, t: &str, label: &str) {
        self.emit(&format!("jzero{}", no_bool(type_)), &[t, label]);
    }

    pub fn ic_fparam(&mut self, type_: BasicType, t: &str) {
        self.emit(&format!("fparam{}", no_bool(type_)), &[t]);
    }

    pub fn ic_param(&mut self, type_: BasicType, t: &str) {
        self.emit(&format!("param{}", tsuffix(type_)), &[t]);
    }

    pub fn ic_aaddr(&mut self, t1: &str, t2: &str) {
        self.emit("aaddr", &[t1, t2]);
    }

    pub fn ic_lenstr(&mut self, t1: &str, t2: &str) {
        self.emit("lenstr", &[t1, t2]);
    }

    pub fn ic_out(&mut self, t1: &str, t2: &str) {
        self.emit("out", &[t1, t2]);
    }

    pub fn ic_paaddr(&mut self, t1: &str, t2: &str) {
        self.emit("paaddr", &[t1, t2]);
    }

    pub fn ic_paddr(&mut self, t1: &str, t2: &str) {
        self.emit("paddr", &[t1, t2]);
    }

    pub fn ic_call(&mut self, label: &str, num: i64) {
        self.emit("call", &[label, &num.to_string()]);
    }

    pub fn ic_cast(&mut self, t1: &str, type1: BasicType, type2: BasicType, t2: &str) {
        self.emit("cast", &[t1, tsuffix(type1), tsuffix(type2), t2]);
    }

    pub fn ic_data(&mut self, type_: BasicType, data: &[Item]) {
        self.emit("data", &[tsuffix(type_), &list_str(data)]);
    }

    pub fn ic_deflabel(&mut self, label: &str, t: &str) {
        self.emit("deflabel", &[label, t]);
    }

    pub fn ic_end(&mut self, t: &str) {
        self.emit("end", &[t]);
    }

    pub fn ic_enter(&mut self, t: &str) {
        self.emit("enter", &[t]);
    }

    pub fn ic_in(&mut self, t: &str) {
        self.emit("in", &[t]);
    }

    pub fn ic_jump(&mut self, t: &str) {
        self.emit("jump", &[t]);
    }

    pub fn ic_label(&mut self, t: &str) {
        self.emit("label", &[t]);
    }

    pub fn ic_leave(&mut self, t: &str) {
        self.emit("leave", &[t]);
    }

    pub fn ic_exchg(&mut self) {
        self.emit("exchg", &[]);
    }

    pub fn ic_nop(&mut self) {
        self.emit("nop", &[]);
    }

    pub fn ic_inline(&mut self, asm_code: &str) {
        self.emit("inline", &[asm_code]);
    }

    pub fn ic_larrd(&mut self, offset: &str, arg1: &[Item], size: &str, arg2: &[Item], bound_ptrs: &[Item]) {
        self.emit("larrd", &[offset, &list_str(arg1), size, &list_str(arg2), &list_str(bound_ptrs)]);
    }

    pub fn ic_lvard(&mut self, offset: &str, default_value: &[Item]) {
        self.emit("lvard", &[offset, &list_str(default_value)]);
    }

    pub fn ic_lvarx(&mut self, type_: BasicType, offset: &str, default_value: &[Item]) {
        self.emit("lvarx", &[offset, tsuffix(type_), &list_str(default_value)]);
    }

    pub fn ic_memcopy(&mut self, t1: &str, t2: &str, t3: &str) {
        self.emit("memcopy", &[t1, t2, t3]);
    }

    pub fn ic_return(&mut self, addr: &str) {
        self.emit("ret", &[addr]);
    }

    pub fn ic_var(&mut self, name: &str, size: &str) {
        self.emit("var", &[name, size]);
    }

    pub fn ic_vard(&mut self, name: &str, data: &[Item]) {
        self.emit("vard", &[name, &list_str(data)]);
    }

    pub fn ic_varx(&mut self, name: &str, type_: BasicType, default_value: &[Item]) {
        self.emit("varx", &[name, tsuffix(type_), &list_str(default_value)]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_print_as_python_prints_them() {
        assert_eq!(list_str(&[]), "[]");
        assert_eq!(list_str(&["00".into(), "0A".into()]), "['00', '0A']");
        assert_eq!(list_str(&[Item::Int(3), Item::None, "it's".into()]), "[3, None, \"it's\"]");
        assert_eq!(strs(&["_a.__LBOUND__".to_string(), "0".to_string()]), "['_a.__LBOUND__', '0']");
    }

    #[test]
    fn suffixes() {
        assert_eq!(tsuffix(BasicType::Fixed), "f16");
        assert_eq!(tsuffix(BasicType::Boolean), "bool");
        assert_eq!(no_bool(BasicType::Boolean), "u8");
        assert_eq!(no_bool(BasicType::String), "str");
    }
}
