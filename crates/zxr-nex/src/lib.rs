//! The NEX packer: a port of `tools/nextcreator.py`, for the directives
//! NextBuild writes into a `.cfg` (`!COR`, `!BMP`, `!MMU`, `!PCSP`, `!BANK`
//! and plain `file,bank,address` lines).
//!
//! A NEX file is a 512-byte header, an optional loading screen, and the 16K
//! banks that hold anything, in the loader's order (5, 2, 0, 1, 3, 4, 6, 7,
//! then 8 upwards).

use std::fs;
use std::path::{Path, PathBuf};

const BANK: usize = 16384;
const BANKS: usize = 112;
const LAYER2: u8 = 1;

/// Where the loader keeps 16K bank `bank` in the file's bank area.
fn real_bank(bank: usize) -> usize {
    if bank > 7 { bank } else { [2, 3, 1, 4, 5, 0, 6, 7][bank] }
}

/// The bank stored at position `index` of the bank area.
fn bank_order(index: usize) -> usize {
    if index > 7 { index } else { [5, 2, 0, 1, 3, 4, 6, 7][index] }
}

/// The bank a file runs on into when it fills `bank`.
fn next_bank(bank: usize) -> usize {
    if bank > 7 { bank + 1 } else { [1, 3, 0, 4, 6, 2, 7, 8][bank] }
}

/// `$1F00`, `1F00h` or decimal.
fn parse_int(text: &str) -> Result<i64, String> {
    let t = text.trim();
    let parsed = if let Some(hex) = t.strip_prefix('$') {
        i64::from_str_radix(hex, 16)
    } else if t.to_lowercase().ends_with('h') {
        i64::from_str_radix(&t[..t.len() - 1], 16)
    } else {
        t.parse()
    };
    parsed.map_err(|_| format!("invalid number '{text}'"))
}

fn int(text: &str) -> Result<i64, String> {
    text.trim().parse().map_err(|_| format!("invalid number '{text}'"))
}

/// Splits on either slash and joins again, which among other things turns an
/// absolute path into a relative one, as the Python tool does.
fn normalize_path(path: &str) -> PathBuf {
    if path == "/" {
        return PathBuf::from(path);
    }
    let mut out = PathBuf::new();
    for part in path.split(['/', '\\']) {
        // os.path.join starts over at an absolute component; an empty one
        // only contributes a separator.
        if !part.is_empty() {
            out.push(part);
        } else if out.as_os_str().is_empty() {
            continue;
        }
    }
    out
}

pub struct Nex {
    /// Directory relative paths in the description are resolved against.
    dir: PathBuf,
    /// One line per thing done, as the Python tool prints them.
    pub log: Vec<String>,
    line: usize,
    current_bank: usize,
    current_address: i64,
    last_bank: Option<usize>,
    file_added: bool,
    /// Bank contents by position in the bank area.
    data: Vec<u8>,
    banks: [u8; BANKS],
    palette: Vec<u8>,
    loading: Vec<u8>,
    loading_screen: u8,
    border_color: u8,
    loading_bar: u8,
    loading_color: u8,
    loading_bank_delay: u8,
    loaded_delay: u8,
    sp: u16,
    pc: u16,
    core_required: Vec<u8>,
    hi_res_colors: u8,
    entry_bank: u8,
    version: &'static str,
}

impl Nex {
    pub fn new(dir: &Path) -> Nex {
        Nex {
            dir: dir.to_path_buf(),
            log: Vec::new(),
            line: 0,
            current_bank: 0,
            current_address: 0,
            last_bank: None,
            file_added: false,
            data: Vec::new(),
            banks: [0; BANKS],
            palette: vec![0; 512],
            loading: vec![0; 49152],
            loading_screen: 0,
            border_color: 0,
            loading_bar: 0,
            loading_color: 0,
            loading_bank_delay: 0,
            loaded_delay: 0,
            sp: 0,
            pc: 0,
            core_required: vec![0, 0, 0],
            hi_res_colors: 0,
            entry_bank: 0,
            version: "V1.1",
        }
    }

    fn read(&self, path: &Path) -> Result<Vec<u8>, String> {
        fs::read(self.dir.join(path)).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Lays a file into memory from the current bank and address, running on
    /// into the following banks.
    fn add_file(&mut self, fname: &str, bank: Option<&str>, address: Option<&str>) -> Result<(), String> {
        if fname.chars().count() < 4 {
            return Err(format!("Wrong filename \"{fname}\" at line {}", self.line));
        }
        if fname.ends_with("sna") {
            return Err(format!("snapshot files are not supported: {fname}"));
        }
        if let Some(bank) = bank {
            self.current_bank = usize::try_from(int(bank)?).ok().filter(|&b| b < BANKS).ok_or_else(|| format!("bank {bank} is out of range (0-{})", BANKS - 1))?;
        }
        if let Some(address) = address {
            self.current_address = parse_int(address)?;
        }
        let mut path = normalize_path(fname);
        // A name with no match as written is tried against the entries of
        // the working directory, lower-cased.
        if let (Some(base), Ok(entries)) = (path.file_name().map(|n| n.to_string_lossy().into_owned()), fs::read_dir(&self.dir)) {
            let mut names: Vec<String> = entries.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
            names.sort();
            if let Some(actual) = names.iter().find(|n| n.to_lowercase() == base) {
                path.set_file_name(actual);
            }
        }
        let content = self.read(&path)?;
        let mut rest = &content[..];
        while !rest.is_empty() {
            if self.current_bank >= BANKS {
                return Err(format!("\"{fname}\" runs past bank {}, the last a NEX file holds", BANKS - 1));
            }
            let real = real_bank(self.current_bank);
            let within = (self.current_address & 16383) as usize;
            let length = (BANK - within).min(rest.len());
            let offset = real * BANK + within;
            if self.data.len() < offset + length {
                self.data.resize((real + 1) * BANK, 0);
            }
            self.data[offset..offset + length].copy_from_slice(&rest[..length]);
            rest = &rest[length..];
            if self.current_bank < BANKS {
                self.banks[self.current_bank] = 1;
            }
            self.log.push(format!("bank={},addr={:04x},realbank={real},{length}", self.current_bank, self.current_address));
            if self.last_bank.is_none_or(|last| real > last) {
                self.last_bank = Some(real);
            }
            self.current_address = ((self.current_address & 49152) + 16384) & 49152;
            self.current_bank = next_bank(self.current_bank);
        }
        Ok(())
    }

    /// `!MMUfile,page8k,address`: a file placed by 8K page.
    fn load_mmu(&mut self, filename: &str, bank8k: Option<&str>, address8k: Option<&str>) -> Result<(), String> {
        let bank8k = bank8k.map(int).transpose()?;
        if let Some(bank) = bank8k {
            self.current_bank = (bank >> 1) as usize;
        }
        if let Some(address) = address8k {
            self.current_address = parse_int(address)?;
            // The odd page of a pair is the upper half of its 16K bank.
            if bank8k != Some((self.current_bank as i64) << 1) {
                self.current_address += 8192;
            }
        }
        let shown = bank8k.map_or("None".to_string(), |b| b.to_string());
        self.log.push(format!("File '{filename}' 8K bank {shown}(16K bank {}, {:04x})", self.current_bank, self.current_address));
        self.add_file(filename, None, None)
    }

    /// `!BMP`: a 256-colour bitmap as the Layer 2 loading screen.
    fn load_bmp(&mut self, filename: &str, dont_save_palette: bool, params: &[&str]) -> Result<(), String> {
        let content = self.read(Path::new(filename))?;
        let byte = |i: usize| content.get(i).copied().unwrap_or(0) as u32;
        let level = |v: u32| if v > 255 { 255 } else { v >> 5 };
        self.palette = (0..256)
            .flat_map(|i| {
                let at = 54 + i * 4;
                let value = ((level(byte(at)) >> 2) & (1 << 8)) + (level(byte(at)) >> 1) + (level(byte(at + 1)) << 2) + (level(byte(at + 2)) << 5);
                [value as u8, (value >> 8) as u8]
            })
            .collect();
        let pixels = (byte(10) + (byte(11) << 8)) as usize;
        // A positive height means the rows are stored bottom up.
        let bottom_up = byte(25) < 128;
        for row in 0..192 {
            let offset = 256 * if bottom_up { 191 - row } else { row };
            for x in 0..256 {
                self.loading[offset + x] = content.get(pixels + row * 256 + x).copied().unwrap_or(0);
            }
        }
        self.loading_screen |= LAYER2 + if dont_save_palette { 128 } else { 0 };
        self.log.push(format!("Loading Screen '{filename}'"));
        self.file_added = true;
        let set = |slot: &mut u8, index: usize| -> Result<(), String> {
            if let Some(text) = params.get(index) {
                *slot = int(text)? as u8;
            }
            Ok(())
        };
        set(&mut self.border_color, 0)?;
        set(&mut self.loading_bar, 1)?;
        set(&mut self.loading_color, 2)?;
        set(&mut self.loading_bank_delay, 3)?;
        set(&mut self.loaded_delay, 4)?;
        Ok(())
    }

    fn set_entry_bank(&mut self, bank: &str) -> Result<(), String> {
        self.entry_bank = int(bank)? as u8;
        if self.entry_bank > 0 && self.version == "V1.1" {
            self.version = "V1.2";
            self.log.push(format!("Entry Bank={}", self.entry_bank));
        }
        Ok(())
    }

    /// Reads a `.cfg` description.
    pub fn parse(&mut self, text: &str) -> Result<(), String> {
        self.line = 0;
        self.current_bank = 0;
        self.current_address = 0;
        for raw in text.lines() {
            self.line += 1;
            let line = raw.trim();
            if line.is_empty() || line.starts_with(';') {
                continue;
            }
            let number = self.line;
            let at = |e: String| format!("line {number}: {e}");
            if !line.starts_with('!') {
                let fields: Vec<&str> = line.split(',').map(str::trim).collect();
                self.add_file(fields[0], fields.get(1).copied(), fields.get(2).copied()).map_err(at)?;
            } else if let Some(rest) = line.strip_prefix("!COR") {
                self.core_required = rest.split(',').map(|x| int(x).map(|v| v as u8)).collect::<Result<_, _>>().map_err(at)?;
                let c = |i: usize| self.core_required.get(i).copied().unwrap_or(0);
                self.log.push(format!("Requires Core {}.{}.{} or greater", c(0), c(1), c(2)));
            } else if let Some(mut rest) = line.strip_prefix("!BMP") {
                if let Some(colour) = rest.strip_prefix(',') {
                    int(colour).map_err(at)?;
                    continue;
                }
                let dont_save = rest.starts_with('!');
                if dont_save {
                    rest = &rest[1..];
                }
                if let Some(after) = rest.strip_prefix('8') {
                    rest = after.strip_prefix(',').unwrap_or(after);
                }
                let fields: Vec<&str> = rest.split(',').collect();
                self.load_bmp(fields[0], dont_save, &fields[1..]).map_err(at)?;
            } else if let Some(rest) = line.strip_prefix("!PCSP") {
                let fields: Vec<&str> = rest.split(',').collect();
                self.pc = parse_int(fields[0]).map_err(at)? as u16;
                self.log.push(format!("PC=${:04x}", self.pc));
                if let Some(sp) = fields.get(1) {
                    self.sp = parse_int(sp).map_err(at)? as u16;
                    self.log.push(format!("SP=${:04x}", self.sp));
                }
                if let Some(bank) = fields.get(2) {
                    self.set_entry_bank(bank).map_err(at)?;
                }
            } else if let Some(rest) = line.strip_prefix("!MMU") {
                let fields: Vec<&str> = rest.split(',').collect();
                self.load_mmu(fields[0], fields.get(1).copied(), fields.get(2).copied()).map_err(at)?;
            } else if let Some(rest) = line.strip_prefix("!BANK") {
                self.set_entry_bank(rest).map_err(at)?;
            } else if ["!SCR", "!SLR", "!SHR", "!SHC"].iter().any(|d| line.starts_with(d)) {
                return Err(at(format!("loading screen directive not supported: {line}")));
            }
        }
        Ok(())
    }

    /// The finished file, or `None` if nothing was added to it.
    pub fn generate(&mut self, ram_required: Option<u8>) -> Option<Vec<u8>> {
        if self.last_bank.is_none() && !self.file_added {
            return None;
        }
        self.log.push(format!("Generating NEX file in {} format", self.version));
        let loaded = self.banks.iter().filter(|&&b| b > 0).count() as u8;
        // Anything from bank 48 up needs the 2MB machine.
        let ram = ram_required.map_or(self.banks[48..].iter().any(|&b| b > 0) as u8, |mb| mb.wrapping_sub(1));
        self.log.push(format!("Generating NEX file for {}MB machine", ram as u32 + 1));

        let mut out = Vec::with_capacity(512 + self.data.len());
        out.extend(b"Next");
        out.extend(self.version.as_bytes());
        out.extend([ram, loaded, self.loading_screen, self.border_color]);
        out.extend(self.sp.to_le_bytes());
        out.extend(self.pc.to_le_bytes());
        out.extend(0u16.to_le_bytes());
        out.extend(self.banks);
        out.extend([self.loading_bar, self.loading_color, self.loading_bank_delay, self.loaded_delay, 0]);
        out.extend((0..3).map(|i| self.core_required.get(i).copied().unwrap_or(0)));
        out.extend([self.hi_res_colors, self.entry_bank]);
        out.resize(512, 0);

        if self.loading_screen & LAYER2 != 0 {
            if self.loading_screen & 128 == 0 {
                out.extend(&self.palette);
            }
            out.extend(&self.loading);
        }
        for index in 0..BANKS {
            if self.banks[bank_order(index)] != 0 {
                let start = index * BANK;
                let have = self.data.len().saturating_sub(start).min(BANK);
                out.extend(&self.data[start.min(self.data.len())..start.min(self.data.len()) + have]);
                out.extend(std::iter::repeat_n(0, BANK - have));
            }
        }
        Some(out)
    }
}

/// Packs the description in `cfg` into `output`. Relative paths are taken
/// from the current directory, as with the Python tool.
pub fn pack(cfg: &Path, output: &Path, ram_required: Option<u8>) -> Result<Vec<String>, String> {
    let text = fs::read_to_string(cfg).map_err(|e| format!("{}: {e}", cfg.display()))?;
    let mut nex = Nex::new(Path::new("."));
    nex.parse(&text)?;
    if let Some(bytes) = nex.generate(ram_required) {
        let path = normalize_path(&output.to_string_lossy());
        fs::write(&path, bytes).map_err(|_| format!("Can't open file '{}'. Nothing output", output.display()))?;
    }
    Ok(nex.log)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bank_maps_are_inverse() {
        for bank in 0..8 {
            assert_eq!(bank_order(real_bank(bank)), bank);
        }
        assert_eq!(real_bank(40), 40);
        assert_eq!(next_bank(5), 2);
        assert_eq!(next_bank(7), 8);
    }

    #[test]
    fn numbers() {
        assert_eq!(parse_int("$1C00"), Ok(0x1C00));
        assert_eq!(parse_int("1C00h"), Ok(0x1C00));
        assert_eq!(parse_int(" 42 "), Ok(42));
        assert!(parse_int("0x10").is_err());
    }
}
