REM -O4 for the Next: the block after .LABEL1 is split off by the flow graph
REM and must stay a zxnext block, or the `ld d, 7` before `mul d, e` is
REM deleted (the split block's cells were rebuilt as z80 ones, which think
REM mul reads nothing). Look for `ld d, 7` after the second label.
function Blit(byval x as ubyte, byval w as uinteger, byval h as ubyte) as ubyte
    asm
        pop     ix
        jp      $C0F0
    end asm
end function
sub Glyph(byval col as uinteger, byval w as ubyte, byval h as ubyte)
    Blit(w, col * 13, h)
end sub
sub Card(byval c as ubyte)
    dim r, s as ubyte
    r = c mod 13
    s = c / 13
    if c > 20
        Glyph(117 + r * 9, 9, 7)
    else
        Glyph(r * 9, 9, 7)
    end if
    Glyph(234 + s * 7, 7, 7)
end sub
Card(40)
