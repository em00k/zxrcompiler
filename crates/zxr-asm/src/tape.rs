//! Tape images (`.tap`, `.tzx`) and the BASIC loader that goes in front of
//! the code.

/// A tokenised Sinclair BASIC program.
#[derive(Default)]
pub struct Basic {
    pub bytes: Vec<u8>,
    current_line: u16,
}

pub enum Part {
    /// Text copied as is.
    Literal(&'static str),
    Number(i128),
    /// A keyword inside a statement.
    Token(u8),
}

pub const CLEAR: u8 = 253;
pub const LOAD: u8 = 239;
pub const RANDOMIZE: u8 = 249;
pub const REM: u8 = 234;
pub const CODE: u8 = 175;
pub const USR: u8 = 192;

impl Basic {
    /// A number as BASIC stores it: its digits, then the hidden binary form.
    fn number(value: i128) -> Vec<u8> {
        let mut out: Vec<u8> = value.to_string().bytes().collect();
        out.push(14);
        let sign = if value < 0 { 255 } else { 0 };
        out.extend([0, sign, value as u8, (value >> 8) as u8, 0]);
        out
    }

    /// Adds a line made of statements, each a keyword followed by its parts.
    pub fn add_line(&mut self, statements: &[(u8, Vec<Part>)]) {
        self.current_line += 10;
        let mut body = Vec::new();
        for (i, (keyword, parts)) in statements.iter().enumerate() {
            if i > 0 {
                body.push(b':');
            }
            body.push(*keyword);
            for part in parts {
                match part {
                    Part::Literal(text) => body.extend(text.bytes()),
                    Part::Number(value) => body.extend(Self::number(*value)),
                    Part::Token(token) => body.push(*token),
                }
            }
        }
        body.push(13);
        self.bytes.extend(self.current_line.to_be_bytes());
        self.bytes.extend((body.len() as u16).to_le_bytes());
        self.bytes.extend(body);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Tap,
    Tzx,
}

pub struct Tape {
    format: Format,
    pub output: Vec<u8>,
}

impl Tape {
    pub fn new(format: Format) -> Tape {
        let output = match format {
            Format::Tap => Vec::new(),
            // Signature, end-of-text marker, version 1.21.
            Format::Tzx => [b"ZXTape!".as_slice(), &[26, 1, 21]].concat(),
        };
        Tape { format, output }
    }

    pub fn standard_block(&mut self, bytes: &[u8]) {
        if self.format == Format::Tzx {
            // Block type 0x10, then a pause of 1000 ms.
            self.output.push(16);
            self.output.extend(1000u16.to_le_bytes());
        }
        self.output.extend(((bytes.len() + 1) as u16).to_le_bytes());
        self.output.extend(bytes);
        self.output.push(bytes.iter().fold(0, |sum, b| sum ^ b));
    }

    fn header(&mut self, kind: u8, title: &str, length: usize, param1: i128, param2: i128) {
        let mut bytes = vec![0, kind];
        // Ten characters, padded with spaces; a character beyond Latin-1
        // keeps its low byte.
        bytes.extend(title.chars().chain(std::iter::repeat(' ')).take(10).map(|c| c as u32 as u8));
        bytes.extend((length as u16).to_le_bytes());
        bytes.extend((param1 as u16).to_le_bytes());
        bytes.extend((param2 as u16).to_le_bytes());
        self.standard_block(&bytes);
    }

    fn data(&mut self, bytes: &[u8]) {
        let mut block = vec![255];
        block.extend(bytes);
        self.standard_block(&block);
    }

    pub fn save_code(&mut self, title: &str, address: i128, bytes: &[u8]) {
        self.header(3, title, bytes.len(), address, 32768);
        self.data(bytes);
    }

    pub fn save_program(&mut self, title: &str, bytes: &[u8], line: i128) {
        self.header(0, title, bytes.len(), line, bytes.len() as i128);
        self.data(bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loader_line() {
        let mut basic = Basic::default();
        basic.add_line(&[(CLEAR, vec![Part::Number(32767)])]);
        assert_eq!(basic.bytes, [0, 10, 13, 0, 253, b'3', b'2', b'7', b'6', b'7', 14, 0, 0, 0xFF, 0x7F, 0, 13]);
    }

    #[test]
    fn tap_block_has_length_and_checksum() {
        let mut tape = Tape::new(Format::Tap);
        tape.standard_block(&[1, 2, 3]);
        assert_eq!(tape.output, [4, 0, 1, 2, 3, 0]);
    }
}
