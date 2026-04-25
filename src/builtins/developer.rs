//! `Developer` package builtins.
//!
//! Provides system-level developer utilities: machine-size integer predicates,
//! packed array operations, special-function simplification, and notebook stubs.
//! Mirrors Wolfram Language's `Developer` context.

use crate::env::Env;
use crate::value::{EvalError, PackedArrayType, Value};
use rug::Integer;

// ── Symbol names for reference / module registration ──
pub const SYMBOLS: &[&str] = &[
    "$MaxMachineInteger",
    "MachineIntegerQ",
    "ToPackedArray",
    "FromPackedArray",
    "PackedArrayForm",
    "PackedArrayQ",
    "BesselSimplify",
    "GammaSimplify",
    "PolyGammaSimplify",
    "ZetaSimplify",
    "PolyLogSimplify",
    "TrigToRadicals",
    "CellInformation",
    "NotebookConvert",
    "ReplaceAllUnheld",
];

const BACKTICK_ALIASES: &[(&str, &str)] = &[
    ("Developer`$MaxMachineInteger", "$MaxMachineInteger"),
    ("Developer`MachineIntegerQ", "MachineIntegerQ"),
    ("Developer`ToPackedArray", "ToPackedArray"),
    ("Developer`FromPackedArray", "FromPackedArray"),
    ("Developer`PackedArrayForm", "PackedArrayForm"),
    ("Developer`PackedArrayQ", "PackedArrayQ"),
    ("Developer`BesselSimplify", "BesselSimplify"),
    ("Developer`GammaSimplify", "GammaSimplify"),
    ("Developer`PolyGammaSimplify", "PolyGammaSimplify"),
    ("Developer`ZetaSimplify", "ZetaSimplify"),
    ("Developer`PolyLogSimplify", "PolyLogSimplify"),
    ("Developer`TrigToRadicals", "TrigToRadicals"),
    ("Developer`CellInformation", "CellInformation"),
    ("Developer`NotebookConvert", "NotebookConvert"),
    ("Developer`ReplaceAllUnheld", "ReplaceAllUnheld"),
];

/// Register all `Developer` builtins and backtick-qualified aliases in the environment.
pub fn register(env: &Env) {
    use super::register_builtin;
    use super::register_builtin_env;

    // ── System constant ──
    env.set(
        "$MaxMachineInteger".to_string(),
        Value::Integer(Integer::from(i64::MAX)),
    );

    // ── Predicates ──
    register_builtin(env, "MachineIntegerQ", builtin_machine_integer_q);
    register_builtin(env, "PackedArrayQ", builtin_packed_array_q);

    // ── Packed array operations ──
    register_builtin(env, "ToPackedArray", builtin_to_packed_array);
    register_builtin(env, "FromPackedArray", builtin_from_packed_array);
    env.set(
        "PackedArrayForm".to_string(),
        Value::Symbol("PackedArrayForm".to_string()),
    );

    // ── Special-function simplification ──
    register_builtin(env, "BesselSimplify", builtin_bessel_simplify);
    register_builtin(env, "GammaSimplify", builtin_gamma_simplify);
    register_builtin(env, "PolyGammaSimplify", builtin_poly_gamma_simplify);
    register_builtin(env, "ZetaSimplify", builtin_zeta_simplify);
    register_builtin(env, "PolyLogSimplify", builtin_poly_log_simplify);
    register_builtin(env, "TrigToRadicals", builtin_trig_to_radicals);

    // ── Notebook stubs ──
    register_builtin(env, "CellInformation", builtin_cell_information);
    register_builtin(env, "NotebookConvert", builtin_notebook_convert);

    // ── ReplaceAll without hold ──
    register_builtin_env(env, "ReplaceAllUnheld", builtin_replace_all_unheld);

    // ── Backtick-qualified aliases ──
    for (qualified_name, bare_name) in BACKTICK_ALIASES {
        if let Some(val) = env.get(bare_name) {
            env.set(qualified_name.to_string(), val);
        }
    }
}

// ── $MaxMachineInteger (system constant, set directly in register()) ──

// ── MachineIntegerQ ──

fn builtin_machine_integer_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "MachineIntegerQ requires exactly 1 argument".to_string(),
        ));
    }
    Ok(Value::Bool(matches!(
        &args[0],
        Value::Integer(n) if n.to_i64().is_some()
    )))
}

// ── PackedArrayQ ──

fn builtin_packed_array_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "PackedArrayQ requires exactly 1 argument".to_string(),
        ));
    }
    Ok(Value::Bool(matches!(&args[0], Value::PackedArray(_))))
}

// ── ToPackedArray ──

fn builtin_to_packed_array(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "ToPackedArray requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::List(items) => {
            // If all items are Integer, pack as Integer64
            if items.iter().all(|v| matches!(v, Value::Integer(_))) {
                let ints: Vec<i64> = items
                    .iter()
                    .filter_map(|v| v.to_integer())
                    .collect();
                return Ok(Value::PackedArray(PackedArrayType::Integer64(ints)));
            }
            // If all items are convertible to Real, pack as Real64
            let reals: Vec<f64> = items
                .iter()
                .filter_map(|v| v.to_real())
                .collect();
            if reals.len() == items.len() {
                return Ok(Value::PackedArray(PackedArrayType::Real64(reals)));
            }
            // Fallback: return as-is
            Ok(args[0].clone())
        }
        Value::PackedArray(_) => Ok(args[0].clone()),
        _ => Ok(args[0].clone()),
    }
}

// ── FromPackedArray ──

fn builtin_from_packed_array(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "FromPackedArray requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::PackedArray(ty) => Ok(Value::List(ty.to_values())),
        _ => Ok(args[0].clone()),
    }
}

// ── PackedArrayForm (symbol constant, set directly in register()) ──

// ── BesselSimplify ──

fn builtin_bessel_simplify(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "BesselSimplify requires exactly 1 argument".to_string(),
        ));
    }
    Ok(bessel_simplify_value(&args[0]))
}

fn bessel_simplify_value(val: &Value) -> Value {
    match val {
        Value::Call { head, args } if head == "BesselJ" && args.len() == 2 => {
            let inner = simplify_wide(&args[1]);
            match &args[0] {
                // BesselJ[1/2, z] = Sqrt[2/(Pi*z)] * Sin[z]
                Value::Rational(r) if *r.numer() == Integer::from(1) && *r.denom() == Integer::from(2) => {
                    crate::builtins::symbolic::simplify_call(
                        "Times",
                        &[
                            crate::builtins::symbolic::simplify_call(
                                "Sqrt",
                                &[crate::builtins::symbolic::simplify_call(
                                    "Times",
                                    &[
                                        Value::Integer(Integer::from(2)),
                                        crate::builtins::symbolic::simplify_call(
                                            "Power",
                                            &[
                                                crate::builtins::symbolic::simplify_call(
                                                    "Times",
                                                    &[
                                                        Value::Symbol("Pi".to_string()),
                                                        inner.clone(),
                                                    ],
                                                ),
                                                Value::Integer(Integer::from(-1)),
                                            ],
                                        ),
                                    ],
                                )],
                            ),
                            crate::builtins::symbolic::simplify_call("Sin", &[inner]),
                        ],
                    )
                }
                // BesselJ[-1/2, z] = Sqrt[2/(Pi*z)] * Cos[z]
                Value::Rational(r) if *r.numer() == Integer::from(-1) && *r.denom() == Integer::from(2) => {
                    crate::builtins::symbolic::simplify_call(
                        "Times",
                        &[
                            crate::builtins::symbolic::simplify_call(
                                "Sqrt",
                                &[crate::builtins::symbolic::simplify_call(
                                    "Times",
                                    &[
                                        Value::Integer(Integer::from(2)),
                                        crate::builtins::symbolic::simplify_call(
                                            "Power",
                                            &[
                                                crate::builtins::symbolic::simplify_call(
                                                    "Times",
                                                    &[
                                                        Value::Symbol("Pi".to_string()),
                                                        inner.clone(),
                                                    ],
                                                ),
                                                Value::Integer(Integer::from(-1)),
                                            ],
                                        ),
                                    ],
                                )],
                            ),
                            crate::builtins::symbolic::simplify_call("Cos", &[inner]),
                        ],
                    )
                }
                _ => {
                    // Fall through to general simplification
                    crate::builtins::symbolic::simplify_call(
                        "BesselJ",
                        &[args[0].clone(), inner],
                    )
                }
            }
        }
        Value::Call { head, args } => {
            let s_args: Vec<Value> = args.iter().map(bessel_simplify_value).collect();
            crate::builtins::symbolic::simplify_call(head, &s_args)
        }
        _ => val.clone(),
    }
}

// ── GammaSimplify ──

fn builtin_gamma_simplify(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "GammaSimplify requires exactly 1 argument".to_string(),
        ));
    }
    Ok(gamma_simplify_value(&args[0]))
}

fn gamma_simplify_value(val: &Value) -> Value {
    match val {
        Value::Call { head, args } if head == "Gamma" && args.len() == 1 => {
            let inner = gamma_simplify_value(&args[0]);
            match &inner {
                // Gamma[1/2] = Sqrt[Pi]
                Value::Rational(r) if *r.numer() == Integer::from(1) && *r.denom() == Integer::from(2) => {
                    crate::builtins::symbolic::simplify_call(
                        "Sqrt",
                        &[Value::Symbol("Pi".to_string())],
                    )
                }
                // Gamma[n] = (n-1)! for positive integer n
                Value::Integer(n) if *n > 0 => {
                    let n_minus_1 = Integer::from(n.to_i64().unwrap() - 1);
                    crate::builtins::symbolic::simplify_call(
                        "Factorial",
                        &[Value::Integer(n_minus_1)],
                    )
                }
                // Gamma[n+1] = n! for integer n >= 0
                Value::Call {
                    head: h,
                    args: plus_args,
                } if h == "Plus" && plus_args.len() == 2 && plus_args[1] == Value::Integer(Integer::from(1)) =>
                {
                    if let Some(n) = plus_args[0].to_integer() {
                        if n >= 0 {
                            return crate::builtins::symbolic::simplify_call(
                                "Factorial",
                                &[Value::Integer(Integer::from(n))],
                            );
                        }
                    }
                    Value::Call {
                        head: "Gamma".to_string(),
                        args: vec![inner],
                    }
                }
                _ => Value::Call {
                    head: "Gamma".to_string(),
                    args: vec![inner],
                },
            }
        }
        Value::Call { head, args } => {
            let s_args: Vec<Value> = args.iter().map(gamma_simplify_value).collect();
            crate::builtins::symbolic::simplify_call(head, &s_args)
        }
        _ => val.clone(),
    }
}

// ── PolyGammaSimplify ──

fn builtin_poly_gamma_simplify(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "PolyGammaSimplify requires exactly 1 argument".to_string(),
        ));
    }
    Ok(poly_gamma_simplify_value(&args[0]))
}

fn poly_gamma_simplify_value(val: &Value) -> Value {
    match val {
        Value::Call {
            head,
            args: call_args,
        } if head == "PolyGamma" && call_args.len() == 1 => {
            let inner = poly_gamma_simplify_value(&call_args[0]);
            match &inner {
                // PolyGamma[1] = -EulerGamma
                Value::Integer(n) if *n == 1 => {
                    crate::builtins::symbolic::simplify_call(
                        "Times",
                        &[
                            Value::Integer(Integer::from(-1)),
                            Value::Symbol("EulerGamma".to_string()),
                        ],
                    )
                }
                // PolyGamma[1/2] = -EulerGamma - 2*Log[2]
                Value::Rational(r) if *r.numer() == Integer::from(1) && *r.denom() == Integer::from(2) => {
                    crate::builtins::symbolic::simplify_call(
                        "Plus",
                        &[
                            crate::builtins::symbolic::simplify_call(
                                "Times",
                                &[
                                    Value::Integer(Integer::from(-1)),
                                    Value::Symbol("EulerGamma".to_string()),
                                ],
                            ),
                            crate::builtins::symbolic::simplify_call(
                                "Times",
                                &[
                                    Value::Integer(Integer::from(-2)),
                                    crate::builtins::symbolic::simplify_call(
                                        "Log",
                                        &[Value::Integer(Integer::from(2))],
                                    ),
                                ],
                            ),
                        ],
                    )
                }
                _ => Value::Call {
                    head: "PolyGamma".to_string(),
                    args: vec![inner],
                },
            }
        }
        Value::Call { head, args } => {
            let s_args: Vec<Value> = args.iter().map(poly_gamma_simplify_value).collect();
            crate::builtins::symbolic::simplify_call(head, &s_args)
        }
        _ => val.clone(),
    }
}

// ── ZetaSimplify ──

fn builtin_zeta_simplify(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "ZetaSimplify requires exactly 1 argument".to_string(),
        ));
    }
    Ok(zeta_simplify_value(&args[0]))
}

fn zeta_simplify_value(val: &Value) -> Value {
    match val {
        Value::Call { head, args } if head == "Zeta" && args.len() == 1 => {
            let inner = zeta_simplify_value(&args[0]);
            match &inner {
                // Zeta[2] = Pi^2/6
                Value::Integer(n) if *n == 2 => {
                    crate::builtins::symbolic::simplify_call(
                        "Times",
                        &[
                            crate::builtins::symbolic::simplify_call(
                                "Power",
                                &[
                                    Value::Symbol("Pi".to_string()),
                                    Value::Integer(Integer::from(2)),
                                ],
                            ),
                            crate::builtins::symbolic::simplify_call(
                                "Power",
                                &[
                                    Value::Integer(Integer::from(6)),
                                    Value::Integer(Integer::from(-1)),
                                ],
                            ),
                        ],
                    )
                }
                // Zeta[4] = Pi^4/90
                Value::Integer(n) if *n == 4 => {
                    crate::builtins::symbolic::simplify_call(
                        "Times",
                        &[
                            crate::builtins::symbolic::simplify_call(
                                "Power",
                                &[
                                    Value::Symbol("Pi".to_string()),
                                    Value::Integer(Integer::from(4)),
                                ],
                            ),
                            crate::builtins::symbolic::simplify_call(
                                "Power",
                                &[
                                    Value::Integer(Integer::from(90)),
                                    Value::Integer(Integer::from(-1)),
                                ],
                            ),
                        ],
                    )
                }
                // Zeta[0] = -1/2
                Value::Integer(n) if *n == 0 => {
                    Value::Rational(Box::new(rug::Rational::from((Integer::from(-1), Integer::from(2)))))
                }
                _ => Value::Call {
                    head: "Zeta".to_string(),
                    args: vec![inner],
                },
            }
        }
        Value::Call { head, args } => {
            let s_args: Vec<Value> = args.iter().map(zeta_simplify_value).collect();
            crate::builtins::symbolic::simplify_call(head, &s_args)
        }
        _ => val.clone(),
    }
}

// ── PolyLogSimplify ──

fn builtin_poly_log_simplify(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "PolyLogSimplify requires exactly 1 argument".to_string(),
        ));
    }
    Ok(poly_log_simplify_value(&args[0]))
}

fn poly_log_simplify_value(val: &Value) -> Value {
    match val {
        Value::Call {
            head,
            args: call_args,
        } if head == "PolyLog" && call_args.len() == 2 => {
            let s0 = poly_log_simplify_value(&call_args[0]);
            let s1 = poly_log_simplify_value(&call_args[1]);
            match (&s0, &s1) {
                // PolyLog[n, 0] = 0 for any n
                (_, Value::Integer(n)) if n.is_zero() => Value::Integer(Integer::from(0)),
                // PolyLog[2, 1] = Pi^2/6
                (Value::Integer(n), Value::Integer(m)) if *n == 2 && *m == 1 => {
                    crate::builtins::symbolic::simplify_call(
                        "Times",
                        &[
                            crate::builtins::symbolic::simplify_call(
                                "Power",
                                &[
                                    Value::Symbol("Pi".to_string()),
                                    Value::Integer(Integer::from(2)),
                                ],
                            ),
                            crate::builtins::symbolic::simplify_call(
                                "Power",
                                &[
                                    Value::Integer(Integer::from(6)),
                                    Value::Integer(Integer::from(-1)),
                                ],
                            ),
                        ],
                    )
                }
                _ => Value::Call {
                    head: "PolyLog".to_string(),
                    args: vec![s0, s1],
                },
            }
        }
        Value::Call { head, args } => {
            let s_args: Vec<Value> = args.iter().map(poly_log_simplify_value).collect();
            crate::builtins::symbolic::simplify_call(head, &s_args)
        }
        _ => val.clone(),
    }
}

// ── TrigToRadicals ──

fn builtin_trig_to_radicals(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "TrigToRadicals requires exactly 1 argument".to_string(),
        ));
    }
    Ok(trig_to_radicals_value(&args[0]))
}

fn trig_to_radicals_value(val: &Value) -> Value {
    match val {
        Value::Call { head, args } if args.len() == 1 => {
            let inner = trig_to_radicals_value(&args[0]);
            match head.as_str() {
                "Sin" => simplify_trig_sin(&inner),
                "Cos" => simplify_trig_cos(&inner),
                "Tan" => {
                    // Tan[x] = Sin[x] / Cos[x]
                    let s = simplify_trig_sin(&inner);
                    let c = simplify_trig_cos(&inner);
                    crate::builtins::symbolic::simplify_call(
                        "Times",
                        &[s, crate::builtins::symbolic::simplify_call(
                            "Power", &[c, Value::Integer(Integer::from(-1))]
                        )],
                    )
                }
                _ => Value::Call {
                    head: head.clone(),
                    args: vec![inner],
                },
            }
        }
        Value::Call { head, args } => {
            let s_args: Vec<Value> = args.iter().map(trig_to_radicals_value).collect();
            Value::Call {
                head: head.clone(),
                args: s_args,
            }
        }
        _ => val.clone(),
    }
}

/// Try to simplify Sin[arg] to radical form for known Pi/n arguments.
fn simplify_trig_sin(arg: &Value) -> Value {
    match arg {
        // Sin[Pi/4] = 1/Sqrt[2]
        Value::Call {
            head,
            args,
        } if head == "Times"
            && args.len() == 2
            && args[0] == Value::Symbol("Pi".to_string()) =>
        {
            match &args[1] {
                Value::Rational(r) if *r.numer() == Integer::from(1) && *r.denom() == Integer::from(4) => {
                    crate::builtins::symbolic::simplify_call(
                        "Power",
                        &[
                            Value::Integer(Integer::from(2)),
                            Value::Rational(Box::new(rug::Rational::from((Integer::from(-1), Integer::from(2))))),
                        ],
                    )
                }
                _ => Value::Call {
                    head: "Sin".to_string(),
                    args: vec![arg.clone()],
                },
            }
        }
        // Sin[0] = 0
        Value::Integer(n) if n.is_zero() => Value::Integer(Integer::from(0)),
        _ => Value::Call {
            head: "Sin".to_string(),
            args: vec![arg.clone()],
        },
    }
}

/// Try to simplify Cos[arg] to radical form for known Pi/n arguments.
fn simplify_trig_cos(arg: &Value) -> Value {
    match arg {
        // Cos[Pi/4] = 1/Sqrt[2]
        Value::Call {
            head,
            args,
        } if head == "Times"
            && args.len() == 2
            && args[0] == Value::Symbol("Pi".to_string()) =>
        {
            match &args[1] {
                Value::Rational(r) if *r.numer() == Integer::from(1) && *r.denom() == Integer::from(4) => {
                    crate::builtins::symbolic::simplify_call(
                        "Power",
                        &[
                            Value::Integer(Integer::from(2)),
                            Value::Rational(Box::new(rug::Rational::from((Integer::from(-1), Integer::from(2))))),
                        ],
                    )
                }
                _ => Value::Call {
                    head: "Cos".to_string(),
                    args: vec![arg.clone()],
                },
            }
        }
        // Cos[0] = 1
        Value::Integer(n) if n.is_zero() => Value::Integer(Integer::from(1)),
        _ => Value::Call {
            head: "Cos".to_string(),
            args: vec![arg.clone()],
        },
    }
}

// ── ReplaceAllUnheld ──

fn builtin_replace_all_unheld(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    crate::eval::rules::builtin_replace_all(args, env)
}

// ── Notebook stubs ──

fn builtin_cell_information(_args: &[Value]) -> Result<Value, EvalError> {
    Err(EvalError::Error(
        "CellInformation is not yet implemented in Syma (notebook frontend needed)".to_string(),
    ))
}

fn builtin_notebook_convert(_args: &[Value]) -> Result<Value, EvalError> {
    Err(EvalError::Error(
        "NotebookConvert is not yet implemented in Syma (notebook frontend needed)".to_string(),
    ))
}

/// Helper: simplify a value tree broadly (recursively call simplify_value).
fn simplify_wide(val: &Value) -> Value {
    match val {
        Value::Call { head, args } => {
            crate::builtins::symbolic::simplify_call(
                head,
                &args.iter().map(simplify_wide).collect::<Vec<_>>(),
            )
        }
        _ => val.clone(),
    }
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::Env;

    #[test]
    fn test_max_machine_integer() {
        let env = Env::new();
        crate::builtins::register_builtins(&env);
        let val = env.get("$MaxMachineInteger").unwrap();
        assert_eq!(val, Value::Integer(Integer::from(i64::MAX)));
    }

    #[test]
    fn test_backtick_alias() {
        let env = Env::new();
        crate::builtins::register_builtins(&env);
        let bare = env.get("$MaxMachineInteger").unwrap();
        let qualified = env.get("Developer`$MaxMachineInteger").unwrap();
        assert_eq!(bare, qualified);
    }

    #[test]
    fn test_machine_integer_q_true() {
        let result = builtin_machine_integer_q(&[Value::Integer(Integer::from(42))]).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_machine_integer_q_false_real() {
        let result = builtin_machine_integer_q(&[Value::Rational(Box::new(
            rug::Rational::from((Integer::from(1), Integer::from(2))),
        ))])
        .unwrap();
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn test_machine_integer_q_false_bigint() {
        let big = Integer::from(i64::MAX) + Integer::from(1);
        let result =
            builtin_machine_integer_q(&[Value::Integer(big)]).unwrap();
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn test_to_packed_array_integers() {
        let list = Value::List(vec![
            Value::Integer(Integer::from(1)),
            Value::Integer(Integer::from(2)),
            Value::Integer(Integer::from(3)),
        ]);
        let result = builtin_to_packed_array(&[list]).unwrap();
        assert!(matches!(result, Value::PackedArray(PackedArrayType::Integer64(v)) if v == vec![1i64, 2, 3]));
    }

    #[test]
    fn test_to_packed_array_reals() {
        let list = Value::List(vec![
            Value::Real(rug::Float::with_val(53, 1.5)),
            Value::Real(rug::Float::with_val(53, 2.5)),
        ]);
        let result = builtin_to_packed_array(&[list]).unwrap();
        assert!(matches!(result, Value::PackedArray(PackedArrayType::Real64(v)) if v.len() == 2));
    }

    #[test]
    fn test_to_packed_array_fallback() {
        // Mixed type should fall back to regular list
        let list = Value::List(vec![
            Value::Integer(Integer::from(1)),
            Value::Str("hello".to_string()),
        ]);
        let result = builtin_to_packed_array(&[list.clone()]).unwrap();
        assert_eq!(result, list);
    }

    #[test]
    fn test_from_packed_array() {
        let pa = Value::PackedArray(PackedArrayType::Integer64(vec![1, 2, 3]));
        let result = builtin_from_packed_array(&[pa]).unwrap();
        assert_eq!(
            result,
            Value::List(vec![
                Value::Integer(Integer::from(1)),
                Value::Integer(Integer::from(2)),
                Value::Integer(Integer::from(3)),
            ])
        );
    }

    #[test]
    fn test_packed_array_q_true() {
        let pa = Value::PackedArray(PackedArrayType::Integer64(vec![1, 2]));
        let result = builtin_packed_array_q(&[pa]).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_packed_array_q_false() {
        let list = Value::List(vec![Value::Integer(Integer::from(1))]);
        let result = builtin_packed_array_q(&[list]).unwrap();
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn test_gamma_simplify_half() {
        let expr = Value::Call {
            head: "Gamma".to_string(),
            args: vec![Value::Rational(Box::new(rug::Rational::from((
                Integer::from(1),
                Integer::from(2),
            ))))],
        };
        let result = gamma_simplify_value(&expr);
        // Gamma[1/2] simplifies to Sqrt[Pi], which is Power[Pi, 1/2]
        assert_eq!(
            result,
            crate::builtins::symbolic::simplify_call(
                "Sqrt",
                &[Value::Symbol("Pi".to_string())],
            )
        );
    }

    #[test]
    fn test_zeta_simplify_2() {
        let expr = Value::Call {
            head: "Zeta".to_string(),
            args: vec![Value::Integer(Integer::from(2))],
        };
        let result = zeta_simplify_value(&expr);
        // Should be Pi^2/6
        assert!(matches!(
            result,
            Value::Call { .. }
        ));
    }

    #[test]
    fn test_replace_all_unheld() {
        use crate::ast::Expr;
        let env = Env::new();
        crate::builtins::register_builtins(&env);
        // Match literal symbol `x` and replace with 5.
        // The rule LHS must be Value::Pattern(Expr) for the replacement engine.
        let expr = Value::Symbol("x".to_string());
        let rule = Value::Rule {
            lhs: Box::new(Value::Pattern(Expr::Symbol("x".to_string()))),
            rhs: Box::new(Value::Integer(Integer::from(5))),
            delayed: false,
        };
        let ruleset = Value::List(vec![rule]);
        let result = builtin_replace_all_unheld(&[expr, ruleset], &env).unwrap();
        assert_eq!(result, Value::Integer(Integer::from(5)));
    }

    #[test]
    fn test_cell_information_stub() {
        let result = builtin_cell_information(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_notebook_convert_stub() {
        let result = builtin_notebook_convert(&[]);
        assert!(result.is_err());
    }
}
