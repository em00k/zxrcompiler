	org 32768
.core.__START_PROGRAM:
	di
	push iy
	ld iy, 0x5C3A  ; ZX Spectrum ROM variables address
	ld (.core.__CALL_BACK__), sp
	ei
	call .core.__FAR_INIT
	jp .core.__MAIN_PROGRAM__
.core.__CALL_BACK__:
	DEFW 0
	.core.__FAR_MMU_REG EQU 83
	.core.__FAR_STACK_SIZE EQU 48
.core.__CODE_BANK_TABLE:
	DEFB 11, 30
.core.ZXBASIC_USER_DATA:
.core.ZXBASIC_USER_DATA_LEN EQU .core.ZXBASIC_USER_DATA_END - .core.ZXBASIC_USER_DATA
	.core.__LABEL__.ZXBASIC_USER_DATA_LEN EQU .core.ZXBASIC_USER_DATA_LEN
	.core.__LABEL__.ZXBASIC_USER_DATA EQU .core.ZXBASIC_USER_DATA
_r:
	DEFB 00
.core.ZXBASIC_USER_DATA_END:
.core.__MAIN_PROGRAM__:
	ld a, 2
	push af
	call _Nth
	ld (_r), a
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
_Nth:
		call .core.__FAR_CALL
		DEFB 1
		DEFW _Nth.__far
		CODEBANK 1
.LABEL._blob:
		defb 1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17
_Nth.__far:
	push ix
	ld ix, 0
	add ix, sp
	ld a, (ix+5)
	ld l, a
	ld h, 0
	push hl
	ld hl, _buf
	call .core.__ARRAY
	ld a, (hl)
_Nth__leave:
	ld sp, ix
	pop ix
	exx
	pop hl
	ex (sp), hl
	exx
	ret
		CODEBANK 0
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
	    push namespace core
__ARRAY_PTR:   ;; computes an array offset from a pointer
	    ld c, (hl)
	    inc hl
	    ld h, (hl)
	    ld l, c    ;; HL <-- [HL]
__ARRAY:
	    PROC
	    LOCAL LOOP
	    LOCAL ARRAY_END
	    LOCAL TMP_ARR_PTR            ; Ptr to Array DATA region. Stored temporarily
	    LOCAL LBOUND_PTR, UBOUND_PTR ; LBound and UBound PTR indexes
	    LOCAL RET_ADDR               ; Contains the return address popped from the stack
	    LOCAL __ARRAY_BUFFER
	LBOUND_PTR EQU __ARRAY_BUFFER ; 23698           ; Uses MEMBOT as a temporary variable
	UBOUND_PTR EQU LBOUND_PTR + 2  ; Next 2 bytes for UBOUND PTR
	RET_ADDR EQU UBOUND_PTR + 2    ; Next 2 bytes for RET_ADDR
	TMP_ARR_PTR EQU RET_ADDR + 2   ; Next 2 bytes for TMP_ARR_PTR
	    ld e, (hl)
	    inc hl
	    ld d, (hl)
	    inc hl      ; DE <-- PTR to Dim sizes table
	    ld (TMP_ARR_PTR), hl  ; HL = Array __DATA__.__PTR__
	    inc hl
	    inc hl
	    ld c, (hl)
	    inc hl
	    ld b, (hl)  ; BC <-- Array __LBOUND__ PTR
	    ld (LBOUND_PTR), bc  ; Store it for later
	    ex de, hl   ; HL <-- PTR to Dim sizes table, DE <-- dummy
	    ex (sp), hl	; Return address in HL, PTR Dim sizes table onto Stack
	    ld (RET_ADDR), hl ; Stores it for later
	    exx
	    pop hl		; Will use H'L' as the pointer to Dim sizes table
	    ld c, (hl)	; Loads Number of dimensions from (hl)
	    inc hl
	    ld b, (hl)
	    inc hl		; Ready
	    exx
	    ld hl, 0	; HL = Element Offset "accumulator"
LOOP:
	    ex de, hl   ; DE = Element Offset
	    ld hl, (LBOUND_PTR)
	    ld a, h
	    or l
	    ld b, h
	    ld c, l
	    jr z, 1f
	    ld c, (hl)
	    inc hl
	    ld b, (hl)
	    inc hl
	    ld (LBOUND_PTR), hl
1:
	    pop hl      ; Get next index (Ai) from the stack
	    sbc hl, bc  ; Subtract LBOUND
	    add hl, de	; Adds current index
	    exx			; Checks if B'C' = 0
	    ld a, b		; Which means we must exit (last element is not multiplied by anything)
	    or c
	    jr z, ARRAY_END		; if B'Ci == 0 we are done
	    dec bc				; Decrements loop counter
	    ld e, (hl)			; Loads next dimension size into D'E'
	    inc hl
	    ld d, (hl)
	    inc hl
	    push de
	    exx
	    pop de				; DE = Max bound Number (i-th dimension)
	    call __FNMUL        ; HL <= HL * DE mod 65536
	    jp LOOP
ARRAY_END:
	    ld a, (hl)
	    exx
	    LOCAL ARRAY_SIZE_LOOP
	    ex de, hl
	    ld hl, 0
	    ld b, a
ARRAY_SIZE_LOOP:
	    add hl, de
	    djnz ARRAY_SIZE_LOOP
	    ex de, hl
	    ld hl, (TMP_ARR_PTR)
	    ld a, (hl)
	    inc hl
	    ld h, (hl)
	    ld l, a
	    add hl, de  ; Adds element start
	    ld de, (RET_ADDR)
	    push de
	    ret
	    LOCAL __FNMUL, __FNMUL2
__FNMUL:
	    xor a
	    or h
	    jp nz, __MUL16_FAST
	    or l
	    ret z
	    cp 33
	    jp nc, __MUL16_FAST
	    ld b, l
	    ld l, h  ; HL = 0
__FNMUL2:
	    add hl, de
	    djnz __FNMUL2
	    ret
__ARRAY_BUFFER:
	    defs 8
	    ENDP
	    pop namespace
	    push namespace core
__FAR_CUR_BANK:     DEFB 0                  ; logical bank currently mapped (0 = resident)
__FAR_SP:           DEFW __FAR_STACK        ; shadow stack pointer
__FAR_STACK:        DEFS __FAR_STACK_SIZE   ; frames of [prev_bank][ret_lo][ret_hi]
__FAR_CALL:
	        push af                     ; SP = S-2
	        push hl                     ; SP = S-4
	        ld   hl, 4
	        add  hl, sp                 ; HL = S
	        push de                     ; SP = S-6
	        ld   e, (hl)
	        inc  hl
	        ld   d, (hl)                ; DE = payload ptr
	        dec  hl                     ; HL = S
	        ex   de, hl                 ; HL = payload ptr, DE = S
	        ld   a, (hl)                ; A = wanted logical bank
	        inc  hl                     ; HL = &target
	        push hl
	        ld   hl, __FAR_CUR_BANK
	        cp   (hl)
	        pop  hl                     ; (POP does not disturb the flags)
	        jr   nz, __FAR_CALL_SWITCH
	        ld   a, (hl)
	        inc  hl
	        ld   h, (hl)
	        ld   l, a                   ; HL = target
	        ex   de, hl                 ; DE = target, HL = S
	        ld   (hl), e
	        inc  hl
	        ld   (hl), d                ; overwrite the payload ptr with the target
	        pop  de
	        pop  hl
	        pop  af
	        ret                         ; -> target, caller ret now on top
__FAR_CALL_SWITCH:
	        push bc                     ; SP = S-8
	        ld   c, a                   ; C = wanted logical bank
	        ld   a, (hl)
	        inc  hl
	        ld   h, (hl)
	        ld   l, a                   ; HL = target
	        ex   de, hl                 ; DE = target, HL = S
	        ld   (hl), e
	        inc  hl
	        ld   (hl), d                ; overwrite the payload ptr with the target
	        inc  hl                     ; HL = &caller ret
	        ld   e, (hl)
	        inc  hl
	        ld   d, (hl)                ; DE = caller ret
	        push de
	        ld   de, __FAR_RETURN
	        ld   (hl), d
	        dec  hl
        ld   (hl), e                ; caller ret := __FAR_RETURN
	        pop  de                     ; DE = caller ret
	        ld   hl, (__FAR_SP)         ; push {previous bank, caller ret}
	        ld   a, (__FAR_CUR_BANK)
	        ld   (hl), a
	        inc  hl
	        ld   (hl), e
	        inc  hl
	        ld   (hl), d
	        inc  hl
	        ld   (__FAR_SP), hl
	        ld   a, c
	        ld   (__FAR_CUR_BANK), a
	        ld   hl, __CODE_BANK_TABLE
	        add  a, l
	        ld   l, a
	        adc  a, h
	        sub  l
	        ld   h, a
	        ld   a, (hl)                ; A = physical 8K page
	        nextreg __FAR_MMU_REG, a
	        pop  bc
	        pop  de
	        pop  hl
	        pop  af
	        ret                         ; -> target
__FAR_RETURN:
	        push hl                     ; SP = R-2, reserved for the real return address
	        push hl                     ; SP = R-4, the actual save of HL
	        push af
	        push de
	        push bc                     ; SP = R-10
	        ld   hl, (__FAR_SP)         ; pop {previous bank, caller ret}
	        dec  hl
	        ld   b, (hl)
	        dec  hl
	        ld   c, (hl)                ; BC = caller ret
	        dec  hl
	        ld   a, (hl)                ; A = previous logical bank
	        ld   (__FAR_SP), hl
	        ld   (__FAR_CUR_BANK), a
	        ld   hl, __CODE_BANK_TABLE
	        add  a, l
	        ld   l, a
	        adc  a, h
	        sub  l
	        ld   h, a
	        ld   a, (hl)
	        nextreg __FAR_MMU_REG, a
	        ld   hl, 8
	        add  hl, sp                 ; HL = R-2
	        ld   (hl), c
	        inc  hl
	        ld   (hl), b                ; fill in the reserved slot
	        pop  bc
	        pop  de
	        pop  af
	        pop  hl
	        ret                         ; -> the real caller, SP = R
__FAR_INIT:
	        ld   bc, 0x243B             ; TBBlue register select
	        ld   a, __FAR_MMU_REG
	        out  (c), a
	        inc  b                      ; BC = 0x253B, register access
	        in   a, (c)
	        ld   (__CODE_BANK_TABLE), a ; table[0] = the boot page for this window
	        xor  a
	        ld   (__FAR_CUR_BANK), a
	        ld   hl, __FAR_STACK
	        ld   (__FAR_SP), hl
	        ret
	    pop namespace
	CODEBANK 1
	_buf.__DATA__ EQU .LABEL._blob
_buf:
	DEFW .LABEL.__LABEL0
_buf.__DATA__.__PTR__:
	DEFW .LABEL._blob
	DEFW 0
	DEFW 0
.LABEL.__LABEL0:
	DEFW 0000h
	DEFB 01h
	CODEBANK 0
__EXIT_FUNCTION:
	ld sp, ix
	pop ix
	pop de
	add hl, sp
	ld sp, hl
	push de
	exx
	ret
	END
