//! zxbc's command line. The Python compiler uses `argparse` with `-` and `+`
//! as prefix characters; this accepts the same spellings: clustered short
//! flags (`-aB`), attached values (`-O4`, `--arch=zxnext`) and unique
//! prefixes of long options.

#[derive(Default, Debug)]
pub struct ZxbcArgs {
    pub program: Option<String>,
    pub debug: u32,
    pub optimize: Option<String>,
    pub output_file: Option<String>,
    pub output_format: Option<String>,
    pub tzx: bool,
    pub tap: bool,
    pub asm: bool,
    pub emit_backend: bool,
    pub parse_only: bool,
    pub basic: bool,
    pub autorun: bool,
    pub org: Option<String>,
    pub code_window: Option<String>,
    pub code_window_size: Option<String>,
    pub code_bank_base: Option<String>,
    pub code_bank_pages: Option<String>,
    pub code_bank_depth: Option<String>,
    pub stderr: Option<String>,
    pub array_base: Option<String>,
    pub string_base: Option<String>,
    pub sinclair: bool,
    pub heap_size: Option<String>,
    pub heap_address: Option<String>,
    pub debug_memory: bool,
    pub debug_array: bool,
    pub strict_bool: bool,
    pub enable_break: bool,
    pub explicit: bool,
    pub defines: Vec<String>,
    pub memory_map: Option<String>,
    /// `--sld FILE`: write source-level debug data there.
    pub sld: Option<String>,
    pub ignore_case: bool,
    pub include_path: String,
    pub strict: bool,
    pub headerless: bool,
    pub version: bool,
    pub append_binary: Vec<String>,
    pub append_headless_binary: Vec<String>,
    pub zxnext: bool,
    pub arch: Option<String>,
    pub expect_warnings: Option<String>,
    pub disable_warnings: Vec<String>,
    pub enable_warnings: Vec<String>,
    pub hide_warning_codes: bool,
    pub config_file: Option<String>,
    pub save_config: Option<String>,
    pub opt_strategy: Option<String>,
    /// Port only: stop after a stage and write what it produced. `pp` is the
    /// preprocessed source.
    pub stage: Option<String>,
    /// Port only: skip the front end and assemble this file, as if it were
    /// the assembly the compiler had generated for PROGRAM.
    pub from_asm: Option<String>,
    /// Port only: skip the front end and finish the compile from this record
    /// of what the backend consumes (the `backend-in` seam).
    pub from_ir: Option<String>,
    /// Port only, with `--from-ir` at -O3 and above: use `PREFIX.opt-out` as
    /// what the flow optimiser returns, after checking that it would have
    /// been given the lines in `PREFIX.opt-in`.
    pub flow_stand_in: Option<String>,
}

struct Opt {
    short: Option<&'static str>,
    long: &'static str,
    takes_value: bool,
}

const fn opt(short: Option<&'static str>, long: &'static str, takes_value: bool) -> Opt {
    Opt { short, long, takes_value }
}

const OPTIONS: &[Opt] = &[
    opt(Some("-d"), "--debug", false),
    opt(Some("-O"), "--optimize", true),
    opt(Some("-o"), "--output", true),
    opt(Some("-T"), "--tzx", false),
    opt(Some("-t"), "--tap", false),
    opt(Some("-A"), "--asm", false),
    opt(Some("-E"), "--emit-backend", false),
    opt(None, "--parse-only", false),
    opt(Some("-f"), "--output-format", true),
    opt(Some("-B"), "--BASIC", false),
    opt(Some("-a"), "--autorun", false),
    opt(Some("-S"), "--org", true),
    opt(None, "--code-window", true),
    opt(None, "--code-window-size", true),
    opt(None, "--code-bank-base", true),
    opt(None, "--code-bank-pages", true),
    opt(None, "--code-bank-depth", true),
    opt(Some("-e"), "--errmsg", true),
    opt(None, "--array-base", true),
    opt(None, "--string-base", true),
    opt(Some("-Z"), "--sinclair", false),
    opt(Some("-H"), "--heap-size", true),
    opt(None, "--heap-address", true),
    opt(None, "--debug-memory", false),
    opt(None, "--debug-array", false),
    opt(None, "--strict-bool", false),
    opt(None, "--enable-break", false),
    opt(None, "--explicit", false),
    opt(Some("-D"), "--define", true),
    opt(Some("-M"), "--mmap", true),
    opt(None, "--sld", true),
    opt(Some("-i"), "--ignore-case", false),
    opt(Some("-I"), "--include-path", true),
    opt(None, "--strict", false),
    opt(None, "--headerless", false),
    opt(None, "--version", false),
    opt(None, "--append-binary", true),
    opt(None, "--append-headless-binary", true),
    opt(Some("-N"), "--zxnext", false),
    opt(None, "--arch", true),
    opt(None, "--expect-warnings", true),
    opt(Some("-W"), "--disable-warning", true),
    opt(Some("+W"), "--enable-warning", true),
    opt(None, "--hide-warning-codes", false),
    opt(Some("-F"), "--config-file", true),
    opt(None, "--save-config", true),
    opt(None, "--opt-strategy", true),
    opt(None, "--stage", true),
    opt(None, "--from-asm", true),
    opt(None, "--from-ir", true),
    opt(None, "--flow-stand-in", true),
];

impl ZxbcArgs {
    fn set(&mut self, long: &str, value: Option<String>) {
        let v = value.unwrap_or_default();
        match long {
            "--debug" => self.debug += 1,
            "--optimize" => self.optimize = Some(v),
            "--output" => self.output_file = Some(v),
            "--tzx" => self.tzx = true,
            "--tap" => self.tap = true,
            "--asm" => self.asm = true,
            "--emit-backend" => self.emit_backend = true,
            "--parse-only" => self.parse_only = true,
            "--output-format" => self.output_format = Some(v),
            "--BASIC" => self.basic = true,
            "--autorun" => self.autorun = true,
            "--org" => self.org = Some(v),
            "--code-window" => self.code_window = Some(v),
            "--code-window-size" => self.code_window_size = Some(v),
            "--code-bank-base" => self.code_bank_base = Some(v),
            "--code-bank-pages" => self.code_bank_pages = Some(v),
            "--code-bank-depth" => self.code_bank_depth = Some(v),
            "--errmsg" => self.stderr = Some(v),
            "--array-base" => self.array_base = Some(v),
            "--string-base" => self.string_base = Some(v),
            "--sinclair" => self.sinclair = true,
            "--heap-size" => self.heap_size = Some(v),
            "--heap-address" => self.heap_address = Some(v),
            "--debug-memory" => self.debug_memory = true,
            "--debug-array" => self.debug_array = true,
            "--strict-bool" => self.strict_bool = true,
            "--enable-break" => self.enable_break = true,
            "--explicit" => self.explicit = true,
            "--define" => self.defines.push(v),
            "--mmap" => self.memory_map = Some(v),
            "--sld" => self.sld = Some(v),
            "--ignore-case" => self.ignore_case = true,
            // Not cumulative: a second -I replaces the first.
            "--include-path" => self.include_path = v,
            "--strict" => self.strict = true,
            "--headerless" => self.headerless = true,
            "--version" => self.version = true,
            "--append-binary" => self.append_binary.push(v),
            "--append-headless-binary" => self.append_headless_binary.push(v),
            "--zxnext" => self.zxnext = true,
            "--arch" => self.arch = Some(v),
            "--expect-warnings" => self.expect_warnings = Some(v),
            "--disable-warning" => self.disable_warnings.push(v),
            "--enable-warning" => self.enable_warnings.push(v),
            "--hide-warning-codes" => self.hide_warning_codes = true,
            "--config-file" => self.config_file = Some(v),
            "--save-config" => self.save_config = Some(v),
            "--opt-strategy" => self.opt_strategy = Some(v),
            "--stage" => self.stage = Some(v),
            "--from-asm" => self.from_asm = Some(v),
            "--from-ir" => self.from_ir = Some(v),
            "--flow-stand-in" => self.flow_stand_in = Some(v),
            other => unreachable!("option {other} has no field"),
        }
    }

    pub fn parse(args: &[String]) -> Result<ZxbcArgs, String> {
        let mut out = ZxbcArgs::default();
        let mut i = 0;
        let next_value = |i: &mut usize, name: &str| -> Result<String, String> {
            *i += 1;
            args.get(*i).cloned().ok_or_else(|| format!("argument {name}: expected one argument"))
        };
        while i < args.len() {
            let arg = &args[i];
            if arg.starts_with("--") && arg.len() > 2 {
                let (name, inline) = match arg.split_once('=') {
                    Some((n, v)) => (n, Some(v.to_string())),
                    None => (arg.as_str(), None),
                };
                let found = match OPTIONS.iter().find(|o| o.long == name) {
                    Some(o) => o,
                    None => {
                        let matches: Vec<&Opt> = OPTIONS.iter().filter(|o| o.long.starts_with(name)).collect();
                        match matches[..] {
                            [one] => one,
                            [] => return Err(format!("unrecognized arguments: {arg}")),
                            _ => {
                                let names: Vec<&str> = matches.iter().map(|o| o.long).collect();
                                return Err(format!("ambiguous option: {name} could match {}", names.join(", ")));
                            }
                        }
                    }
                };
                let value = match (found.takes_value, inline) {
                    (true, Some(v)) => Some(v),
                    (true, None) => Some(next_value(&mut i, found.long)?),
                    (false, Some(_)) => return Err(format!("argument {}: ignored explicit argument", found.long)),
                    (false, None) => None,
                };
                out.set(found.long, value);
            } else if (arg.starts_with('-') || arg.starts_with('+')) && arg.len() > 1 {
                // One or more short flags; the first that takes a value
                // consumes the rest of the argument.
                let prefix = &arg[..1];
                let rest: Vec<char> = arg[1..].chars().collect();
                let mut k = 0;
                while k < rest.len() {
                    let short = format!("{prefix}{}", rest[k]);
                    let Some(found) = OPTIONS.iter().find(|o| o.short == Some(short.as_str())) else {
                        return Err(format!("unrecognized arguments: {arg}"));
                    };
                    if found.takes_value {
                        let attached: String = rest[k + 1..].iter().collect();
                        let attached = attached.strip_prefix('=').map(String::from).unwrap_or(attached);
                        let value = if attached.is_empty() { next_value(&mut i, &short)? } else { attached };
                        out.set(found.long, Some(value));
                        break;
                    }
                    out.set(found.long, None);
                    k += 1;
                }
            } else if out.program.is_none() {
                out.program = Some(arg.clone());
            } else {
                return Err(format!("unrecognized arguments: {arg}"));
            }
            i += 1;
        }
        Ok(out)
    }
}

/// Numbers as the compiler's options accept them: decimal, `0x1F`, `$1F`,
/// `01Fh`, `%101`, `101b`. A hex number with an `h` suffix must start with a
/// digit, since `A0h` could be a label.
pub fn parse_int(text: &str) -> Option<i64> {
    let num = text.trim().to_uppercase();
    let first = num.chars().next()?;
    let last = num.chars().last()?;
    let (digits, base) = if num.starts_with("0X") {
        (&num[2..], 16)
    } else if last == 'H' {
        if !first.is_ascii_digit() {
            return None;
        }
        (&num[..num.len() - 1], 16)
    } else if first == '$' {
        (&num[1..], 16)
    } else if first == '%' {
        (&num[1..], 2)
    } else if last == 'B' {
        if first != '0' && first != '1' {
            return None;
        }
        (&num[..num.len() - 1], 2)
    } else {
        (num.as_str(), 10)
    };
    i64::from_str_radix(&digits.replace('_', ""), base).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> ZxbcArgs {
        ZxbcArgs::parse(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>()).unwrap()
    }

    #[test]
    fn nextbuild_argument_list() {
        let a = parse(&["p.bas", "--arch=zxnext", "-W", "160", "-W", "140", "-S", "$8000", "-O", "4", "-H", "1024", "-M", "p.map", "-o", "p.bin", "-I", "a", "-I", "b", "-D", "ZXNEXT"]);
        assert_eq!(a.program.as_deref(), Some("p.bas"));
        assert_eq!(a.arch.as_deref(), Some("zxnext"));
        assert_eq!(a.disable_warnings, ["160", "140"]);
        assert_eq!(a.org.as_deref(), Some("$8000"));
        assert_eq!(a.include_path, "b");
        assert_eq!(a.defines, ["ZXNEXT"]);
    }

    #[test]
    fn short_clusters_and_attached_values() {
        let a = parse(&["-aB", "-O4", "-W160", "+W", "150", "--optim=2", "x.bas"]);
        assert!(a.autorun && a.basic);
        assert_eq!(a.optimize.as_deref(), Some("2"));
        assert_eq!(a.disable_warnings, ["160"]);
        assert_eq!(a.enable_warnings, ["150"]);
    }

    #[test]
    fn numbers() {
        assert_eq!(parse_int("0xFF"), Some(255));
        assert_eq!(parse_int("$ff"), Some(255));
        assert_eq!(parse_int("0FFh"), Some(255));
        assert_eq!(parse_int("FFh"), None);
        assert_eq!(parse_int("111b"), Some(7));
        assert_eq!(parse_int("%111"), Some(7));
        assert_eq!(parse_int("32768"), Some(32768));
        assert_eq!(parse_int(""), None);
    }
}
