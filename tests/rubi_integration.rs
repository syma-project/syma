#[cfg(feature = "rubi")]
mod rubi_tests {
    use syma::rubi::engine::RubiEngine;
    use syma::rubi::wl_ast::{BinOp, IntRule, RuleFile, WLExpr};
    use syma::value::Value;

    // ── Helpers ──

    fn sym(s: &str) -> Value {
        Value::Symbol(s.to_string())
    }

    fn int(n: i64) -> Value {
        Value::Integer(rug::Integer::from(n))
    }

    fn call(head: &str, args: Vec<Value>) -> Value {
        Value::Call {
            head: head.to_string(),
            args,
        }
    }

    /// Build the basic rule set matching what's in src/rubi/mod.rs builtin_rules().
    fn power_rule_rules() -> Vec<RuleFile> {
        use WLExpr::*;

        // Rule 1: Int[1/x_, x_Symbol] := Log[x]
        let rule_1_over_x = IntRule {
            index: 0,
            source: "test: 1/x".to_string(),
            pattern: BinaryOp {
                op: BinOp::Divide,
                lhs: Box::new(Integer(1)),
                rhs: Box::new(NamedBlank("x".to_string())),
            },
            result: Call {
                head: Box::new(Symbol("Log".to_string())),
                args: vec![Symbol("x".to_string())],
            },
            condition: None,
        };

        // Rule 2: Int[x_^m_., x_Symbol] := x^(m+1)/(m+1) /; FreeQ[m, x] && NeQ[m, -1]
        let rule_power = IntRule {
            index: 1,
            source: "test: x^m".to_string(),
            pattern: BinaryOp {
                op: BinOp::Power,
                lhs: Box::new(NamedBlank("x".to_string())),
                rhs: Box::new(Optional("m".to_string())),
            },
            result: BinaryOp {
                op: BinOp::Divide,
                lhs: Box::new(BinaryOp {
                    op: BinOp::Power,
                    lhs: Box::new(Symbol("x".to_string())),
                    rhs: Box::new(BinaryOp {
                        op: BinOp::Plus,
                        lhs: Box::new(Symbol("m".to_string())),
                        rhs: Box::new(Integer(1)),
                    }),
                }),
                rhs: Box::new(BinaryOp {
                    op: BinOp::Plus,
                    lhs: Box::new(Symbol("m".to_string())),
                    rhs: Box::new(Integer(1)),
                }),
            },
            condition: Some(BinaryOp {
                op: BinOp::And,
                lhs: Box::new(Call {
                    head: Box::new(Symbol("FreeQ".to_string())),
                    args: vec![Symbol("m".to_string()), Symbol("x".to_string())],
                }),
                rhs: Box::new(Call {
                    head: Box::new(Symbol("NeQ".to_string())),
                    args: vec![Symbol("m".to_string()), Integer(-1)],
                }),
            }),
        };

        let file = RuleFile {
            name: "test_power_rules".to_string(),
            rules: vec![rule_1_over_x, rule_power],
        };

        vec![file]
    }

    // ── Integration Tests ──

    #[test]
    fn test_integrate_x_power_constant() {
        let mut engine = RubiEngine::new();
        engine.load_rules(power_rule_rules());

        // Integrate[x^2, x] should match rule 2 (x^m with m=2)
        let result = engine.integrate(&call("Power", vec![sym("x"), int(2)]), "x");
        assert!(result.is_ok(), "Integration failed: {:?}", result.err());
        let val = result.unwrap();

        // Expected: Times[Power[x, Plus[2, 1]], Power[Plus[2, 1], -1]]
        // i.e., x^3 / 3
        assert!(
            !matches!(val, Value::Call { ref head, .. } if head == "Integrate"),
            "Integration returned unevaluated: {:?}",
            val
        );
    }

    #[test]
    fn test_integrate_x_power_1() {
        let mut engine = RubiEngine::new();
        engine.load_rules(power_rule_rules());

        // Integrate[x^1, x] → x^2/2
        let result = engine.integrate(&call("Power", vec![sym("x"), int(1)]), "x");
        assert!(result.is_ok());
    }

    #[test]
    fn test_integrate_1_over_x() {
        let mut engine = RubiEngine::new();
        engine.load_rules(power_rule_rules());

        // Integrate[1/x, x] → Log[x]
        let result = engine.integrate(
            &call(
                "Times",
                vec![int(1), call("Power", vec![sym("x"), int(-1)])],
            ),
            "x",
        );
        assert!(result.is_ok(), "Integration failed: {:?}", result.err());
        let val = result.unwrap();

        // Should match Log[x]
        assert!(
            !matches!(&val, Value::Call { head, .. } if head == "Integrate"),
            "Expected evaluated result, got: {:?}",
            val
        );
    }

    #[test]
    fn test_integrate_x_squared_constant_times() {
        let mut engine = RubiEngine::new();
        engine.load_rules(power_rule_rules());

        // Integrate[3*x^2, x] — the engine needs to handle this
        // x^2 runs through rule 2, producing x^3/3
        let result = engine.integrate(
            &call("Times", vec![int(3), call("Power", vec![sym("x"), int(2)])]),
            "x",
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_integrate_constant() {
        let mut engine = RubiEngine::new();
        engine.load_rules(power_rule_rules());

        // Integrate[5, x] — constant, x^0 case
        // This should match x_^m_ with x=5, m=0, producing 5*x
        let result = engine.integrate(&int(5), "x");
        assert!(result.is_ok());
    }

    #[test]
    fn test_integrate_no_match_returns_unevaluated() {
        let mut engine = RubiEngine::new();
        engine.load_rules(power_rule_rules());

        // Integrate[Sin[x], x] — no rule matches
        let result = engine.integrate(&call("Sin", vec![sym("x")]), "x");
        assert!(result.is_ok());
        let val = result.unwrap();
        assert!(
            matches!(&val, Value::Call { head, .. } if head == "Integrate"),
            "Expected unevaluated Integrate call, got: {:?}",
            val
        );
    }

    #[test]
    fn test_engine_empty_rules() {
        let mut engine = RubiEngine::new();

        // With no rules loaded, everything returns unevaluated
        let result = engine.integrate(&call("Power", vec![sym("x"), int(2)]), "x");
        assert!(result.is_ok());
        let val = result.unwrap();
        assert!(
            matches!(&val, Value::Call { head, .. } if head == "Integrate"),
            "Expected unevaluated, got: {:?}",
            val
        );
    }

    #[test]
    fn test_integrate_multiple_rules_loaded() {
        let mut engine = RubiEngine::new();

        // Load rules in two batches
        let file1 = RuleFile {
            name: "batch1".to_string(),
            rules: vec![IntRule {
                index: 0,
                source: "batch1: 1/x".to_string(),
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
            }],
        };

        let file2 = RuleFile {
            name: "batch2".to_string(),
            rules: vec![IntRule {
                index: 0,
                source: "batch2: x^m".to_string(),
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
            }],
        };

        engine.load_rules(vec![file1, file2]);
        assert!(engine.is_loaded());

        // Should still work
        let result = engine.integrate(&call("Power", vec![sym("x"), int(3)]), "x");
        assert!(result.is_ok());
    }

    #[test]
    fn test_rule_ordering_matters() {
        let mut engine = RubiEngine::new();

        // Two rules: one that matches x and returns 42, and a generic one
        engine.load_rules(vec![RuleFile {
            name: "special_first".to_string(),
            rules: vec![
                // First rule: match x specifically -> 42
                IntRule {
                    index: 0,
                    source: "x -> 42".to_string(),
                    pattern: WLExpr::Symbol("x".to_string()),
                    result: WLExpr::Integer(42),
                    condition: None,
                },
                // Second rule: match anything -> 0
                IntRule {
                    index: 1,
                    source: "_ -> 0".to_string(),
                    pattern: WLExpr::Blank,
                    result: WLExpr::Integer(0),
                    condition: None,
                },
            ],
        }]);

        // Integrating wrt x should match the first rule (x -> 42)
        let result = engine.integrate(&sym("x"), "x");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), int(42));

        // Integrating something else should match the second rule (_ -> 0)
        let result = engine.integrate(&sym("y"), "x");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), int(0));
    }

    #[test]
    fn test_integrate_respects_variable_name() {
        let mut engine = RubiEngine::new();
        engine.load_rules(power_rule_rules());

        // Integrate[y^2, y] should work (variable named y)
        let result = engine.integrate(&call("Power", vec![sym("y"), int(2)]), "y");
        assert!(result.is_ok());
        let val = result.unwrap();
        assert!(
            !matches!(&val, Value::Call { head, .. } if head == "Integrate"),
            "Integration w.r.t. y failed: {:?}",
            val
        );

        // Note: The current rule engine matches NamedBlank("x") to any base,
        // so Integrate[y^2, x] matches the power rule with x_=y, m=2.
        // This is a known limitation of the prototype.
        let result2 = engine.integrate(&call("Power", vec![sym("y"), int(2)]), "x");
        assert!(result2.is_ok());
    }
} // mod rubi_tests
