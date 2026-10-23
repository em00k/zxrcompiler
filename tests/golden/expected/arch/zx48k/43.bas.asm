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
.core.ZXBASIC_USER_DATA_END:
.core.__MAIN_PROGRAM__:
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
_test:
	push ix
	ld ix, 0
	add ix, sp
	ld hl, 0
	push hl
	push hl
	push hl
	ld hl, .LABEL.__LABEL1
	push hl
	ld hl, -6
	ld de, .LABEL.__LABEL0
	ld bc, 3
	call .core.__ALLOC_INITIALIZED_LOCAL_ARRAY
	ld a, (ix+5)
	push af
	ld l, (ix-4)
	ld h, (ix-3)
	inc hl
	pop af
	add a, (hl)
	call .core.__U8TOFREG
_test__leave:
	ex af, af'
	exx
	ld l, (ix-4)
	ld h, (ix-3)
	call .core.__MEM_FREE
	ex af, af'
	exx
	ld sp, ix
	pop ix
	exx
	pop hl
	ex (sp), hl
	exx
	ret
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
__MEM_CALLOC:
	    push bc
	    call __MEM_ALLOC
	    pop bc
	    ld a, h
	    or l
	    ret z  ; No memory
	    ld (hl), 0
	    dec bc
	    ld a, b
	    or c
	    ret z  ; Already filled (1 byte-length block)
	    ld d, h
	    ld e, l
	    inc de
	    push hl
	    ldir
	    pop hl
	    ret
	    pop namespace
	    push namespace core
__ALLOC_LOCAL_ARRAY:
	    push de
	    push ix
	    pop de
	    add hl, de  ; hl = ix + hl
	    pop de
	    ld (hl), e
	    inc hl
	    ld (hl), d
	    inc hl
	    push hl
	    call __MEM_CALLOC
	    pop de
	    ex de, hl
	    ld (hl), e
	    inc hl
	    ld (hl), d
	    ret
__ALLOC_INITIALIZED_LOCAL_ARRAY:
	    push bc
	    call __ALLOC_LOCAL_ARRAY
	    pop bc
	    exx
	    pop hl       ; HL <- RET address
	    ex (sp), hl  ; HL <- Data table, [SP] <- RET address
	    push hl      ; [SP] <- Data table
	    exx
	    ex (sp), hl  ; HL = Data table, (SP) = (IX + HL + 4) - start of array address lbound
	    ldir
	    pop hl  ; HL = addr of LBound area if used
	    ret
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
.LABEL.__LABEL0:
	DEFB 00h
	DEFB 00h
	DEFB 01h
.LABEL.__LABEL1:
	DEFB 0AAh
	DEFB 0BBh
	DEFB 0CCh
	END
