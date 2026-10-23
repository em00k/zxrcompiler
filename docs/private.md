# PRIVATE routines in a CODEBANK

This compiler only; the Python compiler has no such thing.

```
CODEBANK 1
    PRIVATE SUB DrawTile(x as uByte, y as uByte)
        ...
    END SUB

    PRIVATE FUNCTION fastcall Clamp(v as uByte) as uByte
        ...
    END FUNCTION

    SUB DrawMap()            ' public: callable from anywhere
        DrawTile(0, 0)       ' a plain CALL, no far call
    END SUB
END CODEBANK
```

An ordinary routine in a bank is reached through a 6-byte trampoline in the
resident program, and every call to it, even from the next routine of the
same bank, goes through the far-call runtime: page the bank in, call, page
the old one back. A `PRIVATE` routine has no trampoline. Its body is in the
bank under its own label and a call to it is one `CALL`. That saves the 6
resident bytes and the far-call time on each call, and it only works while
its bank is paged in, so the compiler holds you to that:

| Written | Result |
|---|---|
| a call from the same bank | direct `CALL` |
| a call from another bank or from the resident program | error: `'DrawTile' is PRIVATE to CODEBANK 1 and cannot be called from ...` |
| `@DrawTile` in the same bank | its address in the bank's window |
| `@DrawTile` anywhere else | error: `... cannot be addressed from ...` |
| `PRIVATE` on a routine that is not in a bank | error |
| `DECLARE` and definition disagreeing about `PRIVATE` | error |
| `call _DrawTile` in inline asm outside the bank | the assembler's cross-bank error |

All of these are reported when the source has been read, so
`zxbc --parse-only` (an editor checking as you type) shows them.

## The hint: W930

```
game.bas:12: warning: [W930] 'DrawTile' is only called from its own CODEBANK 1. Declared PRIVATE it would be called directly, with no trampoline
```

Given for a banked routine every call of which is in its own bank, and whose
address is never taken. Calls from inline asm are not seen, so if asm outside
the bank calls it, leave it public. Checking one file of a project on its own
only sees that file's calls: the answer that counts is the main program's.
`-W930` turns it off.

Notes:

- `private` is not a reserved word. It means this only directly before `SUB`
  or `FUNCTION` (any case); a variable called `private` is still fine.
- A `DECLARE PRIVATE ...` may stand outside the `CODEBANK` block. The bank is
  the one the definition is in.
- The address of a private routine is only good while its bank is paged in.
  Handing it to code in another bank, or keeping it in a resident variable
  to call later from elsewhere, is not checked and will not work. A resident
  `DIM p as uInteger = @DrawTile` is rejected by the assembler, since that is
  resident data naming a bank's label.
- Module-level statements between `CODEBANK` and `END CODEBANK` are resident
  code, as always, so they cannot call a private routine either.
- Nothing is inferred: a routine is private only where it says so.
- The map file lists a private routine as `B<n>:<addr>: ._Name`, with no
  `.__far` entry.
