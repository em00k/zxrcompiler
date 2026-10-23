# zxrcompiler

A Rust port of the NextBuild fork of Boriel's ZX BASIC compiler (`zxbc`,
`zxbasm`, `zxbpp`), including CODEBANK, for NextBuild. It targets the ZX Spectrum
Next only; a build ends in a `.nex` file.

Status: version 0.1.0. Every stage is written. For 1,408 of the 1,418 source
files in the NextBuild corpus a native build gives the same binaries, bank
files, map and diagnostics as the Python compiler (1.18.7-nb12); the other 10
are inputs on which the Python compiler ends in a traceback. `nextbuild.py`
works with it unchanged, and the simulator suites pass. See
`docs/quirks.md` for every known difference.

## Building

Rust 1.93 or later:

```
cargo build --release      # target/release/zxbc, zxbasm, zxbpp, zxnex
cargo test
```

## Using it with NextBuild

```
cargo xtask dist
```

(which needs the Python compiler in `reference/zxbasic`, see below, for
`nextcreator.py`) builds `dist/zxrcompiler`: statically linked tools under `bin/`, and around
them the layout `nextbuild.py` expects of a compiler directory. In
`Scripts/nextbuild.config`, set

```
ZXBASIC=/absolute/path/to/dist/zxrcompiler
```

or copy the directory into the NextBuild root and name it relative to that.
Nothing else changes. To go back, restore the previous `ZXBASIC=` line.

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
| `CHANGELOG.md` | what changed since 0.1.0, and what is fixed here only |
| `crates/` | the compiler: `zxr-re`, `zxr-lalr`, `zxr-lex`, `zxr-core`, `zxr-pp`, `zxr-syntax`, `zxr-ir`, `zxr-z80`, `zxr-opt`, `zxr-asm`, `zxr-nex`, `zxr-cli` |
| `docs/` | `extending.md`, `seams.md`, `quirks.md`, `private.md`, `sld.md`, `records.md`, `upstream-notes.md` |
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
