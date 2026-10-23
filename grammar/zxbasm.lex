# Lexer rules of zxbasm: state, function, token type, regular expression.
# Within a state the rules are tried in this order and the first match wins.
# Expressions use Python `re` syntax with re.VERBOSE.
INITIAL	t_INITIAL_preproc_skip	skip	[ \t]+
INITIAL	t_CHAR	CHAR	'.'
INITIAL	t_HEXA	HEXA	([0-9](_?[0-9a-fA-F])*[hH])|(\$[0-9a-fA-F](_?[0-9a-fA-F])*)|(0x[0-9a-fA-F](_?[0-9a-fA-F])*)
INITIAL	t_BIN	BIN	(%[01](_?[01])*)|(0[bB](_?[01])+)
INITIAL	t_INITIAL_TMPLABEL	TMPLABEL	[0-9]+[FfBb]
INITIAL	t_INITIAL_preproc_INTEGER	INTEGER	[0-9](_?\d)*
INITIAL	t_INITIAL_ID	ID	[._a-zA-Z][._a-zA-Z0-9]*
INITIAL	t_asm_PREPROCLINE	asm_PREPROCLINE	\#[ \t]*[Ll][Ii][Nn][Ee][ \t]+([0-9]+)(?:[ \t]+"((?:[^"]|"")*)")?[ \t]*\r?\n
INITIAL	t_COMMA	COMMA	,
INITIAL	t_ADDR	ADDR	\$
INITIAL	t_LB	LB	\[
INITIAL	t_RB	RB	\]
INITIAL	t_LP	LP	\(
INITIAL	t_RP	RP	\)
INITIAL	t_LSHIFT	LSHIFT	<<
INITIAL	t_RSHIFT	RSHIFT	>>
INITIAL	t_BAND	BAND	&
INITIAL	t_BOR	BOR	\|
INITIAL	t_BXOR	BXOR	~
INITIAL	t_PLUS	PLUS	\+
INITIAL	t_MINUS	MINUS	\-
INITIAL	t_MUL	MUL	\*
INITIAL	t_DIV	DIV	\/
INITIAL	t_MOD	MOD	\%
INITIAL	t_POW	POW	\^
INITIAL	t_APO	APO	'
INITIAL	t_CO	CO	:
INITIAL	t_INITIAL_preproc_STRING	STRING	"(""|[^"])*"
INITIAL	t_INITIAL_preproc_CONTINUE	CONTINUE	\\\r?\n
INITIAL	t_COMMENT	COMMENT	;.*
INITIAL	t_INITIAL_preproc_NEWLINE	NEWLINE	\r?\n
INITIAL	t_INITIAL_SHARP	SHARP	\#
INITIAL	t_INITIAL_preproc_ERROR	ERROR	.
INITIAL	t_INITIAL_preproc_error	error	-
INITIAL	-	ignore	''
preproc	t_INITIAL_preproc_skip	skip	[ \t]+
preproc	t_INITIAL_preproc_INTEGER	INTEGER	[0-9](_?\d)*
preproc	t_preproc_ID	ID	[_a-zA-Z][_a-zA-Z0-9]*
preproc	t_INITIAL_preproc_STRING	STRING	"(""|[^"])*"
preproc	t_INITIAL_preproc_CONTINUE	CONTINUE	\\\r?\n
preproc	t_INITIAL_preproc_NEWLINE	NEWLINE	\r?\n
preproc	t_INITIAL_preproc_ERROR	ERROR	.
preproc	t_INITIAL_preproc_error	error	-
preproc	-	ignore	''
