//! A backtracking regular expression matcher with the semantics of Python's
//! `re` module, for the subset of syntax the compiler's lexers and peephole
//! patterns use.
//!
//! It exists because the Python compiler's behaviour is defined by `re`:
//! alternatives are tried in order and the first that leads to a match wins
//! (not the longest), and peephole patterns use backreferences. Matching the
//! same way is what keeps token boundaries identical.
//!
//! Input is a slice of `char`, so positions are code-point indices, as in
//! Python.

use std::fmt;

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Flags {
    /// `re.VERBOSE`: unescaped whitespace is ignored and `#` starts a comment.
    pub verbose: bool,
    /// `re.IGNORECASE`.
    pub ignore_case: bool,
    /// `re.DOTALL`: `.` matches a newline too.
    pub dot_all: bool,
    /// `re.MULTILINE`: `^` and `$` match at line boundaries.
    pub multi_line: bool,
}

impl Flags {
    pub const NONE: Flags = Flags { verbose: false, ignore_case: false, dot_all: false, multi_line: false };
    pub const VERBOSE: Flags = Flags { verbose: true, ignore_case: false, dot_all: false, multi_line: false };
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid regular expression: {}", self.0)
    }
}

impl std::error::Error for Error {}

#[derive(Debug, Clone, PartialEq)]
enum ClassItem {
    Char(char),
    Range(char, char),
    Digit(bool),
    Word(bool),
    Space(bool),
}

#[derive(Debug, Clone, PartialEq)]
struct Class {
    negated: bool,
    items: Vec<ClassItem>,
}

#[derive(Debug, Clone, PartialEq)]
enum Node {
    Empty,
    Char(char),
    Any,
    Class(Class),
    LineStart,
    LineEnd,
    TextStart,
    TextEnd,
    WordBoundary(bool),
    Group(Box<Node>, usize),
    Concat(Vec<Node>),
    Alt(Vec<Node>),
    Repeat { node: Box<Node>, min: usize, max: usize, greedy: bool },
    Backref(usize),
    Look { node: Box<Node>, ahead: bool, negated: bool, width: usize },
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn is_space(c: char) -> bool {
    c.is_whitespace() || ('\x1c'..='\x1f').contains(&c)
}

fn is_digit(c: char) -> bool {
    c.is_ascii_digit() || (!c.is_ascii() && c.is_numeric() && c.to_digit(10).is_some())
}

struct Parser<'a> {
    src: &'a [char],
    pos: usize,
    flags: Flags,
    groups: usize,
    names: Vec<(String, usize)>,
}

impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.src.get(self.pos).copied()
    }

    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(c) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn err<T>(&self, msg: &str) -> Result<T, Error> {
        Err(Error(format!("{msg} at position {}", self.pos)))
    }

    fn skip_verbose(&mut self) {
        if !self.flags.verbose {
            return;
        }
        while let Some(c) = self.peek() {
            if c.is_whitespace() {
                self.pos += 1;
            } else if c == '#' {
                while self.peek().is_some_and(|c| c != '\n') {
                    self.pos += 1;
                }
            } else {
                break;
            }
        }
    }

    fn alternation(&mut self) -> Result<Node, Error> {
        let mut branches = vec![self.sequence()?];
        while self.eat('|') {
            branches.push(self.sequence()?);
        }
        Ok(if branches.len() == 1 { branches.pop().unwrap() } else { Node::Alt(branches) })
    }

    fn sequence(&mut self) -> Result<Node, Error> {
        let mut items = Vec::new();
        loop {
            self.skip_verbose();
            match self.peek() {
                None | Some('|') | Some(')') => break,
                _ => {}
            }
            let atom = self.atom()?;
            self.skip_verbose();
            items.push(self.quantified(atom)?);
        }
        Ok(match items.len() {
            0 => Node::Empty,
            1 => items.pop().unwrap(),
            _ => Node::Concat(items),
        })
    }

    fn number(&mut self) -> Option<usize> {
        let start = self.pos;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.pos += 1;
        }
        (self.pos > start).then(|| self.src[start..self.pos].iter().collect::<String>().parse().unwrap())
    }

    fn quantified(&mut self, atom: Node) -> Result<Node, Error> {
        let (min, max) = match self.peek() {
            Some('*') => (0, usize::MAX),
            Some('+') => (1, usize::MAX),
            Some('?') => (0, 1),
            Some('{') => {
                // `{` only starts a quantifier when it is well formed.
                let save = self.pos;
                self.pos += 1;
                let lo = self.number();
                let (min, max) = if self.eat(',') {
                    let hi = self.number();
                    (lo.unwrap_or(0), hi.unwrap_or(usize::MAX))
                } else {
                    (lo.unwrap_or(usize::MAX), lo.unwrap_or(0))
                };
                if self.peek() != Some('}') || (lo.is_none() && min == usize::MAX) {
                    self.pos = save;
                    return Ok(atom);
                }
                self.pos += 1;
                let greedy = !self.eat('?');
                return Ok(Node::Repeat { node: Box::new(atom), min, max, greedy });
            }
            _ => return Ok(atom),
        };
        self.pos += 1;
        let greedy = !self.eat('?');
        Ok(Node::Repeat { node: Box::new(atom), min, max, greedy })
    }

    fn atom(&mut self) -> Result<Node, Error> {
        let c = self.peek().unwrap();
        self.pos += 1;
        match c {
            '(' => self.group(),
            '[' => self.class().map(Node::Class),
            '.' => Ok(Node::Any),
            '^' => Ok(Node::LineStart),
            '$' => Ok(Node::LineEnd),
            '\\' => self.escape(),
            '*' | '+' | '?' => self.err("nothing to repeat"),
            c => Ok(Node::Char(c)),
        }
    }

    fn group(&mut self) -> Result<Node, Error> {
        let mut capture = None;
        let mut look = None;
        if self.eat('?') {
            match self.peek() {
                Some(':') => self.pos += 1,
                Some('=') => {
                    self.pos += 1;
                    look = Some((true, false));
                }
                Some('!') => {
                    self.pos += 1;
                    look = Some((true, true));
                }
                Some('<') => {
                    self.pos += 1;
                    let negated = match self.peek() {
                        Some('=') => false,
                        Some('!') => true,
                        _ => return self.err("unknown group extension"),
                    };
                    self.pos += 1;
                    look = Some((false, negated));
                }
                Some('#') => {
                    while self.peek().is_some_and(|c| c != ')') {
                        self.pos += 1;
                    }
                    self.pos += 1;
                    return Ok(Node::Empty);
                }
                Some('P') => {
                    self.pos += 1;
                    let backref = self.eat('=');
                    if !backref && !self.eat('<') {
                        return self.err("unknown group extension");
                    }
                    let start = self.pos;
                    let close = if backref { ')' } else { '>' };
                    while self.peek().is_some_and(|c| c != close) {
                        self.pos += 1;
                    }
                    let name: String = self.src[start..self.pos].iter().collect();
                    self.pos += 1;
                    if backref {
                        return match self.names.iter().find(|(n, _)| *n == name) {
                            Some((_, index)) => Ok(Node::Backref(*index)),
                            None => self.err("unknown group name"),
                        };
                    }
                    self.groups += 1;
                    self.names.push((name, self.groups));
                    capture = Some(self.groups);
                }
                _ => return self.err("unknown group extension"),
            }
        } else {
            self.groups += 1;
            capture = Some(self.groups);
        }
        let inner = self.alternation()?;
        if !self.eat(')') {
            return self.err("missing )");
        }
        if let Some((ahead, negated)) = look {
            let width = if ahead { 0 } else { fixed_width(&inner).ok_or(Error("look-behind requires fixed-width pattern".into()))? };
            return Ok(Node::Look { node: Box::new(inner), ahead, negated, width });
        }
        Ok(match capture {
            Some(index) => Node::Group(Box::new(inner), index),
            None => inner,
        })
    }

    fn hex(&mut self, digits: usize) -> Result<char, Error> {
        let text: String = self.src.get(self.pos..self.pos + digits).unwrap_or(&[]).iter().collect();
        self.pos += digits;
        u32::from_str_radix(&text, 16).ok().and_then(char::from_u32).ok_or(Error("bad hex escape".into()))
    }

    /// A character escape valid both inside and outside a class.
    fn char_escape(&mut self, c: char) -> Result<Option<char>, Error> {
        Ok(Some(match c {
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            'f' => '\x0c',
            'v' => '\x0b',
            'a' => '\x07',
            'x' => self.hex(2)?,
            'u' => self.hex(4)?,
            _ => return Ok(None),
        }))
    }

    fn escape(&mut self) -> Result<Node, Error> {
        let Some(c) = self.peek() else { return self.err("bad escape (end of pattern)") };
        self.pos += 1;
        if let Some(ch) = self.char_escape(c)? {
            return Ok(Node::Char(ch));
        }
        Ok(match c {
            'd' => Node::Class(Class { negated: false, items: vec![ClassItem::Digit(true)] }),
            'D' => Node::Class(Class { negated: false, items: vec![ClassItem::Digit(false)] }),
            'w' => Node::Class(Class { negated: false, items: vec![ClassItem::Word(true)] }),
            'W' => Node::Class(Class { negated: false, items: vec![ClassItem::Word(false)] }),
            's' => Node::Class(Class { negated: false, items: vec![ClassItem::Space(true)] }),
            'S' => Node::Class(Class { negated: false, items: vec![ClassItem::Space(false)] }),
            'b' => Node::WordBoundary(true),
            'B' => Node::WordBoundary(false),
            'A' => Node::TextStart,
            'Z' => Node::TextEnd,
            '0' => Node::Char('\0'),
            '1'..='9' => {
                self.pos -= 1;
                Node::Backref(self.number().unwrap())
            }
            c if c.is_ascii_alphabetic() => return self.err("bad escape"),
            c => Node::Char(c),
        })
    }

    fn class(&mut self) -> Result<Class, Error> {
        let negated = self.eat('^');
        let mut items = Vec::new();
        let mut first = true;
        loop {
            let Some(c) = self.peek() else { return self.err("unterminated character set") };
            self.pos += 1;
            if c == ']' && !first {
                break;
            }
            first = false;
            let lo = if c == '\\' {
                let Some(e) = self.peek() else { return self.err("bad escape (end of pattern)") };
                self.pos += 1;
                match e {
                    'd' | 'D' => {
                        items.push(ClassItem::Digit(e == 'd'));
                        continue;
                    }
                    'w' | 'W' => {
                        items.push(ClassItem::Word(e == 'w'));
                        continue;
                    }
                    's' | 'S' => {
                        items.push(ClassItem::Space(e == 's'));
                        continue;
                    }
                    'b' => '\x08',
                    e => self.char_escape(e)?.unwrap_or(e),
                }
            } else {
                c
            };
            // A range, unless the `-` is the last character of the set.
            if self.peek() == Some('-') && self.src.get(self.pos + 1).is_some_and(|&n| n != ']') {
                self.pos += 1;
                let mut hi = self.peek().unwrap();
                self.pos += 1;
                if hi == '\\' {
                    let e = self.peek().unwrap();
                    self.pos += 1;
                    hi = self.char_escape(e)?.unwrap_or(e);
                }
                if hi < lo {
                    return self.err("bad character range");
                }
                items.push(ClassItem::Range(lo, hi));
            } else {
                items.push(ClassItem::Char(lo));
            }
        }
        Ok(Class { negated, items })
    }
}

fn fixed_width(node: &Node) -> Option<usize> {
    Some(match node {
        Node::Empty | Node::LineStart | Node::LineEnd | Node::TextStart | Node::TextEnd | Node::WordBoundary(_) | Node::Look { .. } => 0,
        Node::Char(_) | Node::Any | Node::Class(_) => 1,
        Node::Group(inner, _) => fixed_width(inner)?,
        Node::Concat(items) => items.iter().map(fixed_width).sum::<Option<usize>>()?,
        Node::Alt(branches) => {
            let widths: Vec<usize> = branches.iter().map(fixed_width).collect::<Option<_>>()?;
            if widths.windows(2).any(|w| w[0] != w[1]) {
                return None;
            }
            widths[0]
        }
        Node::Repeat { node, min, max, .. } if min == max => fixed_width(node)? * min,
        Node::Repeat { .. } | Node::Backref(_) => return None,
    })
}

/// Capture positions: index 0 is the whole match.
pub type Captures = Vec<Option<(usize, usize)>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    pub start: usize,
    pub end: usize,
    pub groups: Captures,
}

impl Match {
    /// The text of a group, or `None` if it did not take part in the match.
    pub fn group(&self, input: &[char], index: usize) -> Option<String> {
        self.groups.get(index).copied().flatten().map(|(a, b)| input[a..b].iter().collect())
    }
}

#[derive(Debug, Clone)]
pub struct Regex {
    root: Node,
    groups: usize,
    names: Vec<(String, usize)>,
    flags: Flags,
    /// The characters a match can start with, when that is known and the
    /// pattern cannot match the empty string. ASCII only; `None` means "try".
    first: Option<[bool; 128]>,
}

type Cont<'k> = &'k mut dyn FnMut(usize, &mut Captures) -> bool;

impl Regex {
    pub fn new(pattern: &str, flags: Flags) -> Result<Regex, Error> {
        let src: Vec<char> = pattern.chars().collect();
        let mut parser = Parser { src: &src, pos: 0, flags, groups: 0, names: Vec::new() };
        let root = parser.alternation()?;
        if parser.pos != src.len() {
            return parser.err("unbalanced parenthesis");
        }
        let mut regex = Regex { root, groups: parser.groups, names: parser.names, flags, first: None };
        regex.first = regex.first_set();
        Ok(regex)
    }

    pub fn group_count(&self) -> usize {
        self.groups
    }

    pub fn group_index(&self, name: &str) -> Option<usize> {
        self.names.iter().find(|(n, _)| n == name).map(|(_, i)| *i)
    }

    /// Like `re.match`: a match starting exactly at `pos`.
    pub fn match_at(&self, input: &[char], pos: usize) -> Option<Match> {
        if let Some(first) = &self.first {
            match input.get(pos) {
                Some(&c) if (c as u32) < 128 && !first[c as usize] => return None,
                None => return None,
                _ => {}
            }
        }
        let mut caps: Captures = vec![None; self.groups + 1];
        let mut end = None;
        let mut result = None;
        self.node(&self.root, input, pos, &mut caps, &mut |p, c| {
            end = Some(p);
            result = Some(c.clone());
            true
        });
        let end = end?;
        let mut groups = result.unwrap();
        groups[0] = Some((pos, end));
        Some(Match { start: pos, end, groups })
    }

    /// Like `re.fullmatch`.
    pub fn full_match(&self, input: &[char]) -> Option<Match> {
        let mut caps: Captures = vec![None; self.groups + 1];
        let mut result = None;
        self.node(&self.root, input, 0, &mut caps, &mut |p, c| {
            if p != input.len() {
                return false;
            }
            result = Some(c.clone());
            true
        });
        let mut groups = result?;
        groups[0] = Some((0, input.len()));
        Some(Match { start: 0, end: input.len(), groups })
    }

    /// Like `re.search`: the leftmost match at or after `start`.
    pub fn search(&self, input: &[char], start: usize) -> Option<Match> {
        (start..=input.len()).find_map(|pos| self.match_at(input, pos))
    }

    /// Convenience for matching against a `&str` from its beginning.
    pub fn match_str(&self, text: &str) -> Option<Match> {
        let chars: Vec<char> = text.chars().collect();
        self.match_at(&chars, 0)
    }

    fn eq_char(&self, a: char, b: char) -> bool {
        a == b || (self.flags.ignore_case && a.to_lowercase().eq(b.to_lowercase()))
    }

    fn class_has(&self, class: &Class, c: char) -> bool {
        let test = |c: char| {
            class.items.iter().any(|item| match *item {
                ClassItem::Char(x) => x == c,
                ClassItem::Range(lo, hi) => lo <= c && c <= hi,
                ClassItem::Digit(yes) => is_digit(c) == yes,
                ClassItem::Word(yes) => is_word(c) == yes,
                ClassItem::Space(yes) => is_space(c) == yes,
            })
        };
        let mut hit = test(c);
        if !hit && self.flags.ignore_case {
            hit = c.to_lowercase().chain(c.to_uppercase()).any(test);
        }
        hit != class.negated
    }

    /// Whether a one-character node matches `c`; `None` for other nodes.
    fn single(&self, node: &Node, c: char) -> Option<bool> {
        Some(match node {
            Node::Char(x) => self.eq_char(*x, c),
            Node::Any => self.flags.dot_all || c != '\n',
            Node::Class(class) => self.class_has(class, c),
            _ => return None,
        })
    }

    fn node(&self, node: &Node, input: &[char], pos: usize, caps: &mut Captures, k: Cont) -> bool {
        match node {
            Node::Empty => k(pos, caps),
            Node::Char(_) | Node::Any | Node::Class(_) => match input.get(pos) {
                Some(&c) if self.single(node, c) == Some(true) => k(pos + 1, caps),
                _ => false,
            },
            Node::LineStart => {
                (pos == 0 || (self.flags.multi_line && input[pos - 1] == '\n')) && k(pos, caps)
            }
            Node::LineEnd => {
                let at_end = pos == input.len() || (pos + 1 == input.len() && input[pos] == '\n');
                (at_end || (self.flags.multi_line && input[pos] == '\n')) && k(pos, caps)
            }
            Node::TextStart => pos == 0 && k(pos, caps),
            Node::TextEnd => pos == input.len() && k(pos, caps),
            Node::WordBoundary(want) => {
                let before = pos > 0 && is_word(input[pos - 1]);
                let after = pos < input.len() && is_word(input[pos]);
                ((before != after) == *want) && k(pos, caps)
            }
            Node::Group(inner, index) => self.node(inner, input, pos, caps, &mut |p, c| {
                let old = c[*index];
                c[*index] = Some((pos, p));
                if k(p, c) {
                    return true;
                }
                c[*index] = old;
                false
            }),
            Node::Concat(items) => self.sequence(items, input, pos, caps, k),
            Node::Alt(branches) => branches.iter().any(|branch| self.node(branch, input, pos, caps, k)),
            Node::Repeat { node: inner, min, max, greedy } => {
                if matches!(**inner, Node::Char(_) | Node::Any | Node::Class(_)) {
                    // One character at a time: count, then back off, with no recursion.
                    let mut count = 0;
                    while count < *max && input.get(pos + count).is_some_and(|&c| self.single(inner, c) == Some(true)) {
                        count += 1;
                    }
                    if count < *min {
                        return false;
                    }
                    if *greedy {
                        (*min..=count).rev().any(|n| k(pos + n, caps))
                    } else {
                        (*min..=count).any(|n| k(pos + n, caps))
                    }
                } else {
                    self.repeat(inner, *min, *max, *greedy, 0, input, pos, caps, k)
                }
            }
            Node::Backref(index) => match caps.get(*index).copied().flatten() {
                Some((a, b)) => {
                    let len = b - a;
                    pos + len <= input.len() && (0..len).all(|i| self.eq_char(input[a + i], input[pos + i])) && k(pos + len, caps)
                }
                None => false,
            },
            Node::Look { node: inner, ahead, negated, width } => {
                let mut scratch = caps.clone();
                let matched = if *ahead {
                    self.node(inner, input, pos, &mut scratch, &mut |_, _| true)
                } else {
                    pos >= *width && self.node(inner, input, pos - width, &mut scratch, &mut |p, _| p == pos)
                };
                if matched == *negated {
                    return false;
                }
                if !negated {
                    *caps = scratch;
                }
                k(pos, caps)
            }
        }
    }

    fn sequence(&self, items: &[Node], input: &[char], pos: usize, caps: &mut Captures, k: Cont) -> bool {
        match items.split_first() {
            None => k(pos, caps),
            Some((first, rest)) => self.node(first, input, pos, caps, &mut |p, c| self.sequence(rest, input, p, c, k)),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn repeat(&self, inner: &Node, min: usize, max: usize, greedy: bool, count: usize, input: &[char], pos: usize, caps: &mut Captures, k: Cont) -> bool {
        let more = |caps: &mut Captures, k: Cont| {
            count < max
                && self.node(inner, input, pos, caps, &mut |p, c| {
                    // An empty iteration can only be useful while below the minimum.
                    if p == pos && count >= min {
                        return false;
                    }
                    self.repeat(inner, min, max, greedy, count + 1, input, p, c, k)
                })
        };
        if greedy {
            more(caps, k) || (count >= min && k(pos, caps))
        } else {
            (count >= min && k(pos, caps)) || more(caps, k)
        }
    }

    /// The ASCII characters a non-empty match may begin with.
    fn first_set(&self) -> Option<[bool; 128]> {
        let mut set = [false; 128];
        let nullable = self.first_of(&self.root, &mut set)?;
        (!nullable).then_some(set)
    }

    /// Adds the possible first characters of `node` to `set` and returns
    /// whether the node can match without consuming anything. `None` means
    /// the node is not analysed and every start must be tried.
    fn first_of(&self, node: &Node, set: &mut [bool; 128]) -> Option<bool> {
        Some(match node {
            Node::Empty => true,
            Node::Char(_) | Node::Any | Node::Class(_) => {
                for code in 0..128u8 {
                    if self.single(node, code as char) == Some(true) {
                        set[code as usize] = true;
                    }
                }
                false
            }
            Node::LineStart | Node::LineEnd | Node::TextStart | Node::TextEnd | Node::WordBoundary(_) | Node::Look { .. } => true,
            Node::Group(inner, _) => self.first_of(inner, set)?,
            Node::Concat(items) => {
                for item in items {
                    if !self.first_of(item, set)? {
                        return Some(false);
                    }
                }
                true
            }
            Node::Alt(branches) => {
                let mut nullable = false;
                for branch in branches {
                    nullable |= self.first_of(branch, set)?;
                }
                nullable
            }
            Node::Repeat { node, min, .. } => self.first_of(node, set)? || *min == 0,
            Node::Backref(_) => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(pattern: &str, text: &str) -> Option<String> {
        let re = Regex::new(pattern, Flags::VERBOSE).unwrap();
        let chars: Vec<char> = text.chars().collect();
        re.match_at(&chars, 0).map(|m| chars[m.start..m.end].iter().collect())
    }

    #[test]
    fn first_alternative_wins_not_longest() {
        assert_eq!(m("a|ab", "ab").as_deref(), Some("a"));
        assert_eq!(m("=>|<=|<>|=|<|>", "<>").as_deref(), Some("<>"));
        assert_eq!(m("=|<|>|<<", "<<").as_deref(), Some("<"));
    }

    #[test]
    fn greedy_and_lazy() {
        assert_eq!(m(".*b", "aabab").as_deref(), Some("aabab"));
        assert_eq!(m(".*?b", "aabab").as_deref(), Some("aab"));
        assert_eq!(m("a{2,3}", "aaaa").as_deref(), Some("aaa"));
        assert_eq!(m("a{2}", "a"), None);
        assert_eq!(m("x{", "x{").as_deref(), Some("x{"));
    }

    #[test]
    fn classes() {
        assert_eq!(m(r"[][}{'`.:$*/+<>|&~%^-]", "]").as_deref(), Some("]"));
        assert_eq!(m(r"[][}{'`.:$*/+<>|&~%^-]", "-").as_deref(), Some("-"));
        assert_eq!(m(r"[][}{'`.:$*/+<>|&~%^-]", "a"), None);
        assert_eq!(m(r"[^>]*", "ab>c").as_deref(), Some("ab"));
        assert_eq!(m(r"[_a-zA-Z][_a-zA-Z0-9]*[$%]?", "ab_1$ x").as_deref(), Some("ab_1$"));
    }

    #[test]
    fn verbose_ignores_blanks_but_not_in_classes() {
        assert_eq!(m(r"a b", "ab").as_deref(), Some("ab"));
        assert_eq!(m(r"[ \t]+ \#", "  #").as_deref(), Some("  #"));
        assert_eq!(m(r"a # comment", "a").as_deref(), Some("a"));
        let plain = Regex::new("a b", Flags::NONE).unwrap();
        assert!(plain.match_str("ab").is_none());
    }

    #[test]
    fn word_boundaries() {
        assert_eq!(m(r"(\b[Rr][Ee][Mm]\b)|'", "rem x").as_deref(), Some("rem"));
        assert_eq!(m(r"(\b[Rr][Ee][Mm]\b)|'", "remark"), None);
        let re = Regex::new(r"\basm\b", Flags::NONE).unwrap();
        let text: Vec<char> = "xasm asm".chars().collect();
        assert!(re.match_at(&text, 1).is_none());
        assert!(re.match_at(&text, 5).is_some());
    }

    #[test]
    fn groups_and_backreferences() {
        let re = Regex::new(r#"\#[ \t]*line[ \t]+([0-9]+)(?:[ \t]+"((?:[^"]|"")*)")?"#, Flags::NONE).unwrap();
        let text: Vec<char> = "#line 12 \"a.bas\"".chars().collect();
        let found = re.match_at(&text, 0).unwrap();
        assert_eq!(found.group(&text, 1).as_deref(), Some("12"));
        assert_eq!(found.group(&text, 2).as_deref(), Some("a.bas"));
        let text: Vec<char> = "#line 7".chars().collect();
        assert_eq!(re.match_at(&text, 0).unwrap().group(&text, 2), None);
        assert_eq!(m(r"(a+)b\1", "aabaa").as_deref(), Some("aabaa"));
        assert_eq!(m(r"(a+)b\1", "aaba"), None);
        assert_eq!(m(r"(?P<x>.)(?P=x)", "aa").as_deref(), Some("aa"));
    }

    #[test]
    fn group_repeats_backtrack() {
        assert_eq!(m(r#""([^"\n]|"")*""#, r#""a""b" c"#).as_deref(), Some(r#""a""b""#));
        assert_eq!(m(r"(ab|a)*c", "abac").as_deref(), Some("abac"));
        assert_eq!(m(r"(a*)*b", "aaa"), None);
    }

    #[test]
    fn lookaround_and_anchors() {
        assert_eq!(m(r"a(?=b)", "ab").as_deref(), Some("a"));
        assert_eq!(m(r"a(?!b)", "ab"), None);
        let re = Regex::new(r"(?<=x)y", Flags::NONE).unwrap();
        let text: Vec<char> = "xy".chars().collect();
        assert!(re.match_at(&text, 1).is_some());
        assert!(re.match_at(&text, 0).is_none());
        assert_eq!(m(r"^a$", "a\n").as_deref(), Some("a"));
        assert_eq!(m(r".*\n", "ab\ncd").as_deref(), Some("ab\n"));
    }

    #[test]
    fn search_and_full_match() {
        let re = Regex::new("b+", Flags::NONE).unwrap();
        let text: Vec<char> = "aabbbc".chars().collect();
        let found = re.search(&text, 0).unwrap();
        assert_eq!((found.start, found.end), (2, 5));
        assert!(re.full_match(&text).is_none());
        assert!(Regex::new("a+b+c", Flags::NONE).unwrap().full_match(&text).is_some());
    }

    #[test]
    fn ignore_case() {
        let re = Regex::new("[a-c]x", Flags { ignore_case: true, ..Flags::NONE }).unwrap();
        assert!(re.match_str("BX").is_some());
    }
}
