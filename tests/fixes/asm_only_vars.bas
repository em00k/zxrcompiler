' Globals that only asm uses. No BASIC statement reads them, so the Python
' compiler removes them above -O1 and the asm is left with undefined labels.
' Checks itself in the far-call emulator; see tests/private/README.md.
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
dim counter as ubyte
dim preset as ubyte = 5
dim word as uinteger = $1234
dim buf(15) as ubyte
dim table(3) as ubyte => {10, 20, 30, 40}
dim seen as ubyte
dim wseen as uinteger

' written by BASIC, read by asm
dim fromBasic as ubyte
fromBasic = 77

SUB Called()
    asm
        ld a, (_counter)
        inc a
        ld (_counter), a
    end asm
END SUB

Called()
Called()
asm
    ld a, (_counter)
    ld (_seen), a
end asm
Ck(seen = 2)

asm
    ld a, (_preset)
    ld (_seen), a
    ld hl, (._word)
    ld (_wseen), hl
end asm
Ck(seen = 5)
Ck(wseen = $1234)

asm
    ld hl, _buf.__DATA__
    ld (hl), 99
    ld a, (_buf.__DATA__)
    ld (_seen), a
end asm
Ck(seen = 99)

asm
    ld a, (_table.__DATA__ + 2)
    ld (_seen), a
end asm
Ck(seen = 30)

asm
    ld a, (_fromBasic)
    ld (_seen), a
end asm
Ck(seen = 77)
