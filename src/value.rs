/// Runtime values for Syma language.
///
/// Everything in Syma evaluates to a Value. Values are the runtime
/// representation of symbolic expressions.
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

use rug::Float;
use rug::Integer;

use crate::ast::Expr;

/// Default precision for floating-point numbers (128 bits ≈ 38 decimal digits).
pub const DEFAULT_PRECISION: u32 = 128;

/// Convert rug's `to_string_radix` output (e.g. `"3.14159e0"`, `"-1.5e2"`)
/// into standard decimal notation (e.g. `"3.14159"`, `"-150.0"`).
/// Rug uses `e` as exponent marker for base 10 (MPFR convention).
pub fn rug_radix_to_decimal(s: &str) -> String {
    // Split on 'e' or '@' to get mantissa and base-10 exponent.
    let sep_pos = s[1..].find(|c| c == 'e' || c == '@').map(|i| i + 1);
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
    Function(Rc<FunctionDef>),

    /// A built-in function.
    Builtin(String, BuiltinFn),

    /// A pure function (lambda) with slots.
    PureFunction {
        body: Expr,
        #[allow(dead_code)]
        slot_count: usize,
    },

    /// A method bound to an object.
    #[allow(dead_code)]
    Method {
        name: String,
        object: Box<Value>,
    },

    // ── Objects ──
    /// A class instance.
    #[allow(dead_code)]
    Object {
        class_name: String,
        fields: HashMap<String, Value>,
    },

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
    },

    // ── Hold ──
    Hold(Box<Value>),
    HoldComplete(Box<Value>),
}

/// A built-in function implementation.
pub type BuiltinFn = fn(&[Value]) -> Result<Value, EvalError>;

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
    #[allow(dead_code)]
    pub delayed: bool,
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
    #[allow(dead_code)]
    UnknownSymbol(String),
    /// User-thrown error.
    #[allow(dead_code)]
    Thrown(Value),
    /// General error message.
    Error(String),
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
        }
    }
}

impl std::error::Error for EvalError {}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Integer(a), Value::Integer(b)) => a == b,
            (Value::Real(a), Value::Real(b)) => a == b,
            (Value::Complex { re: a1, im: a2 }, Value::Complex { re: b1, im: b2 }) => {
                a1 == b1 && a2 == b2
            }
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Null, Value::Null) => true,
            (Value::Symbol(a), Value::Symbol(b)) => a == b,
            (Value::List(a), Value::List(b)) => a == b,
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
                },
                Value::Module {
                    name: n2,
                    exports: e2,
                },
            ) => n1 == n2 && e1 == e2,
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
            Value::Complex { .. } => "Complex",
            Value::Str(_) => "String",
            Value::Bool(_) => "Boolean",
            Value::Null => "Null",
            Value::Symbol(_) => "Symbol",
            Value::List(_) => "List",
            Value::Call { .. } => "Expr",
            Value::Assoc(_) => "Assoc",
            Value::Rule { .. } => "Rule",
            Value::Function(_) => "Function",
            Value::Builtin(_, _) => "Builtin",
            Value::PureFunction { .. } => "PureFunction",
            Value::Method { .. } => "Method",
            Value::Object { class_name, .. } => class_name,
            Value::RuleSet { .. } => "RuleSet",
            Value::Pattern(_) => "Pattern",
            Value::Module { .. } => "Module",
            Value::Hold(_) => "Hold",
            Value::HoldComplete(_) => "HoldComplete",
        }
    }

    /// Check if this value matches a type pattern.
    pub fn matches_type(&self, type_name: &str) -> bool {
        match type_name {
            "Number" => matches!(
                self,
                Value::Integer(_) | Value::Real(_) | Value::Complex { .. }
            ),
            "Integer" => matches!(self, Value::Integer(_)),
            "Real" => matches!(self, Value::Real(_)),
            "Complex" => matches!(self, Value::Complex { .. }),
            "String" => matches!(self, Value::Str(_)),
            "Boolean" => matches!(self, Value::Bool(_)),
            "Symbol" => matches!(self, Value::Symbol(_)),
            "List" => matches!(self, Value::List(_)),
            "Assoc" => matches!(self, Value::Assoc(_)),
            "Rule" => matches!(self, Value::Rule { .. }),
            "Function" => matches!(
                self,
                Value::Function(_) | Value::Builtin(_, _) | Value::PureFunction { .. }
            ),
            "Module" => matches!(self, Value::Module { .. }),
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
            Value::Bool(b) => *b,
            Value::Integer(n) => !n.is_zero(),
            Value::Real(r) => !r.is_zero(),
            Value::Null => false,
            Value::List(l) => !l.is_empty(),
            _ => true,
        }
    }

    /// Try to convert to i64.
    pub fn to_integer(&self) -> Option<i64> {
        match self {
            Value::Integer(n) => Some(n.to_i64().unwrap_or(0)),
            Value::Real(r) => Some(r.to_f64() as i64),
            _ => None,
        }
    }

    /// Try to convert to f64.
    #[allow(dead_code)]
    pub fn to_real(&self) -> Option<f64> {
        match self {
            Value::Integer(n) => Some(n.to_f64()),
            Value::Real(r) => Some(r.to_f64()),
            Value::Complex { re, im } if *im == 0.0 => Some(*re),
            _ => None,
        }
    }

    /// Check if two values are structurally equal.
    pub fn struct_eq(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Integer(a), Value::Integer(b)) => a == b,
            (Value::Real(a), Value::Real(b)) => a == b,
            (Value::Complex { re: a1, im: a2 }, Value::Complex { re: b1, im: b2 }) => {
                a1 == b1 && a2 == b2
            }
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Null, Value::Null) => true,
            (Value::Symbol(a), Value::Symbol(b)) => a == b,
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
            Value::RuleSet { name, .. } => write!(f, "RuleSet[{}]", name),
            Value::Pattern(expr) => write!(f, "Pattern[{}]", expr),
            Value::Module { name, exports } => {
                write!(
                    f,
                    "Module[{}, {{{}}}]",
                    name,
                    exports.keys().cloned().collect::<Vec<_>>().join(", ")
                )
            }
            Value::Hold(v) => write!(f, "Hold[{}]", v),
            Value::HoldComplete(v) => write!(f, "HoldComplete[{}]", v),
        }
    }
}
