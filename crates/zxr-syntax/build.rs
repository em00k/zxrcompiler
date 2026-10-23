use std::fmt::Write;
use std::path::Path;

fn main() {
    let grammar_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../grammar");
    let out = std::env::var("OUT_DIR").unwrap();
    let read = |name: &str| {
        let path = grammar_dir.join(name);
        println!("cargo::rerun-if-changed={}", path.display());
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    };
    let write = |name: &str, text: String| std::fs::write(Path::new(&out).join(name), text).unwrap();

    let grammar = zxr_lalr::grammar::Grammar::parse(&read("zxbasic.y")).expect("grammar/zxbasic.y");
    write("grammar.rs", zxr_lalr::codegen::generate(&grammar, &zxr_lalr::build::build(&grammar)));
    write("lex.rs", zxr_lex::generate(&read("zxbasic.lex")));

    // Keywords: the word, and the grammar terminal it lexes as.
    let mut text = String::from("// Generated from grammar/zxbasic.keywords. Do not edit.\n\n");
    let rows: Vec<(String, String)> =
        read("zxbasic.keywords").lines().filter(|l| !l.starts_with('#') && !l.is_empty()).filter_map(|l| l.split_once('\t')).map(|(a, b)| (a.to_string(), b.to_string())).collect();
    writeln!(text, "pub static KEYWORDS: [(&str, &str, u16); {}] = [", rows.len()).unwrap();
    for (word, token) in &rows {
        let id = grammar.terminal(token).unwrap_or_else(|| panic!("keyword token {token} is not a terminal"));
        writeln!(text, "    ({word:?}, {token:?}, {id}),").unwrap();
    }
    text.push_str("];\n");
    write("keywords.rs", text);
}
