use std::collections::BTreeSet;
use std::fmt::Write;
use std::path::Path;

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = std::env::var("OUT_DIR").unwrap();

    // The peephole rule files, embedded. Within a directory they are in
    // name order: that is the order the Python compiler reads them in, and
    // rules that share an OFLAG keep it.
    let mut text = String::from("// Generated from data/peephole. Do not edit.\n\n");
    for arch in ["z80", "zxnext"] {
        let dir = root.join("data/peephole").join(arch);
        println!("cargo::rerun-if-changed={}", dir.display());
        let mut names: Vec<String> = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
            .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
            .filter(|name| name.ends_with(".opt"))
            .collect();
        names.sort();
        writeln!(text, "pub static {}_OPTS: [(&str, &str); {}] = [", arch.to_uppercase(), names.len()).unwrap();
        for name in &names {
            let path = dir.join(name).canonicalize().unwrap();
            println!("cargo::rerun-if-changed={}", path.display());
            writeln!(text, "    ({name:?}, include_str!({:?})),", path.display().to_string()).unwrap();
        }
        text.push_str("];\n\n");
    }
    std::fs::write(Path::new(&out).join("opts.rs"), text).unwrap();

    // The mnemonics of the opcode table (`Z80INSTR`): the first word of
    // each key.
    let table = root.join("grammar/z80.tsv");
    println!("cargo::rerun-if-changed={}", table.display());
    let table = std::fs::read_to_string(&table).unwrap_or_else(|e| panic!("{}: {e}", table.display()));
    let mnemonics: BTreeSet<&str> = table.lines().filter(|l| !l.starts_with('#') && !l.is_empty()).filter_map(|l| l.split('\t').next()?.split_whitespace().next()).collect();
    let mut text = String::from("// Generated from grammar/z80.tsv. Do not edit.\n\n");
    writeln!(text, "/// Every mnemonic, in upper case, sorted.\npub static Z80INSTR: [&str; {}] = [", mnemonics.len()).unwrap();
    for name in &mnemonics {
        writeln!(text, "    {name:?},").unwrap();
    }
    text.push_str("];\n");
    std::fs::write(Path::new(&out).join("z80instr.rs"), text).unwrap();
}
