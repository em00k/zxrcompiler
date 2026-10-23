//! What has been assembled: bytes by address, per output segment, and the
//! labels.
//!
//! Segment 0 is the resident program. Segments 1 and up are ZX Next code
//! banks, each assembled at the code-window address and written out on its
//! own. Labels live in one namespace across all of them.

use crate::expr::{Label, LabelData, LabelValue};
use crate::instr::Asm;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;
use zxr_core::Session;

const MAX_MEM: i128 = 65535;
const BRANCHES: [&str; 4] = ["JP ", "JR ", "CALL ", "DJNZ "];
/// A reference to a label with one of these endings may cross into a bank:
/// the far-call trampoline's target and a FARPTR alias.
const FAR_POINTER_SUFFIXES: [&str; 2] = [".__far", ".__faraddr"];

pub struct Segment {
    pub id: i64,
    pub base: i128,
    /// Where the next byte goes.
    pub index: i128,
    pub bytes: BTreeMap<i128, u8>,
    /// Instructions by the address they start at.
    pub orgs: BTreeMap<i128, Vec<usize>>,
}

impl Segment {
    fn new(id: i64, org: i128) -> Segment {
        Segment { id, base: org, index: org, bytes: BTreeMap::new(), orgs: BTreeMap::new() }
    }
}

/// A dictionary that remembers the order things were added in.
#[derive(Default)]
pub struct LabelMap {
    order: Vec<String>,
    map: HashMap<String, Label>,
}

impl LabelMap {
    pub fn get(&self, name: &str) -> Option<&Label> {
        self.map.get(name)
    }

    pub fn insert(&mut self, name: &str, label: Label) {
        if self.map.insert(name.to_string(), label).is_none() {
            self.order.push(name.to_string());
        }
    }

    pub fn values(&self) -> impl Iterator<Item = &Label> {
        self.order.iter().map(|name| &self.map[name])
    }
}

pub fn normalize_namespace(namespace: &str) -> String {
    format!(".{}", namespace.split('.').filter(|part| !part.is_empty()).collect::<Vec<_>>().join("."))
}

fn is_decimal(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|c| c.is_ascii_digit())
}

pub struct Memory {
    pub segments: BTreeMap<i64, Segment>,
    cur: i64,
    /// Label scopes: the global one, then one per open PROC.
    pub local_labels: Vec<LabelMap>,
    /// Line of each open PROC.
    pub scopes: Vec<u32>,
    pub instructions: Vec<Asm>,
    /// Current namespace, a prefix for the labels declared in it.
    pub namespace: String,
    pub namespace_stack: Vec<String>,
    /// Lines that declare a temporary label, per file, ascending.
    tmp_lines: HashMap<String, Vec<u32>>,
    tmp_labels: HashMap<(String, u32), HashMap<String, Label>>,
    /// References to temporary labels still to be resolved, per file.
    tmp_pending: Vec<(String, Vec<Label>)>,
    /// The name of the file being read, shared by all that is read from it.
    file: Rc<str>,
    /// For debug data: the source line of the label about to be declared.
    pub label_source: Option<(Rc<str>, u32)>,
}

impl Default for Memory {
    fn default() -> Self {
        Memory {
            segments: BTreeMap::from([(0, Segment::new(0, 0))]),
            cur: 0,
            local_labels: vec![LabelMap::default()],
            scopes: Vec::new(),
            instructions: Vec::new(),
            namespace: ".".to_string(),
            namespace_stack: Vec::new(),
            tmp_lines: HashMap::new(),
            tmp_labels: HashMap::new(),
            tmp_pending: Vec::new(),
            file: Rc::from(""),
            label_source: None,
        }
    }
}

/// The file an error found late belongs to. The Python assembler names the
/// file it read last, whatever the line is a line of.
fn file_of<'a>(sess: &Session, filename: &'a str) -> Option<&'a str> {
    (!sess.oracle_diagnostics && !filename.is_empty()).then_some(filename)
}

impl Memory {
    fn here(&mut self, sess: &Session) -> Rc<str> {
        if *self.file != *sess.filename {
            self.file = Rc::from(sess.filename.as_str());
        }
        Rc::clone(&self.file)
    }

    fn seg(&mut self) -> &mut Segment {
        self.segments.get_mut(&self.cur).unwrap()
    }

    pub fn org(&self) -> i128 {
        self.segments[&self.cur].index
    }

    /// Whether nothing has been assembled into the current segment.
    pub fn is_empty(&self) -> bool {
        self.segments[&self.cur].bytes.is_empty()
    }

    /// The highest address written in the current segment.
    pub fn last_address(&self) -> i128 {
        self.segments[&self.cur].bytes.keys().next_back().copied().unwrap_or(0)
    }

    /// The lowest address anything starts at in the current segment.
    pub fn first_org(&self) -> i128 {
        self.segments[&self.cur].orgs.keys().next().copied().unwrap_or(0)
    }

    pub fn current_segment(&self) -> i64 {
        self.cur
    }

    pub fn global_labels(&self) -> &LabelMap {
        &self.local_labels[0]
    }

    pub fn set_segment(&mut self, sess: &mut Session, segment: i128, base: i128, lineno: u32) {
        if segment < 0 {
            sess.error(lineno, &format!("Invalid CODEBANK number {segment}. Must be 0 or greater"), None);
            return;
        }
        if self.local_labels.len() > 1 {
            sess.error(lineno, "CODEBANK cannot be used inside a PROC", None);
            return;
        }
        let id = segment as i64;
        self.segments.entry(id).or_insert_with(|| Segment::new(id, base));
        self.cur = id;
        // Temporary labels are positional and must not resolve across a switch.
        self.flush_temporary_labels(sess, lineno);
    }

    pub fn enter_proc(&mut self, lineno: u32) {
        self.local_labels.push(LabelMap::default());
        self.scopes.push(lineno);
    }

    pub fn exit_proc(&mut self, sess: &mut Session, lineno: u32) {
        if self.local_labels.len() <= 1 {
            sess.error(lineno, "ENDP in global scope (with no PROC)", None);
            return;
        }
        let labels: Vec<Label> = self.local_labels.last().unwrap().values().cloned().collect();
        for label in labels {
            let (local, defined, name, line, value, segment) = {
                let l = label.borrow();
                (l.local, l.defined(), l.name(), l.lineno, l.value.clone(), l.segment)
            };
            if local {
                if !defined {
                    sess.error(lineno, &format!("Undefined LOCAL label '{name}'"), None);
                    return;
                }
                continue;
            }
            // Anything not declared LOCAL belongs to the global scope.
            match self.local_labels[0].get(&name).cloned() {
                None => self.local_labels[0].insert(&name, label),
                Some(global) => {
                    self.define(sess, &global, value, line);
                    global.borrow_mut().segment = segment;
                }
            }
        }
        self.local_labels.pop();
        self.scopes.pop();
    }

    pub fn set_org(&mut self, sess: &mut Session, value: i128, lineno: u32) {
        if !(0..=MAX_MEM).contains(&value) {
            sess.error(lineno, &format!("Memory ORG out of range [0 .. 65535]. Current value: {value}"), None);
        }
        self.clear_temporary_labels();
        self.seg().index = value;
    }

    /// The full name of a label: prefixed with the namespace unless it
    /// starts with a dot or is a temporary (numeric) label.
    fn id_name(&self, label: &str) -> (String, String) {
        let namespace = self.namespace.clone();
        let temporary = is_decimal(label) || (label.ends_with(['B', 'F']) && is_decimal(&label[..label.len() - 1]));
        if temporary || label.starts_with('.') {
            return (label.to_string(), namespace);
        }
        (normalize_namespace(&format!("{namespace}.{label}")), namespace)
    }

    fn define(&mut self, sess: &mut Session, label: &Label, value: Option<LabelValue>, lineno: u32) {
        let mut l = label.borrow_mut();
        if l.defined() {
            sess.error(lineno, &format!("label '{}' already defined at line {}", l.name(), l.lineno), None);
        }
        l.value = value;
        l.lineno = lineno;
        l.filename = Rc::clone(&self.file);
        l.namespace = Some(self.namespace.clone());
    }

    fn set_memory_slot(&mut self) {
        let org = self.org();
        let seg = self.seg();
        if !seg.orgs.contains_key(&org) {
            seg.orgs.insert(org, Vec::new());
            seg.bytes.insert(org, 0);
        }
    }

    pub fn add_instruction(&mut self, sess: &mut Session, mut instr: Asm) {
        if sess.has_errors > 0 {
            return;
        }
        self.set_memory_slot();
        instr.filename = self.here(sess);
        let bytes = instr.bytes(sess);
        let index = self.instructions.len();
        self.instructions.push(instr);
        let org = self.org();
        let seg = self.seg();
        seg.orgs.get_mut(&org).unwrap().push(index);
        for byte in bytes {
            seg.bytes.insert(seg.index, byte);
            seg.index += 1;
        }
    }

    /// Declares a label: at the current address, or with `value` for an EQU.
    pub fn declare_label(&mut self, sess: &mut Session, label: &str, lineno: u32, value: Option<LabelValue>) {
        let (ex_label, namespace) = self.id_name(label);
        let file = self.here(sess);
        let is_address = value.is_none();
        let value = value.unwrap_or(LabelValue::Int(self.org()));
        if is_decimal(label) {
            let fname = sess.filename.clone();
            let lines = self.tmp_lines.entry(fname.clone()).or_default();
            if lines.last() != Some(&lineno) {
                lines.push(lineno);
            }
            let declared = LabelData::new(&ex_label, lineno, Some(value), false, Some(namespace), true);
            self.tmp_labels.entry((fname, lineno)).or_default().insert(ex_label, declared);
            return;
        }
        let existing = self.local_labels.last().unwrap().get(&ex_label).cloned();
        let declared = match existing {
            Some(existing) => {
                self.define(sess, &existing, Some(value), lineno);
                existing.borrow_mut().is_address = is_address;
                existing
            }
            None => {
                let new = LabelData::new(&ex_label, lineno, Some(value), false, Some(namespace), is_address);
                new.borrow_mut().filename = file;
                self.local_labels.last_mut().unwrap().insert(&ex_label, Rc::clone(&new));
                new
            }
        };
        if is_address {
            declared.borrow_mut().segment = self.cur;
        }
        declared.borrow_mut().source = self.label_source.clone();
        self.set_memory_slot();
    }

    /// The label of that name as seen from here, created undefined if it is
    /// not known yet.
    pub fn get_label(&mut self, sess: &Session, label: &str, lineno: u32) -> Label {
        let (ex_label, namespace) = self.id_name(label);
        let result = LabelData::new(&ex_label, lineno, None, false, Some(namespace), false);
        result.borrow_mut().filename = self.here(sess);
        if result.borrow().is_temporary() {
            match self.tmp_pending.iter_mut().find(|(f, _)| *f == sess.filename) {
                Some((_, list)) => list.push(Rc::clone(&result)),
                None => self.tmp_pending.push((sess.filename.clone(), vec![Rc::clone(&result)])),
            }
            return result;
        }
        if let Some(found) = self.local_labels.iter().rev().find_map(|scope| scope.get(&ex_label)) {
            return Rc::clone(found);
        }
        self.local_labels.last_mut().unwrap().insert(&ex_label, Rc::clone(&result));
        result
    }

    /// Marks a label as LOCAL to the current PROC.
    pub fn set_label(&mut self, sess: &mut Session, label: &str, lineno: u32) {
        let (ex_label, _) = self.id_name(label);
        let file = self.here(sess);
        let existing = self.local_labels.last().unwrap().get(&ex_label).cloned();
        let result = match existing {
            Some(found) => {
                found.borrow_mut().lineno = lineno;
                found.borrow_mut().filename = file;
                found
            }
            None => {
                let new = LabelData::new(&ex_label, lineno, None, false, Some(self.namespace.clone()), false);
                new.borrow_mut().filename = file;
                self.local_labels.last_mut().unwrap().insert(&ex_label, Rc::clone(&new));
                new
            }
        };
        if result.borrow().local {
            sess.warning(lineno, &format!("label '{label}' already declared as LOCAL"), None);
        }
        result.borrow_mut().local = true;
    }

    fn resolve_temporary_label(&self, fname: &str, label: &Label) {
        let (direction, lineno, name, namespace) = {
            let l = label.borrow();
            (l.direction(), l.lineno, l.name(), l.namespace.clone())
        };
        let empty = Vec::new();
        let lines = self.tmp_lines.get(fname).unwrap_or(&empty);
        let candidates: Vec<u32> = match direction {
            -1 => lines.iter().copied().filter(|&l| l <= lineno).rev().collect(),
            1 => lines.iter().copied().filter(|&l| l >= lineno).collect(),
            _ => return,
        };
        for line in candidates {
            let Some(found) = self.tmp_labels.get(&(fname.to_string(), line)).and_then(|m| m.get(&name)) else { continue };
            let f = found.borrow();
            if f.name() == name && f.namespace == namespace {
                label.borrow_mut().value = f.value.clone();
                return;
            }
        }
    }

    pub fn flush_temporary_labels(&mut self, sess: &mut Session, lineno: u32) {
        for (fname, labels) in &self.tmp_pending {
            for label in labels {
                self.resolve_temporary_label(fname, label);
                if !label.borrow().defined() {
                    sess.error(lineno, &format!("Undefined temporary label '{}'", label.borrow().name()), None);
                }
            }
        }
        self.clear_temporary_labels();
    }

    pub fn clear_temporary_labels(&mut self) {
        self.tmp_lines.clear();
        self.tmp_labels.clear();
        self.tmp_pending.clear();
    }

    pub fn check_undefined_labels(&mut self, sess: &mut Session) {
        for (fname, labels) in &self.tmp_pending {
            for label in labels {
                self.resolve_temporary_label(fname, label);
                let l = label.borrow();
                if !l.defined() {
                    sess.error(l.lineno, &format!("Undefined temporary label '{}'", l.name()), file_of(sess, &l.filename));
                }
            }
        }
        for label in self.local_labels[0].values() {
            let l = label.borrow();
            if !l.defined() {
                sess.error(l.lineno, &format!("Undefined GLOBAL label '{}'", l.name()), file_of(sess, &l.filename));
            }
        }
    }

    /// Reports references from one segment to a label in a different bank.
    /// Two banks are never mapped at once, so such an address means nothing.
    pub fn check_cross_segment_refs(&self, sess: &mut Session) {
        if self.segments.len() < 2 {
            return;
        }
        for seg in self.segments.values() {
            for indexes in seg.orgs.values() {
                for &index in indexes {
                    let instr = &self.instructions[index];
                    for label in &instr.label_refs {
                        let (target, name) = {
                            let l = label.borrow();
                            (l.segment, l.name())
                        };
                        if target == 0 || target == seg.id || FAR_POINTER_SUFFIXES.iter().any(|s| name.ends_with(s)) {
                            continue;
                        }
                        let origin = if seg.id != 0 { format!("CODEBANK {}", seg.id) } else { "the resident program".to_string() };
                        let message = if BRANCHES.iter().any(|b| instr.asm.starts_with(b)) {
                            format!("'{}' branches from {origin} to '{name}' in CODEBANK {target}. Only calls through the resident trampoline may cross banks", instr.asm)
                        } else {
                            format!("'{}' in {origin} refers to '{name}', which lives in CODEBANK {target}. Bank-local data is only addressable while its own bank is paged in", instr.asm)
                        };
                        sess.error(instr.lineno, &message, file_of(sess, &instr.filename));
                    }
                }
            }
        }
    }

    /// The bytes of a segment from its lowest to its highest address, with
    /// every operand that was waiting for a label filled in.
    pub fn dump(&mut self, sess: &mut Session, segment: i64) -> (i128, Vec<u8>) {
        let Some(seg) = self.segments.get(&segment) else { return (0, Vec::new()) };
        let (Some(&org), Some(&last)) = (seg.bytes.keys().next(), seg.bytes.keys().next_back()) else { return (seg.base, Vec::new()) };
        let mut output = Vec::new();
        // Addresses that hold a label but no instruction are only emitted if
        // something follows them.
        let mut align = 0usize;
        let mut address = org;
        while address <= last {
            if sess.has_errors > 0 {
                return (org, output);
            }
            let slot = self.segments[&segment].orgs.get(&address).cloned();
            if let Some(indexes) = slot {
                let Some(&first) = indexes.first() else {
                    align += 1;
                    address += 1;
                    continue;
                };
                output.extend(std::iter::repeat_n(0, align));
                align = 0;
                if self.instructions[first].pending {
                    // What is wrong with it is said of the file it is in.
                    let file = file_of(sess, &self.instructions[first].filename).map(str::to_string);
                    let last_read = file.map(|file| std::mem::replace(&mut sess.filename, file));
                    self.instructions[first].resolve(sess);
                    let bytes = self.instructions[first].bytes(sess);
                    if let Some(last_read) = last_read {
                        sess.filename = last_read;
                    }
                    let seg = self.segments.get_mut(&segment).unwrap();
                    for (offset, byte) in bytes.into_iter().enumerate() {
                        seg.bytes.insert(address + offset as i128, byte);
                    }
                }
            }
            output.push(self.segments[&segment].bytes.get(&address).copied().unwrap_or(0));
            address += 1;
        }
        (org, output)
    }

    /// One line per address label, sorted: `8000: .name`, or `B2:6000: .name`
    /// for one in a code bank.
    pub fn memory_map(&self) -> String {
        let mut lines: Vec<String> = self
            .local_labels[0]
            .values()
            .filter_map(|label| {
                let l = label.borrow();
                let Some(LabelValue::Int(value)) = l.value else { return None };
                if !l.is_address {
                    return None;
                }
                let prefix = if l.segment != 0 { format!("B{}:", l.segment) } else { String::new() };
                Some(format!("{prefix}{value:04X}: {}", l.name()))
            })
            .collect();
        lines.sort();
        lines.join("\n")
    }
}
