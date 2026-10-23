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
_fp:
	DEFB 00, 00, 00, 00
_near:
	DEFB 00
.core.ZXBASIC_USER_DATA_END:
.core.__MAIN_PROGRAM__:
	ld hl, ((.LABEL._tbl.__faraddr) + 65536) & 0xFFFF
	ld de, ((.LABEL._tbl.__faraddr) + 65536) >> 16
	ld (_fp), hl
	ld (_fp + 2), de
	ld hl, (((.LABEL._tbl.__faraddr) + 65536) + (2)) & 0xFFFF
	ld de, (((.LABEL._tbl.__faraddr) + 65536) + (2)) >> 16
	ld (_fp), hl
	ld (_fp + 2), de
	ld hl, ((_lut.__DATA__.__faraddr) + 65536) & 0xFFFF
	ld de, ((_lut.__DATA__.__faraddr) + 65536) >> 16
	ld (_fp), hl
	ld (_fp + 2), de
	ld hl, ((_lut.__DATA__.__faraddr + 3) + 65536) & 0xFFFF
	ld de, ((_lut.__DATA__.__faraddr + 3) + 65536) >> 16
	ld (_fp), hl
	ld (_fp + 2), de
	ld hl, ((_near)) & 0xFFFF
	ld de, ((_near)) >> 16
	ld (_fp), hl
	ld (_fp + 2), de
	call _Own
	ld (_near), a
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
_Own:
		call .core.__FAR_CALL
		DEFB 1
		DEFW _Own.__far
		CODEBANK 1
.LABEL._tbl:
.LABEL._tbl.__faraddr:
		defb 9, 8, 7, 6
_Own.__far:
	push ix
	ld ix, 0
	add ix, sp
	ld a, (_lut.__DATA__ + 0)
_Own__leave:
	ld sp, ix
	pop ix
	ret
		CODEBANK 0
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
_lut:
	DEFW .LABEL.__LABEL0
_lut.__DATA__.__PTR__:
	DEFW _lut.__DATA__
	DEFW 0
	DEFW 0
_lut.__DATA__.__faraddr:
_lut.__DATA__:
	DEFB 01h
	DEFB 02h
	DEFB 03h
	DEFB 04h
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
