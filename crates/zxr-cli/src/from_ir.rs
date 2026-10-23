//! `zxbc --from-ir FILE`: finishes a compile from a record of what the
//! backend consumes, in place of the front end.
//!
//! The record is the `backend-in` seam of docs/seams.md: the quads of the
//! code and of each variable pass, and the state the translators left
//! behind. `tools/oracle_stage.py backend-in` takes it from the Python
//! compiler; from there on this does what `zxbc` does: assembly text with
//! `-f asm`, otherwise the assembled program, its code banks and the map.

use crate::zxbc_args::ZxbcArgs;
use std::io::Write;
use zxr_asm::{AsmOptions, Assembler};
use zxr_core::{paths, Session};
use zxr_ir::Quad;
use zxr_pp::{Mode, Preprocessor};
use zxr_z80::finish::{self, BankVars};
use zxr_z80::{Backend, Options};

/// One field of a record: `\t`, `\n`, `\r` and `\\` are escaped.
fn unescape(field: &str) -> String {
    let mut out = String::with_capacity(field.len());
    let mut chars = field.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

/// A pass of the backend: the code, or the variables of one bank.
#[derive(Default)]
struct Pass {
    kind: String,
    bank: i64,
    tmp_label: u64,
    banks_with_data: Vec<i64>,
    quads: Vec<Quad>,
}

/// The record, read.
#[derive(Default)]
struct Dump {
    options: Vec<(String, String)>,
    /// The macros the options define (`OPTIONS.__DEFINES`), which the front
    /// end adds to: `___PRINT_IS_USED___` when the program prints.
    defines: Vec<(String, String)>,
    requires: Vec<String>,
    inits: Vec<String>,
    use_function_exit: bool,
    end_emitted: bool,
    asm_count: u64,
    at_end: Vec<String>,
    far_trampolines: Vec<(String, String)>,
    banks_used: Vec<i64>,
    banks_warned: Vec<i64>,
    /// Bank, line and file of the first declaration in each bank.
    bank_decls: Vec<(i64, u32, String)>,
    passes: Vec<Pass>,
}

impl Dump {
    fn parse(text: &str) -> Result<Dump, String> {
        let mut dump = Dump::default();
        let mut lines = text.split('\n');
        if lines.next().map(|l| l.split('\t').collect::<Vec<_>>()) != Some(vec!["zxr-backend-in", "1"]) {
            return Err("not a backend-in record (version 1)".to_string());
        }
        for (number, line) in lines.enumerate() {
            if line.is_empty() {
                continue;
            }
            let f: Vec<String> = line.split('\t').map(unescape).collect();
            let bad = || format!("line {}: malformed '{}' record", number + 2, f[0]);
            let field = |i: usize| f.get(i).cloned().ok_or_else(bad);
            let int = |i: usize| field(i)?.parse::<i64>().map_err(|_| bad());
            match f[0].as_str() {
                "option" => dump.options.push((field(1)?, field(2)?)),
                "define" => dump.defines.push((field(1)?, f.get(2).cloned().unwrap_or_default())),
                "require" => dump.requires.push(field(1)?),
                "init" => dump.inits.push(field(1)?),
                "flag" => match field(1)?.as_str() {
                    "use_function_exit" => dump.use_function_exit = int(2)? != 0,
                    "end_emitted" => dump.end_emitted = int(2)? != 0,
                    _ => return Err(bad()),
                },
                "counter" => match (field(1)?.as_str(), dump.passes.last_mut()) {
                    ("asm", None) => dump.asm_count = int(2)? as u64,
                    ("tmp_label", Some(pass)) => pass.tmp_label = int(2)? as u64,
                    _ => return Err(bad()),
                },
                "at_end" => dump.at_end.push(field(1)?),
                "far_trampoline" => dump.far_trampolines.push((field(1)?, field(2)?)),
                "bank_used" => dump.banks_used.push(int(1)?),
                "bank_warned" => dump.banks_warned.push(int(1)?),
                "bank_decl" => dump.bank_decls.push((int(1)?, int(2)? as u32, field(3)?)),
                "pass" => dump.passes.push(Pass { kind: field(1)?, bank: int(2)?, ..Pass::default() }),
                "banks_with_data" => {
                    let pass = dump.passes.last_mut().ok_or_else(bad)?;
                    pass.banks_with_data = field(1)?.split(',').filter(|b| !b.is_empty()).map(|b| b.parse().map_err(|_| bad())).collect::<Result<_, _>>()?;
                }
                "quad" => {
                    let pass = dump.passes.last_mut().ok_or_else(bad)?;
                    pass.quads.push(Quad { op: field(1)?, args: f[2..].to_vec(), origin: 0 });
                }
                "end" => {}
                _ => return Err(format!("line {}: unknown record '{}'", number + 2, f[0])),
            }
        }
        if dump.passes.first().is_none_or(|p| p.kind != "code") {
            return Err("no code pass".to_string());
        }
        Ok(dump)
    }

    fn option(&self, name: &str) -> &str {
        self.options.iter().find(|(n, _)| n == name).map_or("", |(_, v)| v.as_str())
    }

    fn int(&self, name: &str) -> Result<i64, String> {
        self.option(name).parse().map_err(|_| format!("option {name}: '{}' is not a number", self.option(name)))
    }

    fn flag(&self, name: &str) -> bool {
        self.option(name) == "True"
    }

    fn backend_options(&self) -> Result<Options, String> {
        Ok(Options {
            architecture: self.option("architecture").to_string(),
            org: self.int("org")?,
            heap_size: self.int("heap_size")?,
            heap_address: if self.option("heap_address") == "None" { None } else { Some(self.int("heap_address")?) },
            heap_start_label: self.option("heap_start_label").to_string(),
            heap_size_label: self.option("heap_size_label").to_string(),
            headerless: self.flag("headerless"),
            autorun: self.flag("autorun"),
            optimization_level: self.int("optimization_level")?,
            opt_strategy: self.option("opt_strategy").to_string(),
            codewindow: self.int("codewindow")?,
            codewindowsize: self.int("codewindowsize")?,
            codebankbase: self.int("codebankbase")?,
            codebankpages: self.option("codebankpages").to_string(),
            codebankdepth: self.int("codebankdepth")?,
        })
    }
}

/// `OPTIONS.__DEFINES[name] = value`: a macro already there keeps its place.
fn set_define(defines: &mut Vec<(String, String)>, name: &str, value: &str) {
    match defines.iter_mut().find(|(n, _)| n == name) {
        Some(entry) => entry.1 = value.to_string(),
        None => defines.push((name.to_string(), value.to_string())),
    }
}

fn fail(message: &str) -> i32 {
    eprintln!("zxbc: {message}");
    1
}

/// Finishes the compile from the record in `ir_file`. Where the Python
/// backend would die with a traceback (a quad it cannot make sense of), this
/// prints one line and gives the status of an internal error.
pub fn run(session: &mut Session, args: &ZxbcArgs, program: &str, ir_file: &str, lib_root: &str) -> i32 {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| compile(session, args, program, ir_file, lib_root)));
    std::panic::set_hook(previous);
    match result {
        Ok(code) => code,
        // An exit requested through the session is not ours to report.
        Err(payload) if payload.is::<zxr_core::session::ExitRequest>() => std::panic::resume_unwind(payload),
        Err(payload) => {
            let message = payload.downcast_ref::<String>().cloned().or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            eprintln!("zxbc: internal error in the backend: {message}");
            70
        }
    }
}

fn compile(session: &mut Session, args: &ZxbcArgs, program: &str, ir_file: &str, lib_root: &str) -> i32 {
    let dump = match std::fs::read_to_string(ir_file).map_err(|e| e.to_string()).and_then(|text| Dump::parse(&text)) {
        Ok(dump) => dump,
        Err(error) => return fail(&format!("{ir_file}: {error}")),
    };
    let options = match dump.backend_options() {
        Ok(options) => options,
        Err(error) => return fail(&format!("{ir_file}: {error}")),
    };
    // What a flow optimiser would have made of the code, if one is standing in.
    let stand_in = match &args.flow_stand_in {
        Some(prefix) if options.optimization_level > 2 => {
            let read = |ext: &str| std::fs::read_to_string(format!("{prefix}.{ext}")).map_err(|e| format!("{prefix}.{ext}: {e}"));
            match read("opt-in").and_then(|input| Ok((input, read("opt-out")?))) {
                Ok(texts) => Some(texts),
                Err(error) => return fail(&error),
            }
        }
        _ => None,
    };

    // The first preprocessor pass, for what it leaves behind: the files it
    // included once are not included again by the second.
    let mut pp = Preprocessor::new(session, Mode::Basic, lib_root);
    pp.run(Some(program));
    let _ = pp.sess.stderr.flush();
    if pp.sess.has_errors > 0 {
        return 1;
    }

    let mut backend = Backend::new(options);
    let c = &mut backend.common;
    c.requires.extend(dump.requires.iter().cloned());
    c.inits.extend(dump.inits.iter().cloned());
    c.flag_use_function_exit = dump.use_function_exit;
    c.flag_end_emitted = dump.end_emitted;
    c.asmcount = dump.asm_count;
    c.at_end = dump.at_end.clone();
    c.codebank.far_trampolines = dump.far_trampolines.clone();
    c.codebank.banks_used.extend(&dump.banks_used);
    c.codebank.no_routine_warned.extend(&dump.banks_warned);

    let mut passes = dump.passes.into_iter();
    // Hands a pass's quads over with the counters the front end left.
    let enter = |backend: &mut Backend, pass: &Pass| {
        let labels = &mut backend.common.tmp_labels;
        if labels.counter > pass.tmp_label {
            eprintln!("zxbc: the backend has generated {} temporary labels where the record expects {}", labels.counter, pass.tmp_label);
        }
        labels.advance_to(pass.tmp_label);
        backend.common.codebank.banks_with_data = pass.banks_with_data.iter().copied().collect();
    };

    let code = passes.next().unwrap();
    enter(&mut backend, &code);
    backend.memory = code.quads;
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
    if let Some(lines) = mismatch {
        // Leave what was produced beside the expected lines, to compare.
        let path = format!("{}.opt-in.port", args.flow_stand_in.as_deref().unwrap_or_default());
        let _ = std::fs::write(&path, lines);
        return fail(&format!("the lines handed to the flow optimiser differ from the oracle's (written to {path})"));
    }

    // Now filter them against the preprocessor again
    pp.sess.options.defines = dump.defines.clone();
    let defines = &mut pp.sess.options.defines;
    if dump_flag(&dump.options, "memory_check") {
        set_define(defines, "__MEMORY_CHECK__", "");
    }
    if dump_flag(&dump.options, "array_check") {
        set_define(defines, "__CHECK_ARRAY_BOUNDARY__", "");
    }
    if dump_flag(&dump.options, "enable_break") {
        set_define(defines, "__ENABLE_BREAK__", "");
    }
    if backend.common.options.codewindowsize > 8192 {
        set_define(defines, "__FAR_WINDOW_16K__", "");
    }
    let strategy = backend.common.options.opt_strategy.clone();
    set_define(defines, "__OPT_STRATEGY__", &strategy);
    let mut asm_output = finish::preprocess(&mut pp, &asm_output, program);
    finish::get_inits(&mut backend, &mut asm_output); // Find out remaining inits
    drop(pp);

    // The global variables, and then those of each code bank.
    let Some(vars) = passes.next() else { return fail(&format!("{ir_file}: no variable pass")) };
    enter(&mut backend, &vars);
    if session.has_errors > 0 {
        let _ = session.stderr.flush();
        return 1; // Exit with errors
    }
    let tmp = finish::emit_vars(&mut backend, vars.quads);
    let decls = dump.bank_decls;
    let bank_vars = finish::emit_banked_vars(&mut backend, session, |backend| {
        let pass = passes.next()?;
        enter(backend, &pass);
        let (_, lineno, filename) = decls.iter().find(|(bank, _, _)| *bank == pass.bank).cloned().unwrap_or((pass.bank, 0, String::new()));
        Some(BankVars { bank: pass.bank, quads: pass.quads, lineno, filename })
    });
    let asm_output = finish::splice(&backend, tmp, asm_output, bank_vars);
    let text = finish::output(&asm_output);
    let _ = session.stderr.flush();

    let format = if let Some(f) = &args.output_format {
        f.clone()
    } else if args.asm {
        "asm".to_string()
    } else if args.tzx {
        "tzx".to_string()
    } else if args.tap {
        "tap".to_string()
    } else {
        "bin".to_string()
    };
    let output = args.output_file.clone().unwrap_or_else(|| {
        let base = paths::basename(program);
        let stem = base.rfind('.').filter(|&dot| dot > 0).map_or(base.as_str(), |dot| &base[..dot]);
        format!("{stem}.{format}")
    });
    if format == "asm" {
        // Only output assembler file
        return match std::fs::write(&output, text) {
            Ok(()) => 0,
            Err(error) => fail(&format!("{output}: {error}")),
        };
    }

    let o = &backend.common.options;
    let asm_options = AsmOptions {
        zxnext: dump_flag(&dump.options, "zxnext"),
        force_brackets: dump_flag(&dump.options, "force_asm_brackets"),
        code_window: o.codewindow as i128,
        code_window_size: o.codewindowsize as i128,
        code_bank_base: o.codebankbase as i128,
        code_bank_pages: o.codebankpages.clone(),
        use_basic_loader: dump_flag(&dump.options, "use_basic_loader"),
        autorun: o.autorun,
    };
    // INCBIN looks beside the program, then in the library and -I.
    let resolver = zxr_pp::Resolver {
        current_dir: paths::dirname(program),
        lib_root: lib_root.to_string(),
        architecture: session.options.architecture.clone(),
        include_path: session.options.include_path.clone(),
    };
    session.filename = program.to_string();
    let mut asm = Assembler::new(session, asm_options, resolver);
    asm.assemble(&text);
    if let Err(error) = asm.generate_binary(&output, &format) {
        return fail(&format!("{output}: {error}"));
    }
    if asm.sess.has_errors > 0 {
        let _ = asm.sess.stderr.flush();
        return 5; // Error in assembly
    }
    if let Some(path) = &args.memory_map {
        if let Err(error) = std::fs::write(path, asm.memory.memory_map()) {
            return fail(&format!("{path}: {error}"));
        }
    }
    0
}

fn dump_flag(options: &[(String, String)], name: &str) -> bool {
    options.iter().any(|(n, v)| n == name && v == "True")
}
