//! The end of a compile: what `zxbc.py`'s `main()` does once the
//! translators have filled the backend with quads.
//!
//! In order: the code is emitted ([`emit_code`]) and, at -O3 and above,
//! handed to the flow optimiser; the inline assembly parked under `##ASMn`
//! tokens is put back; the whole text goes through the preprocessor a second
//! time, in ASM mode, which is where the runtime library is pulled in
//! ([`preprocess`]); `#init` lines are collected ([`get_inits`]); the global
//! variables are emitted, and then those of each code bank ([`emit_vars`],
//! [`emit_banked_vars`]); and everything is spliced between the prologue and
//! the epilogue ([`splice`]) and written out ([`output`]).
//!
//! The variable passes are translated by the front end *after* the code has
//! been emitted, and both sides draw temporary labels from one counter, so
//! each step here is a function of its own for the front end to call in
//! turn.

use crate::common::{DATA_END_LABEL, MAIN_LABEL};
use crate::Backend;
use std::sync::OnceLock;
use zxr_core::Session;
use zxr_ir::Quad;
use zxr_pp::{Mode, Preprocessor};
use zxr_re::{Flags, Regex};

fn regex(cell: &'static OnceLock<Regex>, pattern: &str, flags: Flags) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern, flags).unwrap())
}

/// `optimizer/patterns.py`: a label definition (simply an identifier ending
/// with a colon). The identifier has no dots and no underscore after its
/// first character.
fn re_label() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    regex(&RE, r"^[ \t]*[_a-zA-Z][a-zA-Z\d]*[ \t]*:", Flags::NONE)
}

/// `optimizer/patterns.py`: a pragma line.
fn re_pragma() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    regex(&RE, r"^#[ \t]?pragma[ \t]opt[ \t]", Flags::NONE)
}

fn re_init() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    regex(&RE, r#"^#[ \t]*init[ \t]+((?:[._a-zA-Z][._a-zA-Z0-9]*)|(?:"[._a-zA-Z][._a-zA-Z0-9]*"))[ \t]*$"#, Flags { ignore_case: true, ..Flags::NONE })
}

fn py_strip(text: &str) -> &str {
    text.trim_matches(|c: char| c.is_whitespace() || ('\x1c'..='\x1f').contains(&c))
}

/// `Optimizer._cleanup_mem`: cleans up initial memory. Each label must be
/// ALONE. Each instruction must have an space, etc...
pub fn cleanup_mem(initial_memory: &mut Vec<String>) {
    let mut i = 0;
    while i < initial_memory.len() {
        let tmp = initial_memory[i].clone();
        let chars: Vec<char> = tmp.chars().collect();

        if let Some(found) = re_label().match_at(&chars, 0) {
            let group: String = chars[..found.end].iter().collect();
            let rstripped = tmp.trim_end_matches(|c: char| c.is_whitespace() || ('\x1c'..='\x1f').contains(&c));
            if rstripped != group {
                let rest: String = chars[found.end..].iter().collect();
                initial_memory[i] = py_strip(&rest).to_string();
                initial_memory.insert(i, format!("{}:", py_strip(&group[..group.len() - 1])));
            }
        }

        i += 1;
    }
}

/// What `Optimizer.optimize` does up to -O2: nothing beyond tidying labels
/// and dropping the pragmas meant for the flow optimiser.
pub fn optimize_up_to_o2(mut initial_memory: Vec<String>) -> String {
    cleanup_mem(&mut initial_memory);
    join_without_pragmas(initial_memory.iter())
}

/// The lines as one text, without `#pragma opt` lines: how every level of
/// `Optimizer.optimize` hands its result back.
pub fn join_without_pragmas<'a>(lines: impl Iterator<Item = &'a String>) -> String {
    lines.filter(|x| re_pragma().match_str(x).is_none()).map(String::as_str).collect::<Vec<_>>().join("\n")
}

/// The flow optimiser of -O3 and -O4: given the backend (for its parked
/// inline assembly and code-bank registries) and the lines `emit` produced,
/// returns the optimised text. It is expected to call [`cleanup_mem`] first
/// and to finish with [`join_without_pragmas`], as the Python one does.
pub type FlowOptimizer<'a> = &'a mut dyn FnMut(&Backend, Vec<String>) -> String;

/// `Optimizer.optimize` at -O3 and above, given what the optimiser has to
/// know of the compile (see [`zxr_opt::flow::Env`]).
pub fn flow_optimize_with(env: zxr_opt::flow::Env, mut initial_memory: Vec<String>) -> String {
    cleanup_mem(&mut initial_memory);
    let lines = zxr_opt::flow::optimize(env, &initial_memory);
    join_without_pragmas(lines.iter())
}

/// The flow optimiser of -O3 and -O4 over the lines `emit` produced.
pub fn flow_optimize(backend: &Backend, initial_memory: Vec<String>) -> String {
    let common = &backend.common;
    let env = zxr_opt::flow::Env {
        is_asm: &|line| common.asms.contains_key(line),
        far_body: &|label| common.codebank.far_body(label).map(String::from),
        zxnext: common.options.architecture == "zxnext",
        level: common.options.optimization_level,
    };
    flow_optimize_with(env, initial_memory)
}

/// Emits the code and puts the inline assembly back: the text that goes to
/// the preprocessor. At -O3 and above the code goes through the flow
/// optimiser: the one given, which is how a test can put the oracle's
/// output in its place, or else [`flow_optimize`].
pub fn emit_code(backend: &mut Backend, flow_optimizer: Option<FlowOptimizer>) -> String {
    let level = backend.common.options.optimization_level;
    // Join all lines into a single string and ensures an INTRO at end of file
    // ZXR_TIMINGS=1 splits the stage: quads to assembly with the peephole
    // rules of levels 1 and 2, then the flow optimiser.
    let timings = std::env::var_os("ZXR_TIMINGS").is_some();
    let start = std::time::Instant::now();
    let lines = backend.emit(level > 0);
    let emitted = start.elapsed();
    // For debug data: the lines as they are now, to find them again in what
    // the optimisers make of them.
    let before = backend.origins.is_some().then(|| lines.clone());
    let asm_output = match flow_optimizer {
        Some(optimize) if level > 2 => optimize(backend, lines),
        None if level > 2 => flow_optimize(backend, lines),
        _ => optimize_up_to_o2(lines),
    };
    if timings {
        eprintln!("zxbc: timing:   emit and peephole            {:8.1} ms", emitted.as_secs_f64() * 1000.0);
        eprintln!("zxbc: timing:   flow optimiser               {:8.1} ms", (start.elapsed() - emitted).as_secs_f64() * 1000.0);
    }
    if let (Some(before), Some(origins)) = (before, backend.origins.take()) {
        let after: Vec<String> = asm_output.split('\n').map(String::from).collect();
        let origins = follow(&before, &origins, &after);
        return expand_with_marks(backend, &after, &origins) + "\n";
    }
    expand_text(backend, &(asm_output + "\n"))
}

/// A comment that begins so tells the assembler which source line the code
/// below it was generated for: a number in the front end's table, 0 for
/// none. (`zxr_asm::lexer::PIN`.)
pub const SOURCE_MARK: &str = ";@sld ";

/// How far apart a line and the same line in the changed text are looked
/// for.
const FOLLOW_WINDOW: usize = 64;

/// The source line of each line of `after`, given those of `before`, of
/// which `after` is a changed copy: lines removed, added or rewritten here
/// and there, in the same order. A line found again keeps its source line;
/// one that is new takes that of the line it stands in place of, or of the
/// line before it.
pub fn follow(before: &[String], origins: &[u32], after: &[String]) -> Vec<u32> {
    let origin = |i: usize| origins.get(i).copied().unwrap_or(0);
    let mut out = Vec::with_capacity(after.len());
    let (mut i, mut j) = (0, 0);
    while j < after.len() {
        if i < before.len() && before[i] == after[j] {
            out.push(origin(i));
            i += 1;
            j += 1;
            continue;
        }
        // The nearest pair of equal lines ahead.
        let mut found = None;
        'search: for distance in 1..=2 * FOLLOW_WINDOW {
            for a in distance.saturating_sub(FOLLOW_WINDOW)..=distance.min(FOLLOW_WINDOW) {
                let b = distance - a;
                if i + a < before.len() && j + b < after.len() && before[i + a] == after[j + b] {
                    found = Some((a, b));
                    break 'search;
                }
            }
        }
        let last = out.last().copied().unwrap_or(0);
        match found {
            Some((a, b)) => {
                for t in 0..b {
                    out.push(if a == b { origin(i + t) } else if a > 0 { origin(i + t.min(a - 1)) } else { last });
                }
                i += a;
                j += b;
            }
            None => {
                out.push(if i < before.len() { origin(i) } else { last });
                i = (i + 1).min(before.len());
                j += 1;
            }
        }
    }
    out
}

/// [`expand_text`] for debug data: before each run of generated lines, a
/// mark saying which source line they are for. Parked asm brings the
/// `#line` of its own source with it.
fn expand_with_marks(backend: &Backend, lines: &[String], origins: &[u32]) -> String {
    let mut out: Vec<String> = Vec::with_capacity(lines.len() + lines.len() / 4);
    let mut marked = None;
    for (line, &origin) in lines.iter().zip(origins) {
        match backend.common.asms.get(line.as_str()) {
            Some(parked) => {
                out.push(parked.join("\n"));
                marked = None;
            }
            None => {
                if marked != Some(origin) {
                    out.push(format!("{SOURCE_MARK}{origin}"));
                    marked = Some(origin);
                }
                out.push(line.clone());
            }
        }
    }
    out.join("\n")
}

/// The `##ASMn` substitution over a text: each line that is a parked block's
/// token becomes the block.
pub fn expand_text(backend: &Backend, asm_output: &str) -> String {
    let lines: Vec<String> = asm_output
        .split('\n')
        .map(|line| match backend.common.asms.get(line) {
            Some(parked) => parked.join("\n"),
            None => line.to_string(),
        })
        .collect();
    lines.join("\n")
}

/// Filters the generated code against the preprocessor again, in ASM mode.
/// The macro table is emptied down to what the options define, which the
/// caller must have brought up to date (`set_option_defines`); what the
/// first pass included once stays included.
pub fn preprocess(pp: &mut Preprocessor, asm_output: &str, input_filename: &str) -> Vec<String> {
    pp.reset_table();
    pp.set_mode(Mode::Asm);
    pp.output.clear();
    pp.filter(asm_output, input_filename, "INITIAL");
    pp.output.split('\n').map(String::from).collect()
}

/// Finds out remaining inits: each `#init` line of the preprocessed code
/// names a routine to call at start-up, and is blanked.
pub fn get_inits(backend: &mut Backend, memory: &mut [String]) {
    // `backend.INITS.union(zxbparser.INITS)` in the Python code discards
    // its result: the `#init`s of the BASIC source are not added here.
    for m in memory.iter_mut() {
        let chars: Vec<char> = m.chars().collect();
        if let Some(found) = re_init().match_at(&chars, 0) {
            let name = found.group(&chars, 1).unwrap_or_default();
            backend.common.inits.insert(name.trim_matches('"').to_string());
            m.clear();
        }
    }
}

/// Substitutes parked inline-asm text back into a run of emitted lines.
fn expand_inline_asm(backend: &Backend, lines: Vec<String>) -> Vec<String> {
    let mut output = Vec::new();
    for line in lines {
        match backend.common.asms.get(&line) {
            Some(parked) => output.extend(parked.iter().cloned()),
            None => output.push(line),
        }
    }
    output
}

/// True for a line of emitted variable storage worth keeping. The variable
/// passes emit preprocessor directives that are noise in the final
/// assembly, but `#line` is not: it is what attributes generated storage to
/// the BASIC declaration it came from.
fn keep_var_line(line: &str) -> bool {
    let stripped = py_strip(line);
    if stripped.is_empty() {
        return false;
    }

    !stripped.starts_with('#') || stripped.starts_with("#line")
}

/// Emits a pass of variable declarations: the quads a `VarTranslator` made
/// of the globals of one bank (bank 0 is the resident variable area).
pub fn emit_vars(backend: &mut Backend, quads: Vec<Quad>) -> Vec<String> {
    backend.memory = quads;
    let lines = backend.emit(false);
    expand_inline_asm(backend, lines).into_iter().filter(|x| keep_var_line(x)).collect()
}

/// One code bank's variables, as the front end translated them.
pub struct BankVars {
    pub bank: i64,
    pub quads: Vec<Quad>,
    /// Line and file of the first declaration in the bank, to report a bank
    /// that nothing pages in against.
    pub lineno: u32,
    pub filename: String,
}

/// Emits the storage for variables and arrays bound to a ZX Next code bank:
/// one pass per bank, each fenced by `CODEBANK` directives for the
/// assembler. `translate` is called for each bank in turn, right before its
/// pass, so that the front end translates it with the counters as they
/// stand; it returns `None` when there are no more banks.
pub fn emit_banked_vars(backend: &mut Backend, sess: &mut Session, mut translate: impl FnMut(&mut Backend) -> Option<BankVars>) -> Vec<String> {
    let mut output: Vec<String> = Vec::new();
    while let Some(pass) = translate(backend) {
        let lines = emit_vars(backend, pass.quads);
        if !lines.iter().any(|x| !py_strip(x).starts_with('#')) {
            continue; // everything in this bank was dropped as unused
        }

        warn_if_no_routines(backend, sess, pass.bank, pass.lineno, &pass.filename);

        output.push(format!("CODEBANK {}", pass.bank));
        output.extend(lines);
    }

    if !output.is_empty() {
        output.push("CODEBANK 0".to_string());
    }

    output
}

/// Warns that a bank holds data but nothing that will ever page it in.
/// Deduplicated, because a bank may reach this from both a diverted asm
/// block and a bank-local variable, and one warning is enough.
pub fn warn_if_no_routines(backend: &mut Backend, sess: &mut Session, bank: i64, lineno: u32, fname: &str) {
    let codebank = &mut backend.common.codebank;
    if codebank.far_memory_used || codebank.banks_used.contains(&bank) || codebank.no_routine_warned.contains(&bank) {
        return;
    }

    codebank.no_routine_warned.insert(bank);
    let message = format!("CODEBANK {bank} holds data but no SUB or FUNCTION, so nothing ever pages it in. Bank-local data is only reachable from routines in the same bank");
    sess.warning_coded("910", lineno, &message, Some(fname));
}

/// The whole program: prologue, resident variables, the two labels that
/// close the data area and open the code, the code, the banks' variables
/// and the epilogue. The prologue is built here, last, because it depends on
/// everything the earlier steps found out.
pub fn splice(backend: &Backend, vars: Vec<String>, code: Vec<String>, bank_vars: Vec<String>) -> Vec<String> {
    let mut asm_output = backend.emit_prologue();
    asm_output.extend(vars);
    asm_output.push(format!("{DATA_END_LABEL}:"));
    asm_output.push(format!("{MAIN_LABEL}:"));
    asm_output.extend(code);
    asm_output.extend(bank_vars);
    asm_output.extend(backend.emit_epilogue());
    asm_output
}

/// Filters the output removing useless preprocessor #directives and gives
/// the text of the assembly file.
pub fn output(memory: &[String]) -> String {
    let mut out = String::new();
    for m in memory {
        let m = m.trim_end_matches(['\r', '\n', '\t', ' ']); // Ensures no trailing newlines (might with upon includes)
        if m.starts_with('#') {
            // Preprocessor directive?
            out.push_str(m);
            out.push('\n');
            continue;
        }

        // Prints a 4 spaces "tab" for non labels
        if !m.is_empty() && !m.contains(':') {
            out.push('\t');
        }

        out.push_str(m);
        out.push('\n');
    }
    out
}
