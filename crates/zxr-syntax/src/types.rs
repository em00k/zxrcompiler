//! BASIC's types, and numbers with the arithmetic of the Python values the
//! original compiler folds constants with.

use std::fmt;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub enum BasicType {
    Unknown,
    Byte,
    UByte,
    Integer,
    UInteger,
    Long,
    ULong,
    Fixed,
    Float,
    String,
    Boolean,
}

impl BasicType {
    pub const ALL: [BasicType; 11] = [
        BasicType::Unknown,
        BasicType::Byte,
        BasicType::UByte,
        BasicType::Integer,
        BasicType::UInteger,
        BasicType::Long,
        BasicType::ULong,
        BasicType::Fixed,
        BasicType::Float,
        BasicType::String,
        BasicType::Boolean,
    ];

    pub fn name(self) -> &'static str {
        match self {
            BasicType::Unknown => "unknown",
            BasicType::Byte => "byte",
            BasicType::UByte => "ubyte",
            BasicType::Integer => "integer",
            BasicType::UInteger => "uinteger",
            BasicType::Long => "long",
            BasicType::ULong => "ulong",
            BasicType::Fixed => "fixed",
            BasicType::Float => "float",
            BasicType::String => "string",
            BasicType::Boolean => "boolean",
        }
    }

    pub fn by_name(name: &str) -> Option<BasicType> {
        Self::ALL.into_iter().find(|t| t.name() == name)
    }

    /// Size in bytes; a string is a pointer.
    pub fn size(self) -> i64 {
        match self {
            BasicType::Unknown => 0,
            BasicType::Byte | BasicType::UByte | BasicType::Boolean => 1,
            BasicType::Integer | BasicType::UInteger | BasicType::String => 2,
            BasicType::Long | BasicType::ULong | BasicType::Fixed => 4,
            BasicType::Float => 5,
        }
    }

    pub fn is_signed(self) -> bool {
        matches!(self, BasicType::Byte | BasicType::Integer | BasicType::Long | BasicType::Fixed | BasicType::Float)
    }

    pub fn is_unsigned(self) -> bool {
        matches!(self, BasicType::Boolean | BasicType::UByte | BasicType::UInteger | BasicType::ULong)
    }

    pub fn is_integral(self) -> bool {
        matches!(self, BasicType::Boolean | BasicType::Byte | BasicType::UByte | BasicType::Integer | BasicType::UInteger | BasicType::Long | BasicType::ULong)
    }

    pub fn is_decimal(self) -> bool {
        matches!(self, BasicType::Fixed | BasicType::Float)
    }

    pub fn is_numeric(self) -> bool {
        self.is_integral() || self.is_decimal()
    }

    pub fn to_signed(self) -> BasicType {
        match self {
            BasicType::Boolean | BasicType::UByte => BasicType::Byte,
            BasicType::UInteger => BasicType::Integer,
            BasicType::ULong => BasicType::Long,
            t if t.is_signed() => t,
            _ => BasicType::Unknown,
        }
    }
}

/// A type as attached to an expression or identifier. Two are equal when
/// they come down to the same basic type; whether the type was written out or
/// assumed, and where, rides along.
#[derive(Clone, Copy, Debug)]
pub struct Ty {
    pub basic: BasicType,
    /// Assumed rather than declared (`DIM a` with no `AS`).
    pub implicit: bool,
    pub lineno: u32,
}

impl Ty {
    pub const fn new(basic: BasicType) -> Ty {
        Ty { basic, implicit: false, lineno: 0 }
    }

    /// A reference to a type from a line of source.
    pub fn reference(basic: BasicType, lineno: u32, implicit: bool) -> Ty {
        Ty { basic, implicit, lineno }
    }

    pub fn name(&self) -> &'static str {
        self.basic.name()
    }

    pub fn size(&self) -> i64 {
        self.basic.size()
    }

    /// Strings live on the heap.
    pub fn is_dynamic(&self) -> bool {
        self.basic == BasicType::String
    }
}

impl PartialEq for Ty {
    fn eq(&self, other: &Ty) -> bool {
        self.basic == other.basic
    }
}

impl PartialEq<BasicType> for Ty {
    fn eq(&self, other: &BasicType) -> bool {
        self.basic == *other
    }
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// The type two operands are brought to before an operation.
pub fn common_type(a: BasicType, b: BasicType) -> BasicType {
    use BasicType::*;
    if a == b {
        return a;
    }
    if a == Unknown {
        return b;
    }
    if b == Unknown {
        return a;
    }
    if a == Float || b == Float {
        return Float;
    }
    if a == Fixed || b == Fixed {
        return Fixed;
    }
    if a == String || b == String {
        return Unknown;
    }
    let result = if a.size() > b.size() { a } else { b };
    if !a.is_unsigned() || !b.is_unsigned() { result.to_signed() } else { result }
}

/// A number as Python holds it: an integer of any size, or a float.
#[derive(Clone, Copy, Debug)]
pub enum Num {
    Int(i128),
    Float(f64),
}

impl Num {
    /// A value that is whole becomes an integer, as in `SymbolNUMBER`.
    pub fn normalised(self) -> Num {
        match self {
            Num::Float(f) if f.is_finite() && f.fract() == 0.0 && f.abs() < 1.0e38 => Num::Int(f as i128),
            other => other,
        }
    }

    pub fn as_f64(self) -> f64 {
        match self {
            Num::Int(i) => i as f64,
            Num::Float(f) => f,
        }
    }

    pub fn is_zero(self) -> bool {
        match self {
            Num::Int(i) => i == 0,
            Num::Float(f) => f == 0.0,
        }
    }

    /// `math.floor(value)`.
    pub fn floor(self) -> i128 {
        match self {
            Num::Int(i) => i,
            Num::Float(f) => f.floor() as i128,
        }
    }

    /// The type a literal of this value gets when none is given.
    pub fn literal_type(self) -> BasicType {
        match self {
            Num::Float(f) if -32768.0 < f && f < 32767.0 => BasicType::Fixed,
            Num::Float(_) => BasicType::Float,
            Num::Int(i) if (0..256).contains(&i) => BasicType::UByte,
            Num::Int(i) if (-128..128).contains(&i) => BasicType::Byte,
            Num::Int(i) if (0..65536).contains(&i) => BasicType::UInteger,
            Num::Int(i) if (-32768..32768).contains(&i) => BasicType::Integer,
            Num::Int(i) if i < 0 => BasicType::Long,
            Num::Int(_) => BasicType::ULong,
        }
    }
}

impl PartialEq for Num {
    fn eq(&self, other: &Num) -> bool {
        match (self, other) {
            (Num::Int(a), Num::Int(b)) => a == b,
            _ => self.as_f64() == other.as_f64(),
        }
    }
}

impl PartialOrd for Num {
    fn partial_cmp(&self, other: &Num) -> Option<std::cmp::Ordering> {
        match (self, other) {
            (Num::Int(a), Num::Int(b)) => a.partial_cmp(b),
            _ => self.as_f64().partial_cmp(&other.as_f64()),
        }
    }
}

/// `str()` of the value: digits for an integer, `repr` for a float.
impl fmt::Display for Num {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Num::Int(i) => write!(f, "{i}"),
            Num::Float(x) => f.write_str(&zxr_core::pynum::float_repr(*x)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_types() {
        assert_eq!(Num::Int(0).literal_type(), BasicType::UByte);
        assert_eq!(Num::Int(255).literal_type(), BasicType::UByte);
        assert_eq!(Num::Int(-1).literal_type(), BasicType::Byte);
        assert_eq!(Num::Int(256).literal_type(), BasicType::UInteger);
        assert_eq!(Num::Int(-129).literal_type(), BasicType::Integer);
        assert_eq!(Num::Int(65536).literal_type(), BasicType::ULong);
        assert_eq!(Num::Int(-32769).literal_type(), BasicType::Long);
        assert_eq!(Num::Float(0.5).literal_type(), BasicType::Fixed);
        assert_eq!(Num::Float(40000.5).literal_type(), BasicType::Float);
        assert_eq!(Num::Float(2.0).normalised().literal_type(), BasicType::UByte);
    }

    #[test]
    fn common_types() {
        use BasicType::*;
        assert_eq!(common_type(UByte, UByte), UByte);
        assert_eq!(common_type(UByte, UInteger), UInteger);
        assert_eq!(common_type(UByte, Byte), Byte);
        assert_eq!(common_type(UInteger, Byte), Integer);
        assert_eq!(common_type(ULong, Integer), Long);
        assert_eq!(common_type(Long, Fixed), Fixed);
        assert_eq!(common_type(Fixed, Float), Float);
        assert_eq!(common_type(String, UByte), Unknown);
        assert_eq!(common_type(Unknown, UByte), UByte);
    }
}
