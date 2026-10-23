//! Shared by the command-line tools.

pub mod compile;
pub mod from_ir;
pub mod zxbc_args;

/// Runs a tool's `main` on a thread with a large stack (macro expansion and
/// expression matching recurse on their input) and exits with its status.
pub fn run(main: fn(Vec<String>) -> i32) -> ! {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let worker = std::thread::Builder::new().stack_size(512 << 20).spawn(move || zxr_core::run_catching_exit(|| main(args)));
    let code = match worker.map(|w| w.join()) {
        Ok(Ok(code)) => code,
        _ => 70,
    };
    std::process::exit(code & 0xFF)
}

/// With `ZXR_TIMINGS` set in the environment, reports on stderr the time
/// since the previous call (or since the first) under `stage`. Port only.
pub fn stage_done(stage: &str) {
    use std::sync::Mutex;
    use std::time::Instant;
    static LAST: Mutex<Option<Instant>> = Mutex::new(None);
    if std::env::var_os("ZXR_TIMINGS").is_none() {
        return;
    }
    let now = Instant::now();
    let mut last = LAST.lock().unwrap();
    if let Some(then) = *last {
        eprintln!("zxbc: timing: {stage:<24} {:>9.1} ms", (now - then).as_secs_f64() * 1000.0);
    }
    *last = Some(now);
}

include!(concat!(env!("OUT_DIR"), "/library.rs"));

/// The directory holding `arch/<name>/{stdlib,runtime}`.
///
/// One found beside the executable (or named by `ZXR_LIB`) is used as it is.
/// Failing that, the copy built into the binary is written out once under
/// the user's cache directory and used from there.
pub fn lib_root() -> String {
    let found = zxr_pp::find_lib_root();
    if std::path::Path::new(&found).join("arch").is_dir() {
        return found;
    }
    let cache = std::env::var_os("XDG_CACHE_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".cache")))
        .unwrap_or_else(std::env::temp_dir);
    let dir = cache.join("zxrcompiler").join(format!("lib-{LIBRARY_TAG}"));
    if !dir.join("arch").is_dir() {
        // Written beside its final place and renamed, so that two compiles
        // starting at once cannot see half a library.
        let staging = cache.join("zxrcompiler").join(format!("lib-{LIBRARY_TAG}.{}", std::process::id()));
        let written = LIBRARY.iter().try_for_each(|(rel, bytes)| {
            let path = staging.join(rel);
            std::fs::create_dir_all(path.parent().unwrap())?;
            std::fs::write(path, bytes)
        });
        if let Err(error) = written.and_then(|_| std::fs::rename(&staging, &dir)) {
            let _ = std::fs::remove_dir_all(&staging);
            if !dir.join("arch").is_dir() {
                eprintln!("zxbc: cannot unpack the library into {}: {error}", dir.display());
                zxr_core::exit(1);
            }
        }
    }
    dir.to_string_lossy().into_owned()
}
