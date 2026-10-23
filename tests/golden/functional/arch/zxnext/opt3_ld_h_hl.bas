' The loop test reads a byte through a parameter: ld l,(ix+4) / ld h,(ix+5) /
' ld h,(hl). The second load is not a dead store, though the third writes H.
function Count(byval p as uinteger) as uinteger
    dim q as uinteger
    dim n as ubyte
    q = p + 19
    if peek(q) = 58 and peek(q + 1) = 47 then p = q + 3
    n = 0
    while n < 63 and peek(p) <> 0
        n = n + 1
        p = p + 1
    wend
    return n
end function
dim r as uinteger
r = Count($C000)
