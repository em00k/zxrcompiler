//! The tables built here must be PLY's tables: same states, same numbering,
//! same action for every terminal. The fixtures under tests/lalr are dumped
//! from PLY by tools/oracle_dump.py.

use std::collections::HashMap;
use std::path::PathBuf;
use zxr_lalr::build::build;
use zxr_lalr::grammar::Grammar;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn check(name: &str) {
    let grammar = Grammar::parse(&std::fs::read_to_string(root().join(format!("grammar/{name}.y"))).unwrap()).unwrap();
    let tables = build(&grammar);
    let dump = std::fs::read_to_string(root().join(format!("tests/lalr/{name}.tables"))).unwrap();
    let mut lines = dump.lines();

    let count: usize = lines.next().unwrap().strip_prefix("productions ").unwrap().parse().unwrap();
    assert_eq!(count, grammar.rules.len(), "number of productions");
    for rule in &grammar.rules {
        let fields: Vec<&str> = lines.next().unwrap().split('\t').collect();
        let rhs: Vec<&str> = rule.rhs.iter().map(|&s| grammar.name(s)).collect();
        assert_eq!(fields[1], grammar.name(rule.lhs), "lhs of rule {}", rule.number);
        assert_eq!(fields[4], rhs.join(" "), "rhs of rule {}", rule.number);
        if rule.number > 0 {
            assert_eq!(fields[3], rule.action, "action of rule {}", rule.number);
        }
    }

    let mut expected: Vec<(HashMap<String, Option<i32>>, HashMap<String, usize>)> = Vec::new();
    for line in lines {
        let words: Vec<&str> = line.split(' ').collect();
        match words[0] {
            "state" => expected.push(Default::default()),
            "a" => {
                expected.last_mut().unwrap().0.insert(words[1].to_string(), words[2].parse().ok());
            }
            "g" => {
                expected.last_mut().unwrap().1.insert(words[1].to_string(), words[2].parse().unwrap());
            }
            other => panic!("unexpected line kind {other}"),
        }
    }
    assert_eq!(expected.len(), tables.action.len(), "number of states");
    for (state, (actions, gotos)) in expected.iter().enumerate() {
        let ours: HashMap<String, Option<i32>> = tables.action[state].iter().map(|(&t, &a)| (grammar.name(t).to_string(), a)).collect();
        assert_eq!(&ours, actions, "actions of state {state}");
        let ours: HashMap<String, usize> = tables.goto[state].iter().map(|(&s, &t)| (grammar.name(s).to_string(), t)).collect();
        assert_eq!(&ours, gotos, "gotos of state {state}");
    }
}

#[test]
fn zxbpp() {
    check("zxbpp");
}

#[test]
fn zxbasic() {
    check("zxbasic");
}

#[test]
fn zxbasm() {
    check("zxbasm");
}

#[test]
fn zxbasm_next() {
    check("zxbasm_next");
}
