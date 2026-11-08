# zxrcompiler

A Rust port of the NextBuild fork of Boriel's ZX BASIC compiler (`zxbc`,
`zxbasm`, `zxbpp`), including CODEBANK, for NextBuild. It targets the ZX Spectrum
Next only; a build ends in a `.nex` file.

Status: version 0.2.1. Every stage is written. For 1,408 of the 1,418 source
files in the NextBuild corpus a native build gives the same binaries, bank
files, map and diagnostics as the Python compiler (1.18.7-nb12); the other 10
are inputs on which the Python compiler ends in a traceback. `nextbuild.py`
works with it unchanged, and the simulator suites pass. See
`docs/quirks.md` for every known difference.

## Why

Speed. The Python compiler takes minutes over a large NextBuild project;
this one takes seconds, and gives the same output. Each program compiled
once, one at a time, with the flags `nextbuild.py` gives it, on the same
Linux machine:

| Program | Python | Rust | |
|---|---|---|---|
| AGDNext (36k lines in 61 files, 28 code banks) | 224 s | 3.4 s | 66× |
| NextBench (12 code banks) | 134 s | 2.6 s | 52× |
| NextTestSuite | 23 s | 0.74 s | 31× |
| NextSprint | 13.5 s | 0.51 s | 26× |
| a NextBuild example with a code bank | 1.33 s | 0.08 s | 17× |
| a small NextBuild example | 1.21 s | 0.07 s | 18× |

## CODEBANK

The NextBuild fork's banked code is part of the port. SUBs and FUNCTIONs in a
`CODEBANK` are compiled into the Next's 8K pages and paged in when they are
called, so a program is not limited to the code that fits beside its data in
64K. Call sites do not change. The bank files, the `banks.json` manifest and
the NEX packing are what the Python fork produces.
[docs/codebanks.md](docs/codebanks.md)

## Building

Rust 1.93 or later:

```
cargo build --release      # target/release/zxbc, zxbasm, zxbpp, zxnex
cargo test
```

## Using it with NextBuild

A drop-in directory takes the Python compiler's place: it has the layout
`nextbuild.py` expects of a compiler directory, with the native tools under
`bin/`. Point `ZXBASIC=` in `Scripts/nextbuild.config` at it,

```
ZXBASIC=/absolute/path/to/zxrcompiler-linux-x64
ZXBASIC=C:\NextBuild\zxrcompiler-windows-x64
```

or copy it into the NextBuild root and name it relative to that. Nothing else
changes. To go back, restore the previous `ZXBASIC=` line.

```
cargo xtask dist --all    dist/zxrcompiler-{linux-x64,windows-x64,macos}, and an archive of each
cargo xtask dist          dist/zxrcompiler, for this Linux machine
```

`--all` cross-builds in the `cargo-zigbuild` container, so it needs Docker, and
both need the Python compiler in `reference/zxbasic` (see below) for
`nextcreator.py`.

Each directory is named with the version (`zxrcompiler-0.2.1-windows-x64`)
and holds a `README.md` for whoever unpacks it (installing, checking,
going back), a `VERSION` file naming the commit and date it was built from,
the `CHANGELOG.md` and the licence.

- **Linux**: x86-64, statically linked (musl), for any distribution.
- **Windows**: x86-64, Windows 10 or later. Checked under Wine: NextBench,
  with its 12 code banks, gives the same binary, banks and map as on Linux.
- **macOS**: one universal binary per tool, for Intel and Apple Silicon; not
  yet run on a Mac. The tools are not notarised, so after unpacking a
  downloaded archive clear the quarantine flag once, or macOS refuses to run
  them: `xattr -dr com.apple.quarantine zxrcompiler-macos`.

The tools also run on their own (`bin/zxbc`, `bin/zxbasm`, `bin/zxbpp`,
`bin/zxnex`). The runtime library is built into them: with no `lib`
directory beside the binary and no `ZXR_LIB`, it is unpacked once under
`~/.cache/zxrcompiler`.

`ZXR_TIMINGS=1` prints the time each stage of a compile took.

Things the Python compiler does not have: `PRIVATE` routines in a
CODEBANK, called directly instead of through a trampoline. See
[docs/private.md](docs/private.md).

And `--sld FILE`, which writes the table a debugger needs to stop on a line
of BASIC and step through it: [docs/sld.md](docs/sld.md).

And records: `TYPE … END TYPE`, arrays of them and pointers to them
(`p->field`), in FreeBASIC's syntax: [docs/records.md](docs/records.md).

## Layout

| Path | What |
|---|---|
| `CHANGELOG.md` | what changed in each version, and what is fixed here only |
| `crates/` | the compiler: `zxr-re`, `zxr-lalr`, `zxr-lex`, `zxr-core`, `zxr-pp`, `zxr-syntax`, `zxr-ir`, `zxr-z80`, `zxr-opt`, `zxr-asm`, `zxr-nex`, `zxr-cli` |
| `docs/` | `extending.md`, `seams.md`, `quirks.md`, `codebanks.md`, `private.md`, `sld.md`, `records.md`, `upstream-notes.md` |
| `grammar/` | the grammars (`.y`) and lexer rules (`.lex`) the parsers are generated from |
| `xtask/` | the test harness and project automation |
| `lib/` | runtime and standard library, copied unchanged from the Python compiler |
| `data/` | the backend's quad and runtime-label tables and the peephole rule files, extracted from the Python compiler |
| `tests/golden/functional/` | inputs of the functional suite inherited from the Python compiler |
| `tests/golden/expected/` | what the Python compiler produces for each, recorded |
| `oracle.lock` | the exact commit of the Python compiler everything is measured against |
| `tests/fixes/`, `tests/private/`, `tests/records/`, `tests/sld/` | programs for the fixes and features this compiler alone has |
| `reference/` | not included: the Python compiler (`zxbasic/`) the harness compares against |

## The oracle

The Python compiler being ported is referred to as the oracle. It lives in
`reference/zxbasic`, a git repository of its own, and must be at the commit
`oracle.lock` names with no uncommitted changes; the harness refuses to record
anything otherwise.

## Commands

These are the comparison harness. Apart from `cargo xtask golden check`,
which reads its expectations from `tests/golden/`, they need the Python
compiler in `reference/zxbasic`; the `corpus` commands also need a snapshot of
NextBuild sources in `reference/corpus`, which is not published.

```
cargo xtask dist                     build dist/zxrcompiler, the drop-in directory for NextBuild
cargo xtask dist --all               the drop-in directories for Linux, Windows and macOS (Docker)
cargo xtask oracle check             reference/zxbasic is at the pinned commit
cargo xtask lib check                lib/ is an unchanged copy of the oracle's library

cargo xtask golden check             run the functional suite against the port
cargo xtask golden check --toolchain oracle
cargo xtask golden bless             re-record expectations from the oracle
cargo xtask golden upstream-diff     where our expectations differ from upstream's files

cargo xtask corpus manifest          re-record the oracle's result for every corpus file
cargo xtask corpus check             hold the port to the manifest
cargo xtask corpus check --stage pp  compare preprocessed source only
cargo xtask corpus check --stage lex compare BASIC token streams only
cargo xtask corpus hybrid-asm        the oracle's generated assembly through the port's assembler
cargo xtask corpus summary

cargo xtask backend check            the oracle's front end into the port's backend (docs/seams.md)
cargo xtask backend check --level 2 --binary --suite corpus
cargo xtask backend data --check     data/ is an unchanged copy of the oracle's tables and peephole rules
```

`--filter TEXT` narrows any check to cases whose path contains `TEXT`;
`--verbose` lists every failure; `--output-only` ignores diagnostics and exit
codes, for bringing a stage up before its error paths exist. `--diagnostics-only`
is the reverse: `golden check` runs only the cases the oracle fails and compares
diagnostics and exit code, not the output file.

The port is expected at `target/release/{zxbc,zxbasm,zxbpp}`.

## What "passes" means

A case passes when the toolchain does exactly what the oracle did: the same
output file (or none), the same stdout and stderr, the same exit code. A source
file the oracle rejects must be rejected with the same messages. Nothing is
required to compile or run correctly in its own right.

Text outputs are compared after the same clean-up the Python test driver
applies: comment lines, `#line` markers, blank lines and trailing blanks are
dropped from asm, and install paths are rewritten to `/zxbasic`.

## Licence

AGPL-3.0-or-later, as the compiler it is ported from. The runtime library in
`lib/` and the test inputs in `tests/golden/functional/` are that project's
files, unchanged.
