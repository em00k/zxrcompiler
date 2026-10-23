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
_CLICK:
	DEFB 00, 00, 00, 00
_TotalMinutes:
	DEFB 00, 00, 00, 00, 00
.core.ZXBASIC_USER_DATA_END:
.core.__MAIN_PROGRAM__:
	ld hl, (_CLICK + 2)
	push hl
	ld hl, (_CLICK)
	push hl
	ld de, 0
	ld hl, 20
	call .core.__SWAP32
	call .core.__DIVU32
	call .core.__U32TOFREG
	ld hl, _TotalMinutes
	call .core.__STOREF
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
__ABS32:
	    bit 7, d
	    ret z
__NEG32: ; Negates DEHL (Two's complement)
	    ld a, l
	    cpl
	    ld l, a
	    ld a, h
	    cpl
	    ld h, a
	    ld a, e
	    cpl
	    ld e, a
	    ld a, d
	    cpl
	    ld d, a
	    inc l
	    ret nz
	    inc h
	    ret nz
	    inc de
	    ret
	    pop namespace
	    push namespace core
__DIVU32:    ; 32 bit unsigned division
	    exx
	    pop hl   ; return address
	    pop de   ; low part
	    ex (sp), hl ; CALLEE Convention ; H'L'D'E' => Dividend
__DIVU32START: ; Performs D'E'H'L' / HLDE
	    push de ; push Lowpart(Q)
	    ex de, hl	; DE = HL
	    ld hl, 0
	    exx
	    ld b, h
	    ld c, l
	    pop hl
	    push de
	    ex de, hl
	    ld hl, 0        ; H'L'HL = 0
	    exx
	    pop bc          ; Pop HightPart(B) => B = B'C'BC
	    exx
	    ld a, 32 ; Loop count
__DIV32LOOP:
	    sll c  ; B'C'BC << 1 ; Output most left bit to carry
	    rl  b
	    exx
	    rl c
	    rl b
	    exx
	    adc hl, hl
	    exx
	    adc hl, hl
	    exx
	    sbc hl,de
	    exx
	    sbc hl,de
	    exx
	    jp nc, __DIV32NOADD	; use JP inside a loop for being faster
	    add hl, de
	    exx
	    adc hl, de
	    exx
	    dec bc
__DIV32NOADD:
	    dec a
	    jp nz, __DIV32LOOP	; use JP inside a loop for being faster
	    push hl
	    exx
	    pop de
	    ex de, hl ; D'E'H'L' = 32 bits modulus
	    push bc
	    exx
	    pop de    ; DE = B'C'
	    ld h, b
	    ld l, c   ; DEHL = quotient D'E'H'L' = Modulus
	    ret     ; DEHL = quotient, D'E'H'L' = Modulus
__MODU32:    ; 32 bit modulus for 32bit unsigned division
	    exx
	    pop hl   ; return address
	    pop de   ; low part
	    ex (sp), hl ; CALLEE Convention ; H'L'D'E' => Dividend
	    call __DIVU32START	; At return, modulus is at D'E'H'L'
__MODU32START:
	    exx
	    push de
	    push hl
	    exx
	    pop hl
	    pop de
	    ret
__DIVI32:    ; 32 bit signed division
	    exx
	    pop hl   ; return address
	    pop de   ; low part
	    ex (sp), hl ; CALLEE Convention ; H'L'D'E' => Dividend
__DIVI32START:
	    exx
	    ld a, d	 ; Save sign
	    ex af, af'
	    bit 7, d ; Negative?
	    call nz, __NEG32 ; Negates DEHL
	    exx		; Now works with H'L'D'E'
	    ex af, af'
	    xor h
	    ex af, af'  ; Stores sign of the result for later
	    bit 7, h ; Negative?
	    ex de, hl ; HLDE = DEHL
	    call nz, __NEG32
	    ex de, hl
	    call __DIVU32START
	    ex af, af' ; Recovers sign
	    and 128	   ; positive?
	    ret z
	    jp __NEG32 ; Negates DEHL and returns from there
__MODI32:	; 32bits signed division modulus
	    exx
	    pop hl   ; return address
	    pop de   ; low part
	    ex (sp), hl ; CALLEE Convention ; H'L'D'E' => Dividend
	    call __DIVI32START
	    jp __MODU32START
	    pop namespace
	    push namespace core
__PISTOREF:	; Indect Stores a float (A, E, D, C, B) at location stored in memory, pointed by (IX + HL)
	    push de
	    ex de, hl	; DE <- HL
	    push ix
	    pop hl		; HL <- IX
	    add hl, de  ; HL <- IX + HL
	    pop de
__ISTOREF:  ; Load address at hl, and stores A,E,D,C,B registers at that address. Modifies A' register
	    ex af, af'
	    ld a, (hl)
	    inc hl
	    ld h, (hl)
	    ld l, a     ; HL = (HL)
	    ex af, af'
__STOREF:	; Stores the given FP number in A EDCB at address HL
	    ld (hl), a
	    inc hl
	    ld (hl), e
	    inc hl
	    ld (hl), d
	    inc hl
	    ld (hl), c
	    inc hl
	    ld (hl), b
	    ret
	    pop namespace
	    push namespace core
__SWAP32:
	    pop bc ; Return address
	    ex (sp), hl
	    inc sp
	    inc sp
	    ex de, hl
	    ex (sp), hl
	    ex de, hl
	    dec sp
	    dec sp
	    push bc
	    ret
	    pop namespace
	    push namespace core
__I8TOFREG:
	    ld l, a
	    rlca
	    sbc a, a	; A = SGN(A)
	    ld h, a
	    ld e, a
	    ld d, a
__I32TOFREG:	; Converts a 32bit signed integer (stored in DEHL)
	    ld a, d
	    or a		; Test sign
	    jp p, __U32TOFREG	; It was positive, proceed as 32bit unsigned
	    call __NEG32		; Convert it to positive
	    call __U32TOFREG	; Convert it to Floating point
	    set 7, e			; Put the sign bit (negative) in the 31bit of mantissa
	    ret
__U8TOFREG:
	    ld l, a
	    ld h, 0
	    ld e, h
	    ld d, h
__U32TOFREG:	; Converts an unsigned 32 bit integer (DEHL)
	    PROC
	    LOCAL __U32TOFREG_END
	    ld a, d
	    or e
	    or h
	    or l
	    ld b, d
	    ld c, e		; Returns 00 0000 0000 if ZERO
	    ret z
	    push de
	    push hl
	    exx
	    pop de  ; Loads integer into B'C' D'E'
	    pop bc
	    exx
	    ld l, 128	; Exponent
	    ld bc, 0	; DEBC = 0
	    ld d, b
	    ld e, c
__U32TOFREG_LOOP: ; Also an entry point for __F16TOFREG
	    exx
	    ld a, d 	; B'C'D'E' == 0 ?
	    or e
	    or b
	    or c
	    jp z, __U32TOFREG_END	; We are done
	    srl b ; Shift B'C' D'E' >> 1, output bit stays in Carry
	    rr c
	    rr d
	    rr e
	    exx
	    rr e ; Shift EDCB >> 1, inserting the carry on the left
	    rr d
	    rr c
	    rr b
	    inc l	; Increment exponent
	    jp __U32TOFREG_LOOP
__U32TOFREG_END:
	    exx
	    ld a, l     ; Puts the exponent in a
	    res 7, e	; Sets the sign bit to 0 (positive)
	    ret
	    ENDP
	    pop namespace
	END
