//! The backend seam: the oracle's front end, the port's backend.
//!
//! For each corpus program and each case of the functional suite, the
//! oracle compiles the source and records what its backend consumed
//! (`tools/oracle_stage.py backend-in`); the port finishes the compile from
//! that record (`zxbc --from-ir`), and the two results are compared: the
//! assembly text, or with `--binary` the assembled program, its code banks,
//! the bank manifest and the map.
//!
//! What the oracle produces is kept under `target/oracle-cache/backend`, so
//! only the first sweep at a level pays for it.

use crate::corpus;
use crate::golden::{self, Kind};
use crate::toolchain::{Tool, Toolchain};
use crate::util::{self, par_map, repo_root, sha256, Scratch};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(900);

pub struct Options {
    /// `corpus`, `functional` or `all`.
    pub suite: String,
    /// The optimisation level to compile everything at; `None` leaves each
    /// program the level it asks for.
    pub level: Option<u32>,
    /// Compare assembled output instead of assembly text.
    pub binary: bool,
    pub filter: Option<String>,
    pub verbose: bool,
    /// At -O3 and above, give the port the oracle's flow optimiser output
    /// in place of its own (which isolates the backend from the optimiser).
    pub flow_stand_in: bool,
}

/// One program to compile: where, and with which arguments.
struct Item {
    id: String,
    /// The row of the report it is counted in.
    group: String,
    cwd: PathBuf,
    /// Everything but `-o`, `-f` and `-M`.
    args: Vec<String>,
    /// The recorded expectation of a functional case, if it has one.
    expected: Option<PathBuf>,
}

fn with_level(mut args: Vec<String>, level: Option<u32>) -> Vec<String> {
    if let Some(level) = level {
        // The last -O on the command line wins.
        args.push(format!("-O{level}"));
    }
    args
}

fn corpus_items(options: &Options) -> Result<Vec<Item>, String> {
    let root = corpus::corpus_root();
    let root_str = root.to_string_lossy().to_string();
    Ok(corpus::load_manifest(corpus::Seam::default())?
        .into_iter()
        .filter(|r| r.exit == "0")
        .map(|record| {
            let path = root.join(&record.rel);
            let mut args = vec![path.to_string_lossy().to_string()];
            args.extend(record.flags.replace("/corpus", &root_str).split(' ').map(String::from));
            Item { id: record.rel.clone(), group: format!("corpus {}", record.tier), cwd: path.parent().unwrap().to_path_buf(), args: with_level(args, options.level), expected: None }
        })
        .collect())
}

fn functional_items(options: &Options) -> Vec<Item> {
    let root = golden::functional_root();
    golden::cases()
        .into_iter()
        .filter(|case| case.tool == Tool::Zxbc && case.kind == Kind::Asm && case.id.starts_with("arch/"))
        .map(|case| {
            let mut args = case.pre.clone();
            // The format is given again by the comparison.
            args.retain(|a| !a.starts_with("--output-format"));
            args.push(case.rel.file_name().unwrap().to_string_lossy().to_string());
            args.extend(case.post.iter().cloned());
            let expected = PathBuf::from(format!("{}.asm", case.expected_stem(&golden::expected_root()).display()));
            Item {
                id: case.id.clone(),
                group: format!("functional {}", case.rel.parent().unwrap().display()),
                cwd: root.join(case.rel.parent().unwrap()),
                args: with_level(args, options.level),
                // An expectation holds for the level it was recorded at only.
                expected: options.level.is_none().then_some(expected),
            }
        })
        .collect()
}

fn oracle_stage(root: &Path) -> Command {
    let mut cmd = Command::new(std::env::var("ZXR_PYTHON").unwrap_or_else(|_| "/usr/bin/python3".into()));
    cmd.arg(repo_root().join("tools/oracle_stage.py")).arg(root).arg("backend-in");
    cmd.env("PYTHONDONTWRITEBYTECODE", "1").env("COLUMNS", "80").env_remove("PYTHONHASHSEED");
    cmd
}

/// `name=hash` of every file in `dir` whose name starts with `prefix`.
fn hashes(dir: &Path, prefix: &str) -> String {
    let list: Vec<String> = util::walk(dir)
        .iter()
        .filter_map(|file| {
            let key = file.to_string_lossy().strip_prefix(prefix)?.to_string();
            Some(format!("{key}={}", &sha256(&fs::read(dir.join(file)).ok()?)[..16]))
        })
        .collect();
    list.join(",")
}

/// The error lines of a toolchain's diagnostics. Warnings are left out: in
/// this pipeline the front end's come from the oracle's run alone.
fn errors(stderr: &[u8]) -> String {
    String::from_utf8_lossy(stderr).lines().filter(|l| l.contains(": error: ")).map(|l| format!("{l}\n")).collect()
}

/// The oracle's compile of one program to assembly, with the record of what
/// its backend consumed beside it (`out.asm.backend-in`) and, at -O3 and
/// above, what went into its flow optimiser and came out (`out.asm.opt-in`,
/// `out.asm.opt-out`). Made once and kept under `target/oracle-cache/backend`.
/// Returns the directory and the recorded status: `0` when all of it is
/// there, otherwise the exit code. `None` if the run was killed.
pub(crate) fn oracle_record(id: &str, level_tag: &str, cwd: &Path, args: &[String], index: usize) -> std::io::Result<Option<(PathBuf, String)>> {
    let oracle = Toolchain::oracle();
    let Toolchain::Oracle { root: oracle_root, .. } = &oracle else { unreachable!() };
    let cache = repo_root().join("target/oracle-cache/backend").join(level_tag).join(id.replace(" :: ", "@").replace(' ', "_"));
    let status = cache.join("status");
    if !status.exists() {
        let _ = fs::remove_dir_all(&cache);
        fs::create_dir_all(&cache)?;
        let scratch = Scratch::new("backend-oracle", index)?;
        let mut cmd = oracle_stage(oracle_root);
        cmd.current_dir(cwd).args(args).args(["-f", "asm", "-o"]).arg(cache.join("out.asm"));
        let run = util::run(cmd, &scratch.0, TIMEOUT)?;
        let ok = run.code == 0 && cache.join("out.asm").exists() && cache.join("out.asm.backend-in").exists();
        if run.timed_out || run.code < 0 {
            // Killed: nothing to record.
            return Ok(None);
        }
        let _ = fs::write(&status, if ok { "0".to_string() } else { format!("{}", if run.code == 0 { -2 } else { run.code }) });
    }
    let status = fs::read_to_string(&status).unwrap_or_default();
    Ok(Some((cache, status)))
}

enum Verdict {
    /// Byte for byte.
    Same,
    /// The same once comments, `#line` markers and blank lines are dropped;
    /// with the first difference before that.
    SameFiltered(String),
    /// The oracle does not compile it with these arguments.
    Skipped,
    Differs(String),
}

fn first_difference(expected: &str, actual: &str) -> String {
    let (mut a, mut b) = (expected.lines(), actual.lines());
    let mut n = 1;
    loop {
        match (a.next(), b.next()) {
            (Some(x), Some(y)) if x == y => n += 1,
            (None, None) => return "no difference in lines".to_string(),
            (x, y) => return format!("line {n}: oracle `{}` port `{}`", x.unwrap_or("<end>").trim(), y.unwrap_or("<end>").trim()),
        }
    }
}

fn run_item(item: &Item, index: usize, options: &Options, level_tag: &str) -> Verdict {
    let oracle = Toolchain::oracle();
    let port = Toolchain::port();
    let fail = |e: std::io::Error| Verdict::Differs(e.to_string());

    // The oracle's assembly and its backend-in record.
    let cache = match oracle_record(&item.id, level_tag, &item.cwd, &item.args, index) {
        Ok(Some((cache, status))) if status == "0" => cache,
        Ok(_) => return Verdict::Skipped,
        Err(e) => return fail(e),
    };

    let scratch = match Scratch::new("backend", index) {
        Ok(s) => s,
        Err(e) => return fail(e),
    };
    let mut cmd = port.command(Tool::Zxbc);
    cmd.current_dir(&item.cwd).args(&item.args).arg("--from-ir").arg(cache.join("out.asm.backend-in"));
    if options.flow_stand_in && cache.join("out.asm.opt-out").exists() {
        cmd.arg("--flow-stand-in").arg(cache.join("out.asm"));
    }

    if options.binary {
        // A full oracle build, once.
        let recorded = cache.join("binary");
        if !recorded.exists() {
            let scratch = match Scratch::new("backend-oracle-bin", index) {
                Ok(s) => s,
                Err(e) => return fail(e),
            };
            let mut cmd = oracle.command(Tool::Zxbc);
            cmd.current_dir(&item.cwd).args(&item.args).args(["-f", "bin", "-M"]).arg(scratch.0.join("out.map")).arg("-o").arg(scratch.0.join("out.bin"));
            let run = match util::run(cmd, &scratch.0, TIMEOUT) {
                Ok(run) => run,
                Err(e) => return fail(e),
            };
            // A build the oracle fails is a result too: its exit code and
            // the errors it reports.
            let text = if run.code == 0 { hashes(&scratch.0, "out.") } else { format!("exit {}\n{}", run.code, errors(&oracle.normalise(&run.stderr))) };
            if !run.timed_out && run.code >= 0 {
                let _ = fs::write(&recorded, text);
            }
        }
        let want = fs::read_to_string(&recorded).unwrap_or_default();
        cmd.args(["-f", "bin", "-M"]).arg(scratch.0.join("out.map")).arg("-o").arg(scratch.0.join("out.bin"));
        let run = match util::run(cmd, &scratch.0, TIMEOUT) {
            Ok(run) => run,
            Err(e) => return fail(e),
        };
        if want.is_empty() {
            return Verdict::Skipped;
        }
        if want.starts_with("exit ") {
            let got = format!("exit {}\n{}", run.code, errors(&oracle.normalise(&port.normalise(&run.stderr))));
            if got == want {
                return Verdict::Same;
            }
            let line = |text: &str| text.lines().nth(1).unwrap_or("").to_string();
            return Verdict::Differs(format!("oracle fails with `{} {}`, port with `{} {}`", want.lines().next().unwrap_or(""), line(&want), got.lines().next().unwrap_or(""), line(&got)));
        }
        if run.code != 0 {
            let text = String::from_utf8_lossy(&run.stderr).to_string();
            return Verdict::Differs(format!("exit {}: {}", run.code, text.lines().last().unwrap_or("")));
        }
        let got = hashes(&scratch.0, "out.");
        if got == want {
            return Verdict::Same;
        }
        let names = |list: &str| list.split(',').map(String::from).collect::<std::collections::BTreeSet<_>>();
        let differing: Vec<String> = names(&want).symmetric_difference(&names(&got)).map(|s| s.split('=').next().unwrap().to_string()).collect::<std::collections::BTreeSet<_>>().into_iter().collect();
        return Verdict::Differs(format!("differs in {}", differing.join(", ")));
    }

    cmd.args(["-f", "asm", "-o"]).arg(scratch.0.join("out.asm"));
    let run = match util::run(cmd, &scratch.0, TIMEOUT) {
        Ok(run) => run,
        Err(e) => return fail(e),
    };
    if run.code != 0 {
        let text = String::from_utf8_lossy(&run.stderr).to_string();
        return Verdict::Differs(format!("exit {}: {}", run.code, text.lines().last().unwrap_or("")));
    }
    // Paths into each toolchain's own library appear in `#line` markers;
    // those inside inline assembly came with the record, from the oracle.
    let want = oracle.normalise(&fs::read(cache.join("out.asm")).unwrap_or_default());
    let got = oracle.normalise(&port.normalise(&fs::read(scratch.0.join("out.asm")).unwrap_or_default()));
    let (want_f, got_f) = (golden::filter_asm(&want), golden::filter_asm(&got));
    if let Some(expected) = &item.expected {
        // The recorded expectation went through the same clean-up.
        match fs::read(expected) {
            Ok(recorded) if recorded == got_f => {}
            Ok(recorded) => return Verdict::Differs(format!("expectation: {}", first_difference(&String::from_utf8_lossy(&recorded), &String::from_utf8_lossy(&got_f)))),
            Err(_) => return Verdict::Differs("no recorded expectation".to_string()),
        }
    }
    if want == got {
        Verdict::Same
    } else if want_f == got_f {
        Verdict::SameFiltered(first_difference(&String::from_utf8_lossy(&want), &String::from_utf8_lossy(&got)))
    } else {
        Verdict::Differs(first_difference(&String::from_utf8_lossy(&want_f), &String::from_utf8_lossy(&got_f)))
    }
}

pub fn check(options: &Options) -> Result<bool, String> {
    let mut items = Vec::new();
    if options.suite == "corpus" || options.suite == "all" {
        items.extend(corpus_items(options)?);
    }
    if options.suite == "functional" || options.suite == "all" {
        items.extend(functional_items(options));
    }
    if let Some(filter) = &options.filter {
        items.retain(|item| item.id.contains(filter.as_str()));
    }
    let level_tag = options.level.map_or("own".to_string(), |l| format!("O{l}"));
    let verdicts = par_map(&items, |i, item| run_item(item, i, options, &level_tag));

    // same, same after the clean-up, differ, skipped
    let mut table: BTreeMap<String, [usize; 4]> = BTreeMap::new();
    let mut failures = Vec::new();
    for (item, verdict) in items.iter().zip(&verdicts) {
        let row = table.entry(item.group.clone()).or_default();
        match verdict {
            Verdict::Same => row[0] += 1,
            Verdict::SameFiltered(why) => {
                row[1] += 1;
                if options.verbose {
                    println!("FILTERED {}: {why}", item.id);
                }
            }
            Verdict::Differs(why) => {
                row[2] += 1;
                failures.push(format!("{}: {why}", item.id));
            }
            Verdict::Skipped => row[3] += 1,
        }
    }
    let what = if options.binary { "binaries" } else { "assembly" };
    println!("backend seam, {what}, level {}", options.level.map_or("as each program asks".to_string(), |l| format!("-O{l}")));
    println!("{:<28} {:>7} {:>9} {:>7} {:>7} {:>9}", "group", "same", "filtered", "differ", "total", "(skipped)");
    let mut sum = [0usize; 4];
    for (group, row) in &table {
        println!("{group:<28} {:>7} {:>9} {:>7} {:>7} {:>9}", row[0], row[1], row[2], row[0] + row[1] + row[2], row[3]);
        for (total, n) in sum.iter_mut().zip(row) {
            *total += n;
        }
    }
    println!("{:<28} {:>7} {:>9} {:>7} {:>7} {:>9}", "all", sum[0], sum[1], sum[2], sum[0] + sum[1] + sum[2], sum[3]);
    let shown = if options.verbose { failures.len() } else { failures.len().min(25) };
    for failure in &failures[..shown] {
        println!("DIFF {failure}");
    }
    if failures.len() > shown {
        println!("... and {} more (use --verbose)", failures.len() - shown);
    }
    Ok(failures.is_empty())
}

/// The backend's tables and the peephole rule files are copies of the
/// oracle's; `tools/oracle_dump.py --backend` writes them and checks them.
pub fn data(check: bool) -> Result<bool, String> {
    let mut cmd = Command::new(std::env::var("ZXR_PYTHON").unwrap_or_else(|_| "/usr/bin/python3".into()));
    cmd.arg(repo_root().join("tools/oracle_dump.py")).arg(repo_root().join("reference/zxbasic")).arg("--backend");
    if check {
        cmd.arg("--check");
    }
    cmd.env("PYTHONDONTWRITEBYTECODE", "1");
    let status = cmd.status().map_err(|e| e.to_string())?;
    Ok(status.success())
}
