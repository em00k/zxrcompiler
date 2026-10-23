//! One compilation's options and diagnostics. The Python compiler keeps
//! these in module globals; here they travel in a `Session`.

use std::collections::{BTreeMap, HashSet};
use std::io::Write;

pub struct Options {
    pub architecture: String,
    /// Extra include directories, `:` separated.
    pub include_path: String,
    /// `-D` definitions, in the order given.
    pub defines: Vec<(String, String)>,
    pub debug_level: u32,
    /// The first N warnings are counted but not shown.
    pub expected_warnings: u32,
    pub hide_warning_codes: bool,
    pub max_syntax_errors: u32,
    pub sinclair: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            architecture: crate::ARCHITECTURES[0].to_string(),
            include_path: String::new(),
            defines: Vec::new(),
            debug_level: 0,
            expected_warnings: 0,
            hide_warning_codes: false,
            max_syntax_errors: 20,
            sinclair: false,
        }
    }
}

/// Payload of the panic that stands in for Python's `sys.exit`.
pub struct ExitRequest(pub i32);

/// Leaves the compilation at once with the given status, as `sys.exit` does.
/// Caught by [`run_catching_exit`].
pub fn exit(code: i32) -> ! {
    std::panic::panic_any(ExitRequest(code))
}

/// Runs `f`, turning an [`exit`] inside it into its status.
pub fn run_catching_exit(f: impl FnOnce() -> i32) -> i32 {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if info.payload().downcast_ref::<ExitRequest>().is_none() {
            previous(info);
        }
    }));
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(code) => code,
        Err(payload) => match payload.downcast::<ExitRequest>() {
            Ok(request) => request.0,
            Err(other) => std::panic::resume_unwind(other),
        },
    }
}

pub struct Session {
    pub options: Options,
    /// The program name as invoked. The Python compiler attributes
    /// command-line macros to it.
    pub program: String,
    /// Where diagnostics go: stderr, or the file given with `-e`.
    pub stderr: Box<dyn Write>,
    /// The file diagnostics are attributed to when none is given.
    pub filename: String,
    pub has_errors: u32,
    pub has_warnings: u32,
    /// Messages already shown; a repeat is dropped.
    shown: HashSet<String>,
    /// Warning codes and whether each is enabled.
    pub warnings: BTreeMap<String, bool>,
    /// `ZXR_ORACLE_DIAGNOSTICS` is set: say what the Python compiler says,
    /// where this one would say more or say it better.
    pub oracle_diagnostics: bool,
    /// `ZXR_ORACLE_RUNTIME` is set: the runtime library as the Python
    /// compiler has it, without the fixes made to it here.
    pub oracle_runtime: bool,
}

impl Default for Session {
    fn default() -> Self {
        Session::new(Options::default())
    }
}

impl Session {
    pub fn new(options: Options) -> Session {
        Session {
            options,
            program: "zxbc".to_string(),
            stderr: Box::new(std::io::stderr()),
            filename: "(stdin)".to_string(),
            has_errors: 0,
            has_warnings: 0,
            shown: HashSet::new(),
            warnings: BTreeMap::new(),
            oracle_diagnostics: std::env::var_os("ZXR_ORACLE_DIAGNOSTICS").is_some(),
            oracle_runtime: std::env::var_os("ZXR_ORACLE_RUNTIME").is_some(),
        }
    }

    fn output(&mut self, message: String) {
        if self.shown.contains(&message) {
            return;
        }
        let _ = writeln!(self.stderr, "{message}");
        self.shown.insert(message);
    }

    /// Writes raw text to the diagnostics stream.
    pub fn write_stderr(&mut self, text: &str) {
        let _ = write!(self.stderr, "{text}");
    }

    pub fn error(&mut self, lineno: u32, message: &str, fname: Option<&str>) {
        let fname = fname.unwrap_or(&self.filename).to_string();
        let too_many = self.has_errors > self.options.max_syntax_errors;
        let message = if too_many { "Too many errors. Giving up!" } else { message };
        self.output(format!("{fname}:{lineno}: error: {message}"));
        if too_many {
            let _ = self.stderr.flush();
            exit(1);
        }
        self.has_errors += 1;
    }

    fn warn(&mut self, prefix: &str, lineno: u32, message: &str, fname: Option<&str>) {
        self.has_warnings += 1;
        if self.has_warnings <= self.options.expected_warnings {
            return;
        }
        let fname = fname.unwrap_or(&self.filename).to_string();
        self.output(format!("{fname}:{lineno}: {prefix} {message}"));
    }

    /// A warning with no code.
    pub fn warning(&mut self, lineno: u32, message: &str, fname: Option<&str>) {
        self.warn("warning:", lineno, message, fname);
    }

    /// A warning that can be switched off by its code.
    pub fn warning_coded(&mut self, code: &str, lineno: u32, message: &str, fname: Option<&str>) {
        if !self.warnings.get(code).copied().unwrap_or(true) {
            return;
        }
        let prefix = if self.options.hide_warning_codes { "warning:".to_string() } else { format!("warning: [W{code}]") };
        self.warn(&prefix, lineno, message, fname);
    }
}
