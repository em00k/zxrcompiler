//! PLY's `LRParser.parse`, including its error recovery.

/// The tables of one grammar, as emitted by [`crate::codegen`].
pub struct Tables {
    pub n_terminals: usize,
    pub n_nonterminals: usize,
    /// `action[state * n_terminals + terminal]`; [`NO_ACTION`] where there is none.
    pub action: &'static [i32],
    /// `goto[state * n_nonterminals + nonterminal]`; negative where there is none.
    pub goto: &'static [i32],
    pub productions: &'static [Production],
    pub terminal_names: &'static [&'static str],
    pub end: u16,
    pub error: u16,
}

pub const NO_ACTION: i32 = i32::MIN;

/// The kind of a token whose type is not one of the grammar's terminals.
pub const UNKNOWN_TERMINAL: u16 = u16::MAX;

pub struct Production {
    /// Index of the left-hand side among the nonterminals.
    pub lhs: u16,
    pub len: u16,
}

/// A terminal or nonterminal on the parse stack.
#[derive(Clone, Debug, Default)]
pub struct Symbol<V> {
    /// A terminal id, or `n_terminals + nonterminal index`.
    pub kind: u16,
    pub value: V,
    /// Zero when unknown, which is what PLY's `p.lineno(n)` reports for a
    /// nonterminal when position tracking is off.
    pub lineno: u32,
    pub lexpos: usize,
    pub end_lineno: u32,
    pub end_lexpos: usize,
}

/// Raised by a reduction to make the parser enter error recovery, as a
/// `SyntaxError` raised from a PLY rule does.
#[derive(Debug)]
pub struct SyntaxError;

/// What the error handler asks the parser to do.
pub enum OnError<V> {
    /// Carry on with PLY's recovery.
    Recover,
    /// The handler called `errok()`: resume, with this as the next lookahead.
    Resume(Option<Symbol<V>>),
}

pub trait Driver<V> {
    /// The next token, or `None` at the end of input.
    fn token(&mut self) -> Option<Symbol<V>>;

    /// Runs the action of `production`. `rhs` holds the symbols being reduced;
    /// the result becomes the value of the left-hand side.
    fn reduce(&mut self, production: usize, rhs: &mut [Symbol<V>]) -> Result<V, SyntaxError>;

    /// Called on a syntax error with the offending token, or `None` at end of input.
    fn syntax_error(&mut self, token: Option<&Symbol<V>>) -> OnError<V>;

    /// The lexer's current line and position, used for the location of an
    /// empty production when tracking.
    fn position(&self) -> (u32, usize) {
        (0, 0)
    }
}

#[derive(Clone, Copy, Default)]
pub struct ParseOptions {
    /// PLY's `tracking=True`: nonterminals carry the position of what they span.
    pub tracking: bool,
    /// Reduce without reading a lookahead in states where only one reduction
    /// is possible. PLY does this by default; a grammar turns it off by
    /// clearing `parser.defaulted_states`.
    pub defaulted_states: bool,
}

const ERROR_COUNT: u32 = 3;

pub fn parse<V: Default, D: Driver<V>>(tables: &Tables, driver: &mut D, options: ParseOptions) -> Option<V> {
    let nt = tables.n_terminals;
    let action_of = |state: usize, terminal: u16| -> Option<i32> {
        // A lexer may hand over a token type the grammar does not know;
        // nothing accepts it, so it is a syntax error wherever it appears.
        if terminal as usize >= nt {
            return None;
        }
        let a = tables.action[state * nt + terminal as usize];
        (a != NO_ACTION).then_some(a)
    };
    // A state with exactly one entry, and that a reduction, needs no lookahead.
    let defaulted: Vec<Option<i32>> = if options.defaulted_states {
        (0..tables.action.len() / nt)
            .map(|state| {
                let mut entries = tables.action[state * nt..(state + 1) * nt].iter().filter(|&&a| a != NO_ACTION);
                match (entries.next(), entries.next()) {
                    (Some(&a), None) if a < 0 => Some(a),
                    _ => None,
                }
            })
            .collect()
    } else {
        Vec::new()
    };

    let end_symbol = || Symbol { kind: tables.end, ..Symbol::default() };
    let mut lookahead: Option<Symbol<V>> = None;
    let mut lookahead_stack: Vec<Symbol<V>> = Vec::new();
    let mut states: Vec<usize> = vec![0];
    let mut symbols: Vec<Symbol<V>> = vec![end_symbol()];
    let mut state = 0usize;
    let mut error_count = 0u32;
    let mut error_ok = true;

    loop {
        let t = match defaulted.get(state).copied().flatten() {
            Some(reduction) => Some(reduction),
            None => {
                if lookahead.is_none() {
                    lookahead = lookahead_stack.pop().or_else(|| driver.token()).or_else(|| Some(end_symbol()));
                }
                action_of(state, lookahead.as_ref().unwrap().kind)
            }
        };

        match t {
            Some(t) if t > 0 => {
                state = t as usize;
                states.push(state);
                symbols.push(lookahead.take().unwrap());
                error_count = error_count.saturating_sub(1);
            }
            Some(t) if t < 0 => {
                let number = (-t) as usize;
                let production = &tables.productions[number];
                let len = production.len as usize;
                let mut sym = Symbol { kind: nt as u16 + production.lhs, ..Symbol::default() };
                let mut rhs: Vec<Symbol<V>> = symbols.split_off(symbols.len() - len);
                if options.tracking {
                    if let (Some(first), Some(last)) = (rhs.first(), rhs.last()) {
                        sym.lineno = first.lineno;
                        sym.lexpos = first.lexpos;
                        sym.end_lineno = if last.end_lineno != 0 { last.end_lineno } else { last.lineno };
                        sym.end_lexpos = if last.end_lexpos != 0 { last.end_lexpos } else { last.lexpos };
                    } else {
                        (sym.lineno, sym.lexpos) = driver.position();
                    }
                }
                match driver.reduce(number, &mut rhs) {
                    Ok(value) => {
                        sym.value = value;
                        states.truncate(states.len() - len);
                        symbols.push(sym);
                        let target = tables.goto[states[states.len() - 1] * tables.n_nonterminals + production.lhs as usize];
                        debug_assert!(target >= 0, "no goto");
                        state = target as usize;
                        states.push(state);
                    }
                    Err(SyntaxError) => {
                        // Put back all but the last symbol and feed in an error token.
                        lookahead_stack.extend(lookahead.take());
                        if len > 0 {
                            rhs.pop();
                            symbols.extend(rhs);
                        }
                        states.pop();
                        state = states[states.len() - 1];
                        lookahead = Some(Symbol { kind: tables.error, ..Symbol::default() });
                        error_count = ERROR_COUNT;
                        error_ok = false;
                    }
                }
            }
            Some(_) => return symbols.pop().map(|s| s.value),
            None => {
                let current = lookahead.as_ref().unwrap();
                if error_count == 0 || error_ok {
                    error_count = ERROR_COUNT;
                    error_ok = false;
                    let token = (current.kind != tables.end).then_some(current);
                    if let OnError::Resume(next) = driver.syntax_error(token) {
                        error_ok = true;
                        lookahead = next;
                        continue;
                    }
                } else {
                    error_count = ERROR_COUNT;
                }
                let current = lookahead.as_ref().unwrap();
                // Nothing left to unwind: drop the token and start again.
                if states.len() <= 1 && current.kind != tables.end {
                    lookahead = None;
                    state = 0;
                    lookahead_stack.clear();
                    continue;
                }
                if current.kind == tables.end {
                    return None;
                }
                if current.kind != tables.error {
                    // Already recovering and still stuck: discard the token.
                    if symbols.last().is_some_and(|s| s.kind == tables.error) {
                        if options.tracking {
                            let (line, pos) = (current.lineno, current.lexpos);
                            let top = symbols.last_mut().unwrap();
                            top.end_lineno = line;
                            top.end_lexpos = pos;
                        }
                        lookahead = None;
                        continue;
                    }
                    let bad = lookahead.take().unwrap();
                    let error = Symbol { kind: tables.error, lineno: bad.lineno, lexpos: bad.lexpos, end_lineno: bad.lineno, end_lexpos: bad.lexpos, ..Symbol::default() };
                    lookahead_stack.push(bad);
                    lookahead = Some(error);
                } else {
                    // Unwind one state, looking for one that accepts `error`.
                    let popped = symbols.pop().unwrap();
                    if options.tracking {
                        let l = lookahead.as_mut().unwrap();
                        l.lineno = popped.lineno;
                        l.lexpos = popped.lexpos;
                    }
                    states.pop();
                    state = states[states.len() - 1];
                }
            }
        }
    }
}
