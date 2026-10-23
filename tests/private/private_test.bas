' ---------------------------------------------------------------
' private_test - PRIVATE routines in a CODEBANK.
'
' A PRIVATE SUB or FUNCTION has no resident trampoline: its body sits in
' the bank under its ordinary label and is called directly, which only
' works while that bank is paged in. This runs them for real: every
' calling convention, results of every width, recursion, and a private
' routine that far-calls out of its bank and comes back.
'
'   python3 run_far.py <bin> <banks.json> <map> 32768 private_test.bas
' ---------------------------------------------------------------
'!org=32768
'!opt=4
'!codebank=40

#define RESULTS 61440          ' $F000, clear of the program and the heap

DIM t_run as uInteger          ' checks executed
DIM t_fail as uInteger         ' of those, failed

SUB Ck(ok as uByte)
    POKE RESULTS + t_run, ok
    IF ok = 0 THEN
        t_fail = t_fail + 1
    END IF
    t_run = t_run + 1
END SUB

DIM private as uByte = 7       ' the word is still an ordinary name
DIM mark as uInteger

' Declared ahead, outside the bank: the definition decides where it lives.
DECLARE PRIVATE FUNCTION Fact(n as uByte) as uLong

CODEBANK 2
    FUNCTION Ten() as uByte
        RETURN 10
    END FUNCTION
END CODEBANK

CODEBANK 1
    DIM local as uInteger = 500

    PRIVATE SUB Mark(v as uInteger)
        mark = v + local
    END SUB

    PRIVATE FUNCTION Add(a as uByte, b as uByte) as uByte
        RETURN a + b
    END FUNCTION

    ' Mixed case, and fastcall: the argument arrives in HL, with no frame.
    Private Function fastcall Twice(x as uInteger) as uInteger
        ASM
            add hl, hl
        END ASM
    END FUNCTION

    PRIVATE FUNCTION Fact(n as uByte) as uLong
        IF n < 2 THEN RETURN 1
        RETURN n * Fact(n - 1)
    END FUNCTION

    ' Over 11 bytes of parameters: leaves through the resident
    ' __EXIT_FUNCTION.
    PRIVATE FUNCTION Sum(a as uLong, b as uLong, c as uLong, d as uLong) as uLong
        RETURN a + b + c + d
    END FUNCTION

    PRIVATE FUNCTION Name$(n as uByte) as String
        IF n = 1 THEN RETURN "one"
        RETURN "many"
    END FUNCTION

    ' Out of the bank through another bank's trampoline, and back.
    PRIVATE FUNCTION ViaTen(x as uByte) as uByte
        RETURN Add(Ten(), x)
    END FUNCTION

    FUNCTION RunMark() as uInteger
        Mark(11)
        RETURN mark
    END FUNCTION

    FUNCTION RunAdd() as uByte
        RETURN Add(private, 3)
    END FUNCTION

    FUNCTION RunTwice() as uInteger
        RETURN Twice(Twice(300))
    END FUNCTION

    FUNCTION RunFact() as uLong
        RETURN Fact(10)
    END FUNCTION

    FUNCTION RunSum() as uLong
        RETURN Sum(100000, 200000, 300000, 400000)
    END FUNCTION

    FUNCTION RunName() as uByte
        RETURN (Name$(1) = "one") AND (Name$(2) = "many")
    END FUNCTION

    FUNCTION RunVia() as uByte
        RETURN ViaTen(5)
    END FUNCTION

    ' The address of a private routine is good inside its bank.
    FUNCTION RunAddress() as uByte
        DIM p as uInteger
        p = @Mark
        mark = 0
        ASM
            ld hl, 77
            push hl
            ld hl, back
            push hl
            ld l, (ix-2)
            ld h, (ix-1)
            jp (hl)
        back:
        END ASM
        RETURN (mark = 577)
    END FUNCTION
END CODEBANK

Ck(RunMark() = 511)
Ck(RunAdd() = 10)
Ck(RunTwice() = 1200)
Ck(RunFact() = 3628800)
Ck(RunSum() = 1000000)
Ck(RunName() <> 0)
Ck(RunVia() = 15)
Ck(RunAddress() <> 0)
Ck(private = 7)
