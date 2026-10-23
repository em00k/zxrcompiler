# Grammar of zxbasm, extracted from the Python compiler's PLY rules.

%token A ADC ADD ADDR AF ALIGN AND APO B BAND BC BIT BOR BRLC BSLA BSRA BSRF BSRL BXOR C CALL CCF CO CODEBANK COMMA CP CPD CPDR CPI CPIR CPL D DAA DE DEC DEFB DEFS DEFW DI DIV DJNZ E EI END ENDP EQU EX EXX H HALT HL I ID IM IN INC INCBIN IND INDR INI INIR INTEGER IX IXH IXL IY IYH IYL JP JR L LB LD LDD LDDR LDDRX LDDX LDI LDIR LDIRX LDIX LDPIRX LDWS LOCAL LP LPP LSHIFT M MINUS MIRROR MOD MUL NAMESPACE NC NEG NEWLINE NEXTREG NOP NZ OR ORG OTDR OTIR OUT OUTD OUTI OUTINB P PE PIXELAD PIXELDN PLUS PO POP POW PROC PUSH R RB RES RET RETI RETN RL RLA RLC RLCA RLD RP RPP RR RRA RRC RRCA RRD RSHIFT RST SBC SCF SET SETAE SLA SLL SP SRA SRL STRING SUB SWAPNIB TEST UMINUS XOR Z _INIT
%left RSHIFT LSHIFT BAND BOR BXOR
%left PLUS MINUS
%left MUL DIV MOD
%right POW
%right UMINUS
%start start

@p_start
start : program
start : program endline

@p_program_endline
endline : END NEWLINE
endline : endline NEWLINE

@p_program_endline2
endline : END expr NEWLINE
endline : END pexpr NEWLINE

@p_program
program : line

@p_program_line
program : program line

@p_def_label
line : ID EQU expr NEWLINE
line : ID EQU pexpr NEWLINE

@p_line_asm
line : asms NEWLINE
line : asms CO NEWLINE

@p_asms_empty
asms :

@p_asms_asm
asms : asm

@p_asms_asms_asm
asms : asms CO asm

@p_asm_label
asm : ID
asm : INTEGER

@p_asm_ld8
asm : LD reg8 COMMA reg8_hl
asm : LD reg8_hl COMMA reg8
asm : LD reg8 COMMA reg8
asm : LD SP COMMA HL
asm : LD SP COMMA reg16i
asm : LD A COMMA reg8
asm : LD reg8 COMMA A
asm : LD reg8_hl COMMA A
asm : LD A COMMA reg8_hl
asm : LD A COMMA A
asm : LD A COMMA I
asm : LD I COMMA A
asm : LD A COMMA R
asm : LD R COMMA A
asm : LD A COMMA reg8i
asm : LD reg8i COMMA A
asm : LD reg8 COMMA reg8i
asm : LD reg8i COMMA regBCDE
asm : LD reg8i COMMA reg8i

@p_LDa
asm : LD A COMMA LP BC RP
asm : LD A COMMA LB BC RB
asm : LD A COMMA LP DE RP
asm : LD A COMMA LB DE RB
asm : LD LP BC RP COMMA A
asm : LD LB BC RB COMMA A
asm : LD LP DE RP COMMA A
asm : LD LB DE RB COMMA A

@p_PROC
asm : PROC

@p_ENDP
asm : ENDP

@p_LOCAL
asm : LOCAL id_list

@p_idlist
id_list : ID

@p_idlist_id
id_list : id_list COMMA ID

@p_DEFB
asm : DEFB expr_list

@p_DEFS
asm : DEFS number_list

@p_DEFW
asm : DEFW number_list

@p_expr_list_from_string
expr_list : STRING

@p_expr_list_from_num
expr_list : expr
expr_list : pexpr

@p_expr_list_plus_expr
expr_list : expr_list COMMA expr
expr_list : expr_list COMMA pexpr

@p_expr_list_plus_string
expr_list : expr_list COMMA STRING

@p_number_list
number_list : expr
number_list : pexpr

@p_number_list_number
number_list : number_list COMMA expr
number_list : number_list COMMA pexpr

@p_asm_ldind_r8
asm : LD reg8_I COMMA reg8
asm : LD reg8_I COMMA A

@p_asm_ldr8_ind
asm : LD reg8 COMMA reg8_I
asm : LD A COMMA reg8_I

@p_reg8_hl
reg8_hl : LP HL RP
reg8_hl : LB HL RB

@p_ind8_I
reg8_I : LP IX expr RP
reg8_I : LP IY expr RP
reg8_I : LP IX PLUS pexpr RP
reg8_I : LP IX MINUS pexpr RP
reg8_I : LP IY PLUS pexpr RP
reg8_I : LP IY MINUS pexpr RP
reg8_I : LB IX expr RB
reg8_I : LB IY expr RB
reg8_I : LB IX PLUS pexpr RB
reg8_I : LB IX MINUS pexpr RB
reg8_I : LB IY PLUS pexpr RB
reg8_I : LB IY MINUS pexpr RB

@p_ex_af_af
asm : EX AF COMMA AF APO

@p_ex_de_hl
asm : EX DE COMMA HL

@p_org
asm : ORG expr
asm : ORG pexpr

@p_namespace
asm : NAMESPACE ID

@p_push_namespace
asm : PUSH NAMESPACE
asm : PUSH NAMESPACE ID

@p_pop_namespace
asm : POP NAMESPACE

@p_align
asm : ALIGN expr
asm : ALIGN pexpr

@p_incbin
asm : INCBIN STRING
asm : INCBIN STRING COMMA expr
asm : INCBIN STRING COMMA expr COMMA expr

@p_ex_sp_reg8
asm : EX LP SP RP COMMA reg16i
asm : EX LB SP RB COMMA reg16i
asm : EX LP SP RP COMMA HL
asm : EX LB SP RB COMMA HL

@p_incdec
asm : INC inc_reg
asm : DEC inc_reg

@p_incdeci
asm : INC reg8_I
asm : DEC reg8_I

@p_LD_reg_val
asm : LD reg8 COMMA expr
asm : LD reg8 COMMA pexpr
asm : LD reg16 COMMA expr
asm : LD reg8_hl COMMA expr
asm : LD A COMMA expr
asm : LD SP COMMA expr
asm : LD reg8i COMMA expr

@p_LD_regI_val
asm : LD reg8_I COMMA expr

@p_JP_hl
asm : JP reg8_hl
asm : JP LP reg16i RP
asm : JP LB reg16i RB

@p_SBCADD
asm : SBC A COMMA reg8
asm : SBC A COMMA reg8i
asm : SBC A COMMA A
asm : SBC A COMMA reg8_hl
asm : SBC HL COMMA SP
asm : SBC HL COMMA BC
asm : SBC HL COMMA DE
asm : SBC HL COMMA HL
asm : ADD A COMMA reg8
asm : ADD A COMMA reg8i
asm : ADD A COMMA A
asm : ADD A COMMA reg8_hl
asm : ADC A COMMA reg8
asm : ADC A COMMA reg8i
asm : ADC A COMMA A
asm : ADC A COMMA reg8_hl
asm : ADD HL COMMA BC
asm : ADD HL COMMA DE
asm : ADD HL COMMA HL
asm : ADD HL COMMA SP
asm : ADC HL COMMA BC
asm : ADC HL COMMA DE
asm : ADC HL COMMA HL
asm : ADC HL COMMA SP
asm : ADD reg16i COMMA BC
asm : ADD reg16i COMMA DE
asm : ADD reg16i COMMA HL
asm : ADD reg16i COMMA SP
asm : ADD reg16i COMMA reg16i

@p_arith_A_expr
asm : SBC A COMMA expr
asm : SBC A COMMA pexpr
asm : ADD A COMMA expr
asm : ADD A COMMA pexpr
asm : ADC A COMMA expr
asm : ADC A COMMA pexpr

@p_arith_A_regI
asm : SBC A COMMA reg8_I
asm : ADD A COMMA reg8_I
asm : ADC A COMMA reg8_I

@p_bitwiseop_reg
asm : bitwiseop reg8
asm : bitwiseop reg8i
asm : bitwiseop A
asm : bitwiseop reg8_hl

@p_bitwiseop_regI
asm : bitwiseop reg8_I

@p_bitwise_expr
asm : bitwiseop expr
asm : bitwiseop pexpr

@p_bitwise
bitwiseop : OR
bitwiseop : AND
bitwiseop : XOR
bitwiseop : SUB
bitwiseop : CP

@p_PUSH_POP
asm : PUSH AF
asm : PUSH reg16
asm : POP AF
asm : POP reg16

@p_LD_addr_reg
asm : LD pexpr COMMA A
asm : LD pexpr COMMA reg16
asm : LD pexpr COMMA SP
asm : LD mem_indir COMMA A
asm : LD mem_indir COMMA reg16
asm : LD mem_indir COMMA SP

@p_LD_reg_addr
asm : LD A COMMA pexpr
asm : LD reg16 COMMA pexpr
asm : LD SP COMMA pexpr
asm : LD A COMMA mem_indir
asm : LD reg16 COMMA mem_indir
asm : LD SP COMMA mem_indir

@p_ROTATE
asm : rotation reg8
asm : rotation reg8_hl
asm : rotation A

@p_ROTATE_ix
asm : rotation reg8_I

@p_BIT
asm : bitop expr COMMA A
asm : bitop pexpr COMMA A
asm : bitop expr COMMA reg8
asm : bitop pexpr COMMA reg8
asm : bitop expr COMMA reg8_hl
asm : bitop pexpr COMMA reg8_hl

@p_BIT_ix
asm : bitop expr COMMA reg8_I
asm : bitop pexpr COMMA reg8_I

@p_bitop
bitop : BIT
bitop : RES
bitop : SET

@p_rotation
rotation : RR
rotation : RL
rotation : RRC
rotation : RLC
rotation : SLA
rotation : SLL
rotation : SRA
rotation : SRL

@p_reg_inc
inc_reg : SP
inc_reg : reg8
inc_reg : reg16
inc_reg : reg8_hl
inc_reg : A
inc_reg : reg8i

@p_reg8
reg8 : H
reg8 : L
reg8 : regBCDE

@p_regBCDE
regBCDE : B
regBCDE : C
regBCDE : D
regBCDE : E

@p_reg8i
reg8i : IXH
reg8i : IXL
reg8i : IYH
reg8i : IYL

@p_reg16
reg16 : BC
reg16 : DE
reg16 : HL
reg16 : reg16i

@p_reg16i
reg16i : IX
reg16i : IY

@p_jp
asm : JP jp_flags COMMA expr
asm : JP jp_flags COMMA pexpr
asm : CALL jp_flags COMMA expr
asm : CALL jp_flags COMMA pexpr

@p_ret
asm : RET jp_flags

@p_jpflags_other
jp_flags : P
jp_flags : M
jp_flags : PO
jp_flags : PE
jp_flags : jr_flags

@p_jr
asm : JR jr_flags COMMA expr
asm : JR jr_flags COMMA pexpr

@p_jr_flags
jr_flags : Z
jr_flags : C
jr_flags : NZ
jr_flags : NC

@p_jrjp
asm : JP expr
asm : JR expr
asm : CALL expr
asm : DJNZ expr
asm : JP pexpr
asm : JR pexpr
asm : CALL pexpr
asm : DJNZ pexpr

@p_rst
asm : RST expr

@p_im
asm : IM expr

@p_in
asm : IN A COMMA LP C RP
asm : IN A COMMA LB C RB
asm : IN reg8 COMMA LP C RP
asm : IN reg8 COMMA LB C RB

@p_out
asm : OUT LP C RP COMMA A
asm : OUT LB C RB COMMA A
asm : OUT LP C RP COMMA reg8
asm : OUT LB C RB COMMA reg8

@p_in_expr
asm : IN A COMMA mem_indir
asm : IN A COMMA pexpr

@p_out_expr
asm : OUT mem_indir COMMA A
asm : OUT pexpr COMMA A

@p_single
asm : NOP
asm : EXX
asm : CCF
asm : SCF
asm : LDIR
asm : LDI
asm : LDDR
asm : LDD
asm : CPIR
asm : CPI
asm : CPDR
asm : CPD
asm : DAA
asm : NEG
asm : CPL
asm : HALT
asm : EI
asm : DI
asm : OUTD
asm : OUTI
asm : OTDR
asm : OTIR
asm : IND
asm : INI
asm : INDR
asm : INIR
asm : RET
asm : RETI
asm : RETN
asm : RLA
asm : RLCA
asm : RRA
asm : RRCA
asm : RLD
asm : RRD

@p_expr_div_expr
expr : expr BAND expr
expr : expr BOR expr
expr : expr BXOR expr
expr : expr PLUS expr
expr : expr MINUS expr
expr : expr MUL expr
expr : expr DIV expr
expr : expr MOD expr
expr : expr POW expr
expr : expr LSHIFT expr
expr : expr RSHIFT expr
expr : pexpr BAND expr
expr : pexpr BOR expr
expr : pexpr BXOR expr
expr : pexpr PLUS expr
expr : pexpr MINUS expr
expr : pexpr MUL expr
expr : pexpr DIV expr
expr : pexpr MOD expr
expr : pexpr POW expr
expr : pexpr LSHIFT expr
expr : pexpr RSHIFT expr
expr : expr BAND pexpr
expr : expr BOR pexpr
expr : expr BXOR pexpr
expr : expr PLUS pexpr
expr : expr MINUS pexpr
expr : expr MUL pexpr
expr : expr DIV pexpr
expr : expr MOD pexpr
expr : expr POW pexpr
expr : expr LSHIFT pexpr
expr : expr RSHIFT pexpr
expr : pexpr BAND pexpr
expr : pexpr BOR pexpr
expr : pexpr BXOR pexpr
expr : pexpr PLUS pexpr
expr : pexpr MINUS pexpr
expr : pexpr MUL pexpr
expr : pexpr DIV pexpr
expr : pexpr MOD pexpr
expr : pexpr POW pexpr
expr : pexpr LSHIFT pexpr
expr : pexpr RSHIFT pexpr

@p_expr_lprp
pexpr : LP expr RP
pexpr : LP pexpr RP

@p_mem_indir
mem_indir : LB expr RB

@p_expr_uminus
expr : MINUS expr %prec UMINUS
expr : PLUS expr %prec UMINUS
expr : MINUS pexpr %prec UMINUS
expr : PLUS pexpr %prec UMINUS

@p_expr_int
expr : INTEGER

@p_expr_label
expr : ID

@p_expr_paren
expr : LPP expr RPP

@p_expr_addr
expr : ADDR

@p_preprocessor_line
line : preproc_line

@p_preproc_line_init
preproc_line : _INIT STRING
