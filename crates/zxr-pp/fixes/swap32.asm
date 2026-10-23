; Exchanges current DE HL with the
; ones in the stack

; Never leaves a word that is still wanted below SP, where an interrupt
; would write over it. Changes AF: every routine called after it sets A
; and the flags before reading them.

    push namespace core

__SWAP32:
    pop bc ; Return address
    pop af ; Low word in the stack
    ex de, hl
    ex (sp), hl
    push de
    ex de, hl
    push af
    pop hl
    push bc
    ret

    pop namespace

