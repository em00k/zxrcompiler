# Lexer rules of zxbpp_asm: state, function, token type, regular expression.
# Within a state the rules are tried in this order and the first match wins.
# Expressions use Python `re` syntax with re.VERBOSE.
INITIAL	t_INITIAL_asmcomment_CONTINUE	CONTINUE	[\\_]\r?\n
INITIAL	t_INITIAL_COMMENT	COMMENT	;
INITIAL	t_INITIAL_NEWLINE	NEWLINE	\r?\n
INITIAL	t_INITIAL_ID	ID	[_A-Za-z][_A-Za-z0-9]*
INITIAL	t_INITIAL_CHAR	CHAR	'([^'\n]|'')'
INITIAL	t_INITIAL_TOKEN	TOKEN	[][}{%'`,.:$()*/<>~&|+^-]
INITIAL	t_INITIAL_prepro_pragma_defexpr_STRING	STRING	"([^"]|"")*"
INITIAL	t_INIIAL_sharp	INIIAL_sharp	\#
INITIAL	t_defexpr_INITIAL_SEPARATOR	SEPARATOR	[ \t]+
INITIAL	t_defexpr_INITIAL_HEXA	HEXA	([0-9][0-9a-fA-F]*[hH])|(\$[0-9a-fA-F]+)
INITIAL	t_defexpr_INITIAL_BIN	BIN	(%[01]+)|([01]+[bB])
INITIAL	t_defexpr_INITIAL_NUMBER	NUMBER	(([0-9]+(\.[0-9]+)?)|(\.[0-9]+))([eE][-+]?[0-9]+)?
INITIAL	t_line_defargs_defargsopt_prepro_define_defexpr_pragma_singlecomment_INITIAL_asmcomment_ANY	ANY	.
INITIAL	t_if_line_msg_defargs_defargsopt_prepro_define_defexpr_pragma_singlecomment_INITIAL_asmcomment_error	error	-
INITIAL	-	ignore	''
asmcomment	t_INITIAL_asmcomment_CONTINUE	CONTINUE	[\\_]\r?\n
asmcomment	t_asmcomment_TOKEN	TOKEN	.+
asmcomment	t_line_singlecomment_asmcomment_prepro_define_defargs_defargsopt_defexpr_pragma_if_NEWLINE	NEWLINE	\r?\n
asmcomment	t_line_defargs_defargsopt_prepro_define_defexpr_pragma_singlecomment_INITIAL_asmcomment_ANY	ANY	.
asmcomment	t_if_line_msg_defargs_defargsopt_prepro_define_defexpr_pragma_singlecomment_INITIAL_asmcomment_error	error	-
asmcomment	-	ignore	''
defargs	t_line_singlecomment_asmcomment_prepro_define_defargs_defargsopt_defexpr_pragma_if_NEWLINE	NEWLINE	\r?\n
defargs	t_prepro_define_defargs_defargsopt_defexpr_pragma_COMMENT	COMMENT	;
defargs	t_prepro_define_pragma_defargs_defargsopt_CONTINUE	CONTINUE	[_\\]\r?\n
defargs	t_line_prepro_pragma_defargs_define_skip_if	skip_if	[ \t]+
defargs	t_defargs_LP	LP	\(
defargs	t_defargs_RP	RP	\)
defargs	t_defargs_defargsopt_CONTINUE	CONTINUE	[\\_]\r?\n
defargs	t_defargs_ID	ID	[_a-zA-Z][_a-zA-Z0-9]*
defargs	t_defexpr_defargs_prepro_COMMA	COMMA	,
defargs	t_line_defargs_defargsopt_prepro_define_defexpr_pragma_singlecomment_INITIAL_asmcomment_ANY	ANY	.
defargs	t_if_line_msg_defargs_defargsopt_prepro_define_defexpr_pragma_singlecomment_INITIAL_asmcomment_error	error	-
defargs	-	ignore	''
defargsopt	t_line_singlecomment_asmcomment_prepro_define_defargs_defargsopt_defexpr_pragma_if_NEWLINE	NEWLINE	\r?\n
defargsopt	t_prepro_define_defargs_defargsopt_defexpr_pragma_COMMENT	COMMENT	;
defargsopt	t_prepro_define_pragma_defargs_defargsopt_CONTINUE	CONTINUE	[_\\]\r?\n
defargsopt	t_defargsopt_LP	LP	\(
defargsopt	t_defargsopt_TOKEN	TOKEN	[ \t)@,%&~|{}:.+*/-]|=>|<=|<>|=|<|>|<<|>>
defargsopt	t_defargsopt_STRING	STRING	"([^"\n]|"")*"
defargsopt	t_defargsopt_defexpr_ID	ID	[_a-zA-Z][_a-zA-Z0-9]*
defargsopt	t_defargs_defargsopt_CONTINUE	CONTINUE	[\\_]\r?\n
defargsopt	t_defargsopt_SEPARATOR	SEPARATOR	[ \t]+
defargsopt	t_line_defargs_defargsopt_prepro_define_defexpr_pragma_singlecomment_INITIAL_asmcomment_ANY	ANY	.
defargsopt	t_if_line_msg_defargs_defargsopt_prepro_define_defexpr_pragma_singlecomment_INITIAL_asmcomment_error	error	-
defargsopt	-	ignore	''
defexpr	t_line_singlecomment_asmcomment_prepro_define_defargs_defargsopt_defexpr_pragma_if_NEWLINE	NEWLINE	\r?\n
defexpr	t_prepro_define_defargs_defargsopt_defexpr_pragma_COMMENT	COMMENT	;
defexpr	t_defexpr_LLP	LLP	\(
defexpr	t_defexpr_RRP	RRP	\)
defexpr	t_defexpr_CONTINUE	CONTINUE	[\\_]\r?\n
defexpr	t_defexpr_STRINGIZING	STRINGIZING	\#[ \t]*
defexpr	t_defexpr_PASTE	PASTE	[ \t]*\#\#[ \t]*
defexpr	t_defargsopt_defexpr_ID	ID	[_a-zA-Z][_a-zA-Z0-9]*
defexpr	t_INITIAL_prepro_pragma_defexpr_STRING	STRING	"([^"]|"")*"
defexpr	t_defexpr_defargs_prepro_COMMA	COMMA	,
defexpr	t_defexpr_TOKEN	TOKEN	=>|<=|>=|<>|[@:.<>^=+*/&|%-]
defexpr	t_defexpr_INITIAL_SEPARATOR	SEPARATOR	[ \t]+
defexpr	t_defexpr_INITIAL_HEXA	HEXA	([0-9][0-9a-fA-F]*[hH])|(\$[0-9a-fA-F]+)
defexpr	t_defexpr_INITIAL_BIN	BIN	(%[01]+)|([01]+[bB])
defexpr	t_defexpr_INITIAL_NUMBER	NUMBER	(([0-9]+(\.[0-9]+)?)|(\.[0-9]+))([eE][-+]?[0-9]+)?
defexpr	t_line_defargs_defargsopt_prepro_define_defexpr_pragma_singlecomment_INITIAL_asmcomment_ANY	ANY	.
defexpr	t_if_line_msg_defargs_defargsopt_prepro_define_defexpr_pragma_singlecomment_INITIAL_asmcomment_error	error	-
defexpr	-	ignore	''
define	t_line_singlecomment_asmcomment_prepro_define_defargs_defargsopt_defexpr_pragma_if_NEWLINE	NEWLINE	\r?\n
define	t_prepro_define_defargs_defargsopt_defexpr_pragma_COMMENT	COMMENT	;
define	t_prepro_define_pragma_defargs_defargsopt_CONTINUE	CONTINUE	[_\\]\r?\n
define	t_line_prepro_pragma_defargs_define_skip_if	skip_if	[ \t]+
define	t_define_ID	ID	[_a-zA-Z][_a-zA-Z0-9]*
define	t_line_defargs_defargsopt_prepro_define_defexpr_pragma_singlecomment_INITIAL_asmcomment_ANY	ANY	.
define	t_if_line_msg_defargs_defargsopt_prepro_define_defexpr_pragma_singlecomment_INITIAL_asmcomment_error	error	-
define	-	ignore	''
if	t_line_singlecomment_asmcomment_prepro_define_defargs_defargsopt_defexpr_pragma_if_NEWLINE	NEWLINE	\r?\n
if	t_if_EQ	EQ	==
if	t_if_NE	NE	!=|<>
if	t_if_GE	GE	>=
if	t_if_GT	GT	>
if	t_if_LE	LE	<=
if	t_if_LT	LT	<
if	t_if_AND	AND	&&
if	t_if_OR	OR	\|\|
if	t_if_line_msg_defargs_defargsopt_prepro_define_defexpr_pragma_singlecomment_INITIAL_asmcomment_error	error	-
if	-	ignore	''
line	t_line_singlecomment_asmcomment_prepro_define_defargs_defargsopt_defexpr_pragma_if_NEWLINE	NEWLINE	\r?\n
line	t_line_prepro_pragma_defargs_define_skip_if	skip_if	[ \t]+
line	t_line_INTEGER	INTEGER	[0-9]+
line	t_line_STRING	STRING	"([^"]|"")*"
line	t_line_defargs_defargsopt_prepro_define_defexpr_pragma_singlecomment_INITIAL_asmcomment_ANY	ANY	.
line	t_if_line_msg_defargs_defargsopt_prepro_define_defexpr_pragma_singlecomment_INITIAL_asmcomment_error	error	-
line	-	ignore	''
msg	t_msg_TEXT	TEXT	.*\n
msg	t_if_line_msg_defargs_defargsopt_prepro_define_defexpr_pragma_singlecomment_INITIAL_asmcomment_error	error	-
msg	-	ignore	''
pragma	t_line_singlecomment_asmcomment_prepro_define_defargs_defargsopt_defexpr_pragma_if_NEWLINE	NEWLINE	\r?\n
pragma	t_prepro_define_defargs_defargsopt_defexpr_pragma_COMMENT	COMMENT	;
pragma	t_prepro_define_pragma_defargs_defargsopt_CONTINUE	CONTINUE	[_\\]\r?\n
pragma	t_line_prepro_pragma_defargs_define_skip_if	skip_if	[ \t]+
pragma	t_pragma_LP	LP	\(
pragma	t_pragma_RP	RP	\)
pragma	t_pragma_ID	ID	[_a-zA-Z][_a-zA-Z0-9]*
pragma	t_prepro_pragma_INTEGER	INTEGER	[0-9]+
pragma	t_INITIAL_prepro_pragma_defexpr_STRING	STRING	"([^"]|"")*"
pragma	t_pragma_EQ	EQ	=
pragma	t_line_defargs_defargsopt_prepro_define_defexpr_pragma_singlecomment_INITIAL_asmcomment_ANY	ANY	.
pragma	t_if_line_msg_defargs_defargsopt_prepro_define_defexpr_pragma_singlecomment_INITIAL_asmcomment_error	error	-
pragma	-	ignore	''
prepro	t_line_singlecomment_asmcomment_prepro_define_defargs_defargsopt_defexpr_pragma_if_NEWLINE	NEWLINE	\r?\n
prepro	t_prepro_define_defargs_defargsopt_defexpr_pragma_COMMENT	COMMENT	;
prepro	t_prepro_define_pragma_defargs_defargsopt_CONTINUE	CONTINUE	[_\\]\r?\n
prepro	t_prepro_LB	LB	\[
prepro	t_prepro_RB	RB	\]
prepro	t_prepro_CO	CO	\:
prepro	t_line_prepro_pragma_defargs_define_skip_if	skip_if	[ \t]+
prepro	t_prepro_ID	ID	[._a-zA-Z][._a-zA-Z0-9]*
prepro	t_prepro_pragma_INTEGER	INTEGER	[0-9]+
prepro	t_INITIAL_prepro_pragma_defexpr_STRING	STRING	"([^"]|"")*"
prepro	t_defexpr_defargs_prepro_COMMA	COMMA	,
prepro	t_prepro_FILENAME	FILENAME	<[^>]*>
prepro	t_line_defargs_defargsopt_prepro_define_defexpr_pragma_singlecomment_INITIAL_asmcomment_ANY	ANY	.
prepro	t_if_line_msg_defargs_defargsopt_prepro_define_defexpr_pragma_singlecomment_INITIAL_asmcomment_error	error	-
prepro	-	ignore	''
singlecomment	t_line_singlecomment_asmcomment_prepro_define_defargs_defargsopt_defexpr_pragma_if_NEWLINE	NEWLINE	\r?\n
singlecomment	t_singlecomment_comment_Skip	comment_Skip	.+
singlecomment	t_line_defargs_defargsopt_prepro_define_defexpr_pragma_singlecomment_INITIAL_asmcomment_ANY	ANY	.
singlecomment	t_if_line_msg_defargs_defargsopt_prepro_define_defexpr_pragma_singlecomment_INITIAL_asmcomment_error	error	-
singlecomment	-	ignore	''
