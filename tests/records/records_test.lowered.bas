#line 1 "records_test.bas"




#line 6 "records_test.bas"
DIM t_run as uInteger
DIM t_fail as uInteger
SUB Ck(ok as uByte)
    POKE 61440 + t_run, ok
    IF ok = 0 THEN
        t_fail = t_fail + 1
    END IF
    t_run = t_run + 1
END SUB


SUB CkEq(a as uInteger, b as uInteger)
    Ck(a = b)
END SUB
























DIM one(0 TO 42) AS UBYTE
DIM many(0 TO 3, 0 TO 42) AS UBYTE
DIM from1(1 TO 3, 0 TO 42) AS UBYTE
DIM two(0 TO 1, 0 TO 2, 0 TO 42) AS UBYTE
DIM i, j AS UBYTE
DIM t AS UINTEGER
DIM p AS UINTEGER
DIM hd AS UINTEGER
DIM raw(9) AS UBYTE
DIM fl AS FLOAT
DIM f325 AS FLOAT = 3.25

SUB Hurt(BYVAL e AS UINTEGER, by AS UBYTE)
    POKE UINTEGER e + 6, PEEK(UINTEGER, e + 6) - by
END SUB

FUNCTION Nth(n AS UBYTE) AS UINTEGER
    RETURN (@many(0, 0) + (CAST(UINTEGER, n)) * 43)
END FUNCTION

SUB Locals()
    DIM r(0 TO 42) AS UBYTE
    DIM rs(0 TO 2, 0 TO 42) AS UBYTE
    DIM k AS UBYTE = 2
    POKE UINTEGER @r(6), 555 : POKE INTEGER @r(4), -2
    POKE UBYTE @rs(k, 0), 7 : POKE UBYTE @rs(k, 25 + (CAST(UINTEGER, k) - 1)), 8
    Ck(PEEK(UINTEGER, @r(6)) = 555 AND PEEK(INTEGER, @r(4)) = -2)
    Ck(PEEK(UBYTE, @rs(2, 0)) = 7 AND PEEK(UBYTE, @rs(2, 26)) = 8)
    Hurt((@r(0)), 5)
    Ck(PEEK(UINTEGER, @r(6)) = 550)
END SUB

SUB Hide()
    DIM one AS UBYTE
    one = 3
    Ck(one = 3)
END SUB


Ck(43 = 43)
Ck(4 = 4)
CkEq((@one(0) + 6) - (@one(0)), 6)
CkEq((@one(0) + 41) - (@one(0)), 41)


POKE UBYTE @one(0), 200 : POKE BYTE @one(0) + 1, -5 : POKE INTEGER @one(0) + 2, -300 : POKE INTEGER @one(0) + 4, 1234
POKE UINTEGER @one(0) + 6, 60000 : POKE ULONG @one(0) + 8, 3000000000 : POKE LONG @one(0) + 12, -70000
POKE FIXED @one(0) + 16, 1.5 : POKE FLOAT @one(0) + 20, 3.25
Ck(PEEK(UBYTE, @one(0)) = 200)
Ck(PEEK(BYTE, @one(0) + 1) = -5)
Ck(PEEK(INTEGER, @one(0) + 2) = -300)
Ck(PEEK(INTEGER, @one(0) + 4) = 1234)
Ck(PEEK(UINTEGER, @one(0) + 6) = 60000)
Ck(PEEK(ULONG, @one(0) + 8) = 3000000000)
Ck(PEEK(LONG, @one(0) + 12) = -70000)
Ck(PEEK(FIXED, @one(0) + 16) = 1.5)


fl = PEEK(FLOAT, @one(0) + 20)
Ck(PEEK(ULONG, @fl) = PEEK(ULONG, @f325) AND PEEK(@fl + 4) = PEEK(@f325 + 4))
Ck(PEEK(ULONG, (@one(0) + 20)) = PEEK(ULONG, @f325))
Ck(PEEK(UINTEGER, (@one(0)) + 6) = 60000)


FOR i = 1 TO 4 : POKE UBYTE @one(0) + 25 + (CAST(UINTEGER, i) - 1), i * 10 : NEXT i
Ck(PEEK(UBYTE, @one(0) + 25) = 10)
Ck(PEEK(UBYTE, @one(0) + 28) = 40)
i = 3 : Ck(PEEK(UBYTE, @one(0) + 25 + (CAST(UINTEGER, i) - 1)) = 30)
FOR i = 0 TO 1
    FOR j = 0 TO 2
        POKE UINTEGER @one(0) + 29 + (((CAST(UINTEGER, i)) * 3 + CAST(UINTEGER, j)) << 1), i * 100 + j
    NEXT j
NEXT i
Ck(PEEK(UINTEGER, @one(0) + 39) = 102)
i = 1 : j = 1 : Ck(PEEK(UINTEGER, @one(0) + 29 + (((CAST(UINTEGER, i)) * 3 + CAST(UINTEGER, j)) << 1)) = 101)
CkEq((@one(0) + 35) - (@one(0) + 29), 6)


FOR i = 0 TO 3 : POKE UBYTE @many(0, 0) + (CAST(UINTEGER, i)) * 43, i + 1 : POKE UINTEGER @many(0, 0) + 6 + (CAST(UINTEGER, i)) * 43, 1000 * i : NEXT i
Ck(PEEK(UBYTE, @many(0, 0)) = 1)
Ck(PEEK(UBYTE, @many(0, 0) + 129) = 4)
Ck(PEEK(UINTEGER, @many(0, 0) + 92) = 2000)
CkEq((@many(0, 0) + 43) - (@many(0, 0)), 43)
i = 2 : POKE INTEGER @many(0, 0) + 4 + (CAST(UINTEGER, i)) * 43, 77
Ck(PEEK(INTEGER, @many(0, 0) + 90) = 77)
POKE UBYTE @from1(1, 0), 11 : POKE UBYTE @from1(1, 0) + 86, 33
Ck(PEEK(UBYTE, @from1(1, 0)) = 11 AND PEEK(UBYTE, @from1(1, 0) + 86) = 33)
CkEq((@from1(1, 0) + 86) - (@from1(1, 0)), 86)
POKE UINTEGER @two(0, 0, 0) + 221, 12345
Ck(PEEK(UINTEGER, @two(0, 0, 0) + 221) = 12345)
CkEq((@two(0, 0, 0) + 215) - (@two(0, 0, 0)), 5 * 43)


p = (@many(0, 0) + 86)
Ck(PEEK(UBYTE, p) = 3)
POKE UINTEGER p + 6, 4242 : Ck(PEEK(UINTEGER, @many(0, 0) + 92) = 4242)
POKE INTEGER p + 2, -1 : Ck(PEEK(INTEGER, @many(0, 0) + 88) = -1)
POKE UBYTE p + 26, 9 : Ck(PEEK(UBYTE, @many(0, 0) + 112) = 9)
POKE UINTEGER @many(0, 0) + 41, (@many(0, 0) + 43) : POKE UINTEGER @many(0, 0) + 84, (@many(0, 0) + 129) : POKE UINTEGER @many(0, 0) + 170, 0
Ck(PEEK(UBYTE, (PEEK(UINTEGER, @many(0, 0) + 41))) = 2)
Ck(PEEK(UBYTE, (PEEK(UINTEGER, (PEEK(UINTEGER, @many(0, 0) + 41)) + 41))) = 4)
t = 0 : p = (@many(0, 0))
WHILE p
    t = t + PEEK(UBYTE, p)
    p = PEEK(UINTEGER, p + 41)
WEND
Ck(t = 7)


POKE UINTEGER @many(0, 0) + 49, 100 : Hurt((@many(0, 0) + 43), 30)
Ck(PEEK(UINTEGER, @many(0, 0) + 49) = 70)
Hurt((@one(0)), 1)
Ck(PEEK(UINTEGER, @one(0) + 6) = 59999)
Ck(PEEK(UBYTE, (Nth(3))) = 4)
POKE UBYTE (Nth(1)), 99
Ck(PEEK(UBYTE, @many(0, 0) + 43) = 99)


raw(0) = 16 : raw(1) = 8 : raw(2) = $34 : raw(3) = $12
hd = @raw(0)
Ck(PEEK(UBYTE, hd) = 16 AND PEEK(UBYTE, hd + 1) = 8 AND PEEK(UINTEGER, hd + 2) = $1234)
POKE UINTEGER hd + 2, 7 : Ck(raw(2) = 7 AND raw(3) = 0)


i = 1
IF i = 1 THEN POKE UBYTE @one(0), 5 ELSE POKE UBYTE @one(0), 6
Ck(PEEK(UBYTE, @one(0)) = 5)
IF i = 2 THEN POKE UBYTE @one(0), 5 ELSE POKE UBYTE @one(0), 6
Ck(PEEK(UBYTE, @one(0)) = 6)
IF i = 1 THEN
    POKE BYTE @one(0) + 1, 1
ELSE
    POKE BYTE @one(0) + 1, 2
END IF
Ck(PEEK(BYTE, @one(0) + 1) = 1)

Locals()
Hide()
