' The address of a 32 bit PEEK given as a CONST is as immediate as one given
' as a number: all three load HL with 49152.
const BUF as uinteger = $C000
dim a, b, c as ulong
a = peek(ulong, BUF)
b = peek(ulong, $C000)
c = peek(ulong, BUF + 0)
