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
if os.name == "nt":
    BINARY += ".exe"

if __name__ == "__main__":
    sys.exit(subprocess.call([BINARY] + sys.argv[1:]))
"#;

const MODULE: &str = r#""""The compiler as NextBuild imports it: `zxbc.main(argv)` returns its exit
status. The work is done by the native binary in ../../bin."""
import os
import subprocess
import sys

BINARY = os.path.join(os.path.dirname(os.path.abspath(__file__)), os.pardir, os.pardir, "bin", "zxbc")
if os.name == "nt":
    BINARY += ".exe"


def main(args=None, emitter=None):
    if args is None:
        args = sys.argv[1:]
    sys.stdout.flush()
    sys.stderr.flush()
    return subprocess.call([os.path.normpath(BINARY)] + [str(a) for a in args])
"#;

const TOOLS: [&str; 4] = ["zxbc", "zxbasm", "zxbpp", "zxnex"];

/// `dist/zxrcompiler` for this machine (Linux, x86-64): the tools linked
/// statically, so the directory can be copied to another Linux machine as it
/// is. They go to their own target directory: the flag would otherwise
/// rebuild everything the tests use.
pub fn dist() -> Result<(), String> {
    let root = repo_root();
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
    let out = root.join("dist/zxrcompiler");
    assemble(&out, &root.join("target").join(TARGET).join("release"), "", "linux-x64")?;
    for tool in TOOLS {
        // Debug information is of no use to whoever only runs the tools.
        let _ = std::process::Command::new("strip").arg(out.join(format!("bin/{tool}"))).status();
    }
    println!("wrote {}", out.display());
    Ok(())
}

/// The drop-in directories for Linux, Windows and macOS, each beside an
/// archive of itself: `dist/zxrcompiler-<os>` and `.tar.gz` (`.zip` for
/// Windows). The tools are cross-built in the cargo-zigbuild container
/// (Docker), which has the Rust targets and zig to link them: a static musl
/// build for Linux, the GNU target for Windows, and one universal binary for
/// macOS on Intel and Apple Silicon.
pub fn dist_all() -> Result<(), String> {
    let root = repo_root();
    let uid = String::from_utf8_lossy(&std::process::Command::new("id").arg("-u").output().map_err(|e| e.to_string())?.stdout).trim().to_string();
    let gid = String::from_utf8_lossy(&std::process::Command::new("id").arg("-g").output().map_err(|e| e.to_string())?.stdout).trim().to_string();
    const BUILDS: [(&str, &str, &str); 3] =
        [("linux-x64", "x86_64-unknown-linux-musl", ""), ("windows-x64", "x86_64-pc-windows-gnu", ".exe"), ("macos", "universal2-apple-darwin", "")];
    let script: String = BUILDS.iter().map(|(_, target, _)| format!("cargo zigbuild --release -p zxr-cli --target {target} && ")).collect::<String>() + "true";
    let status = std::process::Command::new("docker")
        .args(["run", "--rm", "-u", &format!("{uid}:{gid}"), "-v", &format!("{}:/src", root.display()), "-w", "/src"])
        .args(["-e", "HOME=/tmp", "-e", "CARGO_HOME=/tmp/cargo", "-e", "CARGO_TARGET_DIR=/src/target/cross"])
        .args(["-e", "CARGO_PROFILE_RELEASE_DEBUG=0", "-e", "CARGO_PROFILE_RELEASE_STRIP=symbols"])
        .args(["ghcr.io/rust-cross/cargo-zigbuild:latest", "sh", "-c", &script])
        .status()
        .map_err(|e| format!("docker: {e}"))?;
    if !status.success() {
        return Err("the cross build failed".into());
    }
    for (os, target, exe) in BUILDS {
        let name = format!("zxrcompiler-{}-{os}", env!("CARGO_PKG_VERSION"));
        let out = root.join("dist").join(&name);
        assemble(&out, &root.join("target/cross").join(target).join("release"), exe, os)?;
        let archive = |cmd: &mut std::process::Command| -> Result<(), String> {
            match cmd.current_dir(root.join("dist")).status() {
                Ok(s) if s.success() => Ok(()),
                other => Err(format!("archiving {name}: {other:?}")),
            }
        };
        if exe.is_empty() {
            let _ = fs::remove_file(root.join(format!("dist/{name}.tar.gz")));
            archive(std::process::Command::new("tar").args(["-czf", &format!("{name}.tar.gz"), &name]))?;
        } else {
            let _ = fs::remove_file(root.join(format!("dist/{name}.zip")));
            archive(std::process::Command::new("python3").args(["-m", "zipfile", "-c", &format!("{name}.zip"), &name]))?;
        }
        println!("wrote {}", out.display());
    }
    Ok(())
}

/// Lays out a drop-in directory at `out` from the binaries in `built`, each
/// named with `exe` after it, for the platform `os` (`linux-x64`,
/// `windows-x64`, `macos`).
fn assemble(out: &std::path::Path, built: &std::path::Path, exe: &str, os: &str) -> Result<(), String> {
    let root = repo_root();
    let _ = fs::remove_dir_all(out);
    let put = |rel: &str, data: &[u8]| write_file(&out.join(rel), data).map_err(|e| format!("{rel}: {e}"));
    for tool in TOOLS {
        let binary = built.join(format!("{tool}{exe}"));
        let data = fs::read(&binary).map_err(|e| format!("{}: {e}", binary.display()))?;
        put(&format!("bin/{tool}{exe}"), &data)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(out.join(format!("bin/{tool}{exe}")), fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
        }
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
    // What NextBuild prints as `ZXbasic ver`: the name and version, short.
    put("src/zxbc/version.py", format!("VERSION = \"zxrcompiler {}\"\n", env!("CARGO_PKG_VERSION")).as_bytes())?;

    // The runtime and standard library, where the binary looks for them.
    let lib = root.join("lib");
    for rel in walk(&lib) {
        put(&format!("src/lib/{}", rel.display()), &fs::read(lib.join(&rel)).map_err(|e| e.to_string())?)?;
    }
    // The NEX packer nextbuild.py imports from the compiler tree.
    let creator = root.join("reference/zxbasic/tools/nextcreator.py");
    put("tools/nextcreator.py", &fs::read(&creator).map_err(|e| format!("{}: {e}", creator.display()))?)?;
    put("LICENSE.txt", &fs::read(root.join("LICENSE.txt")).map_err(|e| e.to_string())?)?;
    put("CHANGELOG.md", &fs::read(root.join("CHANGELOG.md")).map_err(|e| e.to_string())?)?;

    // What it is, how to install it, and exactly which build it is.
    let run = |cmd: &str, args: &[&str]| {
        std::process::Command::new(cmd).current_dir(&root).args(args).output().ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default()
    };
    let mut commit = run("git", &["rev-parse", "--short", "HEAD"]);
    if !run("git", &["status", "--porcelain", "--untracked-files=no"]).is_empty() {
        commit.push_str(" with uncommitted changes");
    }
    let date = run("date", &["-u", "+%Y-%m-%d"]);
    let version = env!("CARGO_PKG_VERSION");
    let folder = out.file_name().unwrap().to_string_lossy().to_string();
    let (platform, install, notes, zxbc, exe_note) = match os {
        "windows-x64" => (
            "Windows (x86-64)",
            format!("   ```\n   ZXBASIC=C:\\NextBuild\\{folder}\n   ```\n"),
            "Windows 10 or later. The programs in `bin/` need nothing else installed.\n",
            r"bin\zxbc.exe",
            " (`.exe`)",
        ),
        "macos" => (
            "macOS (Intel and Apple Silicon)",
            format!("   ```\n   ZXBASIC=/Users/you/NextBuild/{folder}\n   ```\n"),
            "**macOS will refuse to run the programs** until the quarantine flag a\ndownload puts on them is cleared. Once, in Terminal, in the folder you\nunpacked this into:\n\n```\nxattr -dr com.apple.quarantine {FOLDER}\n```\n\nThe programs are universal: one file runs on Intel and Apple Silicon Macs.\nThis build has not been tried on a Mac yet; please report what happens.\n",
            "bin/zxbc",
            "",
        ),
        _ => (
            "Linux (x86-64)",
            format!("   ```\n   ZXBASIC=/home/you/NextBuild/{folder}\n   ```\n"),
            "The programs in `bin/` are linked statically: they run on any Linux\ndistribution, with nothing else installed. Unpack with `tar -xzf`, which\nkeeps them executable.\n",
            "bin/zxbc",
            "",
        ),
    };
    let readme = include_str!("dist_readme.md")
        .replace("{PLATFORM_NOTES}", notes)
        .replace("{INSTALL}", &install)
        .replace("{PLATFORM}", platform)
        .replace("{VERSION}", version)
        .replace("{BASE}", &base)
        .replace("{COMMIT}", &commit)
        .replace("{DATE}", &date)
        .replace("{FOLDER}", &folder)
        .replace("{ZXBC}", zxbc)
        .replace("{EXE_NOTE}", exe_note);
    put("README.md", readme.as_bytes())?;
    let version_file = format!(
        "zxrcompiler {version}\nplatform: {os}\ncommit: {commit}\nbuilt: {date}\ncompatible with: zxbasic {base} (the NextBuild fork), the Python compiler\nnextbuild.py shows: zxrcompiler {version}\n"
    );
    put("VERSION", version_file.as_bytes())?;
    Ok(())
}
