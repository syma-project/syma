/// Rubi (Rule-Based Integrator) module for Syma.
///
/// This module implements rule-based integration using Rubi's
/// integration rules, loaded lazily on first `Integrate` call.
///
/// The rules are stored as Wolfram Language `.m` files in the
/// Rubi reference directory and converted to Rust data structures
/// at build time.
pub mod engine;
pub mod helpers;
pub mod parser;
pub mod rule;
pub mod wl_ast;

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::OnceLock;

use crate::env::{Env, LazyProvider};
use crate::rubi::engine::RubiEngine;
use crate::value::{EvalError, Value};

/// Global Rubi engine instance, initialized on first use.
fn global_rubi_engine() -> &'static Mutex<RubiEngine> {
    static ENGINE: OnceLock<Mutex<RubiEngine>> = OnceLock::new();
    ENGINE.get_or_init(|| {
        let engine = RubiEngine::new();
        let mut eng = engine;
        let rules = builtin_rules();
        eng.load_rules(rules);
        Mutex::new(eng)
    })
}

/// Register the Rubi integration lazy provider.
///
/// This replaces the existing `Integrate` lazy provider (which loads
/// the builtin stub) with one that loads the Rubi rule engine.
pub fn register_integrate(env: &Env) {
    env.register_lazy_provider(
        "Integrate",
        LazyProvider::Custom(Arc::new(move |env| load_rubi_integrate(env))),
    );
}

/// Create a basic Integrate builtin that uses the Rubi engine.
fn load_rubi_integrate(env: &Env) -> Result<Value, EvalError> {
    // Ensure the global engine is initialized with rules
    global_rubi_engine();

    // Create the builtin function — a plain fn pointer, no captures
    fn integrate_builtin(args: &[Value]) -> Result<Value, EvalError> {
        if args.len() != 2 {
            return Err(EvalError::Error(
                "Integrate requires exactly 2 arguments".to_string(),
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
        let mut eng = global_rubi_engine().lock().unwrap();
        eng.integrate(&args[0], &var)
    }

    env.set(
        "Integrate".to_string(),
        Value::Builtin("Integrate".to_string(), integrate_builtin),
    );
    Ok(Value::Symbol("Null".to_string()))
}

/// Built-in rule definitions for the Rubi engine.
///
/// These are manually defined rules that cover basic integration cases.
/// In a full implementation, these would be automatically generated from
/// Rubi `.m` files.
fn builtin_rules() -> Vec<crate::rubi::rule::RuleFile> {
    use crate::rubi::wl_ast::*;

    // Category 1.1.1.1: (a+b x)^m
    let rules_1111 = vec![
        // Int[1/x_, x_Symbol] := Log[x]
        IntRule {
            index: 0,
            source: "1.1.1.1 (a+b x)^m".to_string(),
            pattern: WLExpr::BinaryOp {
                op: BinOp::Divide,
                lhs: Box::new(WLExpr::Integer(1)),
                rhs: Box::new(WLExpr::NamedBlank("x".to_string())),
            },
            result: WLExpr::Call {
                head: Box::new(WLExpr::Symbol("Log".to_string())),
                args: vec![WLExpr::Symbol("x".to_string())],
            },
            condition: None,
        },
        // Int[x_^m_., x_Symbol] := x^(m + 1)/(m + 1) /; FreeQ[m, x] && NeQ[m, -1]
        IntRule {
            index: 1,
            source: "1.1.1.1 (a+b x)^m".to_string(),
            pattern: WLExpr::BinaryOp {
                op: BinOp::Power,
                lhs: Box::new(WLExpr::NamedBlank("x".to_string())),
                rhs: Box::new(WLExpr::Optional("m".to_string())),
            },
            result: WLExpr::BinaryOp {
                op: BinOp::Divide,
                lhs: Box::new(WLExpr::BinaryOp {
                    op: BinOp::Power,
                    lhs: Box::new(WLExpr::Symbol("x".to_string())),
                    rhs: Box::new(WLExpr::BinaryOp {
                        op: BinOp::Plus,
                        lhs: Box::new(WLExpr::Symbol("m".to_string())),
                        rhs: Box::new(WLExpr::Integer(1)),
                    }),
                }),
                rhs: Box::new(WLExpr::BinaryOp {
                    op: BinOp::Plus,
                    lhs: Box::new(WLExpr::Symbol("m".to_string())),
                    rhs: Box::new(WLExpr::Integer(1)),
                }),
            },
            condition: Some(WLExpr::BinaryOp {
                op: BinOp::And,
                lhs: Box::new(WLExpr::Call {
                    head: Box::new(WLExpr::Symbol("FreeQ".to_string())),
                    args: vec![
                        WLExpr::Symbol("m".to_string()),
                        WLExpr::Symbol("x".to_string()),
                    ],
                }),
                rhs: Box::new(WLExpr::Call {
                    head: Box::new(WLExpr::Symbol("NeQ".to_string())),
                    args: vec![WLExpr::Symbol("m".to_string()), WLExpr::Integer(-1)],
                }),
            }),
        },
        // Int[1/(a_ + b_.*x_), x_Symbol] := Log[RemoveContent[a + b*x, x]]/b /; FreeQ[{a, b}, x]
        IntRule {
            index: 2,
            source: "1.1.1.1 (a+b x)^m".to_string(),
            pattern: WLExpr::BinaryOp {
                op: BinOp::Divide,
                lhs: Box::new(WLExpr::Integer(1)),
                rhs: Box::new(WLExpr::BinaryOp {
                    op: BinOp::Plus,
                    lhs: Box::new(WLExpr::Optional("a".to_string())),
                    rhs: Box::new(WLExpr::BinaryOp {
                        op: BinOp::Times,
                        lhs: Box::new(WLExpr::Optional("b".to_string())),
                        rhs: Box::new(WLExpr::NamedBlank("x".to_string())),
                    }),
                }),
            },
            result: WLExpr::BinaryOp {
                op: BinOp::Divide,
                lhs: Box::new(WLExpr::Call {
                    head: Box::new(WLExpr::Symbol("Log".to_string())),
                    args: vec![WLExpr::Call {
                        head: Box::new(WLExpr::Symbol("RemoveContent".to_string())),
                        args: vec![
                            WLExpr::BinaryOp {
                                op: BinOp::Plus,
                                lhs: Box::new(WLExpr::Symbol("a".to_string())),
                                rhs: Box::new(WLExpr::BinaryOp {
                                    op: BinOp::Times,
                                    lhs: Box::new(WLExpr::Symbol("b".to_string())),
                                    rhs: Box::new(WLExpr::Symbol("x".to_string())),
                                }),
                            },
                            WLExpr::Symbol("x".to_string()),
                        ],
                    }],
                }),
                rhs: Box::new(WLExpr::Symbol("b".to_string())),
            },
            condition: Some(WLExpr::Call {
                head: Box::new(WLExpr::Symbol("FreeQ".to_string())),
                args: vec![
                    WLExpr::List(vec![
                        WLExpr::Symbol("a".to_string()),
                        WLExpr::Symbol("b".to_string()),
                    ]),
                    WLExpr::Symbol("x".to_string()),
                ],
            }),
        },
        // Int[(a_. + b_.*x_)^m_, x_Symbol] := (a + b*x)^(m + 1)/(b*(m + 1)) /; FreeQ[{a, b, m}, x] && NeQ[m, -1]
        IntRule {
            index: 3,
            source: "1.1.1.1 (a+b x)^m".to_string(),
            pattern: WLExpr::BinaryOp {
                op: BinOp::Power,
                lhs: Box::new(WLExpr::BinaryOp {
                    op: BinOp::Plus,
                    lhs: Box::new(WLExpr::Optional("a".to_string())),
                    rhs: Box::new(WLExpr::BinaryOp {
                        op: BinOp::Times,
                        lhs: Box::new(WLExpr::Optional("b".to_string())),
                        rhs: Box::new(WLExpr::NamedBlank("x".to_string())),
                    }),
                }),
                rhs: Box::new(WLExpr::Optional("m".to_string())),
            },
            result: WLExpr::BinaryOp {
                op: BinOp::Divide,
                lhs: Box::new(WLExpr::BinaryOp {
                    op: BinOp::Power,
                    lhs: Box::new(WLExpr::BinaryOp {
                        op: BinOp::Plus,
                        lhs: Box::new(WLExpr::Symbol("a".to_string())),
                        rhs: Box::new(WLExpr::BinaryOp {
                            op: BinOp::Times,
                            lhs: Box::new(WLExpr::Symbol("b".to_string())),
                            rhs: Box::new(WLExpr::Symbol("x".to_string())),
                        }),
                    }),
                    rhs: Box::new(WLExpr::BinaryOp {
                        op: BinOp::Plus,
                        lhs: Box::new(WLExpr::Symbol("m".to_string())),
                        rhs: Box::new(WLExpr::Integer(1)),
                    }),
                }),
                rhs: Box::new(WLExpr::BinaryOp {
                    op: BinOp::Times,
                    lhs: Box::new(WLExpr::Symbol("b".to_string())),
                    rhs: Box::new(WLExpr::BinaryOp {
                        op: BinOp::Plus,
                        lhs: Box::new(WLExpr::Symbol("m".to_string())),
                        rhs: Box::new(WLExpr::Integer(1)),
                    }),
                }),
            },
            condition: Some(WLExpr::BinaryOp {
                op: BinOp::And,
                lhs: Box::new(WLExpr::Call {
                    head: Box::new(WLExpr::Symbol("FreeQ".to_string())),
                    args: vec![
                        WLExpr::List(vec![
                            WLExpr::Symbol("a".to_string()),
                            WLExpr::Symbol("b".to_string()),
                            WLExpr::Symbol("m".to_string()),
                        ]),
                        WLExpr::Symbol("x".to_string()),
                    ],
                }),
                rhs: Box::new(WLExpr::Call {
                    head: Box::new(WLExpr::Symbol("NeQ".to_string())),
                    args: vec![WLExpr::Symbol("m".to_string()), WLExpr::Integer(-1)],
                }),
            }),
        },
        // Int[(a_. + b_.*u_)^m_, x_Symbol] := 1/Coefficient[u, x, 1]*Subst[Int[(a + b*x)^m, x], x, u] /; FreeQ[{a, b, m}, x] && LinearQ[u, x] && NeQ[u, x]
        IntRule {
            index: 4,
            source: "1.1.1.1 (a+b x)^m".to_string(),
            pattern: WLExpr::BinaryOp {
                op: BinOp::Power,
                lhs: Box::new(WLExpr::BinaryOp {
                    op: BinOp::Plus,
                    lhs: Box::new(WLExpr::Optional("a".to_string())),
                    rhs: Box::new(WLExpr::BinaryOp {
                        op: BinOp::Times,
                        lhs: Box::new(WLExpr::Optional("b".to_string())),
                        rhs: Box::new(WLExpr::NamedBlank("u".to_string())),
                    }),
                }),
                rhs: Box::new(WLExpr::Optional("m".to_string())),
            },
            result: WLExpr::BinaryOp {
                op: BinOp::Times,
                lhs: Box::new(WLExpr::BinaryOp {
                    op: BinOp::Divide,
                    lhs: Box::new(WLExpr::Integer(1)),
                    rhs: Box::new(WLExpr::Call {
                        head: Box::new(WLExpr::Symbol("Coefficient".to_string())),
                        args: vec![
                            WLExpr::Symbol("u".to_string()),
                            WLExpr::Symbol("x".to_string()),
                            WLExpr::Integer(1),
                        ],
                    }),
                }),
                rhs: Box::new(WLExpr::Call {
                    head: Box::new(WLExpr::Symbol("Subst".to_string())),
                    args: vec![
                        WLExpr::Call {
                            head: Box::new(WLExpr::Symbol("Int".to_string())),
                            args: vec![
                                WLExpr::BinaryOp {
                                    op: BinOp::Power,
                                    lhs: Box::new(WLExpr::BinaryOp {
                                        op: BinOp::Plus,
                                        lhs: Box::new(WLExpr::Symbol("a".to_string())),
                                        rhs: Box::new(WLExpr::BinaryOp {
                                            op: BinOp::Times,
                                            lhs: Box::new(WLExpr::Symbol("b".to_string())),
                                            rhs: Box::new(WLExpr::Symbol("x".to_string())),
                                        }),
                                    }),
                                    rhs: Box::new(WLExpr::Symbol("m".to_string())),
                                },
                                WLExpr::Symbol("x".to_string()),
                            ],
                        },
                        WLExpr::Symbol("x".to_string()),
                        WLExpr::Symbol("u".to_string()),
                    ],
                }),
            },
            condition: Some(WLExpr::BinaryOp {
                op: BinOp::And,
                lhs: Box::new(WLExpr::BinaryOp {
                    op: BinOp::And,
                    lhs: Box::new(WLExpr::Call {
                        head: Box::new(WLExpr::Symbol("FreeQ".to_string())),
                        args: vec![
                            WLExpr::List(vec![
                                WLExpr::Symbol("a".to_string()),
                                WLExpr::Symbol("b".to_string()),
                                WLExpr::Symbol("m".to_string()),
                            ]),
                            WLExpr::Symbol("x".to_string()),
                        ],
                    }),
                    rhs: Box::new(WLExpr::Call {
                        head: Box::new(WLExpr::Symbol("LinearQ".to_string())),
                        args: vec![
                            WLExpr::Symbol("u".to_string()),
                            WLExpr::Symbol("x".to_string()),
                        ],
                    }),
                }),
                rhs: Box::new(WLExpr::Call {
                    head: Box::new(WLExpr::Symbol("NeQ".to_string())),
                    args: vec![
                        WLExpr::Symbol("u".to_string()),
                        WLExpr::Symbol("x".to_string()),
                    ],
                }),
            }),
        },
    ];

    let file_1111 = crate::rubi::rule::RuleFile {
        name: "1.1.1.1 (a+b x)^m".to_string(),
        rules: rules_1111,
    };

    vec![file_1111]
}

/// Register the Rubi lazy provider (called during builtin registration).
pub fn register(env: &Env) {
    register_integrate(env);
}
