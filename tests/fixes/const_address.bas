' PEEK and POKE at an address that is a constant expression, `@g(0) + k`.
' The Python backend popped an address nobody pushed for PEEK(FIXED, ...),
' PEEK(FLOAT, ...) and POKE FLOAT, and for a 32-bit or FIXED PEEK as the
' second operand of an operation: the stack went a word out. Every type, at
' -O0, -O2 and -O4. A FLOAT is compared by its bytes, since arithmetic on it
' needs the ROM, which the emulator does not have. Checks itself in the
' far-call emulator; see tests/private/README.md.
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
DIM g(0 TO 31) AS UBYTE
DIM b AS BYTE
DIM ub AS UBYTE
DIM i AS INTEGER
DIM ui AS UINTEGER
DIM l AS LONG
DIM ul AS ULONG
DIM fx AS FIXED
DIM fx2 AS FIXED = 2.25
DIM fl AS FLOAT
DIM f325 AS FLOAT = 3.25
DIM k AS UBYTE

POKE BYTE @g(0) + 1, -3 : b = PEEK(BYTE, @g(0) + 1) : Ck(b = -3)
POKE UBYTE @g(0) + 2, 200 : ub = PEEK(UBYTE, @g(0) + 2) : Ck(ub = 200)
POKE INTEGER @g(0) + 3, -1000 : i = PEEK(INTEGER, @g(0) + 3) : Ck(i = -1000)
POKE UINTEGER @g(0) + 5, 60000 : ui = PEEK(UINTEGER, @g(0) + 5) : Ck(ui = 60000)
POKE LONG @g(0) + 7, -70000 : l = PEEK(LONG, @g(0) + 7) : Ck(l = -70000)
POKE ULONG @g(0) + 11, 3000000000 : ul = PEEK(ULONG, @g(0) + 11) : Ck(ul = 3000000000)
POKE FIXED @g(0) + 15, 1.5 : fx = PEEK(FIXED, @g(0) + 15) : Ck(fx = 1.5)
POKE FLOAT @g(0) + 19, 3.25
Ck(PEEK(ULONG, @g(0) + 19) = PEEK(ULONG, @f325) AND g(23) = PEEK(@f325 + 4))
fl = PEEK(FLOAT, @g(0) + 19)
Ck(PEEK(ULONG, @fl) = PEEK(ULONG, @f325) AND PEEK(@fl + 4) = PEEK(@f325 + 4))
POKE FLOAT @g(0) + 19, fl
Ck(PEEK(ULONG, @g(0) + 19) = PEEK(ULONG, @f325))

' As either operand of an operation.
ul = 5
Ck(ul + PEEK(ULONG, @g(0) + 11) = 3000000005)
Ck(PEEK(ULONG, @g(0) + 11) + ul = 3000000005)
Ck(PEEK(ULONG, @g(0) + 11) - ul = 2999999995)
Ck(PEEK(LONG, @g(0) + 7) < l + 1)
fx = 1
Ck(fx + PEEK(FIXED, @g(0) + 15) = 2.5)
Ck(PEEK(FIXED, @g(0) + 15) + fx = 2.5)
Ck(PEEK(FIXED, @g(0) + 15) - fx = 0.5)
Ck(fx2 - PEEK(FIXED, @g(0) + 15) = 0.75)
Ck(PEEK(FIXED, @g(0) + 15) < fx2)

' In a loop, where a word too many on the stack would show.
FOR k = 1 TO 50
    fx = PEEK(FIXED, @g(0) + 15)
    fl = PEEK(FLOAT, @g(0) + 19)
    POKE FLOAT @g(0) + 19, fl
NEXT k
Ck(fx = 1.5)
