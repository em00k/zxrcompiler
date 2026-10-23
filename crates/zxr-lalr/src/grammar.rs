//! The grammar file format.
//!
//! ```text
//! # comment
//! %token ID NUMBER PLUS
//! %left PLUS            lowest precedence first, one line per level
//! %start program
//!
//! @p_expr_plus          names the action for the productions that follow
//! expr : expr PLUS expr
//! expr : NUMBER %prec PLUS
//! empty :
//! ```
//!
//! One production per line. A symbol is a terminal if it was declared with
//! `%token` (or is `error`); anything else is a nonterminal.

use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Assoc {
    Left,
    Right,
    NonAssoc,
}

#[derive(Clone, Debug)]
pub struct Rule {
    pub number: usize,
    pub lhs: usize,
    pub rhs: Vec<usize>,
    pub prec: (Assoc, u32),
    /// Name of the action, as written after `@`.
    pub action: String,
}

/// Symbols are numbered terminals first, then nonterminals.
#[derive(Clone, Debug)]
pub struct Grammar {
    pub terminals: Vec<String>,
    pub nonterminals: Vec<String>,
    pub precedence: HashMap<usize, (Assoc, u32)>,
    /// Rule 0 is the augmented start rule `S' -> start`.
    pub rules: Vec<Rule>,
    /// The distinct action names, in order of first use.
    pub actions: Vec<String>,
}

pub const END: &str = "$end";
pub const ERROR: &str = "error";

impl Grammar {
    pub fn is_terminal(&self, symbol: usize) -> bool {
        symbol < self.terminals.len()
    }

    pub fn name(&self, symbol: usize) -> &str {
        if self.is_terminal(symbol) { &self.terminals[symbol] } else { &self.nonterminals[symbol - self.terminals.len()] }
    }

    pub fn terminal(&self, name: &str) -> Option<usize> {
        self.terminals.iter().position(|t| t == name)
    }

    pub fn end(&self) -> usize {
        self.terminal(END).unwrap()
    }

    pub fn parse(text: &str) -> Result<Grammar, String> {
        let mut terminals: Vec<String> = Vec::new();
        let mut levels: Vec<(Assoc, Vec<String>)> = Vec::new();
        let mut start = None;
        let mut raw: Vec<(String, Vec<String>, Option<String>, String)> = Vec::new();
        let mut action = String::new();
        for (index, line) in text.lines().enumerate() {
            let at = |msg: &str| format!("line {}: {msg}", index + 1);
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let words: Vec<&str> = line.split_whitespace().collect();
            match words[0] {
                "%token" => terminals.extend(words[1..].iter().map(|w| w.to_string())),
                "%left" | "%right" | "%nonassoc" => {
                    let assoc = match words[0] {
                        "%left" => Assoc::Left,
                        "%right" => Assoc::Right,
                        _ => Assoc::NonAssoc,
                    };
                    levels.push((assoc, words[1..].iter().map(|w| w.to_string()).collect()));
                }
                "%start" => start = Some(words.get(1).ok_or_else(|| at("%start needs a symbol"))?.to_string()),
                word if word.starts_with('@') => action = word[1..].to_string(),
                _ => {
                    if words.get(1) != Some(&":") {
                        return Err(at("expected `name : symbols`"));
                    }
                    let mut rhs: Vec<String> = words[2..].iter().map(|w| w.to_string()).collect();
                    let mut prec = None;
                    if let Some(p) = rhs.iter().position(|w| w == "%prec") {
                        if p + 2 != rhs.len() {
                            return Err(at("%prec must be followed by one terminal at the end of the rule"));
                        }
                        prec = rhs.pop();
                        rhs.pop();
                    }
                    if action.is_empty() {
                        return Err(at("production before any @action"));
                    }
                    raw.push((words[0].to_string(), rhs, prec, action.clone()));
                }
            }
        }
        let start = start.ok_or("no %start")?;
        // A terminal named only in a precedence line (a %prec marker) still counts.
        for (_, names) in &levels {
            for name in names {
                if !terminals.contains(name) {
                    terminals.push(name.clone());
                }
            }
        }
        terminals.push(ERROR.to_string());
        terminals.push(END.to_string());

        let mut nonterminals: Vec<String> = vec!["S'".to_string()];
        for (lhs, _, _, _) in &raw {
            if terminals.contains(lhs) {
                return Err(format!("rule name `{lhs}` is also a token"));
            }
            if !nonterminals.contains(lhs) {
                nonterminals.push(lhs.clone());
            }
        }
        let nt = terminals.len();
        let symbol = |name: &str| -> Result<usize, String> {
            terminals
                .iter()
                .position(|t| t == name)
                .or_else(|| nonterminals.iter().position(|n| n == name).map(|i| i + nt))
                .ok_or_else(|| format!("symbol `{name}` is neither a token nor a rule"))
        };
        let mut precedence = HashMap::new();
        for (level, (assoc, names)) in levels.iter().enumerate() {
            for name in names {
                precedence.insert(symbol(name)?, (*assoc, level as u32 + 1));
            }
        }
        let default_prec = (Assoc::Right, 0);
        let mut rules = vec![Rule { number: 0, lhs: nt, rhs: vec![symbol(&start)?], prec: default_prec, action: String::new() }];
        let mut actions: Vec<String> = Vec::new();
        for (lhs, rhs, prec, action) in &raw {
            let rhs: Vec<usize> = rhs.iter().map(|s| symbol(s)).collect::<Result<_, _>>()?;
            let prec = match prec {
                Some(name) => *precedence.get(&symbol(name)?).ok_or_else(|| format!("nothing known about the precedence of `{name}`"))?,
                // The precedence of the rightmost terminal, if it has one.
                None => rhs.iter().rev().find(|&&s| s < nt).and_then(|s| precedence.get(s)).copied().unwrap_or(default_prec),
            };
            if !actions.contains(action) {
                actions.push(action.clone());
            }
            rules.push(Rule { number: rules.len(), lhs: symbol(lhs)?, rhs, prec, action: action.clone() });
        }
        Ok(Grammar { terminals, nonterminals, precedence, rules, actions })
    }
}
