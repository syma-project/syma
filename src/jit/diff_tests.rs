//! Differential tests for the native JIT tier.
//!
//! For a battery of function definitions we define the function in a fresh
//! environment, call it once (tree-walk) to obtain the expected result, then
//! call it enough times to drive it through bytecode promotion and on to
//! native JIT compilation, asserting the results are identical. Error results
//! are compared by their rendered message so that error paths are covered too.
//!
//! `diff_all` additionally exercises the pure bytecode VM tier (before the JIT
//! threshold) so a divergence is attributed to the right tier, and so that
//! functions the JIT deliberately rejects (`Break`/`Continue` escaping to a
//! tree-walk loop) still run correctly on the fallback path.

#![cfg(test)]

use crate::ast::Expr;
use crate::builtins;
use crate::env::Env;
use crate::eval;
use crate::lexer;
use crate::parser;
use crate::value::{EvalError, Value};

fn parse_program(src: &str) -> Vec<Expr> {
    let tokens = lexer::tokenize(src).expect("lex");
    parser::parse(tokens).expect("parse")
}

fn make_env() -> Env {
    let env = Env::new();
    builtins::register_builtins(&env);
    env
}

/// Format an evaluation result for comparison. Errors compare by message.
fn render(r: &Result<Value, EvalError>) -> String {
    match r {
        Ok(v) => format!("{v}"),
        Err(e) => format!("ERR:{e}"),
    }
}

/// True when the function value has been JIT-compiled (not merely promoted to
/// bytecode, and not marked as a permanent JIT failure).
fn is_jit_compiled(f: &Value) -> bool {
    const JIT_FAILED: usize = 1;
    if let Value::BytecodeFunction(bc) = f {
        let p = bc.jit_fn_ptr.load(std::sync::atomic::Ordering::Relaxed) as usize;
        !bc.jit_fn_ptr
            .load(std::sync::atomic::Ordering::Relaxed)
            .is_null()
            && p != JIT_FAILED
    } else {
        false
    }
}

/// Evaluate `name` in a fresh env once (tree-walk) and return its result.
fn treewalk(defs: &str, name: &str, args: &[Value]) -> Result<Value, EvalError> {
    let env = make_env();
    eval::eval_program(&parse_program(defs), &env).expect("define (expected)");
    let f = env.get(name).unwrap_or_else(|| panic!("no {name} defined"));
    eval::apply_function(&f, args, &env)
}

/// Drive `name` to native JIT and assert every call agrees with the tree-walk
/// result. `defs` is a program that defines `name`.
fn diff_jit(defs: &str, name: &str, args: &[Value]) {
    let expected = treewalk(defs, name, args);

    let env = make_env();
    eval::eval_program(&parse_program(defs), &env).expect("define (actual)");
    let mut last = None;
    for _ in 0..2500 {
        let f = env.get(name).unwrap();
        last = Some(eval::apply_function(&f, args, &env));
    }
    let last = last.expect("at least one call");

    let f_now = env.get(name).unwrap();
    assert!(
        is_jit_compiled(&f_now),
        "{name}: expected native JIT compilation for `{defs}`"
    );
    assert_eq!(
        render(&last),
        render(&expected),
        "JIT diverged from tree-walk for `{defs}` with args {args:?}"
    );
}

/// Like [`diff_jit`] but also checks the bytecode VM tier and does *not*
/// require native compilation (covering the JIT fallback path).
fn diff_all(defs: &str, name: &str, args: &[Value]) {
    let expected = treewalk(defs, name, args);

    // Bytecode VM tier (just past the promotion threshold, below the JIT one).
    let vm_env = make_env();
    eval::eval_program(&parse_program(defs), &vm_env).expect("define (vm)");
    let mut vm_last = None;
    for _ in 0..400 {
        let f = vm_env.get(name).unwrap();
        vm_last = Some(eval::apply_function(&f, args, &vm_env));
    }
    assert_eq!(
        render(&vm_last.unwrap()),
        render(&expected),
        "bytecode VM diverged from tree-walk for `{defs}` with args {args:?}"
    );

    // Native JIT tier.
    let jit_env = make_env();
    eval::eval_program(&parse_program(defs), &jit_env).expect("define (jit)");
    let mut jit_last = None;
    for _ in 0..2500 {
        let f = jit_env.get(name).unwrap();
        jit_last = Some(eval::apply_function(&f, args, &jit_env));
    }
    assert_eq!(
        render(&jit_last.unwrap()),
        render(&expected),
        "JIT diverged from tree-walk for `{defs}` with args {args:?}"
    );
}

/// Assert that `name` is *not* natively compiled (the JIT declined it and the
/// interpreter fallback is used) and that all tiers still agree.
fn diff_fallback(defs: &str, name: &str, args: &[Value]) {
    let expected = treewalk(defs, name, args);
    let env = make_env();
    eval::eval_program(&parse_program(defs), &env).expect("define");
    let mut last = None;
    for _ in 0..2500 {
        let f = env.get(name).unwrap();
        last = Some(eval::apply_function(&f, args, &env));
    }
    let f = env.get(name).unwrap();
    assert!(
        !is_jit_compiled(&f),
        "{name}: expected JIT fallback for `{defs}`"
    );
    assert_eq!(
        render(&last.unwrap()),
        render(&expected),
        "fallback diverged from tree-walk for `{defs}` with args {args:?}"
    );
}

mod coverage {
    use super::*;
    use rug::Integer;

    fn i(n: i64) -> Value {
        Value::Integer(Integer::from(n))
    }
    fn real(n: f64) -> Value {
        Value::Real(rug::Float::with_val(53, n))
    }
    fn list(xs: &[i64]) -> Value {
        Value::List(xs.iter().map(|&n| i(n)).collect())
    }

    #[test]
    fn which_and_switch() {
        for x in [i(5), i(-5), i(0)] {
            diff_jit("f[x_] := Which[x > 0, x, x < 0, -x, True, 0]", "f", &[x]);
        }
        diff_jit("f[x_] := Switch[x, 1, 10, 2, 20, _, 0]", "f", &[i(2)]);
        diff_jit("f[x_] := Switch[x, 1, 10, 2, 20, _, 0]", "f", &[i(9)]);
        diff_jit("f[x_] := Switch[x, y_Integer, y + 1, _, 0]", "f", &[i(4)]);
        diff_jit(
            "f[x_] := Switch[x, y_Integer, y + 1, _, 0]",
            "f",
            &[Value::Str("s".into())],
        );
    }

    #[test]
    fn loops_and_traversal() {
        diff_jit(
            "f[x_] := (s = 0; Do[s = s + k, {k, 1, x}]; s)",
            "f",
            &[i(5)],
        );
        diff_jit(
            "f[x_] := (s = 0; Do[(If[k > 3, Break[]]; s = s + k), {k, 1, x}]; s)",
            "f",
            &[i(10)],
        );
        diff_jit(
            "f[x_] := (s = 0; Do[(If[Mod[k, 2] == 0, Continue[]]; s = s + k), {k, 1, x}]; s)",
            "f",
            &[i(6)],
        );
        diff_jit(
            "f[x_] := (s = 0; For[i = 1, i <= x, i++, s = s + i]; s)",
            "f",
            &[i(4)],
        );
        diff_jit(
            "f[x_] := (s = 0; For[i = 1, i <= x, i = i + 1, s = s + i]; s)",
            "f",
            &[i(4)],
        );
        diff_jit("f[x_] := (s = 0; While[s < x, s = s + 1]; s)", "f", &[i(6)]);
        diff_jit(
            "f[x_] := (s = 0; While[s < x, (If[Mod[s, 2] == 0, (s = s + 1; Continue[])]; s = s + 1)]; s)",
            "f",
            &[i(6)],
        );
    }

    #[test]
    fn nested_loops_break_continue() {
        diff_jit(
            "f[x_] := (s = 0; Do[Do[(If[j > i, Break[]]; s = s + 1), {j, 1, x}], {i, 1, x}]; s)",
            "f",
            &[i(4)],
        );
        diff_jit(
            "f[x_] := (s = 0; For[i = 1, i <= x, i++, For[j = 1, j <= i, j++, (If[j == 2, Continue[]]; s = s + j)]]; s)",
            "f",
            &[i(3)],
        );
    }

    #[test]
    fn break_continue_escaping_delegated_subtree() {
        // `Break`/`Continue` lexically inside a delegated special form
        // (Module/Catch/Table) must still target the enclosing compiled loop.
        diff_jit(
            "f[x_] := (s = 0; While[s < x, Module[{}, If[s == 2, Break[]]; s = s + 1]]; s)",
            "f",
            &[i(6)],
        );
        diff_jit(
            "f[x_] := (s = 0; Do[Module[{}, If[Mod[k, 2] == 0, Continue[]]; s = s + k], {k, 1, x}]; s)",
            "f",
            &[i(6)],
        );
        diff_jit(
            "f[x_] := (s = 0; Do[Catch[If[k == 3, Break[]]; s = s + k], {k, 1, x}]; s)",
            "f",
            &[i(6)],
        );
        diff_jit(
            "f[x_] := (s = 0; Do[Block[{}, If[k == 2, Break[]]; s = s + k], {k, 1, x}]; s)",
            "f",
            &[i(6)],
        );
    }

    #[test]
    fn function_slots_and_closures() {
        diff_jit("f[x_] := (Function[{y}, x + y])[10]", "f", &[i(5)]);
        diff_jit("f[x_] := (Function[{y}, x * y])[10]", "f", &[i(3)]);
        diff_jit("f[x_] := ((# + x) &)[7]", "f", &[i(1)]);
        diff_jit(
            "f[x_, y_] := (Function[{z}, x + y + z])[1]",
            "f",
            &[i(2), i(3)],
        );
        diff_jit("f[x_] := Function[{}, x][];", "f", &[i(9)]);
    }

    #[test]
    fn map_apply_select_fold() {
        diff_jit(
            "f[x_] := Map[Function[{y}, y + x], {1, 2, 3}]",
            "f",
            &[i(10)],
        );
        diff_jit("f[x_] := Map[# + x &, {1, 2, 3}]", "f", &[i(10)]);
        diff_jit("f[x_] := Apply[Plus, {x, 1, 2}]", "f", &[i(10)]);
        diff_jit("f[x_] := Select[{1, 2, 3, 4}, # > x &]", "f", &[i(2)]);
        diff_jit("f[x_] := Fold[Plus, x, {1, 2, 3}]", "f", &[i(0)]);
        diff_jit("f[x_] := FoldList[Plus, x, {1, 2, 3}]", "f", &[i(0)]);
        diff_jit("f[x_] := MapIndexed[#1 + x &, {1, 2}]", "f", &[i(1)]);
    }

    #[test]
    fn nest_fixedpoint_scan() {
        diff_jit("f[x_] := Nest[Function[{y}, y + 1], x, 3]", "f", &[i(0)]);
        diff_jit(
            "f[x_] := NestList[Function[{y}, y * 2], x, 3]",
            "f",
            &[i(1)],
        );
        diff_jit(
            "f[x_] := FixedPoint[Function[{y}, If[y > 10, y, y + 3]], x]",
            "f",
            &[i(0)],
        );
        diff_jit(
            "f[x_] := Scan[Function[{y}, Null], {x, 1, 2}]",
            "f",
            &[i(1)],
        );
        diff_jit("f[x_] := Nest[Function[{y}, y + x], 0, 4]", "f", &[i(2)]);
    }

    #[test]
    fn nested_table_and_iterators() {
        diff_jit("f[x_] := Table[i + j + x, {i, 2}, {j, 2}]", "f", &[i(1)]);
        diff_jit("f[x_] := Table[i * x, {i, 3}]", "f", &[i(2)]);
        diff_jit("f[x_] := Sum[k + x, {k, 1, 4}]", "f", &[i(1)]);
        diff_jit("f[x_] := Product[k, {k, 1, x}]", "f", &[i(5)]);
        diff_jit(
            "f[x_] := (s = 0; Do[s = s + k, {k, {1, 2, 3}}]; s)",
            "f",
            &[i(0)],
        );
    }

    #[test]
    fn scoping_constructs() {
        diff_jit("f[x_] := Module[{y = x + 1}, y * 2]", "f", &[i(5)]);
        diff_jit("f[x_] := Block[{y}, y = x + 1; y * 2]", "f", &[i(5)]);
        diff_jit("f[x_] := With[{y = x + 1}, y * 2]", "f", &[i(5)]);
        diff_jit(
            "f[x_] := Module[{y}, y = x; Table[y + k, {k, 3}]]",
            "f",
            &[i(2)],
        );
        // A `Return` inside a delegated scoping form exits the function.
        diff_jit("f[x_] := Module[{}, If[x > 0, Return[x]]; 0]", "f", &[i(7)]);
        diff_jit("f[x_] := Block[{}, Return[x + 1]]", "f", &[i(7)]);
    }

    #[test]
    fn catch_throw() {
        diff_all("f[x_] := Catch[If[x < 0, Throw[\"neg\"], x]]", "f", &[i(3)]);
        diff_all(
            "f[x_] := Catch[If[x < 0, Throw[\"neg\"], x]]",
            "f",
            &[i(-1)],
        );
        diff_all("f[x_] := Catch[x + 1]", "f", &[i(41)]);
        diff_all("f[x_] := Catch[Throw[x, \"t\"], \"t\"]", "f", &[i(5)]);
        diff_all("f[x_] := Catch[Throw[x, \"other\"], \"t\"]", "f", &[i(5)]);
    }

    #[test]
    fn string_ops() {
        diff_jit(
            "f[x_] := StringJoin[\"a\", x, \"b\"]",
            "f",
            &[Value::Str("X".into())],
        );
        diff_jit(
            "f[x_] := StringLength[StringJoin[x, x]]",
            "f",
            &[Value::Str("ab".into())],
        );
        diff_jit("f[x_] := ToUpperCase[x]", "f", &[Value::Str("ab".into())]);
        diff_jit(
            "f[x_] := StringReverse[x]",
            "f",
            &[Value::Str("abc".into())],
        );
        diff_all("f[x_] := StringTake[x, 5]", "f", &[Value::Str("ab".into())]);
    }

    #[test]
    fn part_and_slicing() {
        diff_jit("f[x_] := Part[x, 2]", "f", &[list(&[1, 2, 3])]);
        diff_jit("f[x_] := x[[2]]", "f", &[list(&[1, 2, 3])]);
        diff_jit("f[x_] := x[[-1]]", "f", &[list(&[1, 2, 3])]);
        diff_jit("f[x_] := Take[x, 2]", "f", &[list(&[1, 2, 3])]);
        diff_jit("f[x_] := Take[x, {1, 2}]", "f", &[list(&[1, 2, 3])]);
        diff_jit("f[x_] := Drop[x, 1]", "f", &[list(&[1, 2, 3])]);
        diff_all("f[x_] := Part[x, 9]", "f", &[list(&[1, 2, 3])]);
    }

    #[test]
    fn arithmetic_and_comparison() {
        for (src, a, b) in [
            ("x + y", 2, 3),
            ("x - y", 7, 4),
            ("x * y", 6, 7),
            ("x / y", 9, 2),
            ("x^y", 2, 8),
            ("x == y", 1, 1),
            ("x < y", 1, 2),
            ("x >= y", 2, 3),
            ("x + y", 1, 2),
        ] {
            diff_jit(&format!("f[x_, y_] := {src}"), "f", &[i(a), i(b)]);
        }
        diff_jit("f[x_, y_] := x + y", "f", &[real(2.5), real(1.25)]);
        diff_jit("f[x_, y_] := x / y", "f", &[real(3.0), real(2.0)]);
        diff_jit("f[x_] := -x", "f", &[i(5)]);
    }

    #[test]
    fn error_paths() {
        // Evaluated through the complete pipeline; errors must propagate with
        // the same message on every tier rather than being swallowed.
        diff_all("f[x_] := First[{}]", "f", &[i(7)]);
        diff_all("f[x_] := First[x]", "f", &[list(&[])]);
        diff_all("f[x_] := Part[{}, 1]", "f", &[i(0)]);
        diff_all("f[x_] := 1/0", "f", &[i(1)]);
        diff_all("f[x_] := 1.0/0.0", "f", &[i(1)]);
        diff_all("f[x_] := Sqrt[-1]", "f", &[i(1)]);
    }

    #[test]
    fn assignment_propagation() {
        // `Set` inside a promoted function must propagate to the global binding
        // (set_propagate), not shadow it in the call frame.
        diff_jit("f[x_] := (g = x + 1; g)", "f", &[i(41)]);

        // Verify the global is actually updated on the JIT tier.
        let env = make_env();
        eval::eval_program(&parse_program("f[x_] := (g = x + 1; g)"), &env).unwrap();
        env.set("g".to_string(), i(1));
        for _ in 0..2500 {
            let f = env.get("f").unwrap();
            eval::apply_function(&f, &[i(41)], &env).unwrap();
        }
        assert_eq!(env.get("g"), Some(i(42)));
    }

    #[test]
    fn recursion() {
        diff_jit("f[x_] := If[x <= 1, 1, x * f[x - 1]]", "f", &[i(6)]);
        diff_jit("f[x_] := If[x <= 0, 0, x + f[x - 1]]", "f", &[i(10)]);
    }

    #[test]
    fn jit_fallback_for_escaping_control() {
        // A bare `Break[]`/`Continue[]` with no enclosing compiled loop lowers
        // to a `Break`/`Continue` instruction, which the JIT deliberately
        // rejects; the interpreter fallback must be used and stay correct.
        diff_fallback("f[x_] := (If[x > 0, Break[]]; x)", "f", &[i(5)]);
        diff_fallback("f[x_] := (If[x > 0, Continue[]]; x)", "f", &[i(5)]);
    }
}
