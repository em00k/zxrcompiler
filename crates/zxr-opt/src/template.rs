//! `peephole/template.py`: the WITH side of a rule.

use crate::evaluator::Value;
use crate::pattern::BasicLinePattern;
use std::collections::BTreeMap;

/// Given a template line (i.e. 'push $1') and a dictionary of variables
/// {'$1': value1, '$2': value2} replaces such variables with their values.
/// '$$' is replaced by '$'. An unbound variable is an error.
#[derive(Clone, Debug)]
pub struct LineTemplate {
    pub base: BasicLinePattern,
}

impl LineTemplate {
    /// Applies a list of vars to the given pattern and returns the line
    pub fn filter(&self, vars: &BTreeMap<String, Value>) -> String {
        let mut result = String::new();
        for tok in &self.base.output {
            if tok.chars().count() > 1 && tok.starts_with('$') {
                match vars.get(tok) {
                    None | Some(Value::None) => panic!("UnboundVarError: Unbound variable {tok}"),
                    Some(Value::Str(val)) => result.push_str(val),
                    Some(other) => panic!("TypeError: can only concatenate str (not \"{}\") to str", other.type_name()),
                }
            } else {
                result.push_str(tok);
            }
        }

        crate::asm::py_strip(&result).to_string()
    }
}

/// Extends a Line template to a block of them
#[derive(Clone, Debug)]
pub struct BlockTemplate {
    pub templates: Vec<LineTemplate>,
}

impl BlockTemplate {
    pub fn new(lines: &[String]) -> BlockTemplate {
        let templates = lines.iter().map(|x| crate::asm::py_strip(x)).filter(|x| !x.is_empty()).map(|x| LineTemplate { base: BasicLinePattern::new(x) }).collect();
        BlockTemplate { templates }
    }

    pub fn filter(&self, vars: &BTreeMap<String, Value>) -> Vec<String> {
        self.templates.iter().map(|x| x.filter(vars)).filter(|y| !y.is_empty()).collect()
    }
}
