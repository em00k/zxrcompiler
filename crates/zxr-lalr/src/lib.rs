//! LALR(1) parsing the way PLY does it.
//!
//! The Python compiler's three grammars rely on PLY's conflict resolution,
//! precedence rules and error recovery, so "a correct LALR(1) parser" is not
//! enough: the tables have to be PLY's tables. [`grammar`] reads a grammar
//! file, [`build`] constructs the same states in the same order with the same
//! actions, [`codegen`] turns them into Rust source for a build script, and
//! [`runtime`] is PLY's parse loop.

pub mod build;
pub mod codegen;
pub mod grammar;
pub mod runtime;

pub use runtime::{parse, Driver, OnError, ParseOptions, Production, Symbol, SyntaxError, Tables};
