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
	call .core.__MEM_INIT
	jp .core.__MAIN_PROGRAM__
.core.__CALL_BACK__:
	DEFW 0
.core.ZXBASIC_USER_DATA:
.core.ZXBASIC_HEAP_SIZE EQU 4768
.core.ZXBASIC_MEM_HEAP:
	DEFS 4768
.core.ZXBASIC_USER_DATA_LEN EQU .core.ZXBASIC_USER_DATA_END - .core.ZXBASIC_USER_DATA
	.core.__LABEL__.ZXBASIC_USER_DATA_LEN EQU .core.ZXBASIC_USER_DATA_LEN
	.core.__LABEL__.ZXBASIC_USER_DATA EQU .core.ZXBASIC_USER_DATA
_a:
	DEFB 00, 00, 00, 00, 00
.core.ZXBASIC_USER_DATA_END:
.core.__MAIN_PROGRAM__:
	ld hl, .DATA.__DATA__0
	call .core.__RESTORE
	ld a, 9
	call .core.__READ
	ld hl, _a
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
.DATA.__DATA__0:
	DEFB 3
	DEFB 10
__DATA__END:
	DEFB 00h
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
__MEM_INIT: ; Initializes the library using (RAMTOP) as start, and
	    ld hl, ZXBASIC_MEM_HEAP  ; Change this with other address of heap start
	    ld de, ZXBASIC_HEAP_SIZE ; Change this with your size
__MEM_INIT2:
	    PROC
	    dec de
	    dec de
	    dec de
	    dec de        ; DE = length - 4; HL = start
	    xor a
	    ld (hl), a
	    inc hl
    ld (hl), a ; First "free" block is a header: size=0, Pointer=&(Block) + 4
	    inc hl
	    ld b, h
	    ld c, l
	    inc bc
	    inc bc      ; BC = starts of next block
	    ld (hl), c
	    inc hl
	    ld (hl), b
	    inc hl      ; Pointer to next block
	    ld (hl), e
	    inc hl
	    ld (hl), d
	    inc hl      ; Block size (should be length - 4 at start); This block contains all the available memory
	    ld (hl), a ; NULL (0000h) ; No more blocks (a list with a single block)
	    inc hl
	    ld (hl), a
	    ld a, 201
	    ld (__MEM_INIT), a; "Pokes" with a RET so ensure this routine is not called again
	    ret
	    ENDP
	    pop namespace
	    push namespace core
MEM_ALLOC:
__MEM_ALLOC: ; Returns the 1st free block found of the given length (in BC)
	    PROC
	    LOCAL __MEM_LOOP
	    LOCAL __MEM_DONE
	    LOCAL __MEM_SUBTRACT
	    LOCAL __MEM_START
	    LOCAL TEMP, TEMP0
	TEMP EQU TEMP0 + 1
	    ld hl, 0
	    ld (TEMP), hl
__MEM_START:
	    ld hl, ZXBASIC_MEM_HEAP  ; This label point to the heap start
	    inc bc
	    inc bc  ; BC = BC + 2 ; block size needs 2 extra bytes for hidden pointer
__MEM_LOOP:  ; Loads lengh at (HL, HL+). If Lenght >= BC, jump to __MEM_DONE
	    ld a, h ;  HL = NULL (No memory available?)
	    or l
	    ret z ; NULL
	    ld e, (hl)
	    inc hl
	    ld d, (hl)
	    inc hl          ; DE = Block Length
	    push hl         ; HL = *pointer to -> next block
	    ex de, hl
	    or a            ; CF = 0
	    sbc hl, bc      ; FREE >= BC (Length)  (HL = BlockLength - Length)
	    jp nc, __MEM_DONE
	    pop hl
	    ld (TEMP), hl
	    ex de, hl
	    ld e, (hl)
	    inc hl
	    ld d, (hl)
	    ex de, hl
	    jp __MEM_LOOP
__MEM_DONE:  ; A free block has been found.
	    push hl
	    exx  ; exx to preserve bc
	    pop hl
	    ld bc, 4
	    or a
	    sbc hl, bc
	    exx
	    jp nc, __MEM_SUBTRACT
	    pop hl     ; Discard current block pointer
	    push de
	    ex de, hl  ; DE = Previous block pointer; (HL) = Next block pointer
	    ld a, (hl)
	    inc hl
	    ld h, (hl)
	    ld l, a    ; HL = (HL)
	    ex de, hl  ; HL = Previous block pointer; DE = Next block pointer
TEMP0:
	    ld hl, 0   ; Pre-previous block pointer
	    ld (hl), e
	    inc hl
	    ld (hl), d ; LINKED
	    pop hl ; Returning block.
	    ret
__MEM_SUBTRACT:
	    ex de, hl
	    dec hl
	    ld (hl), d
	    dec hl
	    ld (hl), e ; Store new block length
	    add hl, de ; New length + DE => free-block start
	    pop de     ; Remove previous HL off the stack
	    ld (hl), c ; Store length on its 1st word
	    inc hl
	    ld (hl), b
	    inc hl     ; Return hl
	    ret
	    ENDP
	    pop namespace
	    push namespace core
__ILOADSTR:		; This is the indirect pointer entry HL = (HL)
	    ld a, h
	    or l
	    ret z
	    ld a, (hl)
	    inc hl
	    ld h, (hl)
	    ld l, a
__LOADSTR:		; __FASTCALL__ entry
	    ld a, h
	    or l
	    ret z	; Return if NULL
	    ld c, (hl)
	    inc hl
	    ld b, (hl)
	    dec hl  ; BC = LEN(a$)
	    inc bc
	    inc bc	; BC = LEN(a$) + 2 (two bytes for length)
	    push hl
	    push bc
	    call __MEM_ALLOC
	    pop bc  ; Recover length
	    pop de  ; Recover origin
	    ld a, h
	    or l
	    ret z	; Return if NULL (No memory)
	    ex de, hl ; ldir takes HL as source, DE as destiny, so SWAP HL,DE
	    push de	; Saves destiny start
	    ldir	; Copies string (length number included)
	    pop hl	; Recovers destiny in hl as result
	    ret
	    pop namespace
	    push namespace core
__ILOAD32:
	    ld e, (hl)
	    inc hl
	    ld d, (hl)
	    inc hl
	    ld a, (hl)
	    inc hl
	    ld h, (hl)
	    ld l, a
	    ex de, hl
	    ret
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
__FTOF16REG:	; Converts a Float to 16.16 (32 bit) fixed point decimal
	    ld l, a     ; Saves exponent for later
	    or d
	    or e
	    or b
	    or c
	    ld h, e
	    ret z		; Return if ZERO
	    push hl  ; Stores it for later (Contains sign in H, exponent in L)
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
	    pop bc
	    ld a, c  ; Get exponent
	    sub 112  ; Exponent -= 128 + 16
	    push bc  ; Saves sign in b again
	    jp z, __FTOU32REG_END	; If it was <= 128, we are done (Integers must be > 128)
	    jp c, __FTOU32REG_END	; It was decimal (0.xxx). We are done (return 0)
	    ld b, a  ; Loop counter = exponent - 128 + 16 (we need to shift 16 bit more)
	    jp __FTOU32REG_LOOP ; proceed as an u32 integer
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
	    push namespace core
__F16TOFREG:	; Converts a 16.16 signed fixed point (stored in DEHL)
	    PROC
	    LOCAL __F16TOFREG2
	    ld a, d
	    or a		; Test sign
	    jp p, __F16TOFREG2	; It was positive, proceed as 32bit unsigned
	    call __NEG32		; Convert it to positive
	    call __F16TOFREG2	; Convert it to Floating point
	    set 7, e			; Put the sign bit (negative) in the 31bit of mantissa
	    ret
__F16TOFREG2:	; Converts an unsigned 32 bit integer (DEHL)
	    ld a, d
	    or e
	    or h
	    or l
	    ld b, h
	    ld c, l
	    ret z       ; Return 00 0000 0000 if 0
	    push de
	    push hl
	    exx
	    pop de  ; Loads integer into B'C' D'E'
	    pop bc
	    exx
	    ld l, 112	; Exponent
	    ld bc, 0	; DEBC = 0
	    ld d, b
	    ld e, c
	    jp __U32TOFREG_LOOP ; Proceed as an integer
	    ENDP
	    pop namespace
	    push namespace core
MEM_FREE:
__MEM_FREE: ; Frees the block pointed by HL
	    PROC
	    LOCAL __MEM_LOOP2
	    LOCAL __MEM_LINK_PREV
	    LOCAL __MEM_JOIN_TEST
	    LOCAL __MEM_BLOCK_JOIN
	    ld a, h
	    or l
	    ret z       ; Return if NULL pointer
	    dec hl
	    dec hl
	    ld b, h
	    ld c, l    ; BC = Block pointer
	    ld hl, ZXBASIC_MEM_HEAP  ; This label point to the heap start
__MEM_LOOP2:
	    inc hl
	    inc hl     ; Next block ptr
	    ld e, (hl)
	    inc hl
	    ld d, (hl) ; Block next ptr
	    ex de, hl  ; DE = &(block->next); HL = block->next
	    ld a, h    ; HL == NULL?
	    or l
	    jp z, __MEM_LINK_PREV; if so, link with previous
	    or a       ; Clear carry flag
	    sbc hl, bc ; Carry if BC > HL => This block if before
	    add hl, bc ; Restores HL, preserving Carry flag
	    jp c, __MEM_LOOP2 ; This block is before. Keep searching PASS the block
__MEM_LINK_PREV:    ; Link (DE) with BC, and BC->next with HL
	    ex de, hl
	    push hl
	    dec hl
	    ld (hl), c
	    inc hl
	    ld (hl), b ; (DE) <- BC
	    ld h, b    ; HL <- BC (Free block ptr)
	    ld l, c
	    inc hl     ; Skip block length (2 bytes)
	    inc hl
	    ld (hl), e ; Block->next = DE
	    inc hl
	    ld (hl), d
	    call __MEM_JOIN_TEST
	    pop hl
__MEM_JOIN_TEST:   ; Checks for fragmented contiguous blocks and joins them
	    ld d, (hl)
	    dec hl
	    ld e, (hl)
	    dec hl
	    ld b, (hl) ; Loads block length into BC
	    dec hl
	    ld c, (hl) ;
	    push hl    ; Saves it for later
	    add hl, bc ; Adds its length. If HL == DE now, it must be joined
	    or a
	    sbc hl, de ; If Z, then HL == DE => We must join
	    pop hl
	    ret nz
__MEM_BLOCK_JOIN:  ; Joins current block (pointed by HL) with next one (pointed by DE). HL->length already in BC
	    push hl    ; Saves it for later
	    ex de, hl
	    ld e, (hl) ; DE -> block->next->length
	    inc hl
	    ld d, (hl)
	    inc hl
	    ex de, hl  ; DE = &(block->next)
	    add hl, bc ; HL = Total Length
	    ld b, h
	    ld c, l    ; BC = Total Length
	    ex de, hl
	    ld e, (hl)
	    inc hl
	    ld d, (hl) ; DE = block->next
	    pop hl     ; Recovers Pointer to block
	    ld (hl), c
	    inc hl
	    ld (hl), b ; Length Saved
	    inc hl
	    ld (hl), e
	    inc hl
	    ld (hl), d ; Next saved
	    ret
	    ENDP
	    pop namespace
	    push namespace core
__RESTORE:
	    PROC
	    LOCAL __DATA_ADDR
	    ld (__DATA_ADDR), hl
	    ret
__READ:
	    LOCAL read_restart, cont, cont2, table, no_func
	    LOCAL dynamic_cast, dynamic_cast2, dynamic_cast3, dynamic_cast4
	    LOCAL _decode_table, coerce_to_int, coerce_to_int2, promote_to_i16
	    LOCAL _from_i8, _from_u8
	    LOCAL _from_i16, _from_u16
	    LOCAL _from_i32, _from_u32
	    LOCAL _from_fixed, __data_error
	    push af  ; type of data to read
	    ld hl, (__DATA_ADDR)
read_restart:
	    ld a, (hl)
	    or a   ; 0 => OUT of data
	    jr nz, cont
	    ld hl, .DATA.__DATA__0
	    ld (__DATA_ADDR), hl
	    jr read_restart  ; Start again
cont:
	    and 0x80
	    ld a, (hl)
	    push af
	    jp z, no_func    ;; Loads data directly, not a function
	    inc hl
	    ld e, (hl)
	    inc hl
	    ld d, (hl)
	    inc hl
	    ld (__DATA_ADDR), hl  ;; Store address of next DATA
	    ex de, hl
cont2:
	    ld de, dynamic_cast
	    push de  ; ret address
	    jp (hl)  ; "call (hl)"
dynamic_cast:
	    exx
	    ex af, af'
	    pop af   ; type READ
	    and 0x7F ; clear bit 7
	    pop hl   ; type requested by USER (type of the READ variable)
	    ld c, h  ; save requested type (save it in register C)
	    cp h
	    exx
	    jr nz, dynamic_cast2  ; Types are identical?
	    ex af, af'
	    ret
dynamic_cast2:
	    cp 1             ; Requested a number, but read a string?
	    jr nz, dynamic_cast3
	    call __MEM_FREE     ; Frees str from memory
	    jr __data_error
dynamic_cast3:
	    exx
	    ld b, a     ; Read type
	    ld a, c     ; Requested type
	    cp 1
	    jr z, __data_error
	    cp b
	    jr c, dynamic_cast4
	    ld a, b
	    sub 2
	    add a, a
	    ld l, a
	    ld h, 0
	    ld de, _decode_table
	    add hl, de
	    ld e, (hl)
	    inc hl
	    ld h, (hl)
	    ld l, e
	    push hl
	    ld a, c     ; Requested type
	    exx
	    ret
__data_error:
	    ld a, ERROR_InvalidArg
	    call __STOP  ; The user expected a string, but read a number
	    xor a
	    ld h, a
	    ld l, a
	    ld e, a
	    ld d, a
	    ld b, a
	    ld c, a
	    ret
_decode_table:
	    dw _from_i8
	    dw _from_u8
	    dw _from_i16
	    dw _from_u16
	    dw _from_i32
	    dw _from_u32
	    dw _from_fixed
_from_i8:
	    cp 4
	    jr nc, promote_to_i16
	    ex af, af'
	    ret     ;; Was from Byte to Ubyte
promote_to_i16:
	    ex af, af'
	    ld l, a
	    rla
	    sbc a, a
	    ld h, a     ; copy sgn to h
	    ex af, af'
	    jr _before_from_i16
_from_u8:
	    ex af, af'
	    ld l, a
	    ld h, 0
	    ex af, af'
_before_from_i16:
_from_i16:
	    cp 6
	    ret c  ;; from i16 to u16
	    ex af, af'
	    ld a, h
	    rla
	    sbc a, a
	    ld e, a
	    ld d, a
	    ex af, af'
_from_i32:
	    cp 7
	    ret z ;; From i32 to u32
	    ret c ;; From u16 to i32
	    cp 9
	    jp z, __I32TOFREG
_from_u32:
	    cp 9
	    jp z, __U32TOFREG
	    ex de, hl
	    ld hl, 0
	    cp 8
	    ret z
_from_fixed:  ;; From fixed to float
	    jp __F16TOFREG
_from_u16:
	    ld de, 0    ; HL 0x0000 => 32 bits
	    jp _from_i32
dynamic_cast4:
	    ld a, b ;; read type
	    cp 4 ;; if user type < read type < _i16 => From Ubyte to Byte. Return af'
	    jr nc, 1f
	    ex af, af'
	    ret
1:
	    ld a, c ;; recover user required type
	    cp 8 ;; required type
	    jr c, before_to_int  ;; required < fixed (f16)
	    ex af, af'
	    exx     ;; Ok, we must convert from float to f16
	    jp __FTOF16REG
before_to_int:
	    ld a, b ;; read type
	    cp 8 ;;
	    jr c, coerce_to_int2
	    jr nz, coerce_to_int  ;; From float to int
	    ld a, c ;; user type
	    exx
	    ex de, hl
	    ld a, h
	    rla
	    sbc a, a
	    ld d, a
	    ld e, a
	    exx
	    jr coerce_to_int2
coerce_to_int:
	    exx
	    ex af, af'
	    call __FTOU32REG
	    ex af, af'   ; a contains user type
	    exx
coerce_to_int2:  ; At this point we have an u/integer in hl
	    exx
	    cp 4
	    ret nc       ; Already done. Return the result
	    ld a, l      ; Truncate to byte
	    ret
no_func:
	    exx
	    ld de, dynamic_cast
	    push de ; Ret address
	    dec a        ; 0 => string; 1, 2 => byte; 3, 4 => integer; 5, 6 => long, 7 => fixed; 8 => float
	    ld h, 0
	    add a, a
	    ld l, a
	    ld de, table
	    add hl, de
	    ld e, (hl)
	    inc hl
	    ld h, (hl)
	    ld l, e
	    push hl ; address to jump to
	    exx
	    inc hl
	    ret     ; jp (sp)  => jump to table[a - 1]
table:
	    LOCAL __01_decode_string
	    LOCAL __02_decode_byte
	    LOCAL __03_decode_ubyte
	    LOCAL __04_decode_integer
	    LOCAL __05_decode_uinteger
	    LOCAL __06_decode_long
	    LOCAL __07_decode_ulong
	    LOCAL __08_decode_fixed
	    LOCAL __09_decode_float
	    dw __01_decode_string
	    dw __02_decode_byte
	    dw __03_decode_ubyte
	    dw __04_decode_integer
	    dw __05_decode_uinteger
	    dw __06_decode_long
	    dw __07_decode_ulong
	    dw __08_decode_fixed
	    dw __09_decode_float
__01_decode_string:
	    ld e, (hl)
	    inc hl
	    ld d, (hl)
	    inc hl
	    ld (__DATA_ADDR), hl  ;; Store address of next DATA
	    ex de, hl
	    jp __LOADSTR
__02_decode_byte:
__03_decode_ubyte:
	    ld a, (hl)
	    inc hl
	    ld (__DATA_ADDR), hl
	    ret
__04_decode_integer:
__05_decode_uinteger:
	    ld e, (hl)
	    inc hl
	    ld d, (hl)
	    inc hl
	    ld (__DATA_ADDR), hl
	    ex de, hl
	    ret
__06_decode_long:
__07_decode_ulong:
__08_decode_fixed:
	    ld b, h
	    ld c, l
	    inc bc
	    inc bc
	    inc bc
	    inc bc
	    ld (__DATA_ADDR), bc
	    jp __ILOAD32
__09_decode_float:
	    call __LOADF
	    inc hl
	    ld (__DATA_ADDR), hl
	    ld h, a  ; returns A in H; sets A free
	    ret
__DATA_ADDR:  ;; Stores current DATA ptr
	    dw .DATA.__DATA__0
	    ENDP
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
	END
