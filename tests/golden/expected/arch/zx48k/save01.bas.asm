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
	ld hl, .LABEL.__LABEL0
	call .core.__LOADSTR
	push hl
	ld hl, 32768
	push hl
	ld hl, 25
	push hl
	call .core.SAVE_CODE
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
.LABEL.__LABEL0:
	DEFW 0008h
	DEFB 43h
	DEFB 6Fh
	DEFB 6Eh
	DEFB 74h
	DEFB 65h
	DEFB 6Eh
	DEFB 74h
	DEFB 73h
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
SAVE_CODE:
	    PROC
	    LOCAL MEMBOT
	    LOCAL SAVE_CONT
	    LOCAL ROM_SAVE
	    LOCAL __ERR_EMPTY
	    LOCAL SAVE_STOP
	    LOCAL STR_PTR
	    LOCAL SAVE_EMPTY_ERROR
	    MEMBOT EQU 23698 ; Use the CALC mem to store header
	    STR_PTR EQU MEMBOT + 17
	    pop hl   ; Return address
	    pop bc     ; data length in bytes
	    pop de   ; address start
	    ex (sp), hl ; CALLE => now hl = String
	    ld (STR_PTR), hl
__SAVE_CODE: ; INLINE version
	    ld a, b
	    or c
	    jr z, SAVE_EMPTY_ERROR    ; Return if block length == 0
	    push ix
	    ld a, h
	    or l
	    jr z, __ERR_EMPTY  ; Return if NULL STRING
	    ld ix, MEMBOT
	    ld (ix + 00), 3 ; CODE
	    ld (ix + 11), c
	    ld (ix + 12), b ; Store long in bytes
	    ld (ix + 13), e
	    ld (ix + 14), d ; Store address in bytes
	    push hl
	    ld bc, 9
	    ld HL, MEMBOT + 1
	    ld DE, MEMBOT + 2
	    ld (hl), ' '
	    ldir   ; Fill the filename with blanks
	    pop hl
	    ld c, (hl)
	    inc hl
	    ld b, (hl)
	    inc hl
	    ld a, b
	    or c
__ERR_EMPTY:
	    ld a, ERROR_InvalidFileName
	    jr z, SAVE_STOP        ; Return if str len == 0
	    ex de, hl  ; Saves HL in DE
	    ld hl, 10
	    or a
	    sbc hl, bc  ; Test BC > 10?
	    ex de, hl
	    jr nc, SAVE_CONT ; Ok BC <= 10
	    ld bc, 10 ; BC at most 10 chars
SAVE_CONT:
	    ld de, MEMBOT + 1
	    ldir     ; Copy String block NAME
	    ld hl, (STR_PTR)
	    call MEM_FREE
	    ld l, (ix + 13)
	    ld h, (ix + 14)    ; Restores start of bytes
	    ld a, r
	    push af
	    call ROM_SAVE
	    LOCAL NO_INT
	    pop af
	    jp po, NO_INT
	    ei
NO_INT:
	    ld hl, 1821h
	    ld (23682), hl
	    pop ix
	    ret
SAVE_EMPTY_ERROR:
	    ld a, ERROR_InvalidArg
SAVE_STOP:
	    pop ix
	    push af
	    ld hl, (STR_PTR)
	    call MEM_FREE
	    pop af
	    jp __STOP
	    LOCAL CHAN_OPEN
	    LOCAL PO_MSG
	    LOCAL WAIT_KEY
	    LOCAL SA_BYTES
	    LOCAL SA_CHK_BRK
	    LOCAL SA_CONT
	    CHAN_OPEN EQU 1601h
	    PO_MSG EQU 0C0Ah
	    WAIT_KEY EQU 15D4h
	    SA_BYTES EQU 04C6h
ROM_SAVE:
	    push hl
	    ld a, 0FDh
	    call CHAN_OPEN
	    xor a
	    ld de, 09A1h
	    call PO_MSG
	    set 5, (iy + 02h)
	    call WAIT_KEY
	    push ix
	    ld de, 0011h
	    xor a
	    call SA_BYTES
	    pop ix
	    call SA_CHK_BRK
	    jr c, SA_CONT
	    pop ix
	    ret
SA_CONT:
	    ei
	    ld b, 32h
	LOCAL SA_1_SEC
SA_1_SEC:
	    halt
	    djnz SA_1_SEC
	    ld e, (ix + 0Bh)
	    ld d, (ix + 0Ch)
	    ld a, 0FFh
	    pop ix
	    call SA_BYTES
SA_CHK_BRK:
	    ld b, a
	    ld a, (5C48h)
	    and 38h
	    rrca
	    rrca
	    rrca
	    out (0FEh), a
	    ld a, 7Fh
	    in a, (0FEh)
	    rra
	    ld a, b
	    ret
	    ENDP
	    pop namespace
	END
