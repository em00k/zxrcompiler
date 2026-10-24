' arr(i) = @label: the value of an array store with an index known only
' at run time, when the value is a label's address (`#.LABEL._x`, or
' `#(.LABEL._x) & 0xFF` for a byte). The Python backend recognises only
' `_name` as a constant there and pops a value nobody pushed. Global, local
' and parameter arrays of each width (a global ULONG array was right in
' Python too). Built by the Python compiler it corrupts its stack and never
' finishes. Checks itself in the far-call emulator; see
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
dim g16 as uinteger
dim g32 as ulong
dim gt(2) as uinteger
dim t8(2) as ubyte
dim t16(2) as uinteger
dim t32(2) as ulong
dim i as ubyte = 1
' The expected low byte, worked out at run time: `@lab2 band 255` would be
' a constant expression, which the Python compiler cannot write out (an
' address operator, a separate bug), so it would never reach the stores.
dim want8 as ubyte
function Id(v as uinteger) as uinteger
    return v
end function
sub Locals()
    dim lt(2) as uinteger
    dim l8(2) as ubyte
    dim l32(2) as ulong
    dim k as ubyte = 2
    dim lv as uinteger
    lt(k) = @lab2 : Ck(lt(2) = @lab2)
    l8(k) = @lab2 : Ck(l8(2) = want8)
    l32(k) = @lab2 : Ck(l32(2) = @lab2)
    lv = @lab2 : Ck(lv = @lab2)
end sub
sub Param(p() as uinteger)
    dim k as ubyte = 1
    p(k) = @lab2
end sub
t16(0) = @lab2 : want8 = t16(0) band 255
for i = 0 to 2
    t16(i) = @lab2
    t8(i) = @lab2
    t32(i) = @lab2
next i
Ck(t16(0) = @lab2) : Ck(t16(2) = @lab2)
Ck(t8(1) = want8) : Ck(t32(2) = @lab2)
i = 1
Locals()
Param(gt) : Ck(gt(1) = @lab2)
g16 = @lab2 : Ck(g16 = @lab2)
g32 = @lab2 : Ck(g32 = @lab2)
Ck(Id(@lab2) = @lab2)
poke uinteger 61500, @lab2 : Ck(peek(uinteger, 61500) = @lab2)
gt(i + 1) = @lab2 + 1 : Ck(gt(2) = @lab2 + 1)
GOTO finish
lab2:
asm
    nop
end asm
finish:
