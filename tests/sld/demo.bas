'!org=32768
dim score as uinteger
dim i as ubyte
dim big as ulong
dim cells(3) as ubyte
dim flags as ubyte

SUB Bump(n as ubyte)
    score = score + n
    asm
    flags:
        ld a, 2
        out ($fe), a
    end asm
END SUB

FUNCTION Twice(v as ubyte) as ubyte
    return v * 2
END FUNCTION

for i = 1 to 10
    Bump(i)
    if score > 20 then
        score = Twice(3)
    end if
next i
poke 16384, score
big = score
cells(1) = i
flags = 1
poke 16385, cells(1) + flags + big
