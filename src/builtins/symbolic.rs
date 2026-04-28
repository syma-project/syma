use crate::ast::Expr;
use crate::env::Env;
use crate::value::{DEFAULT_PRECISION, EvalError, Value};
use rug::Float;
use rug::Integer;
use rug::Rational;
use rug::ops::Pow;

/// Build a symbolic Call node.
fn call(head: &str, args: Vec<Value>) -> Value {
    Value::Call {
        head: head.to_string(),
        args,
    }
}

/// Build a symbolic Call from a slice (copies args).
fn call_ref(head: &str, args: &[Value]) -> Value {
    Value::Call {
        head: head.to_string(),
        args: args.to_vec(),
    }
}

// ── Simplify ──

pub fn builtin_simplify(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Simplify requires exactly 1 argument".to_string(),
        ));
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

pub fn simplify_call(head: &str, args: &[Value]) -> Value {
    match head {
        "Plus" => simplify_plus(args),
        "Times" => simplify_times(args),
        "Power" => simplify_power(args),
        "Sin" => simplify_sin(args),
        "Cos" => simplify_cos(args),
        "Log" => simplify_log(args),
        "Exp" => simplify_exp(args),
        "Divide" if args.len() == 2 => {
            let canceled = cancel_common_factors(&args[0], &args[1]);
            if matches!(&canceled.1, Value::Integer(n) if *n == 1) {
                canceled.0
            } else if let Some(quotient) = try_polynomial_divide(&canceled.0, &canceled.1) {
                quotient
            } else {
                call("Divide", vec![canceled.0, canceled.1])
            }
        }
        _ => call_ref(head, args),
    }
}

fn simplify_plus(args: &[Value]) -> Value {
    if args.is_empty() {
        return Value::Integer(Integer::from(0));
    }
    let mut terms: Vec<Value> = Vec::new();
    for arg in args {
        match arg {
            Value::Integer(n) if n.is_zero() => {}
            Value::Real(r) if *r == Float::with_val(DEFAULT_PRECISION, 0.0) => {}
            Value::Call { head, args: a } if head == "Plus" => {
                terms.extend(a.iter().cloned());
            }
            _ => terms.push(arg.clone()),
        }
    }
    if terms.is_empty() {
        return Value::Integer(Integer::from(0));
    }
    if terms.len() == 1 {
        return terms.into_iter().next().unwrap();
    }
    let terms = combine_like_terms(terms);
    if terms.len() == 1 {
        return terms.into_iter().next().unwrap();
    }
    call("Plus", terms)
}

/// Combine like terms: terms with same base get their coefficients added.
/// e.g. 3*x^2 + (-5)*x^2 + 2*x^2 → -x^2
fn combine_like_terms(terms: Vec<Value>) -> Vec<Value> {
    // (base, coefficient) groups — use linear scan since Value doesn't hash
    let mut groups: Vec<(Value, Integer)> = Vec::new();

    for term in terms {
        let (base, coeff) = extract_coeff_and_base(&term);
        // Find matching group
        let mut found = false;
        for g in &mut groups {
            if g.0.struct_eq(&base) {
                g.1 += coeff.clone();
                found = true;
                break;
            }
        }
        if !found {
            groups.push((base, coeff));
        }
    }

    // Rebuild: skip zero coefficients
    let mut result = Vec::new();
    let one = Integer::from(1);
    for (base, coeff) in groups {
        if coeff.is_zero() {
            continue;
        }
        if base.struct_eq(&Value::Integer(Integer::from(1))) {
            // pure constant
            result.push(Value::Integer(coeff));
        } else if coeff == one {
            result.push(base);
        } else {
            result.push(simplify_call("Times", &[Value::Integer(coeff), base]));
        }
    }

    if result.is_empty() {
        vec![Value::Integer(Integer::from(0))]
    } else {
        result
    }
}

/// Extract (base, integer_coeff) from a term.
/// Integer(n) → (1, n)
/// Times[n, base] where n is Integer → (base, n)
/// other → (term, 1)
fn extract_coeff_and_base(term: &Value) -> (Value, Integer) {
    match term {
        Value::Integer(n) => (Value::Integer(Integer::from(1)), n.clone()),
        Value::Call { head, args } if head == "Times" && args.len() == 2 => match &args[0] {
            Value::Integer(n) => (args[1].clone(), n.clone()),
            _ => match &args[1] {
                Value::Integer(n) => (args[0].clone(), n.clone()),
                _ => (term.clone(), Integer::from(1)),
            },
        },
        _ => (term.clone(), Integer::from(1)),
    }
}

fn simplify_times(args: &[Value]) -> Value {
    if args.is_empty() {
        return Value::Integer(Integer::from(1));
    }
    // Flatten nested Times, filter zeros, skip ones
    let mut flat: Vec<Value> = Vec::new();
    for arg in args {
        match arg {
            Value::Integer(n) if n.is_zero() => return Value::Integer(Integer::from(0)),
            Value::Integer(n) if *n == Integer::from(1) => {}
            Value::Real(r) if *r == Float::with_val(DEFAULT_PRECISION, 0.0) => {
                return Value::Integer(Integer::from(0));
            }
            Value::Real(r) if *r == Float::with_val(DEFAULT_PRECISION, 1.0) => {}
            Value::Call { head, args: a } if head == "Times" => {
                flat.extend(a.iter().cloned());
            }
            _ => flat.push(arg.clone()),
        }
    }
    if flat.is_empty() {
        return Value::Integer(Integer::from(1));
    }

    // Separate numeric factors (Integer, Real, Rational) from symbolic
    let mut numeric_factors: Vec<Value> = vec![Value::Integer(Integer::from(1))];
    let mut symbolic: Vec<Value> = Vec::new();
    for factor in flat {
        match &factor {
            Value::Integer(_) | Value::Real(_) | Value::Rational(_) => {
                numeric_factors.push(factor.clone());
            }
            _ => symbolic.push(factor),
        }
    }
    // Multiply all numeric factors together
    let numeric_product = numeric_factors
        .iter()
        .skip(1)
        .fold(Value::Integer(Integer::from(1)), |acc, v| {
            crate::builtins::arithmetic::mul_values_public(&acc, v).unwrap_or(acc.clone())
        });
    // Check if numeric product is zero
    match &numeric_product {
        Value::Integer(n) if n.is_zero() => return Value::Integer(Integer::from(0)),
        Value::Real(r) if *r == Float::with_val(DEFAULT_PRECISION, 0.0) => {
            return Value::Integer(Integer::from(0));
        }
        _ => {}
    }

    // Convert symbolic factors to (base, exponent) pairs for merging
    // n * n^2 → (n, 1) + (n, 2) → Power[n, 3]
    let mut base_exp: Vec<(Value, Integer)> = Vec::new();
    for factor in symbolic {
        match &factor {
            Value::Call { head, args } if head == "Power" && args.len() == 2 => {
                if let Value::Integer(exp) = &args[1] {
                    if let Some((_, acc)) = base_exp.iter_mut().find(|(b, _)| *b == args[0]) {
                        *acc += exp;
                    } else {
                        base_exp.push((args[0].clone(), exp.clone()));
                    }
                } else {
                    // Non-integer exponent, can't merge — treat as base=itself
                    if let Some((_, acc)) = base_exp.iter_mut().find(|(b, _)| *b == factor) {
                        *acc += 1;
                    } else {
                        base_exp.push((factor, Integer::from(1)));
                    }
                }
            }
            _ => {
                if let Some((_, acc)) = base_exp.iter_mut().find(|(b, _)| *b == factor) {
                    *acc += 1;
                } else {
                    base_exp.push((factor, Integer::from(1)));
                }
            }
        }
    }

    // Build result: numeric_product with each base^exp
    let mut result: Vec<Value> = Vec::new();
    // Add numeric product unless it's 1 (in any form)
    let is_one = match &numeric_product {
        Value::Integer(n) => *n == Integer::from(1),
        Value::Real(r) => *r == Float::with_val(DEFAULT_PRECISION, 1.0),
        Value::Rational(r) => **r == Rational::from(1_i64),
        _ => false,
    };
    if !is_one {
        result.push(numeric_product);
    }
    for (base, exp) in base_exp {
        if exp.is_zero() {
            continue;
        }
        if exp == Integer::from(1) {
            result.push(base);
        } else {
            result.push(simplify_call("Power", &[base, Value::Integer(exp)]));
        }
    }

    if result.is_empty() {
        Value::Integer(Integer::from(1))
    } else if result.len() == 1 {
        result.into_iter().next().unwrap()
    } else {
        call("Times", result)
    }
}

fn simplify_power(args: &[Value]) -> Value {
    if args.len() != 2 {
        return call_ref("Power", args);
    }
    match &args[1] {
        Value::Integer(n) if n.is_zero() => Value::Integer(Integer::from(1)),
        Value::Integer(n) if *n == 1 => args[0].clone(),
        _ => match &args[0] {
            Value::Integer(n) if n.is_zero() => Value::Integer(Integer::from(0)),
            Value::Integer(n) if *n == 1 => Value::Integer(Integer::from(1)),
            // Nested Power: Power[Power[x, a], b] → Power[x, a*b]
            Value::Call { head, args: inner } if head == "Power" && inner.len() == 2 => {
                let new_exp = simplify_call("Times", &[inner[1].clone(), args[1].clone()]);
                simplify_call("Power", &[inner[0].clone(), new_exp])
            }
            _ => call_ref("Power", args),
        },
    }
}

fn simplify_sin(args: &[Value]) -> Value {
    if args.len() != 1 {
        return call_ref("Sin", args);
    }
    match &args[0] {
        Value::Integer(n) if n.is_zero() => Value::Integer(Integer::from(0)),
        _ => call_ref("Sin", args),
    }
}

fn simplify_cos(args: &[Value]) -> Value {
    if args.len() != 1 {
        return call_ref("Cos", args);
    }
    match &args[0] {
        Value::Integer(n) if n.is_zero() => Value::Integer(Integer::from(1)),
        _ => call_ref("Cos", args),
    }
}

fn simplify_log(args: &[Value]) -> Value {
    if args.len() != 1 {
        return call_ref("Log", args);
    }
    match &args[0] {
        Value::Integer(n) if *n == 1 => Value::Integer(Integer::from(0)),
        Value::Real(r) => {
            let e_val = Float::with_val(DEFAULT_PRECISION, 1).exp();
            if (r.clone() - e_val).abs() < 1e-10 {
                Value::Integer(Integer::from(1))
            } else {
                call_ref("Log", args)
            }
        }
        Value::Call { head, args: inner } if head == "Exp" && inner.len() == 1 => inner[0].clone(),
        _ => call_ref("Log", args),
    }
}

fn simplify_exp(args: &[Value]) -> Value {
    if args.len() != 1 {
        return call_ref("Exp", args);
    }
    match &args[0] {
        Value::Integer(n) if n.is_zero() => Value::Integer(Integer::from(1)),
        Value::Call { head, args: inner } if head == "Log" && inner.len() == 1 => inner[0].clone(),
        _ => call_ref("Exp", args),
    }
}

// ── Expand ──

pub fn builtin_expand(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Expand requires exactly 1 argument".to_string(),
        ));
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
                "Divide" if args.len() == 2 => {
                    let canceled = cancel_common_factors(&args[0], &args[1]);
                    let num_expanded = expand_value(&canceled.0);
                    let den_expanded = expand_value(&canceled.1);
                    if matches!(&den_expanded, Value::Integer(n) if *n == 1) {
                        num_expanded
                    } else {
                        call("Divide", vec![num_expanded, den_expanded])
                    }
                }
                _ => call(head, expanded_args),
            }
        }
        _ => val.clone(),
    }
}

/// Cancel common factors between numerator and denominator.
/// Handles Power[a, m] / Power[a, n] → Power[a, n-m] / Power[a, m-n] etc.
fn cancel_common_factors(num: &Value, den: &Value) -> (Value, Value) {
    // Collect num factors as (base, exp) pairs
    let mut num_factors: Vec<(Value, Integer)> = Vec::new();
    collect_power_factors(num, &mut num_factors);

    // Collect den factors as (base, exp) pairs
    let mut den_factors: Vec<(Value, Integer)> = Vec::new();
    collect_power_factors(den, &mut den_factors);

    // For each matching base, subtract exponents
    let mut remaining_num: Vec<(Value, Integer)> = Vec::new();
    let mut remaining_den: Vec<(Value, Integer)> = Vec::new();

    for (nb, ne) in &num_factors {
        if let Some((_db, de)) = den_factors.iter_mut().find(|(b, _)| *b == *nb) {
            if ne > de {
                remaining_num.push((nb.clone(), ne.clone() - de.clone()));
            } else if *de > *ne {
                remaining_den.push((nb.clone(), de.clone() - ne.clone()));
            }
            // equal: cancel completely
        } else {
            remaining_num.push((nb.clone(), ne.clone()));
        }
    }

    // Add unmatched den factors
    for (db, de) in &den_factors {
        if !num_factors.iter().any(|(b, _)| *b == *db) {
            remaining_den.push((db.clone(), de.clone()));
        }
    }

    let new_num = rebuild_from_factors(&remaining_num);
    let new_den = rebuild_from_factors(&remaining_den);

    (new_num, new_den)
}

/// Collect factors into (base, exp) pairs.
/// Times[a, b^2, c] → [(a,1), (b,2), (c,1)]
/// Power[a, 3] → [(a, 3)]
/// x → [(x, 1)]
fn collect_power_factors(val: &Value, factors: &mut Vec<(Value, Integer)>) {
    match val {
        Value::Call { head, args } if head == "Times" => {
            for arg in args {
                collect_power_factors(arg, factors);
            }
        }
        Value::Call { head, args } if head == "Power" && args.len() == 2 => {
            let base = &args[0];
            let exp = match &args[1] {
                Value::Integer(n) => n.clone(),
                _ => Integer::from(1),
            };
            factors.push((base.clone(), exp));
        }
        _ => {
            factors.push((val.clone(), Integer::from(1)));
        }
    }
}

/// Rebuild a Value from (base, exp) factors.
fn rebuild_from_factors(factors: &[(Value, Integer)]) -> Value {
    if factors.is_empty() {
        return Value::Integer(Integer::from(1));
    }
    if factors.len() == 1 {
        let (base, exp) = &factors[0];
        if exp == &Integer::from(1) {
            return base.clone();
        }
        return simplify_call("Power", &[base.clone(), Value::Integer(exp.clone())]);
    }
    let terms: Vec<Value> = factors
        .iter()
        .map(|(base, exp)| {
            if exp == &Integer::from(1) {
                base.clone()
            } else {
                simplify_call("Power", &[base.clone(), Value::Integer(exp.clone())])
            }
        })
        .collect();
    simplify_call("Times", &terms)
}

/// Try polynomial long division of `num` by `den`.
/// Returns Some(quotient) if the division is exact (zero remainder), None otherwise.
fn try_polynomial_divide(num: &Value, den: &Value) -> Option<Value> {
    // Pick a variable to divide by (prefer the first symbol found in denominator)
    let var = find_polynomial_var(den)?;
    if !expr_contains_var(num, &var) {
        return None;
    }

    // Extract coefficients: coeffs[i] = coefficient of var^i
    let num_coeffs = extract_polynomial_coeffs(num, &var);
    let den_coeffs = extract_polynomial_coeffs(den, &var);

    let num_deg = (num_coeffs.len() - 1) as i64;
    let den_deg = (den_coeffs.len() - 1) as i64;
    if num_deg < den_deg {
        return None;
    }

    // Leading coefficient of denominator must be a constant (integer or number) for simple division
    let den_lead = &den_coeffs[den_coeffs.len() - 1];
    if !is_numeric_scalar(den_lead) {
        // Try anyway — use symbolic division
    }

    // Polynomial long division
    let mut remainder: Vec<Value> = num_coeffs.iter().map(|v| simplify_value(v)).collect();
    let den_lead_inv = invert_value(den_lead);

    let quot_deg = num_deg - den_deg;
    let mut quot_coeffs: Vec<Value> =
        vec![Value::Integer(Integer::from(0)); (quot_deg + 1) as usize];

    for i in (0..=quot_deg).rev() {
        let j = (i + den_deg) as usize;
        if j >= remainder.len() {
            continue;
        }
        let r = &remainder[j];
        if is_zero_value(r) {
            continue;
        }
        let q = simplify_call("Times", &[r.clone(), den_lead_inv.clone()]);
        quot_coeffs[i as usize] = q.clone();

        // Subtract q * den_coeffs shifted by i from remainder
        for k in 0..den_coeffs.len() {
            let idx = (i + k as i64) as usize;
            if idx < remainder.len() {
                let sub =
                    expand_value(&simplify_call("Times", &[q.clone(), den_coeffs[k].clone()]));
                let neg = expand_value(&simplify_call(
                    "Times",
                    &[Value::Integer(Integer::from(-1)), sub],
                ));
                remainder[idx] =
                    expand_value(&simplify_call("Plus", &[remainder[idx].clone(), neg]));
            }
        }
        // Simplify all remainder coefficients after each subtraction step
        for v in remainder.iter_mut() {
            *v = simplify_value(v);
        }
    }

    // Check if all remaining coefficients are zero
    let all_zero = remainder.iter().all(|v| is_zero_value(v));
    if !all_zero {
        return None;
    }

    // Reconstruct quotient polynomial
    Some(rebuild_poly_from_coeffs(&quot_coeffs, &var))
}

fn invert_value(val: &Value) -> Value {
    match val {
        Value::Integer(n) => {
            if *n == 1 {
                val.clone()
            } else {
                call(
                    "Power",
                    vec![val.clone(), Value::Integer(Integer::from(-1))],
                )
            }
        }
        _ => call(
            "Power",
            vec![val.clone(), Value::Integer(Integer::from(-1))],
        ),
    }
}

fn is_zero_value(val: &Value) -> bool {
    matches!(val, Value::Integer(n) if n.is_zero())
}

fn is_numeric_scalar(val: &Value) -> bool {
    matches!(val, Value::Integer(_) | Value::Real(_))
}

fn expr_contains_var(expr: &Value, var: &str) -> bool {
    match expr {
        Value::Symbol(s) => s == var,
        Value::Call { args, .. } => args.iter().any(|a| expr_contains_var(a, var)),
        _ => false,
    }
}

fn rebuild_poly_from_coeffs(coeffs: &[Value], var: &str) -> Value {
    let x = Value::Symbol(var.to_string());
    let mut terms: Vec<Value> = Vec::new();
    for (i, coeff) in coeffs.iter().enumerate() {
        if is_zero_value(coeff) {
            continue;
        }
        let term = match i {
            0 => coeff.clone(),
            1 => simplify_call("Times", &[coeff.clone(), x.clone()]),
            _ => simplify_call(
                "Times",
                &[
                    coeff.clone(),
                    call(
                        "Power",
                        vec![x.clone(), Value::Integer(Integer::from(i as i64))],
                    ),
                ],
            ),
        };
        terms.push(term);
    }
    if terms.is_empty() {
        Value::Integer(Integer::from(0))
    } else if terms.len() == 1 {
        terms.into_iter().next().unwrap()
    } else {
        call("Plus", terms)
    }
}

fn expand_times(args: &[Value]) -> Value {
    if args.len() != 2 {
        return call_ref("Times", args);
    }
    let (left, right) = (&args[0], &args[1]);
    if let Value::Call {
        head,
        args: plus_args,
    } = right
        && head == "Plus"
    {
        let terms: Vec<Value> = plus_args
            .iter()
            .map(|term| expand_value(&simplify_call("Times", &[left.clone(), term.clone()])))
            .collect();
        return simplify_call("Plus", &terms);
    }
    if let Value::Call {
        head,
        args: plus_args,
    } = left
        && head == "Plus"
    {
        let terms: Vec<Value> = plus_args
            .iter()
            .map(|term| expand_value(&simplify_call("Times", &[term.clone(), right.clone()])))
            .collect();
        return simplify_call("Plus", &terms);
    }
    call_ref("Times", args)
}

fn expand_power(args: &[Value]) -> Value {
    if args.len() != 2 {
        return call_ref("Power", args);
    }
    let (base, exp) = (&args[0], &args[1]);
    if let Value::Integer(n) = exp
        && let Some(n_i64) = n.to_i64()
        && let Value::Call {
            head,
            args: plus_args,
        } = base
        && head == "Plus"
        && plus_args.len() == 2
        && (0..=10).contains(&n_i64)
    {
        let (a, b) = (&plus_args[0], &plus_args[1]);
        let mut terms = Vec::new();
        for k in 0..=n_i64 {
            let coeff = binomial(n_i64, k);
            let a_pow = if n_i64 - k == 0 {
                Value::Integer(Integer::from(1))
            } else if n_i64 - k == 1 {
                a.clone()
            } else {
                simplify_call(
                    "Power",
                    &[a.clone(), Value::Integer(Integer::from(n_i64 - k))],
                )
            };
            let b_pow = if k == 0 {
                Value::Integer(Integer::from(1))
            } else if k == 1 {
                b.clone()
            } else {
                simplify_call("Power", &[b.clone(), Value::Integer(Integer::from(k))])
            };
            let term = simplify_call(
                "Times",
                &[Value::Integer(Integer::from(coeff)), a_pow, b_pow],
            );
            terms.push(term);
        }
        return simplify_call("Plus", &terms);
    }
    call_ref("Power", args)
}

fn binomial(n: i64, k: i64) -> i64 {
    if k < 0 || k > n {
        return 0;
    }
    if k == 0 || k == n {
        return 1;
    }
    let k = if k > n - k { n - k } else { k };
    let mut result = 1i64;
    for i in 0..k {
        result = result * (n - i) / (i + 1);
    }
    result
}

// ── Differentiation ──

/// D[expr, x] — Symbolic differentiation.
/// D[expr, x, y, ...] — Mixed partial derivatives.
/// D[expr, {x, n}] — n-th order derivative.
/// D[expr, {x, n, x0}] — n-th order derivative evaluated at x=x0.
/// D[expr, {x, n}, y, ...] — n-th derivative w.r.t. x, then w.r.t. y, etc.
///
/// Multi-arg and n-th-order forms are normalized to D[f, var] (2-arg)
/// before dispatching to Syma rules loaded from D.syma.
///
/// Loading order (disk → embedded):
/// 1. Search paths (SYMA_HOME/SystemFiles, current dir, etc.)
/// 2. Executable-relative SystemFiles/Kernel/Calculus/D.syma
/// 3. Embedded source (compiled-in fallback for `cargo run` without install)
pub fn builtin_d(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "D requires at least 2 arguments".to_string(),
        ));
    }

    lazy_load_d(env)?;

    // Unwrap Hold/Pattern wrappers from HoldAll attribute
    let unwrapped: Vec<Value> = args
        .iter()
        .map(|a| match a {
            Value::Hold(inner) | Value::HoldComplete(inner) => (**inner).clone(),
            Value::Pattern(p) => crate::pattern::unwrap_expr_to_value(p),
            _ => a.clone(),
        })
        .collect();

    // Check if second argument is a List {x, n} or {x, n, x0}
    if let Value::List(items) = &unwrapped[1] {
        if !items.is_empty() && items.len() >= 2 && items.len() <= 3 {
            let var = items[0].clone();
            let n = items[1].to_integer().ok_or_else(|| {
                EvalError::Error("D: order n must be a non-negative integer".to_string())
            })?;
            if n < 0 {
                return Err(EvalError::Error(
                    "D: order n must be a non-negative integer".to_string(),
                ));
            }
            // Repeatedly differentiate n times w.r.t. var
            let mut result = unwrapped[0].clone();
            for _ in 0..n as usize {
                result = eval_d_2(result, var.clone(), env)?;
            }
            if items.len() == 3 {
                // D[f, {x, n, x0}] — evaluate at x = x0
                let var_name = match &var {
                    Value::Symbol(s) => s.clone(),
                    _ => {
                        return Err(EvalError::Error(
                            "D: variable in {x, n, x0} must be a symbol".to_string(),
                        ));
                    }
                };
                if unwrapped.len() > 2 {
                    for arg in &unwrapped[2..] {
                        result = eval_d_2(result, arg.clone(), env)?;
                    }
                }
                return Ok(substitute_and_eval(&result, &var_name, &items[2]));
            }
            if unwrapped.len() > 2 {
                for arg in &unwrapped[2..] {
                    result = eval_d_2(result, arg.clone(), env)?;
                }
            }
            return Ok(result);
        }
    }

    // Multi-arg: D[f, x, y, z, ...] — fold left: D[D[D[f, x], y], z]
    if unwrapped.len() > 2 {
        let mut result = unwrapped[0].clone();
        for arg in &unwrapped[1..] {
            result = eval_d_2(result, arg.clone(), env)?;
        }
        return Ok(result);
    }

    // 2-arg case: D[f, x]
    eval_d_2(unwrapped[0].clone(), unwrapped[1].clone(), env)
}

/// Evaluate D[expr, var] (2-arg form) by dispatching to Syma function definitions.
/// Handles multi-arg Times/Plus in Rust because the Flat attribute causes
/// pattern matching to flatten nested calls, breaking 2-arg patterns.
fn eval_d_2(expr: Value, var: Value, env: &Env) -> Result<Value, EvalError> {
    lazy_load_d(env)?;

    // Unwrap Value::Pattern/Value::Hold to get a concrete Value for inspection.
    // D has HoldAll, so args come in as Value::Pattern(Expr) or Value::Hold(Value).
    let concrete = match &expr {
        Value::Pattern(p) => crate::pattern::unwrap_expr_to_value(p),
        Value::Hold(inner) | Value::HoldComplete(inner) => (**inner).clone(),
        _ => expr.clone(),
    };

    // If the expression is a D call, evaluate the inner D first.
    // This handles nested derivatives like D[D[f, x], x].
    if let Value::Call {
        head: ref h,
        ref args,
    } = concrete
    {
        if h == "D" && args.len() == 2 {
            // Call eval_d_2 directly with already-unwrapped args
            let inner_result = eval_d_2(args[0].clone(), args[1].clone(), env)?;
            return eval_d_2(inner_result, var, env);
        }
    }

    // Flatten nested Plus/Times (parser builds nested, Flat flattens at runtime)
    let flattened = match &concrete {
        &Value::Call { ref head, ref args }
            if (head == "Plus" || head == "Times") && args.len() >= 2 =>
        {
            let flat_args = crate::eval::flatten_flat_args(head, args);
            if flat_args.len() != args.len() {
                Some(Value::Call {
                    head: head.clone(),
                    args: flat_args,
                })
            } else {
                None
            }
        }
        _ => None,
    };
    let concrete = flattened.unwrap_or(concrete);

    // Handle multi-arg Times/Plus in Rust
    if let Value::Call { ref head, ref args } = concrete {
        let name = head.as_str();
        if (name == "Times" || name == "Plus") && args.len() > 2 {
            let n = args.len();
            let rest = Value::Call {
                head: name.to_string(),
                args: args[..n - 1].to_vec(),
            };
            let last = args[n - 1].clone();

            if name == "Times" {
                // Product rule: D[f*g*h, x] = D[f*g, x]*h + f*g*D[h, x]
                let d_rest = eval_d_2(rest.clone(), var.clone(), env)?;
                let d_last = eval_d_2(last.clone(), var.clone(), env)?;
                return Ok(add_times(vec![d_rest, last.clone()], vec![rest, d_last]));
            } else {
                // Plus rule: D[f + g + h, x] = D[f + g, x] + D[h, x]
                let d_rest = eval_d_2(rest, var.clone(), env)?;
                let d_last = eval_d_2(last, var.clone(), env)?;
                return Ok(make_plus(vec![d_rest, d_last]));
            }
        }
    }

    if let Some(d_func) = env.get("D")
        && let Value::Function(_) = &d_func
    {
        // Evaluate Pattern args in the current environment to resolve symbolic
        // variable names (u, n, etc.) that come from parent rule bodies.
        // D has HoldAll, so args come as Value::Pattern(Expr). When the D call
        // originates from another D rule's body (e.g., D[u, x] from the Power rule),
        // the Expr contains symbolic variable names. Passing Pattern-wrapped symbols
        // to the D function definitions causes pattern matching to bind to those
        // symbolic names. By evaluating the Expr in env, we resolve them to actual values.
        let concrete_expr = match &expr {
            Value::Pattern(p) => crate::eval::eval(p, env)?,
            Value::Hold(inner) | Value::HoldComplete(inner) => (**inner).clone(),
            _ => expr.clone(),
        };
        let concrete_var = match &var {
            Value::Pattern(p) => crate::eval::eval(p, env)?,
            Value::Hold(inner) | Value::HoldComplete(inner) => (**inner).clone(),
            _ => var.clone(),
        };
        return crate::eval::apply_function(&d_func, &[concrete_expr, concrete_var], env);
    }
    Ok(Value::Call {
        head: "D".to_string(),
        args: vec![expr, var],
    })
}

/// Create a Plus of values (single arg → that arg, two args → Plus[a,b])
fn make_plus(items: Vec<Value>) -> Value {
    match items.len() {
        0 => Value::Integer(rug::Integer::from(0)),
        1 => items.into_iter().next().unwrap(),
        _ => Value::Call {
            head: "Plus".to_string(),
            args: items,
        },
    }
}

/// Create Plus[Times[...], Times[...]]
fn add_times(left: Vec<Value>, right: Vec<Value>) -> Value {
    let l = make_times(left);
    let r = make_times(right);
    make_plus(vec![l, r])
}

/// Create a Times of values. Returns 0 if any arg is 0.
fn make_times(items: Vec<Value>) -> Value {
    if items.iter().any(|v| is_zero(v)) {
        return Value::Integer(rug::Integer::from(0));
    }
    match items.len() {
        0 => Value::Integer(rug::Integer::from(1)),
        1 => items.into_iter().next().unwrap(),
        _ => Value::Call {
            head: "Times".to_string(),
            args: items,
        },
    }
}

/// Check if a Value represents zero.
fn is_zero(v: &Value) -> bool {
    match v {
        Value::Integer(n) => n.is_zero(),
        Value::Real(r) => r.is_zero(),
        Value::Rational(r) => r.is_zero(),
        _ => false,
    }
}

/// Lazily load D.syma differentiation rules into the environment.
/// Tries disk first (search_paths, executable-relative), falls back to embedded.
/// Caches the parsed AST so subsequent calls are cheap.
fn lazy_load_d(env: &Env) -> Result<(), EvalError> {
    use std::sync::OnceLock;
    static PARSED_AST: OnceLock<Vec<(crate::ast::Expr, bool)>> = OnceLock::new();

    // Relative path within SystemFiles — matches Mathematica convention:
    // $SYMA_HOME/SystemFiles/Kernel/Calculus/D.syma
    const REL_PATH: &str = "Kernel/Calculus/D.syma";

    let stmts = PARSED_AST.get_or_init(|| {
        // Try search_paths first (SYMA_HOME/SystemFiles, current dir, etc.)
        let paths = env.search_paths.lock().unwrap();
        for dir in paths.iter() {
            let candidate = dir.join(REL_PATH);
            if candidate.exists() {
                let source =
                    std::fs::read_to_string(&candidate).unwrap_or_else(|_| embedded_d_source());
                let tokens = crate::lexer::tokenize(&source).unwrap();
                return crate::parser::parse_with_suppress(tokens).unwrap();
            }
        }

        // Try executable-relative SystemFiles (installed layout)
        if let Some(exe_dir) = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        {
            let candidate = exe_dir.join("../").join("SystemFiles").join(REL_PATH);
            if candidate.exists() {
                let source =
                    std::fs::read_to_string(&candidate).unwrap_or_else(|_| embedded_d_source());
                let tokens = crate::lexer::tokenize(&source).unwrap();
                return crate::parser::parse_with_suppress(tokens).unwrap();
            }
        }

        // Fallback: embedded source (for cargo run without install)
        let source = embedded_d_source();
        let tokens = crate::lexer::tokenize(&source).unwrap();
        crate::parser::parse_with_suppress(tokens).unwrap()
    });

    for (expr, _suppress) in stmts {
        crate::eval::eval(expr, env).map_err(|e| EvalError::Error(e.to_string()))?;
    }

    // After loading, ensure the D function is in the root scope so it's
    // accessible from all child scopes.
    if let Some(d_func) = env.get("D") {
        if let Value::Function(_) = &d_func {
            env.root_env().set("D".to_string(), d_func);
        }
    }

    Ok(())
}

/// Embedded D.syma source — fallback when no disk file is found.
fn embedded_d_source() -> String {
    // Keep this in sync with SystemFiles/Kernel/Calculus/D.syma
    include_str!("../../SystemFiles/Kernel/Calculus/D.syma").to_string()
}

// ── Integration ──

/// Integrate[expr, x] — Symbolic integration via Rubi rules + hardcoded fallback.
pub fn builtin_integrate(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Integrate requires exactly 2 arguments".to_string(),
        ));
    }
    // Unwrap Value::Pattern (from HoldAll attribute)
    let expr = match &args[0] {
        Value::Pattern(p) => crate::pattern::unwrap_expr_to_value(p),
        _ => args[0].clone(),
    };

    // Extract variable name and optional bounds for definite integral
    let second = match &args[1] {
        Value::Pattern(p) => crate::pattern::unwrap_expr_to_value(p),
        _ => args[1].clone(),
    };
    let (var_name, bounds) = if let Value::List(ref items) = second {
        if items.len() == 3 {
            let vn = match &items[0] {
                Value::Symbol(s) => s.clone(),
                _ => {
                    return Err(EvalError::TypeError {
                        expected: "Symbol (variable name)".to_string(),
                        got: items[0].type_name().to_string(),
                    });
                }
            };
            (Some(vn), Some((items[1].clone(), items[2].clone())))
        } else {
            (None, None)
        }
    } else {
        (
            match &second {
                Value::Symbol(s) => Some(s.clone()),
                _ => None,
            },
            None,
        )
    };

    // Load Rubi rules (lazy, once)
    lazy_load_integrate(env)?;

    let var_name = var_name.ok_or_else(|| EvalError::TypeError {
        expected: "Symbol (variable name)".to_string(),
        got: args[1].type_name().to_string(),
    })?;

    // Compute indefinite integral first (needed for both indefinite and definite forms)
    // Unwrap Value::Pattern wrappers (from HoldAll) before matching Rubi rules
    let var_sym = Value::Symbol(var_name.clone());
    let indefinite_args = vec![expr.clone(), var_sym.clone()];
    let mut antideriv = None;

    // Dispatch to Integrate function definitions (loaded from .syma files)
    if let Some(Value::Function(func_def)) = env.get("Integrate") {
        for def in &func_def.definitions {
            if let Some(bindings) =
                crate::eval::try_match_params(&def.params, &indefinite_args, env)?
            {
                if let Some(guard_expr) = &def.guard {
                    let guard_env = env.child();
                    for (name, value) in &bindings {
                        guard_env.set(name.clone(), value.clone());
                    }
                    if !crate::eval::eval(guard_expr, &guard_env)?.to_bool() {
                        continue;
                    }
                }
                let child_env = env.child();
                for (name, value) in &bindings {
                    child_env.set(name.clone(), value.clone());
                }
                match crate::eval::eval(&def.body, &child_env) {
                    Ok(result) => {
                        antideriv = Some(result);
                        break;
                    }
                    Err(_) => continue,
                }
            }
        }
    }

    // Fallback to hardcoded integration if Rubi rules didn't match
    if antideriv.is_none() {
        antideriv = Some(integrate(&expr, &var_name));
    }

    let antideriv = antideriv.unwrap();

    // Check if antiderivative is still an unevaluated Integrate call
    let is_unevaluated = matches!(&antideriv, Value::Call { head, .. } if head == "Integrate");

    // Indefinite form: Integrate[expr, x]
    if bounds.is_none() {
        return Ok(antideriv);
    }

    // Definite form: Integrate[expr, {x, a, b}] -> F(b) - F(a)
    // If we couldn't compute the antiderivative, return unevaluated
    if is_unevaluated {
        return Ok(Value::Call {
            head: "Integrate".to_string(),
            args: vec![expr, second],
        });
    }

    let (lower, upper) = bounds.unwrap();
    let f_upper = substitute_and_eval(&antideriv, &var_name, &upper);
    let f_lower = substitute_and_eval(&antideriv, &var_name, &lower);
    let neg_lower = simplify_call("Times", &[Value::Integer(rug::Integer::from(-1)), f_lower]);
    Ok(simplify_call("Plus", &[f_upper, neg_lower]))
}

/// Lazy-load Integrate rules from .syma files (like lazy_load_d for D).
fn lazy_load_integrate(env: &Env) -> Result<(), EvalError> {
    use std::sync::OnceLock;
    static LOADED: OnceLock<()> = OnceLock::new();
    LOADED.get_or_init(|| {
        let files = [
            "1-algebraic.syma",
            "2-exponentials.syma",
            "3-logarithms.syma",
            "4-trig.syma",
            "5-inverse-trig.syma",
            "6-hyperbolic.syma",
            "7-inverse-hyperbolic.syma",
            "8-special.syma",
            "9-miscellaneous.syma",
        ];
        for fname in &files {
            load_integrate_file(env, fname);
        }
    });
    Ok(())
}

/// Load a single Integrate rule file.
fn load_integrate_file(env: &Env, fname: &str) {
    const REL_PATH: &str = "Kernel/Calculus/";

    // Try search_paths first
    {
        let paths = env.search_paths.lock().unwrap();
        for dir in paths.iter() {
            let candidate = dir.join(REL_PATH).join(fname);
            if candidate.exists() {
                if let Ok(source) = std::fs::read_to_string(&candidate) {
                    if let Ok(tokens) = crate::lexer::tokenize(&source) {
                        if let Ok(stmts) = crate::parser::parse_with_suppress(tokens) {
                            for (expr, _suppress) in stmts {
                                let _ = crate::eval::eval(&expr, env);
                            }
                        }
                    }
                }
                return;
            }
        }
    }

    // Try exe-relative SystemFiles
    if let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
    {
        let candidate = exe_dir
            .join("../")
            .join("SystemFiles")
            .join(REL_PATH)
            .join(fname);
        if candidate.exists() {
            if let Ok(source) = std::fs::read_to_string(&candidate) {
                if let Ok(tokens) = crate::lexer::tokenize(&source) {
                    if let Ok(stmts) = crate::parser::parse_with_suppress(tokens) {
                        for (expr, _suppress) in stmts {
                            let _ = crate::eval::eval(&expr, env);
                        }
                    }
                }
            }
            return;
        }
    }

    // Fallback: embedded source
    if let Some(source) = embedded_integrate_source(fname) {
        if let Ok(tokens) = crate::lexer::tokenize(&source) {
            if let Ok(stmts) = crate::parser::parse_with_suppress(tokens) {
                for (expr, _suppress) in stmts {
                    let _ = crate::eval::eval(&expr, env);
                }
            }
        }
    }
}

/// Embedded Integrate rule files — fallback for cargo run.
fn embedded_integrate_source(fname: &str) -> Option<String> {
    match fname {
        "1-algebraic.syma" => {
            Some(include_str!("../../SystemFiles/Kernel/Calculus/1-algebraic.syma").to_string())
        }
        "2-exponentials.syma" => {
            Some(include_str!("../../SystemFiles/Kernel/Calculus/2-exponentials.syma").to_string())
        }
        "3-logarithms.syma" => {
            Some(include_str!("../../SystemFiles/Kernel/Calculus/3-logarithms.syma").to_string())
        }
        "4-trig.syma" => {
            Some(include_str!("../../SystemFiles/Kernel/Calculus/4-trig.syma").to_string())
        }
        "5-inverse-trig.syma" => {
            Some(include_str!("../../SystemFiles/Kernel/Calculus/5-inverse-trig.syma").to_string())
        }
        "6-hyperbolic.syma" => {
            Some(include_str!("../../SystemFiles/Kernel/Calculus/6-hyperbolic.syma").to_string())
        }
        "7-inverse-hyperbolic.syma" => Some(
            include_str!("../../SystemFiles/Kernel/Calculus/7-inverse-hyperbolic.syma").to_string(),
        ),
        "8-special.syma" => {
            Some(include_str!("../../SystemFiles/Kernel/Calculus/8-special.syma").to_string())
        }
        "9-miscellaneous.syma" => {
            Some(include_str!("../../SystemFiles/Kernel/Calculus/9-miscellaneous.syma").to_string())
        }
        _ => None,
    }
}

fn integrate(expr: &Value, var: &str) -> Value {
    let x = Value::Symbol(var.to_string());
    match expr {
        Value::Integer(n) => simplify_call("Times", &[Value::Integer(n.clone()), x]),
        Value::Real(r) => simplify_call("Times", &[Value::Real(r.clone()), x]),
        Value::Symbol(s) => {
            if s == var {
                simplify_call(
                    "Times",
                    &[
                        Value::Real(Float::with_val(DEFAULT_PRECISION, 0.5)),
                        simplify_call("Power", &[x, Value::Integer(Integer::from(2))]),
                    ],
                )
            } else {
                simplify_call("Times", &[Value::Symbol(s.clone()), x])
            }
        }
        Value::Call { head, args } => match head.as_str() {
            "Plus" => {
                let terms: Vec<Value> = args.iter().map(|a| integrate(a, var)).collect();
                simplify_call("Plus", &terms)
            }
            "Times" => {
                let (constants, vars): (Vec<_>, Vec<_>) =
                    args.iter().partition(|a| is_constant_wrt(a, var));
                if vars.is_empty() {
                    simplify_call("Times", &[simplify_call("Times", args), x])
                } else if vars.len() == 1 {
                    let var_part = integrate(vars[0], var);
                    let const_vals: Vec<Value> = constants.iter().map(|c| (*c).clone()).collect();
                    let const_product = if constants.is_empty() {
                        Value::Integer(Integer::from(1))
                    } else {
                        simplify_call("Times", &const_vals)
                    };
                    simplify_call("Times", &[const_product, var_part])
                } else {
                    call("Integrate", vec![expr.clone(), x])
                }
            }
            "Power" if args.len() == 2 && args[0].struct_eq(&x) => match &args[1] {
                Value::Integer(n) if *n == -1 => simplify_call("Log", &[x]),
                Value::Integer(n) => {
                    let new_exp: Integer = n.clone() + 1;
                    simplify_call(
                        "Times",
                        &[
                            simplify_call("Power", &[x, Value::Integer(new_exp.clone())]),
                            simplify_call(
                                "Power",
                                &[Value::Integer(new_exp), Value::Integer(Integer::from(-1))],
                            ),
                        ],
                    )
                }
                Value::Real(n) => {
                    let new_exp: Float = n.clone() + 1.0;
                    simplify_call(
                        "Times",
                        &[
                            simplify_call("Power", &[x, Value::Real(new_exp.clone())]),
                            simplify_call(
                                "Power",
                                &[Value::Real(new_exp), Value::Integer(Integer::from(-1))],
                            ),
                        ],
                    )
                }
                _ => call("Integrate", vec![expr.clone(), x]),
            },
            "Sin" if args.len() == 1 && args[0].struct_eq(&x) => simplify_call(
                "Times",
                &[
                    Value::Integer(Integer::from(-1)),
                    simplify_call("Cos", &[x]),
                ],
            ),
            "Cos" if args.len() == 1 && args[0].struct_eq(&x) => simplify_call("Sin", &[x]),
            "Exp" if args.len() == 1 && args[0].struct_eq(&x) => simplify_call("Exp", &[x]),
            "Tan" if args.len() == 1 && args[0].struct_eq(&x) => simplify_call(
                "Times",
                &[
                    Value::Integer(Integer::from(-1)),
                    simplify_call("Log", &[simplify_call("Cos", &[x])]),
                ],
            ),
            // ∫ sec²(x) dx = tan(x)
            "Power"
                if args.len() == 2
                    && args[0].struct_eq(&simplify_call("Sec", &[x.clone()]))
                    && args[1].struct_eq(&Value::Integer(Integer::from(2))) =>
            {
                simplify_call("Tan", &[x.clone()])
            }
            // ∫ csc²(x) dx = -cot(x)
            "Power"
                if args.len() == 2
                    && args[0].struct_eq(&simplify_call("Csc", &[x.clone()]))
                    && args[1].struct_eq(&Value::Integer(Integer::from(2))) =>
            {
                simplify_call(
                    "Times",
                    &[
                        Value::Integer(Integer::from(-1)),
                        simplify_call("Cot", &[x.clone()]),
                    ],
                )
            }
            // ∫ log(x) dx = x*log(x) - x
            "Log" if args.len() == 1 && args[0].struct_eq(&x) => simplify_call(
                "Plus",
                &[
                    simplify_call("Times", &[x.clone(), simplify_call("Log", &[x.clone()])]),
                    simplify_call("Times", &[Value::Integer(Integer::from(-1)), x.clone()]),
                ],
            ),
            // Linear substitution: ∫ f(a*x + b) dx = (1/a) * F(a*x + b)
            // where F is the antiderivative of f, and a, b are constants
            head @ ("Sin" | "Cos" | "Exp" | "Tan") if args.len() == 1 => {
                if let Some((a, b)) = try_extract_linear(&args[0], var)
                    && a != Value::Integer(Integer::from(0))
                {
                    // Compute ∫ f(u) du where u = a*x + b
                    let inner = Value::Call {
                        head: head.to_string(),
                        args: vec![Value::Symbol("u".to_string())],
                    };
                    let f_of_u = integrate(&inner, "u");
                    // Substitute back u = a*x + b
                    let mut result = substitute_var(
                        &f_of_u,
                        "u",
                        &Value::Call {
                            head: "Plus".to_string(),
                            args: vec![
                                simplify_call("Times", &[a.clone(), x.clone()]),
                                b.clone().unwrap_or(Value::Integer(Integer::from(0))),
                            ],
                        },
                    );
                    // Multiply by 1/a for the chain rule
                    let inv_a = simplify_call("Power", &[a, Value::Integer(Integer::from(-1))]);
                    result = simplify_call("Times", &[inv_a, result]);
                    return result;
                }
                call("Integrate", vec![expr.clone(), x])
            }
            _ => call("Integrate", vec![expr.clone(), x]),
        },
        _ => call("Integrate", vec![expr.clone(), x]),
    }
}

/// Try to extract a*x + b form from an expression.
/// Returns Some((a, Some(b))) for a*x + b, Some((a, None)) for a*x, None otherwise.
fn try_extract_linear(expr: &Value, var: &str) -> Option<(Value, Option<Value>)> {
    let x = Value::Symbol(var.to_string());
    match expr {
        Value::Symbol(s) if s == var => Some((Value::Integer(Integer::from(1)), None)),
        Value::Call { head, args } if head == "Times" && args.len() == 2 => {
            if args[0].struct_eq(&x) && is_constant_wrt(&args[1], var) {
                Some((args[1].clone(), None))
            } else if args[1].struct_eq(&x) && is_constant_wrt(&args[0], var) {
                Some((args[0].clone(), None))
            } else {
                None
            }
        }
        Value::Call { head, args } if head == "Plus" && args.len() == 2 => {
            // Check if one arg is a*x form and the other is constant
            let linear = try_extract_linear(&args[0], var);
            let constant = try_extract_linear(&args[1], var);
            match (linear, constant) {
                (Some((a, None)), _) if is_constant_wrt(&args[1], var) => {
                    Some((a, Some(args[1].clone())))
                }
                (_, Some((a, None))) if is_constant_wrt(&args[0], var) => {
                    Some((a, Some(args[0].clone())))
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// Substitute a variable with an expression in a value tree.
fn substitute_var(expr: &Value, old_var: &str, new_expr: &Value) -> Value {
    match expr {
        Value::Symbol(s) if s == old_var => new_expr.clone(),
        Value::Integer(_) | Value::Real(_) | Value::Bool(_) | Value::Str(_) | Value::Null => {
            expr.clone()
        }
        Value::Symbol(s) => Value::Symbol(s.clone()),
        Value::List(items) => Value::List(
            items
                .iter()
                .map(|i| substitute_var(i, old_var, new_expr))
                .collect(),
        ),
        Value::Call { head, args } => Value::Call {
            head: head.clone(),
            args: args
                .iter()
                .map(|a| substitute_var(a, old_var, new_expr))
                .collect(),
        },
        other => other.clone(),
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

// ── Factor ──

/// Factor[expr] — Polynomial factorization.
pub fn builtin_factor(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Factor requires exactly 1 argument".to_string(),
        ));
    }
    Ok(factor_expr(&args[0]))
}

/// Try to factor a polynomial expression.
fn factor_expr(expr: &Value) -> Value {
    // Find the variable (first non-constant symbol)
    let var = match find_polynomial_var(expr) {
        Some(v) => v,
        None => return expr.clone(),
    };
    let coeffs = extract_polynomial_coeffs(expr, &var);
    if coeffs.is_empty() {
        return expr.clone();
    }
    match coeffs.len() - 1 {
        0 => coeffs[0].clone(),
        1 => factor_linear(&coeffs, &var),
        2 => factor_quadratic(&coeffs, &var),
        _ => factor_by_gcd(&coeffs, &var),
    }
}

/// Find the polynomial variable in an expression.
fn find_polynomial_var(expr: &Value) -> Option<String> {
    match expr {
        Value::Symbol(s) if !is_known_constant(s) => Some(s.clone()),
        Value::Call { head, args } => {
            if head == "Plus" || head == "Times" {
                for arg in args {
                    if let Some(v) = find_polynomial_var(arg) {
                        return Some(v);
                    }
                }
            }
            if head == "Power"
                && args.len() == 2
                && let Some(v) = find_polynomial_var(&args[0])
            {
                return Some(v);
            }
            None
        }
        _ => None,
    }
}

fn is_known_constant(s: &str) -> bool {
    matches!(s, "Pi" | "E" | "I" | "True" | "False" | "Null" | "Degree")
}

fn factor_linear(coeffs: &[Value], var: &str) -> Value {
    let a = &coeffs[0];
    let b = &coeffs[1];
    let x = Value::Symbol(var.to_string());
    if let (Value::Integer(ai), Value::Integer(bi)) = (a, b) {
        let gcd = ai.clone().gcd(bi);
        if gcd > 1 {
            let new_a = Value::Call {
                head: "Plus".to_string(),
                args: vec![
                    Value::Integer(ai.clone() / &gcd),
                    Value::Call {
                        head: "Times".to_string(),
                        args: vec![Value::Integer(bi.clone() / &gcd), x],
                    },
                ],
            };
            return Value::Call {
                head: "Times".to_string(),
                args: vec![Value::Integer(gcd), new_a],
            };
        }
    }
    Value::Call {
        head: "Plus".to_string(),
        args: vec![
            a.clone(),
            Value::Call {
                head: "Times".to_string(),
                args: vec![b.clone(), x],
            },
        ],
    }
}

fn factor_quadratic(coeffs: &[Value], var: &str) -> Value {
    let c = &coeffs[0];
    let b = &coeffs[1];
    let a = &coeffs[2];
    let x = Value::Symbol(var.to_string());

    if let (Value::Integer(ai), Value::Integer(bi), Value::Integer(ci)) = (a, b, c)
        && !ai.is_zero()
    {
        // Discriminant: b^2 - 4ac
        let b_sq: Integer = (bi * bi).into();
        let four_ac: Integer = Integer::from(4) * ai * ci;
        let disc: Integer = b_sq - four_ac;
        if !disc.is_negative() {
            let sqrt_disc = disc.clone().sqrt();
            let sqrt_disc_sq: Integer = (&sqrt_disc * &sqrt_disc).into();
            if sqrt_disc_sq == disc {
                let two_a: Integer = Integer::from(2) * ai;
                let neg_bi: Integer = (-bi).into();
                let r1_num: Integer = (&neg_bi + &sqrt_disc).into();
                let r2_num: Integer = (&neg_bi - &sqrt_disc).into();
                if r1_num.is_divisible(&two_a) && r2_num.is_divisible(&two_a) {
                    let r1: Integer = r1_num / &two_a;
                    let r2: Integer = r2_num / &two_a;
                    let factor1 = Value::Call {
                        head: "Plus".to_string(),
                        args: vec![x.clone(), Value::Integer(-r1)],
                    };
                    let factor2 = Value::Call {
                        head: "Plus".to_string(),
                        args: vec![x, Value::Integer(-r2)],
                    };
                    if *ai == 1 {
                        return Value::Call {
                            head: "Times".to_string(),
                            args: vec![factor1, factor2],
                        };
                    } else {
                        return Value::Call {
                            head: "Times".to_string(),
                            args: vec![Value::Integer(ai.clone()), factor1, factor2],
                        };
                    }
                }
            }
        }
    }
    expr_from_coeffs(coeffs, var)
}

fn factor_by_gcd(coeffs: &[Value], var: &str) -> Value {
    let integers: Vec<&Integer> = coeffs
        .iter()
        .filter_map(|c| match c {
            Value::Integer(n) => Some(n),
            _ => None,
        })
        .collect();
    if integers.len() != coeffs.len() || integers.is_empty() {
        return expr_from_coeffs(coeffs, var);
    }
    let mut gcd = integers[0].clone();
    for n in &integers[1..] {
        gcd = gcd.gcd(n);
    }
    if gcd <= 1 {
        return expr_from_coeffs(coeffs, var);
    }
    let new_coeffs: Vec<Value> = coeffs
        .iter()
        .map(|c| match c {
            Value::Integer(n) => Value::Integer(n.clone() / &gcd),
            other => other.clone(),
        })
        .collect();
    let inner = expr_from_coeffs(&new_coeffs, var);
    Value::Call {
        head: "Times".to_string(),
        args: vec![Value::Integer(gcd), inner],
    }
}

fn expr_from_coeffs(coeffs: &[Value], var: &str) -> Value {
    let x = Value::Symbol(var.to_string());
    let mut terms = Vec::new();
    for (i, coeff) in coeffs.iter().enumerate() {
        if matches!(coeff, Value::Integer(n) if n.is_zero()) {
            continue;
        }
        let term = match i {
            0 => coeff.clone(),
            1 => Value::Call {
                head: "Times".to_string(),
                args: vec![coeff.clone(), x.clone()],
            },
            _ => Value::Call {
                head: "Times".to_string(),
                args: vec![
                    coeff.clone(),
                    Value::Call {
                        head: "Power".to_string(),
                        args: vec![x.clone(), Value::Integer(Integer::from(i))],
                    },
                ],
            },
        };
        terms.push(term);
    }
    if terms.is_empty() {
        Value::Integer(Integer::from(0))
    } else if terms.len() == 1 {
        terms.into_iter().next().unwrap()
    } else {
        Value::Call {
            head: "Plus".to_string(),
            args: terms,
        }
    }
}

// ── Solve ──

/// Strip Value::Pattern wrapper (from HoldAll) if present, recursively.
fn strip_pattern(v: &Value) -> Value {
    match v {
        Value::Pattern(unevaluated) => expr_to_value(unevaluated),
        Value::List(items) => Value::List(items.iter().map(|item| strip_pattern(item)).collect()),
        _ => v.clone(),
    }
}

/// Convert an AST Expr to a Value without evaluation.
fn expr_to_value(expr: &crate::ast::Expr) -> Value {
    match expr {
        crate::ast::Expr::Integer(n) => Value::Integer(n.clone()),
        crate::ast::Expr::Real(r) => Value::Real(r.clone()),
        crate::ast::Expr::Bool(b) => Value::Bool(*b),
        crate::ast::Expr::Str(s) => Value::Str(s.clone()),
        crate::ast::Expr::Null => Value::Null,
        crate::ast::Expr::Symbol(s) => Value::Symbol(s.clone()),
        crate::ast::Expr::List(items) => Value::List(items.iter().map(expr_to_value).collect()),
        crate::ast::Expr::Call { head, args } => {
            let head_str = match head.as_ref() {
                crate::ast::Expr::Symbol(s) => s.clone(),
                _ => String::new(),
            };
            Value::Call {
                head: head_str,
                args: args.iter().map(expr_to_value).collect(),
            }
        }
        _ => Value::Pattern(expr.clone()),
    }
}

/// Solve[equation, x] — Symbolic equation solving.
pub fn builtin_solve(args: &[Value]) -> Result<Value, EvalError> {
    let args: Vec<Value> = args.iter().map(strip_pattern).collect();
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Solve requires exactly 2 arguments".to_string(),
        ));
    }
    let var = match &args[1] {
        Value::Symbol(s) => s.clone(),
        _ => {
            return Err(EvalError::TypeError {
                expected: "Symbol".to_string(),
                got: args[1].type_name().to_string(),
            });
        }
    };
    let (lhs, rhs) = match &args[0] {
        Value::Call {
            head,
            args: eq_args,
        } if head == "Equal" && eq_args.len() == 2 => (eq_args[0].clone(), eq_args[1].clone()),
        _ => {
            return Ok(Value::Call {
                head: "Solve".to_string(),
                args: args.to_vec(),
            });
        }
    };
    let poly = simplify_call(
        "Plus",
        &[
            lhs,
            simplify_call("Times", &[Value::Integer(Integer::from(-1)), rhs]),
        ],
    );
    Ok(solve_polynomial(&poly, &var))
}

fn solve_polynomial(expr: &Value, var: &str) -> Value {
    let coeffs = extract_polynomial_coeffs(expr, var);
    match coeffs.len() {
        2 => {
            let (b, a) = (&coeffs[0], &coeffs[1]);
            match (a, b) {
                (Value::Integer(ai), Value::Integer(bi)) => {
                    if ai.is_zero() {
                        return Value::List(vec![]);
                    }
                    let result = Float::with_val(DEFAULT_PRECISION, bi)
                        / Float::with_val(DEFAULT_PRECISION, ai);
                    let neg_result = -result;
                    Value::List(vec![Value::Rule {
                        lhs: Box::new(Value::Symbol(var.to_string())),
                        rhs: Box::new(Value::Real(neg_result)),
                        delayed: false,
                    }])
                }
                _ => Value::List(vec![Value::Rule {
                    lhs: Box::new(Value::Symbol(var.to_string())),
                    rhs: Box::new(simplify_call(
                        "Times",
                        &[
                            Value::Integer(Integer::from(-1)),
                            simplify_call("Power", &[a.clone(), Value::Integer(Integer::from(-1))]),
                            b.clone(),
                        ],
                    )),
                    delayed: false,
                }]),
            }
        }
        3 => {
            let (c, b, a) = (&coeffs[0], &coeffs[1], &coeffs[2]);
            match (a, b, c) {
                (Value::Integer(ai), Value::Integer(bi), Value::Integer(ci)) => {
                    let disc = bi * bi - Integer::from(4) * ai * ci;
                    if disc < 0 {
                        return Value::List(vec![]);
                    }
                    let disc_f = Float::with_val(DEFAULT_PRECISION, &disc);
                    let sqrt_disc = disc_f.sqrt();
                    let bi_f = Float::with_val(DEFAULT_PRECISION, bi);
                    let ai_f = Float::with_val(DEFAULT_PRECISION, ai);
                    let two = Float::with_val(DEFAULT_PRECISION, 2);
                    let x1 = (-bi_f.clone() + sqrt_disc.clone()) / (two.clone() * ai_f.clone());
                    let x2 = (-bi_f - sqrt_disc) / (two * ai_f);
                    if disc.is_zero() {
                        Value::List(vec![Value::Rule {
                            lhs: Box::new(Value::Symbol(var.to_string())),
                            rhs: Box::new(Value::Real(x1)),
                            delayed: false,
                        }])
                    } else {
                        Value::List(vec![
                            Value::Rule {
                                lhs: Box::new(Value::Symbol(var.to_string())),
                                rhs: Box::new(Value::Real(x1)),
                                delayed: false,
                            },
                            Value::Rule {
                                lhs: Box::new(Value::Symbol(var.to_string())),
                                rhs: Box::new(Value::Real(x2)),
                                delayed: false,
                            },
                        ])
                    }
                }
                _ => call(
                    "Solve",
                    vec![
                        simplify_call("Equal", &[expr.clone(), Value::Integer(Integer::from(0))]),
                        Value::Symbol(var.to_string()),
                    ],
                ),
            }
        }
        4 => solve_cubic(&coeffs, var),
        5 => solve_quartic(&coeffs, var),
        _ => call(
            "Solve",
            vec![
                simplify_call("Equal", &[expr.clone(), Value::Integer(Integer::from(0))]),
                Value::Symbol(var.to_string()),
            ],
        ),
    }
}

/// Build a Rational value from numerator and denominator Integers.
fn make_rat(num: &Integer, den: &Integer) -> Value {
    Value::Rational(Box::new(Rational::from((num.clone(), den.clone()))))
}

/// Solve cubic ax^3 + bx^2 + cx + d = 0 using Cardano's formula.
/// Returns exact symbolic expressions using Power[..., Rational[1,3]] for cube roots.
fn solve_cubic(coeffs: &[Value], var: &str) -> Value {
    let (d, c, b, a) = (&coeffs[0], &coeffs[1], &coeffs[2], &coeffs[3]);
    match (a, b, c, d) {
        (Value::Integer(ai), Value::Integer(bi), Value::Integer(ci), Value::Integer(di)) => {
            if ai.is_zero() {
                return Value::List(vec![]);
            }

            // Depress the cubic: x = t - b/(3a) => t^3 + pt + q = 0
            // p = (3ac - b^2) / (3a^2)
            // q = (2b^3 - 9abc + 27a^2*d) / (27a^3)
            let mut three_a = Integer::from(3);
            three_a *= ai;
            let mut three_a_sq = three_a.clone();
            three_a_sq *= ai;
            let mut p_num = Integer::from(3);
            p_num *= ai;
            p_num *= ci;
            let mut bi_sq = bi.clone();
            bi_sq *= bi;
            p_num -= &bi_sq;
            let p = make_rat(&p_num, &three_a_sq);

            let mut twenty_seven_a_cub = Integer::from(27);
            twenty_seven_a_cub *= ai;
            twenty_seven_a_cub *= ai;
            twenty_seven_a_cub *= ai;
            let mut q_num = Integer::from(2);
            q_num *= bi;
            q_num *= bi;
            q_num *= bi;
            let mut term2 = Integer::from(9);
            term2 *= ai;
            term2 *= bi;
            term2 *= ci;
            q_num -= &term2;
            let mut term3 = Integer::from(27);
            term3 *= ai;
            term3 *= ai;
            term3 *= di;
            q_num += &term3;
            let q = make_rat(&q_num, &twenty_seven_a_cub);

            // Cardano: t = u + v, where
            // u^3 = -q/2 + sqrt(q^2/4 + p^3/27), v^3 = -q/2 - sqrt(q^2/4 + p^3/27)
            //
            // D = q^2/4 + p^3/27
            //   = (27 * q_num^2 * p_den^3 + 4 * p_num^3 * q_den^2) / (108 * q_den^2 * p_den^3)

            let Value::Rational(p_rat) = &p else {
                unreachable!()
            };
            let Value::Rational(q_rat) = &q else {
                unreachable!()
            };

            let p_num_r = p_rat.numer();
            let p_den_r = p_rat.denom();
            let q_num_r = q_rat.numer();
            let q_den_r = q_rat.denom();

            let p_den_cub = p_den_r.clone() * p_den_r.clone() * p_den_r.clone();
            let q_den_sq = q_den_r.clone() * q_den_r.clone();
            let p_num_cub = p_num_r.clone() * p_num_r.clone() * p_num_r.clone();
            let q_num_sq = q_num_r.clone() * q_num_r.clone();

            let disc_num = Integer::from(27) * &q_num_sq * &p_den_cub
                + Integer::from(4) * &p_num_cub * &q_den_sq;
            let disc_den = Integer::from(108) * &q_den_sq * &p_den_cub;

            // Build symbolic cube root arguments
            // neg_q_half = -q_num / (2 * q_den)
            let mut two_q_den = Integer::from(2);
            two_q_den *= q_den_r.clone();
            let neg_q_half = make_rat(&(-q_num_r.clone()), &two_q_den);

            // sqrt_arg = disc_num / disc_den
            let sqrt_arg = make_rat(&disc_num, &disc_den);

            // u^3 = neg_q_half + Sqrt[D], v^3 = neg_q_half - Sqrt[D]
            let u_cub = simplify_call(
                "Plus",
                &[neg_q_half.clone(), call("Sqrt", vec![sqrt_arg.clone()])],
            );
            let v_cub = simplify_call(
                "Plus",
                &[
                    neg_q_half.clone(),
                    simplify_call(
                        "Times",
                        &[
                            Value::Integer(Integer::from(-1)),
                            call("Sqrt", vec![sqrt_arg]),
                        ],
                    ),
                ],
            );

            // Cube roots: Power[u_cub, 1/3] and Power[v_cub, 1/3]
            let one_third = make_rat(&Integer::from(1), &Integer::from(3));
            let u_root = call("Power", vec![u_cub, one_third.clone()]);
            let v_root = call("Power", vec![v_cub, one_third.clone()]);

            // Shift: x = t - b/(3a) => x = t + shift, shift = -b/(3a)
            let shift = make_rat(&(-bi.clone()), &three_a);

            // Root 1: x1 = shift + u + v
            let root1 = simplify_call("Plus", &[shift.clone(), u_root.clone(), v_root.clone()]);

            // Cube roots of unity:
            // omega = -1/2 + Sqrt[-3]/2
            // omega^2 = -1/2 - Sqrt[-3]/2
            let neg_one_half = make_rat(&Integer::from(-1), &Integer::from(2));
            let sqrt_neg3 = call("Sqrt", vec![Value::Integer(Integer::from(-3))]);
            let half_inv = call(
                "Power",
                vec![
                    Value::Integer(Integer::from(2)),
                    make_rat(&Integer::from(-1), &Integer::from(1)),
                ],
            );
            let omega = simplify_call(
                "Plus",
                &[
                    neg_one_half.clone(),
                    simplify_call("Times", &[half_inv.clone(), sqrt_neg3.clone()]),
                ],
            );

            let omega_sq = simplify_call(
                "Plus",
                &[
                    neg_one_half.clone(),
                    simplify_call(
                        "Times",
                        &[
                            Value::Integer(Integer::from(-1)),
                            simplify_call("Times", &[half_inv, sqrt_neg3]),
                        ],
                    ),
                ],
            );

            // Root 2: x2 = shift + u*omega + v*omega^2
            let root2 = simplify_call(
                "Plus",
                &[
                    shift.clone(),
                    simplify_call("Times", &[u_root.clone(), omega.clone()]),
                    simplify_call("Times", &[v_root.clone(), omega_sq.clone()]),
                ],
            );

            // Root 3: x3 = shift + u*omega^2 + v*omega
            let root3 = simplify_call(
                "Plus",
                &[
                    shift,
                    simplify_call("Times", &[u_root, omega_sq]),
                    simplify_call("Times", &[v_root, omega]),
                ],
            );

            let x_sym = Value::Symbol(var.to_string());
            Value::List(vec![
                Value::Rule {
                    lhs: Box::new(x_sym.clone()),
                    rhs: Box::new(root1),
                    delayed: false,
                },
                Value::Rule {
                    lhs: Box::new(x_sym.clone()),
                    rhs: Box::new(root2),
                    delayed: false,
                },
                Value::Rule {
                    lhs: Box::new(x_sym),
                    rhs: Box::new(root3),
                    delayed: false,
                },
            ])
        }
        _ => Value::List(vec![]),
    }
}

/// Solve quartic ax^4 + bx^3 + cx^2 + dx + e = 0 using Ferrari's method.
/// Returns numeric results.
fn solve_quartic(coeffs: &[Value], var: &str) -> Value {
    let (e, d, c, b, a) = (&coeffs[0], &coeffs[1], &coeffs[2], &coeffs[3], &coeffs[4]);
    match (a, b, c, d, e) {
        (
            Value::Integer(ai),
            Value::Integer(bi),
            Value::Integer(ci),
            Value::Integer(di),
            Value::Integer(ei),
        ) => {
            if ai.is_zero() {
                return Value::List(vec![]);
            }

            let a = ai.to_f64();
            let b = bi.to_f64();
            let c = ci.to_f64();
            let d = di.to_f64();
            let e = ei.to_f64();

            // Depress quartic: x = t - b/(4a)
            // t^4 + p*t^2 + q*t + r = 0
            let p = c / a - 6.0 * (b / a).powi(2) / 4.0;
            let q = d / a + 3.0 * (b / a).powi(3) / 8.0 - b * c / (2.0 * a * a);
            let r = e / a - 3.0 * (b / a).powi(4) / 256.0 + (b / a).powi(2) * c / (16.0 * a)
                - b * d / (8.0 * a * a);

            // Resolvent cubic: m^3 + 2p*m^2 + (p^2 - 4r)*m - q^2 = 0
            // Substitute m = y - 2p/3 to depress:
            // y^3 + Py + Q = 0
            let resolvent_p = p * p / 3.0 - 4.0 * r;
            let resolvent_q = 2.0 * p * p * p / 27.0 - p * (p * p - 4.0 * r) / 3.0 - q * q;

            // Cardano: resolvent_disc = resolvent_q^2/4 + resolvent_p^3/27
            let resolvent_disc =
                resolvent_q * resolvent_q / 4.0 + resolvent_p * resolvent_p * resolvent_p / 27.0;

            if resolvent_disc >= 0.0 {
                let u_cub = -resolvent_q / 2.0 + resolvent_disc.sqrt();
                let v_cub = -resolvent_q / 2.0 - resolvent_disc.sqrt();
                let u = u_cub.copysign(1.0).abs().powf(1.0 / 3.0);
                let v = v_cub.copysign(1.0).abs().powf(1.0 / 3.0);
                let y = u + v;
                let m = y - 2.0 * p / 3.0;

                if m < 0.0 {
                    return Value::List(vec![]);
                }

                let sqrt_m = m.sqrt();

                // Two quadratics:
                // t^2 + sqrt(m)*t + (m/2 + p/sqrt(m)) = 0  [sign chosen to match -q]
                // t^2 - sqrt(m)*t + (m/2 - p/sqrt(m)) = 0
                let q_sign = if q >= 0.0 { 1.0 } else { -1.0 };
                let s1 = q_sign * sqrt_m;
                let s2 = -q_sign * sqrt_m;
                let c1 = m / 2.0 + p / sqrt_m;
                let c2 = m / 2.0 - p / sqrt_m;

                // Quadratic 1: t^2 + s1*t + c1 = 0
                // Quadratic 2: t^2 + s2*t + c2 = 0

                let mut roots: Vec<Value> = Vec::new();

                for (s, c) in [(s1, c1), (s2, c2)] {
                    let disc = s * s - 4.0 * c;
                    if disc >= 0.0 {
                        let sqrt_disc = disc.sqrt();
                        let t1 = (-s + sqrt_disc) / 2.0;
                        let t2 = (-s - sqrt_disc) / 2.0;
                        let shift = -b / (4.0 * a);
                        let x1 = t1 + shift;
                        let x2 = t2 + shift;
                        if disc > 1e-12 {
                            roots.push(Value::Real(Float::with_val(DEFAULT_PRECISION, x1)));
                            roots.push(Value::Real(Float::with_val(DEFAULT_PRECISION, x2)));
                        } else {
                            roots.push(Value::Real(Float::with_val(DEFAULT_PRECISION, x1)));
                        }
                    }
                }

                let x_sym = Value::Symbol(var.to_string());
                Value::List(
                    roots
                        .into_iter()
                        .map(|r| Value::Rule {
                            lhs: Box::new(x_sym.clone()),
                            rhs: Box::new(r),
                            delayed: false,
                        })
                        .collect(),
                )
            } else {
                // resolvent_disc < 0: casus irreducibilis for resolvent cubic
                // Try trigonometric method for the resolvent cubic
                // y^3 + resolvent_p*y + resolvent_q = 0 with resolvent_disc < 0
                let mut roots: Vec<Value> = Vec::new();
                let r_trig = ((-resolvent_q / 2.0) / (-resolvent_p / 3.0).powf(1.5))
                    .abs()
                    .min(1.0);
                let theta = (r_trig).acos();

                for k in 0..3 {
                    let y = 2.0
                        * (-resolvent_p / 3.0).sqrt()
                        * ((theta as f64 - 2.0 * std::f64::consts::PI * (k as f64) / 3.0).cos());
                    let m = y - 2.0 * p / 3.0;

                    if m < 1e-12 {
                        continue;
                    }

                    let sqrt_m = m.sqrt();
                    let c1 = m / 2.0 + p / sqrt_m;
                    let c2 = m / 2.0 - p / sqrt_m;
                    let q_sign = if q >= 0.0 { 1.0 } else { -1.0 };
                    let s1 = q_sign * sqrt_m;
                    let s2 = -q_sign * sqrt_m;

                    for (s, c) in [(s1, c1), (s2, c2)] {
                        let disc = s * s - 4.0 * c;
                        if disc >= 0.0 {
                            let sqrt_disc = disc.sqrt();
                            let t1 = (-s + sqrt_disc) / 2.0;
                            let t2 = (-s - sqrt_disc) / 2.0;
                            let shift = -b / (4.0 * a);
                            let x1 = t1 + shift;
                            let x2 = t2 + shift;
                            if disc > 1e-12 {
                                roots.push(Value::Real(Float::with_val(DEFAULT_PRECISION, x1)));
                                roots.push(Value::Real(Float::with_val(DEFAULT_PRECISION, x2)));
                            } else {
                                roots.push(Value::Real(Float::with_val(DEFAULT_PRECISION, x1)));
                            }
                        }
                    }

                    // Return first valid factorization found
                    if !roots.is_empty() {
                        let x_sym = Value::Symbol(var.to_string());
                        return Value::List(
                            roots
                                .into_iter()
                                .map(|r| Value::Rule {
                                    lhs: Box::new(x_sym.clone()),
                                    rhs: Box::new(r),
                                    delayed: false,
                                })
                                .collect(),
                        );
                    }
                }

                Value::List(vec![])
            }
        }
        _ => Value::List(vec![]),
    }
}

pub fn extract_polynomial_coeffs(expr: &Value, var: &str) -> Vec<Value> {
    let terms = flatten_to_plus_terms(expr);
    let mut max_degree = 0i64;
    let mut coeff_map: std::collections::HashMap<i64, Value> = std::collections::HashMap::new();
    for term in &terms {
        let (coeff, degree) = extract_term_coeff_degree(term, var);
        if degree >= 0 {
            max_degree = max_degree.max(degree);
            let existing = coeff_map
                .remove(&degree)
                .unwrap_or(Value::Integer(Integer::from(0)));
            coeff_map.insert(degree, simplify_call("Plus", &[existing, coeff]));
        }
    }
    let mut result = Vec::new();
    for d in 0..=max_degree {
        result.push(
            coeff_map
                .remove(&d)
                .unwrap_or(Value::Integer(Integer::from(0))),
        );
    }
    result
}

fn flatten_to_plus_terms(expr: &Value) -> Vec<Value> {
    match expr {
        Value::Call { head, args } if head == "Plus" => {
            let mut result = Vec::new();
            for arg in args {
                result.extend(flatten_to_plus_terms(arg));
            }
            result
        }
        _ => vec![expr.clone()],
    }
}

fn extract_term_coeff_degree(term: &Value, var: &str) -> (Value, i64) {
    match term {
        Value::Symbol(s) if s == var => (Value::Integer(Integer::from(1)), 1),
        Value::Symbol(_) | Value::Integer(_) | Value::Real(_) => {
            if is_constant_wrt(term, var) {
                (term.clone(), 0)
            } else {
                (Value::Integer(Integer::from(0)), -1)
            }
        }
        Value::Call { head, args } => match head.as_str() {
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
                    Value::Integer(n) => {
                        (Value::Integer(Integer::from(1)), n.to_i64().unwrap_or(0))
                    }
                    _ => (Value::Integer(Integer::from(0)), -1),
                }
            }
            _ => {
                if is_constant_wrt(term, var) {
                    (term.clone(), 0)
                } else {
                    (Value::Integer(Integer::from(0)), -1)
                }
            }
        },
        _ => (Value::Integer(Integer::from(0)), -1),
    }
}

// ── Series ──

/// Series[expr, {x, x0, n}] — Taylor series expansion.
pub fn builtin_series(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Series requires exactly 2 arguments".to_string(),
        ));
    }
    let spec = match &args[1] {
        Value::List(items) => items,
        _ => {
            return Err(EvalError::TypeError {
                expected: "List".to_string(),
                got: args[1].type_name().to_string(),
            });
        }
    };
    if spec.len() != 3 {
        return Err(EvalError::Error(
            "Series spec must be {x, x0, n}".to_string(),
        ));
    }
    let var = match &spec[0] {
        Value::Symbol(s) => s.clone(),
        _ => {
            return Err(EvalError::TypeError {
                expected: "Symbol".to_string(),
                got: spec[0].type_name().to_string(),
            });
        }
    };
    let x0 = spec[1].clone();
    let order = spec[2].to_integer().ok_or_else(|| EvalError::TypeError {
        expected: "Integer".to_string(),
        got: spec[2].type_name().to_string(),
    })?;

    let _x_sym = Value::Symbol(var.clone());
    let mut coefficients: Vec<Value> = Vec::new();
    let mut derivative = args[0].clone();

    for n in 0..=order {
        let coeff_val = substitute_and_eval(&derivative, &var, &x0);
        let factorial_val = Value::Integer(super::math::factorial(n));
        let coeff = match (&coeff_val, &factorial_val) {
            (Value::Integer(c), Value::Integer(f)) if !f.is_zero() => {
                Value::Rational(Box::new(Rational::from((c.clone(), f.clone()))))
            }
            (Value::Real(c), Value::Integer(f)) if !f.is_zero() => {
                let f_f = Float::with_val(DEFAULT_PRECISION, f);
                Value::Real(c / f_f)
            }
            _ => simplify_call(
                "Times",
                &[
                    coeff_val,
                    simplify_call("Power", &[factorial_val, Value::Integer(Integer::from(-1))]),
                ],
            ),
        };
        let coeff = combine_plus_terms(coeff);
        coefficients.push(coeff);
        derivative = crate::eval::apply_function(
            &Value::Builtin("D".to_string(), crate::value::BuiltinFn::Env(builtin_d)),
            &[derivative.clone(), Value::Symbol(var.clone())],
            env,
        )
        .unwrap_or(Value::Call {
            head: "D".to_string(),
            args: vec![derivative.clone(), Value::Symbol(var.clone())],
        });
    }

    Ok(Value::SeriesData {
        variable: Box::new(Value::Symbol(var)),
        expansion_point: Box::new(x0),
        coefficients,
        min_exponent: 0,
        max_exponent: order as i32 + 1,
        denominator: 1,
    })
}

/// Extract numeric coefficient and variable part from a term.
/// Used by combine_plus_terms to group like terms.
fn split_coeff_var(term: &Value) -> (Integer, Value) {
    match term {
        Value::Integer(n) => (n.clone(), Value::Integer(Integer::from(1))),
        Value::Call { head, args } if head == "Times" => {
            let mut coeff = Integer::from(1);
            let mut vars: Vec<Value> = Vec::new();
            for arg in args {
                if let Value::Integer(n) = arg {
                    coeff *= n;
                } else {
                    vars.push(arg.clone());
                }
            }
            let var_part = if vars.is_empty() {
                Value::Integer(Integer::from(1))
            } else if vars.len() == 1 {
                vars.into_iter().next().unwrap()
            } else {
                Value::Call {
                    head: "Times".to_string(),
                    args: vars,
                }
            };
            (coeff, var_part)
        }
        _ => (Integer::from(1), term.clone()),
    }
}

/// Recursively combine like terms in all Plus nodes.
/// Expands Times-over-Plus (distribution) so like terms at same level collect.
fn combine_plus_terms(val: Value) -> Value {
    match val {
        Value::Call { head, args } if head == "Plus" => {
            let expanded: Vec<Value> = args.into_iter().flat_map(expand_times_over_plus).collect();
            let mut groups: Vec<(Value, Integer)> = Vec::new();
            for term in expanded {
                let term = combine_plus_terms(term);
                let (coeff, var_part) = split_coeff_var(&term);
                if let Some((_, sum)) = groups.iter_mut().find(|(v, _)| *v == var_part) {
                    *sum += &coeff;
                } else {
                    groups.push((var_part, coeff));
                }
            }
            let mut result: Vec<Value> = Vec::new();
            for (var_part, coeff) in groups {
                if coeff.is_zero() {
                    continue;
                }
                let term = if var_part == Value::Integer(Integer::from(1)) {
                    Value::Integer(coeff)
                } else if coeff == 1 {
                    var_part
                } else {
                    Value::Call {
                        head: "Times".to_string(),
                        args: vec![Value::Integer(coeff), var_part],
                    }
                };
                result.push(term);
            }
            if result.is_empty() {
                Value::Integer(Integer::from(0))
            } else if result.len() == 1 {
                result.into_iter().next().unwrap()
            } else {
                Value::Call {
                    head: "Plus".to_string(),
                    args: result,
                }
            }
        }
        Value::Call { head, args } => Value::Call {
            head,
            args: args.into_iter().map(combine_plus_terms).collect(),
        },
        _ => val,
    }
}

/// Distribute Times over Plus: Times[c, Plus[a, b]] -> [c*a, c*b].
/// Called during combine_plus_terms to expose like terms.
fn expand_times_over_plus(val: Value) -> Vec<Value> {
    match &val {
        Value::Call { head, args } if head == "Times" => {
            for (i, arg) in args.iter().enumerate() {
                if let Value::Call {
                    head: h,
                    args: plus_args,
                } = arg
                    && h == "Plus"
                {
                    let factor_args: Vec<Value> = args
                        .iter()
                        .enumerate()
                        .filter(|(j, _)| *j != i)
                        .map(|(_, v)| v.clone())
                        .collect();
                    let factor = if factor_args.len() == 1 {
                        factor_args.into_iter().next().unwrap()
                    } else {
                        Value::Call {
                            head: "Times".to_string(),
                            args: factor_args,
                        }
                    };
                    return plus_args
                        .iter()
                        .map(|pa| simplify_call("Times", &[factor.clone(), pa.clone()]))
                        .collect();
                }
            }
            vec![val]
        }
        _ => vec![val],
    }
}

fn substitute_and_eval(expr: &Value, var: &str, val: &Value) -> Value {
    match expr {
        Value::Symbol(s) if s == var => val.clone(),
        Value::Symbol(_)
        | Value::Integer(_)
        | Value::Real(_)
        | Value::Bool(_)
        | Value::Str(_)
        | Value::Null => expr.clone(),
        Value::Call { head, args } => {
            let new_args: Vec<Value> = args
                .iter()
                .map(|a| substitute_and_eval(a, var, val))
                .collect();
            let result = simplify_call(head, &new_args);
            match &result {
                Value::Call { head: h, args: a } if h == head => {
                    try_numerical_eval(head, a).unwrap_or(result)
                }
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
                    Value::Real(r) => {
                        sum += r;
                        all_int = false;
                    }
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
                    Value::Real(r) => {
                        product *= r;
                        all_int = false;
                    }
                    _ => return None,
                }
            }
            if all_int && product.is_integer() {
                let i = product.to_f64() as i64;
                return Some(Value::Integer(Integer::from(i)));
            }
            Some(Value::Real(product))
        }
        "Power" if args.len() == 2 => match (&args[0], &args[1]) {
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
        },
        "Sin" if args.len() == 1 => match &args[0] {
            Value::Integer(n) => {
                let f = Float::with_val(DEFAULT_PRECISION, n);
                Some(Value::Real(f.sin()))
            }
            Value::Real(r) => Some(Value::Real(r.clone().sin())),
            _ => None,
        },
        "Cos" if args.len() == 1 => match &args[0] {
            Value::Integer(n) => {
                let f = Float::with_val(DEFAULT_PRECISION, n);
                Some(Value::Real(f.cos()))
            }
            Value::Real(r) => Some(Value::Real(r.clone().cos())),
            _ => None,
        },
        "Exp" if args.len() == 1 => match &args[0] {
            Value::Integer(n) => {
                let f = Float::with_val(DEFAULT_PRECISION, n);
                Some(Value::Real(f.exp()))
            }
            Value::Real(r) => Some(Value::Real(r.clone().exp())),
            _ => None,
        },
        "Log" if args.len() == 1 => match &args[0] {
            Value::Integer(n) if !n.is_zero() && !n.is_negative() => {
                let f = Float::with_val(DEFAULT_PRECISION, n);
                Some(Value::Real(f.ln()))
            }
            Value::Real(r) if !r.is_zero() && !r.is_sign_negative() => {
                Some(Value::Real(r.clone().ln()))
            }
            _ => None,
        },
        _ => None,
    }
}

fn extract_symbol_from(val: &Value) -> Result<String, EvalError> {
    let val = match val {
        Value::Hold(inner) | Value::HoldComplete(inner) => inner.as_ref(),
        _ => val,
    };
    match val {
        Value::Symbol(s) | Value::Str(s) => Ok(s.clone()),
        Value::Builtin(name, _) => Ok(name.clone()),
        Value::Function(fd) => Ok(fd.name.clone()),
        Value::Pattern(p) => match p {
            Expr::Symbol(s) => Ok(s.clone()),
            _ => Err(EvalError::TypeError {
                expected: "Symbol or String".to_string(),
                got: val.type_name().to_string(),
            }),
        },
        _ => Err(EvalError::TypeError {
            expected: "Symbol or String".to_string(),
            got: val.type_name().to_string(),
        }),
    }
}

/// SetAttributes[sym, attr1, attr2, ...] — set attributes on a symbol.
pub fn builtin_set_attributes(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "SetAttributes requires at least 2 arguments: symbol and attributes".to_string(),
        ));
    }
    let sym_name = extract_symbol_from(&args[0])?;
    // Locked attribute prevents modification
    if env.has_attribute(&sym_name, "Locked") {
        return Ok(Value::Null);
    }
    let attrs: Vec<String> = args[1..].iter().map(|a| a.to_string()).collect();
    env.set_attributes(&sym_name, attrs);
    Ok(Value::Null)
}

pub fn builtin_clear_attributes(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Err(EvalError::Error(
            "ClearAttributes requires at least 1 argument: symbol and optionally attributes to clear"
                .to_string(),
        ));
    }
    let sym_name = extract_symbol_from(&args[0])?;
    // Locked attribute prevents modification
    if env.has_attribute(&sym_name, "Locked") {
        return Ok(Value::Null);
    }
    if args.len() == 1 {
        // Clear all attributes
        env.clear_attributes(&sym_name);
    } else {
        // Clear specific attributes
        let mut current = env.get_attributes(&sym_name);
        let to_remove: Vec<String> = args[1..].iter().map(|a| a.to_string()).collect();
        current.retain(|a| !to_remove.contains(a));
        env.set_attributes(&sym_name, current);
    }
    Ok(Value::Null)
}

pub fn builtin_attributes(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Attributes requires exactly 1 argument".to_string(),
        ));
    }
    let sym_name = match &args[0] {
        Value::Symbol(s) | Value::Str(s) => s.clone(),
        Value::Builtin(name, _) => name.clone(),
        Value::Function(fd) => fd.name.clone(),
        Value::Pattern(p) => match p {
            Expr::Symbol(s) => s.clone(),
            _ => {
                return Err(EvalError::TypeError {
                    expected: "Symbol or String".to_string(),
                    got: args[0].type_name().to_string(),
                });
            }
        },
        _ => {
            return Err(EvalError::TypeError {
                expected: "Symbol or String".to_string(),
                got: args[0].type_name().to_string(),
            });
        }
    };
    let attrs = env.get_attributes(&sym_name);
    let list: Vec<Value> = attrs.into_iter().map(Value::Symbol).collect();
    Ok(Value::List(list))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rug::Integer;

    fn sym(s: &str) -> Value {
        Value::Symbol(s.to_string())
    }

    fn int(n: i64) -> Value {
        Value::Integer(Integer::from(n))
    }

    fn env() -> crate::env::Env {
        let env = crate::env::Env::new();
        crate::builtins::register_builtins(&env);
        env
    }

    #[test]
    fn test_factor_constant() {
        assert_eq!(factor_expr(&int(5)), int(5));
    }

    #[test]
    fn test_factor_linear_gcd() {
        // Factor[2x + 4] = 2(x + 2)
        let expr = simplify_call(
            "Plus",
            &[simplify_call("Times", &[int(2), sym("x")]), int(4)],
        );
        let result = factor_expr(&expr);
        match &result {
            Value::Call { head, args } if head == "Times" => {
                assert_eq!(args.len(), 2);
                assert_eq!(args[0], int(2));
            }
            _ => panic!("Expected Times call, got {:?}", result),
        }
    }

    #[test]
    fn test_factor_difference_of_squares() {
        // Factor[x^2 - 1] = (x - 1)(x + 1)
        let expr = simplify_call(
            "Plus",
            &[simplify_call("Power", &[sym("x"), int(2)]), int(-1)],
        );
        let result = factor_expr(&expr);
        match &result {
            Value::Call { head, args } if head == "Times" => {
                assert_eq!(args.len(), 2);
            }
            _ => panic!("Expected Times call, got {:?}", result),
        }
    }

    #[test]
    fn test_factor_perfect_square() {
        // Factor[x^2 + 2x + 1] = (x + 1)^2
        let expr = simplify_call(
            "Plus",
            &[
                simplify_call("Power", &[sym("x"), int(2)]),
                simplify_call("Times", &[int(2), sym("x")]),
                int(1),
            ],
        );
        let result = factor_expr(&expr);
        match &result {
            Value::Call { head, args } if head == "Times" => {
                assert_eq!(args.len(), 2);
            }
            _ => panic!("Expected Times call, got {:?}", result),
        }
    }

    // ── Expand tests ──

    #[test]
    fn test_expand_basic_power() {
        // Expand[(x+1)^2] = x^2 + 2x + 1
        let expr = simplify_call(
            "Power",
            &[simplify_call("Plus", &[sym("x"), int(1)]), int(2)],
        );
        let result = builtin_expand(&[expr]).unwrap();
        match &result {
            Value::Call { head, .. } if head == "Plus" => {}
            _ => panic!("Expected Plus (expanded form), got {:?}", result),
        }
    }

    #[test]
    fn test_expand_product() {
        // Expand[(x+1)(x+2)] = x^2 + 3x + 2
        let expr = simplify_call(
            "Times",
            &[
                simplify_call("Plus", &[sym("x"), int(1)]),
                simplify_call("Plus", &[sym("x"), int(2)]),
            ],
        );
        let result = builtin_expand(&[expr]).unwrap();
        match &result {
            Value::Call { head, .. } if head == "Plus" => {}
            _ => panic!("Expected Plus (expanded), got {:?}", result),
        }
    }

    // ── Differentiation integration tests ──
    // Moved to tests/cli.rs — D is now implemented as Syma rules (D.syma),
    // so tests go through the full language pipeline (lexer→parser→eval).

    #[test]
    fn test_integrate_power() {
        // Integrate[x^2, x] = x^3/3 (stored as Times[Power[x,3], Power[3,-1]])
        let env = Env::new();
        crate::builtins::register_builtins(&env);
        let expr = simplify_call("Power", &[sym("x"), int(2)]);
        let result = builtin_integrate(&[expr, sym("x")], &env).unwrap();
        match &result {
            Value::Call { head, args } if head == "Times" => {
                // Contains Power[x, 3]
                assert!(
                    args.iter().any(|a| matches!(a,
                        Value::Call { head, args: a_args } if head == "Power" && a_args.len() == 2
                            && a_args[0] == sym("x") && a_args[1] == int(3)
                    )),
                    "Expected Power[x,3] in result, got {:?}",
                    result
                );
            }
            _ => panic!("Expected Times (x^3/3), got {:?}", result),
        }
    }

    #[test]
    fn test_integrate_sin() {
        // Integrate[Sin[x], x] = -Cos[x]
        let env = Env::new();
        crate::builtins::register_builtins(&env);
        let expr = simplify_call("Sin", &[sym("x")]);
        let result = builtin_integrate(&[expr, sym("x")], &env).unwrap();
        match &result {
            Value::Call { head, args } if head == "Times" => {
                // Should contain Cos[x]
                assert!(
                    args.iter().any(|a| *a == simplify_call("Cos", &[sym("x")])),
                    "Expected Cos[x] in result, got {:?}",
                    result
                );
            }
            _ => panic!("Expected Times containing Cos[x], got {:?}", result),
        }
    }

    #[test]
    fn test_integrate_constant() {
        // Integrate[5, x] = 5x
        let env = Env::new();
        crate::builtins::register_builtins(&env);
        let result = builtin_integrate(&[int(5), sym("x")], &env).unwrap();
        match &result {
            Value::Call { head, .. } if head == "Times" => {}
            _ => panic!("Expected Times (5*x), got {:?}", result),
        }
    }

    // ── Solve tests ──

    #[test]
    fn test_solve_linear() {
        // Solve[2x + 1 == 0, x]
        let eq = Value::Call {
            head: "Equal".to_string(),
            args: vec![
                simplify_call(
                    "Plus",
                    &[simplify_call("Times", &[int(2), sym("x")]), int(1)],
                ),
                int(0),
            ],
        };
        let result = builtin_solve(&[eq, sym("x")]).unwrap();
        match &result {
            Value::List(items) if !items.is_empty() => {}
            _ => panic!("Expected a list of solutions, got {:?}", result),
        }
    }

    #[test]
    fn test_solve_quadratic() {
        // Solve[x^2 - 4 == 0, x]
        let eq = Value::Call {
            head: "Equal".to_string(),
            args: vec![
                simplify_call(
                    "Plus",
                    &[simplify_call("Power", &[sym("x"), int(2)]), int(-4)],
                ),
                int(0),
            ],
        };
        let result = builtin_solve(&[eq, sym("x")]).unwrap();
        match &result {
            Value::List(items) if !items.is_empty() => {}
            _ => panic!("Expected a list of solutions, got {:?}", result),
        }
    }

    #[test]
    fn test_solve_cubic() {
        // Solve[x^3 - 6*x^2 + 11*x - 6 == 0, x]
        // Roots are x=1, x=2, x=3
        let eq = Value::Call {
            head: "Equal".to_string(),
            args: vec![
                simplify_call(
                    "Plus",
                    &[
                        simplify_call("Power", &[sym("x"), int(3)]),
                        simplify_call(
                            "Times",
                            &[int(-6), simplify_call("Power", &[sym("x"), int(2)])],
                        ),
                        simplify_call("Times", &[int(11), sym("x")]),
                        int(-6),
                    ],
                ),
                int(0),
            ],
        };
        let result = builtin_solve(&[eq, sym("x")]).unwrap();
        match &result {
            Value::List(items) if items.len() == 3 => {
                // Each item should be a Rule
                for item in items {
                    assert!(
                        matches!(item, Value::Rule { .. }),
                        "Expected Rule, got {:?}",
                        item
                    );
                }
            }
            _ => panic!("Expected a list of 3 solutions, got {:?}", result),
        }
    }

    #[test]
    fn test_solve_quartic() {
        // Solve[x^4 - 5*x^2 + 4 == 0, x]
        // Roots are x=-2, x=-1, x=1, x=2
        let eq = Value::Call {
            head: "Equal".to_string(),
            args: vec![
                simplify_call(
                    "Plus",
                    &[
                        simplify_call("Power", &[sym("x"), int(4)]),
                        simplify_call(
                            "Times",
                            &[int(-5), simplify_call("Power", &[sym("x"), int(2)])],
                        ),
                        int(4),
                    ],
                ),
                int(0),
            ],
        };
        let result = builtin_solve(&[eq, sym("x")]).unwrap();
        match &result {
            Value::List(items) if !items.is_empty() => {
                // Each item should be a Rule
                for item in items {
                    assert!(
                        matches!(item, Value::Rule { .. }),
                        "Expected Rule, got {:?}",
                        item
                    );
                }
            }
            _ => panic!("Expected a list of solutions, got {:?}", result),
        }
    }

    // ── Series tests ──

    #[test]
    fn test_series_sin_minimal() {
        // Series[Sin[x], {x, 0, 1}] — minimal to avoid recursion issues
        let var_spec = Value::List(vec![sym("x"), int(0), int(1)]);
        let expr = simplify_call("Sin", &[sym("x")]);
        let result = builtin_series(&[expr, var_spec], &env()).unwrap();
        // Should return SeriesData
        assert!(matches!(result, Value::SeriesData { .. }));
        if let Value::SeriesData {
            coefficients,
            max_exponent,
            ..
        } = &result
        {
            assert_eq!(coefficients.len(), 2); // n=0 and n=1
            assert_eq!(*max_exponent, 2); // order+1
        }
    }

    #[test]
    fn test_series_exp_10() {
        // Series[Exp[x], {x, 0, 10}]
        let var_spec = Value::List(vec![sym("x"), int(0), int(10)]);
        let expr = simplify_call("Exp", &[sym("x")]);
        let result = builtin_series(&[expr, var_spec], &env()).unwrap();
        assert!(matches!(result, Value::SeriesData { .. }));
        if let Value::SeriesData { coefficients, .. } = &result {
            assert_eq!(coefficients.len(), 11); // n=0 through n=10
        }
    }

    // ── Simplify tests ──

    #[test]
    fn test_simplify_trivial() {
        // Simplify[x] = x
        let result = builtin_simplify(&[sym("x")]).unwrap();
        assert_eq!(result, sym("x"));
    }
}
