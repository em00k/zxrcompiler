use std::fmt::Write;
use std::path::Path;

fn main() {
    let data_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/backend");
    let out = std::env::var("OUT_DIR").unwrap();
    let read = |name: &str| {
        let path = data_dir.join(name);
        println!("cargo::rerun-if-changed={}", path.display());
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    };
    let rows = |text: &str| -> Vec<Vec<String>> {
        text.lines().filter(|l| !l.starts_with('#') && !l.is_empty()).map(|l| l.split('\t').map(String::from).collect()).collect()
    };

    // The runtime labels: one constant each, and the library file a call to
    // each pulls in.
    let labels = rows(&read("runtime_labels.tsv"));
    let mut text = String::from("// Generated from data/backend/runtime_labels.tsv. Do not edit.\n\n");
    for row in &labels {
        writeln!(text, "pub const {}: &str = {:?};", row[0], row[1]).unwrap();
    }
    writeln!(text, "\n/// Every runtime label, sorted.\npub static RUNTIME_LABELS: [&str; {}] = [", labels.len()).unwrap();
    let mut sorted: Vec<&str> = labels.iter().map(|r| r[1].as_str()).collect();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), labels.len(), "duplicated runtime label");
    for label in &sorted {
        writeln!(text, "    {label:?},").unwrap();
    }
    text.push_str("];\n\n/// Label and the library file it lives in, sorted by label.\npub static LABEL_REQUIRED_MODULES: &[(&str, &str)] = &[\n");
    let mut modules: Vec<(&str, &str)> = labels.iter().filter(|r| r[2] != "-").map(|r| (r[1].as_str(), r[2].as_str())).collect();
    modules.sort();
    for (label, module) in &modules {
        writeln!(text, "    ({label:?}, {module:?}),").unwrap();
    }
    text.push_str("];\n");
    std::fs::write(Path::new(&out).join("runtime_labels.rs"), text).unwrap();

    // The quad table. A handler is named as the Python table names it:
    // `Bits8.add8` is a method of a class, `_jump` a function of the module
    // it is defined in, and `zxnext:` marks the ZX Next's own version.
    let module_of = |func: &str| -> &'static str {
        if func.starts_with('_') {
            if func.starts_with("_pa") && func != "_paddr" {
                return "parray";
            }
            if func.starts_with("_a") {
                return "array";
            }
            if func.starts_with("_p") || func == "_fploadstr" {
                return "pload";
            }
            return "generic";
        }
        unreachable!()
    };
    let handler = |spec: &str| -> String {
        let (next, name) = match spec.strip_prefix("zxnext:") {
            Some(rest) => (true, rest),
            None => (false, spec),
        };
        match name.split_once('.') {
            Some((class, method)) if next => format!("crate::zxnext::{class}::{method}"),
            // The Python class `String` is `Str` here, so as not to hide the
            // standard type.
            Some(("String", method)) => format!("crate::string::Str::{method}"),
            Some((class, method)) => format!("crate::{}::{class}::{method}", match class {
                "Bits8" => "bits8",
                "Bits16" => "bits16",
                "Bits32" => "bits32",
                "Fixed16" => "f16",
                "Float" => "float",
                other => panic!("unknown class {other}"),
            }),
            None if next => format!("crate::zxnext::{name}"),
            None => format!("crate::{}::{name}", module_of(name)),
        }
    };
    let quads = rows(&read("quads.tsv"));
    let mut text = String::from("// Generated from data/backend/quads.tsv. Do not edit.\n\n");
    writeln!(text, "/// Name, operand count, and the handlers for zx48k and for zxnext.\npub static QUAD_TABLE: [(&str, usize, Handler, Handler); {}] = [", quads.len()).unwrap();
    for row in &quads {
        writeln!(text, "    ({:?}, {}, {}, {}),", row[0], row[1], handler(&row[2]), handler(&row[3])).unwrap();
    }
    text.push_str("];\n");
    std::fs::write(Path::new(&out).join("quad_table.rs"), text).unwrap();
}
