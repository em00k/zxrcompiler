# Grammar of zxbpp, extracted from the Python compiler's PLY rules.

%token AND CO COMMA CONTINUE DEFINE ELIF ELSE ENDIF EQ ERROR FILENAME GE GT ID IF IFDEF IFNDEF INCLUDE INIT INTEGER LB LE LINE LLP LP LT NE NEWLINE NUMBER ONCE OR PASTE POP PRAGMA PUSH RB REQUIRE RP RRP SEPARATOR STRING STRINGIZING TEXT TOKEN UNDEF WARNING _ENDFILE_
%nonassoc DUMMY
%left OR
%left AND
%left EQ NE LT LE GT GE
%right LLP
%left PASTE STRINGIZING
%start start

@p_start
start : program

@p_program
program : include_file
program : line
program : init
program : undef
program : ifdef
program : require
program : pragma
program : errormsg
program : warningmsg

@p_program_tokenstring
program : defs NEWLINE

@p_program_tokenstring_2
program : define NEWLINE

@p_program_char
program : program include_file
program : program line
program : program init
program : program undef
program : program ifdef
program : program require
program : program pragma
program : program errormsg
program : program warningmsg

@p_program_newline
program : program defs NEWLINE

@p_program_newline_2
program : program define NEWLINE

@p_token
token : STRING
token : TOKEN
token : CONTINUE
token : SEPARATOR
token : NUMBER

@p_include_file
include_file : include NEWLINE program _ENDFILE_

@p_include_file_empty
include_file : include NEWLINE _ENDFILE_

@p_include_once_empty
include_file : include_once NEWLINE _ENDFILE_

@p_include_once_ok
include_file : include_once NEWLINE program _ENDFILE_

@p_include_fname
include : INCLUDE include_modifier FILENAME

@p_include_macro
include : INCLUDE include_modifier expr

@p_include_once
include_once : INCLUDE ONCE include_modifier STRING

@p_include_once_fname
include_once : INCLUDE ONCE include_modifier FILENAME

@p_include_modifier
include_modifier :
include_modifier : LB ID CO ID RB

@p_line
line : LINE INTEGER NEWLINE

@p_line_file
line : LINE INTEGER STRING NEWLINE

@p_require_file
require : REQUIRE STRING NEWLINE

@p_init
init : INIT ID NEWLINE

@p_init_str
init : INIT STRING NEWLINE

@p_undef
undef : UNDEF ID

@p_errormsg
errormsg : ERROR TEXT

@p_warningmsg
warningmsg : WARNING TEXT

@p_define
define : DEFINE ID params defs

@p_define_params_epsilon
params :

@p_define_params_empty
params : LP RP

@p_define_params_paramlist
params : LP paramlist RP

@p_paramlist_single
paramlist : ID

@p_paramlist_paramlist
paramlist : paramlist COMMA ID

@p_pragma_id
pragma : PRAGMA ID

@p_pragma_id_expr
pragma : PRAGMA ID EQ ID
pragma : PRAGMA ID EQ INTEGER

@p_pragma_id_string
pragma : PRAGMA ID EQ STRING

@p_pragma_push
pragma : PRAGMA PUSH LP ID RP
pragma : PRAGMA POP LP ID RP

@p_pragma_once
pragma : PRAGMA ONCE

@p_ifdef
ifdef : if_header NEWLINE program ENDIF

@p_ifdef_else
ifdef : ifdefelsea ifdefelseb ENDIF

@p_ifdef_else_a
ifdefelsea : if_header NEWLINE program

@p_ifdef_else_b
ifdefelseb : ELSE NEWLINE program

@p_if_header
if_header : IFDEF ID

@p_ifn_header
if_header : IFNDEF ID

@p_if_expr_header
if_header : IF expr

@p_expr
expr : macrocall

@p_expr_val
expr : NUMBER

@p_expr_str
expr : STRING

@p_exprand
expr : expr AND expr

@p_expror
expr : expr OR expr

@p_exprne
expr : expr NE expr

@p_expreq
expr : expr EQ expr

@p_exprlt
expr : expr LT expr

@p_exprle
expr : expr LE expr

@p_exprgt
expr : expr GT expr

@p_exprge
expr : expr GE expr

@p_expr_par
expr : LLP expr RRP

@p_defs_list_eps
defs :

@p_defs_list
defs : defs def

@p_def
def : token
def : COMMA
def : RRP
def : LLP

@p_def_macrocall
def : macrocall %prec DUMMY

@p_macrocall
macrocall : ID

@p_macrocall_args
macrocall : macrocall args

@p_macrocall_paste
macrocall : macrocall PASTE macrocall

@p_macrocall_stringizing
macrocall : STRINGIZING macrocall

@p_args
args : LLP arglist RRP

@p_arglist
arglist : arglist COMMA arg

@p_arglist_arg
arglist : arg

@p_arg_eps
arg :

@p_arg_argstring
arg : argstring

@p_argstring
argstring : token
argstring : macrocall %prec DUMMY

@p_argstring_argslist
argstring : LLP arglist RRP

@p_argstring_token
argstring : argstring token
argstring : argstring macrocall %prec DUMMY

@p_argstring_argstring
argstring : argstring LLP arglist RRP
