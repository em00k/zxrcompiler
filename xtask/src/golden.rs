//! The functional suite inherited from the Python compiler, driven as a
//! black box: one process per case, expectations recorded from the oracle.
//!
//! A case is an input file plus an argument list. What is compared is the
//! output file (or its absence), stdout, stderr and the exit code.

use crate::toolchain::{Tool, Toolchain};
use crate::util::{self, par_map, repo_root, write_file, Scratch};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Asm,
    Ir,
    Binary,
    Preprocessed,
}

#[derive(Clone, Debug)]
pub struct Case {
    /// `arch/zx48k/00.bas`, or `arch/zx48k/61.bas :: --sinclair` for a variant.
    pub id: String,
    pub tool: Tool,
    /// Path of the input, relative to the functional root.
    pub rel: PathBuf,
    /// Arguments before the input file.
    pub pre: Vec<String>,
    /// Arguments after `-o <output>`.
    pub post: Vec<String>,
    pub out_ext: String,
    pub kind: Kind,
}

impl Case {
    fn category(&self) -> String {
        let dir = self.rel.parent().unwrap().to_string_lossy().to_string();
        if self.id.contains(" :: ") { format!("{dir} (variants)") } else { dir }
    }

    /// Where the expectation files for this case live, minus extension.
    pub(crate) fn expected_stem(&self, expected_root: &Path) -> PathBuf {
        let mut name = self.rel.to_string_lossy().to_string();
        if let Some((_, extras)) = self.id.split_once(" :: ") {
            name.push('@');
            name.extend(extras.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }));
        }
        expected_root.join(name)
    }
}

pub fn functional_root() -> PathBuf {
    repo_root().join("tests/golden/functional")
}

pub(crate) fn expected_root() -> PathBuf {
    repo_root().join("tests/golden/expected")
}

fn stem(path: &Path) -> String {
    // The Python driver splits on the first dot, not the last.
    path.file_name().unwrap().to_string_lossy().split('.').next().unwrap().to_string()
}

fn opt_prefix(name: &str) -> Option<&str> {
    let rest = name.strip_prefix("opt")?;
    let digits = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    (digits > 0 && rest[digits..].starts_with('_')).then(|| &rest[..digits])
}

fn base_case(rel: &Path) -> Option<Case> {
    let name = stem(rel);
    let ext = rel.extension()?.to_str()?;
    let (tool, pre, post, out_ext, kind) = match ext {
        "bas" => {
            let arch = rel.parent()?.file_name()?.to_string_lossy().to_string();
            let mut pre = vec!["-O1".to_string(), "--arch".into(), arch];
            if let Some(level) = opt_prefix(&name) {
                pre.push(format!("-O{level}"));
            }
            let binary = ["tzx", "tap", "sna", "z80"].iter().find(|p| name.starts_with(&format!("{p}_")));
            if let Some(fmt) = binary {
                pre.push(format!("--output-format={fmt}"));
                (Tool::Zxbc, pre, vec!["-a".to_string(), "-B".into()], fmt.to_string(), Kind::Binary)
            } else if name.ends_with("_IC") {
                pre.push("--output-format=ir".into());
                (Tool::Zxbc, pre, vec![], "ic".to_string(), Kind::Ir)
            } else {
                pre.push("--output-format=asm".into());
                (Tool::Zxbc, pre, vec![], "asm".to_string(), Kind::Asm)
            }
        }
        "asm" => {
            let post = if name.starts_with("zxnext_") { vec!["--zxnext".to_string()] } else { vec![] };
            (Tool::Zxbasm, vec![], post, "bin".to_string(), Kind::Binary)
        }
        "bi" => (Tool::Zxbpp, vec![], vec![], "out".to_string(), Kind::Preprocessed),
        _ => return None,
    };
    Some(Case { id: rel.to_string_lossy().to_string(), tool, rel: rel.to_path_buf(), pre, post, out_ext, kind })
}

/// The upstream doctest files call `process_file('path'[, [params]])`; each
/// distinct parameter set becomes a variant case.
fn doctest_variants(root: &Path) -> Vec<(PathBuf, Vec<String>)> {
    let mut out = Vec::new();
    for file in util::walk(&root.join("cmdline")) {
        if file.extension().and_then(|e| e.to_str()) != Some("txt") {
            continue;
        }
        let text = fs::read_to_string(root.join("cmdline").join(&file)).unwrap_or_default();
        for line in text.lines() {
            let Some(call) = line.trim().strip_prefix(">>> process_file(") else { continue };
            let quoted: Vec<&str> = call.split('\'').skip(1).step_by(2).collect();
            let Some((path, params)) = quoted.split_first() else { continue };
            let explicit = call.contains('[');
            let mut extras: Vec<String> = Vec::new();
            if explicit {
                for p in params {
                    if let Some(rest) = p.strip_prefix("-O ") {
                        extras.extend(rest.split_whitespace().map(String::from));
                    }
                }
            } else if path.to_lowercase().ends_with(".bas") {
                extras.push("--hide-warning-codes".into());
            }
            out.push((PathBuf::from(path), extras));
        }
    }
    out
}

pub fn cases() -> Vec<Case> {
    let root = functional_root();
    let mut cases: BTreeMap<String, Case> = BTreeMap::new();
    for rel in util::walk(&root) {
        // An .asm beside a .bas of the same name is an include or a leftover
        // expectation, never an assembler test.
        if rel.extension().and_then(|e| e.to_str()) == Some("asm") && root.join(&rel).with_extension("bas").exists() {
            continue;
        }
        if let Some(case) = base_case(&rel) {
            cases.insert(case.id.clone(), case);
        }
    }
    for (rel, extras) in doctest_variants(&root) {
        if extras.is_empty() || !root.join(&rel).exists() {
            continue;
        }
        if let Some(mut case) = base_case(&rel) {
            case.id = format!("{} :: {}", case.id, extras.join(" "));
            case.post.extend(extras);
            cases.insert(case.id.clone(), case);
        }
    }
    cases.into_values().collect()
}

/// Comment lines and `#line` markers carry paths and are dropped, as the
/// Python driver does; so are blank lines and trailing blanks.
pub(crate) fn filter_asm(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    for line in data.split(|&b| b == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        let lead = line.iter().take_while(|&&b| b == b' ' || b == b'\t').count();
        let is_line_marker = |rest: &[u8]| {
            rest.strip_prefix(b"#").is_some_and(|r| {
                let skip = r.iter().take_while(|&&b| b == b' ' || b == b'\t').count();
                r[skip..].starts_with(b"line")
            })
        };
        if line[lead..].starts_with(b";") || is_line_marker(line) {
            continue;
        }
        if let Some(rest) = line.strip_prefix(b"('inline',") {
            let skip = rest.iter().take_while(|&&b| b == b' ' || b == b'\t').count();
            if skip > 0 && rest[skip..].strip_prefix(b"'").is_some_and(is_line_marker) {
                continue;
            }
        }
        let end = line.iter().rposition(|&b| b != b' ' && b != b'\t').map_or(0, |p| p + 1);
        if end == 0 {
            continue;
        }
        out.extend_from_slice(&line[..end]);
        out.push(b'\n');
    }
    out
}

#[derive(PartialEq, Eq, Clone)]
pub struct Outcome {
    pub code: i32,
    pub output: Option<Vec<u8>>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

pub fn run_case(toolchain: &Toolchain, case: &Case, index: usize) -> Outcome {
    let root = functional_root();
    let scratch = Scratch::new("golden", index).expect("scratch dir");
    let input = case.rel.file_name().unwrap();
    let out_path = scratch.0.join(format!("{}.{}", stem(&case.rel), case.out_ext));
    let mut cmd = toolchain.command(case.tool);
    cmd.current_dir(root.join(case.rel.parent().unwrap()));
    cmd.args(&case.pre).arg(input).arg("-o").arg(&out_path).args(&case.post);
    let result = util::run(cmd, &scratch.0, TIMEOUT).expect("failed to start the toolchain");
    let scratch_str = scratch.0.to_string_lossy().to_string();
    let clean = |data: &[u8]| util::replace_bytes(&toolchain.normalise(data), scratch_str.as_bytes(), b"/out");
    let output = fs::read(&out_path).ok().map(|data| match case.kind {
        Kind::Asm | Kind::Ir => filter_asm(&data),
        Kind::Preprocessed => clean(&data),
        Kind::Binary => data,
    });
    let mut stderr = clean(&result.stderr);
    if result.timed_out {
        stderr.extend_from_slice(b"*** timed out ***\n");
    }
    Outcome { code: result.code, output, stdout: clean(&result.stdout), stderr }
}

fn load_expected(case: &Case, index: &BTreeMap<String, i32>) -> Option<Outcome> {
    let stem = case.expected_stem(&expected_root());
    let with = |ext: &str| PathBuf::from(format!("{}.{ext}", stem.display()));
    Some(Outcome {
        code: *index.get(&case.id)?,
        output: fs::read(with(&case.out_ext)).ok(),
        stdout: fs::read(with("stdout")).unwrap_or_default(),
        stderr: fs::read(with("stderr")).unwrap_or_default(),
    })
}

fn load_index() -> BTreeMap<String, i32> {
    let text = fs::read_to_string(expected_root().join("INDEX.tsv")).unwrap_or_default();
    text.lines().filter_map(|l| l.split_once('\t')).filter_map(|(id, code)| Some((id.to_string(), code.parse().ok()?))).collect()
}

/// Copies the functional suite's inputs out of the oracle tree. Upstream's own
/// expectation files are left behind: ours are recorded from the pinned oracle.
pub fn import() -> Result<(), String> {
    let src = repo_root().join("reference/zxbasic/tests/functional");
    let dst = functional_root();
    let _ = fs::remove_dir_all(&dst);
    let mut copied = 0;
    for rel in util::walk(&src) {
        let ext = rel.extension().and_then(|e| e.to_str()).unwrap_or("");
        let top = rel.components().next().unwrap().as_os_str().to_string_lossy().to_string();
        let sibling = |e: &str| src.join(&rel).with_extension(e).exists();
        let skip = rel.components().any(|c| c.as_os_str() == "__pycache__")
            || matches!(ext, "py" | "pyc")
            || rel.file_name().is_some_and(|n| n == "Makefile")
            || (matches!(ext, "asm" | "ic" | "tap" | "tzx" | "sna" | "z80") && sibling("bas"))
            || (top == "asm" && ext == "bin" && sibling("asm"))
            || (top == "zxbpp" && ext == "out" && sibling("bi"));
        if skip {
            continue;
        }
        write_file(&dst.join(&rel), &fs::read(src.join(&rel)).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        copied += 1;
    }
    println!("imported {copied} files into {}", dst.strip_prefix(repo_root()).unwrap().display());
    Ok(())
}

pub fn bless(toolchain: &Toolchain) -> Result<(), String> {
    let cases = cases();
    let outcomes = par_map(&cases, |i, case| run_case(toolchain, case, i));
    let root = expected_root();
    let _ = fs::remove_dir_all(&root);
    let mut index = String::new();
    for (case, outcome) in cases.iter().zip(&outcomes) {
        let stem = case.expected_stem(&root);
        let with = |ext: &str| PathBuf::from(format!("{}.{ext}", stem.display()));
        index.push_str(&format!("{}\t{}\n", case.id, outcome.code));
        let put = |ext: &str, data: &[u8]| write_file(&with(ext), data).map_err(|e| e.to_string());
        if let Some(output) = &outcome.output {
            put(&case.out_ext, output)?;
        }
        if !outcome.stdout.is_empty() {
            put("stdout", &outcome.stdout)?;
        }
        if !outcome.stderr.is_empty() {
            put("stderr", &outcome.stderr)?;
        }
    }
    write_file(&root.join("INDEX.tsv"), index.as_bytes()).map_err(|e| e.to_string())?;
    let with_output = outcomes.iter().filter(|o| o.output.is_some()).count();
    println!("recorded {} cases from the {} ({with_output} produce output, {} do not)", cases.len(), toolchain.label(), cases.len() - with_output);
    Ok(())
}

pub struct CheckOptions {
    pub filter: Option<String>,
    pub verbose: bool,
    /// Compare only the output file, not diagnostics or the exit code.
    pub output_only: bool,
    /// Run only the cases the oracle fails, and compare only diagnostics and
    /// the exit code, not the output file.
    pub diagnostics_only: bool,
}

fn first_difference(expected: &[u8], actual: &[u8]) -> String {
    let (e, a): (Vec<&[u8]>, Vec<&[u8]>) = (expected.split(|&b| b == b'\n').collect(), actual.split(|&b| b == b'\n').collect());
    let line = e.iter().zip(&a).position(|(x, y)| x != y).unwrap_or(e.len().min(a.len()));
    let show = |v: &Vec<&[u8]>| v.get(line).map_or("<end>".to_string(), |l| String::from_utf8_lossy(l).to_string());
    format!("line {}: expected `{}`, got `{}`", line + 1, show(&e), show(&a))
}

pub fn check(toolchain: &Toolchain, options: &CheckOptions) -> Result<bool, String> {
    let index = load_index();
    if index.is_empty() {
        return Err("no expectations recorded; run `cargo xtask golden bless` first".into());
    }
    let cases: Vec<Case> = cases().into_iter().filter(|c| options.filter.as_ref().is_none_or(|f| c.id.contains(f.as_str()))).collect();
    let cases: Vec<Case> = cases.into_iter().filter(|c| !options.diagnostics_only || load_expected(c, &index).is_some_and(|e| e.code != 0)).collect();
    let outcomes = par_map(&cases, |i, case| run_case(toolchain, case, i));
    let mut per_category: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut failures = Vec::new();
    for (case, actual) in cases.iter().zip(&outcomes) {
        let entry = per_category.entry(case.category()).or_default();
        entry.1 += 1;
        let Some(expected) = load_expected(case, &index) else {
            failures.push(format!("{}: no recorded expectation", case.id));
            continue;
        };
        let mut why = Vec::new();
        match (&expected.output, &actual.output) {
            _ if options.diagnostics_only => {}
            (Some(e), Some(a)) if e != a => why.push(format!("output differs, {}", first_difference(e, a))),
            (Some(_), None) => why.push("no output file produced".to_string()),
            (None, Some(_)) => why.push("output file produced, none expected".to_string()),
            _ => {}
        }
        if !options.output_only {
            if expected.code != actual.code {
                why.push(format!("exit code {} (expected {})", actual.code, expected.code));
            }
            if expected.stderr != actual.stderr {
                why.push(format!("stderr differs, {}", first_difference(&expected.stderr, &actual.stderr)));
            }
            if expected.stdout != actual.stdout {
                why.push(format!("stdout differs, {}", first_difference(&expected.stdout, &actual.stdout)));
            }
        }
        if why.is_empty() {
            entry.0 += 1;
        } else {
            failures.push(format!("{}: {}", case.id, why.join("; ")));
        }
    }
    println!("{:<28} {:>6} {:>6}", "category", "pass", "total");
    let (mut pass, mut total) = (0, 0);
    for (category, (p, t)) in &per_category {
        println!("{category:<28} {p:>6} {t:>6}");
        pass += p;
        total += t;
    }
    println!("{:<28} {pass:>6} {total:>6}", format!("TOTAL ({})", toolchain.label()));
    let shown = if options.verbose { failures.len() } else { failures.len().min(20) };
    for failure in &failures[..shown] {
        println!("FAIL {failure}");
    }
    if failures.len() > shown {
        println!("... and {} more (use --verbose)", failures.len() - shown);
    }
    Ok(failures.is_empty())
}

/// Lists where the expectations recorded from the oracle differ from the
/// expectation files upstream ships. Every entry should be explainable by a
/// deliberate change in the fork.
pub fn upstream_diff() -> Result<(), String> {
    let upstream = repo_root().join("reference/zxbasic/tests/functional");
    let index = load_index();
    let mut differing = Vec::new();
    let mut compared = 0;
    for case in cases().iter().filter(|c| !c.id.contains(" :: ")) {
        let Some(ours) = load_expected(case, &index) else { continue };
        let path = upstream.join(&case.rel).with_file_name(format!("{}.{}", stem(&case.rel), case.out_ext));
        let theirs = fs::read(&path).ok().map(|data| match case.kind {
            Kind::Asm | Kind::Ir => filter_asm(&data),
            _ => data,
        });
        compared += 1;
        match (&ours.output, &theirs) {
            (Some(a), Some(b)) if a == b => {}
            (None, None) => {}
            (Some(_), Some(_)) => differing.push(format!("{}: content differs", case.id)),
            (Some(_), None) => differing.push(format!("{}: oracle produces output, upstream expects none", case.id)),
            (None, Some(_)) => differing.push(format!("{}: oracle produces nothing, upstream expects output", case.id)),
        }
    }
    println!("{compared} cases compared with upstream's expectation files, {} differ", differing.len());
    for line in differing {
        println!("  {line}");
    }
    Ok(())
}

/// The seams of the front end over the suite's BASIC cases: each case is run
/// up to an intermediate stage (`zxbc --stage`, `tools/oracle_stage.py`) and
/// what the stage wrote is recorded as a hash, with the diagnostics and the
/// exit code, in `tests/golden/seam-<stage>[-O<N>].tsv`.
fn seam_path(seam: crate::corpus::Seam) -> PathBuf {
    let stage = seam.stage.unwrap_or("full");
    repo_root().join("tests/golden").join(match seam.opt {
        Some(level) => format!("seam-{stage}-O{level}.tsv"),
        None => format!("seam-{stage}.tsv"),
    })
}

fn seam_cases() -> Vec<Case> {
    cases().into_iter().filter(|c| c.tool == Tool::Zxbc && c.rel.starts_with("arch")).collect()
}

/// Exit code, hash of the stage's output (`-` if none) and hash of the
/// diagnostics (`-` if none).
fn run_seam_case(toolchain: &Toolchain, case: &Case, index: usize, seam: crate::corpus::Seam) -> (String, String, String) {
    let stage = seam.stage.unwrap();
    let root = functional_root();
    let scratch = Scratch::new("golden-seam", index).expect("scratch dir");
    let out_path = scratch.0.join(format!("{}.{stage}", stem(&case.rel)));
    let mut cmd = toolchain.stage_command(stage);
    cmd.current_dir(root.join(case.rel.parent().unwrap()));
    // The flags the suite derives from the file name, less the output format
    // and what only a tape needs.
    cmd.args(case.pre.iter().filter(|a| !a.starts_with("--output-format=")));
    cmd.arg(case.rel.file_name().unwrap()).arg("-o").arg(&out_path);
    let skip = if case.kind == Kind::Binary { 2 } else { 0 };
    cmd.args(&case.post[skip.min(case.post.len())..]);
    if let Some(level) = seam.opt {
        cmd.arg(format!("-O{level}"));
    }
    let result = util::run(cmd, &scratch.0, TIMEOUT).expect("failed to start the toolchain");
    let scratch_str = scratch.0.to_string_lossy().to_string();
    let clean = |data: &[u8]| util::replace_bytes(&toolchain.normalise(data), scratch_str.as_bytes(), b"/out");
    let output = fs::read(&out_path).ok().map(|data| clean(&data));
    let mut diagnostics = clean(&result.stderr);
    diagnostics.extend_from_slice(&clean(&result.stdout));
    if let Some(dir) = seam.keep {
        let files: Vec<(String, Vec<u8>)> = output.iter().map(|data| (stage.to_string(), data.clone())).collect();
        crate::corpus::keep_outputs(dir, toolchain, &case.id, &files, &diagnostics);
    }
    let hash = |data: &[u8]| util::sha256(data)[..16].to_string();
    let code = if result.timed_out { "timeout".to_string() } else { result.code.to_string() };
    (code, output.as_deref().map_or("-".to_string(), hash), if diagnostics.is_empty() { "-".to_string() } else { hash(&diagnostics) })
}

pub fn seam_manifest(seam: crate::corpus::Seam) -> Result<(), String> {
    let oracle = Toolchain::oracle();
    let cases = seam_cases();
    let results = par_map(&cases, |i, case| run_seam_case(&oracle, case, i, seam));
    let mut text = String::from("# What the oracle writes at this stage for each BASIC case of the functional suite.\n# case\texit\toutput\tdiagnostics\n");
    let mut tally: BTreeMap<String, usize> = BTreeMap::new();
    for (case, (code, output, diagnostics)) in cases.iter().zip(&results) {
        text.push_str(&format!("{}\t{code}\t{output}\t{diagnostics}\n", case.id));
        *tally.entry(code.clone()).or_default() += 1;
    }
    write_file(&seam_path(seam), text.as_bytes()).map_err(|e| e.to_string())?;
    let tally: Vec<String> = tally.iter().map(|(code, n)| format!("{n} exit {code}")).collect();
    println!("recorded {} cases: {}", cases.len(), tally.join(", "));
    Ok(())
}

pub fn seam_check(toolchain: &Toolchain, seam: crate::corpus::Seam, filter: Option<String>, verbose: bool) -> Result<bool, String> {
    let text = fs::read_to_string(seam_path(seam)).map_err(|_| "nothing recorded; run `cargo xtask golden manifest --stage ...` first".to_string())?;
    let expected: BTreeMap<&str, Vec<&str>> =
        text.lines().filter(|l| !l.starts_with('#')).map(|l| l.split('\t').collect::<Vec<_>>()).filter(|f| f.len() == 4).map(|f| (f[0], f[1..].to_vec())).collect();
    let cases: Vec<Case> = seam_cases().into_iter().filter(|c| filter.as_ref().is_none_or(|f| c.id.contains(f.as_str()))).collect();
    let results = par_map(&cases, |i, case| run_seam_case(toolchain, case, i, seam));
    let mut per_category: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut failures = Vec::new();
    for (case, (code, output, diagnostics)) in cases.iter().zip(&results) {
        let entry = per_category.entry(case.category()).or_default();
        entry.1 += 1;
        let Some(want) = expected.get(case.id.as_str()) else {
            failures.push(format!("{}: nothing recorded", case.id));
            continue;
        };
        if want[0] != code {
            failures.push(format!("{}: exit {code} (expected {})", case.id, want[0]));
        } else if want[1] != output {
            failures.push(format!("{}: output differs", case.id));
        } else if want[2] != diagnostics {
            failures.push(format!("{}: diagnostics differ", case.id));
        } else {
            entry.0 += 1;
        }
    }
    println!("{:<28} {:>6} {:>6}", "category", "same", "total");
    let (mut pass, mut total) = (0, 0);
    for (category, (p, t)) in &per_category {
        println!("{category:<28} {p:>6} {t:>6}");
        pass += p;
        total += t;
    }
    println!("{:<28} {pass:>6} {total:>6}", format!("TOTAL ({})", toolchain.label()));
    let shown = if verbose { failures.len() } else { failures.len().min(20) };
    for failure in &failures[..shown] {
        println!("DIFF {failure}");
    }
    if failures.len() > shown {
        println!("... and {} more (use --verbose)", failures.len() - shown);
    }
    Ok(failures.is_empty())
}
