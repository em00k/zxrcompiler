' Stores through pointers and what the optimiser may still believe afterwards.
' The Python compiler fails the first check at -O3 and -O4. Checks itself in
' the far-call emulator; see tests/private/README.md.
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
dim x as ubyte
dim w as uinteger
dim ptr as uinteger
dim result as ubyte
dim wres as uinteger
dim arr(3) as ubyte
dim i as ubyte

SUB Bump()
    x = x + 10
END SUB

SUB ByRefSet(byref v as ubyte)
    v = 77
END SUB

FUNCTION Local() as ubyte
    dim a as ubyte
    dim p as uinteger
    a = 1
    p = @a
    poke p, 9
    return a
END FUNCTION

' a byte through a pointer
ptr = @x
x = 1
poke ptr, 2
result = x
Ck(result = 2)

' a word through a pointer
ptr = @w
w = 1000
poke uinteger ptr, 2000
wres = w
Ck(wres = 2000)

' the pointer moved on by one
ptr = @w
w = $0101
poke ptr + 1, 5
wres = w
Ck(wres = $0501)

' reading through the pointer after a store to the variable
ptr = @x
result = peek(ptr)
x = 44
result = peek(ptr)
Ck(result = 44)

' an array element reached by a variable index, and by address
arr(1) = 3
i = 1
arr(i) = 8
result = arr(1)
Ck(result = 8)
ptr = @arr(0)
arr(2) = 5
poke ptr + 2, 6
result = arr(2)
Ck(result = 6)

' a call in between, and BYREF
x = 1
Bump()
result = x
Ck(result = 11)
x = 1
ByRefSet(x)
result = x
Ck(result = 77)

' a local, through its address
Ck(Local() = 9)

' increment in place through a pointer
ptr = @x
x = 5
asm
    ld hl, (_ptr)
    inc (hl)
end asm
result = x
Ck(result = 6)
