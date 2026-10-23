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
_a:
	DEFB 00, 00
_b:
	DEFB 00, 00
.core.ZXBASIC_USER_DATA_END:
.core.__MAIN_PROGRAM__:
	ld hl, (_a)
	ld (_b), hl
	ld hl, (_a)
	srl h
	rr l
	ld (_b), hl
	ld hl, (_a)
	ld de, 4
	call .core.__DIVU16
	ld (_b), hl
	ld hl, 0
	ld (_b), hl
	ld hl, 1
	ld de, (_a)
	call .core.__DIVU16
	ld (_b), hl
	ld hl, 2
	ld de, (_a)
	call .core.__DIVU16
	ld (_b), hl
	ld hl, 4
	ld de, (_a)
	call .core.__DIVU16
	ld (_b), hl
	ld hl, (_a)
	ld de, (_a)
	call .core.__DIVU16
	ld (_b), hl
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
	    push namespace core
__ABS16:
	    bit 7, h
	    ret z
__NEGHL:
	    ld a, l			; HL = -HL
	    cpl
	    ld l, a
	    ld a, h
	    cpl
	    ld h, a
	    inc hl
	    ret
	    pop namespace
	    push namespace core
__DIVU16:    ; 16 bit unsigned division
__DIVU16_FAST:
	    ld a, h
	    ld c, l
	    ld hl, 0
	    ld b, 16
__DIV16LOOP:
	    sll c
	    rla
	    adc hl,hl
	    sbc hl,de
	    jr  nc, __DIV16NOADD
	    add hl,de
	    dec c
__DIV16NOADD:
	    djnz __DIV16LOOP
	    ex de, hl
	    ld h, a
	    ld l, c
	    ret     ; HL = quotient, DE = Mudulus
__MODU16:    ; 16 bit modulus
	    call __DIVU16_FAST
	    ex de, hl	; hl = reminder (modulus)
	    ret
__DIVI16:	; 16 bit signed division
__DIVI16_FAST:
	    ld a, d
	    xor h
	    ex af, af'		; BIT 7 of a contains result
	    bit 7, d		; DE is negative?
	    jr z, __DIVI16A
	    ld a, e			; DE = -DE
	    cpl
	    ld e, a
	    ld a, d
	    cpl
	    ld d, a
	    inc de
__DIVI16A:
	    bit 7, h		; HL is negative?
	    call nz, __NEGHL
__DIVI16B:
	    call __DIVU16_FAST
	    ex af, af'
	    or a
	    ret p	; return if positive
	    jp __NEGHL
__MODI16:    ; 16 bit modulus
	    call __DIVI16_FAST
	    ex de, hl	; hl = reminder (modulus)
	    ret
	    pop namespace
	END
