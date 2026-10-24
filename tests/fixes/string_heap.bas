' Strings when the heap runs out. Build with a small heap (-H 256) and run
' with heap_check.py, which walks the heap afterwards. Both cases here run
' out of room on purpose:
'
' 1. k$ = "/" + k$ until the result no longer fits. __ADDSTR returned its
'    first operand when it could not allocate the result, so k$ came to
'    point at the literal "/" in the program, and the next assignment freed
'    it: the heap's free list then ran through program memory.
' 2. a$ = "<a literal>" with no room for the copy. __STRASSIGN returned the
'    address of the literal, which __STORE_STR stored in a$; the next
'    assignment to a$ then reallocated, and freed, program memory. (Between
'    two variables, a$ = b$ copies b$ to a new string first, and a$ is
'    given that, so a failure there only makes a$ empty.)
'
' With the fixes a concatenation that does not fit gives "" and an
' assignment that does not fit leaves a$ as it was; the heap stays whole.
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

DIM k$ AS STRING
DIM a$ AS STRING
DIM b$ AS STRING
DIM c$ AS STRING
DIM n AS UINTEGER
DIM most AS UINTEGER
DIM fails AS UINTEGER

' 1. Prepend until there is no room, many times over.
FOR n = 1 TO 400
    k$ = "/" + k$
    IF LEN(k$) > most THEN most = LEN(k$)
    IF LEN(k$) = 0 THEN fails = fails + 1
NEXT n
Ck(most > 50)                       ' it grew
Ck(fails > 0)                       ' and ran out of room at least once
Ck(LEN(k$) <= most)
Ck(LEN("/") = 1)                    ' the literal is still the literal

' 2. An assignment with no room for the copy.
k$ = ""
FOR n = 1 TO 100 : b$ = b$ + "b" : NEXT n
FOR n = 1 TO 60 : c$ = c$ + "c" : NEXT n
a$ = "a"
a$ = "LLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLLL"
Ck(a$ = "a")                        ' did not fit: a$ is as it was
b$ = ""
c$ = ""
a$ = "after"                        ' the heap is still usable
Ck(a$ = "after")
k$ = "/" + a$
Ck(k$ = "/after")
