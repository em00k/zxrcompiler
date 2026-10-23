//! The assembler.
//!
//! As with the preprocessor, this follows the Python `zxbasm` rule for rule:
//! the grammars are `grammar/zxbasm.y` and `grammar/zxbasm_next.y`, and each
//! arm of the reduction is the body of the Python function of that name.

pub mod expr;
pub mod instr;
pub mod lexer;
pub mod memory;
pub mod tape;

#[allow(clippy::all)]
pub mod grammar {
    pub mod next {
        include!(concat!(env!("OUT_DIR"), "/grammar_next.rs"));
    }
    pub mod z80 {
        include!(concat!(env!("OUT_DIR"), "/grammar_z80.rs"));
    }
}

use expr::{Expr, Label, LabelValue};
use lexer::Mark;
use std::collections::HashMap;
use std::rc::Rc;
use grammar::next::{term, Rule};
use instr::{Arg, Asm, Kind};
use lexer::{AsmLexer, TokenValue};
use memory::Memory;
use zxr_core::{paths, Session};
use zxr_lalr::{Driver, OnError, ParseOptions, Symbol, SyntaxError};
use zxr_pp::Resolver;

#[derive(Clone, Debug)]
pub struct AsmOptions {
    /// Accept the Z80N instructions and the CODEBANK directive.
    pub zxnext: bool,
    /// `-b`: only brackets mean memory indirection.
    pub force_brackets: bool,
    /// Address code banks are assembled at.
    pub code_window: i128,
    pub code_window_size: i128,
    /// Physical 8K page of bank 1.
    pub code_bank_base: i128,
    /// Explicit first page per bank, comma separated; overrides the base.
    pub code_bank_pages: String,
    pub use_basic_loader: bool,
    pub autorun: bool,
}

impl Default for AsmOptions {
    fn default() -> Self {
        AsmOptions { zxnext: false, force_brackets: false, code_window: 0x6000, code_window_size: 8192, code_bank_base: 30, code_bank_pages: String::new(), use_basic_loader: false, autorun: false }
    }
}

impl AsmOptions {
    fn window_slots(&self) -> i128 {
        if self.code_window_size >= 16384 { 2 } else { 1 }
    }

    /// The first physical 8K page of a logical bank.
    fn physical_page(&self, bank: i128) -> i128 {
        if bank <= 0 {
            return 0;
        }
        let stride = self.window_slots();
        // Python's int(text, 0): decimal, or 0x / 0o / 0b prefixed.
        let parse = |text: &str| {
            let t = text.to_lowercase();
            let (digits, radix) = if let Some(d) = t.strip_prefix("0x") {
                (d, 16)
            } else if let Some(d) = t.strip_prefix("0b") {
                (d, 2)
            } else if let Some(d) = t.strip_prefix("0o") {
                (d, 8)
            } else {
                (t.as_str(), 10)
            };
            i128::from_str_radix(digits, radix).unwrap_or(0)
        };
        let pages: Vec<&str> = self.code_bank_pages.split(',').map(str::trim).filter(|p| !p.is_empty()).collect();
        if pages.is_empty() {
            return self.code_bank_base + (bank - 1) * stride;
        }
        match pages.get(bank as usize - 1) {
            Some(page) => parse(page),
            None => parse(pages[pages.len() - 1]) + (bank - pages.len() as i128) * stride,
        }
    }
}

#[derive(Default)]
pub enum Value {
    #[default]
    None,
    Text(String),
    Int(i128),
    /// An instruction, or nothing where there was an error or a directive.
    Asm(Option<Box<Asm>>),
    Expr(Expr),
    Exprs(Vec<Expr>),
    /// An indexed operand: the pattern `(IX+N)` and the displacement.
    Indexed(String, Expr),
    /// An operation name with the line it is on.
    Named(String, u32),
    Ids(Vec<(String, u32)>),
}

type Sym = Symbol<Value>;

fn text(sym: &Sym) -> String {
    match &sym.value {
        Value::Text(s) => s.clone(),
        Value::Int(i) => i.to_string(),
        _ => panic!("expected a token or register name"),
    }
}

fn expr_of(sym: &Sym) -> Expr {
    match &sym.value {
        Value::Expr(e) => e.clone(),
        _ => panic!("expected an expression"),
    }
}

fn exprs_of(sym: &mut Sym) -> Vec<Expr> {
    match std::mem::take(&mut sym.value) {
        Value::Exprs(e) => e,
        _ => panic!("expected an expression list"),
    }
}

fn indexed(sym: &Sym) -> (String, Expr) {
    match &sym.value {
        Value::Indexed(pattern, e) => (pattern.clone(), e.clone()),
        _ => panic!("expected an indexed operand"),
    }
}

fn named(sym: &Sym) -> (String, u32) {
    match &sym.value {
        Value::Named(name, line) => (name.clone(), *line),
        _ => panic!("expected an operation"),
    }
}

/// `data[start:stop]` as Python slices: negative indices count from the end
/// and everything is clamped.
fn python_slice(data: &[u8], start: i128, stop: i128) -> Vec<u8> {
    let len = data.len() as i128;
    let clamp = |i: i128| if i < 0 { (i + len).max(0) } else { i.min(len) } as usize;
    let (a, b) = (clamp(start), clamp(stop));
    if a < b { data[a..b].to_vec() } else { Vec::new() }
}

pub struct Assembler<'s> {
    pub sess: &'s mut Session,
    pub options: AsmOptions,
    lexer: Option<AsmLexer>,
    pub memory: Memory,
    /// `#init` routines: label and line.
    pub inits: Vec<(String, u32)>,
    /// Where execution starts, if an `END address` said so.
    pub autorun_addr: Option<i128>,
    /// Where INCBIN looks for files.
    pub resolver: Resolver,
    /// Whether debug data (SLD) is wanted: every instruction and label is
    /// then given the source line it came from.
    pub debug: bool,
    /// The source lines the compiler's marks name, the first being 1.
    pub origins: Vec<(Rc<str>, u32)>,
    /// The program's global variables, for debug data: name in BASIC,
    /// label, file and line of the declaration.
    pub variables: Vec<(String, String, Rc<str>, u32)>,
    /// The source line of the tokens last reduced.
    source: Option<(Rc<str>, u32)>,
}

impl<'s> Assembler<'s> {
    pub fn new(sess: &'s mut Session, options: AsmOptions, resolver: Resolver) -> Assembler<'s> {
        Assembler { sess, options, lexer: None, memory: Memory::default(), inits: Vec::new(), autorun_addr: None, resolver, debug: false, origins: Vec::new(), variables: Vec::new(), source: None }
    }

    /// Assembles source text into [`Assembler::memory`]. Returns the number
    /// of errors so far.
    pub fn assemble(&mut self, input: &str) -> u32 {
        self.lexer = Some(AsmLexer::new(input, self.options.zxnext, self.options.force_brackets));
        let tables = if self.options.zxnext { &grammar::next::TABLES } else { &grammar::z80::TABLES };
        zxr_lalr::parse(tables, self, ParseOptions { tracking: false, defaulted_states: true });
        if let Some(&line) = self.memory.scopes.last() {
            self.sess.error(line, "Missing ENDP to close this scope", None);
        }
        self.sess.has_errors
    }

    /// The source line of what begins at `lexpos`, itself on line `lineno`
    /// of the text: that very line in a source file, or the line the
    /// compiler generated it for.
    fn source_at(&self, lexpos: usize, lineno: u32) -> Option<(Rc<str>, u32)> {
        match self.lexer.as_ref().and_then(|lexer| lexer.mark_at(lexpos)) {
            None => Some((Rc::from(self.sess.filename.as_str()), lineno)),
            Some(Mark::Track(file)) => Some((Rc::clone(file), lineno)),
            Some(Mark::Pin(0)) => None,
            Some(Mark::Pin(n)) => self.origins.get(*n as usize - 1).cloned(),
        }
    }

    fn eval(&mut self, e: &Expr) -> Option<i128> {
        expr::eval(e, true, self.sess)
    }

    fn ins(&mut self, lineno: u32, asm: String, args: Vec<Arg>) -> Value {
        Value::Asm(Asm::instruction(self.sess, lineno, &asm, args).map(Box::new))
    }

    fn add(&mut self, value: Value) {
        if let Value::Asm(Some(asm)) = value {
            self.memory.add_instruction(self.sess, *asm);
        }
    }

    fn rules(&self) -> &'static [Rule] {
        if self.options.zxnext { &grammar::next::RULES } else { &grammar::z80::RULES }
    }

    /// Adds an instruction outside the source text (the calls to `#init`
    /// routines that the standalone assembler appends).
    pub fn add_instruction(&mut self, asm: &str, args: Vec<Arg>) {
        let value = self.ins(0, asm.to_string(), args);
        self.add(value);
    }

    /// The largest routines of a bank, for the message when it overflows.
    fn routines_in_segment(&mut self, segment: i64) -> String {
        let mut labels: Vec<(String, i128)> = self
            .memory
            .global_labels()
            .values()
            .filter_map(|label| {
                let l = label.borrow();
                let Some(LabelValue::Int(value)) = l.value else { return None };
                (l.segment == segment && l.is_address && !l.name().ends_with("__leave")).then(|| (l.name(), value))
            })
            .collect();
        labels.sort_by_key(|(_, value)| *value);
        if labels.is_empty() {
            return "<no labels>".to_string();
        }
        let (_, data) = self.memory.dump(self.sess, segment);
        let top = labels[0].1 + data.len() as i128;
        let mut sizes: Vec<(String, i128)> =
            labels.iter().enumerate().map(|(i, (name, value))| (name.trim_start_matches('.').to_string(), labels.get(i + 1).map_or(top, |next| next.1) - value)).collect();
        sizes.sort_by_key(|(_, size)| -size);
        const LIMIT: usize = 12;
        let mut shown = sizes.iter().take(LIMIT).map(|(name, size)| format!("{name} ({size} bytes)")).collect::<Vec<_>>().join(", ");
        if sizes.len() > LIMIT {
            shown.push_str(&format!(", and {} more", sizes.len() - LIMIT));
        }
        shown
    }

    /// Writes each code bank beside the main output as `<name>.bank<N>.bin`,
    /// and a `<name>.banks.json` manifest describing them.
    fn generate_code_banks(&mut self, output: &str, resident_org: i128, resident_len: usize) -> std::io::Result<()> {
        let ids: Vec<i64> = self.memory.segments.keys().copied().filter(|&id| id != 0).collect();
        let mut banks = Vec::new();
        for id in ids {
            let (org, data) = self.memory.dump(self.sess, id);
            if !data.is_empty() {
                banks.push((id, org, data));
            }
        }
        if banks.is_empty() {
            return Ok(());
        }
        let window_start = self.options.code_window;
        let window_end = window_start + self.options.code_window_size;
        let resident_end = resident_org + resident_len as i128;
        if resident_org < window_end && window_start < resident_end {
            let message = format!(
                "The code window (${window_start:04X}-${:04X}) overlaps the resident program (${resident_org:04X}-${:04X}). Move the program ORG or the code window",
                window_end - 1,
                resident_end - 1
            );
            self.sess.error(0, &message, None);
            return Ok(());
        }
        let base = match output.rfind('.') {
            Some(dot) if !output[dot..].contains('/') && dot > output.rfind('/').map_or(0, |s| s + 1) => &output[..dot],
            _ => output,
        };
        let mut entries = Vec::new();
        for (id, org, data) in banks {
            let size = self.options.code_window_size;
            if data.len() as i128 > size {
                let routines = self.routines_in_segment(id);
                let message = format!("CODEBANK {id} is {} bytes, {} over the {size} byte code window. Largest routines: {routines}", data.len(), data.len() as i128 - size);
                self.sess.error(0, &message, None);
                continue;
            }
            let fname = format!("{base}.bank{id}.bin");
            std::fs::write(&fname, &data)?;
            let first = self.options.physical_page(id as i128);
            let pages: Vec<String> = (0..self.options.window_slots()).map(|i| format!("        {}", first + i)).collect();
            entries.push(format!(
                "    {{\n      \"bank\": {id},\n      \"file\": {},\n      \"org\": {org},\n      \"size\": {},\n      \"page\": {first},\n      \"pages\": [\n{}\n      ]\n    }}",
                json_string(&paths::basename(&fname)),
                data.len(),
                pages.join(",\n")
            ));
        }
        if !entries.is_empty() {
            // The layout of Python's json.dump(..., indent=2).
            let json = format!("{{\n  \"window\": {window_start},\n  \"window_size\": {},\n  \"banks\": [\n{}\n  ]\n}}", self.options.code_window_size, entries.join(",\n"));
            std::fs::write(format!("{base}.banks.json"), json)?;
        }
        Ok(())
    }

    /// The 8K page of the Next's memory an address of a segment is in: a
    /// code bank's own pages, or for the resident program what a NEX file
    /// maps there (banks 5, 2 and 0). None below $4000, where the ROM is.
    fn page_of(&self, segment: i64, address: i128) -> Option<i128> {
        if segment != 0 {
            return Some(self.options.physical_page(segment as i128) + (address - self.options.code_window).max(0) / 8192);
        }
        [None, None, Some(10), Some(11), Some(4), Some(5), Some(0), Some(1)].get((address >> 13) as usize).copied().flatten()
    }

    /// Source-level debug data in the SLD format of sjasmplus, version 1:
    /// a line for each instruction with the source line it came from, and
    /// one for each label. `top` is the main source file.
    pub fn sld(&mut self, top: &str) -> String {
        let mut paths: HashMap<Rc<str>, String> = HashMap::new();
        let mut path = |file: &Rc<str>| -> String {
            paths.entry(Rc::clone(file)).or_insert_with(|| std::fs::canonicalize(&**file).map_or_else(|_| file.to_string(), |p| p.to_string_lossy().into_owned())).clone()
        };
        let top: Rc<str> = Rc::from(top);
        let mut out = vec![
            "|SLD.data.version|1".to_string(),
            "||K|KEYWORDS|WPMEM,LOGPOINT,ASSERTION".to_string(),
            format!("{}|1||0|-1|-1|Z|pages.size:8192,pages.count:224,slots.count:8,slots.adr:0,8192,16384,24576,32768,40960,49152,57344", path(&top)),
        ];
        let labels: Vec<Label> = self.memory.global_labels().values().cloned().collect();
        let mut known: HashMap<String, (i128, Option<i128>)> = HashMap::new();
        for label in labels {
            let (name, value, is_address, segment, source) = {
                let l = label.borrow();
                (l.name(), l.value.clone(), l.is_address, l.segment, l.source.clone())
            };
            let value = match value {
                Some(LabelValue::Int(value)) => value,
                Some(LabelValue::Expr(e)) => match expr::eval(&e, false, self.sess) {
                    Some(value) => value,
                    None => continue,
                },
                None => continue,
            };
            let (file, line) = source.unwrap_or((Rc::clone(&top), 1));
            let name = name.strip_prefix('.').unwrap_or(&name);
            let page = if is_address { self.page_of(segment, value) } else { None };
            match page {
                Some(page) => out.push(format!("{}|{line}||0|{page}|{value}|L|,{name},,", path(&file))),
                None => out.push(format!("{}|{line}||0|-1|{value}|L|,{name},,+equ", path(&file))),
            }
            known.insert(name.to_string(), (value, page));
        }
        // A variable is `_name` to the assembler and `name` in the source,
        // which is the word a debugger is given. An asm label of that name
        // keeps it.
        for (name, label, file, line) in &self.variables {
            if let (Some(&(value, Some(page))), false) = (known.get(label), known.contains_key(name)) {
                out.push(format!("{}|{line}||0|{page}|{value}|L|,{name},,", path(file)));
            }
        }
        for seg in self.memory.segments.values() {
            for (&address, indexes) in &seg.orgs {
                for &index in indexes {
                    let instr = &self.memory.instructions[index];
                    let (Kind::Opcode, Some(Some((file, line))), Some(page)) = (instr.kind, &instr.source, self.page_of(seg.id, address)) else { continue };
                    out.push(format!("{}|{line}||0|{page}|{address}|T|", path(file)));
                }
            }
        }
        out.join("\n") + "\n"
    }

    /// Writes the assembled program as `bin`, `tap` or `tzx`.
    pub fn generate_binary(&mut self, output: &str, format: &str) -> std::io::Result<()> {
        self.memory.check_undefined_labels(self.sess);
        self.memory.check_cross_segment_refs(self.sess);
        let (org, binary) = self.memory.dump(self.sess, 0);
        if self.sess.has_errors > 0 {
            return Ok(());
        }
        self.generate_code_banks(output, org, binary.len())?;
        if self.sess.has_errors > 0 {
            return Ok(());
        }
        let entry = *self.autorun_addr.get_or_insert(org);
        let loader = self.options.use_basic_loader.then(|| {
            use tape::{Basic, Part, CLEAR, CODE, LOAD, RANDOMIZE, REM, USR};
            let mut program = Basic::default();
            if org > 16383 {
                program.add_line(&[(CLEAR, vec![Part::Number(org - 1)])]);
            }
            program.add_line(&[(LOAD, vec![Part::Literal("\"\""), Part::Token(CODE)])]);
            let run = (RANDOMIZE, vec![Part::Token(USR), Part::Number(entry)]);
            if self.options.autorun {
                program.add_line(&[run]);
            } else {
                program.add_line(&[(REM, Vec::new()), run]);
            }
            program.bytes
        });
        let data = match format {
            "tap" | "tzx" => {
                let mut tape = tape::Tape::new(if format == "tap" { tape::Format::Tap } else { tape::Format::Tzx });
                if let Some(loader) = &loader {
                    tape.save_program("loader", loader, 1);
                }
                let name: String = paths::basename(output).chars().take(10).collect();
                tape.save_code(&name, entry, &binary);
                tape.output
            }
            _ => binary,
        };
        std::fs::write(output, data)
    }
}

/// A string as Python's `json` writes it: ASCII only, the rest escaped.
fn json_string(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 || (c as u32) > 0x7E => {
                let mut units = [0u16; 2];
                for unit in c.encode_utf16(&mut units) {
                    out.push_str(&format!("\\u{unit:04x}"));
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

impl Driver<Value> for Assembler<'_> {
    fn token(&mut self) -> Option<Sym> {
        let token = self.lexer.as_mut()?.token(self.sess)?;
        let value = match token.value {
            TokenValue::Text(s) => Value::Text(s),
            TokenValue::Int(i) => Value::Int(i),
        };
        Some(Symbol { kind: token.kind, value, lineno: token.lineno, lexpos: token.lexpos, ..Symbol::default() })
    }

    fn syntax_error(&mut self, token: Option<&Sym>) -> OnError<Value> {
        match token {
            Some(token) if token.kind == term::NEWLINE => self.sess.error(token.lineno, "Syntax error. Unexpected end of line [NEWLINE]", None),
            Some(token) => {
                let name = grammar::next::TABLES.terminal_names[token.kind as usize];
                self.sess.error(token.lineno, &format!("Syntax error. Unexpected token '{}' [{name}]", text(token)), None);
            }
            None => {
                self.sess.write_stderr("General syntax error at assembler (unexpected End of File?)");
                self.sess.has_errors += 1;
            }
        }
        OnError::Recover
    }

    fn reduce(&mut self, production: usize, rhs: &mut [Sym]) -> Result<Value, SyntaxError> {
        let line = |rhs: &[Sym], n: usize| rhs[n - 1].lineno;
        let e = |sym: &Sym| Arg::Expr(expr_of(sym));
        // Any token of the rule will do: marks stand between lines. (A
        // symbol that is not a token has no position.)
        // A rule made of other rules alone is of the line they were of.
        if self.debug {
            if let Some(token) = rhs.iter().find(|sym| sym.lexpos != 0) {
                self.source = self.source_at(token.lexpos, token.lineno);
            }
            self.memory.label_source = self.source.clone();
        }
        let source = self.source.clone();
        let mut value = match self.rules()[production] {
            Rule::start_ | Rule::p_start | Rule::p_program_endline | Rule::p_program | Rule::p_program_line | Rule::p_line_asm | Rule::p_preprocessor_line | Rule::p_asms_empty => Value::None,
            Rule::p_program_endline2 => {
                self.autorun_addr = self.eval(&expr_of(&rhs[1]));
                Value::None
            }
            Rule::p_def_label => {
                self.memory.declare_label(self.sess, &text(&rhs[0]), line(rhs, 1), Some(LabelValue::Expr(expr_of(&rhs[2]))));
                Value::None
            }
            Rule::p_asms_asm => {
                self.add(std::mem::take(&mut rhs[0].value));
                Value::None
            }
            Rule::p_asms_asms_asm => {
                self.add(std::mem::take(&mut rhs[2].value));
                Value::None
            }
            Rule::p_asm_label => {
                self.memory.declare_label(self.sess, &text(&rhs[0]), line(rhs, 1), None);
                Value::None
            }
            Rule::p_asm_ld8 => {
                let (a, b) = (text(&rhs[1]), text(&rhs[3]));
                if matches!(a.as_str(), "H" | "L") && matches!(b.as_str(), "IXH" | "IXL" | "IYH" | "IYL") {
                    self.sess.error(0, &format!("Unexpected token '{b}'"), None);
                    Value::Asm(None)
                } else {
                    self.ins(line(rhs, 1), format!("LD {a},{b}"), vec![])
                }
            }
            Rule::p_LDa => {
                let operands: String = rhs[1..].iter().map(|s| text(s).replace('[', "(").replace(']', ")")).collect();
                self.ins(line(rhs, 1), format!("LD {operands}"), vec![])
            }
            Rule::p_PROC => {
                self.memory.enter_proc(line(rhs, 1));
                Value::Asm(None)
            }
            Rule::p_ENDP => {
                self.memory.exit_proc(self.sess, line(rhs, 1));
                Value::Asm(None)
            }
            Rule::p_LOCAL => {
                let Value::Ids(ids) = std::mem::take(&mut rhs[1].value) else { unreachable!() };
                for (label, lineno) in ids {
                    self.memory.set_label(self.sess, &label, lineno);
                }
                Value::Asm(None)
            }
            Rule::p_idlist => Value::Ids(vec![(text(&rhs[0]), line(rhs, 1))]),
            Rule::p_idlist_id => {
                let Value::Ids(mut ids) = std::mem::take(&mut rhs[0].value) else { unreachable!() };
                ids.push((text(&rhs[2]), line(rhs, 3)));
                Value::Ids(ids)
            }
            Rule::p_DEFB => Value::Asm(Some(Box::new(Asm::data(line(rhs, 1), Kind::Defb, exprs_of(&mut rhs[1]))))),
            Rule::p_DEFS => {
                let mut args = exprs_of(&mut rhs[1]);
                if args.len() > 2 {
                    self.sess.error(line(rhs, 1), "too many arguments for DEFS", None);
                }
                if args.len() < 2 {
                    args.push(expr::int(0, line(rhs, 1)));
                }
                Value::Asm(Some(Box::new(Asm::data(line(rhs, 1), Kind::Defs, args))))
            }
            Rule::p_DEFW => Value::Asm(Some(Box::new(Asm::data(line(rhs, 1), Kind::Defw, exprs_of(&mut rhs[1]))))),
            Rule::p_expr_list_from_string => Value::Exprs(text(&rhs[0]).chars().map(|c| expr::int(c as i128, line(rhs, 1))).collect()),
            Rule::p_expr_list_from_num | Rule::p_number_list => Value::Exprs(vec![expr_of(&rhs[0])]),
            Rule::p_expr_list_plus_expr | Rule::p_number_list_number => {
                let mut list = exprs_of(&mut rhs[0]);
                list.push(expr_of(&rhs[2]));
                Value::Exprs(list)
            }
            Rule::p_expr_list_plus_string => {
                let mut list = exprs_of(&mut rhs[0]);
                list.extend(text(&rhs[2]).chars().map(|c| expr::int(c as i128, line(rhs, 3))));
                Value::Exprs(list)
            }
            Rule::p_asm_ldind_r8 => {
                let (pattern, displacement) = indexed(&rhs[1]);
                self.ins(line(rhs, 1), format!("LD {pattern},{}", text(&rhs[3])), vec![Arg::Expr(displacement)])
            }
            Rule::p_asm_ldr8_ind => {
                let (pattern, displacement) = indexed(&rhs[3]);
                self.ins(line(rhs, 1), format!("LD {},{pattern}", text(&rhs[1])), vec![Arg::Expr(displacement)])
            }
            Rule::p_reg8_hl => Value::Text("(HL)".to_string()),
            Rule::p_ind8_I => {
                let register = text(&rhs[1]);
                let displacement = if rhs.len() == 5 {
                    // `(IX + (expr))`: the sign is a token of its own.
                    let inner = expr_of(&rhs[3]);
                    if text(&rhs[2]) == "-" { expr::op("-", line(rhs, 2), vec![inner]) } else { inner }
                } else {
                    // `(IX+expr)`: the expression must start with a sign.
                    let inner = expr_of(&rhs[2]);
                    let first = leading_token(&inner);
                    if first != "-" && first != "+" {
                        self.sess.error(line(rhs, 2), &format!("Unexpected token '{first}'. Expected '+' or '-'"), None);
                    }
                    inner
                };
                Value::Indexed(format!("({register}+N)"), displacement)
            }
            Rule::p_ex_af_af => self.ins(line(rhs, 1), "EX AF,AF'".to_string(), vec![]),
            Rule::p_ex_de_hl => self.ins(line(rhs, 1), "EX DE,HL".to_string(), vec![]),
            Rule::p_org => {
                if self.memory.current_segment() != 0 {
                    self.sess.error(line(rhs, 1), "ORG is not allowed inside a CODEBANK. The bank is fixed at the code window address", None);
                } else if let Some(value) = self.eval(&expr_of(&rhs[1])) {
                    self.memory.set_org(self.sess, value, line(rhs, 1));
                }
                Value::Asm(None)
            }
            Rule::p_namespace => {
                self.memory.namespace = memory::normalize_namespace(&text(&rhs[1]));
                Value::Asm(None)
            }
            Rule::p_push_namespace => {
                self.memory.namespace_stack.push(self.memory.namespace.clone());
                let name = if rhs.len() == 3 { text(&rhs[2]) } else { self.memory.namespace.clone() };
                self.memory.namespace = memory::normalize_namespace(&name);
                Value::Asm(None)
            }
            Rule::p_pop_namespace => {
                match self.memory.namespace_stack.pop() {
                    Some(previous) => self.memory.namespace = previous,
                    None => {
                        let message = format!("Stack underflow. No more Namespaces to pop. Current namespace is {}", self.memory.namespace);
                        self.sess.error(line(rhs, 2), &message, None);
                    }
                }
                Value::Asm(None)
            }
            Rule::p_align => {
                if let Some(align) = self.eval(&expr_of(&rhs[1])) {
                    if align < 2 {
                        self.sess.error(line(rhs, 1), "ALIGN value must be greater than 1", None);
                    } else {
                        let org = self.memory.org();
                        let padding = (align - org.rem_euclid(align)).rem_euclid(align);
                        self.memory.set_org(self.sess, org + padding, line(rhs, 1));
                    }
                }
                Value::Asm(None)
            }
            Rule::p_incbin => {
                let name = text(&rhs[1]);
                let fname = self.resolver.search(self.sess, &name, line(rhs, 2), true, "");
                if fname.is_empty() {
                    return Ok(Value::Asm(None));
                }
                let Ok(content) = std::fs::read(&fname) else {
                    self.sess.error(line(rhs, 2), &format!("cannot read file '{name}'"), None);
                    return Ok(Value::Asm(None));
                };
                let len = content.len() as i128;
                let mut offset = if rhs.len() > 3 { self.eval(&expr_of(&rhs[3])).unwrap_or(0) } else { 0 };
                let mut length = None;
                if rhs.len() > 5 {
                    let value = self.eval(&expr_of(&rhs[5])).unwrap_or(0);
                    if value < 1 {
                        self.sess.error(line(rhs, 5), "INCBIN length must be greater than 0", None);
                    }
                    length = Some(value);
                }
                if offset < 0 {
                    offset += len;
                    if offset < 0 || offset >= len {
                        self.sess.error(line(rhs, 4), "INCBIN offset is out of range", None);
                    }
                }
                let length = length.unwrap_or(len - offset);
                if offset + length > len {
                    let excess = len - (offset + length);
                    self.sess.warning(rhs.get(4).map_or(0, |s| s.lineno), &format!("INCBIN length if beyond file length by {excess} bytes"), None);
                }
                Value::Asm(Some(Box::new(Asm::blob(line(rhs, 1), python_slice(&content, offset, offset + length)))))
            }
            Rule::p_ex_sp_reg8 => self.ins(line(rhs, 1), format!("EX (SP),{}", text(&rhs[5])), vec![]),
            Rule::p_incdec | Rule::p_PUSH_POP => self.ins(line(rhs, 1), format!("{} {}", text(&rhs[0]), text(&rhs[1])), vec![]),
            Rule::p_incdeci => {
                let (pattern, displacement) = indexed(&rhs[1]);
                self.ins(line(rhs, 1), format!("{} {pattern}", text(&rhs[0])), vec![Arg::Expr(displacement)])
            }
            Rule::p_LD_reg_val => {
                let register = text(&rhs[1]);
                let wide = matches!(register.as_str(), "BC" | "DE" | "HL" | "SP" | "IX" | "IY");
                self.ins(line(rhs, 1), format!("LD {register},{}", if wide { "NN" } else { "N" }), vec![e(&rhs[3])])
            }
            Rule::p_LD_regI_val => {
                let (pattern, displacement) = indexed(&rhs[1]);
                self.ins(line(rhs, 1), format!("LD {pattern},N"), vec![Arg::Expr(displacement), e(&rhs[3])])
            }
            Rule::p_JP_hl => {
                let first = text(&rhs[1]);
                let target = if first == "(HL)" { first } else { format!("({})", text(&rhs[2])) };
                self.ins(line(rhs, 1), format!("JP {target}"), vec![])
            }
            Rule::p_SBCADD => self.ins(line(rhs, 1), format!("{} {},{}", text(&rhs[0]), text(&rhs[1]), text(&rhs[3])), vec![]),
            Rule::p_arith_A_expr => self.ins(line(rhs, 1), format!("{} A,N", text(&rhs[0])), vec![e(&rhs[3])]),
            Rule::p_arith_A_regI => {
                let (pattern, displacement) = indexed(&rhs[3]);
                self.ins(line(rhs, 1), format!("{} A,{pattern}", text(&rhs[0])), vec![Arg::Expr(displacement)])
            }
            Rule::p_bitwiseop_reg | Rule::p_ROTATE => {
                let (name, lineno) = named(&rhs[0]);
                self.ins(lineno, format!("{name} {}", text(&rhs[1])), vec![])
            }
            Rule::p_bitwiseop_regI | Rule::p_ROTATE_ix => {
                let (name, lineno) = named(&rhs[0]);
                let (pattern, displacement) = indexed(&rhs[1]);
                self.ins(lineno, format!("{name} {pattern}"), vec![Arg::Expr(displacement)])
            }
            Rule::p_bitwise_expr => {
                let (name, lineno) = named(&rhs[0]);
                self.ins(lineno, format!("{name} N"), vec![e(&rhs[1])])
            }
            Rule::p_bitwise | Rule::p_rotation => Value::Named(text(&rhs[0]), line(rhs, 1)),
            Rule::p_LD_addr_reg => self.ins(line(rhs, 1), format!("LD (NN),{}", text(&rhs[3])), vec![e(&rhs[1])]),
            Rule::p_LD_reg_addr => self.ins(line(rhs, 1), format!("LD {},(NN)", text(&rhs[1])), vec![e(&rhs[3])]),
            Rule::p_BIT | Rule::p_BIT_ix => {
                let Some(bit) = self.eval(&expr_of(&rhs[1])) else { return Ok(Value::Asm(None)) };
                if !(0..=7).contains(&bit) {
                    self.sess.error(line(rhs, 3), &format!("Invalid bit position {bit}. Must be in [0..7]"), None);
                    return Ok(Value::Asm(None));
                }
                let operation = text(&rhs[0]);
                if let Value::Indexed(pattern, displacement) = &rhs[3].value {
                    let (pattern, displacement) = (pattern.clone(), displacement.clone());
                    self.ins(line(rhs, 3), format!("{operation} {bit},{pattern}"), vec![Arg::Expr(displacement)])
                } else {
                    self.ins(line(rhs, 3), format!("{operation} {bit},{}", text(&rhs[3])), vec![])
                }
            }
            Rule::p_bitop | Rule::p_reg_inc | Rule::p_reg8 | Rule::p_regBCDE | Rule::p_reg8i | Rule::p_reg16 | Rule::p_reg16i | Rule::p_jpflags_other | Rule::p_jr_flags => {
                std::mem::take(&mut rhs[0].value)
            }
            Rule::p_jp => self.ins(line(rhs, 1), format!("{} {},NN", text(&rhs[0]), text(&rhs[1])), vec![e(&rhs[3])]),
            Rule::p_ret => self.ins(line(rhs, 1), format!("RET {}", text(&rhs[1])), vec![]),
            Rule::p_jr => {
                // A relative jump stores the distance from the next instruction.
                let next = expr::int(self.memory.org() + 2, line(rhs, 1));
                let distance = expr::op("-", line(rhs, 3), vec![expr_of(&rhs[3]), next]);
                self.ins(line(rhs, 1), format!("JR {},N", text(&rhs[1])), vec![Arg::Expr(distance)])
            }
            Rule::p_jrjp => {
                let name = text(&rhs[0]);
                if matches!(name.as_str(), "JR" | "DJNZ") {
                    let next = expr::int(self.memory.org() + 2, line(rhs, 1));
                    let distance = expr::op("-", line(rhs, 1), vec![expr_of(&rhs[1]), next]);
                    self.ins(line(rhs, 1), format!("{name} N"), vec![Arg::Expr(distance)])
                } else {
                    self.ins(line(rhs, 1), format!("{name} NN"), vec![e(&rhs[1])])
                }
            }
            Rule::p_rst => match self.eval(&expr_of(&rhs[1])) {
                Some(value) if [0, 8, 16, 24, 32, 40, 48, 56].contains(&value) => self.ins(line(rhs, 1), format!("RST {value:X}H"), vec![]),
                Some(value) => {
                    self.sess.error(line(rhs, 1), &format!("Invalid RST number {value}"), None);
                    Value::Asm(None)
                }
                None => Value::Asm(None),
            },
            Rule::p_im => match self.eval(&expr_of(&rhs[1])) {
                Some(value) if (0..=2).contains(&value) => self.ins(line(rhs, 1), format!("IM {value}"), vec![]),
                Some(value) => {
                    self.sess.error(line(rhs, 1), &format!("Invalid IM number {value}"), None);
                    Value::Asm(None)
                }
                None => Value::Asm(None),
            },
            Rule::p_in => self.ins(line(rhs, 1), format!("IN {},(C)", text(&rhs[1])), vec![]),
            Rule::p_out => self.ins(line(rhs, 1), format!("OUT (C),{}", text(&rhs[5])), vec![]),
            Rule::p_in_expr => self.ins(line(rhs, 1), "IN A,(N)".to_string(), vec![e(&rhs[3])]),
            Rule::p_out_expr => self.ins(line(rhs, 1), "OUT (N),A".to_string(), vec![e(&rhs[1])]),
            Rule::p_single | Rule::p_simple_instruction => self.ins(line(rhs, 1), text(&rhs[0]), vec![]),
            Rule::p_expr_div_expr => Value::Expr(expr::op(expr::operator(&text(&rhs[1])), line(rhs, 2), vec![expr_of(&rhs[0]), expr_of(&rhs[2])])),
            Rule::p_expr_lprp | Rule::p_mem_indir | Rule::p_expr_paren => std::mem::take(&mut rhs[1].value),
            Rule::p_expr_uminus => Value::Expr(expr::op(expr::operator(&text(&rhs[0])), line(rhs, 1), vec![expr_of(&rhs[1])])),
            Rule::p_expr_int => {
                let Value::Int(value) = rhs[0].value else { unreachable!() };
                Value::Expr(expr::int(value, line(rhs, 1)))
            }
            Rule::p_expr_label => {
                let label = self.memory.get_label(self.sess, &text(&rhs[0]), line(rhs, 1));
                Value::Expr(expr::label(label, line(rhs, 1)))
            }
            Rule::p_expr_addr => Value::Expr(expr::int(self.memory.org(), line(rhs, 1))),
            Rule::p_preproc_line_init => {
                self.inits.push((text(&rhs[1]).trim_matches('"').to_string(), line(rhs, 2)));
                Value::None
            }
            Rule::p_codebank => {
                if let Some(bank) = self.eval(&expr_of(&rhs[1])) {
                    self.memory.set_segment(self.sess, bank, self.options.code_window, line(rhs, 1));
                }
                Value::Asm(None)
            }
            Rule::p_mul_d_e => self.ins(line(rhs, 1), "MUL D,E".to_string(), vec![]),
            Rule::p_add_reg16_a => self.ins(line(rhs, 1), format!("ADD {},A", text(&rhs[1])), vec![]),
            Rule::p_JP_c => self.ins(line(rhs, 1), "JP (C)".to_string(), vec![]),
            Rule::p_bxxxx_de_b => self.ins(line(rhs, 1), format!("{} DE,B", text(&rhs[0])), vec![]),
            Rule::p_add_reg_NN => self.ins(line(rhs, 1), format!("ADD {},NN", text(&rhs[1])), vec![e(&rhs[3])]),
            Rule::p_test_nn => self.ins(line(rhs, 1), "TEST N".to_string(), vec![e(&rhs[1])]),
            Rule::p_nextreg_expr => self.ins(line(rhs, 1), "NEXTREG N,N".to_string(), vec![e(&rhs[1]), e(&rhs[3])]),
            Rule::p_nextreg_a => self.ins(line(rhs, 1), "NEXTREG N,A".to_string(), vec![e(&rhs[1])]),
            Rule::p_push_imm => {
                // PUSH nn stores its operand high byte first.
                let l = line(rhs, 1);
                let value = expr_of(&rhs[1]);
                let high = expr::op("<<", l, vec![expr::op("&", l, vec![value.clone(), expr::int(0xFF, l)]), expr::int(8, l)]);
                let low = expr::op("&", l, vec![expr::op(">>", l, vec![value, expr::int(8, l)]), expr::int(0xFF, l)]);
                self.ins(l, "PUSH NN".to_string(), vec![Arg::Expr(expr::op("|", l, vec![high, low]))])
            }
        };
        // Where it begins, its mnemonic, is the first symbol of the rule
        // that makes it; the rules above only hand it on.
        if let Value::Asm(Some(asm)) = &mut value {
            if self.debug && asm.source.is_none() {
                asm.source = Some(source);
            }
        }
        Ok(value)
    }
}

/// What an indexed displacement starts with, as the Python code works it
/// out: the operator above the leftmost leaf, or that leaf if the operator
/// has two operands.
fn leading_token(root: &Expr) -> String {
    let mut parent: Option<&Expr> = None;
    let mut node = root;
    while let Some(first) = node.children.first() {
        parent = Some(node);
        node = first;
    }
    let show = |e: &Expr| match &e.item {
        expr::Item::Int(v) => v.to_string(),
        expr::Item::Op(o) => o.to_string(),
        expr::Item::Label(l) => l.borrow().name(),
    };
    match parent {
        None => "<nothing>".to_string(),
        Some(p) if p.children.len() == 2 => show(node),
        Some(p) => show(p),
    }
}
