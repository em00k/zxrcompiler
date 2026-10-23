//! `peephole/pattern.py`: the REPLACE side of a rule. A line such as
//! `push $1` compiles to a regular expression; a block of them matches a run
//! of instructions, with a variable that appears on several lines bound to
//! the same text on each.

use std::collections::{BTreeMap, BTreeSet};
use zxr_re::{Flags, Regex};

/// `RE_SVAR.match`: `\$(?:\$|[0-9]+)` at the start of the text. Returns the
/// length of the match.
pub fn svar_prefix(text: &str) -> Option<usize> {
    let rest = text.strip_prefix('$')?;
    if rest.starts_with('$') {
        return Some(2);
    }
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    (digits > 0).then_some(1 + digits)
}

/// `RE_SVAR.split(token)`, without the empty pieces.
fn split_svar(token: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut plain = String::new();
    let mut i = 0;
    while i < token.len() {
        if let Some(n) = svar_prefix(&token[i..]) {
            if !plain.is_empty() {
                out.push(std::mem::take(&mut plain));
            }
            out.push(token[i..i + n].to_string());
            i += n;
        } else {
            let c = token[i..].chars().next().unwrap();
            plain.push(c);
            i += c.len_utf8();
        }
    }
    if !plain.is_empty() {
        out.push(plain);
    }
    out
}

fn is_space(c: char) -> bool {
    c.is_whitespace() || ('\x1c'..='\x1f').contains(&c)
}

/// `RE_PARSE.split(line)`: the line cut at `(\s+|"(?:[^"]|"")*")`, the
/// separators kept. Empty pieces are left out, which is all the callers
/// want of them.
fn split_parse(line: &str) -> Vec<String> {
    let chars: Vec<char> = line.chars().collect();
    let mut out = Vec::new();
    let mut plain = String::new();
    let mut i = 0;
    let flush = |plain: &mut String, out: &mut Vec<String>| {
        if !plain.is_empty() {
            out.push(std::mem::take(plain));
        }
    };
    while i < chars.len() {
        if is_space(chars[i]) {
            flush(&mut plain, &mut out);
            let from = i;
            while i < chars.len() && is_space(chars[i]) {
                i += 1;
            }
            out.push(chars[from..i].iter().collect());
            continue;
        }
        if chars[i] == '"' {
            // "(?:[^"]|"")*" is greedy: a doubled quote stays inside, and
            // the string ends at a quote that is not followed by another,
            // or, backtracking, at the last quote that closes anything.
            if let Some(end) = quoted_end(&chars, i) {
                flush(&mut plain, &mut out);
                out.push(chars[i..end].iter().collect());
                i = end;
                continue;
            }
        }
        plain.push(chars[i]);
        i += 1;
    }
    flush(&mut plain, &mut out);
    out
}

/// Where the quoted string starting at `start` ends (one past its closing
/// quote), as the regular expression `"(?:[^"]|"")*"` finds it.
fn quoted_end(chars: &[char], start: usize) -> Option<usize> {
    // Greedy with backtracking: consume as much as possible, remembering
    // each place where a closing quote could stand.
    let mut i = start + 1;
    let mut last_close = None;
    loop {
        match chars.get(i) {
            None => return last_close,
            Some('"') => {
                // Either the close, or the first half of a doubled quote.
                last_close = Some(i + 1);
                if chars.get(i + 1) == Some(&'"') {
                    i += 2;
                } else {
                    return last_close;
                }
            }
            Some(_) => i += 1,
        }
    }
}

/// Returns a sanitized pattern version of a string to be later compiled
/// into a reg exp
fn sanitize(pattern: &str) -> String {
    let meta = r".^$*+?{}[]\|()";
    pattern.chars().map(|x| if meta.contains(x) { format!("\\{x}") } else { x.to_string() }).collect()
}

/// Defines a pattern for a line, like 'push $1' being $1 a pattern variable
#[derive(Clone, Debug)]
pub struct BasicLinePattern {
    pub line: String,
    /// The pieces of the line: text, a blank, or a variable.
    pub output: Vec<String>,
    pub re_pattern: String,
    /// The variables, as `$1`, in order of first appearance.
    pub vars: Vec<String>,
}

impl BasicLinePattern {
    pub fn new(line: &str) -> BasicLinePattern {
        let joined: String = split_parse(line).iter().map(|x| if crate::asm::py_strip(x).is_empty() { " " } else { crate::asm::py_strip(x) }).collect();
        let line = crate::asm::py_strip(&joined).to_string();
        let mut vars: Vec<String> = Vec::new();
        let mut re_pattern = String::new();
        let mut output = Vec::new();

        for token in split_parse(&line) {
            if token == " " {
                re_pattern.push_str(r"\s+");
                output.push(" ".to_string());
                continue;
            }

            for tok in split_svar(&token) {
                if tok == "$$" {
                    re_pattern.push_str(r"\$");
                    output.push("$".to_string());
                } else if svar_prefix(&tok).is_some() {
                    output.push(tok.clone());
                    let mvar = format!("_{}", &tok[1..]);
                    match vars.iter().position(|v| *v == mvar) {
                        None => {
                            re_pattern.push_str(&format!("(?P<{mvar}>.*)"));
                            vars.push(mvar);
                        }
                        Some(index) => re_pattern.push_str(&format!("\\{}", index + 1)),
                    }
                } else {
                    re_pattern.push_str(&sanitize(&tok));
                    output.push(tok);
                }
            }
        }

        BasicLinePattern { line, output, re_pattern, vars: vars.iter().map(|x| x.replace('_', "$")).collect() }
    }
}

/// Defines a pattern to match against a source assembler. Given an
/// assembler instruction with substitution variables ($1, $2, ...) creates
/// an instance that matches against a list of real assembler instructions.
/// e.g. `push $1` matches against `push af` and bounds $1 to 'af'. Note that
/// $$ matches against the $ sign.
/// One piece of a line pattern's regular expression.
#[derive(Clone, Debug, PartialEq)]
enum Piece {
    /// Fixed text.
    Text(String),
    /// `\s+`
    Space,
    /// `(?P<_n>.*)`: the first appearance of a variable.
    Var(usize),
    /// `\n`: a later appearance, which must repeat the first.
    Back(usize),
}

#[derive(Clone, Debug)]
pub struct LinePattern {
    pub base: BasicLinePattern,
    /// The expression itself. Matching goes through `pieces`, which is the
    /// same expression taken apart; a test holds the two to each other.
    #[cfg_attr(not(test), allow(dead_code))]
    re: Regex,
    pieces: Vec<Piece>,
    /// The text every line this pattern matches starts with: a quick test
    /// before anything else is tried.
    prefix: String,
}

/// Matches `pieces` against `line` from byte `pos`, as `re.match` would
/// match the expression they came from: `.*` and `\s+` take as much as they
/// can and give back one character at a time, and the match need not reach
/// the end of the line.
fn match_pieces(pieces: &[Piece], line: &str, pos: usize, caps: &mut [(usize, usize)]) -> bool {
    let Some((piece, rest)) = pieces.split_first() else { return true };
    match piece {
        Piece::Text(text) => line[pos..].starts_with(text.as_str()) && match_pieces(rest, line, pos + text.len(), caps),
        Piece::Back(index) => {
            let (a, b) = caps[*index];
            line[pos..].starts_with(&line[a..b]) && match_pieces(rest, line, pos + (b - a), caps)
        }
        Piece::Space => {
            let mut end = pos;
            for c in line[pos..].chars() {
                if !is_space(c) {
                    break;
                }
                end += c.len_utf8();
            }
            // Longest run first, then one character less at a time.
            while end > pos {
                if match_pieces(rest, line, end, caps) {
                    return true;
                }
                end -= 1;
                while !line.is_char_boundary(end) {
                    end -= 1;
                }
            }
            false
        }
        Piece::Var(index) => {
            // `.` does not match a line break.
            let mut end = line[pos..].find('\n').map_or(line.len(), |n| pos + n);
            loop {
                caps[*index] = (pos, end);
                if match_pieces(rest, line, end, caps) {
                    return true;
                }
                if end == pos {
                    return false;
                }
                end -= 1;
                while !line.is_char_boundary(end) {
                    end -= 1;
                }
            }
        }
    }
}

impl LinePattern {
    pub fn new(line: &str) -> Result<LinePattern, String> {
        let base = BasicLinePattern::new(line);
        let re = Regex::new(&base.re_pattern, Flags::NONE).map_err(|e| e.to_string())?;
        let prefix = base.output.iter().take_while(|tok| *tok != " " && !tok.starts_with('$')).cloned().collect();
        // The same walk over the pattern that built the expression.
        let mut pieces: Vec<Piece> = Vec::new();
        let mut seen: Vec<&String> = Vec::new();
        for token in &base.output {
            let piece = if token == " " {
                Piece::Space
            } else if token.len() > 1 && svar_prefix(token) == Some(token.len()) {
                match seen.iter().position(|v| *v == token) {
                    Some(index) => Piece::Back(index),
                    None => {
                        seen.push(token);
                        Piece::Var(seen.len() - 1)
                    }
                }
            } else {
                Piece::Text(token.clone())
            };
            match (pieces.last_mut(), &piece) {
                (Some(Piece::Text(last)), Piece::Text(more)) => last.push_str(more),
                _ => pieces.push(piece),
            }
        }
        Ok(LinePattern { base, re, pieces, prefix })
    }

    /// The text of each variable if the pattern matches `line`, in the
    /// order of `base.vars`. Matched from the start of the line; the match
    /// need not reach its end.
    fn captures<'l>(&self, line: &'l str) -> Option<Vec<&'l str>> {
        if !line.starts_with(&self.prefix) {
            return None;
        }
        let mut caps = vec![(0usize, 0usize); self.base.vars.len()];
        match_pieces(&self.pieces, line, 0, &mut caps).then(|| caps.iter().map(|&(a, b)| &line[a..b]).collect())
    }

    /// Returns whether the pattern matched. If it did, `vars` is updated
    /// with the unified variables. A variable already bound is compared
    /// afterwards, with no second attempt.
    pub fn matches(&self, line: &str, vars: &mut BTreeMap<String, String>) -> bool {
        let Some(values) = self.captures(line) else { return false };
        if self.base.vars.iter().zip(&values).any(|(name, value)| vars.get(name).is_some_and(|bound| bound != value)) {
            return false;
        }
        for (name, value) in self.base.vars.iter().zip(values) {
            vars.insert(name.clone(), value.to_string());
        }
        true
    }

    /// The same captures through the regular expression itself.
    #[cfg(test)]
    fn captures_by_regex(&self, line: &str) -> Option<Vec<String>> {
        let chars: Vec<char> = line.chars().collect();
        let found = self.re.match_at(&chars, 0)?;
        Some((0..self.base.vars.len()).map(|i| found.group(&chars, i + 1).unwrap_or_default()).collect())
    }
}

/// Given a list asm instructions, tries to match them
#[derive(Clone, Debug)]
pub struct BlockPattern {
    pub patterns: Vec<LinePattern>,
    pub lines: Vec<String>,
    pub vars: BTreeSet<String>,
}

impl BlockPattern {
    pub fn new(lines: &[String]) -> Result<BlockPattern, String> {
        let mut patterns = Vec::new();
        for line in lines.iter().map(|x| crate::asm::py_strip(x)).filter(|x| !x.is_empty()) {
            patterns.push(LinePattern::new(line)?);
        }
        let lines = patterns.iter().map(|p| p.base.line.clone()).collect();
        let vars = patterns.iter().flat_map(|p| p.base.vars.iter().cloned()).collect();
        Ok(BlockPattern { patterns, lines, vars })
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    /// The fixed text every match's first line starts with; empty if the
    /// pattern has no lines or its first line starts with a variable.
    pub fn first_prefix(&self) -> &str {
        self.patterns.first().map_or("", |p| p.prefix.as_str())
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Given a list of instructions and a starting point, returns whether
    /// this pattern matches or not from such point onwards. E.g. given the
    /// pattern
    ///   push $1
    ///   pop $1
    /// and the list
    ///   ld a, 5
    ///   push af
    ///   pop af
    /// this pattern will match at position 1. `chars` holds the lines'
    /// characters once they have been needed, so that the many patterns
    /// tried at one position share them.
    pub fn matches(&self, instructions: &[String], start: usize, _chars: &mut LineChars) -> Option<BTreeMap<String, String>> {
        if self.len() > instructions.len().saturating_sub(start) {
            return None;
        }

        // Every line has to start with its pattern's fixed text: a test
        // that costs nothing next to a regular expression, made for all the
        // lines before any expression is tried.
        if self.patterns.iter().enumerate().any(|(offset, patt)| !instructions[start + offset].starts_with(&patt.prefix)) {
            return None;
        }

        // Nothing is copied until every line has matched: most attempts
        // fail, and they are made millions of times.
        let mut bound: Vec<(&String, &str)> = Vec::new();
        for (offset, patt) in self.patterns.iter().enumerate() {
            let values = patt.captures(&instructions[start + offset])?;
            // A variable already bound must have the same text here; one
            // that appears first on this line is bound once the line is done.
            if patt.base.vars.iter().zip(&values).any(|(name, value)| bound.iter().any(|(n, v)| *n == name && v != value)) {
                return None;
            }
            for (name, value) in patt.base.vars.iter().zip(values) {
                match bound.iter_mut().find(|(n, _)| *n == name) {
                    Some(entry) => entry.1 = value,
                    None => bound.push((name, value)),
                }
            }
        }

        Some(bound.into_iter().map(|(name, value)| (name.clone(), value.to_string())).collect())
    }
}

/// Kept for the callers' sake: matching no longer needs the lines' characters.
#[derive(Default)]
pub struct LineChars;

#[cfg(test)]
mod tests {
    use super::*;

    /// The direct matcher must capture exactly what the regular expression
    /// captures, for every line pattern of every rule, on real assembly and
    /// on lines made to provoke backtracking.
    #[test]
    fn pieces_match_as_the_expression_does() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden/expected/arch");
        let mut lines: Vec<String> = Vec::new();
        for arch in ["zxnext", "zx48k"] {
            let mut files: Vec<_> = std::fs::read_dir(root.join(arch)).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "asm")).collect();
            files.sort();
            for file in files.iter().step_by(9) {
                lines.extend(String::from_utf8_lossy(&std::fs::read(file).unwrap()).lines().map(|l| l.trim().to_string()));
            }
        }
        lines.sort();
        lines.dedup();
        for odd in [
            "", "ld", "ld ", "ld a", "ld a,", "ld a, ", "ld  a,  b", "ld\ta,\tb", "ld a, b, c", "ld a, (ix+5), x", "push af, af", "pop  af ", "ld (hl), a ; note",
            "ld a, a", "ld hl, (hl), (hl)", "jp nz, .LABEL.__LABEL1", "call .core.__ADDF", "ld h, (hl)", "add hl, hl", "ex de, hl", "ld a, 'x, y'", "ld é, ü", "ld a, \u{a0}b",
            "inc hl hl", "ld a, 1\nld b, 2", "xor a  ", "ld (ix+4), l", "ld bc, bc", "sbc a, a", "or a", "and a, a",
        ] {
            lines.push(odd.to_string());
        }
        let mut checked = 0;
        for arch in ["zx48k", "zxnext"] {
            for rule in &crate::engine::Engine::for_arch(arch).patterns {
                for patt in &rule.patt.patterns {
                    for line in &lines {
                        let direct = patt.captures(line).map(|v| v.into_iter().map(String::from).collect::<Vec<_>>());
                        let by_regex = if line.starts_with(&patt.prefix) { patt.captures_by_regex(line) } else { None };
                        assert_eq!(direct, by_regex, "pattern {:?} ({}) on line {:?}", patt.base.line, patt.base.re_pattern, line);
                        checked += 1;
                    }
                }
            }
        }
        assert!(checked > 100_000, "only {checked} comparisons made");
    }
}
