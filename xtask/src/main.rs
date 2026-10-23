//! Project automation: `cargo xtask <command>`.

mod backend;
mod corpus;
mod dist;
mod golden;
mod toolchain;
mod util;

use toolchain::Toolchain;

const USAGE: &str = "\
usage: cargo xtask <command>

  dist                         build dist/zxrcompiler, the drop-in directory for NextBuild
  oracle check                 verify reference/zxbasic is at the commit in oracle.lock
  lib sync | lib check         copy the runtime library from the oracle / verify the copy
  golden import                copy the functional suite's inputs from the oracle
  golden bless                 record expectations from the oracle
  golden check [options]       run the suite against a toolchain
  golden upstream-diff         compare recorded expectations with upstream's files
  corpus manifest              record what the oracle makes of every corpus file
  corpus check [options]       run a toolchain over the corpus and compare
  corpus hybrid-asm            assemble the oracle's generated assembly with the port
                               and compare the binaries with the manifest
  corpus summary               print totals from the recorded manifest
  backend check [options]      the oracle's front end into the port's backend: compare
                               the assembly (or, with --binary, the assembled output)
  backend data [--check]       copy the backend's tables and the peephole rules from
                               the oracle, or verify the copies

options:
  --toolchain oracle|port      which compiler to run (default: port)
  --stage pp                   corpus: record or compare an intermediate stage
                               instead of the full build (pp: preprocessed source,
                               lex: tokens, ast / ast2: the tree before and after
                               the passes, ir: quads and backend state).
                               golden manifest|check --stage S: the same seam over
                               the BASIC cases of the functional suite
  --opt N                      with --stage: force optimisation level N on every
                               case; recorded apart, in seam-<stage>-O<N>.tsv
  --flow-stand-in              corpus check (full build) and backend check: at -O3 and above hand the port
                               the oracle's flow-optimiser output for each program
                               (recorded under target/oracle-cache/backend)
  --keep DIR                   with --stage: leave what the toolchain wrote for
                               each case in DIR/<toolchain>/, to diff by hand
  --filter TEXT                only cases whose id contains TEXT
  --output-only                golden check: ignore diagnostics and exit codes
  --diagnostics-only           golden check: only the cases the oracle fails, compared
                               on diagnostics and exit code, not on the output file
  --verbose                    list every failure
  --suite corpus|functional    backend check: which programs (default: all)
  --level N                    backend check: compile everything at -ON instead of
                               the level each program asks for
  --binary                     backend check: compare binaries, bank files and maps
";

fn flag(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}

fn value(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let words: Vec<&str> = args.iter().take(2).map(String::as_str).collect();
    let stage = value(&args, "--stage");
    let opt = value(&args, "--opt");
    let keep = value(&args, "--keep").map(std::path::PathBuf::from);
    let seam = corpus::Seam { stage: stage.as_deref(), opt: opt.as_deref(), keep: keep.as_deref(), flow_stand_in: flag(&args, "--flow-stand-in") };
    let toolchain = || Toolchain::parse(&value(&args, "--toolchain").unwrap_or_else(|| "port".into()));
    let result: Result<bool, String> = (|| match words.as_slice() {
        ["dist"] => dist::dist().map(|_| true),
        ["oracle", "check"] => corpus::oracle_check().map(|_| true),
        ["lib", "sync"] => corpus::lib_sync().map(|_| true),
        ["lib", "check"] => corpus::lib_check(),
        ["golden", "import"] => golden::import().map(|_| true),
        ["golden", "bless"] => {
            corpus::oracle_check()?;
            golden::bless(&Toolchain::oracle()).map(|_| true)
        }
        ["golden", "manifest"] if stage.is_some() => {
            corpus::oracle_check()?;
            golden::seam_manifest(seam).map(|_| true)
        }
        ["golden", "check"] if stage.is_some() => golden::seam_check(&toolchain()?, seam, value(&args, "--filter"), flag(&args, "--verbose")),
        ["golden", "check"] => golden::check(
            &toolchain()?,
            &golden::CheckOptions { filter: value(&args, "--filter"), verbose: flag(&args, "--verbose"), output_only: flag(&args, "--output-only"), diagnostics_only: flag(&args, "--diagnostics-only") },
        ),
        ["golden", "upstream-diff"] => golden::upstream_diff().map(|_| true),
        ["corpus", "manifest"] => {
            corpus::oracle_check()?;
            corpus::manifest(seam).map(|_| true)
        }
        ["corpus", "check"] => corpus::check(&toolchain()?, seam, value(&args, "--filter"), flag(&args, "--verbose")),
        ["corpus", "hybrid-asm"] => corpus::hybrid_asm(value(&args, "--filter"), flag(&args, "--verbose")),
        ["corpus", "summary"] => corpus::summary(seam).map(|_| true),
        ["backend", "check"] => {
            corpus::oracle_check()?;
            backend::check(&backend::Options {
                suite: value(&args, "--suite").unwrap_or_else(|| "all".into()),
                level: value(&args, "--level").map(|l| l.parse().map_err(|_| format!("--level: '{l}' is not a number"))).transpose()?,
                binary: flag(&args, "--binary"),
                filter: value(&args, "--filter"),
                verbose: flag(&args, "--verbose"),
                flow_stand_in: flag(&args, "--flow-stand-in"),
            })
        }
        ["backend", "data"] => backend::data(flag(&args, "--check")),
        _ => {
            print!("{USAGE}");
            Ok(args.is_empty() || flag(&args, "--help"))
        }
    })();
    match result {
        Ok(true) => {}
        Ok(false) => std::process::exit(1),
        Err(message) => {
            eprintln!("error: {message}");
            std::process::exit(2);
        }
    }
}
