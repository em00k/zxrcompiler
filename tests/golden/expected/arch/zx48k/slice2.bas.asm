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
	call .core.CLS
	ld hl, .LABEL.__LABEL0
	call .core.__LOADSTR
	push hl
	xor a
	push af
	ld a, 10
	push af
	call _doubleSizePrint
	ld a, 8
	call .core.__STOP
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
_doubleSizePrint:
	push ix
	ld ix, 0
	add ix, sp
	ld hl, 0
	push hl
	push hl
	inc sp
	ld (ix-1), 0
	jp .LABEL.__LABEL1
.LABEL.__LABEL4:
	ld l, (ix+8)
	ld h, (ix+9)
	push hl
	ld a, (ix-1)
	ld l, a
	ld h, 0
	push hl
	ld a, (ix-1)
	ld l, a
	ld h, 0
	push hl
	xor a
	call .core.__STRSLICE
	ld d, h
	ld e, l
	ld bc, -3
	call .core.__PSTORE_STR2
	ld a, (ix+7)
	add a, 2
	ld (ix+7), a
.LABEL.__LABEL5:
	inc (ix-1)
.LABEL.__LABEL1:
	ld a, (ix-1)
	push af
	ld l, (ix+8)
	ld h, (ix+9)
	call .core.__STRLEN
	dec hl
	ld a, l
	pop hl
	cp h
	jp nc, .LABEL.__LABEL4
.LABEL.__LABEL3:
_doubleSizePrint__leave:
	ex af, af'
	exx
	ld l, (ix+8)
	ld h, (ix+9)
	call .core.__MEM_FREE
	ld l, (ix-3)
	ld h, (ix-2)
	call .core.__MEM_FREE
	ex af, af'
	exx
	ld sp, ix
	pop ix
	exx
	pop hl
	pop bc
	pop bc
	ex (sp), hl
	exx
	ret
.LABEL.__LABEL0:
	DEFW 000Bh
	DEFB 48h
	DEFB 65h
	DEFB 6Ch
	DEFB 6Ch
	DEFB 6Fh
	DEFB 20h
	DEFB 57h
	DEFB 6Fh
	DEFB 72h
	DEFB 6Ch
	DEFB 64h
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
CLS:
	    PROC
	    ld hl, 0
	    ld (COORDS), hl
	    ld hl, SCR_SIZE
	    ld (S_POSN), hl
	    ld hl, (SCREEN_ADDR)
	    ld (DFCC), hl
	    ld (hl), 0
	    ld d, h
	    ld e, l
	    inc de
	    ld bc, 6143
	    ldir
	    ld hl, (SCREEN_ATTR_ADDR)
	    ld (DFCCL), hl
	    ld d, h
	    ld e, l
	    inc de
	    ld a, (ATTR_P)
	    ld (hl), a
	    ld bc, 767
	    ldir
	    ret
	    ENDP
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
__PISTORE_STR2: ; Indirect store temporary string at (IX + BC)
	    push ix
	    pop hl
	    add hl, bc
__ISTORE_STR2:
	    ld c, (hl)  ; Dereferences HL
	    inc hl
	    ld h, (hl)
	    ld l, c		; HL = *HL (real string variable address)
__STORE_STR2:
	    push hl
	    ld c, (hl)
	    inc hl
	    ld h, (hl)
	    ld l, c		; HL = *HL (real string address)
	    push de
	    call __MEM_FREE
	    pop de
	    pop hl
	    ld (hl), e
	    inc hl
	    ld (hl), d
	    dec hl		; HL points to mem address variable. This might be useful in the future.
	    ret
	    pop namespace
	    push namespace core
__PSTORE_STR2:
	    push ix
	    pop hl
	    add hl, bc
	    jp __STORE_STR2
	    pop namespace
	    push namespace core
__STRLEN:	; Direct FASTCALL entry
	    ld a, h
	    or l
	    ret z
	    ld a, (hl)
	    inc hl
	    ld h, (hl)  ; LEN(str) in HL
	    ld l, a
	    ret
	    pop namespace
	    push namespace core
__STRSLICE:			; Callee entry
	    pop hl			; Return ADDRESS
	    pop bc			; Last char pos
	    pop de			; 1st char pos
	    ex (sp), hl		; CALLEE. -> String start
__STRSLICE_FAST:	; __FASTCALL__ Entry
	    PROC
	    LOCAL __CONT
	    LOCAL __EMPTY
	    LOCAL __FREE_ON_EXIT
	    push hl			; Stores original HL pointer to be recovered on exit
	    ex af, af'		; Saves A register for later
	    push hl
	    call __STRLEN
	    inc bc			; Last character position + 1 (string starts from 0)
	    or a
	    sbc hl, bc		; Compares length with last char position
	    jr nc, __CONT	; If Carry => We must copy to end of string
	    add hl, bc		; Restore back original LEN(a$) in HL
	    ld b, h
	    ld c, l			; Copy to the end of str
	    ccf				; Clears Carry flag for next subtraction
__CONT:
	    ld h, b
	    ld l, c			; HL = Last char position to copy (1 for char 0, 2 for char 1, etc)
	    sbc hl, de		; HL = LEN(a$) - DE => Number of chars to copy
	    jr z, __EMPTY	; 0 Chars to copy => Return HL = 0 (NULL STR)
	    jr c, __EMPTY	; If Carry => Nothing to return (NULL STR)
	    ld b, h
	    ld c, l			; BC = Number of chars to copy
	    inc bc
	    inc bc			; +2 bytes for string length number
	    push bc
	    push de
	    call __MEM_ALLOC
	    pop de
	    pop bc
	    ld a, h
	    or l
	    jr z, __EMPTY	; Return if NULL (no memory)
	    dec bc
	    dec bc			; Number of chars to copy (Len of slice)
	    ld (hl), c
	    inc hl
	    ld (hl), b
	    inc hl			; Stores new string length
	    ex (sp), hl		; Pointer to A$ now in HL; Pointer to new string chars in Stack
	    inc hl
	    inc hl			; Skip string length
	    add hl, de		; Were to start from A$
	    pop de			; Start of new string chars
	    push de			; Stores it again
	    ldir			; Copies BC chars
	    pop de
	    dec de
	    dec de			; Points to String LEN start
	    ex de, hl		; Returns it in HL
	    jr __FREE_ON_EXIT
__EMPTY:			; Return NULL (empty) string
	    pop hl
	    ld hl, 0		; Return NULL
__FREE_ON_EXIT:
	    ex af, af'		; Recover original A register
	    ex (sp), hl		; Original HL pointer
	    or a
	    call nz, __MEM_FREE
	    pop hl			; Recover result
	    ret
	    ENDP
	    pop namespace
	END
