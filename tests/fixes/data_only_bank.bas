' A bank holding only data, read with the far memory routines. The Python
' compiler leaves the far-call runtime out and the program does not assemble
' (Undefined GLOBAL label '.core.__FAR_CUR_BANK'). Checks itself in the
' far-call emulator; see tests/private/README.md.
'!org=32768
#include <farmem.bas>
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
CODEBANK 1
    DIM lut(3) as uByte => {21, 22, 23, 24}
END CODEBANK
CODEBANK 2
tbl:
asm
    db 1,2,3,4
    dw 4660
end asm
END CODEBANK
dim buf(3) as ubyte
Ck(FarPeek(FARPTR lut(2)) = 23)
Ck(FarPeek(FARPTR tbl + 3) = 4)
Ck(FarPeekW(FARPTR tbl + 4) = 4660)
FarCopy(FARPTR lut, @buf(0), 4)
Ck(buf(0) = 21 AND buf(3) = 24)
FarPoke(FARPTR tbl, 200)
Ck(FarPeek(FARPTR tbl) = 200)
Ck(FarPeek(FARPTR lut(0)) = 21)
