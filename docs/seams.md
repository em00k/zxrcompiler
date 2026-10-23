# Seams

Where the compiler can be cut in two and each half checked against the
oracle on its own. Every seam has an oracle side (a script under `tools/` that
drives the Python compiler without modifying it) and a port side (`zxbc
--stage`), and both write the same text.

| Seam | Oracle | Port | Recorded in |
|---|---|---|---|
| preprocessed source | `tools/oracle_stage.py <root> pp` | `zxbc --stage pp` | `tests/corpus/seam-pp.tsv` |
| tokens | `... lex` | `zxbc --stage lex` | `tests/corpus/seam-lex.tsv` |
| tree as parsed | `... ast` | `zxbc --stage ast` | `seam-ast.tsv` |
| tree after the passes | `... ast2` | `zxbc --stage ast2` | `seam-ast2.tsv` |
| quads and backend state (S2) | `... ir` | `zxbc --stage ir` | `seam-ir.tsv` |
| everything the backend consumes | `... backend-in` | read by `zxbc --from-ir` | `target/oracle-cache/backend` (not committed) |

## Recording and checking

```
cargo xtask corpus manifest --stage ir            record the oracle over the corpus
cargo xtask corpus check    --stage ir            hold the port to it
cargo xtask golden manifest --stage ir            the same over the functional suite's
cargo xtask golden check    --stage ir            BASIC cases (tests/golden/seam-ir.tsv)
```

`--opt N` forces optimisation level N on every case instead of the level the
source or the file name asks for; such a sweep is recorded apart, in
`seam-<stage>-O<N>.tsv`. `--filter TEXT` and `--verbose` work as elsewhere.

A record holds hashes, not text. To see a difference, run both toolchains
with `--keep DIR` and diff what they leave:

```
cargo xtask golden check --stage ast --filter zx48k/60.bas --toolchain oracle --keep /tmp/k
cargo xtask golden check --stage ast --filter zx48k/60.bas --keep /tmp/k
diff /tmp/k/oracle /tmp/k/port
```

Each case leaves `<case>.<stage>` (if the stage wrote anything) and
`<case>.stderr`, cleaned of the paths that differ between toolchains.

The functional-suite seam runs every `arch/**/*.bas` case and variant with the
flags the suite derives from the file name (`--arch`, `-O1`, the `optN_`
level, a variant's extra flags), less the output format and the tape flags.

## `ast` and `ast2`

Written by `dump_ast` in `tools/oracle_seam.py` and by `Compiler::dump_ast`
in `crates/zxr-syntax/src/dump.rs`. The two are the definition of the format
and must be changed together; a new node attribute is added to `node_text`
on both sides, a new identifier attribute to `entry`.

`ast` is taken straight after the parse, `ast2` after
UnreachableCodeVisitor, the code-bank asm passes, `validate_codebank_options`,
FunctionGraphVisitor and OptimizerVisitor. If the compile has reported an
error by then, nothing is written and the exit code is 1, as in `main()`.

The dump never changes what the compiler goes on to do. A node's temporary is
shown as it stands (`t=` is the raw `_t`: absent until something has asked
for it), and the cached offset of an array access is `unset` until computed.

Sections, in order:

- `--- ast`, `--- data_ast`: the two trees. One line per node, indented two
  blanks per level:
  `TOKEN PythonClass [L<lineno>] [type=<name>[?]] <attributes> [t=<_t>] [requires=...] [required_by=...]`.
  `?` marks a type that was assumed rather than declared. The attributes are
  the class's own: `file= sentinel=` (SENTENCE), `file= sentinel= bank= asm=`
  (ASM), `lower= upper=` (BOUND), `size=` (PARAMLIST), `file= byref= name=`
  (ARGUMENT), `value=` (NUMBER as `repr`, STRING escaped), `op=` (UNARY),
  `op= func=0|1` (BINARY; whether it carries a folding function), `fname=
  discard=` (BUILTIN), `file= offset=` (CALL, FUNCCALL, ARRAYACCESS,
  ARRAYLOAD). A missing child is `None`. An identifier is one line and is not
  descended into: `TOKEN SymbolID <name> <mangled> <scope>`; the exception is
  the entry of a FUNCDECL, under which the function's PARAMLIST and body
  follow, the first time the function is met.
- `--- datas`: `gl.DATAS`, each `data <label>` followed by its items as trees.
- `--- symbols`: every scope in `SYMBOL_TABLE.namespaces`, in dict order:
  `scope <namespace> parent=<namespace> owner=<function>`, then its entries
  in dict order. A type is `type <key> <class> <name>`. An identifier is
  `id <key> token= class= name= orig= mangled= scope= type= L<lineno> file=
  declared= accessed= caseins= has_address= in=<scope it lives in> t=<ref._t>
  callable= offset= faraddr=`, followed where they apply by `requires`,
  `required_by`, `addr:` and a tree, `var byref= alias= bank=`, `array
  lbound_used= ubound_used= dynamic=`, `bounds:` and a tree, `default_value:`
  and a tree (for an array, nested `[` ... `]` lists of trees), `value:` and
  a tree (CONST), `label bank= line_number= owners=`, `func bank= convention=
  forwarded= params_size= locals_size= locals=<namespace>` and `body:` with
  the parameters and body (or `body: above` if a FUNCDECL showed them).
  `caseins <key> <mangled>` lines list the scope's case-insensitive table.
- `--- globals`: `temporaries <next number>`, `tmp_labels <next number>`,
  `data_label`, `data_label_required` (sorted), `data_ptr_current`,
  `data_is_used`, `data_function`, `init` (`gl.INITS`, sorted),
  `backend_init` and `require` (the backend's sets, sorted),
  `function_call` (calls still to be checked), `function`,
  `function_bankinit`, `print_is_used`, `warnings <count>`, then every option
  as `option name=value` in the order the oracle registers them, and every
  macro as `define name=value`. Left out: the streams, `output_filename`
  and `project_filename` (paths that differ per run) and `debug_zxbpp` (the
  preprocessor's own).

Text is escaped onto one line: a backslash is doubled and anything outside
printable ASCII is `\u{hex}`.

## `ir`

```
<quads of the main pass>
--- vars
<quads of the resident variable pass>
--- vars bank <n>            one section per bank that has a declaration, lowest first
<quads of that bank's variable pass>
--- state
<records>
```

Quads are printed as `-f ir` prints them. The main pass is the program, the
functions, then `emit_data_blocks`, `emit_strings`, `emit_jump_tables` and
the end-of-user-code marker. The variable passes are what a full compile
runs (`VariableVisitor`, then `VarTranslator` for bank 0 and for each bank in
turn, as `emit_banked_vars` does), but straight after the main pass, with no
`backend.emit()` in between. In a real compile the backend runs between
them and takes temporary labels of its own, so label numbers in the variable
passes of a real compile can be higher than here; the backend seam
(`backend-in`) records those.

**`-f ir` versus `--stage ir`.** What the oracle writes for `-f ir` is the
main pass followed by the bank 0 variable pass *without* `VariableVisitor`
and without the bank passes. For a program in which the variable pass reports
no error and no bank holds data, the `ir` stage up to `--- state`, less the
`--- vars` line, is exactly the `-f ir` file (checked over the 1,257 BASIC
cases of the functional suite: the only differences are three code-bank
cases, which have bank sections, and five cases where the variable pass
reports an error, where `-f ir` still writes its file and exits 0 while the
stage, like a full compile, writes nothing and exits 1). The port's `zxbc -f
ir` follows the oracle's `-f ir`; it is tested by the `_IC` cases of
`golden check`.

The state records are tab-separated, with `\\`, `\t`, `\n`, `\r` escaped, in
the vocabulary of the backend seam's header where the two overlap:

| Record | Python | `Compiler` field |
|---|---|---|
| `option <name> <value>` | `OPTIONS.<name>`, for the options the backend reads | `opts` |
| `define <name> <value>` | `OPTIONS.__DEFINES` | `defines` |
| `require <file>` | `backend.common.REQUIRES`, sorted | `requires` |
| `init <label>` | `backend.common.INITS`, sorted | `backend_inits` |
| `flag use_function_exit|end_emitted <0|1>` | `common.FLAG_*` | `flag_use_function_exit`, `flag_end_emitted` |
| `at_end <line>` | `common.AT_END` | `at_end` |
| `far_trampoline <label> <body label>` | `codebank.FAR_TRAMPOLINES` | `far_trampolines` |
| `bank_used <n>` | `codebank.BANKS_USED`, sorted | `banks_used` |
| `bank_warned <n>` | `codebank._NO_ROUTINE_WARNED`, sorted | `no_routine_warned` |
| `bank_decl <n> <lineno> <file>` | first declaration in each bank, from `data_ast` | `data_ast` |
| `banks_with_data <n,n,...>` | `codebank.BANKS_WITH_DATA`, sorted | `banks_with_data` |
| `counter tmp_label <next>` | `tmp_labels.LABEL_COUNTER` | `label_counter` |
| `counter temporary <next>` | `opcodestemps._COUNTER` | `temp_counter` |
| `string_label <label> <text>` | `string_labels.STRING_LABELS`, dict order | `string_labels` |
| `data_label_required <label>` | `gl.DATA_LABELS_REQUIRED`, sorted | `data_labels_required` |
| `warnings <count>` | `gl.has_warnings` | `sess.has_warnings` |

The quads of the pass being translated are `backend.MEMORY`, which is
`Compiler::memory`. `gl.INITS` (the `#init` lines of the BASIC source) is
`Compiler::inits`; it is shown by the `ast` stages. Jump tables are not in
the state: `emit_jump_tables` has turned them into quads by then.

## `backend-in`

The seam under the backend: on one side the front end and its translators,
on the other everything from `backend.emit()` to the assembly text (the
backend, the peephole rules, the second preprocessor pass, the variable
passes, the splice) and on through the assembler.

```
python3 tools/oracle_stage.py <root> backend-in <zxbc arguments...> -o OUT
zxbc <the same arguments> -o OUT2 --from-ir OUT.backend-in
cargo xtask backend check [--suite corpus|functional] [--level N] [--binary]
```

Unlike the other stages, the oracle side is a whole compile: it writes what
`zxbc.py` would write to `OUT`, and beside it `OUT.backend-in`. It has to be,
because the variable passes are translated *after* the code has been emitted
and preprocessed, and draw temporary labels from the counter the backend has
been using in between. The record is taken by wrapping `Backend.emit`
(`tools/oracle_backend_in.py`): every call is a pass.

The record is lines of tab-separated fields with `\\`, `\t`, `\n`, `\r`
escaped, UTF-8, `\n` line ends. The first line is `zxr-backend-in 1`. Then
the header, which is the state as the first `emit` finds it:

| Record | Python |
|---|---|
| `option <name> <value>` | `str(OPTIONS.<name>)` (`None`, `True`, `False` as Python prints them; `opt_strategy` by its value) for `architecture`, `org`, `heap_size`, `heap_address`, `heap_start_label`, `heap_size_label`, `headerless`, `autorun`, `optimization_level`, `opt_strategy`, `memory_check`, `array_check`, `enable_break`, `zxnext`, `force_asm_brackets`, `use_basic_loader`, `output_file_type`, `codebank`, `codewindow`, `codewindowsize`, `codebankbase`, `codebankpages`, `codebankdepth`, `input_filename`. These are the values after parsing, so a `#pragma` has had its say. |
| `define <name> <value>` | `OPTIONS.__DEFINES`, in dict order. The front end adds to it (`___PRINT_IS_USED___` = `1`), and the second preprocessor pass starts from it. |
| `require <file>` | `backend.common.REQUIRES`, sorted |
| `init <label>` | `backend.common.INITS`, sorted |
| `flag use_function_exit <0|1>`, `flag end_emitted <0|1>` | `common.FLAG_*` (`_prepare_far_calls` sets the first) |
| `counter asm <next>` | `common.ASMCOUNT`; 0, since `_inline` runs inside `emit` |
| `at_end <line>` | `common.AT_END` |
| `far_trampoline <label> <body label>` | `codebank.FAR_TRAMPOLINES`, dict order |
| `bank_used <n>` | `codebank.BANKS_USED`, sorted |
| `bank_warned <n>` | `codebank._NO_ROUTINE_WARNED`, sorted |
| `bank_decl <n> <lineno> <file>` | the first declaration in each bank holding variables, from `data_ast` (what `emit_banked_vars` reports a bank with no routine against) |

and then one section per pass, in the order `main()` runs them: the code,
the resident variables, and each bank's variables.

| Record | Meaning |
|---|---|
| `pass code|vars <bank> <optimize 0|1>` | starts a pass; `bank` is the `VarTranslator`'s (0 for the code and for the resident variables) |
| `counter tmp_label <next>` | `tmp_labels.LABEL_COUNTER` as the pass starts. The backend has taken labels since the previous pass (`lvard`, `larrd`, shifts, ...), and the front end has taken more translating this one. |
| `banks_with_data <n,n,...>` | `codebank.BANKS_WITH_DATA` as the pass starts, sorted. The prologue is built from its value after the last pass. |
| `quad <op> <operand>...` | `backend.MEMORY`, one quad per line |

The last line is `end <exit code>`. Nothing is written if the compile never
reaches `emit` (an error, `--parse-only`, `-f ir`).

Not in the record, because the port rebuilds it: the state of the first
preprocessor pass. The second pass does not include again what the first
included once, so `zxbc --from-ir` runs the BASIC preprocessor over the
program before anything else, exactly as a compile does.

At -O3 and -O4 the oracle side also writes `OUT.opt-in`, the lines
`Optimizer.optimize` was given joined with `\n`, and `OUT.opt-out`, the text
it returned. `zxbc --from-ir ... --flow-stand-in OUT` uses the second in
place of a flow optimiser, after checking that the port's own lines at that
point equal the first.

`cargo xtask corpus check --flow-stand-in` does the same for a native
build (`zxbc ... --flow-stand-in OUT` without `--from-ir`), taking the
oracle's optimiser output from the record of each program.

Without `--flow-stand-in` (`backend check` and `corpus check` alike) the
port's own flow optimiser runs: `finish::emit_code` calls
`finish::flow_optimize` when it is given no stand-in. The pair of files is
also a seam of its own, text in and text out, with nothing else of the
compiler involved:

```
cargo run --release -p zxr-z80 --example flow_check -- [-v] [-j N] target/oracle-cache/backend/O4 ...
tools/flow_fuzz.py fuzz [--seed N] [--count N] [--rounds N]
tools/flow_fuzz.py order [X.opt-in ...]
```

`flow_check` runs `zxr_opt::flow` over every `X.opt-in` under the
directories (architecture, level and the code banks' trampolines come from
`X.backend-in`; a `##ASMn` line is parked assembly) and compares with
`X.opt-out`. `flow_fuzz.py fuzz` does the same with generated programs for
both architectures at both levels, the oracle's optimiser being called
directly; `order` checks that the oracle's answer does not depend on the
order of its sets (`docs/quirks.md`).

What the front end does to drive the backend without a record
(`crates/zxr-cli/src/compile.rs`) is the
sequence in `crates/zxr-z80/src/finish.rs`: `emit_code`, `preprocess`,
`get_inits`, then translate the resident variables and `emit_vars`, then
`emit_banked_vars` (which asks for each bank's quads in turn), `splice` and
`output`. `Common::tmp_labels` is the counter both sides share.
