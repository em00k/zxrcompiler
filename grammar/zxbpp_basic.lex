# Lexer rules of zxbpp_basic: state, function, token type, regular expression.
# Within a state the rules are tried in this order and the first match wins.
# Expressions use Python `re` syntax with re.VERBOSE.
INITIAL	t_INITIAL_COMMENT	COMMENT	(\b[Rr][Ee][Mm]\b)|'
INITIAL	t_INITIAL_asmBegin	asmBegin	\b[aA][sS][mM]\b
INITIAL	t_INITIAL_CONTINUE	CONTINUE	[\\_]\r?\n
INITIAL	t_INITIAL_ID	ID	[_a-zA-Z][_a-zA-Z0-9]*[$%]?
INITIAL	t_INITIAL_asm_NEWLINE	NEWLINE	\r?\n
INITIAL	t_INITIAL_comment_beginBlock	beginBlock	/'
INITIAL	t_INITIAL_asm_defexpr_if_LLP	LLP	\(
INITIAL	t_INITIAL_asm_defexpr_if_RRP	RRP	\)
INITIAL	t_INITIAL_pragma_prepro_defexpr_asm_if_STRING	STRING	"([^"\n]|"")*"
INITIAL	t_INITIAL_asm_defexpr_defargs_prepro_COMMA	COMMA	,
INITIAL	t_INITIAL_asm_sharp	sharp	[ \t]*\#
INITIAL	t_INITIAL_defexpr_TOKEN	TOKEN	=>|<=|>=|<>|[$!&|~@:;{}.<>^=+*/%-]
INITIAL	t_INITIAL_defexpr_asm_SEPARATOR	SEPARATOR	[ \t]+
INITIAL	t_INITIAL_defexpr_asm_HEXA	HEXA	([0-9][0-9a-fA-F]*[hH])|(\$[0-9a-fA-F]+)
INITIAL	t_INITIAL_defexpr_asm_BIN	BIN	(%[01]+)|([01]+[bB])
INITIAL	t_INITIAL_defexpr_asm_if_NUMBER	NUMBER	(([0-9]+(\.[0-9]+)?)|(\.[0-9]+))([eE][-+]?[0-9]+)?
INITIAL	t_INITIAL_line_defargs_defargsopt_prepro_define_defexpr_pragma_comment_singlecomment_ANY	ANY	.
INITIAL	t_INITIAL_if_line_defargs_defargsopt_prepro_define_defexpr_pragma_comment_singlecomment_error	error	-
INITIAL	-	ignore	''
asm	t_asm_asmEnd	asmEnd	\b[Ee][Nn][Dd][ \t]+[Aa][Ss][Mm]\b
asm	t_asm_CONTINUE	CONTINUE	[\\_]\r?\n
asm	t_asm_COMMENT	COMMENT	;.*
asm	t_asm_PREPROCLINE	PREPROCLINE	\#[ \t]*[Ll][Ii][Nn][Ee][ \t]+([0-9]+)(?:[ \t]+"((?:[^"]|"")*)")?[ \t]*\r?\n
asm	t_asm_ID	ID	[_A-Za-z][_A-Za-z0-9]*
asm	t_asm_CHAR	CHAR	'[^'\n]|'''
asm	t_asm_TOKEN	TOKEN	[][}{'`.:$*/+<>|&~%^-]
asm	t_INITIAL_asm_NEWLINE	NEWLINE	\r?\n
asm	t_INITIAL_asm_defexpr_if_LLP	LLP	\(
asm	t_INITIAL_asm_defexpr_if_RRP	RRP	\)
asm	t_INITIAL_pragma_prepro_defexpr_asm_if_STRING	STRING	"([^"\n]|"")*"
asm	t_INITIAL_asm_defexpr_defargs_prepro_COMMA	COMMA	,
asm	t_INITIAL_asm_sharp	sharp	[ \t]*\#
asm	t_INITIAL_defexpr_asm_SEPARATOR	SEPARATOR	[ \t]+
asm	t_INITIAL_defexpr_asm_HEXA	HEXA	([0-9][0-9a-fA-F]*[hH])|(\$[0-9a-fA-F]+)
asm	t_INITIAL_defexpr_asm_BIN	BIN	(%[01]+)|([01]+[bB])
asm	t_INITIAL_defexpr_asm_if_NUMBER	NUMBER	(([0-9]+(\.[0-9]+)?)|(\.[0-9]+))([eE][-+]?[0-9]+)?
asm	t_asm_asmcomment_if_ANY	ANY	.
asm	t_asm_asmcomment_if_msg_error	error	-
asm	-	ignore	''
asmcomment	t_asm_asmcomment_if_ANY	ANY	.
asmcomment	t_asm_asmcomment_if_msg_error	error	-
asmcomment	-	ignore	''
comment	t_INITIAL_comment_beginBlock	beginBlock	/'
comment	t_comment_NEWLINE	NEWLINE	\r?\n
comment	t_comment_endBlock	endBlock	'/
comment	t_singlecomment_comment_Skip	Skip	.
comment	t_INITIAL_line_defargs_defargsopt_prepro_define_defexpr_pragma_comment_singlecomment_ANY	ANY	.
comment	t_INITIAL_if_line_defargs_defargsopt_prepro_define_defexpr_pragma_comment_singlecomment_error	error	-
comment	-	ignore	''
defargs	t_line_singlecomment_prepro_define_defargs_defargsopt_defexpr_pragma_if_NEWLINE	NEWLINE	\r?\n
defargs	t_prepro_define_defargs_defargsopt_defexpr_pragma_COMMENT	COMMENT	'
defargs	t_prepro_define_pragma_defargs_defargsopt_CONTINUE	CONTINUE	[_\\]\r?\n
defargs	t_line_prepro_pragma_defargs_define_if_skip	skip	[ \t]+
defargs	t_defargs_LP	LP	\(
defargs	t_defargs_RP	RP	\)
defargs	t_defargs_if_ID	ID	[_a-zA-Z][_a-zA-Z0-9]*
defargs	t_INITIAL_asm_defexpr_defargs_prepro_COMMA	COMMA	,
defargs	t_INITIAL_line_defargs_defargsopt_prepro_define_defexpr_pragma_comment_singlecomment_ANY	ANY	.
defargs	t_INITIAL_if_line_defargs_defargsopt_prepro_define_defexpr_pragma_comment_singlecomment_error	error	-
defargs	-	ignore	''
defargsopt	t_line_singlecomment_prepro_define_defargs_defargsopt_defexpr_pragma_if_NEWLINE	NEWLINE	\r?\n
defargsopt	t_prepro_define_defargs_defargsopt_defexpr_pragma_COMMENT	COMMENT	'
defargsopt	t_prepro_define_pragma_defargs_defargsopt_CONTINUE	CONTINUE	[_\\]\r?\n
defargsopt	t_defargsopt_LP	LP	\(
defargsopt	t_defargsopt_TOKEN	TOKEN	[ \t)@,{}:;.+*/!|&~$-]|=>|<=|<>|=|<|>
defargsopt	t_defargsopt_STRING	STRING	"([^"\n]|"")*"
defargsopt	t_defargsopt_defexpr_ID	ID	[_a-zA-Z][_a-zA-Z0-9]*
defargsopt	t_defargsopt_SEPARATOR	SEPARATOR	[ \t]+
defargsopt	t_INITIAL_line_defargs_defargsopt_prepro_define_defexpr_pragma_comment_singlecomment_ANY	ANY	.
defargsopt	t_INITIAL_if_line_defargs_defargsopt_prepro_define_defexpr_pragma_comment_singlecomment_error	error	-
defargsopt	-	ignore	''
defexpr	t_line_singlecomment_prepro_define_defargs_defargsopt_defexpr_pragma_if_NEWLINE	NEWLINE	\r?\n
defexpr	t_prepro_define_defargs_defargsopt_defexpr_pragma_COMMENT	COMMENT	'
defexpr	t_defexpr_CONTINUE	CONTINUE	[\\_]\r?\n
defexpr	t_INITIAL_asm_defexpr_if_LLP	LLP	\(
defexpr	t_INITIAL_asm_defexpr_if_RRP	RRP	\)
defexpr	t_defargsopt_defexpr_ID	ID	[_a-zA-Z][_a-zA-Z0-9]*
defexpr	t_INITIAL_pragma_prepro_defexpr_asm_if_STRING	STRING	"([^"\n]|"")*"
defexpr	t_INITIAL_asm_defexpr_defargs_prepro_COMMA	COMMA	,
defexpr	t_INITIAL_defexpr_TOKEN	TOKEN	=>|<=|>=|<>|[$!&|~@:;{}.<>^=+*/%-]
defexpr	t_defexpr_PASTE	PASTE	[ \t]*\#\#[ \t]*
defexpr	t_defexpr_STRINGIZING	STRINGIZING	\#[ \t]*
defexpr	t_INITIAL_defexpr_asm_SEPARATOR	SEPARATOR	[ \t]+
defexpr	t_INITIAL_defexpr_asm_HEXA	HEXA	([0-9][0-9a-fA-F]*[hH])|(\$[0-9a-fA-F]+)
defexpr	t_INITIAL_defexpr_asm_BIN	BIN	(%[01]+)|([01]+[bB])
defexpr	t_INITIAL_defexpr_asm_if_NUMBER	NUMBER	(([0-9]+(\.[0-9]+)?)|(\.[0-9]+))([eE][-+]?[0-9]+)?
defexpr	t_INITIAL_line_defargs_defargsopt_prepro_define_defexpr_pragma_comment_singlecomment_ANY	ANY	.
defexpr	t_INITIAL_if_line_defargs_defargsopt_prepro_define_defexpr_pragma_comment_singlecomment_error	error	-
defexpr	-	ignore	''
define	t_line_singlecomment_prepro_define_defargs_defargsopt_defexpr_pragma_if_NEWLINE	NEWLINE	\r?\n
define	t_prepro_define_defargs_defargsopt_defexpr_pragma_COMMENT	COMMENT	'
define	t_prepro_define_pragma_defargs_defargsopt_CONTINUE	CONTINUE	[_\\]\r?\n
define	t_line_prepro_pragma_defargs_define_if_skip	skip	[ \t]+
define	t_define_ID	ID	[_a-zA-Z][_a-zA-Z0-9]*
define	t_INITIAL_line_defargs_defargsopt_prepro_define_defexpr_pragma_comment_singlecomment_ANY	ANY	.
define	t_INITIAL_if_line_defargs_defargsopt_prepro_define_defexpr_pragma_comment_singlecomment_error	error	-
define	-	ignore	''
if	t_line_singlecomment_prepro_define_defargs_defargsopt_defexpr_pragma_if_NEWLINE	NEWLINE	\r?\n
if	t_line_prepro_pragma_defargs_define_if_skip	skip	[ \t]+
if	t_if_EQ	EQ	==
if	t_if_NE	NE	!=|<>
if	t_if_GE	GE	>=
if	t_if_GT	GT	>
if	t_if_LE	LE	<=
if	t_if_LT	LT	<
if	t_if_AND	AND	&&
if	t_if_OR	OR	\|\|
if	t_INITIAL_asm_defexpr_if_LLP	LLP	\(
if	t_INITIAL_asm_defexpr_if_RRP	RRP	\)
if	t_defargs_if_ID	ID	[_a-zA-Z][_a-zA-Z0-9]*
if	t_INITIAL_pragma_prepro_defexpr_asm_if_STRING	STRING	"([^"\n]|"")*"
if	t_INITIAL_defexpr_asm_if_NUMBER	NUMBER	(([0-9]+(\.[0-9]+)?)|(\.[0-9]+))([eE][-+]?[0-9]+)?
if	t_asm_asmcomment_if_ANY	ANY	.
if	t_asm_asmcomment_if_msg_error	error	-
if	-	ignore	''
line	t_line_singlecomment_prepro_define_defargs_defargsopt_defexpr_pragma_if_NEWLINE	NEWLINE	\r?\n
line	t_line_prepro_pragma_defargs_define_if_skip	skip	[ \t]+
line	t_line_INTEGER	INTEGER	[0-9]+
line	t_line_STRING	STRING	"([^"]|"")*"
line	t_INITIAL_line_defargs_defargsopt_prepro_define_defexpr_pragma_comment_singlecomment_ANY	ANY	.
line	t_INITIAL_if_line_defargs_defargsopt_prepro_define_defexpr_pragma_comment_singlecomment_error	error	-
line	-	ignore	''
msg	t_msg_TEXT	TEXT	.*\n
msg	t_asm_asmcomment_if_msg_error	error	-
msg	-	ignore	''
pragma	t_line_singlecomment_prepro_define_defargs_defargsopt_defexpr_pragma_if_NEWLINE	NEWLINE	\r?\n
pragma	t_prepro_define_defargs_defargsopt_defexpr_pragma_COMMENT	COMMENT	'
pragma	t_prepro_define_pragma_defargs_defargsopt_CONTINUE	CONTINUE	[_\\]\r?\n
pragma	t_line_prepro_pragma_defargs_define_if_skip	skip	[ \t]+
pragma	t_pragma_LP	LP	\(
pragma	t_pragma_RP	RP	\)
pragma	t_pragma_ID	ID	[_a-zA-Z][_a-zA-Z0-9]*
pragma	t_prepro_pragma_INTEGER	INTEGER	[0-9]+
pragma	t_INITIAL_pragma_prepro_defexpr_asm_if_STRING	STRING	"([^"\n]|"")*"
pragma	t_pragma_EQ	EQ	=
pragma	t_INITIAL_line_defargs_defargsopt_prepro_define_defexpr_pragma_comment_singlecomment_ANY	ANY	.
pragma	t_INITIAL_if_line_defargs_defargsopt_prepro_define_defexpr_pragma_comment_singlecomment_error	error	-
pragma	-	ignore	''
prepro	t_line_singlecomment_prepro_define_defargs_defargsopt_defexpr_pragma_if_NEWLINE	NEWLINE	\r?\n
prepro	t_prepro_define_defargs_defargsopt_defexpr_pragma_COMMENT	COMMENT	'
prepro	t_prepro_define_pragma_defargs_defargsopt_CONTINUE	CONTINUE	[_\\]\r?\n
prepro	t_prepro_LB	LB	\[
prepro	t_prepro_RB	RB	\]
prepro	t_prepro_CO	CO	\:
prepro	t_line_prepro_pragma_defargs_define_if_skip	skip	[ \t]+
prepro	t_prepro_ID	ID	[._a-zA-Z][._a-zA-Z0-9]*
prepro	t_prepro_pragma_INTEGER	INTEGER	[0-9]+
prepro	t_INITIAL_pragma_prepro_defexpr_asm_if_STRING	STRING	"([^"\n]|"")*"
prepro	t_INITIAL_asm_defexpr_defargs_prepro_COMMA	COMMA	,
prepro	t_prepro_FILENAME	FILENAME	<[^>]*>
prepro	t_INITIAL_line_defargs_defargsopt_prepro_define_defexpr_pragma_comment_singlecomment_ANY	ANY	.
prepro	t_INITIAL_if_line_defargs_defargsopt_prepro_define_defexpr_pragma_comment_singlecomment_error	error	-
prepro	-	ignore	''
singlecomment	t_line_singlecomment_prepro_define_defargs_defargsopt_defexpr_pragma_if_NEWLINE	NEWLINE	\r?\n
singlecomment	t_singlecomment_CONTINUE	CONTINUE	\\\r?\n
singlecomment	t_singlecomment_comment_Skip	Skip	.
singlecomment	t_INITIAL_line_defargs_defargsopt_prepro_define_defexpr_pragma_comment_singlecomment_ANY	ANY	.
singlecomment	t_INITIAL_if_line_defargs_defargsopt_prepro_define_defexpr_pragma_comment_singlecomment_error	error	-
singlecomment	-	ignore	''
