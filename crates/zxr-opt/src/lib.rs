//! The peephole optimiser: `src/arch/z80/peephole` of the Python compiler.
//!
//! A rule is a file under `data/peephole`: a block of instruction patterns
//! to REPLACE, the block to replace it WITH, and an optional condition. The
//! backend applies the rules of levels 1 and 2 as it emits each quad's
//! instructions ([`engine::apply_match`]); those of level 3 and above are
//! applied by the flow optimiser, basic block by basic block, with what it
//! knows about the CPU's state supplied through [`evaluator::Context`].
//!
//! Also here, because the rules' functions are defined by them, are the
//! parts of the flow optimiser's own modules that describe one instruction:
//! [`asm`], [`memcell`] and [`helpers`]; and the flow optimiser itself,
//! [`flow`].

pub mod asm;
pub mod engine;
pub mod evaluator;
pub mod flow;
pub mod helpers;
pub mod memcell;
pub mod parser;
pub mod pattern;
pub mod pyeval;
pub mod template;
