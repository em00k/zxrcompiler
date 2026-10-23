//! `--sld`: source-level debug data. A feature of this compiler alone.

use std::path::PathBuf;
use std::process::Command;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

/// Compiles `source` in `dir` with and without `--sld` and returns the
/// debug data, having seen that the code is the same either way.
fn compile(dir: &str, source: &str, level: &str, more: &[&str], outputs: &[&str]) -> String {
    let dir = root().join(dir);
    // Tests run side by side, and two may compile the same file.
    static RUNS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let run = RUNS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = std::env::temp_dir().join(format!("zxr-sld-{}-{run}-{source}-{level}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let run = |name: &str, sld: bool| {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_zxbc"));
        cmd.current_dir(&dir).env("ZXR_LIB", root().join("lib")).args(["--arch=zxnext", level, "-S", "32768", source, "-o", tmp.join(name).to_str().unwrap()]).args(more);
        if sld {
            cmd.args(["--sld", tmp.join("out.sld").to_str().unwrap()]);
        }
        let out = cmd.output().unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    };
    run("plain.bin", false);
    run("debug.bin", true);
    for suffix in outputs {
        let read = |name: &str| std::fs::read(tmp.join(name).with_extension(suffix)).unwrap();
        assert!(read("plain") == read("debug"), "{source} {level}: .{suffix} differs with --sld");
    }
    let sld = std::fs::read_to_string(tmp.join("out.sld")).unwrap();
    std::fs::remove_dir_all(&tmp).unwrap();
    // Paths in it are absolute.
    sld.replace(&format!("{}/", root().display()), "")
}

/// Every line of BASIC, the asm in it, and the routines' entries and exits.
#[test]
fn lines_of_a_small_program() {
    let sld = compile("tests/sld", "demo.bas", "-O2", &[], &["bin"]);
    let expected = std::fs::read_to_string(root().join("tests/sld/demo.sld")).unwrap();
    assert_eq!(sld, expected);
}

/// A global is also given under the name it has in BASIC: an array as its
/// first cell, and not where asm has a label of that name.
#[test]
fn variables_by_their_basic_names() {
    let sld = compile("tests/sld", "demo.bas", "-O2", &[], &["bin"]);
    let at = |name: &str| {
        let rows: Vec<&str> = sld.lines().filter(|l| l.ends_with(&format!("|L|,{name},,"))).collect();
        assert_eq!(rows.len(), 1, "{name}");
        let fields: Vec<&str> = rows[0].split('|').collect();
        (fields[1].to_string(), fields[4].to_string(), fields[5].to_string())
    };
    assert_eq!((at("score").1, at("score").2), (at("_score").1, at("_score").2));
    assert_eq!(at("score").0, "2");
    assert_eq!(at("cells").2, at("_cells.__DATA__").2);
    assert_eq!(at("flags").0, "11");
}

/// The code is the same with and without, whatever the optimiser does.
#[test]
fn same_code_at_every_level() {
    for level in ["-O0", "-O1", "-O3", "-O4"] {
        let sld = compile("tests/sld", "demo.bas", level, &[], &["bin"]);
        assert!(sld.lines().any(|l| l.starts_with("tests/sld/demo.bas|22|") && l.ends_with("|T|")), "{level}");
    }
}

/// Code in a bank is of that bank's page, and the library's asm is there.
#[test]
fn code_banks_and_the_library() {
    let sld = compile("tests/private", "private_test.bas", "-O4", &["--code-bank-base", "40"], &["bin", "bank1.bin", "bank2.bin"]);
    let traced = |file: &str, page: &str| sld.lines().any(|l| l.ends_with("|T|") && l.contains(file) && l.split('|').nth(4) == Some(page));
    assert!(traced("private_test.bas", "4") && traced("private_test.bas", "40") && traced("private_test.bas", "41"));
    assert!(traced("runtime/farcall.asm", "4"));
}
