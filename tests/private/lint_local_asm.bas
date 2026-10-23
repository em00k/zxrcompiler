' W950: a local that BASIC does not use, in a routine whose asm reaches
' locals through IX.
SUB Tick(p as ubyte)
    dim spare as uinteger
    dim k as ubyte
    dim arr(3) as ubyte
    k = p
    asm
        ld (ix-2), 1    ; meant for spare
    end asm
    poke 16384, k
END SUB

' Parameters are always there: nothing to say.
SUB Fine(p as ubyte)
    dim q as ubyte
    asm
        ld a, (ix+5)
    end asm
END SUB

Tick(1)
Fine(2)
