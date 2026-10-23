' W940: a FASTCALL routine has no stack frame, so BASIC code in it cannot
' read a parameter or keep a local. Expected output: lint_fastcall.err
FUNCTION fastcall Fine(x as uByte) as uByte
    ASM
        inc a
    END ASM
END FUNCTION

SUB fastcall ReadsParam(x as uByte)
    POKE 40000, x
END SUB

FUNCTION fastcall HasLocal(x as uByte) as uByte
    DIM y as uByte
    y = 3
    RETURN y
END FUNCTION

POKE 40001, Fine(1) + HasLocal(1)
ReadsParam(2)
