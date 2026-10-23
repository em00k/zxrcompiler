' PRIVATE outside a bank, and a DECLARE that disagrees with its definition.
DECLARE FUNCTION Loose(x as uByte) as uByte

PRIVATE SUB Nowhere()
END SUB

CODEBANK 1
    PRIVATE FUNCTION Loose(x as uByte) as uByte
        RETURN x
    END FUNCTION
END CODEBANK
