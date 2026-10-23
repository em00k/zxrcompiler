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
	DEFB 00, 00, 00, 00
_b:
	DEFB 00
.core.ZXBASIC_USER_DATA_END:
.core.__MAIN_PROGRAM__:
	ld hl, (_a + 2)
	push hl
	ld hl, (_a)
	push hl
	ld de, 0
	ld hl, 0
	call .core.__XOR32
	neg
	ld (_b), a
	ld hl, (_a + 2)
	push hl
	ld hl, (_a)
	push hl
	ld de, 0
	ld hl, 1
	call .core.__XOR32
	neg
	ld (_b), a
	ld hl, (_a)
	ld de, (_a + 2)
	ld bc, 0
	push bc
	ld bc, 0
	push bc
	call .core.__XOR32
	neg
	ld (_b), a
	ld hl, (_a)
	ld de, (_a + 2)
	ld bc, 0
	push bc
	ld bc, 1
	push bc
	call .core.__XOR32
	neg
	ld (_b), a
	ld hl, (_a + 2)
	push hl
	ld hl, (_a)
	push hl
	ld hl, (_a)
	ld de, (_a + 2)
	call .core.__XOR32
	neg
	ld (_b), a
	ld hl, (_a + 2)
	push hl
	ld hl, (_a)
	push hl
	ld hl, (_a)
	ld de, (_a + 2)
	call .core.__EQ32
	push af
	ld hl, (_a + 2)
	push hl
	ld hl, (_a)
	push hl
	ld hl, (_a)
	ld de, (_a + 2)
	call .core.__EQ32
	ld h, a
	pop af
	call .core.__XOR8
	neg
	ld (_b), a
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
__XOR16:
	    ld a, h
	    or l
	    ld h, a
	    ld a, d
	    or e
__XOR8:
	    sub 1
	    sbc a, a
	    ld l, a  ; l = 00h or FFh
	    ld a, h
	    sub 1
	    sbc a, a ; a = 00h or FFh
	    xor l
	    ret
	    pop namespace
	    push namespace core
__XOR32:
	    ld a, h
	    or l
	    or d
	    or e
	    ld c, a
	    pop hl  ; RET address
	    pop de
	    ex (sp), hl
	    ld a, h
	    or l
	    or d
	    or e
	    ld h, c
	    jp __XOR8
	    pop namespace
	    push namespace core
__EQ32:	; Test if 32bit value HLDE equals top of the stack
	    exx
	    pop bc ; Return address
	    exx
	    xor a	; Reset carry flag
	    pop bc
	    sbc hl, bc ; Low part
	    ex de, hl
	    pop bc
	    sbc hl, bc ; High part
	    exx
	    push bc ; CALLEE
	    exx
	    ld a, h
	    or l
	    or d
	    or e   ; a = 0 and Z flag set only if HLDE = 0
	    ld a, 1
	    ret z
	    xor a
	    ret
	    pop namespace
	END
