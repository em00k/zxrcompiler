' Records: TYPE ... END TYPE, arrays of records and pointers to them
' (crates/zxr-syntax/src/records.rs). Checks itself in the far-call
' emulator; see tests/records/README.md.
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
' A comparison of two addresses is a traceback in the Python compiler, which
' compiles the lowered text of this file as a cross-check: compare here.
SUB CkEq(a as uInteger, b as uInteger)
    Ck(a = b)
END SUB

TYPE Point
    x, y AS INTEGER
END TYPE

TYPE Thing
    id AS UBYTE
    sb AS BYTE
    pos AS Point            ' a record inside a record
    hp AS UINTEGER
    big AS ULONG
    neg AS LONG
    fx AS FIXED
    fl AS FLOAT
    cells(1 TO 4) AS UBYTE
    grid(1, 2) AS UINTEGER
    next AS Thing PTR       ' a pointer to another
END TYPE

TYPE Head
    w, h AS UBYTE
    length AS UINTEGER
END TYPE

DIM one AS Thing
DIM many(3) AS Thing
DIM from1(1 TO 3) AS Thing
DIM two(1, 2) AS Thing
DIM i, j AS UBYTE
DIM t AS UINTEGER
DIM p AS Thing PTR
DIM hd AS Head PTR
DIM raw(9) AS UBYTE
DIM fl AS FLOAT
DIM f325 AS FLOAT = 3.25

SUB Hurt(e AS Thing PTR, by AS UBYTE)
    e->hp = e->hp - by
END SUB

FUNCTION Nth(n AS UBYTE) AS Thing PTR
    RETURN @many(n)
END FUNCTION

SUB Locals()
    DIM r AS Thing
    DIM rs(2) AS Thing
    DIM k AS UBYTE = 2
    r.hp = 555 : r.pos.y = -2
    rs(k).id = 7 : rs(k).cells(k) = 8
    Ck(r.hp = 555 AND r.pos.y = -2)
    Ck(rs(2).id = 7 AND rs(2).cells(2) = 8)
    Hurt(@r, 5)
    Ck(r.hp = 550)
END SUB

SUB Hide()
    DIM one AS UBYTE
    one = 3
    Ck(one = 3)
END SUB

' Sizes and layout: fields packed in order.
Ck(SIZEOF(Thing) = 43)
Ck(SIZEOF(Point) = 4)
CkEq(@one.hp - @one, 6)
CkEq(@one.next - @one, 41)

' Every field type.
one.id = 200 : one.sb = -5 : one.pos.x = -300 : one.pos.y = 1234
one.hp = 60000 : one.big = 3000000000 : one.neg = -70000
one.fx = 1.5 : one.fl = 3.25
Ck(one.id = 200)
Ck(one.sb = -5)
Ck(one.pos.x = -300)
Ck(one.pos.y = 1234)
Ck(one.hp = 60000)
Ck(one.big = 3000000000)
Ck(one.neg = -70000)
Ck(one.fx = 1.5)
' A FLOAT is compared by its bytes: arithmetic on it needs the ROM, which
' the emulator does not have.
fl = one.fl
Ck(PEEK(ULONG, @fl) = PEEK(ULONG, @f325) AND PEEK(@fl + 4) = PEEK(@f325 + 4))
Ck(PEEK(ULONG, @one.fl) = PEEK(ULONG, @f325))
Ck(PEEK(UINTEGER, @one + 6) = 60000)

' Array fields, by a number and by a variable.
FOR i = 1 TO 4 : one.cells(i) = i * 10 : NEXT i
Ck(one.cells(1) = 10)
Ck(one.cells(4) = 40)
i = 3 : Ck(one.cells(i) = 30)
FOR i = 0 TO 1
    FOR j = 0 TO 2
        one.grid(i, j) = i * 100 + j
    NEXT j
NEXT i
Ck(one.grid(1, 2) = 102)
i = 1 : j = 1 : Ck(one.grid(i, j) = 101)
CkEq(@one.grid(1, 0) - @one.grid(0, 0), 6)

' Arrays of records.
FOR i = 0 TO 3 : many(i).id = i + 1 : many(i).hp = 1000 * i : NEXT i
Ck(many(0).id = 1)
Ck(many(3).id = 4)
Ck(many(2).hp = 2000)
CkEq(@many(1) - @many(0), 43)
i = 2 : many(i).pos.y = 77
Ck(many(2).pos.y = 77)
from1(1).id = 11 : from1(3).id = 33
Ck(from1(1).id = 11 AND from1(3).id = 33)
CkEq(@from1(3) - @from1(1), 86)
two(1, 2).hp = 12345
Ck(two(1, 2).hp = 12345)
CkEq(@two(1, 2) - @two(0, 0), 5 * 43)

' Pointers.
p = @many(2)
Ck(p->id = 3)
p->hp = 4242 : Ck(many(2).hp = 4242)
p->pos.x = -1 : Ck(many(2).pos.x = -1)
p->cells(2) = 9 : Ck(many(2).cells(2) = 9)
many(0).next = @many(1) : many(1).next = @many(3) : many(3).next = 0
Ck(many(0).next->id = 2)
Ck(many(0).next->next->id = 4)
t = 0 : p = @many(0)
WHILE p
    t = t + p->id
    p = p->next
WEND
Ck(t = 7)

' A pointer parameter, and a FUNCTION that returns a pointer.
many(1).hp = 100 : Hurt(@many(1), 30)
Ck(many(1).hp = 70)
Hurt(@one, 1)
Ck(one.hp = 59999)
Ck(Nth(3)->id = 4)
Nth(1)->id = 99
Ck(many(1).id = 99)

' A pointer laid over memory nobody DIMmed as a record.
raw(0) = 16 : raw(1) = 8 : raw(2) = $34 : raw(3) = $12
hd = @raw(0)
Ck(hd->w = 16 AND hd->h = 8 AND hd->length = $1234)
hd->length = 7 : Ck(raw(2) = 7 AND raw(3) = 0)

' Assignments after THEN and ELSE.
i = 1
IF i = 1 THEN one.id = 5 ELSE one.id = 6
Ck(one.id = 5)
IF i = 2 THEN one.id = 5 ELSE one.id = 6
Ck(one.id = 6)
IF i = 1 THEN
    one.sb = 1
ELSE
    one.sb = 2
END IF
Ck(one.sb = 1)

Locals()
Hide()
