//! predicates: DuplicateFreeQ, FunctionQ, OrderedQ, PalindromeQ, SquareMatrixQ,
//! StringQ, SymbolQ, NonPositiveQ, IntervalMemberQ, NumberQ and the numeric
//! exactness predicates, plus VectorQ/MatrixQ/ArrayQ shape variants.
//!
//! Gap-fill module; see the individual builtin docs for semantics. Every
//! predicate returns a `Boolean` and is deliberately tolerant of malformed
//! input: wrong arity or a non-matching shape yields `False` rather than an
//! evaluation error.

use crate::env::Env;
use crate::value::{DEFAULT_PRECISION, EvalError, Value};
use rug::Float;
use std::cmp::Ordering;

/// Register the builtins implemented in this module.
pub fn register(env: &Env) {
    for (name, func) in BUILTINS {
        super::register_builtin(env, name, *func);
    }
    // The shape predicates accept an optional test function, so they need
    // environment access to apply it.
    super::register_builtin_env(env, "VectorQ", builtin_vector_q);
    super::register_builtin_env(env, "MatrixQ", builtin_matrix_q);
    super::register_builtin_env(env, "ArrayQ", builtin_array_q);
}

#[allow(clippy::type_complexity)]
const BUILTINS: &[(&str, fn(&[Value]) -> Result<Value, EvalError>)] = &[
    ("DuplicateFreeQ", builtin_duplicate_free_q),
    ("ExactNumberQ", builtin_exact_number_q),
    ("FunctionQ", builtin_function_q),
    ("InexactNumberQ", builtin_inexact_number_q),
    ("IntervalMemberQ", builtin_interval_member_q),
    ("MachineNumberQ", builtin_machine_number_q),
    ("NonNegativeQ", builtin_non_negative_q),
    ("NonPositiveQ", builtin_non_positive_q),
    ("NumberQ", builtin_number_q),
    ("OrderedQ", builtin_ordered_q),
    ("PalindromeQ", builtin_palindrome_q),
    ("SquareMatrixQ", builtin_square_matrix_q),
    ("StringQ", builtin_string_q),
    ("SymbolQ", builtin_symbol_q),
];

// ─────────────────────────────────────────────────────────────────────────────
// Small helpers
// ─────────────────────────────────────────────────────────────────────────────

fn t() -> Value {
    Value::Bool(true)
}

fn f() -> Value {
    Value::Bool(false)
}

/// Numeric view of a value, if it is a real-valued number.
fn as_float(v: &Value) -> Option<Float> {
    match v {
        Value::Integer(n) => Some(Float::with_val(DEFAULT_PRECISION, n)),
        Value::Real(r) => Some(r.clone()),
        Value::Rational(r) => Some(Float::with_val(DEFAULT_PRECISION, &**r)),
        _ => None,
    }
}

/// Compare two values in the canonical order used by `OrderedQ`. Values of
/// unrelated kinds are ordered by type name then printed form so that a
/// deterministic (total) order always exists.
fn canonical_cmp(a: &Value, b: &Value) -> Ordering {
    if let Value::Formatted { value, .. } = a {
        return canonical_cmp(value, b);
    }
    if let Value::Formatted { value, .. } = b {
        return canonical_cmp(a, value);
    }
    if let (Some(x), Some(y)) = (as_float(a), as_float(b)) {
        return x.partial_cmp(&y).unwrap_or(Ordering::Equal);
    }
    match (a, b) {
        (Value::Str(x), Value::Str(y)) => x.cmp(y),
        (Value::Symbol(x), Value::Symbol(y)) => x.cmp(y),
        (Value::Bool(x), Value::Bool(y)) => x.cmp(y),
        (Value::Null, Value::Null) => Ordering::Equal,
        (Value::List(x), Value::List(y)) => cmp_seq(x, y),
        (Value::Call { head: h1, args: a1 }, Value::Call { head: h2, args: a2 }) => {
            h1.cmp(h2).then_with(|| cmp_seq(a1, a2))
        }
        _ => a
            .type_name()
            .cmp(b.type_name())
            .then_with(|| a.to_string().cmp(&b.to_string())),
    }
}

fn cmp_seq(x: &[Value], y: &[Value]) -> Ordering {
    for (a, b) in x.iter().zip(y.iter()) {
        let ord = canonical_cmp(a, b);
        if ord != Ordering::Equal {
            return ord;
        }
    }
    x.len().cmp(&y.len())
}

/// True when `v` is callable (pure function, named function, bytecode or builtin).
fn is_function_value(v: &Value) -> bool {
    matches!(
        v,
        Value::Function(_)
            | Value::BytecodeFunction(_)
            | Value::Builtin(..)
            | Value::PureFunction { .. }
    )
}

/// True for an explicit number atom.
fn is_number_value(v: &Value) -> bool {
    matches!(
        v,
        Value::Integer(_) | Value::Real(_) | Value::Rational(_) | Value::Complex { .. }
    )
}

/// True for an exact number atom (integer, rational or algebraic root).
fn is_exact_number_value(v: &Value) -> bool {
    matches!(
        v,
        Value::Integer(_) | Value::Rational(_) | Value::Root { .. }
    )
}

/// Convert a value to an expression so it can be used as a pattern.
fn value_to_pattern_expr(val: &Value) -> crate::ast::Expr {
    use crate::ast::Expr;
    match val {
        Value::Pattern(expr) => expr.clone(),
        Value::List(items) => Expr::List(items.iter().map(value_to_pattern_expr).collect()),
        Value::Call { head, args } => Expr::Call {
            head: Box::new(Expr::Symbol(head.clone())),
            args: args.iter().map(value_to_pattern_expr).collect(),
        },
        Value::Integer(n) => Expr::Integer(n.clone()),
        Value::Real(r) => Expr::Real(r.clone()),
        Value::Str(s) => Expr::Str(s.clone()),
        Value::Bool(b) => Expr::Bool(*b),
        Value::Null => Expr::Null,
        Value::Symbol(s) => Expr::Symbol(s.clone()),
        Value::Builtin(name, _) => Expr::Symbol(name.clone()),
        _ => Expr::Symbol(val.to_string()),
    }
}

/// True when `value` structurally matches the pattern `pattern`.
fn matches_pattern(value: &Value, pattern: &Value) -> bool {
    let pat_expr = match pattern {
        Value::Pattern(expr) => expr.clone(),
        _ => value_to_pattern_expr(pattern),
    };
    matches!(
        crate::pattern::match_pattern(&pat_expr, value, None),
        crate::pattern::MatchResult::Match(_)
    )
}

/// True when a bare type symbol (e.g. `Integer`) names `value`'s type.
fn matches_type_symbol(value: &Value, pattern: &Value) -> bool {
    match pattern {
        Value::Symbol(s) => value.matches_type(s),
        _ => false,
    }
}

/// Evaluate an element criterion that may be a function or a pattern.
fn element_ok(value: &Value, test: &Value, env: &Env) -> Result<bool, EvalError> {
    if is_function_value(test) {
        Ok(apply_function(test, &[value.clone()], env)?.to_bool())
    } else {
        Ok(matches_pattern(value, test) || matches_type_symbol(value, test))
    }
}

/// A flat list (no element is itself a list).
fn is_vector(v: &Value) -> bool {
    matches!(v, Value::List(items) if !items.iter().any(|i| matches!(i, Value::List(_))))
}

/// A rectangular list of equally long flat rows (a matrix).
fn is_matrix(v: &Value) -> bool {
    let rows = match v {
        Value::List(rows) => rows,
        _ => return false,
    };
    if rows.is_empty() {
        return true;
    }
    let first_len = match &rows[0] {
        Value::List(cols) => cols.len(),
        _ => return false,
    };
    rows.iter().all(|row| match row {
        Value::List(cols) => {
            cols.len() == first_len && !cols.iter().any(|c| matches!(c, Value::List(_)))
        }
        _ => false,
    })
}

/// If `v` is a rectangular nested-list array, return its shape.
fn array_shape(v: &Value) -> Option<Vec<usize>> {
    let items = match v {
        Value::List(items) => items,
        _ => return None,
    };
    let mut child_shape: Option<Vec<usize>> = None;
    let mut seen_list = false;
    let mut seen_leaf = false;
    for it in items {
        match array_shape(it) {
            Some(shape) => {
                seen_list = true;
                match &child_shape {
                    None => child_shape = Some(shape),
                    Some(existing) if existing == &shape => {}
                    Some(_) => return None,
                }
            }
            None => seen_leaf = true,
        }
    }
    if seen_list && seen_leaf {
        return None;
    }
    let mut shape = vec![items.len()];
    if let Some(cs) = child_shape {
        shape.extend(cs);
    }
    Some(shape)
}

/// True when `v` is a full (rectangular) n-dimensional array.
fn is_full_array(v: &Value) -> bool {
    matches!(v, Value::List(_)) && array_shape(v).is_some()
}

/// Collect the deepest-level elements of a full array.
fn array_leaves(v: &Value) -> Vec<Value> {
    match v {
        Value::List(items) if items.iter().any(|i| matches!(i, Value::List(_))) => {
            let mut out = Vec::new();
            for it in items {
                out.extend(array_leaves(it));
            }
            out
        }
        Value::List(items) => items.clone(),
        other => vec![other.clone()],
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Structural predicates
// ─────────────────────────────────────────────────────────────────────────────

/// DuplicateFreeQ[list] — True if no two elements are structurally identical.
/// A non-list argument is treated as a singleton and returns True.
pub fn builtin_duplicate_free_q(args: &[Value]) -> Result<Value, EvalError> {
    let Some(v) = args.first() else {
        return Ok(f());
    };
    let items = match v {
        Value::List(items) => items,
        _ => return Ok(t()),
    };
    for i in 0..items.len() {
        for j in (i + 1)..items.len() {
            if items[i].struct_eq(&items[j]) {
                return Ok(f());
            }
        }
    }
    Ok(t())
}

/// FunctionQ[expr] — True if expr is a callable (pure/named function or builtin).
pub fn builtin_function_q(args: &[Value]) -> Result<Value, EvalError> {
    let Some(v) = args.first() else {
        return Ok(f());
    };
    Ok(bool_val(match v {
        Value::List(items) => !items.is_empty() && items.iter().all(is_function_value),
        _ => is_function_value(v),
    }))
}

/// OrderedQ[list] — True if the elements appear in canonical (non-decreasing) order.
pub fn builtin_ordered_q(args: &[Value]) -> Result<Value, EvalError> {
    let Some(v) = args.first() else {
        return Ok(f());
    };
    let items = match v {
        Value::List(items) => items,
        _ => return Ok(t()),
    };
    Ok(bool_val(
        items
            .windows(2)
            .all(|w| canonical_cmp(&w[0], &w[1]) != Ordering::Greater),
    ))
}

/// PalindromeQ[expr] — True for a list, string or integer that reads the same
/// forwards and backwards.
pub fn builtin_palindrome_q(args: &[Value]) -> Result<Value, EvalError> {
    let Some(v) = args.first() else {
        return Ok(f());
    };
    let result = match v {
        Value::List(items) => items
            .iter()
            .zip(items.iter().rev())
            .all(|(a, b)| a.struct_eq(b)),
        Value::Str(s) => {
            let chars: Vec<char> = s.chars().collect();
            chars.iter().eq(chars.iter().rev())
        }
        Value::Integer(n) => {
            let s = n.to_string();
            let b = s.as_bytes();
            b.iter().eq(b.iter().rev())
        }
        _ => false,
    };
    Ok(bool_val(result))
}

/// SquareMatrixQ[m] — True if m is a square (n x n) matrix.
pub fn builtin_square_matrix_q(args: &[Value]) -> Result<Value, EvalError> {
    let Some(v) = args.first() else {
        return Ok(f());
    };
    let square = match v {
        Value::List(rows) => {
            is_matrix(v)
                && rows.first().is_none_or(|r| match r {
                    Value::List(cols) => cols.len() == rows.len(),
                    _ => false,
                })
        }
        _ => false,
    };
    Ok(bool_val(square))
}

/// StringQ[expr] — True if expr is a string (or a list of strings).
pub fn builtin_string_q(args: &[Value]) -> Result<Value, EvalError> {
    let Some(v) = args.first() else {
        return Ok(f());
    };
    Ok(bool_val(match v {
        Value::List(items) => !items.is_empty() && items.iter().all(|i| matches!(i, Value::Str(_))),
        _ => matches!(v, Value::Str(_)),
    }))
}

/// SymbolQ[expr] — True if expr is a symbol (or a list of symbols).
pub fn builtin_symbol_q(args: &[Value]) -> Result<Value, EvalError> {
    let Some(v) = args.first() else {
        return Ok(f());
    };
    Ok(bool_val(match v {
        Value::List(items) => {
            !items.is_empty() && items.iter().all(|i| matches!(i, Value::Symbol(_)))
        }
        _ => matches!(v, Value::Symbol(_)),
    }))
}

// ─────────────────────────────────────────────────────────────────────────────
// Numeric predicates
// ─────────────────────────────────────────────────────────────────────────────

/// NumberQ[expr] — True if expr is an explicit number atom.
pub fn builtin_number_q(args: &[Value]) -> Result<Value, EvalError> {
    let Some(v) = args.first() else {
        return Ok(f());
    };
    Ok(bool_val(match v {
        Value::List(items) => !items.is_empty() && items.iter().all(is_number_value),
        _ => is_number_value(v),
    }))
}

/// MachineNumberQ[expr] — True if expr is a machine-precision real or complex.
pub fn builtin_machine_number_q(args: &[Value]) -> Result<Value, EvalError> {
    let Some(v) = args.first() else {
        return Ok(f());
    };
    let machine = |x: &Value| matches!(x, Value::Real(_) | Value::Complex { .. });
    Ok(bool_val(match v {
        Value::List(items) => !items.is_empty() && items.iter().all(machine),
        _ => machine(v),
    }))
}

/// InexactNumberQ[expr] — True if expr is an approximate (floating-point) number.
pub fn builtin_inexact_number_q(args: &[Value]) -> Result<Value, EvalError> {
    let Some(v) = args.first() else {
        return Ok(f());
    };
    let inexact = |x: &Value| matches!(x, Value::Real(_) | Value::Complex { .. });
    Ok(bool_val(match v {
        Value::List(items) => !items.is_empty() && items.iter().all(inexact),
        _ => inexact(v),
    }))
}

/// ExactNumberQ[expr] — True if expr is an exact number atom.
pub fn builtin_exact_number_q(args: &[Value]) -> Result<Value, EvalError> {
    let Some(v) = args.first() else {
        return Ok(f());
    };
    Ok(bool_val(match v {
        Value::List(items) => !items.is_empty() && items.iter().all(is_exact_number_value),
        _ => is_exact_number_value(v),
    }))
}

/// NonPositiveQ[x] — True if x <= 0.
pub fn builtin_non_positive_q(args: &[Value]) -> Result<Value, EvalError> {
    let Some(v) = args.first() else {
        return Ok(f());
    };
    let non_positive = match v {
        Value::Integer(n) => !n.is_positive(),
        Value::Real(r) => !(r.is_sign_positive() && !r.is_zero()),
        Value::Rational(r) => **r <= 0,
        _ => false,
    };
    Ok(bool_val(non_positive))
}

/// NonNegativeQ[x] — True if x >= 0.
pub fn builtin_non_negative_q(args: &[Value]) -> Result<Value, EvalError> {
    let Some(v) = args.first() else {
        return Ok(f());
    };
    let non_negative = match v {
        Value::Integer(n) => !n.is_negative(),
        Value::Real(r) => !r.is_sign_negative(),
        Value::Rational(r) => **r >= 0,
        _ => false,
    };
    Ok(bool_val(non_negative))
}

// ─────────────────────────────────────────────────────────────────────────────
// Membership predicates
// ─────────────────────────────────────────────────────────────────────────────

/// IntervalMemberQ[interval, x] — True if x lies in the interval `{a, b}` or
/// `Interval[{a, b}, …]`. A list `x` is tested element-wise.
pub fn builtin_interval_member_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Ok(f());
    }
    let intervals = parse_intervals(&args[0]);
    if intervals.is_empty() {
        return Ok(f());
    }
    let in_interval = |x: &Value| -> bool {
        let Some(xv) = as_float(x) else {
            return false;
        };
        intervals.iter().any(|(lo, hi)| {
            let (lo, hi) = if lo <= hi { (lo, hi) } else { (hi, lo) };
            xv >= *lo && xv <= *hi
        })
    };
    match &args[1] {
        Value::List(items) => Ok(Value::List(
            items.iter().map(|x| bool_val(in_interval(x))).collect(),
        )),
        x => Ok(bool_val(in_interval(x))),
    }
}

/// Parse an interval specification into numeric `(low, high)` pairs.
fn parse_intervals(v: &Value) -> Vec<(Float, Float)> {
    fn pair(v: &Value) -> Option<(Float, Float)> {
        match v {
            Value::List(items) if items.len() == 2 => {
                Some((as_float(&items[0])?, as_float(&items[1])?))
            }
            _ => None,
        }
    }
    match v {
        Value::List(items) if items.len() == 2 => pair(v).into_iter().collect(),
        Value::Call { head, args } if head == "Interval" => args.iter().filter_map(pair).collect(),
        _ => Vec::new(),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Shape predicates
// ─────────────────────────────────────────────────────────────────────────────

/// VectorQ[list] — True if list is a flat list.
/// VectorQ[list, test] — True if list is flat and every element passes `test`.
pub fn builtin_vector_q(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    let Some(v) = args.first() else {
        return Ok(f());
    };
    if !is_vector(v) {
        return Ok(f());
    }
    if let Some(test) = args.get(1)
        && let Value::List(items) = v
    {
        for item in items {
            if !element_ok(item, test, env)? {
                return Ok(f());
            }
        }
    }
    Ok(t())
}

/// MatrixQ[list] — True if list is a rectangular matrix.
/// MatrixQ[list, test] — True if list is a matrix and every element passes `test`.
pub fn builtin_matrix_q(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    let Some(v) = args.first() else {
        return Ok(f());
    };
    if !is_matrix(v) {
        return Ok(f());
    }
    if let Some(test) = args.get(1)
        && let Value::List(rows) = v
    {
        for row in rows {
            if let Value::List(cols) = row {
                for item in cols {
                    if !element_ok(item, test, env)? {
                        return Ok(f());
                    }
                }
            }
        }
    }
    Ok(t())
}

/// ArrayQ[expr] — True if expr is a full rectangular array.
/// ArrayQ[expr, pattern] — True if expr is a full array whose leaves match `pattern`
/// (or, when a function is given, whose leaves pass that function).
/// ArrayQ[expr, pattern, test] — additionally requires every leaf to pass `test`.
pub fn builtin_array_q(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    let Some(v) = args.first() else {
        return Ok(f());
    };
    if !is_full_array(v) {
        return Ok(f());
    }
    if let Some(pattern) = args.get(1) {
        for leaf in array_leaves(v) {
            if !element_ok(&leaf, pattern, env)? {
                return Ok(f());
            }
        }
    }
    if let Some(test) = args.get(2) {
        for leaf in array_leaves(v) {
            if !element_ok(&leaf, test, env)? {
                return Ok(f());
            }
        }
    }
    Ok(t())
}

fn apply_function(func: &Value, args: &[Value], env: &Env) -> Result<Value, EvalError> {
    crate::eval::apply_function(func, args, env)
}

fn bool_val(b: bool) -> Value {
    Value::Bool(b)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::Env;
    use rug::Integer;

    fn int(n: i64) -> Value {
        Value::Integer(Integer::from(n))
    }
    #[allow(dead_code)]
    fn sym(s: &str) -> Value {
        Value::Symbol(s.to_string())
    }
    fn list(v: Vec<Value>) -> Value {
        Value::List(v)
    }

    fn env_with_builtins() -> Env {
        let env = Env::new();
        crate::builtins::register_builtins(&env);
        env
    }

    fn eval_str(input: &str) -> Value {
        let env = env_with_builtins();
        let tokens = crate::lexer::tokenize(input).unwrap();
        let ast = crate::parser::parse(tokens).unwrap_or_else(|e| panic!("parse {input}: {e:?}"));
        crate::eval::eval_program(&ast, &env).unwrap_or_else(|e| panic!("eval {input}: {e:?}"))
    }

    #[test]
    fn test_duplicate_free_q() {
        assert_eq!(
            builtin_duplicate_free_q(&[list(vec![int(1), int(2), int(3)])]).unwrap(),
            t()
        );
        assert_eq!(
            builtin_duplicate_free_q(&[list(vec![int(1), int(2), int(1)])]).unwrap(),
            f()
        );
        // Non-list and odd input are tolerated.
        assert_eq!(builtin_duplicate_free_q(&[int(5)]).unwrap(), t());
        assert_eq!(builtin_duplicate_free_q(&[]).unwrap(), f());
    }

    #[test]
    fn test_function_q_end_to_end() {
        assert_eq!(eval_str("FunctionQ[Sin]"), t());
        assert_eq!(eval_str("FunctionQ[5]"), f());
        assert_eq!(eval_str("FunctionQ[#&]"), t());
    }

    #[test]
    fn test_ordered_q() {
        assert_eq!(eval_str("OrderedQ[{1, 2, 3}]"), t());
        assert_eq!(eval_str("OrderedQ[{3, 2, 1}]"), f());
        assert_eq!(eval_str("OrderedQ[{a, b, c}]"), t());
        assert_eq!(eval_str("OrderedQ[{b, a}]"), f());
        assert_eq!(eval_str("OrderedQ[5]"), t());
    }

    #[test]
    fn test_palindrome_q() {
        assert_eq!(eval_str("PalindromeQ[{1, 2, 1}]"), t());
        assert_eq!(eval_str("PalindromeQ[{1, 2, 3}]"), f());
        assert_eq!(eval_str("PalindromeQ[\"abcba\"]"), t());
        assert_eq!(eval_str("PalindromeQ[\"abc\"]"), f());
        assert_eq!(eval_str("PalindromeQ[12321]"), t());
        assert_eq!(eval_str("PalindromeQ[123]"), f());
    }

    #[test]
    fn test_square_matrix_q() {
        assert_eq!(eval_str("SquareMatrixQ[{{1, 2}, {3, 4}}]"), t());
        assert_eq!(eval_str("SquareMatrixQ[{{1, 2}, {3, 4}, {5, 6}}]"), f());
        assert_eq!(eval_str("SquareMatrixQ[{1, 2}]"), f());
        assert_eq!(builtin_square_matrix_q(&[]).unwrap(), f());
    }

    #[test]
    fn test_string_q_and_symbol_q() {
        assert_eq!(eval_str("StringQ[\"hi\"]"), t());
        assert_eq!(eval_str("StringQ[5]"), f());
        assert_eq!(eval_str("SymbolQ[x]"), t());
        assert_eq!(eval_str("SymbolQ[\"x\"]"), f());
    }

    #[test]
    fn test_numeric_predicates() {
        assert_eq!(eval_str("NumberQ[5]"), t());
        assert_eq!(eval_str("NumberQ[1/2]"), t());
        assert_eq!(eval_str("NumberQ[Pi]"), f());
        assert_eq!(eval_str("ExactNumberQ[1/2]"), t());
        assert_eq!(eval_str("ExactNumberQ[1.5]"), f());
        assert_eq!(eval_str("InexactNumberQ[1.5]"), t());
        assert_eq!(eval_str("InexactNumberQ[1/2]"), f());
        assert_eq!(eval_str("MachineNumberQ[1.5]"), t());
        assert_eq!(eval_str("MachineNumberQ[1/2]"), f());
        // Odd input tolerated.
        assert_eq!(builtin_number_q(&[]).unwrap(), f());
    }

    #[test]
    fn test_non_positive_and_non_negative_q() {
        assert_eq!(eval_str("NonPositiveQ[-3]"), t());
        assert_eq!(eval_str("NonPositiveQ[0]"), t());
        assert_eq!(eval_str("NonPositiveQ[3]"), f());
        assert_eq!(eval_str("NonPositiveQ[x]"), f());
        assert_eq!(eval_str("NonNegativeQ[0]"), t());
        assert_eq!(eval_str("NonNegativeQ[-1]"), f());
    }

    #[test]
    fn test_interval_member_q() {
        assert_eq!(eval_str("IntervalMemberQ[{0, 10}, 5]"), t());
        assert_eq!(eval_str("IntervalMemberQ[{0, 10}, 11]"), f());
        assert_eq!(eval_str("IntervalMemberQ[Interval[{0, 10}], 5]"), t());
        assert_eq!(
            eval_str("IntervalMemberQ[Interval[{0, 10}, {20, 30}], 25]"),
            t()
        );
        assert_eq!(eval_str("IntervalMemberQ[{10, 0}, 5]"), t());
        assert_eq!(
            builtin_interval_member_q(&[list(vec![int(0), int(10)]), list(vec![int(1), int(20)])])
                .unwrap(),
            list(vec![t(), f()])
        );
        assert_eq!(builtin_interval_member_q(&[]).unwrap(), f());
    }

    #[test]
    fn test_vector_and_matrix_q_extras() {
        assert_eq!(eval_str("VectorQ[{1, 2, 3}]"), t());
        assert_eq!(eval_str("VectorQ[{1, {2}}]"), f());
        assert_eq!(eval_str("VectorQ[{1, 2, 3}, IntegerQ]"), t());
        assert_eq!(eval_str("VectorQ[{1, 2, x}, IntegerQ]"), f());
        assert_eq!(eval_str("MatrixQ[{{1, 2}, {3, 4}}]"), t());
        assert_eq!(eval_str("MatrixQ[{{1, 2}, {3, 4, 5}}]"), f());
        assert_eq!(eval_str("MatrixQ[{{1, 2}, {3, 4}}, IntegerQ]"), t());
        assert_eq!(eval_str("ArrayQ[{{1, 2}, {3, 4}}]"), t());
        assert_eq!(eval_str("ArrayQ[{{1, 2}, {3}}]"), f());
        assert_eq!(eval_str("ArrayQ[{{1, 2}, {3, 4}}, Integer]"), t());
        assert_eq!(eval_str("ArrayQ[{{1, 2}, {3, 4}}, _Integer]"), t());
        assert_eq!(eval_str("ArrayQ[{{1, 2}, {3, x}}, Integer]"), f());
    }

    #[test]
    fn test_odd_input_never_errors() {
        for name in [
            "DuplicateFreeQ",
            "FunctionQ",
            "OrderedQ",
            "PalindromeQ",
            "SquareMatrixQ",
            "StringQ",
            "SymbolQ",
            "NonPositiveQ",
            "NumberQ",
            "MachineNumberQ",
            "InexactNumberQ",
            "ExactNumberQ",
        ] {
            let out = eval_str(&format!("{name}[{{a, {{b}}, 3.5}}]"));
            assert!(
                matches!(out, Value::Bool(_) | Value::List(_)),
                "{name} should not error, got {out:?}"
            );
        }
    }
}
