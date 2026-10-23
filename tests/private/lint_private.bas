' W930: a banked routine that could be PRIVATE. Expected output: lint_private.err
CODEBANK 1
    SUB Inner(v as uByte)
        POKE 40001, v
    END SUB
    SUB Shared()
        Inner(1)
    END SUB
    SUB Pointed()
    END SUB
    SUB Pub()
        Inner(2)
        Shared()
        Pointed()
        POKE uInteger 40002, @Pointed
    END SUB
END CODEBANK
Pub()
Shared()
