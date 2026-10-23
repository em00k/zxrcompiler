//! The flow optimiser of -O3 and -O4: `src/arch/z80/optimizer` of the
//! Python compiler (`main.py`, `basicblock.py`, `flow_graph.py`) and the ZX
//! Spectrum Next's subclasses of it.
//!
//! The code `emit` produced is cut into basic blocks, the blocks are linked
//! into a graph (jumps, fall-through, and from every exit of a routine back
//! to the instruction after each of its calls), jumps to jumps are
//! threaded, and then each block is run through the peephole rules of
//! levels 3 and 4 with a symbolic CPU ([`cpustate`]) and a liveness test
//! ([`Optimizer::is_used`]) answering the rules' `GVAL`, `FLAGVAL` and
//! `IS_REQUIRED`.
//!
//! Python's blocks refer to each other freely; here they live in one arena
//! (`Optimizer::blocks`) and are named by their index. The sets of blocks
//! (`comes_from`, `goes_to`, `used_by`, `called_by`) are ordered by that
//! index, which is not the order CPython iterates them in. `docs/quirks.md`
//! says, site by site, why no result depends on it.

pub mod cell;
pub mod cpustate;

pub use cell::{Cell, Env};
use cell::ids_or_numbers;
use cpustate::CpuState;

use crate::engine::{to_values, Engine, OptPattern};
use crate::evaluator::{Context, Value};
use crate::helpers::{idx_args, is_16bit_oper_register, simplify_asm_args, single_registers, TmpVals, ALL_REGS, END_PROGRAM_LABEL};
use crate::pattern::LineChars;
use std::cell::Cell as Flag;
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

/// A basic block, or a `DummyBasicBlock` when `dummy` is set.
#[derive(Debug, Default)]
pub struct Block {
    pub mem: Vec<Cell>,
    /// Which (if any) basic block follows this one in memory
    pub next: Option<usize>,
    /// Which (if any) basic block precedes to this one in the code
    pub prev: Option<usize>,
    /// True if this block is being accessed by other subroutine
    lock: Flag<bool>,
    /// Possible jumps to this block
    pub comes_from: BTreeSet<usize>,
    /// Possible blocks to jump from here
    pub goes_to: BTreeSet<usize>,
    /// True if something has been changed during optimization
    pub modified: bool,
    pub called_by: BTreeSet<usize>,
    /// True if this block can be ignored (it's useless)
    pub ignored: bool,
    /// True if this block was already optimized
    pub optimized: bool,
    pub cpu: CpuState,
    /// A dummy basic block with some basic information about what
    /// registers uses an destroys.
    pub dummy: Option<Dummy>,
}

#[derive(Debug)]
pub struct Dummy {
    pub destroys: Vec<String>,
    pub requires: BTreeSet<String>,
}

/// Class describing label information: which basic block contains it, and
/// which use it.
#[derive(Debug)]
pub struct LabelInfo {
    pub label: String,
    /// Memory address or 0
    pub addr: usize,
    /// Basic Block this label is in
    pub basic_block: usize,
    /// Position within the Basic Block
    pub position: usize,
    /// Which BB uses this label, if any
    pub used_by: BTreeSet<usize>,
}

/// Implements the Peephole Optimizer
pub struct Optimizer<'a> {
    pub env: Env<'a>,
    /// The counter of unknown values (`helpers._RAND_COUNT`).
    pub tmp: TmpVals,
    pub blocks: Vec<Block>,
    /// PROC labels name space counter
    proc_counter: usize,
    /// Label -> LabelInfo object. A dictionary of labels where an existing
    /// label cannot be overwritten.
    pub labels: HashMap<String, LabelInfo>,
    /// Labels which must start a basic block, because they're used in a
    /// JP/CALL
    pub jump_labels: BTreeSet<String>,
    /// `BasicBlock.clean_asm_args`, of the blocks that hold code.
    pub clean_asm_args: bool,
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|x| x.to_string()).collect()
}

/// The registers `BasicBlock.requires` and `destroys` look for.
const BLOCK_REGS: [&str; 14] = ["a", "b", "c", "d", "e", "f", "h", "l", "i", "ixh", "ixl", "iyh", "iyl", "sp"];

impl<'a> Optimizer<'a> {
    pub fn new(env: Env<'a>) -> Optimizer<'a> {
        let mut optimizer = Optimizer { env, tmp: TmpVals::default(), blocks: Vec::new(), proc_counter: 0, labels: HashMap::new(), jump_labels: BTreeSet::new(), clean_asm_args: false };
        optimizer.init();
        optimizer
    }

    pub fn init(&mut self) {
        self.tmp.count = 0; // helpers.init()

        self.jump_labels.clear();
        self.proc_counter = 0;
        self.labels.clear();

        let all: Vec<&str> = ALL_REGS.to_vec();
        let table: [(&str, &str, &[&str]); 24] = [
            ("*START*", "*START*", &all), // Special START BLOCK
            ("*__END_PROGRAM*", "__END_PROGRAM", &["b", "c"]),
            // SOME Global modules initialization
            ("__ADDF", "__ADDF", &["a", "e", "d", "b", "c"]),
            ("__SUBF", "__SUBF", &["a", "e", "d", "b", "c"]),
            ("__DIVF", "__DIVF", &["a", "e", "d", "b", "c"]),
            ("__MULF", "__MULF", &["a", "e", "d", "b", "c"]),
            ("__GEF", "__GEF", &["a", "e", "d", "b", "c"]),
            ("__GTF", "__GTF", &["a", "e", "d", "b", "c"]),
            ("__EQF", "__EQF", &["a", "e", "d", "b", "c"]),
            ("__STOREF", "__STOREF", &["h", "l", "a", "e", "d", "b", "c"]),
            ("PRINT_AT", "PRINT_AT", &["a"]),
            ("INK", "INK", &["a"]),
            ("INK_TMP", "INK_TMP", &["a"]),
            ("PAPER", "PAPER", &["a"]),
            ("PAPER_TMP", "PAPER_TMP", &["a"]),
            ("RND", "RND", &[]),
            ("INKEY", "INKEY", &[]),
            ("PLOT", "PLOT", &["a"]),
            ("DRAW", "DRAW", &["h", "l"]),
            ("DRAW3", "DRAW3", &["a", "b", "c", "d", "e"]),
            ("__ARRAY", "__ARRAY", &["h", "l"]),
            ("__MEMCPY", "__MEMCPY", &["b", "c", "d", "e", "h", "l"]),
            ("__PLOADF", "__PLOADF", &all),
            ("__PSTOREF", "__PSTOREF", &all),
        ];
        for (key, label, requires) in table {
            let destroys: &[&str] = if key == "__MEMCPY" { &["b", "c", "d", "e", "f", "h", "l"] } else { &all };
            let block = self.new_dummy_block(destroys, requires);
            self.labels.insert(key.to_string(), LabelInfo { label: label.to_string(), addr: 0, basic_block: block, position: 0, used_by: BTreeSet::new() });
        }
    }

    /// `BasicBlock(memory, optimizer)`
    pub fn new_block(&mut self, memory: &[String]) -> usize {
        self.blocks.push(Block::default());
        let id = self.blocks.len() - 1;
        self.set_code(id, memory);
        self.blocks[id].cpu = CpuState::new(false, &mut self.tmp);
        if self.env.zxnext {
            // The Next's subclass makes its own CPU after the Z80's.
            self.blocks[id].cpu = CpuState::new(true, &mut self.tmp);
        }
        id
    }

    /// `DummyBasicBlock(destroys, requires, optimizer)`: always the Z80's
    /// class.
    pub fn new_dummy_block(&mut self, destroys: &[&str], requires: &[&str]) -> usize {
        self.blocks.push(Block::default());
        let id = self.blocks.len() - 1;
        self.blocks[id].cpu = CpuState::new(false, &mut self.tmp);
        self.blocks[id].dummy = Some(Dummy { destroys: strings(destroys), requires: requires.iter().map(|x| x.to_string()).collect() });
        self.blocks[id].mem = vec![Cell::new("ret", 0)];
        id
    }

    /// `block.code`
    pub fn code(&self, block: usize) -> Vec<String> {
        self.blocks[block].mem.iter().map(|x| x.code().to_string()).collect()
    }

    /// `block.code = value`
    pub fn set_code(&mut self, block: usize, value: &[String]) {
        let clean = self.clean_asm_args && self.blocks[block].dummy.is_none();
        self.blocks[block].mem = value.iter().enumerate().map(|(i, asm)| if clean { Cell::new(&simplify_asm_args(asm), i) } else { Cell::new(asm, i) }).collect();
    }

    /// Removes the basic_block ptr from the list for "comes_from" if it
    /// exists.
    pub fn delete_comes_from(&mut self, block: usize, basic_block: Option<usize>) {
        let Some(other) = basic_block else { return };
        if !self.blocks[block].comes_from.remove(&other) {
            return;
        }
        if !self.blocks[other].goes_to.remove(&block) {
            panic!("KeyError: a basic block");
        }
    }

    /// Removes the basic_block ptr from the list for "goes_to" if it
    /// exists.
    pub fn delete_goes_to(&mut self, block: usize, basic_block: Option<usize>) {
        let Some(other) = basic_block else { return };
        if !self.blocks[block].goes_to.remove(&other) {
            return;
        }
        if !self.blocks[other].comes_from.remove(&block) {
            panic!("KeyError: a basic block");
        }
    }

    /// Adds the basic_block to the comes_from list if not done already.
    pub fn add_comes_from(&mut self, block: usize, basic_block: Option<usize>) {
        let Some(other) = basic_block else { return };
        if !self.blocks[block].comes_from.insert(other) {
            return;
        }
        self.blocks[other].goes_to.insert(block);
    }

    /// Adds the basic_block to the goes_to list if not done already.
    pub fn add_goes_to(&mut self, block: usize, basic_block: Option<usize>) {
        let Some(other) = basic_block else { return };
        if !self.blocks[block].goes_to.insert(other) {
            return;
        }
        self.blocks[other].comes_from.insert(block);
    }

    /// Checks whether any of the given regs are required from the given
    /// point to the end or not.
    pub fn is_used(&self, block: usize, regs: &[String], i: usize, top: Option<usize>) -> bool {
        let blk = &self.blocks[block];
        if let Some(dummy) = &blk.dummy {
            return regs.iter().any(|x| dummy.requires.contains(x));
        }

        if blk.lock.get() {
            return true;
        }

        let len = blk.mem.len();
        let top = match top {
            None => len,
            Some(top) => top + 1,
        };

        if let Some(first) = regs.first() {
            if first.is_empty() {
                panic!("IndexError: string index out of range");
            }
            if first.starts_with('(') && first.ends_with(')') {
                // A memory address
                let chars: Vec<char> = first.chars().collect();
                let inner: String = chars[1..chars.len().max(2) - 1].iter().collect();
                let r16 = if is_16bit_oper_register(&inner) { single_registers(&[inner.as_str()]) } else { Vec::new() };
                let ix = match idx_args(&inner) {
                    Some(idx) => single_registers(&[idx.0.as_str()]),
                    None => Vec::new(),
                };

                let rr = !r16.is_empty() || !ix.is_empty();
                let mem_vars: BTreeSet<&str> = if rr { BTreeSet::new() } else { ids_or_numbers(first).into_iter().collect() };

                // For memory accesses only mark as NOT used if it's overwritten
                for mem in blk.mem.iter().take(top.min(len)).skip(i) {
                    let opers = mem.opers();
                    let oper0 = || opers.first().unwrap_or_else(|| panic!("IndexError: list index out of range"));
                    if mem.inst() == "ld" && oper0() == first {
                        return false;
                    }

                    // And, Or, Xor uses both operands
                    if matches!(mem.inst(), "and" | "or" | "xor") && oper0() == first {
                        return true;
                    }

                    if opers.last().is_some_and(|last| last == first) {
                        return true;
                    }

                    // (hl) :: inc hl / (ix + n) :: inc ix
                    if rr && mem.destroys(&self.env).iter().any(|x| r16.contains(x)) {
                        return true;
                    }

                    if opers.last().is_some_and(|last| ids_or_numbers(last).iter().any(|x| mem_vars.contains(x))) {
                        return true;
                    }
                }

                return true;
            }
        }

        // make a copy
        let mut regs: Vec<String> = regs.iter().flat_map(|x| single_registers(&[x.as_str()])).collect();
        for ii in i..top {
            let mem = blk.mem.get(ii).unwrap_or_else(|| panic!("IndexError: list index out of range"));
            if mem.requires(&self.env).iter().any(|r| regs.contains(r)) {
                return true;
            }

            for r in mem.destroys(&self.env) {
                if let Some(at) = regs.iter().position(|x| x == r) {
                    regs.remove(at);
                }
            }

            if regs.is_empty() {
                return false;
            }
        }

        blk.lock.set(true);
        let result = self.goes_requires(block, &regs);
        blk.lock.set(false);

        result
    }

    /// Returns a list of registers and variables this block requires.
    pub fn requires(&self, block: usize, i: usize, end_: Option<usize>) -> BTreeSet<String> {
        let blk = &self.blocks[block];
        if let Some(dummy) = &blk.dummy {
            return dummy.requires.clone();
        }
        let len = blk.mem.len();
        let end_ = end_.filter(|e| *e <= len).unwrap_or(len);
        let mut regs: BTreeSet<&str> = BLOCK_REGS.into_iter().collect();
        let mut result = BTreeSet::new();

        for ii in i..end_ {
            for r in blk.mem[ii].requires(&self.env) {
                let r = r.to_lowercase();
                if regs.remove(r.as_str()) {
                    result.insert(r);
                }
            }

            for r in blk.mem[ii].destroys(&self.env) {
                let r = r.to_lowercase();
                regs.remove(r.as_str());
            }

            if regs.is_empty() {
                break;
            }
        }

        result
    }

    /// Returns a list of registers this block destroys. (The order within
    /// one instruction is that of a Python set there, and nothing uses it.)
    pub fn destroys(&self, block: usize, i: usize) -> Vec<String> {
        let blk = &self.blocks[block];
        if let Some(dummy) = &blk.dummy {
            return dummy.destroys.clone();
        }
        let mut regs: BTreeSet<&str> = BLOCK_REGS.into_iter().collect();
        let mut result = Vec::new();

        for ii in i..blk.mem.len() {
            for r in blk.mem[ii].destroys(&self.env) {
                if regs.remove(r.as_str()) {
                    result.push(r.clone());
                }
            }

            if regs.is_empty() {
                break;
            }
        }

        result
    }

    /// Returns whether any of the goes_to block requires any of the given
    /// registers.
    fn goes_requires(&self, block: usize, regs: &[String]) -> bool {
        self.blocks[block].goes_to.iter().any(|&other| self.is_used(other, regs, 0, None))
    }

    /// Returns the position of the memcell of the given block, which is
    /// not a LABEL.
    fn get_first_non_label_instruction(&self, block: usize) -> Option<(usize, usize)> {
        self.blocks[block].mem.iter().position(|mem| !mem.is_label()).map(|at| (block, at))
    }

    /// Return the first non label instruction to be executed, either in
    /// this block or in the following one. If there are more than one,
    /// return None. Also returns None if there is no instruction to be
    /// executed. The instruction is given as (block, position).
    pub fn get_next_exec_instruction(&self, block: usize) -> Option<(usize, usize)> {
        let mut result = self.get_first_non_label_instruction(block);
        let mut blk = block;

        while result.is_none() {
            if self.blocks[blk].goes_to.len() != 1 {
                return None;
            }

            blk = *self.blocks[blk].goes_to.iter().next().unwrap();
            result = self.get_first_non_label_instruction(blk);
        }

        result
    }

    /// Returns two dictionaries (regs, memory) that contains the common
    /// values of the cpustates of all comes_from blocks
    pub fn guesses_initial_state_from_origin_blocks(&self, block: usize) -> (HashMap<String, String>, HashMap<String, String>) {
        let mut origins = self.blocks[block].comes_from.iter();
        let Some(&first) = origins.next() else { return (HashMap::new(), HashMap::new()) };

        let mut regs = self.blocks[first].cpu.regs.clone();
        let mut mems = self.blocks[first].cpu.mem.data.clone();

        for &blk in origins {
            let other = &self.blocks[blk].cpu;
            regs.retain(|k, v| other.regs.get(k) == Some(v));
            mems.retain(|k, v| other.mem.data.get(k) == Some(v));
        }

        (regs, mems)
    }

    /// Resets and updates internal cpu state of this block executing the
    /// instructions of the block. The block must be a basic block (i.e.
    /// already partitioned)
    pub fn compute_cpu_state(&mut self, block: usize) {
        let blk = &mut self.blocks[block];
        blk.cpu.reset(None, None, &mut self.tmp);

        for mem in &blk.mem {
            blk.cpu.execute(mem.code(), &mut self.tmp);
        }
    }

    /// Tries to detect peep-hole patterns in this basic block and remove
    /// them.
    pub fn optimize_block(&mut self, block: usize, patterns_list: &[&OptPattern]) {
        if self.blocks[block].optimized {
            return;
        }

        let mut changed = true;
        let mut code = self.code(block);

        let (regs, mems) = if self.env.level > 3 { self.guesses_initial_state_from_origin_blocks(block) } else { (HashMap::new(), HashMap::new()) };

        // The block's CPU and the counter are lent to the rules' functions
        // while the rest of the graph is only read.
        let mut cpu = std::mem::take(&mut self.blocks[block].cpu);
        let mut tmp = std::mem::take(&mut self.tmp);

        let mut candidates = crate::engine::Candidates::default();
        while changed {
            changed = false;
            cpu.reset(Some(&regs), Some(&mems), &mut tmp);

            for i in 0..code.len() {
                let mut chars = LineChars::default();
                // Only the rules whose first line could match this line.
                for p in candidates.at(patterns_list, &code[i]) {
                    // HINT: {} is also a valid match
                    let Some(found) = p.patt.matches(&code, i, &mut chars) else { continue };
                    let mut vars = to_values(found);
                    let mut ctx = BlockContext { optimizer: self, cpu: &mut cpu, tmp: &mut tmp, block, pos: i + p.patt.len() };

                    for (var, expr) in &p.defines {
                        let value = expr.eval(&vars, &mut ctx);
                        vars.insert(var.clone(), value);
                    }

                    if !p.cond.eval(&vars, &mut ctx).truthy() {
                        continue;
                    }

                    // all patterns matched successfully. Apply this rule
                    let mut new_code = code.clone();
                    new_code.splice(i..i + p.patt.len(), p.template.filter(&vars));
                    changed = new_code != code;
                    if changed {
                        code = new_code;
                        self.set_code(block, &code);
                        break;
                    }
                }

                if changed {
                    self.blocks[block].modified = true;
                    break;
                }

                cpu.execute(&code[i], &mut tmp);
            }
        }

        self.blocks[block].cpu = cpu;
        self.tmp = tmp;
        self.blocks[block].optimized = true;
    }

    /// Traverses memory, to make any local label a unique global one. At
    /// this point there's only a single code block
    pub fn cleanup_local_labels(&mut self, block: usize) {
        let mut stack: Vec<Vec<String>> = vec![Vec::new()];
        let mut hashes: Vec<HashMap<String, String>> = vec![HashMap::new()];
        let mut stackprc: Vec<usize> = vec![self.proc_counter];
        // List of hashes of unresolved labels per scope: label -> the cells
        // (by position) that name it, in the order first seen.
        let mut used: Vec<Vec<(String, Vec<usize>)>> = vec![Vec::new()];

        let mut memory = std::mem::take(&mut self.blocks[block].mem);

        for at in 0..memory.len() {
            if memory[at].inst().to_uppercase() == "PROC" {
                stack.push(Vec::new());
                hashes.push(HashMap::new());
                stackprc.push(self.proc_counter);
                used.push(Vec::new());
                self.proc_counter += 1;
                continue;
            }

            if memory[at].inst().to_uppercase() == "ENDP" {
                if stack.len() > 1 {
                    // There might be unbalanced stack due to syntax errors
                    for (label, cells) in used.last().unwrap() {
                        if stack.last().unwrap().contains(label) {
                            let newlabel = &hashes.last().unwrap()[label];
                            for &cell in cells {
                                memory[cell].replace_label(label, newlabel);
                            }
                        }
                    }

                    stack.pop();
                    hashes.pop();
                    stackprc.pop();
                    used.pop();
                }
                continue;
            }

            let tmp = memory[at].code().to_string();
            if tmp.to_uppercase().starts_with("LOCAL") {
                let rest: String = tmp.chars().skip(5).collect();
                for lbl in rest.split(',') {
                    let lbl = crate::asm::py_strip(lbl).to_string();
                    if stack.last().unwrap().contains(&lbl) {
                        continue;
                    }

                    hashes.last_mut().unwrap().insert(lbl.clone(), format!("PROC{}.{lbl}", stackprc.last().unwrap()));
                    stack.last_mut().unwrap().push(lbl);
                }

                // Remove it. (Python writes ";" and the `repr` of the Asm
                // object, which nothing reads.)
                memory[at].set_asm(";LOCAL");
                continue;
            }

            if memory[at].is_label() {
                let label = memory[at].inst().to_string();
                for i in (0..stack.len()).rev() {
                    if stack[i].contains(&label) {
                        let label = &hashes[i][&label];
                        memory[at].set_asm(&format!("{label}:"));
                        break;
                    }
                }
                continue;
            }

            for label in memory[at].used_labels(&self.env) {
                let mut label_used = false;
                for i in (0..stack.len()).rev() {
                    if stack[i].contains(&label) {
                        let newlabel = hashes[i][&label].clone();
                        memory[at].replace_label(&label, &newlabel);
                        label_used = true;
                        break;
                    }
                }

                if !label_used {
                    let scope = used.last_mut().unwrap();
                    match scope.iter_mut().find(|(l, _)| *l == label) {
                        Some((_, cells)) => cells.push(at),
                        None => scope.push((label, vec![at])),
                    }
                }
            }
        }

        memory.retain(|cell| !cell.code().starts_with(';'));
        self.blocks[block].mem = memory;
    }

    fn define_label(&mut self, key: &str, info: LabelInfo) {
        if self.labels.contains_key(key) {
            panic!("DuplicatedLabelError: Duplicated label '{key}'");
        }
        self.labels.insert(key.to_string(), info);
    }

    /// Traverses memory, to annotate all the labels in the global LABELS
    /// table
    pub fn get_labels(&mut self, basic_block: usize) {
        for i in 0..self.blocks[basic_block].mem.len() {
            let cell = &self.blocks[basic_block].mem[i];
            if cell.is_label() {
                let label = cell.inst().to_string();
                let addr = cell.addr;
                // Stores it globally
                self.define_label(&label.clone(), LabelInfo { label, addr, basic_block, position: i, used_by: BTreeSet::new() });
            }
        }
    }

    /// Initializes global memory array with the one in the main (initial)
    /// basic_block
    pub fn initialize_memory(&mut self, basic_block: usize) {
        self.init();
        self.get_labels(basic_block);
    }

    // ---- flow_graph.py ----

    /// Returns the first position where a block's cells can be partitioned
    /// or None if there's no such point.
    fn first_partition_idx<'c>(&self, cells: impl ExactSizeIterator<Item = &'c Cell>) -> Option<usize> {
        let len = cells.len();
        for (i, mem) in cells.enumerate() {
            if i > 0 && mem.is_label() && self.jump_labels.contains(mem.inst()) {
                return Some(i);
            }

            if (mem.is_ender() || (self.env.is_asm)(mem.code())) && i + 1 < len {
                return Some(i + 1);
            }
        }

        None
    }

    pub fn get_first_partition_idx(&self, block: usize) -> Option<usize> {
        self.first_partition_idx(self.blocks[block].mem.iter())
    }

    /// Given the main basic block (which contain the entire program),
    /// returns the LABELS used by CALL, JR, JP (i.e JP LABEL0). Also
    /// updates the global LABELS index with the pertinent information.
    ///
    /// Any BasicBlock containing a JUMP_LABEL in any position which is not
    /// the initial one (0 position) must be split at that point into two
    /// basic blocks.
    fn get_jump_labels(&mut self, main_basic_block: usize) -> BTreeSet<String> {
        let mut jump_labels = BTreeSet::new();

        for i in 0..self.blocks[main_basic_block].mem.len() {
            let mem = &self.blocks[main_basic_block].mem[i];
            if mem.is_label() {
                let label = mem.inst().to_string();
                if self.labels.remove(&label).is_none() {
                    panic!("KeyError: '{label}'");
                }
                // position: Unknown yet
                self.define_label(&label.clone(), LabelInfo { label, addr: i, basic_block: main_basic_block, position: i, used_by: BTreeSet::new() });
                continue;
            }

            if !mem.is_ender() {
                continue;
            }

            let Some(lbl) = mem.branch_arg().map(String::from) else { continue };

            jump_labels.insert(lbl.clone());

            if !self.labels.contains_key(&lbl) {
                // INFO: lbl is not defined. No optimization is done.
                let all: Vec<&str> = ALL_REGS.to_vec();
                let block = self.new_dummy_block(&all, &all);
                self.define_label(&lbl.clone(), LabelInfo { label: lbl, addr: 0, basic_block: block, position: 0, used_by: BTreeSet::new() });
            }
        }

        jump_labels
    }

    /// Points the labels of a block's cells at it.
    fn claim_labels(&mut self, block: usize) {
        for i in 0..self.blocks[block].mem.len() {
            let mem = &self.blocks[block].mem[i];
            if mem.is_label() {
                if let Some(info) = self.labels.get_mut(mem.inst()) {
                    info.basic_block = block;
                    info.position = i;
                }
            }
        }
    }

    /// The links `_split_block` makes between a block and the new one that
    /// takes its tail, once the cells are in place.
    fn link_split(&mut self, block: usize, new_block: usize) {
        self.blocks[new_block].next = self.blocks[block].next;
        self.blocks[block].next = Some(new_block);
        self.blocks[new_block].prev = Some(block);

        if let Some(next) = self.blocks[new_block].next {
            self.blocks[next].prev = Some(new_block);
        }

        for blk in self.blocks[block].goes_to.clone() {
            self.delete_goes_to(block, Some(blk));
            self.add_goes_to(new_block, Some(blk));
        }

        self.add_goes_to(block, Some(new_block));
    }

    fn unlink_after_ender(&mut self, block: usize) {
        let last = self.blocks[block].mem.last().unwrap_or_else(|| panic!("IndexError: list index out of range"));
        if last.is_ender() && last.condition_flag().is_none_or(str::is_empty) {
            // If it's an unconditional jp, jr, call, ret
            let next = self.blocks[block].next;
            self.delete_goes_to(block, next);
        }
    }

    /// `_split_block`: the cells from `start_of_new_block` on move to a
    /// new block, which is returned.
    pub fn split_block(&mut self, block: usize, start_of_new_block: usize) -> usize {
        assert!(start_of_new_block < self.blocks[block].mem.len(), "AssertionError: Invalid split pos: {start_of_new_block}");
        // The block's own class: a zxnext block's tail is a zxnext block.
        let new_block = self.new_block(&[]);
        let tail = self.blocks[block].mem.split_off(start_of_new_block);
        self.blocks[new_block].mem = tail;
        self.link_split(block, new_block);
        self.claim_labels(new_block);
        self.unlink_after_ender(block);
        new_block
    }

    /// If a block is not partitionable, returns a list with the same
    /// block. Otherwise, returns a list with the resulting blocks.
    pub fn get_basic_blocks(&mut self, block: usize) -> Vec<usize> {
        let mut result = vec![block];
        self.jump_labels = self.get_jump_labels(block);

        // Split basic blocks per label or branch instruction. This is
        // `_split_block` over and over on the block that holds the rest of
        // the program; the rest is kept aside while that goes on, so that
        // cutting its head off does not move all of it each time, and a
        // block's labels are pointed at it once, when its cells are known.
        // (Python points every label of the rest at each new block in
        // turn; nothing looks at them in between.)
        let mut block = block;
        let mut rest: VecDeque<Cell> = std::mem::take(&mut self.blocks[block].mem).into();
        let mut first = true;
        while let Some(split_pos) = self.first_partition_idx(rest.iter()) {
            self.blocks[block].mem = rest.drain(..split_pos).collect();
            if !first {
                self.claim_labels(block);
            }
            first = false;
            let new_block = self.new_block(&[]);
            self.link_split(block, new_block);
            self.unlink_after_ender(block);
            block = new_block;
            result.push(block);
        }
        self.blocks[block].mem = rest.into();
        if !first {
            self.claim_labels(block);
        }

        self.compute_calls(&result);

        result
    }

    fn last_cell(&self, block: usize) -> &Cell {
        self.blocks[block].mem.last().unwrap_or_else(|| panic!("IndexError: list index out of range"))
    }

    fn compute_calls(&mut self, basic_blocks: &[usize]) {
        // caller -> the block its call enters, in the order found
        let mut calling_blocks: Vec<(usize, usize)> = Vec::new();

        // Compute which blocks use jump labels
        for &bb in basic_blocks {
            let last = self.last_cell(bb);
            if last.is_ender() {
                if let Some(op) = last.branch_arg().map(String::from) {
                    if let Some(info) = self.labels.get_mut(&op) {
                        info.used_by.insert(bb);
                    }
                }
            }
        }

        // For these blocks, add the referenced block in the goes_to
        for label in self.jump_labels.clone() {
            let info = self.labels.get(&label).unwrap_or_else(|| panic!("KeyError: '{label}'"));
            let target = info.basic_block;
            for bb in info.used_by.clone() {
                self.add_goes_to(bb, Some(target));
            }
        }

        // Annotate which blocks uses call (which should be the last instruction)
        for &bb in basic_blocks {
            let last = self.last_cell(bb);
            if last.inst() != "call" {
                continue;
            }

            // A call to a ZX Next banked routine names its resident
            // trampoline, not the body. Follow it through, or the body's
            // `ret` block would be left with no goes_to at all and its
            // return value taken for dead. If the body is not in this
            // listing, the trampoline itself is the next best thing.
            let mut op = last.branch_arg().map(String::from);
            if let Some(body) = op.as_deref().and_then(|op| (self.env.far_body)(op)) {
                if self.labels.contains_key(&body) {
                    op = Some(body);
                }
            }
            if let Some(info) = op.and_then(|op| self.labels.get(&op)) {
                let entry = info.basic_block;
                self.blocks[entry].called_by.insert(bb);
                calling_blocks.push((bb, entry));
            }
        }

        // Link every `ret` a callee can reach back to the instruction after
        // each of its calls. _split_block removed the call's fall-through
        // edge, so this is the only way the block after a call learns it is
        // entered from the callee.
        let mut exits: HashMap<usize, BTreeSet<usize>> = HashMap::new();
        for (caller, entry) in calling_blocks {
            if !exits.contains_key(&entry) {
                let found = self.find_exits(entry);
                exits.insert(entry, found);
            }

            let next = self.blocks[caller].next;
            for bb in exits[&entry].clone() {
                self.add_goes_to(bb, next);
            }
        }
    }

    /// Returns the blocks that can leave the routine starting at `entry`:
    /// those ending in a ret, plus any jump target that cannot be followed
    /// (e.g. `jp (hl)` or an undefined label, both of which resolve to a
    /// DummyBasicBlock whose code is a bare `ret`).
    fn find_exits(&self, entry: usize) -> BTreeSet<usize> {
        let mut result = BTreeSet::new();
        let mut visited = BTreeSet::new();
        let mut pending: Vec<Option<usize>> = vec![Some(entry)];

        while let Some(bb) = pending.pop() {
            let Some(bb) = bb else { continue };
            if !visited.insert(bb) {
                continue;
            }

            let blk = &self.blocks[bb];
            let Some(last) = blk.mem.last() else {
                pending.push(blk.next);
                continue;
            };

            if !last.is_ender() {
                // it does not branch: execution runs into the next block
                pending.push(blk.next);
                continue;
            }

            if matches!(last.inst(), "ret" | "reti" | "retn") {
                result.insert(bb);
                if last.condition_flag().is_some_and(|c| !c.is_empty()) {
                    pending.push(blk.next);
                }
                continue;
            }

            if matches!(last.inst(), "call" | "rst") {
                // a nested call returns to the next block
                pending.push(blk.next);
                continue;
            }

            // jp, jr, djnz
            if last.condition_flag().is_some_and(|c| !c.is_empty()) || last.inst() == "djnz" {
                pending.push(blk.next);
            }

            match last.branch_arg().and_then(|target| self.labels.get(target)) {
                Some(info) => pending.push(Some(info.basic_block)),
                None => {
                    result.insert(bb);
                }
            }
        }

        result
    }

    // ---- main.py ----

    /// `Optimizer.optimize` from -O3 up, between `_cleanup_mem` and the
    /// final join: the lines in, the lines out (`#pragma opt` lines still
    /// among them).
    pub fn optimize(&mut self, initial_memory: &[String]) -> Vec<String> {
        self.proc_counter = 0;

        self.clean_asm_args = self.env.level > 3;
        let bb = self.new_block(initial_memory);
        self.cleanup_local_labels(bb);
        self.initialize_memory(bb);

        // 1st partition the Basic Blocks
        let basic_blocks = self.get_basic_blocks(bb);

        // The debug trace is formatted whether or not it is shown, and
        // that asks each block what it requires and destroys: instructions
        // answer here for the first time, and keep the answer.
        for &b in &basic_blocks {
            self.requires(b, 0, None);
            self.destroys(b, 0);
        }

        let start = self.labels["*START*"].basic_block;
        self.add_goes_to(start, Some(basic_blocks[0]));
        self.blocks[start].next = Some(basic_blocks[0]);

        self.blocks[basic_blocks[0]].prev = Some(start);
        if let Some(info) = self.labels.get(END_PROGRAM_LABEL) {
            let (from, to) = (info.basic_block, self.labels["*__END_PROGRAM*"].basic_block);
            self.add_goes_to(from, Some(to));
        }

        // In O3 we simplify the graph by reducing jumps over jumps.
        // Sorted: a chain of jumps threads differently depending on which
        // link is visited first.
        for label in self.jump_labels.clone() {
            let block = self.labels[&label].basic_block;
            if self.blocks[block].dummy.is_some() {
                continue;
            }

            // The instruction that starts this block must be one of jr / jp
            let Some((fb, fi)) = self.get_next_exec_instruction(block) else { continue };
            let first = &self.blocks[fb].mem[fi];
            if !matches!(first.inst(), "jp" | "jr") {
                continue;
            }

            for blk in self.labels[&label].used_by.clone() {
                let first = &self.blocks[fb].mem[fi];
                let first_flag = first.condition_flag().filter(|c| !c.is_empty()).map(String::from);
                if first_flag.is_none() || self.last_cell(blk).condition_flag().map(String::from) == first_flag {
                    let new_label = first.opers().first().unwrap_or_else(|| panic!("IndexError: list index out of range")).clone();
                    let last = self.blocks[blk].mem.last_mut().unwrap();
                    let text = last.code().replace(&label, &new_label);
                    last.set_asm(&text);
                    self.delete_comes_from(block, Some(blk));
                    self.labels.get_mut(&label).unwrap().used_by.remove(&blk);
                    let target = self.labels.get_mut(&new_label).unwrap_or_else(|| panic!("KeyError: '{new_label}'"));
                    target.used_by.insert(blk);
                    let target = target.basic_block;
                    self.add_goes_to(blk, Some(target));
                }
            }
        }

        for &x in &basic_blocks {
            self.compute_cpu_state(x);
        }

        let arch = if self.env.zxnext { "zxnext" } else { "z80" };
        let level = self.env.level;
        let filtered_patterns_list: Vec<&OptPattern> = Engine::for_arch(arch).patterns.iter().filter(|p| level >= p.level && p.level >= 3).collect();
        for &x in &basic_blocks {
            self.optimize_block(x, &filtered_patterns_list);
        }

        // `x.comes_from == []` compares a set with a list, which is never
        // equal: no block is ever ignored.

        basic_blocks.iter().filter(|&&x| !self.blocks[x].ignored).flat_map(|&x| self.code(x)).collect()
    }
}

/// What the rules' functions see while one block is optimised: its CPU as
/// it stands before the line being matched, and the graph.
struct BlockContext<'o, 'a> {
    optimizer: &'o Optimizer<'a>,
    cpu: &'o mut CpuState,
    tmp: &'o mut TmpVals,
    block: usize,
    /// `i + len(p.patt)`: the first line after the match.
    pos: usize,
}

impl Context for BlockContext<'_, '_> {
    fn is_asm_block(&self, line: &str) -> bool {
        (self.optimizer.env.is_asm)(line)
    }

    fn tmp_vals(&mut self) -> &mut TmpVals {
        self.tmp
    }

    fn gval(&mut self, x: &Value) -> Value {
        let text = match x {
            Value::None => None,
            other => Some(other.py_str()),
        };
        match self.cpu.get(text.as_deref(), self.tmp) {
            Some(value) => Value::Str(value),
            None => Value::None,
        }
    }

    fn flagval(&mut self, x: &Value) -> Value {
        // The dictionary is built, and the default made, before the lookup.
        let c = match self.cpu.c() {
            Some(c) => c.to_string(),
            None => self.tmp.new_tmp_val(),
        };
        let z = match self.cpu.z() {
            Some(z) => z.to_string(),
            None => self.tmp.new_tmp_val(),
        };
        let default = self.tmp.new_tmp_val();
        let Value::Str(x) = x else { panic!("AttributeError: '{}' object has no attribute 'lower'", x.type_name()) };
        Value::Str(match x.to_lowercase().as_str() {
            "c" => c,
            "z" => z,
            _ => default,
        })
    }

    fn is_required(&mut self, x: &Value) -> Value {
        let Value::Str(x) = x else { panic!("TypeError: IS_REQUIRED takes a register") };
        Value::Bool(self.optimizer.is_used(self.block, std::slice::from_ref(x), self.pos, None))
    }
}

/// `helpers.dict_intersection`: given 2 dictionaries a, b, returns a new
/// one which contains the common key/pair values.
pub fn dict_intersection(dict_a: &BTreeMap<String, String>, dict_b: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    dict_a.iter().filter(|(k, v)| dict_b.get(*k) == Some(v)).map(|(k, v)| (k.clone(), v.clone())).collect()
}

/// The flow optimiser over the lines `_cleanup_mem` has tidied: what
/// `Optimizer().optimize` returns, before it is joined.
pub fn optimize(env: Env, initial_memory: &[String]) -> Vec<String> {
    Optimizer::new(env).optimize(initial_memory)
}

/// `tests/arch/zx48k/optimizer/test_basicblock.py` and `test_optimizer.py`
#[cfg(test)]
mod tests {
    use super::*;

    fn lines(code: &str) -> Vec<String> {
        code.split('\n').map(crate::asm::py_strip).filter(|x| !x.is_empty()).map(String::from).collect()
    }

    /// An optimiser for the Z80 at a level, and an empty block (`setUp`).
    fn with<R>(level: i64, test: impl FnOnce(&mut Optimizer, usize) -> R) -> R {
        let env = Env { is_asm: &|_| false, far_body: &|_| None, zxnext: false, level };
        let mut optimizer = Optimizer::new(env);
        let blk = optimizer.new_block(&[]);
        test(&mut optimizer, blk)
    }

    fn partition(code: &str, initialize: bool, test: impl FnOnce(&Optimizer, &[usize])) {
        with(2, |o, blk| {
            o.set_code(blk, &lines(code));
            if initialize {
                o.initialize_memory(blk);
            }
            let blks = o.get_basic_blocks(blk);
            test(o, &blks);
        })
    }

    fn goes(o: &Optimizer, from: usize, to: usize) -> bool {
        let there = o.blocks[from].goes_to.contains(&to);
        assert_eq!(there, o.blocks[to].comes_from.contains(&from));
        there
    }

    #[test]
    fn block_partition() {
        for (ender, falls_through) in [("jp __UNKNOWN", false), ("call __UNKNOWN", false), ("jp z, __UNKNOWN", true), ("call z, __UNKNOWN", true)] {
            partition(&format!("nop\n{ender}\nnop"), false, |o, blks| {
                assert_eq!(blks.len(), 2);
                assert_eq!(o.code(blks[0]), ["nop", ender]);
                assert_eq!(o.code(blks[1]), ["nop"]);
                assert_eq!(goes(o, blks[0], blks[1]), falls_through, "{ender}");
            });
        }
    }

    #[test]
    fn empty_basic_block_is_false() {
        with(2, |o, blk| assert!(o.blocks[blk].mem.is_empty()));
    }

    #[test]
    fn call_part() {
        partition("my_block:\nld a, 3\nret\nld a, 1\ncall my_block\nld a, 2", true, |o, blks| {
            assert_eq!(blks.len(), 3);
            assert_eq!(o.code(blks[0]), ["my_block:", "ld a, 3", "ret"]);
            assert_eq!(o.code(blks[1]), ["ld a, 1", "call my_block"]);
            assert_eq!(o.code(blks[2]), ["ld a, 2"]);
            assert!(goes(o, blks[1], blks[0]));
            assert!(goes(o, blks[0], blks[2]));
        });
    }

    #[test]
    fn call_ret2() {
        partition("ld a, 1\ncall my_block\nld a, 2\nret\nmy_block:\nld a, 3\nret", true, |o, blks| {
            assert_eq!(blks.len(), 3);
            assert_eq!(o.code(blks[0]), ["ld a, 1", "call my_block"]);
            assert_eq!(o.code(blks[1]), ["ld a, 2", "ret"]);
            assert_eq!(o.code(blks[2]), ["my_block:", "ld a, 3", "ret"]);
            assert!(goes(o, blks[0], blks[2]));
            assert!(goes(o, blks[2], blks[1]));
            assert!(o.blocks[blks[1]].goes_to.is_empty());
        });
    }

    /// A callee that branches before its ret must still return to the
    /// caller. The walk used to stop at the first jp, leaving the block
    /// after the call reached only from the jump that skipped it.
    #[test]
    fn call_ret_after_branch() {
        let code = "
        ld a, (_g)
        or a
        jp z, __LABEL3
        call my_sub
    __LABEL3:
        ld a, 2
        ret
    my_sub:
        ld a, (_g)
        jp nz, __LABEL0
        ld hl, 1
        jp __LABEL1
    __LABEL0:
        ld hl, 2
    __LABEL1:
        ret
        ";
        partition(code, true, |o, blks| {
            assert_eq!(o.code(blks[1]), ["call my_sub"]);
            assert_eq!(o.code(blks[2]), ["__LABEL3:", "ld a, 2", "ret"]);
            assert_eq!(o.code(blks[6]), ["__LABEL1:", "ret"]);
            assert!(goes(o, blks[6], blks[2]));
            assert!(goes(o, blks[0], blks[2]));
        });
    }

    /// The return from a nested call carries on inside the callee.
    #[test]
    fn call_ret_after_nested_call() {
        partition("call my_sub\nld a, 2\nret\nmy_sub:\ncall __UNKNOWN\nld hl, 1\nret", true, |o, blks| {
            assert_eq!(o.code(blks[1]), ["ld a, 2", "ret"]);
            assert_eq!(o.code(blks[3]), ["ld hl, 1", "ret"]);
            assert!(goes(o, blks[3], blks[1]));
        });
    }

    /// A callee leaving by `jp (hl)` cannot be followed, so it must be
    /// taken as possibly returning, not as never returning.
    #[test]
    fn call_ret_through_indirect_jump() {
        partition("call my_sub\nld a, 2\nret\nmy_sub:\npop hl\njp (hl)", true, |o, blks| {
            assert_eq!(o.code(blks[1]), ["ld a, 2", "ret"]);
            assert!(!o.blocks[blks[1]].comes_from.is_empty());
        });
    }

    #[test]
    fn long_block() {
        let code = "
        ld a, 0
        jp __LABEL2
    __LABEL0:
        ld a, 1
        jp z, __LABEL1
        ld a, 2
    __LABEL1:
        ld a, 3
    __LABEL2:
        ld a, 4
        jp nc, __LABEL0
        ld a, 5
        ";
        partition(code, true, |o, blks| {
            assert_eq!(blks.len(), 6);
            assert_eq!(o.code(blks[0]), ["ld a, 0", "jp __LABEL2"]);
            assert_eq!(o.code(blks[1]), ["__LABEL0:", "ld a, 1", "jp z, __LABEL1"]);
            assert_eq!(o.code(blks[2]), ["ld a, 2"]);
            assert_eq!(o.code(blks[3]), ["__LABEL1:", "ld a, 3"]);
            assert_eq!(o.code(blks[4]), ["__LABEL2:", "ld a, 4", "jp nc, __LABEL0"]);
            assert_eq!(o.code(blks[5]), ["ld a, 5"]);
            assert!(goes(o, blks[0], blks[4]));
            assert!(goes(o, blks[4], blks[1]));
            assert!(goes(o, blks[3], blks[4]));
            assert!(goes(o, blks[4], blks[5]));
            assert!(!goes(o, blks[2], blks[1]));
            assert!(!goes(o, blks[3], blks[1]));
            assert!(o.blocks[blks[5]].goes_to.is_empty());
        });
    }

    #[test]
    fn basic_block_clean_ld_hl() {
        with(4, |o, blk| {
            o.clean_asm_args = true;
            o.set_code(blk, &lines("ld hl, (30001 - 1)"));
            assert_eq!(o.code(blk), ["ld hl, (30000)"]);
        });
    }

    #[test]
    fn mempos_requires() {
        with(2, |o, blk| {
            o.set_code(blk, &lines("ld hl, (_k - 1)\nld (_k), a"));
            assert!(o.is_used(blk, &["(_k)".to_string()], 0, None));
        });
    }

    #[test]
    fn is_used_or_and_xor_ix() {
        for inst in ["or", "and", "xor"] {
            with(2, |o, blk| {
                o.set_code(blk, &lines(&format!("{inst} (ix-1)\nld (ix-1), a")));
                assert!(o.is_used(blk, &["(ix-1)".to_string()], 0, None));
            });
        }
    }

    #[test]
    fn loop_goes_and_comes() {
        let code = "
        ld (_dir), hl
        .LABEL.__LABEL0:
        ld a, (_n)
        ld hl, (_dir)
        ld (hl), a
        ld hl, _n
        inc (hl)
        jp .LABEL.__LABEL0
        ld (ix-1), a
        ";
        partition(code, true, |o, blks| {
            assert_eq!(blks.len(), 3);
            let only_b2: BTreeSet<usize> = [blks[1]].into();
            assert_eq!(o.blocks[blks[0]].goes_to, only_b2);
            assert_eq!(o.blocks[blks[1]].goes_to, only_b2);
            assert!(o.blocks[blks[2]].comes_from.is_empty());
        });
    }

    fn optimized(level: i64, code: &str) -> Vec<String> {
        optimize(Env { is_asm: &|_| false, far_body: &|_| None, zxnext: false, level }, &lines(code))
    }

    #[test]
    fn unrequired_or_a() {
        let code = "
        call .core.__LTI8
        or a
        ld bc, 0
        di
        ld hl, (.core.__CALL_BACK__)
        ld sp, hl
        exx
        pop hl
        pop iy
        pop ix
        exx
        ei
        ret
        ";
        assert_eq!(optimized(4, code)[..2], ["call .core.__LTI8", "ld bc, 0"]);
    }

    #[test]
    fn ld_sp_requires_sp() {
        assert_eq!(optimized(4, "ld sp, hl\npop iy")[..2], ["ld sp, hl", "pop iy"]);
    }

    #[test]
    fn hd_sp_requires_sp() {
        assert_eq!(optimized(3, "add hl, sp\npop iy\njp (hl)"), ["add hl, sp", "pop iy", "jp (hl)"]);
    }

    /// `if g then Other() : Take(a(s))` at -O4. The label after the call is
    /// also reached from `jp z`, where HL is still `s` zero-extended;
    /// Other() changes HL. The index widening must survive, or a(s) is
    /// read at HL.
    #[test]
    fn label_after_call_keeps_registers() {
        let code = "
        _Other:
        ld a, (_g)
        sub 3
        jp nz, .LABEL.__LABEL0
        ld hl, (_w)
        ld de, 12345
        add hl, de
        ld (_w), hl
        jp .LABEL.__LABEL1
        .LABEL.__LABEL0:
        ld hl, (_w)
        dec hl
        ld (_w), hl
        .LABEL.__LABEL1:
        ld a, (_g)
        ld (30000), a
        ret
        _Test:
        ld a, (_s)
        ld l, a
        ld h, 0
        ld (_w), hl
        ld a, (_g)
        or a
        jp z, .LABEL.__LABEL3
        call _Other
        .LABEL.__LABEL3:
        ld a, (_s)
        ld l, a
        ld h, 0
        push hl
        ld hl, _a
        call .core.__ARRAY
        ld a, (hl)
        ret
        ";
        let optimized_code = optimized(4, code);
        let i = optimized_code.iter().position(|x| x == ".LABEL.__LABEL3:").unwrap();
        assert_eq!(optimized_code[i + 1..i + 5], ["ld a, (_s)", "ld l, a", "ld h, 0", "push hl"]);
    }

    /// Not in the Python suite: LOCAL labels are given a name of their
    /// own, where the instruction spells them as declared.
    #[test]
    fn local_labels() {
        let code = "PROC\nLOCAL LOOP\nLOOP:\ndjnz LOOP\nENDP\nret";
        assert_eq!(optimized(3, code), ["PROC", "PROC0.LOOP:", "djnz PROC0.LOOP", "ENDP", "ret"]);
    }

    /// A renamed label is still entered in the table under the name it
    /// had (the cell's `inst` was asked before), so the same LOCAL name
    /// in two PROCs ends the Python optimiser, and this one.
    #[test]
    #[should_panic(expected = "Duplicated label 'LOOP'")]
    fn local_labels_twice() {
        optimized(3, "PROC\nLOCAL LOOP\nLOOP:\ndjnz LOOP\nENDP\nPROC\nLOCAL LOOP\nLOOP:\njp nz, LOOP\nENDP\nret");
    }
}
