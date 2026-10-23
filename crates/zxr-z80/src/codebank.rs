//! ZX Spectrum Next banked-code (CODEBANK) support: `backend/codebank.py`
//! and the layout helpers of `api/codebank_options.py`.
//!
//! A SUB/FUNCTION may be assigned to a *code bank*. Its body is emitted into
//! a separate output segment assembled at the code-window address, and a
//! small resident trampoline is emitted under the routine's ordinary mangled
//! label. All of that is decided by the front end; what is left for the
//! backend is the resident data the far-call runtime needs, which goes in
//! the prologue, and the registries that data is built from.

use crate::common::{Options, NAMESPACE};
use std::collections::BTreeSet;

/// Suffix of the label under which a banked routine's real body is emitted.
pub const FAR_BODY_SUFFIX: &str = ".__far";
/// Suffix of the alias label beside bank-local storage a FARPTR names.
pub const FAR_ADDR_SUFFIX: &str = ".__faraddr";

/// 8K page normally mapped at each MMU slot on a stock machine. Only used
/// as the static default for logical bank 0; `__FAR_INIT` overwrites it at
/// startup with whatever the loader really had mapped.
const DEFAULT_SLOT_PAGE: [i64; 8] = [0xFF, 0xFF, 10, 11, 4, 5, 0, 1];

/// The registries the front end fills as it decides what goes in a bank.
#[derive(Default, Debug)]
pub struct CodeBank {
    /// A resident trampoline label (`_Foo`) and the real banked body label
    /// (`_Foo.__far`), in the order they were registered. Consumed by the
    /// flow optimiser's call-graph builder.
    pub far_trampolines: Vec<(String, String)>,
    /// Logical code banks that actually hold something.
    pub banks_used: BTreeSet<i64>,
    /// Logical code banks holding bank-local variables or arrays.
    pub banks_with_data: BTreeSet<i64>,
    /// Banks already warned about for holding data but no routine.
    pub no_routine_warned: BTreeSet<i64>,
    /// The far memory routines are in the program, so a bank of nothing but
    /// data can be reached.
    pub far_memory_used: bool,
}

impl CodeBank {
    /// True once at least one banked routine or bank-local variable exists.
    pub fn is_banking_active(&self) -> bool {
        !self.far_trampolines.is_empty() || !self.banks_with_data.is_empty()
    }

    /// The real body label of a trampoline, if `label` is one.
    pub fn far_body(&self, label: &str) -> Option<&str> {
        self.far_trampolines.iter().find(|(l, _)| l == label).map(|(_, body)| body.as_str())
    }

    /// Resident data and constants the far-call runtime needs. Emitted from
    /// `emit_prologue()`, which runs after every variable pass, so both
    /// registries are complete. Names are fully qualified because the
    /// prologue sits outside the `core` namespace.
    pub fn emit_prologue_data(&self, options: &Options) -> Vec<String> {
        if !self.is_banking_active() {
            return Vec::new();
        }

        let slot = options.codewindow >> 13;
        let slots = window_slots(options);
        // max() of an empty set is a ValueError in Python; it cannot be
        // empty here unless the registries disagree.
        let highest = *self.banks_used.union(&self.banks_with_data).max().expect("ValueError: max() arg is an empty sequence");

        // One byte per logical bank for a single-slot window, two for a 16K
        // one. The boot row cannot be derived from its first byte: the stock
        // mapping is (.., 10, 11, 4, 5, ..), so a window at $6000 boots with
        // pages 11 and 4, which are not consecutive even though banked pairs
        // always are.
        let mut pages: Vec<i64> = (0..slots).map(|i| DEFAULT_SLOT_PAGE[((slot + i) & 7) as usize]).collect();
        for bank in 1..=highest {
            let first = physical_page(options, bank);
            pages.extend((0..slots).map(|i| first + i));
        }

        let mut output = vec!["; --- ZX Next banked code (CODEBANK) ---".to_string(), format!("{NAMESPACE}.__FAR_MMU_REG EQU {}", window_mmu_reg(options))];

        if slots > 1 {
            // A separate EQU rather than `__FAR_MMU_REG + 1` in the nextreg
            // operand, so the runtime does not depend on the assembler
            // folding an expression into an immediate.
            output.push(format!("{NAMESPACE}.__FAR_MMU_REG2 EQU {}", window_mmu_reg(options) + 1));
        }

        output.push(format!("{NAMESPACE}.__FAR_STACK_SIZE EQU {}", 3 * options.codebankdepth));
        output.push(format!("{NAMESPACE}.__CODE_BANK_TABLE:"));
        output.push(format!("DEFB {}", pages.iter().map(i64::to_string).collect::<Vec<_>>().join(", ")));
        output
    }
}

/// Label of the real body of a banked routine, as opposed to its trampoline.
pub fn banked_label(mangled: &str) -> String {
    format!("{mangled}{FAR_BODY_SUFFIX}")
}

/// NextReg number of the first MMU slot covering the code window ($50..$57).
pub fn window_mmu_reg(options: &Options) -> i64 {
    0x50 + (options.codewindow >> 13)
}

/// Number of 8K MMU slots the code window spans: 1 or 2.
pub fn window_slots(options: &Options) -> i64 {
    if options.codewindowsize >= 16384 { 2 } else { 1 }
}

/// Python's `int(text, 0)`: the base is read from the prefix.
fn int_any_base(text: &str) -> i64 {
    let t = text.trim();
    let (negative, body) = match t.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    let lower = body.to_ascii_lowercase();
    let parsed = if let Some(hex) = lower.strip_prefix("0x") {
        i64::from_str_radix(&hex.replace('_', ""), 16)
    } else if let Some(oct) = lower.strip_prefix("0o") {
        i64::from_str_radix(&oct.replace('_', ""), 8)
    } else if let Some(bin) = lower.strip_prefix("0b") {
        i64::from_str_radix(&bin.replace('_', ""), 2)
    } else if lower.len() > 1 && lower.starts_with('0') && lower.bytes().any(|b| b != b'0' && b != b'_') {
        // A decimal literal cannot have leading zeros.
        "".parse()
    } else {
        lower.replace('_', "").parse()
    };
    let value = parsed.unwrap_or_else(|_| panic!("ValueError: invalid literal for int() with base 0: '{text}'"));
    if negative { -value } else { value }
}

/// First physical 8K page backing a logical code bank. A bank occupies
/// `window_slots()` consecutive pages; `codebankpages` lists the *first*
/// page of each bank.
pub fn physical_page(options: &Options, logical_bank: i64) -> i64 {
    if logical_bank <= 0 {
        return 0;
    }

    let stride = window_slots(options);
    let pages: Vec<&str> = options.codebankpages.split(',').map(str::trim).filter(|p| !p.is_empty()).collect();
    if !pages.is_empty() {
        if logical_bank as usize <= pages.len() {
            return int_any_base(pages[logical_bank as usize - 1]);
        }
        // Past the end of an explicit list, keep allocating sequentially
        return int_any_base(pages[pages.len() - 1]) + (logical_bank - pages.len() as i64) * stride;
    }

    options.codebankbase + (logical_bank - 1) * stride
}
