//! What every stage of the compiler shares.

pub mod paths;
pub mod pynum;
pub mod session;
pub mod text;

pub use session::{exit, run_catching_exit, Options, Session};

/// The architectures the compiler targets; the first is the default.
pub const ARCHITECTURES: [&str; 2] = ["zx48k", "zxnext"];
