//! The second half of `main()` in `src/zxbc/zxbc.py`: from the quads the
//! translators left in `Compiler::memory` to the files a compile writes.
//!
//! The Python compiler keeps everything both halves touch in module
//! globals. Here the front end's side is on `zxr_syntax::Compiler` and the
//! backend's on `zxr_z80::Backend`, so the shared state is handed across at
//! each point where control changes sides ([`hand_over`], [`hand_back`]):
//! the counter of temporary labels above all, which the translators and the
//! backend (`lvard`, `larrd`, the shifts) both draw from, and the code-bank
//! registries.

use crate::zxbc_args::ZxbcArgs;
use std::io::Write;
use zxr_asm::{AsmOptions, Assembler};
use zxr_core::paths;
use zxr_pp::{Preprocessor, Suspended};
use zxr_syntax::ast::NodeId;
use zxr_syntax::options::OptValue;
use zxr_syntax::Compiler;
use zxr_z80::finish;
use zxr_z80::{Backend, Options};

fn fail(message: &str) -> i32 {
    eprintln!("zxbc: {message}");
    1
}

/// The options the backend reads, as the parse (and so every `#pragma`)
/// left them.
fn backend_options(c: &Compiler) -> Options {
    let int = |name: &str| c.opts.int(name) as i64;
    Options {
        architecture: c.opts.str("architecture").to_string(),
        org: int("org"),
        heap_size: int("heap_size"),
        heap_address: match c.opts.get("heap_address") {
            OptValue::None => None,
            _ => Some(int("heap_address")),
        },
        heap_start_label: c.opts.str("heap_start_label").to_string(),
        heap_size_label: c.opts.str("heap_size_label").to_string(),
        headerless: c.opts.bool("headerless"),
        autorun: c.opts.bool("autorun"),
        optimization_level: int("optimization_level"),
        opt_strategy: c.opts.get("opt_strategy").to_string(),
        codewindow: int("codewindow"),
        codewindowsize: int("codewindowsize"),
        codebankbase: int("codebankbase"),
        codebankpages: c.opts.str("codebankpages").to_string(),
        codebankdepth: int("codebankdepth"),
    }
}

/// The front end has finished a pass: its quads and what it knows go to the
/// backend.
fn hand_over(c: &mut Compiler, backend: &mut Backend) {
    backend.memory = std::mem::take(&mut c.memory);
    let common = &mut backend.common;
    common.tmp_labels.advance_to(c.label_counter as u64);
    common.requires.extend(c.requires.iter().cloned());
    common.inits.extend(c.backend_inits.iter().cloned());
    common.codebank.far_trampolines = c.far_trampolines.iter().map(|(label, body)| (label.to_string(), body.clone())).collect();
    common.codebank.banks_used.extend(&c.banks_used);
    common.codebank.banks_with_data.extend(&c.banks_with_data);
    common.codebank.no_routine_warned.extend(&c.no_routine_warned);
    common.codebank.far_memory_used = c.far_memory_used;
}

/// The backend has emitted a pass: the labels it took are taken for the
/// front end too.
fn hand_back(c: &mut Compiler, backend: &Backend) {
    while (c.label_counter as u64) < backend.common.tmp_labels.counter {
        c.tmp_label();
    }
    c.no_routine_warned.extend(&backend.common.codebank.no_routine_warned);
}

/// `set_option_defines()`: sets some macros automatically, according to
/// options.
fn set_option_defines(c: &mut Compiler) {
    if c.opts.bool("memory_check") {
        c.define("__MEMORY_CHECK__", "");
    }
    if c.opts.bool("array_check") {
        c.define("__CHECK_ARRAY_BOUNDARY__", "");
    }
    if c.opts.bool("enable_break") {
        c.define("__ENABLE_BREAK__", "");
    }
    // ZX Next: a 16K code window spans two MMU slots. Set here because
    // `#pragma codewindowsize` is applied while the source is being parsed.
    if c.opts.int("codewindowsize") > 8192 {
        c.define("__FAR_WINDOW_16K__", "");
    }
    let strategy = c.opts.get("opt_strategy").to_string();
    c.define("__OPT_STRATEGY__", &strategy);
}

/// `emit_banked_vars()`: the storage for variables and arrays bound to a ZX
/// Next code bank, one extra pass per bank, each fenced by `CODEBANK`
/// directives for the assembler.
fn emit_banked_vars(c: &mut Compiler, backend: &mut Backend, data_ast: NodeId) -> Vec<String> {
    // Bank -> the first declaration in it, kept only to report a line number
    let mut declared_in: Vec<(i64, NodeId)> = Vec::new();
    for &child in c.children(data_ast) {
        let Some(entry) = child.and_then(|child| c.child(child, 0)) else { continue };
        let bank = c.bank_of_var(Some(entry));
        if bank != 0 && !declared_in.iter().any(|&(b, _)| b == bank) {
            declared_in.push((bank, entry));
        }
    }
    declared_in.sort_by_key(|&(bank, _)| bank);

    let mut output: Vec<String> = Vec::new();
    for (bank, entry) in declared_in {
        c.memory.clear();
        c.translate_vars(data_ast, bank);
        hand_over(c, backend);
        let quads = std::mem::take(&mut backend.memory);
        let lines = finish::emit_vars(backend, quads);
        hand_back(c, backend);
        if !lines.iter().any(|x| !x.trim().starts_with('#')) {
            continue; // everything in this bank was dropped as unused
        }

        let (lineno, filename) = (c.lineno(entry), c.id(entry).filename.to_string());
        c.warn_if_no_routines(bank, lineno, Some(&filename));

        output.push(format!("CODEBANK {bank}"));
        output.extend(lines);
    }

    if !output.is_empty() {
        output.push("CODEBANK 0".to_string());
    }

    output
}

/// Finishes the compile `c` has translated the code of. `pp` is the
/// preprocessor as the BASIC source left it.
pub fn run(c: &mut Compiler, pp: Suspended, args: &ZxbcArgs, program: &str, lib_root: &str) -> i32 {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| compile(c, pp, args, program, lib_root)));
    std::panic::set_hook(previous);
    match result {
        Ok(code) => code,
        // An exit requested through the session is not ours to report.
        Err(payload) if payload.is::<zxr_core::session::ExitRequest>() => std::panic::resume_unwind(payload),
        Err(payload) => {
            let _ = c.sess.stderr.flush();
            let message = payload.downcast_ref::<String>().cloned().or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            eprintln!("zxbc: internal error in the backend: {message}");
            70
        }
    }
}

fn compile(c: &mut Compiler, pp: Suspended, args: &ZxbcArgs, program: &str, lib_root: &str) -> i32 {
    let Some(data_ast) = c.data_ast else { return fail("no data tree") };
    let mut backend = Backend::new(backend_options(c));
    if args.sld.is_some() {
        backend.origins = Some(Vec::new());
    }
    backend.common.flag_use_function_exit = c.flag_use_function_exit;
    backend.common.flag_end_emitted = c.flag_end_emitted;
    backend.common.at_end = c.at_end.clone();
    let level = backend.common.options.optimization_level;

    // What a flow optimiser would have made of the code, if one is standing in.
    let stand_in = match &args.flow_stand_in {
        Some(prefix) if level > 2 => {
            let read = |ext: &str| std::fs::read_to_string(format!("{prefix}.{ext}")).map_err(|e| format!("{prefix}.{ext}: {e}"));
            match read("opt-in").and_then(|input| Ok((input, read("opt-out")?))) {
                Ok(texts) => Some(texts),
                Err(error) => return fail(&error),
            }
        }
        _ => None,
    };

    // Join all lines into a single string and ensures an INTRO at end of file
    hand_over(c, &mut backend);
    let mut mismatch = None;
    let asm_output = match &stand_in {
        Some((expected_in, out)) => finish::emit_code(
            &mut backend,
            Some(&mut |_, lines: Vec<String>| {
                if lines.join("\n") != *expected_in {
                    mismatch = Some(lines.join("\n"));
                }
                out.clone()
            }),
        ),
        None => finish::emit_code(&mut backend, None),
    };
    crate::stage_done("backend and peephole");
    hand_back(c, &backend);
    if let Some(lines) = mismatch {
        // Leave what was produced beside the expected lines, to compare.
        let path = format!("{}.opt-in.port", args.flow_stand_in.as_deref().unwrap_or_default());
        let _ = std::fs::write(&path, lines);
        return fail(&format!("the lines handed to the flow optimiser differ from the oracle's (written to {path})"));
    }

    // Now filter them against the preprocessor again
    set_option_defines(c); // Needed for zxbpp.init()
    c.sess.options.defines = c.defines.clone();
    let mut asm_output = {
        let mut pp = Preprocessor::resume(&mut c.sess, pp);
        finish::preprocess(&mut pp, &asm_output, program)
    };
    finish::get_inits(&mut backend, &mut asm_output); // Find out remaining inits
    crate::stage_done("second preprocessor pass");

    // This will fill MEMORY with global declared variables
    c.memory.clear();
    c.variable_visitor(data_ast);
    c.translate_vars(data_ast, 0);
    if c.sess.has_errors > 0 {
        let _ = c.sess.stderr.flush();
        return 1; // Exit with errors
    }
    hand_over(c, &mut backend);
    let quads = std::mem::take(&mut backend.memory);
    let tmp = finish::emit_vars(&mut backend, quads);
    hand_back(c, &backend);

    // ZX Next: variables declared inside a CODEBANK block are emitted into
    // that bank's 8K page instead of the resident variable area.
    let bank_vars = emit_banked_vars(c, &mut backend, data_ast);
    // The prologue reads the registries as the last pass left them.
    hand_over(c, &mut backend);

    let mut asm_output = asm_output;
    if args.sld.is_some() {
        // Neither what follows the code nor what goes before it is of any
        // source line.
        asm_output.push(format!("{}0", finish::SOURCE_MARK));
    }
    let mut asm_output = finish::splice(&backend, tmp, asm_output, bank_vars);
    if args.sld.is_some() {
        asm_output.insert(0, format!("{}0", finish::SOURCE_MARK));
    }
    let text = finish::output(&asm_output);
    crate::stage_done("variables and splice");
    let _ = c.sess.stderr.flush();

    let format = c.opts.str("output_file_type").to_string();
    let output = match c.opts.get("output_filename") {
        OptValue::Str(name) if !name.is_empty() => name.clone(),
        _ => {
            let base = paths::basename(program);
            let stem = base.rfind('.').filter(|&dot| dot > 0).map_or(base.as_str(), |dot| &base[..dot]);
            format!("{stem}.{format}")
        }
    };
    if format == "asm" {
        // Only output assembler file
        if let Err(error) = std::fs::write(&output, text) {
            return fail(&format!("{output}: {error}"));
        }
        return c.sess.has_errors as i32;
    }

    let asm_options = AsmOptions {
        zxnext: c.opts.bool("zxnext"),
        force_brackets: c.opts.bool("force_asm_brackets"),
        code_window: c.opts.int("codewindow"),
        code_window_size: c.opts.int("codewindowsize"),
        code_bank_base: c.opts.int("codebankbase"),
        code_bank_pages: c.opts.str("codebankpages").to_string(),
        use_basic_loader: c.opts.bool("use_basic_loader"),
        autorun: c.opts.bool("autorun"),
    };
    // INCBIN looks beside the program, then in the library and -I.
    let resolver = zxr_pp::Resolver {
        current_dir: paths::dirname(program),
        lib_root: lib_root.to_string(),
        architecture: c.sess.options.architecture.clone(),
        include_path: c.sess.options.include_path.clone(),
    };
    c.sess.filename = program.to_string();
    let memory_map = match c.opts.get("memory_map") {
        OptValue::Str(path) if !path.is_empty() => Some(path.clone()),
        _ => None,
    };
    let variables = if args.sld.is_some() { c.debug_variables() } else { Vec::new() };
    let mut asm = Assembler::new(&mut c.sess, asm_options, resolver);
    asm.debug = args.sld.is_some();
    asm.variables = variables;
    asm.origins = std::mem::take(&mut c.origins);
    asm.assemble(&text);
    crate::stage_done("assembler");
    if let Err(error) = asm.generate_binary(&output, &format) {
        return fail(&format!("{output}: {error}"));
    }
    let _ = asm.sess.stderr.flush();
    if asm.sess.has_errors > 0 {
        return 5; // Error in assembly
    }
    crate::stage_done("output files");
    if let Some(path) = memory_map {
        if let Err(error) = std::fs::write(&path, asm.memory.memory_map()) {
            return fail(&format!("{path}: {error}"));
        }
    }
    if let Some(path) = &args.sld {
        if let Err(error) = std::fs::write(path, asm.sld(program)) {
            return fail(&format!("{path}: {error}"));
        }
    }
    asm.sess.has_errors as i32 // Exit success
}
