//! `peephole/engine.py`: the rules, loaded and sorted, and the function
//! that tries them at one position of a list of instructions.

use crate::evaluator::{Context, Evaluator, Value};
use crate::parser;
use crate::pattern::{BlockPattern, LineChars};
use crate::template::BlockTemplate;
use std::collections::BTreeMap;
use std::sync::OnceLock;

include!(concat!(env!("OUT_DIR"), "/opts.rs"));

#[derive(Debug)]
pub struct OptPattern {
    pub level: i64,
    pub flag: i64,
    pub patt: BlockPattern,
    pub cond: Evaluator,
    pub template: BlockTemplate,
    /// Variables computed from the ones the pattern binds, in order.
    pub defines: Vec<(String, Evaluator)>,
    pub fname: String,
}

/// The rules of one architecture.
#[derive(Debug, Default)]
pub struct Engine {
    /// Sorted by OFLAG; rules that share one stay in the order read.
    pub patterns: Vec<OptPattern>,
    /// Max len of any pattern read
    pub maxlen: usize,
}

/// Parses one rule file. An `Err` is the reason the Python compiler would
/// ignore it, with a warning.
pub fn read_opt(fname: &str, text: &str) -> Result<OptPattern, String> {
    let parsed = parser::parse_str(text).map_err(|(line, message)| format!("{fname}:{line}: {message}"))?;
    let error = |e: String| format!("{fname}:1: There is an error in this template and it will be ignored ({e})");
    let patt = BlockPattern::new(&parsed.replace).map_err(error)?;
    let mut defines = Vec::new();
    for (var, lineno, tree) in &parsed.defines {
        if patt.vars.contains(var) {
            return Err(format!("{fname}:{lineno}: variable '{var}' already defined in pattern"));
        }
        defines.push((var.clone(), Evaluator::new(tree).map_err(error)?));
    }
    Ok(OptPattern {
        level: parsed.level,
        flag: parsed.flag,
        patt,
        template: BlockTemplate::new(&parsed.with),
        cond: Evaluator::new(&parsed.cond).map_err(error)?,
        defines,
        fname: fname.to_string(),
    })
}

impl Engine {
    /// Reads (and parses) rule files, one directory after the other,
    /// retaining only those with no errors. After each directory everything
    /// read so far is sorted by OFLAG, stably.
    pub fn load(directories: &[&[(&str, &str)]]) -> (Engine, Vec<String>) {
        let mut engine = Engine::default();
        let mut errors = Vec::new();
        for files in directories {
            for (fname, text) in files.iter() {
                match read_opt(fname, text) {
                    Ok(pattern) => {
                        engine.maxlen = engine.maxlen.max(pattern.patt.len());
                        engine.patterns.push(pattern);
                    }
                    Err(error) => errors.push(error),
                }
            }
            engine.patterns.sort_by_key(|p| p.flag);
        }
        (engine, errors)
    }

    /// The rules the compiler ships for an architecture: the Z80's, and for
    /// the ZX Spectrum Next its own after them.
    pub fn for_arch(arch: &str) -> &'static Engine {
        static Z80: OnceLock<Engine> = OnceLock::new();
        static NEXT: OnceLock<Engine> = OnceLock::new();
        let build = |dirs: &[&[(&str, &str)]]| {
            let (engine, errors) = Engine::load(dirs);
            assert!(errors.is_empty(), "the peephole rules do not parse: {errors:?}");
            engine
        };
        if arch == "zxnext" { NEXT.get_or_init(|| build(&[&Z80_OPTS, &ZXNEXT_OPTS])) } else { Z80.get_or_init(|| build(&[&Z80_OPTS])) }
    }
}

/// Narrows a list of rules to those that could match at a given line, by
/// the line's first word. A rule whose first line starts with fixed text
/// (`push $1` starts with `push`) cannot match a line that does not start
/// with it, so most rules need not be tried at most lines. The order of the
/// rules is kept, so the first that matches is the same one.
#[derive(Default)]
pub struct Candidates<'p> {
    by_word: std::collections::HashMap<String, Vec<&'p OptPattern>>,
}

impl<'p> Candidates<'p> {
    pub fn at<'c>(&'c mut self, patterns: &[&'p OptPattern], line: &str) -> &'c [&'p OptPattern] {
        // A rule's fixed prefix never contains blank space, so only the
        // first word of the line can decide.
        let space = |c: char| c.is_whitespace() || ('\x1c'..='\x1f').contains(&c);
        let word = line.split(space).next().unwrap_or("");
        if !self.by_word.contains_key(word) {
            let list = patterns.iter().copied().filter(|p| word.starts_with(p.patt.first_prefix())).collect();
            self.by_word.insert(word.to_string(), list);
        }
        &self.by_word[word]
    }
}

/// A match's variables as the evaluator takes them.
pub fn to_values(vars: BTreeMap<String, String>) -> BTreeMap<String, Value> {
    vars.into_iter().map(|(k, v)| (k, Value::Str(v))).collect()
}

/// Tries to match optimization patterns against the given ASM list block,
/// starting at offset `index` within that block. Returns true if there was
/// a match and `asm_list` was changed.
pub fn apply_match<'p>(asm_list: &mut Vec<String>, patterns: impl IntoIterator<Item = &'p OptPattern>, index: usize, ctx: &mut dyn Context) -> bool {
    apply_match_sized(asm_list, patterns, index, ctx).is_some()
}

/// [`apply_match`], saying of a match how many lines were replaced and by
/// how many.
pub fn apply_match_sized<'p>(asm_list: &mut Vec<String>, patterns: impl IntoIterator<Item = &'p OptPattern>, index: usize, ctx: &mut dyn Context) -> Option<(usize, usize)> {
    let mut chars = LineChars::default();
    for p in patterns {
        // HINT: {} is also a valid match
        let Some(found) = p.patt.matches(asm_list, index, &mut chars) else { continue };
        let mut vars = to_values(found);

        for (var, expr) in &p.defines {
            let value = expr.eval(&vars, ctx);
            vars.insert(var.clone(), value);
        }

        if !p.cond.eval(&vars, ctx).truthy() {
            continue;
        }

        // All patterns have matched successfully. Apply this pattern
        let applied = p.template.filter(&vars);
        let sizes = (p.patt.len(), applied.len());
        asm_list.splice(index..index + p.patt.len(), applied);
        return Some(sizes);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::Engine;

    /// The rules as this crate reads them, in the form of the fixtures
    /// `tools/oracle_dump.py --backend` records from the Python compiler.
    fn dump(arch: &str) -> String {
        let engine = Engine::for_arch(arch);
        let mut rows = vec![format!("# maxlen {}", engine.maxlen)];
        for p in &engine.patterns {
            rows.push(format!("rule\t{}\t{}\t{}", p.fname, p.level, p.flag));
            rows.extend(p.patt.patterns.iter().map(|lp| format!("replace\t{}", lp.base.re_pattern)));
            rows.extend(p.template.templates.iter().map(|t| format!("with\t{}", t.base.output.join("\t"))));
            rows.push(format!("if\t{}", p.cond.str_));
            rows.extend(p.defines.iter().map(|(var, expr)| format!("define\t{var}\t{}", expr.str_)));
        }
        rows.join("\n") + "\n"
    }

    #[test]
    fn rules_parse_as_in_python() {
        assert_eq!(dump("z80"), include_str!("../../../data/peephole/z80.rules"));
        assert_eq!(dump("zxnext"), include_str!("../../../data/peephole/zxnext.rules"));
    }

    /// Applies the rules up to `level` over a whole list, the way the
    /// backend's `_output_join` walks it.
    fn run(lines: &[&str], level: i64) -> Vec<String> {
        use super::apply_match;
        use crate::evaluator::BasicContext;
        let engine = Engine::for_arch("z80");
        let mut out: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
        let mut ctx = BasicContext { is_asm: &|_| false, tmp: Default::default() };
        let mut i = 0;
        while i < out.len() {
            if apply_match(&mut out, engine.patterns.iter().filter(|p| p.level <= level), i, &mut ctx) {
                i = i.saturating_sub(engine.maxlen);
            } else {
                i += 1;
            }
        }
        out
    }

    #[test]
    fn rules_apply_as_in_python() {
        // The same walk with the Python engine gives these.
        let t = |lines: &[&str], level: i64, want: &[&str]| assert_eq!(run(lines, level), want, "{lines:?}");
        t(&["push af", "pop af"], 1, &[]);
        t(&["push hl", "pop de"], 1, &["ld d, h", "ld e, l"]);
        t(&["ld a, 5", "push af", "pop af", "ld (x), a"], 1, &["ld a, 5", "ld (x), a"]);
        t(&["ld hl, (_a)", "push hl", "ld hl, (_b)", "pop de", "add hl, de"], 2, &["ld de, (_a)", "ld hl, (_b)", "add hl, de"]);
        t(&["jp nz, .L1", "jp .L2", ".L1:"], 1, &["jp z, .L2", ".L1:"]);
        t(&["ld a, (_x)", "or a", "jp z, .L", "xor a", "jp z, .L2"], 1, &["ld a, (_x)", "or a", "jp z, .L", "jp .L2"]);
        t(&["ld hl, 0", "ld (_a), hl", "pop hl"], 2, &["ld hl, 0", "ld (_a), hl", "pop hl"]);
        t(&["call .core.X", "pop hl"], 2, &["call .core.X", "pop hl"]);
        t(&["neg", "neg"], 1, &[]);
        t(&["ld h, a", "pop af", "or h"], 1, &["pop de", "or d"]);
        t(&["ld a, (hl)", "inc a", "ld (hl), a"], 1, &["inc (hl)"]);
        t(&["sub 1", "jp nc, .A", "or a", "jp z, .B"], 1, &["or a", "jp nz, .A", "or a", "jp z, .B"]);
        // tests/arch/zx48k/optimizer/test_o1_optimizer.py
        t(&["call .core.__LEI8", "sub 1", "sbc a, a", "inc a"], 1, &["call .core.__LEI8"]);
    }
}
