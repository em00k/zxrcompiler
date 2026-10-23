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
	DEFB 00, 00
.core.ZXBASIC_USER_DATA_END:
.core.__MAIN_PROGRAM__:
	ld hl, .LABEL.__LABEL0
	push hl
	xor a
	push af
	ld hl, 1
	push hl
	ld hl, 1
	push hl
	ld hl, (_a)
	call .core.__LETSUBSTR
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
	DEFW 0001h
	DEFB 2Eh
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
__LETSUBSTR:
	    PROC
	    LOCAL __CONT0
	    LOCAL __CONT1
	    LOCAL __CONT2
	    LOCAL __FREE_STR
	    exx
	    pop hl ; Return address
	    pop de ; p1
	    pop bc ; p0
	    exx
	    pop af ; Flag
	    ex af, af'	; Save it for later
	    pop de ; B$
	    exx
	    push hl ; push ret addr back
	    exx
	    push de ; B$ addr to be freed upon return (if A != 0)
	    ld a, h
	    or l
	    jp z, __FREE_STR ; Return if null
	    ld c, (hl)
	    inc hl
	    ld b, (hl) ; BC = Str length
	    inc hl	; HL = String start
	    push bc
	    exx
	    ex de, hl
	    or a
	    sbc hl, bc ; HL = Length of string requested by user
	    inc hl	   ; len (a$(p0 TO p1)) = p1 - p0 + 1
	    ex de, hl  ; Saves it in DE
	    pop hl	   ; HL = String length
	    exx
	    jp c, __FREE_STR	   ; Return if p0 > p1
	    exx
	    or a
	    sbc hl, bc ; P0 >= String length?
	    exx
	    jp z, __FREE_STR	   ; Return if equal
	    jp c, __FREE_STR	   ; Return if greater
	    exx
	    add hl, bc ; Add it back
	    sbc hl, de ; Length of substring > string => Truncate it
	    add hl, de ; add it back
	    jr nc, __CONT0 ; Length of substring within a$
	    ld d, h
	    ld e, l	   ; Truncate length of substring to fit within the strlen
__CONT0:	   ; At this point DE = Length of substring to copy
	    push de
	    push bc
	    exx
	    pop bc
	    add hl, bc ; Start address (within a$) so copy from b$ (in DE)
	    push hl
	    exx
	    pop hl	   ; Start address (within a$) so copy from b$ (in DE)
	    ld b, d	   ; Length of string
	    ld c, e
	    ld (hl), ' '
	    ld d, h
	    ld e, l
	    inc de
	    dec bc
	    ld a, b
	    or c
	    jr z, __CONT2
	    ldir
__CONT2:
	    pop bc	; Recovers Length of string to copy
	    exx
	    ex de, hl  ; HL = Source, DE = Target
	    ld a, h
	    or l
	    jp z, __FREE_STR ; Return if B$ is NULL
	    ld c, (hl)
	    inc hl
	    ld b, (hl)
	    inc hl
	    ld a, b
	    or c
	    jp z, __FREE_STR ; Return if len(b$) = 0
	    push de
	    push hl
	    push bc
	    exx
	    pop hl	; LEN (b$)
	    or a
	    sbc hl, bc
	    add hl, bc
	    jr nc, __CONT1
	    ld b, h ; BC = len(to copy)
	    ld c, l
__CONT1:
	    pop hl
	    pop de
	    ldir	; Copy b$ into a$(x to y)
__FREE_STR:
	    pop hl
	    ex af, af'
	    or a		; If not 0, free
	    jp nz, __MEM_FREE
	    ret
	    ENDP
	    pop namespace
	END
