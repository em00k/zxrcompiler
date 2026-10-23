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
_c:
	DEFB 00, 00
.core.ZXBASIC_USER_DATA_END:
.core.__MAIN_PROGRAM__:
	ld de, .LABEL.__LABEL0
	ld hl, _c
	call .core.__STORE_STR
	ld hl, (_c)
	call .core.__LOADSTR
	push hl
	call _test
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
	inc sp
	ld l, (ix+4)
	ld h, (ix+5)
	xor a
	call .core.VAL
	ld hl, -5
	call .core.__PSTOREF
_test__leave:
	ex af, af'
	exx
	ld l, (ix+4)
	ld h, (ix+5)
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
.LABEL.__LABEL0:
	DEFW 0001h
	DEFB 35h
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
__REALLOC:    ; Reallocates block pointed by HL, with new length BC
	    PROC
	    LOCAL __REALLOC_END
	    ld a, h
	    or l
	    jp z, __MEM_ALLOC    ; If HL == NULL, just do a malloc
	    ld e, (hl)
	    inc hl
	    ld d, (hl)    ; DE = First 2 bytes of HL block
	    push hl
	    exx
	    pop de
	    inc de        ; DE' <- HL + 2
	    exx            ; DE' <- HL (Saves current pointer into DE')
	    dec hl        ; HL = Block start
	    push de
	    push bc
	    call __MEM_FREE        ; Frees current block
	    pop bc
	    push bc
	    call __MEM_ALLOC    ; Gets a new block of length BC
	    pop bc
	    pop de
	    ld a, h
	    or l
	    ret z        ; Return if HL == NULL (No memory)
	    ld (hl), e
	    inc hl
	    ld (hl), d
	    inc hl        ; Recovers first 2 bytes in HL
	    dec bc
	    dec bc        ; BC = BC - 2 (Two bytes copied)
	    ld a, b
	    or c
	    jp z, __REALLOC_END        ; Ret if nothing to copy (BC == 0)
	    exx
	    push de
	    exx
	    pop de        ; DE <- DE' ; Start of remaining block
	    push hl        ; Saves current Block + 2 start
    ex de, hl    ; Exchanges them: DE is destiny block
	    ldir        ; Copies BC Bytes
	    pop hl        ; Recovers Block + 2 start
__REALLOC_END:
	    dec hl        ; Set HL
	    dec hl        ; To begin of block
	    ret
	    ENDP
	    pop namespace
	    push namespace core
__STRASSIGN: ; Performs a$ = b$ (HL = address of a$; DE = Address of b$)
	    PROC
	    LOCAL __STRREALLOC
	    LOCAL __STRCONTINUE
	    LOCAL __B_IS_NULL
	    LOCAL __NOTHING_TO_COPY
	    ld b, d
	    ld c, e
	    ld a, b
	    or c
	    jr z, __B_IS_NULL
	    ex de, hl
	    ld c, (hl)
	    inc hl
	    ld b, (hl)
	    dec hl		; BC = LEN(b$)
	    ex de, hl	; DE = &b$
__B_IS_NULL:		; Jumps here if B$ pointer is NULL
	    inc bc
	    inc bc		; BC = BC + 2  ; (LEN(b$) + 2 bytes for storing length)
	    push de
	    push hl
	    ld a, h
	    or l
	    jr z, __STRREALLOC
	    dec hl
	    ld d, (hl)
	    dec hl
	    ld e, (hl)	; DE = MEMBLOCKSIZE(a$)
	    dec de
	    dec de		; DE = DE - 2  ; (Membloksize takes 2 bytes for memblock length)
	    ld h, b
	    ld l, c		; HL = LEN(b$) + 2  => Minimum block size required
	    ex de, hl	; Now HL = BLOCKSIZE(a$), DE = LEN(b$) + 2
	    or a		; Prepare to subtract BLOCKSIZE(a$) - LEN(b$)
	    sbc hl, de  ; Carry if len(b$) > Blocklen(a$)
	    jr c, __STRREALLOC ; No need to realloc
	    ex de, hl	; DE = Remaining bytes in a$ mem block.
	    ld hl, 4
	    sbc hl, de  ; if remaining bytes < 4 we can continue
	    jr nc,__STRCONTINUE ; Otherwise, we realloc, to free some bytes
__STRREALLOC:
	    pop hl
	    call __REALLOC	; Returns in HL a new pointer with BC bytes allocated
	    push hl
__STRCONTINUE:	;   Pops hl and de SWAPPED
	    pop de	;	DE = &a$
	    pop hl	; 	HL = &b$
	    ld a, d		; Return if not enough memory for new length
	    or e
	    ret z		; Return if DE == NULL (0)
__STRCPY:	; Copies string pointed by HL into string pointed by DE
	    ld a, h
	    or l
	    jr z, __NOTHING_TO_COPY
	    ld c, (hl)
	    inc hl
	    ld b, (hl)
	    dec hl
	    inc bc
	    inc bc
	    push de
	    ldir
	    pop hl
	    ret
__NOTHING_TO_COPY:
	    ex de, hl
	    ld (hl), e
	    inc hl
	    ld (hl), d
	    dec hl
	    ret
	    ENDP
	    pop namespace
	    push namespace core
__PISTORE_STR:          ; Indirect assignment at (IX + BC)
	    push ix
	    pop hl
	    add hl, bc
__ISTORE_STR:           ; Indirect assignment, hl point to a pointer to a pointer to the heap!
	    ld c, (hl)
	    inc hl
	    ld h, (hl)
	    ld l, c             ; HL = (HL)
__STORE_STR:
	    push de             ; Pointer to b$
	    push hl             ; Pointer to a$
	    ld c, (hl)
	    inc hl
	    ld h, (hl)
	    ld l, c             ; HL = (HL)
	    call __STRASSIGN    ; HL (a$) = DE (b$); HL changed to a new dynamic memory allocation
	    ex de, hl           ; DE = new address of a$
	    pop hl              ; Recover variable memory address pointer
	    ld (hl), e
	    inc hl
	    ld (hl), d          ; Stores a$ ptr into element ptr
	    pop hl              ; Returns ptr to b$ in HL (Caller might needed to free it from memory)
	    ret
	    pop namespace
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
VAL: ; Computes VAL(a$) using ROM FP-CALC
	    PROC
	    LOCAL STK_STO_S
	    LOCAL __RET_ZERO
	    LOCAL ERR_SP
	    LOCAL STKBOT
	    LOCAL RECLAIM1
	    LOCAL CH_ADD
	    LOCAL __VAL_ERROR
	    LOCAL __VAL_EMPTY
	    LOCAL SET_MIN
	RECLAIM1	EQU 6629
	STKBOT		EQU 23651
	ERR_SP		EQU 23613
	CH_ADD      EQU 23645
	STK_STO_S	EQU	2AB2h
	SET_MIN     EQU 16B0h
	    ld d, a ; Preserves A register in DE
	    ld a, h
	    or l
	    jr z, __RET_ZERO ; NULL STRING => Return 0
	    push de ; Saves A Register (now in D)
	    push hl	; Not null string. Save its address for later
	    ld c, (hl)
	    inc hl
	    ld b, (hl)
	    inc hl
	    ld a, b
	    or c
	    jr z, __VAL_EMPTY ; Jumps VAL_EMPTY on empty string
	    ex de, hl ; DE = String start
	    ld hl, (CH_ADD)
	    push hl
	    ld hl, (STKBOT)
	    push hl
	    ld hl, (ERR_SP)
	    push hl
	    ld hl, __VAL_ERROR
	    push hl
	    ld (ERR_SP), sp
	    call STK_STO_S ; Enter it on the stack
	    ld b, 1Dh ; "VAL"
	    rst 28h	; ROM CALC
	    defb 1Dh ; VAL
	    defb 38h ; END CALC
	    pop hl 	; Discards our current error handler
	    pop hl
	    ld (ERR_SP), hl	; Restores ERR_SP
	    pop de	         ; old STKBOT
	    ld hl, (STKBOT)  ; current SKTBOT
	    call	RECLAIM1 ; Recover unused space
	    pop hl  ; Discards old CH_ADD value
	    pop hl 	; String pointer
	    pop af	; Deletion flag
	    or a
	    call nz, __MEM_FREE	; Frees string content before returning
	    ld a, ERROR_Ok      ; Sets OK in the result
	    ld (ERR_NR), a
	    jp __FPSTACK_POP	; Recovers result and return from there
__VAL_ERROR:	; Jumps here on ERROR
	    pop hl
	    ld (ERR_SP), hl ; Restores ERR_SP
	    ld hl, (STKBOT)  ; current SKTBOT
	    pop de	; old STKBOT
	    pop hl
	    ld (CH_ADD), hl  ; Recovers old CH_ADD
	    call 16B0h       ; Resets temporary areas after an error
__VAL_EMPTY:	; Jumps here on empty string
	    pop hl      ; Recovers initial string address
    pop af      ; String flag: If not 0 => it's temporary
	    or a
	    call nz, __MEM_FREE ; Frees "" string
__RET_ZERO:	; Returns 0 Floating point on error
	    ld a, ERROR_InvalidArg
	    ld (ERR_NR), a
	    xor a
	    ld b, a
	    ld c, a
	    ld d, b
	    ld e, c
	    ret
	    ENDP
	    pop namespace
	END
