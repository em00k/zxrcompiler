//! The Z80 backend: quads to assembly text.
//!
//! This follows `src/arch/z80/backend` of the Python compiler module for
//! module and function for function; the ZX Spectrum Next's differences
//! (`src/arch/zxnext/backend`) are in [`zxnext`] and in the two prologues.
//! Each quad has a handler that returns the instructions for it as text; the
//! handlers read meaning from the first character of an operand (`*`
//! indirect, `#` immediate or address, `$` a local string, `_` a global, `t`
//! a temporary on the stack).

pub mod array;
pub mod bits16;
pub mod bits32;
pub mod bits8;
pub mod codebank;
pub mod common;
pub mod f16;
pub mod finish;
pub mod float;
pub mod fp;
pub mod generic;
pub mod parray;
pub mod pload;
pub mod runtime;
pub mod string;
pub mod zxnext;

pub use common::{Common, Options};

use common::{CALL_BACK, DATA_END_LABEL, DATA_LABEL, MAIN_LABEL, MEMINITS, NAMESPACE, START_LABEL};
use std::collections::{BTreeMap, BTreeSet};
use zxr_ir::Quad;
use zxr_opt::asm::Asm;
use zxr_opt::engine::{apply_match_sized, Candidates, Engine, OptPattern};
use zxr_opt::evaluator::BasicContext;
use zxr_opt::helpers::TmpVals;
use zxr_re::{Flags, Regex};

/// What turns one quad into instructions.
pub type Handler = fn(&mut Common, &Quad) -> Vec<String>;

include!(concat!(env!("OUT_DIR"), "/quad_table.rs"));

/// Implements a backend for the Z80 architecture, or for the ZX Spectrum
/// Next when the options say `zxnext`.
pub struct Backend {
    pub common: Common,
    /// The quads to convert (`MEMORY`).
    pub memory: Vec<Quad>,
    /// 'OPERATOR' -> emitting function.
    quad_table: BTreeMap<&'static str, Handler>,
    /// The counter of "unknown" values the peephole rules draw from.
    pub tmp_vals: TmpVals,
    /// The peephole rules of the level in force and, per first word of a
    /// line, those that could match there. Kept between calls: the join
    /// below runs once for every quad.
    peephole: Option<(u32, Vec<&'static OptPattern>, Candidates<'static>)>,
    /// For debug data, when wanted: the source line (see `Quad::origin`) of
    /// each line `emit` gave, in step with them.
    pub origins: Option<Vec<u32>>,
}

/// `re.sub(pattern, repl, text)` with a replacement that has no escapes.
fn sub(re: &Regex, repl: &str, text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut pos = 0;
    while pos <= chars.len() {
        match re.search(&chars, pos) {
            Some(m) => {
                out.extend(&chars[pos..m.start]);
                out.push_str(repl);
                if m.end == m.start {
                    // An empty match: step over one character.
                    if let Some(c) = chars.get(m.end) {
                        out.push(*c);
                    }
                    pos = m.end + 1;
                } else {
                    pos = m.end;
                }
            }
            None => {
                out.extend(&chars[pos.min(chars.len())..]);
                break;
            }
        }
    }
    out
}

impl Backend {
    pub fn new(options: Options) -> Backend {
        let zxnext = options.architecture == "zxnext";
        // The Next overrides the z80 implementation of three quads.
        let quad_table = QUAD_TABLE.iter().map(|&(name, _, z80, next)| (name, if zxnext { next } else { z80 })).collect();
        Backend { common: Common::new(options), memory: Vec::new(), quad_table, tmp_vals: TmpVals::default(), peephole: None, origins: None }
    }

    fn zxnext(&self) -> bool {
        self.common.options.architecture == "zxnext"
    }

    /// Program Start routine
    pub fn emit_prologue(&self) -> Vec<String> {
        let c = &self.common;
        let options = &c.options;
        let mut heap_init = vec![format!("{DATA_LABEL}:")];
        let mut output = vec![format!("org {}", options.org)];

        if c.requires.iter().any(|r| MEMINITS.contains(&r.as_str())) || c.inits.contains(&format!("{NAMESPACE}.__MEM_INIT")) {
            heap_init.push(format!("; Defines HEAP SIZE\n{} EQU {}", options.heap_size_label, options.heap_size));
            match options.heap_address {
                None => {
                    heap_init.push(format!("{}:", options.heap_start_label));
                    heap_init.push(format!("DEFS {}", options.heap_size));
                }
                Some(address) => heap_init.push(format!("; Defines HEAP ADDRESS\n{} EQU {address}", options.heap_start_label)),
            }
        }

        heap_init.push(format!("; Defines USER DATA Length in bytes\n{NAMESPACE}.ZXBASIC_USER_DATA_LEN EQU {DATA_END_LABEL} - {DATA_LABEL}"));
        heap_init.push(format!("{NAMESPACE}.__LABEL__.ZXBASIC_USER_DATA_LEN EQU {NAMESPACE}.ZXBASIC_USER_DATA_LEN"));
        heap_init.push(format!("{NAMESPACE}.__LABEL__.ZXBASIC_USER_DATA EQU {DATA_LABEL}"));

        output.push(format!("{START_LABEL}:"));
        if options.headerless {
            output.extend(heap_init);
            return output;
        }

        output.push("di".to_string());
        if self.zxnext() {
            output.push("push iy".to_string());
            output.push("ld iy, 0x5C3A  ; ZX Spectrum ROM variables address".to_string());
        } else {
            output.extend(common::strs(&["push ix", "push iy", "exx", "push hl", "exx"]));
        }
        output.push(format!("ld ({CALL_BACK}), sp"));
        output.push("ei".to_string());

        output.extend(c.inits.iter().map(|x| format!("call {x}")));

        output.push(format!("jp {MAIN_LABEL}"));
        output.push(format!("{CALL_BACK}:"));
        output.push("DEFW 0".to_string());
        output.extend(c.codebank.emit_prologue_data(options));
        output.extend(heap_init);

        output
    }

    /// This special ending autoinitializes required inits (mainly
    /// alloc.asm) and changes the MEMORY initial address if it is ORG XXXX
    /// to ORG XXXX + heap size
    pub fn emit_epilogue(&self) -> Vec<String> {
        let mut output = self.common.at_end.clone();

        if self.common.options.autorun {
            output.push(format!("END {START_LABEL}"));
        } else {
            output.push("END".to_string());
        }

        output
    }

    /// Removes unused labels from the output (list of asm instructions)
    pub fn remove_unused_labels(&self, output: &mut Vec<String>) {
        let tmp_labels = &self.common.tmp_labels.labels;
        let mut labels_used: BTreeSet<String> = BTreeSet::new();
        let mut labels_to_delete: BTreeMap<String, usize> = BTreeMap::new();
        let mut labels: BTreeSet<String> = BTreeSet::new();
        let mut label_alias: BTreeMap<String, String> = BTreeMap::new();

        let mut prev: Option<String> = None;
        for ins in output.iter() {
            if let Some(ins) = ins.strip_suffix(':') {
                labels.insert(ins.to_string());
                if let Some(prev) = &prev {
                    if !tmp_labels.contains(prev) && tmp_labels.contains(ins) || label_alias.contains_key(prev) {
                        label_alias.insert(ins.to_string(), prev.clone());
                    } else {
                        label_alias.insert(prev.clone(), ins.to_string());
                    }
                }
                prev = Some(ins.to_string());
            } else {
                prev = None;
            }
        }

        for i in 0..output.len() {
            let ins = output[i].clone();
            // `ins[:-1]`: whatever the last character is.
            let mut try_label = ins.clone();
            try_label.pop();
            if tmp_labels.contains(&try_label) {
                if labels_used.contains(&try_label) {
                    labels_to_delete.remove(&try_label);
                } else {
                    labels_to_delete.insert(try_label, i);
                }
                continue;
            }

            for op in Asm::opers(&ins) {
                if labels.contains(&op) {
                    let mut new_label = &op;
                    while let Some(alias) = label_alias.get(new_label) {
                        new_label = alias;
                    }

                    labels_used.insert(new_label.clone());
                    labels_to_delete.remove(new_label);

                    if *new_label != op {
                        // The pattern is built from the label with only its
                        // dots escaped, as the Python code builds it, and is
                        // applied to the line as first read: if a second
                        // operand is replaced too, the first replacement is
                        // lost.
                        let pattern = format!("((?<![.a-zA-Z0-9_])){}(?=$|\\s)", op.replace('.', "[.]"));
                        let re = Regex::new(&pattern, Flags::NONE).unwrap_or_else(|e| panic!("re.error: {e}"));
                        output[i] = sub(&re, new_label, &ins);
                    }
                }
            }
        }

        let mut doomed: Vec<usize> = labels_to_delete.into_values().collect();
        doomed.sort_unstable_by(|a, b| b.cmp(a));
        for i in doomed {
            output.remove(i);
        }
    }

    /// Extends output instruction list performing a little peep-hole
    /// optimization (O1)
    fn output_join(&mut self, output: &mut Vec<String>, new_chunk: Vec<String>, optimize: bool, chunk_origins: Option<Vec<u32>>) {
        let base_index = output.len(); // output length before extending it
        output.extend(new_chunk);
        if let (Some(origins), Some(chunk_origins)) = (&mut self.origins, chunk_origins) {
            origins.extend(chunk_origins);
        }

        if !optimize {
            return;
        }

        // Optimization patterns: at this point no more than -O2
        let engine = Engine::for_arch(&self.common.options.architecture);
        let level = self.common.options.optimization_level.min(2);
        if self.peephole.as_ref().is_none_or(|(cached, _, _)| *cached != level as u32) {
            let patterns = engine.patterns.iter().filter(|x| x.level <= level).collect();
            self.peephole = Some((level as u32, patterns, Candidates::default()));
        }
        let (_, patterns, candidates) = self.peephole.as_mut().unwrap();
        let asms = &self.common.asms;
        let mut ctx = BasicContext { is_asm: &|line| asms.contains_key(line), tmp: std::mem::take(&mut self.tmp_vals) };

        // Only the rules whose first line could match the line at hand are
        // tried; see `Candidates`.
        let mut idx = base_index.saturating_sub(engine.maxlen);
        while idx < output.len() {
            let rules = candidates.at(patterns, &output[idx]);
            match apply_match_sized(output, rules.iter().copied(), idx, &mut ctx) {
                // Nothing changed
                None => idx += 1,
                Some((replaced, by)) => {
                    // The new lines are of the lines they stand for.
                    if let Some(origins) = &mut self.origins {
                        let new: Vec<u32> = (0..by).map(|i| origins[idx + i.min(replaced.saturating_sub(1))]).collect();
                        origins.splice(idx..idx + replaced, new);
                    }
                    idx = idx.saturating_sub(engine.maxlen);
                }
            }
        }
        self.tmp_vals = ctx.tmp;
    }

    /// Begin converting each quad instruction to asm by iterating over the
    /// "mem" array, and called its associated function. Each function
    /// returns an array of ASM instructions which will be appended to the
    /// 'output' array
    pub fn emit(&mut self, optimize: bool) -> Vec<String> {
        let mut output: Vec<String> = Vec::new();
        let debug = self.origins.is_some();
        if debug {
            self.origins = Some(Vec::new());
        }
        let memory = std::mem::take(&mut self.memory);
        for quad in &memory {
            let Some(handler) = self.quad_table.get(quad.op.as_str()) else { panic!("KeyError: '{}'", quad.op) };
            let chunk = handler(&mut self.common, quad);
            let origins = debug.then(|| vec![quad.origin; chunk.len()]);
            self.output_join(&mut output, chunk, optimize, origins);
        }
        self.memory = memory;

        if optimize && self.common.options.optimization_level > 1 {
            let before = debug.then(|| output.clone());
            self.remove_unused_labels(&mut output);
            let tmp = std::mem::take(&mut output);
            let origins = before.map(|before| finish::follow(&before, &self.origins.replace(Vec::new()).unwrap_or_default(), &tmp));
            self.output_join(&mut output, tmp, optimize, origins);
        }

        output.extend(self.common.requires.iter().map(|j| format!("#include once <{j}>")));
        if let Some(origins) = &mut self.origins {
            origins.resize(output.len(), 0);
        }

        output // Caller will save its contents to a file, or whatever
    }
}
