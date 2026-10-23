//! Quads: the intermediate code the front end emits and the backend turns
//! into assembly.
//!
//! As in the Python compiler a quad is an instruction name and up to three
//! operands, all of them text. The backend reads meaning from the text
//! itself: a leading `*` is an indirection, `#` an immediate or address,
//! `$` a local string, `_` a global and `t` a temporary.

use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Quad {
    pub op: String,
    pub args: Vec<String>,
    /// For debug data: which source line it was generated for, as a number
    /// in the front end's table; 0 is none. Not part of the quad as text.
    pub origin: u32,
}

impl Quad {
    pub fn new(op: &str, args: &[&str]) -> Quad {
        Quad { op: op.to_string(), args: args.iter().map(|a| a.to_string()).collect(), origin: 0 }
    }

    /// Parses a line of `zxbc -f ir` output.
    pub fn parse(line: &str) -> Option<Quad> {
        let inner = line.trim().strip_prefix('(')?.strip_suffix(')')?;
        let chars: Vec<char> = inner.chars().collect();
        let mut fields = Vec::new();
        let mut i = 0;
        while i < chars.len() {
            match chars[i] {
                ' ' | ',' => i += 1,
                quote @ ('\'' | '"') => {
                    let mut text = String::new();
                    i += 1;
                    while i < chars.len() && chars[i] != quote {
                        if chars[i] == '\\' && i + 1 < chars.len() {
                            i += 1;
                            match chars[i] {
                                'n' => text.push('\n'),
                                'r' => text.push('\r'),
                                't' => text.push('\t'),
                                'x' => {
                                    let hex: String = chars[i + 1..i + 3].iter().collect();
                                    text.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
                                    i += 2;
                                }
                                other => text.push(other),
                            }
                        } else {
                            text.push(chars[i]);
                        }
                        i += 1;
                    }
                    i += 1;
                    fields.push(text);
                }
                _ => return None,
            }
        }
        let mut fields = fields.into_iter();
        Some(Quad { op: fields.next()?, args: fields.collect(), origin: 0 })
    }
}

/// The form `zxbc -f ir` writes: a Python tuple of strings.
impl fmt::Display for Quad {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({}", zxr_core::pynum::str_repr(&self.op))?;
        if self.args.is_empty() {
            // A tuple of one element has a trailing comma.
            return write!(f, ",)");
        }
        for arg in &self.args {
            write!(f, ", {}", zxr_core::pynum::str_repr(arg))?;
        }
        write!(f, ")")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prints_and_parses_like_python() {
        let quad = Quad::new("call", &[".core.BORDER", "0"]);
        assert_eq!(quad.to_string(), "('call', '.core.BORDER', '0')");
        assert_eq!(Quad::parse(&quad.to_string()), Some(quad));
        assert_eq!(Quad::new("nop", &[]).to_string(), "('nop',)");
        let inline = Quad::new("inline", &["ld a, 'x'\n"]);
        assert_eq!(inline.to_string(), "('inline', \"ld a, 'x'\\n\")");
        assert_eq!(Quad::parse(&inline.to_string()), Some(inline));
    }
}
