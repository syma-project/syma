use crate::builtins::arithmetic::{builtin_divide, builtin_plus};
use crate::value::{DEFAULT_PRECISION, EvalError, Value};
use rug::Float;
use rug::Integer;

// ── Pi-multiple detection for exact trig results ──

/// Known special angle as (numerator, denominator) in [0, 2π) range.
type PiFrac = (i64, u32);

/// Tolerance for comparing `r / pi` to a known rational.
fn trig_epsilon() -> Float {
    Float::with_val(DEFAULT_PRECISION, 1e-30)
}

/// Try to extract a Pi-multiple from a symbolic expression.
/// Returns Some((num, den)) if the expression is Pi, Pi/n, n*Pi, or n*Pi/m.
fn extract_pi_multiple(val: &Value) -> Option<(i64, u32)> {
    match val {
        // Pi itself → 1*Pi
        Value::Symbol(s) if s == "Pi" => Some((1, 1)),
        // Divide[Pi, n] or Times[Pi, Power[n, -1]]
        Value::Call { head, args } if head == "Divide" && args.len() == 2 => {
            if let (Value::Symbol(s), Value::Integer(d)) = (&args[0], &args[1]) {
                if s == "Pi" && !d.is_zero() && !d.is_negative() {
                    return d.to_u32().map(|den| (1, den));
                }
            }
            None
        }
        // Times[n, Pi] or Times[Pi, n]
        Value::Call { head, args } if head == "Times" && args.len() == 2 => {
            match (&args[0], &args[1]) {
                (Value::Integer(n), Value::Symbol(s))
                | (Value::Symbol(s), Value::Integer(n))
                    if s == "Pi" =>
                {
                    if !n.is_negative() && !n.is_zero() {
                        return n.to_u32().map(|num| (num as i64, 1));
                    }
                    None
                }
                // Times[n, Divide[Pi, d]] or similar — skip for now
                _ => None,
            }
        }
        _ => None,
    }
}

/// Check if `r` is any rational multiple of Pi (not just special angles).
fn is_pi_multiple(r: &Float) -> bool {
    let prec = r.prec().max(DEFAULT_PRECISION);
    let pi = Float::with_val(prec, rug::float::Constant::Pi);
    let ratio = Float::with_val(prec, r) / &pi;
    // Check if ratio is close to a rational with small denominator
    let eps = trig_epsilon();
    for den in [1u32, 2, 3, 4, 6, 5, 8, 10, 12] {
        let scaled = Float::with_val(prec, &ratio * Float::with_val(prec, den));
        let rounded = Float::with_val(prec, &scaled).round();
        let diff = Float::with_val(prec, &scaled - &rounded).abs();
        if diff < eps {
            return true;
        }
    }
    false
}

/// If `r` is a known rational multiple of Pi, return `Some((num, den))`
/// where the equivalent angle is `num/den * Pi` in [0, 2π).
fn pi_multiple(r: &Float) -> Option<PiFrac> {
    let prec = r.prec().max(DEFAULT_PRECISION);
    let pi = Float::with_val(prec, rug::float::Constant::Pi);
    let ratio = Float::with_val(prec, r) / &pi;

    // Reduce to [0, 2) range: ratio = 2*k + frac, frac ∈ [0, 2)
    let two = Float::with_val(prec, 2u32);
    let k = Float::with_val(prec, &ratio / &two).floor();
    let kt = Float::with_val(prec, &k * &two);
    let frac = Float::with_val(prec, &ratio - &kt);

    let eps = trig_epsilon();
    let candidates: &[(i64, u32)] = &[
        (0, 1),  // 0
        (1, 6),  // 1/6
        (1, 4),  // 1/4
        (1, 3),  // 1/3
        (1, 2),  // 1/2
        (2, 3),  // 2/3
        (3, 4),  // 3/4
        (5, 6),  // 5/6
        (1, 1),  // 1
        (7, 6),  // 7/6
        (5, 4),  // 5/4
        (4, 3),  // 4/3
        (3, 2),  // 3/2
        (5, 3),  // 5/3
        (7, 4),  // 7/4
        (11, 6), // 11/6
    ];

    for &(num, den) in candidates {
        let target = Float::with_val(prec, num) / Float::with_val(prec, den);
        let diff_val = Float::with_val(prec, &frac - &target);
        let diff = diff_val.abs();
        if diff < eps {
            return Some((num, den));
        }
    }

    None
}

// ── Symbolic value constructors for exact trig results ──

fn val_int(n: i64) -> Value {
    Value::Integer(Integer::from(n))
}

/// Sqrt[n] as a symbolic Call
fn val_sqrt(n: i64) -> Value {
    Value::Call {
        head: "Sqrt".to_string(),
        args: vec![val_int(n)],
    }
}

/// Divide[a, b] as a symbolic Call
fn val_div(a: Value, b: Value) -> Value {
    Value::Call {
        head: "Divide".to_string(),
        args: vec![a, b],
    }
}

/// -v as Times[-1, v] (or Integer for simple cases)
fn val_neg(v: Value) -> Value {
    match v {
        Value::Integer(n) => Value::Integer(-n),
        other => Value::Call {
            head: "Times".to_string(),
            args: vec![val_int(-1), other],
        },
    }
}

/// Sqrt[n] / d  →  Divide[Sqrt[n], d]
fn sqrt_over(n: i64, d: i64) -> Value {
    val_div(val_sqrt(n), val_int(d))
}

/// 1 / d  →  Divide[1, d]
fn one_over(d: i64) -> Value {
    val_div(val_int(1), val_int(d))
}

/// -Sqrt[n] / d
fn neg_sqrt_over(n: i64, d: i64) -> Value {
    val_neg(sqrt_over(n, d))
}

/// -1 / d
fn neg_one_over(d: i64) -> Value {
    val_neg(one_over(d))
}

/// Compute exact sin for a known Pi fraction. (num, den) is in [0, 2) range.
fn exact_sin(num: i64, den: u32) -> Option<Value> {
    let result = match (num, den) {
        (0, _) | (1, 1) => val_int(0),           // sin(0) = sin(π) = 0
        (1, 6) | (5, 6) => one_over(2),           // sin(π/6) = sin(5π/6) = 1/2
        (1, 4) | (3, 4) => sqrt_over(2, 2),       // sin(π/4) = sin(3π/4) = √2/2
        (1, 3) | (2, 3) => sqrt_over(3, 2),       // sin(π/3) = sin(2π/3) = √3/2
        (1, 2) => val_int(1),                      // sin(π/2) = 1
        (7, 6) | (11, 6) => neg_one_over(2),       // sin(7π/6) = sin(11π/6) = -1/2
        (5, 4) | (7, 4) => neg_sqrt_over(2, 2),   // sin(5π/4) = sin(7π/4) = -√2/2
        (4, 3) | (5, 3) => neg_sqrt_over(3, 2),   // sin(4π/3) = sin(5π/3) = -√3/2
        (3, 2) => val_int(-1),                     // sin(3π/2) = -1
        _ => return None,
    };
    Some(result)
}

/// Compute exact cos for a known Pi fraction. (num, den) is in [0, 2) range.
fn exact_cos(num: i64, den: u32) -> Option<Value> {
    let result = match (num, den) {
        (0, _) => val_int(1),                      // cos(0) = 1
        (1, 6) | (11, 6) => sqrt_over(3, 2),       // cos(π/6) = cos(11π/6) = √3/2
        (1, 4) | (7, 4) => sqrt_over(2, 2),        // cos(π/4) = cos(7π/4) = √2/2
        (1, 3) | (5, 3) => one_over(2),            // cos(π/3) = cos(5π/3) = 1/2
        (1, 2) | (3, 2) => val_int(0),             // cos(π/2) = cos(3π/2) = 0
        (2, 3) | (4, 3) => neg_one_over(2),        // cos(2π/3) = cos(4π/3) = -1/2
        (3, 4) | (5, 4) => neg_sqrt_over(2, 2),    // cos(3π/4) = cos(5π/4) = -√2/2
        (5, 6) | (7, 6) => neg_sqrt_over(3, 2),    // cos(5π/6) = cos(7π/6) = -√3/2
        (1, 1) => val_int(-1),                     // cos(π) = -1
        _ => return None,
    };
    Some(result)
}

/// Compute exact tan for a known Pi fraction. (num, den) is in [0, 2) range.
fn exact_tan(num: i64, den: u32) -> Option<Value> {
    // tan is undefined at π/2 + kπ
    if den == 2 && (num == 1 || num == 3) {
        return None;
    }
    let result = match (num, den) {
        (0, _) | (1, 1) => val_int(0),             // tan(0) = tan(π) = 0
        (1, 6) | (7, 6) => val_div(val_int(1), val_sqrt(3)),   // 1/√3
        (1, 4) | (5, 4) => val_int(1),             // tan(π/4) = tan(5π/4) = 1
        (1, 3) | (4, 3) => val_sqrt(3),            // tan(π/3) = tan(4π/3) = √3
        (2, 3) | (5, 3) => val_neg(val_sqrt(3)),   // -√3
        (3, 4) | (7, 4) => val_int(-1),            // -1
        (5, 6) | (11, 6) => val_neg(val_div(val_int(1), val_sqrt(3))),  // -1/√3
        _ => return None,
    };
    Some(result)
}

// ── Trigonometric ──

pub fn builtin_sin(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Sin requires exactly 1 argument".to_string(),
        ));
    }
    // Symbolic Pi-multiple detection (before numerical eval)
    if let Some((num, den)) = extract_pi_multiple(&args[0]) {
        if let Some(result) = exact_sin(num, den) {
            return Ok(result);
        }
        // Pi-multiple but not a special angle — keep symbolic
        return Ok(Value::Call {
            head: "Sin".to_string(),
            args: args.to_vec(),
        });
    }
    match &args[0] {
        Value::Integer(n) if n.is_zero() => Ok(Value::Integer(Integer::from(0))),
        Value::Integer(_) => Ok(Value::Call {
            head: "Sin".to_string(),
            args: args.to_vec(),
        }),
        Value::Real(r) => {
            if let Some((num, den)) = pi_multiple(r) {
                if let Some(result) = exact_sin(num, den) {
                    return Ok(result);
                }
            }
            if is_pi_multiple(r) {
                return Ok(Value::Call {
                    head: "Sin".to_string(),
                    args: args.to_vec(),
                });
            }
            Ok(Value::Real(r.clone().sin()))
        }
        _ => Ok(Value::Call {
            head: "Sin".to_string(),
            args: args.to_vec(),
        }),
    }
}

pub fn builtin_cos(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Cos requires exactly 1 argument".to_string(),
        ));
    }
    if let Some((num, den)) = extract_pi_multiple(&args[0]) {
        if let Some(result) = exact_cos(num, den) {
            return Ok(result);
        }
        return Ok(Value::Call {
            head: "Cos".to_string(),
            args: args.to_vec(),
        });
    }
    match &args[0] {
        Value::Integer(n) if n.is_zero() => Ok(Value::Integer(Integer::from(1))),
        Value::Integer(_) => Ok(Value::Call {
            head: "Cos".to_string(),
            args: args.to_vec(),
        }),
        Value::Real(r) => {
            if let Some((num, den)) = pi_multiple(r) {
                if let Some(result) = exact_cos(num, den) {
                    return Ok(result);
                }
            }
            if is_pi_multiple(r) {
                return Ok(Value::Call {
                    head: "Cos".to_string(),
                    args: args.to_vec(),
                });
            }
            Ok(Value::Real(r.clone().cos()))
        }
        _ => Ok(Value::Call {
            head: "Cos".to_string(),
            args: args.to_vec(),
        }),
    }
}

pub fn builtin_tan(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Tan requires exactly 1 argument".to_string(),
        ));
    }
    if let Some((num, den)) = extract_pi_multiple(&args[0]) {
        if let Some(result) = exact_tan(num, den) {
            return Ok(result);
        }
        return Ok(Value::Call {
            head: "Tan".to_string(),
            args: args.to_vec(),
        });
    }
    match &args[0] {
        Value::Integer(n) if n.is_zero() => Ok(Value::Integer(Integer::from(0))),
        Value::Integer(_) => Ok(Value::Call {
            head: "Tan".to_string(),
            args: args.to_vec(),
        }),
        Value::Real(r) => {
            if let Some((num, den)) = pi_multiple(r) {
                if let Some(result) = exact_tan(num, den) {
                    return Ok(result);
                }
            }
            if is_pi_multiple(r) {
                return Ok(Value::Call {
                    head: "Tan".to_string(),
                    args: args.to_vec(),
                });
            }
            Ok(Value::Real(r.clone().tan()))
        }
        _ => Ok(Value::Call {
            head: "Tan".to_string(),
            args: args.to_vec(),
        }),
    }
}

pub fn builtin_arcsin(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "ArcSin requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) => {
            let f = Float::with_val(DEFAULT_PRECISION, n);
            Ok(Value::Real(f.asin()))
        }
        Value::Real(r) => Ok(Value::Real(r.clone().asin())),
        _ => Ok(Value::Call {
            head: "ArcSin".to_string(),
            args: args.to_vec(),
        }),
    }
}

pub fn builtin_arccos(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "ArcCos requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) => {
            let f = Float::with_val(DEFAULT_PRECISION, n);
            Ok(Value::Real(f.acos()))
        }
        Value::Real(r) => Ok(Value::Real(r.clone().acos())),
        _ => Ok(Value::Call {
            head: "ArcCos".to_string(),
            args: args.to_vec(),
        }),
    }
}

pub fn builtin_arctan(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "ArcTan requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) => {
            let f = Float::with_val(DEFAULT_PRECISION, n);
            Ok(Value::Real(f.atan()))
        }
        Value::Real(r) => Ok(Value::Real(r.clone().atan())),
        _ => Ok(Value::Call {
            head: "ArcTan".to_string(),
            args: args.to_vec(),
        }),
    }
}

// ── Reciprocal Trigonometric (exact results for special angles) ──

/// Compute exact csc for a known Pi fraction.
fn exact_csc(num: i64, den: u32) -> Option<Value> {
    match (num, den) {
        (0, _) | (1, 1) => None,                       // csc(0), csc(π) undefined
        (1, 2) | (3, 2) => Some(val_int(1)),            // csc(±π/2) = 1 or -1
        (1, 6) | (5, 6) => Some(val_int(2)),            // csc(π/6) = csc(5π/6) = 2
        (1, 4) | (3, 4) => Some(val_sqrt(2)),           // csc(π/4) = csc(3π/4) = √2
        (1, 3) | (2, 3) => Some(sqrt_over(2, 3)),       // csc(π/3) = csc(2π/3) = 2√3/3
        (7, 6) | (11, 6) => Some(val_int(-2)),
        (5, 4) | (7, 4) => Some(val_neg(val_sqrt(2))),
        (4, 3) | (5, 3) => Some(neg_sqrt_over(2, 3)),
        _ => None,
    }
}

/// Compute exact sec for a known Pi fraction.
fn exact_sec(num: i64, den: u32) -> Option<Value> {
    match (num, den) {
        (0, _) => Some(val_int(1)),                     // sec(0) = 1
        (1, 6) | (11, 6) => Some(sqrt_over(2, 3)),      // sec(π/6) = sec(11π/6) = 2√3/3
        (1, 4) | (7, 4) => Some(val_sqrt(2)),           // sec(π/4) = sec(7π/4) = √2
        (1, 3) | (5, 3) => Some(val_int(2)),            // sec(π/3) = sec(5π/3) = 2
        (1, 6) | (11, 6) => Some(sqrt_over(2, 3)),      // sec(π/6) = 2√3/3
        (1, 4) | (7, 4) => Some(val_sqrt(2)),           // sec(π/4) = √2
        (1, 3) | (5, 3) => Some(val_int(2)),            // sec(π/3) = 2
        (1, 2) | (3, 2) => None,                        // sec(±π/2) undefined
        (2, 3) | (4, 3) => Some(val_int(-2)),
        (3, 4) | (5, 4) => Some(val_neg(val_sqrt(2))),
        (5, 6) | (7, 6) => Some(neg_sqrt_over(2, 3)),
        (1, 1) => Some(val_int(-1)),                    // sec(π) = -1
        _ => None,
    }
}

/// Compute exact cot for a known Pi fraction.
fn exact_cot(num: i64, den: u32) -> Option<Value> {
    match (num, den) {
        (0, _) | (1, 1) => None,                       // cot(0), cot(π) undefined
        (1, 2) | (3, 2) => Some(val_int(0)),            // cot(π/2) = cot(3π/2) = 0
        (1, 6) | (7, 6) => Some(val_sqrt(3)),           // cot(π/6) = cot(7π/6) = √3
        (1, 4) | (5, 4) => Some(val_int(1)),            // cot(π/4) = cot(5π/4) = 1
        (1, 3) | (4, 3) => Some(val_div(val_sqrt(3), val_int(3))), // cot(π/3) = √3/3
        (1, 2) | (3, 2) => Some(val_int(0)),            // cot(π/2) = 0
        (1, 6) | (7, 6) => Some(val_sqrt(3)),           // cot(π/6) = √3
        (1, 4) | (5, 4) => Some(val_int(1)),            // cot(π/4) = 1
        (1, 3) | (4, 3) => Some(val_div(val_sqrt(3), val_int(3))), // √3/3
        (2, 3) | (5, 3) => Some(val_neg(val_div(val_sqrt(3), val_int(3)))),
        (3, 4) | (7, 4) => Some(val_int(-1)),
        (5, 6) | (11, 6) => Some(val_neg(val_sqrt(3))),
        _ => None,
    }
}

pub fn builtin_csc(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("Csc requires exactly 1 argument".to_string()));
    }
    if let Some((num, den)) = extract_pi_multiple(&args[0]) {
        if let Some(result) = exact_csc(num, den) {
            return Ok(result);
        }
        return Ok(Value::Call { head: "Csc".to_string(), args: args.to_vec() });
    }
    match &args[0] {
        Value::Integer(n) if n.is_zero() => {
            Ok(Value::Call { head: "Csc".to_string(), args: args.to_vec() })
        }
        Value::Integer(_) => {
            Ok(Value::Call { head: "Csc".to_string(), args: args.to_vec() })
        }
        Value::Real(r) => {
            if let Some((num, den)) = pi_multiple(r) {
                if let Some(result) = exact_csc(num, den) {
                    return Ok(result);
                }
            }
            let sin_val = r.clone().sin();
            if sin_val.is_zero() {
                return Ok(Value::Call { head: "Csc".to_string(), args: args.to_vec() });
            }
            let prec = r.prec();
            Ok(Value::Real(Float::with_val(prec, 1.0) / sin_val))
        }
        _ => Ok(Value::Call { head: "Csc".to_string(), args: args.to_vec() }),
    }
}

pub fn builtin_sec(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("Sec requires exactly 1 argument".to_string()));
    }
    if let Some((num, den)) = extract_pi_multiple(&args[0]) {
        if let Some(result) = exact_sec(num, den) {
            return Ok(result);
        }
        return Ok(Value::Call { head: "Sec".to_string(), args: args.to_vec() });
    }
    match &args[0] {
        Value::Integer(n) if n.is_zero() => Ok(Value::Integer(Integer::from(1))),
        Value::Integer(_) => {
            Ok(Value::Call { head: "Sec".to_string(), args: args.to_vec() })
        }
        Value::Real(r) => {
            if let Some((num, den)) = pi_multiple(r) {
                if let Some(result) = exact_sec(num, den) {
                    return Ok(result);
                }
            }
            let cos_val = r.clone().cos();
            if cos_val.is_zero() {
                return Ok(Value::Call { head: "Sec".to_string(), args: args.to_vec() });
            }
            let prec = r.prec();
            Ok(Value::Real(Float::with_val(prec, 1.0) / cos_val))
        }
        _ => Ok(Value::Call { head: "Sec".to_string(), args: args.to_vec() }),
    }
}

pub fn builtin_cot(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("Cot requires exactly 1 argument".to_string()));
    }
    if let Some((num, den)) = extract_pi_multiple(&args[0]) {
        if let Some(result) = exact_cot(num, den) {
            return Ok(result);
        }
        return Ok(Value::Call { head: "Cot".to_string(), args: args.to_vec() });
    }
    match &args[0] {
        Value::Integer(n) if n.is_zero() => {
            Ok(Value::Call { head: "Cot".to_string(), args: args.to_vec() })
        }
        Value::Integer(_) => {
            Ok(Value::Call { head: "Cot".to_string(), args: args.to_vec() })
        }
        Value::Real(r) => {
            if let Some((num, den)) = pi_multiple(r) {
                if let Some(result) = exact_cot(num, den) {
                    return Ok(result);
                }
            }
            let sin_val = r.clone().sin();
            if sin_val.is_zero() {
                return Ok(Value::Call { head: "Cot".to_string(), args: args.to_vec() });
            }
            let cos_val = r.clone().cos();
            let prec = r.prec();
            Ok(Value::Real(Float::with_val(prec, cos_val) / Float::with_val(prec, sin_val)))
        }
        _ => Ok(Value::Call { head: "Cot".to_string(), args: args.to_vec() }),
    }
}

// ── Inverse Reciprocal Trigonometric ──

pub fn builtin_arccsc(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("ArcCsc requires exactly 1 argument".to_string()));
    }
    match &args[0] {
        Value::Integer(n) if n.is_zero() => {
            Ok(Value::Call { head: "ArcCsc".to_string(), args: args.to_vec() })
        }
        Value::Integer(n) => {
            let f = Float::with_val(DEFAULT_PRECISION, n);
            let inv = Float::with_val(DEFAULT_PRECISION, 1.0) / f;
            Ok(Value::Real(inv.asin()))
        }
        Value::Real(r) if r.is_zero() => {
            Ok(Value::Call { head: "ArcCsc".to_string(), args: args.to_vec() })
        }
        Value::Real(r) => {
            let prec = r.prec();
            let inv = Float::with_val(prec, 1.0) / r;
            Ok(Value::Real(inv.asin()))
        }
        _ => Ok(Value::Call { head: "ArcCsc".to_string(), args: args.to_vec() }),
    }
}

pub fn builtin_arcsec(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("ArcSec requires exactly 1 argument".to_string()));
    }
    match &args[0] {
        Value::Integer(n) if n.is_zero() => {
            Ok(Value::Call { head: "ArcSec".to_string(), args: args.to_vec() })
        }
        Value::Integer(n) => {
            let f = Float::with_val(DEFAULT_PRECISION, n);
            let inv = Float::with_val(DEFAULT_PRECISION, 1.0) / f;
            Ok(Value::Real(inv.acos()))
        }
        Value::Real(r) if r.is_zero() => {
            Ok(Value::Call { head: "ArcSec".to_string(), args: args.to_vec() })
        }
        Value::Real(r) => {
            let prec = r.prec();
            let inv = Float::with_val(prec, 1.0) / r;
            Ok(Value::Real(inv.acos()))
        }
        _ => Ok(Value::Call { head: "ArcSec".to_string(), args: args.to_vec() }),
    }
}

pub fn builtin_arccot(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("ArcCot requires exactly 1 argument".to_string()));
    }
    match &args[0] {
        Value::Integer(n) if n.is_zero() => {
            // ArcCot[0] = π/2
            let pi_half = Float::with_val(DEFAULT_PRECISION, rug::float::Constant::Pi) / 2u32;
            Ok(Value::Real(pi_half))
        }
        Value::Integer(n) => {
            let f = Float::with_val(DEFAULT_PRECISION, n);
            let inv = Float::with_val(DEFAULT_PRECISION, 1.0) / f;
            Ok(Value::Real(inv.atan()))
        }
        Value::Real(r) if r.is_zero() => {
            let pi_half = Float::with_val(r.prec(), rug::float::Constant::Pi) / 2u32;
            Ok(Value::Real(pi_half))
        }
        Value::Real(r) => {
            let prec = r.prec();
            let inv = Float::with_val(prec, 1.0) / r;
            Ok(Value::Real(inv.atan()))
        }
        _ => Ok(Value::Call { head: "ArcCot".to_string(), args: args.to_vec() }),
    }
}

// ── Haversine ──

pub fn builtin_haversine(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("Haversine requires exactly 1 argument".to_string()));
    }
    // Haversine[x] = Sin[x/2]^2 = (1 - Cos[x]) / 2
    match &args[0] {
        Value::Integer(n) if n.is_zero() => Ok(Value::Integer(Integer::from(0))),
        Value::Integer(_) => {
            // Try symbolic: (1 - Cos[x]) / 2
            let cos_x = builtin_cos(&[args[0].clone()])?;
            let one_minus_cos = builtin_plus(&[Value::Integer(Integer::from(1)), val_neg(cos_x)])?;
            builtin_divide(&[one_minus_cos, Value::Integer(Integer::from(2))])
        }
        Value::Real(r) => {
            let cos_val = r.clone().cos();
            let prec = r.prec();
            let one = Float::with_val(prec, 1u32);
            let two = Float::with_val(prec, 2u32);
            Ok(Value::Real((one - cos_val) / two))
        }
        _ => Ok(Value::Call { head: "Haversine".to_string(), args: args.to_vec() }),
    }
}

pub fn builtin_inverse_haversine(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("InverseHaversine requires exactly 1 argument".to_string()));
    }
    // InverseHaversine[x] = 2 * ArcSin[Sqrt[x]]
    match &args[0] {
        Value::Integer(n) if n.is_zero() => Ok(Value::Integer(Integer::from(0))),
        Value::Integer(n) => {
            let f = Float::with_val(DEFAULT_PRECISION, n);
            if f.is_sign_negative() {
                return Err(EvalError::Error("InverseHaversine: argument must be >= 0".to_string()));
            }
            let sqrt_val = f.sqrt();
            let asin_val = sqrt_val.asin();
            Ok(Value::Real(Float::with_val(DEFAULT_PRECISION, 2) * asin_val))
        }
        Value::Real(r) => {
            if r.is_sign_negative() {
                return Err(EvalError::Error("InverseHaversine: argument must be >= 0".to_string()));
            }
            let prec = r.prec();
            let sqrt_val = r.clone().sqrt();
            let asin_val = sqrt_val.asin();
            Ok(Value::Real(Float::with_val(prec, 2u32) * asin_val))
        }
        _ => Ok(Value::Call { head: "InverseHaversine".to_string(), args: args.to_vec() }),
    }
}

// ── Degree-based Trigonometric ──

pub fn builtin_sin_degrees(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("SinDegrees requires exactly 1 argument".to_string()));
    }
    // SinDegrees[θ] = Sin[θ * π/180]
    let rad = degrees_to_radians(&args[0])?;
    builtin_sin(&[rad])
}

pub fn builtin_cos_degrees(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("CosDegrees requires exactly 1 argument".to_string()));
    }
    let rad = degrees_to_radians(&args[0])?;
    builtin_cos(&[rad])
}

pub fn builtin_tan_degrees(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("TanDegrees requires exactly 1 argument".to_string()));
    }
    let rad = degrees_to_radians(&args[0])?;
    builtin_tan(&[rad])
}

pub fn builtin_csc_degrees(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("CscDegrees requires exactly 1 argument".to_string()));
    }
    let rad = degrees_to_radians(&args[0])?;
    builtin_csc(&[rad])
}

pub fn builtin_sec_degrees(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("SecDegrees requires exactly 1 argument".to_string()));
    }
    let rad = degrees_to_radians(&args[0])?;
    builtin_sec(&[rad])
}

pub fn builtin_cot_degrees(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("CotDegrees requires exactly 1 argument".to_string()));
    }
    let rad = degrees_to_radians(&args[0])?;
    builtin_cot(&[rad])
}

// ── Inverse Trigonometric (Degrees) ──

pub fn builtin_arcsin_degrees(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("ArcSinDegrees requires exactly 1 argument".to_string()));
    }
    let rad = builtin_arcsin(args)?;
    radians_to_degrees(&rad)
}

pub fn builtin_arccos_degrees(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("ArcCosDegrees requires exactly 1 argument".to_string()));
    }
    let rad = builtin_arccos(args)?;
    radians_to_degrees(&rad)
}

pub fn builtin_arctan_degrees(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("ArcTanDegrees requires exactly 1 argument".to_string()));
    }
    let rad = builtin_arctan(args)?;
    radians_to_degrees(&rad)
}

pub fn builtin_arccsc_degrees(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("ArcCscDegrees requires exactly 1 argument".to_string()));
    }
    let rad = builtin_arccsc(args)?;
    radians_to_degrees(&rad)
}

pub fn builtin_arcsec_degrees(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("ArcSecDegrees requires exactly 1 argument".to_string()));
    }
    let rad = builtin_arcsec(args)?;
    radians_to_degrees(&rad)
}

pub fn builtin_arccot_degrees(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("ArcCotDegrees requires exactly 1 argument".to_string()));
    }
    let rad = builtin_arccot(args)?;
    radians_to_degrees(&rad)
}

/// Convert degrees (integer or real) to radians value.
fn degrees_to_radians(val: &Value) -> Result<Value, EvalError> {
    let pi_over_180 = Float::with_val(DEFAULT_PRECISION, rug::float::Constant::Pi) / 180u32;
    match val {
        Value::Integer(n) => {
            let f = Float::with_val(DEFAULT_PRECISION, n);
            Ok(Value::Real(f * pi_over_180))
        }
        Value::Real(r) => {
            let prec = r.prec();
            let pi = Float::with_val(prec, rug::float::Constant::Pi);
            Ok(Value::Real(r.clone() * pi / 180u32))
        }
        _ => Err(EvalError::TypeError {
            expected: "Number".to_string(),
            got: val.type_name().to_string(),
        }),
    }
}

/// Convert a radian value (Real) to degrees.
fn radians_to_degrees(val: &Value) -> Result<Value, EvalError> {
    let factor = Float::with_val(DEFAULT_PRECISION, 180u32)
        / Float::with_val(DEFAULT_PRECISION, rug::float::Constant::Pi);
    match val {
        Value::Real(r) => {
            let prec = r.prec();
            let f180 = Float::with_val(prec, 180u32);
            let pi = Float::with_val(prec, rug::float::Constant::Pi);
            Ok(Value::Real(r.clone() * f180 / pi))
        }
        Value::Integer(n) => {
            // Rare: integer radians → degrees
            let f = Float::with_val(DEFAULT_PRECISION, n);
            Ok(Value::Real(f * factor))
        }
        _ => Ok(val.clone()),
    }
}

// ── Logarithmic / Exponential ──

pub fn builtin_log(args: &[Value]) -> Result<Value, EvalError> {
    match args.len() {
        // Log[x] — natural logarithm
        1 => log_natural(&args[0]),
        // Log[b, x] — base-b logarithm = Ln[x] / Ln[b]
        2 => {
            let b = &args[0];
            let x = &args[1];
            // Log[b, 1] = 0
            if let Value::Integer(n) = x {
                if *n == 1 {
                    return Ok(Value::Integer(Integer::from(0)));
                }
            }
            // Log[b, b] = 1
            if b == x {
                return Ok(Value::Integer(Integer::from(1)));
            }
            // Log[b, b^n] = n  (symbolic: x is Power[b, exp])
            if let Value::Call { head, args: pargs } = x {
                if head == "Power" && pargs.len() == 2 && &pargs[0] == b {
                    return Ok(pargs[1].clone());
                }
            }
            // Log[b, x] where b and x are positive integers: try exact integer result
            if let (Value::Integer(bi), Value::Integer(xi)) = (b, x) {
                if !bi.is_negative() && !bi.is_zero() && !xi.is_negative() && !xi.is_zero() {
                    if let Some(n) = exact_integer_log(xi, bi) {
                        return Ok(Value::Integer(Integer::from(n)));
                    }
                }
            }
            // Numerical evaluation when at least one arg is a float
            match (b, x) {
                (Value::Real(_) | Value::Integer(_), Value::Real(_))
                | (Value::Real(_), Value::Integer(_)) => {
                    let ln_x = log_natural(x)?;
                    let ln_b = log_natural(b)?;
                    match (ln_x, ln_b) {
                        (Value::Real(lx), Value::Real(lb)) => {
                            if lb.is_zero() {
                                return Err(EvalError::Error(
                                    "Log base cannot be 1 or 0".to_string(),
                                ));
                            }
                            let prec = lx.prec().max(lb.prec());
                            Ok(Value::Real(
                                Float::with_val(prec, &lx) / Float::with_val(prec, &lb),
                            ))
                        }
                        _ => Ok(Value::Call {
                            head: "Log".to_string(),
                            args: args.to_vec(),
                        }),
                    }
                }
                _ => Ok(Value::Call {
                    head: "Log".to_string(),
                    args: args.to_vec(),
                }),
            }
        }
        _ => Err(EvalError::Error(
            "Log requires 1 or 2 arguments".to_string(),
        )),
    }
}

/// If x == b^n for a positive integer n, return Some(n). Otherwise None.
fn exact_integer_log(x: &Integer, b: &Integer) -> Option<u32> {
    if *b <= 1 {
        return None;
    }
    let mut remaining = x.clone();
    let mut n: u32 = 0;
    loop {
        if remaining == 1 {
            return Some(n);
        }
        let (q, r) = remaining.clone().div_rem(b.clone());
        if r != 0 {
            return None;
        }
        remaining = q;
        n += 1;
    }
}

fn log_natural(v: &Value) -> Result<Value, EvalError> {
    match v {
        // Exact special values
        Value::Integer(n) if *n == 1 => Ok(Value::Integer(Integer::from(0))),
        // Integer arguments stay symbolic (like Mathematica)
        Value::Integer(n) => {
            if n.is_negative() {
                Err(EvalError::Error("Log of non-positive number".to_string()))
            } else {
                Ok(Value::Call {
                    head: "Log".to_string(),
                    args: vec![v.clone()],
                })
            }
        }
        // Float arguments evaluate numerically
        Value::Real(r) => {
            if r.is_zero() || r.is_sign_negative() {
                Err(EvalError::Error("Log of non-positive number".to_string()))
            } else {
                let prec = r.prec();
                Ok(Value::Real(Float::with_val(prec, r).ln()))
            }
        }
        _ => Ok(Value::Call {
            head: "Log".to_string(),
            args: vec![v.clone()],
        }),
    }
}

pub fn builtin_log2(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Log2 requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) if !n.is_zero() && !n.is_negative() => {
            let f = Float::with_val(DEFAULT_PRECISION, n);
            Ok(Value::Real(f.log2()))
        }
        Value::Real(r) if !r.is_zero() && !r.is_sign_negative() => {
            Ok(Value::Real(r.clone().log2()))
        }
        _ => Err(EvalError::Error("Log2 of non-positive number".to_string())),
    }
}

pub fn builtin_log10(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Log10 requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) if !n.is_zero() && !n.is_negative() => {
            let f = Float::with_val(DEFAULT_PRECISION, n);
            Ok(Value::Real(f.log10()))
        }
        Value::Real(r) if !r.is_zero() && !r.is_sign_negative() => {
            Ok(Value::Real(r.clone().log10()))
        }
        _ => Err(EvalError::Error("Log10 of non-positive number".to_string())),
    }
}

pub fn builtin_exp(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Exp requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) if n.is_zero() => Ok(Value::Integer(Integer::from(1))),
        Value::Integer(_) => Ok(Value::Call {
            head: "Exp".to_string(),
            args: args.to_vec(),
        }),
        Value::Real(r) => Ok(Value::Real(r.clone().exp())),
        _ => Ok(Value::Call {
            head: "Exp".to_string(),
            args: args.to_vec(),
        }),
    }
}

// ── Root / Rounding ──

pub fn builtin_sqrt(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Sqrt requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) => {
            if n.is_negative() {
                Err(EvalError::Error("Sqrt of negative number".to_string()))
            } else {
                let f = Float::with_val(DEFAULT_PRECISION, n);
                let r = f.sqrt();
                // Check if result is an exact integer
                if r.is_integer() {
                    let i = r.to_f64() as i64;
                    return Ok(Value::Integer(Integer::from(i)));
                }
                // Keep symbolic: Sqrt[n]
                Ok(Value::Call {
                    head: "Sqrt".to_string(),
                    args: vec![args[0].clone()],
                })
            }
        }
        Value::Real(r) => {
            if r.is_sign_negative() {
                Err(EvalError::Error("Sqrt of negative number".to_string()))
            } else {
                Ok(Value::Real(r.clone().sqrt()))
            }
        }
        _ => Ok(Value::Call {
            head: "Sqrt".to_string(),
            args: args.to_vec(),
        }),
    }
}

pub fn builtin_floor(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Floor requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) => Ok(Value::Integer(n.clone())),
        Value::Real(r) => {
            let floored = r.clone().floor();
            let int_val = floored.to_integer().unwrap_or(Integer::from(0));
            Ok(Value::Integer(int_val))
        }
        _ => Err(EvalError::TypeError {
            expected: "Number".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

pub fn builtin_ceiling(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Ceiling requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) => Ok(Value::Integer(n.clone())),
        Value::Real(r) => {
            let ceiled = r.clone().ceil();
            let int_val = ceiled.to_integer().unwrap_or(Integer::from(0));
            Ok(Value::Integer(int_val))
        }
        _ => Err(EvalError::TypeError {
            expected: "Number".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

pub fn builtin_round(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Round requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) => Ok(Value::Integer(n.clone())),
        Value::Real(r) => {
            let rounded = r.clone().round();
            let int_val = rounded.to_integer().unwrap_or(Integer::from(0));
            Ok(Value::Integer(int_val))
        }
        _ => Err(EvalError::TypeError {
            expected: "Number".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

// ── Min / Max ──

pub fn builtin_max(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Err(EvalError::Error(
            "Max requires at least 1 argument".to_string(),
        ));
    }
    let mut max = &args[0];
    for arg in &args[1..] {
        match (max, arg) {
            (Value::Integer(a), Value::Integer(b)) => {
                if b > a {
                    max = arg;
                }
            }
            (Value::Real(a), Value::Real(b)) => {
                if b > a {
                    max = arg;
                }
            }
            _ => {}
        }
    }
    Ok(max.clone())
}

pub fn builtin_min(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Err(EvalError::Error(
            "Min requires at least 1 argument".to_string(),
        ));
    }
    let mut min = &args[0];
    for arg in &args[1..] {
        match (min, arg) {
            (Value::Integer(a), Value::Integer(b)) => {
                if b < a {
                    min = arg;
                }
            }
            (Value::Real(a), Value::Real(b)) => {
                if b < a {
                    min = arg;
                }
            }
            _ => {}
        }
    }
    Ok(min.clone())
}

// ── Modular arithmetic / Number theory ──

pub fn builtin_mod(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Mod requires exactly 2 arguments".to_string(),
        ));
    }
    match (&args[0], &args[1]) {
        (Value::Integer(a), Value::Integer(b)) if !b.is_zero() => {
            let result = a.clone() % b;
            let result = if result < 0 {
                result + b.clone().abs()
            } else {
                result
            };
            Ok(Value::Integer(result))
        }
        (Value::Real(a), Value::Real(b)) if !b.is_zero() => {
            let div = a.clone() / b;
            let floored = div.floor();
            let result = a - b * floored;
            Ok(Value::Real(result))
        }
        (Value::Integer(a), Value::Real(b)) if !b.is_zero() => {
            let a_f = Float::with_val(DEFAULT_PRECISION, a);
            let div = a_f.clone() / b;
            let floored = div.floor();
            let result = a_f - b * floored;
            Ok(Value::Real(result))
        }
        (Value::Real(a), Value::Integer(b)) if !b.is_zero() => {
            let b_f = Float::with_val(DEFAULT_PRECISION, b);
            let div = a.clone() / &b_f;
            let floored = div.floor();
            let result = a - &b_f * floored;
            Ok(Value::Real(result))
        }
        _ => Err(EvalError::Error(
            "Mod: division by zero or invalid types".to_string(),
        )),
    }
}

pub fn gcd(mut a: Integer, mut b: Integer) -> Integer {
    while !b.is_zero() {
        let t = b;
        b = a % t.clone();
        a = t;
    }
    a.abs()
}

pub fn builtin_gcd(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "GCD requires exactly 2 arguments".to_string(),
        ));
    }
    match (&args[0], &args[1]) {
        (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(gcd(a.clone(), b.clone()))),
        _ => Err(EvalError::TypeError {
            expected: "Integer".to_string(),
            got: format!("{} and {}", args[0].type_name(), args[1].type_name()),
        }),
    }
}

pub fn builtin_lcm(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "LCM requires exactly 2 arguments".to_string(),
        ));
    }
    match (&args[0], &args[1]) {
        (Value::Integer(a), Value::Integer(b)) => {
            if a.is_zero() || b.is_zero() {
                return Ok(Value::Integer(Integer::from(0)));
            }
            let product = a.clone() * b;
            let abs_product = product.abs();
            let gcd_val = gcd(a.clone(), b.clone());
            Ok(Value::Integer(abs_product / gcd_val))
        }
        _ => Err(EvalError::TypeError {
            expected: "Integer".to_string(),
            got: format!("{} and {}", args[0].type_name(), args[1].type_name()),
        }),
    }
}

pub fn factorial(n: i64) -> Integer {
    if n <= 1 {
        Integer::from(1)
    } else {
        Integer::from(n) * factorial(n - 1)
    }
}

pub fn builtin_factorial(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Factorial requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) if *n >= 0 => {
            let n_i64 = n.to_i64().unwrap_or(0);
            Ok(Value::Integer(factorial(n_i64)))
        }
        _ => Err(EvalError::Error(
            "Factorial requires a non-negative integer".to_string(),
        )),
    }
}

// ── FixedPoint stub (evaluator handles this) ──

pub fn builtin_fixed_point_stub(_args: &[Value]) -> Result<Value, EvalError> {
    Err(EvalError::Error(
        "FixedPoint should be handled by evaluator".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn int(n: i64) -> Value {
        Value::Integer(Integer::from(n))
    }
    fn real(r: f64) -> Value {
        Value::Real(Float::with_val(DEFAULT_PRECISION, r))
    }
    fn pi() -> Value {
        Value::Real(Float::with_val(DEFAULT_PRECISION, rug::float::Constant::Pi))
    }

    #[test]
    fn test_sin_zero_real() {
        // sin(0.0) now returns Integer(0) via pi-multiple detection
        assert_eq!(builtin_sin(&[real(0.0)]).unwrap(), int(0));
    }

    #[test]
    fn test_sin_pi() {
        assert_eq!(builtin_sin(&[pi()]).unwrap(), int(0));
    }

    #[test]
    fn test_sin_pi_over_2() {
        // sin(π/2) = 1
        let half_pi = Value::Real(
            Float::with_val(DEFAULT_PRECISION, rug::float::Constant::Pi) / 2u32,
        );
        assert_eq!(builtin_sin(&[half_pi]).unwrap(), int(1));
    }

    #[test]
    fn test_sin_pi_over_6() {
        // sin(π/6) = 1/2 → Divide[1, 2]
        let arg = Value::Real(
            Float::with_val(DEFAULT_PRECISION, rug::float::Constant::Pi) / 6u32,
        );
        let result = builtin_sin(&[arg]).unwrap();
        assert_eq!(result, val_div(val_int(1), val_int(2)));
    }

    #[test]
    fn test_sin_pi_over_4() {
        // sin(π/4) = √2/2 → Divide[Sqrt[2], 2]
        let arg = Value::Real(
            Float::with_val(DEFAULT_PRECISION, rug::float::Constant::Pi) / 4u32,
        );
        let result = builtin_sin(&[arg]).unwrap();
        assert_eq!(result, sqrt_over(2, 2));
    }

    #[test]
    fn test_sin_pi_over_3() {
        // sin(π/3) = √3/2 → Divide[Sqrt[3], 2]
        let arg = Value::Real(
            Float::with_val(DEFAULT_PRECISION, rug::float::Constant::Pi) / 3u32,
        );
        let result = builtin_sin(&[arg]).unwrap();
        assert_eq!(result, sqrt_over(3, 2));
    }

    #[test]
    fn test_sin_negative_pi() {
        let pi_val = Float::with_val(DEFAULT_PRECISION, rug::float::Constant::Pi);
        let neg_pi = Value::Real(Float::with_val(DEFAULT_PRECISION, -pi_val));
        assert_eq!(builtin_sin(&[neg_pi]).unwrap(), int(0));
    }

    #[test]
    fn test_sin_negative_pi_over_2() {
        // sin(-π/2) = -1
        let neg_half_pi = Value::Real(
            -Float::with_val(DEFAULT_PRECISION, rug::float::Constant::Pi) / 2u32,
        );
        assert_eq!(builtin_sin(&[neg_half_pi]).unwrap(), int(-1));
    }

    #[test]
    fn test_cos_zero_real() {
        assert_eq!(builtin_cos(&[real(0.0)]).unwrap(), int(1));
    }

    #[test]
    fn test_cos_pi() {
        assert_eq!(builtin_cos(&[pi()]).unwrap(), int(-1));
    }

    #[test]
    fn test_cos_pi_over_2() {
        let half_pi = Value::Real(
            Float::with_val(DEFAULT_PRECISION, rug::float::Constant::Pi) / 2u32,
        );
        assert_eq!(builtin_cos(&[half_pi]).unwrap(), int(0));
    }

    #[test]
    fn test_cos_pi_over_3() {
        // cos(π/3) = 1/2 → Divide[1, 2]
        let arg = Value::Real(
            Float::with_val(DEFAULT_PRECISION, rug::float::Constant::Pi) / 3u32,
        );
        let result = builtin_cos(&[arg]).unwrap();
        assert_eq!(result, one_over(2));
    }

    #[test]
    fn test_cos_pi_over_4() {
        // cos(π/4) = √2/2 → Divide[Sqrt[2], 2]
        let arg = Value::Real(
            Float::with_val(DEFAULT_PRECISION, rug::float::Constant::Pi) / 4u32,
        );
        let result = builtin_cos(&[arg]).unwrap();
        assert_eq!(result, sqrt_over(2, 2));
    }

    #[test]
    fn test_tan_pi() {
        assert_eq!(builtin_tan(&[pi()]).unwrap(), int(0));
    }

    #[test]
    fn test_tan_pi_over_4() {
        let arg = Value::Real(
            Float::with_val(DEFAULT_PRECISION, rug::float::Constant::Pi) / 4u32,
        );
        assert_eq!(builtin_tan(&[arg]).unwrap(), int(1));
    }

    #[test]
    fn test_tan_negative_pi_over_4() {
        // tan(-π/4) = -1
        let arg = Value::Real(
            -Float::with_val(DEFAULT_PRECISION, rug::float::Constant::Pi) / 4u32,
        );
        assert_eq!(builtin_tan(&[arg]).unwrap(), int(-1));
    }

    #[test]
    fn test_tan_pi_over_3() {
        // tan(π/3) = √3 → Sqrt[3]
        let arg = Value::Real(
            Float::with_val(DEFAULT_PRECISION, rug::float::Constant::Pi) / 3u32,
        );
        let result = builtin_tan(&[arg]).unwrap();
        assert_eq!(result, val_sqrt(3));
    }

    #[test]
    fn test_sin_non_special_angle() {
        // sin(1.0) should still return a numerical Real
        let result = builtin_sin(&[real(1.0)]).unwrap();
        if let Value::Real(r) = result {
            assert!((r.to_f64() - 1.0_f64.sin()).abs() < 1e-10);
        } else {
            panic!("Expected Real for sin(1.0)");
        }
    }

    #[test]
    fn test_sin_pi_over_5_symbolic() {
        // sin(π/5) is not a special angle — should stay symbolic
        let arg = Value::Real(
            Float::with_val(DEFAULT_PRECISION, rug::float::Constant::Pi) / 5u32,
        );
        let result = builtin_sin(&[arg]).unwrap();
        match result {
            Value::Call { head, .. } => assert_eq!(head, "Sin"),
            _ => panic!("Expected symbolic Sin[...] for sin(π/5), got {:?}", result),
        }
    }

    #[test]
    fn test_log() {
        let result = builtin_log(&[real(1.0)]).unwrap();
        if let Value::Real(r) = result {
            assert!(r.to_f64().abs() < f64::EPSILON);
        } else {
            panic!("Expected Real");
        }
    }

    #[test]
    fn test_log_negative() {
        assert!(builtin_log(&[int(-1)]).is_err());
    }

    #[test]
    fn test_exp() {
        let result = builtin_exp(&[real(0.0)]).unwrap();
        if let Value::Real(r) = result {
            assert!((r.to_f64() - 1.0).abs() < f64::EPSILON);
        } else {
            panic!("Expected Real");
        }
    }

    #[test]
    fn test_sqrt() {
        assert_eq!(builtin_sqrt(&[int(4)]).unwrap(), int(2));
        assert_eq!(builtin_sqrt(&[int(9)]).unwrap(), int(3));
    }

    #[test]
    fn test_sqrt_real() {
        let result = builtin_sqrt(&[real(2.0)]).unwrap();
        if let Value::Real(r) = result {
            assert!((r.to_f64() - std::f64::consts::SQRT_2).abs() < f64::EPSILON);
        } else {
            panic!("Expected Real");
        }
    }

    #[test]
    fn test_sqrt_negative() {
        assert!(builtin_sqrt(&[int(-1)]).is_err());
    }

    #[test]
    fn test_floor() {
        assert_eq!(builtin_floor(&[real(3.7)]).unwrap(), int(3));
        assert_eq!(builtin_floor(&[real(-2.3)]).unwrap(), int(-3));
    }

    #[test]
    fn test_ceiling() {
        assert_eq!(builtin_ceiling(&[real(3.2)]).unwrap(), int(4));
        assert_eq!(builtin_ceiling(&[real(-2.7)]).unwrap(), int(-2));
    }

    #[test]
    fn test_round() {
        assert_eq!(builtin_round(&[real(3.5)]).unwrap(), int(4));
        assert_eq!(builtin_round(&[real(3.4)]).unwrap(), int(3));
    }

    #[test]
    fn test_max() {
        assert_eq!(builtin_max(&[int(1), int(3), int(2)]).unwrap(), int(3));
    }

    #[test]
    fn test_min() {
        assert_eq!(builtin_min(&[int(3), int(1), int(2)]).unwrap(), int(1));
    }

    // ── Reciprocal trig ──

    #[test]
    fn test_csc_pi_over_6() {
        let arg = Value::Real(
            Float::with_val(DEFAULT_PRECISION, rug::float::Constant::Pi) / 6u32,
        );
        assert_eq!(builtin_csc(&[arg]).unwrap(), int(2));
    }

    #[test]
    fn test_csc_pi_over_2() {
        let arg = Value::Real(
            Float::with_val(DEFAULT_PRECISION, rug::float::Constant::Pi) / 2u32,
        );
        assert_eq!(builtin_csc(&[arg]).unwrap(), int(1));
    }

    #[test]
    fn test_csc_pi_over_4() {
        let arg = Value::Real(
            Float::with_val(DEFAULT_PRECISION, rug::float::Constant::Pi) / 4u32,
        );
        assert_eq!(builtin_csc(&[arg]).unwrap(), val_sqrt(2));
    }

    #[test]
    fn test_csc_zero_symbolic() {
        // Csc[0] stays symbolic (ComplexInfinity in Wolfram)
        let result = builtin_csc(&[real(0.0)]).unwrap();
        match result {
            Value::Call { head, .. } => assert_eq!(head, "Csc"),
            _ => panic!("Expected symbolic Csc[...], got {:?}", result),
        }
    }

    #[test]
    fn test_sec_pi_over_3() {
        let arg = Value::Real(
            Float::with_val(DEFAULT_PRECISION, rug::float::Constant::Pi) / 3u32,
        );
        assert_eq!(builtin_sec(&[arg]).unwrap(), int(2));
    }

    #[test]
    fn test_sec_zero() {
        assert_eq!(builtin_sec(&[real(0.0)]).unwrap(), int(1));
    }

    #[test]
    fn test_sec_pi() {
        assert_eq!(builtin_sec(&[pi()]).unwrap(), int(-1));
    }

    #[test]
    fn test_cot_pi_over_4() {
        let arg = Value::Real(
            Float::with_val(DEFAULT_PRECISION, rug::float::Constant::Pi) / 4u32,
        );
        assert_eq!(builtin_cot(&[arg]).unwrap(), int(1));
    }

    #[test]
    fn test_cot_pi_over_6() {
        let arg = Value::Real(
            Float::with_val(DEFAULT_PRECISION, rug::float::Constant::Pi) / 6u32,
        );
        assert_eq!(builtin_cot(&[arg]).unwrap(), val_sqrt(3));
    }

    #[test]
    fn test_cot_pi_over_2() {
        let arg = Value::Real(
            Float::with_val(DEFAULT_PRECISION, rug::float::Constant::Pi) / 2u32,
        );
        assert_eq!(builtin_cot(&[arg]).unwrap(), int(0));
    }

    #[test]
    fn test_csc_negative_pi_over_6() {
        let arg = Value::Real(
            -Float::with_val(DEFAULT_PRECISION, rug::float::Constant::Pi) / 6u32,
        );
        assert_eq!(builtin_csc(&[arg]).unwrap(), int(-2));
    }

    #[test]
    fn test_sec_negative_pi_over_3() {
        let arg = Value::Real(
            -Float::with_val(DEFAULT_PRECISION, rug::float::Constant::Pi) / 3u32,
        );
        assert_eq!(builtin_sec(&[arg]).unwrap(), int(2));
    }

    // ── Inverse reciprocal trig ──

    #[test]
    fn test_arccsc_integer() {
        let result = builtin_arccsc(&[int(2)]).unwrap();
        if let Value::Real(r) = result {
            // ArcCsc[2] = ArcSin[1/2] = π/6 ≈ 0.5236
            assert!((r.to_f64() - std::f64::consts::FRAC_PI_6).abs() < 1e-10);
        } else {
            panic!("Expected Real");
        }
    }

    #[test]
    fn test_arcsec_integer() {
        let result = builtin_arcsec(&[int(2)]).unwrap();
        if let Value::Real(r) = result {
            // ArcSec[2] = ArcCos[1/2] = π/3 ≈ 1.0472
            assert!((r.to_f64() - std::f64::consts::FRAC_PI_3).abs() < 1e-10);
        } else {
            panic!("Expected Real");
        }
    }

    #[test]
    fn test_arccot_zero() {
        let result = builtin_arccot(&[int(0)]).unwrap();
        if let Value::Real(r) = result {
            // ArcCot[0] = π/2
            assert!((r.to_f64() - std::f64::consts::FRAC_PI_2).abs() < 1e-10);
        } else {
            panic!("Expected Real");
        }
    }

    #[test]
    fn test_arccot_one() {
        let result = builtin_arccot(&[int(1)]).unwrap();
        if let Value::Real(r) = result {
            // ArcCot[1] = ArcTan[1] = π/4
            assert!((r.to_f64() - std::f64::consts::FRAC_PI_4).abs() < 1e-10);
        } else {
            panic!("Expected Real");
        }
    }

    // ── Haversine ──

    #[test]
    fn test_haversine_zero() {
        assert_eq!(builtin_haversine(&[int(0)]).unwrap(), int(0));
    }

    #[test]
    fn test_haversine_pi() {
        // Haversine[pi] = (1 - Cos[pi])/2 = (1-(-1))/2 = 1.0 (Real)
        let result = builtin_haversine(&[pi()]).unwrap();
        if let Value::Real(r) = result {
            assert!((r.to_f64() - 1.0).abs() < 1e-15);
        } else {
            panic!("Expected Real, got {:?}", result);
        }
    }

    #[test]
    fn test_haversine_pi_over_2() {
        let half_pi = Value::Real(
            Float::with_val(DEFAULT_PRECISION, rug::float::Constant::Pi) / 2u32,
        );
        // Haversine[pi/2] = (1 - Cos[pi/2])/2 = 0.5 (Real)
        let result = builtin_haversine(&[half_pi]).unwrap();
        if let Value::Real(r) = result {
            assert!((r.to_f64() - 0.5).abs() < 1e-15);
        } else {
            panic!("Expected Real, got {:?}", result);
        }
    }

    #[test]
    fn test_inverse_haversine_zero() {
        assert_eq!(builtin_inverse_haversine(&[int(0)]).unwrap(), int(0));
    }

    #[test]
    fn test_inverse_haversine_one() {
        // InverseHaversine[1] = 2 * ArcSin[1] = 2 * π/2 = π
        let result = builtin_inverse_haversine(&[int(1)]).unwrap();
        if let Value::Real(r) = result {
            let pi_val = std::f64::consts::PI;
            assert!((r.to_f64() - pi_val).abs() < 1e-10);
        } else {
            panic!("Expected Real");
        }
    }

    #[test]
    fn test_inverse_haversine_negative_err() {
        assert!(builtin_inverse_haversine(&[int(-1)]).is_err());
    }

    // ── Degree-based trig ──

    #[test]
    fn test_sin_degrees_30() {
        // SinDegrees[30] = Sin[π/6] = 1/2
        assert_eq!(builtin_sin_degrees(&[int(30)]).unwrap(), one_over(2));
    }

    #[test]
    fn test_sin_degrees_90() {
        // SinDegrees[90] = Sin[π/2] = 1
        assert_eq!(builtin_sin_degrees(&[int(90)]).unwrap(), int(1));
    }

    #[test]
    fn test_cos_degrees_60() {
        // CosDegrees[60] = Cos[π/3] = 1/2
        assert_eq!(builtin_cos_degrees(&[int(60)]).unwrap(), one_over(2));
    }

    #[test]
    fn test_cos_degrees_0() {
        assert_eq!(builtin_cos_degrees(&[int(0)]).unwrap(), int(1));
    }

    #[test]
    fn test_tan_degrees_45() {
        // TanDegrees[45] = Tan[π/4] = 1
        assert_eq!(builtin_tan_degrees(&[int(45)]).unwrap(), int(1));
    }

    #[test]
    fn test_tan_degrees_0() {
        assert_eq!(builtin_tan_degrees(&[int(0)]).unwrap(), int(0));
    }

    #[test]
    fn test_csc_degrees_30() {
        // CscDegrees[30] = Csc[π/6] = 2
        assert_eq!(builtin_csc_degrees(&[int(30)]).unwrap(), int(2));
    }

    #[test]
    fn test_sec_degrees_60() {
        // SecDegrees[60] = Sec[π/3] = 2
        assert_eq!(builtin_sec_degrees(&[int(60)]).unwrap(), int(2));
    }

    #[test]
    fn test_cot_degrees_45() {
        // CotDegrees[45] = Cot[π/4] = 1
        assert_eq!(builtin_cot_degrees(&[int(45)]).unwrap(), int(1));
    }

    #[test]
    fn test_sin_degrees_real() {
        // SinDegrees[30.0] → Sin[pi/6] → exact Divide[1, 2] (via pi-multiple detection)
        let result = builtin_sin_degrees(&[real(30.0)]).unwrap();
        assert_eq!(result, one_over(2));
    }

    // ── Inverse trig (degrees) ──

    #[test]
    fn test_arcsin_degrees() {
        // ArcSinDegrees[0.5] should be 30.0
        let result = builtin_arcsin_degrees(&[real(0.5)]).unwrap();
        if let Value::Real(r) = result {
            assert!((r.to_f64() - 30.0).abs() < 1e-10);
        } else {
            panic!("Expected Real");
        }
    }

    #[test]
    fn test_arccos_degrees() {
        // ArcCosDegrees[0.5] should be 60.0
        let result = builtin_arccos_degrees(&[real(0.5)]).unwrap();
        if let Value::Real(r) = result {
            assert!((r.to_f64() - 60.0).abs() < 1e-10);
        } else {
            panic!("Expected Real");
        }
    }

    #[test]
    fn test_arctan_degrees() {
        // ArcTanDegrees[1.0] should be 45.0
        let result = builtin_arctan_degrees(&[real(1.0)]).unwrap();
        if let Value::Real(r) = result {
            assert!((r.to_f64() - 45.0).abs() < 1e-10);
        } else {
            panic!("Expected Real");
        }
    }

    #[test]
    fn test_arccsc_degrees() {
        // ArcCscDegrees[2] = ArcSinDegrees[1/2] = 30.0
        let result = builtin_arccsc_degrees(&[int(2)]).unwrap();
        if let Value::Real(r) = result {
            assert!((r.to_f64() - 30.0).abs() < 1e-10);
        } else {
            panic!("Expected Real");
        }
    }

    #[test]
    fn test_arcsec_degrees() {
        // ArcSecDegrees[2] = ArcCosDegrees[1/2] = 60.0
        let result = builtin_arcsec_degrees(&[int(2)]).unwrap();
        if let Value::Real(r) = result {
            assert!((r.to_f64() - 60.0).abs() < 1e-10);
        } else {
            panic!("Expected Real");
        }
    }

    #[test]
    fn test_arccot_degrees() {
        // ArcCotDegrees[1] = ArcTanDegrees[1] = 45.0
        let result = builtin_arccot_degrees(&[int(1)]).unwrap();
        if let Value::Real(r) = result {
            assert!((r.to_f64() - 45.0).abs() < 1e-10);
        } else {
            panic!("Expected Real");
        }
    }
}
