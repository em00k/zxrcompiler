//! Emits quads one at a time, each from a clean state: the port's side of
//! `tools/quad_fuzz.py`. Reads records from stdin (`arch <name>`,
//! `strategy <name>` or `quad <op> <operands...>`, tab separated, with the
//! escapes of the backend-in format) and prints, for each quad, the
//! instructions, the libraries required, the lines parked for the end and the
//! inline blocks; or `!!` and the message where the Python code would raise.

use std::io::Read;
use zxr_ir::Quad;
use zxr_z80::{Backend, Options};

fn unescape(field: &str) -> String {
    let mut out = String::new();
    let mut chars = field.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

fn main() {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input).unwrap();
    std::panic::set_hook(Box::new(|_| {}));
    let mut arch = "zx48k".to_string();
    let mut strategy = "auto".to_string();
    let mut out = String::new();
    for line in input.split('\n') {
        let fields: Vec<String> = line.split('\t').map(unescape).collect();
        match fields[0].as_str() {
            "arch" => arch = fields[1].clone(),
            "strategy" => strategy = fields[1].clone(),
            "quad" => {
                let quad = Quad { op: fields[1].clone(), args: fields[2..].to_vec(), origin: 0 };
                let mut backend = Backend::new(Options { architecture: arch.clone(), opt_strategy: strategy.clone(), ..Options::default() });
                // A second copy shows what the handler does once the flags
                // of an earlier one are set.
                backend.memory = vec![quad.clone(), quad.clone()];
                out.push_str(&format!("== {quad}\n"));
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| backend.emit(false))) {
                    Ok(lines) => {
                        for line in lines {
                            out.push_str(&format!("{}\n", line.replace('\n', "\\n")));
                        }
                        for line in &backend.common.at_end {
                            out.push_str(&format!("@ {line}\n"));
                        }
                        for (key, lines) in &backend.common.asms {
                            out.push_str(&format!("{key} {lines:?}\n"));
                        }
                    }
                    Err(_) => out.push_str("!!\n"),
                }
            }
            _ => {}
        }
    }
    print!("{out}");
}
