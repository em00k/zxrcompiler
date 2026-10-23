//! The NEX packer on its own: `zxnex FILE.cfg FILE.nex [--ram-required MB]`.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut files = Vec::new();
    let mut ram = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--ram-required" {
            ram = iter.next().and_then(|v| v.parse().ok());
        } else {
            files.push(arg.clone());
        }
    }
    let [cfg, out] = &files[..] else {
        eprintln!("usage: zxnex FILEIN FILEOUT [--ram-required MB]");
        std::process::exit(2);
    };
    match zxr_nex::pack(std::path::Path::new(cfg), std::path::Path::new(out), ram) {
        Ok(log) => log.iter().for_each(|line| println!("{line}")),
        Err(error) => {
            eprintln!("zxnex: {error}");
            std::process::exit(1);
        }
    }
}
