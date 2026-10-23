//! Python's number formatting. Folded constants are turned into text with
//! `str()` and `repr()`, and that text ends up in generated code.

/// `repr(x)` of a Python float: the shortest digits that read back as the
/// same number, in fixed notation unless the exponent is large.
pub fn float_repr(x: f64) -> String {
    if x.is_nan() {
        return "nan".to_string();
    }
    if x.is_infinite() {
        return if x < 0.0 { "-inf" } else { "inf" }.to_string();
    }
    let sign = if x.is_sign_negative() { "-" } else { "" };
    if x == 0.0 {
        return format!("{sign}0.0");
    }
    // Rust's exponent format gives the shortest round-trip digits.
    let formatted = format!("{:e}", x.abs());
    let (mantissa, exponent) = formatted.split_once('e').unwrap();
    let digits = mantissa.replace('.', "");
    // The value is 0.DIGITS x 10^point.
    let point: i32 = exponent.parse::<i32>().unwrap() + 1;
    let body = if -4 < point && point <= 16 {
        let n = digits.len() as i32;
        if point <= 0 {
            format!("0.{}{digits}", "0".repeat((-point) as usize))
        } else if point >= n {
            format!("{digits}{}.0", "0".repeat((point - n) as usize))
        } else {
            format!("{}.{}", &digits[..point as usize], &digits[point as usize..])
        }
    } else {
        let exp = point - 1;
        let rest = if digits.len() > 1 { format!(".{}", &digits[1..]) } else { String::new() };
        format!("{}{rest}e{}{:02}", &digits[..1], if exp < 0 { '-' } else { '+' }, exp.abs())
    };
    format!("{sign}{body}")
}

/// `repr(s)` of a Python string: single quotes unless the text has one and no
/// double quote, with backslash escapes for what is not printable.
pub fn str_repr(text: &str) -> String {
    let quote = if text.contains('\'') && !text.contains('"') { '"' } else { '\'' };
    let mut out = String::with_capacity(text.len() + 2);
    out.push(quote);
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if (c as u32) < 0x20 || c as u32 == 0x7f => out.push_str(&format!("\\x{:02x}", c as u32)),
            c if (0x80..0xa1).contains(&(c as u32)) || c as u32 == 0xad => out.push_str(&format!("\\x{:02x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

#[cfg(test)]
mod tests {
    use super::{float_repr, str_repr};

    #[test]
    fn matches_python_str_repr() {
        assert_eq!(str_repr("abc"), "'abc'");
        assert_eq!(str_repr("it's"), "\"it's\"");
        assert_eq!(str_repr("a'b\"c"), "'a\\'b\"c'");
        assert_eq!(str_repr("x\ny\\"), "'x\\ny\\\\'");
        assert_eq!(str_repr("\u{1}\u{7f}"), "'\\x01\\x7f'");
        assert_eq!(str_repr("caf\u{e9}"), "'caf\u{e9}'");
    }

    #[test]
    fn matches_python_repr() {
        for (value, expected) in [
            (0.0, "0.0"),
            (-0.0, "-0.0"),
            (1.0, "1.0"),
            (0.1, "0.1"),
            (2.5, "2.5"),
            (100.0, "100.0"),
            (1e15, "1000000000000000.0"),
            (1e16, "1e+16"),
            (1.5e16, "1.5e+16"),
            (0.0001, "0.0001"),
            (0.00001, "1e-05"),
            (1.234e-7, "1.234e-07"),
            (123456789.123, "123456789.123"),
            (-3.75, "-3.75"),
            (1e100, "1e+100"),
            (0.1 + 0.2, "0.30000000000000004"),
            (65535.0, "65535.0"),
        ] {
            assert_eq!(float_repr(value), expected);
        }
    }
}
