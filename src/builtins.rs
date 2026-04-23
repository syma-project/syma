/// Built-in functions for Syma language.
///
/// Provides the core symbolic computation library:
/// - Arithmetic: Plus, Times, Power, Divide
/// - List: Length, First, Last, Rest, Append, etc.
/// - Pattern: MatchQ, Head, TypeOf
/// - String: StringJoin, StringLength, ToString
/// - Math: Sin, Cos, Log, Exp, Sqrt, Abs
/// - Control: If, Which, Map, Fold, Select

use crate::value::{Value, EvalError, DEFAULT_PRECISION};
use crate::env::Env;
use rug::Integer;
use rug::Float;
use rug::ops::Pow;

/// Register all built-in functions in the environment.
pub fn register_builtins(env: &Env) {
    // ── Arithmetic ──
    register_builtin(env, "Plus", builtin_plus);
    register_builtin(env, "Times", builtin_times);
    register_builtin(env, "Power", builtin_power);
    register_builtin(env, "Divide", builtin_divide);
    register_builtin(env, "Minus", builtin_minus);
    register_builtin(env, "Abs", builtin_abs);

    // ── Comparison ──
    register_builtin(env, "Equal", builtin_equal);
    register_builtin(env, "Unequal", builtin_unequal);
    register_builtin(env, "Less", builtin_less);
    register_builtin(env, "Greater", builtin_greater);
    register_builtin(env, "LessEqual", builtin_less_equal);
    register_builtin(env, "GreaterEqual", builtin_greater_equal);

    // ── Logical ──
    register_builtin(env, "And", builtin_and);
    register_builtin(env, "Or", builtin_or);
    register_builtin(env, "Not", builtin_not);

    // ── List ──
    register_builtin(env, "Length", builtin_length);
    register_builtin(env, "First", builtin_first);
    register_builtin(env, "Last", builtin_last);
    register_builtin(env, "Rest", builtin_rest);
    register_builtin(env, "Most", builtin_most);
    register_builtin(env, "Append", builtin_append);
    register_builtin(env, "Prepend", builtin_prepend);
    register_builtin(env, "Join", builtin_join);
    register_builtin(env, "Flatten", builtin_flatten);
    register_builtin(env, "Sort", builtin_sort);
    register_builtin(env, "Reverse", builtin_reverse);
    register_builtin(env, "Part", builtin_part);
    register_builtin(env, "Range", builtin_range);
    register_builtin(env, "Table", builtin_table);
    register_builtin(env, "Map", builtin_map);
    register_builtin(env, "Fold", builtin_fold);
    register_builtin(env, "Select", builtin_select);
    register_builtin(env, "Scan", builtin_scan);
    register_builtin(env, "Nest", builtin_nest);
    register_builtin(env, "Take", builtin_take);
    register_builtin(env, "Drop", builtin_drop);
    register_builtin(env, "Riffle", builtin_riffle);
    register_builtin(env, "Transpose", builtin_transpose);
    register_builtin(env, "Total", builtin_total);
    register_builtin(env, "Sum", builtin_sum);

    // ── Pattern ──
    register_builtin(env, "MatchQ", builtin_match_q);
    register_builtin(env, "Head", builtin_head);
    register_builtin(env, "TypeOf", builtin_type_of);
    register_builtin(env, "FreeQ", builtin_free_q);

    // ── String ──
    register_builtin(env, "StringJoin", builtin_string_join);
    register_builtin(env, "StringLength", builtin_string_length);
    register_builtin(env, "ToString", builtin_to_string);
    register_builtin(env, "ToExpression", builtin_to_expression);

    // ── Math ──
    register_builtin(env, "Sin", builtin_sin);
    register_builtin(env, "Cos", builtin_cos);
    register_builtin(env, "Tan", builtin_tan);
    register_builtin(env, "Log", builtin_log);
    register_builtin(env, "Exp", builtin_exp);
    register_builtin(env, "Sqrt", builtin_sqrt);
    register_builtin(env, "Floor", builtin_floor);
    register_builtin(env, "Ceiling", builtin_ceiling);
    register_builtin(env, "Round", builtin_round);
    register_builtin(env, "Max", builtin_max);
    register_builtin(env, "Min", builtin_min);

    // ── I/O ──
    register_builtin(env, "Print", builtin_print);

    // ── Association ──
    register_builtin(env, "Keys", builtin_keys);
    register_builtin(env, "Values", builtin_values);

    // ── Symbolic ──
    register_builtin(env, "Simplify", builtin_simplify);
    register_builtin(env, "Expand", builtin_expand);
    register_builtin(env, "D", builtin_d);
    register_builtin(env, "Integrate", builtin_integrate);
    register_builtin(env, "Factor", builtin_factor);
    register_builtin(env, "Solve", builtin_solve);
    register_builtin(env, "Series", builtin_series);

    // ── Control (evaluator-dependent) ──
    register_builtin(env, "FixedPoint", builtin_fixed_point_stub);

    // ── Extended math ──
    register_builtin(env, "ArcSin", builtin_arcsin);
    register_builtin(env, "ArcCos", builtin_arccos);
    register_builtin(env, "ArcTan", builtin_arctan);
    register_builtin(env, "Log2", builtin_log2);
    register_builtin(env, "Log10", builtin_log10);
    register_builtin(env, "Mod", builtin_mod);
    register_builtin(env, "GCD", builtin_gcd);
    register_builtin(env, "LCM", builtin_lcm);
    register_builtin(env, "Factorial", builtin_factorial);

    // ── Random ──
    register_builtin(env, "RandomInteger", builtin_random_integer);
    register_builtin(env, "RandomReal", builtin_random_real);
    register_builtin(env, "RandomChoice", builtin_random_choice);

    // ── Extended string ──
    register_builtin(env, "StringSplit", builtin_string_split);
    register_builtin(env, "StringReplace", builtin_string_replace);
    register_builtin(env, "StringTake", builtin_string_take);
    register_builtin(env, "StringDrop", builtin_string_drop);
    register_builtin(env, "StringContainsQ", builtin_string_contains_q);
    register_builtin(env, "StringReverse", builtin_string_reverse);
    register_builtin(env, "ToUpperCase", builtin_to_upper_case);
    register_builtin(env, "ToLowerCase", builtin_to_lower_case);

    // ── Extended list ──
    register_builtin(env, "MemberQ", builtin_member_q);
    register_builtin(env, "Count", builtin_count);
    register_builtin(env, "Position", builtin_position);
    register_builtin(env, "Union", builtin_union);
    register_builtin(env, "Intersection", builtin_intersection);
    register_builtin(env, "Complement", builtin_complement);
    register_builtin(env, "Tally", builtin_tally);
    register_builtin(env, "PadLeft", builtin_pad_left);
    register_builtin(env, "PadRight", builtin_pad_right);

    // ── Association extended ──
    register_builtin(env, "Lookup", builtin_lookup);
    register_builtin(env, "KeyExistsQ", builtin_key_exists_q);

    // ── I/O ──
    register_builtin(env, "Input", builtin_input);

    // ── Constants ──
    env.set("Pi".to_string(), Value::Real(Float::with_val(DEFAULT_PRECISION, rug::float::Constant::Pi)));
    // Euler's number e = exp(1); rug::float::Constant::Euler is the Euler-Mascheroni constant
    let one = Float::with_val(DEFAULT_PRECISION, 1);
    env.set("E".to_string(), Value::Real(one.exp()));
    env.set("I".to_string(), Value::Complex { re: 0.0, im: 1.0 });
}

fn register_builtin(env: &Env, name: &str, func: fn(&[Value]) -> Result<Value, EvalError>) {
    env.set(
        name.to_string(),
        Value::Builtin(name.to_string(), func),
    );
}

// ── Arithmetic ──

fn builtin_plus(args: &[Value]) -> Result<Value, EvalError> {
    let mut result = Value::Integer(Integer::from(0));
    for arg in args {
        result = add_values(&result, arg)?;
    }
    Ok(result)
}

pub fn add_values_public(a: &Value, b: &Value) -> Result<Value, EvalError> {
    add_values(a, b)
}

fn add_values(a: &Value, b: &Value) -> Result<Value, EvalError> {
    // Identity: 0 + x = x
    if matches!(a, Value::Integer(n) if n.is_zero()) {
        return Ok(b.clone());
    }
    if matches!(b, Value::Integer(n) if n.is_zero()) {
        return Ok(a.clone());
    }
    match (a, b) {
        (Value::Integer(x), Value::Integer(y)) => Ok(Value::Integer(x.clone() + y)),
        (Value::Real(x), Value::Real(y)) => Ok(Value::Real(x.clone() + y)),
        (Value::Integer(x), Value::Real(y)) => Ok(Value::Real(Float::with_val(DEFAULT_PRECISION, x) + y)),
        (Value::Real(x), Value::Integer(y)) => Ok(Value::Real(x + Float::with_val(DEFAULT_PRECISION, y))),
        (Value::List(xs), Value::List(ys)) => {
            if xs.len() == ys.len() {
                let result: Result<Vec<Value>, _> = xs.iter().zip(ys.iter())
                    .map(|(x, y)| add_values(x, y))
                    .collect();
                Ok(Value::List(result?))
            } else {
                Err(EvalError::Error("Lists must have same length for addition".to_string()))
            }
        }
        _ => {
            // Return symbolic: Plus[a, b]
            Ok(Value::Call {
                head: "Plus".to_string(),
                args: vec![a.clone(), b.clone()],
            })
        }
    }
}

fn builtin_times(args: &[Value]) -> Result<Value, EvalError> {
    let mut result = Value::Integer(Integer::from(1));
    for arg in args {
        result = mul_values(&result, arg)?;
    }
    Ok(result)
}

fn mul_values(a: &Value, b: &Value) -> Result<Value, EvalError> {
    // Identity: 1 * x = x
    if matches!(a, Value::Integer(n) if *n == 1) {
        return Ok(b.clone());
    }
    if matches!(b, Value::Integer(n) if *n == 1) {
        return Ok(a.clone());
    }
    // Annihilator: 0 * x = 0
    if matches!(a, Value::Integer(n) if n.is_zero()) || matches!(b, Value::Integer(n) if n.is_zero()) {
        return Ok(Value::Integer(Integer::from(0)));
    }
    match (a, b) {
        (Value::Integer(x), Value::Integer(y)) => Ok(Value::Integer(x.clone() * y)),
        (Value::Real(x), Value::Real(y)) => Ok(Value::Real(x.clone() * y)),
        (Value::Integer(x), Value::Real(y)) => Ok(Value::Real(Float::with_val(DEFAULT_PRECISION, x) * y)),
        (Value::Real(x), Value::Integer(y)) => Ok(Value::Real(x * Float::with_val(DEFAULT_PRECISION, y))),
        (Value::List(xs), Value::Integer(s)) | (Value::Integer(s), Value::List(xs)) => {
            let result: Vec<Value> = xs.iter()
                .map(|x| mul_values(x, &Value::Integer(s.clone())))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Value::List(result))
        }
        (Value::List(xs), Value::Real(s)) | (Value::Real(s), Value::List(xs)) => {
            let result: Vec<Value> = xs.iter()
                .map(|x| mul_values(x, &Value::Real(s.clone())))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Value::List(result))
        }
        _ => {
            Ok(Value::Call {
                head: "Times".to_string(),
                args: vec![a.clone(), b.clone()],
            })
        }
    }
}

fn builtin_power(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error("Power requires exactly 2 arguments".to_string()));
    }
    // Identity: x^0 = 1, x^1 = x
    if matches!(&args[1], Value::Integer(n) if n.is_zero()) {
        return Ok(Value::Integer(Integer::from(1)));
    }
    if matches!(&args[1], Value::Integer(n) if *n == 1) {
        return Ok(args[0].clone());
    }
    // Annihilator: 0^x = 0
    if matches!(&args[0], Value::Integer(n) if n.is_zero()) {
        return Ok(Value::Integer(Integer::from(0)));
    }
    match (&args[0], &args[1]) {
        (Value::Integer(base), Value::Integer(exp)) => {
            if let Some(e) = exp.to_u32() {
                Ok(Value::Integer(base.clone().pow(e)))
            } else {
                // Negative exponent: convert to float
                let b = Float::with_val(DEFAULT_PRECISION, base);
                let e = Float::with_val(DEFAULT_PRECISION, exp);
                Ok(Value::Real(b.pow(e)))
            }
        }
        (Value::Real(base), Value::Real(exp)) => Ok(Value::Real(base.clone().pow(exp))),
        (Value::Integer(base), Value::Real(exp)) => {
            let b = Float::with_val(DEFAULT_PRECISION, base);
            Ok(Value::Real(b.pow(exp)))
        }
        (Value::Real(base), Value::Integer(exp)) => {
            let e = Float::with_val(DEFAULT_PRECISION, exp);
            Ok(Value::Real(base.clone().pow(e)))
        }
        _ => {
            Ok(Value::Call {
                head: "Power".to_string(),
                args: args.to_vec(),
            })
        }
    }
}

fn builtin_divide(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error("Divide requires exactly 2 arguments".to_string()));
    }
    // Identity: x/1 = x
    if matches!(&args[1], Value::Integer(n) if *n == 1) {
        return Ok(args[0].clone());
    }
    // Annihilator: 0/x = 0
    if matches!(&args[0], Value::Integer(n) if n.is_zero()) {
        return Ok(Value::Integer(Integer::from(0)));
    }
    match (&args[0], &args[1]) {
        (Value::Integer(_), Value::Integer(b)) if b.is_zero() => Err(EvalError::DivisionByZero),
        (Value::Real(_), Value::Real(b)) if b.is_zero() => Err(EvalError::DivisionByZero),
        (Value::Integer(a), Value::Integer(b)) => {
            if a.is_divisible(b) {
                Ok(Value::Integer(a.clone() / b))
            } else {
                let a_f = Float::with_val(DEFAULT_PRECISION, a);
                let b_f = Float::with_val(DEFAULT_PRECISION, b);
                Ok(Value::Real(a_f / b_f))
            }
        }
        (Value::Real(a), Value::Real(b)) => Ok(Value::Real(a.clone() / b)),
        (Value::Integer(a), Value::Real(b)) => {
            let a_f = Float::with_val(DEFAULT_PRECISION, a);
            Ok(Value::Real(a_f / b))
        }
        (Value::Real(a), Value::Integer(b)) => {
            let b_f = Float::with_val(DEFAULT_PRECISION, b);
            Ok(Value::Real(a / b_f))
        }
        _ => {
            Ok(Value::Call {
                head: "Divide".to_string(),
                args: args.to_vec(),
            })
        }
    }
}

fn builtin_minus(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() == 1 {
        // Negation
        match &args[0] {
            Value::Integer(n) => Ok(Value::Integer(-n.clone())),
            Value::Real(r) => Ok(Value::Real(-r.clone())),
            _ => {
                Ok(Value::Call {
                    head: "Times".to_string(),
                    args: vec![Value::Integer(Integer::from(-1)), args[0].clone()],
                })
            }
        }
    } else if args.len() == 2 {
        // Subtraction
        let neg = builtin_minus(&[args[1].clone()])?;
        add_values(&args[0], &neg)
    } else {
        Err(EvalError::Error("Minus requires 1 or 2 arguments".to_string()))
    }
}

fn builtin_abs(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("Abs requires exactly 1 argument".to_string()));
    }
    match &args[0] {
        Value::Integer(n) => Ok(Value::Integer(n.clone().abs())),
        Value::Real(r) => Ok(Value::Real(r.clone().abs())),
        _ => {
            Ok(Value::Call {
                head: "Abs".to_string(),
                args: args.to_vec(),
            })
        }
    }
}

// ── Comparison ──

fn builtin_equal(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error("Equal requires exactly 2 arguments".to_string()));
    }
    Ok(Value::Bool(args[0].struct_eq(&args[1])))
}

fn builtin_unequal(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error("Unequal requires exactly 2 arguments".to_string()));
    }
    Ok(Value::Bool(!args[0].struct_eq(&args[1])))
}

fn builtin_less(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error("Less requires exactly 2 arguments".to_string()));
    }
    match (&args[0], &args[1]) {
        (Value::Integer(a), Value::Integer(b)) => Ok(Value::Bool(a < b)),
        (Value::Real(a), Value::Real(b)) => Ok(Value::Bool(a < b)),
        (Value::Integer(a), Value::Real(b)) => Ok(Value::Bool(Float::with_val(DEFAULT_PRECISION, a) < *b)),
        (Value::Real(a), Value::Integer(b)) => Ok(Value::Bool(*a < Float::with_val(DEFAULT_PRECISION, b))),
        (Value::Str(a), Value::Str(b)) => Ok(Value::Bool(a < b)),
        _ => Err(EvalError::TypeError {
            expected: "Number or String".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

fn builtin_greater(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error("Greater requires exactly 2 arguments".to_string()));
    }
    match (&args[0], &args[1]) {
        (Value::Integer(a), Value::Integer(b)) => Ok(Value::Bool(a > b)),
        (Value::Real(a), Value::Real(b)) => Ok(Value::Bool(a > b)),
        (Value::Integer(a), Value::Real(b)) => Ok(Value::Bool(Float::with_val(DEFAULT_PRECISION, a) > *b)),
        (Value::Real(a), Value::Integer(b)) => Ok(Value::Bool(*a > Float::with_val(DEFAULT_PRECISION, b))),
        (Value::Str(a), Value::Str(b)) => Ok(Value::Bool(a > b)),
        _ => Err(EvalError::TypeError {
            expected: "Number or String".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

fn builtin_less_equal(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error("LessEqual requires exactly 2 arguments".to_string()));
    }
    match (&args[0], &args[1]) {
        (Value::Integer(a), Value::Integer(b)) => Ok(Value::Bool(a <= b)),
        (Value::Real(a), Value::Real(b)) => Ok(Value::Bool(a <= b)),
        (Value::Integer(a), Value::Real(b)) => Ok(Value::Bool(Float::with_val(DEFAULT_PRECISION, a) <= *b)),
        (Value::Real(a), Value::Integer(b)) => Ok(Value::Bool(*a <= Float::with_val(DEFAULT_PRECISION, b))),
        (Value::Str(a), Value::Str(b)) => Ok(Value::Bool(a <= b)),
        _ => Err(EvalError::TypeError {
            expected: "Number or String".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

fn builtin_greater_equal(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error("GreaterEqual requires exactly 2 arguments".to_string()));
    }
    match (&args[0], &args[1]) {
        (Value::Integer(a), Value::Integer(b)) => Ok(Value::Bool(a >= b)),
        (Value::Real(a), Value::Real(b)) => Ok(Value::Bool(a >= b)),
        (Value::Integer(a), Value::Real(b)) => Ok(Value::Bool(Float::with_val(DEFAULT_PRECISION, a) >= *b)),
        (Value::Real(a), Value::Integer(b)) => Ok(Value::Bool(*a >= Float::with_val(DEFAULT_PRECISION, b))),
        (Value::Str(a), Value::Str(b)) => Ok(Value::Bool(a >= b)),
        _ => Err(EvalError::TypeError {
            expected: "Number or String".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

// ── Logical ──

fn builtin_and(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error("And requires exactly 2 arguments".to_string()));
    }
    Ok(Value::Bool(args[0].to_bool() && args[1].to_bool()))
}

fn builtin_or(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error("Or requires exactly 2 arguments".to_string()));
    }
    Ok(Value::Bool(args[0].to_bool() || args[1].to_bool()))
}

fn builtin_not(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("Not requires exactly 1 argument".to_string()));
    }
    Ok(Value::Bool(!args[0].to_bool()))
}

// ── List ──

fn builtin_length(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("Length requires exactly 1 argument".to_string()));
    }
    match &args[0] {
        Value::List(items) => Ok(Value::Integer(Integer::from(items.len() as i64))),
        Value::Str(s) => Ok(Value::Integer(Integer::from(s.len() as i64))),
        _ => Ok(Value::Integer(Integer::from(1))),
    }
}

fn builtin_first(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("First requires exactly 1 argument".to_string()));
    }
    match &args[0] {
        Value::List(items) => {
            if items.is_empty() {
                Err(EvalError::Error("First called on empty list".to_string()))
            } else {
                Ok(items[0].clone())
            }
        }
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

fn builtin_last(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("Last requires exactly 1 argument".to_string()));
    }
    match &args[0] {
        Value::List(items) => {
            if items.is_empty() {
                Err(EvalError::Error("Last called on empty list".to_string()))
            } else {
                Ok(items[items.len() - 1].clone())
            }
        }
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

fn builtin_rest(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("Rest requires exactly 1 argument".to_string()));
    }
    match &args[0] {
        Value::List(items) => {
            if items.is_empty() {
                Err(EvalError::Error("Rest called on empty list".to_string()))
            } else {
                Ok(Value::List(items[1..].to_vec()))
            }
        }
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

fn builtin_most(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("Most requires exactly 1 argument".to_string()));
    }
    match &args[0] {
        Value::List(items) => {
            if items.is_empty() {
                Err(EvalError::Error("Most called on empty list".to_string()))
            } else {
                Ok(Value::List(items[..items.len() - 1].to_vec()))
            }
        }
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

fn builtin_append(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error("Append requires exactly 2 arguments".to_string()));
    }
    match &args[0] {
        Value::List(items) => {
            let mut new_items = items.clone();
            new_items.push(args[1].clone());
            Ok(Value::List(new_items))
        }
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

fn builtin_prepend(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error("Prepend requires exactly 2 arguments".to_string()));
    }
    match &args[0] {
        Value::List(items) => {
            let mut new_items = vec![args[1].clone()];
            new_items.extend(items.clone());
            Ok(Value::List(new_items))
        }
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

fn builtin_join(args: &[Value]) -> Result<Value, EvalError> {
    let mut result = Vec::new();
    for arg in args {
        match arg {
            Value::List(items) => result.extend(items.clone()),
            _ => return Err(EvalError::TypeError {
                expected: "List".to_string(),
                got: arg.type_name().to_string(),
            }),
        }
    }
    Ok(Value::List(result))
}

fn builtin_flatten(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("Flatten requires exactly 1 argument".to_string()));
    }
    fn flatten(val: &Value) -> Vec<Value> {
        match val {
            Value::List(items) => {
                let mut result = Vec::new();
                for item in items {
                    result.extend(flatten(item));
                }
                result
            }
            _ => vec![val.clone()],
        }
    }
    Ok(Value::List(flatten(&args[0])))
}

fn builtin_sort(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("Sort requires exactly 1 argument".to_string()));
    }
    match &args[0] {
        Value::List(items) => {
            let mut sorted = items.clone();
            sorted.sort_by(|a, b| {
                match (a, b) {
                    (Value::Integer(x), Value::Integer(y)) => x.cmp(y),
                    (Value::Real(x), Value::Real(y)) => x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal),
                    (Value::Str(x), Value::Str(y)) => x.cmp(y),
                    _ => std::cmp::Ordering::Equal,
                }
            });
            Ok(Value::List(sorted))
        }
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

fn builtin_reverse(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("Reverse requires exactly 1 argument".to_string()));
    }
    match &args[0] {
        Value::List(items) => {
            let mut reversed = items.clone();
            reversed.reverse();
            Ok(Value::List(reversed))
        }
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

fn builtin_part(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error("Part requires at least 2 arguments".to_string()));
    }
    match &args[0] {
        Value::List(items) => {
            let index = match &args[1] {
                Value::Integer(n) => n.to_i64().unwrap_or(0),
                _ => return Err(EvalError::TypeError {
                    expected: "Integer".to_string(),
                    got: args[1].type_name().to_string(),
                }),
            };
            let idx = if index > 0 {
                (index - 1) as usize
            } else if index < 0 {
                (items.len() as i64 + index) as usize
            } else {
                return Err(EvalError::IndexOutOfBounds { index, length: items.len() });
            };
            if idx < items.len() {
                Ok(items[idx].clone())
            } else {
                Err(EvalError::IndexOutOfBounds { index, length: items.len() })
            }
        }
        Value::Str(s) => {
            let index = match &args[1] {
                Value::Integer(n) => n.to_i64().unwrap_or(0),
                _ => return Err(EvalError::TypeError {
                    expected: "Integer".to_string(),
                    got: args[1].type_name().to_string(),
                }),
            };
            let idx = if index > 0 {
                (index - 1) as usize
            } else {
                return Err(EvalError::IndexOutOfBounds { index, length: s.len() });
            };
            if idx < s.len() {
                Ok(Value::Str(s.chars().nth(idx).unwrap().to_string()))
            } else {
                Err(EvalError::IndexOutOfBounds { index, length: s.len() })
            }
        }
        _ => Err(EvalError::TypeError {
            expected: "List or String".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

fn builtin_range(args: &[Value]) -> Result<Value, EvalError> {
    match args.len() {
        1 => {
            let n = args[0].to_integer()
                .ok_or_else(|| EvalError::TypeError {
                    expected: "Integer".to_string(),
                    got: args[0].type_name().to_string(),
                })?;
            Ok(Value::List((1..=n).map(|i| Value::Integer(Integer::from(i))).collect()))
        }
        2 => {
            let start = args[0].to_integer()
                .ok_or_else(|| EvalError::TypeError {
                    expected: "Integer".to_string(),
                    got: args[0].type_name().to_string(),
                })?;
            let end = args[1].to_integer()
                .ok_or_else(|| EvalError::TypeError {
                    expected: "Integer".to_string(),
                    got: args[1].type_name().to_string(),
                })?;
            Ok(Value::List((start..=end).map(|i| Value::Integer(Integer::from(i))).collect()))
        }
        3 => {
            let start = args[0].to_integer()
                .ok_or_else(|| EvalError::TypeError {
                    expected: "Integer".to_string(),
                    got: args[0].type_name().to_string(),
                })?;
            let end = args[2].to_integer()
                .ok_or_else(|| EvalError::TypeError {
                    expected: "Integer".to_string(),
                    got: args[2].type_name().to_string(),
                })?;
            let step = args[1].to_integer()
                .ok_or_else(|| EvalError::TypeError {
                    expected: "Integer".to_string(),
                    got: args[1].type_name().to_string(),
                })?;
            if step == 0 {
                return Err(EvalError::Error("Range step cannot be zero".to_string()));
            }
            let mut result = Vec::new();
            if step > 0 {
                let mut i = start;
                while i <= end {
                    result.push(Value::Integer(Integer::from(i)));
                    i += step;
                }
            } else {
                let mut i = start;
                while i >= end {
                    result.push(Value::Integer(Integer::from(i)));
                    i += step;
                }
            }
            Ok(Value::List(result))
        }
        _ => Err(EvalError::Error("Range requires 1-3 arguments".to_string())),
    }
}

fn builtin_table(_args: &[Value]) -> Result<Value, EvalError> {
    // TODO: implement Table with iterator spec
    Err(EvalError::Error("Table not yet implemented".to_string()))
}

fn builtin_map(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error("Map requires exactly 2 arguments".to_string()));
    }
    // Map is handled by the evaluator for proper function application
    Err(EvalError::Error("Map should be handled by evaluator".to_string()))
}

fn builtin_fold(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Err(EvalError::Error("Fold requires exactly 3 arguments".to_string()));
    }
    // Fold is handled by the evaluator for proper function application
    Err(EvalError::Error("Fold should be handled by evaluator".to_string()))
}

fn builtin_select(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error("Select requires exactly 2 arguments".to_string()));
    }
    // Select is handled by the evaluator for proper function application
    Err(EvalError::Error("Select should be handled by evaluator".to_string()))
}

fn builtin_scan(_args: &[Value]) -> Result<Value, EvalError> {
    Err(EvalError::Error("Scan should be handled by evaluator".to_string()))
}

fn builtin_nest(_args: &[Value]) -> Result<Value, EvalError> {
    Err(EvalError::Error("Nest should be handled by evaluator".to_string()))
}

fn builtin_take(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error("Take requires exactly 2 arguments".to_string()));
    }
    match (&args[0], &args[1]) {
        (Value::List(items), Value::Integer(n)) => {
            let n_i64 = n.to_i64().unwrap_or(0);
            let count = if n_i64 >= 0 { n_i64 as usize } else { items.len() - (-n_i64) as usize };
            Ok(Value::List(items[..count.min(items.len())].to_vec()))
        }
        _ => Err(EvalError::TypeError {
            expected: "List and Integer".to_string(),
            got: format!("{} and {}", args[0].type_name(), args[1].type_name()),
        }),
    }
}

fn builtin_drop(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error("Drop requires exactly 2 arguments".to_string()));
    }
    match (&args[0], &args[1]) {
        (Value::List(items), Value::Integer(n)) => {
            let n_i64 = n.to_i64().unwrap_or(0);
            let count = if n_i64 >= 0 { n_i64 as usize } else { items.len() - (-n_i64) as usize };
            Ok(Value::List(items[count.min(items.len())..].to_vec()))
        }
        _ => Err(EvalError::TypeError {
            expected: "List and Integer".to_string(),
            got: format!("{} and {}", args[0].type_name(), args[1].type_name()),
        }),
    }
}

fn builtin_riffle(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error("Riffle requires exactly 2 arguments".to_string()));
    }
    match (&args[0], &args[1]) {
        (Value::List(items), sep) => {
            let mut result = Vec::new();
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    result.push(sep.clone());
                }
                result.push(item.clone());
            }
            Ok(Value::List(result))
        }
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

fn builtin_transpose(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("Transpose requires exactly 1 argument".to_string()));
    }
    match &args[0] {
        Value::List(rows) => {
            if rows.is_empty() {
                return Ok(Value::List(vec![]));
            }
            let cols = match &rows[0] {
                Value::List(items) => items.len(),
                _ => return Err(EvalError::TypeError {
                    expected: "List of Lists".to_string(),
                    got: "List of non-Lists".to_string(),
                }),
            };
            let mut result = vec![Vec::new(); cols];
            for row in rows {
                match row {
                    Value::List(items) => {
                        for (j, item) in items.iter().enumerate() {
                            result[j].push(item.clone());
                        }
                    }
                    _ => return Err(EvalError::TypeError {
                        expected: "List of Lists".to_string(),
                        got: "List of non-Lists".to_string(),
                    }),
                }
            }
            Ok(Value::List(result.into_iter().map(Value::List).collect()))
        }
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

fn builtin_total(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("Total requires exactly 1 argument".to_string()));
    }
    match &args[0] {
        Value::List(items) => {
            builtin_plus(items)
        }
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

fn builtin_sum(_args: &[Value]) -> Result<Value, EvalError> {
    // Sum[expr, {i, min, max}] — handled by evaluator
    Err(EvalError::Error("Sum should be handled by evaluator".to_string()))
}

// ── Pattern ──

fn builtin_match_q(_args: &[Value]) -> Result<Value, EvalError> {
    // MatchQ[value, pattern] — needs evaluator
    Err(EvalError::Error("MatchQ should be handled by evaluator".to_string()))
}

fn builtin_head(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("Head requires exactly 1 argument".to_string()));
    }
    Ok(Value::Symbol(args[0].type_name().to_string()))
}

fn builtin_type_of(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("TypeOf requires exactly 1 argument".to_string()));
    }
    Ok(Value::Symbol(args[0].type_name().to_string()))
}

fn builtin_free_q(_args: &[Value]) -> Result<Value, EvalError> {
    // FreeQ[expr, pattern] — needs evaluator
    Err(EvalError::Error("FreeQ should be handled by evaluator".to_string()))
}

// ── String ──

fn builtin_string_join(args: &[Value]) -> Result<Value, EvalError> {
    let mut result = String::new();
    for arg in args {
        match arg {
            Value::Str(s) => result.push_str(s),
            _ => result.push_str(&arg.to_string()),
        }
    }
    Ok(Value::Str(result))
}

fn builtin_string_length(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("StringLength requires exactly 1 argument".to_string()));
    }
    match &args[0] {
        Value::Str(s) => Ok(Value::Integer(Integer::from(s.len() as i64))),
        _ => Err(EvalError::TypeError {
            expected: "String".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

fn builtin_to_string(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("ToString requires exactly 1 argument".to_string()));
    }
    Ok(Value::Str(args[0].to_string()))
}

// builtin_to_expression is defined later with the real implementation

// ── Math ──

fn builtin_sin(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("Sin requires exactly 1 argument".to_string()));
    }
    match &args[0] {
        Value::Integer(n) => {
            let f = Float::with_val(DEFAULT_PRECISION, n);
            Ok(Value::Real(f.sin()))
        }
        Value::Real(r) => Ok(Value::Real(r.clone().sin())),
        _ => {
            Ok(Value::Call {
                head: "Sin".to_string(),
                args: args.to_vec(),
            })
        }
    }
}

fn builtin_cos(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("Cos requires exactly 1 argument".to_string()));
    }
    match &args[0] {
        Value::Integer(n) => {
            let f = Float::with_val(DEFAULT_PRECISION, n);
            Ok(Value::Real(f.cos()))
        }
        Value::Real(r) => Ok(Value::Real(r.clone().cos())),
        _ => {
            Ok(Value::Call {
                head: "Cos".to_string(),
                args: args.to_vec(),
            })
        }
    }
}

fn builtin_tan(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("Tan requires exactly 1 argument".to_string()));
    }
    match &args[0] {
        Value::Integer(n) => {
            let f = Float::with_val(DEFAULT_PRECISION, n);
            Ok(Value::Real(f.tan()))
        }
        Value::Real(r) => Ok(Value::Real(r.clone().tan())),
        _ => {
            Ok(Value::Call {
                head: "Tan".to_string(),
                args: args.to_vec(),
            })
        }
    }
}

fn builtin_log(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("Log requires exactly 1 argument".to_string()));
    }
    match &args[0] {
        Value::Integer(n) => {
            if n.is_zero() || n.is_negative() {
                Err(EvalError::Error("Log of non-positive number".to_string()))
            } else {
                let f = Float::with_val(DEFAULT_PRECISION, n);
                Ok(Value::Real(f.ln()))
            }
        }
        Value::Real(r) => {
            if r.is_zero() || r.is_sign_negative() {
                Err(EvalError::Error("Log of non-positive number".to_string()))
            } else {
                Ok(Value::Real(r.clone().ln()))
            }
        }
        _ => {
            Ok(Value::Call {
                head: "Log".to_string(),
                args: args.to_vec(),
            })
        }
    }
}

fn builtin_exp(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("Exp requires exactly 1 argument".to_string()));
    }
    match &args[0] {
        Value::Integer(n) => {
            let f = Float::with_val(DEFAULT_PRECISION, n);
            Ok(Value::Real(f.exp()))
        }
        Value::Real(r) => Ok(Value::Real(r.clone().exp())),
        _ => {
            Ok(Value::Call {
                head: "Exp".to_string(),
                args: args.to_vec(),
            })
        }
    }
}

fn builtin_sqrt(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("Sqrt requires exactly 1 argument".to_string()));
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
                Ok(Value::Real(r))
            }
        }
        Value::Real(r) => {
            if r.is_sign_negative() {
                Err(EvalError::Error("Sqrt of negative number".to_string()))
            } else {
                Ok(Value::Real(r.clone().sqrt()))
            }
        }
        _ => {
            Ok(Value::Call {
                head: "Sqrt".to_string(),
                args: args.to_vec(),
            })
        }
    }
}

fn builtin_floor(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("Floor requires exactly 1 argument".to_string()));
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

fn builtin_ceiling(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("Ceiling requires exactly 1 argument".to_string()));
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

fn builtin_round(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("Round requires exactly 1 argument".to_string()));
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

fn builtin_max(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Err(EvalError::Error("Max requires at least 1 argument".to_string()));
    }
    let mut max = &args[0];
    for arg in &args[1..] {
        match (max, arg) {
            (Value::Integer(a), Value::Integer(b)) => {
                if b > a { max = arg; }
            }
            (Value::Real(a), Value::Real(b)) => {
                if b > a { max = arg; }
            }
            _ => {}
        }
    }
    Ok(max.clone())
}

fn builtin_min(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Err(EvalError::Error("Min requires at least 1 argument".to_string()));
    }
    let mut min = &args[0];
    for arg in &args[1..] {
        match (min, arg) {
            (Value::Integer(a), Value::Integer(b)) => {
                if b < a { min = arg; }
            }
            (Value::Real(a), Value::Real(b)) => {
                if b < a { min = arg; }
            }
            _ => {}
        }
    }
    Ok(min.clone())
}

// ── I/O ──

fn builtin_print(args: &[Value]) -> Result<Value, EvalError> {
    for (i, arg) in args.iter().enumerate() {
        if i > 0 {
            print!(" ");
        }
        print!("{}", arg);
    }
    println!();
    Ok(Value::Null)
}

// ── Association ──

fn builtin_keys(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("Keys requires exactly 1 argument".to_string()));
    }
    match &args[0] {
        Value::Assoc(map) => {
            Ok(Value::List(map.keys().map(|k| Value::Str(k.clone())).collect()))
        }
        _ => Err(EvalError::TypeError {
            expected: "Assoc".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

fn builtin_values(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("Values requires exactly 1 argument".to_string()));
    }
    match &args[0] {
        Value::Assoc(map) => {
            Ok(Value::List(map.values().cloned().collect()))
        }
        _ => Err(EvalError::TypeError {
            expected: "Assoc".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

// ── Symbolic ──

fn builtin_simplify(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("Simplify requires exactly 1 argument".to_string()));
    }
    Ok(simplify_value(&args[0]))
}

fn simplify_value(val: &Value) -> Value {
    match val {
        Value::Call { head, args } => {
            let simplified_args: Vec<Value> = args.iter().map(simplify_value).collect();
            simplify_call(head, &simplified_args)
        }
        _ => val.clone(),
    }
}

fn simplify_call(head: &str, args: &[Value]) -> Value {
    match head {
        "Plus" => simplify_plus(args),
        "Times" => simplify_times(args),
        "Power" => simplify_power(args),
        "Sin" => simplify_sin(args),
        "Cos" => simplify_cos(args),
        "Log" => simplify_log(args),
        "Exp" => simplify_exp(args),
        _ => Value::Call { head: head.to_string(), args: args.to_vec() },
    }
}

fn simplify_plus(args: &[Value]) -> Value {
    if args.is_empty() { return Value::Integer(Integer::from(0)); }
    let mut terms: Vec<Value> = Vec::new();
    for arg in args {
        match arg {
            Value::Integer(n) if n.is_zero() => {}
            Value::Call { head, args: a } if head == "Plus" => {
                terms.extend(a.iter().cloned());
            }
            _ => terms.push(arg.clone()),
        }
    }
    if terms.is_empty() { return Value::Integer(Integer::from(0)); }
    if terms.len() == 1 { return terms.into_iter().next().unwrap(); }
    if terms.len() == 2 && terms[0].struct_eq(&terms[1]) {
        return simplify_call("Times", &[Value::Integer(Integer::from(2)), terms[0].clone()]);
    }
    Value::Call { head: "Plus".to_string(), args: terms }
}

fn simplify_times(args: &[Value]) -> Value {
    if args.is_empty() { return Value::Integer(Integer::from(1)); }
    let mut factors: Vec<Value> = Vec::new();
    for arg in args {
        match arg {
            Value::Integer(n) if n.is_zero() => return Value::Integer(Integer::from(0)),
            Value::Integer(n) if *n == 1 => {}
            Value::Call { head, args: a } if head == "Times" => {
                factors.extend(a.iter().cloned());
            }
            _ => factors.push(arg.clone()),
        }
    }
    if factors.is_empty() { return Value::Integer(Integer::from(1)); }
    if factors.len() == 1 { return factors.into_iter().next().unwrap(); }
    Value::Call { head: "Times".to_string(), args: factors }
}

fn simplify_power(args: &[Value]) -> Value {
    if args.len() != 2 { return Value::Call { head: "Power".to_string(), args: args.to_vec() }; }
    match &args[1] {
        Value::Integer(n) if n.is_zero() => Value::Integer(Integer::from(1)),
        Value::Integer(n) if *n == 1 => args[0].clone(),
        _ => match &args[0] {
            Value::Integer(n) if n.is_zero() => Value::Integer(Integer::from(0)),
            Value::Integer(n) if *n == 1 => Value::Integer(Integer::from(1)),
            _ => Value::Call { head: "Power".to_string(), args: args.to_vec() },
        }
    }
}

fn simplify_sin(args: &[Value]) -> Value {
    if args.len() != 1 { return Value::Call { head: "Sin".to_string(), args: args.to_vec() }; }
    match &args[0] {
        Value::Integer(n) if n.is_zero() => Value::Integer(Integer::from(0)),
        _ => Value::Call { head: "Sin".to_string(), args: args.to_vec() },
    }
}

fn simplify_cos(args: &[Value]) -> Value {
    if args.len() != 1 { return Value::Call { head: "Cos".to_string(), args: args.to_vec() }; }
    match &args[0] {
        Value::Integer(n) if n.is_zero() => Value::Integer(Integer::from(1)),
        _ => Value::Call { head: "Cos".to_string(), args: args.to_vec() },
    }
}

fn simplify_log(args: &[Value]) -> Value {
    if args.len() != 1 { return Value::Call { head: "Log".to_string(), args: args.to_vec() }; }
    match &args[0] {
        Value::Integer(n) if *n == 1 => Value::Integer(Integer::from(0)),
        Value::Real(r) => {
            let e_val = Float::with_val(DEFAULT_PRECISION, 1).exp();
            if (r.clone() - e_val).abs() < 1e-10 {
                Value::Integer(Integer::from(1))
            } else {
                Value::Call { head: "Log".to_string(), args: args.to_vec() }
            }
        }
        Value::Call { head, args: inner } if head == "Exp" && inner.len() == 1 => inner[0].clone(),
        _ => Value::Call { head: "Log".to_string(), args: args.to_vec() },
    }
}

fn simplify_exp(args: &[Value]) -> Value {
    if args.len() != 1 { return Value::Call { head: "Exp".to_string(), args: args.to_vec() }; }
    match &args[0] {
        Value::Integer(n) if n.is_zero() => Value::Integer(Integer::from(1)),
        Value::Call { head, args: inner } if head == "Log" && inner.len() == 1 => inner[0].clone(),
        _ => Value::Call { head: "Exp".to_string(), args: args.to_vec() },
    }
}

fn builtin_expand(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("Expand requires exactly 1 argument".to_string()));
    }
    Ok(expand_value(&args[0]))
}

fn expand_value(val: &Value) -> Value {
    match val {
        Value::Call { head, args } => {
            let expanded_args: Vec<Value> = args.iter().map(expand_value).collect();
            match head.as_str() {
                "Times" => expand_times(&expanded_args),
                "Power" => expand_power(&expanded_args),
                _ => Value::Call { head: head.to_string(), args: expanded_args },
            }
        }
        _ => val.clone(),
    }
}

fn expand_times(args: &[Value]) -> Value {
    if args.len() != 2 {
        return Value::Call { head: "Times".to_string(), args: args.to_vec() };
    }
    let (left, right) = (&args[0], &args[1]);
    if let Value::Call { head, args: plus_args } = right {
        if head == "Plus" {
            let terms: Vec<Value> = plus_args.iter()
                .map(|term| simplify_call("Times", &[left.clone(), term.clone()]))
                .collect();
            return simplify_call("Plus", &terms);
        }
    }
    if let Value::Call { head, args: plus_args } = left {
        if head == "Plus" {
            let terms: Vec<Value> = plus_args.iter()
                .map(|term| simplify_call("Times", &[term.clone(), right.clone()]))
                .collect();
            return simplify_call("Plus", &terms);
        }
    }
    Value::Call { head: "Times".to_string(), args: args.to_vec() }
}

fn expand_power(args: &[Value]) -> Value {
    if args.len() != 2 {
        return Value::Call { head: "Power".to_string(), args: args.to_vec() };
    }
    let (base, exp) = (&args[0], &args[1]);
    if let Value::Integer(n) = exp {
        if let Some(n_i64) = n.to_i64() {
            if let Value::Call { head, args: plus_args } = base {
                if head == "Plus" && plus_args.len() == 2 && n_i64 >= 0 && n_i64 <= 10 {
                    let (a, b) = (&plus_args[0], &plus_args[1]);
                    let mut terms = Vec::new();
                    for k in 0..=n_i64 {
                        let coeff = binomial(n_i64, k);
                        let a_pow = if n_i64 - k == 0 { Value::Integer(Integer::from(1)) }
                            else if n_i64 - k == 1 { a.clone() }
                            else { simplify_call("Power", &[a.clone(), Value::Integer(Integer::from(n_i64 - k))]) };
                        let b_pow = if k == 0 { Value::Integer(Integer::from(1)) }
                            else if k == 1 { b.clone() }
                            else { simplify_call("Power", &[b.clone(), Value::Integer(Integer::from(k))]) };
                        let term = simplify_call("Times", &[Value::Integer(Integer::from(coeff)), a_pow, b_pow]);
                        terms.push(term);
                    }
                    return simplify_call("Plus", &terms);
                }
            }
        }
    }
    Value::Call { head: "Power".to_string(), args: args.to_vec() }
}

fn binomial(n: i64, k: i64) -> i64 {
    if k < 0 || k > n { return 0; }
    if k == 0 || k == n { return 1; }
    let k = if k > n - k { n - k } else { k };
    let mut result = 1i64;
    for i in 0..k {
        result = result * (n - i) / (i + 1);
    }
    result
}

/// D[expr, x] — Symbolic differentiation.
fn builtin_d(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error("D requires exactly 2 arguments".to_string()));
    }
    let var = match &args[1] {
        Value::Symbol(s) => s.clone(),
        _ => return Err(EvalError::TypeError {
            expected: "Symbol".to_string(),
            got: args[1].type_name().to_string(),
        }),
    };
    Ok(differentiate(&args[0], &var))
}

fn differentiate(expr: &Value, var: &str) -> Value {
    match expr {
        Value::Integer(_) | Value::Real(_) | Value::Bool(_) | Value::Str(_) | Value::Null => Value::Integer(Integer::from(0)),
        Value::Symbol(s) => {
            if s == var { Value::Integer(Integer::from(1)) } else { Value::Integer(Integer::from(0)) }
        }
        Value::Call { head, args } => {
            match head.as_str() {
                "Plus" => {
                    let terms: Vec<Value> = args.iter().map(|arg| differentiate(arg, var)).collect();
                    simplify_call("Plus", &terms)
                }
                "Times" => {
                    if args.len() == 2 {
                        let (u, v) = (&args[0], &args[1]);
                        let du = differentiate(u, var);
                        let dv = differentiate(v, var);
                        simplify_call("Plus", &[
                            simplify_call("Times", &[du, v.clone()]),
                            simplify_call("Times", &[u.clone(), dv]),
                        ])
                    } else if args.len() == 1 {
                        differentiate(&args[0], var)
                    } else {
                        let mut result = args[0].clone();
                        for i in 1..args.len() {
                            result = simplify_call("Times", &[result, args[i].clone()]);
                        }
                        differentiate(&result, var)
                    }
                }
                "Power" if args.len() == 2 => {
                    let (base, exp) = (&args[0], &args[1]);
                    let dbase = differentiate(base, var);
                    match exp {
                        Value::Integer(n) => {
                            simplify_call("Times", &[
                                Value::Integer(n.clone()),
                                simplify_call("Power", &[base.clone(), Value::Integer(n - Integer::from(1))]),
                                dbase,
                            ])
                        }
                        Value::Real(n) => {
                            let n_minus_1 = n.clone() - 1.0;
                            simplify_call("Times", &[
                                Value::Real(n.clone()),
                                simplify_call("Power", &[base.clone(), Value::Real(n_minus_1)]),
                                dbase,
                            ])
                        }
                        _ => {
                            let dexp = differentiate(exp, var);
                            simplify_call("Times", &[
                                expr.clone(),
                                simplify_call("Plus", &[
                                    simplify_call("Times", &[dexp, simplify_call("Log", &[base.clone()])]),
                                    simplify_call("Times", &[exp.clone(), simplify_call("Times", &[dbase, simplify_call("Power", &[base.clone(), Value::Integer(Integer::from(-1))])])]),
                                ]),
                            ])
                        }
                    }
                }
                "Sin" if args.len() == 1 => {
                    simplify_call("Times", &[simplify_call("Cos", &[args[0].clone()]), differentiate(&args[0], var)])
                }
                "Cos" if args.len() == 1 => {
                    simplify_call("Times", &[Value::Integer(Integer::from(-1)), simplify_call("Sin", &[args[0].clone()]), differentiate(&args[0], var)])
                }
                "Tan" if args.len() == 1 => {
                    simplify_call("Times", &[differentiate(&args[0], var), simplify_call("Power", &[simplify_call("Cos", &[args[0].clone()]), Value::Integer(Integer::from(-2))])])
                }
                "Exp" if args.len() == 1 => {
                    simplify_call("Times", &[simplify_call("Exp", &[args[0].clone()]), differentiate(&args[0], var)])
                }
                "Log" if args.len() == 1 => {
                    simplify_call("Times", &[differentiate(&args[0], var), simplify_call("Power", &[args[0].clone(), Value::Integer(Integer::from(-1))])])
                }
                "Sqrt" if args.len() == 1 => {
                    simplify_call("Times", &[differentiate(&args[0], var), simplify_call("Power", &[simplify_call("Times", &[Value::Integer(Integer::from(2)), simplify_call("Sqrt", &[args[0].clone()])]), Value::Integer(Integer::from(-1))])])
                }
                "ArcSin" if args.len() == 1 => {
                    simplify_call("Times", &[differentiate(&args[0], var), simplify_call("Power", &[simplify_call("Plus", &[Value::Integer(Integer::from(1)), simplify_call("Times", &[Value::Integer(Integer::from(-1)), simplify_call("Power", &[args[0].clone(), Value::Integer(Integer::from(2))])])]), Value::Real(Float::with_val(DEFAULT_PRECISION, -0.5))])])
                }
                "ArcCos" if args.len() == 1 => {
                    simplify_call("Times", &[Value::Integer(Integer::from(-1)), differentiate(&args[0], var), simplify_call("Power", &[simplify_call("Plus", &[Value::Integer(Integer::from(1)), simplify_call("Times", &[Value::Integer(Integer::from(-1)), simplify_call("Power", &[args[0].clone(), Value::Integer(Integer::from(2))])])]), Value::Real(Float::with_val(DEFAULT_PRECISION, -0.5))])])
                }
                "ArcTan" if args.len() == 1 => {
                    simplify_call("Times", &[differentiate(&args[0], var), simplify_call("Power", &[simplify_call("Plus", &[Value::Integer(Integer::from(1)), simplify_call("Power", &[args[0].clone(), Value::Integer(Integer::from(2))])]), Value::Integer(Integer::from(-1))])])
                }
                _ => Value::Call { head: "D".to_string(), args: vec![expr.clone(), Value::Symbol(var.to_string())] },
            }
        }
        _ => Value::Call { head: "D".to_string(), args: vec![expr.clone(), Value::Symbol(var.to_string())] },
    }
}

/// Integrate[expr, x] — Symbolic integration.
fn builtin_integrate(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error("Integrate requires exactly 2 arguments".to_string()));
    }
    let var = match &args[1] {
        Value::Symbol(s) => s.clone(),
        _ => return Err(EvalError::TypeError { expected: "Symbol".to_string(), got: args[1].type_name().to_string() }),
    };
    Ok(integrate(&args[0], &var))
}

fn integrate(expr: &Value, var: &str) -> Value {
    let x = Value::Symbol(var.to_string());
    match expr {
        Value::Integer(n) => simplify_call("Times", &[Value::Integer(n.clone()), x]),
        Value::Real(r) => simplify_call("Times", &[Value::Real(r.clone()), x]),
        Value::Symbol(s) => {
            if s == var {
                simplify_call("Times", &[Value::Real(Float::with_val(DEFAULT_PRECISION, 0.5)), simplify_call("Power", &[x, Value::Integer(Integer::from(2))])])
            } else {
                simplify_call("Times", &[Value::Symbol(s.clone()), x])
            }
        }
        Value::Call { head, args } => {
            match head.as_str() {
                "Plus" => {
                    let terms: Vec<Value> = args.iter().map(|a| integrate(a, var)).collect();
                    simplify_call("Plus", &terms)
                }
                "Times" => {
                    let (constants, vars): (Vec<_>, Vec<_>) = args.iter().partition(|a| is_constant_wrt(a, var));
                    if vars.is_empty() {
                        simplify_call("Times", &[simplify_call("Times", args), x])
                    } else if vars.len() == 1 {
                        let var_part = integrate(&vars[0], var);
                        let const_vals: Vec<Value> = constants.iter().map(|c| (*c).clone()).collect();
                        let const_product = if constants.is_empty() { Value::Integer(Integer::from(1)) } else { simplify_call("Times", &const_vals) };
                        simplify_call("Times", &[const_product, var_part])
                    } else {
                        Value::Call { head: "Integrate".to_string(), args: vec![expr.clone(), x] }
                    }
                }
                "Power" if args.len() == 2 && args[0].struct_eq(&x) => {
                    match &args[1] {
                        Value::Integer(n) if *n == -1 => simplify_call("Log", &[x]),
                        Value::Integer(n) => {
                            let new_exp: Integer = n.clone() + 1;
                            simplify_call("Times", &[simplify_call("Power", &[x, Value::Integer(new_exp.clone())]), simplify_call("Power", &[Value::Integer(new_exp), Value::Integer(Integer::from(-1))])])
                        }
                        Value::Real(n) => {
                            let new_exp: Float = n.clone() + 1.0;
                            simplify_call("Times", &[simplify_call("Power", &[x, Value::Real(new_exp.clone())]), simplify_call("Power", &[Value::Real(new_exp), Value::Integer(Integer::from(-1))])])
                        }
                        _ => Value::Call { head: "Integrate".to_string(), args: vec![expr.clone(), x] }
                    }
                }
                "Sin" if args.len() == 1 && args[0].struct_eq(&x) => {
                    simplify_call("Times", &[Value::Integer(Integer::from(-1)), simplify_call("Cos", &[x])])
                }
                "Cos" if args.len() == 1 && args[0].struct_eq(&x) => simplify_call("Sin", &[x]),
                "Exp" if args.len() == 1 && args[0].struct_eq(&x) => simplify_call("Exp", &[x]),
                "Tan" if args.len() == 1 && args[0].struct_eq(&x) => {
                    simplify_call("Times", &[Value::Integer(Integer::from(-1)), simplify_call("Log", &[simplify_call("Cos", &[x])])])
                }
                _ => Value::Call { head: "Integrate".to_string(), args: vec![expr.clone(), x] },
            }
        }
        _ => Value::Call { head: "Integrate".to_string(), args: vec![expr.clone(), x] },
    }
}

fn is_constant_wrt(val: &Value, var: &str) -> bool {
    match val {
        Value::Integer(_) | Value::Real(_) | Value::Bool(_) | Value::Str(_) | Value::Null => true,
        Value::Symbol(s) => s != var,
        Value::Call { args, .. } => args.iter().all(|a| is_constant_wrt(a, var)),
        _ => true,
    }
}

/// Factor[expr] — Polynomial factorization.
fn builtin_factor(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("Factor requires exactly 1 argument".to_string()));
    }
    Ok(args[0].clone())
}

/// Solve[equation, x] — Symbolic equation solving.
fn builtin_solve(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error("Solve requires exactly 2 arguments".to_string()));
    }
    let var = match &args[1] {
        Value::Symbol(s) => s.clone(),
        _ => return Err(EvalError::TypeError { expected: "Symbol".to_string(), got: args[1].type_name().to_string() }),
    };
    let (lhs, rhs) = match &args[0] {
        Value::Call { head, args: eq_args } if head == "Equal" && eq_args.len() == 2 => {
            (eq_args[0].clone(), eq_args[1].clone())
        }
        _ => return Ok(Value::Call { head: "Solve".to_string(), args: args.to_vec() }),
    };
    let poly = simplify_call("Plus", &[lhs, simplify_call("Times", &[Value::Integer(Integer::from(-1)), rhs])]);
    Ok(solve_polynomial(&poly, &var))
}

fn solve_polynomial(expr: &Value, var: &str) -> Value {
    let coeffs = extract_polynomial_coeffs(expr, var);
    match coeffs.len() {
        2 => {
            let (b, a) = (&coeffs[0], &coeffs[1]);
            match (a, b) {
                (Value::Integer(ai), Value::Integer(bi)) => {
                    if ai.is_zero() { return Value::List(vec![]); }
                    let result = Float::with_val(DEFAULT_PRECISION, bi) / Float::with_val(DEFAULT_PRECISION, ai);
                    let neg_result = -result;
                    Value::List(vec![Value::Rule { lhs: Box::new(Value::Symbol(var.to_string())), rhs: Box::new(Value::Real(neg_result)), delayed: false }])
                }
                _ => Value::List(vec![Value::Rule { lhs: Box::new(Value::Symbol(var.to_string())), rhs: Box::new(simplify_call("Times", &[Value::Integer(Integer::from(-1)), simplify_call("Power", &[a.clone(), Value::Integer(Integer::from(-1))]), b.clone()])), delayed: false }]),
            }
        }
        3 => {
            let (c, b, a) = (&coeffs[0], &coeffs[1], &coeffs[2]);
            match (a, b, c) {
                (Value::Integer(ai), Value::Integer(bi), Value::Integer(ci)) => {
                    let disc = bi * bi - Integer::from(4) * ai * ci;
                    if disc < 0 { return Value::List(vec![]); }
                    let disc_f = Float::with_val(DEFAULT_PRECISION, &disc);
                    let sqrt_disc = disc_f.sqrt();
                    let bi_f = Float::with_val(DEFAULT_PRECISION, bi);
                    let ai_f = Float::with_val(DEFAULT_PRECISION, ai);
                    let two = Float::with_val(DEFAULT_PRECISION, 2);
                    let x1 = (-bi_f.clone() + sqrt_disc.clone()) / (two.clone() * ai_f.clone());
                    let x2 = (-bi_f - sqrt_disc) / (two * ai_f);
                    if disc.is_zero() {
                        Value::List(vec![Value::Rule { lhs: Box::new(Value::Symbol(var.to_string())), rhs: Box::new(Value::Real(x1)), delayed: false }])
                    } else {
                        Value::List(vec![
                            Value::Rule { lhs: Box::new(Value::Symbol(var.to_string())), rhs: Box::new(Value::Real(x1)), delayed: false },
                            Value::Rule { lhs: Box::new(Value::Symbol(var.to_string())), rhs: Box::new(Value::Real(x2)), delayed: false },
                        ])
                    }
                }
                _ => Value::Call { head: "Solve".to_string(), args: vec![simplify_call("Equal", &[expr.clone(), Value::Integer(Integer::from(0))]), Value::Symbol(var.to_string())] },
            }
        }
        _ => Value::Call { head: "Solve".to_string(), args: vec![simplify_call("Equal", &[expr.clone(), Value::Integer(Integer::from(0))]), Value::Symbol(var.to_string())] },
    }
}

fn extract_polynomial_coeffs(expr: &Value, var: &str) -> Vec<Value> {
    let terms = flatten_to_plus_terms(expr);
    let mut max_degree = 0i64;
    let mut coeff_map: std::collections::HashMap<i64, Value> = std::collections::HashMap::new();
    for term in &terms {
        let (coeff, degree) = extract_term_coeff_degree(term, var);
        if degree >= 0 {
            max_degree = max_degree.max(degree);
            let existing = coeff_map.remove(&degree).unwrap_or(Value::Integer(Integer::from(0)));
            coeff_map.insert(degree, simplify_call("Plus", &[existing, coeff]));
        }
    }
    let mut result = Vec::new();
    for d in 0..=max_degree {
        result.push(coeff_map.remove(&d).unwrap_or(Value::Integer(Integer::from(0))));
    }
    result
}

fn flatten_to_plus_terms(expr: &Value) -> Vec<Value> {
    match expr {
        Value::Call { head, args } if head == "Plus" => {
            let mut result = Vec::new();
            for arg in args { result.extend(flatten_to_plus_terms(arg)); }
            result
        }
        _ => vec![expr.clone()],
    }
}

fn extract_term_coeff_degree(term: &Value, var: &str) -> (Value, i64) {
    match term {
        Value::Symbol(s) if s == var => (Value::Integer(Integer::from(1)), 1),
        Value::Symbol(_) | Value::Integer(_) | Value::Real(_) => {
            if is_constant_wrt(term, var) { (term.clone(), 0) } else { (Value::Integer(Integer::from(0)), -1) }
        }
        Value::Call { head, args } => {
            match head.as_str() {
                "Times" => {
                    let mut coeff = Value::Integer(Integer::from(1));
                    let mut degree = 0i64;
                    for arg in args {
                        let (c, d) = extract_term_coeff_degree(arg, var);
                        coeff = simplify_call("Times", &[coeff, c]);
                        degree += d;
                    }
                    (coeff, degree)
                }
                "Power" if args.len() == 2 && args[0].struct_eq(&Value::Symbol(var.to_string())) => {
                    match &args[1] {
                        Value::Integer(n) => (Value::Integer(Integer::from(1)), n.to_i64().unwrap_or(0)),
                        _ => (Value::Integer(Integer::from(0)), -1),
                    }
                }
                _ => if is_constant_wrt(term, var) { (term.clone(), 0) } else { (Value::Integer(Integer::from(0)), -1) },
            }
        }
        _ => (Value::Integer(Integer::from(0)), -1),
    }
}

/// Series[expr, {x, x0, n}] — Taylor series expansion.
fn builtin_series(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error("Series requires exactly 2 arguments".to_string()));
    }
    let spec = match &args[1] {
        Value::List(items) => items,
        _ => return Err(EvalError::TypeError { expected: "List".to_string(), got: args[1].type_name().to_string() }),
    };
    if spec.len() != 3 {
        return Err(EvalError::Error("Series spec must be {x, x0, n}".to_string()));
    }
    let var = match &spec[0] {
        Value::Symbol(s) => s.clone(),
        _ => return Err(EvalError::TypeError { expected: "Symbol".to_string(), got: spec[0].type_name().to_string() }),
    };
    let x0 = spec[1].clone();
    let order = spec[2].to_integer().ok_or_else(|| EvalError::TypeError { expected: "Integer".to_string(), got: spec[2].type_name().to_string() })?;

    let x_sym = Value::Symbol(var.clone());
    let mut terms = Vec::new();
    let mut derivative = args[0].clone();

    for n in 0..=order {
        let coeff_val = substitute_and_eval(&derivative, &var, &x0);
        let factorial_val = Value::Integer(factorial(n));
        let coeff = match (&coeff_val, &factorial_val) {
            (Value::Integer(c), Value::Integer(f)) if !f.is_zero() => {
                let c_f = Float::with_val(DEFAULT_PRECISION, c);
                let f_f = Float::with_val(DEFAULT_PRECISION, f);
                Value::Real(c_f / f_f)
            }
            (Value::Real(c), Value::Integer(f)) if !f.is_zero() => {
                let f_f = Float::with_val(DEFAULT_PRECISION, f);
                Value::Real(c / f_f)
            }
            _ => simplify_call("Times", &[coeff_val, simplify_call("Power", &[factorial_val, Value::Integer(Integer::from(-1))])]),
        };
        if n == 0 {
            terms.push(coeff);
        } else {
            let x_minus_x0 = simplify_call("Plus", &[x_sym.clone(), simplify_call("Times", &[Value::Integer(Integer::from(-1)), x0.clone()])]);
            let power_term = if n == 1 { x_minus_x0 } else { simplify_call("Power", &[x_minus_x0, Value::Integer(Integer::from(n))]) };
            terms.push(simplify_call("Times", &[coeff, power_term]));
        }
        derivative = differentiate(&derivative, &var);
    }
    Ok(simplify_call("Plus", &terms))
}

fn substitute_and_eval(expr: &Value, var: &str, val: &Value) -> Value {
    match expr {
        Value::Symbol(s) if s == var => val.clone(),
        Value::Symbol(_) | Value::Integer(_) | Value::Real(_) | Value::Bool(_) | Value::Str(_) | Value::Null => expr.clone(),
        Value::Call { head, args } => {
            let new_args: Vec<Value> = args.iter().map(|a| substitute_and_eval(a, var, val)).collect();
            let result = simplify_call(head, &new_args);
            match &result {
                Value::Call { head: h, args: a } if h == head => try_numerical_eval(head, a).unwrap_or(result),
                _ => result,
            }
        }
        _ => expr.clone(),
    }
}

fn try_numerical_eval(head: &str, args: &[Value]) -> Option<Value> {
    match head {
        "Plus" => {
            let mut sum = Float::with_val(DEFAULT_PRECISION, 0);
            let mut all_int = true;
            for arg in args {
                match arg {
                    Value::Integer(n) => sum += Float::with_val(DEFAULT_PRECISION, n),
                    Value::Real(r) => { sum += r; all_int = false; }
                    _ => return None,
                }
            }
            if all_int && sum.is_integer() {
                let i = sum.to_f64() as i64;
                return Some(Value::Integer(Integer::from(i)));
            }
            Some(Value::Real(sum))
        }
        "Times" => {
            let mut product = Float::with_val(DEFAULT_PRECISION, 1);
            let mut all_int = true;
            for arg in args {
                match arg {
                    Value::Integer(n) => product *= Float::with_val(DEFAULT_PRECISION, n),
                    Value::Real(r) => { product *= r; all_int = false; }
                    _ => return None,
                }
            }
            if all_int && product.is_integer() {
                let i = product.to_f64() as i64;
                return Some(Value::Integer(Integer::from(i)));
            }
            Some(Value::Real(product))
        }
        "Power" if args.len() == 2 => {
            match (&args[0], &args[1]) {
                (Value::Integer(base), Value::Integer(exp)) => {
                    if let Some(e) = exp.to_u32() {
                        Some(Value::Integer(base.clone().pow(e)))
                    } else {
                        let b = Float::with_val(DEFAULT_PRECISION, base);
                        let e = Float::with_val(DEFAULT_PRECISION, exp);
                        Some(Value::Real(b.pow(e)))
                    }
                }
                (Value::Real(base), Value::Real(exp)) => Some(Value::Real(base.clone().pow(exp))),
                (Value::Integer(base), Value::Real(exp)) => {
                    let b = Float::with_val(DEFAULT_PRECISION, base);
                    Some(Value::Real(b.pow(exp)))
                }
                (Value::Real(base), Value::Integer(exp)) => {
                    let e = Float::with_val(DEFAULT_PRECISION, exp);
                    Some(Value::Real(base.clone().pow(e)))
                }
                _ => None,
            }
        }
        "Sin" if args.len() == 1 => match &args[0] {
            Value::Integer(n) => { let f = Float::with_val(DEFAULT_PRECISION, n); Some(Value::Real(f.sin())) }
            Value::Real(r) => Some(Value::Real(r.clone().sin())),
            _ => None
        },
        "Cos" if args.len() == 1 => match &args[0] {
            Value::Integer(n) => { let f = Float::with_val(DEFAULT_PRECISION, n); Some(Value::Real(f.cos())) }
            Value::Real(r) => Some(Value::Real(r.clone().cos())),
            _ => None
        },
        "Exp" if args.len() == 1 => match &args[0] {
            Value::Integer(n) => { let f = Float::with_val(DEFAULT_PRECISION, n); Some(Value::Real(f.exp())) }
            Value::Real(r) => Some(Value::Real(r.clone().exp())),
            _ => None
        },
        "Log" if args.len() == 1 => match &args[0] {
            Value::Integer(n) if !n.is_zero() && !n.is_negative() => { let f = Float::with_val(DEFAULT_PRECISION, n); Some(Value::Real(f.ln())) }
            Value::Real(r) if !r.is_zero() && !r.is_sign_negative() => Some(Value::Real(r.clone().ln())),
            _ => None
        },
        _ => None,
    }
}

fn factorial(n: i64) -> Integer {
    if n <= 1 { Integer::from(1) } else { Integer::from(n) * factorial(n - 1) }
}

// ── FixedPoint stub (evaluator handles this) ──

fn builtin_fixed_point_stub(_args: &[Value]) -> Result<Value, EvalError> {
    Err(EvalError::Error("FixedPoint should be handled by evaluator".to_string()))
}

// ── New Math ──

fn builtin_arcsin(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 { return Err(EvalError::Error("ArcSin requires exactly 1 argument".to_string())); }
    match &args[0] {
        Value::Integer(n) => { let f = Float::with_val(DEFAULT_PRECISION, n); Ok(Value::Real(f.asin())) }
        Value::Real(r) => Ok(Value::Real(r.clone().asin())),
        _ => Ok(Value::Call { head: "ArcSin".to_string(), args: args.to_vec() }),
    }
}

fn builtin_arccos(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 { return Err(EvalError::Error("ArcCos requires exactly 1 argument".to_string())); }
    match &args[0] {
        Value::Integer(n) => { let f = Float::with_val(DEFAULT_PRECISION, n); Ok(Value::Real(f.acos())) }
        Value::Real(r) => Ok(Value::Real(r.clone().acos())),
        _ => Ok(Value::Call { head: "ArcCos".to_string(), args: args.to_vec() }),
    }
}

fn builtin_arctan(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 { return Err(EvalError::Error("ArcTan requires exactly 1 argument".to_string())); }
    match &args[0] {
        Value::Integer(n) => { let f = Float::with_val(DEFAULT_PRECISION, n); Ok(Value::Real(f.atan())) }
        Value::Real(r) => Ok(Value::Real(r.clone().atan())),
        _ => Ok(Value::Call { head: "ArcTan".to_string(), args: args.to_vec() }),
    }
}

fn builtin_log2(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 { return Err(EvalError::Error("Log2 requires exactly 1 argument".to_string())); }
    match &args[0] {
        Value::Integer(n) if !n.is_zero() && !n.is_negative() => {
            let f = Float::with_val(DEFAULT_PRECISION, n);
            Ok(Value::Real(f.log2()))
        }
        Value::Real(r) if !r.is_zero() && !r.is_sign_negative() => Ok(Value::Real(r.clone().log2())),
        _ => Err(EvalError::Error("Log2 of non-positive number".to_string())),
    }
}

fn builtin_log10(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 { return Err(EvalError::Error("Log10 requires exactly 1 argument".to_string())); }
    match &args[0] {
        Value::Integer(n) if !n.is_zero() && !n.is_negative() => {
            let f = Float::with_val(DEFAULT_PRECISION, n);
            Ok(Value::Real(f.log10()))
        }
        Value::Real(r) if !r.is_zero() && !r.is_sign_negative() => Ok(Value::Real(r.clone().log10())),
        _ => Err(EvalError::Error("Log10 of non-positive number".to_string())),
    }
}

fn builtin_mod(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 { return Err(EvalError::Error("Mod requires exactly 2 arguments".to_string())); }
    match (&args[0], &args[1]) {
        (Value::Integer(a), Value::Integer(b)) if !b.is_zero() => {
            let result = a.clone() % b;
            let result = if result < 0 { result + b.clone().abs() } else { result };
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
        _ => Err(EvalError::Error("Mod: division by zero or invalid types".to_string())),
    }
}

fn gcd(mut a: Integer, mut b: Integer) -> Integer {
    while !b.is_zero() { let t = b; b = a % t.clone(); a = t; }
    a.abs()
}

fn builtin_gcd(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 { return Err(EvalError::Error("GCD requires exactly 2 arguments".to_string())); }
    match (&args[0], &args[1]) {
        (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(gcd(a.clone(), b.clone()))),
        _ => Err(EvalError::TypeError { expected: "Integer".to_string(), got: format!("{} and {}", args[0].type_name(), args[1].type_name()) }),
    }
}

fn builtin_lcm(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 { return Err(EvalError::Error("LCM requires exactly 2 arguments".to_string())); }
    match (&args[0], &args[1]) {
        (Value::Integer(a), Value::Integer(b)) => {
            if a.is_zero() || b.is_zero() { return Ok(Value::Integer(Integer::from(0))); }
            let product = a.clone() * b;
            let abs_product = product.abs();
            let gcd_val = gcd(a.clone(), b.clone());
            Ok(Value::Integer(abs_product / gcd_val))
        }
        _ => Err(EvalError::TypeError { expected: "Integer".to_string(), got: format!("{} and {}", args[0].type_name(), args[1].type_name()) }),
    }
}

fn builtin_factorial(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 { return Err(EvalError::Error("Factorial requires exactly 1 argument".to_string())); }
    match &args[0] {
        Value::Integer(n) if *n >= 0 => {
            let n_i64 = n.to_i64().unwrap_or(0);
            Ok(Value::Integer(factorial(n_i64)))
        }
        _ => Err(EvalError::Error("Factorial requires a non-negative integer".to_string())),
    }
}

// ── New String ──

fn builtin_string_split(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 1 || args.len() > 2 { return Err(EvalError::Error("StringSplit requires 1 or 2 arguments".to_string())); }
    let s = match &args[0] { Value::Str(s) => s, _ => return Err(EvalError::TypeError { expected: "String".to_string(), got: args[0].type_name().to_string() }) };
    let delim = if args.len() == 2 { match &args[1] { Value::Str(d) => d.as_str(), _ => return Err(EvalError::TypeError { expected: "String".to_string(), got: args[1].type_name().to_string() }) } } else { " " };
    Ok(Value::List(s.split(delim).map(|part| Value::Str(part.to_string())).collect()))
}

fn builtin_string_replace(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 { return Err(EvalError::Error("StringReplace requires exactly 2 arguments".to_string())); }
    let s = match &args[0] { Value::Str(s) => s.clone(), _ => return Err(EvalError::TypeError { expected: "String".to_string(), got: args[0].type_name().to_string() }) };
    match &args[1] {
        Value::Rule { lhs, rhs, delayed: false } => {
            let old = match lhs.as_ref() { Value::Str(s) => s.clone(), _ => return Err(EvalError::TypeError { expected: "String".to_string(), got: lhs.type_name().to_string() }) };
            let new = match rhs.as_ref() { Value::Str(s) => s.clone(), _ => return Err(EvalError::TypeError { expected: "String".to_string(), got: rhs.type_name().to_string() }) };
            Ok(Value::Str(s.replace(&old, &new)))
        }
        Value::List(rules) => {
            let mut result = s;
            for rule in rules {
                if let Value::Rule { lhs, rhs, delayed: false } = rule {
                    if let (Value::Str(old), Value::Str(new)) = (lhs.as_ref(), rhs.as_ref()) { result = result.replace(old, new); }
                }
            }
            Ok(Value::Str(result))
        }
        _ => Err(EvalError::TypeError { expected: "Rule or List of Rules".to_string(), got: args[1].type_name().to_string() }),
    }
}

fn builtin_string_take(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 { return Err(EvalError::Error("StringTake requires exactly 2 arguments".to_string())); }
    let s = match &args[0] { Value::Str(s) => s, _ => return Err(EvalError::TypeError { expected: "String".to_string(), got: args[0].type_name().to_string() }) };
    let n = args[1].to_integer().ok_or_else(|| EvalError::TypeError { expected: "Integer".to_string(), got: args[1].type_name().to_string() })?;
    let chars: Vec<char> = s.chars().collect();
    let count = if n >= 0 { n as usize } else { chars.len().saturating_sub((-n) as usize) };
    Ok(Value::Str(chars[..count.min(chars.len())].iter().collect()))
}

fn builtin_string_drop(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 { return Err(EvalError::Error("StringDrop requires exactly 2 arguments".to_string())); }
    let s = match &args[0] { Value::Str(s) => s, _ => return Err(EvalError::TypeError { expected: "String".to_string(), got: args[0].type_name().to_string() }) };
    let n = args[1].to_integer().ok_or_else(|| EvalError::TypeError { expected: "Integer".to_string(), got: args[1].type_name().to_string() })?;
    let chars: Vec<char> = s.chars().collect();
    let count = if n >= 0 { n as usize } else { chars.len().saturating_sub((-n) as usize) };
    Ok(Value::Str(chars[count.min(chars.len())..].iter().collect()))
}

fn builtin_string_contains_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 { return Err(EvalError::Error("StringContainsQ requires exactly 2 arguments".to_string())); }
    let s = match &args[0] { Value::Str(s) => s, _ => return Err(EvalError::TypeError { expected: "String".to_string(), got: args[0].type_name().to_string() }) };
    let sub = match &args[1] { Value::Str(s) => s, _ => return Err(EvalError::TypeError { expected: "String".to_string(), got: args[1].type_name().to_string() }) };
    Ok(Value::Bool(s.contains(sub.as_str())))
}

fn builtin_string_reverse(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 { return Err(EvalError::Error("StringReverse requires exactly 1 argument".to_string())); }
    match &args[0] { Value::Str(s) => Ok(Value::Str(s.chars().rev().collect())), _ => Err(EvalError::TypeError { expected: "String".to_string(), got: args[0].type_name().to_string() }) }
}

fn builtin_to_upper_case(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 { return Err(EvalError::Error("ToUpperCase requires exactly 1 argument".to_string())); }
    match &args[0] { Value::Str(s) => Ok(Value::Str(s.to_uppercase())), _ => Err(EvalError::TypeError { expected: "String".to_string(), got: args[0].type_name().to_string() }) }
}

fn builtin_to_lower_case(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 { return Err(EvalError::Error("ToLowerCase requires exactly 1 argument".to_string())); }
    match &args[0] { Value::Str(s) => Ok(Value::Str(s.to_lowercase())), _ => Err(EvalError::TypeError { expected: "String".to_string(), got: args[0].type_name().to_string() }) }
}

// ── New List ──

fn builtin_member_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 { return Err(EvalError::Error("MemberQ requires exactly 2 arguments".to_string())); }
    match &args[0] {
        Value::List(items) => Ok(Value::Bool(items.iter().any(|item| item.struct_eq(&args[1])))),
        _ => Err(EvalError::TypeError { expected: "List".to_string(), got: args[0].type_name().to_string() }),
    }
}

fn builtin_count(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 { return Err(EvalError::Error("Count requires exactly 2 arguments".to_string())); }
    match &args[0] {
        Value::List(items) => Ok(Value::Integer(Integer::from(items.iter().filter(|item| item.struct_eq(&args[1])).count() as i64))),
        _ => Err(EvalError::TypeError { expected: "List".to_string(), got: args[0].type_name().to_string() }),
    }
}

fn builtin_position(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 { return Err(EvalError::Error("Position requires exactly 2 arguments".to_string())); }
    match &args[0] {
        Value::List(items) => {
            let positions: Vec<Value> = items.iter().enumerate()
                .filter(|(_, item)| item.struct_eq(&args[1]))
                .map(|(i, _)| Value::Integer(Integer::from(i as i64 + 1)))
                .collect();
            Ok(Value::List(positions))
        }
        _ => Err(EvalError::TypeError { expected: "List".to_string(), got: args[0].type_name().to_string() }),
    }
}

fn builtin_union(args: &[Value]) -> Result<Value, EvalError> {
    let mut seen = Vec::new();
    for arg in args {
        match arg {
            Value::List(items) => {
                for item in items {
                    if !seen.iter().any(|s: &Value| s.struct_eq(item)) { seen.push(item.clone()); }
                }
            }
            _ => return Err(EvalError::TypeError { expected: "List".to_string(), got: arg.type_name().to_string() }),
        }
    }
    Ok(Value::List(seen))
}

fn builtin_intersection(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 { return Err(EvalError::Error("Intersection requires exactly 2 arguments".to_string())); }
    match (&args[0], &args[1]) {
        (Value::List(a), Value::List(b)) => Ok(Value::List(a.iter().filter(|item| b.iter().any(|bitem| bitem.struct_eq(item))).cloned().collect())),
        _ => Err(EvalError::TypeError { expected: "List".to_string(), got: format!("{} and {}", args[0].type_name(), args[1].type_name()) }),
    }
}

fn builtin_complement(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 { return Err(EvalError::Error("Complement requires exactly 2 arguments".to_string())); }
    match (&args[0], &args[1]) {
        (Value::List(a), Value::List(b)) => Ok(Value::List(a.iter().filter(|item| !b.iter().any(|bitem| bitem.struct_eq(item))).cloned().collect())),
        _ => Err(EvalError::TypeError { expected: "List".to_string(), got: format!("{} and {}", args[0].type_name(), args[1].type_name()) }),
    }
}

fn builtin_tally(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 { return Err(EvalError::Error("Tally requires exactly 1 argument".to_string())); }
    match &args[0] {
        Value::List(items) => {
            let mut counts: Vec<(Value, i64)> = Vec::new();
            for item in items {
                if let Some(entry) = counts.iter_mut().find(|(k, _)| k.struct_eq(item)) { entry.1 += 1; }
                else { counts.push((item.clone(), 1)); }
            }
            Ok(Value::List(counts.into_iter().map(|(val, count)| Value::List(vec![val, Value::Integer(Integer::from(count))])).collect()))
        }
        _ => Err(EvalError::TypeError { expected: "List".to_string(), got: args[0].type_name().to_string() }),
    }
}

fn builtin_pad_left(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 || args.len() > 3 { return Err(EvalError::Error("PadLeft requires 2 or 3 arguments".to_string())); }
    match &args[0] {
        Value::List(items) => {
            let n = args[1].to_integer().ok_or_else(|| EvalError::TypeError { expected: "Integer".to_string(), got: args[1].type_name().to_string() })? as usize;
            let pad_val = if args.len() == 3 { args[2].clone() } else { Value::Null };
            if n <= items.len() { Ok(Value::List(items[items.len() - n..].to_vec())) }
            else { let mut result: Vec<Value> = vec![pad_val; n - items.len()]; result.extend(items.iter().cloned()); Ok(Value::List(result)) }
        }
        _ => Err(EvalError::TypeError { expected: "List".to_string(), got: args[0].type_name().to_string() }),
    }
}

fn builtin_pad_right(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 || args.len() > 3 { return Err(EvalError::Error("PadRight requires 2 or 3 arguments".to_string())); }
    match &args[0] {
        Value::List(items) => {
            let n = args[1].to_integer().ok_or_else(|| EvalError::TypeError { expected: "Integer".to_string(), got: args[1].type_name().to_string() })? as usize;
            let pad_val = if args.len() == 3 { args[2].clone() } else { Value::Null };
            let mut result = items.clone();
            if n > items.len() { result.resize(n, pad_val); } else { result.truncate(n); }
            Ok(Value::List(result))
        }
        _ => Err(EvalError::TypeError { expected: "List".to_string(), got: args[0].type_name().to_string() }),
    }
}

// ── Random (simple PRNG, no external dependencies) ──

use std::cell::RefCell;

thread_local! {
    static RNG_STATE: RefCell<u64> = RefCell::new(1);
}

fn next_random() -> u64 {
    RNG_STATE.with(|state| {
        let mut s = state.borrow_mut();
        *s ^= *s << 13;
        *s ^= *s >> 7;
        *s ^= *s << 17;
        *s
    })
}

fn builtin_random_integer(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 { return Err(EvalError::Error("RandomInteger requires exactly 1 argument".to_string())); }
    match &args[0] {
        Value::Integer(n) if *n > 0 => {
            let n_i64 = n.to_i64().unwrap_or(1);
            let rand_val = (next_random() as i64).rem_euclid(n_i64);
            Ok(Value::Integer(Integer::from(rand_val)))
        }
        Value::List(items) if items.len() == 2 => {
            let min = items[0].to_integer().ok_or_else(|| EvalError::TypeError { expected: "Integer".to_string(), got: items[0].type_name().to_string() })?;
            let max = items[1].to_integer().ok_or_else(|| EvalError::TypeError { expected: "Integer".to_string(), got: items[1].type_name().to_string() })?;
            if min > max { return Err(EvalError::Error("RandomInteger: min must be <= max".to_string())); }
            let rand_val = min + (next_random() as i64).rem_euclid(max - min + 1);
            Ok(Value::Integer(Integer::from(rand_val)))
        }
        _ => Err(EvalError::TypeError { expected: "Integer or {min, max}".to_string(), got: args[0].type_name().to_string() }),
    }
}

fn builtin_random_real(args: &[Value]) -> Result<Value, EvalError> {
    match args.len() {
        0 => {
            let r = (next_random() as f64) / (u64::MAX as f64);
            Ok(Value::Real(Float::with_val(DEFAULT_PRECISION, r)))
        }
        1 => match &args[0] {
            Value::List(items) if items.len() == 2 => {
                let min = items[0].to_real().ok_or_else(|| EvalError::TypeError { expected: "Number".to_string(), got: items[0].type_name().to_string() })?;
                let max = items[1].to_real().ok_or_else(|| EvalError::TypeError { expected: "Number".to_string(), got: items[1].type_name().to_string() })?;
                let r = (next_random() as f64) / (u64::MAX as f64);
                let result = min + r * (max - min);
                Ok(Value::Real(Float::with_val(DEFAULT_PRECISION, result)))
            }
            _ => Err(EvalError::TypeError { expected: "{min, max}".to_string(), got: args[0].type_name().to_string() }),
        }
        _ => Err(EvalError::Error("RandomReal requires 0 or 1 arguments".to_string())),
    }
}

fn builtin_random_choice(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 { return Err(EvalError::Error("RandomChoice requires exactly 1 argument".to_string())); }
    match &args[0] {
        Value::List(items) if !items.is_empty() => Ok(items[(next_random() as usize) % items.len()].clone()),
        Value::List(_) => Err(EvalError::Error("RandomChoice on empty list".to_string())),
        _ => Err(EvalError::TypeError { expected: "List".to_string(), got: args[0].type_name().to_string() }),
    }
}

// ── Association ──

fn builtin_lookup(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 || args.len() > 3 { return Err(EvalError::Error("Lookup requires 2 or 3 arguments".to_string())); }
    match &args[0] {
        Value::Assoc(map) => {
            let key = match &args[1] { Value::Str(s) => s.clone(), _ => return Err(EvalError::TypeError { expected: "String".to_string(), got: args[1].type_name().to_string() }) };
            match map.get(&key) {
                Some(val) => Ok(val.clone()),
                None => if args.len() == 3 { Ok(args[2].clone()) } else { Ok(Value::Null) },
            }
        }
        _ => Err(EvalError::TypeError { expected: "Assoc".to_string(), got: args[0].type_name().to_string() }),
    }
}

fn builtin_key_exists_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 { return Err(EvalError::Error("KeyExistsQ requires exactly 2 arguments".to_string())); }
    match &args[0] {
        Value::Assoc(map) => {
            let key = match &args[1] { Value::Str(s) => s.clone(), _ => return Err(EvalError::TypeError { expected: "String".to_string(), got: args[1].type_name().to_string() }) };
            Ok(Value::Bool(map.contains_key(&key)))
        }
        _ => Err(EvalError::TypeError { expected: "Assoc".to_string(), got: args[0].type_name().to_string() }),
    }
}

// ── I/O ──

fn builtin_input(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() > 1 { return Err(EvalError::Error("Input requires 0 or 1 arguments".to_string())); }
    if args.len() == 1 {
        if let Value::Str(prompt) = &args[0] { eprint!("{}", prompt); }
    }
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).map_err(|e| EvalError::Error(format!("Input error: {}", e)))?;
    Ok(Value::Str(input.trim().to_string()))
}

// ── ToExpression ──

fn builtin_to_expression(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 { return Err(EvalError::Error("ToExpression requires exactly 1 argument".to_string())); }
    let s = match &args[0] { Value::Str(s) => s, _ => return Err(EvalError::TypeError { expected: "String".to_string(), got: args[0].type_name().to_string() }) };
    let tokens = crate::lexer::tokenize(s).map_err(|e| EvalError::Error(format!("ToExpression parse error: {}", e)))?;
    let ast = crate::parser::parse(tokens).map_err(|e| EvalError::Error(format!("ToExpression parse error: {}", e)))?;
    let env = crate::env::Env::new();
    crate::builtins::register_builtins(&env);
    crate::eval::eval_program(&ast, &env)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn int(n: i64) -> Value { Value::Integer(Integer::from(n)) }
    fn real(r: f64) -> Value { Value::Real(Float::with_val(DEFAULT_PRECISION, r)) }
    fn list(vals: Vec<Value>) -> Value { Value::List(vals) }
    fn boolean(b: bool) -> Value { Value::Bool(b) }
    fn string(s: &str) -> Value { Value::Str(s.to_string()) }

    // ── Arithmetic ──

    #[test]
    fn test_plus_integers() {
        let result = builtin_plus(&[int(1), int(2)]).unwrap();
        assert_eq!(result, int(3));
    }

    #[test]
    fn test_plus_reals() {
        let result = builtin_plus(&[real(1.5), real(2.5)]).unwrap();
        assert_eq!(result, real(4.0));
    }

    #[test]
    fn test_plus_mixed() {
        let result = builtin_plus(&[int(1), real(2.5)]).unwrap();
        assert_eq!(result, real(3.5));
    }

    #[test]
    fn test_plus_multiple_args() {
        let result = builtin_plus(&[int(1), int(2), int(3)]).unwrap();
        assert_eq!(result, int(6));
    }

    #[test]
    fn test_plus_lists() {
        // add_values handles list+list directly
        let result = add_values(
            &list(vec![int(1), int(2)]),
            &list(vec![int(3), int(4)]),
        ).unwrap();
        assert_eq!(result, list(vec![int(4), int(6)]));
    }

    #[test]
    fn test_times_integers() {
        let result = builtin_times(&[int(3), int(4)]).unwrap();
        assert_eq!(result, int(12));
    }

    #[test]
    fn test_times_scalar_list() {
        let result = builtin_times(&[int(2), list(vec![int(1), int(2), int(3)])]).unwrap();
        assert_eq!(result, list(vec![int(2), int(4), int(6)]));
    }

    #[test]
    fn test_power() {
        let result = builtin_power(&[int(2), int(3)]).unwrap();
        assert_eq!(result, int(8));
    }

    #[test]
    fn test_power_negative_exp() {
        let result = builtin_power(&[int(2), int(-1)]).unwrap();
        assert_eq!(result, real(0.5));
    }

    #[test]
    fn test_divide() {
        let result = builtin_divide(&[int(6), int(2)]).unwrap();
        assert_eq!(result, int(3));
    }

    #[test]
    fn test_divide_non_exact() {
        let result = builtin_divide(&[int(5), int(2)]).unwrap();
        assert_eq!(result, real(2.5));
    }

    #[test]
    fn test_divide_by_zero() {
        let result = builtin_divide(&[int(1), int(0)]);
        assert!(matches!(result, Err(EvalError::DivisionByZero)));
    }

    #[test]
    fn test_minus_negation() {
        let result = builtin_minus(&[int(5)]).unwrap();
        assert_eq!(result, int(-5));
    }

    #[test]
    fn test_minus_subtraction() {
        let result = builtin_minus(&[int(10), int(3)]).unwrap();
        assert_eq!(result, int(7));
    }

    #[test]
    fn test_abs_integer() {
        assert_eq!(builtin_abs(&[int(-5)]).unwrap(), int(5));
        assert_eq!(builtin_abs(&[int(5)]).unwrap(), int(5));
    }

    #[test]
    fn test_abs_real() {
        assert_eq!(builtin_abs(&[real(-3.14)]).unwrap(), real(3.14));
    }

    // ── Comparison ──

    #[test]
    fn test_equal() {
        assert_eq!(builtin_equal(&[int(1), int(1)]).unwrap(), boolean(true));
        assert_eq!(builtin_equal(&[int(1), int(2)]).unwrap(), boolean(false));
    }

    #[test]
    fn test_unequal() {
        assert_eq!(builtin_unequal(&[int(1), int(2)]).unwrap(), boolean(true));
        assert_eq!(builtin_unequal(&[int(1), int(1)]).unwrap(), boolean(false));
    }

    #[test]
    fn test_less() {
        assert_eq!(builtin_less(&[int(1), int(2)]).unwrap(), boolean(true));
        assert_eq!(builtin_less(&[int(2), int(1)]).unwrap(), boolean(false));
    }

    #[test]
    fn test_greater() {
        assert_eq!(builtin_greater(&[int(2), int(1)]).unwrap(), boolean(true));
        assert_eq!(builtin_greater(&[int(1), int(2)]).unwrap(), boolean(false));
    }

    #[test]
    fn test_less_equal() {
        assert_eq!(builtin_less_equal(&[int(1), int(1)]).unwrap(), boolean(true));
        assert_eq!(builtin_less_equal(&[int(1), int(2)]).unwrap(), boolean(true));
        assert_eq!(builtin_less_equal(&[int(2), int(1)]).unwrap(), boolean(false));
    }

    #[test]
    fn test_greater_equal() {
        assert_eq!(builtin_greater_equal(&[int(1), int(1)]).unwrap(), boolean(true));
        assert_eq!(builtin_greater_equal(&[int(2), int(1)]).unwrap(), boolean(true));
        assert_eq!(builtin_greater_equal(&[int(1), int(2)]).unwrap(), boolean(false));
    }

    #[test]
    fn test_less_strings() {
        assert_eq!(builtin_less(&[string("a"), string("b")]).unwrap(), boolean(true));
    }

    // ── Logical ──

    #[test]
    fn test_and() {
        assert_eq!(builtin_and(&[boolean(true), boolean(true)]).unwrap(), boolean(true));
        assert_eq!(builtin_and(&[boolean(true), boolean(false)]).unwrap(), boolean(false));
    }

    #[test]
    fn test_or() {
        assert_eq!(builtin_or(&[boolean(false), boolean(true)]).unwrap(), boolean(true));
        assert_eq!(builtin_or(&[boolean(false), boolean(false)]).unwrap(), boolean(false));
    }

    #[test]
    fn test_not() {
        assert_eq!(builtin_not(&[boolean(true)]).unwrap(), boolean(false));
        assert_eq!(builtin_not(&[boolean(false)]).unwrap(), boolean(true));
    }

    // ── List ──

    #[test]
    fn test_length() {
        assert_eq!(builtin_length(&[list(vec![int(1), int(2), int(3)])]).unwrap(), int(3));
        assert_eq!(builtin_length(&[list(vec![])]).unwrap(), int(0));
    }

    #[test]
    fn test_first() {
        assert_eq!(builtin_first(&[list(vec![int(1), int(2)])]).unwrap(), int(1));
    }

    #[test]
    fn test_first_empty() {
        assert!(builtin_first(&[list(vec![])]).is_err());
    }

    #[test]
    fn test_last() {
        assert_eq!(builtin_last(&[list(vec![int(1), int(2), int(3)])]).unwrap(), int(3));
    }

    #[test]
    fn test_rest() {
        let result = builtin_rest(&[list(vec![int(1), int(2), int(3)])]).unwrap();
        assert_eq!(result, list(vec![int(2), int(3)]));
    }

    #[test]
    fn test_most() {
        let result = builtin_most(&[list(vec![int(1), int(2), int(3)])]).unwrap();
        assert_eq!(result, list(vec![int(1), int(2)]));
    }

    #[test]
    fn test_append() {
        let result = builtin_append(&[list(vec![int(1), int(2)]), int(3)]).unwrap();
        assert_eq!(result, list(vec![int(1), int(2), int(3)]));
    }

    #[test]
    fn test_prepend() {
        let result = builtin_prepend(&[list(vec![int(2), int(3)]), int(1)]).unwrap();
        assert_eq!(result, list(vec![int(1), int(2), int(3)]));
    }

    #[test]
    fn test_join() {
        let result = builtin_join(&[
            list(vec![int(1), int(2)]),
            list(vec![int(3), int(4)]),
        ]).unwrap();
        assert_eq!(result, list(vec![int(1), int(2), int(3), int(4)]));
    }

    #[test]
    fn test_flatten() {
        let nested = list(vec![
            list(vec![int(1), int(2)]),
            list(vec![int(3), list(vec![int(4)])]),
        ]);
        let result = builtin_flatten(&[nested]).unwrap();
        assert_eq!(result, list(vec![int(1), int(2), int(3), int(4)]));
    }

    #[test]
    fn test_sort() {
        let result = builtin_sort(&[list(vec![int(3), int(1), int(2)])]).unwrap();
        assert_eq!(result, list(vec![int(1), int(2), int(3)]));
    }

    #[test]
    fn test_reverse() {
        let result = builtin_reverse(&[list(vec![int(1), int(2), int(3)])]).unwrap();
        assert_eq!(result, list(vec![int(3), int(2), int(1)]));
    }

    #[test]
    fn test_part_positive_index() {
        assert_eq!(builtin_part(&[list(vec![int(10), int(20), int(30)]), int(1)]).unwrap(), int(10));
        assert_eq!(builtin_part(&[list(vec![int(10), int(20), int(30)]), int(3)]).unwrap(), int(30));
    }

    #[test]
    fn test_part_negative_index() {
        assert_eq!(builtin_part(&[list(vec![int(10), int(20), int(30)]), int(-1)]).unwrap(), int(30));
    }

    #[test]
    fn test_part_out_of_bounds() {
        assert!(builtin_part(&[list(vec![int(1)]), int(5)]).is_err());
    }

    #[test]
    fn test_range_one_arg() {
        let result = builtin_range(&[int(3)]).unwrap();
        assert_eq!(result, list(vec![int(1), int(2), int(3)]));
    }

    #[test]
    fn test_range_two_args() {
        let result = builtin_range(&[int(2), int(5)]).unwrap();
        assert_eq!(result, list(vec![int(2), int(3), int(4), int(5)]));
    }

    #[test]
    fn test_range_three_args() {
        let result = builtin_range(&[int(0), int(2), int(10)]).unwrap();
        assert_eq!(result, list(vec![int(0), int(2), int(4), int(6), int(8), int(10)]));
    }

    #[test]
    fn test_take() {
        let result = builtin_take(&[list(vec![int(1), int(2), int(3), int(4)]), int(2)]).unwrap();
        assert_eq!(result, list(vec![int(1), int(2)]));
    }

    #[test]
    fn test_drop() {
        let result = builtin_drop(&[list(vec![int(1), int(2), int(3), int(4)]), int(2)]).unwrap();
        assert_eq!(result, list(vec![int(3), int(4)]));
    }

    #[test]
    fn test_total() {
        let result = builtin_total(&[list(vec![int(1), int(2), int(3)])]).unwrap();
        assert_eq!(result, int(6));
    }

    // ── Pattern ──

    #[test]
    fn test_head() {
        assert_eq!(builtin_head(&[int(42)]).unwrap(), Value::Symbol("Integer".to_string()));
        assert_eq!(builtin_head(&[list(vec![])]).unwrap(), Value::Symbol("List".to_string()));
    }

    #[test]
    fn test_type_of() {
        assert_eq!(builtin_type_of(&[string("hello")]).unwrap(), Value::Symbol("String".to_string()));
        assert_eq!(builtin_type_of(&[boolean(true)]).unwrap(), Value::Symbol("Boolean".to_string()));
    }

    // ── String ──

    #[test]
    fn test_string_join() {
        let result = builtin_string_join(&[string("hello"), string(" world")]).unwrap();
        assert_eq!(result, string("hello world"));
    }

    #[test]
    fn test_string_length() {
        assert_eq!(builtin_string_length(&[string("hello")]).unwrap(), int(5));
        assert_eq!(builtin_string_length(&[string("")]).unwrap(), int(0));
    }

    #[test]
    fn test_to_string() {
        assert_eq!(builtin_to_string(&[int(42)]).unwrap(), string("42"));
        assert_eq!(builtin_to_string(&[boolean(true)]).unwrap(), string("True"));
    }

    // ── Math ──

    #[test]
    fn test_sin() {
        let result = builtin_sin(&[real(0.0)]).unwrap();
        if let Value::Real(r) = result {
            assert!(r.to_f64().abs() < f64::EPSILON);
        } else {
            panic!("Expected Real");
        }
    }

    #[test]
    fn test_cos() {
        let result = builtin_cos(&[real(0.0)]).unwrap();
        if let Value::Real(r) = result {
            assert!((r.to_f64() - 1.0).abs() < f64::EPSILON);
        } else {
            panic!("Expected Real");
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
}
