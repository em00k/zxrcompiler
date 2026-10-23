//! The BASIC front end: lexer, grammar, syntax tree, symbol table, the
//! passes over the tree and the translation to intermediate code.

pub mod ast;
pub mod check;
pub mod codebank_asm;
pub mod compiler;
pub mod dump;
pub mod frontend;
pub mod lexer;
pub mod nodes;
pub mod optimize;
pub mod options;
pub mod ordmap;
pub mod parser;
pub mod pyval;
pub mod records;
pub mod symtab;
pub mod translate;
pub mod types;

pub use ast::{Class, Kind, Node, NodeId, Scope, ScopeId, Val};
pub use compiler::Compiler;

#[allow(clippy::all)]
pub mod grammar {
    include!(concat!(env!("OUT_DIR"), "/grammar.rs"));
}
