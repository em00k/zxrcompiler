# Source-level debugging: `--sld`

`zxbc --sld FILE` writes, beside the program, a table of which source line
every instruction came from, in the SLD format of sjasmplus (version 1).
A debugger that reads it can stop on a line of a `.bas` file and step
through it while the program runs in an emulator. [DeZog](https://github.com/maziac/DeZog),
a VS Code extension, does, with CSpect, ZEsarUX, MAME or its own simulator.

The Python compiler has no such option. Code is the same with and without
it: the option only adds the file.

```
zxbc --arch=zxnext -O2 -S 32768 game.bas -o game.bin --sld game.sld
```

With `ZXR_SLD=1` in the environment the compiler writes the file without
being asked on the command line: beside the map file if `-M` names one
(`build/game.bas.map` gives `build/game.bas.sld`), and beside the output
if not. That is how a build through `nextbuild.py` gets one.

## What is in the file

| Row | What |
|---|---|
| `T` | one for each instruction: source file, line, 8K page, address |
| `L` | one for each label: `_score`, `_Bump`, the runtime's, and `EQU`s |
| `Z` | the Next's memory model: 224 pages of 8K in 8 slots |

- **BASIC statements.** Every instruction generated for a statement is of
  that statement's line. A routine's entry and exit code is of the line of
  its `SUB` or `FUNCTION`. The code that closes a `FOR` or a `WHILE` is of
  the line the loop begins on.
- **Asm.** Each instruction of an `asm` block, and of the runtime library's
  `.asm` files, is of its own line.
- **Pages.** Code in a CODEBANK has the bank's own page, so two banks at
  `$6000` are told apart. The resident program has the pages a NEX file
  maps there: 10 and 11 at `$4000`, 4 and 5 at `$8000`, 0 and 1 at `$C000`.
- **Paths** are absolute.

Variable storage, the start-up code and data (`DEFB` and the like) have no
`T` rows.

## How exact it is

Up to `-O2` every instruction is followed through the peephole rules: what
a rule writes is of the lines it replaced. At `-O3` and `-O4` the flow
optimiser's result is compared with what went in, and an instruction it
rewrote takes the line of the one it stood for or of the one before it; a
line can then be off by a statement. For stepping through BASIC, build at
`-O2` or below.

## Variables

Every global variable has two labels: `_name`, as the assembler knows it,
and `name`, as the source has it. The second is what makes a debugger find
the word under the mouse. For an array, `name` is the first element
(`_name` is the array's descriptor). Where an asm block has a label of the
same bare name, that label keeps it and the variable has only `_name`.

In DeZog:

- **Hover** over a global: its address, and the byte and the word there.
- **Watch**: `score` (size guessed from the distance to the next label),
  or with the size given:

  | Type | Watch |
  |---|---|
  | `byte`, `ubyte` | `name` or `name,1` |
  | `integer`, `uinteger` | `name,2` |
  | `long`, `ulong`, `fixed` | `name,4` |
  | `float` | `name,1,5`: five raw bytes |
  | `string` | `name,2` is a pointer to the length word; the text follows it |
  | array of bytes | `cells,1,10`, or one element: `cells[3]` |
  | array of integers | `cells,2,10`, `cells[3],2` |

- **Watchpoints**: `-wpadd name 2 w` in the debug console stops on a write
  to the variable. DeZog does not offer watchpoints with CSpect
  ("Watchpoints not supported for CSpect"); they work with its other
  emulators (ZEsarUX, MAME, the internal simulator).

Limits:

- **Locals and parameters have no name.** They are on the stack at
  `IX±n`; a label is a fixed address and SLD has no notion of a stack
  frame.
- **A variable named like a register shows the register.** DeZog looks for
  a register first, in either case: `a b c d e h l i r f`, `hl de bc ix iy
  sp pc` and the rest. `dim i as ubyte` hovers as register I; `_i` in a
  watch is the variable.
- A variable nothing uses is removed above `-O1` and has no label.

## What a debugger will not show

- A line of BASIC is several instructions, and DeZog steps by instruction.

## DeZog

A `launch.json` entry for CSpect (which includes the DeZog plugin):

```json
{
    "type": "dezog",
    "request": "launch",
    "name": "CSpect",
    "remoteType": "cspect",
    "sjasmplus": [{ "path": "build/game.sld" }],
    "rootFolder": "${workspaceFolder}",
    "load": "build/game.nex",
    "startAutomatically": false
}
```

DeZog loads the `.nex` by copying its banks into the emulator and setting
PC and SP; it does not show the loading screen. `"remoteType": "zsim"` with
`"zsim": { "memoryModel": "ZXNEXT" }` runs the program in DeZog's own
simulator, which has the Next's paging and nothing else of its hardware.

This has been checked against DeZog's SLD parser as written, not by a run
in the debugger.

## How it is made

- The translators give each quad the source line of the statement being
  translated (`Quad::origin`, a number in `Compiler::origins`).
- The backend keeps a source line beside each line of assembly it emits,
  through the peephole rules (`Backend::origins`); `finish::follow` finds
  the lines again in what the later passes return.
- The assembly handed to the assembler has a comment, `;@sld N`, before
  each run of lines of one source line. Asm blocks and included files have
  their `#line` as always.
- The assembler notes, for each instruction and label, the mark or `#line`
  in force where it was read, and `Assembler::sld` writes the table.

Tests: `crates/zxr-cli/tests/sld.rs`, with `tests/sld/demo.bas`.
