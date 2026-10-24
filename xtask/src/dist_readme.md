# zxrcompiler {VERSION} for {PLATFORM}

A drop-in replacement for the Python compiler in NextBuild: Boriel's ZX
BASIC with the NextBuild fork's CODEBANK support, rewritten in Rust. It
builds your programs the same as the Python compiler {BASE} does, much
faster: NextBench in 2.6 s instead of 134 s, AGDNext in 3.4 s instead of
224 s.

Built on {DATE} from commit {COMMIT}.
`VERSION` has the details, and `CHANGELOG.md` what has changed.

## Installing

1. Unpack this folder anywhere. Inside NextBuild's own folder is simplest.
2. Open `Scripts/nextbuild.config` in your NextBuild folder and find the
   `ZXBASIC=` line. Note it down, so you can go back.
3. Change it to name this folder:

{INSTALL}
   A path relative to the NextBuild folder works too, if you put this folder
   there: `ZXBASIC={FOLDER}`.
4. Build a project as usual. Nothing else in NextBuild changes.

{PLATFORM_NOTES}
To check it is the one in use: the build log names the compiler version
`zxrcompiler {VERSION}`, and `{ZXBC} --version` prints `zxbc {VERSION}`.

**To go back** to the Python compiler, put the old `ZXBASIC=` line back.

## What it adds

These are in this compiler only. A program that uses them will not build
with the Python compiler:

- **Records**: `TYPE … END TYPE`, arrays of records, and pointers to them
  (`p->field`).
- **`PRIVATE SUB` / `PRIVATE FUNCTION`** in a code bank: called directly, with
  no trampoline. Warning W930 lists the routines that could be private.
- **`--sld`**: debug data for DeZog. Set `ZXR_SLD=1` and NextBuild writes a
  `.sld` beside the map, for breakpoints and stepping on lines of BASIC.
- **Fixes** for bugs the Python compiler has (all listed in `CHANGELOG.md`).
  A program that does not run into one of them builds byte for byte as
  before.

The full documentation is in the source repository, under `docs/`.

## What is in this folder

| Path | What |
|---|---|
| `bin/` | the compiler (`zxbc`), assembler (`zxbasm`), preprocessor (`zxbpp`) and NEX packer (`zxnex`){EXE_NOTE} |
| `zxbc.py`, `zxbasm.py`, `zxbpp.py`, `src/` | what NextBuild calls; each hands over to the program in `bin/` |
| `src/lib/` | the runtime and standard library |
| `tools/nextcreator.py` | the NEX packer NextBuild uses |

The programs in `bin/` also run on their own, with the same options as the
Python tools.

## Licence and source

AGPL-3.0-or-later, as Boriel's ZX BASIC, which this is a port of: see
`LICENSE.txt`. The source is at https://github.com/em00k/zxrcompiler.
