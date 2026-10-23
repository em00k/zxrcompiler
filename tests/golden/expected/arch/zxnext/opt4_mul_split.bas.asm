	org 32768
.core.__START_PROGRAM:
	di
	push iy
	ld iy, 0x5C3A  ; ZX Spectrum ROM variables address
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
	ld a, 40
	push af
	call _Card
.core.__END_PROGRAM:
	di
	ld hl, (.core.__CALL_BACK__)
	ld sp, hl
	pop iy
	ei
	ret
_Blit:
	push ix
	ld ix, 0
	add ix, sp
		pop     ix
		jp      $C0F0
_Blit__leave:
	ld sp, ix
	pop ix
	exx
	pop hl
	pop bc
	pop bc
	ex (sp), hl
	exx
	ret
_Glyph:
	push ix
	ld ix, 0
	add ix, sp
	ld a, (ix+9)
	push af
	ld l, (ix+4)
	ld h, (ix+5)
	ld de, 13
	call .core.__MUL16_FAST
	push hl
	ld a, (ix+7)
	push af
	call _Blit
_Glyph__leave:
	ld sp, ix
	pop ix
	exx
	pop hl
	pop bc
	pop bc
	ex (sp), hl
	exx
	ret
_Card:
	push ix
	ld ix, 0
	add ix, sp
	ld hl, 0
	push hl
	ld a, (ix+5)
	ld h, 13
	call .core.__MODU8_FAST
	ld (ix-1), a
	ld a, (ix+5)
	ld h, 13
	call .core.__DIVU8_FAST
	ld (ix-2), a
	ld a, 20
	cp (ix+5)
	jp nc, .LABEL.__LABEL0
	ld a, 7
	push af
	ld a, 9
	push af
	ld a, (ix-1)
	ld d, 9
	ld e, a
	mul d, e
	ld a, e
	add a, 117
	ld l, a
	ld h, 0
	push hl
	call _Glyph
	jp .LABEL.__LABEL1
.LABEL.__LABEL0:
	ld a, 7
	push af
	ld a, 9
	push af
	ld a, (ix-1)
	ld d, 9
	ld e, a
	mul d, e
	ld a, e
	ld l, a
	ld h, 0
	push hl
	call _Glyph
.LABEL.__LABEL1:
	ld a, 7
	push af
	push af
	ld a, (ix-2)
	ld d, 7
	ld e, a
	mul d, e
	ld a, e
	add a, 234
	ld l, a
	ld h, 0
	push hl
	call _Glyph
_Card__leave:
	ld sp, ix
	pop ix
	exx
	pop hl
	ex (sp), hl
	exx
	ret
	    push namespace core
__DIVU8:	; 8 bit unsigned integer division
	    pop hl	; --------------------------------
	    ex (sp), hl	; CALLEE
__DIVU8_FAST:	; Does A / H
	    ld l, h
	    ld h, a		; At this point do H / L
	    ld b, 8
	    xor a		; A = 0, Carry Flag = 0
__DIV8LOOP:
	    sla	h
	    rla
	    cp	l
	    jr	c, __DIV8NOSUB
	    sub	l
	    inc	h
__DIV8NOSUB:
	    djnz __DIV8LOOP
	    ld	l, a		; save remainder
	    ld	a, h		;
	    ret			; a = Quotient,
__DIVI8:		; 8 bit signed integer division Divides (Top of stack) / A
	    pop hl		; --------------------------------
	    ex (sp), hl
__DIVI8_FAST:
	    ld e, a		; store operands for later
	    ld c, h
	    or a		; negative?
	    jp p, __DIV8A
	    neg			; Make it positive
__DIV8A:
	    ex af, af'
	    ld a, h
	    or a
	    jp p, __DIV8B
	    neg
	    ld h, a		; make it positive
__DIV8B:
	    ex af, af'
	    call __DIVU8_FAST
	    ld a, c
	    xor l		; bit 7 of A = 1 if result is negative
	    ld a, h		; Quotient
	    ret p		; return if positive
	    neg
	    ret
__MODU8:		; 8 bit module. REturns A mod (Top of stack) (unsigned operands)
	    pop hl
	    ex (sp), hl	; CALLEE
__MODU8_FAST:	; __FASTCALL__ entry
	    call __DIVU8_FAST
	    ld a, l		; Remainder
	    ret		; a = Modulus
__MODI8:		; 8 bit module. REturns A mod (Top of stack) (For singed operands)
	    pop hl
	    ex (sp), hl	; CALLEE
__MODI8_FAST:	; __FASTCALL__ entry
	    call __DIVI8_FAST
	    ld a, l		; remainder
	    ret		; a = Modulus
	    pop namespace
	    push namespace core
__MUL16:	; Multiplies HL with the last value stored into de stack
	    PROC
	    ex de, hl
	    pop hl		; Return address
	    ex (sp), hl ; CALLEE caller convention
__MUL16_FAST:
	    ld a,d                      ; a = xh
	    ld d,h                      ; d = yh
	    ld h,a                      ; h = xh
	    ld c,e                      ; c = xl
	    ld b,l                      ; b = yl
	    mul d,e                     ; yh * yl
	    ex de,hl
	    mul d,e                     ; xh * yl
	    add hl,de                   ; add cross products
	    ld e,c
	    ld d,b
	    mul d,e                     ; yl * xl
	    ld a,l                      ; cross products lsb
	    add a,d                     ; add to msb final
	    ld h,a
	    ld l,e                      ; hl = final
	    ret	; Result in hl (16 lower bits)
	    ENDP
	    pop namespace
	END
