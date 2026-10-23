//! `PRIVATE` routines in a CODEBANK: a feature of this compiler alone, so
//! there is no oracle to compare with. `tests/private/README.md` says how
//! `private_test.bas` is run for real.

use std::path::PathBuf;
use std::process::{Command, Output};

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/private")
}

fn zxbc(source: &str, output: &str) -> Output {
    let lib = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../lib");
    Command::new(env!("CARGO_BIN_EXE_zxbc")).current_dir(dir()).env("ZXR_LIB", lib).args(["--arch=zxnext", "-O2", "-f", "asm", source, "-o", output]).output().unwrap()
}

fn rejected(name: &str) {
    let out = zxbc(&format!("{name}.bas"), "/dev/null");
    assert_eq!(out.status.code(), Some(1));
    let expected = std::fs::read_to_string(dir().join(format!("{name}.err"))).unwrap();
    assert_eq!(String::from_utf8(out.stderr).unwrap(), expected);
}

#[test]
fn calls_and_addresses_from_outside_the_bank() {
    rejected("private_errors");
}

#[test]
fn outside_a_bank_and_against_its_declaration() {
    rejected("private_decl");
}

/// `@a < n`, `@a AND @b`, `@a bAND n`: the Python compiler has no way of
/// writing these out and stops. `@a << 1` it writes as `>>`.
#[test]
fn operators_on_addresses() {
    let asm = std::env::temp_dir().join(format!("zxr-address-ops-{}.asm", std::process::id()));
    let lib = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../lib");
    let out = Command::new(env!("CARGO_BIN_EXE_zxbc")).current_dir(dir().join("../fixes")).env("ZXR_LIB", lib).args(["--arch=zxnext", "-O2", "-f", "asm", "address_ops.bas", "-o", asm.to_str().unwrap()]).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let text = std::fs::read_to_string(&asm).unwrap();
    std::fs::remove_file(&asm).unwrap();
    assert!(text.contains("(_ga) << (1)") && text.contains("(_ga) >> (1)"), "shifts of an address");
}

/// A bank with no routine in it, read through farmem: the runtime the far
/// memory routines lean on is linked, and W910 is not given.
#[test]
fn data_only_bank_with_far_memory() {
    let bin = std::env::temp_dir().join(format!("zxr-data-only-{}.bin", std::process::id()));
    let lib = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../lib");
    let out = Command::new(env!("CARGO_BIN_EXE_zxbc")).current_dir(dir().join("../fixes")).env("ZXR_LIB", lib).args(["--arch=zxnext", "-O2", "-S", "32768", "data_only_bank.bas", "-o", bin.to_str().unwrap()]).output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    for suffix in ["bin", "bank1.bin", "bank2.bin", "banks.json"] {
        let _ = std::fs::remove_file(bin.with_extension(suffix));
    }
    assert!(out.status.success(), "{stderr}");
    assert!(!stderr.contains("W910"), "{stderr}");
}

/// Input that used to end the tools with a stack overflow, a panic, an
/// allocation of everything, or not at all: each is an error with a line.
#[test]
fn bad_input_is_an_error() {
    let bad = dir().join("../fixes/bad");
    let lib = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../lib");
    for (tool, name, args) in [
        (env!("CARGO_BIN_EXE_zxbasm"), "cyclic_equ.asm", vec!["-o", "/dev/null"]),
        (env!("CARGO_BIN_EXE_zxbasm"), "overflow.asm", vec!["-o", "/dev/null"]),
        (env!("CARGO_BIN_EXE_zxbasm"), "defs.asm", vec!["-o", "/dev/null"]),
        (env!("CARGO_BIN_EXE_zxbc"), "include_self.bas", vec!["--arch=zxnext", "--parse-only"]),
        (env!("CARGO_BIN_EXE_zxbc"), "array.bas", vec!["--arch=zxnext", "--parse-only"]),
        (env!("CARGO_BIN_EXE_zxbc"), "literal.bas", vec!["--arch=zxnext", "--parse-only"]),
    ] {
        let out = Command::new(tool).current_dir(&bad).env("ZXR_LIB", &lib).arg(name).args(&args).output().unwrap();
        assert_eq!(out.status.code(), Some(1), "{name}");
        let expected = std::fs::read_to_string(bad.join(name).with_extension("err")).unwrap();
        assert_eq!(String::from_utf8(out.stderr).unwrap(), expected, "{name}");
    }
}

/// The warnings the Python compiler does not have, as an editor running
/// `--parse-only` sees them, and gone when a comparison asks for that.
fn warned(name: &str) {
    let run = |oracle: bool| {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_zxbc"));
        cmd.current_dir(dir()).args(["--arch=zxnext", "--parse-only", &format!("{name}.bas")]);
        if oracle {
            cmd.env("ZXR_ORACLE_DIAGNOSTICS", "1");
        }
        let out = cmd.output().unwrap();
        assert!(out.status.success());
        String::from_utf8(out.stderr).unwrap()
    };
    let expected = std::fs::read_to_string(dir().join(format!("{name}.err"))).unwrap();
    assert_eq!(run(false), expected);
    let kept: String = expected.lines().filter(|l| !l.contains("[W930]") && !l.contains("[W940]") && !l.contains("[W950]")).map(|l| format!("{l}\n")).collect();
    assert_eq!(run(true), kept);
}

#[test]
fn could_be_private() {
    warned("lint_private");
}

#[test]
fn fastcall_without_a_frame() {
    warned("lint_fastcall");
}

#[test]
fn unused_local_under_asm() {
    warned("lint_local_asm");
}

/// An error the assembler finds once all labels are known names the file
/// the line is in, not the one read last; and the Python compiler's answer
/// when a comparison asks for it.
#[test]
fn late_assembler_errors_name_their_file() {
    let lib = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../lib");
    let run = |oracle: bool| {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_zxbc"));
        cmd.current_dir(dir().join("../fixes")).env("ZXR_LIB", &lib).args(["--arch=zxnext", "-O2", "jump_range.bas", "-o", "/dev/null"]);
        if oracle {
            cmd.env("ZXR_ORACLE_DIAGNOSTICS", "1");
        }
        let out = cmd.output().unwrap();
        assert_eq!(out.status.code(), Some(5));
        String::from_utf8(out.stderr).unwrap()
    };
    let expected = std::fs::read_to_string(dir().join("../fixes/jump_range.err")).unwrap();
    assert_eq!(run(false), expected);
    assert_eq!(run(true), expected.replace("jump_range.bas", "jump_range_inc.bas"));
}

/// Globals that only asm names are kept above -O1, where the Python
/// compiler removes them and the asm does not assemble.
#[test]
fn globals_only_asm_uses_are_kept() {
    let lib = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../lib");
    let run = |oracle: bool| {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_zxbc"));
        cmd.current_dir(dir().join("../fixes")).env("ZXR_LIB", &lib).args(["--arch=zxnext", "-O2", "asm_only_vars.bas", "-o", "/dev/null"]);
        if oracle {
            cmd.env("ZXR_ORACLE_DIAGNOSTICS", "1");
        }
        cmd.output().unwrap()
    };
    let out = run(false);
    assert!(out.status.success() && out.stderr.is_empty(), "{}", String::from_utf8_lossy(&out.stderr));
    let out = run(true);
    assert_eq!(out.status.code(), Some(5));
    assert!(String::from_utf8_lossy(&out.stderr).contains("Undefined GLOBAL label '._counter'"));
}

#[test]
fn no_trampoline_and_a_direct_call() {
    let asm = std::env::temp_dir().join(format!("zxr-private-{}.asm", std::process::id()));
    let out = zxbc("private_test.bas", asm.to_str().unwrap());
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let text = std::fs::read_to_string(&asm).unwrap();
    std::fs::remove_file(&asm).unwrap();
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    // A public routine of the bank: trampoline and far body.
    assert!(lines.contains(&"DEFW _RunAdd.__far") && lines.contains(&"_RunAdd.__far:"));
    // A private one: its own label, in the bank, and nothing else.
    for name in ["_Mark", "_Add", "_Twice", "_Fact", "_Sum", "_Name", "_ViaTen"] {
        assert!(lines.contains(&format!("{name}:").as_str()), "{name}");
        assert!(!text.contains(&format!("{name}.__far")), "{name}");
    }
    assert!(lines.contains(&"call _Add"));
    let bank = lines.iter().position(|l| *l == "CODEBANK 1").unwrap();
    let label = lines.iter().position(|l| *l == "_Add:").unwrap();
    let end = bank + lines[bank..].iter().position(|l| *l == "CODEBANK 0").unwrap();
    assert!(bank < label && label < end);
}
