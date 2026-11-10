//! symbolic algebra: ExpandAll, PowerExpand, TrigExpand/Reduce/Factor, TrigToExp, ExpToTrig, ComplexExpand, FactorTerms.
//!
//! Gap-fill module; see the individual builtin docs for semantics.

use crate::env::Env;
use crate::value::{DEFAULT_PRECISION, EvalError, Value};
use rug::Float;
use rug::Integer;
use rug::Rational;

use super::symbolic::{builtin_expand, simplify_call};

/// Register the builtins implemented in this module.
pub fn register(env: &Env) {
    super::register_builtin(env, "ExpandAll", builtin_expand_all);
    super::register_builtin(env, "PowerExpand", builtin_power_expand);
    super::register_builtin(env, "ExpandLog", builtin_expand_log);
    super::register_builtin(env, "TrigExpand", builtin_trig_expand);
    super::register_builtin(env, "TrigReduce", builtin_trig_reduce);
    super::register_builtin(env, "TrigFactor", builtin_trig_factor);
    super::register_builtin(env, "TrigToExp", builtin_trig_to_exp);
    super::register_builtin(env, "ExpToTrig", builtin_exp_to_trig);
    super::register_builtin(env, "ComplexExpand", builtin_complex_expand);
    super::register_builtin(env, "FactorTerms", builtin_factor_terms);
}

// ── Small constructors / predicates ──

fn call(head: &str, args: Vec<Value>) -> Value {
    Value::Call {
        head: head.to_string(),
        args,
    }
}

fn call_ref(head: &str, args: &[Value]) -> Value {
    Value::Call {
        head: head.to_string(),
        args: args.to_vec(),
    }
}

fn int(n: i64) -> Value {
    Value::Integer(Integer::from(n))
}

fn sym(s: &str) -> Value {
    Value::Symbol(s.to_string())
}

/// The exact rational 1/2.
fn half() -> Value {
    Value::Rational(Box::new(Rational::from((1, 2))))
}

/// The imaginary unit as a numeric literal.
fn i_unit() -> Value {
    Value::Complex { re: 0.0, im: 1.0 }
}

fn real_lit(f: f64) -> Value {
    Value::Real(Float::with_val(DEFAULT_PRECISION, f))
}

fn is_zero(v: &Value) -> bool {
    match v {
        Value::Integer(n) => n.is_zero(),
        Value::Real(r) => r.is_zero(),
        Value::Rational(r) => r.is_zero(),
        Value::Complex { re, im } => *re == 0.0 && *im == 0.0,
        _ => false,
    }
}

fn is_one(v: &Value) -> bool {
    match v {
        Value::Integer(n) => *n == 1,
        Value::Real(r) => *r == Float::with_val(DEFAULT_PRECISION, 1.0),
        Value::Rational(r) => **r == 1,
        _ => false,
    }
}

fn expr_contains_var(expr: &Value, var: &str) -> bool {
    match expr {
        Value::Symbol(s) => s == var,
        Value::Call { args, .. } => args.iter().any(|a| expr_contains_var(a, var)),
        Value::List(items) => items.iter().any(|a| expr_contains_var(a, var)),
        _ => false,
    }
}

thread_local! {
    static DEEP_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

fn guard_depth<F: FnOnce() -> Value>(f: F) -> Value {
    let d = DEEP_DEPTH.get();
    if d > 256 {
        return Value::Null;
    }
    DEEP_DEPTH.set(d + 1);
    let r = f();
    DEEP_DEPTH.set(d);
    r
}

// ── ExpandAll ──

/// `ExpandAll[expr]` — expand recursively through all levels, distributing
/// products over sums and expanding denominators/nested powers.
pub fn builtin_expand_all(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "ExpandAll requires exactly 1 argument".to_string(),
        ));
    }
    Ok(expand_all(&args[0]))
}

fn expand_all(val: &Value) -> Value {
    guard_depth(|| match val {
        Value::List(items) => Value::List(items.iter().map(expand_all).collect()),
        Value::Call { head, args } => {
            let eargs: Vec<Value> = args.iter().map(expand_all).collect();
            match head.as_str() {
                "Times" => {
                    let mut it = eargs.into_iter();
                    let mut acc = it.next().unwrap_or_else(|| int(1));
                    for next in it {
                        acc = expand_all_times(&acc, &next);
                    }
                    acc
                }
                "Power" => expand_all_power(&eargs),
                "Divide" if eargs.len() == 2 => {
                    let num = &eargs[0];
                    let den = &eargs[1];
                    // Distribute a sum numerator over the denominator too.
                    if let Value::Call { head: nh, args: na } = num
                        && nh == "Plus"
                    {
                        let terms: Vec<Value> = na
                            .iter()
                            .map(|t| simplify_call("Divide", &[t.clone(), den.clone()]))
                            .collect();
                        return simplify_call("Plus", &terms);
                    }
                    simplify_call("Divide", &eargs)
                }
                _ => call(head, eargs),
            }
        }
        _ => val.clone(),
    })
}

fn expand_all_times(a: &Value, b: &Value) -> Value {
    if let Value::Call { head, args } = b
        && head == "Plus"
    {
        let terms: Vec<Value> = args
            .iter()
            .map(|t| expand_all(&simplify_call("Times", &[a.clone(), t.clone()])))
            .collect();
        return simplify_call("Plus", &terms);
    }
    if let Value::Call { head, args } = a
        && head == "Plus"
    {
        let terms: Vec<Value> = args
            .iter()
            .map(|t| expand_all(&simplify_call("Times", &[t.clone(), b.clone()])))
            .collect();
        return simplify_call("Plus", &terms);
    }
    simplify_call("Times", &[a.clone(), b.clone()])
}

fn expand_all_power(args: &[Value]) -> Value {
    if args.len() != 2 {
        return call_ref("Power", args);
    }
    if let Value::Integer(n) = &args[1]
        && let Some(ni) = n.to_i64()
        && (0..=12).contains(&ni)
        && matches!(&args[0], Value::Call { head, .. } if head == "Plus")
    {
        let base = &args[0];
        let mut acc = int(1);
        for _ in 0..ni {
            acc = expand_all_times(&acc, base);
        }
        return acc;
    }
    call_ref("Power", args)
}

// ── PowerExpand ──

/// `PowerExpand[expr]` — formally apply `(a b)^c → a^c b^c`,
/// `(a^b)^c → a^(b c)`, `Log[a^b] → b Log[a]`, `Log[a b] → Log[a] + Log[b]`.
pub fn builtin_power_expand(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "PowerExpand requires exactly 1 argument".to_string(),
        ));
    }
    Ok(power_expand(&args[0]))
}

fn power_expand(val: &Value) -> Value {
    guard_depth(|| match val {
        Value::List(items) => Value::List(items.iter().map(power_expand).collect()),
        Value::Call { head, args } => {
            let eargs: Vec<Value> = args.iter().map(power_expand).collect();
            match head.as_str() {
                "Power" if eargs.len() == 2 => match pe_power(&eargs[0], &eargs[1]) {
                    Some(v) => power_expand(&v),
                    None => call("Power", eargs),
                },
                "Sqrt" if eargs.len() == 1 => match pe_power(&eargs[0], &half()) {
                    Some(v) => power_expand(&v),
                    None => call("Sqrt", eargs),
                },
                "Log" if eargs.len() == 1 => match pe_log(&eargs[0]) {
                    Some(v) => v,
                    None => call("Log", eargs),
                },
                "Times" => simplify_call("Times", &eargs),
                "Plus" => simplify_call("Plus", &eargs),
                _ => call(head, eargs),
            }
        }
        _ => val.clone(),
    })
}

/// Formal power expansion. Returns `Some` when a rule fired.
fn pe_power(base: &Value, exp: &Value) -> Option<Value> {
    if is_one(exp) {
        return Some(base.clone());
    }
    if let Value::Call { head, args } = base {
        match head.as_str() {
            "Times" => {
                let factors: Vec<Value> = args
                    .iter()
                    .map(|f| power_expand(&call("Power", vec![f.clone(), exp.clone()])))
                    .collect();
                return Some(simplify_call("Times", &factors));
            }
            "Power" if args.len() == 2 => {
                let new_exp = simplify_call("Times", &[args[1].clone(), exp.clone()]);
                return Some(call("Power", vec![args[0].clone(), new_exp]));
            }
            "Abs" if args.len() == 1 => {
                if let Value::Call {
                    head: inner,
                    args: ia,
                } = &args[0]
                    && inner == "Times"
                {
                    let factors: Vec<Value> = ia
                        .iter()
                        .map(|f| {
                            power_expand(&call(
                                "Power",
                                vec![call("Abs", vec![f.clone()]), exp.clone()],
                            ))
                        })
                        .collect();
                    return Some(simplify_call("Times", &factors));
                }
            }
            _ => {}
        }
    }
    None
}

fn pe_log(arg: &Value) -> Option<Value> {
    match arg {
        Value::Call { head, args } if head == "Times" => {
            let terms: Vec<Value> = args
                .iter()
                .map(|f| power_expand(&call("Log", vec![f.clone()])))
                .collect();
            Some(call("Plus", terms))
        }
        Value::Call { head, args } if head == "Divide" && args.len() == 2 => Some(simplify_call(
            "Plus",
            &[
                power_expand(&call("Log", vec![args[0].clone()])),
                simplify_call(
                    "Times",
                    &[int(-1), power_expand(&call("Log", vec![args[1].clone()]))],
                ),
            ],
        )),
        Value::Call { head, args } if head == "Power" && args.len() == 2 => Some(call(
            "Times",
            vec![
                args[1].clone(),
                power_expand(&call("Log", vec![args[0].clone()])),
            ],
        )),
        Value::Call { head, args } if head == "Sqrt" && args.len() == 1 => Some(call(
            "Times",
            vec![half(), power_expand(&call("Log", vec![args[0].clone()]))],
        )),
        Value::Call { head, args } if head == "Abs" && args.len() == 1 => {
            if let Value::Call {
                head: inner,
                args: ia,
            } = &args[0]
                && inner == "Times"
            {
                let terms: Vec<Value> = ia
                    .iter()
                    .map(|f| power_expand(&call("Log", vec![call("Abs", vec![f.clone()])])))
                    .collect();
                Some(call("Plus", terms))
            } else {
                None
            }
        }
        _ => None,
    }
}

// ── ExpandLog ──

/// `ExpandLog[expr]` — expand logarithms of products, quotients and powers.
pub fn builtin_expand_log(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "ExpandLog requires exactly 1 argument".to_string(),
        ));
    }
    let expanded = expand_log_val(&args[0]);
    builtin_expand(&[expanded])
}

fn expand_log_val(val: &Value) -> Value {
    guard_depth(|| match val {
        Value::List(items) => Value::List(items.iter().map(expand_log_val).collect()),
        Value::Call { head, args } => {
            let eargs: Vec<Value> = args.iter().map(expand_log_val).collect();
            match head.as_str() {
                "Log" if eargs.len() == 1 => el_log(&eargs[0]),
                "Plus" => simplify_call("Plus", &eargs),
                "Times" => simplify_call("Times", &eargs),
                "Power" => simplify_call("Power", &eargs),
                _ => call(head, eargs),
            }
        }
        _ => val.clone(),
    })
}

fn el_log(arg: &Value) -> Value {
    match arg {
        Value::Call { head, args } if head == "Times" => {
            let terms: Vec<Value> = args
                .iter()
                .map(|f| expand_log_val(&call("Log", vec![f.clone()])))
                .collect();
            call("Plus", terms)
        }
        Value::Call { head, args } if head == "Divide" && args.len() == 2 => simplify_call(
            "Plus",
            &[
                expand_log_val(&call("Log", vec![args[0].clone()])),
                simplify_call(
                    "Times",
                    &[int(-1), expand_log_val(&call("Log", vec![args[1].clone()]))],
                ),
            ],
        ),
        Value::Call { head, args } if head == "Power" && args.len() == 2 => call(
            "Times",
            vec![
                args[1].clone(),
                expand_log_val(&call("Log", vec![args[0].clone()])),
            ],
        ),
        _ => call("Log", vec![arg.clone()]),
    }
}

// ── TrigExpand ──

/// `TrigExpand[expr]` — expand the arguments of trigonometric functions using
/// sum and multiple-angle identities.
pub fn builtin_trig_expand(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "TrigExpand requires exactly 1 argument".to_string(),
        ));
    }
    Ok(trig_expand(&args[0]))
}

fn trig_expand(val: &Value) -> Value {
    guard_depth(|| match val {
        Value::List(items) => Value::List(items.iter().map(trig_expand).collect()),
        Value::Call { head, args } => {
            let eargs: Vec<Value> = args.iter().map(trig_expand).collect();
            match head.as_str() {
                "Sin" if eargs.len() == 1 => te_pair("Sin", "Cos", &eargs[0]).0,
                "Cos" if eargs.len() == 1 => te_pair("Sin", "Cos", &eargs[0]).1,
                "Tan" if eargs.len() == 1 => te_tan(&eargs[0]),
                "Sinh" if eargs.len() == 1 => te_pair("Sinh", "Cosh", &eargs[0]).0,
                "Cosh" if eargs.len() == 1 => te_pair("Sinh", "Cosh", &eargs[0]).1,
                "Plus" => simplify_call("Plus", &eargs),
                "Times" => simplify_call("Times", &eargs),
                "Power" => simplify_call("Power", &eargs),
                "Divide" if eargs.len() == 2 => simplify_call("Divide", &eargs),
                _ => call(head, eargs),
            }
        }
        _ => val.clone(),
    })
}

/// Return `(expanded head(arg), expanded cohead(arg))`.
fn te_pair(head: &str, cohead: &str, arg: &Value) -> (Value, Value) {
    match arg {
        Value::Call {
            head: ph,
            args: terms,
        } if ph == "Plus" && terms.len() >= 2 => {
            let (mut s, mut c) = (te_head(head, &terms[0]), te_head(cohead, &terms[0]));
            for t in &terms[1..] {
                let st = te_head(head, t);
                let ct = te_head(cohead, t);
                let ns = simplify_call(
                    "Plus",
                    &[
                        simplify_call("Times", &[s.clone(), ct.clone()]),
                        simplify_call("Times", &[c.clone(), st.clone()]),
                    ],
                );
                let nc = simplify_call(
                    "Plus",
                    &[
                        simplify_call("Times", &[c.clone(), ct]),
                        simplify_call("Times", &[int(-1), s.clone(), st]),
                    ],
                );
                s = ns;
                c = nc;
            }
            (s, c)
        }
        Value::Call { head: th, args: ta } if th == "Times" && ta.len() == 2 => {
            match &ta[0] {
                Value::Integer(n) if *n == 0 => (int(0), int(1)),
                Value::Integer(n) => {
                    let ni = n.to_i64().unwrap_or(0);
                    let u = &ta[1];
                    if ni == 1 {
                        (te_head(head, u), te_head(cohead, u))
                    } else if ni > 0 {
                        let b = call("Times", vec![int(ni - 1), u.clone()]);
                        let (sb, cb) = te_pair(head, cohead, &b);
                        let (su, cu) = (te_head(head, u), te_head(cohead, u));
                        let s = simplify_call(
                            "Plus",
                            &[
                                simplify_call("Times", &[sb.clone(), cu.clone()]),
                                simplify_call("Times", &[cb.clone(), su.clone()]),
                            ],
                        );
                        let c = simplify_call(
                            "Plus",
                            &[
                                simplify_call("Times", &[cb, cu]),
                                simplify_call("Times", &[int(-1), sb, su]),
                            ],
                        );
                        (s, c)
                    } else {
                        // Negative multiple: use parity.
                        let pos = call("Times", vec![int(-ni), u.clone()]);
                        let (s, c) = te_pair(head, cohead, &pos);
                        if ni.rem_euclid(2) == 1 {
                            (simplify_call("Times", &[int(-1), s]), c)
                        } else {
                            (s, c)
                        }
                    }
                }
                _ => (
                    call(head, vec![arg.clone()]),
                    call(cohead, vec![arg.clone()]),
                ),
            }
        }
        _ => (
            call(head, vec![arg.clone()]),
            call(cohead, vec![arg.clone()]),
        ),
    }
}

fn te_head(head: &str, arg: &Value) -> Value {
    if head == "Sin" {
        te_pair("Sin", "Cos", arg).0
    } else {
        te_pair("Sin", "Cos", arg).1
    }
}

fn te_tan(arg: &Value) -> Value {
    if let Value::Call { head, args } = arg
        && head == "Plus"
        && args.len() >= 2
    {
        // tan(a + b) = (tan a + tan b) / (1 - tan a tan b)
        let t0 = call("Tan", vec![args[0].clone()]);
        let t1 = call("Tan", vec![args[1].clone()]);
        let num = simplify_call("Plus", &[t0.clone(), t1.clone()]);
        let den = simplify_call(
            "Plus",
            &[int(1), simplify_call("Times", &[int(-1), t0, t1])],
        );
        return simplify_call("Divide", &[num, den]);
    }
    call("Tan", vec![arg.clone()])
}

// ── TrigReduce ──

/// `TrigReduce[expr]` — rewrite products and powers of trigonometric
/// functions as sums (product-to-sum identities).
pub fn builtin_trig_reduce(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "TrigReduce requires exactly 1 argument".to_string(),
        ));
    }
    Ok(trig_reduce(&args[0]))
}

fn trig_reduce(val: &Value) -> Value {
    guard_depth(|| match val {
        Value::List(items) => Value::List(items.iter().map(trig_reduce).collect()),
        Value::Call { head, args } => {
            let eargs: Vec<Value> = args.iter().map(trig_reduce).collect();
            match head.as_str() {
                "Times" => tr_reduce_times(&eargs),
                "Power" if eargs.len() == 2 => tr_reduce_power(&eargs[0], &eargs[1])
                    .unwrap_or_else(|| simplify_call("Power", &eargs)),
                "Plus" => simplify_call("Plus", &eargs),
                _ => call(head, eargs),
            }
        }
        _ => val.clone(),
    })
}

fn tr_reduce_times(args: &[Value]) -> Value {
    let mut coeff = int(1);
    let mut trigs: Vec<(String, Value)> = Vec::new();
    let mut others: Vec<Value> = Vec::new();
    for a in args {
        match a {
            Value::Integer(_) | Value::Real(_) | Value::Rational(_) => {
                coeff = simplify_call("Times", &[coeff.clone(), a.clone()]);
            }
            Value::Call { head, args: ta } if ta.len() == 1 && (head == "Sin" || head == "Cos") => {
                trigs.push((head.clone(), ta[0].clone()));
            }
            _ => others.push(a.clone()),
        }
    }
    if trigs.len() < 2 {
        return simplify_call("Times", args);
    }
    let (h1, a1) = trigs.remove(0);
    let (h2, a2) = trigs.remove(0);
    let combined = product_to_sum(&h1, &a1, &h2, &a2);
    let mut rest = vec![coeff];
    rest.extend(others);
    rest.extend(trigs.into_iter().map(|(h, a)| call(&h, vec![a])));
    if rest.len() == 1 {
        simplify_call("Times", &[rest.remove(0), combined])
    } else {
        simplify_call("Times", &[simplify_call("Times", &rest), combined])
    }
}

/// product-to-sum for two trig factors; returns a `Plus` scaled by 1/2.
fn product_to_sum(h1: &str, a1: &Value, h2: &str, a2: &Value) -> Value {
    let sum = simplify_call("Plus", &[a1.clone(), a2.clone()]);
    let diff = simplify_call(
        "Plus",
        &[a1.clone(), simplify_call("Times", &[int(-1), a2.clone()])],
    );
    match (h1, h2) {
        ("Sin", "Cos") => scale(
            vec![call("Sin", vec![sum]), call("Sin", vec![diff])],
            half(),
        ),
        ("Cos", "Sin") => scale(
            vec![call("Sin", vec![sum]), call("Sin", vec![diff])],
            half(),
        ),
        ("Sin", "Sin") => scale(
            vec![
                call("Cos", vec![diff]),
                simplify_call("Times", &[int(-1), call("Cos", vec![sum])]),
            ],
            half(),
        ),
        ("Cos", "Cos") => scale(
            vec![call("Cos", vec![diff]), call("Cos", vec![sum])],
            half(),
        ),
        _ => call(
            "Times",
            vec![call(h1, vec![a1.clone()]), call(h2, vec![a2.clone()])],
        ),
    }
}

fn scale(terms: Vec<Value>, coeff: Value) -> Value {
    let scaled: Vec<Value> = terms
        .into_iter()
        .map(|t| simplify_call("Times", &[coeff.clone(), t]))
        .collect();
    simplify_call("Plus", &scaled)
}

fn tr_reduce_power(base: &Value, exp: &Value) -> Option<Value> {
    let Value::Integer(n) = exp else { return None };
    let ni = n.to_i64()?;
    let Value::Call { head, args } = base else {
        return None;
    };
    if args.len() != 1 || (head != "Sin" && head != "Cos") {
        return None;
    }
    let x = args[0].clone();
    let double = call("Cos", vec![simplify_call("Times", &[int(2), x.clone()])]);
    let triple = call(head, vec![simplify_call("Times", &[int(3), x.clone()])]);
    let quad = call("Cos", vec![simplify_call("Times", &[int(4), x.clone()])]);
    let single = call(head, vec![x]);
    let odd = head == "Sin";
    match ni {
        2 => Some(scale(
            vec![
                int(1),
                simplify_call("Times", &[int(if odd { -1 } else { 1 }), double]),
            ],
            half(),
        )),
        3 => Some(scale(
            vec![
                simplify_call("Times", &[int(3), single]),
                simplify_call("Times", &[int(if odd { -1 } else { 1 }), triple]),
            ],
            Value::Rational(Box::new(Rational::from((1, 4)))),
        )),
        4 => Some(scale(
            vec![
                int(3),
                simplify_call("Times", &[int(if odd { -4 } else { 4 }), double]),
                quad,
            ],
            Value::Rational(Box::new(Rational::from((1, 8)))),
        )),
        _ => None,
    }
}

// ── TrigFactor ──

/// `TrigFactor[expr]` — combine sums of trigonometric functions into products.
pub fn builtin_trig_factor(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "TrigFactor requires exactly 1 argument".to_string(),
        ));
    }
    Ok(trig_factor(&args[0]))
}

fn trig_factor(val: &Value) -> Value {
    guard_depth(|| match val {
        Value::List(items) => Value::List(items.iter().map(trig_factor).collect()),
        Value::Call { head, args } => {
            let eargs: Vec<Value> = args.iter().map(trig_factor).collect();
            if head == "Plus"
                && eargs.len() == 2
                && let Some(v) = tf_two(&eargs[0], &eargs[1])
            {
                return v;
            }
            call(head, eargs)
        }
        _ => val.clone(),
    })
}

/// Parse a squared trig term `Sin[a]^2`/`Cos[a]^2` into `(sign, head, arg)`.
fn tf_sq(term: &Value) -> Option<(i64, String, Value)> {
    fn square_inner(v: &Value) -> Option<(String, Value)> {
        if let Value::Call { head, args } = v
            && head == "Power"
            && args.len() == 2
            && matches!(&args[1], Value::Integer(n) if *n == 2)
            && let Value::Call { head: th, args: ta } = &args[0]
            && ta.len() == 1
            && (th == "Sin" || th == "Cos")
        {
            return Some((th.clone(), ta[0].clone()));
        }
        None
    }
    if let Some((h, a)) = square_inner(term) {
        return Some((1, h, a));
    }
    if let Value::Call { head, args } = term
        && head == "Times"
    {
        let (mut sign, mut found) = (1i64, None);
        for f in args {
            match f {
                Value::Integer(n) if *n == -1 => sign = -sign,
                _ => match square_inner(f) {
                    Some((h, a)) => found = Some((h, a)),
                    None => return None,
                },
            }
        }
        return found.map(|(h, a)| (sign, h, a));
    }
    None
}

/// Parse a term into `(sign, head, arg)` for `Sin`/`Cos`.
fn tf_term(term: &Value) -> Option<(i64, String, Value)> {
    match term {
        Value::Call { head, args } if args.len() == 1 && (head == "Sin" || head == "Cos") => {
            Some((1, head.clone(), args[0].clone()))
        }
        Value::Call { head, args } if head == "Times" => {
            let (mut sign, mut found) = (1i64, None);
            for f in args {
                match f {
                    Value::Integer(n) if *n == -1 => sign = -sign,
                    Value::Call { head: th, args: ta }
                        if ta.len() == 1 && (th == "Sin" || th == "Cos") =>
                    {
                        found = Some((th.clone(), ta[0].clone()));
                    }
                    _ => return None,
                }
            }
            found.map(|(h, a)| (sign, h, a))
        }
        _ => None,
    }
}

fn tf_two(t0: &Value, t1: &Value) -> Option<Value> {
    // sin^2 ± cos^2 identities.
    if let (Some((s0, h0, a0)), Some((s1, h1, a1))) = (tf_sq(t0), tf_sq(t1))
        && h0 != h1
        && a0.struct_eq(&a1)
    {
        if s0 * s1 > 0 {
            return Some(int(1));
        }
        let two_a = simplify_call("Times", &[int(2), a0.clone()]);
        let c = call("Cos", vec![two_a]);
        return Some(if h0 == "Sin" {
            simplify_call("Times", &[int(-1), c])
        } else {
            c
        });
    }
    let (s0, h0, a0) = tf_term(t0)?;
    let (s1, h1, a1) = tf_term(t1)?;
    if h0 != h1 {
        return None;
    }
    let half_sum = simplify_call(
        "Times",
        &[half(), simplify_call("Plus", &[a0.clone(), a1.clone()])],
    );
    let half_diff = simplify_call(
        "Times",
        &[
            half(),
            simplify_call(
                "Plus",
                &[a0.clone(), simplify_call("Times", &[int(-1), a1.clone()])],
            ),
        ],
    );
    let sign = s0 * s1;
    match (h0.as_str(), sign) {
        ("Sin", 1) => Some(simplify_call(
            "Times",
            &[
                int(2),
                call("Sin", vec![half_sum]),
                call("Cos", vec![half_diff]),
            ],
        )),
        ("Sin", -1) => Some(simplify_call(
            "Times",
            &[
                int(2),
                call("Cos", vec![half_sum]),
                call("Sin", vec![half_diff]),
            ],
        )),
        ("Cos", 1) => Some(simplify_call(
            "Times",
            &[
                int(2),
                call("Cos", vec![half_sum]),
                call("Cos", vec![half_diff]),
            ],
        )),
        ("Cos", -1) => Some(simplify_call(
            "Times",
            &[
                int(-2),
                call("Sin", vec![half_sum]),
                call("Sin", vec![half_diff]),
            ],
        )),
        _ => None,
    }
}

// ── TrigToExp ──

/// `TrigToExp[expr]` — rewrite trigonometric functions in terms of exponentials.
pub fn builtin_trig_to_exp(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "TrigToExp requires exactly 1 argument".to_string(),
        ));
    }
    Ok(trig_to_exp(&args[0]))
}

fn exp_of_ix(x: &Value, sign: i64) -> Value {
    let inner = simplify_call("Times", &[int(sign), i_unit(), x.clone()]);
    call("Exp", vec![inner])
}

fn tte_sin(x: &Value) -> Value {
    simplify_call(
        "Divide",
        &[
            simplify_call(
                "Plus",
                &[
                    exp_of_ix(x, 1),
                    simplify_call("Times", &[int(-1), exp_of_ix(x, -1)]),
                ],
            ),
            simplify_call("Times", &[int(2), i_unit()]),
        ],
    )
}

fn tte_cos(x: &Value) -> Value {
    simplify_call(
        "Divide",
        &[
            simplify_call("Plus", &[exp_of_ix(x, 1), exp_of_ix(x, -1)]),
            int(2),
        ],
    )
}

fn tte_sinh(x: &Value) -> Value {
    simplify_call(
        "Divide",
        &[
            simplify_call(
                "Plus",
                &[
                    call("Exp", vec![x.clone()]),
                    simplify_call("Times", &[int(-1), call("Exp", vec![negate(x)])]),
                ],
            ),
            int(2),
        ],
    )
}

fn tte_cosh(x: &Value) -> Value {
    simplify_call(
        "Divide",
        &[
            simplify_call(
                "Plus",
                &[call("Exp", vec![x.clone()]), call("Exp", vec![negate(x)])],
            ),
            int(2),
        ],
    )
}

fn negate(x: &Value) -> Value {
    simplify_call("Times", &[int(-1), x.clone()])
}

fn trig_to_exp(val: &Value) -> Value {
    guard_depth(|| match val {
        Value::List(items) => Value::List(items.iter().map(trig_to_exp).collect()),
        Value::Call { head, args } => {
            let eargs: Vec<Value> = args.iter().map(trig_to_exp).collect();
            match head.as_str() {
                "Sin" if eargs.len() == 1 => tte_sin(&eargs[0]),
                "Cos" if eargs.len() == 1 => tte_cos(&eargs[0]),
                "Tan" if eargs.len() == 1 => {
                    simplify_call("Divide", &[tte_sin(&eargs[0]), tte_cos(&eargs[0])])
                }
                "Cot" if eargs.len() == 1 => {
                    simplify_call("Divide", &[tte_cos(&eargs[0]), tte_sin(&eargs[0])])
                }
                "Sec" if eargs.len() == 1 => simplify_call("Divide", &[int(1), tte_cos(&eargs[0])]),
                "Csc" if eargs.len() == 1 => simplify_call("Divide", &[int(1), tte_sin(&eargs[0])]),
                "Sinh" if eargs.len() == 1 => tte_sinh(&eargs[0]),
                "Cosh" if eargs.len() == 1 => tte_cosh(&eargs[0]),
                "Tanh" if eargs.len() == 1 => {
                    simplify_call("Divide", &[tte_sinh(&eargs[0]), tte_cosh(&eargs[0])])
                }
                "Plus" => simplify_call("Plus", &eargs),
                "Times" => simplify_call("Times", &eargs),
                "Power" => simplify_call("Power", &eargs),
                "Divide" if eargs.len() == 2 => simplify_call("Divide", &eargs),
                _ => call(head, eargs),
            }
        }
        _ => val.clone(),
    })
}

// ── ExpToTrig ──

/// `ExpToTrig[expr]` — rewrite exponentials in terms of trigonometric functions.
pub fn builtin_exp_to_trig(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "ExpToTrig requires exactly 1 argument".to_string(),
        ));
    }
    Ok(exp_to_trig(&args[0]))
}

fn exp_to_trig(val: &Value) -> Value {
    guard_depth(|| match val {
        Value::List(items) => Value::List(items.iter().map(exp_to_trig).collect()),
        Value::Call { head, args } => {
            let eargs: Vec<Value> = args.iter().map(exp_to_trig).collect();
            match head.as_str() {
                "Exp" if eargs.len() == 1 => e2t_exp(&eargs[0]),
                "Power" if eargs.len() == 2 && eargs[0].struct_eq(&sym("E")) => e2t_exp(&eargs[1]),
                "Plus" => simplify_call("Plus", &eargs),
                "Times" => simplify_call("Times", &eargs),
                "Divide" if eargs.len() == 2 => simplify_call("Divide", &eargs),
                _ => call(head, eargs),
            }
        }
        _ => val.clone(),
    })
}

fn e2t_exp(arg: &Value) -> Value {
    let (r, m) = split_re_im(arg);
    if is_zero(&m) {
        if is_zero(&r) {
            return int(1);
        }
        return simplify_call(
            "Plus",
            &[call("Cosh", vec![r.clone()]), call("Sinh", vec![r])],
        );
    }
    let unit = simplify_call(
        "Plus",
        &[
            call("Cos", vec![m.clone()]),
            simplify_call("Times", &[i_unit(), call("Sin", vec![m])]),
        ],
    );
    if is_zero(&r) {
        unit
    } else {
        let real_exp = simplify_call(
            "Plus",
            &[call("Cosh", vec![r.clone()]), call("Sinh", vec![r])],
        );
        simplify_call("Times", &[real_exp, unit])
    }
}

/// Split `arg` into `(real, imaginary)` assuming `arg = real + I*imag`.
fn split_re_im(arg: &Value) -> (Value, Value) {
    match arg {
        Value::Complex { re, im } => {
            if *im == 0.0 {
                (real_lit(*re), int(0))
            } else if *re == 0.0 {
                (int(0), real_lit(*im))
            } else {
                (real_lit(*re), real_lit(*im))
            }
        }
        Value::Symbol(s) if s == "I" => (int(0), int(1)),
        Value::Call { head, args } if head == "Plus" => {
            let mut real = int(0);
            let mut imag = int(0);
            for t in args {
                let (r, m) = split_re_im(t);
                real = simplify_call("Plus", &[real, r]);
                imag = simplify_call("Plus", &[imag, m]);
            }
            (real, imag)
        }
        Value::Call { head, args } if head == "Times" => {
            let mut real = int(1);
            let mut imag = int(0);
            for f in args {
                let (fr, fm) = split_re_im(f);
                let (nr, nm) = complex_mul(&real, &imag, &fr, &fm);
                real = nr;
                imag = nm;
            }
            (real, imag)
        }
        _ => (arg.clone(), int(0)),
    }
}

/// Complex multiplication `(a + I b)(c + I d)`.
fn complex_mul(a: &Value, b: &Value, c: &Value, d: &Value) -> (Value, Value) {
    let re = simplify_call(
        "Plus",
        &[
            simplify_call("Times", &[a.clone(), c.clone()]),
            simplify_call("Times", &[int(-1), b.clone(), d.clone()]),
        ],
    );
    let im = simplify_call(
        "Plus",
        &[
            simplify_call("Times", &[a.clone(), d.clone()]),
            simplify_call("Times", &[b.clone(), c.clone()]),
        ],
    );
    (re, im)
}

// ── ComplexExpand ──

/// `ComplexExpand[expr]` (and `ComplexExpand[expr, vars]`) — expand assuming
/// that all free symbols are real, splitting into real and imaginary parts.
pub fn builtin_complex_expand(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Err(EvalError::Error(
            "ComplexExpand requires 1 or 2 arguments".to_string(),
        ));
    }
    let (re, im) = re_im(&args[0]);
    if is_zero(&im) {
        return Ok(re);
    }
    Ok(simplify_call(
        "Plus",
        &[re, simplify_call("Times", &[i_unit(), im])],
    ))
}

/// Recursively decompose `val` into `(Re, Im)`, assuming symbols are real.
fn re_im(val: &Value) -> (Value, Value) {
    match val {
        Value::Complex { re, im } => (real_lit(*re), real_lit(*im)),
        Value::Symbol(s) if s == "I" => (int(0), int(1)),
        Value::Symbol(_) => (val.clone(), int(0)),
        Value::Integer(_) | Value::Real(_) | Value::Rational(_) => (val.clone(), int(0)),
        Value::Call { head, args } => match head.as_str() {
            "Plus" => {
                let mut re = int(0);
                let mut im = int(0);
                for a in args {
                    let (r, m) = re_im(a);
                    re = simplify_call("Plus", &[re, r]);
                    im = simplify_call("Plus", &[im, m]);
                }
                (re, im)
            }
            "Times" => {
                let mut re = int(1);
                let mut im = int(0);
                for a in args {
                    let (ar, am) = re_im(a);
                    let (nr, nm) = complex_mul(&re, &im, &ar, &am);
                    re = nr;
                    im = nm;
                }
                (re, im)
            }
            "Divide" if args.len() == 2 => {
                let (ar, am) = re_im(&args[0]);
                let (br, bm) = re_im(&args[1]);
                let denom = simplify_call(
                    "Plus",
                    &[
                        simplify_call("Power", &[br.clone(), int(2)]),
                        simplify_call("Power", &[bm.clone(), int(2)]),
                    ],
                );
                let num = complex_mul(&ar, &am, &br, &simplify_call("Times", &[int(-1), bm]));
                (
                    simplify_call("Divide", &[num.0, denom.clone()]),
                    simplify_call("Divide", &[num.1, denom]),
                )
            }
            "Power" if args.len() == 2 => re_im_power(&args[0], &args[1]),
            "Exp" if args.len() == 1 => re_im_exp(&args[0]),
            "Sin" if args.len() == 1 => {
                let (a, b) = re_im(&args[0]);
                (
                    simplify_call(
                        "Times",
                        &[call("Sin", vec![a.clone()]), call("Cosh", vec![b.clone()])],
                    ),
                    simplify_call("Times", &[call("Cos", vec![a]), call("Sinh", vec![b])]),
                )
            }
            "Cos" if args.len() == 1 => {
                let (a, b) = re_im(&args[0]);
                (
                    simplify_call(
                        "Times",
                        &[call("Cos", vec![a.clone()]), call("Cosh", vec![b.clone()])],
                    ),
                    simplify_call(
                        "Times",
                        &[int(-1), call("Sin", vec![a]), call("Sinh", vec![b])],
                    ),
                )
            }
            "Sinh" if args.len() == 1 => {
                let (a, b) = re_im(&args[0]);
                (
                    simplify_call(
                        "Times",
                        &[call("Sinh", vec![a.clone()]), call("Cos", vec![b.clone()])],
                    ),
                    simplify_call("Times", &[call("Cosh", vec![a]), call("Sin", vec![b])]),
                )
            }
            "Cosh" if args.len() == 1 => {
                let (a, b) = re_im(&args[0]);
                (
                    simplify_call(
                        "Times",
                        &[call("Cosh", vec![a.clone()]), call("Cos", vec![b.clone()])],
                    ),
                    simplify_call("Times", &[call("Sinh", vec![a]), call("Sin", vec![b])]),
                )
            }
            "Log" if args.len() == 1 => {
                let (a, b) = re_im(&args[0]);
                let modulus = simplify_call(
                    "Sqrt",
                    &[simplify_call(
                        "Plus",
                        &[
                            simplify_call("Power", &[a.clone(), int(2)]),
                            simplify_call("Power", &[b.clone(), int(2)]),
                        ],
                    )],
                );
                (call("Log", vec![modulus]), ce_arg_from(&a, &b, &args[0]))
            }
            "Re" if args.len() == 1 => (re_im(&args[0]).0, int(0)),
            "Im" if args.len() == 1 => (re_im(&args[0]).1, int(0)),
            "Abs" if args.len() == 1 => (ce_abs(&args[0]), int(0)),
            "Arg" if args.len() == 1 => (ce_arg(&args[0]), int(0)),
            "Conjugate" if args.len() == 1 => {
                let (a, b) = re_im(&args[0]);
                (a, simplify_call("Times", &[int(-1), b]))
            }
            _ => (val.clone(), int(0)),
        },
        _ => (val.clone(), int(0)),
    }
}

fn re_im_power(base: &Value, exp: &Value) -> (Value, Value) {
    if base.struct_eq(&sym("E")) {
        return re_im_exp(exp);
    }
    let (br, bm) = re_im(base);
    if let Value::Integer(n) = exp
        && let Some(ni) = n.to_i64()
    {
        if (0..=8).contains(&ni) {
            let (mut re, mut im) = (int(1), int(0));
            for _ in 0..ni {
                let (nr, nm) = complex_mul(&re, &im, &br, &bm);
                re = nr;
                im = nm;
            }
            return (re, im);
        }
        if ni < 0 {
            // base^ni = (base^|ni|)^-1, by binary exponentiation.
            let (mut re, mut im) = (int(1), int(0));
            let mut b = (br.clone(), bm.clone());
            let mut p = ni.unsigned_abs();
            while p > 0 {
                if p & 1 == 1 {
                    let (nr, nm) = complex_mul(&re, &im, &b.0, &b.1);
                    re = nr;
                    im = nm;
                }
                b = complex_mul(&b.0, &b.1, &b.0, &b.1);
                p >>= 1;
            }
            return complex_inverse(&re, &im);
        }
    }
    if matches!(exp, Value::Rational(r) if **r == Rational::from((1, 2))) && !is_zero(&bm) {
        return re_im_sqrt(&br, &bm);
    }
    if is_zero(&bm) {
        (val_or(base, exp), int(0))
    } else {
        (call("Power", vec![base.clone(), exp.clone()]), int(0))
    }
}

fn val_or(base: &Value, exp: &Value) -> Value {
    call("Power", vec![base.clone(), exp.clone()])
}

fn complex_inverse(a: &Value, b: &Value) -> (Value, Value) {
    let denom = simplify_call(
        "Plus",
        &[
            simplify_call("Power", &[a.clone(), int(2)]),
            simplify_call("Power", &[b.clone(), int(2)]),
        ],
    );
    (
        simplify_call("Divide", &[a.clone(), denom.clone()]),
        simplify_call(
            "Divide",
            &[simplify_call("Times", &[int(-1), b.clone()]), denom],
        ),
    )
}

fn re_im_sqrt(a: &Value, b: &Value) -> (Value, Value) {
    let modulus = simplify_call(
        "Sqrt",
        &[simplify_call(
            "Plus",
            &[
                simplify_call("Power", &[a.clone(), int(2)]),
                simplify_call("Power", &[b.clone(), int(2)]),
            ],
        )],
    );
    let re = simplify_call(
        "Sqrt",
        &[simplify_call(
            "Times",
            &[half(), simplify_call("Plus", &[modulus.clone(), a.clone()])],
        )],
    );
    let im = simplify_call(
        "Sqrt",
        &[simplify_call(
            "Times",
            &[
                half(),
                simplify_call(
                    "Plus",
                    &[modulus, simplify_call("Times", &[int(-1), a.clone()])],
                ),
            ],
        )],
    );
    let im2 = if let Value::Integer(n) = b {
        if n.is_negative() {
            simplify_call("Times", &[int(-1), im])
        } else {
            im
        }
    } else {
        im
    };
    (re, im2)
}

fn re_im_exp(arg: &Value) -> (Value, Value) {
    let (r, m) = split_re_im(arg);
    if is_zero(&m) {
        return (call("Exp", vec![r]), int(0));
    }
    let unit = (call("Cos", vec![m.clone()]), call("Sin", vec![m]));
    if is_zero(&r) {
        return unit;
    }
    let er = call("Exp", vec![r]);
    (
        simplify_call("Times", &[er.clone(), unit.0]),
        simplify_call("Times", &[er, unit.1]),
    )
}

fn ce_abs(v: &Value) -> Value {
    let (a, b) = re_im(v);
    if is_zero(&b) {
        return call("Abs", vec![a]);
    }
    simplify_call(
        "Sqrt",
        &[simplify_call(
            "Plus",
            &[
                simplify_call("Power", &[a, int(2)]),
                simplify_call("Power", &[b, int(2)]),
            ],
        )],
    )
}

fn ce_arg(v: &Value) -> Value {
    let (a, b) = re_im(v);
    ce_arg_from(&a, &b, v)
}

fn ce_arg_from(a: &Value, b: &Value, original: &Value) -> Value {
    if is_zero(b) {
        return call("Arg", vec![original.clone()]);
    }
    call("ArcTan", vec![b.clone(), a.clone()])
}

// ── FactorTerms ──

/// `FactorTerms[expr]` / `FactorTerms[expr, x]` — pull the overall content
/// (numeric gcd and common factors) out of a sum.
pub fn builtin_factor_terms(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Err(EvalError::Error(
            "FactorTerms requires 1 or 2 arguments".to_string(),
        ));
    }
    let var = match args.get(1) {
        Some(Value::Symbol(s)) => Some(s.clone()),
        Some(_) => {
            return Err(EvalError::TypeError {
                expected: "Symbol".to_string(),
                got: args[1].type_name().to_string(),
            });
        }
        None => None,
    };
    Ok(factor_terms(&args[0], var.as_deref()))
}

/// Extract `(integer coefficient, factors)` from a term.
fn term_factors(term: &Value) -> (Integer, Vec<(Value, Integer)>) {
    match term {
        Value::Integer(n) => (n.clone(), Vec::new()),
        Value::Call { head, args } if head == "Times" => {
            let mut coeff = Integer::from(1);
            let mut factors = Vec::new();
            for f in args {
                match f {
                    Value::Integer(n) => coeff *= n,
                    Value::Call { head: ph, args: pa } if ph == "Power" && pa.len() == 2 => {
                        if let Value::Integer(e) = &pa[1] {
                            factors.push((pa[0].clone(), e.clone()));
                        } else {
                            factors.push((f.clone(), Integer::from(1)));
                        }
                    }
                    _ => factors.push((f.clone(), Integer::from(1))),
                }
            }
            (coeff, factors)
        }
        Value::Call { head, args } if head == "Power" && args.len() == 2 => {
            if let Value::Integer(e) = &args[1] {
                (Integer::from(1), vec![(args[0].clone(), e.clone())])
            } else {
                (Integer::from(1), vec![(term.clone(), Integer::from(1))])
            }
        }
        _ => (Integer::from(1), vec![(term.clone(), Integer::from(1))]),
    }
}

fn factor_terms(expr: &Value, var: Option<&str>) -> Value {
    let terms: Vec<Value> = match expr {
        Value::Call { head, args } if head == "Plus" => args.clone(),
        _ => vec![expr.clone()],
    };
    if terms.is_empty() {
        return expr.clone();
    }

    // Numeric content: gcd of integer coefficients.
    let mut gcd = Integer::from(0);
    let mut contents: Vec<(Integer, Vec<(Value, Integer)>)> = Vec::new();
    for t in &terms {
        let (coeff, factors) = term_factors(t);
        gcd = gcd.gcd(&coeff);
        contents.push((coeff, factors));
    }
    if gcd.is_zero() {
        gcd = Integer::from(1);
    }

    // Common symbolic factors: intersection (positive exponents only).
    let keep = |b: &Value| var.map(|v| !expr_contains_var(b, v)).unwrap_or(true);
    let mut common: Vec<(Value, Integer)> = contents[0]
        .1
        .iter()
        .filter(|(b, e)| *e > 0 && keep(b))
        .cloned()
        .collect();
    for (_, factors) in contents.iter().skip(1) {
        common = common
            .into_iter()
            .filter_map(|(b, e)| {
                factors
                    .iter()
                    .find(|(b2, _)| b2.struct_eq(&b))
                    .map(|(_, e2)| (b, e.clone().min(e2.clone())))
            })
            .collect();
    }

    if gcd <= 1 && common.is_empty() {
        return builtin_expand(&[expr.clone()]).unwrap_or_else(|_| expr.clone());
    }

    // Rebuild each term divided by the extracted content.
    let mut divided: Vec<Value> = Vec::new();
    for (coeff, factors) in &contents {
        let mut term: Vec<Value> = vec![Value::Integer(coeff.clone() / &gcd)];
        let mut remaining: Vec<(Value, Integer)> = Vec::new();
        for (b, e) in factors {
            let sub = common
                .iter()
                .find(|(cb, _)| cb.struct_eq(b))
                .map(|(_, ce)| ce.clone())
                .unwrap_or_else(Integer::default);
            let new_e = e.clone() - sub;
            if new_e > 0 {
                remaining.push((b.clone(), new_e));
            }
        }
        for (b, e) in remaining {
            if e == 1 {
                term.push(b);
            } else {
                term.push(call("Power", vec![b, Value::Integer(e)]));
            }
        }
        divided.push(simplify_call("Times", &term));
    }

    let mut outer: Vec<Value> = Vec::new();
    if gcd > 1 {
        outer.push(Value::Integer(gcd));
    }
    for (b, e) in &common {
        if *e == 1 {
            outer.push(b.clone());
        } else {
            outer.push(call("Power", vec![b.clone(), Value::Integer(e.clone())]));
        }
    }
    outer.push(simplify_call("Plus", &divided));
    simplify_call("Times", &outer)
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    fn has_head(v: &Value, head: &str) -> bool {
        matches!(v, Value::Call { head: h, .. } if h == head)
    }

    fn contains_head(v: &Value, head: &str) -> bool {
        match v {
            Value::Call { head: h, args } => {
                h == head || args.iter().any(|a| contains_head(a, head))
            }
            Value::List(items) => items.iter().any(|a| contains_head(a, head)),
            _ => false,
        }
    }

    fn plus(args: Vec<Value>) -> Value {
        call("Plus", args)
    }

    fn times(args: Vec<Value>) -> Value {
        call("Times", args)
    }

    fn pow(b: Value, e: Value) -> Value {
        call("Power", vec![b, e])
    }

    // ── ExpandAll ──

    #[test]
    fn test_expand_all_square() {
        let expr = pow(plus(vec![sym("x"), int(1)]), int(2));
        let r = builtin_expand_all(&[expr]).unwrap();
        assert!(has_head(&r, "Plus"), "got {r:?}");
    }

    #[test]
    fn test_expand_all_denominator() {
        // (x + 1)/y -> x/y + 1/y
        let expr = call("Divide", vec![plus(vec![sym("x"), int(1)]), sym("y")]);
        let r = builtin_expand_all(&[expr]).unwrap();
        assert!(has_head(&r, "Plus"), "got {r:?}");
    }

    #[test]
    fn test_expand_all_nested() {
        // Sin[(x+1)^2] keeps Sin but expands the inner square.
        let expr = call("Sin", vec![pow(plus(vec![sym("x"), int(1)]), int(2))]);
        let r = builtin_expand_all(&[expr]).unwrap();
        assert!(has_head(&r, "Sin"));
        if let Value::Call { args, .. } = &r {
            assert!(has_head(&args[0], "Plus"));
        }
    }

    // ── PowerExpand ──

    #[test]
    fn test_power_expand_product() {
        let expr = pow(times(vec![sym("a"), sym("b")]), sym("c"));
        let r = builtin_power_expand(&[expr]).unwrap();
        assert!(has_head(&r, "Times"), "got {r:?}");
        assert!(contains_head(&r, "Power"));
    }

    #[test]
    fn test_power_expand_nested_power() {
        let expr = pow(pow(sym("a"), sym("b")), sym("c"));
        let r = builtin_power_expand(&[expr]).unwrap();
        assert!(has_head(&r, "Power"), "got {r:?}");
    }

    #[test]
    fn test_power_expand_log_power() {
        let expr = call("Log", vec![pow(sym("a"), sym("b"))]);
        let r = builtin_power_expand(&[expr]).unwrap();
        assert!(has_head(&r, "Times"), "got {r:?}");
        assert!(contains_head(&r, "Log"));
    }

    #[test]
    fn test_power_expand_log_product() {
        let expr = call("Log", vec![times(vec![sym("a"), sym("b")])]);
        let r = builtin_power_expand(&[expr]).unwrap();
        assert!(has_head(&r, "Plus"), "got {r:?}");
    }

    // ── ExpandLog ──

    #[test]
    fn test_expand_log_product() {
        let expr = call("Log", vec![times(vec![sym("a"), sym("b")])]);
        let r = builtin_expand_log(&[expr]).unwrap();
        assert!(has_head(&r, "Plus"), "got {r:?}");
    }

    #[test]
    fn test_expand_log_power() {
        let expr = call("Log", vec![pow(sym("a"), int(3))]);
        let r = builtin_expand_log(&[expr]).unwrap();
        assert!(has_head(&r, "Times"), "got {r:?}");
    }

    // ── TrigExpand ──

    #[test]
    fn test_trig_expand_sum() {
        let expr = call("Sin", vec![plus(vec![sym("x"), sym("y")])]);
        let r = builtin_trig_expand(&[expr]).unwrap();
        assert!(has_head(&r, "Plus"), "got {r:?}");
        assert!(contains_head(&r, "Sin") && contains_head(&r, "Cos"));
    }

    #[test]
    fn test_trig_expand_double_angle() {
        let expr = call("Sin", vec![times(vec![int(2), sym("x")])]);
        let r = builtin_trig_expand(&[expr]).unwrap();
        assert!(
            contains_head(&r, "Sin") && contains_head(&r, "Cos"),
            "got {r:?}"
        );
    }

    #[test]
    fn test_trig_expand_cos_sum() {
        let expr = call("Cos", vec![plus(vec![sym("x"), sym("y")])]);
        let r = builtin_trig_expand(&[expr]).unwrap();
        assert!(has_head(&r, "Plus"), "got {r:?}");
    }

    // ── TrigReduce ──

    #[test]
    fn test_trig_reduce_product() {
        let expr = times(vec![
            call("Sin", vec![sym("x")]),
            call("Cos", vec![sym("y")]),
        ]);
        let r = builtin_trig_reduce(&[expr]).unwrap();
        assert!(has_head(&r, "Plus"), "got {r:?}");
    }

    #[test]
    fn test_trig_reduce_power() {
        let expr = pow(call("Sin", vec![sym("x")]), int(2));
        let r = builtin_trig_reduce(&[expr]).unwrap();
        assert!(has_head(&r, "Plus"), "got {r:?}");
        assert!(contains_head(&r, "Cos"), "got {r:?}");
    }

    // ── TrigFactor ──

    #[test]
    fn test_trig_factor_sum() {
        let expr = plus(vec![
            call("Sin", vec![sym("x")]),
            call("Sin", vec![sym("y")]),
        ]);
        let r = builtin_trig_factor(&[expr]).unwrap();
        assert!(has_head(&r, "Times"), "got {r:?}");
    }

    #[test]
    fn test_trig_factor_pythagorean() {
        let expr = plus(vec![
            pow(call("Sin", vec![sym("x")]), int(2)),
            pow(call("Cos", vec![sym("x")]), int(2)),
        ]);
        let r = builtin_trig_factor(&[expr]).unwrap();
        assert_eq!(r, int(1));
    }

    // ── TrigToExp / ExpToTrig ──

    #[test]
    fn test_trig_to_exp_sin() {
        let r = builtin_trig_to_exp(&[call("Sin", vec![sym("x")])]).unwrap();
        assert!(has_head(&r, "Divide"), "got {r:?}");
        assert!(contains_head(&r, "Exp"));
    }

    #[test]
    fn test_exp_to_trig_imaginary() {
        let expr = call("Exp", vec![times(vec![i_unit(), sym("x")])]);
        let r = builtin_exp_to_trig(&[expr]).unwrap();
        assert!(has_head(&r, "Plus"), "got {r:?}");
        assert!(contains_head(&r, "Cos") && contains_head(&r, "Sin"));
    }

    #[test]
    fn test_exp_to_trig_real() {
        let r = builtin_exp_to_trig(&[call("Exp", vec![sym("x")])]).unwrap();
        assert!(has_head(&r, "Plus"), "got {r:?}");
    }

    // ── ComplexExpand ──

    #[test]
    fn test_complex_expand_power() {
        let z = plus(vec![sym("x"), times(vec![i_unit(), sym("y")])]);
        let r = builtin_complex_expand(&[pow(z, int(2))]).unwrap();
        assert!(has_head(&r, "Plus"), "got {r:?}");
        assert!(contains_head(&r, "Power"));
    }

    #[test]
    fn test_complex_expand_exp() {
        let expr = call("Exp", vec![times(vec![i_unit(), sym("x")])]);
        let r = builtin_complex_expand(&[expr]).unwrap();
        assert!(
            contains_head(&r, "Cos") && contains_head(&r, "Sin"),
            "got {r:?}"
        );
    }

    #[test]
    fn test_complex_expand_abs() {
        let z = plus(vec![sym("x"), times(vec![i_unit(), sym("y")])]);
        let r = builtin_complex_expand(&[call("Abs", vec![z])]).unwrap();
        assert!(contains_head(&r, "Sqrt"), "got {r:?}");
    }

    // ── FactorTerms ──

    #[test]
    fn test_factor_terms_numeric() {
        // 2 x + 4 -> 2 (x + 2)
        let expr = plus(vec![times(vec![int(2), sym("x")]), int(4)]);
        let r = builtin_factor_terms(&[expr]).unwrap();
        assert!(has_head(&r, "Times"), "got {r:?}");
        if let Value::Call { args, .. } = &r {
            assert_eq!(args[0], int(2));
        }
    }

    #[test]
    fn test_factor_terms_var() {
        let expr = plus(vec![
            times(vec![sym("a"), sym("x")]),
            times(vec![sym("a"), sym("y")]),
        ]);
        let r = builtin_factor_terms(&[expr, sym("y")]).unwrap();
        // a x + a y with x as variable still factors common `a`? y is the var.
        assert!(has_head(&r, "Times"), "got {r:?}");
    }

    // ── Registration ──

    #[test]
    fn test_registered() {
        let env = Env::new();
        crate::builtins::register_builtins(&env);
        for name in [
            "ExpandAll",
            "PowerExpand",
            "ExpandLog",
            "TrigExpand",
            "TrigReduce",
            "TrigFactor",
            "TrigToExp",
            "ExpToTrig",
            "ComplexExpand",
            "FactorTerms",
        ] {
            assert!(env.get(name).is_some(), "{name} not registered");
        }
    }
}
