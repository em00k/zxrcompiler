//! `optimizer/cpustate.py`: the symbolic Z80. Registers and memory cells
//! hold text: a number in decimal, a label, or an "unknown" value
//! (`*UNKNOWN_n`, or two of them joined by `|` for a 16 bit one). Two values
//! are the same when their text is.
//!
//! This is a literal port. Where the Python code compares an `int` with a
//! `str` (which is never true), or tests something that cannot fail, the
//! same thing happens here and a comment says so: the calls made on the way
//! have side effects (reading a memory cell that is not known yet gives it
//! an unknown value, and takes a number from the counter).

use crate::asm::{py_strip, Asm};
use crate::helpers::{
    self, get_H_from_unknown_value, get_L_from_unknown_value, idx_args, is_16bit_composed_register, is_8bit_oper_register, is_label, is_number, is_register, is_unknown, is_unknown16, is_unknown8,
    new_tmp_val16_from_label, to_int, try_py_int, valnum, LO16_val, TmpVals, HI16, HL_SEP, LO16,
};
use std::collections::{BTreeSet, HashMap};
use std::sync::OnceLock;
use zxr_re::{Flags, Regex};

/// `RE_OFFSET`
fn re_offset() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(^[*._a-zA-Z0-9]+(?:[+-]\d+)*)([+-]\d+)$", Flags::NONE).unwrap())
}

/// `"%s%+i" % (base, n)`
fn signed(base: &str, n: i128) -> String {
    format!("{base}{n:+}")
}

/// With `ZXR_ORACLE_OPTIMISER` in the environment, as the harness has it for
/// a comparison with the Python optimiser, its mistakes are made too. They
/// are the stores that forget nothing of what they may alias, the circular
/// rotations that leave the carry alone, and MUL setting flags.
fn as_the_oracle() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("ZXR_ORACLE_OPTIMISER").is_some())
}

/// Implements a memory representation, dealing with unknown values
#[derive(Clone, Debug, Default)]
pub struct Memory {
    pub data: HashMap<String, String>,
}

impl Memory {
    fn get_hl_addr(addr: &str) -> (String, String) {
        // A cell is known by the text of its address, and `_x - 1` is
        // `_x-1` to the pattern below only without its spaces. With them,
        // the high byte of the word read from `(_x - 1)` is a cell
        // `_x - 1+1`, which a store to `_x` does not touch: `x = 1 - x :
        // F(1 - x)` then reuses the old H. The Python optimiser keeps the
        // spaces.
        if !as_the_oracle() && addr.contains(char::is_whitespace) {
            let addr: String = addr.split_whitespace().collect();
            return Memory::get_hl_addr(&addr);
        }
        if is_number(addr) {
            return (addr.to_string(), (to_int(addr) + 1).to_string());
        }

        // Only an address that ends in a digit can match.
        if !addr.trim_end_matches('\n').ends_with(|c: char| c.is_numeric()) {
            return (addr.to_string(), format!("{addr}+1"));
        }
        let chars: Vec<char> = addr.chars().collect();
        let Some(ptr) = re_offset().match_at(&chars, 0) else {
            return (addr.to_string(), format!("{addr}+1"));
        };

        let base = ptr.group(&chars, 1).unwrap();
        let off = helpers::py_int(&ptr.group(&chars, 2).unwrap());
        if off == 0 {
            return Memory::get_hl_addr(&base);
        }

        if off == -1 {
            return (addr.to_string(), base);
        }

        (addr.to_string(), signed(&base, off + 1))
    }

    /// `self[key]`: a cell never written holds a new unknown value from
    /// then on (`__missing__`).
    pub fn item(&mut self, key: &str, tmp: &mut TmpVals) -> String {
        if let Some(value) = self.data.get(key) {
            return value.clone();
        }
        let value = tmp.new_tmp_val();
        self.data.insert(key.to_string(), value.clone());
        value
    }

    pub fn read_16_bit_value(&mut self, addr: &str, tmp: &mut TmpVals) -> String {
        let (addr_lo, addr_hi) = Memory::get_hl_addr(addr);
        let hi = self.item(&addr_hi, tmp);
        let lo = self.item(&addr_lo, tmp);
        if is_number(&hi) && is_number(&lo) {
            return (valnum(&lo).unwrap() + 256 * valnum(&hi).unwrap()).to_string();
        }

        // `get_orig_label_from_unknown16("")` is asked next, and is None.
        format!("{hi}|{lo}")
    }

    pub fn write_16_bit_value(&mut self, addr: &str, value: &str) {
        let (v_hi, v_lo);
        if is_number(value) {
            let value_ = valnum(value).unwrap();
            v_hi = ((value_ >> 8) & 0xFF).to_string();
            v_lo = (value_ & 0xFF).to_string();
        } else {
            let v_ = if is_unknown16(value) { value.to_string() } else { new_tmp_val16_from_label(value) };
            v_hi = get_H_from_unknown_value(&v_);
            v_lo = get_L_from_unknown_value(&v_);
        }

        let (addr_lo, addr_hi) = Memory::get_hl_addr(addr);
        self.forget_aliases(&[&addr_lo, &addr_hi]);
        self.data.insert(addr_lo, v_lo);
        self.data.insert(addr_hi, v_hi);
    }

    /// What is known of memory is kept by the text of an address: a number,
    /// a label, an offset from IX, or the unknown value a register held. Two
    /// of the first three are different cells when they are written
    /// differently. An unknown pointer may be the address of anything, so a
    /// store through one leaves nothing else known, and a store anywhere
    /// leaves nothing known of what unknown pointers point at.
    ///
    /// The Python optimiser has no such rule: after `ld (hl), a` it still
    /// believes what it knew of every variable, and drops the reload of one
    /// that HL was pointing at.
    fn forget_aliases(&mut self, written: &[&str]) {
        if as_the_oracle() {
            return;
        }
        let unknown = |key: &str| key.contains(crate::helpers::UNKNOWN_PREFIX);
        let anywhere = written.iter().any(|key| unknown(key));
        self.data.retain(|key, _| written.contains(&key.as_str()) || !(anywhere || unknown(key)));
    }

    pub fn read_8_bit_value(&mut self, addr: &str, tmp: &mut TmpVals) -> String {
        let (addr_lo, _) = Memory::get_hl_addr(addr);
        let lo = self.item(&addr_lo, tmp);
        if is_number(&lo) {
            return (helpers::py_int(&lo) & 0xFF).to_string();
        }

        lo
    }

    pub fn write_8_bit_value(&mut self, addr: &str, value: &str) {
        let value = if is_number(value) {
            (valnum(value).unwrap() & 0xFF).to_string()
        } else if is_unknown16(value) {
            get_L_from_unknown_value(value)
        } else if is_label(value) {
            get_L_from_unknown_value(&new_tmp_val16_from_label(value))
        } else {
            value.to_string()
        };

        let (addr_lo, _) = Memory::get_hl_addr(addr);
        self.forget_aliases(&[&addr_lo]);
        self.data.insert(addr_lo, value);
    }
}

/// A flag: unknown, or 0 or 1.
pub type Flag = Option<i128>;

/// Given an 8 bit reg. Return the 16 bit reg it belongs to.
fn reg16_of(r: &str) -> Option<&'static str> {
    Some(match r {
        "b" | "c" => "bc",
        "d" | "e" => "de",
        "h" | "l" => "hl",
        "b'" | "c'" => "bc'",
        "d'" | "e'" => "de'",
        "h'" | "l'" => "hl'",
        "ixh" | "ixl" => "ix",
        "iyh" | "iyl" => "iy",
        "a" | "f" => "af",
        "a'" | "f'" => "af'",
        _ => return None,
    })
}

/// Given a 16 bit regs, returns its 8 bits pair
fn reg8_pair(r: &str) -> (&'static str, &'static str) {
    match r {
        "af" => ("a", "f"),
        "bc" => ("b", "c"),
        "de" => ("d", "e"),
        "hl" => ("h", "l"),
        "af'" => ("a'", "f'"),
        "bc'" => ("b'", "c'"),
        "de'" => ("d'", "e'"),
        "hl'" => ("h'", "l'"),
        "ix" => ("ixh", "ixl"),
        "iy" => ("iyh", "iyl"),
        _ => panic!("KeyError: '{r}'"),
    }
}

/// A class storing registers value information (CPU State).
#[derive(Clone, Debug, Default)]
pub struct CpuState {
    pub mem: Memory,
    pub stack: Vec<String>,
    pub regs: HashMap<String, String>,
    c: Flag,
    z: Flag,
    p: Flag,
    s: Flag,
    /// Memory for IX / IY accesses
    pub ix_ptr: BTreeSet<(String, String, String)>,
    /// The ZX Spectrum Next's `CPUState`, which knows `mul`.
    pub zxnext: bool,
}

fn check_flag(val: Flag) {
    assert!(matches!(val, None | Some(0) | Some(1)), "AssertionError");
}

impl CpuState {
    pub fn new(zxnext: bool, tmp: &mut TmpVals) -> CpuState {
        let mut cpu = CpuState { zxnext, ..Default::default() };
        cpu.reset(None, None, tmp);
        cpu
    }

    /// `self.regs[r]`
    pub fn reg(&self, r: &str) -> &str {
        self.regs.get(r).unwrap_or_else(|| panic!("KeyError: '{r}'"))
    }

    fn put(&mut self, r: &str, val: String) {
        self.regs.insert(r.to_string(), val);
    }

    /// Sets a flag, and tries to update the F register accordingly. If the
    /// F register is unknown, sets it with a new unknown value.
    fn flag_to_f(&mut self, val: Flag, mask: i128, shift: u32, tmp: &mut TmpVals) {
        check_flag(val);
        match val {
            Some(val) if is_number(self.reg("f")) => {
                let f = self.getv(Some("f"), tmp).expect("TypeError: unsupported operand type(s) for &: 'NoneType' and 'int'");
                self.put("f", ((f & mask) | (val << shift)).to_string());
            }
            _ => {
                let v = tmp.new_tmp_val();
                self.put("f", v);
            }
        }
    }

    /// The Z flag
    pub fn z(&self) -> Flag {
        self.z
    }

    pub fn set_z(&mut self, val: Flag, tmp: &mut TmpVals) {
        check_flag(val);
        self.z = val;
        self.flag_to_f(val, 0xBF, 6, tmp);
    }

    /// The C flag
    pub fn c(&self) -> Flag {
        self.c
    }

    pub fn set_c(&mut self, val: Flag, tmp: &mut TmpVals) {
        check_flag(val);
        self.c = val;
        self.flag_to_f(val, 0xFE, 0, tmp);
    }

    /// The P flag
    pub fn p(&self) -> Flag {
        self.p
    }

    pub fn set_p(&mut self, val: Flag, tmp: &mut TmpVals) {
        check_flag(val);
        self.p = val;
        self.flag_to_f(val, 0xFB, 2, tmp);
    }

    /// The S flag
    pub fn s(&self) -> Flag {
        self.s
    }

    pub fn set_s(&mut self, val: Flag, tmp: &mut TmpVals) {
        check_flag(val);
        self.s = val;
        self.flag_to_f(val, 0x7F, 7, tmp);
    }

    /// Initial state
    pub fn reset(&mut self, regs: Option<&HashMap<String, String>>, mems: Option<&HashMap<String, String>>, tmp: &mut TmpVals) {
        self.regs = HashMap::with_capacity(40);
        self.stack = Vec::new();
        self.mem = Memory::default(); // dict of label -> value in memory
        (self.c, self.z, self.p, self.s) = (None, None, None, None);

        // Memory for IX / IY accesses
        self.ix_ptr = BTreeSet::new();

        for i in ["a", "b", "c", "d", "e", "f", "h", "l"] {
            let v = tmp.new_tmp_val(); // Initial unknown state
            self.put(i, v);
            let v = tmp.new_tmp_val();
            self.put(&format!("{i}'"), v);
        }

        for i in ["ixh", "ixl", "iyh", "iyl", "sp", "r", "i"] {
            let v = tmp.new_tmp_val();
            self.put(i, v);
        }

        for i in ["af", "bc", "de", "hl"] {
            let v = format!("{}{HL_SEP}{}", self.reg(&i[..1]), self.reg(&i[1..]));
            self.put(i, v);
            let v = format!("{}{HL_SEP}{}", self.reg(&format!("{}'", &i[..1])), self.reg(&format!("{}'", &i[1..])));
            self.put(&format!("{i}'"), v);
        }

        let v = format!("{}{HL_SEP}{}", self.reg("ixh"), self.reg("ixl"));
        self.put("ix", v);
        let v = format!("{}{HL_SEP}{}", self.reg("iyh"), self.reg("iyl"));
        self.put("iy", v);

        if let Some(regs) = regs {
            self.regs.extend(regs.iter().map(|(k, v)| (k.clone(), v.clone())));
        }
        if let Some(mems) = mems {
            self.mem.data.extend(mems.iter().map(|(k, v)| (k.clone(), v.clone())));

            for key_ in mems.keys() {
                if let Some(idx) = idx_args(key_) {
                    self.ix_ptr.insert(idx);
                }
            }
        }

        self.reset_flags(tmp);
    }

    /// Resets flags to an "unknown state"
    pub fn reset_flags(&mut self, tmp: &mut TmpVals) {
        self.set_c(None, tmp);
        self.set_z(None, tmp);
        self.set_p(None, tmp);
        self.set_s(None, tmp);
    }

    /// For the given ix/iy, remove all references of it in memory, which
    /// are not in the form ix/iy +/- n
    pub fn clear_idx_reg_refs(&mut self, r: &str) {
        let r = r.to_lowercase();
        assert!(r == "ix" || r == "iy", "AssertionError");

        for k in self.ix_ptr.clone() {
            if k.0 != r {
                continue;
            }

            if !is_number(&k.2) {
                let key = format!("{}{}{}", k.0, k.1, k.2);
                if self.mem.data.remove(&key).is_none() {
                    panic!("KeyError: '{key}'");
                }
                self.ix_ptr.remove(&k);
            }
        }
    }

    /// Given an idx register r (ix / iy) and an offset, all the references
    /// in memory will be shifted the given amount. I.e. for 'ix', 1
    /// (ix + 1) will contain (ix + 2), and so on. (ix + 127) will contain
    /// an unknown8 value.
    ///
    /// The same applies for negative offsets. For iy - 2 (iy + 2) will
    /// contain current (iy + 0); (iy - 126) and lower will contain random
    /// unknown8.
    pub fn shift_idx_regs_refs(&mut self, r: &str, offset: i128, tmp: &mut TmpVals) {
        if offset == 0 {
            return;
        }

        self.clear_idx_reg_refs(r);
        let r = r.to_lowercase();
        let mut step = |cpu: &mut CpuState, i: i128, out: bool| {
            let idx = signed(&r, i);
            let old_idx = signed(&r, offset + i);
            let value = if out { tmp.new_tmp_val() } else { cpu.mem.read_8_bit_value(&old_idx, tmp) };
            cpu.mem.write_8_bit_value(&idx, &value);
        };
        if offset > 0 {
            for i in -128..128 {
                step(self, i, offset + i > 127);
            }
        } else {
            for i in (-128..128).rev() {
                step(self, i, offset + i < -128);
            }
        }
    }

    /// `set` with a number.
    pub fn set_int(&mut self, r: &str, val: i128, tmp: &mut TmpVals) {
        self.set(r, Some(&val.to_string()), tmp);
    }

    pub fn set(&mut self, r: &str, val: Option<&str>, tmp: &mut TmpVals) {
        let orig_val = val;
        let val = self.get(val, tmp);
        let is_num = val.as_deref().is_some_and(is_number);

        let mut val = match val {
            None => tmp.new_tmp_val16(),
            Some(val) => val,
        };

        if is_num {
            val = (valnum(&val).unwrap() & 0xFFFF).to_string();
        }

        // `if self.getv(r) == val: return`: a number is never equal to
        // text, but the register or the memory cell has been read.
        self.getv(Some(r), tmp);

        if r == "(sp)" {
            if self.stack.is_empty() {
                self.stack = vec![tmp.new_tmp_val16()];
            }

            *self.stack.last_mut().unwrap() = val;
            return;
        }

        if matches!(r, "(hl)" | "(bc)" | "(de)") {
            // ld (bc|de|hl), val
            let r = self.reg(&r[1..r.len() - 1]).to_string();
            self.mem.write_8_bit_value(&r, &val);
            return;
        }

        if r.is_empty() {
            panic!("IndexError: string index out of range");
        }
        if r.starts_with('(') {
            // (mem) <- r  => store in memory address
            let inner: String = {
                let chars: Vec<char> = r.chars().collect();
                chars[1..chars.len().max(2) - 1].iter().collect()
            };
            let r = py_strip(&inner);
            if let Some(idx) = idx_args(r) {
                let r = format!("{}{}{}", idx.0, idx.1, idx.2);
                self.ix_ptr.insert(idx);
                let val = LO16_val(&val, tmp);
                self.mem.write_8_bit_value(&r, &val);
                return;
            }

            let orig_val = orig_val.unwrap_or_else(|| panic!("AttributeError: 'NoneType' object has no attribute 'lower'"));
            if is_8bit_oper_register(orig_val) {
                self.mem.write_8_bit_value(r, &val);
                return;
            }

            if is_unknown8(&val) {
                val = format!("{}{HL_SEP}{val}", tmp.new_tmp_val());
            }

            self.mem.write_16_bit_value(r, &val);
            return;
        }

        if is_8bit_oper_register(r) {
            // `oldval = self.getv(r)`, compared below with text.
            self.getv(Some(r), tmp);
            let val = if is_num { (valnum(&val).unwrap() & 0xFF).to_string() } else { get_L_from_unknown_value(&val) };

            self.put(r, val);
            let Some(hl) = reg16_of(r) else { return };

            let (h, l) = reg8_pair(hl);
            let (h_, l_) = (self.reg(h).to_string(), self.reg(l).to_string());
            if is_number(&h_) && is_number(&l_) {
                self.put(hl, ((valnum(&h_).unwrap() << 8) | valnum(&l_).unwrap()).to_string());
                return;
            }

            self.put(hl, format!("{h_}{HL_SEP}{l_}"));
            return;
        }

        // a 16 bit reg
        assert!(self.regs.contains_key(r), "AssertionError");

        if is_unknown8(&val) {
            val = format!("{}{HL_SEP}{val}", tmp.new_tmp_val());
        }
        assert!(is_num || is_unknown16(&val) || is_label(&val), "AssertionError: val '{val}' is neither a number, nor a label nor an unknown16");

        self.put(r, val.clone());
        if is_16bit_composed_register(r) {
            // sp register is not included. Special case
            if !is_num {
                if is_label(&val) {
                    val = new_tmp_val16_from_label(&val);
                }
                let parts: Vec<&str> = val.split(HL_SEP).collect();
                if parts.len() != 2 {
                    panic!("ValueError: not enough values to unpack (expected 2, got {})", parts.len());
                }
                let (hi, lo) = (parts[0].to_string(), parts[1].to_string());
                self.put(&HI16(r), hi);
                self.put(&LO16(r), lo);
            } else {
                let val = valnum(&val).unwrap();
                self.put(&HI16(r), (val >> 8).to_string());
                self.put(&LO16(r), (val & 0xFF).to_string());
            }

            if r.contains('f') {
                self.reset_flags(tmp);
            }
        }
    }

    /// Returns precomputed value of the given expression
    pub fn get(&mut self, r: Option<&str>, tmp: &mut TmpVals) -> Option<String> {
        let r = r?;
        let lower = r.to_lowercase();

        if lower == "(sp)" && !self.stack.is_empty() {
            return self.stack.last().cloned();
        }

        if matches!(lower.as_str(), "(hl)" | "(bc)" | "(de)") {
            let i = self.reg(&lower[1..lower.len() - 1]).to_string();
            return Some(self.mem.read_8_bit_value(&i, tmp));
        }

        if r.starts_with('(') {
            let chars: Vec<char> = r.chars().collect();
            let inner: String = if chars.len() >= 2 { chars[1..chars.len() - 1].iter().collect() } else { String::new() };
            let v_ = py_strip(&inner);
            if let Some(idx) = idx_args(v_) {
                let v_ = format!("{}{}{}", idx.0, idx.1, idx.2);
                self.ix_ptr.insert(idx);
                return Some(self.mem.read_8_bit_value(&v_, tmp));
            }

            return Some(self.mem.read_16_bit_value(v_, tmp));
        }

        if is_number(r) {
            return Some(valnum(r).unwrap().to_string());
        }

        if is_unknown(r) {
            return Some(r.to_string());
        }

        if !is_register(&lower) {
            // If it's not a register, it *must* be a label
            return Some(r.to_string());
        }

        Some(self.reg(&lower).to_string())
    }

    /// Like the above, but returns the <int> value or None.
    pub fn getv(&mut self, r: Option<&str>, tmp: &mut TmpVals) -> Option<i128> {
        let v = self.get(r, tmp)?;
        if is_unknown(&v) {
            return None;
        }
        try_py_int(&v)
    }

    /// True if values of r1 and r2 registers are equal
    pub fn eq(&self, r1: &str, r2: &str) -> bool {
        if !is_register(r1) || !is_register(r2) {
            return false;
        }

        self.reg(r1) == self.reg(r2)
    }

    pub fn set_flag(&mut self, val: Option<&str>, tmp: &mut TmpVals) {
        let Some(val) = val.filter(|v| is_number(v)) else {
            let v = tmp.new_tmp_val();
            self.put("f", v);
            self.reset_flags(tmp);
            return;
        };

        self.set("f", Some(val), tmp);
        let val = valnum(val).unwrap();
        self.set_c(Some(val & 1), tmp);
        self.set_p(Some((val >> 2) & 1), tmp);
        self.set_z(Some((val >> 6) & 1), tmp);
        self.set_s(Some((val >> 7) & 1), tmp);
    }

    /// `self.mem.write_8_bit_value(self.get(r_), str(v_))` of inc and dec
    /// on a memory operand: `r_` is the operand without its first and last
    /// character.
    fn write_through(&mut self, r: &str, value: &str, tmp: &mut TmpVals) {
        let chars: Vec<char> = r.chars().collect();
        let r_: String = if chars.len() >= 2 { chars[1..chars.len() - 1].iter().collect() } else { String::new() };
        let addr = self.get(Some(&r_), tmp).unwrap();
        self.mem.write_8_bit_value(&addr, value);
    }

    /// Does inc on the register and precomputes flags
    pub fn inc(&mut self, r: &str, tmp: &mut TmpVals) {
        if !is_register(r) {
            if r.is_empty() {
                panic!("IndexError: string index out of range");
            }
            if r.starts_with('(') {
                // a memory position, basically: inc(hl)
                let value = match self.getv(Some(r), tmp) {
                    Some(v_) => {
                        let v_ = (v_ + 1) & 0xFF;
                        self.set_z(Some((v_ == 0) as i128), tmp); // HINT: This might be improved
                        self.set_c(Some((v_ == 0) as i128), tmp);
                        v_.to_string()
                    }
                    None => {
                        let v_ = tmp.new_tmp_val();
                        self.set_flag(None, tmp);
                        v_
                    }
                };

                self.write_through(r, &value, tmp);
            }
            return;
        }

        match self.getv(Some(r), tmp) {
            Some(_) => {
                let v = self.getv(Some(r), tmp).unwrap() + 1;
                self.set_int(r, v, tmp);
            }
            None => self.set(r, None, tmp),
        }

        if r == "ix" || r == "iy" {
            self.shift_idx_regs_refs(r, 1, tmp);
        }

        if !is_8bit_oper_register(r) {
            // INC does not affect flag for 16bit regs
            return;
        }

        if is_unknown(self.reg(r)) {
            self.set_flag(None, tmp);
            return;
        }

        let v_ = self.getv(Some(r), tmp);
        self.set_z(Some((v_ == Some(0)) as i128), tmp);
        self.set_c(Some((v_ == Some(0)) as i128), tmp);
    }

    /// Does dec on the register and precomputes flags
    pub fn dec(&mut self, r: &str, tmp: &mut TmpVals) {
        if !is_register(r) {
            let value = match self.getv(Some(r), tmp) {
                Some(v_) => {
                    let v_ = (v_ - 1) & 0xFF;
                    self.set_z(Some((v_ == 0) as i128), tmp); // HINT: This might be improved
                    self.set_c(Some((v_ == 0xFF) as i128), tmp);
                    v_.to_string()
                }
                None => {
                    let v_ = tmp.new_tmp_val();
                    self.set_flag(None, tmp);
                    v_
                }
            };

            self.write_through(r, &value, tmp);
            return;
        }

        match self.getv(Some(r), tmp) {
            Some(_) => {
                let v = self.getv(Some(r), tmp).unwrap() - 1;
                self.set_int(r, v, tmp);
            }
            None => self.set(r, None, tmp),
        }

        if r == "ix" || r == "iy" {
            self.shift_idx_regs_refs(r, -1, tmp);
        }

        if !is_8bit_oper_register(r) {
            // DEC does not affect flag for 16bit regs
            return;
        }

        if is_unknown(self.reg(r)) {
            self.set_flag(None, tmp);
            return;
        }

        let v_ = self.getv(Some(r), tmp);
        self.set_z(Some((v_ == Some(0)) as i128), tmp);
        self.set_c(Some((v_ == Some(0xFF)) as i128), tmp);
    }

    /// `self.getv(self.regs[r])` where the Python code goes on to do
    /// arithmetic with it.
    fn regv(&mut self, r: &str, tmp: &mut TmpVals) -> i128 {
        let text = self.reg(r).to_string();
        self.getv(Some(&text), tmp).unwrap_or_else(|| panic!("TypeError: unsupported operand type(s): 'NoneType' and 'int'"))
    }

    /// Does a ROTATION to the RIGHT |>>
    pub fn rrc(&mut self, r: &str, tmp: &mut TmpVals) {
        if !is_number(self.reg(r)) {
            self.set(r, None, tmp);
            self.set_flag(None, tmp);
            return;
        }

        let v_ = self.regv(r, tmp) & 0xFF;
        self.put(r, ((v_ >> 1) | ((v_ & 1) << 7)).to_string());
        // The bit that went round is the carry too. The Python optimiser
        // leaves the carry as it was.
        if !as_the_oracle() {
            self.set_c(Some(v_ & 1), tmp);
        }
    }

    /// Like the above, bus uses carry
    pub fn rr(&mut self, r: &str, tmp: &mut TmpVals) {
        if self.c.is_none() || !is_number(self.reg(r)) {
            self.set(r, None, tmp);
            self.set_flag(None, tmp);
            return;
        }

        let carry = self.c.unwrap();
        self.rrc(r, tmp);
        let carry = if as_the_oracle() { self.c.unwrap() } else { carry };
        let v_ = self.regv(r, tmp);
        self.set_c(Some(v_ >> 7), tmp);
        self.put(r, ((v_ & 0x7F) | (carry << 7)).to_string());
    }

    /// Does a ROTATION to the LEFT <<|
    pub fn rlc(&mut self, r: &str, tmp: &mut TmpVals) {
        if !is_number(self.reg(r)) {
            self.set(r, None, tmp);
            self.set_flag(None, tmp);
            return;
        }

        let v_ = self.regv(r, tmp) & 0xFF;
        self.set_int(r, ((v_ << 1) & 0xFF) | (v_ >> 7), tmp);
        if !as_the_oracle() {
            self.set_c(Some(v_ >> 7), tmp);
        }
    }

    /// Like the above, bus uses carry
    pub fn rl(&mut self, r: &str, tmp: &mut TmpVals) {
        if self.c.is_none() || !is_number(self.reg(r)) {
            self.set(r, None, tmp);
            self.set_flag(None, tmp);
            return;
        }

        let carry = self.c.unwrap();
        self.rlc(r, tmp);
        let carry = if as_the_oracle() { self.c.unwrap() } else { carry };
        let v_ = self.regv(r, tmp);
        self.set_c(Some(v_ & 1), tmp);
        self.put(r, ((v_ & 0xFE) | carry).to_string());
    }

    /// True if value of r is val.
    pub fn is(&mut self, r: &str, val: Option<&str>, tmp: &mut TmpVals) -> bool {
        let Some(val) = val else { return false };
        if !is_register(r) {
            return false;
        }

        let r = r.to_lowercase();
        if is_register(val) {
            return self.eq(&r, val);
        }

        let mut val = if is_number(val) { valnum(val).unwrap().to_string() } else { val.to_string() };

        if val.starts_with('(') {
            let chars: Vec<char> = val.chars().collect();
            let inner: String = if chars.len() >= 2 { chars[1..chars.len() - 1].iter().collect() } else { String::new() };
            val = self.mem.item(&inner, tmp);
        }

        self.reg(&r) == val
    }

    /// Tries to update the registers values with the given asm line.
    pub fn execute(&mut self, asm_code: &str, tmp: &mut TmpVals) {
        // The line is taken apart once, for the Next's own instructions and
        // for the rest alike.
        let asm_ = Asm::new(asm_code);
        if asm_.is_label {
            return;
        }

        if self.zxnext {
            // The ZX Spectrum Next's `CPUState.execute`.
            let asm = &asm_;

            if asm.inst == "mul" {
                let o = &asm.oper;
                let operand = |n: usize| o.get(n).map(String::as_str).unwrap_or_else(|| panic!("IndexError: list index out of range"));
                let val_d = self.getv(Some(operand(0)), tmp);
                let val_e = self.getv(Some(operand(1)), tmp);
                if let (Some(val_d), Some(val_e)) = (val_d, val_e) {
                    // MUL leaves the flags alone. The Python optimiser sets
                    // Z, C and S from the product.
                    let val = val_d * val_e;
                    self.set_int("de", val, tmp);
                    if as_the_oracle() {
                        self.set_z(Some((val == 0) as i128), tmp);
                        self.set_c(Some((val < 0) as i128), tmp);
                        self.set_s(Some((val < 0) as i128), tmp);
                    }
                    return;
                }
            }
        }

        let i = asm_.inst.as_str();
        let mut o = asm_.oper.clone();

        for op in o.iter_mut() {
            if is_register(op) {
                *op = op.to_lowercase();
            }
        }
        let at = |o: &[String], n: usize| -> String { o.get(n).cloned().unwrap_or_else(|| panic!("IndexError: list index out of range")) };

        match i {
            "ld" => {
                let (dst, src) = (at(&o, 0), at(&o, 1));
                self.set(&dst, Some(&src), tmp);
            }
            "push" => {
                if valnum(self.reg("sp")).is_some() {
                    let sp = self.regv("sp", tmp);
                    self.set_int("sp", (sp - 2).rem_euclid(0xFFFF), tmp);
                } else {
                    self.set("sp", None, tmp);
                }
                let value = self.reg(&at(&o, 0)).to_string();
                self.stack.push(value);
            }
            "pop" => {
                // `self.stack and self.stack.pop() or None`
                let value = self.stack.pop().filter(|v| !v.is_empty());
                self.set(&at(&o, 0), value.as_deref(), tmp);
                // A stack pointer known to be 0 is not "known" here.
                if valnum(self.reg("sp")).is_some_and(|v| v != 0) {
                    let sp = self.regv("sp", tmp);
                    self.set_int("sp", (sp + 2).rem_euclid(0xFFFF), tmp);
                } else {
                    self.set("sp", None, tmp);
                }
            }
            "inc" => self.inc(&at(&o, 0), tmp),
            "dec" => self.dec(&at(&o, 0), tmp),
            "rra" => self.rr("a", tmp),
            "rla" => self.rl("a", tmp),
            "rlca" => self.rlc("a", tmp),
            "rrca" => self.rrc("a", tmp),
            "rr" => self.rr(&at(&o, 0), tmp),
            "rl" => self.rl(&at(&o, 0), tmp),
            "exx" => {
                for j in ["bc", "de", "hl", "b", "c", "d", "e", "h", "l"] {
                    self.swap(j, &format!("{j}'"));
                }
            }
            "ex" => {
                if o == ["de", "hl"] {
                    for (a, b) in [("de", "hl"), ("d", "h"), ("e", "l")] {
                        self.swap(a, b);
                    }
                } else {
                    // Whatever else is exchanged: `ex (sp), hl` too.
                    for j in ["af", "a", "f"] {
                        self.swap(j, &format!("{j}'"));
                    }
                }
            }
            "xor" => {
                self.set_c(Some(0), tmp);
                let o0 = at(&o, 0);

                if o0 == "a" {
                    self.set_int("a", 0, tmp);
                    self.set_z(Some(1), tmp);
                    return;
                }

                if self.getv(Some("a"), tmp).is_none() || self.getv(Some(&o0), tmp).is_none() {
                    self.set_z(None, tmp);
                    self.set("a", None, tmp);
                    return;
                }

                let v = self.getv(Some("a"), tmp).unwrap() ^ self.getv(Some(&o0), tmp).unwrap();
                self.set_int("a", v, tmp);
                // `int(self.get("a") == 0)`: text is never 0.
                self.get(Some("a"), tmp);
                self.set_z(Some(0), tmp);
            }
            "or" | "and" => {
                self.set_c(Some(0), tmp);
                let o0 = at(&o, 0);

                if self.getv(Some("a"), tmp).is_none() || self.getv(Some(&o0), tmp).is_none() {
                    self.set_z(None, tmp);
                    self.set("a", None, tmp);
                    return;
                }

                let (a, b) = (self.getv(Some("a"), tmp).unwrap(), self.getv(Some(&o0), tmp).unwrap());
                self.set_int("a", if i == "or" { a | b } else { a & b }, tmp);

                self.get(Some("a"), tmp);
                self.set_z(Some(0), tmp);
            }
            "adc" | "sbc" => {
                if o.len() == 1 {
                    o = vec!["a".to_string(), o[0].clone()];
                }
                let (o0, o1) = (at(&o, 0), at(&o, 1));

                let Some(carry) = self.c else {
                    self.set_z(None, tmp);
                    self.set(&o0, None, tmp);
                    return;
                };

                if i == "sbc" && o0 == o1 {
                    self.set_z(Some((carry == 0) as i128), tmp);
                    self.set_int(&o0, -carry, tmp);
                    return;
                }

                if self.getv(Some(&o0), tmp).is_none() || self.getv(Some(&o1), tmp).is_none() {
                    self.set_flag(None, tmp);
                    self.set(&o0, None, tmp);
                    return;
                }

                let (a, b) = (self.getv(Some(&o0), tmp).unwrap(), self.getv(Some(&o1), tmp).unwrap());
                // Setting a flag can change what is known of F, and the
                // carry is read again after each.
                if i == "adc" {
                    let val = a + b + carry;
                    if is_8bit_oper_register(&o0) {
                        self.set_c(Some((val > 0xFF) as i128), tmp);
                    } else {
                        self.set_c(Some((val > 0xFFFF) as i128), tmp);
                    }
                    self.set_int(&o0, val, tmp);
                    return;
                }

                let val = a - b - carry;
                self.set_c(Some((val < 0) as i128), tmp);
                self.set_z(Some((val == 0) as i128), tmp);
                self.set_int(&o0, val, tmp);
            }
            "add" | "sub" => {
                if o.len() == 1 {
                    o = vec!["a".to_string(), o[0].clone()];
                }
                let (o0, o1) = (at(&o, 0), at(&o, 1));

                if i == "sub" && o0 == o1 {
                    self.set_z(Some(1), tmp);
                    self.set_c(Some(0), tmp);
                    self.set_int(&o0, 0, tmp);
                    return;
                }

                // `if not is_number(self.get(o[0])) or is_number(self.get(o[1])) is not None:`
                // which is always true, the second operand being read only
                // when the first holds a number. Nothing is ever added.
                if self.get(Some(&o0), tmp).as_deref().is_some_and(is_number) {
                    self.get(Some(&o1), tmp);
                }
                self.set_flag(None, tmp);
                self.set(&o0, None, tmp);
            }
            "neg" => {
                let Some(a) = self.getv(Some("a"), tmp) else {
                    self.set("a", None, tmp);
                    self.set_flag(None, tmp);
                    return;
                };

                self.getv(Some("a"), tmp);
                let val = -a;
                self.set_int("a", val, tmp);
                self.set_z(Some((val == 0) as i128), tmp);
                self.set_c(Some((self.z == Some(0)) as i128), tmp);
                let val = val & 0xFF;
                self.set_s(Some(val >> 7), tmp);
            }
            "scf" => self.set_c(Some(1), tmp),
            "ccf" => {
                if let Some(carry) = self.c {
                    self.set_c(Some((carry == 0) as i128), tmp);
                }
            }
            "cpl" => {
                let Some(_) = self.getv(Some("a"), tmp) else {
                    self.set("a", None, tmp);
                    return;
                };

                let a = self.getv(Some("a"), tmp).unwrap();
                self.set_int("a", 0xFF ^ a, tmp);
            }
            "cp" => {
                let val = self.getv(Some(&at(&o, 0)), tmp);
                let (true, Some(val)) = (is_number(self.reg("a")), val) else {
                    self.set_flag(None, tmp);
                    return;
                };

                let val = helpers::py_int(self.reg("a")) - val;
                self.set_z(Some((val == 0) as i128), tmp);
                self.set_c(Some((val < 0) as i128), tmp);
                self.set_s(Some((val < 0) as i128), tmp);
            }
            "jp" | "jr" | "ret" | "rst" | "call" => {}
            "djnz" => {
                let Some(_) = self.getv(Some("b"), tmp) else {
                    self.set("b", None, tmp);
                    self.set_z(None, tmp);
                    return;
                };

                let val = (self.getv(Some("b"), tmp).unwrap() - 1) & 0xFF;
                self.set_int("b", val, tmp);
                self.set_z(Some((val == 0) as i128), tmp);
            }
            "out" => {}
            "in" => self.set(&at(&o, 0), None, tmp),
            // Unknown. Resets ALL
            _ => self.reset(None, None, tmp),
        }
    }

    fn swap(&mut self, a: &str, b: &str) {
        let (x, y) = (self.reg(a).to_string(), self.reg(b).to_string());
        self.put(a, y);
        self.put(b, x);
    }
}

/// `tests/arch/zx48k/optimizer/test_cpustate.py`
#[cfg(test)]
mod tests {
    use super::*;
    use crate::helpers::HI16_val;

    struct T {
        cpu: CpuState,
        tmp: TmpVals,
    }

    impl T {
        fn new() -> T {
            let mut tmp = TmpVals::default();
            T { cpu: CpuState::new(false, &mut tmp), tmp }
        }

        fn eval(&mut self, code: &str) {
            for line in code.split('\n').map(py_strip).filter(|x| !x.is_empty()) {
                self.cpu.execute(line, &mut self.tmp);
            }
        }

        fn after(code: &str) -> T {
            let mut t = T::new();
            t.eval(code);
            t
        }

        fn reg(&self, r: &str) -> String {
            self.cpu.reg(r).to_string()
        }

        fn read8(&mut self, addr: &str) -> String {
            self.cpu.mem.read_8_bit_value(addr, &mut self.tmp)
        }

        fn read16(&mut self, addr: &str) -> String {
            self.cpu.mem.read_16_bit_value(addr, &mut self.tmp)
        }

        fn mem(&self) -> Vec<(String, String)> {
            let mut items: Vec<_> = self.cpu.mem.data.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            items.sort();
            items
        }
    }

    #[test]
    fn cpu_state_mem_write_8_bit_numeric_value() {
        let mut t = T::new();
        t.cpu.mem.write_8_bit_value("_x", "257");
        assert_eq!(t.read8("_x"), "1");
    }

    #[test]
    fn cpu_state_mem_write_8_bit_unknown_value() {
        let mut t = T::new();
        t.cpu.mem.write_8_bit_value("_x", "_a");
        assert_eq!(t.read8("_x"), "*UNKNOWN_L__a");
    }

    #[test]
    fn cpu_state_mem_write_16_bit_numeric_value() {
        let mut t = T::new();
        t.cpu.mem.write_16_bit_value("_x", "257");
        assert_eq!(t.read16("_x"), "257");
    }

    #[test]
    fn cpu_state_mem_write_16_bit_unknown_value() {
        let mut t = T::new();
        t.cpu.mem.write_16_bit_value("_x", "_a");
        assert_eq!(t.read16("_x"), "*UNKNOWN_H__a|*UNKNOWN_L__a");
    }

    #[test]
    fn cpu_state_ld_a_unknown() {
        let mut t = T::after("ld a, (_N)");
        assert!(is_unknown8(&t.reg("a")));
        let n = t.read8("_N");
        assert_eq!(t.reg("a"), get_L_from_unknown_value(&n));
    }

    #[test]
    fn cpu_state_ld_unknown_a() {
        let mut t = T::after("xor a\nld (_N), a");
        assert_eq!(t.reg("a"), "0");
        assert_eq!(t.read8("_N"), "0");
        assert!(is_unknown8(&t.read8("_N+1")));
    }

    #[test]
    fn cpu_state_push_pop() {
        let t = T::after("ld hl, 256\npush hl\npop bc");
        assert!(t.cpu.stack.is_empty());
        assert_eq!(t.reg("hl"), t.reg("bc"));
        assert_eq!(t.reg("h"), t.reg("b"));
        assert_eq!(t.reg("l"), t.reg("c"));
        assert_eq!(t.reg("h"), "1");
        assert_eq!(t.reg("l"), "0");
    }

    #[test]
    fn cpu_state_push_pop_unknown() {
        let t = T::after("ld hl, (dw4)\npush hl\npop bc");
        assert!(t.cpu.stack.is_empty());
        assert_eq!(t.reg("hl"), t.reg("bc"));
        assert_eq!(t.reg("h"), t.reg("b"));
        assert_eq!(t.reg("l"), t.reg("c"));
    }

    #[test]
    fn cpu_state_ld_known_and_unknown() {
        for first in ["ld hl, 258", "ld hl, (dw4)"] {
            let t = T::after(&format!("{first}\nld b, h\nld c, l"));
            assert_eq!(t.reg("hl"), t.reg("bc"));
            assert_eq!(t.reg("h"), t.reg("b"));
            assert_eq!(t.reg("l"), t.reg("c"));
        }
    }

    #[test]
    fn cpu_state_inc16_dec16_unknown() {
        for last in ["inc hl", "dec hl"] {
            let mut t = T::new();
            let f = t.reg("f");
            t.eval(&format!("ld hl, (_dw2)\nld b, h\nld c, l\n{last}"));
            assert_ne!(t.reg("h"), t.reg("b"));
            assert_ne!(t.reg("l"), t.reg("c"));
            assert_eq!(f, t.reg("f"), "Flags should be unaffected");
        }
    }

    #[test]
    fn cpu_state_inc16_known_cz_1() {
        let mut t = T::new();
        t.cpu.set_c(Some(0), &mut t.tmp);
        t.cpu.set_z(Some(0), &mut t.tmp);
        let f = t.reg("f");
        t.eval("ld hl, 65535\ninc hl");
        assert_eq!(t.reg("h"), "0");
        assert_eq!(t.reg("l"), "0");
        assert_eq!(t.reg("hl"), "0");
        assert_eq!(f, t.reg("f"), "Flags should be unaffected");
        assert_eq!(t.cpu.c(), Some(0));
        assert_eq!(t.cpu.z(), Some(0));
    }

    #[test]
    fn cpu_state_dec16_known_cz_1() {
        let mut t = T::new();
        t.cpu.set_c(Some(0), &mut t.tmp);
        t.cpu.set_z(Some(1), &mut t.tmp);
        let f = t.reg("f");
        t.eval("ld hl, 0\ndec hl");
        assert_eq!(t.reg("h"), "255");
        assert_eq!(t.reg("l"), "255");
        assert_eq!(t.reg("hl"), "65535");
        assert_eq!(f, t.reg("f"), "Flags should be unaffected");
        assert_eq!(t.cpu.c(), Some(0));
        assert_eq!(t.cpu.z(), Some(1));
    }

    #[test]
    fn cpu_state_inc8_dec8_unknown() {
        for last in ["inc a", "dec a"] {
            let mut t = T::new();
            let f = t.reg("f");
            t.eval(&format!("ld a, (_dw2)\nld b, a\n{last}"));
            assert_ne!(t.reg("a"), t.reg("b"));
            assert_ne!(f, t.reg("f"), "Flags should be affected");
        }
    }

    #[test]
    fn cpu_state_inc8_known_cz_1() {
        let mut t = T::new();
        t.cpu.set_c(Some(0), &mut t.tmp);
        t.cpu.set_z(Some(0), &mut t.tmp);
        let f = t.reg("f");
        t.eval("ld a, 255\ninc a");
        assert_eq!(t.reg("a"), "0");
        assert_ne!(f, t.reg("f"), "Flags should be affected");
        assert_eq!(t.cpu.c(), Some(1));
        assert_eq!(t.cpu.z(), Some(1));
    }

    #[test]
    fn cpu_state_dec8_known_cz_1() {
        let mut t = T::new();
        t.cpu.set_c(Some(0), &mut t.tmp);
        t.cpu.set_z(Some(1), &mut t.tmp);
        let f = t.reg("f");
        t.eval("ld a, 0\ndec a");
        assert_eq!(t.reg("a"), "255");
        assert_ne!(f, t.reg("f"), "Flags should be affected");
        assert_eq!(t.cpu.c(), Some(1));
        assert_eq!(t.cpu.z(), Some(0));
    }

    #[test]
    fn cpu_state_ld_sp_hl() {
        let t = T::after("ld sp, hl");
        assert_eq!(t.reg("sp"), t.reg("hl"));
    }

    #[test]
    fn cpu_state_ld_mem() {
        let mut t = T::after("ld hl, 19\nld (_a), hl");
        assert_eq!(t.reg("hl"), "19");
        assert_eq!(t.read8("_a"), t.reg("hl"));
    }

    #[test]
    fn cpu_state_ld_hl_de() {
        let mut t = T::after("ld hl, 19\nld (_a), hl\nld de, (_a)");
        assert_eq!(t.reg("hl"), "19");
        assert_eq!(t.read8("_a"), t.reg("hl"));
        assert_eq!(t.read8("_a"), t.reg("de"));
    }

    #[test]
    fn cpu_state_ld_hl_a() {
        let t = T::after("ld hl, _a");
        assert_eq!(t.reg("hl"), "_a");
    }

    #[test]
    fn cpu_state_ix_plus_n() {
        let mut t = T::after("ld (ix + 1), _a\nld (IY - 2), 259");
        assert!(is_unknown8(&t.read8("ix+1")));
        assert_eq!(t.read8("iy-2"), (259 & 0xFF).to_string());
        assert!(t.cpu.mem.data.keys().all(|x| t.cpu.ix_ptr.contains(&(x[..2].to_string(), x[2..3].to_string(), x[3..].to_string()))));
    }

    #[test]
    fn cpu_state_ix_plus_n_2() {
        let mut t = T::after("ld a, (ix + 1)");
        assert!(is_unknown8(&t.read8("ix+1")));
        assert_eq!(t.read8("ix+1"), t.reg("a"));
    }

    #[test]
    fn cpu_state_ix_plus_n_rw() {
        let mut t = T::after("ld (ix + 1), 257\nld a, (ix + 1)");
        assert_eq!(t.read8("ix+1"), "1");
        assert_eq!(t.read8("ix+1"), t.reg("a"));
    }

    #[test]
    fn cpu_state_ix_plus_n_inc_rw() {
        let mut t = T::after("ld (ix + 2), 257\ninc ix\nld a, (ix + 1)");
        assert_eq!(t.read8("ix+1"), "1");
        assert_eq!(t.read8("ix+1"), t.reg("a"));
    }

    #[test]
    fn cpu_state_ix_plus_n_dec_rw() {
        let mut t = T::after("ld (ix + 2), 257\ndec ix\nld a, (ix + 3)");
        assert_eq!(t.read8("ix+3"), "1");
        assert_eq!(t.read8("ix+3"), t.reg("a"));
    }

    #[test]
    fn half_hl_unknown16() {
        let t = T::after("ld hl, 5\nld l, (_a)");
        assert!(is_unknown16(&t.reg("hl")));
    }

    #[test]
    fn half_hl_set_mem() {
        // Must not crash
        T::after("ld l, a\nld h, 0\nld (_dw3), hl\nld hl, (_dw3)");
    }

    #[test]
    fn hl_is_half_known() {
        let t = T::after("ld h, 0");
        let hl = t.reg("hl");
        let (h, l) = hl.split_once(HL_SEP).unwrap();
        assert!(is_number(h));
        assert!(is_unknown8(l));
        assert_eq!(t.reg("h"), "0");
    }

    #[test]
    fn reset_mem() {
        let mut t = T::new();
        t.cpu.mem.write_8_bit_value("_test", "5");
        t.cpu.reset(None, None, &mut t.tmp);
        assert_ne!(t.read8("_test"), "5");
    }

    #[test]
    fn xor_a() {
        for code in ["xor a", "xor a\nout (c), a"] {
            let t = T::after(code);
            assert_eq!(t.reg("a"), "0");
            assert_eq!(t.cpu.c(), Some(0));
            assert_eq!(t.cpu.z(), Some(1));
        }
    }

    #[test]
    fn in_a_c() {
        let t = T::after("xor a\nin a, (c)");
        assert_ne!(t.reg("a"), "0");
        assert!(is_unknown8(&t.reg("a")));
        assert_eq!(t.cpu.c(), Some(0));
        assert_eq!(t.cpu.z(), Some(1));
    }

    #[test]
    fn xor_a_ld_n_a() {
        let t = T::after("xor a\nld (_push), a");
        assert_eq!(t.reg("a"), "0");
        assert!(!is_unknown8(&t.reg("a")));
        assert_eq!(t.cpu.c(), Some(0));
        assert_eq!(t.cpu.z(), Some(1));
    }

    #[test]
    fn ex_de_hl() {
        let t = T::after("ld hl, 0x1234\nld de, 0x5678\nex de, hl");
        assert_eq!(t.reg("h"), 0x56.to_string());
        assert_eq!(t.reg("l"), 0x78.to_string());
        assert_eq!(t.reg("d"), 0x12.to_string());
        assert_eq!(t.reg("e"), 0x34.to_string());
    }

    #[test]
    fn ex_de_hl_unknown() {
        let mut t = T::after("ld hl, (x)\nld de, (y)\nex de, hl");
        let (x, y) = (t.read16("x"), t.read16("y"));
        assert_eq!(t.reg("hl"), y);
        assert_eq!(t.reg("de"), x);
        assert_eq!(t.reg("h"), HI16_val(&y, &mut t.tmp));
        assert_eq!(t.reg("l"), LO16_val(&y, &mut t.tmp));
        assert_eq!(t.reg("d"), HI16_val(&x, &mut t.tmp));
        assert_eq!(t.reg("e"), LO16_val(&x, &mut t.tmp));
    }

    #[test]
    fn neg_nz() {
        let t = T::after("xor a\nld a, 1\nneg");
        assert_eq!(t.reg("a"), 0xFF.to_string());
        assert_eq!(t.cpu.c(), Some(1));
        assert_eq!(t.cpu.z(), Some(0));
    }

    #[test]
    fn neg_z() {
        let t = T::after("xor a\ncp 1\nneg");
        assert_eq!(t.reg("a"), "0");
        assert_eq!(t.cpu.c(), Some(0));
        assert_eq!(t.cpu.z(), Some(1));
    }

    #[test]
    fn ix_neg() {
        let mut t = T::after("ld a, (ix-1)\nneg");
        assert_ne!(t.reg("a"), t.read8("ix-1"));
    }

    #[test]
    fn inc_dec_hl() {
        for (code, addr, value, z) in [
            ("ld hl, 32\nld (hl), 1\ninc (hl)", "32", "2", 0),
            ("ld hl, 32\nld (hl), 1\ndec (hl)", "32", "0", 1),
            ("ld hl, _a\nld (hl), 1\ninc (hl)", "_a", "2", 0),
            ("ld hl, _a\nld (hl), 1\ndec (hl)", "_a", "0", 1),
        ] {
            let t = T::after(code);
            assert_eq!(t.reg("hl"), addr);
            assert_eq!(t.mem(), [(addr.to_string(), value.to_string())]);
            assert_eq!(t.cpu.z(), Some(z));
        }
    }

    #[test]
    fn ld_hl_unknown() {
        let mut t = T::after("ld hl, (_temp_ch_len)\nld a, (hl)\nld (_temp_wav_len), a\nld hl, _temp_wav_len\ndec (hl)");
        assert_ne!(t.reg("a"), t.read8("_temp_wav_len"));
        assert_eq!(t.reg("hl"), "_temp_wav_len");
        // The Python test has these equal. But the pointer may be the
        // address of _temp_wav_len, which has since been decremented.
        let pointer = t.read16("_temp_ch_len");
        assert_ne!(t.reg("a"), t.read8(&pointer));
    }

    #[test]
    fn ld_ix() {
        for idx_reg in ["ix", "iy"] {
            let t = T::after(&format!("ld {idx_reg}, 0\nld {idx_reg}h, 1\nld {idx_reg}l, 1"));
            assert_eq!(t.reg(idx_reg), "257");
        }
    }

    /// The Next's `mul d, e` when both are known, and when they are not.
    #[test]
    fn next_mul() {
        let mut tmp = TmpVals::default();
        let mut cpu = CpuState::new(true, &mut tmp);
        for line in ["ld d, 3", "ld e, 5", "mul d, e"] {
            cpu.execute(line, &mut tmp);
        }
        assert_eq!(cpu.reg("de"), "15");
        cpu.execute("ld d, a", &mut tmp);
        cpu.execute("ld hl, 7", &mut tmp);
        cpu.execute("mul d, e", &mut tmp);
        // Not known: everything is forgotten.
        assert!(is_unknown16(cpu.reg("hl")));
    }

    /// MUL preserves the flags.
    #[test]
    fn next_mul_keeps_flags() {
        let mut tmp = TmpVals::default();
        let mut cpu = CpuState::new(true, &mut tmp);
        for line in ["xor a", "scf", "ld d, 1", "ld e, 1", "mul d, e"] {
            cpu.execute(line, &mut tmp);
        }
        assert_eq!((cpu.z(), cpu.c()), (Some(1), Some(1)));
    }

    /// A circular rotation leaves the bit that went round in the carry, and a
    /// rotation through the carry takes the carry it found.
    #[test]
    fn rotations_and_carry() {
        let t = T::after("ld a, 0\nscf\nrrca");
        assert_eq!((t.reg("a"), t.cpu.c()), ("0".to_string(), Some(0)));
        let t = T::after("ld a, 0\nscf\nrrca\nrra");
        assert_eq!((t.reg("a"), t.cpu.c()), ("0".to_string(), Some(0)));
        let t = T::after("ld a, 129\nrlca");
        assert_eq!((t.reg("a"), t.cpu.c()), ("3".to_string(), Some(1)));
        let t = T::after("ld a, 1\nscf\nrra");
        assert_eq!((t.reg("a"), t.cpu.c()), ("128".to_string(), Some(1)));
        let t = T::after("ld a, 128\nscf\nccf\nrla");
        assert_eq!((t.reg("a"), t.cpu.c()), ("0".to_string(), Some(1)));
    }

    /// A store through a pointer nothing is known of may be a store to any
    /// variable, and a store to a variable may change what such a pointer
    /// points at.
    #[test]
    fn stores_and_aliases() {
        // _x is no longer known to be 1.
        let mut t = T::after("ld a, 1\nld (_x), a\nld hl, (_ptr)\nld a, 2\nld (hl), a");
        assert!(is_unknown(&t.read8("_x")));
        // Through a label the cell is known, and so is its neighbour still.
        let mut t = T::after("ld a, 1\nld (_x), a\nld (_y), a\nld hl, _y\nld (hl), 2");
        assert_eq!((t.read8("_x"), t.read8("_y")), ("1".to_string(), "2".to_string()));
        // What an unknown pointer points at is forgotten on a store elsewhere.
        let mut t = T::after("ld hl, (_ptr)\nld (hl), 5");
        let pointer = t.reg("hl");
        assert_eq!(t.read8(&pointer), "5");
        t.eval("ld a, 7\nld (_x), a");
        assert!(is_unknown(&t.read8(&pointer)));
    }

    /// A word read from `(_v - 1)` has its high byte at `_v`, and a store
    /// to `_v` changes it.
    #[test]
    fn overlapping_addresses() {
        let mut t = T::after("ld hl, (_v - 1)\nld a, 1\nsub h\nld (_v), a");
        let h = t.reg("h");
        assert_ne!(t.read16("_v - 1").split('|').next().unwrap(), h);
        assert_eq!(t.read8("_v"), t.read16("_v-1").split('|').next().unwrap());
    }
}
