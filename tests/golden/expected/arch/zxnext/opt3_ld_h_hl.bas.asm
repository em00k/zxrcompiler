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
	ld hl, 49152
	push hl
	call _Count
	ld bc, 0
.core.__END_PROGRAM:
	di
	ld hl, (.core.__CALL_BACK__)
	ld sp, hl
	pop iy
	ei
	ret
_Count:
	push ix
	ld ix, 0
	add ix, sp
	ld hl, 0
	push hl
	push hl
	inc sp
	ld l, (ix+4)
	ld h, (ix+5)
	ld de, 19
	add hl, de
	ld (ix-3), l
	ld (ix-2), h
	ld a, (hl)
	sub 58
	sub 1
	sbc a, a
	push af
	inc hl
	ld a, (hl)
	sub 47
	sub 1
	sbc a, a
	ld h, a
	pop af
	or a
	jr z, .LABEL.__LABEL4
	ld a, h
.LABEL.__LABEL4:
	or a
	jp z, .LABEL.__LABEL1
	ld l, (ix-3)
	ld h, (ix-2)
	inc hl
	inc hl
	inc hl
	ld (ix+4), l
	ld (ix+5), h
.LABEL.__LABEL1:
	ld (ix-1), 0
.LABEL.__LABEL2:
	ld a, (ix-1)
	cp 63
	sbc a, a
	ld l, (ix+4)
	ld h, (ix+5)
	ld h, (hl)
	or a
	jr z, .LABEL.__LABEL5
	ld a, h
.LABEL.__LABEL5:
	or a
	jp z, .LABEL.__LABEL3
	inc (ix-1)
	ld l, (ix+4)
	ld h, (ix+5)
	inc hl
	ld (ix+4), l
	ld (ix+5), h
	jp .LABEL.__LABEL2
.LABEL.__LABEL3:
	ld a, (ix-1)
	ld l, a
	ld h, 0
_Count__leave:
	ld sp, ix
	pop ix
	exx
	pop hl
	ex (sp), hl
	exx
	ret
	END
