' 32-bit and fixed-point operations whose operands come reversed call
' __SWAP32, which in the Python compiler's library leaves the low word of
' the operand in the stack below SP for four instructions; an interrupt
' there writes over it. Checks itself; irq_check.py runs it with an
' interrupt at every instruction of __SWAP32.
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
dim a as ulong = 121328125
dim b as ulong = 7800
dim one as ulong = 1
dim c as long = -121328125
dim d as long = 7800
dim lone as long = 1
dim f as fixed = 10.5
dim g as fixed = 1.5
dim fone as fixed = 1
dim r as ulong
dim s as long
dim h as fixed

r = (a + one) / (b + one) : Ck(r = 15552)
r = (a + one) mod (b + one) : Ck(r = 6974)
r = (a + one) - (b + one) : Ck(r = 121320325)
r = 121335926 / (b + one) : Ck(r = 15553)
s = (c - lone) / (d + lone) : Ck(s = -15552)
s = (lone - c) mod (d + lone) : Ck(s = 6974)
s = (d + lone) - (c - lone) : Ck(s = 121335927)
Ck((b + one) < (a + one))
Ck((d + lone) > (c - lone))
Ck((b + one) <= (a + one))
Ck((d + lone) >= (c - lone))
Ck(NOT ((a + one) < (b + one)))
h = (f + fone) / (g + fone) : Ck(h = 4.6)
h = (f + fone) - (g + fone) : Ck(h = 9)
