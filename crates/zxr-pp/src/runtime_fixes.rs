//! Routines of the runtime library that are wrong, and what is read in
//! their place. `lib/` stays a copy of the Python compiler's library: a
//! file is replaced as it is read, only when its text is exactly the one
//! known to be wrong, and not with `ZXR_ORACLE_RUNTIME` in the environment.

/// Path ending, text as the library has it, text read instead.
const FIXES: &[(&str, &str, &str)] = &[
    // An interrupt between `inc sp` and `dec sp` writes over the low word
    // of the operand in the stack.
    ("runtime/swap32.asm", include_str!("../../../lib/arch/zxnext/runtime/swap32.asm"), include_str!("../fixes/swap32.asm")),
];

/// What to read in place of `text`, the contents of `filename`.
pub fn fixed(filename: &str, text: &str) -> Option<&'static str> {
    let filename = filename.replace('\\', "/");
    FIXES.iter().find(|(path, wrong, _)| (filename == *path || filename.ends_with(&format!("/{path}"))) && text == *wrong).map(|(_, _, right)| *right)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_known_text_is_replaced() {
        let (_, wrong, right) = FIXES[0];
        assert_eq!(fixed("/x/lib/arch/zx48k/runtime/swap32.asm", wrong), Some(right));
        assert_eq!(fixed("lib/arch/zxnext/runtime/swap32.asm", "; edited\n"), None);
        assert_eq!(fixed("lib/arch/zxnext/runtime/other.asm", wrong), None);
    }
}
