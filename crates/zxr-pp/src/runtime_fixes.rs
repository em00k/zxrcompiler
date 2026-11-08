//! Routines of the runtime library that are wrong, and what is read in
//! their place. `lib/` stays a copy of the Python compiler's library: a
//! file is replaced as it is read, only when its text is exactly the one
//! known to be wrong, and not with `ZXR_ORACLE_RUNTIME` in the environment.

/// Path ending, text as the library has it, text read instead.
const FIXES: &[(&str, &str, &str)] = &[
    // An interrupt between `inc sp` and `dec sp` writes over the low word
    // of the operand in the stack.
    ("runtime/swap32.asm", include_str!("../../../lib/arch/zxnext/runtime/swap32.asm"), include_str!("../fixes/swap32.asm")),
    // With no memory for a$ + b$, __ADDSTR returned a$ itself as the new
    // string: the caller then owned, and freed, a$'s block or a literal.
    ("runtime/strcat.asm", include_str!("../../../lib/arch/zxnext/runtime/strcat.asm"), include_str!("../fixes/strcat.asm")),
    // With no memory for a$ = b$, __STRASSIGN returned the address of b$,
    // and the caller stored it in a$. What it returns instead depends on
    // whether that architecture's __REALLOC kept a$'s block (zxnext) or
    // freed it already (zx48k).
    ("zxnext/runtime/strcpy.asm", include_str!("../../../lib/arch/zxnext/runtime/strcpy.asm"), include_str!("../fixes/strcpy_zxnext.asm")),
    // RAND's state moved from its own `ld de, nn` operand into a word of its
    // own, but the load stayed `ld de, 0C0DEh`: every RND after the first
    // returned the same number. zx48k still has the operand form, and is
    // right.
    ("zxnext/runtime/random.asm", include_str!("../../../lib/arch/zxnext/runtime/random.asm"), include_str!("../fixes/random_zxnext.asm")),
    ("zx48k/runtime/strcpy.asm", include_str!("../../../lib/arch/zx48k/runtime/strcpy.asm"), include_str!("../fixes/strcpy_zx48k.asm")),
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
