//! File name handling with the semantics of the Python functions the
//! compiler uses (`os.path.realpath`, `dirname`, `basename`), because paths
//! end up in `#line` markers and messages.

use std::path::{Component, Path, PathBuf};

/// Source files may name includes with back-slashes.
pub fn sanitize(name: &str) -> String {
    name.replace('\\', "/")
}

fn expand_user(name: &str) -> String {
    if name == "~" || name.starts_with("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return format!("{home}{}", &name[1..]);
        }
    }
    name.to_string()
}

fn normalise(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("/");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// On Windows, a path in the form the rest of the compiler handles, as on
/// other systems: `C:/dir/file`, with `/` and without the `\\?\` prefix
/// `canonicalize` puts on it. Windows takes `/` as well as `\`.
fn portable(path: String) -> String {
    if !cfg!(windows) {
        return path;
    }
    let path = match path.strip_prefix(r"\\?\UNC\") {
        Some(share) => format!(r"\\{share}"),
        None => path.strip_prefix(r"\\?\").map_or(path.clone(), String::from),
    };
    path.replace('\\', "/")
}

/// `C:/...` or `C:\...`: absolute on Windows, where it does not start with `/`.
fn has_drive(name: &str) -> bool {
    let b = name.as_bytes();
    cfg!(windows) && b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'/' || b[2] == b'\\')
}

/// `os.path.realpath(os.path.expanduser(name))`: absolute, with symbolic links
/// resolved as far as the path exists. An empty name is the current directory.
pub fn absolute(name: &str) -> String {
    let expanded = expand_user(name);
    let mut path = PathBuf::from(&expanded);
    if !path.is_absolute() {
        path = std::env::current_dir().unwrap_or_default().join(path);
    }
    let path = normalise(&path);
    // Resolve the longest prefix that exists, then append the rest.
    let mut existing = path.clone();
    let mut rest: Vec<std::ffi::OsString> = Vec::new();
    loop {
        if let Ok(real) = existing.canonicalize() {
            let mut out = real;
            for part in rest.iter().rev() {
                out.push(part);
            }
            return portable(out.to_string_lossy().into_owned());
        }
        match existing.file_name() {
            Some(name) => {
                rest.push(name.to_os_string());
                existing.pop();
            }
            None => return portable(path.to_string_lossy().into_owned()),
        }
    }
}

/// A path under the current directory is shown relative to it; any other is
/// shown in full.
pub fn relative_to_cwd(name: &str) -> String {
    let full = absolute(name);
    let cwd = absolute(".");
    match full.strip_prefix(&cwd) {
        Some(rest) => rest.trim_start_matches('/').to_string(),
        None => full,
    }
}

/// `os.path.dirname`.
pub fn dirname(name: &str) -> String {
    match name.rfind('/') {
        Some(at) => {
            let head = &name[..=at];
            let trimmed = head.trim_end_matches('/');
            if trimmed.is_empty() { head.to_string() } else { trimmed.to_string() }
        }
        None => String::new(),
    }
}

/// `os.path.basename`.
pub fn basename(name: &str) -> String {
    name[name.rfind('/').map_or(0, |at| at + 1)..].to_string()
}

/// `os.path.join(a, b)`.
pub fn join(a: &str, b: &str) -> String {
    if b.starts_with('/') || has_drive(b) || a.is_empty() {
        b.to_string()
    } else if a.ends_with('/') {
        format!("{a}{b}")
    } else {
        format!("{a}/{b}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn python_path_functions() {
        assert_eq!(dirname("a/b/c.bas"), "a/b");
        assert_eq!(dirname("c.bas"), "");
        assert_eq!(dirname("/c.bas"), "/");
        assert_eq!(dirname("a//b"), "a");
        assert_eq!(basename("a/b/c.bas"), "c.bas");
        assert_eq!(basename("c.bas"), "c.bas");
        assert_eq!(join("a", "b"), "a/b");
        assert_eq!(join("", "b"), "b");
        assert_eq!(join("a/", "b"), "a/b");
        assert_eq!(join("a", "/b"), "/b");
        assert_eq!(sanitize("a\\b"), "a/b");
    }

    #[test]
    fn absolute_and_relative() {
        let cwd = std::env::current_dir().unwrap().canonicalize().unwrap().to_string_lossy().into_owned();
        assert_eq!(absolute(""), cwd);
        assert_eq!(absolute("x/../no-such-file"), format!("{cwd}/no-such-file"));
        assert_eq!(relative_to_cwd("no-such-file"), "no-such-file");
        assert_eq!(relative_to_cwd(""), "");
        assert_eq!(relative_to_cwd("/no/such"), "/no/such");
    }
}
