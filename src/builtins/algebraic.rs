use crate::env::Env;
use crate::polynomial;
use crate::value::{BuiltinFn, EvalError, Value, DEFAULT_PRECISION, rational_value};
use rug::{Integer, Rational};

// ── Root constructor ────────────────────────────────────────────────────────────────────────────

pub fn builtin_root(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Root requires exactly 2 arguments: Root[{coeffs}, index]".to_string(),
        ));
    }

    let coeffs = match &args[0] {
        Value::List(items) => {
            let mut c = Vec::with_capacity(items.len());
            for item in items {
                match item {
                    Value::Integer(n) => c.push(Rational::from(n.clone())),
                    Value::Rational(r) => c.push(r.clone()),
                    Value::Real(r) => c.push(Rational::from_f64(r.to_f64())
                        .unwrap_or(Rational::from(0))),
                    _ => {
                        return Err(EvalError::TypeError {
                            expected: "Number".to_string(),
                            got: item.type_name().to_string(),
                        });
                    }
                }
            }
            c
        }
        _ => {
            return Err(EvalError::TypeError {
                expected: "List".to_string(),
                got: args[0].type_name().to_string(),
            });
        }
    };

    let index = match &args[1] {
        Value::Integer(n) => {
            let v = usize::try_from(n).unwrap_or(1);
            if v == 0 {
                return Err(EvalError::Error(
                    "Root index must be >= 1".to_string(),
                ));
            }
            v
        }
        _ => {
            return Err(EvalError::TypeError {
                expected: "Integer".to_string(),
                got: args[1].type_name().to_string(),
            });
        }
    };

    let deg = polynomial::poly_degree(&coeffs);
    if deg == 0 {
        return Err(EvalError::Error(
            "Root: polynomial must have degree > 0".to_string(),
        ));
    }
    if index > deg {
        return Err(EvalError::Error(format!(
            "Root: index {} exceeds degree {}",
            index, deg
        )));
    }

    // Make monic and canonical (positive leading coefficient)
    let mut norm = polynomial::make_monic(&coeffs);
    let lead = &norm[deg];
    if lead.is_negative() {
        for c in norm.iter_mut() {
            *c = -c;
        }
    }

    // If linear, return exact rational
    if deg == 1 {
        let neg = -&norm[0];
        let result: Rational = (neg.clone() / norm[1].clone()).into();
        let (num, denom) = result.into_numer_denom();
        if denom == Integer::from(1) {
            return Ok(Value::Integer(num));
        } else {
            return Ok(rational_value(num, denom));
        }
    }

    Ok(Value::Root {
        coeffs: norm,
        index,
    })
}

// ── MinimalPolynomial ───────────────────────────────────────────────────────────

pub fn builtin_minimal_polynomial(
    args: &[Value],
    _env: &Env,
) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Err(EvalError::Error(
            "MinimalPolynomial requires at least 1 argument".to_string(),
        ));
    }
    let expr = &args[0];

    match expr {
        Value::Root { coeffs, index: _ } => {
            let deg = polynomial::poly_degree(coeffs);
            if deg == 0 {
                return Ok(expr.clone());
            }
            Ok(polynomial::coeffs_to_value(coeffs))
        }
        Value::Integer(n) => {
            // Minimal polynomial of integer n: x - n → {−n, 1}
            Ok(Value::List(vec![
                Value::Integer((-n).clone()),
                Value::Integer(Integer::from(1)),
            ]))
        }
        Value::Rational(r) => {
            let (num, den) = r.clone().into_numer_denom();
            Ok(Value::List(vec![
                Value::Integer((-&num).clone()),
                Value::Integer(den.clone()),
            ]))
        }
        _ => Ok(Value::Call {
            head: "MinimalPolynomial".to_string(),
            args: args.to_vec(),
        }),
    }
}

// ── RootReduce ──────────────────────────────────────────────────────────────────

pub fn builtin_root_reduce(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Err(EvalError::Error(
            "RootReduce requires 1 argument".to_string(),
        ));
    }
    root_reduce_value(&args[0])
}

fn root_reduce_value(v: &Value) -> Result<Value, EvalError> {
    match v {
        Value::Root { .. } => Ok(v.clone()),
        Value::Integer(_) | Value::Rational(_) | Value::Real(_) => Ok(v.clone()),
        Value::Call {
            head,
            args: sub_args,
        } if head == "Plus" => {
            if sub_args.is_empty() {
                return Ok(Value::Integer(Integer::from(0)));
            }
            let mut result = root_or_numeric_to_root(&sub_args[0])?;
            for arg in sub_args.iter().skip(1) {
                let b = root_or_numeric_to_root(arg)?;
                result = root_add(&result, &b)?;
            }
            Ok(result)
        }
        Value::Call {
            head,
            args: sub_args,
        } if head == "Times" => {
            let roots: Vec<Value> = sub_args
                .iter()
                .filter_map(|arg| root_or_numeric_to_root(arg).ok())
                .collect();
            let non_root: Vec<Value> = sub_args
                .iter()
                .filter(|arg| root_or_numeric_to_root(arg).is_err())
                .cloned()
                .collect();

            if roots.is_empty() {
                return Ok(v.clone());
            }
            let mut result = roots[0].clone();
            for root in roots.iter().skip(1) {
                result = root_mul(&result, root)?;
            }
            // Multiply by non-root scalars if needed
            if !non_root.is_empty() {
                // For now just return as expression with RootReduce applied
                result = root_mul(&result, &non_root[0].clone())?;
            }
            Ok(result)
        }
        _ => Ok(v.clone()),
    }
}

fn root_or_numeric_to_root(v: &Value) -> Result<Value, EvalError> {
    match v {
        Value::Root { .. } => Ok(v.clone()),
        Value::Integer(n) => Ok(Value::Root {
            coeffs: vec![Rational::from(-n), Rational::from(1)],
            index: 1,
        }),
        Value::Rational(r) => {
            let (num, den) = r.into_numer_denom();
            Ok(Value::Root {
                coeffs: vec![Rational::from(-num.clone()), Rational::from(den.clone())],
                index: 1,
            })
        }
        _ => Err(EvalError::TypeError {
            expected: "Root or Number".to_string(),
            got: v.type_name().to_string(),
        }),
    }
}

fn root_add(a: &Value, b: &Value) -> Result<Value, EvalError> {
    match (a, b) {
        (
            Value::Root {
                coeffs: ca,
                index: ia,
            },
            Value::Root {
                coeffs: cb,
                index: ib,
            },
        ) => {
            let minp = polynomial::min_poly_operation(
                ca,
                *ia,
                cb,
                *ib,
                polynomial::AlgebraicOp::Add,
            );
            if polynomial::poly_degree(&minp) == 0 {
                return Ok(Value::Integer(Integer::from(0)));
            }
            let a_val = root_as_f64(a)?;
            let b_val = root_as_f64(b)?;
            let target = a_val + b_val;
            let roots = polynomial::find_polynomial_roots(&minp);
            let idx = find_root_index(&roots, target).unwrap_or(1);
            Ok(Value::Root {
                coeffs: minp,
                index: idx,
            })
        }
        _ => Ok(Value::Call {
            head: "Plus".to_string(),
            args: vec![a.clone(), b.clone()],
        }),
    }
}

fn root_mul(a: &Value, b: &Value) -> Result<Value, EvalError> {
    match (a, b) {
        (
            Value::Root {
                coeffs: ca,
                index: ia,
            },
            Value::Root {
                coeffs: cb,
                index: ib,
            },
        ) => {
            let minp = polynomial::min_poly_operation(
                ca,
                *ia,
                cb,
                *ib,
                polynomial::AlgebraicOp::Mul,
            );
            if polynomial::poly_degree(&minp) == 0 {
                return Ok(Value::Integer(Integer::from(0)));
            }
            let a_val = root_as_f64(a)?;
            let b_val = root_as_f64(b)?;
            let target = a_val * b_val;
            let roots = polynomial::find_polynomial_roots(&minp);
            let idx = find_root_index(&roots, target).unwrap_or(1);
            Ok(Value::Root {
                coeffs: minp,
                index: idx,
            })
        }
        _ => Ok(Value::Call {
            head: "Times".to_string(),
            args: vec![a.clone(), b.clone()],
        }),
    }
}

fn root_as_f64(v: &Value) -> Result<f64, EvalError> {
    match v {
        Value::Root { coeffs, index } => {
            let roots = polynomial::find_polynomial_roots(coeffs);
            if *index > 0 && *index <= roots.len() {
                let (re, im) = roots[*index - 1];
                if im.abs() > 0.01 {
                    return Err(EvalError::Error(
                        "Root: complex root not yet supported in RootReduce"
                            .to_string(),
                    ));
                }
                Ok(re)
            } else {
                Err(EvalError::Error("Root: invalid index".to_string()))
            }
        }
        Value::Integer(n) => Ok(n.to_f64()),
        Value::Rational(r) => Ok(r.to_f64()),
        Value::Real(r) => Ok(r.to_f64()),
        _ => Err(EvalError::TypeError {
            expected: "Root or Number".to_string(),
            got: v.type_name().to_string(),
        }),
    }
}

fn find_root_index(root_list: &[(f64, f64)], target: f64) -> Option<usize> {
    let mut best: Option<(usize, f64)> = None;
    for (i, &(re, im)) in root_list.iter().enumerate() {
        let dist = ((re - target).powi(2) + im.powi(2)).sqrt();
        if best.is_none() || dist < best.unwrap().1 {
            best = Some((i + 1, dist));
        }
    }
    best
}

// ── ToRadicals ──────────────────────────────────────────────────────────────────

pub fn builtin_to_radicals(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Err(EvalError::Error(
            "ToRadicals requires 1 argument".to_string(),
        ));
    }
    to_radicals_value(&args[0])
}

fn to_radicals_value(v: &Value) -> Result<Value, EvalError> {
    match v {
        Value::Root { coeffs, index } => {
            let deg = polynomial::poly_degree(coeffs);
            match deg {
                2 => quadratic_to_radicals(coeffs, *index),
                3 => cubic_to_radicals(coeffs, *index),
                _ => Ok(v.clone()),
            }
        }
        Value::Integer(_)
        | Value::Rational(_)
        | Value::Real(_)
        | Value::Complex { .. } => Ok(v.clone()),
        Value::Call { head, args: sub } => {
            let converted: Result<Vec<Value>, _> = sub.iter().map(to_radicals_value).collect();
            Ok(Value::Call {
                head: head.clone(),
                args: converted?,
            })
        }
        Value::List(items) => {
            let converted: Result<Vec<Value>, _> =
                items.iter().map(to_radicals_value).collect();
            Ok(Value::List(converted?))
        }
        _ => Ok(v.clone()),
    }
}

// ax² + bx + c with coeffs = [c, b, a]
// roots: (-b ± sqrt(b²-4ac)) / (2a)
fn quadratic_to_radicals(coeffs: &[Rational], index: usize) -> Result<Value, EvalError> {
    // coeffs = [c, b, a] for ax² + bx + c
    let c = &coeffs[0];
    let b = &coeffs[1];
    let a = &coeffs[2];

    // discriminant = b² - 4ac
    let disc = b * b - Rational::from(4) * a * c;

    if disc.is_zero() {
        // Single root: -b / (2a)
        let result_num = -b;
        let result_den = Rational::from(2) * a;
        let result: Rational =
            ((result_num.clone() / result_den.clone()) * a.clone()) / a.clone();
        // Simplify: (-b) / (2a)
        let num = -b;
        let den = Rational::from(2) * a;
        let (n, d) = ((num / den)).into_numer_denom();
        if d == Integer::from(1) {
            return Ok(Value::Integer(n));
        } else {
            return Ok(rational_value(n, d));
        }
    }

    // Build Power[a, Rational[1/2]] for sqrt
    let sqrt_disc = Value::Call {
        head: "Sqrt".to_string(),
        args: vec![polynomial::coeffs_to_value_from_rational(&disc)],
    };

    // root1 = (-b - sqrt(disc)) / (2*a), root2 = (-b + sqrt(disc)) / (2*a)
    // index 1 is the smaller root (more negative real part)

    if disc.is_negative() {
        // Complex roots — return as-is for now
        return Ok(Value::Call {
            head: "Root".to_string(),
            args: vec![
                polynomial::coeffs_to_value(coeffs),
                Value::Integer(Integer::from(index)),
            ],
        });
    }

    let two_a = Rational::from(2) * a;
    // For index 1 (smaller root): (-b - sqrt(disc), but ordered by real part)
    // If disc > 0, root1 < root2, so index 1 → (-b - sqrt(disc)) / (2a)

    // Build numerator as expression: -b + sign * Sqrt[disc]
    let sign = if index == 1 { -1 } else { 1 };
    let neg_b = Value::Call {
        head: "Times".to_string(),
        args: vec![
            Value::Integer(Integer::from(-1)),
            polynomial::coeffs_to_value_from_rational(b),
        ],
    };
    let sqrt_term = if sign < 0 {
        Value::Call {
            head: "Times".to_string(),
            args: vec![Value::Integer(Integer::from(-1)), sqrt_disc],
        }
    } else {
        sqrt_disc
    };
    let numerator = Value::Call {
        head: "Plus".to_string(),
        args: vec![neg_b, sqrt_term],
    };
    let denominator = Value::Call {
        head: "Times".to_string(),
        args: vec![
            Value::Integer(Integer::from(2)),
            polynomial::coeffs_to_value_from_rational(a),
        ],
    };

    Ok(Value::Call {
        head: "Divide".to_string(),
        args: vec![numerator, denominator],
    })
}

fn cubic_to_radicals(_coeffs: &[Rational], _index: usize) -> Result<Value, EvalError> {
    // Cardano formula — complex; placeholder for now
    Err(EvalError::Error(
        "ToRadicals for cubic: not yet implemented".to_string(),
    ))
}

// ── IsolatingInterval ───────────────────────────────────────────────────────────

pub fn builtin_isolating_interval(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Err(EvalError::Error(
            "IsolatingInterval requires 1 argument".to_string(),
        ));
    }
    let (coeffs, index) = match &args[0] {
        Value::Root { coeffs, index } => (coeffs.clone(), *index),
        _ => {
            return Err(EvalError::TypeError {
                expected: "Root".to_string(),
                got: args[0].type_name().to_string(),
            });
        }
    };

    let deg = polynomial::poly_degree(&coeffs);
    if deg == 0 {
        return Err(EvalError::Error(
            "IsolatingInterval: polynomial must have degree > 0".to_string(),
        ));
    }

    let roots = polynomial::find_polynomial_roots(&coeffs);
    if index > roots.len() {
        return Err(EvalError::IndexOutOfBounds {
            index: index as i64,
            length: roots.len(),
        });
    }
    let (approx_re, approx_im) = roots[index - 1];

    if approx_im.abs() < 1e-10 {
        isolating_interval_real(&coeffs, approx_re)
    } else {
        // For complex roots, return approximate interval
        Err(EvalError::Error(
            "IsolatingInterval: complex roots not yet supported".to_string(),
        ))
    }
}

fn isolating_interval_real(coeffs: &[Rational], approx: f64) -> Result<Value, EvalError> {
    let mut lo: Rational = Rational::from(-1).pow(Integer::from(6));
    let mut hi: Rational = Rational::from(1).pow(Integer::from(6));

    // Ensure the root is in [lo, hi]
    while lo.to_f64() > approx - 1.0 {
        lo = lo.clone() * Rational::from(10);
    }
    while hi.to_f64() < approx + 1.0 {
        hi = hi.clone() * Rational::from(10);
    }

    // Bisection
    for _ in 0..200 {
        let mid = (lo.clone() + hi.clone()) / Rational::from(2);
        let count = polynomial::count_real_roots_in(&coeffs, &lo, &mid);
        if count == 1 {
            hi = mid;
        } else {
            lo = mid;
        }
        let diff = &hi - &lo;
        if diff.to_f64() < 1e-15 {
            break;
        }
    }

    Ok(Value::List(vec![
        rat_value(&lo),
        rat_value(&hi),
    ]))
}

fn rat_value(r: &Rational) -> Value {
    let (num, den) = r.clone().into_numer_denom();
    if den == Integer::from(1) {
        Value::Integer(num)
    } else {
        rational_value(num, den)
    }
}

// ── Registration ────────────────────────────────────────────────────────────────

pub fn register(env: &Env) {
    use crate::builtins::{register_builtin, register_builtin_env};

    register_builtin(env, "Root", builtin_root);
    register_builtin_env(env, "MinimalPolynomial", builtin_minimal_polynomial);
    register_builtin(env, "RootReduce", builtin_root_reduce);
    register_builtin(env, "ToRadicals", builtin_to_radicals);
    register_builtin(env, "IsolatingInterval", builtin_isolating_interval);
}
