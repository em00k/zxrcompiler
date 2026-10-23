//! The translation of the tree to intermediate code:
//! `src/arch/z80/visitor/`. Quads are appended to `Compiler::memory`.
//!
//! The Python translators are visitors written as generators: a `yield node`
//! hands the node to the visitor's loop, which visits it before the generator
//! goes on. Here a `yield` is a call to [`Compiler::tr_visit`], made at the
//! same point, so that temporaries, labels and quads come out in the same
//! order. As elsewhere each `visit_*` is the Python method of that name.
//!
//! - `inst.rs`: `translator_inst_visitor.py`, the `ic_*` emitters
//! - this file: `translator_visitor.py` (TranslatorVisitor), the helpers the
//!   translators take from `backend/codebank.py`, `api/fp.py` and `beep.py`
//! - `translator.rs`: `translator.py` (Translator)
//! - `builtin.rs`: `builtin_translator.py` and `unary_op_translator.py`
//! - `function.rs`: `function_translator.py` (FunctionTranslator)
//! - `vars.rs`: `var_translator.py` (VarTranslator)
//! - `runtime.rs`: the runtime labels and the library files they need

mod builtin;
mod function;
pub mod inst;
pub mod runtime;
mod translator;
mod vars;

use std::rc::Rc;
use crate::ast::*;
use crate::compiler::*;
use crate::pyval::crash;
use crate::types::{BasicType, Num, Ty};
use inst::{tsuffix, Item};

/// Which of the Python translator classes is at work.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Mode {
    /// `Translator`: the main body.
    #[default]
    Main,
    /// `FunctionTranslator`.
    Function,
    /// `VarTranslator`.
    Var,
}

/// `LabelledData`.
#[derive(Clone, Debug)]
pub struct LabelledData {
    pub label: String,
    pub data: Vec<String>,
}

/// The state the translator classes keep: `LOOPS` and `JUMP_TABLES` of the
/// class, and what an instance of `FunctionTranslator` or `VarTranslator`
/// holds.
#[derive(Debug, Default)]
pub struct State {
    /// `LOOPS`: kind, exit label and continue label of each open loop.
    pub loops: Vec<(&'static str, String, String)>,
    /// `JUMP_TABLES`: the label of each ON GOTO table and the labels in it.
    pub jump_tables: Vec<(String, Vec<NodeId>)>,
    pub mode: Mode,
    /// `VarTranslator.bank`.
    pub bank: i64,
    /// `FunctionTranslator._deferred_bound_tables`.
    pub deferred_bound_tables: Vec<LabelledData>,
    /// `FunctionTranslator._deferred_resident`.
    pub deferred_resident: Vec<NodeId>,
}

/// `ATTR_TMP`: the tokens of an attribute set for one statement.
const ATTR_TMP: [&str; 8] = ["INK_TMP", "PAPER_TMP", "BRIGHT_TMP", "FLASH_TMP", "OVER_TMP", "INVERSE_TMP", "BOLD_TMP", "ITALIC_TMP"];

/// `DATA_TYPES`: the type code of a DATA item.
fn data_type_code(suffix: &str) -> i128 {
    match suffix {
        "str" => 1,
        "i8" => 2,
        "u8" => 3,
        "i16" => 4,
        "u16" => 5,
        "i32" => 6,
        "u32" => 7,
        "f16" => 8,
        "f" => 9,
        other => crash(&format!("KeyError: '{other}'")),
    }
}

/// `fp.immediate_float(x)`: the five bytes of a ZX floating point number, as
/// the values `C`, `DE` and `HL` are loaded with (`0XXh`, `0XXXXh`).
pub fn immediate_float(x: f64) -> (String, String, String) {
    if !x.is_finite() {
        crash("OverflowError: cannot convert float infinity to integer");
    }
    let mut e: i64 = 0;
    let mut m = x.abs();
    while m >= 1.0 {
        m /= 2.0;
        e += 1;
    }
    while 0.0 < m && m < 0.5 {
        m *= 2.0;
        e -= 1;
    }
    // bindec32(m)[3:]: the bits of the mantissa after the first.
    let mut bits = Vec::with_capacity(32);
    let mut a = m;
    let mut c = a.trunc();
    for _ in 0..32 {
        a -= c;
        a *= 2.0;
        c = a.trunc();
        bits.push(c as u32 & 1);
    }
    let mut mantissa: u32 = (x < 0.0) as u32;
    for bit in &bits[1..] {
        mantissa = mantissa << 1 | bit;
    }
    let exponent = if x != 0.0 { ((e + 128) & 0xFF) as u32 } else { 0 };
    let byte = |n: u32| (mantissa >> (24 - 8 * n)) & 0xFF;
    (format!("0{exponent:02X}h"), format!("0{:02X}{:02X}h", byte(1), byte(0)), format!("0{:02X}{:02X}h", byte(3), byte(2)))
}

/// `beep.getDEHL(duration, pitch)`: the values the ROM's beeper routine
/// takes in DE and HL.
pub fn get_dehl(duration: f64, pitch: f64) -> (i128, i128) {
    const TABLE: [f64; 12] = [
        261.625565290,
        277.182631135,
        293.664768100,
        311.126983881,
        329.627557039,
        349.228231549,
        369.994422674,
        391.995436072,
        415.304697513,
        440.000000000,
        466.163761616,
        493.883301378,
    ];
    let int_pitch = pitch.trunc();
    let fract_pitch = pitch - int_pitch;
    let tmp = 1.0 + 0.0577622606 * fract_pitch;
    if !(-60.0..=127.0).contains(&int_pitch) {
        crash("BeepError: Pitch out of range: must be between [-60, 127]");
    }
    if !(0.0..=10.0).contains(&duration) {
        crash("BeepError: Invalid duration: must be between [0, 10]");
    }
    let a = int_pitch as i64 + 60;
    let b = -5 + (a as f64 / 12.0).trunc() as i32;
    let frec = TABLE[(a % 12) as usize];
    let tmp2 = tmp * frec;
    let f = tmp2 * 2.0f64.powi(b);
    let de = (0.5 + f * duration - 1.0).trunc() as i128;
    let hl = (0.5 + 437500.0 / f - 30.125).trunc() as i128;
    (de, hl)
}

impl Compiler {
    // ------------------------------------------------------------------
    // What frontend.rs calls, in the order of main() in zxbc.py
    // ------------------------------------------------------------------

    /// `Translator(backend).visit(ast)`: the main body. Function bodies are
    /// queued in `self.functions`.
    pub fn translate_program(&mut self, ast: NodeId) {
        self.tr.mode = Mode::Main;
        self.tr_visit(Some(ast));
    }

    /// `FunctionTranslator(backend, gl.FUNCTIONS).start()`.
    pub fn translate_functions(&mut self) {
        if self.functions.iter().any(|&f| self.token(f) != "FUNCTION") {
            crash("AssertionError: all(x.token == \"FUNCTION\" for x in function_list)");
        }
        self.tr.mode = Mode::Function;
        self.tr.deferred_bound_tables.clear();
        self.tr.deferred_resident.clear();
        self.function_translator_start();
        self.tr.mode = Mode::Main;
    }

    /// `VarTranslator(backend, bank).visit(data_ast)`: the storage of the
    /// globals of one bank; bank 0 is the resident program.
    pub fn translate_vars(&mut self, data_ast: NodeId, bank: i64) {
        let saved = (self.tr.mode, self.tr.bank);
        self.tr.mode = Mode::Var;
        self.tr.bank = bank;
        self.tr_visit(Some(data_ast));
        (self.tr.mode, self.tr.bank) = saved;
    }

    // ------------------------------------------------------------------
    // Reading the tree the way the translators do
    // ------------------------------------------------------------------

    /// `node.children[n]`, where the translator goes on to use it.
    pub(crate) fn kid(&self, node: NodeId, n: usize) -> NodeId {
        match self.node(node).children.get(n) {
            Some(Some(child)) => *child,
            Some(None) => crash("AttributeError: 'NoneType' object has no attribute 't'"),
            None => crash("IndexError: list index out of range"),
        }
    }

    /// The basic type of a node's `type_`.
    pub(crate) fn tyb(&self, node: NodeId) -> BasicType {
        match self.ty(node) {
            Some(ty) => ty.basic,
            None => crash("AssertionError: isinstance(type_, sym.TYPE) or TYPE.is_valid(type_)"),
        }
    }

    /// `node.offset` of an identifier: `None` until it has been laid out.
    pub(crate) fn offset_opt(&self, id: NodeId) -> Option<i64> {
        self.id(id).r.offset
    }

    /// `node.offset` of an identifier where arithmetic is done with it.
    pub(crate) fn offset(&self, id: NodeId) -> i64 {
        self.offset_opt(id).unwrap_or_else(|| crash("TypeError: unsupported operand type(s): 'NoneType' and 'int'"))
    }

    /// `str(node.offset)`.
    pub(crate) fn offset_str(&self, id: NodeId) -> String {
        self.offset_opt(id).map_or("None".to_string(), |o| o.to_string())
    }

    /// `node.byref` of an identifier.
    pub(crate) fn byref(&self, id: NodeId) -> bool {
        match self.id(id).var() {
            Some(var) => var.byref,
            None => crash(&format!("AttributeError: '{}' object has no attribute 'byref'", self.token(id))),
        }
    }

    /// `"%i" % value`: a float loses its fraction.
    pub(crate) fn percent_i(value: Num) -> String {
        match value {
            Num::Int(i) => i.to_string(),
            Num::Float(f) => (f.trunc() as i128).to_string(),
        }
    }

    /// `entry.default_value` of an array: the nested lists of its values.
    /// The one place the translators read it.
    pub(crate) fn array_init(&self, entry: NodeId) -> Option<ConstVector> {
        self.id(entry).var().and_then(|v| v.default_vector.clone())
    }

    /// `node.eol` of a PRINT sentence.
    pub(crate) fn print_eol(&self, node: NodeId) -> bool {
        self.node(node).eol
    }

    // ------------------------------------------------------------------
    // backend/codebank.py, as far as the translators use it
    // ------------------------------------------------------------------

    /// `codebank.bank_of`: the code bank of a function, 0 for none.
    pub fn bank_of_func(&self, func: NodeId) -> i64 {
        self.bank_of(func)
    }

    /// `codebank.banked_label`: the label of the body of a banked routine,
    /// as opposed to its trampoline.
    pub fn banked_label(mangled: &str) -> String {
        format!("{mangled}{FAR_BODY_SUFFIX}")
    }

    /// `codebank.is_banking_active`.
    pub fn is_banking_active(&self) -> bool {
        !self.far_trampolines.is_empty() || !self.banks_with_data.is_empty()
    }

    /// `codebank.diverted_bank`: the bank a module-level ASM or LABEL node is
    /// compiled into, 0 if none.
    pub fn diverted_bank(&self, node: NodeId) -> i64 {
        match &self.node(node).kind {
            Kind::Asm { bank, .. } => *bank,
            Kind::Id(data) if data.label().is_some() => data.bank(),
            _ => 0,
        }
    }

    /// `codebank.collect_if_diverted`: parks a node for replay into its
    /// bank. True when the caller must not emit it where it stands.
    pub fn collect_if_diverted(&mut self, node: NodeId) -> bool {
        let bank = self.diverted_bank(node);
        if bank == 0 {
            return false;
        }
        if self.diverted_seen.insert(node) {
            match self.diverted.iter_mut().find(|(b, _)| *b == bank) {
                Some(entry) => entry.1.push(node),
                None => self.diverted.push((bank, vec![node])),
            }
        }
        true
    }

    /// `codebank.warn_if_no_routines`: a bank that holds data but nothing
    /// that will ever page it in. Once for each bank.
    pub fn warn_if_no_routines(&mut self, bank: i64, lineno: u32, fname: Option<&str>) {
        if self.far_memory_used || self.banks_used.contains(&bank) || self.no_routine_warned.contains(&bank) {
            return;
        }
        self.no_routine_warned.insert(bank);
        self.warning_codebank_has_no_routines(lineno, bank, fname);
    }

    // ------------------------------------------------------------------
    // TranslatorVisitor
    // ------------------------------------------------------------------

    /// `TranslatorVisitor.runtime_call`: a call to a routine of the runtime
    /// library, which the program then requires.
    pub fn runtime_call(&mut self, label: &str, num: i64) {
        self.ic_call(label, num);
        if let Some(module) = runtime::required_module(label) {
            self.requires.insert(module.to_string());
        }
    }

    /// `TranslatorVisitor.norm_attr`: normalizes the attribute state.
    pub(crate) fn norm_attr(&mut self) {
        self.runtime_call(runtime::COPY_ATTR, 0);
    }

    /// `TranslatorVisitor.check_attr`: the temporary attributes of a drawing
    /// statement, if it has any.
    pub(crate) fn check_attr(&self, node: NodeId, n: usize) -> Option<NodeId> {
        if self.children(node).len() > n { self.child(node, n) } else { None }
    }

    /// `visit(node)` of whichever translator is at work: what a `yield node`
    /// in a Python visitor comes to. A node that is `None` is skipped.
    pub(crate) fn tr_visit(&mut self, node: Option<NodeId>) {
        let Some(node) = node else { return };
        let token = self.token(node).to_string();
        if ATTR_TMP.contains(&token.as_str()) {
            return self.tr_visit_attr_tmp(node, &token);
        }
        // For debug data: what a statement emits is of its line, and what
        // follows the statements inside it is of its line again.
        let outer = self.origin;
        if self.debug_lines {
            let lineno = self.lineno(node);
            let filename = match &self.node(node).kind {
                Kind::Sentence { filename, .. } => Some(Rc::clone(filename)),
                Kind::Call(call) => Some(Rc::clone(&call.filename)),
                // A routine: its entry and exit are of the line it is declared on.
                Kind::Id(id) if token == "FUNCTION" => Some(Rc::clone(&id.filename)),
                _ => None,
            };
            if let Some(filename) = filename.filter(|_| lineno != 0) {
                self.origin = self.origin_of(&filename, lineno);
            }
        }
        match self.tr.mode {
            Mode::Var => self.var_visit(node, &token),
            Mode::Main | Mode::Function => self.translator_visit(node, &token),
        }
        self.origin = outer;
    }

    /// `NodeVisitor.generic_visit`.
    pub(crate) fn no_visit_method(&self, token: &str) -> ! {
        crash(&format!("RuntimeError: No visit_{token}() method defined"))
    }

    /// `visit_BLOCK`.
    pub(crate) fn tr_visit_block(&mut self, node: NodeId) {
        // The children are walked as they are when each is reached.
        let mut i = 0;
        while i < self.children(node).len() {
            let child = self.children(node)[i];
            self.tr_visit(child);
            i += 1;
        }
    }

    /// `visit_ATTR_TMP`: any temporary attribute.
    fn tr_visit_attr_tmp(&mut self, node: NodeId, token: &str) {
        let child = self.kid(node, 0);
        self.tr_visit(Some(child));
        let (ty, t) = (self.tyb(child), self.t(child));
        self.ic_fparam(ty, &t);
        let label = match token {
            "INK_TMP" => runtime::INK_TMP,
            "PAPER_TMP" => runtime::PAPER_TMP,
            "FLASH_TMP" => runtime::FLASH_TMP,
            "BRIGHT_TMP" => runtime::BRIGHT_TMP,
            "INVERSE_TMP" => runtime::INVERSE_TMP,
            "OVER_TMP" => runtime::OVER_TMP,
            "BOLD_TMP" => runtime::BOLD_TMP,
            _ => runtime::ITALIC_TMP,
        };
        self.runtime_call(label, 0); // a procedure call: the result is dropped
    }

    /// `emit_data_blocks`: the DATA blocks READ takes its values from. Runs
    /// before `emit_strings`, as it may add strings.
    pub fn emit_data_blocks(&mut self) {
        if !self.data_is_used || self.datas.is_empty() {
            return;
        }
        for data_ref in self.datas.clone() {
            let label = self.t(data_ref.label);
            self.ic_label(&label);
            for d in data_ref.datas {
                if matches!(self.node(d).kind, Kind::FuncDecl) {
                    let code = data_type_code(tsuffix(self.tyb(d))) | 0x80;
                    self.ic_data(BasicType::Byte, &[Item::Str(format!("{code:02X}h"))]);
                    let mangled = self.id(self.entry(d)).mangled.clone();
                    self.ic_data(PTR_TYPE, &[Item::Str(mangled)]);
                    continue;
                }
                let value = self.child(d, 0).unwrap_or_else(|| crash("AttributeError: 'NoneType' object has no attribute 'type_'"));
                let ty = self.tyb(value);
                self.ic_data(BasicType::Byte, &[Item::Int(data_type_code(tsuffix(ty)))]);
                if ty == BasicType::String {
                    let string = self.value(value);
                    let label = self.add_string_label(string.as_str());
                    self.ic_data(PTR_TYPE, &[Item::Str(label)]);
                } else if ty == BasicType::Fixed {
                    let bytes = Self::fixed_bits(self.num_value(value));
                    self.ic_data(BasicType::UInteger, &[Item::Str(format!("0x{:04X}", bytes & 0xFFFF)), Item::Str(format!("0x{:04X}", bytes >> 16))]);
                } else {
                    let text = self.traverse_const(value);
                    self.ic_data(ty, &[text.into()]);
                }
            }
        }
        // A label a RESTORE names beyond the last DATA line. A set of
        // strings in Python, walked in sorted order.
        let declared: Vec<String> = self.datas.iter().map(|d| self.id(d.label).name.clone()).collect();
        let mut missing: Vec<String> = self.data_labels_required.iter().filter(|l| !declared.contains(l)).cloned().collect();
        missing.sort();
        missing.dedup();
        for label in missing {
            self.ic_label(&label);
        }
        self.ic_vard("__DATA__END", &["00".into()]);
    }

    /// `emit_strings`: the constant strings, each under its label.
    pub fn emit_strings(&mut self) {
        let strings: Vec<(String, String)> = self.string_labels.iter().map(|(s, l)| (s.to_string(), l.clone())).collect();
        for (string, label) in strings {
            let mut data = vec![Item::Str(format!("{:04X}", string.chars().count() & 0xFFFF))];
            data.extend(string.chars().map(|c| Item::Str(format!("{:02X}", c as u32))));
            self.ic_vard(&label, &data);
        }
    }

    /// `emit_jump_tables`: the tables of ON GOTO and ON GOSUB.
    pub fn emit_jump_tables(&mut self) {
        for (label, addresses) in self.tr.jump_tables.clone() {
            let mut data = vec![Item::Str(format!("#{}", addresses.len()))];
            data.extend(addresses.iter().map(|&x| Item::Str(format!("##{}", self.id(x).mangled))));
            self.ic_vard(&label, &data);
        }
    }

    /// `0xFFFFFFFF & int(value * 2**16)`: a fixed point number's 32 bits.
    fn fixed_bits(value: Num) -> i128 {
        let scaled = match value {
            Num::Int(i) => i * 65536,
            Num::Float(f) => (f * 65536.0).trunc() as i128,
        };
        scaled & 0xFFFF_FFFF
    }

    /// `Translator.default_value(type_, expr)`: the bytes of an initial
    /// value, each as two hexadecimal digits, or expressions for the
    /// assembler where the value is only known to it.
    pub fn default_value(&mut self, type_: Ty, expr: NodeId) -> Vec<Item> {
        if !self.is_static(&[expr]) {
            crash("AssertionError: check.is_static(expr)");
        }
        if matches!(self.token(expr), "CONSTEXPR" | "CONST") {
            // A constant expression like @label + 1.
            if type_ == BasicType::Float || type_ == BasicType::String {
                let lineno = self.lineno(expr);
                self.error(lineno, &format!("Can't convert non-numeric value to {} at compile time", type_.name()));
                return vec!["<ERROR>".into()]; // a placeholder, so that the compilation goes on
            }
            let val = self.traverse_const(expr).unwrap_or_else(|| "None".to_string());
            let expr_size = self.ty_of(expr).size();
            if type_.size() == 1 {
                return vec![if expr_size != 1 { format!("#({val}) & 0xFF") } else { format!("#{val}") }.into()];
            }
            if type_.size() == 2 {
                return vec![if expr_size != 2 { format!("##({val}) & 0xFFFF") } else { format!("##{val}") }.into()];
            }
            if type_ == BasicType::Fixed {
                return vec!["0000".into(), format!("##({val}) & 0xFFFF").into()];
            }
            return vec![format!("##({val}) & 0xFFFF").into(), format!("##(({val}) >> 16) & 0xFFFF").into()];
        }
        let number = self.num_value(expr);
        if type_ == BasicType::Float {
            let (c, de, hl) = immediate_float(number.as_f64());
            // Without the 'h', and the leading zero.
            let c = &c[c.len() - 3..c.len() - 1];
            let de = &de[de.len() - 5..de.len() - 1];
            let hl = &hl[hl.len() - 5..hl.len() - 1];
            return vec![c.into(), de[2..].into(), de[..2].into(), hl[2..].into(), hl[..2].into()];
        }
        let value = if type_ == BasicType::Fixed {
            Self::fixed_bits(number)
        } else {
            match number {
                Num::Int(i) => i,
                Num::Float(f) => f.trunc() as i128,
            }
        };
        [value, value >> 8, value >> 16, value >> 24].iter().take(type_.size() as usize).map(|v| Item::Str(format!("{:02X}", v & 0xFF))).collect()
    }

    /// `Translator.array_default_value(type_, values)`: the bytes of an
    /// array's initial values, row after row.
    pub fn array_default_value(&mut self, type_: Ty, values: &ConstVector) -> Vec<Item> {
        match values {
            ConstVector::Elem(node) => self.default_value(type_, *node),
            ConstVector::List(rows) => {
                let mut bytes = Vec::new();
                for row in rows {
                    bytes.extend(self.array_default_value(type_, row));
                }
                bytes
            }
        }
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

    // Expected values are what the Python functions return.
    #[test]
    fn immediate_floats() {
        let f = |x: f64| {
            let (c, de, hl) = immediate_float(x);
            format!("{c} {de} {hl}")
        };
        assert_eq!(f(0.0), "000h 00000h 00000h");
        assert_eq!(f(1.0), "081h 00000h 00000h");
        assert_eq!(f(-1.0), "081h 00080h 00000h");
        assert_eq!(f(0.5), "080h 00000h 00000h");
        assert_eq!(f(3.14159), "082h 00F49h 080CFh");
        assert_eq!(f(65536.0), "091h 00000h 00000h");
        assert_eq!(f(-0.1), "07Dh 0CCCCh 0CCCCh");
        assert_eq!(f(1e10), "0A2h 00215h 000F9h");
    }

    #[test]
    fn beeps() {
        assert_eq!(get_dehl(1.0, 0.0), (261, 1642));
        assert_eq!(get_dehl(5.0, 0.0), (1307, 1642));
        assert_eq!(get_dehl(1.5, 15.0), (932, 673));
        assert_eq!(get_dehl(0.25, -12.5), (31, 3414));
    }

    #[test]
    fn default_values() {
        let mut c = compiler();
        let mut bytes = |c: &mut Compiler, basic: BasicType, value: Num| {
            let n = c.new_number(value, 1, None);
            inst::list_str(&c.default_value(Ty::new(basic), n))
        };
        assert_eq!(bytes(&mut c, BasicType::UByte, Num::Int(200)), "['C8']");
        assert_eq!(bytes(&mut c, BasicType::Integer, Num::Int(-2)), "['FE', 'FF']");
        assert_eq!(bytes(&mut c, BasicType::ULong, Num::Int(0x12345678)), "['78', '56', '34', '12']");
        assert_eq!(bytes(&mut c, BasicType::Fixed, Num::Float(1.5)), "['00', '80', '01', '00']");
        assert_eq!(bytes(&mut c, BasicType::Fixed, Num::Float(-1.5)), "['00', '80', 'FE', 'FF']");
        assert_eq!(bytes(&mut c, BasicType::Float, Num::Float(3.14159)), "['82', '49', '0F', 'CF', '80']");
        assert_eq!(bytes(&mut c, BasicType::Float, Num::Int(-1)), "['81', '80', '00', '00', '00']");
    }
}
