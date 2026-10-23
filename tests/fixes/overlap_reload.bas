' x = 1 - x : F(1 - x). The word loaded from (_x - 1) has its high byte at
' _x; the -O3 optimiser keeps memory by the text of an address, spaces
' and all, so a store to _x did not change what it knew of (_x - 1) and
' the second load was deleted, leaving the old x in H. ModJukebox's L key.
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
dim loopmode as ubyte
dim other as ubyte = 7
dim got as ubyte
sub Take(v as ubyte)
    got = v
end sub
loopmode = 0
loopmode = 1 - loopmode : Take(1 - loopmode)
Ck(loopmode = 1) : Ck(got = 0)
loopmode = 1 - loopmode : Take(1 - loopmode)
Ck(loopmode = 0) : Ck(got = 1)
Ck(other = 7)
