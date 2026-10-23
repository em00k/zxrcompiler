//! `peephole/parser.py`: reads a rule file.
//!
//! A file has scalar definitions (`OLEVEL: 1`, `OFLAG: 14`) and regions
//! (`REPLACE {{ ... }}`, `WITH {{ ... }}`, `IF {{ ... }}`, `DEFINE {{ ... }}`).
//! The IF region and each DEFINE line hold an expression, parsed here into
//! nested lists for the evaluator.

use crate::asm::py_strip;
use crate::evaluator::{is_binary, is_unary, Evaluator, Tree, BINARY, UNARY};
use crate::pattern::svar_prefix;
use std::collections::BTreeMap;
use std::sync::OnceLock;
use zxr_re::{Flags, Regex};

const COMMENT: &str = ";;";

// Names of the different params
pub const REG_IF: &str = "IF";
pub const REG_REPLACE: &str = "REPLACE";
pub const REG_WITH: &str = "WITH";
pub const REG_DEFINE: &str = "DEFINE";
pub const O_LEVEL: &str = "OLEVEL";
pub const O_FLAG: &str = "OFLAG";

/// Operators : precedence (lower number -> highest priority)
fn if_operator(op: &str) -> Option<u32> {
    Some(match op {
        ".*" | "./" => 3,
        "+" | ".+" | ".-" => 5,
        "<>" | "==" => 10,
        "&&" => 15,
        "||" => 20,
        "IN" => 25,
        "," => 30,
        _ => return None,
    })
}

fn regex(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern, Flags::NONE).unwrap())
}

fn re_ifparse() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    regex(&RE, r#""(""|[^"])*"|[(),]|\b[_a-zA-Z]+\b|[^," \t()]+"#)
}

fn re_id() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    regex(&RE, r"\b[_a-zA-Z]+\b")
}

fn re_region() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    regex(&RE, r"([_a-zA-Z][a-zA-Z0-9]*)[ \t]*\{\{$")
}

fn re_def() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    regex(&RE, r"([_a-zA-Z][a-zA-Z0-9]*)[ \t]*:[ \t]*(.*)")
}

/// Simplifies ("unnest") a list, removing redundant brackets.
/// i.e. [[x, [[y]]] becomes [x, [y]]
fn simplify_expr(expr: Tree) -> Tree {
    match expr {
        Tree::Str(_) => expr,
        Tree::List(mut items) => {
            if items.len() == 1 && matches!(items[0], Tree::List(_)) {
                return simplify_expr(items.pop().unwrap());
            }
            Tree::List(items.into_iter().map(simplify_expr).collect())
        }
    }
}

struct Tokenizer {
    source: Vec<char>,
    lineno: usize,
}

impl Tokenizer {
    /// Returns next token, or "" as EOL
    fn get_token(&mut self) -> Result<String, String> {
        let text: String = self.source.iter().collect();
        self.source = py_strip(&text).chars().collect();
        if self.source.is_empty() {
            return Ok(String::new());
        }

        let Some(found) = re_ifparse().match_at(&self.source, 0) else {
            return Err(format!("Syntax error in line {}: {}", self.lineno, self.source.iter().collect::<String>()));
        };

        let mut tok: String = self.source[..found.end].iter().collect();
        if re_id().match_str(&tok).is_none() {
            // The Python code looks through a set of the operators' names
            // and cuts the token down to the first one it starts with, in
            // whatever order the set gives. No two of them start a token
            // of the rule files in a way that depends on that order; here
            // the longest wins.
            let mut opers: Vec<&str> = UNARY.iter().chain(BINARY.iter()).copied().collect();
            opers.sort_by_key(|op| std::cmp::Reverse(op.len()));
            if let Some(oper) = opers.into_iter().find(|oper| tok.starts_with(oper)) {
                tok = oper.to_string();
            }
        }

        self.source.drain(..tok.chars().count());
        Ok(tok)
    }

    fn has_finished(&self) -> bool {
        self.source.is_empty()
    }
}

fn items(tree: &Tree) -> Option<&Vec<Tree>> {
    match tree {
        Tree::List(items) => Some(items),
        Tree::Str(_) => None,
    }
}

/// Given a line from within a IF region (i.e. $1 == "af'") returns it as a
/// list of tokens ['$1', '==', "af'"]. An `Err` carries the warning the
/// Python code gives before it returns None.
pub fn parse_ifline(if_line: &str, lineno: usize) -> Result<Tree, String> {
    let mut stack: Vec<Vec<Tree>> = Vec::new();
    let mut expr: Vec<Tree> = Vec::new();
    let mut paren = 0i32;
    let mut tokenizer = Tokenizer { source: if_line.chars().collect(), lineno };
    // `stack[-1].append(expr); expr = stack.pop()`
    fn fold(stack: &mut Vec<Vec<Tree>>, expr: &mut Vec<Tree>) {
        let mut top = stack.pop().unwrap_or_else(|| panic!("IndexError: list index out of range"));
        top.push(Tree::List(std::mem::take(expr)));
        *expr = top;
    }

    while !tokenizer.has_finished() {
        let mut tok = tokenizer.get_token()?;

        if tok == "(" {
            paren += 1;
            stack.push(std::mem::take(&mut expr));
            continue;
        }

        if is_unary(&tok) {
            stack.push(std::mem::take(&mut expr));
            expr = vec![Tree::Str(tok)];
            continue;
        }

        if tok == ")" {
            paren -= 1;
            if paren < 0 {
                return Err("Too much closed parenthesis".to_string());
            }

            if expr.last().and_then(Tree::as_str) == Some(",") {
                return Err("missing element in list".to_string());
            }

            if expr.iter().skip(1).step_by(2).any(|x| x.as_str() != Some(",")) {
                return Err(format!("Invalid list {}", Tree::List(expr).repr()));
            }

            fold(&mut stack, &mut expr);
        } else {
            if tok.chars().count() > 1 && tok.starts_with('"') && tok.ends_with('"') {
                tok = tok[1..tok.len() - 1].to_string();
            }
            expr.push(Tree::Str(tok.clone()));
        }

        if tok == "," && (expr.len() < 2 || expr[expr.len() - 2].as_str() == Some(",")) {
            return Err(format!("Unexpected {tok} in list"));
        }

        while expr.len() == 2 && expr[0].as_str().is_some_and(is_unary) {
            fold(&mut stack, &mut expr);
        }

        if expr.len() == 3 && expr[1].as_str() != Some(",") {
            let Some(op) = expr[1].as_str().filter(|op| if_operator(op).is_some()).map(String::from) else {
                return Err(format!("Unexpected binary operator '{}'", match &expr[1] {
                    Tree::Str(s) => s.clone(),
                    list => list.repr(),
                }));
            };

            if let Some(left) = items(&expr[0]).filter(|left| left.len() == 3).cloned() {
                let oper2 = left[1].as_str().and_then(if_operator).unwrap_or_else(|| panic!("ValueError: {} is not a valid FN", left[1].repr()));
                if oper2 > if_operator(&op).unwrap() {
                    // Rebalance tree
                    let right = expr.pop().unwrap();
                    let rebalanced = Tree::List(vec![Tree::List(vec![left[0].clone()]), left[1].clone(), Tree::List(vec![left[2].clone(), Tree::Str(op), right])]);
                    expr = vec![rebalanced];
                    continue;
                }
            }

            expr = vec![Tree::List(expr)];
        }
    }

    if paren != 0 {
        return Err("unclosed parenthesis in IF section".to_string());
    }

    while !stack.is_empty() {
        fold(&mut stack, &mut expr);

        if expr.len() == 2 {
            if !expr[0].as_str().is_some_and(is_unary) {
                return Err(format!("unexpected unary operator '{}'", expr[0].repr()));
            }
        } else if expr.len() == 3 && !expr[1].as_str().is_some_and(is_binary) {
            return Err(format!("unexpected binary operator '{}'", expr[1].repr()));
        }
    }

    let expr = simplify_expr(Tree::List(expr));
    let list = items(&expr).cloned().unwrap_or_default();
    if list.len() == 2 && list[1].as_str().is_some_and(is_binary) {
        return Err(format!("Unexpected binary operator '{}'", list[1].repr()));
    }

    if list.len() == 3 && (!list[1].as_str().is_some_and(is_binary) || list[1].as_str() == Some(",")) {
        return Err(format!("Unexpected binary operator '{}'", list[1].repr()));
    }

    if list.len() > 3 {
        return Err("Lists not allowed in IF section condition. Missing operator".to_string());
    }

    Ok(expr)
}

/// A rule file, read.
#[derive(Debug)]
pub struct Parsed {
    pub level: i64,
    pub flag: i64,
    pub replace: Vec<String>,
    pub with: Vec<String>,
    /// The condition's tree.
    pub cond: Tree,
    /// Each `$n = <expression>` of the DEFINE region, with its line.
    pub defines: Vec<(String, usize, Tree)>,
}

/// Given a string with an optimizer template definition, parses it. An
/// `Err` is `(line, message)`: the warning that makes the Python compiler
/// ignore the file.
pub fn parse_str(spec: &str) -> Result<Parsed, (usize, String)> {
    let mut regions: BTreeMap<String, Vec<(usize, String)>> = BTreeMap::new();
    let mut scalars: BTreeMap<String, i64> = BTreeMap::new();
    let mut region_name: Option<String> = None;
    let mut line_num = 0;
    let is_region = |key: &str| [REG_IF, REG_WITH, REG_REPLACE, REG_DEFINE].contains(&key);
    let is_scalar = |key: &str| [O_FLAG, O_LEVEL].contains(&key);

    for line in spec.split('\n') {
        let mut line = py_strip(line);
        line_num += 1;

        if line.is_empty() {
            continue; // Ignore blank lines
        }

        if let Some(name) = &region_name {
            let mut closed = false;
            if let Some(rest) = line.strip_suffix("}}") {
                line = py_strip(rest);
                closed = true;
            }

            if !line.is_empty() {
                regions.entry(name.clone()).or_default().push((line_num, line.to_string()));
            }

            if closed {
                region_name = None;
            }
            continue;
        }

        if line.starts_with(COMMENT) {
            continue;
        }

        let chars: Vec<char> = line.chars().collect();
        if let Some(x) = re_region().match_at(&chars, 0) {
            let name = x.group(&chars, 1).unwrap();
            let key = name.to_uppercase();
            // A region defined twice, or one with a name that means
            // nothing, is warned about and read all the same.
            if !regions.contains_key(&key) && !scalars.contains_key(&key) && is_region(&key) {
                regions.insert(key, Vec::new());
            }
            region_name = Some(name);
            continue;
        }

        if let Some(x) = re_def().match_at(&chars, 0) {
            let key = x.group(&chars, 1).unwrap().to_uppercase();
            let val = x.group(&chars, 2).unwrap();
            if regions.contains_key(&key) || scalars.contains_key(&key) {
                return Err((line_num, format!("duplicated definition {key}")));
            }
            if !is_region(&key) && !is_scalar(&key) {
                return Err((line_num, format!("unknown definition parameter '{key}'")));
            }
            if !is_scalar(&key) {
                // A region given a value as a scalar is: the Python code
                // then fails further on. No rule file does this.
                return Err((line_num, format!("'{key}' is a region")));
            }
            if val.is_empty() || !val.bytes().all(|b| b.is_ascii_digit()) {
                return Err((line_num, format!("field '{key} must be integer")));
            }
            let number = val.parse().map_err(|_| (line_num, format!("field '{key} must be integer")))?;
            scalars.insert(key, number);
            continue;
        }

        return Err((line_num, "syntax error. Cannot parse file".to_string()));
    }

    let mut defines = Vec::new();
    for (lineno, line) in regions.get(REG_DEFINE).cloned().unwrap_or_default() {
        let Some((var, right)) = line.split_once('=') else {
            return Err((lineno, "assignation '=' not found".to_string()));
        };
        let (var, right) = (py_strip(var), py_strip(right));
        if svar_prefix(var).is_none() {
            // Define vars
            return Err((lineno, format!("'{var}' not a variable name")));
        }
        let expr = parse_ifline(right, lineno).map_err(|e| (lineno, e))?;
        if defines.iter().any(|(name, _, _): &(String, usize, Tree)| name == var) {
            return Err((lineno, format!("duplicated variable '{var}'")));
        }
        defines.push((var.to_string(), lineno, expr));
    }

    let lines = |name: &str| regions.get(name).map(|l| l.iter().map(|(_, line)| line.clone()).collect::<Vec<_>>());
    let cond = parse_ifline(&lines(REG_IF).unwrap_or_default().join(" "), line_num).map_err(|e| (line_num, e))?;

    // Put here all of the above that are required
    let replace = lines(REG_REPLACE).ok_or((line_num, format!("undefined section {REG_REPLACE}")))?;
    let with = lines(REG_WITH).ok_or((line_num, format!("undefined section {REG_WITH}")))?;
    let level = *scalars.get(O_LEVEL).ok_or((line_num, format!("undefined section {O_LEVEL}")))?;
    let flag = *scalars.get(O_FLAG).ok_or((line_num, format!("undefined section {O_FLAG}")))?;

    if replace.is_empty() {
        // Empty REPLACE region??
        return Err((line_num, format!("empty region {REG_REPLACE}")));
    }

    // The evaluators are built by the caller; make sure they can be.
    Evaluator::new(&cond).map_err(|e| (1, e))?;
    Ok(Parsed { level, flag, replace, with, cond, defines })
}
