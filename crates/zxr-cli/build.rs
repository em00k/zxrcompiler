//! Embeds the runtime and standard library in the tools, so that a binary on
//! its own is a complete compiler.

use std::fmt::Write;
use std::path::{Path, PathBuf};

fn walk(base: &Path, dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir).unwrap().flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            walk(base, &path, out);
        } else {
            out.push(path.strip_prefix(base).unwrap().to_path_buf());
        }
    }
}

fn main() {
    let lib = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../lib").canonicalize().expect("the lib directory");
    println!("cargo::rerun-if-changed={}", lib.join("SHA256SUMS").display());
    let mut files = Vec::new();
    walk(&lib, &lib, &mut files);
    let mut text = String::from("// Generated from lib/. Do not edit.\n\n");
    // What the files are is named by the digest list that travels with them.
    let sums = std::fs::read(lib.join("SHA256SUMS")).unwrap_or_default();
    let tag = sums.iter().fold(0xcbf29ce484222325u64, |h, &b| (h ^ b as u64).wrapping_mul(0x100000001b3));
    writeln!(text, "pub const LIBRARY_TAG: &str = \"{tag:016x}\";").unwrap();
    writeln!(text, "pub static LIBRARY: [(&str, &[u8]); {}] = [", files.len()).unwrap();
    for rel in &files {
        writeln!(text, "    ({:?}, include_bytes!({:?})),", rel.to_string_lossy(), lib.join(rel).to_string_lossy()).unwrap();
    }
    text.push_str("];\n");
    std::fs::write(Path::new(&std::env::var("OUT_DIR").unwrap()).join("library.rs"), text).unwrap();
}
