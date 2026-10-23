//! LALR(1) table construction, following PLY's `LRTable` step for step where
//! the order of work is visible in the result: state numbering, the order of
//! items within a state, and the way conflicts are settled.

use crate::grammar::{Assoc, Grammar};
use std::collections::{BTreeSet, HashMap};

type Item = (usize, usize); // (rule, dot)

#[derive(Clone, Debug)]
pub struct Tables {
    /// Per state: terminal -> action. Positive is a shift to that state,
    /// negative a reduction by that rule, zero is accept. A terminal mapped to
    /// `None` is an error PLY records explicitly (a `nonassoc` clash).
    pub action: Vec<HashMap<usize, Option<i32>>>,
    /// Per state: nonterminal symbol -> state.
    pub goto: Vec<HashMap<usize, usize>>,
    pub shift_reduce: usize,
    pub reduce_reduce: usize,
}

struct Builder<'g> {
    g: &'g Grammar,
    /// Rules of each nonterminal, in definition order.
    by_lhs: HashMap<usize, Vec<usize>>,
    nullable: Vec<bool>,
    first: Vec<BTreeSet<usize>>,
    /// States as full closures; the kernel is what identifies a state.
    states: Vec<Vec<Item>>,
    by_kernel: HashMap<Vec<Item>, usize>,
    transitions: Vec<HashMap<usize, usize>>,
}

impl<'g> Builder<'g> {
    fn new(g: &'g Grammar) -> Self {
        let mut by_lhs: HashMap<usize, Vec<usize>> = HashMap::new();
        for rule in &g.rules {
            by_lhs.entry(rule.lhs).or_default().push(rule.number);
        }
        let n = g.terminals.len() + g.nonterminals.len();
        let mut b = Builder { g, by_lhs, nullable: vec![false; n], first: vec![BTreeSet::new(); n], states: Vec::new(), by_kernel: HashMap::new(), transitions: Vec::new() };
        b.compute_first();
        b
    }

    fn compute_first(&mut self) {
        for t in 0..self.g.terminals.len() {
            self.first[t].insert(t);
        }
        loop {
            let mut changed = false;
            for rule in &self.g.rules {
                let mut all_nullable = true;
                for &s in &rule.rhs {
                    let add: Vec<usize> = self.first[s].iter().copied().filter(|t| !self.first[rule.lhs].contains(t)).collect();
                    if !add.is_empty() {
                        self.first[rule.lhs].extend(add);
                        changed = true;
                    }
                    if !self.nullable[s] {
                        all_nullable = false;
                        break;
                    }
                }
                if all_nullable && !self.nullable[rule.lhs] {
                    self.nullable[rule.lhs] = true;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
    }

    fn after_dot(&self, (rule, dot): Item) -> Option<usize> {
        self.g.rules[rule].rhs.get(dot).copied()
    }

    /// PLY's `lr0_closure`: the kernel, then the initial item of every rule
    /// reachable through the symbol after a dot, each added once, in the order
    /// the growing list is scanned.
    fn closure(&self, kernel: &[Item]) -> Vec<Item> {
        let mut items = kernel.to_vec();
        let mut added = vec![false; self.g.rules.len()];
        let mut i = 0;
        while i < items.len() {
            if let Some(rules) = self.after_dot(items[i]).and_then(|s| self.by_lhs.get(&s)) {
                for &r in rules {
                    if !added[r] {
                        added[r] = true;
                        items.push((r, 0));
                    }
                }
            }
            i += 1;
        }
        items
    }

    /// PLY's `lr0_items`: breadth first, following symbols in the order they
    /// first appear in the rules of a state's items.
    fn build_states(&mut self) {
        let start = vec![(0, 0)];
        self.states.push(self.closure(&start));
        self.by_kernel.insert(start, 0);
        self.transitions.push(HashMap::new());
        let mut i = 0;
        while i < self.states.len() {
            let state = self.states[i].clone();
            let mut symbols: Vec<usize> = Vec::new();
            for &(rule, _) in &state {
                for &s in &self.g.rules[rule].rhs {
                    if !symbols.contains(&s) {
                        symbols.push(s);
                    }
                }
            }
            for x in symbols {
                let kernel: Vec<Item> = state.iter().filter(|&&item| self.after_dot(item) == Some(x)).map(|&(r, d)| (r, d + 1)).collect();
                if kernel.is_empty() {
                    continue;
                }
                let target = match self.by_kernel.get(&kernel) {
                    Some(&t) => t,
                    None => {
                        let t = self.states.len();
                        self.states.push(self.closure(&kernel));
                        self.by_kernel.insert(kernel, t);
                        self.transitions.push(HashMap::new());
                        t
                    }
                };
                self.transitions[i].insert(x, target);
            }
            i += 1;
        }
    }

    /// FIRST of a symbol string followed by a lookahead set.
    fn first_of(&self, symbols: &[usize], tail: &BTreeSet<usize>) -> BTreeSet<usize> {
        let mut out = BTreeSet::new();
        for &s in symbols {
            out.extend(self.first[s].iter().copied());
            if !self.nullable[s] {
                return out;
            }
        }
        out.extend(tail.iter().copied());
        out
    }

    /// LR(1) closure of a set of items with lookahead sets, restricted to the
    /// items of one state. Returns the lookaheads of every item.
    fn lr1_closure(&self, seed: &HashMap<Item, BTreeSet<usize>>) -> HashMap<Item, BTreeSet<usize>> {
        let mut la: HashMap<Item, BTreeSet<usize>> = seed.clone();
        let mut work: Vec<Item> = seed.keys().copied().collect();
        while let Some(item) = work.pop() {
            let Some(b) = self.after_dot(item) else { continue };
            let Some(rules) = self.by_lhs.get(&b) else { continue };
            let follow = self.first_of(&self.g.rules[item.0].rhs[item.1 + 1..], &la[&item]);
            for &r in rules {
                let is_new = !la.contains_key(&(r, 0));
                let entry = la.entry((r, 0)).or_default();
                let before = entry.len();
                entry.extend(follow.iter().copied());
                if is_new || entry.len() != before {
                    work.push((r, 0));
                }
            }
        }
        la
    }

    /// Lookaheads of the kernel items of every state, by propagation.
    fn kernel_lookaheads(&self) -> Vec<HashMap<Item, BTreeSet<usize>>> {
        const HASH: usize = usize::MAX;
        let kernels: Vec<Vec<Item>> = self.states.iter().enumerate().map(|(i, s)| s.iter().copied().filter(|&(r, d)| d > 0 || (i == 0 && r == 0)).collect()).collect();
        let mut la: Vec<HashMap<Item, BTreeSet<usize>>> = kernels.iter().map(|k| k.iter().map(|&i| (i, BTreeSet::new())).collect()).collect();
        la[0].get_mut(&(0, 0)).unwrap().insert(self.g.end());
        let mut links: Vec<((usize, Item), (usize, Item))> = Vec::new();
        for (i, kernel) in kernels.iter().enumerate() {
            for &k in kernel {
                let seed = HashMap::from([(k, BTreeSet::from([HASH]))]);
                for (item, set) in self.lr1_closure(&seed) {
                    let Some(x) = self.after_dot(item) else { continue };
                    let target = self.transitions[i][&x];
                    let next = (item.0, item.1 + 1);
                    for &t in &set {
                        if t == HASH {
                            links.push(((i, k), (target, next)));
                        } else {
                            la[target].get_mut(&next).unwrap().insert(t);
                        }
                    }
                }
            }
        }
        loop {
            let mut changed = false;
            for &((s, item), (ts, titem)) in &links {
                let add: Vec<usize> = la[s][&item].iter().copied().filter(|t| !la[ts][&titem].contains(t)).collect();
                if !add.is_empty() {
                    la[ts].get_mut(&titem).unwrap().extend(add);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        la
    }
}

pub fn build(g: &Grammar) -> Tables {
    let mut b = Builder::new(g);
    b.build_states();
    let kernel_la = b.kernel_lookaheads();
    let nt = g.terminals.len();
    let default = (Assoc::Right, 0u32);
    let mut tables = Tables { action: Vec::new(), goto: Vec::new(), shift_reduce: 0, reduce_reduce: 0 };

    for (st, state) in b.states.iter().enumerate() {
        let la = b.lr1_closure(&kernel_la[st]);
        let mut action: HashMap<usize, Option<i32>> = HashMap::new();
        // The rule behind each entry, to look up its precedence later.
        let mut owner: HashMap<usize, usize> = HashMap::new();
        for &item in state {
            let rule = &g.rules[item.0];
            match b.after_dot(item) {
                None if item.0 == 0 => {
                    action.insert(g.end(), Some(0));
                    owner.insert(g.end(), 0);
                }
                None => {
                    let empty = BTreeSet::new();
                    for &a in la.get(&item).unwrap_or(&empty) {
                        match action.get(&a).copied().flatten() {
                            // PLY treats an explicit error entry like a free one.
                            None => {
                                action.insert(a, Some(-(rule.number as i32)));
                                owner.insert(a, rule.number);
                            }
                            Some(r) if r > 0 => {
                                // Shift/reduce: reduce if the rule binds tighter, or
                                // equally and is left associative.
                                let (_, slevel) = g.precedence.get(&a).copied().unwrap_or(default);
                                let (rprec, rlevel) = rule.prec;
                                if slevel < rlevel || (slevel == rlevel && rprec == Assoc::Left) {
                                    action.insert(a, Some(-(rule.number as i32)));
                                    owner.insert(a, rule.number);
                                    if slevel == 0 && rlevel == 0 {
                                        tables.shift_reduce += 1;
                                    }
                                } else if slevel == rlevel && rprec == Assoc::NonAssoc {
                                    action.insert(a, None);
                                } else if rlevel == 0 {
                                    tables.shift_reduce += 1;
                                }
                            }
                            Some(r) => {
                                // Reduce/reduce: the rule defined first wins.
                                if (-r) as usize > rule.number {
                                    action.insert(a, Some(-(rule.number as i32)));
                                    owner.insert(a, rule.number);
                                }
                                tables.reduce_reduce += 1;
                            }
                        }
                    }
                }
                Some(a) if a < nt => {
                    let j = b.transitions[st][&a] as i32;
                    match action.get(&a).copied().flatten() {
                        None => {
                            action.insert(a, Some(j));
                            owner.insert(a, rule.number);
                        }
                        Some(r) if r > 0 => assert_eq!(r, j, "shift/shift conflict in state {st}"),
                        Some(_) => {
                            let (_, slevel) = g.precedence.get(&a).copied().unwrap_or(default);
                            let (rprec, rlevel) = g.rules[owner[&a]].prec;
                            if slevel > rlevel || (slevel == rlevel && rprec == Assoc::Right) {
                                action.insert(a, Some(j));
                                owner.insert(a, rule.number);
                                if rlevel == 0 {
                                    tables.shift_reduce += 1;
                                }
                            } else if slevel == rlevel && rprec == Assoc::NonAssoc {
                                action.insert(a, None);
                            } else if slevel == 0 && rlevel == 0 {
                                tables.shift_reduce += 1;
                            }
                        }
                    }
                }
                Some(_) => {}
            }
        }
        tables.action.push(action);
        tables.goto.push(b.transitions[st].iter().filter(|(s, _)| **s >= nt).map(|(&s, &t)| (s, t)).collect());
    }
    tables
}
