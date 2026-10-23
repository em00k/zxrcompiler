# Extending the compiler

Where each kind of change goes, and what has to be kept true while making it.

## The rule that comes first

Everything in `tests/` compares the port with the Python compiler pinned by
`oracle.lock`. A change that alters generated code or diagnostics therefore
has two halves: the same change in the oracle (`reference/zxbasic`, as a new
`nb` revision with a `codebanks-fork/CHANGELOG.md` entry and a functional
test), and the change here. Then:

```
printf 'commit %s\nversion <new>\n' "$(git -C reference/zxbasic rev-parse HEAD)" > oracle.lock
cargo xtask backend data        # peephole rules and tables from the oracle
cargo xtask lib sync            # runtime library, if it changed
cargo xtask golden import && cargo xtask golden bless
cargo xtask corpus manifest     # and --stage / --opt for the seams you touched
```

`1.18.7-nb12` (the dropped `ld h` at `-O3`, and `PEEK(ulong, CONST)`) is a
worked example of the whole round: see `git log` for "Oracle 1.18.7-nb12".

A change that does not alter output (speed, structure) needs none of that:
`cargo xtask golden check` and `cargo xtask corpus check` must simply stay as
they were.

Once the port no longer needs to match the oracle, the first half falls away
and the recorded expectations become the port's own.

## A keyword

1. `grammar/zxbasic.keywords`: the word in lower case and its token.
2. `grammar/zxbasic.y`: add the token to `%token`.

The lexer reads the keyword table; nothing else knows the list.

## Syntax

Productions live in `grammar/zxbasic.y`, one per line, under the `@name` of
their action:

```
@p_codebank_begin
preproc_line : CODEBANK NUMBER
preproc_line : CODEBANK INTEGER
```

The build generates the tables and a `Rule` enum from the file, so a new
`@name` is a compile error until `Driver::reduce` has an arm for it
(`crates/zxr-syntax/src/parser/actions_a.rs` and `actions_b.rs`). The arm
receives the right-hand side as a slice and returns the value of the left.

Conflicts are resolved as PLY resolves them (precedence lines at the top of
the file, then shift over reduce, then the earlier rule), silently. Check what
a new rule did to the tables by listing conflicts in the oracle with
`ZXB_PARSER_DEBUG=1`, or by reading the generated tables.

The preprocessor (`grammar/zxbpp.y`, `zxbpp_basic.lex`, `zxbpp_asm.lex`) and
the assembler (`grammar/zxbasm.y`, `zxbasm_next.y`, `zxbasm.lex`) work the
same way, with their actions in `crates/zxr-pp/src/lib.rs` and
`crates/zxr-asm/src/lib.rs`.

## A lexer rule

`grammar/*.lex` lists, for each lexer state, the rules in the order they are
tried: state, function name, token type, regular expression (Python `re`
syntax, verbose). The first that matches wins, whatever its length. What a
rule does once matched is a `match` arm on its function name in the crate's
`lexer.rs`.

## An option or `#pragma`

`crates/zxr-syntax/src/options.rs` registers every option with its type and
default. A registered option is at once a `#pragma name = value`, a
`#pragma push(name)` / `pop(name)` stack, and a field the rest of the compiler
reads. A command-line flag for it is added in
`crates/zxr-cli/src/zxbc_args.rs`.

## A warning

Messages and their codes are with the code that raises them; a coded warning
goes through `Session::warning_coded` (`crates/zxr-core/src/session.rs`),
which is what `-W code` switches off. 900 and up are the fork's.

A warning the oracle does not give must be listed beside 930 and 940 in
`crates/zxr-cli/src/bin/zxbc.rs`, where `ZXR_ORACLE_DIAGNOSTICS` turns such
warnings off; the harness sets it, and without that every program the new
warning fires on stops comparing equal.

## A statement or built-in, end to end

1. Syntax, as above; the action builds a node (`crates/zxr-syntax/src/nodes.rs`).
2. If it needs a pass of its own, add it to the order in
   `crates/zxr-syntax/src/frontend.rs`. Two constraints carried over from the
   oracle: anything that depends on source position runs before the
   optimiser, and anything that needs `CHKBREAK` gone runs after
   unreachable-code removal.
3. Translation to quads: `crates/zxr-syntax/src/translate/` (`translator.rs`
   for statements, `builtin.rs` for built-in functions).
4. If it needs a new quad: its handler in `crates/zxr-z80/src/` and a row in
   `data/backend/quads.tsv`.
5. A runtime routine: the `.asm` file under `lib/arch/zxnext/runtime`, and its
   label in `data/backend/runtime_labels.tsv`, which is what pulls the file in
   when the label is used.

## A peephole rule

Rules are data: `data/peephole/z80/*.opt` and `data/peephole/zxnext/*.opt`,
read in name order and sorted by `OFLAG`. `OLEVEL` 1 and 2 rules run as code
is emitted; 3 and 4 run inside the flow optimiser, where `GVAL`, `FLAGVAL` and
`IS_REQUIRED` know the simulated CPU state. A rule's condition must rule out
every case in which the rewrite changes what the code does: rule 102 lacked
one, and that was the `-O3` bug of nb12.

## CODEBANK, as the map of a feature that touches everything

| Part | Where |
|---|---|
| keywords `CODEBANK`, `FARPTR` | `grammar/zxbasic.keywords`, `%token` in `zxbasic.y` |
| block syntax, bank binding, `FARPTR` | `p_codebank_begin`, `p_codebank_end`, `p_function_header`, the `FARPTR` rules in `parser/actions_b.rs` |
| options (`codebank`, `codewindow`, ...) | `crates/zxr-syntax/src/options.rs`; flags in `zxbc_args.rs` |
| bank-local data and access checks | `crates/zxr-syntax/src/symtab.rs` |
| module-level asm in a bank | `crates/zxr-syntax/src/codebank_asm.rs` |
| trampolines, bank partition, deferred tables | `crates/zxr-syntax/src/translate/function.rs`, `crates/zxr-z80/src/codebank.rs` |
| the optimiser following a trampoline | `crates/zxr-opt/src/flow/` |
| assembler segments, cross-bank checks, bank files and manifest | `crates/zxr-asm/src/memory.rs`, `lib.rs` |
| runtime | `lib/arch/zxnext/runtime/farcall.asm`, `farmem.asm`; `stdlib/farmem.bas` |

The design itself is described in the oracle tree:
`reference/zxbasic/codebanks-fork/INTERNALS.md` and `PIPELINE.md`.

## PRIVATE, as the map of a feature the oracle does not have

`PRIVATE SUB` and `PRIVATE FUNCTION` exist only here (see
[private.md](private.md)). With no oracle, the rule that comes first becomes:
a program that does not use the feature compiles exactly as before, and the
feature has tests of its own that run the code.

| Part | Where |
|---|---|
| the word | `Parser::token` in `crates/zxr-syntax/src/parser/mod.rs`. Not a keyword and not in the grammar: `private` directly before `SUB` or `FUNCTION` is dropped and noted in `Compiler::pending_private`, so the grammar and its tables stay PLY's and the word stays free as a name |
| the flag | `FuncRef::private` in `ast.rs`, set in `p_function_def`; the definition is checked in `p_funcdecl` |
| code | `prepare_far_calls` and `tr_visit_function` in `translate/function.rs`: no trampoline, and the body under the routine's ordinary label |
| checks and the W930 hint | `Compiler::check_private_uses` in `compiler.rs`, run at the end of `parse` over the calls and `@name`s noted on the way (`note_call`, `note_address`), so that `--parse-only` reports them |
| tests | `tests/private/`, `crates/zxr-cli/tests/private.rs` |

## Records, as a feature that lowers to BASIC

`TYPE … END TYPE` ([records.md](records.md)) needs syntax PRIVATE's one
token of lookahead cannot give (`a(i).x`, `p->next->x`), and still leaves the
grammar alone: `crates/zxr-syntax/src/records.rs` rewrites the preprocessed
text into plain BASIC before `Compiler::parse` lexes it, line for line. A
record is a UBYTE array with one dimension more, a field a PEEK or POKE, a
pointer a UINTEGER. Because the result is BASIC, the Python compiler can
build it too, which checks everything after the rewrite
(`tests/records/README.md`). `ZXR_DUMP_RECORDS=FILE` writes it out.

| Part | Where |
|---|---|
| when it runs | `records::lower`, called first in `Compiler::parse`; nothing happens unless a line starts with `TYPE name` |
| TYPEs, layout, `SIZEOF` | `Lower::types`, `Lower::layout` |
| DIM, headers, scopes | `Lower::dim`, `Lower::header`; a local name hides a global record |
| fields and paths | `Lower::chain` reads `v.a(i).b->c`, `render` writes the address |
| tests | unit tests in `records.rs`; `tests/records/`, `crates/zxr-cli/tests/records.rs` |

Syntax that cannot be done by reading one token ahead needs productions in
`grammar/zxbasic.y`, and then `tests/lalr/zxbasic.tables` no longer describes
the grammar: the fixture test has to learn to leave the additions out first.

## Finding where something went wrong

`docs/seams.md` lists the points at which the pipeline can be cut and each
half compared with the oracle: preprocessed source, tokens, the tree before
and after the passes, quads, the optimiser's input and output, assembly,
binaries. Narrow a difference to one seam before reading code.
