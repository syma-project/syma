/// Runtime values for Syma language.
///
/// Everything in Syma evaluates to a Value. Values are the runtime
/// representation of symbolic expressions.
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use rug::Float;
use rug::Integer;
use rug::Rational;

use crate::ast::Expr;
use crate::bytecode::BytecodeFunctionDef;
use crate::env::Env;

// ── FFI types ─────────────────────────────────────────────────────────────────

/// The C type tags used in native function signatures.
#[derive(Debug, Clone, PartialEq)]
pub enum NativeType {
    Void,
    I32,
    I64,
    U32,
    U64,
    F32,
    F64,
    /// Null-terminated UTF-8 string (`*const c_char`).
    CString,
    /// Marshalled as `i32` (C convention: 0 = false).
    Bool,
}

impl NativeType {
    /// Parse a Syma string literal (e.g. `"Integer64"`) into a `NativeType`.
    pub fn from_syma_name(s: &str) -> Option<Self> {
        match s {
            "Void" => Some(NativeType::Void),
            "Integer32" => Some(NativeType::I32),
            "Integer64" => Some(NativeType::I64),
            "UnsignedInteger32" => Some(NativeType::U32),
            "UnsignedInteger64" => Some(NativeType::U64),
            "Real32" => Some(NativeType::F32),
            "Real64" => Some(NativeType::F64),
            "CString" => Some(NativeType::CString),
            "Boolean" => Some(NativeType::Bool),
            _ => None,
        }
    }

    /// Return the Syma display name.
    pub fn syma_name(&self) -> &'static str {
        match self {
            NativeType::Void => "Void",
            NativeType::I32 => "Integer32",
            NativeType::I64 => "Integer64",
            NativeType::U32 => "UnsignedInteger32",
            NativeType::U64 => "UnsignedInteger64",
            NativeType::F32 => "Real32",
            NativeType::F64 => "Real64",
            NativeType::CString => "CString",
            NativeType::Bool => "Boolean",
        }
    }
}

/// Type signature of a native C function.
#[derive(Debug, Clone, PartialEq)]
pub struct NativeSig {
    pub params: Vec<NativeType>,
    pub ret: NativeType,
}

/// Opaque handle to an OS-level dynamic library (`dlopen` handle).
///
/// Stored as `usize` so that `NativeLibHandle: Send + Sync` without
/// requiring `unsafe impl` — the OS guarantees the handle is valid
/// across threads for the lifetime of the loaded library.
pub struct NativeLibHandle {
    /// Raw `dlopen` / `LoadLibrary` handle stored as `usize`.
    pub(crate) raw: usize,
}

impl fmt::Debug for NativeLibHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "NativeLibHandle(0x{:x})", self.raw)
    }
}

// SAFETY: dlopen handles are valid from any thread once the library is loaded.
unsafe impl Send for NativeLibHandle {}
unsafe impl Sync for NativeLibHandle {}

/// Create a Value::Rational from numerator and denominator, canonicalizing to
/// Value::Integer when the denominator is 1.
pub fn rational_value(num: Integer, den: Integer) -> Value {
    if den == 1 {
        Value::Integer(num)
    } else {
        Value::Rational(Box::new(Rational::from((num, den))))
    }
}

/// Default precision for floating-point numbers (128 bits ≈ 38 decimal digits).
pub const DEFAULT_PRECISION: u32 = 128;

/// Output format specification for `Value::Formatted`.
///
/// Controls how a value is displayed without affecting its semantics.
/// Analogous to Wolfram Language's `Format` types (`InputForm`, `Short`, `NumberForm`, etc.).
#[derive(Debug, Clone, PartialEq)]
pub enum Format {
    /// Default display (FullForm-style).
    Standard,
    /// Explicit FullForm (head[arg, ...] notation).
    FullForm,
    /// InputForm (infix notation with operators, e.g. `a + b` instead of `Plus[a, b]`).
    InputForm,
    /// Truncated output — at most `n` top-level items. `Short[expr]` alias with default 5.
    Short(usize),
    /// Limit nesting depth — at most `n` levels shown. `Shallow[expr]` default depth 3.
    Shallow(usize),
    /// Number formatting — `NumberForm[expr, n]` shows n significant digits.
    NumberForm { digits: usize },
    /// Scientific notation — `ScientificForm[expr, n]` shows n significant digits in `m·10^e`.
    ScientificForm { digits: usize },
    /// Number display in a given base — `BaseForm[expr, base]` (2 ≤ base ≤ 36).
    BaseForm(u32),
    /// Table/grid layout for 2D lists — `Grid[list]`.
    Grid,
    /// Deferred evaluation — display placeholder (e.g. `Defer[1 + 1]` → `1 + 1`).
    Deferred,
}

/// Convert rug's `to_string_radix` output (e.g. `"3.14159e0"`, `"-1.5e2"`)
/// into standard decimal notation (e.g. `"3.14159"`, `"-150.0"`).
/// Rug uses `e` as exponent marker for base 10 (MPFR convention).
pub fn rug_radix_to_decimal(s: &str) -> String {
    // Split on 'e' or '@' to get mantissa and base-10 exponent.
    let sep_pos = s[1..].find(['e', '@']).map(|i| i + 1);
    let (mantissa, exp_str) = if let Some(at) = sep_pos {
        (&s[..at], &s[at + 1..])
    } else {
        // No exponent — already standard decimal.
        return trim_float_zeros(s);
    };

    let exp: i64 = exp_str.parse().unwrap_or(0);
    let negative = mantissa.starts_with('-');
    let digits_part = if negative { &mantissa[1..] } else { mantissa };

    // digits_part is like "3.14159265..." — collect all digit characters.
    let mut digits: Vec<char> = digits_part.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        digits.push('0');
    }

    // The mantissa has 1 digit before the decimal, so the actual integer part
    // ends at index `1 + exp` in the digit sequence.
    let dot_pos = 1i64 + exp; // position from the left where '.' goes

    let result = if dot_pos <= 0 {
        // All digits are after the decimal point; need leading "0.000..." prefix.
        let leading_zeros = (-dot_pos) as usize;
        let frac: String = digits.iter().collect();
        format!("0.{}{}", "0".repeat(leading_zeros), frac)
    } else if dot_pos as usize >= digits.len() {
        // Dot is at or beyond the last digit — integer, possibly with trailing zeros.
        let extra = dot_pos as usize - digits.len();
        let int_part: String = digits.iter().collect();
        format!("{}{}.0", int_part, "0".repeat(extra))
    } else {
        let int_part: String = digits[..dot_pos as usize].iter().collect();
        let frac_part: String = digits[dot_pos as usize..].iter().collect();
        format!("{}.{}", int_part, frac_part)
    };

    let result = trim_float_zeros(&result);
    if negative {
        format!("-{}", result)
    } else {
        result
    }
}

fn trim_float_zeros(s: &str) -> String {
    if s.contains('.') {
        let t = s.trim_end_matches('0');
        if t.ends_with('.') {
            format!("{}0", t)
        } else {
            t.to_string()
        }
    } else {
        s.to_string()
    }
}

/// A runtime value.
#[derive(Debug, Clone)]
pub enum Value {
    // ── Atoms ──
    Integer(Integer),
    Real(Float),
    /// An exact rational number, e.g. 2/3.
    /// Boxed because rug::Rational is large (two Integers).
    Rational(Box<Rational>),
    Complex {
        re: f64,
        im: f64,
    },
    Str(String),
    Bool(bool),
    Null,
    Symbol(String),

    // ── Compound ──
    /// A list of values.
    List(Vec<Value>),

    /// A sequence of values that automatically splats into lists and calls.
    /// Analogous to Wolfram Language's Sequence[...].
    Sequence(Vec<Value>),

    /// A value with an explicit display format.
    /// The format controls how the value is rendered (e.g., `InputForm`, `Short`, `NumberForm`),
    /// without affecting its semantic meaning. Analogous to Wolfram Language's `Format[expr, form]`.
    Formatted {
        format: Format,
        value: Box<Value>,
    },

    /// A named function with its head.
    Call {
        head: String,
        args: Vec<Value>,
    },

    /// An association (hash map).
    Assoc(HashMap<String, Value>),

    /// A rule: lhs -> rhs or lhs :> rhs.
    Rule {
        lhs: Box<Value>,
        rhs: Box<Value>,
        delayed: bool,
    },

    // ── Callable ──
    /// A user-defined function with pattern-matched definitions.
    Function(Arc<FunctionDef>),

    /// A function compiled to bytecode.
    BytecodeFunction(Arc<BytecodeFunctionDef>),

    /// A built-in function.
    Builtin(String, BuiltinFn),

    /// A pure function (lambda) with slots or named parameters.
    PureFunction {
        body: Expr,
        slot_count: usize,
        /// Named parameter names (e.g., ["k"] for Function[{k}, body]).
        /// Non-empty when created via Function[{x, y}, body]; empty for slot-based (#, #1) lambdas.
        params: Vec<String>,
    },

    /// A method bound to an object.
    Method {
        name: String,
        object: Box<Value>,
    },

    // ── Objects ──
    /// A class instance.
    Object {
        class_name: String,
        fields: HashMap<String, Value>,
    },

    /// A class definition (metadata for instantiation).
    Class(Arc<ClassDef>),

    // ── Rules ──
    /// A named rule set.
    RuleSet {
        name: String,
        rules: Vec<(Value, Value)>,
    },

    // ── Pattern (for internal use) ──
    Pattern(Expr),

    // ── Module ──
    /// A first-class module value produced by a `module Foo { ... }` definition.
    Module {
        name: String,
        /// The exported symbols and their evaluated values.
        exports: HashMap<String, Value>,
        /// All internal bindings (including non-exported helpers) from the module body.
        /// Used by `Needs` to make internal symbols available to exported functions.
        locals: HashMap<String, Value>,
    },

    // ── Image ──
    /// A raster image. Uses the `image` crate's DynamicImage internally.
    /// Wrapped in `Arc` for zero-cost sharing (images are immutable).
    Image(Arc<image::DynamicImage>),

    // ── Dataset ──
    /// A Dataset wrapping a structured value.
    /// Wrapped in Arc for cheap clone and thread-safe sharing.
    /// Primarily useful for tabular data (lists of associations).
    Dataset(Arc<Value>),

    // ── Hold ──
    Hold(Box<Value>),
    HoldComplete(Box<Value>),

    // ── FFI ──
    /// A loaded native dynamic library.
    NativeLib {
        name: String,
        handle: Arc<NativeLibHandle>,
    },

    /// A callable symbol resolved from a native library.
    NativeFunction {
        lib_name: String,
        symbol_name: String,
        /// Raw function pointer stored as `usize` for `Send + Sync`.
        fn_ptr: usize,
        signature: NativeSig,
    },
}

/// A built-in function implementation.
///
/// `Pure` functions only receive their arguments. `Env` functions also receive
/// the current evaluation environment (needed for recursive `eval`/`apply_function` calls,
/// lazy loading, or module registration).
#[derive(Debug, Clone)]
pub enum BuiltinFn {
    /// Pure builtin — does not need environment access.
    Pure(fn(&[Value]) -> Result<Value, EvalError>),
    /// Environment-aware builtin — receives the current evaluation environment.
    Env(fn(&[Value], &Env) -> Result<Value, EvalError>),
}

/// A user-defined function with multiple pattern-matched definitions.
#[derive(Debug, Clone)]
pub struct FunctionDef {
    pub name: String,
    pub definitions: Vec<FunctionDefinition>,
}

/// A single function definition (one pattern-match case).
#[derive(Debug, Clone)]
pub struct FunctionDefinition {
    pub params: Vec<Expr>,
    pub body: Expr,
    pub delayed: bool,
}

/// A class definition with fields, methods, constructor, and inheritance info.
#[derive(Debug, Clone)]
pub struct ClassDef {
    pub name: String,
    pub parent: Option<String>,
    pub mixins: Vec<String>,
    pub fields: Vec<ClassField>,
    pub methods: HashMap<String, ClassMethod>,
    pub constructor: Option<ClassConstructor>,
}

/// A class field with optional type hint and default value.
#[derive(Debug, Clone)]
pub struct ClassField {
    pub name: String,
    pub type_hint: Option<String>,
    pub default: Option<Expr>,
}

/// A class method.
#[derive(Debug, Clone)]
pub struct ClassMethod {
    pub name: String,
    pub params: Vec<Expr>,
    pub body: Expr,
}

/// A class constructor.
#[derive(Debug, Clone)]
pub struct ClassConstructor {
    pub params: Vec<Expr>,
    pub body: Expr,
}

/// Evaluation error.
#[derive(Debug, Clone)]
pub enum EvalError {
    /// No matching definition for the given arguments.
    NoMatch { head: String, args: Vec<Value> },
    /// Type error.
    TypeError { expected: String, got: String },
    /// Division by zero.
    DivisionByZero,
    /// Index out of bounds.
    IndexOutOfBounds { index: i64, length: usize },
    /// Unknown symbol.
    UnknownSymbol(String),
    /// User-thrown error.
    Thrown(Value),
    /// General error message.
    Error(String),
    /// FFI / foreign-function error.
    FfiError(String),
}

impl fmt::Display for EvalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EvalError::NoMatch { head, args } => {
                write!(f, "No matching definition for {}[", head)?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", arg)?;
                }
                write!(f, "]")
            }
            EvalError::TypeError { expected, got } => {
                write!(f, "Type error: expected {}, got {}", expected, got)
            }
            EvalError::DivisionByZero => write!(f, "Division by zero"),
            EvalError::IndexOutOfBounds { index, length } => {
                write!(f, "Index {} out of bounds (length {})", index, length)
            }
            EvalError::UnknownSymbol(s) => write!(f, "Unknown symbol: {}", s),
            EvalError::Thrown(v) => write!(f, "Thrown: {}", v),
            EvalError::Error(s) => write!(f, "Error: {}", s),
            EvalError::FfiError(s) => write!(f, "FFI error: {}", s),
        }
    }
}

impl std::error::Error for EvalError {}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Integer(a), Value::Integer(b)) => a == b,
            (Value::Real(a), Value::Real(b)) => a == b,
            (Value::Rational(a), Value::Rational(b)) => a == b,
            (Value::Rational(a), Value::Integer(b)) | (Value::Integer(b), Value::Rational(a)) => {
                a.denom() == &Integer::from(1) && a.numer() == b
            }
            (Value::Complex { re: a1, im: a2 }, Value::Complex { re: b1, im: b2 }) => {
                a1 == b1 && a2 == b2
            }
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Null, Value::Null) => true,
            (Value::Symbol(a), Value::Symbol(b)) => a == b,
            (Value::List(a), Value::List(b)) => a == b,
            (Value::Sequence(a), Value::Sequence(b)) => a == b,
            (Value::Call { head: h1, args: a1 }, Value::Call { head: h2, args: a2 }) => {
                h1 == h2 && a1 == a2
            }
            (Value::Assoc(a), Value::Assoc(b)) => a == b,
            (
                Value::Rule {
                    lhs: l1,
                    rhs: r1,
                    delayed: d1,
                },
                Value::Rule {
                    lhs: l2,
                    rhs: r2,
                    delayed: d2,
                },
            ) => d1 == d2 && l1 == l2 && r1 == r2,
            (
                Value::RuleSet {
                    name: n1,
                    rules: r1,
                },
                Value::RuleSet {
                    name: n2,
                    rules: r2,
                },
            ) => n1 == n2 && r1 == r2,
            (
                Value::Module {
                    name: n1,
                    exports: e1,
                    ..
                },
                Value::Module {
                    name: n2,
                    exports: e2,
                    ..
                },
            ) => n1 == n2 && e1 == e2,
            // NativeLib and NativeFunction: compare by name/symbol (pointer identity not stable)
            (Value::NativeLib { name: n1, .. }, Value::NativeLib { name: n2, .. }) => n1 == n2,
            (
                Value::NativeFunction {
                    lib_name: l1,
                    symbol_name: s1,
                    signature: sig1,
                    ..
                },
                Value::NativeFunction {
                    lib_name: l2,
                    symbol_name: s2,
                    signature: sig2,
                    ..
                },
            ) => l1 == l2 && s1 == s2 && sig1 == sig2,
            // Formatted: compare inner values (format is only a display hint)
            (Value::Formatted { value: a, .. }, Value::Formatted { value: b, .. }) => a == b,
            (Value::Formatted { value, .. }, other) | (other, Value::Formatted { value, .. }) => {
                value.as_ref() == other
            }
            // Image: compare by pixel data (using byte representation)
            (Value::Image(a), Value::Image(b)) => a.as_bytes() == b.as_bytes(),
            // Dataset: compare by inner data
            (Value::Dataset(a), Value::Dataset(b)) => a.as_ref() == b.as_ref(),
            // BytecodeFunction: never equal (like other function types)
            (Value::BytecodeFunction(_), Value::BytecodeFunction(_)) => false,
            _ => false,
        }
    }
}

impl Value {
    /// Get the type name of this value.
    pub fn type_name(&self) -> &str {
        match self {
            Value::Integer(_) => "Integer",
            Value::Real(_) => "Real",
            Value::Rational(_) => "Rational",
            Value::Complex { .. } => "Complex",
            Value::Str(_) => "String",
            Value::Bool(_) => "Boolean",
            Value::Null => "Null",
            Value::Symbol(_) => "Symbol",
            Value::List(_) => "List",
            Value::Sequence(_) => "Sequence",
            Value::Call { .. } => "Expr",
            Value::Assoc(_) => "Assoc",
            Value::Rule { .. } => "Rule",
            Value::Function(_) => "Function",
            Value::BytecodeFunction(_) => "BytecodeFunction",
            Value::Builtin(_, _) => "Builtin",
            Value::PureFunction { .. } => "PureFunction",
            Value::Method { .. } => "Method",
            Value::Object { class_name, .. } => class_name,
            Value::Class(class_def) => &class_def.name,
            Value::RuleSet { .. } => "RuleSet",
            Value::Pattern(_) => "Pattern",
            Value::Module { .. } => "Module",
            Value::Image(_) => "Image",
            Value::Dataset(_) => "Dataset",
            Value::Hold(_) => "Hold",
            Value::HoldComplete(_) => "HoldComplete",
            Value::NativeLib { .. } => "NativeLib",
            Value::NativeFunction { .. } => "NativeFunction",
            Value::Formatted { format, value } => match format {
                Format::InputForm => "InputForm",
                Format::Short(_) => "Short",
                Format::Shallow(_) => "Shallow",
                Format::NumberForm { .. } => "NumberForm",
                Format::ScientificForm { .. } => "ScientificForm",
                Format::BaseForm(_) => "BaseForm",
                Format::Grid => "Grid",
                Format::Deferred => "Deferred",
                Format::Standard | Format::FullForm => value.type_name(),
            },
        }
    }

    /// Check if this value matches a type pattern.
    pub fn matches_type(&self, type_name: &str) -> bool {
        match type_name {
            "Number" => matches!(
                self,
                Value::Integer(_) | Value::Real(_) | Value::Rational(_) | Value::Complex { .. }
            ),
            "Integer" => matches!(self, Value::Integer(_)),
            "Real" => matches!(self, Value::Real(_)),
            "Rational" => matches!(self, Value::Rational(_)),
            "Complex" => matches!(self, Value::Complex { .. }),
            "String" => matches!(self, Value::Str(_)),
            "Boolean" => matches!(self, Value::Bool(_)),
            "Symbol" => matches!(self, Value::Symbol(_)),
            "List" => matches!(self, Value::List(_)),
            "Assoc" => matches!(self, Value::Assoc(_)),
            "Rule" => matches!(self, Value::Rule { .. }),
            "Function" => matches!(
                self,
                Value::Function(_)
                    | Value::BytecodeFunction(_)
                    | Value::Builtin(_, _)
                    | Value::PureFunction { .. }
            ),
            "Module" => matches!(self, Value::Module { .. }),
            "Dataset" => matches!(self, Value::Dataset(_)),
            "Object" => matches!(self, Value::Object { .. }),
            "Expr" => true,
            _ => {
                // Check if it's a class name
                if let Value::Object { class_name, .. } = self {
                    class_name == type_name
                } else {
                    false
                }
            }
        }
    }

    /// Convert to boolean.
    pub fn to_bool(&self) -> bool {
        match self {
            Value::Formatted { value, .. } => value.to_bool(),
            Value::Bool(b) => *b,
            Value::Integer(n) => !n.is_zero(),
            Value::Real(r) => !r.is_zero(),
            Value::Rational(r) => !r.is_zero(),
            Value::Null => false,
            Value::List(l) => !l.is_empty(),
            _ => true,
        }
    }

    /// Try to convert to i64.
    pub fn to_integer(&self) -> Option<i64> {
        match self {
            Value::Formatted { value, .. } => value.to_integer(),
            Value::Integer(n) => Some(n.to_i64().unwrap_or(0)),
            Value::Real(r) => Some(r.to_f64() as i64),
            Value::Rational(r) => {
                if *r.denom() == Integer::from(1) {
                    r.numer().to_i64()
                } else {
                    Some(r.to_f64() as i64)
                }
            }
            _ => None,
        }
    }

    /// Try to convert to f64.
    pub fn to_real(&self) -> Option<f64> {
        match self {
            Value::Formatted { value, .. } => value.to_real(),
            Value::Integer(n) => Some(n.to_f64()),
            Value::Real(r) => Some(r.to_f64()),
            Value::Rational(r) => Some(r.to_f64()),
            Value::Complex { re, im } if *im == 0.0 => Some(*re),
            _ => None,
        }
    }

    /// Convert this value to a tagged-JSON representation for frontend consumption.
    pub fn serialize_json(&self) -> serde_json::Value {
        crate::ffi::marshal::value_to_json_full(self)
    }

    /// Check if two values are structurally equal.
    pub fn struct_eq(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Formatted { value: a, .. }, Value::Formatted { value: b, .. }) => {
                a.struct_eq(b)
            }
            (Value::Formatted { value, .. }, other) => value.struct_eq(other),
            (other, Value::Formatted { value, .. }) => other.struct_eq(value),
            (Value::Integer(a), Value::Integer(b)) => a == b,
            (Value::Real(a), Value::Real(b)) => a == b,
            (Value::Rational(a), Value::Rational(b)) => a == b,
            (Value::Complex { re: a1, im: a2 }, Value::Complex { re: b1, im: b2 }) => {
                a1 == b1 && a2 == b2
            }
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Null, Value::Null) => true,
            (Value::Symbol(a), Value::Symbol(b)) => a == b,
            (Value::Image(a), Value::Image(b)) => a.as_bytes() == b.as_bytes(),
            (Value::Dataset(a), Value::Dataset(b)) => a.as_ref().struct_eq(b.as_ref()),
            (Value::Dataset(a), other) => a.as_ref().struct_eq(other),
            (other, Value::Dataset(a)) => other.struct_eq(a.as_ref()),
            (Value::List(a), Value::List(b)) => {
                a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x.struct_eq(y))
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_name() {
        assert_eq!(Value::Integer(Integer::from(1)).type_name(), "Integer");
        assert_eq!(
            Value::Real(Float::with_val(DEFAULT_PRECISION, 1.0)).type_name(),
            "Real"
        );
        assert_eq!(Value::Complex { re: 1.0, im: 2.0 }.type_name(), "Complex");
        assert_eq!(Value::Str("s".to_string()).type_name(), "String");
        assert_eq!(Value::Bool(true).type_name(), "Boolean");
        assert_eq!(Value::Null.type_name(), "Null");
        assert_eq!(Value::Symbol("x".to_string()).type_name(), "Symbol");
        assert_eq!(Value::List(vec![]).type_name(), "List");
        assert_eq!(Value::Assoc(HashMap::new()).type_name(), "Assoc");
    }

    #[test]
    fn test_matches_type_number() {
        assert!(Value::Integer(Integer::from(1)).matches_type("Number"));
        assert!(Value::Real(Float::with_val(DEFAULT_PRECISION, 1.0)).matches_type("Number"));
        assert!(Value::Complex { re: 1.0, im: 0.0 }.matches_type("Number"));
        assert!(!Value::Str("s".to_string()).matches_type("Number"));
    }

    #[test]
    fn test_matches_type_integer() {
        assert!(Value::Integer(Integer::from(1)).matches_type("Integer"));
        assert!(!Value::Real(Float::with_val(DEFAULT_PRECISION, 1.0)).matches_type("Integer"));
    }

    #[test]
    fn test_matches_type_string() {
        assert!(Value::Str("hello".to_string()).matches_type("String"));
        assert!(!Value::Integer(Integer::from(1)).matches_type("String"));
    }

    #[test]
    fn test_matches_type_boolean() {
        assert!(Value::Bool(true).matches_type("Boolean"));
        assert!(!Value::Integer(Integer::from(1)).matches_type("Boolean"));
    }

    #[test]
    fn test_matches_type_list() {
        assert!(Value::List(vec![]).matches_type("List"));
        assert!(!Value::Integer(Integer::from(1)).matches_type("List"));
    }

    #[test]
    fn test_matches_type_expr() {
        // Expr matches everything
        assert!(Value::Integer(Integer::from(1)).matches_type("Expr"));
        assert!(Value::Str("s".to_string()).matches_type("Expr"));
        assert!(Value::Null.matches_type("Expr"));
    }

    #[test]
    fn test_to_bool() {
        assert!(Value::Bool(true).to_bool());
        assert!(!Value::Bool(false).to_bool());
        assert!(Value::Integer(Integer::from(1)).to_bool());
        assert!(!Value::Integer(Integer::from(0)).to_bool());
        assert!(Value::Real(Float::with_val(DEFAULT_PRECISION, 1.0)).to_bool());
        assert!(!Value::Real(Float::with_val(DEFAULT_PRECISION, 0.0)).to_bool());
        assert!(!Value::Null.to_bool());
        assert!(!Value::List(vec![]).to_bool());
        assert!(Value::List(vec![Value::Integer(Integer::from(1))]).to_bool());
        assert!(Value::Str("hello".to_string()).to_bool());
    }

    #[test]
    fn test_to_integer() {
        assert_eq!(Value::Integer(Integer::from(42)).to_integer(), Some(42));
        assert_eq!(
            Value::Real(Float::with_val(DEFAULT_PRECISION, 3.14)).to_integer(),
            Some(3)
        );
        assert_eq!(Value::Str("s".to_string()).to_integer(), None);
    }

    #[test]
    fn test_to_real() {
        assert_eq!(Value::Integer(Integer::from(42)).to_real(), Some(42.0));
        assert_eq!(
            Value::Real(Float::with_val(DEFAULT_PRECISION, 3.14)).to_real(),
            Some(3.14)
        );
        assert_eq!(Value::Complex { re: 1.0, im: 0.0 }.to_real(), Some(1.0));
        assert_eq!(Value::Complex { re: 1.0, im: 2.0 }.to_real(), None);
        assert_eq!(Value::Str("s".to_string()).to_real(), None);
    }

    #[test]
    fn test_struct_eq() {
        assert!(Value::Integer(Integer::from(1)).struct_eq(&Value::Integer(Integer::from(1))));
        assert!(!Value::Integer(Integer::from(1)).struct_eq(&Value::Integer(Integer::from(2))));
        assert!(
            Value::Real(Float::with_val(DEFAULT_PRECISION, 1.0))
                .struct_eq(&Value::Real(Float::with_val(DEFAULT_PRECISION, 1.0)))
        );
        assert!(Value::Str("a".to_string()).struct_eq(&Value::Str("a".to_string())));
        assert!(!Value::Str("a".to_string()).struct_eq(&Value::Str("b".to_string())));
        assert!(Value::Bool(true).struct_eq(&Value::Bool(true)));
        assert!(Value::Null.struct_eq(&Value::Null));
        assert!(
            Value::List(vec![
                Value::Integer(Integer::from(1)),
                Value::Integer(Integer::from(2))
            ])
            .struct_eq(&Value::List(vec![
                Value::Integer(Integer::from(1)),
                Value::Integer(Integer::from(2))
            ]))
        );
        assert!(
            !Value::List(vec![Value::Integer(Integer::from(1))])
                .struct_eq(&Value::List(vec![Value::Integer(Integer::from(2))]))
        );
    }

    #[test]
    fn test_partial_eq() {
        assert_eq!(
            Value::Integer(Integer::from(1)),
            Value::Integer(Integer::from(1))
        );
        assert_ne!(
            Value::Integer(Integer::from(1)),
            Value::Integer(Integer::from(2))
        );
        assert_eq!(
            Value::Real(Float::with_val(DEFAULT_PRECISION, 1.0)),
            Value::Real(Float::with_val(DEFAULT_PRECISION, 1.0))
        );
        assert_eq!(Value::Str("a".to_string()), Value::Str("a".to_string()));
        assert_eq!(Value::Bool(true), Value::Bool(true));
        assert_eq!(Value::Null, Value::Null);
        assert_eq!(
            Value::List(vec![Value::Integer(Integer::from(1))]),
            Value::List(vec![Value::Integer(Integer::from(1))])
        );
        // Different types are not equal
        assert_ne!(
            Value::Integer(Integer::from(1)),
            Value::Real(Float::with_val(DEFAULT_PRECISION, 1.0))
        );
    }

    #[test]
    fn test_display_integer() {
        assert_eq!(format!("{}", Value::Integer(Integer::from(42))), "42");
        assert_eq!(format!("{}", Value::Integer(Integer::from(-7))), "-7");
    }

    #[test]
    fn test_display_real() {
        let val = Float::parse("3.14")
            .map(|v| Float::with_val(DEFAULT_PRECISION, v))
            .unwrap();
        assert_eq!(format!("{}", Value::Real(val)), "3.14");
    }

    #[test]
    fn test_display_string() {
        assert_eq!(format!("{}", Value::Str("hello".to_string())), "\"hello\"");
    }

    #[test]
    fn test_display_bool() {
        assert_eq!(format!("{}", Value::Bool(true)), "True");
        assert_eq!(format!("{}", Value::Bool(false)), "False");
    }

    #[test]
    fn test_display_null() {
        assert_eq!(format!("{}", Value::Null), "Null");
    }

    #[test]
    fn test_display_symbol() {
        assert_eq!(format!("{}", Value::Symbol("x".to_string())), "x");
    }

    #[test]
    fn test_display_list() {
        let list = Value::List(vec![
            Value::Integer(Integer::from(1)),
            Value::Integer(Integer::from(2)),
            Value::Integer(Integer::from(3)),
        ]);
        assert_eq!(format!("{}", list), "{1, 2, 3}");
    }

    #[test]
    fn test_display_empty_list() {
        assert_eq!(format!("{}", Value::List(vec![])), "{}");
    }

    #[test]
    fn test_display_assoc() {
        let mut map = HashMap::new();
        map.insert("a".to_string(), Value::Integer(Integer::from(1)));
        let assoc = Value::Assoc(map);
        let s = format!("{}", assoc);
        assert!(s.starts_with("<|"));
        assert!(s.ends_with("|>"));
        assert!(s.contains("\"a\" -> 1"));
    }

    #[test]
    fn test_display_complex() {
        assert_eq!(format!("{}", Value::Complex { re: 0.0, im: 1.0 }), "1I");
        assert_eq!(format!("{}", Value::Complex { re: 1.0, im: 2.0 }), "1+2I");
        assert_eq!(format!("{}", Value::Complex { re: 1.0, im: -2.0 }), "1-2I");
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Integer(n) => write!(f, "{}", n),
            Value::Rational(r) => write!(f, "{}", r),
            Value::Real(r) => {
                // Use to_string_radix for precise control: gives "d.ddd@exp" format.
                // We convert that to standard decimal notation.
                let prec_digits = ((r.prec() as f64) * std::f64::consts::LOG10_2).ceil() as usize;
                let raw = r.to_string_radix(10, Some(prec_digits.max(1)));
                // raw is like "3.14159...@0" or "3.14159...@-5" etc.
                let formatted = rug_radix_to_decimal(&raw);
                write!(f, "{}", formatted)
            }
            Value::Complex { re, im } => {
                if *re == 0.0 {
                    write!(f, "{}I", im)
                } else if *im >= 0.0 {
                    write!(f, "{}+{}I", re, im)
                } else {
                    write!(f, "{}{}I", re, im)
                }
            }
            Value::Str(s) => write!(f, "\"{}\"", s),
            Value::Bool(b) => write!(f, "{}", if *b { "True" } else { "False" }),
            Value::Null => write!(f, "Null"),
            Value::Symbol(s) => write!(f, "{}", s),
            Value::Sequence(items) => {
                write!(f, "Sequence[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", item)?;
                }
                write!(f, "]")
            }
            Value::List(items) => {
                write!(f, "{{")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", item)?;
                }
                write!(f, "}}")
            }
            Value::Call { head, args } => {
                write!(f, "{}[", head)?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", arg)?;
                }
                write!(f, "]")
            }
            Value::Assoc(map) => {
                write!(f, "<|")?;
                for (i, (k, v)) in map.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "\"{}\" -> {}", k, v)?;
                }
                write!(f, "|>")
            }
            Value::Rule { lhs, rhs, delayed } => {
                let op = if *delayed { ":>" } else { "->" };
                write!(f, "{} {} {}", lhs, op, rhs)
            }
            Value::Function(func) => write!(f, "Function[{}]", func.name),
            Value::BytecodeFunction(bc) => write!(f, "BytecodeFunction[{}]", bc.name),
            Value::Builtin(name, _) => write!(f, "Builtin[{}]", name),
            Value::PureFunction { .. } => write!(f, "PureFunction[...]"),
            Value::Method { name, .. } => write!(f, "Method[{}]", name),
            Value::Object { class_name, fields } => {
                write!(f, "{}[", class_name)?;
                for (i, (k, v)) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{} -> {}", k, v)?;
                }
                write!(f, "]")
            }
            Value::Class(class_def) => write!(f, "Class[{}]", class_def.name),
            Value::RuleSet { name, .. } => write!(f, "RuleSet[{}]", name),
            Value::Pattern(expr) => write!(f, "Pattern[{}]", expr),
            Value::Module { name, exports, .. } => {
                write!(
                    f,
                    "Module[{}, {{{}}}]",
                    name,
                    exports.keys().cloned().collect::<Vec<_>>().join(", ")
                )
            }
            Value::Image(img) => {
                let color_space = match img.color() {
                    image::ColorType::L8 | image::ColorType::L16 => "Grayscale",
                    image::ColorType::La8 | image::ColorType::La16 => "GrayAlpha",
                    image::ColorType::Rgb8 | image::ColorType::Rgb16 | image::ColorType::Rgb32F => {
                        "RGB"
                    }
                    image::ColorType::Rgba8
                    | image::ColorType::Rgba16
                    | image::ColorType::Rgba32F => "RGBA",
                    _ => "Unknown",
                };
                write!(
                    f,
                    "Image[{{{}, {}}}, {}]",
                    img.width(),
                    img.height(),
                    color_space
                )
            }
            Value::Dataset(inner) => format_dataset(inner, f),
            Value::Hold(v) => write!(f, "Hold[{}]", v),
            Value::HoldComplete(v) => write!(f, "HoldComplete[{}]", v),
            Value::NativeLib { name, .. } => write!(f, "NativeLib[\"{}\"]", name),
            Value::NativeFunction {
                lib_name,
                symbol_name,
                ..
            } => write!(f, "NativeFunction[\"{}::{}\"]", lib_name, symbol_name),
            Value::Formatted { format, value } => match format {
                Format::Standard | Format::FullForm => write!(f, "{}", value),
                Format::InputForm => format_input_form(value, f),
                Format::Short(n) => format_short(value, *n, f),
                Format::Shallow(depth) => format_shallow(value, *depth, 0, f),
                Format::NumberForm { digits } => format_number_form(value, *digits, f),
                Format::ScientificForm { digits } => format_scientific_form(value, *digits, f),
                Format::BaseForm(base) => format_base_form(value, *base, f),
                Format::Grid => format_grid(value, f),
                Format::Deferred => write!(f, "{}", value),
            },
        }
    }
}

/// Format a Dataset value as a pretty-printed table (for list-of-assoc) or fallback.
fn format_dataset(data: &Value, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match data {
        Value::List(items) if items.iter().all(|v| matches!(v, Value::Assoc(_))) => {
            // Collect column union (preserve first-occurrence order across all rows)
            let mut columns: Vec<String> = Vec::new();
            for item in items {
                if let Value::Assoc(map) = item {
                    for key in map.keys() {
                        if !columns.contains(key) {
                            columns.push(key.clone());
                        }
                    }
                }
            }
            if columns.is_empty() {
                return write!(f, "Dataset[{{}}]");
            }
            // Measure column widths (header + values)
            let mut widths: Vec<usize> = columns.iter().map(|c| c.len()).collect();
            for item in items {
                if let Value::Assoc(map) = item {
                    for (i, key) in columns.iter().enumerate() {
                        let val_str = map.get(key).map(|v| v.to_string()).unwrap_or_default();
                        let val_len = val_str.len();
                        widths[i] = widths[i].max(val_len);
                    }
                }
            }
            // Helper to write a padded cell
            let write_cell = |f: &mut fmt::Formatter<'_>, s: &str, width: usize| -> fmt::Result {
                write!(f, " {:<width$} ", s, width = width)
            };
            // Header
            write!(f, "|")?;
            for (i, col) in columns.iter().enumerate() {
                write!(f, " ")?;
                write_cell(f, col, widths[i])?;
                write!(f, "|")?;
            }
            writeln!(f)?;
            // Separator
            write!(f, "|")?;
            for w in &widths {
                write!(f, "{}", "-".repeat(w + 2))?;
                write!(f, "|")?;
            }
            writeln!(f)?;
            // Rows
            for item in items {
                if let Value::Assoc(map) = item {
                    write!(f, "|")?;
                    for (i, key) in columns.iter().enumerate() {
                        write!(f, " ")?;
                        let val_str = map.get(key).map(|v| v.to_string()).unwrap_or_default();
                        write_cell(f, &val_str, widths[i])?;
                        write!(f, "|")?;
                    }
                    writeln!(f)?;
                }
            }
            Ok(())
        }
        _ => {
            // Fallback: display as Dataset[inner]
            write!(f, "Dataset[{}]", data)
        }
    }
}

// ── Format display helpers ──────────────────────────────────────────────────────

/// Format a value in InputForm (infix notation) — recursive.
///
/// Common operators are rendered with infix syntax:
/// `Plus[a, b]` → `a + b`, `Times[a, b]` → `a * b`, etc.
/// Child values are also formatted in InputForm recursively.
fn format_input_form(v: &Value, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match v {
        Value::Call { head, args } => {
            // Known infix operators
            match head.as_str() {
                "Plus" if args.len() == 1 => format_input_form(&args[0], f),
                "Plus" => {
                    for (i, arg) in args.iter().enumerate() {
                        if i > 0 {
                            write!(f, " + ")?;
                        }
                        format_input_form(arg, f)?;
                    }
                    Ok(())
                }
                "Times" if args.is_empty() => write!(f, "Times[]"),
                "Times" => {
                    for (i, arg) in args.iter().enumerate() {
                        if i > 0 {
                            write!(f, " ")?;
                        }
                        if matches!(
                            arg,
                            Value::Call { head: h, .. } if h == "Power" || h == "Plus"
                        ) || is_negative_number(arg)
                        {
                            write!(f, "(")?;
                            format_input_form(arg, f)?;
                            write!(f, ")")?;
                        } else {
                            format_input_form(arg, f)?;
                        }
                    }
                    Ok(())
                }
                "Power" if args.len() == 2 => {
                    let base = &args[0];
                    let exp = &args[1];
                    if matches!(
                        base,
                        Value::Call { head: h, .. } if h == "Times" || h == "Plus"
                    ) {
                        write!(f, "(")?;
                        format_input_form(base, f)?;
                        write!(f, ")")?;
                    } else {
                        format_input_form(base, f)?;
                    }
                    write!(f, "^")?;
                    format_input_form(exp, f)
                }
                "Divide" if args.len() == 2 => {
                    format_input_form(&args[0], f)?;
                    write!(f, "/")?;
                    format_input_form(&args[1], f)
                }
                "Minus" if args.len() == 1 => {
                    write!(f, "-")?;
                    format_input_form(&args[0], f)
                }
                "Equal" if args.len() == 2 => {
                    format_input_form(&args[0], f)?;
                    write!(f, " == ")?;
                    format_input_form(&args[1], f)
                }
                "Unequal" if args.len() == 2 => {
                    format_input_form(&args[0], f)?;
                    write!(f, " != ")?;
                    format_input_form(&args[1], f)
                }
                "Less" if args.len() == 2 => {
                    format_input_form(&args[0], f)?;
                    write!(f, " < ")?;
                    format_input_form(&args[1], f)
                }
                "Greater" if args.len() == 2 => {
                    format_input_form(&args[0], f)?;
                    write!(f, " > ")?;
                    format_input_form(&args[1], f)
                }
                "LessEqual" if args.len() == 2 => {
                    format_input_form(&args[0], f)?;
                    write!(f, " <= ")?;
                    format_input_form(&args[1], f)
                }
                "GreaterEqual" if args.len() == 2 => {
                    format_input_form(&args[0], f)?;
                    write!(f, " >= ")?;
                    format_input_form(&args[1], f)
                }
                "And" if args.len() >= 2 => {
                    for (i, arg) in args.iter().enumerate() {
                        if i > 0 {
                            write!(f, " && ")?;
                        }
                        format_input_form(arg, f)?;
                    }
                    Ok(())
                }
                "Or" if args.len() >= 2 => {
                    for (i, arg) in args.iter().enumerate() {
                        if i > 0 {
                            write!(f, " || ")?;
                        }
                        format_input_form(arg, f)?;
                    }
                    Ok(())
                }
                "Not" if args.len() == 1 => {
                    write!(f, "!")?;
                    format_input_form(&args[0], f)
                }
                "Rule" if args.len() == 2 => {
                    format_input_form(&args[0], f)?;
                    write!(f, " -> ")?;
                    format_input_form(&args[1], f)
                }
                "RuleDelayed" if args.len() == 2 => {
                    format_input_form(&args[0], f)?;
                    write!(f, " :> ")?;
                    format_input_form(&args[1], f)
                }
                "List" => {
                    write!(f, "{{")?;
                    for (i, arg) in args.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        format_input_form(arg, f)?;
                    }
                    write!(f, "}}")
                }
                "Part" if args.len() >= 2 => {
                    format_input_form(&args[0], f)?;
                    write!(f, "[[")?;
                    for (i, arg) in args[1..].iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        format_input_form(arg, f)?;
                    }
                    write!(f, "]]")
                }
                "Apply" if args.len() == 2 => {
                    format_input_form(&args[0], f)?;
                    write!(f, " @@ ")?;
                    format_input_form(&args[1], f)
                }
                "Map" if args.len() == 2 => {
                    format_input_form(&args[0], f)?;
                    write!(f, " /@ ")?;
                    format_input_form(&args[1], f)
                }
                "StringJoin" if args.len() >= 2 => {
                    for (i, arg) in args.iter().enumerate() {
                        if i > 0 {
                            write!(f, " <> ")?;
                        }
                        format_input_form(arg, f)?;
                    }
                    Ok(())
                }
                // Default: FullForm with recursive InputForm children
                _ => {
                    write!(f, "{}[", head)?;
                    for (i, arg) in args.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        format_input_form(arg, f)?;
                    }
                    write!(f, "]")
                }
            }
        }
        _ => write!(f, "{}", v),
    }
}

/// Check if a value is a negative number (for parenthesization in Times).
fn is_negative_number(v: &Value) -> bool {
    match v {
        Value::Integer(n) => n < &rug::Integer::from(0),
        Value::Real(r) => r.is_sign_negative(),
        Value::Rational(r) => r.is_negative(),
        _ => false,
    }
}

/// Format with truncation — at most `n` top-level items.
fn format_short(v: &Value, n: usize, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match v {
        Value::List(items) if items.len() > n => {
            write!(f, "{{")?;
            for (i, item) in items.iter().take(n).enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write!(f, "{}", item)?;
            }
            write!(f, ", <<{}>>", items.len() - n)?;
            write!(f, "}}")
        }
        Value::Call { head, args } if args.len() > n => {
            write!(f, "{}[", head)?;
            for (i, arg) in args.iter().take(n).enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write!(f, "{}", arg)?;
            }
            write!(f, ", <<{}>>", args.len() - n)?;
            write!(f, "]")
        }
        // Non-truncatable or small: fallback to standard rendering
        _ => write!(f, "{}", v),
    }
}

/// Format with limited nesting depth.
fn format_shallow(
    v: &Value,
    depth: usize,
    current: usize,
    f: &mut fmt::Formatter<'_>,
) -> fmt::Result {
    if current >= depth {
        return match v {
            Value::List(_) => write!(f, "{{<<...>>}}"),
            Value::Call { head, .. } => write!(f, "{}[<<...>>]", head),
            Value::Assoc(_) => write!(f, "<|<<...>>|>"),
            _ => write!(f, "{}", v),
        };
    }
    match v {
        Value::List(items) => {
            write!(f, "{{")?;
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                format_shallow(item, depth, current + 1, f)?;
            }
            write!(f, "}}")
        }
        Value::Call { head, args } => {
            write!(f, "{}[", head)?;
            for (i, arg) in args.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                format_shallow(arg, depth, current + 1, f)?;
            }
            write!(f, "]")
        }
        // Atoms pass through
        _ => write!(f, "{}", v),
    }
}

/// Format a number with the given number of significant digits.
fn format_number_form(v: &Value, digits: usize, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match v {
        Value::Integer(n) => write!(f, "{}", n),
        Value::Real(r) => {
            let prec_bits = ((digits as f64) / std::f64::consts::LOG10_2).ceil() as u32;
            let rounded = Float::with_val(prec_bits.max(64), r);
            let raw = rounded.to_string_radix(10, Some(digits.max(1)));
            let formatted = rug_radix_to_decimal(&raw);
            write!(f, "{}", formatted)
        }
        Value::Rational(r) => {
            let f_val = Float::with_val(64, r.numer()) / Float::with_val(64, r.denom());
            format_number_form(&Value::Real(f_val), digits, f)
        }
        _ => write!(f, "{}", v),
    }
}

/// Format a number in scientific notation with the given significant digits.
fn format_scientific_form(v: &Value, digits: usize, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match v {
        Value::Integer(n) => {
            let s = n.to_string();
            let exp = s.len() - 1;
            let first = &s[..1];
            let rest = if s.len() > 1 { &s[1..] } else { "" };
            if rest.is_empty() {
                write!(f, "{}·10^{}", first, exp)
            } else {
                write!(f, "{}.{}·10^{}", first, rest, exp)
            }
        }
        Value::Real(r) => {
            let prec_bits = ((digits as f64) / std::f64::consts::LOG10_2).ceil() as u32;
            let rounded = Float::with_val(prec_bits.max(64), r);
            // Use rug's to_string_radix with exponent marker '@'
            let raw = rounded.to_string_radix(10, Some(digits.max(1)));
            // raw is like "3.14159@0" (mantissa @ exponent)
            if let Some(at) = raw.find('@') {
                let mantissa = &raw[..at];
                let exp: i64 = raw[at + 1..].parse().unwrap_or(0);
                let adjusted_exp = exp + (mantissa.len() - 2) as i64; // account for decimal point
                write!(f, "{}·10^{}", mantissa, adjusted_exp)
            } else {
                // No exponent, probably 0
                write!(f, "{}·10^{}", raw, 0)
            }
        }
        Value::Rational(r) => {
            let f_val = Float::with_val(64, r.numer()) / Float::with_val(64, r.denom());
            format_scientific_form(&Value::Real(f_val), digits, f)
        }
        _ => write!(f, "{}", v),
    }
}

/// Format a number in a given base (2–36).
fn format_base_form(v: &Value, base: u32, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match v {
        Value::Integer(n) => {
            let s = n.to_string_radix(base as i32);
            write!(f, "{}(base {})", s, base)
        }
        Value::Real(r) => {
            // Convert to a rational approximation and show integer part in base
            let integer_part = r.clone().floor().to_integer().unwrap();
            let int_str = integer_part.to_string_radix(base as i32);
            write!(f, "{}(base {})", int_str, base)
        }
        Value::Rational(r) => {
            let f_val = Float::with_val(64, r.numer()) / Float::with_val(64, r.denom());
            format_base_form(&Value::Real(f_val), base, f)
        }
        _ => write!(f, "{}", v),
    }
}

/// Format a 2D list as a grid (aligned columns).
fn format_grid(v: &Value, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let rows = match v {
        Value::List(rows) => rows,
        other => return write!(f, "Grid[{}]", other),
    };
    if rows.is_empty() {
        return write!(f, "{{}}");
    }

    // Convert each cell to a string
    let mut cell_strings: Vec<Vec<String>> = Vec::new();
    let mut col_widths: Vec<usize> = Vec::new();
    for row_val in rows {
        let row = match row_val {
            Value::List(cells) => cells,
            _ => {
                let s = format!("{}", row_val);
                return write!(f, "{}", s);
            }
        };
        let mut row_strs: Vec<String> = Vec::new();
        for (col_idx, cell) in row.iter().enumerate() {
            let s = format!("{}", cell);
            if col_idx >= col_widths.len() {
                col_widths.push(s.len());
            } else {
                col_widths[col_idx] = col_widths[col_idx].max(s.len());
            }
            row_strs.push(s);
        }
        cell_strings.push(row_strs);
    }

    // Render table
    for (row_idx, row) in cell_strings.iter().enumerate() {
        if row_idx > 0 {
            writeln!(f)?;
        }
        for (col_idx, cell) in row.iter().enumerate() {
            if col_idx > 0 {
                write!(f, "  ")?;
            }
            let width = col_widths.get(col_idx).copied().unwrap_or(0);
            write!(f, "{:width$}", cell, width = width)?;
        }
    }
    Ok(())
}
