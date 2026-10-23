//! `api/fp.py`: a number as the five bytes of a ZX Spectrum float.

/// ASCII 32 bit binary representation of a number.
fn bin32(f: i128) -> String {
    format!("{:032b}", f & 0xFFFF_FFFF)
}

/// Binary representation of a mantissa x (x is a float).
fn bindec32(f: f64) -> String {
    let mut result = "0".to_string();
    let mut a = f;

    if f >= 1.0 {
        result = bin32(crate::common::float_to_int(f));
    }

    result.push('.');
    let mut c = crate::common::float_to_int(a) as f64;

    for _ in 0..32 {
        a -= c;
        a *= 2.0;
        c = crate::common::float_to_int(a) as f64;
        result.push_str(&format!("{}", c as i64));
    }

    result
}

/// Returns a floating point number as (Mantissa, EXP+128), both in binary.
pub fn fp(x: f64) -> (String, String) {
    let mut e: i128 = 0; // exponent
    let s = if x < 0.0 { 1 } else { 0 }; // sign
    let mut m = x.abs(); // mantissa

    // Python halves an infinity for ever here.
    assert!(!m.is_infinite(), "OverflowError: an infinity has no ZX Spectrum representation");

    while m >= 1.0 {
        m /= 2.0;
        e += 1;
    }

    while 0.0 < m && m < 0.5 {
        m *= 2.0;
        e -= 1;
    }

    let digits = bindec32(m);
    let mantissa = format!("{s}{}", &digits[3..]);
    let exponent = if x != 0.0 { bin32(e + 128)[24..].to_string() } else { bin32(0)[24..].to_string() };

    (mantissa, exponent)
}

/// Returns C DE HL as values for loading an immediate floating point.
pub fn immediate_float(x: f64) -> (String, String, String) {
    let bin2hex = |y: &str| format!("{:02X}", u32::from_str_radix(y, 2).unwrap());

    let (m, e) = fp(x);

    let c = format!("0{}h", bin2hex(&e));
    let ed = format!("0{}{}h", bin2hex(&m[8..16]), bin2hex(&m[..8]));
    let lh = format!("0{}{}h", bin2hex(&m[24..]), bin2hex(&m[16..24]));

    (c, ed, lh)
}

#[cfg(test)]
mod tests {
    use super::immediate_float;

    #[test]
    fn matches_python() {
        // [immediate_float(x) for x in ...] in the Python compiler.
        let t = |x: f64, c: &str, ed: &str, lh: &str| assert_eq!(immediate_float(x), (c.to_string(), ed.to_string(), lh.to_string()), "{x}");
        t(0.0, "000h", "00000h", "00000h");
        t(1.0, "081h", "00000h", "00000h");
        t(-1.0, "081h", "00080h", "00000h");
        t(0.5, "080h", "00000h", "00000h");
        t(3.14159, "082h", "00F49h", "080CFh");
        t(-2.5, "082h", "000A0h", "00000h");
        t(65535.0, "090h", "0FF7Fh", "00000h");
        t(1e-10, "05Fh", "0E65Bh", "0CEFEh");
        t(123456789.123, "09Bh", "0796Bh", "0A3A2h");
        t(1e38, "0FFh", "07616h", "05099h");
        // The exponent byte wraps.
        t(1e39, "002h", "0143Ch", "0A43Fh");
        t(1e-40, "0FCh", "0610Bh", "03B31h");
        t(-0.0, "000h", "00000h", "00000h");
    }
}
