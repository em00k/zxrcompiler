use std::path::Path;

fn main() {
    let grammar_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../grammar");
    let out = std::env::var("OUT_DIR").unwrap();
    let read = |name: &str| {
        let path = grammar_dir.join(name);
        println!("cargo::rerun-if-changed={}", path.display());
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    };

    let grammar = zxr_lalr::grammar::Grammar::parse(&read("zxbpp.y")).expect("grammar/zxbpp.y");
    let tables = zxr_lalr::build::build(&grammar);
    std::fs::write(Path::new(&out).join("grammar.rs"), zxr_lalr::codegen::generate(&grammar, &tables)).unwrap();

    for (file, target) in [("zxbpp_basic.lex", "lex_basic.rs"), ("zxbpp_asm.lex", "lex_asm.rs")] {
        std::fs::write(Path::new(&out).join(target), zxr_lex::generate(&read(file))).unwrap();
    }
}
