//! The flow optimiser alone, against what the oracle's made of the same
//! lines: for every `X.opt-in` under the directories given (with
//! `X.backend-in` beside it for the architecture, the level and the code
//! banks' trampolines), the port's output is compared with `X.opt-out`.
//!
//!     cargo run --release -p zxr-z80 --example flow_check -- [-v] [-j N] DIR...
//!
//! `cargo xtask backend check --level 3|4` records the files under
//! `target/oracle-cache/backend`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, out);
        } else if path.to_string_lossy().ends_with(".opt-in") {
            out.push(path);
        }
    }
}

fn unescape(field: &str) -> String {
    let mut out = String::new();
    let mut chars = field.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

/// Ok(port's text) or Err(the panic's message).
fn run(prefix: &str, input: &str) -> Result<String, String> {
    let record = std::fs::read_to_string(format!("{prefix}.backend-in")).map_err(|e| format!("{prefix}.backend-in: {e}"))?;
    let mut zxnext = false;
    let mut level = 0;
    let mut trampolines: Vec<(String, String)> = Vec::new();
    for line in record.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        match fields.as_slice() {
            ["option", "architecture", arch] => zxnext = *arch == "zxnext",
            ["option", "optimization_level", n] => level = n.parse().unwrap_or(0),
            ["far_trampoline", label, body] => trampolines.push((unescape(label), unescape(body))),
            ["pass", ..] => break,
            _ => {}
        }
    }
    let lines: Vec<String> = input.split('\n').map(String::from).collect();
    std::panic::catch_unwind(|| {
        let is_asm = |line: &str| line.strip_prefix("##ASM").is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
        let far_body = |label: &str| trampolines.iter().find(|(l, _)| l == label).map(|(_, body)| body.clone());
        let env = zxr_opt::flow::Env { is_asm: &is_asm, far_body: &far_body, zxnext, level };
        zxr_z80::finish::flow_optimize_with(env, lines)
    })
    .map_err(|e| e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default())
}

fn main() {
    let mut verbose = false;
    let mut jobs = std::thread::available_parallelism().map_or(4, |n| n.get());
    let mut files = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-v" => verbose = true,
            "-j" => jobs = args.next().and_then(|n| n.parse().ok()).unwrap_or(jobs),
            dir => walk(Path::new(dir), &mut files),
        }
    }
    files.sort();
    std::panic::set_hook(Box::new(|_| {}));

    let next = AtomicUsize::new(0);
    let results: Mutex<Vec<(String, &'static str, Duration, String)>> = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..jobs {
            scope.spawn(|| loop {
                let at = next.fetch_add(1, Ordering::Relaxed);
                let Some(path) = files.get(at) else { break };
                let path = path.to_string_lossy().to_string();
                let prefix = path.strip_suffix(".opt-in").unwrap();
                let input = std::fs::read_to_string(&path).unwrap();
                let expected = std::fs::read_to_string(format!("{prefix}.opt-out")).unwrap_or_default();
                let started = Instant::now();
                let (verdict, note) = match run(prefix, &input) {
                    Ok(text) if text == expected => ("same", String::new()),
                    Ok(text) => {
                        let _ = std::fs::write(format!("{prefix}.opt-out.port"), &text);
                        let at = text.split('\n').zip(expected.split('\n')).position(|(a, b)| a != b).unwrap_or(0);
                        ("DIFFERENT", format!("first at line {}", at + 1))
                    }
                    Err(message) => ("PANIC", message),
                };
                results.lock().unwrap().push((path.clone(), verdict, started.elapsed(), note));
            });
        }
    });

    let mut results = results.into_inner().unwrap();
    results.sort();
    let same = results.iter().filter(|r| r.1 == "same").count();
    for (path, verdict, elapsed, note) in &results {
        if verbose || *verdict != "same" {
            println!("{verdict}\t{:.3}s\t{path}\t{note}", elapsed.as_secs_f64());
        }
    }
    let mut slow: Vec<_> = results.iter().collect();
    slow.sort_by_key(|r| std::cmp::Reverse(r.2));
    for (path, _, elapsed, _) in slow.iter().take(5) {
        println!("slowest\t{:.3}s\t{path}", elapsed.as_secs_f64());
    }
    println!("{same}/{} the same", results.len());
    if same != results.len() {
        std::process::exit(1);
    }
}
