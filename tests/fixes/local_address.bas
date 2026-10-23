' Arithmetic on the address of an element of a local or parameter array:
' `@r(1) + 1`, `@r(2) - @r(0)`. The Python compiler took the address for a
' constant, as it is for a global array, and stopped with "Initializer
' expression is not constant". Checks itself in the far-call emulator; see
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
SUB CkEq(a as uInteger, b as uInteger)
    Ck(a = b)
END SUB
DIM g(9) AS UBYTE

SUB Param(p() AS UBYTE)
    POKE @p(1) + 1, 42
    Ck(p(2) = 42)
    CkEq(@p(3) - @p(0), 3)
END SUB

SUB Locals()
    DIM r(9) AS UBYTE
    DIM w(3, 0 TO 4) AS UINTEGER
    DIM k AS UBYTE = 2
    POKE @r(1) + 1, 7
    Ck(r(2) = 7)
    Ck(PEEK(@r(0) + 2) = 7)
    POKE UINTEGER @w(1, 0) + 2, 1234
    Ck(w(1, 1) = 1234)
    CkEq(@r(5) - @r(0), 5)
    CkEq(@w(2, 0) - @w(1, 0), 10)
    CkEq(@r(k) + 1 - @r(0), 3)
    Param(r)
    Param(g)
END SUB

Locals()
