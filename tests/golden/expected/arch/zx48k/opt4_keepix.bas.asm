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
	call _test
	ld bc, 0
.core.__END_PROGRAM:
	di
	ld hl, (.core.__CALL_BACK__)
	ld sp, hl
	exx
	pop hl
	pop iy
	pop ix
	exx
	ei
	ret
_test:
	push ix
	ld ix, 0
	add ix, sp
	ld hl, 0
	push hl
	push hl
	push hl
	inc sp
	call .core.RND
	push bc
	push de
	push af
	ld a, 083h
	ld de, 00020h
	ld bc, 00000h
	call .core.__MULF
	ld hl, -5
	call .core.__PSTOREF
	push ix
	pop hl
	ld de, -5
	add hl, de
	call .core.__PLOADF
	push bc
	push de
	push af
	ld a, 081h
	ld de, 00000h
	ld bc, 00000h
	call .core.__ADDF
	ld hl, -5
	call .core.__PSTOREF
_test__leave:
	ld sp, ix
	pop ix
	ret
	    push namespace core
	__FPSTACK_PUSH EQU 2AB6h	; Stores an FP number into the ROM FP stack (A, ED CB)
	__FPSTACK_POP  EQU 2BF1h	; Pops an FP number out of the ROM FP stack (A, ED CB)
__FPSTACK_PUSH2: ; Pushes Current A ED CB registers and top of the stack on (SP + 4)
	    call __FPSTACK_PUSH ; Pushes A ED CB into the FP-STACK
	    exx
	    pop hl       ; Caller-Caller return addr
	    exx
	    pop hl       ; Caller return addr
	    pop af
	    pop de
	    pop bc
	    push hl      ; Caller return addr
	    exx
	    push hl      ; Caller-Caller return addr
	    exx
	    jp __FPSTACK_PUSH
__FPSTACK_I16:	; Pushes 16 bits integer in HL into the FP ROM STACK
	    ld a, h
	    rla			; sign to Carry
	    sbc	a, a	; 0 if positive, FF if negative
	    ld e, a
	    ld d, l
	    ld c, h
	    xor a
	    ld b, a
	    jp __FPSTACK_PUSH
	    pop namespace
	    push namespace core
__ADDF:	; Addition
	    call __FPSTACK_PUSH2
	    rst 28h
	    defb 0fh	; ADD
	    defb 38h;   ; END CALC
	    jp __FPSTACK_POP
	    pop namespace
	    push namespace core
__MULF:	; Multiplication
	    call __FPSTACK_PUSH2
	    rst 28h
	    defb 04h	;
	    defb 38h;   ; END CALC
	    jp __FPSTACK_POP
	    pop namespace
	    push namespace core
__ILOADF:
	    ld a, (hl)
	    inc hl
	    ld h, (hl)
	    ld l, a
__LOADF:    ; Loads a 40 bits FP number from address pointed by HL
	    ld a, (hl)
	    inc hl
	    ld e, (hl)
	    inc hl
	    ld d, (hl)
	    inc hl
	    ld c, (hl)
	    inc hl
	    ld b, (hl)
	    ret
	    pop namespace
	    push namespace core
__PLOADF:
	    push ix
	    pop hl
	    add hl, de
	    jp __LOADF
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
__PSTOREF:
	    push de
	    ex de, hl  ; DE <- HL
	    push ix
	    pop hl	   ; HL <- IX
	    add hl, de ; HL <- IX + DE
	    pop de
	    jp __STOREF
	    pop namespace
	    push namespace core
RANDOMIZE:
	    PROC
	    LOCAL TAKE_FRAMES
	    LOCAL FRAMES
	    ld a, h
	    or l
	    or d
	    or e
	    jr z, TAKE_FRAMES
	    ld (RANDOM_SEED_LOW), hl
	    ld (RANDOM_SEED_HIGH), de
	    ret
TAKE_FRAMES:
	    ld hl, (FRAMES)
	    ld (RANDOM_SEED_LOW), hl
	    ld hl, (FRAMES + 2)
	    ld (RANDOM_SEED_HIGH), hl
	    ret
	FRAMES EQU    23672
	    ENDP
	RANDOM_SEED_HIGH EQU RAND+1 ; RANDOM seed, 16 higher bits
	RANDOM_SEED_LOW  EQU 23670  ; RANDOM seed, 16 lower bits
RAND:
	    PROC
	    ld  de,0C0DEh   ; yw -> zt
	    ld  hl,(RANDOM_SEED_LOW)   ; xz -> yw
	    ld  (RANDOM_SEED_LOW),de  ; x = y, z = w
	    ld  a,e         ; w = w ^ ( w << 3 )
	    add a,a
	    add a,a
	    add a,a
	    xor e
	    ld  e,a
	    ld  a,h         ; t = x ^ (x << 1)
	    add a,a
	    xor h
	    ld  d,a
	    rra             ; t = t ^ (t >> 1) ^ w
	    xor d
	    xor e
	    ld  d,l         ; y = z
	    ld  e,a         ; w = t
	    ld  (RANDOM_SEED_HIGH),de
	    ret
	    ENDP
RND:
	    PROC
	    LOCAL RND_LOOP
	    call RAND
	    ld b, h
	    ld c, l
	    ld a, e
	    or d
	    or c
	    or b
	    ret z   ; Returns 0 if BC=DE=0
	    ld l, 81h    ; Exponent
	    ld a, e ; Use A register for rotating E faster (using RLA instead of RL E)
RND_LOOP:
	    dec l
	    sla b
	    rl c
	    rl d
	    rla
	    jp nc, RND_LOOP
	    ccf ; Clears carry to insert a 0 bit back into mantissa -> positive FP number
	    rra
	    rr d
	    rr c
	    rr b
	    ld e, a     ; E must have the highest byte
	    ld a, l     ; exponent in A
	    ret
	    ENDP
	    pop namespace
	END
