//! A toolchain is anything exposing the zxbc / zxbasm / zxbpp command lines:
//! the Python compiler under reference/ (the oracle) or the Rust port.

use crate::util::{repo_root, replace_bytes};
use std::path::PathBuf;
use std::process::Command;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    Zxbc,
    Zxbasm,
    Zxbpp,
}

impl Tool {
    pub fn name(self) -> &'static str {
        match self {
            Tool::Zxbc => "zxbc",
            Tool::Zxbasm => "zxbasm",
            Tool::Zxbpp => "zxbpp",
        }
    }
}

#[derive(Clone)]
pub enum Toolchain {
    /// The Python compiler in reference/zxbasic.
    Oracle { python: String, root: PathBuf },
    /// The Rust port: a directory holding `zxbc`, `zxbasm` and `zxbpp`.
    Port { bin_dir: PathBuf, lib_root: PathBuf },
}

impl Toolchain {
    pub fn oracle() -> Self {
        Toolchain::Oracle {
            python: std::env::var("ZXR_PYTHON").unwrap_or_else(|_| "/usr/bin/python3".into()),
            root: repo_root().join("reference/zxbasic"),
        }
    }

    pub fn port() -> Self {
        Toolchain::Port { bin_dir: repo_root().join("target/release"), lib_root: repo_root().join("lib") }
    }

    pub fn parse(name: &str) -> Result<Self, String> {
        match name {
            "oracle" => Ok(Self::oracle()),
            "port" => Ok(Self::port()),
            other => Err(format!("unknown toolchain '{other}' (expected oracle or port)")),
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Toolchain::Oracle { .. } => "oracle",
            Toolchain::Port { .. } => "port",
        }
    }

    pub fn command(&self, tool: Tool) -> Command {
        match self {
            Toolchain::Oracle { python, root } => {
                let mut cmd = Command::new(python);
                cmd.arg(root.join(format!("{}.py", tool.name())));
                // A fixed environment: no bytecode written into the tree, and
                // a known width for argparse's usage text.
                cmd.env("PYTHONDONTWRITEBYTECODE", "1").env("COLUMNS", "80").env_remove("PYTHONHASHSEED");
                cmd
            }
            Toolchain::Port { bin_dir, .. } => {
                let mut cmd = Command::new(bin_dir.join(tool.name()));
                cmd.env("ZXR_ORACLE_DIAGNOSTICS", "1").env("ZXR_ORACLE_OPTIMISER", "1").env("ZXR_ORACLE_RUNTIME", "1");
                cmd
            }
        }
    }

    /// The command that runs the compiler up to an intermediate stage and
    /// writes what that stage produced to the `-o` file. `pp` is the
    /// preprocessed source.
    pub fn stage_command(&self, stage: &str) -> Command {
        match self {
            Toolchain::Oracle { python, root } => {
                let mut cmd = Command::new(python);
                cmd.arg(repo_root().join("tools/oracle_stage.py")).arg(root).arg(stage);
                cmd.env("PYTHONDONTWRITEBYTECODE", "1").env("COLUMNS", "80").env_remove("PYTHONHASHSEED");
                cmd
            }
            Toolchain::Port { bin_dir, .. } => {
                let mut cmd = Command::new(bin_dir.join("zxbc"));
                cmd.arg("--stage").arg(stage).env("ZXR_ORACLE_DIAGNOSTICS", "1").env("ZXR_ORACLE_OPTIMISER", "1").env("ZXR_ORACLE_RUNTIME", "1");
                cmd
            }
        }
    }

    /// Rewrites the paths and names that legitimately differ between
    /// toolchains, so that outputs can be compared.
    pub fn normalise(&self, data: &[u8]) -> Vec<u8> {
        match self {
            Toolchain::Oracle { root, .. } => {
                let mut out = replace_bytes(data, root.to_string_lossy().as_bytes(), b"/zxbasic");
                for tool in ["zxbc", "zxbasm", "zxbpp"] {
                    out = replace_bytes(&out, format!("{tool}.py").as_bytes(), tool.as_bytes());
                }
                out
            }
            Toolchain::Port { bin_dir, lib_root } => {
                let out = replace_bytes(data, lib_root.to_string_lossy().as_bytes(), b"/zxbasic/src/lib");
                replace_bytes(&out, bin_dir.to_string_lossy().as_bytes(), b"/zxbasic")
            }
        }
    }
}
