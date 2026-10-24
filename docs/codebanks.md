# CODEBANK: code in banked memory

Boriel's ZX BASIC compiles a program to one flat block of memory, so all of
its code has to fit in the 64K the Z80 can see at once, beside the ROM,
screen, variables and stack: in practice about 32K. The NextBuild fork of the
compiler adds CODEBANK, which puts SUBs and FUNCTIONs in the Next's banked
memory instead and pages them in when they are called. A program can then
have as much code as there are 8K pages to hold it. CODEBANK is part of
this port, built to give exactly what the Python fork gives, banks and
manifest included.

## Using it

```basic
'!org=$8000                 ' the program must clear the code window
'!codebank=40               ' CODEBANK 1 -> 8K page 40, 2 -> 41, ...

CODEBANK 1
    DIM table(255) AS UBYTE ' bank-local data: costs no resident memory
    SUB BuildLevel(n AS UBYTE)
        ...
    END SUB
END CODEBANK

#pragma codebank = 2        ' whole files, around an #include
#include "menus.bas"
#pragma codebank = 0        ' and back to resident code

BuildLevel(2)               ' an ordinary call
```

Call sites do not change. Each banked routine leaves a 6-byte trampoline at
its ordinary label in resident memory. The trampoline pages the routine's
bank into the code window (`$6000-$7FFF`, MMU slot 3, by default), runs it,
and pages back whatever was there before. A call to a routine in the bank
already paged in takes a short path.

A `DIM` inside a CODEBANK block puts the variable in that bank's page, so a
large table costs nothing out of resident memory.

## Settings

| `'!` directive (nextbuild.py) | `#pragma` | `zxbc` flag | Meaning | Default |
|---|---|---|---|---|
| `'!codebank=N` | `codebankbase` | `--code-bank-base` | the 8K page that holds CODEBANK 1; later banks follow | 30 |
| `'!codebankpages=a,b,...` | `codebankpages` | `--code-bank-pages` | the first page of each bank, in place of the above | |
| `'!codewindow=$XXXX` | `codewindow` | `--code-window` | where banks are paged in | `$6000` |
| `'!codewindowsize=N` | `codewindowsize` | `--code-window-size` | `8192`, or `16384` for banks of two pages | 8192 |
| `'!codebankdepth=N` | `codebankdepth` | `--code-bank-depth` | how deep calls from bank to bank may nest | 16 |

## Output

Beside the program's `.bin`, `zxbc` writes `<name>.bank<N>.bin` for each bank
and `<name>.banks.json`, which says which pages each bank goes in.
`nextbuild.py` reads the manifest and packs the banks into the `.nex`. The
build prints how full each bank is, and fails, naming the largest items, when
one overflows.

## What the compiler checks

- A bank fits its window: 8192 bytes, or 16384 with a 16K window.
- No `GOTO`, `jp` or `call` from one bank straight into another: calls go
  through the trampolines.
- No reference from outside a bank to a variable, array or asm label inside
  it.
- No `ORG` in a bank, and the program does not overlap the code window.

What it cannot check: an interrupt handler must not be in a bank or call
into one; the stack must not be inside the window; and the address of
bank-local data is no use to code in another bank, which finds its own page
at that address.

## What this port adds

- **`PRIVATE SUB` / `PRIVATE FUNCTION`**: a routine only its own bank calls
  needs no trampoline. It is called directly and costs no resident memory.
  See [private.md](private.md).
- **Warning W930** lists the routines that could be `PRIVATE`.
- **`FarPeek` and the other far-memory routines** assemble against a bank
  that holds only data, which the Python fork rejects.
- **`--sld`** debug data covers the banks: a breakpoint on a line of BASIC in
  a bank stops there when that bank is paged in. See [sld.md](sld.md).

Where each part of CODEBANK lives in the code is in
[extending.md](extending.md#codebank-as-the-map-of-a-feature-that-touches-everything).
The design is described in full in the Python fork's `codebanks-fork/`
directory (`README.md`, `INTERNALS.md`, `PIPELINE.md`).
