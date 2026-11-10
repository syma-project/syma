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

/// Build an integer Value.
fn int(n: i64) -> Value {
    Value::Integer(Integer::from(n))
}

// ── Simplify ──

pub fn builtin_simplify(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Err(EvalError::Error(
            "Simplify requires 1 or 2 arguments".to_string(),
        ));
    }
    let simplified = simplify_value(&args[0]);
    // Optional second argument: an `Assumptions -> a` option (or a bare
    // assumption). When present, refine the result under `a`.
    if args.len() == 2
        && let Some(assumption) = extract_assumptions_option(&args[1])
    {
        return Ok(refine_value(&simplified, &assumption));
    }
    if args.len() == 2
        && let Some(assumption) = bare_assumption(&args[1])
    {
        return Ok(refine_value(&simplified, &assumption));
    }
    Ok(simplified)
}

/// Environment-aware `Simplify`: like [`builtin_simplify`] but also honours the
/// ambient `$Assumptions` set by `Assuming[...]` (or `$Assumptions = ...`).
pub fn builtin_simplify_env(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Err(EvalError::Error(
            "Simplify requires 1 or 2 arguments".to_string(),
        ));
    }
    let simplified = simplify_value(&args[0]);
    let assum = effective_assumptions(args.get(1), env);
    if assumptions_present(&assum) {
        Ok(refine_value(&simplified, &assum))
    } else {
        Ok(simplified)
    }
}

/// Combine a caller-supplied second argument with the ambient `$Assumptions`.
pub fn effective_assumptions(second: Option<&Value>, env: &Env) -> Value {
    let explicit =
        second.and_then(|s| extract_assumptions_option(s).or_else(|| bare_assumption(s)));
    let env_assum = env.get("$Assumptions").unwrap_or_else(no_assumptions);
    combine_assumptions(explicit.unwrap_or_else(no_assumptions), env_assum)
}

/// True when `a` carries at least one usable assumption condition.
pub fn assumptions_present(a: &Value) -> bool {
    match a {
        Value::List(items) => !items.is_empty(),
        Value::Symbol(s) if s == "True" => false,
        Value::Null => false,
        _ => true,
    }
}

fn no_assumptions() -> Value {
    Value::List(vec![])
}

/// Conjoin two assumption values, dropping empty ones.
pub fn combine_assumptions(a: Value, b: Value) -> Value {
    if !assumptions_present(&a) {
        return b;
    }
    if !assumptions_present(&b) {
        return a;
    }
    Value::Call {
        head: "And".to_string(),
        args: vec![a, b],
    }
}

/// Interpret a second argument that is not an `Assumptions -> ...` option as a
/// bare assumption, e.g. `Simplify[expr, x > 0]`. Option lists (lists of rules
/// or a bare rule) yield `None`.
fn bare_assumption(second: &Value) -> Option<Value> {
    match second {
        Value::Rule { .. } => None,
        Value::List(items) if items.iter().any(|i| matches!(i, Value::Rule { .. })) => None,
        _ => Some(unwrap_held(second)),
    }
}

/// Extract the value of an `Assumptions -> a` option from an option argument.
/// Accepts a bare rule, or a list of options. Held/pattern wrappers are unwrapped.
fn extract_assumptions_option(opt: &Value) -> Option<Value> {
    match opt {
        Value::Rule { lhs, rhs, .. } => {
            let lhs = unwrap_held(lhs);
            if matches!(lhs, Value::Symbol(s) if s == "Assumptions") {
                Some(unwrap_held(rhs))
            } else {
                None
            }
        }
        Value::List(items) => items.iter().find_map(extract_assumptions_option),
        _ => None,
    }
}

/// Unwrap `Value::Pattern` (and Hold) wrappers to a plain value.
fn unwrap_held(v: &Value) -> Value {
    match v {
        Value::Pattern(p) => crate::pattern::unwrap_expr_to_value(p),
        Value::Hold(inner) | Value::HoldComplete(inner) => unwrap_held(inner),
        _ => v.clone(),
    }
}

// ── Refine (used by Simplify with Assumptions) ──

/// Apply assumption-based refinements to an expression. Delegates to the
/// `Refine` implementation so `Simplify[expr, Assumptions -> a]` and
/// `Refine[expr, a]` stay in lock-step.
fn refine_value(val: &Value, assum: &Value) -> Value {
    crate::builtins::domains::refine_expr(val, assum).unwrap_or_else(|_| val.clone())
}

fn simplify_value(val: &Value) -> Value {
    match val {
        Value::Call { head, args } => {
            let simplified_args: Vec<Value> = args.iter().map(simplify_value).collect();
            let result = simplify_call(head, &simplified_args);
            // Re-canonicalise trig after simplification: the pass above expands
            // multiple angles to single angles to expose cancellations, and
            // `canonicalize_trig` folds the surviving terms back to the
            // lowest-complexity (normally multiple-angle) form.
            if std::env::var_os("SYMA_NO_TRIGCANON").is_some() {
                result
            } else {
                canonicalize_trig(&result)
            }
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
        "Sec" if args.len() == 1 => simplify_call("Power", &[call("Cos", args.to_vec()), int(-1)]),
        "Csc" if args.len() == 1 => simplify_call("Power", &[call("Sin", args.to_vec()), int(-1)]),
        "Log" => simplify_log(args),
        "Exp" => simplify_exp(args),
        "Divide" if args.len() == 2 => {
            if is_zero_value(&args[0]) && !is_zero_value(&args[1]) {
                return Value::Integer(Integer::from(0));
            }
            if matches!(&args[1], Value::Integer(n) if *n == 1) {
                return args[0].clone();
            }
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
    // Distribute products over sums so like terms line up, and expand
    // double-angle trig terms to expose the angle-addition identities.
    let mut expanded: Vec<Value> = Vec::new();
    for term in terms {
        for distributed in distribute_term(term) {
            expanded.extend(reduce_multiple_angle(&distributed));
        }
    }
    let terms = combine_like_terms(expanded);
    let terms = fold_pythagorean(terms);
    if terms.is_empty() {
        return Value::Integer(Integer::from(0));
    }
    if terms.len() == 1 {
        return terms.into_iter().next().unwrap();
    }
    call("Plus", terms)
}

/// Distribute a product over a sum: `Times[c, Plus[a, b]]` → `[c*a, c*b]`.
/// Recurses so nested products distribute fully.
fn distribute_term(term: Value) -> Vec<Value> {
    let expanded = expand_times_over_plus(term);
    if expanded.len() == 1 {
        expanded
    } else {
        expanded.into_iter().flat_map(distribute_term).collect()
    }
}

/// If `arg` is `2*u`, return `u`.
fn double_angle_arg(arg: &Value) -> Option<Value> {
    if let Value::Call { head, args } = arg
        && head == "Times"
        && args.len() == 2
        && matches!(&args[0], Value::Integer(n) if *n == 2)
    {
        Some(args[1].clone())
    } else {
        None
    }
}

/// Expand `Sin[2u]`/`Cos[2u]` into single-angle expressions so that
/// angle-addition identities can be collected.
fn reduce_multiple_angle(term: &Value) -> Vec<Value> {
    match term {
        Value::Call { head, args } if (head == "Sin" || head == "Cos") && args.len() == 1 => {
            if let Some(u) = double_angle_arg(&args[0]) {
                let sq = |f: &str| simplify_call("Power", &[call(f, vec![u.clone()]), int(2)]);
                if head == "Cos" {
                    return vec![
                        sq("Cos"),
                        simplify_call("Times", &[Value::Integer(Integer::from(-1)), sq("Sin")]),
                    ];
                }
                return vec![simplify_call(
                    "Times",
                    &[int(2), call("Sin", vec![u.clone()]), call("Cos", vec![u])],
                )];
            }
            vec![term.clone()]
        }
        _ => vec![term.clone()],
    }
}

/// Recognize a term of the form `c * f[u]^2`, returning `(c, u, f)`.
fn trig_square_parts(term: &Value) -> Option<(Integer, Value, &'static str)> {
    let (coeff, inner) = match term {
        Value::Call { head, args } if head == "Times" && args.len() == 2 => match &args[0] {
            Value::Integer(c) => (c.clone(), &args[1]),
            _ => (Integer::from(1), term),
        },
        _ => (Integer::from(1), term),
    };
    if let Value::Call { head, args } = inner
        && head == "Power"
        && args.len() == 2
        && matches!(&args[1], Value::Integer(n) if *n == 2)
        && let Value::Call { head: fh, args: fa } = &args[0]
        && (fh == "Sin" || fh == "Cos")
        && fa.len() == 1
    {
        let fname = if fh == "Sin" { "Sin" } else { "Cos" };
        return Some((coeff, fa[0].clone(), fname));
    }
    None
}

/// Fold `Sin[u]^2 + Cos[u]^2 -> 1` (with equal coefficients) into the terms.
fn fold_pythagorean(terms: Vec<Value>) -> Vec<Value> {
    for i in 0..terms.len() {
        if let Some((ci, ui, fi)) = trig_square_parts(&terms[i]) {
            let want = if fi == "Sin" { "Cos" } else { "Sin" };
            for j in 0..terms.len() {
                if i == j {
                    continue;
                }
                if let Some((cj, uj, fj)) = trig_square_parts(&terms[j])
                    && fj == want
                    && ui == uj
                    && ci == cj
                {
                    let mut new_terms: Vec<Value> = Vec::new();
                    for (k, t) in terms.iter().enumerate() {
                        if k != i && k != j {
                            new_terms.push(t.clone());
                        }
                    }
                    new_terms.push(Value::Integer(ci));
                    return fold_pythagorean(combine_like_terms(new_terms));
                }
            }
        }
    }
    terms
}

// ── Trig canonicalization ──
//
// Rewrite surviving trig terms toward a canonical, lowest-complexity form:
//   * `Sin[u]^2 + Cos[u]^2`              → `1`               (Pythagorean)
//   * `Cos[u]^2 - Sin[u]^2`              → `Cos[2 u]`         (double angle)
//   * `c - Sin[u]^2` / `c - 2 Sin[u]^2`  → `(c-1) + Cos[u]^2` / `(c-1) + Cos[2 u]`
//   * `c Sin[u] Cos[u]`                  → `(c/2) Sin[2 u]`
//   * `a Sin[u]^2 + b Cos[u]^2` with `a == -b` → `a Cos[2 u]`
//
// `u` may be any expression (including `k x` or a sum), so `Sin[2 (x + y)]`
// and friends are handled. Every rewrite strictly shrinks the term count, and
// the pass is bounded, so it cannot loop.

/// Canonicalise trig subexpressions bottom-up.
fn canonicalize_trig(val: &Value) -> Value {
    let val = match val {
        Value::Call { head, args } => Value::Call {
            head: head.clone(),
            args: args.iter().map(canonicalize_trig).collect(),
        },
        Value::List(items) => Value::List(items.iter().map(canonicalize_trig).collect()),
        _ => val.clone(),
    };
    match &val {
        Value::Call { head, args } if head == "Plus" => {
            let mut terms = args.clone();
            let bound = terms.len() + 1;
            for _ in 0..bound {
                match collapse_trig_sum(&terms) {
                    Some(next) => terms = next,
                    None => break,
                }
            }
            // Combining like terms lets a collapsed `Cos[2 u]` cancel against a
            // pre-existing multiple of it (e.g. `-Cos[2 u] + Cos[2 u] → 0`).
            let terms = fold_integer_constants(combine_like_terms(terms));
            match terms.len() {
                0 => Value::Integer(Integer::from(0)),
                1 => terms.into_iter().next().unwrap(),
                _ => Value::Call {
                    head: "Plus".to_string(),
                    args: terms,
                },
            }
        }
        Value::Call { head, args } if head == "Times" => {
            collapse_trig_product(args).unwrap_or_else(|| val.clone())
        }
        _ => val,
    }
}

/// Sum the integer terms of a `Plus`, leaving the symbolic terms untouched.
fn fold_integer_constants(terms: Vec<Value>) -> Vec<Value> {
    let mut sum = Integer::from(0);
    let mut has_int = false;
    let mut kept: Vec<Value> = Vec::new();
    for t in terms {
        if let Value::Integer(n) = t {
            sum += n;
            has_int = true;
        } else {
            kept.push(t);
        }
    }
    if has_int {
        kept.push(Value::Integer(sum));
    }
    kept
}

/// Match a factor of the form `Sin[u]` or `Cos[u]`, returning `(is_sin, u)`.
fn trig_factor(v: &Value) -> Option<(bool, Value)> {
    if let Value::Call { head, args } = v
        && args.len() == 1
    {
        if head == "Sin" {
            return Some((true, args[0].clone()));
        }
        if head == "Cos" {
            return Some((false, args[0].clone()));
        }
    }
    None
}

/// Decompose a term into `(integer_coeff, is_sin, u)` if it is a numeric
/// multiple of `Sin[u]^2` or `Cos[u]^2`.
fn trig_square_term(term: &Value) -> Option<(Integer, bool, Value)> {
    let (coeff, base) = match term {
        Value::Call { head, args } if head == "Times" && args.len() == 2 => match &args[0] {
            Value::Integer(c) => (c.clone(), &args[1]),
            _ => (Integer::from(1), term),
        },
        _ => (Integer::from(1), term),
    };
    if let Value::Call { head, args } = base
        && head == "Power"
        && args.len() == 2
        && matches!(&args[1], Value::Integer(n) if *n == 2)
        && let Some((is_sin, u)) = trig_factor(&args[0])
    {
        return Some((coeff, is_sin, u));
    }
    None
}

/// `2 * u`, normalised through `simplify_call` so `2*(2 w) -> 4 w`.
fn times_two(u: &Value) -> Value {
    simplify_call("Times", &[int(2), u.clone()])
}

/// Attach `coeff` to `base`, dropping a unit coefficient.
fn scale_term(coeff: Integer, base: Value) -> Value {
    if coeff.is_zero() {
        Value::Integer(Integer::from(0))
    } else if coeff == 1 {
        base
    } else {
        simplify_call("Times", &[Value::Integer(coeff), base])
    }
}

/// Fold one pair/case in a `Plus` (bounded: each fold shrinks the term list).
fn collapse_trig_sum(terms: &[Value]) -> Option<Vec<Value>> {
    let mut squares: Vec<(usize, Integer, bool, Value)> = Vec::new();
    let mut constant = Integer::from(0);
    let mut constant_idx: Vec<usize> = Vec::new();
    for (i, t) in terms.iter().enumerate() {
        if let Some((c, is_sin, u)) = trig_square_term(t) {
            squares.push((i, c, is_sin, u));
        } else if let Value::Integer(n) = t {
            constant += n;
            constant_idx.push(i);
        }
    }

    // Pairwise: `a Cos[u]^2 + b Sin[u]^2`.
    for a in 0..squares.len() {
        for b in (a + 1)..squares.len() {
            if squares[a].2 == squares[b].2 || !commutative_eq(&squares[a].3, &squares[b].3) {
                continue;
            }
            // Normalise so `cos` indexes the Cos^2 term and `sin` the Sin^2 term.
            let (cos, sin) = if squares[a].2 {
                (&squares[b], &squares[a])
            } else {
                (&squares[a], &squares[b])
            };
            let a_coeff = cos.1.clone(); // coefficient on Cos^2
            let b_coeff = sin.1.clone(); // coefficient on Sin^2
            let result = if a_coeff == b_coeff {
                Some(Value::Integer(a_coeff.clone()))
            } else if a_coeff == -b_coeff.clone() {
                Some(scale_term(a_coeff, call("Cos", vec![times_two(&cos.3)])))
            } else {
                None
            };
            if let Some(result) = result {
                let mut new_terms: Vec<Value> = terms
                    .iter()
                    .enumerate()
                    .filter(|(k, _)| *k != cos.0 && *k != sin.0)
                    .map(|(_, t)| t.clone())
                    .collect();
                new_terms.push(result);
                return Some(new_terms);
            }
        }
    }

    // Single square plus a constant. Uses `1 - Sin[u]^2 = Cos[u]^2`,
    // `1 - 2 Sin[u]^2 = Cos[2 u]`, `2 Cos[u]^2 - 1 = Cos[2 u]` and sign
    // variants, requiring a constant term so the rewrite can only help.
    if squares.len() == 1 && !constant_idx.is_empty() {
        let (idx, coeff, is_sin, u) = &squares[0];
        let cos2 = call("Cos", vec![times_two(u)]);
        let (residual, result): (Integer, Option<Value>) = if *is_sin {
            if *coeff == 2 {
                (
                    Integer::from(&constant + 1),
                    Some(scale_term(Integer::from(-1), cos2)),
                )
            } else if *coeff == -2 {
                (
                    Integer::from(&constant - 1),
                    Some(scale_term(Integer::from(1), cos2)),
                )
            } else if *coeff == -1 {
                (
                    Integer::from(&constant - 1),
                    Some(simplify_call(
                        "Power",
                        &[call("Cos", vec![u.clone()]), int(2)],
                    )),
                )
            } else {
                (constant.clone(), None)
            }
        } else if *coeff == 2 {
            (
                Integer::from(&constant + 1),
                Some(scale_term(Integer::from(1), cos2)),
            )
        } else if *coeff == -2 {
            (
                Integer::from(&constant - 1),
                Some(scale_term(Integer::from(-1), cos2)),
            )
        } else if *coeff == -1 {
            (
                Integer::from(&constant - 1),
                Some(simplify_call(
                    "Power",
                    &[call("Sin", vec![u.clone()]), int(2)],
                )),
            )
        } else {
            (constant.clone(), None)
        };
        if let Some(result) = result {
            let mut new_terms: Vec<Value> = Vec::new();
            for (i, t) in terms.iter().enumerate() {
                if i == *idx || constant_idx.contains(&i) {
                    continue;
                }
                new_terms.push(t.clone());
            }
            if !residual.is_zero() {
                new_terms.push(Value::Integer(residual));
            }
            new_terms.push(result);
            return Some(new_terms);
        }
    }
    None
}

/// Fold `c * Sin[u] * Cos[u] → (c/2) Sin[2 u]` inside a product.
fn collapse_trig_product(args: &[Value]) -> Option<Value> {
    let mut coeff = Rational::from(1);
    let mut sin_u: Option<Value> = None;
    let mut cos_u: Option<Value> = None;
    let mut rest: Vec<Value> = Vec::new();
    for f in args {
        match f {
            Value::Integer(n) => coeff *= Rational::from(n.clone()),
            Value::Rational(r) => coeff *= (**r).clone(),
            Value::Real(_) => return None,
            _ => {
                if let Some((is_sin, u)) = trig_factor(f) {
                    if is_sin {
                        if sin_u.is_none() {
                            sin_u = Some(u);
                        } else {
                            rest.push(f.clone());
                        }
                    } else if cos_u.is_none() {
                        cos_u = Some(u);
                    } else {
                        rest.push(f.clone());
                    }
                } else {
                    rest.push(f.clone());
                }
            }
        }
    }
    let u = sin_u?;
    let cu = cos_u?;
    if !commutative_eq(&u, &cu) || coeff.is_zero() {
        return None;
    }
    let half = coeff / Rational::from(2);
    let half_value = if half.is_integer() {
        Value::Integer(half.numer().clone())
    } else {
        Value::Rational(Box::new(half))
    };
    let sin2 = call("Sin", vec![times_two(&u)]);
    let mut factors: Vec<Value> = rest;
    if !matches!(&half_value, Value::Integer(n) if *n == 1) {
        factors.push(half_value);
    }
    factors.push(sin2);
    Some(simplify_call("Times", &factors))
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
            if commutative_eq(&g.0, &base) {
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
/// Times[n, ...] with numeric factor(s) → (product_of_rest, numeric_coeff)
/// other → (term, 1)
fn extract_coeff_and_base(term: &Value) -> (Value, Integer) {
    // Rewrite a bare `a/b` as `a * b^-1` first so its numeric factor and
    // power-form base are recovered below.
    if let Value::Call { head, args } = term
        && head == "Divide"
        && args.len() == 2
    {
        let normalized = simplify_call(
            "Times",
            &[
                args[0].clone(),
                simplify_call("Power", &[args[1].clone(), int(-1)]),
            ],
        );
        return extract_coeff_and_base(&normalized);
    }
    let (base, coeff) = match term {
        Value::Integer(n) => (Value::Integer(Integer::from(1)), n.clone()),
        Value::Call { head, args } if head == "Times" => {
            let mut coeff = Integer::from(1);
            let mut base: Vec<Value> = Vec::new();
            for arg in args {
                match arg {
                    Value::Integer(n) => coeff *= n,
                    _ => base.push(arg.clone()),
                }
            }
            if base.is_empty() {
                (Value::Integer(Integer::from(1)), coeff)
            } else if base.len() == 1 {
                (base.pop().unwrap(), coeff)
            } else {
                (
                    Value::Call {
                        head: "Times".to_string(),
                        args: base,
                    },
                    coeff,
                )
            }
        }
        _ => (term.clone(), Integer::from(1)),
    };
    (normalize_division(&base), coeff)
}

/// Rewrite `a/b` as `a * b^-1` so terms written with `Divide` compare equal to
/// their power-form equivalents.
fn normalize_division(v: &Value) -> Value {
    match v {
        Value::Call { head, args } if head == "Divide" && args.len() == 2 => simplify_call(
            "Times",
            &[
                args[0].clone(),
                simplify_call("Power", &[args[1].clone(), int(-1)]),
            ],
        ),
        _ => v.clone(),
    }
}

/// Structural equality that treats `Plus` and `Times` arguments as unordered.
fn commutative_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Call { head: ha, args: aa }, Value::Call { head: hb, args: ab })
            if ha == hb && aa.len() == ab.len() =>
        {
            if ha == "Plus" || ha == "Times" {
                let mut used = vec![false; ab.len()];
                'outer: for x in aa {
                    for (i, y) in ab.iter().enumerate() {
                        if !used[i] && commutative_eq(x, y) {
                            used[i] = true;
                            continue 'outer;
                        }
                    }
                    return false;
                }
                true
            } else {
                aa.iter().zip(ab).all(|(x, y)| commutative_eq(x, y))
            }
        }
        _ => a == b,
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
            Value::Integer(n) if *n == 1 => {}
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
        Value::Integer(n) => *n == 1,
        Value::Real(r) => *r == Float::with_val(DEFAULT_PRECISION, 1.0),
        Value::Rational(r) => **r == 1,
        _ => false,
    };
    if !is_one {
        result.push(numeric_product);
    }
    for (base, exp) in base_exp {
        if exp.is_zero() {
            continue;
        }
        if exp == 1 {
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
            Value::Real(r) if *r == Float::with_val(DEFAULT_PRECISION, 1) => {
                Value::Integer(Integer::from(1))
            }
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
        // sin(-x) -> -sin(x)
        Value::Call { head, args: a } if head == "Times" && !a.is_empty() => {
            if let Value::Integer(n) = &a[0]
                && *n == -1
                && a.len() == 2
            {
                return simplify_call(
                    "Times",
                    &[Value::Integer((-1).into()), call("Sin", vec![a[1].clone()])],
                );
            }
            call_ref("Sin", args)
        }
        _ => call_ref("Sin", args),
    }
}

fn simplify_cos(args: &[Value]) -> Value {
    if args.len() != 1 {
        return call_ref("Cos", args);
    }
    match &args[0] {
        Value::Integer(n) if n.is_zero() => Value::Integer(Integer::from(1)),
        // cos(-x) -> cos(x)
        Value::Call { head, args: a } if head == "Times" && !a.is_empty() => {
            if let Value::Integer(n) = &a[0]
                && *n == -1
                && a.len() == 2
            {
                return call("Cos", vec![a[1].clone()]);
            }
            call_ref("Cos", args)
        }
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
        _ => match signed_infinity(&args[0]) {
            Some(s) if s < 0 => Value::Integer(Integer::from(0)),
            Some(_) => Value::Symbol("Infinity".to_string()),
            None => call_ref("Exp", args),
        },
    }
}

/// If `v` is `±Infinity` (optionally `c*Infinity`), return the sign (+1 / -1).
fn signed_infinity(v: &Value) -> Option<i32> {
    match v {
        Value::Symbol(s) if s == "Infinity" => Some(1),
        Value::Call { head, args } if head == "Times" => {
            let mut sign = 1;
            let mut has_inf = false;
            for f in args {
                match f {
                    Value::Symbol(s) if s == "Infinity" => has_inf = true,
                    Value::Integer(n) if n.is_negative() => {
                        sign = -sign;
                    }
                    Value::Real(r) if r.is_sign_negative() && !r.is_zero() => {
                        sign = -sign;
                    }
                    Value::Rational(r) if r.is_negative() => {
                        sign = -sign;
                    }
                    _ => {}
                }
            }
            if has_inf { Some(sign) } else { None }
        }
        _ => None,
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
                "Times" => match expanded_args.len() {
                    0 => Value::Integer(Integer::from(1)),
                    1 => expanded_args.into_iter().next().unwrap(),
                    2 => expand_times(&expanded_args),
                    // Fold pairwise so products of 3+ factors (that may contain
                    // sums) still distribute fully.
                    _ => {
                        let mut it = expanded_args.into_iter();
                        let mut acc = it.next().unwrap();
                        for next in it {
                            acc = expand_times(&[acc, next]);
                        }
                        acc
                    }
                },
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
    let mut remainder: Vec<Value> = num_coeffs.iter().map(simplify_value).collect();
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
        for (k, den_k) in den_coeffs.iter().enumerate() {
            let idx = (i + k as i64) as usize;
            if idx < remainder.len() {
                let sub = expand_value(&simplify_call("Times", &[q.clone(), den_k.clone()]));
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
    let all_zero = remainder.iter().all(is_zero_value);
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
        && (0..=10).contains(&n_i64)
    {
        if plus_args.len() != 2 {
            // General (multinomial) case: repeated distribution.
            let mut acc = Value::Integer(Integer::from(1));
            for _ in 0..n_i64 {
                acc = expand_times(&[acc, base.clone()]);
            }
            return acc;
        }
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
    let mut unwrapped: Vec<Value> = args
        .iter()
        .map(|a| match a {
            Value::Hold(inner) | Value::HoldComplete(inner) => (**inner).clone(),
            Value::Pattern(p) => crate::pattern::unwrap_expr_to_value(p),
            _ => a.clone(),
        })
        .collect();

    // D holds its arguments, but the expression to differentiate must be
    // evaluated first so composite forms such as `Integrate[x^2, x]` or
    // `f[x]` (with `f` defined) resolve before the differentiation rules run.
    // The variable specification is deliberately left symbolic.
    if let Value::Pattern(expr) = &args[0]
        && let Ok(evaluated) = crate::eval::eval(expr, env)
    {
        unwrapped[0] = evaluated;
    }

    // Check if second argument is a List {x, n} or {x, n, x0}
    if let Value::List(items) = &unwrapped[1]
        && !items.is_empty()
        && items.len() >= 2
        && items.len() <= 3
    {
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
                result = eval_d_sequence(result, &unwrapped[2..], env)?;
            }
            return Ok(substitute_and_eval(&result, &var_name, &items[2]));
        }
        if unwrapped.len() > 2 {
            result = eval_d_sequence(result, &unwrapped[2..], env)?;
        }
        return Ok(result);
    }

    // Multi-arg: D[f, x, y, z, ...] — differentiate w.r.t. each variable. A
    // trailing positive integer after a variable denotes an order, so
    // D[f, x, 2] is the 2nd derivative w.r.t. x.
    if unwrapped.len() > 2 {
        return eval_d_sequence(unwrapped[0].clone(), &unwrapped[1..], env);
    }

    // 2-arg case: D[f, x]
    eval_d_2(unwrapped[0].clone(), unwrapped[1].clone(), env)
}

/// Differentiate `expr` following a sequence of variable/order specifications.
/// A spec element that is a non-negative integer sets the repetition count for
/// the preceding variable; otherwise each variable is differentiated once.
fn eval_d_sequence(mut result: Value, specs: &[Value], env: &Env) -> Result<Value, EvalError> {
    let mut i = 0;
    while i < specs.len() {
        let var = specs[i].clone();
        i += 1;
        let mut order = 1usize;
        if i < specs.len()
            && let Some(k) = specs[i].to_integer()
            && k >= 0
        {
            order = k as usize;
            i += 1;
        }
        for _ in 0..order {
            result = eval_d_2(result, var.clone(), env)?;
        }
    }
    Ok(result)
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
        && h == "D"
        && args.len() == 2
    {
        // Call eval_d_2 directly with already-unwrapped args
        let inner_result = eval_d_2(args[0].clone(), args[1].clone(), env)?;
        // If the inner derivative did not reduce (e.g. no rule matched in a bare
        // environment), re-differentiating would loop forever; stop here.
        if inner_result == concrete {
            return Ok(inner_result);
        }
        return eval_d_2(inner_result, var, env);
    }

    // Flatten nested Plus/Times (parser builds nested, Flat flattens at runtime)
    let flattened = match &concrete {
        Value::Call { head, args } if (head == "Plus" || head == "Times") && args.len() >= 2 => {
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

    // Normalize Divide[a, b] → Times[a, Power[b, -1]] so D rules can match
    let concrete = match &concrete {
        Value::Call { head, args } if head == "Divide" && args.len() == 2 => Value::Call {
            head: "Times".to_string(),
            args: vec![
                args[0].clone(),
                Value::Call {
                    head: "Power".to_string(),
                    args: vec![args[1].clone(), Value::Integer(Integer::from(-1))],
                },
            ],
        },
        _ => concrete,
    };

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
        // Use the normalized concrete expression (Divide→Times, flattened) for matching.
        // For the variable, evaluate Pattern to resolve symbolic names from rule bodies.
        let concrete_var = match &var {
            Value::Pattern(p) => crate::eval::eval(p, env)?,
            Value::Hold(inner) | Value::HoldComplete(inner) => (**inner).clone(),
            _ => var.clone(),
        };
        return match crate::eval::apply_function(
            &d_func,
            &[concrete.clone(), concrete_var.clone()],
            env,
        ) {
            Ok(v) => Ok(v),
            Err(EvalError::NoMatch { .. }) => {
                // No D rule matched — return unevaluated D with concrete (unwrapped) args
                Ok(Value::Call {
                    head: "D".to_string(),
                    args: vec![concrete, concrete_var],
                })
            }
            Err(e) => Err(e),
        };
    }
    // Use concrete (unwrapped, normalized) values for the unevaluated return
    let concrete_var = match &var {
        Value::Pattern(p) => crate::pattern::unwrap_expr_to_value(p),
        Value::Hold(inner) | Value::HoldComplete(inner) => (**inner).clone(),
        _ => var.clone(),
    };
    Ok(Value::Call {
        head: "D".to_string(),
        args: vec![concrete, concrete_var],
    })
}

/// Create a Plus of values (single arg → that arg, two args → Plus[a,b]).
/// Explicit zero terms are dropped so the Rust differentiation fast paths do
/// not produce `Plus[..., 0]` (which would otherwise fall through the `D` rules).
fn make_plus(items: Vec<Value>) -> Value {
    let items: Vec<Value> = items.into_iter().filter(|v| !is_zero(v)).collect();
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
    if items.iter().any(is_zero) {
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
    use std::cell::Cell;
    thread_local! {
        // Evaluating the definitions below can re-enter `D` (e.g. while the
        // LHS `D[c_, x_]` is inspected), so guard against recursive reloads.
        static LOADING: Cell<bool> = const { Cell::new(false) };
    }
    // Already loaded (D.syma replaces the builtin with a `Function`): avoid
    // re-evaluating the definitions, which would re-enter `D` and diverge.
    if matches!(env.get("D"), Some(Value::Function(_))) {
        return Ok(());
    }
    if LOADING.with(Cell::get) {
        return Ok(());
    }
    LOADING.with(|c| c.set(true));
    let result = lazy_load_d_inner(env);
    LOADING.with(|c| c.set(false));
    result
}

fn lazy_load_d_inner(env: &Env) -> Result<(), EvalError> {
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
    if let Some(d_func) = env.get("D")
        && let Value::Function(_) = &d_func
    {
        env.root_env().set("D".to_string(), d_func);
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

    // Normalize Divide[a, b] → Times[a, Power[b, -1]] for Rubi rule matching
    fn normalize_divide(expr: Value) -> Value {
        match &expr {
            Value::Call { head, args: dargs } if head == "Divide" && dargs.len() == 2 => {
                Value::Call {
                    head: "Times".to_string(),
                    args: vec![
                        dargs[0].clone(),
                        Value::Call {
                            head: "Power".to_string(),
                            args: vec![dargs[1].clone(), Value::Integer(Integer::from(-1))],
                        },
                    ],
                }
            }
            _ => expr,
        }
    }
    let expr = normalize_divide(expr);

    // Evaluate the integrand before matching Rubi rules. Rules often embed
    // helper calls in an integrand they re-integrate, e.g.
    // `Integrate[FunctionOfExponentialFunction[u, x]/x, x]`; evaluating first
    // turns that into `Integrate[1, x]` and lets trivial cancellations (`x/x`)
    // be recognized.
    let expr = match crate::eval::eval(&crate::eval::table::value_to_expr(&expr), env) {
        Ok(v) => normalize_divide(v),
        Err(_) => expr,
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
    // Prefer the closed-form hardcoded integrator for the forms it fully
    // handles; fall through to the Rubi rule set otherwise. Rubi's generic
    // fallback rules are not always applicable (e.g. `x Exp[x]`,
    // `1/(1+x^2)`), whereas the hardcoded forms are exact.
    let hardcoded = integrate(&expr, &var_name);
    let hardcoded_ok = !matches!(&hardcoded, Value::Call { head, .. } if head == "Integrate");
    // The generic Rubi exponential-substitution rule hijacks `x^g(x)` and
    // returns a spurious (non-)antiderivative, e.g. `x^x` came back as the
    // integrand itself. Prefer the unevaluated hardcoded form for that shape.
    let prefer_hardcoded = hardcoded_ok || is_var_power_of_var(&expr, &var_name);
    let mut antideriv = if prefer_hardcoded {
        Some(hardcoded.clone())
    } else {
        None
    };

    // Dispatch to Integrate function definitions (loaded from .syma files)
    if antideriv.is_none()
        && let Some(Value::Function(func_def)) = env.get("$IntegrateRules")
    {
        for (def_idx, def) in func_def.definitions.iter().enumerate() {
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
                if std::env::var_os("SYMA_INTEG_DEBUG").is_some() {
                    eprintln!("MATCH[{}] expr={} body={}", def_idx, expr, def.body);
                }
                let child_env = env.child();
                for (name, value) in &bindings {
                    child_env.set(name.clone(), value.clone());
                }
                match crate::eval::eval(&def.body, &child_env) {
                    // A body whose result is a bare guard (`expr /; cond`) acts
                    // as a definition condition: fall through to the next rule
                    // when it does not hold (same semantics as `apply_function`).
                    Ok(result) => match crate::eval::resolve_body_condition(result, &child_env) {
                        Some(v) => {
                            antideriv = Some(v);
                            break;
                        }
                        None => continue,
                    },
                    Err(_) => continue,
                }
            }
        }
    }

    // Last resort: hardcoded result (which may itself be an unevaluated
    // `Integrate[...]`).
    if antideriv.is_none() {
        antideriv = Some(hardcoded);
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
    // Evaluate the Newton–Leibniz difference so numeric residue such as
    // `1/3 + (-1) 0/3` collapses to `1/3` (and `Infinity` limits resolve).
    let result = simplify_call("Plus", &[f_upper, neg_lower]);
    match crate::eval::eval(&crate::eval::table::value_to_expr(&result), env) {
        Ok(v) => Ok(v),
        Err(_) => Ok(result),
    }
}

/// Lazy-load Integrate rules from .syma files (like lazy_load_d for D).
fn lazy_load_integrate(env: &Env) -> Result<(), EvalError> {
    // Already loaded into this environment (or one of its ancestors)?
    if env.get("$IntegrateRules").is_some() {
        return Ok(());
    }
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
    // The .syma files define `Integrate[...] := ...`, which replaces the
    // builtin with a `Value::Function`. Restore the builtin as the dispatcher
    // (so nested/recursive `Integrate` calls still reach the hardcoded
    // fallback), and keep the loaded rules under `$IntegrateRules`.
    if let Some(func @ Value::Function(_)) = env.root_env().get("Integrate") {
        env.root_env().set("$IntegrateRules".to_string(), func);
        env.root_env().set(
            "Integrate".to_string(),
            Value::Builtin(
                "Integrate".to_string(),
                crate::value::BuiltinFn::Env(builtin_integrate),
            ),
        );
    }
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
                if let Ok(source) = std::fs::read_to_string(&candidate)
                    && let Ok(tokens) = crate::lexer::tokenize(&source)
                    && let Ok(stmts) = crate::parser::parse_with_suppress(tokens)
                {
                    for (expr, _suppress) in stmts {
                        let _ = crate::eval::eval(&expr, env);
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
            if let Ok(source) = std::fs::read_to_string(&candidate)
                && let Ok(tokens) = crate::lexer::tokenize(&source)
                && let Ok(stmts) = crate::parser::parse_with_suppress(tokens)
            {
                for (expr, _suppress) in stmts {
                    let _ = crate::eval::eval(&expr, env);
                }
            }
            return;
        }
    }

    // Fallback: embedded source
    if let Some(source) = embedded_integrate_source(fname)
        && let Ok(tokens) = crate::lexer::tokenize(&source)
        && let Ok(stmts) = crate::parser::parse_with_suppress(tokens)
    {
        for (expr, _suppress) in stmts {
            let _ = crate::eval::eval(&expr, env);
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
                } else if vars.len() == 2
                    && let Some(parts) = integrate_poly_times_exp(&vars, var)
                {
                    let const_vals: Vec<Value> = constants.iter().map(|c| (*c).clone()).collect();
                    let const_product = if constants.is_empty() {
                        Value::Integer(Integer::from(1))
                    } else {
                        simplify_call("Times", &const_vals)
                    };
                    simplify_call("Times", &[const_product, parts])
                } else {
                    call("Integrate", vec![expr.clone(), x])
                }
            }
            // ∫ b^u dx = b^u / (u' Log[b]) for a constant base `b` and a
            // linear exponent `u` (e.g. `a^x`, `E^x`, `2^(3 x + 1)`).
            // Non-linear exponents (`E^(x^2)`) fall through unevaluated.
            "Power"
                if args.len() == 2
                    && is_constant_wrt(&args[0], var)
                    && !is_constant_wrt(&args[1], var) =>
            {
                if let Some((a, _b)) = try_extract_linear(&args[1], var)
                    && !a.struct_eq(&Value::Integer(Integer::from(0)))
                {
                    let log_base = simplify_call("Log", &[args[0].clone()]);
                    let inv = simplify_call(
                        "Times",
                        &[
                            simplify_call("Power", &[a, Value::Integer(Integer::from(-1))]),
                            simplify_call("Power", &[log_base, Value::Integer(Integer::from(-1))]),
                        ],
                    );
                    simplify_call("Times", &[expr.clone(), inv])
                } else {
                    call("Integrate", vec![expr.clone(), x])
                }
            }
            // ∫ dx/(a + b x^2) = 1/Sqrt[a b] ArcTan[Sqrt[b/a] x]
            "Power"
                if args.len() == 2
                    && args[1].struct_eq(&Value::Integer(Integer::from(-1)))
                    && !args[0].struct_eq(&x) =>
            {
                if let Some(r) = integrate_reciprocal_quadratic(&args[0], var) {
                    r
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
                // Symbolic (or rational) constant exponent: ∫ x^e dx =
                // x^(e+1)/(e+1), valid for e != -1.
                _ if is_constant_wrt(&args[1], var) => {
                    let new_exp =
                        simplify_call("Plus", &[args[1].clone(), Value::Integer(Integer::from(1))]);
                    simplify_call(
                        "Times",
                        &[
                            simplify_call("Power", &[x.clone(), new_exp.clone()]),
                            simplify_call("Power", &[new_exp, Value::Integer(Integer::from(-1))]),
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

/// Try to recognise `x^m * Exp[a*x + b]` (m a non-negative integer, `a != 0`)
/// and return its antiderivative via integration by parts.
fn integrate_poly_times_exp(vars: &[&Value], var: &str) -> Option<Value> {
    let x = Value::Symbol(var.to_string());
    let mut m: Option<i64> = None;
    let mut exp_factor: Option<Value> = None;
    let mut k: Option<Value> = None;
    for v in vars {
        if let Some(mm) = poly_power_of_var(v, var) {
            if m.is_some() {
                return None;
            }
            m = Some(mm);
        } else if let Value::Call { head, args } = v {
            if head == "Exp" && args.len() == 1 {
                if exp_factor.is_some() {
                    return None;
                }
                let (a, _b) = try_extract_linear(&args[0], var)?;
                if a.struct_eq(&Value::Integer(Integer::from(0))) {
                    return None;
                }
                k = Some(a);
                exp_factor = Some((*v).clone());
            } else {
                return None;
            }
        } else {
            return None;
        }
    }
    let m = m?;
    let k = k?;
    let exp_factor = exp_factor?;
    // ∫ x^m e^{k x} dx = e^{k x} Σ_{j=0}^{m} (-1)^j m!/(m-j)! x^{m-j} / k^{j+1}
    let mut terms: Vec<Value> = Vec::new();
    for j in 0..=m {
        let sign = if j % 2 == 0 { 1 } else { -1 };
        let mut ratio = Integer::from(1);
        for t in 0..j {
            ratio *= Integer::from(m - t);
        }
        let coeff = Value::Integer(Integer::from(sign) * ratio);
        let inv_k = simplify_call(
            "Power",
            &[k.clone(), Value::Integer(Integer::from(-(j + 1)))],
        );
        let xpow = if m - j == 0 {
            Value::Integer(Integer::from(1))
        } else {
            simplify_call("Power", &[x.clone(), Value::Integer(Integer::from(m - j))])
        };
        terms.push(simplify_call("Times", &[coeff, inv_k, xpow]));
    }
    let poly = simplify_call("Plus", &terms);
    Some(simplify_call("Times", &[exp_factor, poly]))
}

/// If `v` is `x^m` for a non-negative integer `m` (or `x`), return `m`.
fn poly_power_of_var(v: &Value, var: &str) -> Option<i64> {
    match v {
        Value::Symbol(s) if s == var => Some(1),
        Value::Call { head, args }
            if head == "Power"
                && args.len() == 2
                && matches!(&args[0], Value::Symbol(s) if s == var) =>
        {
            match &args[1] {
                Value::Integer(n) if !n.is_negative() => n.to_i64(),
                _ => None,
            }
        }
        _ => None,
    }
}

/// ∫ dx/(a + b*x^2) = 1/Sqrt[a*b] * ArcTan[Sqrt[b/a] * x] for constant a, b > 0.
fn integrate_reciprocal_quadratic(base: &Value, var: &str) -> Option<Value> {
    let x = Value::Symbol(var.to_string());
    let (a, b) = match quadratic_reciprocal_parts(base, var) {
        Some(ab) => ab,
        None => {
            let coeffs = crate::builtins::symbolic::extract_polynomial_coeffs(base, var);
            if coeffs.len() != 3 {
                return None;
            }
            (coeffs[0].clone(), coeffs[2].clone())
        }
    };
    if is_negative_const(&a) || is_negative_const(&b) {
        return None;
    }
    let ab = simplify_call("Times", &[a.clone(), b.clone()]);
    let sqrt_ab = simplify_call(
        "Power",
        &[ab, Value::Rational(Box::new(Rational::from((1, 2))))],
    );
    let b_over_a = simplify_call(
        "Times",
        &[
            b,
            simplify_call("Power", &[a, Value::Integer(Integer::from(-1))]),
        ],
    );
    let sqrt_ba = simplify_call(
        "Power",
        &[b_over_a, Value::Rational(Box::new(Rational::from((1, 2))))],
    );
    let arg = simplify_call("Times", &[sqrt_ba, x]);
    let atan = simplify_call("ArcTan", &[arg]);
    Some(simplify_call(
        "Times",
        &[
            simplify_call("Power", &[sqrt_ab, Value::Integer(Integer::from(-1))]),
            atan,
        ],
    ))
}

/// Split `a + b*x^2` (any order, possibly with Real exponents from `N`) into
/// `(a, b)`. Returns `None` when `base` is not of this shape.
fn quadratic_reciprocal_parts(base: &Value, var: &str) -> Option<(Value, Value)> {
    let x = Value::Symbol(var.to_string());
    let is_x_squared = |v: &Value| -> Option<Value> {
        let term_is_x2 = |f: &Value| -> bool {
            match f {
                Value::Call { head, args } if head == "Power" && args.len() == 2 => {
                    args[0].struct_eq(&x) && exponent_is_two(&args[1])
                }
                _ => false,
            }
        };
        match v {
            _ if term_is_x2(v) => Some(Value::Integer(Integer::from(1))),
            Value::Call { head, args } if head == "Times" => {
                let mut coeff = Value::Integer(Integer::from(1));
                let mut found = false;
                for f in args {
                    if !found && term_is_x2(f) {
                        found = true;
                    } else {
                        coeff = simplify_call("Times", &[coeff, f.clone()]);
                    }
                }
                if found { Some(coeff) } else { None }
            }
            _ => None,
        }
    };
    if let Value::Call { head, args } = base
        && head == "Plus"
        && args.len() == 2
    {
        if let Some(b) = is_x_squared(&args[1])
            && is_constant_wrt(&args[0], var)
        {
            return Some((args[0].clone(), b));
        }
        if let Some(b) = is_x_squared(&args[0])
            && is_constant_wrt(&args[1], var)
        {
            return Some((args[1].clone(), b));
        }
    }
    None
}

/// True if `e` is exactly 2 (as an Integer or a Real 2.0).
fn exponent_is_two(e: &Value) -> bool {
    match e {
        Value::Integer(n) => *n == 2,
        Value::Real(r) => *r == Float::with_val(DEFAULT_PRECISION, 2),
        _ => false,
    }
}

fn is_negative_const(v: &Value) -> bool {
    match v {
        Value::Integer(n) => n.is_negative(),
        Value::Real(r) => r.is_sign_negative() && !r.is_zero(),
        _ => false,
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

/// True for `var^g(var)` with a non-constant exponent `g` — the shape
/// `∫ x^g(x) dx` for which Mathematica (and Rubi) leave the integral
/// unevaluated (`x^x` being the canonical example).
fn is_var_power_of_var(expr: &Value, var: &str) -> bool {
    let x = Value::Symbol(var.to_string());
    matches!(
        expr,
        Value::Call { head, args }
            if head == "Power"
                && args.len() == 2
                && args[0].struct_eq(&x)
                && !is_constant_wrt(&args[1], var)
    )
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
            let new_a = simplify_call(
                "Plus",
                &[
                    Value::Integer(ai.clone() / &gcd),
                    simplify_call("Times", &[Value::Integer(bi.clone() / &gcd), x]),
                ],
            );
            return simplify_call("Times", &[Value::Integer(gcd), new_a]);
        }
    }
    // Simplify so that Factor[x] returns `x`, not `Plus[0, Times[1, x]]`.
    simplify_call(
        "Plus",
        &[a.clone(), simplify_call("Times", &[b.clone(), x])],
    )
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
        Value::List(items) => Value::List(items.iter().map(strip_pattern).collect()),
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
/// Solve[{eq1, eq2, ...}, {x, y, ...}] — solve a (linear) system.
///
/// Solutions are returned as `{var -> {val1, val2, ...}}`.
pub fn builtin_solve(args: &[Value]) -> Result<Value, EvalError> {
    let args: Vec<Value> = args.iter().map(strip_pattern).collect();
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Solve requires exactly 2 arguments".to_string(),
        ));
    }
    match &args[1] {
        Value::Symbol(var) => solve_single(&args[0], var, &args),
        Value::List(vars) => solve_system(&args[0], vars, &args),
        _ => Err(EvalError::TypeError {
            expected: "Symbol or List of Symbols".to_string(),
            got: args[1].type_name().to_string(),
        }),
    }
}

fn solve_single(eq: &Value, var: &str, orig: &[Value]) -> Result<Value, EvalError> {
    let (lhs, rhs) = match eq {
        Value::Call { head, args } if head == "Equal" && args.len() == 2 => {
            (args[0].clone(), args[1].clone())
        }
        _ => return Ok(call("Solve", orig.to_vec())),
    };
    let poly = simplify_call("Plus", &[lhs, simplify_call("Times", &[int(-1), rhs])]);
    Ok(wrap_solution_values(solve_polynomial(&poly, var), var))
}

/// Collect the RHS values of a list of rules into `{var -> {values}}`.
fn wrap_solution_values(result: Value, var: &str) -> Value {
    match result {
        Value::List(items) if items.iter().all(|i| matches!(i, Value::Rule { .. })) => {
            let values: Vec<Value> = items
                .into_iter()
                .map(|item| match item {
                    Value::Rule { rhs, .. } => *rhs,
                    _ => unreachable!(),
                })
                .collect();
            Value::List(vec![Value::Rule {
                lhs: Box::new(Value::Symbol(var.to_string())),
                rhs: Box::new(Value::List(values)),
                delayed: false,
            }])
        }
        other => other,
    }
}

/// Solve a system of (linear) equations in several variables.
fn solve_system(eq: &Value, vars: &[Value], orig: &[Value]) -> Result<Value, EvalError> {
    let names: Vec<String> = vars
        .iter()
        .map(|v| match v {
            Value::Symbol(s) => Ok(s.clone()),
            _ => Err(EvalError::TypeError {
                expected: "Symbol".to_string(),
                got: v.type_name().to_string(),
            }),
        })
        .collect::<Result<_, _>>()?;
    if names.is_empty() {
        return Err(EvalError::Error("Solve: empty variable list".to_string()));
    }

    let equations: Vec<Value> = match eq {
        Value::List(items) => items.clone(),
        Value::Call { head, .. } if head == "Equal" => vec![eq.clone()],
        _ => return Ok(call("Solve", orig.to_vec())),
    };

    // Build linear rows `(coeffs, constant)` from each equation.
    let mut rows: Vec<(Vec<Integer>, Integer)> = Vec::with_capacity(equations.len());
    for equation in &equations {
        let expr = match equation {
            Value::Call { head, args } if head == "Equal" && args.len() == 2 => simplify_call(
                "Plus",
                &[
                    args[0].clone(),
                    simplify_call("Times", &[int(-1), args[1].clone()]),
                ],
            ),
            _ => return Ok(call("Solve", orig.to_vec())),
        };
        match linear_coeffs(&expr, &names) {
            Some(row) => rows.push(row),
            None => return Ok(call("Solve", orig.to_vec())),
        }
    }

    match solve_linear_system(rows) {
        Some(values) => Ok(Value::List(vec![Value::Rule {
            lhs: Box::new(Value::Symbol(names[0].clone())),
            rhs: Box::new(Value::List(values)),
            delayed: false,
        }])),
        None => Ok(call("Solve", orig.to_vec())),
    }
}

/// Extract integer coefficients and the constant term of a linear expression:
/// `sum(coeffs[i] * names[i]) + constant`.
fn linear_coeffs(expr: &Value, names: &[String]) -> Option<(Vec<Integer>, Integer)> {
    let terms = flatten_to_plus_terms(expr);
    let mut coeffs = vec![Integer::from(0); names.len()];
    let mut constant = Integer::from(0);
    for term in terms {
        let (coeff, base) = split_coeff_var(&term);
        match &base {
            Value::Integer(_) => constant += coeff,
            Value::Symbol(s) => {
                let idx = names.iter().position(|n| n == s)?;
                coeffs[idx] += coeff;
            }
            Value::Call { head, args }
                if head == "Times" && args.iter().all(|a| matches!(a, Value::Symbol(_))) =>
            {
                // Product of distinct variables is not linear — bail out.
                return None;
            }
            _ => return None,
        }
    }
    Some((coeffs, constant))
}

/// Solve `rows` (augmented linear system) exactly via Gaussian elimination.
/// Returns the unique solution vector, or `None` if singular/indeterminate.
#[allow(clippy::needless_range_loop)]
fn solve_linear_system(rows: Vec<(Vec<Integer>, Integer)>) -> Option<Vec<Value>> {
    let n = rows.len();
    if n == 0 || rows.iter().any(|(c, _)| c.len() != n) {
        return None;
    }
    let mut m: Vec<Vec<Rational>> = rows
        .into_iter()
        .map(|(coeffs, constant)| {
            let mut row: Vec<Rational> = coeffs.into_iter().map(Rational::from).collect();
            row.push(Rational::from(-constant));
            row
        })
        .collect();

    for col in 0..n {
        let pivot = (col..n).find(|&r| !m[r][col].is_zero())?;
        m.swap(col, pivot);
        let pivot_row = m[col].clone();
        let pivot_val = pivot_row[col].clone();
        for r in 0..n {
            if r == col || m[r][col].is_zero() {
                continue;
            }
            let factor = m[r][col].clone() / &pivot_val;
            for c in col..=n {
                let sub = factor.clone() * &pivot_row[c];
                m[r][c] = m[r][c].clone() - sub;
            }
        }
    }

    let mut values = Vec::with_capacity(n);
    for (i, row) in m.iter().enumerate() {
        if row[i].is_zero() {
            return None;
        }
        let val = row[n].clone() / &row[i];
        values.push(rational_to_value(val));
    }
    Some(values)
}

fn rational_to_value(r: Rational) -> Value {
    if r.is_integer() {
        Value::Integer(r.numer().clone())
    } else {
        Value::Rational(Box::new(r))
    }
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

            // Depress quartic: x = t - b/(4a)  =>  t^4 + p t^2 + q t + r = 0
            let p = c / a - 3.0 * b * b / (8.0 * a * a);
            let q = d / a - b * c / (2.0 * a * a) + b * b * b / (8.0 * a * a * a);
            let r = e / a - 3.0 * b.powi(4) / (256.0 * a.powi(4)) + b * b * c / (16.0 * a.powi(3))
                - b * d / (4.0 * a.powi(3));

            // Resolvent cubic in m: m^3 + 2p m^2 + (p^2 - 4r) m - q^2 = 0.
            // Depress with m = y - 2p/3 to get y^3 + P y + Q = 0.
            let res_a = 2.0 * p;
            let res_b = p * p - 4.0 * r;
            let res_c = -q * q;
            let res_p = res_b - res_a * res_a / 3.0;
            let res_q = 2.0 * res_a * res_a * res_a / 27.0 - res_a * res_b / 3.0 + res_c;

            // A real root y of the depressed resolvent cubic.
            let disc = res_q * res_q / 4.0 + res_p * res_p * res_p / 27.0;
            let y = if disc >= 0.0 {
                (-res_q / 2.0 + disc.sqrt()).cbrt() + (-res_q / 2.0 - disc.sqrt()).cbrt()
            } else {
                // Three real roots: pick the largest so that m = y - 2p/3 > 0.
                let s = (-res_p / 3.0).sqrt();
                let arg = (3.0 * res_q) / (2.0 * res_p) * (-3.0 / res_p).sqrt();
                let theta = arg.clamp(-1.0, 1.0).acos() / 3.0;
                let mut best = f64::NEG_INFINITY;
                for k in 0..3 {
                    let yy =
                        2.0 * s * (theta - 2.0 * std::f64::consts::PI * (k as f64) / 3.0).cos();
                    if yy > best {
                        best = yy;
                    }
                }
                best
            };
            let m = y - 2.0 * p / 3.0;

            let mut roots: Vec<Value> = Vec::new();
            if m > 1e-12 {
                // t^4 + p t^2 + q t + r = (t^2 + s t + c1)(t^2 - s t + c2)
                let s = m.sqrt();
                let c1 = (p + m - q / s) / 2.0;
                let c2 = (p + m + q / s) / 2.0;
                let shift = -b / (4.0 * a);
                for (sq, cc) in [(s, c1), (-s, c2)] {
                    let d2 = sq * sq - 4.0 * cc;
                    if d2 >= 0.0 {
                        let sd = d2.sqrt();
                        let x1 = (-sq + sd) / 2.0 + shift;
                        let x2 = (-sq - sd) / 2.0 + shift;
                        roots.push(Value::Real(Float::with_val(DEFAULT_PRECISION, x1)));
                        if d2 > 1e-12 {
                            roots.push(Value::Real(Float::with_val(DEFAULT_PRECISION, x2)));
                        }
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

/// Convert a purely numeric expression tree to an exact `Rational`, if
/// possible (folding `Plus`/`Times`/`Divide`/integer `Power`). Used to keep
/// series coefficients as `Rational`s rather than `Divide`/`Times` trees.
fn to_exact_rational(v: &Value) -> Option<Rational> {
    fn fold(v: &Value) -> Option<Rational> {
        match v {
            Value::Integer(n) => Some(Rational::from(n.clone())),
            Value::Rational(r) => Some((**r).clone()),
            Value::Call { head, args } if head == "Divide" && args.len() == 2 => {
                let a = fold(&args[0])?;
                let b = fold(&args[1])?;
                if b.is_zero() {
                    return None;
                }
                Some(a / b)
            }
            Value::Call { head, args } if head == "Times" => {
                let mut acc = Rational::from(1);
                for a in args {
                    acc *= fold(a)?;
                }
                Some(acc)
            }
            Value::Call { head, args } if head == "Plus" => {
                let mut acc = Rational::from(0);
                for a in args {
                    acc += fold(a)?;
                }
                Some(acc)
            }
            Value::Call { head, args } if head == "Power" && args.len() == 2 => {
                let b = fold(&args[0])?;
                let e = fold(&args[1])?;
                if e.is_integer() {
                    let exp = e.numer().to_i32()?;
                    if (-32..=32).contains(&exp) {
                        return Some(b.pow(exp));
                    }
                }
                None
            }
            _ => None,
        }
    }
    fold(v)
}

/// Normalize a coefficient to an exact `Integer`/`Rational` when it is numeric.
fn normalize_coefficient(v: &Value) -> Value {
    match to_exact_rational(v) {
        Some(r) if *r.denom() == 1 => Value::Integer(r.numer().clone()),
        Some(r) => Value::Rational(Box::new(r)),
        None => v.clone(),
    }
}

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

    let _ = env;
    // Use the `symbolicmanip` simplifier/derivative: its `simplify_call` keeps
    // `a/b` as a `Divide` (needed for L'Hôpital) and `basic_diff` covers the
    // full arity of `Times`, quotients, and variable exponents.
    let simp = crate::builtins::symbolicmanip::simplify_call;

    // Synthetic-division coefficient extraction: each coefficient is the limit
    // of the remainder over (x - x0)^n, kept as an explicit num/den. This keeps
    // the working expressions small (no exploding derivative denominators).
    let x_minus_x0 = simp(
        "Plus",
        &[
            Value::Symbol(var.clone()),
            simp("Times", &[Value::Integer(Integer::from(-1)), x0.clone()]),
        ],
    );
    let rule = Value::Rule {
        lhs: Box::new(Value::Symbol(var.clone())),
        rhs: Box::new(x0.clone()),
        delayed: false,
    };
    let mut coefficients: Vec<Value> = Vec::new();
    let mut num = args[0].clone();
    let mut den = Value::Integer(Integer::from(1));
    for n in 0..=order {
        let ratio = simp("Divide", &[num.clone(), den.clone()]);
        let coeff_val =
            match crate::builtins::symbolicmanip::builtin_limit(&[ratio.clone(), rule.clone()]) {
                Ok(v) if !matches!(&v, Value::Call { head, .. } if head == "Limit") => v,
                _ => substitute_and_eval(&ratio, &var, &x0),
            };
        if std::env::var("SYMA_SERIES_DEBUG").is_ok() {
            eprintln!("SERIES n={n} ratio={ratio:?} coeff={coeff_val:?}");
        }
        coefficients.push(normalize_coefficient(&combine_plus_terms(
            coeff_val.clone(),
        )));
        // Advance: num <- num - c_n*den, den <- den*(x - x0).
        let sub = simp(
            "Times",
            &[Value::Integer(Integer::from(-1)), coeff_val, den.clone()],
        );
        num = simp("Plus", &[num, sub]);
        den = simp("Times", &[den, x_minus_x0.clone()]);
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
                } else if let Some(e) = (-exp.clone()).to_u32() {
                    // Negative integer exponent → keep the result exact, e.g.
                    // `3^-1` stays `1/3` rather than `0.333…`.
                    Some(Value::Rational(Box::new(Rational::from((
                        Integer::from(1),
                        base.clone().pow(e),
                    )))))
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
        Value::Pattern(Expr::Symbol(s)) | Value::Pattern(Expr::Str(s)) => Ok(s.clone()),
        Value::Pattern(_) => Err(EvalError::TypeError {
            expected: "Symbol or String".to_string(),
            got: val.type_name().to_string(),
        }),
        _ => Err(EvalError::TypeError {
            expected: "Symbol or String".to_string(),
            got: val.type_name().to_string(),
        }),
    }
}

/// Collect symbol names from an argument, unwrapping a list of symbols.
fn extract_symbols(val: &Value) -> Result<Vec<String>, EvalError> {
    let val = match val {
        Value::Hold(inner) | Value::HoldComplete(inner) => inner.as_ref(),
        other => other,
    };
    match val {
        Value::List(items) => items.iter().map(extract_symbol_from).collect(),
        Value::Pattern(Expr::List(items)) => items.iter().map(pattern_symbol_name).collect(),
        other => Ok(vec![extract_symbol_from(other)?]),
    }
}

fn pattern_symbol_name(expr: &Expr) -> Result<String, EvalError> {
    match expr {
        Expr::Symbol(s) | Expr::Str(s) => Ok(s.clone()),
        _ => Err(EvalError::TypeError {
            expected: "Symbol or String".to_string(),
            got: "Expression".to_string(),
        }),
    }
}

/// Normalize an attribute argument (symbol or string) to its bare name.
fn attr_name(val: &Value) -> String {
    match val {
        Value::Symbol(s) | Value::Str(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Flatten attribute arguments, expanding list-valued ones.
fn collect_attrs(values: &[Value]) -> Vec<String> {
    let mut out = Vec::new();
    for v in values {
        match v {
            Value::List(items) => out.extend(items.iter().map(attr_name)),
            other => out.push(attr_name(other)),
        }
    }
    out
}

/// SetAttributes[sym, attr1, attr2, ...] — add attributes to a symbol.
/// Existing attributes are preserved (attributes accumulate).
pub fn builtin_set_attributes(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "SetAttributes requires at least 2 arguments: symbol and attributes".to_string(),
        ));
    }
    let syms = extract_symbols(&args[0])?;
    let new_attrs = collect_attrs(&args[1..]);
    for sym_name in syms {
        // Locked attribute prevents modification
        if env.has_attribute(&sym_name, "Locked") {
            continue;
        }
        let mut current = env.get_attributes(&sym_name);
        for attr in &new_attrs {
            if !current.contains(attr) {
                current.push(attr.clone());
            }
        }
        env.set_attributes(&sym_name, current);
    }
    Ok(Value::Null)
}

pub fn builtin_clear_attributes(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Err(EvalError::Error(
            "ClearAttributes requires at least 1 argument: symbol and optionally attributes to clear"
                .to_string(),
        ));
    }
    let syms = extract_symbols(&args[0])?;
    let to_remove = collect_attrs(&args[1..]);
    for sym_name in syms {
        // Locked attribute prevents modification
        if env.has_attribute(&sym_name, "Locked") {
            continue;
        }
        if args.len() == 1 {
            // Clear all attributes
            env.clear_attributes(&sym_name);
        } else {
            // Clear specific attributes
            let mut current = env.get_attributes(&sym_name);
            current.retain(|a| !to_remove.contains(a));
            env.set_attributes(&sym_name, current);
        }
    }
    Ok(Value::Null)
}

pub fn builtin_attributes(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Attributes requires exactly 1 argument".to_string(),
        ));
    }
    let is_list = matches!(args[0], Value::List(_) | Value::Pattern(Expr::List(_)));
    let syms = extract_symbols(&args[0])?;
    let results: Vec<Value> = syms
        .iter()
        .map(|sym_name| {
            Value::List(
                env.get_attributes(sym_name)
                    .into_iter()
                    .map(Value::Symbol)
                    .collect(),
            )
        })
        .collect();
    if is_list {
        Ok(Value::List(results))
    } else {
        Ok(results
            .into_iter()
            .next()
            .unwrap_or_else(|| Value::List(vec![])))
    }
}

// ── CoefficientList ──────────────────────────────────────────────────────────

/// CoefficientList[poly, x] — Extract coefficients of a polynomial in variable x,
/// starting from the constant term.
/// CoefficientList[1 + 3*x + 5*x^2, x] -> {1, 3, 5}
/// CoefficientList[3*x^2 + 1, x] -> {1, 0, 3}
pub fn builtin_coefficient_list(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "CoefficientList requires exactly 2 arguments".to_string(),
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

    let coeffs = extract_polynomial_coeffs(&args[0], &var);
    if coeffs.is_empty() {
        return Ok(Value::List(vec![Value::Integer(Integer::from(0))]));
    }
    Ok(Value::List(coeffs))
}

// ── FromCoefficientList ──────────────────────────────────────────────────────

/// FromCoefficientList[list, x] — Construct a polynomial from a list of coefficients
/// (constant term first).
/// FromCoefficientList[{1, 3, 5}, x] -> 1 + 3*x + 5*x^2
pub fn builtin_from_coefficient_list(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "FromCoefficientList requires exactly 2 arguments".to_string(),
        ));
    }

    let coeffs = match &args[0] {
        Value::List(items) => items,
        _ => {
            return Err(EvalError::TypeError {
                expected: "List".to_string(),
                got: args[0].type_name().to_string(),
            });
        }
    };

    let var = match &args[1] {
        Value::Symbol(s) => s.clone(),
        _ => {
            return Err(EvalError::TypeError {
                expected: "Symbol".to_string(),
                got: args[1].type_name().to_string(),
            });
        }
    };

    Ok(rebuild_poly_from_coeffs(coeffs, &var))
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

    fn list(items: Vec<Value>) -> Value {
        Value::List(items)
    }

    fn call(head: &str, args: Vec<Value>) -> Value {
        Value::Call {
            head: head.to_string(),
            args,
        }
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

    #[test]
    fn test_expand_three_factors() {
        // Regression: Times with 3+ factors was not distributed.
        let expr = simplify_call(
            "Times",
            &[
                simplify_call("Plus", &[sym("x"), int(1)]),
                simplify_call("Plus", &[sym("x"), int(2)]),
                simplify_call("Plus", &[sym("x"), int(3)]),
            ],
        );
        let result = builtin_expand(&[expr]).unwrap();
        let coeffs = extract_polynomial_coeffs(&result, "x");
        let vals: Vec<i64> = coeffs
            .iter()
            .map(|c| match c {
                Value::Integer(n) => n.to_i64().unwrap_or(0),
                _ => 0,
            })
            .collect();
        // (x+1)(x+2)(x+3) = x^3 + 6x^2 + 11x + 6
        assert_eq!(vals, vec![6, 11, 6, 1]);
    }

    #[test]
    fn test_expand_trinomial_power() {
        // Regression: (x+y+1)^2 with a 3-term base was not expanded.
        let base = simplify_call("Plus", &[sym("x"), sym("y"), int(1)]);
        let expr = simplify_call("Power", &[base, int(2)]);
        let result = builtin_expand(&[expr]).unwrap();
        let at_one = substitute_and_eval(&result, "x", &int(1));
        let at_one = substitute_and_eval(&at_one, "y", &int(1));
        // (1+1+1)^2 = 9
        assert!(
            matches!(&at_one, Value::Integer(n) if *n == 9),
            "got {at_one}"
        );
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
            Value::Call { head, args } if head == "Times" || head == "Divide" => {
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
            _ => panic!("Expected x^3/3 (Times or Divide), got {:?}", result),
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

    /// Structural "does `expr` contain `target` anywhere" check.
    fn contains(expr: &Value, target: &Value) -> bool {
        if expr.struct_eq(target) {
            return true;
        }
        match expr {
            Value::Call { args, .. } => args.iter().any(|a| contains(a, target)),
            Value::List(items) => items.iter().any(|a| contains(a, target)),
            _ => false,
        }
    }

    #[test]
    fn test_integrate_x_to_the_x_unevaluated() {
        // Regression: Integrate[x^x, x] used to return the integrand `x^x`
        // (from a spurious Rubi exponential-substitution match). Mathematica
        // leaves it unevaluated.
        let env = env();
        let expr = call("Power", vec![sym("x"), sym("x")]);
        let result = builtin_integrate(&[expr.clone(), sym("x")], &env).unwrap();
        match &result {
            Value::Call { head, args } if head == "Integrate" => {
                assert_eq!(args.len(), 2);
                assert_eq!(args[0], expr);
                assert_eq!(args[1], sym("x"));
            }
            _ => panic!("Expected unevaluated Integrate[x^x, x], got {:?}", result),
        }
    }

    #[test]
    fn test_integrate_symbolic_power_regression() {
        // Integrate[x^n, x] = x^(n+1)/(n+1)
        let env = env();
        let expr = call("Power", vec![sym("x"), sym("n")]);
        let result = builtin_integrate(&[expr, sym("x")], &env).unwrap();
        let n_plus_1 = call("Plus", vec![sym("n"), int(1)]);
        assert!(
            contains(&result, &call("Power", vec![sym("x"), n_plus_1.clone()])),
            "expected x^(n+1) in {result:?}"
        );
        assert!(
            contains(&result, &call("Power", vec![n_plus_1, int(-1)])),
            "expected 1/(n+1) in {result:?}"
        );
    }

    #[test]
    fn test_integrate_constant_base_exponential_regression() {
        // Integrate[a^x, x] = a^x/Log[a]
        let env = env();
        let expr = call("Power", vec![sym("a"), sym("x")]);
        let result = builtin_integrate(&[expr, sym("x")], &env).unwrap();
        assert!(
            contains(
                &result,
                &call("Power", vec![call("Log", vec![sym("a")]), int(-1)])
            ),
            "expected Log[a]^-1 in {result:?}"
        );
        assert!(
            contains(&result, &call("Power", vec![sym("a"), sym("x")])),
            "expected a^x in {result:?}"
        );
    }

    #[test]
    fn test_integrate_reciprocal_quadratic_regression() {
        // Integrate[1/(1+x^2), x] = ArcTan[x]
        let env = env();
        let denom = call("Plus", vec![int(1), call("Power", vec![sym("x"), int(2)])]);
        let expr = call("Power", vec![denom, int(-1)]);
        let result = builtin_integrate(&[expr, sym("x")], &env).unwrap();
        assert!(
            contains(&result, &call("ArcTan", vec![sym("x")])),
            "expected ArcTan[x] in {result:?}"
        );
    }

    #[test]
    fn test_integrate_definite_x_squared() {
        // Integrate[x^2, {x, 0, 1}] = 1/3
        let env = env();
        let expr = call("Power", vec![sym("x"), int(2)]);
        let bounds = list(vec![sym("x"), int(0), int(1)]);
        let result = builtin_integrate(&[expr, bounds], &env).unwrap();
        let one_third = Value::Rational(Box::new(rug::Rational::from((1, 3))));
        assert!(
            result == one_third || result == sym("1/3"),
            "expected 1/3, got {result:?}"
        );
    }

    // ── Solve tests ──

    /// Extract the solution values from a `{var -> {v1, ...}}` result.
    fn solve_values(result: &Value) -> Vec<Value> {
        match result {
            Value::List(items) if items.len() == 1 => match &items[0] {
                Value::Rule { rhs, .. } => match rhs.as_ref() {
                    Value::List(values) => values.clone(),
                    _ => vec![],
                },
                _ => vec![],
            },
            _ => vec![],
        }
    }

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
        let roots_values = solve_values(&result);
        assert_eq!(
            roots_values.len(),
            3,
            "expected three roots, got {result:?}"
        );
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
        let mut roots: Vec<f64> = solve_values(&result)
            .iter()
            .filter_map(|rhs| match rhs {
                Value::Real(r) => Some(r.to_f64()),
                Value::Integer(n) => Some(n.to_f64()),
                _ => None,
            })
            .collect();
        roots.sort_by(|a, b| a.partial_cmp(b).unwrap());
        // Regression: wrong depression/resolvent formulas produced
        // roots near -0.33 and -2.03 instead of {-2,-1,1,2}.
        assert_eq!(roots.len(), 4, "expected four roots, got {roots:?}");
        for (got, want) in roots.iter().zip([-2.0, -1.0, 1.0, 2.0]) {
            assert!((got - want).abs() < 1e-9, "roots {roots:?}");
        }
    }

    #[test]
    fn test_solve_quartic_shifted() {
        // (x-1)(x-2)(x-3)(x-4) = 0 → roots 1,2,3,4. Here b != 0, which
        // exercises the quartic depression/resolvent formulas.
        let poly = simplify_call(
            "Times",
            &[
                simplify_call("Plus", &[sym("x"), int(-1)]),
                simplify_call("Plus", &[sym("x"), int(-2)]),
                simplify_call("Plus", &[sym("x"), int(-3)]),
                simplify_call("Plus", &[sym("x"), int(-4)]),
            ],
        );
        let poly = builtin_expand(&[poly]).unwrap();
        let eq = Value::Call {
            head: "Equal".to_string(),
            args: vec![poly, int(0)],
        };
        let result = builtin_solve(&[eq, sym("x")]).unwrap();
        let mut roots: Vec<f64> = solve_values(&result)
            .iter()
            .filter_map(|rhs| match rhs {
                Value::Real(r) => Some(r.to_f64()),
                Value::Integer(n) => Some(n.to_f64()),
                _ => None,
            })
            .collect();
        roots.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(roots.len(), 4, "expected four roots, got {roots:?}");
        for (got, want) in roots.iter().zip([1.0, 2.0, 3.0, 4.0]) {
            assert!((got - want).abs() < 1e-9, "roots {roots:?}");
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
    fn test_d_constant_smoke() {
        assert_eq!(eval_src("D[1, {x, 1}]"), int(0));
        assert_eq!(eval_src("D[x^2, x]"), eval_src("2 x"));
    }

    #[test]
    fn test_simplify_trivial() {
        // Simplify[x] = x
        let result = builtin_simplify(&[sym("x")]).unwrap();
        assert_eq!(result, sym("x"));
    }

    /// Evaluate a source string in a fresh, fully-registered environment.
    fn eval_src(src: &str) -> Value {
        let e = env();
        let tokens = crate::lexer::tokenize(src).unwrap();
        let stmts = crate::parser::parse_with_suppress(tokens).unwrap();
        let mut last = Value::Null;
        for (expr, _) in stmts {
            last = crate::eval::eval(&expr, &e).unwrap();
        }
        last
    }

    #[test]
    fn test_simplify_double_angle_collapse() {
        // Cos[x]^2 - Sin[x]^2 → Cos[2 x]
        assert_eq!(
            eval_src("Simplify[Cos[x]^2 - Sin[x]^2]"),
            eval_src("Cos[2 x]")
        );
        // Sin[x]^2 - Cos[x]^2 → -Cos[2 x]
        assert_eq!(
            eval_src("Simplify[Sin[x]^2 - Cos[x]^2]"),
            eval_src("-Cos[2 x]")
        );
        // 2 Sin[x] Cos[x] → Sin[2 x]
        assert_eq!(eval_src("Simplify[2 Sin[x] Cos[x]]"), eval_src("Sin[2 x]"));
        // Sin[x] Cos[x] → Sin[2 x]/2
        assert_eq!(
            eval_src("Simplify[Sin[x] Cos[x]]"),
            eval_src("1/2 Sin[2 x]")
        );
        // Angles that are themselves sums / multiples.
        assert_eq!(
            eval_src("Simplify[2 Sin[x + y] Cos[x + y]]"),
            eval_src("Sin[2 (x + y)]")
        );
        assert_eq!(
            eval_src("Simplify[Cos[2 x] - (Cos[x]^2 - Sin[x]^2)]"),
            int(0)
        );
    }

    #[test]
    fn test_simplify_pythagorean() {
        assert_eq!(eval_src("Simplify[Sin[x]^2 + Cos[x]^2]"), int(1));
        assert_eq!(eval_src("Simplify[3 Sin[x]^2 + 3 Cos[x]^2]"), int(3));
        assert_eq!(eval_src("Simplify[1 - Sin[x]^2]"), eval_src("Cos[x]^2"));
        assert_eq!(eval_src("Simplify[1 - 2 Sin[x]^2]"), eval_src("Cos[2 x]"));
        assert_eq!(eval_src("Simplify[2 Cos[x]^2 - 1]"), eval_src("Cos[2 x]"));
    }

    #[test]
    fn test_simplify_normal_inputs_unchanged() {
        assert_eq!(eval_src("Simplify[x + x]"), eval_src("2 x"));
        assert_eq!(eval_src("Simplify[(x^2 - 1)/(x - 1)]"), eval_src("x + 1"));
        // Sin[2 x] is already canonical — must stay.
        assert_eq!(eval_src("Simplify[Sin[2 x]]"), eval_src("Sin[2 x]"));
    }

    #[test]
    fn test_simplify_no_assumptions_no_overreach() {
        assert_eq!(eval_src("Simplify[Sqrt[x^2]]"), eval_src("Sqrt[x^2]"));
        assert_eq!(eval_src("Simplify[Abs[x]]"), eval_src("Abs[x]"));
        assert_eq!(eval_src("Simplify[Sign[x]]"), eval_src("Sign[x]"));
        assert_eq!(eval_src("Simplify[Conjugate[x]]"), eval_src("Conjugate[x]"));
        assert_eq!(eval_src("Simplify[Sqrt[a b]]"), eval_src("Sqrt[a b]"));
    }

    #[test]
    fn test_simplify_assumptions_option() {
        assert_eq!(
            eval_src("Simplify[Sqrt[x^2], Assumptions -> x > 0]"),
            sym("x")
        );
        assert_eq!(
            eval_src("Simplify[Sqrt[x^2], Assumptions -> x < 0]"),
            eval_src("-x")
        );
        assert_eq!(
            eval_src("Simplify[Sqrt[x^2], Assumptions -> Element[x, Reals]]"),
            eval_src("Abs[x]")
        );
        // Bare assumption (no `Assumptions ->`).
        assert_eq!(eval_src("Simplify[Abs[x], x < 0]"), eval_src("-x"));
    }

    #[test]
    fn test_simplify_assuming_feeds_assumptions() {
        assert_eq!(eval_src("Assuming[x > 0, Simplify[Sqrt[x^2]]]"), sym("x"));
        assert_eq!(
            eval_src("Assuming[x < 0, Simplify[Abs[x]]]"),
            eval_src("-x")
        );
        assert_eq!(
            eval_src("Assuming[Element[x, Reals], Refine[Conjugate[x]]]"),
            sym("x")
        );
    }

    #[test]
    fn test_simplify_positive_power_and_sqrt_split() {
        assert_eq!(
            eval_src("Simplify[Sqrt[a b], Assumptions -> a > 0 && b > 0]"),
            eval_src("Sqrt[a] Sqrt[b]")
        );
        assert_eq!(
            eval_src("Simplify[(x y)^2, Assumptions -> x > 0 && y > 0]"),
            eval_src("x^2 y^2")
        );
    }

    // ── CoefficientList ──

    #[test]
    fn test_coefficient_list_basic() {
        // CoefficientList[1 + 3*x + 5*x^2, x] -> {1, 3, 5}
        let poly = call(
            "Plus",
            vec![
                int(1),
                call("Times", vec![int(3), sym("x")]),
                call("Times", vec![int(5), call("Power", vec![sym("x"), int(2)])]),
            ],
        );
        let result = builtin_coefficient_list(&[poly, sym("x")]).unwrap();
        assert_eq!(result, list(vec![int(1), int(3), int(5)]));
    }

    #[test]
    fn test_coefficient_list_with_gaps() {
        // CoefficientList[3*x^2 + 1, x] -> {1, 0, 3}
        let poly = call(
            "Plus",
            vec![
                int(1),
                call("Times", vec![int(3), call("Power", vec![sym("x"), int(2)])]),
            ],
        );
        let result = builtin_coefficient_list(&[poly, sym("x")]).unwrap();
        assert_eq!(result, list(vec![int(1), int(0), int(3)]));
    }

    #[test]
    fn test_coefficient_list_constant() {
        // CoefficientList[7, x] -> {7}
        let result = builtin_coefficient_list(&[int(7), sym("x")]).unwrap();
        assert_eq!(result, list(vec![int(7)]));
    }

    #[test]
    fn test_coefficient_list_zero() {
        // CoefficientList[0, x] -> {0}
        let result = builtin_coefficient_list(&[int(0), sym("x")]).unwrap();
        assert_eq!(result, list(vec![int(0)]));
    }

    #[test]
    fn test_coefficient_list_linear() {
        // CoefficientList[2*x + 3, x] -> {3, 2}
        let poly = call("Plus", vec![int(3), call("Times", vec![int(2), sym("x")])]);
        let result = builtin_coefficient_list(&[poly, sym("x")]).unwrap();
        assert_eq!(result, list(vec![int(3), int(2)]));
    }

    #[test]
    fn test_coefficient_list_bad_args() {
        assert!(builtin_coefficient_list(&[]).is_err());
        assert!(builtin_coefficient_list(&[int(1)]).is_err());
    }

    // ── FromCoefficientList ──

    #[test]
    fn test_from_coefficient_list_basic() {
        // FromCoefficientList[{1, 3, 5}, x] -> 1 + 3*x + 5*x^2
        let result =
            builtin_from_coefficient_list(&[list(vec![int(1), int(3), int(5)]), sym("x")]).unwrap();
        // Verify structure: Plus[1, Times[3, x], Times[5, Power[x, 2]]]
        match result {
            Value::Call { head, args } => {
                assert_eq!(head, "Plus");
                assert_eq!(args.len(), 3);
            }
            _ => panic!("Expected Plus Call, got {:?}", result),
        }
    }

    #[test]
    fn test_from_coefficient_list_constant() {
        // FromCoefficientList[{7}, x] -> 7
        let result = builtin_from_coefficient_list(&[list(vec![int(7)]), sym("x")]).unwrap();
        assert_eq!(result, int(7));
    }

    #[test]
    fn test_from_coefficient_list_with_zeros() {
        // FromCoefficientList[{1, 0, 3}, x] -> 1 + 3*x^2
        let result =
            builtin_from_coefficient_list(&[list(vec![int(1), int(0), int(3)]), sym("x")]).unwrap();
        match result {
            Value::Call { head, args } => {
                assert_eq!(head, "Plus");
                assert_eq!(args.len(), 2);
            }
            _ => panic!("Expected Plus Call, got {:?}", result),
        }
    }

    #[test]
    fn test_from_coefficient_list_round_trip() {
        // Round-trip: FromCoefficientList[CoefficientList[poly, x], x] should simplify to poly
        let poly = call(
            "Plus",
            vec![
                int(1),
                call("Times", vec![int(3), sym("x")]),
                call("Times", vec![int(5), call("Power", vec![sym("x"), int(2)])]),
            ],
        );
        let coeffs = builtin_coefficient_list(&[poly.clone(), sym("x")]).unwrap();
        let reconstructed = builtin_from_coefficient_list(&[coeffs, sym("x")]).unwrap();
        // Both should be structurally equal (or equal after simplification)
        let simplified_orig = builtin_simplify(&[poly]).unwrap();
        let simplified_recon = builtin_simplify(&[reconstructed]).unwrap();
        assert!(simplified_orig.struct_eq(&simplified_recon));
    }

    #[test]
    fn test_from_coefficient_list_empty() {
        // FromCoefficientList[{}, x] -> 0
        let result = builtin_from_coefficient_list(&[list(vec![]), sym("x")]).unwrap();
        assert_eq!(result, int(0));
    }

    #[test]
    fn test_from_coefficient_list_bad_args() {
        assert!(builtin_from_coefficient_list(&[]).is_err());
        assert!(builtin_from_coefficient_list(&[int(1)]).is_err());
    }

    // ── Attribute builtins ──

    #[test]
    fn test_set_attributes_accumulates() {
        let env = env();
        builtin_set_attributes(&[sym("aa"), sym("HoldAll")], &env).unwrap();
        builtin_set_attributes(&[sym("aa"), sym("Listable")], &env).unwrap();
        let attrs = env.get_attributes("aa");
        assert!(attrs.contains(&"HoldAll".to_string()));
        assert!(attrs.contains(&"Listable".to_string()));
        assert_eq!(attrs.len(), 2);
    }

    #[test]
    fn test_set_attributes_string_and_list() {
        let env = env();
        builtin_set_attributes(&[sym("bb"), Value::Str("HoldAll".to_string())], &env).unwrap();
        assert!(env.has_attribute("bb", "HoldAll"));

        let names = list(vec![sym("cc"), sym("dd")]);
        builtin_set_attributes(&[names, sym("Listable")], &env).unwrap();
        assert!(env.has_attribute("cc", "Listable"));
        assert!(env.has_attribute("dd", "Listable"));
    }

    #[test]
    fn test_clear_attributes_specific() {
        let env = env();
        builtin_set_attributes(&[sym("ee"), sym("HoldAll"), sym("Listable")], &env).unwrap();
        builtin_clear_attributes(&[sym("ee"), sym("HoldAll")], &env).unwrap();
        let attrs = env.get_attributes("ee");
        assert!(!attrs.contains(&"HoldAll".to_string()));
        assert!(attrs.contains(&"Listable".to_string()));
    }

    #[test]
    fn test_attributes_list_returns_list_of_lists() {
        let env = env();
        builtin_set_attributes(&[sym("ff"), sym("HoldAll")], &env).unwrap();
        let result = builtin_attributes(&[list(vec![sym("ff"), sym("gg")])], &env).unwrap();
        match result {
            Value::List(outer) => {
                assert_eq!(outer.len(), 2);
                assert!(matches!(&outer[0], Value::List(v) if v.len() == 1));
                assert!(matches!(&outer[1], Value::List(v) if v.is_empty()));
            }
            other => panic!("expected list of lists, got {other:?}"),
        }
    }
}
