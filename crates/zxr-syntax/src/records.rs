//! Records: `TYPE … END TYPE`, variables and arrays of a TYPE, and pointers
//! to one (`AS T PTR`), lowered to plain BASIC before the parser reads the
//! program. Not in the Python compiler; see docs/records.md.
//!
//! A record is a UBYTE array with one dimension more, `0 TO size - 1`, so
//! that the compiler's own array code finds the record, and a field is a
//! PEEK or POKE at its offset in it. A pointer is a UINTEGER. The output has
//! the lines of the input, so every later message points at the user's line.

use std::collections::HashMap;
use zxr_core::Session;

/// The types a field can have, with the word PEEK and POKE take and the size.
const SCALARS: &[(&str, &str, u32)] = &[
    ("byte", "BYTE", 1),
    ("ubyte", "UBYTE", 1),
    ("integer", "INTEGER", 2),
    ("uinteger", "UINTEGER", 2),
    ("long", "LONG", 4),
    ("ulong", "ULONG", 4),
    ("fixed", "FIXED", 4),
    ("float", "FLOAT", 5),
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum K {
    Word,
    Num,
    Str,
    Op,
    Nl,
    /// A comment to the end of the line.
    Rem,
    /// Passed through as it is: `asm … end asm`, a block comment, a `#` line.
    Raw,
}

#[derive(Clone, Copy, Debug)]
struct Tok {
    k: K,
    s: usize,
    e: usize,
    /// Physical line in the input, from 0.
    line: usize,
}

/// What a field holds.
#[derive(Clone, Copy, Debug)]
enum Of {
    Scalar(&'static str, u32),
    /// A pointer to the TYPE with this index.
    Ptr(usize),
    Rec(usize),
}

#[derive(Debug)]
struct Field {
    name: String,
    of: Of,
    /// Bounds of an array field, each `(lo, hi)`.
    dims: Vec<(i64, i64)>,
    off: u32,
}

#[derive(Debug)]
struct Type {
    name: String,
    fields: Vec<Field>,
    size: u32,
    /// The `type` token that starts it.
    tok: usize,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum VarKind {
    Rec,
    Ptr,
}

#[derive(Clone, Copy, Debug)]
struct Var {
    kind: VarKind,
    ty: usize,
    array: bool,
    local: bool,
    /// For an array of records: its bounds, in `Lower::shapes`.
    shape: Option<usize>,
}

/// Where a record, or a field of one, is.
#[derive(Clone, Debug)]
struct Addr {
    base: Base,
    off: i64,
    /// Offsets known only at run time, in bytes, as BASIC expressions.
    dynamic: Vec<String>,
}

#[derive(Clone, Debug)]
enum Base {
    /// The UBYTE array a record variable became, and the indexes of the
    /// record in it (none for a single record).
    Arr { name: String, idx: Vec<String>, local: bool },
    /// The address a pointer holds.
    Ptr(String),
}

/// How far a path such as `a(i).pos.x` has got.
#[derive(Clone, Debug)]
enum Cur {
    Rec { addr: Addr, ty: usize },
    Ptr { text: String, ty: usize },
    Scalar { addr: Addr, t: &'static str },
    /// A pointer held in a field.
    PtrField { addr: Addr, ty: usize },
    /// `SIZEOF(T)`.
    Size(u32),
}

/// Lowers the records in `input`. None when the program has no TYPE, and
/// then the input is used as it is.
pub fn lower(sess: &mut Session, input: &str, case_insensitive: bool, array_base: i64) -> Option<String> {
    if !input.lines().any(starts_type) {
        return None;
    }
    let mut l = Lower::new(sess, input, case_insensitive);
    l.base = array_base;
    l.types();
    if l.blocks.is_empty() {
        return None;
    }
    l.functions();
    l.walk();
    Some(l.out)
}

/// A line that could be `TYPE name`.
fn starts_type(line: &str) -> bool {
    let line = line.trim_start();
    line.len() > 5 && line[..4].eq_ignore_ascii_case("type") && matches!(line.as_bytes()[4], b' ' | b'\t')
}

struct Lower<'a> {
    sess: &'a mut Session,
    src: &'a str,
    toks: Vec<Tok>,
    /// File and line of each physical line, after `#line`.
    lines: Vec<(usize, u32)>,
    files: Vec<String>,
    types: Vec<Type>,
    /// TYPE blocks: the token that starts each and the one after its end.
    blocks: HashMap<usize, usize>,
    /// FUNCTIONs that return a pointer to a record.
    ptr_funcs: HashMap<String, usize>,
    globals: HashMap<String, Var>,
    /// Inside a SUB or FUNCTION: its parameters and locals, None for a name
    /// that is not a record and hides a global one.
    locals: Option<HashMap<String, Option<Var>>>,
    ci: bool,
    ci_stack: Vec<bool>,
    /// `array_base`, the lower bound of a dimension that gives none.
    base: i64,
    base_stack: Vec<i64>,
    /// The bounds of each global array of records, as text: `(lo, hi)`.
    shapes: Vec<Vec<(String, String)>>,
    /// While the TYPEs are read: their errors, given in the order of the
    /// lines once all are known.
    deferred: Option<Vec<(usize, String)>>,
    out: String,
    /// How much of the input has been copied or replaced.
    cur: usize,
}

fn is_word_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_'
}

fn is_word(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

/// A number as the lexer reads one: decimal, `$FF`, `0xFF`, `FFh`, `%101`,
/// `101b`, `17o`.
fn parse_int(text: &str) -> Option<i64> {
    let t = text.to_ascii_lowercase();
    if let Some(h) = t.strip_prefix('$').or_else(|| t.strip_prefix("0x")) {
        return i64::from_str_radix(h, 16).ok();
    }
    if let Some(b) = t.strip_prefix('%') {
        return i64::from_str_radix(b, 2).ok();
    }
    if let Some(h) = t.strip_suffix('h') {
        return i64::from_str_radix(h, 16).ok();
    }
    if let Some(b) = t.strip_suffix('b') {
        if let Ok(v) = i64::from_str_radix(b, 2) {
            return Some(v);
        }
    }
    if let Some(o) = t.strip_suffix('o') {
        return i64::from_str_radix(o, 8).ok();
    }
    t.parse().ok()
}

/// An identifier, which needs no parentheses around it.
fn simple(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(is_word)
}

fn plus(off: i64) -> String {
    match off {
        0 => String::new(),
        o if o < 0 => format!(" - {}", -o),
        o => format!(" + {o}"),
    }
}

/// Adds to `addr` the offset of element `idx` of an array with bounds
/// `shape` and elements of `size` bytes: a number when the subscripts and
/// bounds are, otherwise an expression. Cheaper than the compiler's array
/// routine, which the record's extra dimension would make multiply in a loop.
fn element(addr: &mut Addr, idx: &[String], shape: &[(String, String)], size: i64) {
    let numbers = |t: &str| t.trim().strip_prefix('(').and_then(|t| t.strip_suffix(')')).unwrap_or(t.trim()).parse::<i64>().ok();
    let known: Option<Vec<(i64, i64, i64)>> = idx
        .iter()
        .zip(shape)
        .map(|(i, (lo, hi))| Some((numbers(i).or_else(|| parse_int(i.trim()))?, parse_int(lo)?, parse_int(hi)?)))
        .collect();
    if let Some(known) = known {
        let mut flat = 0;
        for (i, lo, hi) in known {
            flat = flat * (hi - lo + 1) + (i - lo);
        }
        addr.off += flat * size;
        return;
    }
    let mut flat = String::new();
    for (n, (i, (lo, hi))) in idx.iter().zip(shape).enumerate() {
        let term = if parse_int(lo) == Some(0) { format!("CAST(UINTEGER, {i})") } else { format!("(CAST(UINTEGER, {i}) - ({lo}))") };
        flat = if n == 0 {
            term
        } else {
            let count = match (parse_int(lo), parse_int(hi)) {
                (Some(lo), Some(hi)) => (hi - lo + 1).to_string(),
                _ => format!("(({hi}) - ({lo}) + 1)"),
            };
            format!("({flat}) * {count} + {term}")
        };
    }
    addr.dynamic.push(scale(&flat, size));
}

/// `(expr)` times `size`: a shift when the size is a power of two, as the
/// compiler multiplies by any constant with a call.
fn scale(expr: &str, size: i64) -> String {
    match size {
        1 => format!("({expr})"),
        s if s > 0 && s & (s - 1) == 0 => format!("(({expr}) << {})", s.trailing_zeros()),
        s => format!("({expr}) * {s}"),
    }
}

fn render(a: &Addr) -> String {
    let dynamic: String = a.dynamic.iter().map(|d| format!(" + {d}")).collect();
    match &a.base {
        Base::Arr { name, idx, local: false } => {
            let mut idx = idx.clone();
            idx.push("0".into());
            format!("@{name}({}){}{dynamic}", idx.join(", "), plus(a.off))
        }
        Base::Arr { name, idx, local: true } => {
            // `@local(0) + k` is "Initializer expression is not constant";
            // the offset goes into the index instead.
            let mut idx = idx.clone();
            idx.push(if a.dynamic.is_empty() {
                a.off.to_string()
            } else if a.off == 0 {
                a.dynamic.join(" + ")
            } else {
                format!("{}{}", a.off, dynamic)
            });
            format!("@{name}({})", idx.join(", "))
        }
        Base::Ptr(p) => {
            let p = if simple(p) { p.clone() } else { format!("({p})") };
            format!("{p}{}{dynamic}", plus(a.off))
        }
    }
}

impl<'a> Lower<'a> {
    fn new(sess: &'a mut Session, src: &'a str, ci: bool) -> Lower<'a> {
        let toks = scan(src);
        let mut files = vec![sess.filename.clone()];
        let mut lines = Vec::new();
        let (mut file, mut next) = (0, 1u32);
        for line in src.split('\n') {
            lines.push((file, next));
            next += 1;
            let t = line.trim_start();
            if let Some(rest) = t.strip_prefix('#') {
                let rest = rest.trim_start();
                if rest.len() > 4 && rest[..4].eq_ignore_ascii_case("line") {
                    let mut parts = rest[4..].trim().splitn(2, char::is_whitespace);
                    if let Some(n) = parts.next().and_then(|n| n.parse().ok()) {
                        next = n;
                        if let Some(f) = parts.next() {
                            let f = f.trim().trim_matches('"').replace("\"\"", "\"");
                            file = files.iter().position(|x| *x == f).unwrap_or_else(|| {
                                files.push(f);
                                files.len() - 1
                            });
                        }
                    }
                }
            }
        }
        Lower {
            sess,
            src,
            toks,
            lines,
            files,
            types: Vec::new(),
            blocks: HashMap::new(),
            ptr_funcs: HashMap::new(),
            globals: HashMap::new(),
            locals: None,
            ci,
            ci_stack: Vec::new(),
            base: 0,
            base_stack: Vec::new(),
            shapes: Vec::new(),
            deferred: None,
            out: String::with_capacity(src.len() + 1024),
            cur: 0,
        }
    }

    // ------------------------------------------------------------------
    // Tokens
    // ------------------------------------------------------------------

    fn text(&self, i: usize) -> &'a str {
        let t = self.toks[i];
        &self.src[t.s..t.e]
    }

    fn kind(&self, i: usize) -> Option<K> {
        self.toks.get(i).map(|t| t.k)
    }

    fn is_op(&self, i: usize, op: &str) -> bool {
        self.kind(i) == Some(K::Op) && self.text(i) == op
    }

    fn is_kw(&self, i: usize, kw: &str) -> bool {
        self.kind(i) == Some(K::Word) && self.text(i).eq_ignore_ascii_case(kw)
    }

    fn word(&self, i: usize) -> Option<&'a str> {
        (self.kind(i) == Some(K::Word)).then(|| self.text(i))
    }

    /// Source text from token `a` to token `b - 1`.
    fn span(&self, a: usize, b: usize) -> &'a str {
        &self.src[self.toks[a].s..self.toks[b - 1].e]
    }

    /// The `)` that closes the `(` at `i`, before `lim`.
    fn close(&self, i: usize, lim: usize) -> Option<usize> {
        let mut depth = 0;
        for j in i..lim {
            match self.kind(j) {
                Some(K::Op) => match self.text(j) {
                    "(" => depth += 1,
                    ")" => {
                        depth -= 1;
                        if depth == 0 {
                            return Some(j);
                        }
                    }
                    _ => {}
                },
                Some(K::Nl) => return None,
                _ => {}
            }
        }
        None
    }

    /// Splits `a..b` at the commas outside parentheses and braces.
    fn split(&self, a: usize, b: usize) -> Vec<(usize, usize)> {
        let mut parts = Vec::new();
        let (mut depth, mut start) = (0, a);
        for j in a..b {
            if self.kind(j) == Some(K::Op) {
                match self.text(j) {
                    "(" | "{" => depth += 1,
                    ")" | "}" => depth -= 1,
                    "," if depth == 0 => {
                        parts.push((start, j));
                        start = j + 1;
                    }
                    _ => {}
                }
            }
        }
        if start < b {
            parts.push((start, b));
        }
        parts
    }

    /// The end of the statement that goes on from `a`: a new line, a
    /// comment, `:` or, if `at_else`, an ELSE outside parentheses.
    fn stmt_end(&self, a: usize, at_else: bool) -> usize {
        let mut depth = 0;
        for j in a..self.toks.len() {
            match self.kind(j) {
                Some(K::Nl | K::Rem) => return j,
                Some(K::Op) => match self.text(j) {
                    "(" | "{" => depth += 1,
                    ")" | "}" => depth -= 1,
                    ":" if depth == 0 => return j,
                    _ => {}
                },
                Some(K::Word) if at_else && depth == 0 && self.is_kw(j, "else") => return j,
                _ => {}
            }
        }
        self.toks.len()
    }

    // ------------------------------------------------------------------
    // Names
    // ------------------------------------------------------------------

    fn same(&self, a: &str, b: &str) -> bool {
        a == b || (self.ci && a.eq_ignore_ascii_case(b))
    }

    fn type_named(&self, name: &str) -> Option<usize> {
        self.types.iter().position(|t| t.name == name).or_else(|| self.types.iter().position(|t| self.same(&t.name, name)))
    }

    fn lookup(&self, name: &str) -> Option<Var> {
        if let Some(locals) = &self.locals {
            if let Some(v) = locals.get(name) {
                return *v;
            }
            if self.ci {
                if let Some((_, v)) = locals.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)) {
                    return *v;
                }
            }
        }
        if let Some(v) = self.globals.get(name) {
            return Some(*v);
        }
        if self.ci {
            return self.globals.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| *v);
        }
        None
    }

    fn ptr_func(&self, name: &str) -> Option<usize> {
        self.ptr_funcs.get(name).copied().or_else(|| {
            if self.ci {
                self.ptr_funcs.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, t)| *t)
            } else {
                None
            }
        })
    }

    fn declare(&mut self, name: &str, var: Option<Var>) {
        let name = if self.ci { name.to_ascii_lowercase() } else { name.to_string() };
        match &mut self.locals {
            Some(locals) => {
                locals.insert(name, var);
            }
            None => {
                if let Some(v) = var {
                    self.globals.insert(name, v);
                }
            }
        }
    }

    fn error(&mut self, i: usize, message: &str) {
        if let Some(deferred) = &mut self.deferred {
            deferred.push((i, message.to_string()));
            return;
        }
        let line = self.toks.get(i).map_or(self.lines.len().saturating_sub(1), |t| t.line);
        let (file, lineno) = self.lines[line.min(self.lines.len() - 1)];
        let file = self.files[file].clone();
        self.sess.error(lineno, message, Some(&file));
    }

    // ------------------------------------------------------------------
    // The TYPE blocks, and the FUNCTIONs that return pointers
    // ------------------------------------------------------------------

    fn types(&mut self) {
        // Field types are resolved once every TYPE is known, so that a TYPE
        // can name one defined after it.
        self.deferred = Some(Vec::new());
        let mut pending: Vec<Vec<(String, String, bool, Vec<(i64, i64)>, usize)>> = Vec::new();
        let n = self.toks.len();
        let mut i = 0;
        while i < n {
            let line_start = i == 0 || self.kind(i - 1) == Some(K::Nl);
            if !(line_start
                && self.is_kw(i, "type")
                && self.word(i + 1).is_some()
                && matches!(self.kind(i + 2), None | Some(K::Nl | K::Rem)))
            {
                i += 1;
                continue;
            }
            let Some(end) = self.type_end(i) else {
                if !self.defines_type_routine() {
                    self.error(i, "TYPE without END TYPE");
                }
                i += 1;
                continue;
            };
            let name = self.text(i + 1).to_string();
            if SCALARS.iter().any(|s| s.0.eq_ignore_ascii_case(&name)) || name.eq_ignore_ascii_case("string") {
                self.error(i, &format!("A TYPE cannot be named '{name}'"));
            } else if self.types.iter().any(|t| t.name == name) {
                self.error(i, &format!("TYPE '{name}' is already defined"));
            }
            let mut fields = Vec::new();
            let mut k = i + 2;
            while k < end - 2 {
                let e = (k..end - 2).find(|&j| matches!(self.kind(j), Some(K::Nl | K::Rem | K::Raw))).unwrap_or(end - 2);
                if e > k {
                    self.field_line(k, e, &mut fields);
                }
                k = e + 1;
            }
            if fields.is_empty() {
                self.error(i, &format!("TYPE '{name}' has no fields"));
            }
            self.types.push(Type { name, fields: Vec::new(), size: 0, tok: i });
            pending.push(fields);
            self.blocks.insert(i, end);
            i = end;
        }
        // Resolve the field types.
        for (t, fields) in pending.into_iter().enumerate() {
            for (fname, tname, ptr, dims, tok) in fields {
                let scalar = SCALARS.iter().find(|s| s.0.eq_ignore_ascii_case(&tname));
                let of = match (scalar, self.type_named(&tname), ptr) {
                    (_, Some(u), true) => Of::Ptr(u),
                    (Some(_), None, true) => {
                        self.error(tok, &format!("'{fname}': a PTR field points to a TYPE; for a {tname} address use UINTEGER"));
                        continue;
                    }
                    (Some(s), None, false) => Of::Scalar(s.1, s.2),
                    (None, Some(u), false) => Of::Rec(u),
                    (None, None, _) if tname.eq_ignore_ascii_case("string") => {
                        self.error(tok, &format!("'{fname}': STRING fields are not supported; hold the address in a UINTEGER"));
                        continue;
                    }
                    (None, None, _) => {
                        self.error(tok, &format!("'{fname}': no type '{tname}'"));
                        continue;
                    }
                    (Some(_), Some(_), false) => unreachable!("a TYPE is not named like a built-in type"),
                };
                if self.types[t].fields.iter().any(|f| self.same(&f.name, &fname)) {
                    self.error(tok, &format!("TYPE '{}' already has a field '{fname}'", self.types[t].name));
                    continue;
                }
                self.types[t].fields.push(Field { name: fname, of, dims, off: 0 });
            }
        }
        // Offsets and sizes, a TYPE inside another first.
        let mut state = vec![0u8; self.types.len()];
        for t in 0..self.types.len() {
            self.layout(t, &mut state);
        }
        let mut errors = self.deferred.take().unwrap_or_default();
        errors.sort_by_key(|e| e.0);
        for (i, message) in errors {
            self.error(i, &message);
        }
    }

    /// The token after the `type` of the END TYPE that closes the TYPE at `i`.
    fn type_end(&self, i: usize) -> Option<usize> {
        let mut k = i + 2;
        while k < self.toks.len() {
            if (k == 0 || self.kind(k - 1) == Some(K::Nl)) && self.is_kw(k, "end") && self.is_kw(k + 1, "type") {
                return Some(k + 2);
            }
            if (k == 0 || self.kind(k - 1) == Some(K::Nl))
                && self.is_kw(k, "type")
                && self.word(k + 1).is_some()
                && matches!(self.kind(k + 2), None | Some(K::Nl | K::Rem))
            {
                return None;
            }
            k += 1;
        }
        None
    }

    /// Whether the program has a SUB or FUNCTION called `type`, and so a
    /// line `type x` is a call.
    fn defines_type_routine(&self) -> bool {
        (0..self.toks.len()).any(|i| {
            (self.is_kw(i, "sub") || self.is_kw(i, "function"))
                && (self.is_kw(i + 1, "type")
                    || ((self.is_kw(i + 1, "fastcall") || self.is_kw(i + 1, "stdcall")) && self.is_kw(i + 2, "type")))
        })
    }

    /// One line of fields: `a, b(3) AS t [PTR]`.
    #[allow(clippy::type_complexity)]
    fn field_line(&mut self, a: usize, b: usize, fields: &mut Vec<(String, String, bool, Vec<(i64, i64)>, usize)>) {
        let bad = |l: &mut Self, why: &str| l.error(a, why);
        if self.is_kw(a, "declare") || self.is_kw(a, "sub") || self.is_kw(a, "function") {
            return bad(self, "A TYPE holds fields only: methods are not supported");
        }
        let Some(as_at) = (a..b).find(|&j| self.is_kw(j, "as")) else {
            return bad(self, &format!("Expected a field, 'name AS type', not '{}'", self.span(a, b)));
        };
        let Some(tname) = self.word(as_at + 1) else {
            return bad(self, "Expected a type after AS");
        };
        let ptr = self.is_kw(as_at + 2, "ptr");
        if as_at + 2 + ptr as usize != b {
            return bad(self, &format!("Unexpected '{}' after the type", self.span(as_at + 2 + ptr as usize, b)));
        }
        for (s, e) in self.split(a, as_at) {
            let Some(fname) = self.word(s) else {
                return bad(self, &format!("Expected a field name, not '{}'", self.span(s, e)));
            };
            let mut dims = Vec::new();
            if s + 1 < e {
                let rp = self.close(s + 1, e).filter(|&rp| self.is_op(s + 1, "(") && rp + 1 == e);
                let Some(rp) = rp else {
                    return bad(self, &format!("Unexpected '{}' after '{fname}'", self.span(s + 1, e)));
                };
                for (ds, de) in self.split(s + 2, rp) {
                    match self.bounds(ds, de) {
                        Some(d) => dims.push(d),
                        None => {
                            return bad(
                                self,
                                &format!("'{fname}': the bounds of an array field are numbers, as in (7) or (1 TO 8)"),
                            )
                        }
                    }
                }
            }
            fields.push((fname.to_string(), tname.to_string(), ptr, dims, a));
        }
    }

    /// `hi` or `lo TO hi`, in numbers.
    fn bounds(&self, a: usize, b: usize) -> Option<(i64, i64)> {
        let num = |j: &mut usize| -> Option<i64> {
            let neg = self.is_op(*j, "-");
            if neg {
                *j += 1;
            }
            let v = (self.kind(*j) == Some(K::Num)).then(|| parse_int(self.text(*j))).flatten()?;
            *j += 1;
            Some(if neg { -v } else { v })
        };
        let mut j = a;
        let first = num(&mut j)?;
        let (lo, hi) = if j < b && self.is_kw(j, "to") {
            j += 1;
            (first, num(&mut j)?)
        } else {
            (0, first)
        };
        (j == b && hi >= lo).then_some((lo, hi))
    }

    fn layout(&mut self, t: usize, state: &mut Vec<u8>) -> u32 {
        match state[t] {
            2 => return self.types[t].size,
            1 => {
                let (name, tok) = (self.types[t].name.clone(), self.types[t].tok);
                self.error(tok, &format!("TYPE '{name}' contains itself"));
                return 0;
            }
            _ => {}
        }
        state[t] = 1;
        let mut off = 0u32;
        for f in 0..self.types[t].fields.len() {
            let size = match self.types[t].fields[f].of {
                Of::Scalar(_, s) => s,
                Of::Ptr(_) => 2,
                Of::Rec(u) => self.layout(u, state),
            };
            let count: i64 = self.types[t].fields[f].dims.iter().map(|(lo, hi)| hi - lo + 1).product();
            self.types[t].fields[f].off = off;
            off = off.saturating_add(size.saturating_mul(count as u32));
        }
        self.types[t].size = off;
        state[t] = 2;
        off
    }

    /// FUNCTIONs that return `T PTR`, so that a call can be followed by `->`
    /// before the FUNCTION is defined.
    fn functions(&mut self) {
        for i in 0..self.toks.len() {
            if !self.is_kw(i, "function") || (i > 0 && self.is_kw(i - 1, "end")) {
                continue;
            }
            let mut j = i + 1;
            if self.is_kw(j, "fastcall") || self.is_kw(j, "stdcall") {
                j += 1;
            }
            let Some(name) = self.word(j) else { continue };
            j += 1;
            if self.is_op(j, "(") {
                let Some(rp) = self.close(j, self.toks.len()) else { continue };
                j = rp + 1;
            }
            if self.is_kw(j, "as") && self.is_kw(j + 2, "ptr") {
                if let Some(t) = self.word(j + 1).and_then(|t| self.type_named(t)) {
                    self.ptr_funcs.insert(name.to_string(), t);
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // Output
    // ------------------------------------------------------------------

    /// Copies the input up to token `i` and the token itself.
    fn keep(&mut self, i: usize) {
        let e = self.toks[i].e;
        self.out.push_str(&self.src[self.cur..e]);
        self.cur = e;
    }

    /// Copies the input up to token `a`, then puts `text` in place of tokens
    /// `a..b`. Lines the replaced text had and `text` has not are kept as
    /// line continuations.
    fn put(&mut self, a: usize, b: usize, text: &str) {
        let (s, e) = (self.toks[a].s, self.toks[b - 1].e);
        self.out.push_str(&self.src[self.cur..s]);
        self.out.push_str(text);
        let lost = self.src[s..e].matches('\n').count().saturating_sub(text.matches('\n').count());
        for _ in 0..lost {
            self.out.push_str(" _\n");
        }
        self.cur = e;
    }

    /// Tokens `a..b` lowered, as text of their own.
    fn sub(&mut self, a: usize, b: usize) -> String {
        if a >= b {
            return String::new();
        }
        let out = std::mem::take(&mut self.out);
        let cur = self.cur;
        self.cur = self.toks[a].s;
        let mut i = a;
        while i < b {
            i = self.step(i, b);
        }
        self.out.push_str(&self.src[self.cur..self.toks[b - 1].e]);
        self.cur = cur;
        std::mem::replace(&mut self.out, out)
    }

    /// One token, or one record operand, of an expression.
    fn step(&mut self, i: usize, lim: usize) -> usize {
        let addr_of = self.is_op(i, "@");
        let at = if addr_of { i + 1 } else { i };
        if self.kind(at) == Some(K::Word) {
            if let Some(result) = self.chain(at, lim) {
                let end = match result {
                    Ok((cur, end, path)) => {
                        let text = match self.finish(i, cur, end, path, addr_of) {
                            Some(text) => text,
                            None => "0".into(),
                        };
                        self.put(i, end, &text);
                        end
                    }
                    Err(end) => {
                        self.put(i, end, "0");
                        end
                    }
                };
                return end;
            }
        }
        self.keep(i);
        i + 1
    }

    /// The text of an operand that has been read: None after an error.
    fn finish(&mut self, i: usize, cur: Cur, end: usize, path: bool, addr_of: bool) -> Option<String> {
        let at = if addr_of { "@" } else { "" };
        Some(match (cur, addr_of) {
            (Cur::Size(n), _) => format!("{at}{n}"),
            (Cur::Ptr { text, .. }, _) => format!("{at}{text}"),
            (Cur::Rec { addr, .. } | Cur::Scalar { addr, .. } | Cur::PtrField { addr, .. }, true) => {
                format!("({})", render(&addr))
            }
            (Cur::Scalar { addr, t }, false) => format!("PEEK({t}, {})", render(&addr)),
            (Cur::PtrField { addr, .. }, false) => format!("PEEK(UINTEGER, {})", render(&addr)),
            (Cur::Rec { .. }, false) => {
                let what = self.span(i, end);
                let message = if path {
                    format!("'{what}' is a record: name one of its fields, or take its address with @")
                } else {
                    format!("'{what}' is a record: name one of its fields ({what}.field), or take its address (@{what})")
                };
                self.error(i, &message);
                return None;
            }
        })
    }

    /// Reads the operand at `i` if it is a record, a pointer to one, a call
    /// of a FUNCTION that returns one or `SIZEOF(T)`, with the `.` and `->`
    /// path after it. None when it is none of these; Err(end) after an error.
    #[allow(clippy::type_complexity)]
    fn chain(&mut self, i: usize, lim: usize) -> Option<Result<(Cur, usize, bool), usize>> {
        let name = self.word(i)?;
        let mut j = i + 1;
        let mut cur = if let Some(v) = self.lookup(name) {
            let mut idx = Vec::new();
            if self.is_op(j, "(") {
                if !v.array {
                    if v.kind == VarKind::Ptr {
                        return None;
                    }
                    self.error(i, &format!("'{name}' is a record, not an array of them"));
                    return Some(Err(self.skip_path(j, lim)));
                }
                let Some(rp) = self.close(j, lim) else { return Some(Err(j)) };
                idx = self.split(j + 1, rp).into_iter().map(|(a, b)| self.sub(a, b).trim().to_string()).collect();
                j = rp + 1;
            } else if v.array {
                if v.kind == VarKind::Ptr {
                    return None;
                }
                self.error(i, &format!("'{name}' is an array of records: {name}(i)"));
                return Some(Err(self.skip_path(j, lim)));
            }
            match v.kind {
                VarKind::Rec if !v.local && v.shape.is_some() => {
                    let shape = self.shapes[v.shape.unwrap()].clone();
                    if shape.len() != idx.len() {
                        self.error(i, &format!("'{name}' has {} dimension(s), not {}", shape.len(), idx.len()));
                        return Some(Err(self.skip_path(j, lim)));
                    }
                    let size = self.types[v.ty].size as i64;
                    let mut addr = Addr {
                        base: Base::Arr { name: name.to_string(), idx: shape.iter().map(|(lo, _)| lo.clone()).collect(), local: false },
                        off: 0,
                        dynamic: Vec::new(),
                    };
                    element(&mut addr, &idx, &shape, size);
                    Cur::Rec { addr, ty: v.ty }
                }
                VarKind::Rec => Cur::Rec {
                    addr: Addr { base: Base::Arr { name: name.to_string(), idx, local: v.local }, off: 0, dynamic: Vec::new() },
                    ty: v.ty,
                },
                VarKind::Ptr if v.array => Cur::Ptr { text: format!("{name}({})", idx.join(", ")), ty: v.ty },
                VarKind::Ptr => Cur::Ptr { text: name.to_string(), ty: v.ty },
            }
        } else if let Some(ty) = self.ptr_func(name) {
            let mut text = name.to_string();
            if self.is_op(j, "(") {
                let Some(rp) = self.close(j, lim) else { return Some(Err(j)) };
                let args: Vec<String> =
                    self.split(j + 1, rp).into_iter().map(|(a, b)| self.sub(a, b).trim().to_string()).collect();
                text = format!("{name}({})", args.join(", "));
                j = rp + 1;
            }
            Cur::Ptr { text, ty }
        } else if name.eq_ignore_ascii_case("sizeof") && self.is_op(j, "(") && self.is_op(j + 2, ")") {
            let t = self.type_named(self.word(j + 1)?)?;
            return Some(Ok((Cur::Size(self.types[t].size), j + 3, false)));
        } else {
            return None;
        };
        if j > lim {
            return Some(Err(self.skip_path(lim, lim)));
        }
        let mut path = false;
        while j + 1 < lim && (self.is_op(j, ".") || self.is_op(j, "->")) {
            let arrow = self.is_op(j, "->");
            let so_far = self.span(i, j);
            let (mut addr, ty) = match (cur, arrow) {
                (Cur::Rec { addr, ty }, false) => (addr, ty),
                (Cur::Ptr { text, ty }, true) => (Addr { base: Base::Ptr(text), off: 0, dynamic: Vec::new() }, ty),
                (Cur::PtrField { addr, ty }, true) => {
                    let p = format!("PEEK(UINTEGER, {})", render(&addr));
                    (Addr { base: Base::Ptr(p), off: 0, dynamic: Vec::new() }, ty)
                }
                (Cur::Rec { .. }, true) => {
                    self.error(i, &format!("'{so_far}' is a record, not a pointer: use '.'"));
                    return Some(Err(self.skip_path(j, lim)));
                }
                (Cur::Ptr { .. } | Cur::PtrField { .. }, false) => {
                    self.error(i, &format!("'{so_far}' is a pointer: use '->'"));
                    return Some(Err(self.skip_path(j, lim)));
                }
                (Cur::Scalar { .. } | Cur::Size(_), _) => {
                    self.error(i, &format!("'{so_far}' is not a record"));
                    return Some(Err(self.skip_path(j, lim)));
                }
            };
            let Some(fname) = self.word(j + 1) else {
                self.error(i, &format!("Expected a field name after '{so_far}{}'", self.text(j)));
                return Some(Err(self.skip_path(j + 1, lim)));
            };
            let Some(f) = self.types[ty].fields.iter().position(|f| self.same(&f.name, fname)) else {
                self.error(i, &format!("TYPE '{}' has no field '{fname}'", self.types[ty].name));
                return Some(Err(self.skip_path(j + 2, lim)));
            };
            j += 2;
            let (of, foff, dims) = {
                let f = &self.types[ty].fields[f];
                (f.of, f.off, f.dims.clone())
            };
            addr.off += foff as i64;
            if !dims.is_empty() {
                if !self.is_op(j, "(") {
                    self.error(i, &format!("'{fname}' is an array: {fname}(i)"));
                    return Some(Err(self.skip_path(j, lim)));
                }
                let Some(rp) = self.close(j, lim) else { return Some(Err(j)) };
                let parts = self.split(j + 1, rp);
                if parts.len() != dims.len() {
                    self.error(i, &format!("'{fname}' has {} dimension(s), not {}", dims.len(), parts.len()));
                    return Some(Err(self.skip_path(rp + 1, lim)));
                }
                let size = match of {
                    Of::Scalar(_, s) => s,
                    Of::Ptr(_) => 2,
                    Of::Rec(u) => self.types[u].size,
                } as i64;
                let known: Option<Vec<i64>> = parts
                    .iter()
                    .map(|&(a, b)| (b == a + 1 && self.kind(a) == Some(K::Num)).then(|| parse_int(self.text(a))).flatten())
                    .collect();
                if let Some(known) = known {
                    let mut flat = 0;
                    for (k, (lo, hi)) in known.iter().zip(&dims) {
                        flat = flat * (hi - lo + 1) + (k - lo);
                    }
                    addr.off += flat * size;
                } else {
                    let mut flat = String::new();
                    for (n, (&(a, b), (lo, hi))) in parts.iter().zip(&dims).enumerate() {
                        let index = self.sub(a, b);
                        let term = format!("CAST(UINTEGER, {}){}", index.trim(), plus(-lo));
                        flat = if n == 0 { term } else { format!("({flat}) * {} + {term}", hi - lo + 1) };
                    }
                    addr.dynamic.push(scale(&flat, size));
                }
                j = rp + 1;
            } else if self.is_op(j, "(") {
                self.error(i, &format!("'{fname}' is not an array"));
                return Some(Err(self.skip_path(j, lim)));
            }
            cur = match of {
                Of::Scalar(t, _) => Cur::Scalar { addr, t },
                Of::Rec(u) => Cur::Rec { addr, ty: u },
                Of::Ptr(u) => Cur::PtrField { addr, ty: u },
            };
            path = true;
        }
        Some(Ok((cur, j, path)))
    }

    /// After an error in a path: the end of what is left of it, so that the
    /// parser does not report the rest again.
    fn skip_path(&self, mut j: usize, lim: usize) -> usize {
        loop {
            if self.is_op(j, "(") {
                match self.close(j, lim) {
                    Some(rp) => j = rp + 1,
                    None => return j.min(lim),
                }
            } else if (self.is_op(j, ".") || self.is_op(j, "->")) && self.word(j + 1).is_some() && j + 1 < lim {
                j += 2;
            } else {
                return j.min(lim);
            }
        }
    }

    // ------------------------------------------------------------------
    // Statements
    // ------------------------------------------------------------------

    fn walk(&mut self) {
        let n = self.toks.len();
        let mut i = 0;
        let mut start = true;
        while i < n {
            match self.kind(i) {
                Some(K::Nl) => {
                    self.keep(i);
                    i += 1;
                    start = true;
                    continue;
                }
                Some(K::Rem) => {
                    self.keep(i);
                    i += 1;
                    continue;
                }
                Some(K::Raw) => {
                    self.pragma(i);
                    self.keep(i);
                    i += 1;
                    continue;
                }
                _ => {}
            }
            if self.is_op(i, ":") || self.is_kw(i, "then") || self.is_kw(i, "else") {
                self.keep(i);
                i += 1;
                start = true;
                continue;
            }
            if !start {
                i = self.step(i, n);
                continue;
            }
            start = false;
            if let Some(&end) = self.blocks.get(&i) {
                let lines = "\n".repeat(self.span(i, end).matches('\n').count());
                self.put(i, end, &lines);
                i = end;
                continue;
            }
            let word = self.word(i).map(str::to_ascii_lowercase);
            match word.as_deref() {
                Some("let" | "private") => {
                    self.keep(i);
                    i += 1;
                    start = true;
                }
                Some("dim") => i = self.dim(i),
                Some("sub" | "function") => i = self.header(i, false),
                Some("declare") => {
                    self.keep(i);
                    i = if self.is_kw(i + 1, "sub") || self.is_kw(i + 1, "function") { self.header(i + 1, true) } else { i + 1 };
                }
                Some("end") if self.is_kw(i + 1, "sub") || self.is_kw(i + 1, "function") => {
                    self.locals = None;
                    self.keep(i);
                    i += 1;
                }
                Some("for") if self.kind(i + 1) == Some(K::Word) => {
                    self.keep(i);
                    let end = self.stmt_end(i + 1, false);
                    if let Some(Ok((Cur::Scalar { .. } | Cur::PtrField { .. } | Cur::Rec { .. }, _, _))) = self.chain(i + 1, end) {
                        self.error(i, "A field cannot be the variable of a FOR: count in a variable and copy it");
                    }
                    i += 1;
                }
                _ => i = self.statement(i),
            }
        }
        self.out.push_str(&self.src[self.cur..]);
    }

    /// A statement that may be `field = value`.
    fn statement(&mut self, i: usize) -> usize {
        let end = self.stmt_end(i, true);
        let Some(result) = self.chain(i, end) else {
            return self.step(i, self.toks.len());
        };
        let (cur, after, path) = match result {
            Ok(r) => r,
            Err(e) => {
                let e = self.stmt_end(e.max(i + 1), true);
                self.put(i, e, "");
                return e;
            }
        };
        if !self.is_op(after, "=") {
            let text = self.finish(i, cur, after, path, false).unwrap_or_else(|| "0".into());
            self.put(i, after, &text);
            return after;
        }
        let (t, addr) = match cur {
            Cur::Scalar { addr, t } => (t, addr),
            Cur::PtrField { addr, .. } => ("UINTEGER", addr),
            Cur::Rec { .. } => {
                self.error(i, "A record cannot be assigned as a whole: assign its fields one by one");
                self.put(i, end, "");
                return end;
            }
            Cur::Ptr { .. } | Cur::Size(_) => {
                // A pointer is assigned as the UINTEGER it is.
                let text = self.finish(i, cur, after, path, false).unwrap_or_default();
                self.put(i, after, &text);
                return after;
            }
        };
        let value = self.sub(after + 1, end);
        if value.trim().is_empty() {
            self.error(i, "Expected a value after '='");
        }
        self.put(i, end, &format!("POKE {t} {}, {}", render(&addr), value.trim()));
        end
    }

    /// `DIM names AS T`, `DIM names AS T PTR`, or any other DIM, whose names
    /// hide records of the same names in a SUB.
    fn dim(&mut self, i: usize) -> usize {
        let end = self.stmt_end(i + 1, false);
        let as_at = (i + 1..end).find(|&j| self.is_kw(j, "as") && self.close_depth(i + 1, j) == 0);
        let names: Vec<(usize, usize)> = self.split(i + 1, as_at.unwrap_or(end));
        let ty = as_at.and_then(|a| self.word(a + 1)).and_then(|t| self.type_named(t));
        let (Some(as_at), Some(ty)) = (as_at, ty) else {
            for &(s, _) in &names {
                if let Some(name) = self.word(s) {
                    if self.locals.is_some() {
                        self.declare(name, None);
                    }
                }
            }
            self.keep(i);
            let mut k = i + 1;
            while k < end {
                k = self.step(k, end);
            }
            return end;
        };
        let ptr = self.is_kw(as_at + 2, "ptr");
        let rest = as_at + 2 + ptr as usize;
        let mut decl = Vec::new();
        for &(s, e) in &names {
            let Some(name) = self.word(s) else {
                self.error(i, &format!("Expected a name, not '{}'", self.span(s, e)));
                return end;
            };
            let bounds = if s + 1 < e {
                match self.close(s + 1, e).filter(|&rp| self.is_op(s + 1, "(") && rp + 1 == e) {
                    Some(rp) => Some((s + 2, rp)),
                    None => {
                        self.error(i, &format!("Unexpected '{}' after '{name}'", self.span(s + 1, e)));
                        return end;
                    }
                }
            } else {
                None
            };
            decl.push((name, bounds));
        }
        let local = self.locals.is_some();
        if ptr {
            for &(name, bounds) in &decl {
                let var = Var { kind: VarKind::Ptr, ty, array: bounds.is_some(), local, shape: None };
                self.declare(name, Some(var));
            }
            self.keep(i);
            let mut k = i + 1;
            while k <= as_at {
                k = self.step(k, as_at + 1);
            }
            self.put(as_at + 1, rest, "UINTEGER");
            let mut k = rest;
            while k < end {
                k = self.step(k, end);
            }
            return end;
        }
        let mut at = None;
        if rest < end {
            if self.is_op(rest, "=") {
                self.error(i, "A record cannot be given a value in its DIM: set its fields");
            } else if self.is_kw(rest, "at") && decl.len() == 1 {
                at = Some(self.sub(rest + 1, end));
            } else {
                self.error(i, &format!("Unexpected '{}' after the TYPE", self.span(rest, end)));
            }
        }
        let size = self.types[ty].size.max(1);
        let mut text = Vec::new();
        for &(name, bounds) in &decl {
            // Every dimension gets both bounds, so that an element's address
            // can be worked out here (see `chain`) whatever `array_base` is.
            let mut shape = Vec::new();
            if let Some((a, b)) = bounds {
                for (s, e) in self.split(a, b) {
                    match (s..e).find(|&k| self.is_kw(k, "to") && self.close_depth(s, k) == 0) {
                        Some(to) => shape.push((self.sub(s, to).trim().to_string(), self.sub(to + 1, e).trim().to_string())),
                        None => shape.push((self.base.to_string(), self.sub(s, e).trim().to_string())),
                    }
                }
            }
            let dims: String = shape.iter().map(|(lo, hi)| format!("{lo} TO {hi}, ")).collect();
            let at = at.as_ref().map(|a| format!(" AT {}", a.trim())).unwrap_or_default();
            text.push(format!("DIM {name}({dims}0 TO {}) AS UBYTE{at}", size - 1));
            let shape = (!shape.is_empty()).then(|| {
                self.shapes.push(shape);
                self.shapes.len() - 1
            });
            self.declare(name, Some(Var { kind: VarKind::Rec, ty, array: bounds.is_some(), local, shape }));
        }
        self.put(i, end, &text.join(" : "));
        end
    }

    /// Parentheses open at `j`, counted from `a`.
    fn close_depth(&self, a: usize, j: usize) -> i32 {
        let mut depth = 0;
        for k in a..j {
            if self.is_op(k, "(") {
                depth += 1;
            } else if self.is_op(k, ")") {
                depth -= 1;
            }
        }
        depth
    }

    /// A SUB or FUNCTION header: record pointers become UINTEGER, and the
    /// parameters are the first locals of its body.
    fn header(&mut self, i: usize, declare: bool) -> usize {
        let end = self.stmt_end(i + 1, false);
        let mut j = i + 1;
        if self.is_kw(j, "fastcall") || self.is_kw(j, "stdcall") {
            j += 1;
        }
        j += 1;
        let mut edits: Vec<(usize, usize, String)> = Vec::new();
        let mut params: Vec<(String, Option<Var>)> = Vec::new();
        if self.is_op(j, "(") {
            let Some(rp) = self.close(j, end) else {
                self.keep(i);
                return i + 1;
            };
            for (a, b) in self.split(j + 1, rp) {
                let mut k = a;
                let given = self.is_kw(k, "byval") || self.is_kw(k, "byref");
                if given {
                    k += 1;
                }
                let Some(pname) = self.word(k) else { continue };
                let as_at = (k + 1..b).find(|&x| self.is_kw(x, "as"));
                let ty = as_at.and_then(|x| self.word(x + 1)).and_then(|t| self.type_named(t));
                match (as_at, ty) {
                    (Some(x), Some(ty)) if self.is_kw(x + 2, "ptr") => {
                        edits.push((x + 1, x + 3, "UINTEGER".into()));
                        if !given {
                            edits.push((k, k, "BYVAL ".into()));
                        }
                        let array = self.is_op(k + 1, "(");
                        params.push((pname.to_string(), Some(Var { kind: VarKind::Ptr, ty, array, local: true, shape: None })));
                    }
                    (Some(_), Some(ty)) => {
                        let tname = self.types[ty].name.clone();
                        self.error(
                            i,
                            &format!("A record is passed by pointer: '{pname} AS {tname} PTR', and '@record' in the call"),
                        );
                    }
                    _ => params.push((pname.to_string(), None)),
                }
            }
            j = rp + 1;
        }
        if self.is_kw(j, "as") {
            if let Some(ty) = self.word(j + 1).and_then(|t| self.type_named(t)) {
                if self.is_kw(j + 2, "ptr") {
                    edits.push((j + 1, j + 3, "UINTEGER".into()));
                } else {
                    let tname = self.types[ty].name.clone();
                    self.error(i, &format!("A FUNCTION returns a pointer to a record: 'AS {tname} PTR'"));
                }
            }
        }
        edits.sort_by_key(|e| (e.0, e.1));
        let mut k = i;
        let mut edits = edits.into_iter().peekable();
        while k < end {
            match edits.peek() {
                Some(&(s, e, _)) if s == k && e == k => {
                    let (_, _, text) = edits.next().unwrap();
                    let at = self.toks[k].s;
                    self.out.push_str(&self.src[self.cur..at]);
                    self.out.push_str(&text);
                    self.cur = at;
                }
                Some(&(s, e, _)) if s == k => {
                    let (_, _, text) = edits.next().unwrap();
                    self.put(s, e, &text);
                    k = e;
                }
                _ => {
                    self.keep(k);
                    k += 1;
                }
            }
        }
        if !declare {
            self.locals = Some(HashMap::new());
            for (name, var) in params {
                self.declare(&name, var);
            }
        }
        end
    }

    /// Follows `#pragma case_insensitive` and `array_base`, and their push
    /// and pop.
    fn pragma(&mut self, i: usize) {
        let text: String = self.text(i).chars().filter(|c| !c.is_whitespace()).collect::<String>().to_ascii_lowercase();
        let Some(rest) = text.strip_prefix("#pragma") else { return };
        if let Some(v) = rest.strip_prefix("case_insensitive=") {
            self.ci = v.starts_with("true") || v.starts_with('1');
        } else if rest.starts_with("push(case_insensitive)") {
            self.ci_stack.push(self.ci);
        } else if rest.starts_with("pop(case_insensitive)") {
            if let Some(v) = self.ci_stack.pop() {
                self.ci = v;
            }
        } else if let Some(v) = rest.strip_prefix("array_base=") {
            if let Ok(v) = v.parse() {
                self.base = v;
            }
        } else if rest.starts_with("push(array_base)") {
            self.base_stack.push(self.base);
        } else if rest.starts_with("pop(array_base)") {
            if let Some(v) = self.base_stack.pop() {
                self.base = v;
            }
        }
    }
}

/// The tokens of `src`, as far as records need them; what lies between two
/// tokens is space or a line continuation.
fn scan(src: &str) -> Vec<Tok> {
    let b = src.as_bytes();
    let n = b.len();
    let mut toks = Vec::new();
    let mut i = 0;
    let mut line = 0;
    let mut line_start = true;
    let lines_in = |s: usize, e: usize| b[s..e].iter().filter(|&&c| c == b'\n').count();
    while i < n {
        let c = b[i];
        let s = i;
        let at_line_start = line_start;
        line_start = false;
        let k = match c {
            b'\n' => {
                i += 1;
                line_start = true;
                toks.push(Tok { k: K::Nl, s, e: i, line });
                line += 1;
                continue;
            }
            b' ' | b'\t' | b'\r' => {
                i += 1;
                line_start = at_line_start;
                continue;
            }
            b'/' if b.get(i + 1) == Some(&b'\'') => {
                // A block comment; they nest.
                let mut depth = 0;
                while i < n {
                    if b[i] == b'/' && b.get(i + 1) == Some(&b'\'') {
                        depth += 1;
                        i += 2;
                    } else if b[i] == b'\'' && b.get(i + 1) == Some(&b'/') {
                        depth -= 1;
                        i += 2;
                        if depth == 0 {
                            break;
                        }
                    } else {
                        i += 1;
                    }
                }
                K::Raw
            }
            b'\'' => {
                while i < n && b[i] != b'\n' {
                    i += 1;
                }
                K::Rem
            }
            b'#' if at_line_start => {
                while i < n && b[i] != b'\n' {
                    i += 1;
                }
                K::Raw
            }
            b'"' => {
                i += 1;
                while i < n {
                    if b[i] == b'"' {
                        if b.get(i + 1) == Some(&b'"') {
                            i += 2;
                            continue;
                        }
                        i += 1;
                        break;
                    }
                    if b[i] == b'\n' {
                        break;
                    }
                    i += 1;
                }
                K::Str
            }
            b'_' | b'\\' if continuation(b, i).is_some() => {
                i = continuation(b, i).unwrap();
                line += 1;
                line_start = false;
                continue;
            }
            c if is_word_start(c) => {
                while i < n && is_word(b[i]) {
                    i += 1;
                }
                if i < n && (b[i] == b'$' || b[i] == b'%') {
                    i += 1;
                }
                let word = &src[s..i];
                if word.eq_ignore_ascii_case("rem") && (i == n || matches!(b[i], b' ' | b'\t' | b'\n' | b'\r')) {
                    while i < n && b[i] != b'\n' {
                        i += 1;
                    }
                    K::Rem
                } else if word.eq_ignore_ascii_case("asm") {
                    i = asm_end(b, i);
                    K::Raw
                } else {
                    K::Word
                }
            }
            c if c.is_ascii_digit()
                || (c == b'.' && b.get(i + 1).is_some_and(u8::is_ascii_digit))
                || (c == b'$' && b.get(i + 1).is_some_and(u8::is_ascii_hexdigit))
                || (c == b'%' && matches!(b.get(i + 1), Some(b'0' | b'1'))) =>
            {
                i += 1;
                while i < n && (b[i].is_ascii_alphanumeric() || b[i] == b'.') {
                    if matches!(b[i], b'e' | b'E') && matches!(b.get(i + 1), Some(b'+' | b'-')) {
                        i += 1;
                    }
                    i += 1;
                }
                K::Num
            }
            _ => {
                let two = &src.as_bytes()[i..(i + 2).min(n)];
                i += if matches!(two, b"->" | b":=" | b">=" | b"<=" | b"<>" | b"=>" | b"<<" | b">>") { 2 } else { 1 };
                // Keep to character boundaries in text that is not ASCII.
                while !src.is_char_boundary(i) {
                    i += 1;
                }
                K::Op
            }
        };
        toks.push(Tok { k, s, e: i, line });
        line += lines_in(s, i);
    }
    toks
}

/// The end of a line continuation (`_` or `\`, then space or a comment, then
/// the end of the line) at `i`, after its newline.
fn continuation(b: &[u8], i: usize) -> Option<usize> {
    let mut j = i + 1;
    while j < b.len() && matches!(b[j], b' ' | b'\t') {
        j += 1;
    }
    let rem = b.len() >= j + 3 && b[j..j + 3].eq_ignore_ascii_case(b"rem") && !b.get(j + 3).is_some_and(|&c| is_word(c));
    if j < b.len() && (b[j] == b'\'' || rem) {
        while j < b.len() && b[j] != b'\n' {
            j += 1;
        }
    }
    if j < b.len() && b[j] == b'\r' {
        j += 1;
    }
    (j < b.len() && b[j] == b'\n').then_some(j + 1)
}

/// The end of `end asm`, for an `asm` that ends at `i`.
fn asm_end(b: &[u8], mut i: usize) -> usize {
    let n = b.len();
    while i < n {
        match b[i] {
            b';' => {
                while i < n && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'"' => {
                i += 1;
                while i < n && b[i] != b'"' && b[i] != b'\n' {
                    i += 1;
                }
                i += 1;
            }
            c if (c == b'e' || c == b'E') && (i == 0 || !is_word(b[i - 1])) && b[i..].len() >= 3 && b[i..i + 3].eq_ignore_ascii_case(b"end") => {
                let mut j = i + 3;
                if j < n && matches!(b[j], b' ' | b'\t') {
                    while j < n && matches!(b[j], b' ' | b'\t') {
                        j += 1;
                    }
                    if b[j..].len() >= 3 && b[j..j + 3].eq_ignore_ascii_case(b"asm") && !b.get(j + 3).is_some_and(|&c| is_word(c)) {
                        return j + 3;
                    }
                }
                i += 3;
            }
            _ => i += 1,
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> (String, String) {
        let mut sess = Session::default();
        sess.filename = "t.bas".into();
        let buf = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        sess.stderr = Box::new(Shared(buf.clone()));
        let out = lower(&mut sess, src, false, 0).unwrap_or_else(|| src.to_string());
        let errors = String::from_utf8(buf.borrow().clone()).unwrap();
        (out, errors)
    }

    struct Shared(std::rc::Rc<std::cell::RefCell<Vec<u8>>>);
    impl std::io::Write for Shared {
        fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
            self.0.borrow_mut().extend_from_slice(data);
            Ok(data.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    const FIELD: &str = "TYPE Field\n  col, row AS UBYTE\n  max AS UBYTE ' the most\n  buf AS UINTEGER\n  next AS Field PTR\nEND TYPE\n";

    fn lowered(body: &str) -> String {
        let (out, errors) = run(&format!("{FIELD}{body}"));
        assert_eq!(errors, "");
        assert_eq!(out.lines().count(), FIELD.lines().count() + body.lines().count());
        out.lines().skip(FIELD.lines().count()).collect::<Vec<_>>().join("\n")
    }

    #[test]
    fn no_type_no_change() {
        let mut sess = Session::default();
        assert!(lower(&mut sess, "dim type as ubyte\ntype = 1\n", false, 0).is_none());
        assert!(lower(&mut sess, "print 1\n", false, 0).is_none());
    }

    #[test]
    fn the_block_keeps_its_lines() {
        let (out, errors) = run(FIELD);
        assert_eq!(errors, "");
        assert_eq!(out, "\n\n\n\n\n\n");
    }

    #[test]
    fn globals() {
        assert_eq!(
            lowered("dim f as Field\nf.max = f.col + 1\nprint f.buf; SIZEOF(Field)\n"),
            "DIM f(0 TO 6) AS UBYTE\nPOKE UBYTE @f(0) + 2, PEEK(UBYTE, @f(0)) + 1\nprint PEEK(UINTEGER, @f(0) + 3); 7"
        );
        assert_eq!(
            lowered("dim fs(1 TO 8) as Field\nfs(i).row = 3 : x = @fs(i)\n"),
            "DIM fs(1 TO 8, 0 TO 6) AS UBYTE\nPOKE UBYTE @fs(1, 0) + 1 + ((CAST(UINTEGER, i) - (1))) * 7, 3 : x = (@fs(1, 0) + ((CAST(UINTEGER, i) - (1))) * 7)"
        );
    }

    #[test]
    fn locals_and_pointers() {
        assert_eq!(
            lowered("sub s(p as Field ptr, n as ubyte)\n  dim f as Field\n  f.max = p->next->max\n  p->next = @f\nend sub\n"),
            "sub s(BYVAL p as UINTEGER, n as ubyte)\n  DIM f(0 TO 6) AS UBYTE\n  POKE UBYTE @f(2), PEEK(UBYTE, (PEEK(UINTEGER, p + 5)) + 2)\n  POKE UINTEGER p + 5, (@f(0))\nend sub"
        );
        assert_eq!(
            lowered("function at(i as ubyte) as Field ptr\n  return 0\nend function\nprint at(2)->buf\n"),
            "function at(i as ubyte) as UINTEGER\n  return 0\nend function\nprint PEEK(UINTEGER, (at(2)) + 3)"
        );
    }

    #[test]
    fn array_base_and_sizes() {
        // The bounds are written out under the array_base of the DIM; a
        // size that is a power of two is a shift.
        assert_eq!(
            lowered("#pragma array_base = 1\ndim fs(3) as Field\nx = fs(i).max\n"),
            "#pragma array_base = 1\nDIM fs(1 TO 3, 0 TO 6) AS UBYTE\nx = PEEK(UBYTE, @fs(1, 0) + 2 + ((CAST(UINTEGER, i) - (1))) * 7)"
        );
        let (out, errors) = run("TYPE Q\n  a, b AS ULONG\nEND TYPE\ndim q(3) as Q\nx = q(i).b\nx = q(2).b\n");
        assert_eq!(errors, "");
        assert_eq!(
            out.lines().skip(4).collect::<Vec<_>>(),
            ["x = PEEK(ULONG, @q(0, 0) + 4 + ((CAST(UINTEGER, i)) << 3))", "x = PEEK(ULONG, @q(0, 0) + 20)"]
        );
    }

    #[test]
    fn a_local_name_hides_a_global_record() {
        assert_eq!(
            lowered("dim f as Field\nsub s()\n  dim f as ubyte\n  f = 1\nend sub\n"),
            "DIM f(0 TO 6) AS UBYTE\nsub s()\n  dim f as ubyte\n  f = 1\nend sub"
        );
    }

    #[test]
    fn nested_and_array_fields() {
        let (out, errors) = run(
            "TYPE P\n  x, y AS INTEGER\nEND TYPE\nTYPE S\n  pos AS P\n  cells(1 TO 4) AS UINTEGER\nEND TYPE\ndim s as S\ns.pos.y = s.cells(2) + s.cells(j)\n",
        );
        assert_eq!(errors, "");
        assert_eq!(
            out.lines().last().unwrap(),
            "POKE INTEGER @s(0) + 2, PEEK(UINTEGER, @s(0) + 6) + PEEK(UINTEGER, @s(0) + 4 + ((CAST(UINTEGER, j) - 1) << 1))"
        );
    }

    #[test]
    fn strings_comments_and_asm_are_left_alone() {
        assert_eq!(
            lowered("dim f as Field\nprint \"f.max\" ' f.max\nasm\n  ld a, (f.max)\nend asm\n"),
            "DIM f(0 TO 6) AS UBYTE\nprint \"f.max\" ' f.max\nasm\n  ld a, (f.max)\nend asm"
        );
    }

    #[test]
    fn errors() {
        let (_, e) = run(&format!("{FIELD}dim f as Field\nprint f.nope\n"));
        assert!(e.contains("t.bas:8: error: TYPE 'Field' has no field 'nope'"), "{e}");
        let (_, e) = run(&format!("{FIELD}dim p as Field ptr\nprint p.max\n"));
        assert!(e.contains("'p' is a pointer: use '->'"), "{e}");
        let (_, e) = run(&format!("{FIELD}dim f, g as Field\nf = g\n"));
        assert!(e.contains("cannot be assigned as a whole"), "{e}");
        let (_, e) = run("TYPE A\n  b AS B\nEND TYPE\nTYPE B\n  a AS A\nEND TYPE\n");
        assert!(e.contains("contains itself"), "{e}");
        let (_, e) = run("TYPE A\n  s AS STRING\nEND TYPE\n");
        assert!(e.contains("STRING fields are not supported"), "{e}");
        let (_, e) = run("TYPE A\n  x AS UBYTE\n");
        assert!(e.contains("TYPE without END TYPE"), "{e}");
    }
}
