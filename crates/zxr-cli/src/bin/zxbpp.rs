//! The preprocessor on its own.

use std::io::Write;
use zxr_core::{Options, Session};
use zxr_pp::{Mode, Preprocessor};

const USAGE: &str = "usage: zxbpp [-h] [-o OUTPUT_FILE] [-d] [-e STDERR] [--arch ARCH]\n             [--expect-warnings EXPECT_WARNINGS]\n             [input_file]\n";

fn usage_error(message: &str) -> ! {
    eprint!("{USAGE}zxbpp: error: {message}\n");
    zxr_core::exit(2)
}

fn main_impl(args: Vec<String>) -> i32 {
    let mut options = Options::default();
    let mut output_file: Option<String> = None;
    let mut stderr_file: Option<String> = None;
    let mut input_file: Option<String> = None;
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        // Options take their value as the next argument or after `=`.
        let (name, inline) = match arg.split_once('=') {
            Some((n, v)) if n.starts_with("--") => (n.to_string(), Some(v.to_string())),
            _ => (arg.clone(), None),
        };
        let mut value = |name: &str| inline.clone().or_else(|| iter.next()).unwrap_or_else(|| usage_error(&format!("argument {name}: expected one argument")));
        match name.as_str() {
            "-o" | "--output" => output_file = Some(value("-o/--output")),
            "-e" | "--errmsg" => stderr_file = Some(value("-e/--errmsg")),
            "-d" | "--debug" => options.debug_level += 1,
            "--arch" => options.architecture = value("--arch"),
            "--expect-warnings" => {
                let text = value("--expect-warnings");
                options.expected_warnings = text.parse().unwrap_or_else(|_| usage_error(&format!("argument --expect-warnings: invalid int value: '{text}'")));
            }
            "-h" | "--help" => {
                print!("{USAGE}");
                return 0;
            }
            other if other.starts_with('-') && other.len() > 1 => usage_error(&format!("unrecognized arguments: {other}")),
            _ if input_file.is_none() => input_file = Some(arg),
            _ => usage_error(&format!("unrecognized arguments: {arg}")),
        }
    }
    if !zxr_core::ARCHITECTURES.contains(&options.architecture.as_str()) {
        usage_error(&format!("Invalid architecture '{}'", options.architecture));
    }

    let mut session = Session::new(options);
    if let Some(path) = &stderr_file {
        match std::fs::File::create(path) {
            Ok(file) => session.stderr = Box::new(file),
            Err(error) => {
                eprintln!("zxbpp: {path}: {error}");
                return 1;
            }
        }
    }
    let lib_root = zxr_cli::lib_root();
    let (errors, output) = {
        let mut pp = Preprocessor::new(&mut session, Mode::Basic, &lib_root);
        let errors = pp.run(input_file.as_deref());
        (errors, std::mem::take(&mut pp.output))
    };
    let _ = session.stderr.flush();
    if session.has_errors == 0 {
        let written = match &output_file {
            Some(path) => std::fs::write(path, output.as_bytes()),
            None => std::io::stdout().write_all(output.as_bytes()),
        };
        if let Err(error) = written {
            eprintln!("zxbpp: {error}");
            return 1;
        }
    }
    errors as i32
}

fn main() {
    zxr_cli::run(main_impl)
}
