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
	DEFB 00, 00, 00, 00, 00
_b:
	DEFB 00, 00, 00, 00, 00
.core.ZXBASIC_USER_DATA_END:
.core.__MAIN_PROGRAM__:
	call .core.COPY_ATTR
	ld a, 11
	push af
	ld a, 22
	call .core.PLOT
	call .core.COPY_ATTR
	ld a, (_a)
	ld de, (_a + 1)
	ld bc, (_a + 3)
	call .core.__FTOU32REG
	ld a, l
	push af
	ld a, 22
	call .core.PLOT
	call .core.COPY_ATTR
	ld a, 11
	push af
	ld a, (_a)
	ld de, (_a + 1)
	ld bc, (_a + 3)
	call .core.__FTOU32REG
	ld a, l
	call .core.PLOT
	call .core.COPY_ATTR
	ld a, (_a)
	ld de, (_a + 1)
	ld bc, (_a + 3)
	call .core.__FTOU32REG
	ld a, l
	push af
	ld a, (_b)
	ld de, (_b + 1)
	ld bc, (_b + 3)
	call .core.__FTOU32REG
	ld a, l
	call .core.PLOT
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
SCREEN_ADDR:        DW 16384  ; Screen address (can be pointed to other place to use a screen buffer)
SCREEN_ATTR_ADDR:   DW 22528  ; Screen attribute address (ditto.)
	CHARS               EQU 23606  ; Pointer to ROM/RAM Charset
	TV_FLAG             EQU 23612  ; Flags for controlling output to screen
	UDG                 EQU 23675  ; Pointer to UDG Charset
	COORDS              EQU 23677  ; Last PLOT coordinates
	FLAGS2              EQU 23681  ;
	ECHO_E              EQU 23682  ;
	DFCC                EQU 23684  ; Next screen addr for PRINT
	DFCCL               EQU 23686  ; Next screen attr for PRINT
	S_POSN              EQU 23688
	ATTR_P              EQU 23693  ; Current Permanent ATTRS set with INK, PAPER, etc commands
	ATTR_T              EQU 23695  ; temporary ATTRIBUTES
	P_FLAG              EQU 23697  ;
	MEM0                EQU 23698  ; Temporary memory buffer used by ROM chars
	SCR_COLS            EQU 33     ; Screen with in columns + 1
	SCR_ROWS            EQU 24     ; Screen height in rows
	SCR_SIZE            EQU (SCR_ROWS << 8) + SCR_COLS
	pop namespace
	    push namespace core
COPY_ATTR:
	    PROC
	    LOCAL INVERSE1
	    LOCAL __REFRESH_TMP
	INVERSE1 EQU 02Fh
	    ld hl, (ATTR_P)
	    ld (ATTR_T), hl
	    ld hl, FLAGS2
	    call __REFRESH_TMP
	    ld hl, P_FLAG
	    call __REFRESH_TMP
__SET_ATTR_MODE:		; Another entry to set print modes. A contains (P_FLAG)
	    ret
__REFRESH_TMP:
	    ld a, (hl)
	    and 0b10101010
	    ld c, a
	    rra
	    or c
	    ld (hl), a
	    ret
	    ENDP
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
	ERR_NR    EQU    23610    ; Error code system variable
	ERROR_Ok                EQU    -1
	ERROR_SubscriptWrong    EQU     2
	ERROR_OutOfMemory       EQU     3
	ERROR_OutOfScreen       EQU     4
	ERROR_NumberTooBig      EQU     5
	ERROR_InvalidArg        EQU     9
	ERROR_IntOutOfRange     EQU    10
	ERROR_NonsenseInBasic   EQU    11
	ERROR_InvalidFileName   EQU    14
	ERROR_InvalidColour     EQU    19
	ERROR_BreakIntoProgram  EQU    20
	ERROR_TapeLoadingErr    EQU    26
__ERROR:
	    ld (__ERROR_CODE), a
	    rst 8
__ERROR_CODE:
	    nop
	    ret
__STOP:
	    ld (ERR_NR), a
	    ret
	    pop namespace
	    push namespace core
__ATTR_ADDR:
	    ld h, 0                     ;  7 T-States
	    ld a, d                     ;  4 T-States
	    ld d, h
	    add a, a     ; a * 2        ;  4 T-States
	    add a, a     ; a * 4        ;  4 T-States
	    ld l, a      ; HL = A * 4   ;  4 T-States
	    add hl, hl   ; HL = A * 8   ; 15 T-States
	    add hl, hl   ; HL = A * 16  ; 15 T-States
	    add hl, hl   ; HL = A * 32  ; 15 T-States
	    add hl, de
	    ld de, (SCREEN_ATTR_ADDR)    ; Adds the screen address
	    add hl, de
	    ret
SET_ATTR:
	    call __IN_SCREEN
	    ret nc
	    call __ATTR_ADDR
__SET_ATTR:
	    PROC
__SET_ATTR2:  ; Sets attr from ATTR_T to (HL) which points to the scr address
	    ld de, (ATTR_T)    ; E = ATTR_T, D = MASK_T
	    ld a, d
	    and (hl)
	    ld c, a    ; C = current screen color, masked
	    ld a, d
	    cpl        ; Negate mask
	    and e    ; Mask current attributes
	    or c    ; Mix them
	    ld (hl), a ; Store result in screen
	    ret
	    ENDP
	    pop namespace
	    push namespace core
__LOAD_S_POSN:
	    PROC
	    ld de, (S_POSN)
	    ld hl, SCR_SIZE
	    or a
	    sbc hl, de
	    ex de, hl
	    ret
	    ENDP
__SAVE_S_POSN:
	    PROC
	    ld hl, SCR_SIZE
	    or a
	    sbc hl, de
	    ld (S_POSN), hl ; saves it again
__SET_SCR_PTR:  ;; Fast
	    push de
	    call __ATTR_ADDR
	    ld (DFCCL), hl
	    pop de
	    ld a, d
	    ld c, a     ; Saves it for later
	    and 0F8h    ; Masks 3 lower bit ; zy
	    ld d, a
	    ld a, c     ; Recovers it
	    and 07h     ; MOD 7 ; y1
	    rrca
	    rrca
	    rrca
	    or e
	    ld e, a
	    ld hl, (SCREEN_ADDR)
	    add hl, de    ; HL = Screen address + DE
	    ld (DFCC), hl
	    ret
	    ENDP
	    pop namespace
	    push namespace core
__IN_SCREEN:
	    PROC
	    LOCAL __IN_SCREEN_ERR
	    ld hl, SCR_SIZE
	    ld a, e
	    cp l
	    jr nc, __IN_SCREEN_ERR	; Do nothing and return if out of range
	    ld a, d
	    cp h
	    ret c                       ; Return if carry (OK)
__IN_SCREEN_ERR:
__OUT_OF_SCREEN_ERR:
	    ld a, ERROR_OutOfScreen
	    jp __STOP   ; Saves error code and exits
	    ENDP
	    pop namespace
	push namespace core
SET_PIXEL_ADDR_ATTR:
	    ld de, (SCREEN_ADDR)
	    or a
	    sbc hl, de
	    ld a, h
	    rrca
	    rrca
	    rrca
	    and 3
	    ld h, a
	    ld de, (SCREEN_ATTR_ADDR)
	    add hl, de  ;; Final screen addr
	    jp __SET_ATTR2
	pop namespace
	    push namespace core
PLOT:
	    PROC
	    LOCAL PLOT_SUB
	    LOCAL PIXEL_ADDR
	    LOCAL COORDS
	    LOCAL __PLOT_ERR
	    LOCAL P_FLAG
	    LOCAL __PLOT_OVER1
	P_FLAG EQU 23697
	    pop hl
	    ex (sp), hl ; Callee
	    ld b, a
	    ld c, h
	    ld a, 191
	    cp b
	    jr c, __PLOT_ERR ; jr is faster here (#1)
__PLOT:			; __FASTCALL__ entry (b, c) = pixel coords (y, x)
	    ld (COORDS), bc	; Saves current point
	    ld a, 191 ; Max y coord
	    call PIXEL_ADDR
	    res 6, h    ; Starts from 0
	    ld bc, (SCREEN_ADDR)
	    add hl, bc  ; Now current offset
	    ld b, a
	    inc b
	    ld a, 0FEh
	LOCAL __PLOT_LOOP
__PLOT_LOOP:
	    rrca
	    djnz __PLOT_LOOP
	    ld b, a
	    ld a, (P_FLAG)
	    ld c, a
	    ld a, (hl)
	    bit 0, c        ; is it OVER 1
	    jr nz, __PLOT_OVER1
	    and b
__PLOT_OVER1:
	    bit 2, c        ; is it inverse 1
	    jr nz, __PLOT_END
	    xor b
	    cpl
	LOCAL __PLOT_END
__PLOT_END:
	    ld (hl), a
	    jp SET_PIXEL_ADDR_ATTR
__PLOT_ERR:
	    jp __OUT_OF_SCREEN_ERR ; Spent 3 bytes, but saves 3 T-States at (#1)
	PLOT_SUB EQU 22ECh
	PIXEL_ADDR EQU 22ACh
	COORDS EQU 5C7Dh
	    ENDP
	    pop namespace
	END
