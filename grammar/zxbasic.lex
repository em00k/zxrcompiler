# Lexer rules of zxbasic: state, function, token type, regular expression.
# Within a state the rules are tried in this order and the first match wins.
# Expressions use Python `re` syntax with re.VERBOSE.
INITIAL	t_INITIAL_bin_comment_beginBlockComment	beginBlockComment	/'
INITIAL	t_PLUS	PLUS	\+
INITIAL	t_MINUS	MINUS	-
INITIAL	t_MUL	MUL	\*
INITIAL	t_DIV	DIV	/
INITIAL	t_LP	LP	\(
INITIAL	t_RP	RP	\)
INITIAL	t_WEQ	WEQ	:=
INITIAL	t_CO	CO	:
INITIAL	t_SC	SC	;
INITIAL	t_COMMA	COMMA	,
INITIAL	t_GE	GE	>=
INITIAL	t_LE	LE	<=
INITIAL	t_NE	NE	<>
INITIAL	t_RIGHTARROW	RIGHTARROW	=>
INITIAL	t_EQ	EQ	=
INITIAL	t_SHL	SHL	<<
INITIAL	t_LT	LT	<
INITIAL	t_SHR	SHR	>>
INITIAL	t_GT	GT	>
INITIAL	t_POW	POW	\^
INITIAL	t_BXOR	BXOR	~
INITIAL	t_BAND	BAND	&
INITIAL	t_BOR	BOR	\|
INITIAL	t_BNOT	BNOT	!
INITIAL	t_initial_PREPROC_LINE	initial_PREPROC_LINE	\#[ \t]*[Ll][Ii][Nn][Ee][ \t]+([0-9]+)(?:[ \t]+"((?:[^"]|"")*)")?[ \t]*\r?\n
INITIAL	t_INITIAL_SHARP	SHARP	\#
INITIAL	t_LBRACE	LBRACE	\{
INITIAL	t_RBRACE	RBRACE	\}
INITIAL	t_ADDRESSOF	ADDRESSOF	@
INITIAL	t_str	str	"
INITIAL	t_asm	asm	\b[aA][sS][mM]\b
INITIAL	t_rem	rem	([']|[Rr][Ee][Mm][ \t]).*
INITIAL	t_EmptyRem	EmptyRem	([']|[Rr][Ee][Mm])\r?\n
INITIAL	t_HEXA	HEXA	([0-9][0-9a-fA-F]*[hH])|(\$[0-9a-fA-F]+)|(0x[0-9a-fA-F]+)
INITIAL	t_OCTAL	OCTAL	[0-7]+[oO]
INITIAL	t_BIN	BIN	(%[01]+)|([01]+[bB])
INITIAL	t_NUMBER	NUMBER	(([0-9]+(\.[0-9]+)?)|(\.[0-9]+))([eE][-+]?[0-9]+)?
INITIAL	t_INITIAL_bin_LineContinue	LineContinue	[_\\][ \t]*('.*|[Rr][Ee][Mm]\b.*|[Rr][Ee][Mm])?\r?\n
INITIAL	t_ID	ID	[a-zA-Z_][_a-zA-Z0-9]*[$%]?
INITIAL	t_INITIAL_bin_preproc_SEPARATOR	SEPARATOR	[ \t]+
INITIAL	t_INITIAL_bin_NEWLINE	NEWLINE	\r?\n
INITIAL	t_INITIAL_bin_string_asm_preproc_comment_ERROR	ERROR	.
INITIAL	t_INITIAL_bin_string_asm_preproc_comment_error	error	-
INITIAL	-	ignore	''
asm	t_asm_ASM	ASM	\b[eE][nN][dD][ \t]+[aA][sS][mM]\b
asm	t_asm_STRING	STRING	"[^"]*"
asm	t_asm_comment	comment	;.*
asm	t_asm_PREPROCLINE	PREPROCLINE	\#[ \t]*[Ll][Ii][Nn][Ee][ \t]+([0-9]+)(?:[ \t]+"((?:[^"]|"")*)")?[ \t]*\r?\n
asm	t_asm_next	next	.
asm	t_asm_NEWLINE	NEWLINE	\r?\n
asm	t_INITIAL_bin_string_asm_preproc_comment_ERROR	ERROR	.
asm	t_INITIAL_bin_string_asm_preproc_comment_error	error	-
asm	-	ignore	''
bin	t_INITIAL_bin_comment_beginBlockComment	beginBlockComment	/'
bin	t_bin_NUMBER	NUMBER	[01]+
bin	t_INITIAL_bin_LineContinue	LineContinue	[_\\][ \t]*('.*|[Rr][Ee][Mm]\b.*|[Rr][Ee][Mm])?\r?\n
bin	t_INITIAL_bin_preproc_SEPARATOR	SEPARATOR	[ \t]+
bin	t_bin_ZERO	ZERO	[^01]
bin	t_INITIAL_bin_NEWLINE	NEWLINE	\r?\n
bin	t_INITIAL_bin_string_asm_preproc_comment_ERROR	ERROR	.
bin	t_INITIAL_bin_string_asm_preproc_comment_error	error	-
bin	-	ignore	''
comment	t_INITIAL_bin_comment_beginBlockComment	beginBlockComment	/'
comment	t_comment_endBlockComment	endBlockComment	'/
comment	t_comment_NEWLINE	NEWLINE	\r?\n
comment	t_comment_charComment	charComment	.
comment	t_INITIAL_bin_string_asm_preproc_comment_ERROR	ERROR	.
comment	t_INITIAL_bin_string_asm_preproc_comment_error	error	-
comment	-	ignore	''
preproc	t_preproc_ID	ID	[_A-Za-z]+
preproc	t_preproc_NEWLINE	NEWLINE	\r?\n
preproc	t_preproc_INTEGER	INTEGER	[0-9]+
preproc	t_preproc_STRING	STRING	"[^"]*"
preproc	t_preproc_LP	LP	\(
preproc	t_preproc_RP	RP	\)
preproc	t_preproc_EQ	EQ	=
preproc	t_INITIAL_bin_preproc_SEPARATOR	SEPARATOR	[ \t]+
preproc	t_INITIAL_bin_string_asm_preproc_comment_ERROR	ERROR	.
preproc	t_INITIAL_bin_string_asm_preproc_comment_error	error	-
preproc	-	ignore	''
string	t_string_SLASH	SLASH	\\\\
string	t_string_COPYRIGHT	COPYRIGHT	\\\*
string	t_string_NGRAPH	NGRAPH	\\[ '.:][ '.:]
string	t_string_UDG	UDG	\\[A-Ua-u]
string	t_string_CODE	CODE	\\\#[0-9][0-9][0-9]
string	t_string_PAPERCODE	PAPERCODE	\\{p[0-9]}
string	t_string_INKCODE	INKCODE	\\{i[0-9]}
string	t_string_FLASH	FLASH	\\{f[01]}
string	t_string_BRIGHT	BRIGHT	\\{b[01]}
string	t_string_INVERSE	INVERSE	\\{v[in01]}
string	t_string_ITALIC	ITALIC	\\{I[01]}
string	t_string_BOLD	BOLD	\\{B[01]}
string	t_string_DQUOTE	DQUOTE	""
string	t_string_STRC	STRC	"
string	t_string_CHAR	CHAR	[^"]
string	t_INITIAL_bin_string_asm_preproc_comment_ERROR	ERROR	.
string	t_INITIAL_bin_string_asm_preproc_comment_error	error	-
string	-	ignore	''
