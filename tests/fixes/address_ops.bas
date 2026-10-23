' Comparisons, logic and shifts of addresses (@name). The Python compiler ends
' with InvalidOperatorError on the first two kinds and swaps the shifts.
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
DIM ga as uByte
DIM gb as uByte
DIM w as uInteger
Ck(@ga < 16384 = 0)
Ck(@ga > 16384)
Ck(@ga >= 32768)
Ck(@ga <= 65535)
Ck(@ga <> @gb)
Ck((@ga = @gb) = 0)
Ck(@ga = @ga)
Ck(@ga > 100 AND @gb > 100)
Ck((@ga < 100 OR @gb < 100) = 0)
w = @ga bAND $FF00
Ck(w = (@ga >> 8) << 8)
w = @ga bOR 1
Ck((w bAND 1) = 1)
w = @ga + 2 - 2
Ck(w = @ga)
w = @ga << 1
Ck(w = @ga * 2)
w = @ga >> 1
Ck(w = @ga / 2)
