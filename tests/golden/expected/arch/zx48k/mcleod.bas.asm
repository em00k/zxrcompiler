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
.core.ZXBASIC_USER_DATA_END:
.core.__MAIN_PROGRAM__:
	call .core.RND
	push bc
	push de
	push af
	ld a, 083h
	ld de, 00000h
	ld bc, 00000h
	call .core.__MULF
	call .core.__FTOU32REG
	ld (_a), hl
	ld (_a + 2), de
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
__MULF:	; Multiplication
	    call __FPSTACK_PUSH2
	    rst 28h
	    defb 04h	;
	    defb 38h;   ; END CALC
	    jp __FPSTACK_POP
	    pop namespace
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
__FTOU32REG:	; Converts a Float to (un)signed 32 bit integer (NOTE: It's ALWAYS 32 bit signed)
	    PROC
	    LOCAL __IS_FLOAT
	    LOCAL __NEGATE
	    or a
	    jr nz, __IS_FLOAT
	    ld h, c
	    ld l, d
	    ld d, e
	    ret
__IS_FLOAT:  ; Jumps here if it is a true floating point number
	    ld h, e
	    push hl  ; Stores it for later (Contains Sign in H)
	    push de
	    push bc
	    exx
	    pop de   ; Loads mantissa into C'B' E'D'
	    pop bc	 ;
	    set 7, c ; Highest mantissa bit is always 1
	    exx
	    ld hl, 0 ; DEHL = 0
	    ld d, h
	    ld e, l
	    sub 128  ; Exponent -= 128
	    jr z, __FTOU32REG_END	; If it was <= 128, we are done (Integers must be > 128)
	    jr c, __FTOU32REG_END	; It was decimal (0.xxx). We are done (return 0)
	    ld b, a  ; Loop counter = exponent - 128
__FTOU32REG_LOOP:
	    exx 	 ; Shift C'B' E'D' << 1, output bit stays in Carry
	    sla d
	    rl e
	    rl b
	    rl c
	    exx		 ; Shift DEHL << 1, inserting the carry on the right
	    rl l
	    rl h
	    rl e
	    rl d
	    djnz __FTOU32REG_LOOP
__FTOU32REG_END:
	    pop af   ; Take the sign bit
	    or a	 ; Sets SGN bit to 1 if negative
	    jp m, __NEGATE ; Negates DEHL
	    ret
__NEGATE:
	    exx
	    ld a, d
	    or e
	    or b
	    or c
	    exx
	    jr z, __END
	    inc l
	    jr nz, __END
	    inc h
	    jr nz, __END
	    inc de
	LOCAL __END
__END:
	    jp __NEG32
	    ENDP
__FTOU8:	; Converts float in C ED LH to Unsigned byte in A
	    call __FTOU32REG
	    ld a, l
	    ret
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
