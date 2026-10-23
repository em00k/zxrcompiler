//! The NextBuild source corpus, the oracle pin and the runtime library copy.
//!
//! "Should compile" is defined by what the oracle does with each file, not by
//! guessing. `corpus manifest` records that; `corpus check` holds any
//! toolchain to it. A file the oracle rejects must be rejected the same way.

use crate::toolchain::{Tool, Toolchain};
use crate::util::{self, par_map, repo_root, sha256, write_file, Scratch};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(600);

// Scripts/nextbuild.config: DEFAULT_ORG, DEFAULT_HEAP, DEFAULT_OPTIMIZE.
const DEFAULT_ORG: &str = "32768";
const DEFAULT_HEAP: &str = "1024";
const DEFAULT_OPT: &str = "4";

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("git {} failed in {}", args.join(" "), dir.display()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// The oracle must be exactly the commit oracle.lock names, with no edits.
pub fn oracle_check() -> Result<(), String> {
    let root = repo_root();
    let lock = fs::read_to_string(root.join("oracle.lock")).map_err(|e| format!("oracle.lock: {e}"))?;
    let want = lock
        .lines()
        .find_map(|l| l.strip_prefix("commit "))
        .ok_or("oracle.lock has no `commit` line")?
        .trim()
        .to_string();
    let oracle = root.join("reference/zxbasic");
    let have = git(&oracle, &["rev-parse", "HEAD"])?;
    if have != want {
        return Err(format!("reference/zxbasic is at {have}, oracle.lock pins {want}"));
    }
    let dirty = git(&oracle, &["status", "--porcelain", "--untracked-files=no"])?;
    if !dirty.is_empty() {
        return Err(format!("reference/zxbasic has uncommitted changes:\n{dirty}"));
    }
    Ok(())
}

fn lib_sums(dir: &Path) -> String {
    util::walk(dir)
        .iter()
        .filter(|rel| rel.file_name().is_some_and(|n| n != "SHA256SUMS"))
        .map(|rel| format!("{}  {}\n", sha256(&fs::read(dir.join(rel)).unwrap_or_default()), rel.display()))
        .collect()
}

/// The runtime and standard library are data the compiler ships. They are
/// copied from the oracle unchanged; identical output depends on it.
pub fn lib_sync() -> Result<(), String> {
    oracle_check()?;
    let src = repo_root().join("reference/zxbasic/src/lib");
    let dst = repo_root().join("lib");
    let _ = fs::remove_dir_all(&dst);
    let files = util::walk(&src);
    for rel in &files {
        write_file(&dst.join(rel), &fs::read(src.join(rel)).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    }
    write_file(&dst.join("SHA256SUMS"), lib_sums(&dst).as_bytes()).map_err(|e| e.to_string())?;
    println!("copied {} library files into lib/", files.len());
    Ok(())
}

pub fn lib_check() -> Result<bool, String> {
    let dst = repo_root().join("lib");
    let recorded = fs::read_to_string(dst.join("SHA256SUMS")).map_err(|e| format!("lib/SHA256SUMS: {e}"))?;
    let mut ok = true;
    if recorded != lib_sums(&dst) {
        println!("lib/ does not match lib/SHA256SUMS");
        ok = false;
    }
    let oracle = repo_root().join("reference/zxbasic/src/lib");
    if oracle.exists() && recorded != lib_sums(&oracle) {
        println!("lib/ does not match the oracle's src/lib");
        ok = false;
    }
    if ok {
        println!("lib/ matches ({} files)", recorded.lines().count());
    }
    Ok(ok)
}

pub(crate) fn corpus_root() -> PathBuf {
    std::env::var("ZXR_CORPUS").map(PathBuf::from).unwrap_or_else(|_| repo_root().join("reference/corpus"))
}

/// The settings nextbuild.py reads from `'!key=value` comments in the first
/// 64 lines of a source file.
#[derive(Default, Debug)]
struct Directives {
    org: Option<String>,
    heap: Option<String>,
    opt: Option<String>,
    headerless: bool,
    code_window: Option<String>,
    code_window_size: Option<String>,
    code_bank_base: Option<String>,
    code_bank_pages: Option<String>,
    code_bank_depth: Option<String>,
}

/// `(\$[0-9a-f]+|0x[0-9a-f]+|\d+)` at the start of `s`.
fn number(s: &str) -> Option<&str> {
    let hex = |t: &str| t.bytes().take_while(u8::is_ascii_hexdigit).count();
    let n = if let Some(rest) = s.strip_prefix('$').filter(|r| hex(r) > 0) {
        1 + hex(rest)
    } else if let Some(rest) = s.strip_prefix("0x").filter(|r| hex(r) > 0) {
        2 + hex(rest)
    } else {
        s.bytes().take_while(u8::is_ascii_digit).count()
    };
    (n > 0).then(|| &s[..n])
}

/// nextbuild.py's `_tidy_directive`: `'! codebank = 80` reads as `'!codebank=80`.
fn tidy(line: &str) -> String {
    let ident = |s: &str| s.bytes().take_while(|b| b.is_ascii_alphanumeric() || *b == b'_').count();
    let ws = |s: &str| s.len() - s.trim_start().len();
    let mut text = line.to_string();
    for (at, _) in line.match_indices("'!") {
        let rest = &line[at + 2..];
        let a = ws(rest);
        let key = &rest[a..];
        let k = ident(key);
        if k == 0 || key.as_bytes()[0].is_ascii_digit() {
            continue;
        }
        let after = &key[k..];
        let b = ws(after);
        if let Some(value) = after[b..].strip_prefix('=') {
            text = format!("{}'!{}={}", &line[..at], &key[..k], value.trim_start());
            break;
        }
    }
    let snapshot = text.clone();
    for (at, _) in snapshot.match_indices("'!") {
        let rest = &snapshot[at + 2..];
        let a = ws(rest);
        if a > 0 && rest[a..].starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') {
            text = format!("{}'!{}", &snapshot[..at], &rest[a..]);
            break;
        }
    }
    text
}

fn directives(source: &[u8]) -> Directives {
    let text = String::from_utf8_lossy(source);
    let mut d = Directives::default();
    for raw in text.lines().take(64) {
        let line = tidy(raw.trim()).to_lowercase();
        let find = |key: &str| line.find(&format!("'!{key}=")).map(|at| &line[at + key.len() + 3..]);
        let num = |key: &str| find(key).and_then(number).map(String::from);
        let set = |slot: &mut Option<String>, key: &str| {
            if let Some(v) = num(key) {
                *slot = Some(v);
            }
        };
        set(&mut d.org, "org");
        set(&mut d.heap, "heap");
        set(&mut d.opt, "opt");
        set(&mut d.code_window, "codewindow");
        set(&mut d.code_window_size, "codewindowsize");
        set(&mut d.code_bank_base, "codebank");
        set(&mut d.code_bank_depth, "codebankdepth");
        if let Some(rest) = find("codebankpages") {
            let n = rest.bytes().take_while(|b| b.is_ascii_hexdigit() || b"x$, \t".contains(b)).count();
            if n > 0 {
                d.code_bank_pages = Some(rest[..n].trim().to_string());
            }
        }
        if line.contains("'!headerless") {
            d.headerless = true;
        }
    }
    d
}

/// The argument list nextbuild.py passes to zxbc for a NEX build, minus the
/// input, `-M` and `-o`, which depend on where the outputs go.
fn flags(d: &Directives, scripts: &Path) -> Vec<String> {
    let mut args: Vec<String> = vec!["--arch=zxnext".into()];
    if d.headerless {
        args.push("--headerless".into());
    }
    for code in ["160", "140", "150", "170", "190"] {
        args.extend(["-W".to_string(), code.to_string()]);
    }
    let pair = |flag: &str, value: &str| [flag.to_string(), value.to_string()];
    args.extend(pair("-S", d.org.as_deref().unwrap_or(DEFAULT_ORG)));
    args.extend(pair("-O", d.opt.as_deref().unwrap_or(DEFAULT_OPT)));
    args.extend(pair("-H", d.heap.as_deref().unwrap_or(DEFAULT_HEAP)));
    args.extend(pair("-I", &scripts.to_string_lossy()));
    if !d.headerless {
        args.extend(pair("-D", "ZXNEXT"));
        for (value, flag) in [
            (&d.code_window, "--code-window"),
            (&d.code_window_size, "--code-window-size"),
            (&d.code_bank_base, "--code-bank-base"),
            (&d.code_bank_pages, "--code-bank-pages"),
            (&d.code_bank_depth, "--code-bank-depth"),
        ] {
            if let Some(v) = value {
                args.extend(pair(flag, v));
            }
        }
    }
    args
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct Record {
    pub(crate) rel: String,
    pub(crate) tier: String,
    /// Exit code, or "skip" for a file that is not zxbasic source.
    pub(crate) exit: String,
    /// `bin=<hash>,map=<hash>,...`, or `-`.
    outputs: String,
    stderr: String,
    pub(crate) flags: String,
    /// First error line, for people; never compared.
    note: String,
}

impl Record {
    fn line(&self) -> String {
        format!("{}\t{}\t{}\t{}\t{}\t{}\t{}\n", self.rel, self.tier, self.exit, self.outputs, self.stderr, self.flags, self.note)
    }

    fn parse(line: &str) -> Option<Self> {
        let f: Vec<&str> = line.split('\t').collect();
        (f.len() == 7).then(|| Record {
            rel: f[0].into(),
            tier: f[1].into(),
            exit: f[2].into(),
            outputs: f[3].into(),
            stderr: f[4].into(),
            flags: f[5].into(),
            note: f[6].into(),
        })
    }

    fn same_result(&self, other: &Record) -> bool {
        self.exit == other.exit && self.outputs == other.outputs && self.stderr == other.stderr
    }
}

fn short(hash: String) -> String {
    hash[..16].to_string()
}

fn sources(root: &Path) -> Vec<String> {
    util::walk(&root.join("Sources"))
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("bas")))
        .map(|p| format!("Sources/{}", p.display()))
        .collect()
}

fn tier_a() -> BTreeSet<String> {
    let text = fs::read_to_string(repo_root().join("tests/corpus/tier-a.txt")).unwrap_or_default();
    text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).map(String::from).collect()
}

/// Which run of the corpus: the full build, or an intermediate stage, at the
/// optimisation level each source asks for or at one forced on all of them.
#[derive(Clone, Copy, Default)]
pub struct Seam<'a> {
    pub stage: Option<&'a str>,
    /// `--opt N`: replaces the level derived from the source.
    pub opt: Option<&'a str>,
    /// `--keep DIR`: where to leave what the toolchain wrote, for diffing.
    pub keep: Option<&'a Path>,
    /// `--flow-stand-in`: at -O3 and above, the port is given what the
    /// oracle's flow optimiser made of the program's code to use in place
    /// of an optimiser of its own (and checks that the lines it would have
    /// optimised are the lines the oracle's was given).
    pub flow_stand_in: bool,
}

/// Saves the cleaned output and diagnostics of one run under
/// `<dir>/<toolchain>/`, named after the case.
pub fn keep_outputs(dir: &Path, toolchain: &Toolchain, case: &str, files: &[(String, Vec<u8>)], diagnostics: &[u8]) {
    let name: String = case.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') { c } else { '_' }).collect();
    let base = dir.join(toolchain.label());
    for (key, data) in files {
        let _ = write_file(&base.join(format!("{name}.{key}")), data);
    }
    let _ = write_file(&base.join(format!("{name}.stderr")), diagnostics);
}

/// Runs one corpus file. With a `stage`, the compiler stops there and its
/// intermediate output is what gets recorded.
fn run_one(toolchain: &Toolchain, root: &Path, rel: &str, tier_a: &BTreeSet<String>, index: usize, seam: Seam) -> (Record, Duration) {
    let stage = seam.stage;
    let path = root.join(rel);
    let source = fs::read(&path).unwrap_or_default();
    let tier = if rel.starts_with("Sources/OlderSources/") {
        "C"
    } else if tier_a.contains(rel) {
        "A"
    } else {
        "B"
    };
    let mut record = Record { rel: rel.into(), tier: tier.into(), exit: "skip".into(), outputs: "-".into(), stderr: "-".into(), flags: "-".into(), note: String::new() };
    // Tokenised NextBASIC loaders and other binaries carry a .bas extension too.
    let head = &source[..source.len().min(512)];
    if source.is_empty() || source.starts_with(b"PLUS3DOS") || head.contains(&0) {
        record.note = "not zxbasic source".into();
        return (record, Duration::ZERO);
    }
    let scratch = Scratch::new("corpus", index).expect("scratch dir");
    let name = path.file_stem().unwrap().to_string_lossy().to_string();
    let mut args = flags(&directives(&source), &root.join("Scripts"));
    if let Some(level) = seam.opt {
        if let Some(at) = args.iter().position(|a| a == "-O") {
            args[at + 1] = level.to_string();
        }
    }
    let mut cmd = match stage {
        Some(stage) => toolchain.stage_command(stage),
        None => toolchain.command(Tool::Zxbc),
    };
    cmd.current_dir(path.parent().unwrap()).arg(&path).args(&args);
    if seam.flow_stand_in && stage.is_none() && matches!(toolchain, Toolchain::Port { .. }) {
        let mut all = vec![path.to_string_lossy().to_string()];
        all.extend(args.iter().cloned());
        let level_tag = seam.opt.map_or("own".to_string(), |l| format!("O{l}"));
        // A compile the oracle fails after its optimiser ran has a record
        // too; one it fails before has none, and needs none.
        if let Ok(Some((cache, _))) = crate::backend::oracle_record(rel, &level_tag, path.parent().unwrap(), &all, index) {
            if cache.join("out.asm.opt-out").exists() {
                cmd.arg("--flow-stand-in").arg(cache.join("out.asm"));
            }
        }
    }
    match stage {
        Some(stage) => cmd.arg("-o").arg(scratch.0.join(format!("{name}.{stage}"))),
        None => cmd.arg("-M").arg(scratch.0.join(format!("{name}.map"))).arg("-o").arg(scratch.0.join(format!("{name}.bin"))),
    };
    let result = util::run(cmd, &scratch.0, TIMEOUT).expect("failed to start the toolchain");

    let root_str = root.to_string_lossy().to_string();
    let scratch_str = scratch.0.to_string_lossy().to_string();
    let clean = |data: &[u8]| {
        let data = util::replace_bytes(&toolchain.normalise(data), root_str.as_bytes(), b"/corpus");
        util::replace_bytes(&data, scratch_str.as_bytes(), b"/out")
    };
    let mut diagnostics = clean(&result.stderr);
    diagnostics.extend_from_slice(&clean(&result.stdout));
    let prefix = format!("{name}.");
    let files: Vec<(String, Vec<u8>)> = util::walk(&scratch.0)
        .iter()
        .filter_map(|file| {
            let key = file.to_string_lossy().strip_prefix(&prefix)?.to_string();
            let data = fs::read(scratch.0.join(file)).ok()?;
            // Intermediate text names files by path; binaries do not.
            let data = if stage.is_some() { clean(&data) } else { data };
            Some((key, data))
        })
        .collect();
    if let Some(dir) = seam.keep {
        keep_outputs(dir, toolchain, rel, &files, &diagnostics);
    }
    let outputs: Vec<String> = files.iter().map(|(key, data)| format!("{key}={}", short(sha256(data)))).collect();
    record.exit = if result.timed_out { "timeout".into() } else { result.code.to_string() };
    record.outputs = if outputs.is_empty() { "-".into() } else { outputs.join(",") };
    record.stderr = if diagnostics.is_empty() { "-".into() } else { short(sha256(&diagnostics)) };
    record.flags = args.join(" ").replace(&root_str, "/corpus");
    let text = String::from_utf8_lossy(&diagnostics);
    let first = text.lines().find(|l| l.contains("error")).or_else(|| text.lines().last()).unwrap_or("");
    record.note = if result.code == 0 { String::new() } else { first.replace('\t', " ").chars().take(160).collect() };
    (record, result.elapsed)
}

/// The full build is recorded in manifest.tsv; each intermediate stage has a
/// file of its own.
fn manifest_path(seam: Seam) -> PathBuf {
    repo_root().join("tests/corpus").join(match (seam.stage, seam.opt) {
        (Some(stage), Some(level)) => format!("seam-{stage}-O{level}.tsv"),
        (Some(stage), None) => format!("seam-{stage}.tsv"),
        (None, Some(level)) => format!("manifest-O{level}.tsv"),
        (None, None) => "manifest.tsv".to_string(),
    })
}

pub(crate) fn load_manifest(seam: Seam) -> Result<Vec<Record>, String> {
    let text = fs::read_to_string(manifest_path(seam)).map_err(|_| "nothing recorded; run `cargo xtask corpus manifest` first".to_string())?;
    Ok(text.lines().filter(|l| !l.starts_with('#')).filter_map(Record::parse).collect())
}

fn snapshot_stamp(root: &Path) -> String {
    fs::read_to_string(root.join("SNAPSHOT_DATE")).unwrap_or_default().trim().to_string()
}

pub fn manifest(seam: Seam) -> Result<(), String> {
    let stage = seam.stage;
    let root = corpus_root();
    let files = sources(&root);
    if files.is_empty() {
        return Err(format!("no .bas files under {}/Sources", root.display()));
    }
    let tier_a = tier_a();
    let oracle = Toolchain::oracle();
    let results = par_map(&files, |i, rel| run_one(&oracle, &root, rel, &tier_a, i, seam));
    let lock = fs::read_to_string(repo_root().join("oracle.lock")).unwrap_or_default();
    let mut text = format!(
        "# What the oracle makes of every .bas in the corpus snapshot.\n# snapshot: {}\n# oracle: {}\n# path\ttier\texit\toutputs\tdiagnostics\tflags\tfirst error\n",
        snapshot_stamp(&root),
        lock.lines().map(str::trim).collect::<Vec<_>>().join(", ")
    );
    let mut timings = String::from("# Oracle wall-clock time per corpus file, milliseconds. The baseline for the speed goal.\n");
    for (record, elapsed) in &results {
        text.push_str(&record.line());
        if record.exit != "skip" {
            timings.push_str(&format!("{}\t{}\n", record.rel, elapsed.as_millis()));
        }
    }
    write_file(&manifest_path(seam), text.as_bytes()).map_err(|e| e.to_string())?;
    if stage.is_none() && seam.opt.is_none() {
        write_file(&repo_root().join("tests/corpus/oracle-timings.tsv"), timings.as_bytes()).map_err(|e| e.to_string())?;
    }
    summary(seam)
}

pub fn summary(seam: Seam) -> Result<(), String> {
    let records = load_manifest(seam)?;
    let mut table: BTreeMap<&str, [usize; 4]> = BTreeMap::new();
    let mut reasons: BTreeMap<String, usize> = BTreeMap::new();
    for r in &records {
        let row = table.entry(r.tier.as_str()).or_default();
        match r.exit.as_str() {
            "0" => row[0] += 1,
            "skip" => row[2] += 1,
            "timeout" => row[3] += 1,
            _ => {
                row[1] += 1;
                // Group by message, dropping the file and line.
                let msg = r.note.split(": error: ").nth(1).unwrap_or(&r.note);
                let key: String = msg.split(['\'', '"']).next().unwrap_or(msg).trim().chars().take(70).collect();
                *reasons.entry(key).or_default() += 1;
            }
        }
    }
    println!("{:<6} {:>9} {:>9} {:>9} {:>9}", "tier", "compiles", "rejected", "not-src", "timeout");
    for (tier, row) in &table {
        println!("{tier:<6} {:>9} {:>9} {:>9} {:>9}", row[0], row[1], row[2], row[3]);
    }
    println!("{} files", records.len());
    let mut reasons: Vec<_> = reasons.into_iter().collect();
    reasons.sort_by(|a, b| b.1.cmp(&a.1));
    println!("\nmost common reasons for rejection:");
    for (reason, count) in reasons.iter().take(15) {
        println!("{count:>6}  {reason}");
    }
    Ok(())
}

pub fn check(toolchain: &Toolchain, seam: Seam, filter: Option<String>, verbose: bool) -> Result<bool, String> {
    let root = corpus_root();
    let expected: Vec<Record> =
        load_manifest(seam)?.into_iter().filter(|r| r.exit != "skip" && filter.as_ref().is_none_or(|f| r.rel.contains(f.as_str()))).collect();
    let tier_a = tier_a();
    let actual = par_map(&expected, |i, r| run_one(toolchain, &root, &r.rel, &tier_a, i, seam).0);
    let mut table: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut failures = Vec::new();
    for (want, got) in expected.iter().zip(&actual) {
        let row = table.entry(want.tier.clone()).or_default();
        row.1 += 1;
        if want.same_result(got) {
            row.0 += 1;
        } else {
            let what = if want.exit != got.exit {
                format!("exit {} (expected {})", got.exit, want.exit)
            } else if want.outputs != got.outputs {
                "outputs differ".to_string()
            } else {
                "diagnostics differ".to_string()
            };
            failures.push(format!("[{}] {}: {what}", want.tier, want.rel));
        }
    }
    println!("{:<6} {:>8} {:>8}", "tier", "same", "total");
    for (tier, (same, total)) in &table {
        println!("{tier:<6} {same:>8} {total:>8}");
    }
    let shown = if verbose { failures.len() } else { failures.len().min(20) };
    for failure in &failures[..shown] {
        println!("DIFF {failure}");
    }
    if failures.len() > shown {
        println!("... and {} more (use --verbose)", failures.len() - shown);
    }
    Ok(failures.is_empty())
}

/// The hybrid pipeline: the oracle's front end and code generator, the
/// port's assembler. For every program the oracle compiles, the oracle's
/// generated assembly is assembled by the port and the outputs compared with
/// the manifest. This holds the assembler to the whole corpus before the
/// port has a front end of its own.
pub fn hybrid_asm(filter: Option<String>, verbose: bool) -> Result<bool, String> {
    let root = corpus_root();
    let records: Vec<Record> =
        load_manifest(Seam::default())?.into_iter().filter(|r| r.exit == "0" && filter.as_ref().is_none_or(|f| r.rel.contains(f.as_str()))).collect();
    let cache = repo_root().join("target/oracle-cache/asm");
    let oracle = Toolchain::oracle();
    let port = Toolchain::port();
    let root_str = root.to_string_lossy().to_string();
    let results = par_map(&records, |i, record| -> Result<String, String> {
        let path = root.join(&record.rel);
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let flags: Vec<String> = record.flags.replace("/corpus", &root_str).split(' ').map(String::from).collect();
        let asm = cache.join(format!("{}.asm", record.rel));
        if !asm.exists() {
            fs::create_dir_all(asm.parent().unwrap()).map_err(|e| e.to_string())?;
            let scratch = Scratch::new("hybrid-oracle", i).map_err(|e| e.to_string())?;
            let mut cmd = oracle.command(Tool::Zxbc);
            cmd.current_dir(path.parent().unwrap()).arg(&path).args(&flags).args(["-f", "asm", "-o"]).arg(scratch.0.join("out.asm"));
            let run = util::run(cmd, &scratch.0, TIMEOUT).map_err(|e| e.to_string())?;
            if run.code != 0 {
                return Err(format!("the oracle could not generate assembly (exit {})", run.code));
            }
            fs::rename(scratch.0.join("out.asm"), &asm).or_else(|_| fs::copy(scratch.0.join("out.asm"), &asm).map(|_| ())).map_err(|e| e.to_string())?;
        }
        let scratch = Scratch::new("hybrid", i).map_err(|e| e.to_string())?;
        let mut cmd = port.command(Tool::Zxbc);
        cmd.current_dir(path.parent().unwrap()).arg(&path).args(&flags).arg("--from-asm").arg(&asm);
        cmd.arg("-M").arg(scratch.0.join(format!("{name}.map"))).arg("-o").arg(scratch.0.join(format!("{name}.bin")));
        let run = util::run(cmd, &scratch.0, TIMEOUT).map_err(|e| e.to_string())?;
        let prefix = format!("{name}.");
        let outputs: Vec<String> = util::walk(&scratch.0)
            .iter()
            .filter_map(|file| {
                let key = file.to_string_lossy().strip_prefix(&prefix)?.to_string();
                Some(format!("{key}={}", short(sha256(&fs::read(scratch.0.join(file)).ok()?))))
            })
            .collect();
        if run.code != 0 {
            let text = String::from_utf8_lossy(&run.stderr).to_string();
            return Err(format!("exit {}: {}", run.code, text.lines().next().unwrap_or("")));
        }
        Ok(outputs.join(","))
    });
    let mut table: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut failures = Vec::new();
    for (record, result) in records.iter().zip(&results) {
        let row = table.entry(record.tier.clone()).or_default();
        row.1 += 1;
        match result {
            Ok(outputs) if *outputs == record.outputs => row.0 += 1,
            Ok(outputs) => {
                let mut want: BTreeSet<&str> = record.outputs.split(',').collect();
                let mut got: BTreeSet<&str> = outputs.split(',').collect();
                // A `#pragma codewindowsize` (or the like) is read by the front
                // end, which the port does not have yet, so the bank manifest
                // cannot be expected to match for such a source.
                let source = fs::read(root.join(&record.rel)).map(|b| String::from_utf8_lossy(&b).to_lowercase()).unwrap_or_default();
                let layout_pragma = source.lines().any(|l| l.trim_start().starts_with("#pragma") && (l.contains("codewindow") || l.contains("codebankbase") || l.contains("codebankpages")));
                if layout_pragma {
                    want.retain(|o| !o.starts_with("banks.json="));
                    got.retain(|o| !o.starts_with("banks.json="));
                    if want == got {
                        row.0 += 1;
                        continue;
                    }
                }
                let differing: Vec<&str> = want.symmetric_difference(&got).map(|s| s.split('=').next().unwrap()).collect::<BTreeSet<_>>().into_iter().collect();
                failures.push(format!("[{}] {}: differs in {}", record.tier, record.rel, differing.join(", ")));
            }
            Err(error) => failures.push(format!("[{}] {}: {error}", record.tier, record.rel)),
        }
    }
    println!("{:<6} {:>8} {:>8}", "tier", "same", "total");
    for (tier, (same, total)) in &table {
        println!("{tier:<6} {same:>8} {total:>8}");
    }
    let shown = if verbose { failures.len() } else { failures.len().min(20) };
    for failure in &failures[..shown] {
        println!("DIFF {failure}");
    }
    if failures.len() > shown {
        println!("... and {} more (use --verbose)", failures.len() - shown);
    }
    Ok(failures.is_empty())
}
