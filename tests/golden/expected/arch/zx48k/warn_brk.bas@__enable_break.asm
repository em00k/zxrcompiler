	org 32768
.core.__START_PROGRAM:
	di
	push ix
	push iy
	exx
	push hl
	exx
	ld (.core.__CALL_BACK__), sp
	ei
	jp .core.__MAIN_PROGRAM__
.core.__CALL_BACK__:
	DEFW 0
.core.ZXBASIC_USER_DATA:
.core.ZXBASIC_USER_DATA_LEN EQU .core.ZXBASIC_USER_DATA_END - .core.ZXBASIC_USER_DATA
	.core.__LABEL__.ZXBASIC_USER_DATA_LEN EQU .core.ZXBASIC_USER_DATA_LEN
	.core.__LABEL__.ZXBASIC_USER_DATA EQU .core.ZXBASIC_USER_DATA
.core.ZXBASIC_USER_DATA_END:
.core.__MAIN_PROGRAM__:
		push hl
	ld hl, 1
	call .core.CHECK_BREAK
	call _distance
	ld (0), a
		push hl
	ld hl, 6
	call .core.CHECK_BREAK
	ld hl, 0
	ld b, h
	ld c, l
.core.__END_PROGRAM:
	di
	ld hl, (.core.__CALL_BACK__)
	ld sp, hl
	exx
	pop hl
	exx
	pop iy
	pop ix
	ei
	ret
_distance:
	push ix
	ld ix, 0
	add ix, sp
	ld hl, 0
	push hl
	ld l, (ix-2)
	ld h, (ix-1)
	ld a, l
_distance__leave:
	ld sp, ix
	pop ix
	ret
	    push namespace core
	ERR_NR    EQU    23610    ; Error code system variable
	ERROR_Ok                EQU    -1
	ERROR_SubscriptWrong    EQU     2
	ERROR_OutOfMemory       EQU     3
	ERROR_OutOfScreen       EQU     4
	ERROR_NumberTooBig      EQU     5
	ERROR_InvalidArg        EQU     9
	ERROR_IntOutOfRange     EQU    10
	ERROR_NonsenseInBasic   EQU    11
	ERROR_InvalidFileName   EQU    14
	ERROR_InvalidColour     EQU    19
	ERROR_BreakIntoProgram  EQU    20
	ERROR_TapeLoadingErr    EQU    26
__ERROR:
	    ld (__ERROR_CODE), a
	    rst 8
__ERROR_CODE:
	    nop
	    ret
__STOP:
	    ld (ERR_NR), a
	    ret
	    pop namespace
	    push namespace core
CHECK_BREAK:
	    PROC
	    LOCAL PPC, TS_BRK, NO_BREAK
	    push af
	    call TS_BRK
	    jr c, NO_BREAK
	    ld (PPC), hl ; line num
	    ld a, ERROR_BreakIntoProgram
	    jp __ERROR   ; this stops the program and exits to BASIC
NO_BREAK:
	    pop af
	    pop hl       ; ret address
	    ex (sp), hl  ; puts it back into the stack and recovers initial HL
	    ret
	PPC EQU 23621
	TS_BRK EQU 8020
	    ENDP
	    pop namespace
	END
