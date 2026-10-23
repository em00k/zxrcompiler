' SUBs and FUNCTIONs that only asm names: called from asm, and in a dw
' table. The Python compiler removes them above -O1 (W170) and the asm
' fails with Undefined GLOBAL label. What such a routine calls is kept too;
' a routine nothing names is still removed. Checks itself in the far-call
' emulator; see tests/private/README.md.
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
dim hits as ubyte
dim fromtable as uinteger
sub Inner()
    hits = hits + 10
end sub
sub FromAsm()
    hits = hits + 1
    Inner()
end sub
function Doubled(v as uinteger) as uinteger
    return v * 2
end function
sub NeverUsed()
    hits = 99
end sub
asm
    call ._FromAsm
    call ._FromAsm
    ld hl, (table)
    ld (._fromtable), hl
    jr skip
table:
    dw ._Doubled
skip:
end asm
Ck(hits = 22)
Ck(fromtable <> 0)
