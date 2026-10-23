//! The assembler on its own.

use zxr_asm::instr::Arg;
use zxr_asm::{expr, AsmOptions, Assembler};
use zxr_core::{Options, Session};
use zxr_pp::{Mode, Preprocessor};

const USAGE: &str = "usage: zxbasm [-h] [-d] [-O OPTIMIZATION_LEVEL] [-o OUTPUT_FILE] [-T] [-t] [-B] [-a]\n              [-e STDERR] [-M MEMORY_MAP] [-b] [-N] [--version]\n              PROGRAM\n";

fn usage_error(message: &str) -> ! {
    eprint!("{USAGE}zxbasm: error: {message}\n");
    zxr_core::exit(2)
}

fn main_impl(args: Vec<String>) -> i32 {
    let mut program: Option<String> = None;
    let mut output: Option<String> = None;
    let mut stderr_file: Option<String> = None;
    let mut memory_map: Option<String> = None;
    let mut format = "bin";
    let (mut basic, mut autorun, mut debug) = (false, false, 0);
    let mut asm_options = AsmOptions::default();

    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        let (name, inline) = match arg.split_once('=') {
            Some((n, v)) if n.starts_with("--") => (n.to_string(), Some(v.to_string())),
            _ => (arg.clone(), None),
        };
        let mut value = |name: &str| inline.clone().or_else(|| iter.next()).unwrap_or_else(|| usage_error(&format!("argument {name}: expected one argument")));
        match name.as_str() {
            "-d" | "--debug" => debug += 1,
            "-O" | "--optimize" => {
                let text = value("-O/--optimize");
                if text.parse::<i64>().is_err() {
                    usage_error(&format!("argument -O/--optimize: invalid int value: '{text}'"));
                }
            }
            "-o" | "--output" => output = Some(value("-o/--output")),
            "-T" | "--tzx" => format = "tzx",
            "-t" | "--tap" if format != "tzx" => format = "tap",
            "-t" | "--tap" => {}
            "-B" | "--BASIC" => basic = true,
            "-a" | "--autorun" => autorun = true,
            "-e" | "--errmsg" => stderr_file = Some(value("-e/--errmsg")),
            "-M" | "--mmap" => memory_map = Some(value("-M/--mmap")),
            "-b" | "--bracket" => asm_options.force_brackets = true,
            "-N" | "--zxnext" => asm_options.zxnext = true,
            "--version" => {
                println!("zxbasm {}", env!("CARGO_PKG_VERSION"));
                return 0;
            }
            "-h" | "--help" => {
                print!("{USAGE}");
                return 0;
            }
            other if other.starts_with('-') && other.len() > 1 => usage_error(&format!("unrecognized arguments: {other}")),
            _ if program.is_none() => program = Some(arg),
            _ => usage_error(&format!("unrecognized arguments: {arg}")),
        }
    }
    let Some(program) = program else { usage_error("the following arguments are required: PROGRAM") };
    if !std::path::Path::new(&program).exists() {
        usage_error(&format!("No such file or directory: '{program}'"));
    }
    asm_options.use_basic_loader = autorun || basic;
    asm_options.autorun = autorun;
    let output = output.unwrap_or_else(|| {
        let base = zxr_core::paths::basename(&program);
        let stem = base.rfind('.').filter(|&dot| dot > 0).map_or(base.as_str(), |dot| &base[..dot]);
        format!("{stem}.{format}")
    });

    // The assembler has no target architecture, so no library directories.
    let options = Options { architecture: String::new(), debug_level: debug, ..Options::default() };
    let mut session = Session::new(options);
    if let Some(path) = &stderr_file {
        match std::fs::File::create(path) {
            Ok(file) => session.stderr = Box::new(file),
            Err(error) => usage_error(&format!("{path}: {error}")),
        }
    }
    if asm_options.use_basic_loader && format == "bin" {
        usage_error("Option --BASIC and --autorun requires tzx or tap format");
    }

    let lib_root = zxr_cli::lib_root();
    let (source, resolver) = {
        let mut pp = Preprocessor::new(&mut session, Mode::Asm, &lib_root);
        pp.run(Some(&program));
        (std::mem::take(&mut pp.output), pp.resolver())
    };

    let mut asm = Assembler::new(&mut session, asm_options, resolver);
    asm.assemble(&source);
    if asm.sess.has_errors > 0 {
        return 1;
    }
    if asm.memory.is_empty() {
        asm.sess.warning(0, "Nothing to assemble. Exiting...", None);
        return 0;
    }

    // `#init` routines are called from code appended to the program, which
    // then becomes its entry point.
    let after_program = asm.memory.last_address() + 1;
    let inits = asm.inits.clone();
    for (label, line) in &inits {
        let target = asm.memory.get_label(asm.sess, label, *line);
        asm.add_instruction("CALL NN", vec![Arg::Expr(expr::label(target, *line))]);
    }
    if !inits.is_empty() {
        let start = asm.autorun_addr.unwrap_or_else(|| asm.memory.first_org());
        asm.add_instruction("JP NN", vec![Arg::Int(start)]);
        asm.autorun_addr = Some(after_program);
    }
    if let Some(path) = &memory_map {
        if let Err(error) = std::fs::write(path, asm.memory.memory_map()) {
            eprintln!("zxbasm: {path}: {error}");
            return 1;
        }
    }
    if let Err(error) = asm.generate_binary(&output, format) {
        eprintln!("zxbasm: {output}: {error}");
        return 1;
    }
    asm.sess.has_errors as i32
}

fn main() {
    zxr_cli::run(main_impl)
}
