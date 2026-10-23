' Every way of getting PRIVATE wrong. Expected output: private_errors.err

SUB Entry()
    Helper(1)
    POKE uInteger 40002, @Helper
END SUB

CODEBANK 1
    PRIVATE SUB Helper(v as uByte)
        POKE 40001, v
    END SUB
END CODEBANK

CODEBANK 2
    SUB Other()
        Helper(2)
    END SUB
END CODEBANK

Entry()
Other()
