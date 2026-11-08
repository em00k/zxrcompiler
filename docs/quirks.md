# Quirks

Places where the port knowingly does something other than copy the oracle, and
oracle behaviour that looks like a bug but is reproduced because output
depends on it. Add an entry whenever either happens.

## The oracle crashes; the port must report an error instead

Seven corpus files end in a Python traceback (all in `Sources/OlderSources`).
`corpus check` flags them at every seam from the one where the oracle dies:
the port stops at the same point with a one-line message instead.

| Input | Oracle |
|---|---|
| `OlderSources/Basic/ReadDir.bas`, `OlderSources/Spectrum/ReadDir.bas` | `AttributeError: 'SymbolSTRING' object has no attribute 'fname'` |
| `OlderSources/httpbank/GetData-5-115200.bas`, `GetData-5-multi.bas`, `GetData4-http.bas`, `GetData456k-http-test.bas`, `GetData456k-http.bas` | `AttributeError: 'VarRef' object has no attribute 'entry'` |

A few paths in the preprocessor and assembler end the same way in the oracle:
a lexer state with no rule for the next character, an `#include` that fails
and is followed by text, an operand that cannot be evaluated where the Python
code expects a number (`ORG`, `DEFS` count, `RST`). The port prints a one-line
message or skips the directive. No test reaches them. Three corpus files
do, at every optimisation level: `Sources/AGDNext/agdc/agdc_core.bas`,
`Sources/AGDNext/engine/eng_core.bas` and
`Sources/AnotherWorld/aw_engine.bas`, parts of a program compiled on their
own, where a `DEFS` count is a label defined elsewhere (`TypeError: can't
multiply sequence by non-int of type 'NoneType'` in `asm.py`, after the
first "Undefined label" error). The port skips the directive, goes on to
report the remaining undefined labels, and exits 5 (or 1 when it gives up
after too many errors); the oracle's exit status is 1.

## The compiler driver

- `-F`/`--config-file` and `--save-config` are not ported: the port refuses
  them (status 2). The oracle also loads `./project.ini` whenever one
  exists; the port prints a warning that it does not read it and carries on.
- `--append-binary` and `--append-headless-binary` are accepted and ignored.
- A quad the backend cannot handle ends a compile with `zxbc: internal error
  in the backend: <what>` and status 70, as with `--from-ir`.
- `ZXR_TIMINGS` in the environment makes `zxbc` print the time of each stage
  on stderr. Port only.

## The BASIC front end (Phase 3)

### The oracle crashes; the port prints one line and exits 1

Each of these ends in a Python traceback in the oracle. The port stops at the
same point with `zxbc: internal error: <the exception text>` and status 1, so
diagnostics differ and nothing else can be compared.

| Input | Oracle |
|---|---|
| A `CHR$` of constants on the right of an assignment to an unused variable at `-O2` and above, or in the condition of an empty IF from `-O1` (`ReadDir.bas` x2 in the corpus; `chr.bas`, `chr1.bas`, `const6.bas` in the functional suite when forced to `-O2`+) | `AttributeError: 'SymbolSTRING' object has no attribute 'fname'` in `optimize.py` |
| `f$()` with empty parentheses where `f$` is a string variable; an array assignment whose target turned out to be a plain variable (`GetData*.bas` x5 in the corpus) | `AttributeError: 'VarRef' object has no attribute 'entry'` |
| `@label` in an array initialiser while the array is kept (`NextBench/NextBench.bas` forced to `-O0`/`-O1`; at `-O2`+ the array is dropped as unused) | `AttributeError: 'SymbolUNARY' object has no attribute 'value'` in `Translator.default_value` |
| An `asm` block not closed at end of file | `TypeError: %i format` (`p_start` hands the production to `p_error`) |
| `SIZEOF(...)`, always | `AttributeError: type object 'Type' has no attribute 'to_type'` |
| Two constant strings compared (`"a" < "b"`), or unary minus / NOT folded on a constant string | `AttributeError: ... no attribute 'text'` |
| A math function folded outside its domain (`SQR(-1)`, `LN(0)`, `ASN(2)`), division or modulo of constants by zero, a negative constant to a fractional power | `ValueError` / `ZeroDivisionError` / an assertion |
| `DIM a(5) AT @b(i)` | `IndexError` (`p.lineno(4)` on a three-symbol rule) |
| `DIM a AS String = 5` | `AttributeError` on `None` |
| A label written as a hex or binary number: `GOTO $10`, `RESTORE %101`, `ON x GOTO 0x10` | `TypeError`: `get_entry` indexes an `int` |
| `NEXT x` with no loop open | `IndexError` |
| `a = b` between array parameters | `TypeError: reduce() of empty iterable` |
| A constant vector after a row with a non-constant element (`{{1, x}, {2, 3}}`) | `TypeError` on `None` |
| `SAVE`/`LOAD`/`VERIFY`, `USR`, `VAL`, `SGN`, `CHR`, a slice or a single-line IF with ELSE, on an operand that already failed | `AttributeError` / `IndexError` on `None` |
| `#pragma name = value` with a value of the wrong type; `#pragma pop(name)` with nothing pushed; `CODEBANK INTEGER` | uncaught `InvalidValueError`, `OptionStackUnderflowError`, `ValueError` |
| LETARRAY into a parameter array at a constant offset; a LABEL reaching the variable translator | `InternalError`, `AttributeError: aliased_by` |

### Extensions

- **`PRIVATE SUB` / `PRIVATE FUNCTION`** ([private.md](private.md)). The
  oracle reads `private` there as a name and reports a syntax error. Anywhere
  else the word is still a name in both.

- **Records: `TYPE … END TYPE`** ([records.md](records.md)), with `.` for a
  field and `->` through a pointer. Lowered to plain BASIC before the parser
  (`crates/zxr-syntax/src/records.rs`), and only when a line starts with
  `TYPE name`; in the oracle `e.x` is an illegal character and `p->x` a
  syntax error, so no program it builds can mean anything else.

- **`--sld FILE`** ([sld.md](sld.md)): source-level debug data for DeZog
  and other debuggers. The code is the same with and without it. The oracle
  does not know the option.

- **Warnings W930 and W940.** W930: a banked routine that could be
  `PRIVATE`. W940: a `FASTCALL` routine whose BASIC code uses a parameter or
  a local variable. Such a routine has no stack frame and the generated code
  reads the caller's (both compilers generate it; only this one says so).
  `-W930` / `-W940` turn them off, and so does `ZXR_ORACLE_DIAGNOSTICS=1` in
  the environment, which the harness sets so that diagnostics still compare
  equal with the oracle's.

- **Warning W950.** A local variable that BASIC code does not use, in a
  routine whose asm has an operand `(ix-n)`. Above `-O1` both compilers
  remove such a local without a word and the other locals move up, so the
  asm reaches a different variable, or memory below the frame. The code is
  as the oracle's; only the warning is new. Off with `-W950` and with
  `ZXR_ORACLE_DIAGNOSTICS=1`. `tests/private/lint_local_asm.bas`.

### Deliberate divergences

- **A global that only asm uses is kept.** A variable or array no BASIC
  statement reads is removed above `-O1`; asm that names it (`_name`,
  `_name.__DATA__`) is then left with `Undefined GLOBAL label`, and W150,
  which would explain it, is one NextBuild turns off (for an array it is
  not given at all). The port counts a name in asm that is compiled as a
  use, unless the asm declares that label itself. A program the oracle
  builds is not changed by this: there the case cannot arise. With
  `ZXR_ORACLE_DIAGNOSTICS=1` the variable is removed as in the oracle.
  `tests/fixes/asm_only_vars.bas`.

- **A SUB or FUNCTION that only asm names is kept**, with what it calls.
  `call ._Foo`, `jp ._Foo` or a `dw ._Foo` table in asm that is compiled
  counts as a use; above `-O1` the oracle removes the routine (W170) and
  the asm fails with `Undefined GLOBAL label`. As for globals, a program
  the oracle builds is not changed, and `ZXR_ORACLE_DIAGNOSTICS=1` removes
  the routine again. `tests/fixes/asm_only_subs.bas`.

- **Arithmetic on the address of a local array's element compiles.**
  `_is_static_unary` asks only whether an element's subscripts are constant,
  so `@r(1) + 1` for a local or parameter array is folded as a constant, and
  translating it ends in "Initializer expression is not constant". The port
  takes such an address as known at run time only. `ZXR_ORACLE_DIAGNOSTICS=1`
  gives the error again. `tests/fixes/local_address.bas`.

- **`poke uByte, addr, v` compiles.** The rule for a type followed by a
  comma asks, without brackets, for the line of a seventh symbol of six:
  `IndexError` in the oracle. The port takes the last symbol's line.
  `tests/fixes/poke_comma.bas`.

- **Errors the assembler finds late name their own file.** An instruction
  that waits for a label is checked when the output is written; the oracle
  then gives its line with the name of the file it read last (`Relative jump
  out of range`, `Undefined GLOBAL label`, a reference across code banks,
  and the errors of an expression). The port records the file with each
  instruction and label. With `ZXR_ORACLE_DIAGNOSTICS=1` it says what the
  oracle says. `tests/fixes/jump_range.bas`.

- **Input neither compiler should accept** (`tests/fixes/bad/`). Each is one
  error with a line number here. In the oracle: EQUs defined in terms of each
  other recurse until it dies; a file that includes itself never ends; an
  array of more than 64K is built item by item until memory runs out. A
  `DEFS` count over 2^24, an assembler expression that overflows 127 bits,
  and a hexadecimal, octal or binary literal too long for them are errors
  too, where the oracle's unbounded integers carry on. `zxnex` refuses a
  bank outside 0-111, which the Python tool leaves out of the file without
  a word.

- **A bank of nothing but data, with `farmem`.** The oracle links the
  far-call runtime only when some bank holds a routine, so `FarPeek` and the
  rest fail to assemble against a data-only bank (`Undefined GLOBAL label
  '.core.__FAR_CUR_BANK'`), after a W910 saying the bank cannot be reached.
  The port links the runtime when the program's asm names
  `__FAR_CUR_BANK`, and gives no W910 then. `tests/fixes/data_only_bank.bas`.

- **Operators on addresses.** With both operands addresses or constants of
  them (`@a < 16384`, `@a AND @b`, `@a bAND $FF00`), the oracle builds a
  constant expression it cannot write out and stops with
  `InvalidOperatorError`. The port computes these when the program runs.
  `@a << n` and `@a >> n` the oracle writes with the operators exchanged;
  the port does not. `tests/fixes/address_ops.bas`.

- **Integers are 127 bits.** Python's are unbounded; a folded constant that
  does not fit `i128` ends the compile with the internal-error line.
- **`VAL("...")` of a constant string** is folded by evaluating a Python
  expression. The port evaluates numeric literals, parentheses, unary
  `+ - ~` and `+ - * / // % ** << >> & | ^`. Anything else `eval` would
  accept (`abs(-3)`, `True`, a comparison, a quoted string) gives 0 with the
  "Invalid string numeric constant" warning, where the oracle gives the value.
- **`eval_to_num`** (array bounds) likewise only reads a number.
- **`#pragma` of `stdin`, `stdout`, `stderr`, `__DEFINES` or `debug_zxbpp`**:
  these are not in the port's option registry, so it warns of an unknown
  pragma where the oracle assigns.
- **Circular dependencies between `DIM ... AT` variables**: the oracle walks
  a set of tuples hashed by object address, so which pair it reports first is
  not defined. The port reports them in discovery order.
- **`gl.INITS`, `REQUIRES`, `DATA_LABELS_REQUIRED`** are Python sets; the port
  keeps them ordered and every dump sorts them.

### Oracle behaviour that looks like a bug and is reproduced

- `traverse_const` has SHL and SHR the wrong way round in its operator table.
- `check.is_dynamic` asks nodes for an attribute none has; it comes down to
  "has a scope, and it is not global".
- A name the symbol table knows is never lexed as a label, even first on its
  line and followed by a colon.
- `(x and not y) or (not x and y)` (XOR folding) and `x or y` / `x and y`
  return an operand, not a boolean.
- A number converted in place keeps a float value that is whole (`2` cast to
  float prints as `2.0`); a negative value cast to an unsigned type keeps its
  sign when it fits.
- `Scope.__delitem__` pops the case-insensitive table with the name as
  written, so an entry can stay behind there.
- Unreachable code: removed silently at `-O0`, warned about and kept at `-O1`
  (but removed without a warning after a later ender in the same block),
  warned about and removed from `-O2`.
- `OptimizerVisitor.visit_FUNCDECL` overwrites the FUNCDECL's second child
  (the PARAMLIST) with the function entry; `visit_IF` discards the visited
  condition and THEN part when it drops an empty ELSE.
- `codebank_asm._addressed_symbols` tests for a token `ARRAY` that no node
  has, so arrays are walked into rather than reported.
- In `visit_FOR`, `check.is_unsigned(step.type_)` is always false;
  `PREV_TOKEN` is never set, so CHKBREAK is always emitted.
- In `FunctionTranslator.start()`, a nested function for a bank already
  emitted is never translated, nor are nested functions found while draining
  the deferred resident list.
- The high byte of each local-array dimension size is always written as `00`.
- A ByRef array-element argument emits its address but no `param` quad.
- `READ x` on an untyped variable warns "Using default implicit type 'float'
  for '(ARGUMENT)'": the name passed is the whole argument list.
- `-f ir` writes its file and exits 0 even when the pass over the globals
  reported an error (`arrlabels10.bas`, `dim_at_label4`-`7`); a full compile
  exits 1 there.
- Each backend's `init()` sets `OPTIONS.zxnext` from the architecture after
  the options are read, overriding `-N`.

## Fixed in the oracle so that there is one answer to copy

`1.18.7-nb11`: jump threading, RESTORE labels past the last DATA line, and
peephole rule files are processed in sorted order. Before that, `-O3`/`-O4`
output varied with `PYTHONHASHSEED`. See `codebanks-fork/CHANGELOG.md` in the
oracle.

## The backend and the peephole optimiser

Reproduced because output depends on it:

- `data` of type `i32`/`u32` loops over the operand's text instead of the
  list read from it, so each character of the list's `repr` becomes a pair of
  `DEFW`s (`_data` in `backend/generic.py`).
- A peephole pattern line is matched from the start of an instruction but
  need not reach its end, and a variable bound on an earlier line is compared
  after the later line has matched on its own, with no second attempt.
- `pop` "destroys" the registers `s` and `p` (`set.update("sp", ...)` adds
  the letters), which matters to nothing but is what `MemCell.destroys` says.
- `remove_unused_labels` rewrites a line from its original text for each
  aliased operand, so on a line with two of them only the last is replaced.
- `is_2n` and the shift counts derived from `log2` use floating-point
  logarithms; `2**29` is not a power of two by that test.
- The quirks of single handlers are commented where they are ported
  (`crates/zxr-z80/src/*.rs`); each was checked by `tools/quad_fuzz.py`.

Where the port differs:

- **An array store of a label's address** (`arr(i) = @label`, the index
  known only at run time): the value is an immediate, `#.LABEL._x` or
  `#(.LABEL._x) & 0xFF`, which `_astore8`/`_astore16` recognise only when
  it starts with `_`, and `_pastore8`/`16`/`32` (local and parameter
  arrays) only when it is a number. Otherwise they pop the value from the
  stack, where nothing put it, so the store is wrong and the stack is off
  by a word. The port loads the constant. No program the oracle compiles
  correctly changes. `tests/fixes/label_store.bas`.
- **PEEK and POKE of FIXED and FLOAT at a constant address** (`@g(0) + 16`,
  an immediate that is an expression): `F16.get_oper` has no case for one
  (`Bits32.get_oper` has had it since nb12), `Float.get_oper` loads an
  address only from a variable, and `_storef` takes only a number, `_name`
  or `.label`. Each pops the address from the stack, where nothing put it.
  The same is true of a FIXED PEEK as the second operand of an operation.
  `Bits32.get_oper` had the same gap for the second operand, closed here
  too, though no BASIC program found reaches it. PEEK(FIXED, …), PEEK(FLOAT, …) and POKE FLOAT were wrong at
  every `-O`, and the stack off by a word; the port loads the address. No
  program the oracle compiles correctly changes. The fields of a global
  record are reached this way. `tests/fixes/const_address.bas`.
- A quad the Python backend cannot handle ends it in a traceback (`KeyError`,
  `IndexError` on an empty operand, `InvalidICError`, ...). `zxbc --from-ir`
  prints `zxbc: internal error in the backend: <what>` and exits 70.
- Python integers have no size; operands are read into 128 bits, and a
  larger one is "not a number" (the float module alone reads integers of any
  length where it has to).
- `data` with a list operand that is not a list (`eval` gives a number) works
  in Python for the `i32` case above; the port rejects it. The front end only
  emits lists.
- `fp.immediate_float` of an infinity loops for ever in Python; the port
  reports an internal error.
- `helpers.is_number` and `valnum` ask Python's `eval`. The port has a small
  evaluator for numeric expressions (`crates/zxr-opt/src/pyeval.rs`); a
  conditional expression, a call, or a name that short-circuiting would have
  skipped is "not a number" there. Up to -O2 the answer only ever guards a
  register lookup, which a number cannot satisfy, so nothing can differ; the
  flow optimiser uses the same evaluator (see its section below).
- The rule-file tokenizer cuts a token down to the first operator name it
  starts with, looking through a Python *set* of names. No token of the 53
  rule files starts with two of them; the port takes the longest.
- A rule file with an error is skipped with a warning in Python. The rule
  files are compiled into the port, which refuses to start if one does not
  parse. `data/peephole/<arch>.rules` records how the oracle reads each file
  and a unit test holds the port's parser to it.
- `-d` makes the Python engine report each rule it applies; the port is silent.

One more input ends in a traceback in the oracle, at -O0 and -O1 only:
`Sources/NextBench/NextBench.bas` (`AttributeError: 'SymbolUNARY' object has
no attribute 'value'`, in the front end).

## The flow optimiser (-O3, -O4)

`crates/zxr-opt/src/flow` is a literal port of `src/arch/z80/optimizer` and
the Next's subclasses. Checked three ways: the oracle's recorded optimiser
input and output for every corpus program and functional case
(`cargo run --release -p zxr-z80 --example flow_check -- target/oracle-cache/backend/O4`),
generated assembly through both (`tools/flow_fuzz.py fuzz`), and the
oracle's unit tests.

### Sets of basic blocks: ordered by index, and why that is enough

In Python a `BasicBlock` hashes by `self.id`, and `id` is read from the Z80
class's counter while `__new__` increments the counter *of the class being
made*. So every block of a Next compile, and every `DummyBasicBlock`, has
the id the Z80 counter last had: 0 for all of them on the Next. The sets are
then single collision chains whose iteration order is insertion and
deletion history, not anything sorted. The port does not imitate that. Its
sets are `BTreeSet`s of arena indices, and at each place one is iterated
the result is the same whatever the order:

| Site | Why the order cannot matter |
|---|---|
| `_split_block`: `for blk in list(block.goes_to)` | every edge is moved to the new block; the edges are a set either way |
| `_compute_calls`: `for label in jump_labels` (a set of `str`, in hash order: not sorted even in nb11), `for bb in labels[label].used_by` | only adds edges |
| `_compute_calls`: `for bb in exits[entry]`; `_find_exits` | adds edges; the exits are what a search reaches, however it is walked |
| `Optimizer.optimize`, jump threading: `for blk in list(used_by)` (the labels themselves are sorted) | each `blk` is rewritten from the label's first instruction, found before the loop, and its own condition; no `blk` reads what another's turn changed |
| `get_next_exec_instruction`: `next(iter(blk.goes_to))` | only when the set has one element |
| `goes_requires`: `for block in self.goes_to` | an "any"; `is_used` sets `lock` only on the blocks of the path being walked and always clears it, so it is a function of the graph |
| `guesses_initial_state_from_origin_blocks`: `sfirst(comes_from)`, `list(comes_from)[1:]` | an intersection of dictionaries, commutative; the order of keys in the result is the first block's and nothing iterates it to an effect |
| `x.comes_from == []`, `for y in self.JUMP_LABELS` (unreachable blocks) | a set is never equal to a list: the loop never runs and no block is ever dropped. Reproduced: nothing is removed |
| `update_next_block` (iterates `goes_to` while deleting from it) | never called |
| Sets of register names (`MemCell.requires`/`destroys`, `ALL_REGS`, `mem_vars`), `ix_ptr` | membership tests and removals; `BasicBlock.destroys()` returns a list in set order, which only the debug trace prints |

`tools/flow_fuzz.py order` asks the oracle itself: the same programs
optimised with blocks hashed by a scrambled number and under other
`PYTHONHASHSEED`s give the same text.

### Reproduced because output depends on it

- **`cached_property` outlives the instruction.** `MemCell.inst`, `opers`,
  `branch_arg`, `requires` and `destroys` are cached, and `cell.asm = ...`
  does not clear them. A jump that threading redirects keeps the operands
  and register needs it had when first asked; a LOCAL label that was renamed
  is still entered in the label table under its old name, so the same LOCAL
  name in two PROCs ends the optimiser with `DuplicatedLabelError`. The
  port's `flow::Cell` caches the same five answers and never clears them.
- **The debug trace is computed even when it is not shown.** Its f-strings
  call `b.requires()` and `b.destroys()` for every block before threading,
  which is when most instructions are first asked (and cached), and where an
  instruction `requires` cannot handle ends the compile: `out (n), a` and
  `in a, (n)` are an `IndexError` there. The port makes the same calls.
- `CPUState`: `add`/`sub` never compute anything (the test
  `is_number(...) is not None` is always true); `xor`/`or`/`and` of known
  values always leave Z at 0 (`self.get("a") == 0` compares text with a
  number); `set()` compares `getv(r)` (a number) with text, so it never
  returns early but does read the register or memory cell; anything other
  than `ex de, hl` (`ex (sp), hl` included) swaps AF with AF'; `pop` leaves SP
  unknown when it is known to be 0.
- Reading a memory cell that was never written gives it an unknown value
  and takes a number from the counter, so every read the Python code makes,
  used or not, is made in the same order. (`FLAGVAL` builds both flag
  entries and its default before looking one up.)
- The Next's blocks make a Z80 `CPUState` and then their own; dummy blocks
  are Z80 blocks. The counter of unknown values is reset when the label
  table is built, after the first block exists.
- `BasicBlock.is_used` on a `DummyBasicBlock` tests the names as given, so
  `hl` is not "used" by a routine that needs `h` and `l`.

### Where the port differs

Four mistakes of the Python flow optimiser are not made. Each changes the
code produced at `-O3` and `-O4`, so the harness sets `ZXR_ORACLE_OPTIMISER=1`,
with which they are made after all and output compares equal again.

- **Stores and aliases.** What is known of memory is kept by the text of an
  address. After `ld (hl), a` with HL unknown the oracle still believes what
  it knew of every variable, and deletes the reload of one HL pointed at:
  `ptr = @x : x = 1 : poke ptr, 2 : result = x` gives 1. The port forgets
  everything else on a store through an unknown pointer, and what unknown
  pointers point at on any store. `tests/fixes/pointer_alias.bas`. Cost on
  NextBench: 168 bytes in 92 K.
- **`rlca` and `rrca`** leave the carry as it was in the oracle's model, and
  `rl`/`rr` then rotate in the wrong bit. (The model's helpers for them are
  named `rlc` and `rrc`; the CB-prefixed `rlc r` and `rrc r` never reach
  them, and the model forgets everything after an instruction it does not
  know.)
- **`mul d, e`** sets Z, C and S in the oracle's model; the instruction
  preserves the flags.
- **Overlapping addresses.** A cell is known by the text of its address,
  spaces included, so the high byte of the word read from `(_x - 1)` is a
  cell `_x - 1+1` that a store to `_x` does not touch. `x = 1 - x :
  F(1 - x)` reloads `x` with `ld hl, (_x - 1)`, and the oracle deletes the
  second reload, leaving the old `x` in H. The port drops the spaces before
  keying a cell. `tests/fixes/overlap_reload.bas`. Changes 3 of the 149
  Tier A programs (BallKicker26, both Crimbo): in each, a reload after
  `x = x + n` that the oracle deleted is kept. Whether that showed as a
  fault in them was not checked.

- **A traceback is one line and exit 70.** The Python optimiser dies on
  input its simulator has no case for (`rr (hl)` once the carry is known,
  `push 5`, `ld a, _label`,
  the `in`/`out` forms above, a label defined twice, an empty program, ...).
  The port panics at the same point with the exception's name and
  `zxbc` prints `zxbc: internal error in the backend: ...`. None of the 888
  corpus programs or 1,139 functional cases gets there; `flow_fuzz.py` finds
  the same programs fatal on both sides.
- **`RecursionError`.** `is_used` recurses along the flow graph, two Python
  frames per block, so a long path on which a register is neither read nor
  written ends the oracle (600 blocks did in a test, 480 did not; it depends
  on the stack). The port has no such limit. The
  deepest walk in the corpus and the functional suite at -O3 and -O4 is 29
  blocks.
- **`simplify_arg` asks `eval`.** The port's evaluator (`pyeval.rs`) knows
  numbers and operators. Not reproduced: an operand whose value is a `bool`
  (`1 < 2`, `not 0`, `True` are left as written; Python writes `True`), a
  call (`abs(3)`), a conditional expression, a name that short-circuiting
  skips (`1 or x`), and the exceptions `eval` raises that `simplify_arg`
  does not catch (`1/0` is a `ZeroDivisionError` in Python and "not a
  number" here). `ld a, abs(3)` is `ld a, 3` to Python and a label, which
  an 8 bit register cannot hold, to the port: it stops there. The backend emits none of these; user assembly is parked
  as `##ASMn` and never reaches the optimiser.
- **`MemCell.used_labels`** runs the assembler's lexer over the instruction.
  The port has a small scanner with that lexer's rules for the tokens that
  can swallow an identifier (`flow/cell.rs`). It only matters between PROC
  and ENDP, which the backend does not emit. The Python lexer also reports
  a character it does not know (`illegal character '<'`) as an error of the
  compile; the scanner steps over it.
- `Asm.bytes`, `sizeof` and `max_tstates` (the regexes generated from the
  opcode table) are not ported: only the debug trace shows them.
- The counter of unknown values (`helpers._RAND_COUNT`) is the optimiser's
  own; the backend's keeps its count from the code pass. In Python they are
  one, so later passes draw higher numbers. Unknown values are only ever
  compared, and never with one from before.
- `cleanup_local_labels` blanks a `LOCAL` line with `";"` and the `repr` of
  an object; the port writes `;LOCAL`. The line is deleted next.

## The runtime library

`lib/` is the Python compiler's library, unchanged (`cargo xtask lib
check`). A routine found wrong is replaced as it is read
(`crates/zxr-pp/src/runtime_fixes.rs`, the new text in `crates/zxr-pp/fixes/`),
and only when the file's text is exactly the one known to be wrong. The
harness sets `ZXR_ORACLE_RUNTIME=1`, which reads the library as it is.

- **`RND` repeats after the first call (the fork's nb10).** `RAND`'s state
  was the operand of its own `ld de, nn` (`RANDOM_SEED_HIGH EQU RAND+1`) and
  the ROM's `SEED`; nb10 moved both into words in the program, but the load
  stayed `ld de, 0C0DEh`. Each call stores that constant as the low word and
  starts again from it, so every `RND` after the first is the same number.
  The port loads the word. `RANDOMIZE` with the frame counter at 0 gave a
  state of 0, where xorshift stays: it now keeps the state there is.
  zx48k's `random.asm` still has the operand form and is right.
  `tests/fixes/rnd_repeat.bas`.

- **A string that does not fit corrupts the heap.** The heap is small
  (NextBuild gives it 1,024 bytes) and fragments, so an allocation can fail
  with plenty free in total. Two routines then return a pointer the caller
  takes for a new block of its own:
  `__ADDSTR` (`a$ + b$`, `strcat.asm`) returns a$, the first operand; in
  `k$ = "/" + k$` that is the literal, and the next assignment to `k$`
  frees it into the program, after which the free list runs through code.
  `__STRASSIGN` (`a$ = "literal"`, `strcpy.asm`, also used by string-array
  copies) returns the address of the source, which `__STORE_STR` stores in
  a$: a literal again, or a block named twice, and a$'s old block is lost.
  A program then crashes some time later, far from the cause. The port
  returns NULL (the empty string) from `__ADDSTR`, and leaves a$ as it was
  in `__STRASSIGN` on the Next, whose `__REALLOC` keeps the old block when
  it cannot allocate (zx48k's frees it first, so there a$ becomes empty).
  `tests/fixes/string_heap.bas`, run with `heap_check.py`, which walks the
  heap: each fix alone leaves the other case corrupt or hung. There is no
  leak besides: 300 rounds of string parameters, locals, results, slices
  and arrays in SUBs and FUNCTIONs give all their memory back in both
  compilers.

- **`__SWAP32` is not interrupt-safe.** It moves SP up past the low word of
  the operand in the stack (`inc sp` twice) and back four instructions
  later; an interrupt between writes its return address over that word. A
  32-bit or fixed-point division, modulo, subtraction or comparison whose
  operands come reversed calls it, so in a program with interrupts on the
  result is wrong now and then: a divisor of 7800 became $A96C. Found by
  NextTestSuite under a 15625 Hz CTC interrupt. The replacement pops and
  pushes only, never leaving a word still wanted below SP; it is one byte
  shorter and changes AF, which every routine called after it sets before
  reading. Changes 7 of the 149 Tier A programs.
  `tests/fixes/swap32_irq.bas`, run by `tests/fixes/irq_check.py` with an
  interrupt at every instruction of the routine: 5 of 14 checks fail with
  the old one at `-O0`, `-O2` and `-O4`, none with the new.

Reproduced, not fixed:

- **`MOD` of a negative `long`** is the remainder of the magnitudes at run
  time (`__MODI32`): `-121328126 mod 7801` is 6974. Folded as a constant it
  is Python's floored modulo, 827. Neither is C's -6974.
