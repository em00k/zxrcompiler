//! Reading source files the way the Python compiler does: UTF-8 (with or
//! without a byte order mark), falling back to Windows-1252.

/// Code points of bytes 0x80..=0x9F in Windows-1252; zero where undefined.
const CP1252_HIGH: [u32; 32] = [
    0x20AC, 0, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039, 0x0152, 0, 0x017D, 0,
    0, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0, 0x017E, 0x0178,
];

fn decode_cp1252(bytes: &[u8]) -> Option<String> {
    bytes
        .iter()
        .map(|&b| match b {
            0x80..=0x9F => char::from_u32(CP1252_HIGH[(b - 0x80) as usize]).filter(|&c| c != '\0'),
            _ => Some(b as char),
        })
        .collect()
}

pub const ENCODINGS: &str = "utf-8-sig, cp1252";

/// Decodes file contents, or `None` if they are valid in neither encoding.
pub fn decode(bytes: &[u8]) -> Option<String> {
    match std::str::from_utf8(bytes) {
        Ok(text) => Some(text.strip_prefix('\u{feff}').unwrap_or(text).to_string()),
        Err(_) => decode_cp1252(bytes),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_with_and_without_bom() {
        assert_eq!(decode(b"abc").as_deref(), Some("abc"));
        assert_eq!(decode(b"\xef\xbb\xbfabc").as_deref(), Some("abc"));
        assert_eq!(decode("é".as_bytes()).as_deref(), Some("é"));
    }

    #[test]
    fn cp1252_fallback() {
        assert_eq!(decode(b"caf\xe9").as_deref(), Some("café"));
        assert_eq!(decode(b"\x80").as_deref(), Some("€"));
        assert_eq!(decode(b"\x81"), None);
    }
}
