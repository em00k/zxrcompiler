' poke uByte, addr, v (a comma after the type, no brackets). The Python
' compiler ends in an IndexError: its rule asks for the line of a seventh
' symbol of six. Checks itself in the far-call emulator; see
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
dim a as uinteger = 61500
poke uByte, a, 5 : Ck(peek(a) = 5)
poke uInteger, a, 1234 : Ck(peek(uinteger, a) = 1234)
poke uByte a + 2, 7 : Ck(peek(a + 2) = 7)
poke (uByte, a + 3, 9) : Ck(peek(a + 3) = 9)
