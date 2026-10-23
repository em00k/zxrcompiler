# Grammar of zxbasic, extracted from the Python compiler's PLY rules.

%token ABS ACS ADDRESSOF AND ARRAY_ID AS ASM ASN AT ATN BAND BEEP BIN BNOT BOLD BOR BORDER BRIGHT BXOR BYREF BYTE BYVAL CAST CHR CIRCLE CLS CO CODE CODEBANK COMMA CONST CONTINUE COS DATA DECLARE DIM DIV DO DRAW ELSE ELSEIF END ENDIF EQ ERROR EXIT EXP FARPTR FASTCALL FIXED FLASH FLOAT FOR FUNCTION GE GO GOSUB GOTO GT ID IF IN INK INKEY INT INTEGER INVERSE ITALIC LABEL LBOUND LBRACE LE LEN LET LN LOAD LONG LOOP LP LT MINUS MOD MUL NE NEWLINE NEXT NOT NUMBER ON OR OUT OVER PAPER PAUSE PEEK PI PLOT PLUS POKE POW PRINT RANDOMIZE RBRACE READ RESTORE RETURN RIGHTARROW RND RP SAVE SC SGN SHL SHR SIN SIZEOF SQR STDCALL STEP STOP STR STRC STRING SUB TAB TAN THEN TO UBOUND UBYTE UINTEGER ULONG UNTIL USR VAL VERIFY WEND WEQ WHILE XOR _INIT _LINE _POP _PRAGMA _PUSH _REQUIRE
%nonassoc ID ARRAY_ID
%left OR
%left AND
%left XOR
%right NOT
%left LT GT EQ LE GE NE
%left BOR
%left BAND BXOR SHR SHL
%left BNOT PLUS MINUS
%left MOD
%left MUL DIV
%right UMINUS
%right POW
%left RP
%right LP
%right ELSE
%left CO
%left LABEL
%left NEWLINE
%start start

@p_start
start : program

@p_program_program_line
program : program_line

@p_program
program : program program_line

@p_program_line
program_line : preproc_line NEWLINE
program_line : label_line NEWLINE
program_line : statements NEWLINE
program_line : statements_co NEWLINE
program_line : co_statements NEWLINE
program_line : co_statements_co NEWLINE
program_line : NEWLINE

@p_co_statements_co
co_statements_co : co_statements CO
co_statements_co : co_statements_co CO
co_statements_co : CO

@p_co_statements
co_statements : co_statements_co statement

@p_statements_co
statements_co : statements CO
statements_co : statements_co CO

@p_statements_statement
statements : statement
statements : statements_co statement

@p_var_decls
statement : var_decl

@p_label
label : LABEL

@p_program_line_label
label_line : label statements
label_line : label co_statements

@p_label_line_label_line_co
label_line : label_line_co

@p_label_line_co
label_line_co : label statements_co %prec CO
label_line_co : label co_statements_co %prec CO
label_line_co : label %prec CO

@p_program_line_co
program_co : program %prec CO
program_co : program label_line_co
program_co : program co_statements_co %prec CO
program_co : program statements_co %prec CO

@p_var_decl
var_decl : DIM idlist typedef

@p_var_decl_at
var_decl : DIM idlist typedef AT expr

@p_var_decl_ini
var_decl : DIM idlist typedef EQ expr
var_decl : CONST idlist typedef EQ expr

@p_singleid
singleid : ID
singleid : ARRAY_ID

@p_idlist_id
idlist : singleid

@p_idlist_idlist_id
idlist : idlist COMMA singleid

@p_arr_decl
var_decl : var_arr_decl
var_decl : var_arr_decl_addr

@p_arr_decl_attr
var_arr_decl_addr : var_arr_decl AT expr

@p_decl_arr
var_arr_decl : DIM idlist LP bound_list RP typedef

@p_arr_decl_initialized
var_decl : DIM idlist LP bound_list RP typedef RIGHTARROW const_vector
var_decl : DIM idlist LP bound_list RP typedef EQ const_vector

@p_bound_list
bound_list : bound

@p_bound_list_bound
bound_list : bound_list COMMA bound

@p_bound
bound : expr

@p_bound_to_bound
bound : expr TO expr

@p_const_vector
const_vector : LBRACE const_vector_list RBRACE
const_vector : LBRACE const_number_list RBRACE

@p_const_vector_elem_list
const_number_list : expr

@p_const_vector_elem_list_list
const_number_list : const_number_list COMMA expr

@p_const_vector_list
const_vector_list : const_vector

@p_const_vector_vector_list
const_vector_list : const_vector_list COMMA const_vector

@p_staement_func_decl
statement : function_declaration

@p_statement_border
statement : BORDER expr

@p_statement_plot
statement : PLOT expr COMMA expr

@p_statement_plot_attr
statement : PLOT attr_list expr COMMA expr

@p_statement_draw3
statement : DRAW expr COMMA expr COMMA expr

@p_statement_draw3_attr
statement : DRAW attr_list expr COMMA expr COMMA expr

@p_statement_draw
statement : DRAW expr COMMA expr

@p_statement_draw_attr
statement : DRAW attr_list expr COMMA expr

@p_statement_circle
statement : CIRCLE expr COMMA expr COMMA expr

@p_statement_circle_attr
statement : CIRCLE attr_list expr COMMA expr COMMA expr

@p_statement_cls
statement : CLS

@p_statement_asm
statement : ASM

@p_statement_randomize
statement : RANDOMIZE

@p_statement_randomize_expr
statement : RANDOMIZE expr

@p_statement_beep
statement : BEEP expr COMMA expr

@p_statement_call
statement : ID arg_list
statement : ID arguments
statement : ID

@p_assignment
statement : lexpr expr

@p_lexpr
lexpr : ID EQ
lexpr : LET ID EQ

@p_array_copy
statement : ARRAY_ID EQ ARRAY_ID
statement : LET ARRAY_ID EQ ARRAY_ID

@p_arr_assignment
statement : ARRAY_ID arg_list EQ expr
statement : LET ARRAY_ID arg_list EQ expr

@p_substr_assignment_no_let
statement : ID LP expr RP EQ expr

@p_substr_assignment
statement : LET ID arg_list EQ expr

@p_str_assign
statement : ID substr EQ expr
statement : LET ID substr EQ expr

@p_goto
statement : goto NUMBER
statement : goto ID

@p_go
goto : GO TO
goto : GO SUB
goto : GOTO
goto : GOSUB

@p_if_sentence
statement : if_then_part NEWLINE program_co endif
statement : if_then_part NEWLINE endif
statement : if_then_part NEWLINE statements_co endif
statement : if_then_part NEWLINE co_statements_co endif
statement : if_then_part NEWLINE label statements_co endif

@p_endif
endif : END IF
endif : label END IF
endif : ENDIF
endif : label ENDIF

@p_statement_if
statement : if_inline %prec RP

@p_statement_if_then_endif
statement : if_then_part statements_co endif
statement : if_then_part co_statements_co endif

@p_single_line_if
if_inline : if_then_part statements %prec ID
if_inline : if_then_part co_statements_co %prec NEWLINE
if_inline : if_then_part statements_co %prec NEWLINE
if_inline : if_then_part co_statements %prec ID

@p_if_elseif
statement : if_then_part NEWLINE program_co elseiflist
statement : if_then_part NEWLINE elseiflist

@p_elseif_part
elseif_expr : ELSEIF expr then
elseif_expr : label ELSEIF expr then

@p_elseif_list
elseiflist : elseif_expr program_co endif
elseiflist : elseif_expr program_co else_part

@p_elseif_elseiflist
elseiflist : elseif_expr program_co elseiflist

@p_else_part_endif
else_part_inline : ELSE NEWLINE program_co endif
else_part_inline : ELSE NEWLINE statements_co endif
else_part_inline : ELSE NEWLINE co_statements_co endif
else_part_inline : ELSE NEWLINE endif
else_part_inline : ELSE NEWLINE label statements_co endif
else_part_inline : ELSE NEWLINE label co_statements_co endif
else_part_inline : ELSE statements_co endif
else_part_inline : ELSE co_statements_co endif

@p_else_part
else_part_inline : ELSE statements_co
else_part_inline : ELSE co_statements_co
else_part_inline : ELSE co_statements
else_part_inline : ELSE statements

@p_else_part_is_inline
else_part : else_part_inline

@p_else_part_label
else_part : label ELSE program_co endif
else_part : label ELSE statements_co endif
else_part : label ELSE co_statements_co endif

@p_if_then_part
if_then_part : IF expr then

@p_if_inline
statement : if_inline else_part_inline

@p_if_else
statement : if_then_part NEWLINE program_co else_part

@p_then
then :
then : THEN

@p_for_sentence
statement : for_start program_co label_next
statement : for_start co_statements_co label_next
statement : for_start program label_next

@p_next
label_next : label NEXT
label_next : NEXT

@p_next1
label_next : label NEXT ID
label_next : NEXT ID

@p_for_sentence_start
for_start : FOR ID EQ expr TO expr step

@p_step
step :

@p_step_expr
step : STEP expr

@p_end
statement : END expr
statement : END

@p_error_raise
statement : ERROR expr

@p_stop_raise
statement : STOP expr
statement : STOP

@p_loop
label_loop : label LOOP
label_loop : LOOP

@p_do_loop
statement : do_start program_co label_loop
statement : do_start label_loop
statement : DO label_loop

@p_do_loop_until
statement : do_start program_co label_loop UNTIL expr
statement : do_start label_loop UNTIL expr
statement : DO label_loop UNTIL expr

@p_data
statement : DATA arguments

@p_restore
statement : RESTORE
statement : RESTORE ID
statement : RESTORE NUMBER

@p_read
statement : READ arguments

@p_do_loop_while
statement : do_start program_co label_loop WHILE expr
statement : do_start label_loop WHILE expr
statement : DO label_loop WHILE expr

@p_do_while_loop
statement : do_while_start program_co LOOP
statement : do_while_start co_statements_co LOOP
statement : do_while_start LOOP

@p_do_until_loop
statement : do_until_start program_co LOOP
statement : do_until_start co_statements_co LOOP
statement : do_until_start LOOP

@p_do_while_start
do_while_start : DO WHILE expr

@p_do_until_start
do_until_start : DO UNTIL expr

@p_do_start
do_start : DO CO
do_start : DO NEWLINE

@p_label_end_while
label_end_while : label WEND
label_end_while : label END WHILE
label_end_while : WEND
label_end_while : END WHILE

@p_while_sentence
statement : while_start co_statements_co label_end_while
statement : while_start program_co label_end_while

@p_while_start
while_start : WHILE expr

@p_exit
statement : EXIT WHILE
statement : EXIT DO
statement : EXIT FOR

@p_continue
statement : CONTINUE WHILE
statement : CONTINUE DO
statement : CONTINUE FOR

@p_print_sentence
statement : PRINT print_list

@p_print_elem_expr
print_elem : expr

@p_print_list_expr
print_elem : print_at
print_elem : print_tab
print_elem : attr
print_elem : BOLD expr
print_elem : ITALIC expr

@p_attr_list
attr_list : attr SC

@p_attr_list_list
attr_list : attr_list attr SC

@p_attr
attr : OVER expr
attr : INVERSE expr
attr : INK expr
attr : PAPER expr
attr : BRIGHT expr
attr : FLASH expr

@p_print_list_epsilon
print_elem :

@p_print_list_elem
print_list : print_elem

@p_print_list
print_list : print_list SC print_elem

@p_print_list_comma
print_list : print_list COMMA print_elem

@p_print_list_at
print_at : AT expr COMMA expr

@p_print_list_tab
print_tab : TAB expr

@p_on_goto
statement : ON expr goto label_list

@p_label_list
label_list : ID
label_list : NUMBER

@p_label_list_list
label_list : label_list COMMA ID
label_list : label_list COMMA NUMBER

@p_return
statement : RETURN

@p_return_expr
statement : RETURN expr

@p_pause
statement : PAUSE expr

@p_poke
statement : POKE expr COMMA expr
statement : POKE LP expr COMMA expr RP

@p_poke2
statement : POKE numbertype expr COMMA expr
statement : POKE LP numbertype expr COMMA expr RP

@p_poke3
statement : POKE numbertype COMMA expr COMMA expr
statement : POKE LP numbertype COMMA expr COMMA expr RP

@p_out
statement : OUT expr COMMA expr

@p_simple_instruction
statement : ITALIC expr
statement : BOLD expr
statement : INK expr
statement : PAPER expr
statement : BRIGHT expr
statement : FLASH expr
statement : OVER expr
statement : INVERSE expr

@p_save_code
statement : SAVE expr CODE expr COMMA expr
statement : SAVE expr ID
statement : SAVE expr ARRAY_ID

@p_save_data
statement : SAVE expr DATA
statement : SAVE expr DATA ID
statement : SAVE expr DATA ID LP RP

@p_load_or_verify
load_or_verify : LOAD
load_or_verify : VERIFY

@p_load_code
statement : load_or_verify expr ID
statement : load_or_verify expr CODE
statement : load_or_verify expr CODE expr
statement : load_or_verify expr CODE expr COMMA expr

@p_load_data
statement : load_or_verify expr DATA
statement : load_or_verify expr DATA ID
statement : load_or_verify expr DATA ID LP RP

@p_numbertype
numbertype : BYTE
numbertype : UBYTE
numbertype : INTEGER
numbertype : UINTEGER
numbertype : LONG
numbertype : ULONG
numbertype : FIXED
numbertype : FLOAT

@p_expr_plus_expr
expr : expr PLUS expr

@p_expr_minus_expr
expr : expr MINUS expr

@p_expr_mul_expr
expr : expr MUL expr

@p_expr_div_expr
expr : expr DIV expr

@p_expr_mod_expr
expr : expr MOD expr

@p_expr_pow_expr
expr : expr POW expr

@p_expr_shl_expr
expr : expr SHL expr

@p_expr_shr_expr
expr : expr SHR expr

@p_minus_expr
expr : MINUS expr %prec UMINUS

@p_expr_EQ_expr
expr : expr EQ expr

@p_expr_LT_expr
expr : expr LT expr

@p_expr_LE_expr
expr : expr LE expr

@p_expr_GT_expr
expr : expr GT expr

@p_expr_GE_expr
expr : expr GE expr

@p_expr_NE_expr
expr : expr NE expr

@p_expr_OR_expr
expr : expr OR expr

@p_expr_BOR_expr
expr : expr BOR expr

@p_expr_XOR_expr
expr : expr XOR expr

@p_expr_BXOR_expr
expr : expr BXOR expr

@p_expr_AND_expr
expr : expr AND expr

@p_expr_BAND_expr
expr : expr BAND expr

@p_NOT_expr
expr : NOT expr

@p_BNOT_expr
expr : BNOT expr

@p_lp_expr_rp
bexpr : LP expr RP %prec ID

@p_cast
expr : CAST LP numbertype COMMA expr RP

@p_number_expr
bexpr : NUMBER

@p_expr_PI
bexpr : PI

@p_expr_string
bexpr : string %prec ID

@p_string_func_call
string : func_call substr

@p_string_func_call_single
string : func_call LP expr RP

@p_string_str
string : STRC

@p_string_lprp
string : string LP RP

@p_string_lp_expr_rp
string : string LP expr RP

@p_expr_id_substr
string : ID substr

@p_string_substr
string : string substr

@p_string_expr_lp
string : LP expr RP substr

@p_subind_str
substr : LP expr TO expr RP

@p_subind_strTO
substr : LP TO expr RP

@p_subind_TOstr
substr : LP expr TO RP

@p_subind_TO
substr : LP TO RP

@p_id_expr
bexpr : ID

@p_addr_of_id
bexpr : ADDRESSOF singleid

@p_farptr_of_id
bexpr : FARPTR singleid

@p_farptr_of_array_element
bexpr : FARPTR ARRAY_ID arg_list

@p_err_farptr_undefined_arr
bexpr : FARPTR ID arg_list

@p_expr_bexpr
expr : bexpr

@p_expr_funccall
bexpr : func_call %prec ID

@p_idcall_expr
func_call : ID arg_list %prec UMINUS

@p_array_eq_error
statement : LET ARRAY_ID EQ expr

@p_arr_access_expr
func_call : ARRAY_ID arg_list

@p_let_arr_substr
statement : LET ARRAY_ID arg_list substr EQ expr
statement : ARRAY_ID arg_list substr EQ expr

@p_let_arr_substr_single
statement : LET ARRAY_ID arg_list LP expr RP EQ expr
statement : ARRAY_ID arg_list LP expr RP EQ expr

@p_let_arr_substr_in_args
statement : LET ARRAY_ID LP arguments TO RP EQ expr
statement : ARRAY_ID LP arguments TO RP EQ expr

@p_let_arr_substr_in_args2
statement : LET ARRAY_ID LP arguments COMMA TO expr RP EQ expr
statement : ARRAY_ID LP arguments COMMA TO expr RP EQ expr

@p_let_arr_substr_in_args3
statement : LET ARRAY_ID LP arguments COMMA TO RP EQ expr
statement : ARRAY_ID LP arguments COMMA TO RP EQ expr

@p_let_arr_substr_in_args4
statement : LET ARRAY_ID LP arguments TO expr RP EQ expr
statement : ARRAY_ID LP arguments TO expr RP EQ expr

@p_addr_of_array_element
bexpr : ADDRESSOF ARRAY_ID arg_list

@p_err_undefined_arr_access
bexpr : ADDRESSOF ID arg_list

@p_bexpr_func
bexpr : ID bexpr

@p_arg_list
arg_list : LP RP

@p_arg_list_arg
arg_list : LP arguments RP

@p_arguments
arguments : argument

@p_arguments_argument
arguments : arguments COMMA argument

@p_argument
argument : expr %prec ID

@p_named_argument
argument : ID WEQ expr %prec ID

@p_argument_array
argument : ARRAY_ID

@p_funcdecl
function_declaration : function_header function_body

@p_funcdeclforward
function_declaration : DECLARE function_header_pre

@p_function_header
function_header : function_header_pre CO
function_header : function_header_pre NEWLINE

@p_function_header_error
function_header : function_def error CO
function_header : function_def error NEWLINE

@p_function_header_pre
function_header_pre : function_def param_decl typedef

@p_function_error
function_declaration : function_header program_co END error

@p_function_def
function_def : FUNCTION convention ID
function_def : SUB convention ID

@p_convention
convention :
convention : STDCALL

@p_convention2
convention : FASTCALL

@p_param_decl_none
param_decl :
param_decl : LP RP

@p_param_decl
param_decl : LP param_decl_list RP

@p_param_decl_errpr
param_decl : LP error RP

@p_param_decl_list
param_decl_list : param_definition

@p_param_decl_list2
param_decl_list : param_decl_list COMMA param_definition

@p_param_byref_definition
param_definition : BYREF param_def

@p_param_byval_definition
param_definition : BYVAL param_def

@p_param_definition
param_definition : param_def

@p_param_def_array
param_def : singleid LP RP typedef

@p_param_def_type
param_def : singleid typedef default_arg_value

@p_param_def_default_arg_value
default_arg_value :
default_arg_value : EQ expr

@p_function_body
function_body : program_co END FUNCTION
function_body : program_co END SUB
function_body : statements_co END FUNCTION
function_body : statements_co END SUB
function_body : co_statements_co END FUNCTION
function_body : co_statements_co END SUB
function_body : END FUNCTION
function_body : END SUB

@p_type_def_empty
typedef :

@p_type_def
typedef : AS type

@p_type
type : BYTE
type : UBYTE
type : INTEGER
type : UINTEGER
type : LONG
type : ULONG
type : FIXED
type : FLOAT
type : STRING

@p_preproc_line_init
preproc_line : _INIT ID

@p_preproc_line_require
preproc_line : _REQUIRE STRING

@p_preproc_line_pragma_option
preproc_line : _PRAGMA ID EQ ID
preproc_line : _PRAGMA ID EQ STRING
preproc_line : _PRAGMA ID EQ INTEGER

@p_codebank_begin
preproc_line : CODEBANK NUMBER
preproc_line : CODEBANK INTEGER

@p_codebank_end
preproc_line : END CODEBANK

@p_preproc_pragma_push
preproc_line : _PRAGMA _PUSH LP ID RP

@p_preproc_pragma_pop
preproc_line : _PRAGMA _POP LP ID RP

@p_expr_usr
bexpr : USR bexpr %prec UMINUS

@p_expr_rnd
bexpr : RND %prec ID
bexpr : RND LP RP

@p_expr_peek
bexpr : PEEK bexpr %prec UMINUS

@p_expr_peektype_
bexpr : PEEK LP numbertype COMMA expr RP

@p_expr_in
bexpr : IN bexpr %prec UMINUS

@p_expr_lbound
bexpr : LBOUND LP ARRAY_ID RP
bexpr : UBOUND LP ARRAY_ID RP

@p_expr_lbound_expr
bexpr : LBOUND LP ARRAY_ID COMMA expr RP
bexpr : UBOUND LP ARRAY_ID COMMA expr RP

@p_len
bexpr : LEN bexpr %prec UMINUS

@p_sizeof
bexpr : SIZEOF LP type RP
bexpr : SIZEOF LP ID RP
bexpr : SIZEOF LP ARRAY_ID RP

@p_str
string : STR expr %prec UMINUS

@p_inkey
string : INKEY

@p_chr_one
string : CHR bexpr %prec UMINUS

@p_chr
string : CHR arg_list

@p_val
bexpr : VAL bexpr %prec UMINUS

@p_code
bexpr : CODE bexpr %prec UMINUS

@p_sgn
bexpr : SGN bexpr %prec UMINUS

@p_expr_trig
bexpr : math_fn bexpr %prec UMINUS

@p_math_fn
math_fn : SIN
math_fn : COS
math_fn : TAN
math_fn : ASN
math_fn : ACS
math_fn : ATN
math_fn : LN
math_fn : EXP
math_fn : SQR

@p_expr_int
bexpr : INT bexpr %prec UMINUS

@p_abs
bexpr : ABS bexpr %prec UMINUS
