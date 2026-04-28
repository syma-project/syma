use crate::value::{DEFAULT_PRECISION, EvalError, Value};
use rug::Float;
use rug::Integer;

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

// ── Helper functions ──

/// Substitute a variable with a value throughout an expression tree.
fn substitute(expr: &Value, var: &str, value: &Value) -> Value {
    match expr {
        Value::Symbol(s) if s == var => value.clone(),
        Value::Call { head, args } => {
            let new_args: Vec<Value> = args
                .iter()
                .map(|a| substitute(a, var, value))
                .collect();
            Value::Call {
                head: head.clone(),
                args: new_args,
            }
        }
        Value::List(items) => {
            Value::List(items.iter().map(|i| substitute(i, var, value)).collect())
        }
        _ => expr.clone(),
    }
}

/// Check if a variable appears free in an expression.
fn free_appears_p(expr: &Value, var: &str) -> bool {
    match expr {
        Value::Symbol(s) => s == var,
        Value::Call { args, .. } => args.iter().any(|a| free_appears_p(a, var)),
        Value::List(items) => items.iter().any(|i| free_appears_p(i, var)),
        _ => false,
    }
}

/// Check if a value is zero.
fn is_zero_v(v: &Value) -> bool {
    match v {
        Value::Integer(n) => n.is_zero(),
        Value::Real(r) => r.is_zero(),
        Value::Rational(r) => r.is_zero(),
        _ => false,
    }
}

/// Extract (numerator, denominator) from a rational expression.
/// Returns (expr, 1) if not a fraction.
fn extract_num_den(expr: &Value) -> (Value, Value) {
    match expr {
        Value::Call { head, args } if head == "Divide" && args.len() == 2 => {
            (args[0].clone(), args[1].clone())
        }
        _ => (expr.clone(), Value::Integer(Integer::from(1))),
    }
}

/// Simplify a call node using the symbolic module (re-export for use here).
fn simplify_call(head: &str, args: &[Value]) -> Value {
    match head {
        "Plus" => simplify_plus(args),
        "Times" => simplify_times(args),
        "Power" => simplify_power(args),
        "Divide" if args.len() == 2 => {
            let (num, den) = cancel_factors(&args[0], &args[1]);
            if is_one_v(&den) {
                num
            } else {
                call("Divide", vec![num, den])
            }
        }
        _ => call_ref(head, args),
    }
}

/// Simplify a value recursively.
fn simplify_value(val: &Value) -> Value {
    match val {
        Value::Call { head, args } => {
            let sargs: Vec<Value> = args.iter().map(simplify_value).collect();
            simplify_call(head, &sargs)
        }
        _ => val.clone(),
    }
}

fn is_one_v(v: &Value) -> bool {
    match v {
        Value::Integer(n) => *n == Integer::from(1),
        Value::Real(r) => *r == Float::with_val(DEFAULT_PRECISION, 1.0),
        _ => false,
    }
}

/// Simplify Plus by flattening and combining like terms (integer coefficients).
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
    let combined = combine_plus_terms(terms);
    if combined.len() == 1 {
        return combined.into_iter().next().unwrap();
    }
    call("Plus", combined)
}

fn combine_plus_terms(terms: Vec<Value>) -> Vec<Value> {
    let mut groups: Vec<(Value, Integer)> = Vec::new();
    for term in terms {
        let (base, coeff) = extract_coeff_base(&term);
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
    let one = Integer::from(1);
    let mut result = Vec::new();
    for (base, coeff) in groups {
        if coeff.is_zero() {
            continue;
        }
        if base.struct_eq(&Value::Integer(Integer::from(1))) {
            result.push(Value::Integer(coeff));
        } else if coeff == one {
            result.push(base);
        } else {
            result.push(simplify_call(
                "Times",
                &[Value::Integer(coeff), base],
            ));
        }
    }
    if result.is_empty() {
        vec![Value::Integer(Integer::from(0))]
    } else {
        result
    }
}

fn extract_coeff_base(term: &Value) -> (Value, Integer) {
    match term {
        Value::Integer(n) => (Value::Integer(Integer::from(1)), n.clone()),
        Value::Call { head, args } if head == "Times" && args.len() == 2 => {
            match &args[0] {
                Value::Integer(n) => (args[1].clone(), n.clone()),
                _ => match &args[1] {
                    Value::Integer(n) => (args[0].clone(), n.clone()),
                    _ => (term.clone(), Integer::from(1)),
                },
            }
        }
        _ => (term.clone(), Integer::from(1)),
    }
}

fn simplify_times(args: &[Value]) -> Value {
    if args.is_empty() {
        return Value::Integer(Integer::from(1));
    }
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
    if flat.len() == 1 {
        return flat.into_iter().next().unwrap();
    }
    let (num, sym): (Vec<_>, Vec<_>) =
        flat.into_iter().partition(|v| matches!(v, Value::Integer(_) | Value::Real(_) | Value::Rational(_)));
    let mut numeric = Value::Integer(Integer::from(1));
    for n in num {
        numeric = simplify_call("Times", &[numeric, n]);
    }
    let mut merged: Vec<Value> = Vec::new();
    let base_exp: Vec<(Value, Integer)> = sym.into_iter().flat_map(|f| {
        if let Value::Call { head, args } = &f {
            if head == "Power" && args.len() == 2 {
                if let Value::Integer(exp) = &args[1] {
                    vec![(args[0].clone(), exp.clone())]
                } else {
                    vec![(f.clone(), Integer::from(1))]
                }
            } else {
                vec![(f, Integer::from(1))]
            }
        } else {
            vec![(f, Integer::from(1))]
        }
    }).collect::<Vec<_>>();
    
    let mut base_exp_map: Vec<(Value, Integer)> = Vec::new();
    for (base, exp) in base_exp {
        let mut found = false;
        for g in &mut base_exp_map {
            if g.0.struct_eq(&base) {
                g.1 += exp;
                found = true;
                break;
            }
        }
        if !found {
            base_exp_map.push((base, exp));
        }
    }
    
    if !is_one_v(&numeric) {
        merged.push(numeric);
    }
    let one = Integer::from(1);
    for (base, exp) in base_exp_map {
        if exp.is_zero() {
            continue;
        }
        if exp == one {
            merged.push(base);
        } else {
            merged.push(simplify_call("Power", &[base, Value::Integer(exp)]));
        }
    }
    if merged.is_empty() {
        Value::Integer(Integer::from(1))
    } else if merged.len() == 1 {
        merged.into_iter().next().unwrap()
    } else {
        call("Times", merged)
    }
}

fn simplify_power(args: &[Value]) -> Value {
    if args.len() != 2 {
        return call_ref("Power", args);
    }
    match &args[1] {
        Value::Integer(n) if n.is_zero() => Value::Integer(Integer::from(1)),
        Value::Integer(n) if *n == Integer::from(1) => args[0].clone(),
        _ => match &args[0] {
            Value::Integer(n) if n.is_zero() => Value::Integer(Integer::from(0)),
            Value::Integer(n) if *n == Integer::from(1) => Value::Integer(Integer::from(1)),
            _ => call_ref("Power", args),
        },
    }
}

/// Cancel common power factors between numerator and denominator.
fn cancel_factors(num: &Value, den: &Value) -> (Value, Value) {
    let mut nf: Vec<(Value, Integer)> = Vec::new();
    collect_factors(num, &mut nf);
    let mut df: Vec<(Value, Integer)> = Vec::new();
    collect_factors(den, &mut df);

    let mut rn: Vec<(Value, Integer)> = Vec::new();
    let mut rd: Vec<(Value, Integer)> = Vec::new();

    for (nb, ne) in &nf {
        if let Some((_db, de)) = df.iter_mut().find(|(b, _)| b.struct_eq(nb)) {
            if *ne > *de {
                rn.push((nb.clone(), ne.clone() - de.clone()));
            } else if *de > *ne {
                rd.push((nb.clone(), de.clone() - ne.clone()));
            }
        } else {
            rn.push((nb.clone(), ne.clone()));
        }
    }
    for (db, de) in &df {
        if !nf.iter().any(|(b, _)| b.struct_eq(db)) {
            rd.push((db.clone(), de.clone()));
        }
    }
    let new_num = rebuild_f(&rn);
    let new_den = rebuild_f(&rd);
    (new_num, new_den)
}

fn collect_factors(val: &Value, factors: &mut Vec<(Value, Integer)>) {
    match val {
        Value::Call { head, args } if head == "Times" => {
            for arg in args {
                collect_factors(arg, factors);
            }
        }
        Value::Call { head, args } if head == "Power" && args.len() == 2 => {
            let exp = match &args[1] {
                Value::Integer(n) => n.clone(),
                _ => Integer::from(1),
            };
            factors.push((args[0].clone(), exp));
        }
        _ => factors.push((val.clone(), Integer::from(1))),
    }
}

fn rebuild_f(factors: &[(Value, Integer)]) -> Value {
    if factors.is_empty() {
        return Value::Integer(Integer::from(1));
    }
    if factors.len() == 1 {
        let (base, exp) = &factors[0];
        if exp == &Integer::from(1) {
            return base.clone();
        }
        return call("Power", vec![base.clone(), Value::Integer(exp.clone())]);
    }
    let one = Integer::from(1);
    let terms: Vec<Value> = factors
        .iter()
        .map(|(b, e)| {
            if *e == one {
                b.clone()
            } else {
                call("Power", vec![b.clone(), Value::Integer(e.clone())])
            }
        })
        .collect();
    call("Times", terms)
}

/// Expand a value: distribute Times over Plus, expand binomial Powers.
fn expand_value(val: &Value) -> Value {
    match val {
        Value::Call { head, args } => {
            let eargs: Vec<Value> = args.iter().map(expand_value).collect();
            match head.as_str() {
                "Times" => {
                    if eargs.len() != 2 {
                        return call_ref("Times", &eargs);
                    }
                    expand_times2(&eargs[0], &eargs[1])
                }
                "Power" => {
                    if eargs.len() != 2 {
                        return call_ref("Power", &eargs);
                    }
                    expand_pow2(&eargs[0], &eargs[1])
                }
                _ => call(head.as_str(), eargs),
            }
        }
        _ => val.clone(),
    }
}

fn expand_times2(a: &Value, b: &Value) -> Value {
    if let Value::Call { head, args: pa } = b {
        if head == "Plus" {
            let terms: Vec<Value> = pa
                .iter()
                .map(|t| expand_value(&simplify_call("Times", &[a.clone(), t.clone()])))
                .collect();
            return simplify_call("Plus", &terms);
        }
    }
    if let Value::Call { head, args: pa } = a {
        if head == "Plus" {
            let terms: Vec<Value> = pa
                .iter()
                .map(|t| expand_value(&simplify_call("Times", &[t.clone(), b.clone()])))
                .collect();
            return simplify_call("Plus", &terms);
        }
    }
    call("Times", vec![a.clone(), b.clone()])
}

fn expand_pow2(base: &Value, exp: &Value) -> Value {
    if let Value::Integer(n) = exp {
        if let Some(ni) = n.to_i64() {
            if let Value::Call { head, args: pa } = base {
                if head == "Plus" && pa.len() == 2 && (0..=10).contains(&ni) {
                    let mut terms = Vec::new();
                    for k in 0..=ni {
                        let c = binom(ni, k);
                        let ap = if ni - k == 0 {
                            Value::Integer(Integer::from(1))
                        } else if ni - k == 1 {
                            pa[0].clone()
                        } else {
                            call("Power", vec![pa[0].clone(), Value::Integer(Integer::from(ni - k))])
                        };
                        let bp = if k == 0 {
                            Value::Integer(Integer::from(1))
                        } else if k == 1 {
                            pa[1].clone()
                        } else {
                            call("Power", vec![pa[1].clone(), Value::Integer(Integer::from(k))])
                        };
                        terms.push(simplify_call("Times", &[Value::Integer(Integer::from(c)), ap, bp]));
                    }
                    return simplify_call("Plus", &terms);
                }
            }
        }
    }
    call("Power", vec![base.clone(), exp.clone()])
}

fn binom(n: i64, k: i64) -> i64 {
    if k < 0 || k > n {
        return 0;
    }
    if k == 0 || k == n {
        return 1;
    }
    let k = if k > n - k { n - k } else { k };
    let mut r = 1i64;
    for i in 0..k {
        r = r * (n - i) / (i + 1);
    }
    r
}

/// Differentiate expr with respect to var (basic rules, no env dispatch).
fn basic_diff(expr: &Value, var: &str) -> Value {
    match expr {
        Value::Integer(_) | Value::Real(_) | Value::Rational(_) => Value::Integer(Integer::from(0)),
        Value::Symbol(s) => {
            if s == var {
                Value::Integer(Integer::from(1))
            } else {
                Value::Integer(Integer::from(0))
            }
        }
        Value::Call { head, args } => match head.as_str() {
            "Plus" => {
                let terms: Vec<Value> =
                    args.iter().map(|a| basic_diff(a, var)).collect();
                simplify_call("Plus", &terms)
            }
            "Times" if args.len() == 2 => {
                let d_left = basic_diff(&args[0], var);
                let d_right = basic_diff(&args[1], var);
                simplify_call(
                    "Plus",
                    &[
                        simplify_call("Times", &[d_left, args[1].clone()]),
                        simplify_call("Times", &[args[0].clone(), d_right]),
                    ],
                )
            }
            "Power" if args.len() == 2 => {
                let base = &args[0];
                let exp = &args[1];
                if !free_appears_p(exp, var) {
                    let d_base = basic_diff(base, var);
                    simplify_call(
                        "Times",
                        &[
                            exp.clone(),
                            call("Power", vec![
                                base.clone(),
                                simplify_call("Plus", &[exp.clone(), Value::Integer(Integer::from(-1))]),
                            ]),
                            d_base,
                        ],
                    )
                } else if base.struct_eq(&Value::Symbol("E".to_string())) {
                    let d_exp = basic_diff(exp, var);
                    simplify_call("Times", &[call("Exp", vec![exp.clone()]), d_exp])
                } else {
                    call("D", vec![expr.clone(), Value::Symbol(var.to_string())])
                }
            }
            "Sin" if args.len() == 1 => {
                let d_inner = basic_diff(&args[0], var);
                simplify_call("Times", &[call("Cos", vec![args[0].clone()]), d_inner])
            }
            "Cos" if args.len() == 1 => {
                let d_inner = basic_diff(&args[0], var);
                simplify_call(
                    "Times",
                    &[
                        Value::Integer(Integer::from(-1)),
                        call("Sin", vec![args[0].clone()]),
                        d_inner,
                    ],
                )
            }
            "Exp" if args.len() == 1 => {
                let d_inner = basic_diff(&args[0], var);
                simplify_call("Times", &[call("Exp", vec![args[0].clone()]), d_inner])
            }
            "Log" if args.len() == 1 => {
                let d_inner = basic_diff(&args[0], var);
                simplify_call(
                    "Times",
                    &[
                        call("Power", vec![args[0].clone(), Value::Integer(Integer::from(-1))]),
                        d_inner,
                    ],
                )
            }
            _ => call("D", vec![expr.clone(), Value::Symbol(var.to_string())]),
        },
        _ => call("D", vec![expr.clone(), Value::Symbol(var.to_string())]),
    }
}

/// Parse the var -> target rule from Limit's second argument.
/// Returns (var_name, target_value).
fn parse_limit_rule(arg: &Value) -> Option<(String, Value)> {
    match arg {
        Value::Rule { lhs, rhs, .. } => {
            let var = match lhs.as_ref() {
                Value::Symbol(s) => s.clone(),
                _ => return None,
            };
            let target = (**rhs).clone();
            Some((var, target))
        }
        Value::Call { head, args } if head == "Rule" && args.len() == 2 => {
            let var = match &args[0] {
                Value::Symbol(s) => s.clone(),
                _ => return None,
            };
            Some((var, args[1].clone()))
        }
        _ => None,
    }
}

/// Check if a value represents Infinity.
fn is_infinity(v: &Value) -> bool {
    matches!(v, Value::Symbol(s) if s == "Infinity")
}

/// Check if a value represents -Infinity.
fn is_neg_infinity(v: &Value) -> bool {
    match v {
        Value::Symbol(s) => s == "Infinity",
        Value::Call { head, args } if head == "Times" && args.len() == 2 => {
            match &args[0] {
                Value::Integer(n) if *n == Integer::from(-1) => {
                    matches!(&args[1], Value::Symbol(s) if s == "Infinity")
                }
                _ => matches!(&args[1], Value::Integer(n) if *n == Integer::from(-1))
                    && matches!(&args[0], Value::Symbol(s) if s == "Infinity"),
            }
        }
        _ => false,
    }
}

/// Check if a value is a numeric constant (Integer, Real, Rational, or known symbol constant).
fn is_numeric_constant(v: &Value) -> bool {
    matches!(v, Value::Integer(_) | Value::Real(_) | Value::Rational(_))
        || matches!(v, Value::Symbol(s) if is_known_constant_sym(s))
}

fn is_known_constant_sym(s: &str) -> bool {
    matches!(s, "Pi" | "E" | "Infinity" | "I")
}

// ── Limit ──

pub fn builtin_limit(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Limit requires exactly 2 arguments: Limit[expr, var -> x0]".to_string(),
        ));
    }
    let expr = &args[0];
    let (var, target) = match parse_limit_rule(&args[1]) {
        Some(v) => v,
        None => {
            return Ok(call("Limit", args.to_vec()));
        }
    };
    Ok(compute_limit(expr, &var, &target))
}

fn compute_limit(expr: &Value, var: &str, target: &Value) -> Value {
    // Constant expressions (no dependence on var)
    if !free_appears_p(expr, var) {
        return expr.clone();
    }

    // Direct substitution when no singularity
    if is_infinity(target) || is_neg_infinity(target) {
        return limit_at_infinity(expr, var, false);
    }
    if is_neg_infinity(target) {
        return limit_at_infinity(expr, var, true);
    }

    let target_val = target.clone();

    // Handle specific function forms at the target
    match expr {
        // Limit[x, x -> a] = a
        Value::Symbol(s) => {
            if s == var {
                return target_val;
            }
        }
        // Limit[x^n, x -> 0]
        Value::Call {
            head,
            args,
        } if head == "Power" && args.len() == 2 && args[0].struct_eq(&Value::Symbol(var.to_string())) => {
            return limit_power_at_point(&args[1], target);
        }
        // Limit[1/x, x -> 0] -> unevaluated
        Value::Call {
            head,
            args,
        } if head == "Divide" && args.len() == 2 => {
            let num = &args[0];
            let den = &args[1];
            let den_at_target = substitute(den, var, target);
            let den_simplified = simplify_value(&den_at_target);
            let num_at_target = substitute(num, var, target);
            let num_simplified = simplify_value(&num_at_target);

            // If denominator is 0, check for 0/0 (L'Hôpital)
            if is_zero_v(&den_simplified) {
                if is_zero_v(&num_simplified) {
                    // 0/0 indeterminate form: try L'Hôpital's rule
                    let d_num = basic_diff(num, var);
                    let d_den = basic_diff(den, var);
                    let d_num_at = simplify_value(&substitute(&d_num, var, target));
                    let d_den_at = simplify_value(&substitute(&d_den, var, target));
                    if !is_zero_v(&d_den_at) {
                        return simplify_value(&call(
                            "Divide",
                            vec![d_num_at, d_den_at],
                        ));
                    }
                }
                // Infinity or undefined — return unevaluated
                return call("Limit", vec![expr.clone(), call("Rule", vec![Value::Symbol(var.to_string()), target_val])]);
            }
            // Direct substitution works
            return simplify_value(&call(
                "Divide",
                vec![num_simplified, den_simplified],
            ));
        }
        // Limit[Sin[x]/x, x -> 0] recognized via 0/0 check
        _ => {
            let sub = substitute(expr, var, target);
            let simp = simplify_value(&sub);
            // If substitution gives a well-defined result, return it
            if !matches!(&simp, Value::Call { head, .. } if head == "Divide") {
                if !is_zero_v(&simp) || !free_appears_p(expr, var) {
                    // Non-zero finite result
                } else {
                    return simp;
                }
            }
            // Check for 0/0 by expanding and checking for Divide[0, 0]
            let expanded = expand_value(expr);
            let exp_sub = substitute(&expanded, var, target);
            let exp_simp = simplify_value(&exp_sub);
            if let Value::Call { head, args } = &exp_simp {
                if head == "Divide" && args.len() == 2 {
                    if is_zero_v(&args[0]) && is_zero_v(&args[1]) {
                        // Apply L'Hôpital
                        let d_num = basic_diff(expr, var);
                        let d_den = basic_diff(expr, var);
                        let d_result = simplify_value(&substitute(&d_num, var, target));
                        if let Value::Call { head: dh, dargs } = &d_result {
                            if dh == "Divide" && dargs.len() == 2 && !is_zero_v(&dargs[1]) {
                                return d_result;
                            }
                        }
                        if !is_zero_v(&d_result) && !matches!(&d_result, Value::Call { head, .. } if head == "Divide" && {
                            let d = &d_result;
                            if let Value::Call { args: aa, .. } = d {
                                aa.len() == 2 && is_zero_v(&aa[1])
                            } else { false }
                        }) {
                            return d_result;
                        }
                        return call("Limit", vec![expr.clone(), call("Rule", vec![Value::Symbol(var.to_string()), target_val.clone()])]);
                    }
                    if is_zero_v(&args[1]) {
                        return call("Limit", vec![expr.clone(), call("Rule", vec![Value::Symbol(var.to_string()), target_val])]);
                    }
                    return exp_simp;
                }
            }
            if !is_zero_v(&simp) {
                return simp;
            }
        }
        // Specific known limits
        Value::Call { head, args } if head == "Sin" && args.len() == 1 => {
            let inner = &args[0];
            if inner.struct_eq(&Value::Symbol(var.to_string())) {
                let t = substitute(inner, var, target);
                let ts = simplify_value(&t);
                if is_zero_v(&ts) {
                    return Value::Integer(Integer::from(0));
                }
            }
        }
        Value::Call { head, args } if head == "Exp" && args.len() == 1 => {
            let inner = &args[0];
            let inner_at = simplify_value(&substitute(inner, var, target));
            if is_zero_v(&inner_at) {
                return Value::Integer(Integer::from(1));
            }
            if is_infinity(&inner_at) {
                return Value::Symbol("Infinity".to_string());
            }
            if is_neg_infinity(&inner_at) {
                return Value::Integer(Integer::from(0));
            }
        }
        Value::Call { head, args } if head == "Log" && args.len() == 1 => {
            let inner = &args[0];
            if inner.struct_eq(&Value::Symbol(var.to_string())) {
                let t = simplify_value(&substitute(inner, var, target));
                if is_zero_v(&t) {
                    return call("Times", vec![Value::Integer(Integer::from(-1)), Value::Symbol("Infinity".to_string())]);
                }
            }
        }
        _ => {}
    }

    // Try direct substitution as general approach
    let sub = substitute(expr, var, target);
    let simp_sub = simplify_value(&sub);
    if !is_zero_v(&simp_sub)
        || !free_appears_p(expr, var)
        || is_numeric_constant(&simp_sub)
    {
        return simp_sub;
    }

    // General L'Hôpital attempt for 0/0
    if let Value::Call {
        head,
        args,
    } = &simp_sub
        && head == "Divide" && args.len() == 2
        && is_zero_v(&args[0]) && is_zero_v(&args[1])
    {
        let d_num = basic_diff(expr, var);
        let d_den = basic_diff(expr, var);
        let d_ratio = simplify_value(&substitute(&d_num, var, target));
        if let Value::Call {
            head: dh,
            args: da,
        } = &d_ratio
            && dh == "Divide" && da.len() == 2 && !is_zero_v(&da[1])
        {
            return d_ratio;
        }
        if !matches!(&d_ratio, Value::Call { head, .. } if head == "D") {
            return d_ratio;
        }
    }

    // Return unevaluated
    call("Limit", vec![
        expr.clone(),
        call("Rule", vec![Value::Symbol(var.to_string()), target.clone()]),
    ])
}

fn limit_power_at_point(exp: &Value, target: &Value) -> Value {
    match exp {
        Value::Integer(n) => {
            if target.struct_eq(&Value::Integer(Integer::from(0))) {
                if n > 0 {
                    return Value::Integer(Integer::from(0));
                }
                if n == 0 {
                    return Value::Integer(Integer::from(1));
                }
                // Negative power at 0
                return call("Limit", vec![
                    call("Power", vec![Value::Symbol("x".to_string()), exp.clone()]),
                    call("Rule", vec![Value::Symbol("x".to_string()), Value::Integer(Integer::from(0))]),
                ]);
            }
            // Direct substitution
            call("Power", vec![target.clone(), exp.clone()])
        }
        _ => {
            // Non-integer exponent at 0
            if target.struct_eq(&Value::Integer(Integer::from(0))) {
                return call("Limit", vec![
                    call("Power", vec![Value::Symbol("x".to_string()), exp.clone()]),
                    call("Rule", vec![Value::Symbol("x".to_string()), Value::Integer(Integer::from(0))]),
                ]);
            }
            call("Power", vec![target.clone(), exp.clone()])
        }
    }
}

fn limit_at_infinity(expr: &Value, var: &str, neg: bool) -> Value {
    match expr {
        // Constant
        _ if !free_appears_p(expr, var) => return expr.clone(),
        // Variable itself
        Value::Symbol(s) => {
            if s == var {
                return if neg {
                    call("Times", vec![Value::Integer(Integer::from(-1)), Value::Symbol("Infinity".to_string())])
                } else {
                    Value::Symbol("Infinity".to_string())
                };
            }
        }
        // Power: x^n at +/- Infinity
        Value::Call {
            head,
            args,
        } if head == "Power" && args.len() == 2 && args[0].struct_eq(&Value::Symbol(var.to_string())) => {
            if let Value::Integer(n) = &args[1] {
                if *n > 0 {
                    if neg && n.is_odd() {
                        return call("Times", vec![Value::Integer(Integer::from(-1)), Value::Symbol("Infinity".to_string())]);
                    }
                    return Value::Symbol("Infinity".to_string());
                }
                if *n < 0 {
                    return Value::Integer(Integer::from(0));
                }
                // n == 0
                return Value::Integer(Integer::from(1));
            }
        }
        // Exp[x] at Infinity -> Infinity, at -Infinity -> 0
        Value::Call { head, args } if head == "Exp" && args.len() == 1 => {
            let inner = &args[0];
            if inner.struct_eq(&Value::Symbol(var.to_string())) {
                if neg {
                    return Value::Integer(Integer::from(0));
                }
                return Value::Symbol("Infinity".to_string());
            }
            if let Value::Call { head: ih, iargs } = inner {
                if ih == "Times" && iargs.len() == 2 {
                    if let Value::Integer(c) = &iargs[0] {
                        if iargs[1].struct_eq(&Value::Symbol(var.to_string())) {
                            if c > 0 {
                                if neg {
                                    return Value::Integer(Integer::from(0));
                                }
                                return Value::Symbol("Infinity".to_string());
                            } else {
                                if neg {
                                    return Value::Symbol("Infinity".to_string());
                                }
                                return Value::Integer(Integer::from(0));
                            }
                        }
                    }
                    if let Value::Integer(c) = &iargs[1] {
                        if iargs[0].struct_eq(&Value::Symbol(var.to_string())) {
                            if c > 0 {
                                if neg {
                                    return Value::Integer(Integer::from(0));
                                }
                                return Value::Symbol("Infinity".to_string());
                            } else {
                                if neg {
                                    return Value::Symbol("Infinity".to_string());
                                }
                                return Value::Integer(Integer::from(0));
                            }
                        }
                    }
                }
            }
        }
        // Log[x] at Infinity -> Infinity, at -Infinity -> return unevaluated
        Value::Call { head, args } if head == "Log" && args.len() == 1 => {
            let inner = &args[0];
            if inner.struct_eq(&Value::Symbol(var.to_string())) {
                if neg {
                    return call("Limit", vec![expr.clone(), call("Rule", vec![Value::Symbol(var.to_string()), call("Times", vec![Value::Integer(Integer::from(-1)), Value::Symbol("Infinity".to_string())])])]);
                }
                return Value::Symbol("Infinity".to_string());
            }
        }
        // Plus: limit of sum = sum of limits
        Value::Call { head, args } if head == "Plus" => {
            let terms: Vec<Value> = args
                .iter()
                .map(|a| limit_at_infinity(a, var, neg))
                .collect();
            let combined = simplify_value(&call("Plus", terms));
            if is_infinity(&combined) || is_neg_infinity(&combined) {
                return combined;
            }
            return combined;
        }
        // Times: product
        Value::Call { head, args } if head == "Times" && args.len() == 2 => {
            let l = limit_at_infinity(&args[0], var, neg);
            let r = limit_at_infinity(&args[1], var, neg);
            if is_numeric_constant(&l) && is_numeric_constant(&r) {
                return simplify_call("Times", &[l, r]);
            }
            if is_infinity(&l) || is_infinity(&r) {
                return Value::Symbol("Infinity".to_string());
            }
            if is_zero_v(&l) || is_zero_v(&r) {
                return Value::Integer(Integer::from(0));
            }
            return call("Times", vec![l, r]);
        }
        // Divide
        Value::Call { head, args } if head == "Divide" && args.len() == 2 => {
            let num_lim = limit_at_infinity(&args[0], var, neg);
            let den_lim = limit_at_infinity(&args[1], var, neg);
            if is_zero_v(&den_lim) {
                return Value::Symbol("Infinity".to_string());
            }
            if is_infinity(&num_lim) && !is_infinity(&den_lim) {
                return Value::Symbol("Infinity".to_string());
            }
            if is_infinity(&num_lim) && is_infinity(&den_lim) {
                return call("Limit", vec![expr.clone(), call("Rule", vec![Value::Symbol(var.to_string()), call_ref("Times", &[Value::Integer(Integer::from(if neg { -1 } else { 1 })), Value::Symbol("Infinity".to_string())])])]);
            }
            simplify_call("Divide", &[num_lim, den_lim])
        }
        _ => {
            // For polynomial-like expressions, look at highest power
            let expanded = expand_value(expr);
            if let Value::Call { head, args } = &expanded {
                if head == "Plus" {
                    // Find the term with highest power of var
                    let mut best_term: Option<(i64, Value)> = None;
                    for arg in args {
                        let (deg, _) = poly_degree(arg, var);
                        if let Some((bd, _)) = best_term {
                            if deg > bd {
                                best_term = Some((deg, arg.clone()));
                            }
                        } else {
                            best_term = Some((deg, arg.clone()));
                        }
                    }
                    if let Some((deg, dominant)) = best_term {
                        if deg > 0 {
                            return limit_at_infinity(&dominant, var, neg);
                        }
                        if deg == 0 {
                            return dominant;
                        }
                    }
                }
            }
            // Return unevaluated
            let target_val = if neg {
                call("Times", vec![Value::Integer(Integer::from(-1)), Value::Symbol("Infinity".to_string())])
            } else {
                Value::Symbol("Infinity".to_string())
            };
            call("Limit", vec![expr.clone(), call("Rule", vec![Value::Symbol(var.to_string()), target_val])])
        }
    }
}

/// Get polynomial degree in var.
fn poly_degree(expr: &Value, var: &str) -> (i64, Value) {
    match expr {
        Value::Symbol(s) => {
            if s == var {
                (1, expr.clone())
            } else {
                (0, expr.clone())
            }
        }
        Value::Integer(_) | Value::Real(_) | Value::Rational(_) => (0, expr.clone()),
        Value::Call { head, args } if head == "Power" && args.len() == 2 => {
            if args[0].struct_eq(&Value::Symbol(var.to_string())) {
                if let Value::Integer(n) = &args[1] {
                    if let Some(ni) = n.to_i64() {
                        return (ni, expr.clone());
                    }
                }
            }
            (0, expr.clone())
        }
        Value::Call { head, args } if head == "Times" => {
            let mut total = 0i64;
            for arg in args {
                let (d, _) = poly_degree(arg, var);
                total += d;
            }
            (total, expr.clone())
        }
        _ => (0, expr.clone()),
    }
}

// ── Apart ──

pub fn builtin_apart(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 1 || args.len() > 2 {
        return Err(EvalError::Error(
            "Apart requires 1 or 2 arguments: Apart[expr, var?]".to_string(),
        ));
    }
    let expr = &args[0];
    let var = if args.len() == 2 {
        match &args[1] {
            Value::Symbol(s) => s.clone(),
            _ => {
                return Err(EvalError::TypeError {
                    expected: "Symbol".to_string(),
                    got: args[1].type_name().to_string(),
                });
            }
        }
    } else {
        match find_var(expr) {
            Some(v) => v,
            None => return Ok(expr.clone()),
        }
    };
    Ok(apart_expr(expr, &var))
}

fn find_var(expr: &Value) -> Option<String> {
    match expr {
        Value::Symbol(s) => {
            if !is_known_constant_sym(s) {
                Some(s.clone())
            } else {
                None
            }
        }
        Value::Call { head, args } => {
            if head == "Plus" || head == "Times" || head == "Divide" {
                for arg in args {
                    if let Some(v) = find_var(arg) {
                        return Some(v);
                    }
                }
            }
            if head == "Power" && args.len() == 2 {
                return find_var(&args[0]);
            }
            None
        }
        _ => None,
    }
}

fn apart_expr(expr: &Value, var: &str) -> Value {
    let (num, den) = extract_num_den(expr);
    let den_expanded = expand_value(&den);
    let num_expanded = expand_value(&num);

    // Extract linear factors from denominator
    let factors = extract_linear_factors(&den_expanded, var);
    if factors.is_empty() {
        // Can't decompose — return as-is
        if is_one_v(&den) {
            return expr.clone();
        }
        return call("Apart", vec![expr.clone(), Value::Symbol(var.to_string())]);
    }

    // Check if numerator degree >= denominator degree
    let num_deg = degree_in(&num_expanded, var);
    let den_deg = degree_in(&den_expanded, var);
    let mut remainder_num = num_expanded;
    let mut polynomial_part = Value::Integer(Integer::from(0));

    if num_deg >= den_deg {
        // Polynomial long division
        let (quot, rem) = polyn_div(&num_expanded, &den_expanded, var);
        polynomial_part = quot;
        remainder_num = rem;
    }

    // Set up partial fraction: sum of A_i / (factor_i) for simple linear factors
    let coeffs = solve_partial_fractions(&remainder_num, &factors, var);
    if coeffs.is_empty() {
        if is_one_v(&den) {
            return polynomial_part;
        }
        return call("Apart", vec![expr.clone(), Value::Symbol(var.to_string())]);
    }

    // Build sum of partial fractions
    let mut pf_terms: Vec<Value> = Vec::new();
    for (i, (_, factor)) in factors.iter().enumerate() {
        if i >= coeffs.len() {
            break;
        }
        let coeff = &coeffs[i];
        // Check if coefficient is zero
        if is_zero_v(&simplify_value(coeff)) {
            continue;
        }
        pf_terms.push(simplify_call(
            "Divide",
            &[coeff.clone(), factor.clone()],
        ));
    }

    let pf_sum = if pf_terms.is_empty() {
        Value::Integer(Integer::from(0))
    } else if pf_terms.len() == 1 {
        pf_terms.into_iter().next().unwrap()
    } else {
        simplify_call("Plus", &pf_terms)
    };

    if is_zero_v(&simplify_value(&polynomial_part)) {
        pf_sum
    } else if is_zero_v(&simplify_value(&pf_sum)) {
        polynomial_part
    } else {
        simplify_call("Plus", &[polynomial_part, pf_sum])
    }
}

/// Extract linear factors (x + c) from a product in the denominator.
fn extract_linear_factors(expr: &Value, var: &str) -> Vec<(Value, Value)> {
    let mut factors = Vec::new();
    match expr {
        Value::Call { head, args } if head == "Times" => {
            for arg in args {
                extract_linear_factors_inner(arg, var, &mut factors);
            }
        }
        _ => {
            extract_linear_factors_inner(expr, var, &mut factors);
        }
    }
    factors
}

fn extract_linear_factors_inner(expr: &Value, var: &str, factors: &mut Vec<(Value, Value)>) {
    // Recognize (x + c) or (x - c) pattern
    if let Value::Call { head, args } = expr {
        if head == "Plus" && args.len() == 2 {
            // Check for c + x or x + c
            if let Value::Symbol(s) = &args[0] {
                if s == var {
                    if is_numeric_constant(&args[1]) {
                        factors.push((args[1].clone(), expr.clone()));
                        return;
                    }
                }
            }
            if let Value::Symbol(s) = &args[1] {
                if s == var {
                    if is_numeric_constant(&args[0]) {
                        factors.push((args[0].clone(), expr.clone()));
                        return;
                    }
                }
            }
        }
        if head == "Times" {
            for arg in args {
                extract_linear_factors_inner(arg, var, factors);
            }
            return;
        }
    }
    // Try to factor quadratic
    if let Value::Call { head, args } = expr {
        if head != "Plus" && head != "Times" {
            // Single unfactored polynomial — try to factor
            let factored = simple_factor_linear(expr, var);
            if let Some(fs) = factored {
                factors.extend(fs);
            }
        }
    }
}

/// Try to factor a polynomial into linear factors: (x+a)*(x+b) etc.
fn simple_factor_linear(poly: &Value, var: &str) -> Option<Vec<(Value, Value)>> {
    let coeffs = poly_coeffs(poly, var);
    if coeffs.len() < 3 {
        return None;
    }

    // For quadratic: a*x^2 + b*x + c
    if coeffs.len() == 3 {
        if let (Value::Integer(a), Value::Integer(b), Value::Integer(c)) =
            (&coeffs[2], &coeffs[1], &coeffs[0])
        {
            let disc = b * b - Integer::from(4) * a * c;
            if !disc.is_negative() {
                let sqrt_disc = disc.sqrt();
                if (sqrt_disc.clone() * sqrt_disc.clone()) == disc {
                    let two_a = Integer::from(2) * a;
                    let neg_b = (-b).into();
                    let r1 = (&neg_b + &sqrt_disc) / &two_a;
                    let r2 = (&neg_b - &sqrt_disc) / &two_a;
                    let x = Value::Symbol(var.to_string());
                    let f1 = call("Plus", vec![x.clone(), call("Times", vec![Value::Integer(Integer::from(-1)), Value::Integer(r1)])]);
                    let f2 = call("Plus", vec![x, call("Times", vec![Value::Integer(Integer::from(-1)), Value::Integer(r2)])]);
                    return Some(vec![
                        (-r1, f1),
                        (-r2, f2),
                    ]);
                }
            }
        }
    }
    None
}

fn poly_coeffs(expr: &Value, var: &str) -> Vec<Value> {
    let deg = degree_in(expr, var);
    if deg < 0 {
        return vec![];
    }
    let mut coeffs = vec![Value::Integer(Integer::from(0)); (deg + 1) as usize];
    let expanded = expand_value(expr);
    collect_coeff_terms(&expanded, var, &mut coeffs);
    coeffs
}

fn collect_coeff_terms(expr: &Value, var: &str, coeffs: &mut Vec<Value>) {
    match expr {
        Value::Call { head, args } if head == "Plus" => {
            for arg in args {
                collect_coeff_terms(arg, var, coeffs);
            }
        }
        Value::Call { head, args } if head == "Times" => {
            let mut var_power = 0i64;
            let mut coeff = Value::Integer(Integer::from(1));
            for arg in args {
                if let Value::Symbol(s) = arg {
                    if s == var {
                        var_power += 1;
                    } else {
                        coeff = simplify_call("Times", &[coeff, arg.clone()]);
                    }
                } else if let Value::Call {
                    head: "Power",
                    args: pa,
                } = arg
                    && pa.len() == 2 && pa[0].struct_eq(&Value::Symbol(var.to_string()))
                {
                    if let Value::Integer(n) = &pa[1] {
                        if let Some(ni) = n.to_i64() {
                            var_power += ni;
                        }
                    }
                } else {
                    coeff = simplify_call("Times", &[coeff, arg.clone()]);
                }
            }
            if var_power >= 0 && (var_power as usize) < coeffs.len() {
                let old = &coeffs[var_power as usize];
                coeffs[var_power as usize] = simplify_call("Plus", &[old.clone(), coeff]);
            }
        }
        Value::Symbol(s) if s == var => {
            let one = Value::Integer(Integer::from(1));
            let zero = Value::Integer(Integer::from(0));
            let old = &coeffs[1];
            coeffs[1] = simplify_call("Plus", &[old.clone(), one]);
            coeffs[0] = simplify_call("Plus", &[coeffs[0].clone(), zero]);
        }
        Value::Integer(_) | Value::Real(_) | Value::Rational(_) => {
            let c = expr.clone();
            let zero = Value::Integer(Integer::from(0));
            coeffs[0] = simplify_call("Plus", &[coeffs[0].clone(), c]);
        }
        _ => {}
    }
}

fn degree_in(expr: &Value, var: &str) -> i64 {
    let coeffs = get_polynomial_degree(expr, var);
    coeffs
}

fn get_polynomial_degree(expr: &Value, var: &str) -> i64 {
    match expr {
        Value::Symbol(s) => {
            if s == var {
                1
            } else {
                0
            }
        }
        Value::Integer(_) | Value::Real(_) | Value::Rational(_) => 0,
        Value::Call { head, args } if head == "Power" && args.len() == 2 => {
            if args[0].struct_eq(&Value::Symbol(var.to_string())) {
                if let Value::Integer(n) = &args[1] {
                    if let Some(ni) = n.to_i64() {
                        return ni;
                    }
                }
            }
            0
        }
        Value::Call { head, args } if head == "Times" => {
            let mut total = 0i64;
            for arg in args {
                total += get_polynomial_degree(arg, var);
            }
            total
        }
        Value::Call { head, args } if head == "Plus" => {
            let mut max_d = 0i64;
            for arg in args {
                let d = get_polynomial_degree(arg, var);
                if d > max_d {
                    max_d = d;
                }
            }
            max_d
        }
        _ => 0,
    }
}

/// Polynomial division: returns (quotient, remainder).
fn polyn_div(num: &Value, den: &Value, var: &str) -> (Value, Value) {
    let num_coeffs = poly_coeffs(num, var);
    let den_coeffs = poly_coeffs(den, var);

    let nd = num_coeffs.len() as i64 - 1;
    let dd = den_coeffs.len() as i64 - 1;

    if nd < dd {
        return (Value::Integer(Integer::from(0)), num.clone());
    }

    let mut remainder: Vec<Value> = num_coeffs
        .iter()
        .map(|v| simplify_value(v))
        .collect();
    let den_lead = &den_coeffs[den_coeffs.len() - 1];
    let den_lead_inv = call(
        "Power",
        vec![den_lead.clone(), Value::Integer(Integer::from(-1))],
    );

    let mut quot_coeffs: Vec<Value> =
        vec![Value::Integer(Integer::from(0)); (nd - dd + 1) as usize];

    for i in (0..=(nd - dd)).rev() {
        let j = (i + dd) as usize;
        if j >= remainder.len() {
            continue;
        }
        let r = &remainder[j];
        if is_zero_v(r) {
            continue;
        }
        let q_i = simplify_call("Times", &[r.clone(), den_lead_inv.clone()]);
        quot_coeffs[i as usize] = q_i.clone();

        for k in 0..den_coeffs.len() {
            let idx = (i + k as i64) as usize;
            if idx < remainder.len() {
                let sub_term = expand_value(&simplify_call(
                    "Times",
                    &[q_i.clone(), den_coeffs[k].clone()],
                ));
                remainder[idx] = simplify_call(
                    "Plus",
                    &[
                        remainder[idx].clone(),
                        simplify_call("Times", &[Value::Integer(Integer::from(-1)), sub_term]),
                    ],
                );
            }
        }
        for v in remainder.iter_mut() {
            *v = simplify_value(v);
        }
    }

    let remainder_val = rebuild_poly(&remainder, var);
    let ndeg = quot_coeffs.len() as i64 - 1;
    let mut qc_trimmed = quot_coeffs;
    while qc_trimmed.len() > 1 && is_zero_v(&qc_trimmed[qc_trimmed.len() - 1]) {
        qc_trimmed.pop();
    }
    let quot_val = rebuild_poly(&qc_trimmed, var);
    (quot_val, remainder_val)
}

fn rebuild_poly(coeffs: &[Value], var: &str) -> Value {
    let x = Value::Symbol(var.to_string());
    let mut terms = Vec::new();
    for (i, c) in coeffs.iter().enumerate() {
        if is_zero_v(c) {
            continue;
        }
        let term = match i {
            0 => c.clone(),
            1 => simplify_call("Times", &[c.clone(), x.clone()]),
            _ => simplify_call(
                "Times",
                &[
                    c.clone(),
                    call("Power", vec![x.clone(), Value::Integer(Integer::from(i as i64))]),
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

/// Solve for partial fraction coefficients using polynomial identity.
/// For factors (x + a_i), set up: num = sum(coeff_i * product(other_factors)).
fn solve_partial_fractions(num: &Value, factors: &[(Value, Value)], var: &str) -> Vec<Value> {
    let n = factors.len();
    if n == 0 {
        return vec![];
    }

    // For simple linear factors, use residue method: substitute root of each factor.
    // For A/(x+a): A = num(-a) / product(other_factors(-a))
    let mut coeffs = Vec::new();
    for (i, (root_neg, _factor)) in factors.iter().enumerate() {
        // root_neg is the constant c in (x + c), so root is -c
        let root = simplify_call(
            "Times",
            &[Value::Integer(Integer::from(-1)), root_neg.clone()],
        );

        // Evaluate numerator at root
        let num_at_root = simplify_value(&substitute(num, var, &root));

        // Evaluate product of all other factors at root
        let mut product = Value::Integer(Integer::from(1));
        for (j, (_root_neg, factor)) in factors.iter().enumerate() {
            if j == i {
                continue;
            }
            let fac_at_root = simplify_value(&substitute(factor, var, &root));
            product = simplify_call("Times", &[product, fac_at_root]);
        }

        let coeff = simplify_call("Divide", &[num_at_root, product]);
        coeffs.push(coeff);
    }
    coeffs
}

// ── Together ──

pub fn builtin_together(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Together requires exactly 1 argument".to_string(),
        ));
    }
    Ok(together_expr(&args[0]))
}

fn together_expr(expr: &Value) -> Value {
    match expr {
        // For a sum of fractions
        Value::Call { head, args } if head == "Plus" => {
            let terms = args;
            if terms.is_empty() {
                return expr.clone();
            }
            // Collect numerators and denominators
            let mut numerators: Vec<Value> = Vec::new();
            let mut denominators: Vec<Value> = Vec::new();
            for term in terms {
                let (n, d) = extract_num_den(term);
                numerators.push(n);
                denominators.push(d);
            }

            // Common denominator = LCM of all denominators (product / GCD approach)
            // For simplicity: use product of all denominators
            let mut common_den = Value::Integer(Integer::from(1));
            for d in &denominators {
                if !is_one_v(d) {
                    common_den = simplify_call("Times", &[common_den, d.clone()]);
                }
            }

            // For each term, multiply numerator by (common_den / own_den)
            let mut scaled_numerator: Vec<Value> = Vec::new();
            for (i, n) in numerators.iter().enumerate() {
                let own_den = &denominators[i];
                let scale = if is_one_v(own_den) {
                    Value::Integer(Integer::from(1))
                } else {
                    simplify_call("Divide", &[common_den.clone(), own_den.clone()])
                };
                let scaled = simplify_call("Times", &[n.clone(), scale]);
                scaled_numerator.push(expand_value(&simplify_value(&scaled)));
            }

            // Sum all scaled numerators
            let combined_num = if scaled_numerator.len() == 1 {
                scaled_numerator.into_iter().next().unwrap()
            } else {
                expand_value(&simplify_call("Plus", &scaled_numerator))
            };

            let combined_num_s = simplify_value(&combined_num);
            let combined_den_s = simplify_value(&common_den);

            // Try to cancel common factors
            let (final_num, final_den) = cancel_factors(&combined_num_s, &combined_den_s);

            if is_one_v(&simplify_value(&final_den)) {
                simplify_value(&final_num)
            } else {
                simplify_call("Divide", &[simplify_value(&final_num), simplify_value(&final_den)])
            }
        }
        // Single fraction
        Value::Call { head, args } if head == "Divide" && args.len() == 2 => {
            let (n, d) = cancel_factors(&args[0], &args[1]);
            if is_one_v(&simplify_value(&d)) {
                simplify_value(&n)
            } else {
                call("Divide", vec![simplify_value(&n), simplify_value(&d)])
            }
        }
        // Non-sum, non-division: unchanged
        _ => expr.clone(),
    }
}

// ── Cancel ──

pub fn builtin_cancel(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Cancel requires exactly 1 argument".to_string(),
        ));
    }
    Ok(cancel_expr(&args[0]))
}

fn cancel_expr(expr: &Value) -> Value {
    // Numeric rational: reduce by GCD
    if let Value::Rational(r) = expr {
        let num = r.numer();
        let den = r.denom();
        let g = num.gcd(&den);
        if g > 1 {
            return Value::Rational(Box::new(
                rug::Rational::from((num / &g, den / &g)),
            ));
        }
        return expr.clone();
    }

    // Integer — nothing to cancel
    if matches!(expr, Value::Integer(_)) {
        return expr.clone();
    }

    let (num, den) = extract_num_den(expr);

    // Try symbolic factor cancellation
    let num_factored = try_factor_cancel(&num);
    let den_factored = try_factor_cancel(&den);

    let (c_num, c_den) = cancel_symbolic_factors(&num_factored, &den_factored);

    if is_one_v(&c_den) {
        return simplify_value(&c_num);
    }

    // Try polynomial division as extra step
    if let Value::Call {
        head,
        args,
    } = &c_den
        && head == "Times"
    {
        for factor in args {
            if let Some(divided) = try_poly_divide_by_factor(&c_num, factor) {
            let new_num = simplify_value(&divided);
                let remaining_factors: Vec<Value> = args
                    .iter()
                    .filter(|f| !f.struct_eq(factor))
                    .cloned()
                    .collect();
                let new_den = if remaining_factors.is_empty() {
                    Value::Integer(Integer::from(1))
                } else if remaining_factors.len() == 1 {
                    remaining_factors.into_iter().next().unwrap()
                } else {
                    call("Times", remaining_factors)
                };
                return cancel_expr(&call("Divide", vec![new_num, new_den]));
            }
        }
    }

    // Try direct polynomial division
    let (num_s, den_s) = (simplify_value(&c_num), simplify_value(&c_den));
    if let Some(quotient) = polyn_div_exact(&num_s, &den_s) {
        return simplify_value(&quotient);
    }

    call("Divide", vec![c_num, c_den])
}

/// Try to recognize and factor patterns like x^2-1 -> (x-1)(x+1).
fn try_factor_cancel(expr: &Value) -> Value {
    match expr {
        Value::Call { head, args } if head == "Plus" => {
            // x^2 - 1 pattern: x^2 + (-1)
            if args.len() == 2 {
                if let (Value::Call {
                    head: "Power",
                    args: pargs,
                }, Value::Integer(c)) = (&args[0], &args[1])
                    && pargs.len() == 2
                {
                    let base = &pargs[0];
                    if let Value::Integer(exp) = &pargs[1] {
                        if *exp == Integer::from(2) {
                            if let Some(c_i64) = c.to_i64() {
                                if c_i64 < 0 && (-c_i64) as f64 == (((-c_i64) as f64).sqrt()).floor() as f64 {
                                    let sqrt_c = ((-c_i64) as f64).sqrt() as i64;
                                    let x = base.clone();
                                    let f1 = call("Plus", vec![x.clone(), Value::Integer(Integer::from(sqrt_c))]);
                                    let f2 = call("Plus", vec![x, Value::Integer(Integer::from(-sqrt_c))]);
                                    return call("Times", vec![f1, f2]);
                                }
                            }
                        }
                    }
                }
                if let (Value::Integer(c), Value::Call {
                    head: "Power",
                    args: pargs,
                }) = (&args[0], &args[1])
                    && pargs.len() == 2
                {
                    let base = &pargs[0];
                    if let Value::Integer(exp) = &pargs[1] {
                        if *exp == Integer::from(2) {
                            if let Some(c_i64) = c.to_i64() {
                                if c_i64 < 0 && (-c_i64) as f64 == (((-c_i64) as f64).sqrt()).floor() as f64 {
                                    let sqrt_c = ((-c_i64) as f64).sqrt() as i64;
                                    let x = base.clone();
                                    let f1 = call("Plus", vec![x.clone(), Value::Integer(Integer::from(sqrt_c))]);
                                    let f2 = call("Plus", vec![x, Value::Integer(Integer::from(-sqrt_c))]);
                                    return call("Times", vec![f1, f2]);
                                }
                            }
                        }
                    }
                }
            }
            expr.clone()
        }
        _ => expr.clone(),
    }
}

/// Cancel matching factors between two expressions in Times form.
fn cancel_symbolic_factors(a: &Value, b: &Value) -> (Value, Value) {
    let mut a_factors = Vec::new();
    collect_times_factors(a, &mut a_factors);
    let mut b_factors = Vec::new();
    collect_times_factors(b, &mut b_factors);

    let mut ra = Vec::new();
    let mut rb = Vec::new();

    for (af, _) in &a_factors {
        let pos = b_factors.iter().position(|(bf, _)| af.struct_eq(bf));
        if let Some(p) = pos {
            b_factors.remove(p);
        } else {
            ra.push(af.clone());
        }
    }
    for (bf, _) in &b_factors {
        rb.push(bf.clone());
    }

    let new_a = if ra.is_empty() {
        Value::Integer(Integer::from(1))
    } else if ra.len() == 1 {
        ra.into_iter().next().unwrap()
    } else {
        call("Times", ra)
    };
    let new_b = if rb.is_empty() {
        Value::Integer(Integer::from(1))
    } else if rb.len() == 1 {
        rb.into_iter().next().unwrap()
    } else {
        call("Times", rb)
    };
    (new_a, new_b)
}

fn collect_times_factors(val: &Value, factors: &mut Vec<Value>) {
    match val {
        Value::Call { head, args } if head == "Times" => {
            for arg in args {
                collect_times_factors(arg, factors);
            }
        }
        _ => factors.push(val.clone()),
    }
}

fn try_poly_divide_by_factor(poly: &Value, factor: &Value) -> Option<Value> {
    let (quot, rem) = polyn_div(poly, factor, find_any_var(poly));
    if is_zero_v(&simplify_value(&rem)) {
        Some(quot)
    } else {
        None
    }
}

fn find_any_var(expr: &Value) -> String {
    if let Some(v) = find_var(expr) {
        return v;
    }
    "x".to_string()
}

fn polyn_div_exact(num: &Value, den: &Value) -> Option<Value> {
    let var = find_any_var(num);
    let (quot, rem) = polyn_div(num, den, &var);
    if is_zero_v(&simplify_value(&rem)) {
        Some(quot)
    } else {
        None
    }
}

// ── Collect ──

pub fn builtin_collect(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Collect requires exactly 2 arguments: Collect[expr, var]".to_string(),
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
    Ok(collect_expr(&args[0], &var))
}

fn collect_expr(expr: &Value, var: &str) -> Value {
    // Expand the expression into a sum of terms
    let expanded = expand_value(expr);
    let terms = match &expanded {
        Value::Call { head, args } if head == "Plus" => args.clone(),
        _ => vec![expanded],
    };

    // Group by power of var
    let mut power_groups: Vec<(i64, Value)> = Vec::new();

    for term in &terms {
        let (power, coeff) = extract_var_coeff(term, var);
        let mut found = false;
        for (p, c) in &mut power_groups {
            if *p == power {
                *c = simplify_call("Plus", &[*c, coeff.clone()]);
                found = true;
                break;
            }
        }
        if !found {
            power_groups.push((power, coeff));
        }
    }

    // Sort by power descending
    power_groups.sort_by(|a, b| b.0.cmp(&a.0));

    // Reconstruct
    let mut result_terms: Vec<Value> = Vec::new();
    for (power, coeff) in power_groups {
        let coeff_s = simplify_value(&coeff);
        if is_zero_v(&coeff_s) {
            continue;
        }
        if power == 0 {
            result_terms.push(coeff_s);
        } else if power == 1 {
            result_terms.push(simplify_call(
                "Times",
                &[coeff_s, Value::Symbol(var.to_string())],
            ));
        } else {
            result_terms.push(simplify_call(
                "Times",
                &[
                    coeff_s,
                    call(
                        "Power",
                        vec![
                            Value::Symbol(var.to_string()),
                            Value::Integer(Integer::from(power)),
                        ],
                    ),
                ],
            ));
        }
    }

    if result_terms.is_empty() {
        Value::Integer(Integer::from(0))
    } else if result_terms.len() == 1 {
        result_terms.into_iter().next().unwrap()
    } else {
        call("Plus", result_terms)
    }
}

/// Extract the power of var from a term, and its coefficient.
/// x^2 -> (2, 1), a*x^2 -> (2, a), 3*x -> (1, 3), 5 -> (0, 5)
fn extract_var_coeff(term: &Value, var: &str) -> (i64, Value) {
    match term {
        Value::Symbol(s) => {
            if s == var {
                (1, Value::Integer(Integer::from(1)))
            } else {
                (0, term.clone())
            }
        }
        Value::Integer(_) | Value::Real(_) | Value::Rational(_) => {
            (0, term.clone())
        }
        Value::Call {
            head: "Power",
            args,
        } if args.len() == 2 && args[0].struct_eq(&Value::Symbol(var.to_string())) => {
            if let Value::Integer(n) = &args[1] {
                if let Some(ni) = n.to_i64() {
                    return (ni, Value::Integer(Integer::from(1)));
                }
            }
            (0, term.clone())
        }
        Value::Call {
            head: "Times",
            args,
        } => {
            let mut power = 0i64;
            let mut coeff = Value::Integer(Integer::from(1));
            for arg in args {
                match arg {
                    Value::Symbol(s) if s == var => {
                        power += 1;
                    }
                    Value::Call {
                        head: "Power",
                        args: pargs,
                    } if pargs.len() == 2
                        && pargs[0].struct_eq(&Value::Symbol(var.to_string())) =>
                    {
                        if let Value::Integer(n) = &pargs[1] {
                            if let Some(ni) = n.to_i64() {
                                power += ni;
                            }
                        }
                    }
                    _ => {
                        coeff = simplify_call("Times", &[coeff, arg.clone()]);
                    }
                }
            }
            (power, coeff)
        }
        _ => (0, term.clone()),
    }
}

// ── FunctionExpand ──

pub fn builtin_function_expand(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "FunctionExpand requires exactly 1 argument".to_string(),
        ));
    }
    Ok(function_expand_value(&args[0]))
}

fn function_expand_value(expr: &Value) -> Value {
    match expr {
        Value::Call { head, args } => {
            let eargs: Vec<Value> = args.iter().map(function_expand_value).collect();
            match head.as_str() {
                "Plus" => simplify_call("Plus", &eargs),
                "Times" => simplify_call("Times", &eargs),
                "Power" => {
                    if eargs.len() == 2 {
                        return expand_power_fn(&eargs[0], &eargs[1]);
                    }
                    call_ref("Power", &eargs)
                }
                "Log" if eargs.len() == 1 => expand_log(&eargs[0]),
                "Sin" if eargs.len() == 1 => expand_sin(&eargs[0]),
                "Cos" if eargs.len() == 1 => expand_cos(&eargs[0]),
                "Gamma" if eargs.len() == 1 => expand_gamma(&eargs[0]),
                "BesselJ" if eargs.len() == 2 => expand_bessel_j(&eargs[0], &eargs[1]),
                "Divide" if eargs.len() == 2 => {
                    call("Divide", vec![eargs[0].clone(), eargs[1].clone()])
                }
                _ => call(head.as_str(), eargs),
            }
        }
        _ => expr.clone(),
    }
}

fn expand_log(arg: &Value) -> Value {
    match arg {
        Value::Call {
            head: "Times",
            args,
        } => {
            let terms: Vec<Value> = args
                .iter()
                .map(|a| call("Log", vec![function_expand_value(a)]))
                .collect();
            simplify_call("Plus", &terms)
        }
        Value::Call {
            head: "Divide",
            args,
        } if args.len() == 2 => {
            simplify_call(
                "Plus",
                &[
                    call("Log", vec![function_expand_value(&args[0])]),
                    simplify_call(
                        "Times",
                        &[
                            Value::Integer(Integer::from(-1)),
                            call("Log", vec![function_expand_value(&args[1])]),
                        ],
                    ),
                ],
            )
        }
        Value::Call {
            head: "Power",
            args: pargs,
        } if pargs.len() == 2 => {
            // Log[x^n] -> n * Log[x]
            simplify_call(
                "Times",
                &[
                    pargs[1].clone(),
                    call("Log", vec![function_expand_value(&pargs[0])]),
                ],
            )
        }
        _ => call("Log", vec![arg.clone()]),
    }
}

fn expand_sin(arg: &Value) -> Value {
    // Sin[2*x] -> 2*Sin[x]*Cos[x]
    if let Value::Call {
        head: "Times",
        args,
    } = arg
        && args.len() == 2
        && matches!(
            &args[0],
            Value::Integer(n) if *n == Integer::from(2)
        )
    {
        let x = function_expand_value(&args[1]);
        return simplify_call(
            "Times",
            &[
                Value::Integer(Integer::from(2)),
                call("Sin", vec![x.clone()]),
                call("Cos", vec![x]),
            ],
        );
    }
    // Sin[x + y] -> Sin[x]*Cos[y] + Cos[x]*Sin[y]
    if let Value::Call {
        head: "Plus",
        args,
    } = arg
        && args.len() == 2
    {
        let a = function_expand_value(&args[0]);
        let b = function_expand_value(&args[1]);
        return simplify_call(
            "Plus",
            &[
                simplify_call("Times", &[call("Sin", vec![a.clone()]), call("Cos", vec![b.clone()])]),
                simplify_call("Times", &[call("Cos", vec![a]), call("Sin", vec![b])]),
            ],
        );
    }
    call("Sin", vec![arg.clone()])
}

fn expand_cos(arg: &Value) -> Value {
    // Cos[2*x] -> Cos[x]^2 - Sin[x]^2
    if let Value::Call {
        head: "Times",
        args,
    } = arg
        && args.len() == 2
        && matches!(
            &args[0],
            Value::Integer(n) if *n == Integer::from(2)
        )
    {
        let x = function_expand_value(&args[1]);
        return simplify_call(
            "Plus",
            &[
                call("Power", vec![call("Cos", vec![x.clone()]), Value::Integer(Integer::from(2))]),
                simplify_call(
                    "Times",
                    &[
                        Value::Integer(Integer::from(-1)),
                        call("Power", vec![call("Sin", vec![x]), Value::Integer(Integer::from(2))]),
                    ],
                ),
            ],
        );
    }
    // Cos[x + y] -> Cos[x]*Cos[y] - Sin[x]*Sin[y]
    if let Value::Call {
        head: "Plus",
        args,
    } = arg
        && args.len() == 2
    {
        let a = function_expand_value(&args[0]);
        let b = function_expand_value(&args[1]);
        return simplify_call(
            "Plus",
            &[
                simplify_call("Times", &[call("Cos", vec![a.clone()]), call("Cos", vec![b.clone()])]),
                simplify_call(
                    "Times",
                    &[
                        Value::Integer(Integer::from(-1)),
                        call("Sin", vec![a]),
                        call("Sin", vec![b]),
                    ],
                ),
            ],
        );
    }
    call("Cos", vec![arg.clone()])
}

fn expand_gamma(arg: &Value) -> Value {
    // Gamma[n+1] -> n * Gamma[n]
    if let Value::Call {
        head: "Plus",
        args,
    } = arg
        && args.len() == 2
    {
        if let Value::Integer(n) = &args[1] {
            if *n == Integer::from(1) {
                return simplify_call(
                    "Times",
                    &[args[0].clone(), call("Gamma", vec![args[0].clone()])],
                );
            }
        }
        if let Value::Integer(n) = &args[0] {
            if *n == Integer::from(1) {
                return simplify_call(
                    "Times",
                    &[args[1].clone(), call("Gamma", vec![args[1].clone()])],
                );
            }
        }
    }
    call("Gamma", vec![arg.clone()])
}

fn expand_bessel_j(param: &Value, arg: &Value) -> Value {
    // BesselJ[1/2, x] -> Sqrt[2/(Pi*x)] * Sin[x]
    if let Value::Rational(r) = param {
        if *r.numer() == rug::Integer::from(1) && *r.denom() == rug::Integer::from(2) {
            return simplify_call(
                "Times",
                &[
                    call(
                        "Sqrt",
                        vec![call(
                            "Divide",
                            vec![
                                Value::Integer(Integer::from(2)),
                                simplify_call("Times", &[Value::Symbol("Pi".to_string()), arg.clone()]),
                            ],
                        )],
                    ),
                    call("Sin", vec![arg.clone()]),
                ],
            );
        }
    }
    // BesselJ[-1/2, x] -> Sqrt[2/(Pi*x)] * Cos[x]
    if let Value::Rational(r) = param {
        if *r.numer() == rug::Integer::from(-1) && *r.denom() == rug::Integer::from(2) {
            return simplify_call(
                "Times",
                &[
                    call(
                        "Sqrt",
                        vec![call(
                            "Divide",
                            vec![
                                Value::Integer(Integer::from(2)),
                                simplify_call("Times", &[Value::Symbol("Pi".to_string()), arg.clone()]),
                            ],
                        )],
                    ),
                    call("Cos", vec![arg.clone()]),
                ],
            );
        }
    }
    call("BesselJ", vec![param.clone(), arg.clone()])
}

fn expand_power_fn(base: &Value, exp: &Value) -> Value {
    // Sqrt[x] is Power[x, 1/2], leave as is
    // Exp[x^n] -> unchanged unless special
    if matches!(base, Value::Symbol(s) if s == "E") {
        return call("Exp", vec![exp.clone()]);
    }
    call("Power", vec![base.clone(), exp.clone()])
}

// ── NLimit ──

pub fn builtin_nlimit(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "NLimit requires exactly 2 arguments: NLimit[expr, var -> x0]".to_string(),
        ));
    }
    let expr = &args[0];
    let (var, target) = match parse_limit_rule(&args[1]) {
        Some(v) => v,
        None => {
            return Ok(call("NLimit", args.to_vec()));
        }
    };
    Ok(numeric_limit(expr, &var, &target))
}

fn numeric_limit(expr: &Value, var: &str, target: &Value) -> Value {
    let target_f = match value_to_float(target) {
        Some(f) => f,
        None => {
            return call("NLimit", vec![
                expr.clone(),
                call("Rule", vec![Value::Symbol(var.to_string()), target.clone()]),
            ]);
        }
    };

    // Evaluate at progressively closer points from both sides
    let mut prev_pos = None::<Float>;
    let mut prev_neg = None::<Float>;
    let tol = Float::with_val(DEFAULT_PRECISION, 1e-12);

    for k in 1..=30u32 {
        let delta = Float::with_val(DEFAULT_PRECISION, 10i64).pow(if k > 9 { 9 } else { k as i32 });
        let inv_delta = Float::with_val(DEFAULT_PRECISION, 1.0) / &delta;

        // Evaluate from right: x0 + inv_delta
        let x_pos = target_f.clone() + &inv_delta;
        let val_pos = eval_numeric(expr, var, &x_pos);

        // Evaluate from left: x0 - inv_delta
        let x_neg = target_f.clone() - &inv_delta;
        let val_neg = eval_numeric(expr, var, &x_neg);

        // Check convergence
        let pos_converged = if let Some(prev) = prev_pos {
            (val_pos.clone() - prev).abs() < tol
        } else {
            false
        };
        let neg_converged = if let Some(prev) = prev_neg {
            (val_neg.clone() - prev).abs() < tol
        } else {
            false
        };

        if pos_converged && neg_converged {
            // Return average of both sides
            let avg = (val_pos + val_neg) / Float::with_val(DEFAULT_PRECISION, 2);
            return Value::Real(avg);
        }

        if pos_converged {
            return Value::Real(val_pos);
        }

        if neg_converged {
            return Value::Real(val_neg);
        }

        prev_pos = Some(val_pos);
        prev_neg = Some(val_neg);
    }

    // Didn't converge — return unevaluated
    call("NLimit", vec![
        expr.clone(),
        call("Rule", vec![Value::Symbol(var.to_string()), target.clone()]),
    ])
}

fn value_to_float(val: &Value) -> Option<Float> {
    match val {
        Value::Integer(n) => Some(Float::with_val(DEFAULT_PRECISION, n)),
        Value::Real(r) => Some(r.clone()),
        Value::Rational(r) => {
            let num_f = Float::with_val(DEFAULT_PRECISION, r.numer());
            let den_f = Float::with_val(DEFAULT_PRECISION, r.denom());
            Some(num_f / den_f)
        }
        Value::Symbol(s) if s == "Pi" => {
            Some(Float::with_val(DEFAULT_PRECISION, rug::float::Constant::Pi))
        }
        Value::Symbol(s) if s == "E" => {
            let one = Float::with_val(DEFAULT_PRECISION, 1);
            Some(one.exp())
        }
        _ => None,
    }
}

fn eval_numeric(expr: &Value, var: &str, value: &Float) -> Float {
    let val = Value::Real(value.clone());
    let subbed = substitute(expr, var, &val);
    let simplified = simplify_value(&subbed);
    match &simplified {
        Value::Real(r) => r.clone(),
        Value::Integer(n) => Float::with_val(DEFAULT_PRECISION, n),
        Value::Rational(r) => {
            let num_f = Float::with_val(DEFAULT_PRECISION, r.numer());
            let den_f = Float::with_val(DEFAULT_PRECISION, r.denom());
            num_f / den_f
        }
        // Try to evaluate remaining symbolic expressions
        _ => {
            // Attempt basic numeric evaluation of common functions
            numeric_eval(&simplified)
        }
    }
}

fn numeric_eval(expr: &Value) -> Float {
    let zero = Float::with_val(DEFAULT_PRECISION, 0.0);
    match expr {
        Value::Real(r) => r.clone(),
        Value::Integer(n) => Float::with_val(DEFAULT_PRECISION, n),
        Value::Rational(r) => {
            let num_f = Float::with_val(DEFAULT_PRECISION, r.numer());
            let den_f = Float::with_val(DEFAULT_PRECISION, r.denom());
            num_f / den_f
        }
        Value::Symbol(s) if s == "Pi" => {
            Float::with_val(DEFAULT_PRECISION, rug::float::Constant::Pi)
        }
        Value::Symbol(s) if s == "E" => {
            let one = Float::with_val(DEFAULT_PRECISION, 1);
            one.exp()
        }
        Value::Call { head, args } if head == "Times" => {
            let mut result = Float::with_val(DEFAULT_PRECISION, 1.0);
            for arg in args {
                let a = numeric_eval(arg);
                result *= a;
            }
            result
        }
        Value::Call { head, args } if head == "Plus" => {
            let mut result = Float::with_val(DEFAULT_PRECISION, 0.0);
            for arg in args {
                let a = numeric_eval(arg);
                result += a;
            }
            result
        }
        Value::Call { head, args } if head == "Divide" && args.len() == 2 => {
            let num = numeric_eval(&args[0]);
            let den = numeric_eval(&args[1]);
            num / den
        }
        Value::Call { head, args } if head == "Power" && args.len() == 2 => {
            let base = numeric_eval(&args[0]);
            let exp = numeric_eval(&args[1]);
            base.pow(&exp)
        }
        Value::Call { head, args } if head == "Sin" && args.len() == 1 => {
            let arg = numeric_eval(&args[0]);
            arg.sin()
        }
        Value::Call { head, args } if head == "Cos" && args.len() == 1 => {
            let arg = numeric_eval(&args[0]);
            arg.cos()
        }
        Value::Call { head, args } if head == "Exp" && args.len() == 1 => {
            let arg = numeric_eval(&args[0]);
            arg.exp()
        }
        Value::Call { head, args } if head == "Log" && args.len() == 1 => {
            let arg = numeric_eval(&args[0]);
            arg.ln()
        }
        Value::Call { head, args } if head == "Sqrt" && args.len() == 1 => {
            let arg = numeric_eval(&args[0]);
            arg.sqrt()
        }
        Value::Call { head, args } if head == "Abs" || head == "Times" && args.len() == 2 => {
            if let Value::Integer(n) = &args[0] {
                if *n == Integer::from(-1) {
                    let inner = numeric_eval(&args[1]);
                    let neg = -inner;
                    if neg > zero {
                        neg
                    } else {
                        -neg
                    }
                } else {
                    numeric_eval(expr)
                }
            } else {
                numeric_eval(expr)
            }
        }
        _ => zero,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn val_int(n: i64) -> Value {
        Value::Integer(Integer::from(n))
    }
    fn val_sym(name: &str) -> Value {
        Value::Symbol(name.to_string())
    }
    fn val_call(head: &str, args: Vec<Value>) -> Value {
        Value::Call {
            head: head.to_string(),
            args,
        }
    }
    fn val_rule(lhs: Value, rhs: Value) -> Value {
        Value::Rule {
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
            delayed: false,
        }
    }

    // ── Limit tests ──

    #[test]
    fn test_limit_constant() {
        let result = builtin_limit(&[val_int(5), val_rule(val_sym("x"), val_int(0))]).unwrap();
        assert!(matches!(&result, Value::Integer(n) if *n == Integer::from(5)));
    }

    #[test]
    fn test_limit_x_at_a() {
        let result = builtin_limit(&[val_sym("x"), val_rule(val_sym("x"), val_int(3))]).unwrap();
        assert!(matches!(&result, Value::Integer(n) if *n == Integer::from(3)));
    }

    #[test]
    fn test_limit_xn_at_0() {
        let expr = val_call("Power", vec![val_sym("x"), val_int(3)]);
        let result = builtin_limit(&[expr, val_rule(val_sym("x"), val_int(0))]).unwrap();
        assert!(matches!(&result, Value::Integer(n) if n.is_zero()));
    }

    #[test]
    fn test_limit_xn_at_0_n_zero() {
        let expr = val_call("Power", vec![val_sym("x"), val_int(0)]);
        let result = builtin_limit(&[expr, val_rule(val_sym("x"), val_int(0))]).unwrap();
        assert!(matches!(&result, Value::Integer(n) if *n == Integer::from(1)));
    }

    #[test]
    fn test_limit_x_at_infinity() {
        let inf = val_sym("Infinity");
        let result = builtin_limit(&[val_sym("x"), val_rule(val_sym("x"), inf.clone())]).unwrap();
        assert!(matches!(&result, Value::Symbol(s) if s == "Infinity"));
    }

    #[test]
    fn test_limit_xn_positive_at_infinity() {
        let expr = val_call("Power", vec![val_sym("x"), val_int(3)]);
        let inf = val_sym("Infinity");
        let result = builtin_limit(&[expr, val_rule(val_sym("x"), inf)]).unwrap();
        assert!(matches!(&result, Value::Symbol(s) if s == "Infinity"));
    }

    #[test]
    fn test_limit_xn_negative_at_infinity() {
        let expr = val_call("Power", vec![val_sym("x"), val_int(-2)]);
        let inf = val_sym("Infinity");
        let result = builtin_limit(&[expr, val_rule(val_sym("x"), inf)]).unwrap();
        assert!(matches!(&result, Value::Integer(n) if n.is_zero()));
    }

    #[test]
    fn test_limit_exp_infinity() {
        let expr = val_call("Exp", vec![val_sym("x")]);
        let inf = val_sym("Infinity");
        let result = builtin_limit(&[expr, val_rule(val_sym("x"), inf)]).unwrap();
        assert!(matches!(&result, Value::Symbol(s) if s == "Infinity"));
    }

    #[test]
    fn test_limit_exp_neg_infinity() {
        let expr = val_call("Exp", vec![val_sym("x")]);
        let neg_inf = val_call("Times", vec![val_int(-1), val_sym("Infinity")]);
        let result = builtin_limit(&[expr, val_rule(val_sym("x"), neg_inf)]).unwrap();
        assert!(matches!(&result, Value::Integer(n) if n.is_zero()));
    }

    #[test]
    fn test_limit_log_at_0() {
        let expr = val_call("Log", vec![val_sym("x")]);
        let result = builtin_limit(&[expr, val_rule(val_sym("x"), val_int(0))]).unwrap();
        // Should be -Infinity
        assert!(matches!(&result, Value::Call { head, .. } if head == "Times"));
    }

    #[test]
    fn test_limit_lhopital_sin_over_x() {
        let expr = val_call("Divide", vec![
            val_call("Sin", vec![val_sym("x")]),
            val_sym("x"),
        ]);
        let result = builtin_limit(&[expr.clone(), val_rule(val_sym("x"), val_int(0))]).unwrap();
        assert!(matches!(&result, Value::Integer(n) if *n == Integer::from(1)));
    }

    // ── Together tests ──

    #[test]
    fn test_together_basic() {
        // 1/x + 1/y -> (x+y)/(x*y)
        let expr = val_call("Plus", vec![
            val_call("Divide", vec![val_int(1), val_sym("x")]),
            val_call("Divide", vec![val_int(1), val_sym("y")]),
        ]);
        let result = builtin_together(&[expr]).unwrap();
        assert!(matches!(&result, Value::Call { head, .. } if head == "Divide"));
    }

    #[test]
    fn test_together_non_sum() {
        let expr = val_call("Divide", vec![val_sym("x"), val_int(2)]);
        let result = builtin_together(&[expr.clone()]).unwrap();
        assert!(result.struct_eq(&expr));
    }

    // ── Cancel tests ──

    #[test]
    fn test_cancel_numeric() {
        let (num, den) = (Integer::from(6), Integer::from(8));
        let val = Value::Rational(Box::new(rug::Rational::from((num, den))));
        let result = builtin_cancel(&[val]).unwrap();
        if let Value::Rational(r) = result {
            assert_eq!(*r.numer(), Integer::from(3));
            assert_eq!(*r.denom(), Integer::from(4));
        } else {
            panic!("Expected Rational");
        }
    }

    #[test]
    fn test_cancel_constant() {
        let result = builtin_cancel(&[val_int(42)]).unwrap();
        assert!(matches!(&result, Value::Integer(n) if *n == Integer::from(42)));
    }

    // ── Collect tests ──

    #[test]
    fn test_collect_basic() {
        // a*x^2 + b*x + c*x + d -> a*x^2 + (b+c)*x + d
        let expr = val_call("Plus", vec![
            val_call("Times", vec![val_sym("a"), val_call("Power", vec![val_sym("x"), val_int(2)])]),
            val_call("Times", vec![val_sym("b"), val_sym("x")]),
            val_call("Times", vec![val_sym("c"), val_sym("x")]),
            val_sym("d"),
        ]);
        let result = builtin_collect(&[expr, val_sym("x")]).unwrap();
        // Should be Plus[Times[a, x^2], Times[Plus[b, c], x], d]
        assert!(matches!(&result, Value::Call { head, .. } if head == "Plus"));
    }

    #[test]
    fn test_collect_like_powers() {
        // a*x^2 + c*x^2 -> (a+c)*x^2
        let expr = val_call("Plus", vec![
            val_call("Times", vec![val_sym("a"), val_call("Power", vec![val_sym("x"), val_int(2)])]),
            val_call("Times", vec![val_sym("c"), val_call("Power", vec![val_sym("x"), val_int(2)])]),
        ]);
        let result = builtin_collect(&[expr, val_sym("x")]).unwrap();
        assert!(matches!(&result, Value::Call { head, .. } if head == "Times"));
    }

    // ── FunctionExpand tests ──

    #[test]
    fn test_function_expand_log_product() {
        // Log[a*b] -> Log[a] + Log[b]
        let expr = val_call("Log", vec![val_call("Times", vec![val_sym("a"), val_sym("b")])]);
        let result = builtin_function_expand(&[expr]).unwrap();
        assert!(matches!(&result, Value::Call { head, .. } if head == "Plus"));
    }

    #[test]
    fn test_function_expand_log_quotient() {
        // Log[a/b] -> Log[a] - Log[b]
        let expr = val_call("Log", vec![val_call("Divide", vec![val_sym("a"), val_sym("b")])]);
        let result = builtin_function_expand(&[expr]).unwrap();
        assert!(matches!(&result, Value::Call { head, .. } if head == "Plus"));
    }

    #[test]
    fn test_function_expand_log_power() {
        // Log[x^n] -> n * Log[x]
        let expr = val_call("Log", vec![val_call("Power", vec![val_sym("x"), val_int(3)])]);
        let result = builtin_function_expand(&[expr]).unwrap();
        assert!(matches!(&result, Value::Call { head, .. } if head == "Times"));
    }

    #[test]
    fn test_function_expand_sin_2x() {
        // Sin[2*x] -> 2*Sin[x]*Cos[x]
        let expr = val_call("Sin", vec![val_call("Times", vec![val_int(2), val_sym("x")])]);
        let result = builtin_function_expand(&[expr]).unwrap();
        assert!(matches!(&result, Value::Call { head, .. } if head == "Times"));
    }

    #[test]
    fn test_function_expand_sin_sum() {
        // Sin[x+y] -> Sin[x]*Cos[y] + Cos[x]*Sin[y]
        let expr = val_call("Sin", vec![val_call("Plus", vec![val_sym("x"), val_sym("y")])]);
        let result = builtin_function_expand(&[expr]).unwrap();
        assert!(matches!(&result, Value::Call { head, .. } if head == "Plus"));
    }

    #[test]
    fn test_function_expand_cos_2x() {
        // Cos[2*x] -> Cos[x]^2 - Sin[x]^2
        let expr = val_call("Cos", vec![val_call("Times", vec![val_int(2), val_sym("x")])]);
        let result = builtin_function_expand(&[expr]).unwrap();
        assert!(matches!(&result, Value::Call { head, .. } if head == "Plus"));
    }

    #[test]
    fn test_function_expand_gamma() {
        // Gamma[n+1] -> n * Gamma[n]
        let expr = val_call("Gamma", vec![val_call("Plus", vec![val_sym("n"), val_int(1)])]);
        let result = builtin_function_expand(&[expr]).unwrap();
        assert!(matches!(&result, Value::Call { head, .. } if head == "Times"));
    }

    #[test]
    fn test_function_expand_bessel_j_half() {
        let half = Value::Rational(Box::new(rug::Rational::from((1_i64, 2_i64))));
        let expr = val_call("BesselJ", vec![half, val_sym("x")]);
        let result = builtin_function_expand(&[expr]).unwrap();
        assert!(matches!(&result, Value::Call { head, .. } if head == "Times"));
    }

    // ── Helper function tests ──

    #[test]
    fn test_substitute() {
        let expr = val_call("Plus", vec![val_sym("x"), val_int(1)]);
        let result = substitute(&expr, "x", &val_int(5));
        let expected = val_call("Plus", vec![val_int(5), val_int(1)]);
        assert!(result.struct_eq(&expected));
    }

    #[test]
    fn test_substitute_nested() {
        let expr = val_call("Power", vec![val_sym("x"), val_int(2)]);
        let result = substitute(&expr, "x", &val_int(3));
        let expected = val_call("Power", vec![val_int(3), val_int(2)]);
        assert!(result.struct_eq(&expected));
    }

    #[test]
    fn test_free_appears_p() {
        let expr = val_call("Plus", vec![val_sym("x"), val_sym("y")]);
        assert!(free_appears_p(&expr, "x"));
        assert!(free_appears_p(&expr, "y"));
        assert!(!free_appears_p(&expr, "z"));
    }

    #[test]
    fn test_free_appears_p_constant() {
        assert!(!free_appears_p(&val_int(5), "x"));
        assert!(!free_appears_p(&val_sym("a"), "x"));
    }

    // ── Apart tests ──

    #[test]
    fn test_apart_quadratic_denom() {
        // 1/((x-1)*(x+1)) -> 1/2/(x-1) - 1/2/(x+1)
        let denom = val_call("Times", vec![
            val_call("Plus", vec![val_sym("x"), val_int(-1)]),
            val_call("Plus", vec![val_sym("x"), val_int(1)]),
        ]);
        let expr = val_call("Divide", vec![val_int(1), denom]);
        let result = builtin_apart(&[expr, val_sym("x")]).unwrap();
        // Should be a sum of two fractions
        assert!(matches!(&result, Value::Call { head, .. } if head == "Plus"));
    }
}
