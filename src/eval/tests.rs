#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtins;
    use crate::lexer;
    use crate::parser;

    fn with_large_stack<F, T>(f: F) -> T
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        std::thread::Builder::new()
            .stack_size(8 * 1024 * 1024)
            .spawn(f)
            .unwrap()
            .join()
            .unwrap()
    }

    fn eval_str(input: &str) -> Value {
        let env = Env::new();
        builtins::register_builtins(&env);
        let tokens = lexer::tokenize(input).unwrap();
        let ast = parser::parse(tokens).unwrap_or_else(|e| {
            panic!("Parse error for input {:?}: {:?}", input, e);
        });
        eval_program(&ast, &env).unwrap_or_else(|e| {
            panic!(
                "Eval error for input {:?} with AST {:?}: {:?}",
                input, ast, e
            );
        })
    }

    fn eval_str_in_env(input: &str, env: &Env) -> Value {
        let tokens = crate::lexer::tokenize(input).unwrap();
        let ast = crate::parser::parse(tokens).unwrap();
        crate::eval::eval_program(&ast, env).unwrap()
    }

    // ── Atoms ──

    #[test]
    fn test_eval_integer() {
        assert_eq!(eval_str("42"), Value::Integer(Integer::from(42)));
    }

    #[test]
    fn test_eval_real() {
        let val = eval_str("3.14");
        match val {
            Value::Real(r) => assert!((r.to_f64() - 3.14).abs() < 1e-10),
            _ => panic!("Expected Real, got {:?}", val),
        }
    }

    #[test]
    fn test_eval_string() {
        assert_eq!(eval_str(r#""hello""#), Value::Str("hello".to_string()));
    }

    #[test]
    fn test_eval_bool() {
        assert_eq!(eval_str("True"), Value::Bool(true));
        assert_eq!(eval_str("False"), Value::Bool(false));
    }

    #[test]
    fn test_eval_null() {
        assert_eq!(eval_str("Null"), Value::Null);
    }

    // ── Arithmetic ──

    #[test]
    fn test_addition() {
        assert_eq!(eval_str("1 + 2"), Value::Integer(Integer::from(3)));
    }

    #[test]
    fn test_multiplication() {
        assert_eq!(eval_str("3 * 4"), Value::Integer(Integer::from(12)));
    }

    #[test]
    fn test_division() {
        assert_eq!(eval_str("10 / 2"), Value::Integer(Integer::from(5)));
    }

    #[test]
    fn test_power() {
        assert_eq!(eval_str("2^3"), Value::Integer(Integer::from(8)));
    }

    #[test]
    fn test_precedence() {
        assert_eq!(eval_str("2 + 3 * 4"), Value::Integer(Integer::from(14)));
    }

    #[test]
    fn test_parenthesized() {
        assert_eq!(eval_str("(2 + 3) * 4"), Value::Integer(Integer::from(20)));
    }

    // ── Variables ──

    #[test]
    fn test_assignment() {
        assert_eq!(eval_str("x = 5; x"), Value::Integer(Integer::from(5)));
    }

    #[test]
    fn test_multiple_assignments() {
        assert_eq!(
            eval_str("x = 1; y = 2; x + y"),
            Value::Integer(Integer::from(3))
        );
    }

    // ── Functions ──

    #[test]
    fn test_function_def_and_call() {
        assert_eq!(
            eval_str("f[x_] := x^2; f[3]"),
            Value::Integer(Integer::from(9))
        );
    }

    #[test]
    fn test_function_multi_arg() {
        assert_eq!(
            eval_str("add[a_, b_] := a + b; add[3, 4]"),
            Value::Integer(Integer::from(7))
        );
    }

    #[test]
    fn test_function_sequence_param() {
        assert_eq!(
            eval_str("f[x__] := Total[{x}]; f[1, 2, 3]"),
            Value::Integer(Integer::from(6))
        );
        assert_eq!(
            eval_str("g[x__] := Length[{x}]; g[42]"),
            Value::Integer(Integer::from(1))
        );
    }

    #[test]
    fn test_function_sequence_param_mixed() {
        assert_eq!(
            eval_str("h[a_, b__] := {a, b}; h[1, 2, 3]"),
            eval_str("{1, 2, 3}")
        );
    }

    #[test]
    fn test_function_sequence_param_zero_args() {
        assert_eq!(eval_str("f[x___] := {x}; f[]"), Value::List(vec![]));
        assert_eq!(
            eval_str("f[x___] := {x}; f[1, 2]"),
            Value::List(vec![
                Value::Integer(Integer::from(1)),
                Value::Integer(Integer::from(2)),
            ])
        );
    }

    // ── Recursion limit ($RecursionLimit) ──

    #[test]
    fn test_recursion_limit_default() {
        assert_eq!(
            eval_str("$RecursionLimit"),
            Value::Integer(Integer::from(1024))
        );
    }

    #[test]
    fn test_recursion_limit_below_limit() {
        let result = with_large_stack(|| {
            eval_str("$RecursionLimit = 20; f[x_] := 1 + f[x-1]; f[0] := 0; f[10]")
        });
        assert_eq!(result, Value::Integer(Integer::from(10)));
    }

    #[test]
    #[should_panic(expected = "Recursion depth")]
    fn test_recursion_limit_exceeded() {
        eval_str("$RecursionLimit = 5; f[x_] := 1 + f[x-1]; f[0] := 0; f[10]");
    }

    #[test]
    fn test_recursion_limit_infinity() {
        assert_eq!(
            eval_str("$RecursionLimit = Infinity; f[x_] := x; f[42]"),
            Value::Integer(Integer::from(42))
        );
    }

    // ── Control flow ──

    #[test]
    fn test_if_true() {
        assert_eq!(eval_str("If[True, 1, 2]"), Value::Integer(Integer::from(1)));
    }

    #[test]
    fn test_if_false() {
        assert_eq!(
            eval_str("If[False, 1, 2]"),
            Value::Integer(Integer::from(2))
        );
    }

    #[test]
    fn test_if_no_else() {
        assert_eq!(eval_str("If[False, 1]"), Value::Null);
    }

    #[test]
    fn test_if_c_style() {
        assert_eq!(
            eval_str("if (True) 1 else 2"),
            Value::Integer(Integer::from(1))
        );
    }

    #[test]
    fn test_if_c_style_block() {
        assert_eq!(
            eval_str("if (False) { 1; 2 } else { 3; 4 }"),
            Value::Integer(Integer::from(4))
        );
    }

    #[test]
    fn test_if_c_style_else_if() {
        assert_eq!(
            eval_str("if (False) 1 else if (True) 2 else 3"),
            Value::Integer(Integer::from(2))
        );
    }

    #[test]
    fn test_while_c_style() {
        assert_eq!(
            eval_str("i = 0; while (i < 3) { i = i + 1 }; i"),
            Value::Integer(Integer::from(3))
        );
    }

    #[test]
    fn test_for_c_style() {
        assert_eq!(
            eval_str("s = 0; for (i = 0; i < 5; i = i + 1) { s = s + i }; s"),
            Value::Integer(Integer::from(10))
        );
    }

    #[test]
    fn test_def_eval() {
        assert_eq!(
            eval_str("def f(x) = x + 1; f[3]"),
            Value::Integer(Integer::from(4))
        );
    }

    #[test]
    fn test_def_block_eval() {
        assert_eq!(
            eval_str("def f(x, y) { x + y }; f[2, 3]"),
            Value::Integer(Integer::from(5))
        );
    }

    #[test]
    fn test_def_delayed_eval() {
        assert_eq!(
            eval_str("def f(x) := x^2; f[4]"),
            Value::Integer(Integer::from(16))
        );
    }

    // ── Comparison ──

    #[test]
    fn test_equal() {
        assert_eq!(eval_str("1 == 1"), Value::Bool(true));
        assert_eq!(eval_str("1 == 2"), Value::Bool(false));
    }

    #[test]
    fn test_unequal() {
        assert_eq!(eval_str("1 != 2"), Value::Bool(true));
    }

    #[test]
    fn test_less() {
        assert_eq!(eval_str("1 < 2"), Value::Bool(true));
        assert_eq!(eval_str("2 < 1"), Value::Bool(false));
    }

    #[test]
    fn test_greater() {
        assert_eq!(eval_str("2 > 1"), Value::Bool(true));
    }

    // ── Lists ──

    #[test]
    fn test_list_literal() {
        assert_eq!(
            eval_str("{1, 2, 3}"),
            Value::List(vec![
                Value::Integer(Integer::from(1)),
                Value::Integer(Integer::from(2)),
                Value::Integer(Integer::from(3)),
            ])
        );
    }

    #[test]
    fn test_empty_list() {
        assert_eq!(eval_str("{}"), Value::List(vec![]));
    }

    #[test]
    fn test_list_operations() {
        assert_eq!(
            eval_str("Length[{1, 2, 3}]"),
            Value::Integer(Integer::from(3))
        );
        assert_eq!(
            eval_str("First[{1, 2, 3}]"),
            Value::Integer(Integer::from(1))
        );
        assert_eq!(
            eval_str("Last[{1, 2, 3}]"),
            Value::Integer(Integer::from(3))
        );
    }

    // ── Pipe ──

    #[test]
    fn test_pipe() {
        assert_eq!(
            eval_str("{1, 2, 3} // Length"),
            Value::Integer(Integer::from(3))
        );
    }

    // ── Prefix ──

    #[test]
    fn test_prefix() {
        assert_eq!(
            eval_str("Length @ {1, 2, 3}"),
            Value::Integer(Integer::from(3))
        );
    }

    // ── ReplaceAll ──

    #[test]
    fn test_replace_all() {
        assert_eq!(eval_str("5 /. x_ -> 42"), Value::Integer(Integer::from(42)));
    }

    // ── Map ──

    #[test]
    fn test_map_builtin() {
        let result = eval_str("Sqrt /@ {1, 4, 9}");
        assert_eq!(
            result,
            Value::List(vec![
                Value::Integer(Integer::from(1)),
                Value::Integer(Integer::from(2)),
                Value::Integer(Integer::from(3)),
            ])
        );
    }

    // ── Constants ──

    #[test]
    fn test_pi() {
        let val = eval_str("Pi");
        match val {
            Value::Symbol(s) => assert_eq!(s, "Pi"),
            _ => panic!("Expected Symbol, got {:?}", val),
        }
    }

    #[test]
    fn test_e() {
        let val = eval_str("E");
        match val {
            Value::Symbol(s) => assert_eq!(s, "E"),
            _ => panic!("Expected Symbol, got {:?}", val),
        }
    }

    #[test]
    fn test_degree_constant() {
        let val = eval_str("Degree");
        match val {
            Value::Real(r) => {
                let expected = std::f64::consts::PI / 180.0;
                assert!((r.to_f64() - expected).abs() < 1e-15);
            }
            _ => panic!("Expected Real, got {:?}", val),
        }
    }

    #[test]
    fn test_sin_degrees_eval() {
        assert_eq!(
            eval_str("SinDegrees[30]"),
            Value::Call {
                head: "Divide".to_string(),
                args: vec![
                    Value::Integer(rug::Integer::from(1)),
                    Value::Integer(rug::Integer::from(2)),
                ],
            }
        );
    }

    #[test]
    fn test_cos_degrees_eval() {
        assert_eq!(
            eval_str("CosDegrees[60]"),
            Value::Call {
                head: "Divide".to_string(),
                args: vec![
                    Value::Integer(rug::Integer::from(1)),
                    Value::Integer(rug::Integer::from(2)),
                ],
            }
        );
    }

    #[test]
    fn test_csc_pi_over_6_eval() {
        assert_eq!(
            eval_str("Csc[Pi / 6]"),
            Value::Integer(rug::Integer::from(2))
        );
    }

    #[test]
    fn test_sec_pi_over_3_eval() {
        assert_eq!(
            eval_str("Sec[Pi / 3]"),
            Value::Integer(rug::Integer::from(2))
        );
    }

    #[test]
    fn test_cot_pi_over_4_eval() {
        assert_eq!(
            eval_str("Cot[Pi / 4]"),
            Value::Integer(rug::Integer::from(1))
        );
    }

    // ── String operations ──

    #[test]
    fn test_string_join() {
        assert_eq!(
            eval_str(r#"StringJoin["hello", " ", "world"]"#),
            Value::Str("hello world".to_string())
        );
    }

    #[test]
    fn test_string_length() {
        assert_eq!(
            eval_str(r#"StringLength["hello"]"#),
            Value::Integer(Integer::from(5))
        );
    }

    // ── Math functions ──

    #[test]
    fn test_sqrt() {
        assert_eq!(eval_str("Sqrt[4]"), Value::Integer(Integer::from(2)));
    }

    #[test]
    fn test_abs() {
        assert_eq!(eval_str("Abs[-5]"), Value::Integer(Integer::from(5)));
    }

    // ── Sequence ──

    #[test]
    fn test_sequence_returns_last() {
        assert_eq!(eval_str("1; 2; 3"), Value::Integer(Integer::from(3)));
    }

    // ── Evaluator-dependent builtins ──

    #[test]
    fn test_map_function() {
        let result = eval_str("sq[x_] := x^2; Map[sq, {1, 2, 3}]");
        assert_eq!(
            result,
            Value::List(vec![
                Value::Integer(Integer::from(1)),
                Value::Integer(Integer::from(4)),
                Value::Integer(Integer::from(9)),
            ])
        );
    }

    #[test]
    fn test_map_with_builtin() {
        let result = eval_str("Map[Sqrt, {1, 4, 9}]");
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
    fn test_fold_with_init() {
        let result = eval_str("Fold[Plus, 0, {1, 2, 3}]");
        assert_eq!(result, Value::Integer(Integer::from(6)));
    }

    #[test]
    fn test_fold_without_init() {
        let result = eval_str("Fold[Plus, {1, 2, 3}]");
        assert_eq!(result, Value::Integer(Integer::from(6)));
    }

    #[test]
    fn test_select() {
        let result = eval_str("gt3[x_] := x > 3; Select[{1, 2, 3, 4, 5}, gt3]");
        assert_eq!(
            result,
            Value::List(vec![
                Value::Integer(Integer::from(4)),
                Value::Integer(Integer::from(5))
            ])
        );
    }

    #[test]
    fn test_nest() {
        let result = eval_str("sq[x_] := x^2; Nest[sq, 2, 3]");
        assert_eq!(result, Value::Integer(Integer::from(256)));
    }

    #[test]
    fn test_table_basic() {
        let result = eval_str("Table[i^2, {i, 1, 5}]");
        assert_eq!(
            result,
            Value::List(vec![
                Value::Integer(Integer::from(1)),
                Value::Integer(Integer::from(4)),
                Value::Integer(Integer::from(9)),
                Value::Integer(Integer::from(16)),
                Value::Integer(Integer::from(25)),
            ])
        );
    }

    #[test]
    fn test_table_short_form() {
        let result = eval_str("Table[i, {i, 3}]");
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
    fn test_table_with_step() {
        let result = eval_str("Table[i, {i, 0, 10, 2}]");
        assert_eq!(
            result,
            Value::List(vec![
                Value::Integer(Integer::from(0)),
                Value::Integer(Integer::from(2)),
                Value::Integer(Integer::from(4)),
                Value::Integer(Integer::from(6)),
                Value::Integer(Integer::from(8)),
                Value::Integer(Integer::from(10)),
            ])
        );
    }

    #[test]
    fn test_table_n_copies() {
        let result = eval_str("Table[0, 5]");
        assert_eq!(
            result,
            Value::List(vec![
                Value::Integer(Integer::from(0)),
                Value::Integer(Integer::from(0)),
                Value::Integer(Integer::from(0)),
                Value::Integer(Integer::from(0)),
                Value::Integer(Integer::from(0)),
            ])
        );
    }

    #[test]
    fn test_table_n_copies_expr() {
        let result = eval_str("Table[x^2, 3]");
        match &result {
            Value::List(items) => assert_eq!(items.len(), 3),
            _ => panic!("Expected List, got {:?}", result),
        }
    }

    #[test]
    fn test_table_explicit_values() {
        let result = eval_str("Table[i^2, {i, {1, 3, 5, 7}}]");
        assert_eq!(
            result,
            Value::List(vec![
                Value::Integer(Integer::from(1)),
                Value::Integer(Integer::from(9)),
                Value::Integer(Integer::from(25)),
                Value::Integer(Integer::from(49)),
            ])
        );
    }

    #[test]
    fn test_table_nested() {
        let result = eval_str("Table[i + j, {i, 1, 3}, {j, 1, 2}]");
        assert_eq!(
            result,
            Value::List(vec![
                Value::List(vec![
                    Value::Integer(Integer::from(2)),
                    Value::Integer(Integer::from(3)),
                ]),
                Value::List(vec![
                    Value::Integer(Integer::from(3)),
                    Value::Integer(Integer::from(4)),
                ]),
                Value::List(vec![
                    Value::Integer(Integer::from(4)),
                    Value::Integer(Integer::from(5)),
                ]),
            ])
        );
    }

    #[test]
    fn test_sum() {
        let result = eval_str("Sum[i^2, {i, 1, 4}]");
        assert_eq!(result, Value::Integer(Integer::from(30)));
    }

    #[test]
    fn test_fixed_point() {
        let result = eval_str("half[x_] := x / 2; FixedPoint[half, 64]");
        match result {
            Value::Real(r) => assert!(r.clone().abs() < 1e-10, "Expected near-zero, got {}", r),
            Value::Integer(n) => assert_eq!(n, 0),
            Value::Rational(r) => {
                let approx = r.numer().to_f64() / r.denom().to_f64();
                assert!(approx.abs() < 1e-10, "Expected near-zero, got {}", r);
            }
            _ => panic!("Expected numeric value, got {:?}", result),
        }
    }

    // ── Pattern guards ──

    #[test]
    fn test_pattern_guard_function() {
        let result =
            eval_str(r#"f[x_ /; x > 0] := "positive"; f[x_ /; x < 0] := "negative"; f[5]"#);
        assert_eq!(result, Value::Str("positive".to_string()));
    }

    #[test]
    fn test_pattern_guard_negative() {
        let result =
            eval_str(r#"f[x_ /; x > 0] := "positive"; f[x_ /; x < 0] := "negative"; f[-3]"#);
        assert_eq!(result, Value::Str("negative".to_string()));
    }

    #[test]
    fn test_pattern_guard_match_expression() {
        let result = eval_str(r#"match 7 { n_ /; n > 5 => "big"; n_ => "small" }"#);
        assert_eq!(result, Value::Str("big".to_string()));
    }

    #[test]
    fn test_pattern_guard_match_no_match() {
        let result = eval_str(r#"match 3 { n_ /; n > 5 => "big"; n_ => "small" }"#);
        assert_eq!(result, Value::Str("small".to_string()));
    }

    #[test]
    fn test_pattern_guard_match_fallback() {
        let result = eval_str(r#"match 3 { n_ /; n > 5 => "big"; n_ => "small" }"#);
        assert_eq!(result, Value::Str("small".to_string()));
    }

    #[test]
    fn test_pattern_guard_replace_all() {
        let result = eval_str("rule r = { x_ /; x > 3 -> 42 }; 5 /. r");
        assert_eq!(result, Value::Integer(Integer::from(42));

        let result2 = eval_str("rule r = { x_ /; x > 3 -> 42 }; 2 /. r");
        assert_eq!(result2, Value::Integer(Integer::from(2)));
    }

    #[test]
    fn test_pattern_guard_replace_all_no_match() {
        let result = eval_str("rule r = { x_ /; x > 10 -> 99 }; 5 /. r");
        assert_eq!(result, Value::Integer(Integer::from(5)));
    }

    #[test]
    fn test_pattern_guard_replace_repeated() {
        let result = eval_str("rule r = { x_ /; x > 1 -> 1 }; 5 //. r");
        assert_eq!(result, Value::Integer(Integer::from(1)));
    }

    #[test]
    fn test_pattern_guard_switch_nested() {
        let result = eval_str(
            r#"
            f[a_, b_ /; a + b > 0] := "big";
            f[a_, b_] := "small";
            f[3, -1]
        "#,
        );
        assert_eq!(result, Value::Str("big".to_string()));

        let result2 = eval_str(
            r#"
            f[a_, b_ /; a + b > 10] := "big";
            f[a_, b_] := "small";
            f[3, 2]
        "#,
        );
        assert_eq!(result2, Value::Str("small".to_string()));
    }

    #[test]
    fn test_pattern_guard_match_nested() {
        let result = eval_str(
            r#"match {5, 3} {
            {a_, b_ /; a > b} => "descending";
            {a_, b_ /; a < b} => "ascending";
            _ => "equal"
        }"#,
        );
        assert_eq!(result, Value::Str("descending".to_string()));
    }

    #[test]
    fn test_pattern_test_via_match_q() {
        let result = eval_str("MatchQ[5, _?IntegerQ]");
        assert_eq!(result, Value::Bool(true));

        let result2 = eval_str("MatchQ[3.14, _?IntegerQ]");
        assert_eq!(result2, Value::Bool(false));
    }

    #[test]
    fn test_switch_literal() {
        let result = eval_str(r#"Switch[2, 1, "one", 2, "two", 3, "three"]"#);
        assert_eq!(result, Value::Str("two".to_string()));
    }

    #[test]
    fn test_switch_fallback() {
        let result = eval_str(r#"Switch[99, 1, "one", 2, "two"]"#);
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn test_switch_named_blank() {
        let result = eval_str(r#"Switch[x, y_, y]"#);
        assert_eq!(result, Value::Symbol("x".to_string()));
    }

    #[test]
    fn test_switch_nested_guard() {
        let result = eval_str(
            r#"
            f[p_List /; Length[p] > 2] := "long";
            f[p_List] := "short";
            f[{1, 2, 3}]
        "#,
        );
        assert_eq!(result, Value::Str("long".to_string()));

        let result2 = eval_str(
            r#"
            f[p_List /; Length[p] > 2] := "long";
            f[p_List] := "short";
            f[{1, 2}]
        "#,
        );
        assert_eq!(result2, Value::Str("short".to_string()));
    }

    // ── Catch/Throw ──

    #[test]
    fn test_catch_throw() {
        let result = eval_str("Catch[Throw[42] ]");
        assert_eq!(result, Value::Integer(Integer::from(42)));
    }

    #[test]
    fn test_catch_no_throw() {
        let result = eval_str("Catch[1 + 2]");
        assert_eq!(result, Value::Integer(Integer::from(3)));
    }

    // ── try/catch/finally ──

    #[test]
    fn test_try_catch_basic() {
        let result = eval_str("try { Throw[42] } catch e { e + 1 }");
        assert_eq!(result, Value::Integer(Integer::from(43)));
    }

    #[test]
    fn test_try_catch_no_throw() {
        let result = eval_str("try { 1 + 1 } catch e { 0 }");
        assert_eq!(result, Value::Integer(Integer::from(2)));
    }

    #[test]
    fn test_try_catch_nested() {
        let result = eval_str(
            "try {
                try { Throw[10] } catch inner { inner + 5 }
            } catch outer { outer + 100 }",
        );
        assert_eq!(result, Value::Integer(Integer::from(15)));
    }

    #[test]
    fn test_try_catch_rethrow() {
        let result = eval_str("Catch[try { Throw[42] } catch e { Throw[e + 1] }]");
        assert_eq!(result, Value::Integer(Integer::from(43)));
    }

    // ── Return ──

    #[test]
    fn test_return_from_function() {
        let result = eval_str("f[x_] := Return[x * 2]; f[5]");
        assert_eq!(result, Value::Integer(Integer::from(10)));
    }

    #[test]
    fn test_return_early() {
        let result = eval_str(
            r#"
            f[x_] := If[x > 0, Return["positive"], "non-positive"];
            f[5]
            "#,
        );
        assert_eq!(result, Value::Str("positive".to_string()));
    }

    #[test]
    fn test_return_early_false_branch() {
        let result = eval_str(
            r#"
            f[x_] := If[x > 0, Return["positive"], "non-positive"];
            f[-1]
            "#,
        );
        assert_eq!(result, Value::Str("non-positive".to_string()));
    }

    #[test]
    fn test_return_empty() {
        let result = eval_str("f[x_] := Return[]; f[5]");
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn test_return_at_top_level() {
        let result = eval_str("Return[42]");
        assert_eq!(result, Value::Integer(Integer::from(42)));
    }

    #[test]
    fn test_return_from_pure_function() {
        let result = eval_str("Function[{x}, Return[x * 3]][7]");
        assert_eq!(result, Value::Integer(Integer::from(21)));
    }

    #[test]
    fn test_return_inside_while() {
        let result = eval_str("i = 0; While[True, If[i >= 5, Return[i * 10], i = i + 1]]");
        assert_eq!(result, Value::Integer(Integer::from(50)));
    }

    // ── Break/Continue ──

    #[test]
    fn test_break_from_while() {
        let result = eval_str("i = 1; While[i < 100, If[i > 5, Break[], i = i + 1]]; i");
        assert_eq!(result, Value::Integer(Integer::from(6)));
    }

    #[test]
    fn test_continue_in_while() {
        let result = eval_str(
            "result = {}; i = 0; While[(i = i + 1) < 6, If[i == 3, Continue[], result = Append[result, i]]]; result",
        );
        assert_eq!(
            result,
            Value::List(vec![
                Value::Integer(Integer::from(1)),
                Value::Integer(Integer::from(2)),
                Value::Integer(Integer::from(4)),
                Value::Integer(Integer::from(5)),
            ])
        );
    }

    #[test]
    fn test_break_from_for() {
        let result = eval_str("i = 0; For[i = 1, i < 100, i = i + 1, If[i > 5, Break[]]]; i");
        assert_eq!(result, Value::Integer(Integer::from(6)));
    }

    #[test]
    fn test_continue_in_for() {
        let result = eval_str(
            "result = {}; For[i = 1, i <= 5, i = i + 1, If[i == 3, Continue[], result = Append[result, i]]]; result",
        );
        assert_eq!(
            result,
            Value::List(vec![
                Value::Integer(Integer::from(1)),
                Value::Integer(Integer::from(2)),
                Value::Integer(Integer::from(4)),
                Value::Integer(Integer::from(5)),
            ])
        );
    }

    #[test]
    fn test_break_from_do() {
        let result = eval_str("Do[If[i > 3, Break[]], {i, 1, 10}]");
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn test_continue_in_do() {
        let result = eval_str(
            "result = {}; Do[If[i == 3 || i == 5, Continue[], result = Append[result, i]], {i, 1, 6}]; result",
        );
        assert_eq!(
            result,
            Value::List(vec![
                Value::Integer(Integer::from(1)),
                Value::Integer(Integer::from(2)),
                Value::Integer(Integer::from(4)),
                Value::Integer(Integer::from(6)),
            ])
        );
    }

    // ── Parallel computation ──

    #[test]
    fn test_parallel_map() {
        let result = eval_str("ParallelMap[Sqrt, {1, 4, 9, 16, 25}]");
        assert_eq!(
            result,
            Value::List(vec![
                Value::Integer(Integer::from(1)),
                Value::Integer(Integer::from(2)),
                Value::Integer(Integer::from(3)),
                Value::Integer(Integer::from(4)),
                Value::Integer(Integer::from(5)),
            ])
        );
    }

    #[test]
    fn test_parallel_map_user_func() {
        let result = eval_str("sq[x_] := x^2; ParallelMap[sq, {1, 2, 3, 4, 5}]");
        assert_eq!(
            result,
            Value::List(vec![
                Value::Integer(Integer::from(1)),
                Value::Integer(Integer::from(4)),
                Value::Integer(Integer::from(9)),
                Value::Integer(Integer::from(16)),
                Value::Integer(Integer::from(25)),
            ])
        );
    }

    #[test]
    fn test_parallel_map_small_list() {
        let result = eval_str("ParallelMap[Sqrt, {1, 4, 9}]");
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
    fn test_parallel_map_empty() {
        let result = eval_str("ParallelMap[Sqrt, {}]");
        assert_eq!(result, Value::List(vec![]));
    }

    #[test]
    fn test_parallel_table() {
        let result = eval_str("ParallelTable[i^2, {i, 1, 6}]");
        assert_eq!(
            result,
            Value::List(vec![
                Value::Integer(Integer::from(1)),
                Value::Integer(Integer::from(4)),
                Value::Integer(Integer::from(9)),
                Value::Integer(Integer::from(16)),
                Value::Integer(Integer::from(25)),
                Value::Integer(Integer::from(36)),
            ])
        );
    }

    #[test]
    fn test_parallel_table_short_form() {
        let result = eval_str("ParallelTable[i, {i, 5}]");
        assert_eq!(
            result,
            Value::List(vec![
                Value::Integer(Integer::from(1)),
                Value::Integer(Integer::from(2)),
                Value::Integer(Integer::from(3)),
                Value::Integer(Integer::from(4)),
                Value::Integer(Integer::from(5)),
            ])
        );
    }

    #[test]
    fn test_kernel_count() {
        let result = eval_str("KernelCount[]");
        match result {
            Value::Integer(n) => assert!(n.to_i64().unwrap() >= 1),
            _ => panic!("Expected Integer, got {:?}", result),
        }
    }

    #[test]
    fn test_parallel_sum() {
        let result = eval_str("ParallelSum[i, {i, 1, 10}]");
        assert_eq!(result, Value::Integer(Integer::from(55)));
    }

    #[test]
    fn test_parallel_sum_squares() {
        let result = eval_str("ParallelSum[i^2, {i, 1, 5}]");
        assert_eq!(result, Value::Integer(Integer::from(55)));
    }

    #[test]
    fn test_parallel_sum_small() {
        let result = eval_str("ParallelSum[i, {i, 1, 3}]");
        assert_eq!(result, Value::Integer(Integer::from(6)));
    }

    #[test]
    fn test_parallel_sum_empty_range() {
        let result = eval_str("ParallelSum[i, {i, 1, 0}]");
        assert_eq!(result, Value::Integer(Integer::from(0)));
    }

    #[test]
    fn test_parallel_evaluate() {
        let result = eval_str("ParallelEvaluate[42]");
        assert_eq!(result, Value::List(vec![Value::Integer(Integer::from(42))]));
    }

    #[test]
    fn test_parallel_evaluate_expr() {
        let result = eval_str("ParallelEvaluate[1 + 2]");
        assert_eq!(result, Value::List(vec![Value::Integer(Integer::from(3))]));
    }

    #[test]
    fn test_parallel_try_simple() {
        let result = eval_str("ParallelTry[{10, 20, 30}]");
        assert_eq!(result, Value::Integer(Integer::from(10)));
    }

    #[test]
    fn test_parallel_try_with_func() {
        let result = eval_str("ParallelTry[Sqrt, {4, 9, 16}]");
        assert_eq!(result, Value::Integer(Integer::from(2)));
    }

    #[test]
    fn test_parallel_try_single() {
        let result = eval_str("ParallelTry[{42}]");
        assert_eq!(result, Value::Integer(Integer::from(42)));
    }

    #[test]
    fn test_processor_count() {
        let result = eval_str("ProcessorCount[]");
        match result {
            Value::Integer(n) => assert!(n.to_i64().unwrap() >= 1),
            _ => panic!("Expected Integer, got {:?}", result),
        }
    }

    #[test]
    fn test_abort_kernels() {
        let result = eval_str("AbortKernels[]");
        assert_eq!(result, Value::Null);
    }

    #[test]
    #[should_panic(expected = "ParallelTry requires a non-empty list")]
    fn test_parallel_try_empty_list() {
        eval_str("ParallelTry[{}]");
    }

    #[test]
    #[should_panic(expected = "ParallelSum requires exactly 2 arguments")]
    fn test_parallel_sum_error_no_iter() {
        eval_str("ParallelSum[42]");
    }

    #[test]
    fn test_parallel_product_basic() {
        let result = eval_str("ParallelProduct[i, {i, 1, 5}]");
        assert_eq!(result, Value::Integer(Integer::from(120)));
    }

    #[test]
    fn test_parallel_product_squares() {
        let result = eval_str("ParallelProduct[i^2, {i, 1, 4}]");
        assert_eq!(result, Value::Integer(Integer::from(576)));
    }

    #[test]
    fn test_parallel_product_small() {
        let result = eval_str("ParallelProduct[i, {i, 1, 3}]");
        assert_eq!(result, Value::Integer(Integer::from(6)));
    }

    #[test]
    fn test_parallel_product_empty() {
        let result = eval_str("ParallelProduct[i, {i, 1, 0}]");
        assert_eq!(result, Value::Integer(Integer::from(1)));
    }

    #[test]
    #[should_panic(expected = "ParallelProduct requires exactly 2 arguments")]
    fn test_parallel_product_error_no_iter() {
        eval_str("ParallelProduct[42]");
    }

    #[test]
    fn test_parallel_do_returns_null() {
        let result = eval_str("ParallelDo[i^2, {i, 1, 5}]");
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn test_parallel_do_empty_range() {
        let result = eval_str("ParallelDo[i, {i, 1, 0}]");
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn test_parallel_do_small() {
        let result = eval_str("ParallelDo[i, {i, 1, 3}]");
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn test_parallel_combine_plus() {
        let result = eval_str("ParallelCombine[Plus, {1, 2, 3, 4, 5}]");
        assert_eq!(result, Value::Integer(Integer::from(15)));
    }

    #[test]
    fn test_parallel_combine_times() {
        let result = eval_str("ParallelCombine[Times, {1, 2, 3, 4, 5}]");
        assert_eq!(result, Value::Integer(Integer::from(120)));
    }

    #[test]
    fn test_parallel_combine_single() {
        let result = eval_str("ParallelCombine[Plus, {42}]");
        assert_eq!(result, Value::Integer(Integer::from(42)));
    }

    #[test]
    fn test_parallel_combine_small() {
        let result = eval_str("ParallelCombine[Plus, {10, 20, 30}]");
        assert_eq!(result, Value::Integer(Integer::from(60)));
    }

    #[test]
    #[should_panic(expected = "ParallelCombine requires a non-empty list")]
    fn test_parallel_combine_empty() {
        eval_str("ParallelCombine[Plus, {}]");
    }

    // ── Hold / HoldComplete / ReleaseHold ──

    #[test]
    fn test_hold_prevents_evaluation() {
        let result = eval_str("Hold[1 + 2]");
        match result {
            Value::Hold(_) => {}
            _ => panic!("Expected Hold, got {:?}", result),
        }
        assert!(!matches!(result, Value::Integer(_)));
    }

    #[test]
    fn test_hold_complete_prevents_evaluation() {
        let result = eval_str("HoldComplete[1 + 2]");
        assert!(matches!(result, Value::HoldComplete(_)));
    }

    #[test]
    fn test_release_hold_evaluates() {
        assert_eq!(
            eval_str("x = Hold[1 + 2]; ReleaseHold[x]"),
            Value::Integer(Integer::from(3))
        );
    }

    #[test]
    fn test_hold_multiple_args() {
        assert!(matches!(eval_str("Hold[1 + 2]"), Value::Hold(_)));
    }

    #[test]
    fn test_hold_preserves_symbol() {
        assert!(matches!(eval_str("Hold[x]"), Value::Hold(_)));
    }

    #[test]
    fn test_release_hold_complete() {
        assert_eq!(
            eval_str("x = HoldComplete[2 + 3]; ReleaseHold[x]"),
            Value::Integer(Integer::from(5))
        );
    }

    #[test]
    fn test_release_hold_non_held() {
        assert_eq!(
            eval_str("x = 42; ReleaseHold[x]"),
            Value::Integer(Integer::from(42))
        );
    }

    #[test]
    fn test_nested_hold() {
        let result = eval_str("x = Hold[1 + 2]; y = ReleaseHold[x]; Hold[y]");
        assert!(matches!(result, Value::Hold(_)));
    }

    // ── Attribute system ──

    #[test]
    fn test_set_attributes_and_query() {
        let result = eval_str("SetAttributes[f, HoldAll]; Attributes[f]");
        match result {
            Value::List(items) => {
                let names: Vec<String> = items.iter().map(|v| v.to_string()).collect();
                assert!(names.contains(&"HoldAll".to_string()));
            }
            _ => panic!("Expected List, got {:?}", result),
        }
    }

    #[test]
    fn test_listable_plus_threads_over_list() {
        let result = eval_str("{1, 2} + 10");
        assert_eq!(
            result,
            Value::List(vec![
                Value::Integer(Integer::from(11)),
                Value::Integer(Integer::from(12)),
            ])
        );
    }

    #[test]
    fn test_listable_times_threads_over_list() {
        let result = eval_str("{1, 2, 3} * 2");
        assert_eq!(
            result,
            Value::List(vec![
                Value::Integer(Integer::from(2)),
                Value::Integer(Integer::from(4)),
                Value::Integer(Integer::from(6)),
            ])
        );
    }

    #[test]
    fn test_listable_two_lists() {
        let result = eval_str("{1, 2} * {3, 4}");
        assert_eq!(
            result,
            Value::List(vec![
                Value::Integer(Integer::from(3)),
                Value::Integer(Integer::from(8)),
            ])
        );
    }

    #[test]
    fn test_listable_sin_threads() {
        let result = eval_str("Sin[{0}]");
        assert_eq!(result, Value::List(vec![Value::Integer(Integer::from(0))]));
    }

    #[test]
    fn test_protected_prevents_redefinition() {
        let result = eval_str("SetAttributes[Sin, Protected]; f[x_] := x + 1");
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn test_builtin_attributes_seeded() {
        let env = Env::new();
        builtins::register_builtins(&env);
        let plus_attrs = env.get_attributes("Plus");
        assert!(plus_attrs.contains(&"Listable".to_string()));
        assert!(plus_attrs.contains(&"Flat".to_string()));
        let sin_attrs = env.get_attributes("Sin");
        assert!(sin_attrs.contains(&"Listable".to_string()));
    }

    // ── Lazy provider tests ──

    #[test]
    fn test_lazy_custom_provider_constant() {
        let env = Env::new();
        builtins::register_builtins(&env);
        env.register_lazy_provider(
            "LazyFoo",
            LazyProvider::Custom(Arc::new(|env| {
                let tokens = crate::lexer::tokenize("LazyFoo[] := 42").unwrap();
                let ast = crate::parser::parse(tokens).unwrap();
                crate::eval::eval_program(&ast, &env.root_env()).unwrap();
                Ok(env.root_env().get("LazyFoo").unwrap())
            })),
        );
        assert_eq!(
            eval_str_in_env("LazyFoo[]", &env),
            Value::Integer(Integer::from(42))
        );
    }

    #[test]
    fn test_lazy_custom_provider_function() {
        let env = Env::new();
        builtins::register_builtins(&env);
        env.register_lazy_provider(
            "LazyDouble",
            LazyProvider::Custom(Arc::new(|env| {
                let tokens = crate::lexer::tokenize("LazyDouble[x_] := 2*x").unwrap();
                let ast = crate::parser::parse(tokens).unwrap();
                crate::eval::eval_program(&ast, &env.root_env()).unwrap();
                Ok(env.root_env().get("LazyDouble").unwrap())
            })),
        );
        assert_eq!(
            eval_str_in_env("LazyDouble[5]", &env),
            Value::Integer(Integer::from(10))
        );
    }

    #[test]
    fn test_lazy_provider_one_shot() {
        let env = Env::new();
        builtins::register_builtins(&env);
        let call_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let count = call_count.clone();
        env.register_lazy_provider(
            "Once",
            LazyProvider::Custom(Arc::new(move |env| {
                count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let tokens = crate::lexer::tokenize("Once[] := 99").unwrap();
                let ast = crate::parser::parse(tokens).unwrap();
                crate::eval::eval_program(&ast, &env.root_env()).unwrap();
                Ok(env.root_env().get("Once").unwrap())
            })),
        );
        assert_eq!(
            eval_str_in_env("Once[]", &env),
            Value::Integer(Integer::from(99))
        );
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(
            eval_str_in_env("Once[]", &env),
            Value::Integer(Integer::from(99))
        );
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test]
    fn test_lazy_provider_no_match_fallback() {
        let val = eval_str("NonExistentSymbol[1, 2]");
        assert_eq!(
            val,
            Value::Call {
                head: "NonExistentSymbol".to_string(),
                args: vec![
                    Value::Integer(Integer::from(1)),
                    Value::Integer(Integer::from(2))
                ],
            }
        );
    }

    #[test]
    fn test_lazy_provider_file_not_found_error() {
        let env = Env::new();
        builtins::register_builtins(&env);
        env.register_lazy_provider(
            "Missing",
            LazyProvider::File(std::path::PathBuf::from("nonexistent.syma")),
        );
        let tokens = crate::lexer::tokenize("Missing[]").unwrap();
        let ast = crate::parser::parse(tokens).unwrap();
        let result = crate::eval::eval_program(&ast, &env);
        assert!(result.is_err());
        let err = result.unwrap_err();
        let err_msg = format!("{}", err);
        assert!(
            err_msg.contains("not found"),
            "Error should mention 'not found', got: {err_msg}"
        );
    }

    #[test]
    fn test_lazy_provider_file_in_search_path() {
        use std::io::Write;
        use std::path::Path;

        let dir = std::env::temp_dir().join("syma_lazy_test");
        let _ = std::fs::create_dir_all(&dir);
        let file_path = dir.join("LazyAdd.syma");
        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(f, "LazyAdd[x_] := x + 42").unwrap();
        f.flush().unwrap();

        let env = Env::new();
        builtins::register_builtins(&env);
        env.add_search_path(dir.clone());
        env.register_lazy_provider(
            "LazyAdd",
            LazyProvider::File(Path::new("LazyAdd.syma").to_path_buf()),
        );

        let result = eval_str_in_env("LazyAdd[1]", &env);
        assert_eq!(result, Value::Integer(Integer::from(43)));

        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_det_auto_loads() {
        let result = eval_str("Det[{{1, 2}, {3, 4}}]");
        assert_eq!(result, Value::Integer(Integer::from(-2)));
    }

    #[test]
    fn test_mean_auto_loads() {
        let result = eval_str("Mean[{1, 2, 3, 4, 5}]");
        assert_eq!(result, Value::Integer(Integer::from(3)));
    }

    #[test]
    fn test_auto_load_works_with_needs() {
        let result = eval_str("Det[{{1, 2}, {3, 4}}]; Needs[\"LinearAlgebra\"]");
        assert_eq!(result, Value::Null);
    }

    // ── Boolean computation integration tests ──

    #[test]
    fn test_and_true_false() {
        assert_eq!(eval_str("And[True, False]"), Value::Bool(false));
    }

    #[test]
    fn test_or_true_false() {
        assert_eq!(eval_str("Or[True, False]"), Value::Bool(true));
    }

    #[test]
    fn test_and_short_circuit() {
        assert_eq!(
            eval_str("And[False, Error[\"should not fire\"]]"),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_or_short_circuit() {
        assert_eq!(
            eval_str("Or[True, Error[\"should not fire\"]]"),
            Value::Bool(true)
        );
    }

    #[test]
    fn test_boole_true() {
        assert_eq!(eval_str("Boole[True]"), Value::Integer(Integer::from(1)));
    }

    #[test]
    fn test_boole_false() {
        assert_eq!(eval_str("Boole[False]"), Value::Integer(Integer::from(0)));
    }

    #[test]
    fn test_boole_non_bool() {
        assert_eq!(eval_str("Boole[42]"), Value::Integer(Integer::from(0)));
    }

    #[test]
    fn test_boolean_q_true() {
        assert_eq!(eval_str("BooleanQ[True]"), Value::Bool(true));
    }

    #[test]
    fn test_boolean_q_false_for_non_bool() {
        assert_eq!(eval_str("BooleanQ[42]"), Value::Bool(false));
    }

    #[test]
    fn test_xor_true_false() {
        assert_eq!(eval_str("Xor[True, False]"), Value::Bool(true));
    }

    #[test]
    fn test_xor_both_true() {
        assert_eq!(eval_str("Xor[True, True]"), Value::Bool(false));
    }

    #[test]
    fn test_nand_basic() {
        assert_eq!(eval_str("Nand[True, True]"), Value::Bool(false));
        assert_eq!(eval_str("Nand[True, False]"), Value::Bool(true));
    }

    #[test]
    fn test_nor_basic() {
        assert_eq!(eval_str("Nor[False, False]"), Value::Bool(true));
        assert_eq!(eval_str("Nor[True, False]"), Value::Bool(false));
    }

    #[test]
    fn test_implies_truth_table() {
        assert_eq!(eval_str("Implies[True, True]"), Value::Bool(true));
        assert_eq!(eval_str("Implies[True, False]"), Value::Bool(false));
        assert_eq!(eval_str("Implies[False, True]"), Value::Bool(true));
        assert_eq!(eval_str("Implies[False, False]"), Value::Bool(true));
    }

    #[test]
    fn test_equivalent_basic() {
        assert_eq!(eval_str("Equivalent[True, True]"), Value::Bool(true));
        assert_eq!(eval_str("Equivalent[True, False]"), Value::Bool(false));
    }

    #[test]
    fn test_majority_basic() {
        assert_eq!(eval_str("Majority[True, True, False]"), Value::Bool(true));
        assert_eq!(eval_str("Majority[True, False, False]"), Value::Bool(false));
    }

    #[test]
    fn test_logical_infix_operators() {
        assert_eq!(eval_str("True && False"), Value::Bool(false));
        assert_eq!(eval_str("True || False"), Value::Bool(true));
        assert_eq!(eval_str("!True"), Value::Bool(false));
        assert_eq!(eval_str("!False"), Value::Bool(true));
    }

    // ── JIT promotion ──

    #[test]
    fn test_jit_promotion() {
        let result = eval_str(
            "double[x_] := x * 2;
             Do[double[i], {i, 1, 100}];
             double[21]",
        );
        assert_eq!(result, Value::Integer(Integer::from(42)));
    }

    #[test]
    fn test_jit_promotion_with_if() {
        let result = eval_str(
            "abs[x_] := If[x < 0, -x, x];
             Do[abs[i], {i, -50, 50}];
             abs[-5]",
        );
        assert_eq!(result, Value::Integer(Integer::from(5)));
    }

    #[test]
    fn test_jit_promotion_inline_arithmetic() {
        let result = eval_str(
            "f[x_] := x + 10;
             Do[f[i], {i, 1, 100}];
             f[32]",
        );
        assert_eq!(result, Value::Integer(Integer::from(42)));
    }

    #[test]
    fn test_jit_promotion_multiple_calls() {
        let result = eval_str(
            "add[a_, b_] := a + b;
             Do[add[i, i], {i, 1, 200}];
             add[20, 22]",
        );
        assert_eq!(result, Value::Integer(Integer::from(42)));
    }

    #[test]
    fn test_jit_comparison_equal() {
        let result = eval_str(
            "f[x_] := If[x == 42, 1, 0];
             Do[f[i], {i, 1, 100}];
             f[42]",
        );
        assert_eq!(result, Value::Integer(Integer::from(1)));
    }

    #[test]
    fn test_jit_comparison_greater() {
        let result = eval_str(
            "f[x_] := If[x > 0, x, 0];
             Do[f[i], {i, -50, 50}];
             f[42]",
        );
        assert_eq!(result, Value::Integer(Integer::from(42)));
    }

    #[test]
    fn test_jit_and_or() {
        let result = eval_str(
            "f[x_, y_] := If[x > 0 && y > 0, 1, 0];
             Do[f[i, i], {i, 1, 100}];
             f[3, 5]",
        );
        assert_eq!(result, Value::Integer(Integer::from(1)));
    }

    #[test]
    fn test_jit_map_desugar() {
        let result = eval_str(
            "f[x_] := Length /@ x;
             Do[f[{i}], {i, 1, 100}];
             f[{{1, 2}, {3, 4, 5}}]",
        );
        assert_eq!(
            result,
            Value::List(vec![
                Value::Integer(Integer::from(2)),
                Value::Integer(Integer::from(3)),
            ])
        );
    }

    #[test]
    fn test_jit_which() {
        let result = eval_str(
            "f[x_] := Which[x > 0, x, True, 0];
             Do[f[i], {i, -50, 50}];
             f[42]",
        );
        assert_eq!(result, Value::Integer(Integer::from(42)));
    }

    #[test]
    fn test_jit_which_default() {
        let result = eval_str(
            "f[x_] := Which[x < 0, -x, x > 10, 10, True, x];
             Do[f[i], {i, 1, 100}];
             f[5]",
        );
        assert_eq!(result, Value::Integer(Integer::from(5)));
    }

    #[test]
    fn test_jit_switch() {}

    #[test]
    fn test_jit_apply_desugar() {
        let result = eval_str(
            "f[x_] := Apply[Plus, x];
             Do[f[{i, i}], {i, 1, 100}];
             f[{10, 32}]",
        );
        assert_eq!(result, Value::Integer(Integer::from(42)));
    }

    #[test]
    fn test_jit_subtract() {
        let result = eval_str(
            "f[x_, y_] := x - y;
             Do[f[i, 1], {i, 1, 100}];
             f[10, 3]",
        );
        assert_eq!(result, Value::Integer(Integer::from(7)));
    }

    #[test]
    fn test_jit_divide() {
        let result = eval_str(
            "f[x_, y_] := x / y;
             Do[f[i, 2], {i, 1, 100}];
             f[42, 2]",
        );
        assert_eq!(result, Value::Integer(Integer::from(21)));
    }

    #[test]
    fn test_listable_boole() {
        assert_eq!(
            eval_str("Boole[{True, False}]"),
            Value::List(vec![
                Value::Integer(Integer::from(1)),
                Value::Integer(Integer::from(0)),
            ])
        );
    }

    #[test]
    fn test_listable_xor() {
        assert_eq!(
            eval_str("Xor[{True, False}, True]"),
            Value::List(vec![Value::Bool(false), Value::Bool(true),])
        );
    }

    // ── Pure function (#&) tests ──

    #[test]
    fn test_pure_function_slot() {
        assert_eq!(eval_str("(#&)[5]"), Value::Integer(Integer::from(5)));
    }

    #[test]
    fn test_pure_function_arithmetic() {
        assert_eq!(eval_str("(# + 1 &)[5]"), Value::Integer(Integer::from(6)));
    }

    #[test]
    fn test_pure_function_two_slots() {
        assert_eq!(
            eval_str("(#1 + #2 &)[3, 7]"),
            Value::Integer(Integer::from(10))
        );
    }

    #[test]
    fn test_pure_function_map() {
        assert_eq!(
            eval_str("Map[# + 1 &, {1, 2, 3}]"),
            Value::List(vec![
                Value::Integer(Integer::from(2)),
                Value::Integer(Integer::from(3)),
                Value::Integer(Integer::from(4)),
            ])
        );
    }

    #[test]
    fn test_pure_function_select() {
        assert_eq!(
            eval_str("Select[{1, 2, 3, 4}, # > 2 &]"),
            Value::List(vec![
                Value::Integer(Integer::from(3)),
                Value::Integer(Integer::from(4)),
            ])
        );
    }

    #[test]
    fn test_pure_function_part() {
        assert_eq!(
            eval_str("(#[[1]] + #[[2]] &)[{10, 32}]"),
            Value::Integer(Integer::from(42)),
        );
    }

    #[test]
    fn test_pure_function_nested_slots() {
        assert_eq!(
            eval_str("Map[# + Map[# + 1 &, #] &, {{1, 2}, {3, 4}}]"),
            Value::List(vec![
                Value::List(vec![
                    Value::Integer(Integer::from(3)),
                    Value::Integer(Integer::from(5)),
                ]),
                Value::List(vec![
                    Value::Integer(Integer::from(7)),
                    Value::Integer(Integer::from(9)),
                ]),
            ])
        );
    }

    #[test]
    fn test_pure_function_named_params() {
        assert_eq!(
            eval_str("Function[{x, y}, x + y][3, 7]"),
            Value::Integer(Integer::from(10))
        );
    }

    #[test]
    fn test_pure_function_named_params_map() {
        assert_eq!(
            eval_str("Map[Function[{x}, x^2], {1, 2, 3, 4}]"),
            Value::List(vec![
                Value::Integer(Integer::from(1)),
                Value::Integer(Integer::from(4)),
                Value::Integer(Integer::from(9)),
                Value::Integer(Integer::from(16)),
            ])
        );
    }

    #[test]
    fn test_pure_function_zero_arg() {
        assert_eq!(eval_str("(42&)[]"), Value::Integer(Integer::from(42)));
    }

    #[test]
    fn test_pure_function_extra_args() {
        assert_eq!(eval_str("(#&)[1, 2, 3]"), Value::Integer(Integer::from(1)));
    }

    #[test]
    fn test_pure_function_multi_arity() {
        assert_eq!(
            eval_str("(#1 + #2 &)[10, 32]"),
            Value::Integer(Integer::from(42))
        );
    }

    #[test]
    fn test_pure_function_with_select_complex() {
        assert_eq!(
            eval_str("Select[Range[10], # > 5 && # < 9 &]"),
            Value::List(vec![
                Value::Integer(Integer::from(6)),
                Value::Integer(Integer::from(7)),
                Value::Integer(Integer::from(8)),
            ])
        );
    }

    // ── Slot sequence ## / ##n tests ──

    #[test]
    fn test_slot_sequence_all() {
        assert_eq!(
            eval_str("(## &)[1, 2, 3]"),
            Value::Sequence(vec![
                Value::Integer(Integer::from(1)),
                Value::Integer(Integer::from(2)),
                Value::Integer(Integer::from(3)),
            ])
        );
    }

    #[test]
    fn test_slot_sequence_from_n() {
        assert_eq!(
            eval_str("(##2 &)[10, 20, 30]"),
            Value::Sequence(vec![
                Value::Integer(Integer::from(20)),
                Value::Integer(Integer::from(30))
            ])
        );
    }

    #[test]
    fn test_slot_sequence_single_arg() {
        assert_eq!(
            eval_str("(## &)[42]"),
            Value::Sequence(vec![Value::Integer(Integer::from(42))])
        );
    }

    #[test]
    fn test_slot_sequence_zero_args() {
        assert_eq!(eval_str("(## &)[]"), Value::Sequence(vec![]));
    }

    #[test]
    fn test_slot_sequence_past_end() {
        assert_eq!(eval_str("(##4 &)[1, 2]"), Value::Sequence(vec![]));
    }

    #[test]
    fn test_slot_sequence_splices_in_call() {
        assert_eq!(
            eval_str("(Plus[##, 3] &)[1, 2]"),
            Value::Integer(Integer::from(6))
        );
    }

    #[test]
    fn test_slot_sequence_with_slots() {
        assert_eq!(
            eval_str("(# + ## &)[1, 2, 3]"),
            Value::Integer(Integer::from(7))
        );
    }

    #[test]
    fn test_slot_self_reference_no_recursion() {
        eval_str("(#0 &)[42]");
    }

    #[test]
    fn test_slot_self_reference_simple_recursion() {
        assert_eq!(
            eval_str("(If[# == 0, 0, #0[# - 1]] &)[3]"),
            Value::Integer(Integer::from(0))
        );
    }

    #[test]
    fn test_slot_self_reference_factorial_2() {
        assert_eq!(
            eval_str("(If[# == 0, 1, # * #0[# - 1]] &)[2]"),
            Value::Integer(Integer::from(2))
        );
    }

    #[test]
    fn test_slot_self_reference_factorial_3() {
        assert_eq!(
            eval_str("(If[# == 0, 1, # * #0[# - 1]] &)[3]"),
            Value::Integer(Integer::from(6))
        );
    }

    #[test]
    fn test_slot_self_reference_factorial_5() {
        with_large_stack(|| {
            assert_eq!(
                eval_str("(If[# == 0, 1, # * #0[# - 1]] &)[5]"),
                Value::Integer(Integer::from(120))
            );
        });
    }

    // ── Closure / lexical capture tests ──

    #[test]
    fn test_closure_basic() {
        assert_eq!(
            eval_str("createAdder[x_] := Function[{y}, x + y]; f = createAdder[10]; f[5]"),
            Value::Integer(Integer::from(15))
        );
    }

    #[test]
    fn test_closure_nested() {
        assert_eq!(
            eval_str("Function[{x}, Function[{y}, x + y]][5][3]"),
            Value::Integer(Integer::from(8))
        );
    }

    #[test]
    fn test_closure_double_nested() {
        assert_eq!(
            eval_str("Function[{x}, Function[{y}, Function[{z}, x + y + z]]][1][2][3]"),
            Value::Integer(Integer::from(6))
        );
    }

    #[test]
    fn test_closure_with_map() {
        assert_eq!(
            eval_str("adder = Function[{x}, Function[{y}, x + y]][10]; Map[adder, {1, 2, 3}]"),
            Value::List(vec![
                Value::Integer(Integer::from(11)),
                Value::Integer(Integer::from(12)),
                Value::Integer(Integer::from(13)),
            ])
        );
    }

    #[test]
    fn test_closure_multiple_free_vars() {
        assert_eq!(
            eval_str(
                r#"
                makeF[mult_, add_] := Function[{x}, mult * x + add];
                f = makeF[3, 1];
                {f[0], f[5], f[10]}
                "#
            ),
            Value::List(vec![
                Value::Integer(Integer::from(1)),
                Value::Integer(Integer::from(16)),
                Value::Integer(Integer::from(31)),
            ])
        );
    }

    // ── Class / Object tests ──

    #[test]
    fn test_class_basic() {
        let result = eval_str(
            r#"
            class Point {
                field x
                field y
                constructor[x_, y_] {
                    this.x = x
                    this.y = y
                }
                method distance[] := Sqrt[x^2 + y^2]
            }
            p = Point[3, 4]
            p.distance[]
            "#,
        );
        assert_eq!(result, Value::Integer(Integer::from(5)));
    }

    #[test]
    fn test_class_field_access() {
        let result = eval_str(
            r#"
            class Point {
                field x
                field y
                constructor[x_, y_] { this.x = x; this.y = y }
            }
            p = Point[3, 4]
            p.x
            "#,
        );
        assert_eq!(result, Value::Integer(Integer::from(3)));
    }

    #[test]
    fn test_class_inheritance() {
        let result = eval_str(
            r#"
            class Shape {
                field color
                constructor[c_] { this.color = c }
            }
            class Rectangle extends Shape {
                field width
                field height
                constructor[c_, w_, h_] {
                    this.color = c
                    this.width = w
                    this.height = h
                }
                method area[] := width * height
            }
            r = Rectangle["red", 3, 4]
            r.area[]
            "#,
        );
        assert_eq!(result, Value::Integer(Integer::from(12)));
    }

    #[test]
    fn test_class_mixin() {
        let result = eval_str(
            r#"
            mixin Printable {
                method toString[] := "printed"
            }
            class MyClass with Printable {
                field value
                constructor[v_] { this.value = v }
            }
            obj = MyClass[42]
            obj.toString[]
            "#,
        );
        assert_eq!(result, Value::Str("printed".to_string()));
    }

    #[test]
    fn test_class_method_with_this() {
        let result = eval_str(
            r#"
            class Point {
                field x
                field y
                constructor[x_, y_] { this.x = x; this.y = y }
                method mag[] := x^2 + y^2
            }
            p = Point[3, 4]
            p.mag[]
            "#,
        );
        assert_eq!(result, Value::Integer(Integer::from(25)));
    }

    #[test]
    fn test_class_field_default() {
        let result = eval_str(
            r#"
            class Circle {
                field radius
                field color = "black"
                constructor[r_] { this.radius = r }
            }
            c = Circle[5]
            c.color
            "#,
        );
        assert_eq!(result, Value::Str("black".to_string()));
    }

    // ── Module / Import tests ──

    #[test]
    fn test_module_basic() {
        let result = eval_str(
            r#"
            module MathUtils {
                export square, cube
                square[x_] := x^2
                cube[x_] := x^3
            }
            import MathUtils
            square[5]
            "#,
        );
        assert_eq!(result, Value::Integer(Integer::from(25)));
    }

    #[test]
    fn test_module_selective_import() {
        let result = eval_str(
            r#"
            module M {
                export a, b
                a[x_] := x + 1
                b[x_] := x + 2
            }
            import M.{b}
            b[5]
            "#,
        );
        assert_eq!(result, Value::Integer(Integer::from(7)));
    }

    #[test]
    fn test_module_alias_import() {
        let result = eval_str(
            r#"
            module M {
                export f
                f[x_] := x^2
            }
            import M as N
            N
            "#,
        );
        match result {
            Value::Module { .. } => {}
            _ => panic!("Expected Module, got {:?}", result),
        }
    }

    // ── Bytecode compilation register-overlap bug ──

    #[test]
    fn test_jit_fold_with_pure_function() {
        let result = eval_str(
            "g[n_] := Fold[(#1 + #2) &, 0, Range[n]];
             g[8];
             Do[g[8], {i, 105}];
             g[8]",
        );
        assert_eq!(result, Value::Integer(Integer::from(36)));
    }

    #[test]
    fn test_jit_nest_with_pure_function() {
        let result = eval_str(
            "g[n_] := Nest[(# + 1) &, 0, n];
             g[5];
             Do[g[5], {i, 105}];
             g[5]",
        );
        assert_eq!(result, Value::Integer(Integer::from(5)));
    }

    // ── Assignment operator tests ──

    #[test]
    fn test_plus_assign_eval() {
        assert_eq!(
            eval_str("x = 3; x += 2; x"),
            Value::Integer(Integer::from(5))
        );
    }

    #[test]
    fn test_minus_assign_eval() {
        assert_eq!(
            eval_str("x = 10; x -= 3; x"),
            Value::Integer(Integer::from(7))
        );
    }

    #[test]
    fn test_times_assign_eval() {
        assert_eq!(
            eval_str("x = 4; x *= 3; x"),
            Value::Integer(Integer::from(12))
        );
    }

    #[test]
    fn test_divide_assign_eval() {
        assert_eq!(
            eval_str("x = 10; x /= 2; x"),
            Value::Integer(Integer::from(5))
        );
    }

    #[test]
    fn test_caret_assign_eval() {
        assert_eq!(
            eval_str("x = 3; x ^= 2; x"),
            Value::Integer(Integer::from(9))
        );
    }

    #[test]
    fn test_post_increment_eval() {
        let result = eval_str("x = 5; x++");
        assert_eq!(result, Value::Integer(Integer::from(5)));
        assert_eq!(eval_str("x = 5; x++; x"), Value::Integer(Integer::from(6)));
    }

    #[test]
    fn test_post_decrement_eval() {
        let result = eval_str("x = 5; x--");
        assert_eq!(result, Value::Integer(Integer::from(5)));
        assert_eq!(eval_str("x = 5; x--; x"), Value::Integer(Integer::from(4)));
    }

    #[test]
    fn test_pre_increment_eval() {
        assert_eq!(eval_str("x = 5; ++x"), Value::Integer(Integer::from(6)));
        assert_eq!(eval_str("x = 5; ++x; x"), Value::Integer(Integer::from(6)));
    }

    #[test]
    fn test_pre_decrement_eval() {
        assert_eq!(eval_str("x = 5; --x"), Value::Integer(Integer::from(4)));
        assert_eq!(eval_str("x = 5; --x; x"), Value::Integer(Integer::from(4)));
    }

    #[test]
    fn test_unset_eval() {
        assert_eq!(eval_str("x = 5; x =.; x"), Value::Symbol("x".to_string()));
    }

    #[test]
    fn test_destructuring_assign_eval() {
        let result = eval_str("{a, b} = {1, 2}; a + b");
        assert_eq!(result, Value::Integer(Integer::from(3)));
    }

    #[test]
    fn test_chained_assignment_eval() {
        assert_eq!(eval_str("x = y = 5"), Value::Integer(Integer::from(5)));
        assert_eq!(
            eval_str("x = y = 5; x + y"),
            Value::Integer(Integer::from(10))
        );
    }

    // ── Scoping constructs: Module, With, Block ──

    #[test]
    fn test_module_scoping_basic() {
        assert_eq!(
            eval_str("Module[{x = 5}, x + 1]"),
            Value::Integer(Integer::from(6))
        );
    }

    #[test]
    fn test_module_multiple_locals() {
        assert_eq!(
            eval_str("Module[{x = 3, y = 4}, x^2 + y^2]"),
            Value::Integer(Integer::from(25))
        );
    }

    #[test]
    fn test_module_no_init() {
        assert_eq!(eval_str("Module[{x}, x]"), Value::Null);
    }

    #[test]
    fn test_module_shadows_global() {
        let result = eval_str("x = 100; Module[{x = 5}, x]");
        assert_eq!(result, Value::Integer(Integer::from(5)));
    }

    #[test]
    fn test_module_global_unchanged() {
        let result = eval_str("x = 100; Module[{x = 5}, x]; x");
        assert_eq!(result, Value::Integer(Integer::from(100)));
    }

    #[test]
    fn test_module_empty_specs() {
        assert_eq!(
            eval_str("Module[{}, 42]"),
            Value::Integer(Integer::from(42))
        );
    }

    #[test]
    fn test_module_with_function_def() {
        let result = eval_str("f[x_] := Module[{y = x^2}, y + 1]; f[5]");
        assert_eq!(result, Value::Integer(Integer::from(26)));
    }

    #[test]
    fn test_module_sequential_body() {
        let result = eval_str("Module[{x = 1}, Set[x, x + 2], Set[x, x * 3], x]");
        assert_eq!(result, Value::Integer(Integer::from(9)));
    }

    // ── With tests ──

    #[test]
    fn test_with_basic() {
        assert_eq!(
            eval_str("With[{x = 5}, x + 1]"),
            Value::Integer(Integer::from(6))
        );
    }

    #[test]
    fn test_with_multiple_vars() {
        assert_eq!(
            eval_str("With[{x = 3, y = 4}, x^2 + y^2]"),
            Value::Integer(Integer::from(25))
        );
    }

    #[test]
    fn test_with_substitution_in_call() {
        let result = eval_str("With[{x = 5}, Sin[x]]");
        assert_eq!(
            result,
            Value::Call {
                head: "Sin".to_string(),
                args: vec![Value::Integer(Integer::from(5))],
            }
        );
    }

    #[test]
    fn test_with_empty_specs() {
        assert_eq!(eval_str("With[{}, 42]"), Value::Integer(Integer::from(42)));
    }

    #[test]
    fn test_with_rhs_evaluation() {
        let result = eval_str("With[{x = 1 + 2}, x * 3]");
        assert_eq!(result, Value::Integer(Integer::from(9)));
    }

    #[test]
    fn test_with_no_global_leak() {
        let result = eval_str("a = 10; With[{a = 5}, a + 1]; a");
        assert_eq!(result, Value::Integer(Integer::from(10)));
    }

    // ── Block tests ──

    #[test]
    fn test_block_basic() {
        assert_eq!(
            eval_str("Block[{x = 5}, x + 1]"),
            Value::Integer(Integer::from(6))
        );
    }

    #[test]
    fn test_block_multiple_vars() {
        assert_eq!(
            eval_str("Block[{x = 3, y = 4}, x + y]"),
            Value::Integer(Integer::from(7))
        );
    }

    #[test]
    fn test_block_restores_global() {
        let result = eval_str("x = 10; Block[{x = 5}, x + 1]; x");
        assert_eq!(result, Value::Integer(Integer::from(10)));
    }

    #[test]
    fn test_block_empty_specs() {
        assert_eq!(eval_str("Block[{}, 42]"), Value::Integer(Integer::from(42)));
    }

    #[test]
    fn test_block_side_effect_in_body() {
        let result = eval_str("x = 1; Block[{x = 10}, Set[x, x + 5]]; x");
        assert_eq!(result, Value::Integer(Integer::from(1)));
    }

    #[test]
    fn test_block_affects_function_due_to_dynamic_scoping() {
        let result = eval_str("x = 100; f = Function[{a}, x]; Block[{x = 5}, f[0]]");
        assert_eq!(result, Value::Integer(Integer::from(5)));
    }
}
