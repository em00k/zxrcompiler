//! The option registry: `src/api/options.py` and the options `src/api/config.py`,
//! `src/api/codebank_options.py` and the backend's `init()` register in it.
//!
//! The Python compiler keeps every option in one container, `OPTIONS`, in
//! which each has a name, a type and a stack of saved values. `#pragma name =
//! value` assigns any of them by name, and `#pragma push(name)` / `pop(name)`
//! save and restore one, so the registry is kept as data rather than as the
//! fields of a struct.

use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub enum OptValue {
    None,
    Bool(bool),
    Int(i128),
    Str(String),
}

impl OptValue {
    /// Python's truth of the value.
    pub fn truthy(&self) -> bool {
        match self {
            OptValue::None => false,
            OptValue::Bool(b) => *b,
            OptValue::Int(i) => *i != 0,
            OptValue::Str(s) => !s.is_empty(),
        }
    }
}

/// `str(value)`.
impl fmt::Display for OptValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OptValue::None => f.write_str("None"),
            OptValue::Bool(true) => f.write_str("True"),
            OptValue::Bool(false) => f.write_str("False"),
            OptValue::Int(i) => write!(f, "{i}"),
            OptValue::Str(s) => f.write_str(s),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptType {
    Bool,
    Int,
    Str,
    /// `opt_strategy`: one of `size`, `speed`, `auto`.
    Strategy,
}

impl OptType {
    /// The type as Python prints it in `InvalidValueError`.
    fn python_name(self) -> &'static str {
        match self {
            OptType::Bool => "<class 'bool'>",
            OptType::Int => "<class 'int'>",
            OptType::Str => "<class 'str'>",
            OptType::Strategy => "<enum 'OptimizationStrategy'>",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Opt {
    pub name: &'static str,
    pub ty: OptType,
    pub ignore_none: bool,
    pub value: OptValue,
    pub default: OptValue,
    pub stack: Vec<OptValue>,
}

/// What assigning or popping an option can raise in Python.
#[derive(Debug, PartialEq)]
pub enum OptError {
    /// `UndefinedOptionError`.
    Undefined(String),
    /// `InvalidValueError`.
    InvalidValue { option: String, ty: OptType, value: String },
    /// `OptionStackUnderflowError`.
    StackUnderflow(String),
}

impl fmt::Display for OptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OptError::Undefined(option) => write!(f, "Undefined option '{option}'"),
            OptError::InvalidValue { option, ty, value } => write!(f, "Invalid value '{value}' for option '{option}'. Value type must be '{}'", ty.python_name()),
            OptError::StackUnderflow(option) => write!(f, "Cannot pop option '{option}'. Option stack is empty"),
        }
    }
}

/// `int(text)`: an optional sign and decimal digits, blanks and `_` between
/// digits allowed.
pub fn python_int(text: &str) -> Option<i128> {
    let t = text.trim();
    let (sign, digits) = match t.strip_prefix('-') {
        Some(rest) => (-1, rest),
        None => (1, t.strip_prefix('+').unwrap_or(t)),
    };
    if digits.is_empty() || digits.starts_with('_') || digits.ends_with('_') || digits.contains("__") {
        return None;
    }
    if !digits.chars().all(|c| c.is_ascii_digit() || c == '_') {
        return None;
    }
    digits.replace('_', "").parse::<i128>().ok().map(|n| sign * n)
}

#[derive(Clone, Debug)]
pub struct Options {
    /// In the order they were added, as the Python dict keeps them.
    pub list: Vec<Opt>,
}

impl Default for Options {
    fn default() -> Self {
        Options::new()
    }
}

impl Options {
    /// The options a compile starts with: `config.init()`, then
    /// `Backend.init()` and `init_codebank_options()`. Those that hold
    /// streams or the macro table are not options here.
    pub fn new() -> Options {
        use OptType::*;
        use OptValue as V;
        let mut options = Options { list: Vec::new() };
        let mut add = |name: &'static str, ty: OptType, default: OptValue, ignore_none: bool| {
            options.list.push(Opt { name, ty, ignore_none, value: default.clone(), default, stack: Vec::new() });
        };
        add("output_filename", Str, V::None, false);
        add("input_filename", Str, V::None, false);
        add("stderr_filename", Str, V::None, true);
        add("debug_level", Int, V::Int(0), true);
        add("optimization_level", Int, V::Int(2), true);
        add("case_insensitive", Bool, V::Bool(false), true);
        add("array_base", Int, V::Int(0), true);
        add("default_byref", Bool, V::Bool(false), true);
        add("max_syntax_errors", Int, V::Int(20), false);
        add("string_base", Int, V::Int(0), true);
        add("memory_map", Str, V::None, true);
        add("force_asm_brackets", Bool, V::Bool(false), true);
        add("use_basic_loader", Bool, V::Bool(false), true);
        add("autorun", Bool, V::Bool(false), true);
        add("output_file_type", Str, V::Str("bin".into()), false);
        add("include_path", Str, V::Str(String::new()), false);
        add("memory_check", Bool, V::Bool(false), true);
        add("strict_bool", Bool, V::Bool(false), true);
        add("array_check", Bool, V::Bool(false), true);
        add("enable_break", Bool, V::Bool(false), true);
        add("emit_backend", Bool, V::Bool(false), false);
        add("explicit", Bool, V::Bool(false), true);
        add("sinclair", Bool, V::Bool(false), false);
        add("strict", Bool, V::Bool(false), true);
        add("zxnext", Bool, V::Bool(false), true);
        add("architecture", Str, V::None, true);
        add("expected_warnings", Int, V::Int(0), true);
        add("hide_warning_codes", Bool, V::Bool(false), true);
        add("opt_strategy", Strategy, V::Str("auto".into()), true);
        add("project_filename", Str, V::None, false);
        // Backend.init()
        add("org", Int, V::Int(32768), false);
        add("heap_size", Int, V::Int(4768), true);
        add("heap_address", Int, V::None, false);
        add("heap_start_label", Str, V::Str(".core.ZXBASIC_MEM_HEAP".into()), false);
        add("heap_size_label", Str, V::Str(".core.ZXBASIC_HEAP_SIZE".into()), false);
        add("headerless", Bool, V::Bool(false), true);
        // init_codebank_options()
        add("codebank", Int, V::Int(0), true);
        add("codewindow", Int, V::Int(0x6000), true);
        add("codewindowsize", Int, V::Int(8192), true);
        add("codebankbase", Int, V::Int(30), true);
        add("codebankpages", Str, V::Str(String::new()), true);
        add("codebankdepth", Int, V::Int(16), true);
        options
    }

    pub fn contains(&self, name: &str) -> bool {
        self.list.iter().any(|o| o.name == name)
    }

    pub fn find(&self, name: &str) -> Option<&Opt> {
        self.list.iter().find(|o| o.name == name)
    }

    fn find_mut(&mut self, name: &str) -> Result<&mut Opt, OptError> {
        self.list.iter_mut().find(|o| o.name == name).ok_or_else(|| OptError::Undefined(name.to_string()))
    }

    /// The value of an option that exists.
    pub fn get(&self, name: &str) -> &OptValue {
        &self.find(name).unwrap_or_else(|| panic!("undefined option '{name}'")).value
    }

    pub fn bool(&self, name: &str) -> bool {
        self.get(name).truthy()
    }

    /// An integer option; zero when it is unset.
    pub fn int(&self, name: &str) -> i128 {
        match self.get(name) {
            OptValue::Int(i) => *i,
            OptValue::Bool(b) => *b as i128,
            _ => 0,
        }
    }

    pub fn str(&self, name: &str) -> &str {
        match self.get(name) {
            OptValue::Str(s) => s,
            _ => "",
        }
    }

    /// `Option.value = value`: the value is converted to the option's type
    /// where Python's constructor of that type would accept it.
    fn convert(opt: &Opt, value: OptValue) -> Result<Option<OptValue>, OptError> {
        let invalid = |value: &OptValue| OptError::InvalidValue { option: opt.name.to_string(), ty: opt.ty, value: value.to_string() };
        let converted = match (opt.ty, value) {
            (_, OptValue::None) => {
                if opt.ignore_none {
                    return Ok(None);
                }
                OptValue::None
            }
            (OptType::Bool, OptValue::Bool(b)) => OptValue::Bool(b),
            (OptType::Bool, OptValue::Str(s)) => match s.to_lowercase().as_str() {
                "false" | "off" | "-" | "no" => OptValue::Bool(false),
                "true" | "on" | "+" | "yes" => OptValue::Bool(true),
                _ => return Err(invalid(&OptValue::Str(s))),
            },
            // bool(5) is True.
            (OptType::Bool, OptValue::Int(i)) => OptValue::Bool(i != 0),
            // A bool is an int in Python and is stored as it is.
            (OptType::Int, OptValue::Bool(b)) => OptValue::Bool(b),
            (OptType::Int, OptValue::Int(i)) => OptValue::Int(i),
            (OptType::Int, OptValue::Str(s)) => match python_int(&s) {
                Some(i) => OptValue::Int(i),
                None => return Err(invalid(&OptValue::Str(s))),
            },
            (OptType::Str, OptValue::Str(s)) => OptValue::Str(s),
            (OptType::Str, other) => OptValue::Str(other.to_string()),
            (OptType::Strategy, OptValue::Str(s)) => match s.as_str() {
                "size" | "speed" | "auto" => OptValue::Str(s),
                _ => return Err(invalid(&OptValue::Str(s))),
            },
            (OptType::Strategy, other) => return Err(invalid(&other)),
        };
        Ok(Some(converted))
    }

    pub fn set(&mut self, name: &str, value: OptValue) -> Result<(), OptError> {
        let opt = self.find_mut(name)?;
        if let Some(value) = Self::convert(opt, value)? {
            opt.value = value;
        }
        Ok(())
    }

    /// `OPTIONS[name].push(value)`; with no value the current one is kept.
    pub fn push(&mut self, name: &str, value: Option<OptValue>) -> Result<(), OptError> {
        let opt = self.find_mut(name)?;
        let value = value.unwrap_or_else(|| opt.value.clone());
        let converted = Self::convert(opt, value);
        // Python saves the old value before the assignment can fail.
        opt.stack.push(opt.value.clone());
        if let Some(value) = converted? {
            opt.value = value;
        }
        Ok(())
    }

    pub fn pop(&mut self, name: &str) -> Result<OptValue, OptError> {
        let opt = self.find_mut(name)?;
        let Some(saved) = opt.stack.pop() else { return Err(OptError::StackUnderflow(name.to_string())) };
        let result = opt.value.clone();
        if let Some(value) = Self::convert(opt, saved)? {
            opt.value = value;
        }
        Ok(result)
    }

    /// `current_codebank()`: the code bank in effect, 0 for the resident program.
    pub fn current_codebank(&self) -> i64 {
        self.int("codebank") as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assignment_converts_like_python() {
        let mut o = Options::new();
        o.set("explicit", OptValue::Str("On".into())).unwrap();
        assert_eq!(o.get("explicit"), &OptValue::Bool(true));
        o.set("array_base", OptValue::Str("1".into())).unwrap();
        assert_eq!(o.int("array_base"), 1);
        assert!(o.set("array_base", OptValue::Str("x".into())).is_err());
        assert_eq!(o.set("nothing", OptValue::Int(1)), Err(OptError::Undefined("nothing".into())));
        // None leaves an ignore_none option alone and clears another.
        o.set("array_base", OptValue::None).unwrap();
        assert_eq!(o.int("array_base"), 1);
        o.set("heap_address", OptValue::Int(5)).unwrap();
        o.set("heap_address", OptValue::None).unwrap();
        assert_eq!(o.get("heap_address"), &OptValue::None);
    }

    #[test]
    fn push_and_pop() {
        let mut o = Options::new();
        o.push("codebank", Some(OptValue::Int(3))).unwrap();
        assert_eq!(o.current_codebank(), 3);
        o.push("codebank", None).unwrap();
        assert_eq!(o.pop("codebank").unwrap(), OptValue::Int(3));
        assert_eq!(o.pop("codebank").unwrap(), OptValue::Int(3));
        assert_eq!(o.current_codebank(), 0);
        assert_eq!(o.pop("codebank"), Err(OptError::StackUnderflow("codebank".into())));
    }
}
