# Bugs found and fixed: notes for upstream

What was found wrong in the ZX BASIC compiler while the NextBuild fork was
ported to Rust, and what was done about each. It is a summary, written so
that the upstream developer can see what may be worth taking; detail is in
the files named in each section.

Everything was found against the fork `1.18.7-nb12`, which is upstream
**v1.18.7** plus CODEBANK and ten fixes back-ported from v1.19.0 (listed in
the fork's changelog under nb1; they are upstream's own and are not repeated
here). The fork stayed on v1.18.7 because v1.19.0 rearranged the asm runtime
library again, which breaks NextBuild's libraries and programs that include
it by path; it also moved both parsers to Lark and needs Python 3.14.

Nothing here was checked against v1.19 or later, so some of it may already
be fixed there. Items in code that only the fork has are marked *fork only*;
the rest are in upstream's code as of v1.18.7.

Two places hold the fixes:

- **Python**: the fork (`reference/zxbasic`, releases `nb10` to `nb12`).
  These are ordinary patches to upstream's tree. Detail, causes and tests:
  `codebanks-fork/CHANGELOG.md` there.
- **Rust**: this port only. The Python compiler still has the bug. Detail:
  [quirks.md](quirks.md); tests in `tests/fixes/`.

## 1. Wrong code generated

Programs that compile without a message and then run wrongly.

### Fixed in Python (and so in the port)

| Level | What | Cause | Release |
|---|---|---|---|
| `-O4` | A register load at a label straight after a `CALL` is deleted when the called SUB branches. `IF c THEN Other()` then `Take(a(s))` reads `a(HL)`. | `_compute_calls` (`optimizer/flow_graph.py`) stopped walking the callee at its first jump, so the block after the call never learned that the callee returns there. New `_find_exits()`. | nb10 |
| `-O4`, zxnext | `ld d, n` before `mul d, e` is deleted: `uByte * constant` multiplies by whatever D held. | `_split_block` always made a z80 `BasicBlock`, whose `MemCell` does not know `mul`. Now `type(block)(...)`. | nb10 |
| `-O2`+ | `IF F() THEN unused = 1` drops the call to `F()`. | `visit_IF` (`api/optimize.py`) removed the empty IF without keeping the calls in its condition. | nb10 |
| `-O3`+ | `ld h, (ix+5)` / `ld h, (hl)` becomes `ld h, (hl)`. Comes of `while n < 63 and peek(p) <> 0`. | Peephole rule `102_o3_ld_r_n_ld_r_n2.opt` had no condition; it is only right when the second operand does not read the register. | nb12 |
| all | `PEEK(ulong, NAME)` with `NAME` a `CONST` loads the address from memory at the constant. Same for `long`. | `visit_PEEK` marked the address immediate only for a NUMBER node. | nb12 |
| `-O3`+ | Output differs from run to run with `PYTHONHASHSEED`. Both results are correct, but builds cannot be compared. | Iteration over a set of `str` in jump threading, over RESTORE labels, and `os.listdir()` order of the `.opt` files. All three sorted. | nb11 |

A report of the first, written for upstream's issue tracker and not sent, is
`codebanks-fork/upstream-report-O4-call-return.md`. A sweep of 349 NextBuild
programs found the `mul` bug in six of them.

### Fixed in Rust only

The flow optimiser's model of the CPU (`optimizer/cpustate.py`) is wrong in
four ways. Each can delete an instruction that was needed.

| What | Detail |
|---|---|
| A store through a pointer does not invalidate what is known of memory | Memory is tracked by the text of an address. After `ld (hl), a` with HL unknown, everything known of every variable is still believed. `ptr = @x : x = 1 : poke ptr, 2 : result = x` gives 1 at `-O3`+. The port forgets all other facts on a store through an unknown pointer. Cost: 168 bytes in NextBench's 92 K. |
| `rlca` and `rrca` leave the carry unchanged | A following `rl`/`rr` then rotates in the wrong bit: `rrca : rla` stored 1 for 0, `rlca : rra` 128. (The helpers are also named `rlc` and `rrc`, but the CB-prefixed `rlc r`/`rrc r` are not dispatched to them and are handled safely.) |
| `mul d, e` sets Z, C and S | The instruction preserves all flags. |
| Overlapping addresses | A cell is known by the text of its address with its spaces, so a store to `_x` leaves what was read from `(_x - 1)` believed. `x = 1 - x : F(1 - x)` then passes the old `x` (`-O3`+). |

The fixes are in `crates/zxr-opt/src/flow/cpustate.rs`, behind
`as_the_oracle()`, with unit tests beside them. They change `-O3`/`-O4`
output for 105 of the corpus programs, which were not examined one by one.

Also in the front end:

| What | Detail |
|---|---|
| `@r(1) + 1` for a local or parameter array: "Initializer expression is not constant" | `_is_static_unary` (`api/check.py`) asks only whether the subscripts of an ARRAYACCESS are constant, not whether the array is global, so the sum is folded as a constant. The port checks the scope. |
| `@a << n` and `@a >> n` shift the wrong way | `traverse_const` has SHL and SHR exchanged in its operator table. |
| Comparison, logical and bitwise operators on addresses end in a traceback | `@a < 16384`, `@a AND @b`, `@a bAND $FF00` build a constant expression the assembler cannot write: `InvalidOperatorError`. The port computes them at run time. |

In the backend:

| What | Detail |
|---|---|
| PEEK(FIXED, …), PEEK(FLOAT, …) and POKE FLOAT at a constant address store or read garbage | `@g(0) + 16` is an immediate expression. `F16.get_oper` (`backend/_f16.py`) has no case for it (`_32bit.py` gained one in nb12), `Float.get_oper` (`_float.py`) loads an indirect address only from `_name`, and `_storef` takes only a number, `_name` or `.label`. Each pops the address, which nobody pushed. Also a FIXED PEEK as the second operand of an operation. Every `-O` level. `Bits32.get_oper` has the same gap for the second operand, but no BASIC source found reaches it. |
| `arr(i) = @label` stores garbage | `_astore8`/`_astore16` (`backend/_array.py`) take an immediate for a constant only when it starts with `_`; a label is `#.LABEL._x`. `_pastore*` (`_parray.py`) take only numbers. Otherwise the value is popped from the stack, where nothing put it. Every `-O` level; the stack is then off by a word. |

In the runtime library:

| What | Detail |
|---|---|
| A string that does not fit corrupts the heap | When `__MEM_ALLOC` cannot find a block (a fragmented 1 K heap is enough), `__ADDSTR` (`runtime/strcat.asm`) returns its first operand, `ret z` with HL = a$, as if it were the new string: `k$ = "/" + k$` then makes k$ the literal and frees it into the program on the next assignment. `__STRASSIGN` (`runtime/strcpy.asm`) returns `&b$` when `__REALLOC` fails, and `__STORE_STR` stores it in a$. Long-standing "heap fills up and the program crashes" reports. The port returns NULL from the first and leaves a$ as it was from the second. |
| `__SWAP32` is not interrupt-safe | `runtime/swap32.asm` moves SP past the low word of the operand in the stack and back four instructions later; an interrupt there writes its return address over it. Reversed 32-bit and fixed-point division, modulo, subtraction and comparison call it. Seen as a wrong division under a 15625 Hz interrupt. The port reads a version that only pops and pushes (it changes AF, which no routine called after it reads first). |

Variables and routines that only inline asm uses:

| What | Detail |
|---|---|
| A SUB or FUNCTION only asm names is removed above `-O1` | `call ._Foo`, `jp ._Foo`, `dw ._Foo`: W170, then `Undefined GLOBAL label '._Foo'`. The port keeps it and what it calls. |
| A global used only as `_name` in asm is removed above `-O1` | The asm then fails with `Undefined GLOBAL label '._name'`. W150 says why for a scalar, if it is not turned off; for an array there is no warning at all, since `SymbolTable.arrays` filters by use before the declarations are made. The port counts a name in compiled asm as a use. |
| An unused local is removed above `-O1` and the others move up | Silently. Asm that addresses locals as `(ix-n)` then reaches the wrong one. The port warns (W950) and generates the same code. |

### Reported, not fixed

A `FASTCALL` routine has no stack frame, but BASIC code in it that uses a
second parameter or a local variable is compiled as if it had one, and reads
the caller's frame. Both compilers still generate that code; the port warns
(W940).

### Diagnostics

An error the assembler can only find once every label is known is given
with the right line and the wrong file: the name is `gl.FILENAME`, by then
the last file read. `Relative jump out of range`, `Undefined GLOBAL label`
and the cross-bank reference errors are all affected; in a program that
includes a library, the error is usually attributed to the library. The
port records the file with each instruction and label.

## 2. Tracebacks

Input on which the Python compiler ends in a traceback. The port reports one
line and stops. None is fixed in Python. The full table, with the exception
each raises, is in [quirks.md](quirks.md); the ones a user is likely to meet:

- A `CHR$` of constants assigned to an unused variable at `-O2`+, or in the
  condition of an empty IF from `-O1` (`'SymbolSTRING' object has no
  attribute 'fname'`, `optimize.py`).
- `f$()` with empty parentheses where `f$` is a string variable; an array
  assignment whose target is a plain variable (`'VarRef' object has no
  attribute 'entry'`).
- `@label` in an array initialiser at `-O0`/`-O1` (`'SymbolUNARY' object has
  no attribute 'value'`, `Translator.default_value`).
- `poke uByte, addr, v` (a comma after the type, no brackets): `p_poke3`
  asks for `p.lineno(7)` of a six-symbol rule (`IndexError`).
- `SIZEOF(...)`, always (`type object 'Type' has no attribute 'to_type'`).
- Two constant strings compared (`"a" < "b"`); unary minus or NOT on a
  constant string.
- Constant folding outside a function's domain: `SQR(-1)`, `LN(0)`,
  `ASN(2)`, division or modulo by zero, a negative constant to a fractional
  power.
- An `asm` block not closed at end of file.
- `NEXT x` with no loop open; `DIM a(5) AT @b(i)`; `DIM a AS String = 5`;
  `a = b` between array parameters; `GOTO $10` (a label written in hex).
- `#pragma name = value` with a value of the wrong type; `#pragma pop(name)`
  with nothing pushed.
- In the assembler: a `DEFS` count, `ORG` or `RST` operand that is an
  undefined label (`can't multiply sequence by non-int of type 'NoneType'`).
- In the flow optimiser: `out (n), a` and `in a, (n)` (`IndexError` in
  `requires`), `rr (hl)` once the carry is known, a LOCAL label of the same
  name in two PROCs (`DuplicatedLabelError`).

Ten files of the NextBuild corpus reach one of these.

## 3. Input that never ends

| Input | Python | Port |
|---|---|---|
| EQUs defined in terms of each other | recursion until it dies | `Label '.x' is defined in terms of itself` |
| A file that `#include`s itself (without `once`) | never ends | error after 200 levels |
| An array of more than 64K | built item by item until memory runs out | `Array 'a' is too large` |
| A `DEFS` with an enormous count | builds the bytes | error over 2^24 |
| `is_used` on a long path of basic blocks (600 in a test; the limit depends on the stack) | `RecursionError` | no limit in practice (native recursion) |
| A float constant that is an infinity | `fp.immediate_float` loops for ever | internal error |

Tests: `tests/fixes/bad/`.

## 4. Looks wrong, reproduced unchanged

The port copies these because output depends on them or because nothing
shows they matter. They may still be worth a look upstream; each has a line
in [quirks.md](quirks.md).

- `MOD` of a negative `long`: the runtime gives the remainder of the
  magnitudes (`-121328126 mod 7801` is 6974), constant folding Python's
  floored modulo (827).
- `CPUState.add`/`sub` never compute a value (`is_number(...) is not None`
  is always true); `xor`/`or`/`and` of known values always leave Z at 0
  (text compared with a number); every `ex` other than `ex de, hl`,
  `ex (sp), hl` included, swaps AF with AF'.
- The unreachable-block pass compares a set with `[]`, so it never removes a
  block.
- `BasicBlock.id` is read from the Z80 class's counter while the subclass's
  is incremented: every block of a zxnext compile has id 0 and they all hash
  alike.
- `MemCell`'s cached properties are not cleared when `asm` is assigned.
- `pop` "destroys" registers `s` and `p` (`set.update("sp")`).
- `is_2n` uses floating-point logarithms: `2**29` is not a power of two.
- `data` of type `i32`/`u32` iterates over the text of its operand, not the
  list.
- `remove_unused_labels` replaces only the last of two aliased operands on a
  line.
- The high byte of each local-array dimension size is always written as 0.
- In `visit_FOR`, `is_unsigned(step.type_)` is always false, and
  `PREV_TOKEN` is never set, so CHKBREAK is always emitted.
- `READ x` on an untyped variable names the whole argument list in its
  warning.
- `-f ir` exits 0 after an error in the pass over the globals.
- Each backend's `init()` overrides `-N` from the architecture.

## 5. Fork only

Not upstream's concern, listed for completeness.

- **Fixed in Rust.** `FarPeek` and the other far-memory routines failed to
  assemble against a bank that holds only data, since the far-call runtime
  was linked only when some bank held a routine.
- **Fixed in Rust.** The NEX packer left a bank outside 0-111 out of the
  file without a message; it is an error now.
- **Fixed in Python**, nb1 to nb9: CODEBANK itself and its fixes, Next
  runtime changes. See the fork's changelog.
- **Added in Rust.** `--sld`: source-level debug data in sjasmplus's SLD
  format, down to the line of BASIC ([sld.md](sld.md)). Nothing in it is
  particular to the fork; the scheme would serve upstream as well.
- **Added in Rust.** `PRIVATE SUB`/`FUNCTION` in a CODEBANK
  ([private.md](private.md)); warnings W930 (could be PRIVATE) and W940
  (above).

## How the port keeps both behaviours

The port is held to the Python compiler byte for byte, so each Rust-only
fix that changes output or diagnostics can be switched off:
`ZXR_ORACLE_OPTIMISER=1` restores the four CPU-model mistakes,
`ZXR_ORACLE_RUNTIME=1` the runtime library as it is, and
`ZXR_ORACLE_DIAGNOSTICS=1` silences W930, W940 and W950, removes asm-only
globals again and names files as Python does. The test harness sets
all three. With them set, 1,408 of the 1,418 corpus files and 1,534 of the 1,536
functional cases give what Python gives; the rest are the tracebacks above
and two cases out of scope.
