' Checks of the emulator itself (run_far.py), not of the compiler.
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

DIM victim as uByte = 0
DIM seen as uByte = 0

' 1. Two extended opcodes back to back. If the second is not emulated, its
'    operand bytes $7F $02 run as `ld a,a : ld (bc),a` and hit `victim`.
ASM
    ld bc, _victim
    ld a, 99
    nextreg $7F, 1
    nextreg $7F, 2
    nextreg $7F, 3
    ld d, 6
    ld e, 7
    mul d, e
    mul d, e
    ld a, e
    ld (_seen), a
END ASM
Ck(victim = 0)
Ck(seen = 0)            ' 6*7 = 42 -> D=0,E=42 -> 0*42 = 0

' 2. A page nothing was loaded into is still a page of its own.
POKE $6000, 11
ASM
    ld a, 77
    nextreg $53, a
    ld a, 123
    ld ($6000), a
    ld a, 11
    nextreg $53, a
    ld a, ($6000)
    ld (_seen), a
    ld a, 77
    nextreg $53, a
    ld a, ($6000)
    ld (_victim), a
    ld a, 11
    nextreg $53, a
END ASM
Ck(seen = 11)           ' the boot page kept its byte
Ck(victim = 123)        ' and page 77 kept its own
