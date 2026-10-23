//! Records (`TYPE … END TYPE`): a feature of this compiler alone, lowered to
//! plain BASIC before the parser (`crates/zxr-syntax/src/records.rs`).
//! `tests/records/README.md` says how `records_test.bas` is run for real.

use std::path::PathBuf;
use std::process::{Command, Output};

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/records")
}

fn zxbc(source: &str, output: &str, level: &str, dump: Option<&PathBuf>) -> Output {
    let lib = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../lib");
    let mut command = Command::new(env!("CARGO_BIN_EXE_zxbc"));
    command.current_dir(dir()).env("ZXR_LIB", lib).args(["--arch=zxnext", level, "-S", "32768", source, "-o", output]);
    if let Some(dump) = dump {
        command.env("ZXR_DUMP_RECORDS", dump);
    }
    command.output().unwrap()
}

/// The test program compiles at every level, and its lowered text is what
/// was recorded: a change to the lowering shows here as a diff of BASIC.
#[test]
fn the_test_program_and_its_lowered_text() {
    let tmp = std::env::temp_dir().join(format!("zxr-records-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let dump = tmp.join("lowered.bas");
    for level in ["-O0", "-O2", "-O4"] {
        let bin = tmp.join("records_test.bin");
        let out = zxbc("records_test.bas", bin.to_str().unwrap(), level, Some(&dump));
        assert!(out.status.success(), "{level}: {}", String::from_utf8_lossy(&out.stderr));
    }
    let lowered = std::fs::read_to_string(&dump).unwrap();
    let _ = std::fs::remove_dir_all(&tmp);
    // Only the program's own lines: the #line paths name this machine.
    let own = |text: &str| text.lines().filter(|l| !l.starts_with("#line")).collect::<Vec<_>>().join("\n");
    let expected = std::fs::read_to_string(dir().join("records_test.lowered.bas")).unwrap();
    assert_eq!(own(&lowered), own(&expected));
}

#[test]
fn mistakes() {
    let out = zxbc("records_errors.bas", "/dev/null", "-O2", None);
    assert_eq!(out.status.code(), Some(1));
    let expected = std::fs::read_to_string(dir().join("records_errors.err")).unwrap();
    assert_eq!(String::from_utf8(out.stderr).unwrap(), expected);
}

/// A program without TYPE is not touched: `type` stays an ordinary name.
#[test]
fn type_as_a_name() {
    let tmp = std::env::temp_dir().join(format!("zxr-records-name-{}.bas", std::process::id()));
    std::fs::write(&tmp, "dim type as ubyte\ntype = 3\nprint type\n").unwrap();
    let out = zxbc(tmp.to_str().unwrap(), "/dev/null", "-O2", None);
    let _ = std::fs::remove_file(&tmp);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
}
