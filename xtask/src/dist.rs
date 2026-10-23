//! Builds the drop-in directory NextBuild can be pointed at.
//!
//! `nextbuild.py` imports the compiler as a Python module (`from src.zxbc
//! import zxbc`, then `zxbc.main(argv)`), reads `src/zxbc/version.py`, and
//! imports `tools/nextcreator.py`; the simulator harness runs `zxbasm.py`.
//! The directory therefore looks like the Python compiler's tree, with each
//! entry point handing over to the native binary.

use crate::util::{repo_root, walk, write_file};
use std::fs;

const LAUNCHER: &str = r#"#!/usr/bin/env python3
"""Runs the native TOOL with the arguments given."""
import os
import subprocess
import sys

BINARY = os.path.join(os.path.dirname(os.path.abspath(__file__)), "bin", "TOOL")

if __name__ == "__main__":
    sys.exit(subprocess.call([BINARY] + sys.argv[1:]))
"#;

const MODULE: &str = r#""""The compiler as NextBuild imports it: `zxbc.main(argv)` returns its exit
status. The work is done by the native binary in ../../bin."""
import os
import subprocess
import sys

BINARY = os.path.join(os.path.dirname(os.path.abspath(__file__)), os.pardir, os.pardir, "bin", "zxbc")


def main(args=None, emitter=None):
    if args is None:
        args = sys.argv[1:]
    sys.stdout.flush()
    sys.stderr.flush()
    return subprocess.call([os.path.normpath(BINARY)] + [str(a) for a in args])
"#;

pub fn dist() -> Result<(), String> {
    let root = repo_root();
    let out = root.join("dist/zxrcompiler");
    let _ = fs::remove_dir_all(&out);
    let put = |rel: &str, data: &[u8]| write_file(&out.join(rel), data).map_err(|e| format!("{rel}: {e}"));

    // The tools are linked statically, so the directory can be copied to
    // another Linux machine as it is. They go to their own target directory:
    // the flag would otherwise rebuild everything the tests use.
    const TARGET: &str = "x86_64-unknown-linux-gnu";
    let status = std::process::Command::new(env!("CARGO"))
        .current_dir(&root)
        .args(["build", "--release", "-p", "zxr-cli", "--target", TARGET])
        .env("RUSTFLAGS", "-C target-feature=+crt-static")
        .status()
        .map_err(|e| format!("cargo: {e}"))?;
    if !status.success() {
        return Err("the static build failed".into());
    }
    let built = root.join("target").join(TARGET).join("release");
    for tool in ["zxbc", "zxbasm", "zxbpp", "zxnex"] {
        let binary = built.join(tool);
        let data = fs::read(&binary).map_err(|e| format!("{}: {e}", binary.display()))?;
        put(&format!("bin/{tool}"), &data)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(out.join(format!("bin/{tool}")), fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
        }
        // Debug information is of no use to whoever only runs the tools.
        let _ = std::process::Command::new("strip").arg(out.join(format!("bin/{tool}"))).status();
    }
    for tool in ["zxbc", "zxbasm", "zxbpp"] {
        put(&format!("{tool}.py"), LAUNCHER.replace("TOOL", tool).as_bytes())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(out.join(format!("{tool}.py")), fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
        }
    }
    put("src/__init__.py", b"")?;
    put("src/zxbc/__init__.py", b"")?;
    put("src/zxbc/zxbc.py", MODULE.as_bytes())?;
    let lock = fs::read_to_string(root.join("oracle.lock")).unwrap_or_default();
    let base = lock.lines().find_map(|l| l.strip_prefix("version ")).unwrap_or("unknown").trim().to_string();
    put("src/zxbc/version.py", format!("VERSION = \"{base}-zxr{}\"\n", env!("CARGO_PKG_VERSION")).as_bytes())?;

    // The runtime and standard library, where the binary looks for them.
    let lib = root.join("lib");
    for rel in walk(&lib) {
        put(&format!("src/lib/{}", rel.display()), &fs::read(lib.join(&rel)).map_err(|e| e.to_string())?)?;
    }
    // The NEX packer nextbuild.py imports from the compiler tree.
    let creator = root.join("reference/zxbasic/tools/nextcreator.py");
    put("tools/nextcreator.py", &fs::read(&creator).map_err(|e| format!("{}: {e}", creator.display()))?)?;
    println!("wrote {}", out.display());
    Ok(())
}
