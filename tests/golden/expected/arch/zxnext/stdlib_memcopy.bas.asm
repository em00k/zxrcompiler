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
	ld hl, 0
	ld b, h
	ld c, l
.core.__END_PROGRAM:
	di
	ld hl, (.core.__CALL_BACK__)
	ld sp, hl
	pop iy
	ei
	ret
_MemMove:
		push namespace core
		exx
		pop hl
		exx
		pop de
		pop bc
		exx
		push hl
		exx
		jp __MEMCPY
		pop namespace
_MemMove__leave:
	ret
_MemCopy:
		push namespace core
		exx
		pop hl
		exx
		pop de
		pop bc
		exx
		push hl
		exx
		ldir
		pop namespace
_MemCopy__leave:
	ret
_MemSet:
		push namespace core
		pop de
		pop af
		pop bc
		push de
		ld (hl),a
		dec bc
		ld a, b
		or c
		ret z
		ld d,h
		ld e,l
		inc de
		ldir
		pop namespace
_MemSet__leave:
	ret
	    push namespace core
__MEMCPY:
	    PROC
	    LOCAL __MEMCPY2
	    push hl
	    add hl, bc  ; addr of last source block byte + 1
	    or a
	    sbc hl, de  ; checks if DE > HL + BC
	    pop hl      ; recovers HL. If carry => DE > HL + BC (no overlap)
	    jr c, __MEMCPY2
	    sbc hl, de  ; Even if overlap, if DE < HL then we can LDIR safely
	    add hl, de
	    jr nc, __MEMCPY2
	    dec bc
	    add hl, bc
	    ex de, hl
	    add hl, bc
	    ex de, hl
	    inc bc      ; HL and DE point to the last byte position
	    lddr        ; Copies from end to beginning
	    ret
__MEMCPY2:
	    ldir
	    ret
	    ENDP
	    pop namespace
	END
