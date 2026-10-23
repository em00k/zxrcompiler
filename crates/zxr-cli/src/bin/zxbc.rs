//! The compiler: `main()` of `src/zxbc/zxbc.py`. The front end's part of it
//! is `zxr_syntax::frontend`, the rest `zxr_cli::compile`. Port-only entry
//! points cut it at the seams of docs/seams.md: `--stage S` stops after a
//! stage and writes what it produced, `--from-ir FILE` and `--from-asm FILE`
//! take the place of the front end or of everything before the assembler.

use std::io::Write;
use zxr_cli::zxbc_args::{parse_int, ZxbcArgs};
use zxr_asm::{AsmOptions, Assembler};
use zxr_core::{paths, Options, Session};
use zxr_pp::{Mode, Preprocessor, Resolver};

/// argparse's usage text, as it wraps at 80 columns.
const USAGE: &str = r"usage: zxbc [-h] [-d] [-O OPTIMIZE] [-o OUTPUT_FILE] [-T | -t | -A | -E |
               --parse-only | -f {asm,bin,ir,sna,tap,tzx,z80}] [-B] [-a]
               [-S ORG] [--code-window CODE_WINDOW]
               [--code-window-size CODE_WINDOW_SIZE]
               [--code-bank-base CODE_BANK_BASE]
               [--code-bank-pages CODE_BANK_PAGES]
               [--code-bank-depth CODE_BANK_DEPTH] [-e STDERR]
               [--array-base ARRAY_BASE] [--string-base STRING_BASE] [-Z]
               [-H HEAP_SIZE] [--heap-address HEAP_ADDRESS] [--debug-memory]
               [--debug-array] [--strict-bool] [--enable-break] [--explicit]
               [-D DEFINES] [-M MEMORY_MAP] [-i] [-I INCLUDE_PATH] [--strict]
               [--headerless] [--version] [--append-binary APPEND_BINARY]
               [--append-headless-binary APPEND_HEADLESS_BINARY] [-N]
               [--arch ARCH] [--expect-warnings EXPECT_WARNINGS]
               [-W DISABLE_WARNING] [+W ENABLE_WARNING] [--hide-warning-codes]
               [-F CONFIG_FILE] [--save-config SAVE_CONFIG]
               [--opt-strategy {size,speed,auto}]
               PROGRAM
";

fn usage_error(message: &str) -> ! {
    eprint!("{USAGE}zxbc: error: {message}\n");
    zxr_core::exit(2)
}

/// The combinations of options `parse_options()` refuses.
fn check_options(args: &ZxbcArgs) {
    let format = if let Some(format) = &args.output_format {
        format.as_str()
    } else if args.tzx {
        "tzx"
    } else if args.tap {
        "tap"
    } else if args.asm {
        "asm"
    } else if args.emit_backend {
        "ir"
    } else {
        "bin"
    };
    let both: Vec<String> = args.enable_warnings.iter().filter(|w| args.disable_warnings.contains(w)).map(|w| format!("W{w}")).collect();
    if !both.is_empty() {
        usage_error(&format!("Warning(s) {} cannot be enabled and disabled simultaneously", both.join(", ")));
    }
    if (args.basic || args.autorun) && !matches!(format, "tap" | "tzx" | "sna" | "z80") {
        usage_error("Options --BASIC and --autorun require one of sna, tzx, tap or z80 output format");
    }
    if !(args.basic && args.autorun) && matches!(format, "sna" | "z80") {
        usage_error("Options --BASIC and --autorun are both required for snapshot formats");
    }
    if !args.append_binary.is_empty() && !matches!(format, "tap" | "tzx") {
        usage_error("Option --append-binary needs either tap or tzx output format");
    }
    if format == "asm" && args.memory_map.is_some() {
        usage_error("Option --asm and --mmap cannot be used together");
    }
}

fn main_impl(args: Vec<String>) -> i32 {
    let mut args = ZxbcArgs::parse(&args).unwrap_or_else(|e| usage_error(&e));
    // `ZXR_SLD` in the environment asks for debug data without a change to
    // whatever runs the compiler: beside the map file if there is one, and
    // beside the output if not.
    if args.sld.is_none() && std::env::var_os("ZXR_SLD").is_some_and(|v| !v.is_empty()) {
        let beside = |path: &str| std::path::Path::new(path).with_extension("sld").to_string_lossy().into_owned();
        args.sld = args.memory_map.as_deref().or(args.output_file.as_deref()).map(beside);
    }
    if args.version {
        println!("zxbc {}", env!("CARGO_PKG_VERSION"));
        return 0;
    }
    let Some(program) = args.program.clone() else { usage_error("the following arguments are required: PROGRAM") };

    let mut options = Options::default();
    options.debug_level = args.debug;
    options.sinclair = args.sinclair;
    options.hide_warning_codes = args.hide_warning_codes;
    options.include_path = args.include_path.clone();
    if let Some(n) = &args.expect_warnings {
        options.expected_warnings = n.parse().unwrap_or_else(|_| usage_error(&format!("argument --expect-warnings: invalid int value: '{n}'")));
    }
    if let Some(n) = &args.heap_size {
        if n.parse::<i64>().is_err() {
            usage_error(&format!("argument -H/--heap-size: invalid int value: '{n}'"));
        }
    }
    if let Some(n) = &args.optimize {
        if n.parse::<i64>().is_err() {
            usage_error(&format!("argument -O/--optimize: invalid int value: '{n}'"));
        }
    }
    if let Some(arch) = &args.arch {
        if !zxr_core::ARCHITECTURES.contains(&arch.as_str()) {
            usage_error(&format!("Invalid architecture '{arch}'"));
        }
        options.architecture = arch.clone();
    }
    // Macros from the command line, then the ones the options imply.
    for define in &args.defines {
        let (name, value) = define.split_once('=').unwrap_or((define, ""));
        options.defines.push((name.to_string(), value.to_string()));
    }
    // Not ported: the Python compiler loads options from `-F FILE`, or from
    // ./project.ini if there is one, and `--save-config FILE` writes them.
    // Nothing is done quietly in their place. See docs/quirks.md.
    if args.config_file.is_some() || args.save_config.is_some() {
        usage_error("-F/--config-file and --save-config are not supported by this compiler");
    }
    if std::path::Path::new("project.ini").is_file() {
        eprintln!("zxbc: warning: ./project.ini is not read by this compiler; give its options on the command line");
    }
    check_options(&args);
    if !std::path::Path::new(&program).exists() {
        usage_error(&format!("No such file or directory: '{program}'"));
    }
    let mut implied = |name: &str, value: &str| {
        options.defines.retain(|(n, _)| n != name);
        options.defines.push((name.to_string(), value.to_string()));
    };
    if args.debug_memory {
        implied("__MEMORY_CHECK__", "");
    }
    if args.debug_array {
        implied("__CHECK_ARRAY_BOUNDARY__", "");
    }
    if args.enable_break {
        implied("__ENABLE_BREAK__", "");
    }
    if args.code_window_size.as_deref().and_then(parse_int).is_some_and(|n| n > 8192) {
        implied("__FAR_WINDOW_16K__", "");
    }
    implied("__OPT_STRATEGY__", args.opt_strategy.as_deref().unwrap_or("auto"));

    let mut session = Session::new(options);
    session.program = std::env::args().next().unwrap_or_else(|| "zxbc".to_string());
    // The warnings the Python compiler does not have, off for a comparison
    // with it.
    if session.oracle_diagnostics {
        for code in ["930", "940", "950"] {
            session.warnings.insert(code.to_string(), false);
        }
    }
    for code in &args.disable_warnings {
        session.warnings.insert(code.clone(), false);
    }
    for code in &args.enable_warnings {
        session.warnings.insert(code.clone(), true);
    }
    if let Some(path) = &args.stderr {
        match std::fs::File::create(path) {
            Ok(file) => session.stderr = Box::new(file),
            Err(error) => usage_error(&format!("{path}: {error}")),
        }
    }

    let lib_root = zxr_cli::lib_root();
    if let Some(asm_file) = &args.from_asm {
        return assemble(&mut session, &args, &program, asm_file, &lib_root);
    }
    if let Some(ir_file) = &args.from_ir {
        return zxr_cli::from_ir::run(&mut session, &args, &program, ir_file, &lib_root);
    }
    // The preprocessor outlives the parse: the generated assembly goes
    // through it again, and what it included once stays included.
    zxr_cli::stage_done("start");
    let (output, pp) = {
        let mut pp = Preprocessor::new(&mut session, Mode::Basic, &lib_root);
        pp.run(Some(&program));
        (std::mem::take(&mut pp.output), pp.suspend())
    };
    zxr_cli::stage_done("preprocessor");
    let _ = session.stderr.flush();
    if session.has_errors > 0 {
        return 1;
    }
    let write = |text: &str| {
        let path = args.output_file.clone().unwrap_or_else(|| "a.out".to_string());
        match std::fs::File::create(&path).and_then(|mut f| f.write_all(text.as_bytes())) {
            Ok(()) => 0,
            Err(error) => {
                eprintln!("zxbc: {path}: {error}");
                1
            }
        }
    };
    match args.stage.as_deref() {
        Some("pp") => return write(&output),
        Some("lex") => {
            let tokens = dump_tokens(&mut session, &output);
            let _ = session.stderr.flush();
            let status = write(&tokens);
            return if session.has_errors > 0 { 1 } else { status };
        }
        _ => {}
    }
    front_end(session, pp, &args, &program, &output, &lib_root, &write)
}

/// The options as `parse_options()` in `src/zxbc/args_config.py` leaves them.
fn set_options(c: &mut zxr_syntax::Compiler, args: &ZxbcArgs, program: &str) {
    use zxr_syntax::options::OptValue;
    let mut set = |name: &str, value: OptValue| {
        if let Err(error) = c.set_option(name, value) {
            usage_error(&error.to_string());
        }
    };
    let text = |value: &Option<String>| value.clone().map_or(OptValue::None, OptValue::Str);
    // A flag that is absent is `None` to argparse, and leaves the option alone.
    let flag = |value: bool| if value { OptValue::Bool(true) } else { OptValue::None };
    if args.debug > 0 {
        set("debug_level", OptValue::Int(args.debug as i128));
    }
    set("optimization_level", text(&args.optimize));
    set("output_filename", text(&args.output_file));
    set("stderr_filename", text(&args.stderr));
    set("array_base", text(&args.array_base));
    set("string_base", text(&args.string_base));
    set("sinclair", OptValue::Bool(args.sinclair));
    set("heap_size", text(&args.heap_size));
    set("memory_check", flag(args.debug_memory));
    set("strict_bool", flag(args.strict_bool));
    set("array_check", flag(args.debug_array));
    set("enable_break", OptValue::Bool(args.enable_break));
    set("explicit", OptValue::Bool(args.explicit));
    set("memory_map", text(&args.memory_map));
    set("strict", flag(args.strict));
    set("headerless", flag(args.headerless));
    set("zxnext", flag(args.zxnext));
    set("expected_warnings", text(&args.expect_warnings));
    set("hide_warning_codes", flag(args.hide_warning_codes));
    set("opt_strategy", text(&args.opt_strategy));
    set("architecture", OptValue::Str(args.arch.clone().unwrap_or_else(|| zxr_core::ARCHITECTURES[0].to_string())));
    // main() initialises the backend again once the architecture is known,
    // and each backend's init() sets this option: the zxnext one to true,
    // the zx48k one to false, whatever -N said.
    set("zxnext", OptValue::Bool(args.arch.as_deref() == Some("zxnext")));
    let number = |name: &str, value: &str| match parse_int(value) {
        Some(n) => OptValue::Int(n as i128),
        None => usage_error(&format!("Invalid --{name} option '{value}'")),
    };
    if let Some(org) = &args.org {
        set("org", number("org", org));
    }
    if let Some(address) = &args.heap_address {
        set("heap_address", parse_int(address).map_or(OptValue::None, |n| OptValue::Int(n as i128)));
    }
    for (option, name, value) in [
        ("codewindow", "code-window", &args.code_window),
        ("codewindowsize", "code-window-size", &args.code_window_size),
        ("codebankbase", "code-bank-base", &args.code_bank_base),
        ("codebankdepth", "code-bank-depth", &args.code_bank_depth),
    ] {
        if let Some(value) = value {
            set(option, number(name, value));
        }
    }
    if let Some(pages) = &args.code_bank_pages {
        set("codebankpages", OptValue::Str(pages.clone()));
    }
    if args.sinclair {
        set("array_base", OptValue::Int(1));
        set("string_base", OptValue::Int(1));
        set("case_insensitive", OptValue::Bool(true));
    }
    set("case_insensitive", flag(args.ignore_case));
    set("use_basic_loader", OptValue::Bool(args.basic));
    set("autorun", flag(args.autorun));
    let format = if let Some(format) = &args.output_format {
        Some(format.as_str())
    } else if args.tzx {
        Some("tzx")
    } else if args.tap {
        Some("tap")
    } else if args.asm {
        Some("asm")
    } else if args.emit_backend {
        Some("ir")
    } else {
        None
    };
    if let Some(format) = format {
        set("output_file_type", OptValue::Str(format.to_string()));
    }
    if format == Some("ir") {
        set("emit_backend", OptValue::Bool(true));
    }
    set("include_path", OptValue::Str(args.include_path.clone()));
    set("input_filename", OptValue::Str(paths::basename(program)));
    // The macros of the command line, then those the options imply; the
    // session has them in that order already.
    c.defines = c.sess.options.defines.clone();
}

/// The compile from the parse on: to a stage's dump (`--stage ast`, `ast2`
/// or `ir`), to the intermediate code (`-f ir`), or through the backend and
/// the assembler to the program.
fn front_end(session: Session, pp: zxr_pp::Suspended, args: &ZxbcArgs, program: &str, source: &str, lib_root: &str, write: &dyn Fn(&str) -> i32) -> i32 {
    use zxr_syntax::frontend::{Outcome, Stop};
    let mut c = zxr_syntax::Compiler::new(session);
    set_options(&mut c, args, program);
    let stop = match args.stage.as_deref() {
        Some("ast") => Stop::Ast,
        Some("ast2") => Stop::AstPasses,
        Some("ir") => Stop::Ir,
        Some(other) => usage_error(&format!("unknown stage '{other}'")),
        None if c.opts.bool("emit_backend") => Stop::EmitBackend,
        None if args.parse_only => Stop::ParseOnly,
        None => Stop::Translated,
    };
    c.debug_lines = args.sld.is_some();
    let outcome = c.run_front_end(source, stop);
    zxr_cli::stage_done("front end");
    let _ = c.sess.stderr.flush();
    match outcome {
        Outcome::Exit(code) => code,
        Outcome::Text(text) => {
            let status = write(&text);
            if stop == Stop::EmitBackend { status } else if c.sess.has_errors > 0 { 1 } else { status }
        }
        Outcome::Translated => zxr_cli::compile::run(&mut c, pp, args, program, lib_root),
    }
}

/// The tokens of preprocessed source, one per line: type, line and value.
/// No name is taken for an array, as there is no symbol table.
fn dump_tokens(session: &mut Session, source: &str) -> String {
    use zxr_syntax::lexer::{BasicLexer, TokenValue};
    let mut lexer = BasicLexer::new(source);
    let mut out = String::new();
    while let Some(token) = lexer.token(session, &|_| None) {
        let value = match &token.value {
            TokenValue::Float(f) => format!("f:{}", zxr_core::pynum::float_repr(*f)),
            TokenValue::Int(i) => format!("i:{i}"),
            TokenValue::Text(text) => {
                let escaped: String = text.chars().map(|c| if c == '\\' { "\\\\".to_string() } else if (' '..='~').contains(&c) { c.to_string() } else { format!("\\u{{{:x}}}", c as u32) }).collect();
                format!("s:{escaped}")
            }
        };
        out.push_str(&format!("{}\t{}\t{value}\n", token.name, token.lineno));
    }
    out
}

/// The last stage of a compile: assemble the generated code and write the
/// program, its code banks and the memory map.
fn assemble(session: &mut Session, args: &ZxbcArgs, program: &str, asm_file: &str, lib_root: &str) -> i32 {
    let source = match std::fs::read(asm_file).map(|bytes| String::from_utf8_lossy(&bytes).into_owned()) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("zxbc: {asm_file}: {error}");
            return 1;
        }
    };
    let number = |name: &str, value: &Option<String>, default: i128| match value {
        None => default,
        Some(text) => parse_int(text).map(i128::from).unwrap_or_else(|| usage_error(&format!("Invalid --{name} option '{text}'"))),
    };
    let format = if let Some(f) = &args.output_format {
        f.clone()
    } else if args.tzx {
        "tzx".to_string()
    } else if args.tap {
        "tap".to_string()
    } else {
        "bin".to_string()
    };
    let options = AsmOptions {
        zxnext: args.zxnext || session.options.architecture == "zxnext",
        force_brackets: false,
        code_window: number("code-window", &args.code_window, 0x6000),
        code_window_size: number("code-window-size", &args.code_window_size, 8192),
        code_bank_base: number("code-bank-base", &args.code_bank_base, 30),
        code_bank_pages: args.code_bank_pages.clone().unwrap_or_default(),
        use_basic_loader: args.basic,
        autorun: args.autorun,
    };
    let output = args.output_file.clone().unwrap_or_else(|| {
        let base = paths::basename(program);
        let stem = base.rfind('.').filter(|&dot| dot > 0).map_or(base.as_str(), |dot| &base[..dot]);
        format!("{stem}.{format}")
    });
    // INCBIN looks beside the program, then in the library and -I.
    let resolver =
        Resolver { current_dir: paths::dirname(program), lib_root: lib_root.to_string(), architecture: session.options.architecture.clone(), include_path: session.options.include_path.clone() };
    session.filename = program.to_string();
    let mut asm = Assembler::new(session, options, resolver);
    asm.assemble(&source);
    if let Err(error) = asm.generate_binary(&output, &format) {
        eprintln!("zxbc: {output}: {error}");
        return 1;
    }
    if asm.sess.has_errors > 0 {
        let _ = asm.sess.stderr.flush();
        return 5;
    }
    if let Some(path) = &args.memory_map {
        if let Err(error) = std::fs::write(path, asm.memory.memory_map()) {
            eprintln!("zxbc: {path}: {error}");
            return 1;
        }
    }
    0
}

fn main() {
    zxr_cli::run(main_impl)
}
