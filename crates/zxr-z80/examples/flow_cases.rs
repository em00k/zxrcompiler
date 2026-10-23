//! The port's side of `tools/flow_fuzz.py`: programs on standard input,
//! each ended by a line `%%`, through the flow optimiser.
//!
//!     flow_cases z80|zxnext 3|4 < cases

use std::io::Read;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let zxnext = args[1] == "zxnext";
    let level: i64 = args[2].parse().unwrap();
    let mut text = String::new();
    std::io::stdin().read_to_string(&mut text).unwrap();
    std::panic::set_hook(Box::new(|_| {}));

    let mut out = Vec::new();
    let mut case: Vec<String> = Vec::new();
    for line in text.split('\n') {
        if line != "%%" {
            if !line.is_empty() {
                case.push(line.to_string());
            }
            continue;
        }
        let lines = std::mem::take(&mut case);
        let result = std::panic::catch_unwind(|| {
            let is_asm = |line: &str| line.strip_prefix("##ASM").is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())) && lines.iter().any(|l| l == line);
            let far_body = |label: &str| (label == "_f.__far").then(|| "_f".to_string());
            let env = zxr_opt::flow::Env { is_asm: &is_asm, far_body: &far_body, zxnext, level };
            zxr_z80::finish::flow_optimize_with(env, lines.clone())
        });
        out.push(result.unwrap_or_else(|_| "EXC".to_string()));
        out.push("%%".to_string());
    }
    println!("{}", out.join("\n"));
}
