# Changelog

What changed in zxrcompiler, the Rust port of the NextBuild fork of
Boriel's ZX BASIC compiler. Fixes are made here only: the Python compiler
(`1.18.7-nb12`, pinned in `oracle.lock`) still has the bugs listed under
"Fixed". A program that does not meet one of them builds byte for byte as
the Python compiler builds it. Detail of every difference is in
[docs/quirks.md](docs/quirks.md); a summary for upstream in
[docs/upstream-notes.md](docs/upstream-notes.md).

Each fix that changes output or diagnostics can be switched off, as the test
harness does: `ZXR_ORACLE_OPTIMISER=1` (the flow optimiser),
`ZXR_ORACLE_RUNTIME=1` (the runtime library), `ZXR_ORACLE_DIAGNOSTICS=1`
(warnings, kept variables and routines, file names in errors).

## Unreleased

### Added

- Drop-in directories for Linux (x86-64, static musl), Windows (x86-64) and
  macOS (universal: Intel and Apple Silicon), with an archive of each:
  `cargo xtask dist --all`, cross-built in the cargo-zigbuild container.
- [docs/codebanks.md](docs/codebanks.md): CODEBANK as this compiler has it.
- `--sld FILE`: source-level debug data in sjasmplus's SLD format, for
  DeZog. Breakpoints on lines of BASIC, stepping, code banks and the
  library's asm. With `ZXR_SLD=1` in the environment the file is written
  beside the map file, so `nextbuild.py` needs no change. Every global
  variable is also given under its BASIC name, so hovering it or watching
  it works. The code is the same with and without. [docs/sld.md](docs/sld.md)
- Records: `TYPE … END TYPE`, variables and arrays of a TYPE, `v.field`,
  and pointers to a record (`AS T PTR`, `p->field`) for SUBs, FUNCTIONs and
  memory nobody DIMmed. FreeBASIC's syntax. A field of a global record costs
  what a plain variable does. [docs/records.md](docs/records.md)
- `PRIVATE SUB` / `PRIVATE FUNCTION` in a CODEBANK: called directly, with no
  trampoline. [docs/private.md](docs/private.md)
- Warnings W930 (a routine only its own bank calls could be PRIVATE), W940
  (a FASTCALL routine whose BASIC code uses a second parameter or a local
  reads the caller's frame) and W950 (a local only asm uses, in a routine
  whose asm addresses locals through IX, is removed above `-O1`).
- `ZXR_ORACLE_RUNTIME`, the switch for fixes to the runtime library.

### Fixed

- On Windows: paths are handled as `C:/dir/file` throughout, so a local
  `#include` is found beside the file that includes it, and messages and
  `#line` markers name files without the `\\?\` prefix; a drive letter in
  `-I` (`C:\NextBuild\Scripts`) is kept with its path, where the Python
  compiler splits it at the colon.
- `__SWAP32` was not interrupt-safe: an interrupt during it wrote over the
  low word of an operand, and a 32-bit or fixed-point division, modulo,
  subtraction or comparison came out wrong now and then. Found by
  NextTestSuite under a 15625 Hz interrupt.
- PEEK(FIXED, …), PEEK(FLOAT, …) and POKE FLOAT at a constant address
  (`@g(0) + 16`), and a 32-bit or FIXED PEEK as the second operand of an
  operation, read or wrote garbage and left the stack a word out, at every
  `-O`.
- Arithmetic on the address of a local or parameter array's element
  (`@r(1) + 1`) ended in "Initializer expression is not constant".
- `arr(i) = @label` with an index known only at run time stored junk and
  left the stack a word out, at every `-O`, in global, local and parameter
  arrays.
- `-O3`/`-O4`: a store to `x` did not change what the optimiser knew of the
  word at `x - 1`, so `x = 1 - x : F(1 - x)` passed the old `x`.
- `-O3`/`-O4`: a store through a pointer left what was known of every
  variable believed; `rlca`, `rrca`, `rlc` and `rrc` were taken to leave the
  carry alone; `mul d, e` was taken to change the flags. Each could delete
  an instruction that was needed.
- A global variable or array only asm uses, and a SUB or FUNCTION only asm
  names (`call ._Foo`, `jp ._Foo`, `dw ._Foo`), were removed above `-O1`,
  and the asm then failed with `Undefined GLOBAL label`.
- `poke uByte, addr, v` (a comma after the type) ended in an internal error.
- `@a << n` and `@a >> n` shifted the wrong way; comparisons and logical or
  bitwise operators on addresses ended in an internal error.
- `FarPeek` and the other far-memory routines failed to assemble against a
  bank holding only data.
- An error the assembler finds late (`Relative jump out of range`,
  `Undefined GLOBAL label`, a reference across banks) named the last file
  read instead of its own.
- Input that ended a tool with a stack overflow, a panic, an allocation of
  all memory, a silent zero or no end is an error: EQUs defined in terms of
  each other, a file that includes itself, an array over 64K, a `DEFS`
  count beyond any memory, expressions and literals over 127 bits. A NEX
  bank outside 0-111 is an error instead of being left out of the file
  without a message.
- The unit tests of `zxr-asm` compile again.

### Docs

- [docs/upstream-notes.md](docs/upstream-notes.md): the bugs found, for the
  upstream developer.

## 0.1.0 (2026-10-10)

The first complete version: preprocessor, compiler, assembler, peephole and
flow optimisers and NEX packer, with the runtime library built in and a
statically linked drop-in directory for NextBuild (`cargo xtask dist`).
1,408 of the 1,418 corpus files and 1,534 of the 1,536 functional cases give
what the Python compiler gives; the rest are inputs on which it ends in a
traceback.

### Fixed

Fixed in the Python fork and so in the port, which was built against
`1.18.7-nb12`:

- `-O4`: a register load after a `CALL` to a SUB that branches was deleted
  (`IF c THEN Other()` then `Take(a(s))` read `a(HL)`). nb10.
- `-O4` on the Next: `ld d, n` before `mul d, e` was deleted, so
  `uByte * constant` multiplied by whatever D held. nb10.
- `-O2`+: `IF F() THEN unused = 1` dropped the call to `F()`. nb10.
- `-O3`+: output differed from run to run unless `PYTHONHASHSEED` was set.
  nb11.
- `-O3`+: peephole rule 102 dropped `ld h, (ix+5)` before `ld h, (hl)`, as
  in `WHILE k <> 0 AND peek(p) <> 0`. nb12.
- `PEEK(ulong, NAME)` and `PEEK(long, NAME)` with `NAME` a `CONST` read
  through the constant. nb12.

Not in the Python compiler:

- Input on which it ends in a traceback is a one-line error, exit 1 (the
  table in [docs/quirks.md](docs/quirks.md)); an internal fault in the
  backend is `zxbc: internal error in the backend: ...`, exit 70.
- A path of some 480 basic blocks no longer ends the `-O3`/`-O4` optimiser
  in `RecursionError`; a float constant that is an infinity is an internal
  error where Python loops for ever.
