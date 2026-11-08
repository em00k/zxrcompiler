' RANDOMIZE, then RND again and again: every call after the first returned
' the same number. On the Next, nb10 moved RAND's state out of its own
' `ld de, nn` operand into a word of its own, but left the load as
' `ld de, 0C0DEh`, so each call started from the same state. A float is
' compared by its bytes: arithmetic on it needs the ROM, which the emulator
' does not have. Checks itself in the far-call emulator; see
' tests/private/README.md.
'!org=32768
#define RESULTS 61440
DIM t_run as uInteger
DIM t_fail as uInteger
SUB Ck(ok as uByte)
    POKE RESULTS + t_run, ok
    IF ok = 0 THEN
        t_fail = t_fail + 1
    END IF
    t_run = t_run + 1
END SUB

DIM prev AS FLOAT
DIM cur AS FLOAT
DIM a AS FLOAT
DIM b AS FLOAT
DIM c AS FLOAT
DIM d AS FLOAT
DIM n AS UBYTE
DIM repeats AS UBYTE

FUNCTION Same(x AS UINTEGER, y AS UINTEGER) AS UBYTE
    RETURN PEEK(ULONG, x) = PEEK(ULONG, y) AND PEEK(x + 4) = PEEK(y + 4)
END FUNCTION

' Thirty calls in a row: none repeats the one before.
SUB Run()
    repeats = 0
    prev = RND
    FOR n = 1 TO 30
        cur = RND
        IF Same(@cur, @prev) THEN repeats = repeats + 1
        prev = cur
    NEXT n
END SUB

RANDOMIZE
Run()
Ck(repeats = 0)

RANDOMIZE 12345
Run()
Ck(repeats = 0)

' A seed given twice gives the same numbers twice, and they differ.
RANDOMIZE 999 : a = RND : b = RND
RANDOMIZE 999 : c = RND : d = RND
Ck(Same(@a, @c))
Ck(Same(@b, @d))
Ck(NOT Same(@a, @b))
