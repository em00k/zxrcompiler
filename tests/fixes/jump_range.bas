' A jump that is found to be out of range once its label is known. The error
' is of this file, line 7, not of the file the assembler read last.
#include "jump_range_inc.bas"
Later()
asm
    nop
    jr far
    ds 300
far:
end asm
