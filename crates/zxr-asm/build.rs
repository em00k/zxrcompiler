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

    // The Next grammar is the plain Z80 one plus the Z80N rules; both use the
    // same actions, so the plain one borrows the Next grammar's Rule enum.
    let next = zxr_lalr::grammar::Grammar::parse(&read("zxbasm_next.y")).expect("grammar/zxbasm_next.y");
    write("grammar_next.rs", zxr_lalr::codegen::generate(&next, &zxr_lalr::build::build(&next)));
    let z80 = zxr_lalr::grammar::Grammar::parse(&read("zxbasm.y")).expect("grammar/zxbasm.y");
    assert_eq!(z80.terminals, next.terminals, "the two assembler grammars must share their tokens");
    write("grammar_z80.rs", zxr_lalr::codegen::generate_sharing_rules(&z80, &zxr_lalr::build::build(&z80), "super::next"));

    write("lex.rs", zxr_lex::generate(&read("zxbasm.lex")));

    // The opcode table. An operand is a run of N in the key ("LD (IX+N),N");
    // its width in bytes is the length of the run.
    let table = read("z80.tsv");
    let rows: Vec<Vec<&str>> = table.lines().filter(|l| !l.starts_with('#') && !l.is_empty()).map(|l| l.split('\t').collect()).collect();
    let mut text = String::from("// Generated from grammar/z80.tsv. Do not edit.\n\n");
    writeln!(text, "pub static OPCODES: [Opcode; {}] = [", rows.len()).unwrap();
    for row in &rows {
        writeln!(text, "    Opcode {{ key: {:?}, tstates: {}, size: {}, encoding: {:?} }},", row[0], row[1], row[2], row[3]).unwrap();
    }
    text.push_str("];\n");
    write("opcodes.rs", text);
}
