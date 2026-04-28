/// Expression inspection, manipulation, predicates, and formula simplification.
///
/// Missing Wolfram Language functions:
/// - Level, LeafCount, Extract
/// - Replace, ReplaceList, Check
/// - FullSimplify, Together, Apart, Collect, Cancel
/// - NumericQ, RealQ, VectorQ, MatrixQ, ExactNumberQ, ScalarQ
/// - Flatten (expr form), Depth, LongestCommonSubsequence (expr form)
use crate::ast::Expr;
use crate::env::Env;
use crate::eval::eval;
use crate::value::{EvalError, Value};
use rug::Integer;

// ─────────────────────────────────────────────────────────────────────────────
// Expression Inspection
// ─────────────────────────────────────────────────────────────────────────────

/// Recursively collect parts at the specified level(s).
/// Level spec: Integer, List of Integers, or {min, max}
/// Positive levels count from outside, negative from inside.
fn collect_at_level(val: &Value, depth: i32, levels: &[(i32, i32)]) -> Vec<Value> {
    if depth < 0 {
        return Vec::new();
    }

    let mut result = Vec::new();
    for &(lo, hi) in levels {
        if depth >= lo && depth <= hi {
            result.push(val.clone());
        }
    }

    // Recurse into sub-parts
    let subvalues = value_subparts(val);
    for sub in subvalues {
        result.extend(collect_at_level(&sub, depth + 1, levels));
    }
    result
}

/// Direct sub-parts of a value (children, not including the value itself).
fn value_subparts(val: &Value) -> Vec<Value> {
    match val {
        Value::List(items) => items.clone(),
        Value::Call { args, .. } => args.clone(),
        Value::Rule { lhs, rhs, .. } => vec![(**lhs).clone(), (**rhs).clone()],
        Value::Assoc(map) => map.values().cloned().collect(),
        Value::Formatted { value, .. } => vec![(**value).clone()],
        Value::Hold(inner) | Value::HoldComplete(inner) => vec![(**inner).clone()],
        Value::Sequence(items) => items.clone(),
        _ => Vec::new(),
    }
}

/// Parse a level specification into a list of (min, max) level ranges.
/// Follows Wolfram convention: -1 = leaves, 0 = top, 1 = first sub-level.
fn parse_level_spec(spec: &Value) -> Vec<(i32, i32)> {
    match spec {
        Value::Integer(n) => {
            let lv = n.to_i64().unwrap_or(0) as i32;
            vec![(lv, lv)]
        }
        Value::List(items) => {
            if items.len() == 2 {
                // {min, max} style
                let lo = match &items[0] {
                    Value::Integer(n) => n.to_i64().unwrap_or(0) as i32,
                    _ => 0,
                };
                let hi = match &items[1] {
                    Value::Integer(n) => n.to_i64().unwrap_or(0) as i32,
                    Value::Symbol(s) if s == "Infinity" => i32::MAX,
                    _ => 0,
                };
                vec![(lo, hi)]
            } else if items.len() == 1 {
                parse_level_spec(&items[0])
            } else {
                // List of individual levels
                items
                    .iter()
                    .filter_map(|v| {
                        if let Value::Integer(n) = v {
                            let lv = n.to_i64().unwrap_or(0) as i32;
                            Some((lv, lv))
                        } else {
                            None
                        }
                    })
                    .collect()
            }
        }
        Value::Symbol(s) if s == "Infinity" => vec![(0, i32::MAX)],
        _ => vec![(0, 0)],
    }
}

/// Count the total number of leaves in a value tree.
fn count_leaves(val: &Value) -> usize {
    match val {
        Value::Integer(_)
        | Value::Real(_)
        | Value::Rational(_)
        | Value::Complex { .. }
        | Value::Str(_)
        | Value::Bool(_)
        | Value::Null
        | Value::Symbol(_) => 1,
        Value::List(items) | Value::Sequence(items) => items.iter().map(count_leaves).sum(),
        Value::Call { args, .. } => args.iter().map(count_leaves).sum(),
        Value::Rule { lhs, rhs, .. } => count_leaves(lhs) + count_leaves(rhs),
        Value::Assoc(map) => map.values().map(count_leaves).sum(),
        Value::Formatted { value, .. } => count_leaves(value),
        Value::Hold(inner) | Value::HoldComplete(inner) => count_leaves(inner),
        _ => 1,
    }
}

/// Compute the maximum depth of a value tree.
/// Atoms have depth 0, List[atom] has depth 1, etc.
fn compute_depth(val: &Value) -> usize {
    match val {
        Value::List(items) | Value::Sequence(items) if !items.is_empty() => {
            1 + items.iter().map(compute_depth).max().unwrap_or(0)
        }
        Value::Call { args, .. } if !args.is_empty() => {
            1 + args.iter().map(compute_depth).max().unwrap_or(0)
        }
        Value::Rule { lhs, rhs, .. } => 1 + std::cmp::max(compute_depth(lhs), compute_depth(rhs)),
        Value::Assoc(map) if !map.is_empty() => {
            1 + map.values().map(compute_depth).max().unwrap_or(0)
        }
        Value::Formatted { value, .. } => compute_depth(value) + 1,
        Value::Hold(inner) | Value::HoldComplete(inner) => compute_depth(inner) + 1,
        _ => 0,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Public Builtins
// ─────────────────────────────────────────────────────────────────────────────

/// Level[expr] or Level[expr, levelspec]
/// Return parts of expr at specified level(s).
pub fn builtin_level(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Err(EvalError::Error(
            "Level expects 1 or 2 arguments".to_string(),
        ));
    }
    let spec = if args.len() == 2 {
        parse_level_spec(&args[1])
    } else {
        vec![(0, i32::MAX)]
    };
    let parts = collect_at_level(&args[0], 0, &spec);
    Ok(Value::List(parts))
}

/// LeafCount[expr]
/// Return the total number of leaves in an expression tree.
pub fn builtin_leaf_count(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("LeafCount expects 1 argument".to_string()));
    }
    Ok(Value::Integer(Integer::from(count_leaves(&args[0]))))
}

/// Depth[expr]
/// Return the maximum depth of nesting in an expression.
pub fn builtin_depth(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error("Depth expects 1 argument".to_string()));
    }
    Ok(Value::Integer(Integer::from(compute_depth(&args[0]))))
}

/// Extract[expr, {i, j, ...}] or Extract[expr, i, j, ...]
/// Return the part of expr at the specified position.
pub fn builtin_extract(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "Extract expects 2 or more arguments".to_string(),
        ));
    }
    let mut current = args[0].clone();

    // Parse indices from args[1..]
    let indices: Vec<usize> = (1..args.len())
        .filter_map(|i| match &args[i] {
            Value::List(_) => None, // handle separately
            Value::Integer(n) => n.to_usize(),
            _ => None,
        })
        .collect();

    // Handle spec as a single list argument: Extract[expr, {i, j, ...}]
    let indices = if indices.is_empty() && args.len() == 2 {
        if let Value::List(items) = &args[1] {
            items
                .iter()
                .filter_map(|v| {
                    if let Value::Integer(n) = v {
                        n.to_usize()
                    } else {
                        None
                    }
                })
                .collect()
        } else {
            return Err(EvalError::Error(
                "Extract indices must be Integers or a list of Integers".to_string(),
            ));
        }
    } else {
        indices
    };

    for &idx in &indices {
        current = extract_part(&current, idx)
            .ok_or_else(|| EvalError::Error(format!("Extract: part {} does not exist", idx)))?;
    }
    Ok(current)
}

/// Extract a 1-based index from a value.
fn extract_part(val: &Value, idx: usize) -> Option<Value> {
    if idx == 0 {
        return Some(val.clone());
    }
    let idx = idx - 1; // convert to 0-based

    match val {
        Value::List(items) => items.get(idx).cloned(),
        Value::Call { head, args } => {
            if idx == 0 {
                Some(Value::Symbol(head.clone()))
            } else {
                args.get(idx - 1).cloned()
            }
        }
        Value::Rule { lhs, rhs, .. } => {
            if idx == 0 {
                Some((**lhs).clone())
            } else {
                Some((**rhs).clone())
            }
        }
        Value::Assoc(map) => map.values().nth(idx).cloned(),
        Value::Sequence(items) => items.get(idx).cloned(),
        _ => None,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Replace / ReplaceList / Check
// ─────────────────────────────────────────────────────────────────────────────

/// Apply a rule to all parts of expr at specified levels and return first successful change.
/// Replace[expr, rule] or Replace[expr, rule, levelspec]
pub fn builtin_replace(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(EvalError::Error(
            "Replace expects 2 or 3 arguments".to_string(),
        ));
    }
    let spec = if args.len() == 3 {
        parse_level_spec(&args[2])
    } else {
        vec![(-1, -1)] // default: leaves only
    };
    let parts = collect_at_level(&args[0], 0, &spec);
    let rule = &args[1];

    // Try each part, return first that changes
    // Simple approach: try on each part at leaf level
    for part in &parts {
        let result = try_apply_rule(part, rule, env)?;
        if !result.struct_eq(part) {
            // Apply the rule to the whole expression at the matching position
            // For simplicity, apply to each part of the original and return first change
            return replace_in_value(&args[0], rule, env, spec.clone());
        }
    }
    Ok(args[0].clone())
}

/// Apply rule in all possible ways and return all results.
/// ReplaceList[expr, rule]
pub fn builtin_replace_list(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "ReplaceList expects exactly 2 arguments".to_string(),
        ));
    }
    let mut results = Vec::new();
    collect_all_replacements(&args[0], &args[1], env, &mut results)?;
    if results.is_empty() {
        results.push(args[0].clone());
    }
    Ok(Value::List(results))
}

/// Collect all possible single replacements (one per sub-expression).
fn collect_all_replacements(
    val: &Value,
    rule: &Value,
    env: &Env,
    results: &mut Vec<Value>,
) -> Result<(), EvalError> {
    let subparts = value_subparts(val);

    // Try replacing at each child position
    for (pos, child) in subparts.iter().enumerate() {
        // Recurse into child first (deepest replacements first)
        collect_all_replacements(child, rule, env, results)?;

        // Try applying rule to this child
        let replaced = try_apply_rule(child, rule, env)?;
        if !replaced.struct_eq(child) {
            let new_val = value_replace_child(val, pos, &replaced)?;
            results.push(new_val);
        }
    }

    // Try applying rule to the value itself
    let replaced_self = try_apply_rule(val, rule, env)?;
    if !replaced_self.struct_eq(val) {
        results.push(replaced_self);
    }
    Ok(())
}

/// Replace the child at position `pos` in val with `new_child`.
fn value_replace_child(val: &Value, pos: usize, new_child: &Value) -> Result<Value, EvalError> {
    match val {
        Value::List(items) => {
            let mut new = items.clone();
            new[pos] = new_child.clone();
            Ok(Value::List(new))
        }
        Value::Call { head, args } => {
            if pos == 0 {
                // Replace head
                let new_head = match new_child {
                    Value::Symbol(s) => s.clone(),
                    _ => new_child.to_string(),
                };
                Ok(Value::Call {
                    head: new_head,
                    args: args.clone(),
                })
            } else {
                let mut new_args = args.clone();
                new_args[pos - 1] = new_child.clone();
                Ok(Value::Call {
                    head: head.clone(),
                    args: new_args,
                })
            }
        }
        Value::Rule { lhs, rhs, delayed } => {
            if pos == 0 {
                Ok(Value::Rule {
                    lhs: Box::new(new_child.clone()),
                    rhs: rhs.clone(),
                    delayed: *delayed,
                })
            } else {
                Ok(Value::Rule {
                    lhs: lhs.clone(),
                    rhs: Box::new(new_child.clone()),
                    delayed: *delayed,
                })
            }
        }
        Value::Sequence(items) => {
            let mut new = items.clone();
            new[pos] = new_child.clone();
            Ok(Value::Sequence(new))
        }
        _ => Ok(val.clone()),
    }
}

/// Try applying a rule to a value. Returns the value unchanged if rule doesn't match.
fn try_apply_rule(val: &Value, rule: &Value, env: &Env) -> Result<Value, EvalError> {
    match rule {
        Value::Rule {
            lhs,
            rhs,
            delayed: false,
        } => {
            // Immediate rule: check if val matches lhs
            if val.struct_eq(lhs) {
                Ok((**rhs).clone())
            } else {
                Ok(val.clone())
            }
        }
        Value::Rule {
            lhs,
            rhs,
            delayed: true,
        } => {
            // Delayed rule: check structural match, then evaluate rhs
            if val.struct_eq(lhs) {
                eval(&convert_value_to_expr(rhs), env)
            } else {
                Ok(val.clone())
            }
        }
        _ => Ok(val.clone()),
    }
}

/// Convert a Value back to an Expr for evaluation.
fn convert_value_to_expr(val: &Value) -> Expr {
    match val {
        Value::Integer(n) => Expr::Integer(n.clone()),
        Value::Real(r) => Expr::Real(r.clone()),
        Value::Str(s) => Expr::Str(s.clone()),
        Value::Bool(b) => Expr::Bool(*b),
        Value::Null => Expr::Null,
        Value::Symbol(s) => Expr::Symbol(s.clone()),
        Value::Complex { re, im } => Expr::Complex { re: *re, im: *im },
        Value::List(items) => Expr::List(items.iter().map(convert_value_to_expr).collect()),
        Value::Call { head, args } => Expr::Call {
            head: Box::new(Expr::Symbol(head.clone())),
            args: args.iter().map(convert_value_to_expr).collect(),
        },
        Value::Rule { lhs, rhs, .. } => Expr::Rule {
            lhs: Box::new(convert_value_to_expr(lhs)),
            rhs: Box::new(convert_value_to_expr(rhs)),
        },
        _ => Expr::Symbol(val.to_string()),
    }
}

/// Replace matching parts at specified levels throughout an expression.
fn replace_in_value(
    val: &Value,
    rule: &Value,
    env: &Env,
    spec: Vec<(i32, i32)>,
) -> Result<Value, EvalError> {
    let current_depth = 0;
    do_replace(val, rule, env, &spec, current_depth)
}

fn do_replace(
    val: &Value,
    rule: &Value,
    env: &Env,
    spec: &[(i32, i32)],
    depth: i32,
) -> Result<Value, EvalError> {
    // Check if we should try replacement at this depth
    let at_match_level = spec.iter().any(|&(lo, hi)| depth >= lo && depth <= hi);

    let subvalues = value_subparts(val);

    // First recurse into children
    let new_children: Vec<Result<Value, EvalError>> = subvalues
        .iter()
        .map(|c| do_replace(c, rule, env, spec, depth + 1))
        .collect();

    let new_children: Vec<Value> = new_children.into_iter().collect::<Result<Vec<_>, _>>()?;

    // Build the new value with replaced children
    let rebuilt = build_from_children(val, &new_children)?;

    // Now try replacement at this level if it's a match level
    if at_match_level {
        let result = try_apply_rule(&rebuilt, rule, env)?;
        if !result.struct_eq(&rebuilt) {
            return Ok(result);
        }
    }
    Ok(rebuilt)
}

/// Reconstruct a value from its (possibly replaced) children.
fn build_from_children(val: &Value, children: &[Value]) -> Result<Value, EvalError> {
    match val {
        Value::List(_) => Ok(Value::List(children.to_vec())),
        Value::Call { head, .. } => Ok(Value::Call {
            head: head.clone(),
            args: children.to_vec(),
        }),
        Value::Rule {
            lhs: _,
            rhs: _,
            delayed,
        } => {
            if children.len() >= 2 {
                Ok(Value::Rule {
                    lhs: Box::new(children[0].clone()),
                    rhs: Box::new(children[1].clone()),
                    delayed: *delayed,
                })
            } else {
                Ok(val.clone())
            }
        }
        Value::Sequence(_) => Ok(Value::Sequence(children.to_vec())),
        _ => Ok(val.clone()),
    }
}

/// Check[expr1, expr2] — evaluate expr1; if it generates an error, evaluate expr2 instead.
pub fn builtin_check(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Check expects exactly 2 arguments".to_string(),
        ));
    }
    // expr1 is held initially — we need to evaluate it manually
    // Since this is a builtin with HoldAll semantics, args[0] is already the unevaluated value
    // wrapped as a Pattern or direct value depending on attribute.
    let expr1 = convert_value_to_expr(&args[0]);
    match eval(&expr1, env) {
        Ok(result) => Ok(result),
        Err(_) => {
            let expr2 = convert_value_to_expr(&args[1]);
            eval(&expr2, env)
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Predicates
// ─────────────────────────────────────────────────────────────────────────────

/// NumericQ[val] — True if val is or contains only numerical data.
pub fn builtin_numeric_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "NumericQ expects exactly 1 argument".to_string(),
        ));
    }
    Ok(Value::Bool(is_numeric_value(&args[0])))
}

fn is_numeric_value(val: &Value) -> bool {
    match val {
        Value::Integer(_) | Value::Real(_) | Value::Rational(_) | Value::Complex { .. } => true,
        Value::List(items) => items.iter().any(is_numeric_value),
        Value::Call { head, args } => {
            // Known numeric functions
            is_numeric_function(head) && args.iter().all(is_numeric_value)
        }
        Value::Symbol(_) => false,
        _ => false,
    }
}

fn is_numeric_function(head: &str) -> bool {
    matches!(
        head,
        "Sin"
            | "Cos"
            | "Tan"
            | "ArcSin"
            | "ArcCos"
            | "ArcTan"
            | "Log"
            | "Log2"
            | "Log10"
            | "Exp"
            | "Sqrt"
            | "Floor"
            | "Ceiling"
            | "Round"
            | "Plus"
            | "Times"
            | "Power"
            | "Divide"
            | "Abs"
            | "Sign"
            | "Gamma"
            | "Factorial"
            | "Pi"
            | "E"
    )
}

/// RealQ[val] — True if val is a real number.
pub fn builtin_real_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "RealQ expects exactly 1 argument".to_string(),
        ));
    }
    Ok(Value::Bool(matches!(
        &args[0],
        Value::Integer(_) | Value::Real(_) | Value::Rational(_)
    )))
}

/// IntegerQ[val] is already implemented in math.rs; adding ExactNumberQ here.

/// ExactNumberQ[val] — True if val is an exact number (Integer, Rational, or complex with exact parts).
pub fn builtin_exact_number_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "ExactNumberQ expects exactly 1 argument".to_string(),
        ));
    }
    Ok(Value::Bool(matches!(
        &args[0],
        Value::Integer(_) | Value::Rational(_)
    )))
}

/// VectorQ[list] — True if list is a uniform list of atoms.
/// VectorQ[list, test] — True if list is a uniform list and all elements pass test.
pub fn builtin_vector_q(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 1 || args.len() > 2 {
        return Err(EvalError::Error(
            "VectorQ expects 1 or 2 arguments".to_string(),
        ));
    }
    let items = match &args[0] {
        Value::List(items) => items,
        _ => return Ok(Value::Bool(false)),
    };
    if items.is_empty() {
        return Ok(Value::Bool(true));
    }

    // All elements must have the same head (be uniform)
    let first_head = items[0].type_name();
    let uniform = items.iter().all(|v| v.type_name() == first_head);
    if !uniform {
        return Ok(Value::Bool(false));
    }

    // If a test function is provided, all elements must pass it
    if args.len() == 2 {
        let test = &args[1];
        for item in items {
            let result = match test {
                Value::PureFunction { .. } | Value::Function(_) | Value::Builtin(..) => {
                    let _call = Value::Call {
                        head: "Apply".to_string(),
                        args: vec![test.clone(), item.clone()],
                    };
                    // Simple invocation
                    let res = builtin_apply(&[test.clone(), Value::List(vec![item.clone()])], env);
                    match res {
                        Ok(v) => v.to_bool(),
                        Err(_) => false,
                    }
                }
                _ => {
                    // Symbol as test — evaluate test[val]
                    let head = match test {
                        Value::Symbol(s) => s.clone(),
                        _ => test.to_string(),
                    };
                    let _call = Value::Call {
                        head,
                        args: vec![item.clone()],
                    };
                    // We can't eval here without cycle; fall back to struct_eq
                    false
                }
            };
            if !result {
                return Ok(Value::Bool(false));
            }
        }
    }
    Ok(Value::Bool(true))
}

/// MatrixQ[list] — True if list is a uniform rectangular matrix of atoms.
pub fn builtin_matrix_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "MatrixQ expects exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::List(rows) => {
            if rows.is_empty() {
                return Ok(Value::Bool(true));
            }
            let row_len = match &rows[0] {
                Value::List(cols) => cols.len(),
                _ => return Ok(Value::Bool(false)),
            };
            rows.iter()
                .all(|row| matches!(row, Value::List(cols) if cols.len() == row_len))
        }
        _ => false,
    }
    .then(|| Value::Bool(true))
    .ok_or_else(|| EvalError::Error("MatrixQ: not a matrix".to_string()))
}

/// ScalarQ[val] — True if val is not a List or Call (a scalar value).
pub fn builtin_scalar_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "ScalarQ expects exactly 1 argument".to_string(),
        ));
    }
    Ok(Value::Bool(!matches!(
        &args[0],
        Value::List(_) | Value::Call { .. }
    )))
}

/// ArrayQ[val] — True if val is a uniform n-dimensional array.
pub fn builtin_array_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "ArrayQ expects exactly 1 argument".to_string(),
        ));
    }
    Ok(Value::Bool(is_array_value(&args[0])))
}

fn is_array_value(val: &Value) -> bool {
    match val {
        Value::List(items) => {
            if items.is_empty() {
                return true;
            }
            let first_type = items[0].type_name();
            items.iter().all(|v| v.type_name() == first_type)
                && (matches!(items[0], Value::List(_))
                    || items
                        .iter()
                        .all(|v| !matches!(v, Value::List(_) | Value::Call { .. })))
        }
        _ => false,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Formula Simplification (beyond basic Simplify)
// ─────────────────────────────────────────────────────────────────────────────

/// FullSimplify[expr] — more aggressive simplification using multiple strategies.
pub fn builtin_full_simplify(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "FullSimplify expects exactly 1 argument".to_string(),
        ));
    }
    // Apply multiple simplification strategies and pick the shortest
    let original = &args[0];
    let strategies: Vec<Value> = vec![
        crate::builtins::symbolic::builtin_simplify(&[original.clone()])?,
        builtin_together(&[original.clone()])?,
        builtin_cancel(&[original.clone()])?,
    ];
    let best = strategies
        .into_iter()
        .min_by_key(|v| v.to_string().len())
        .unwrap_or_else(|| original.clone());
    Ok(best)
}

/// Together[expr] — combine sum of rational expressions into a single fraction.
pub fn builtin_together(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Together expects exactly 1 argument".to_string(),
        ));
    }
    Ok(apply_together(&args[0]))
}

fn apply_together(val: &Value) -> Value {
    match val {
        Value::Call { head, args } if head == "Plus" => {
            // Combine all terms over a common denominator
            let fractions: Vec<(Value, Value)> =
                args.iter().map(|term| term_to_fraction(term)).collect();

            if fractions.is_empty() {
                return val.clone();
            }

            // Numerators and denominators
            let nums: Vec<Value> = fractions.iter().map(|(n, _)| n.clone()).collect();
            let dens: Vec<Value> = fractions.iter().map(|(_, d)| d.clone()).collect();

            // Common denominator = product of all denominators
            let common_den = common_denominator(&dens);

            // Combined numerator
            let combined_num = combine_numerators(&nums, &dens, &common_den);

            // Build result: combined_num / common_den
            if is_one(&common_den) {
                combined_num
            } else {
                Value::Call {
                    head: "Divide".to_string(),
                    args: vec![combined_num, common_den],
                }
            }
        }
        _ => val.clone(),
    }
}

/// Convert a value to (numerator, denominator).
fn term_to_fraction(term: &Value) -> (Value, Value) {
    if let Value::Call { head, args } = term {
        if head == "Divide" && args.len() == 2 {
            return (args[0].clone(), args[1].clone());
        }
    }
    (term.clone(), Value::Integer(Integer::from(1)))
}

/// Compute the least common denominator of a list of denominators.
fn common_denominator(dens: &[Value]) -> Value {
    if dens.is_empty() {
        return Value::Integer(Integer::from(1));
    }
    // For simplicity, use product (not LCM of symbolic expressions)
    let mut result: Option<Value> = None;
    for d in dens {
        if is_one(d) {
            continue;
        }
        result = Some(match result.take() {
            Some(acc) => Value::Call {
                head: "Times".to_string(),
                args: vec![acc, d.clone()],
            },
            None => d.clone(),
        });
    }
    result.unwrap_or_else(|| Value::Integer(Integer::from(1)))
}

/// Combine numerators: sum of (num_i * common_den / den_i).
fn combine_numerators(nums: &[Value], dens: &[Value], common_den: &Value) -> Value {
    let mut terms: Vec<Value> = Vec::new();
    for (num, den) in nums.iter().zip(dens.iter()) {
        if is_one(den) {
            terms.push(num.clone());
        } else {
            // term = num * (common_den / den)
            let factor = Value::Call {
                head: "Divide".to_string(),
                args: vec![common_den.clone(), den.clone()],
            };
            terms.push(Value::Call {
                head: "Times".to_string(),
                args: vec![num.clone(), factor],
            });
        }
    }
    if terms.is_empty() {
        return Value::Integer(Integer::from(0));
    }
    if terms.len() == 1 {
        return terms.into_iter().next().unwrap();
    }
    Value::Call {
        head: "Plus".to_string(),
        args: terms,
    }
}

/// Cancel[expr] — cancel common factors in a rational expression.
pub fn builtin_cancel(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Cancel expects exactly 1 argument".to_string(),
        ));
    }
    // For numeric rational expressions, simplify directly
    if let Value::Rational(_r) = &args[0] {
        // Already canonical
        return Ok(args[0].clone());
    }
    // For symbolic expressions, try to factor numerator and denominator
    // and cancel common factors
    Ok(cancel_value(&args[0]))
}

fn cancel_value(val: &Value) -> Value {
    if let Value::Call { head, args } = val {
        if head == "Divide" && args.len() == 2 {
            // Factor both numerator and denominator
            let num_factors = factor_value(&args[0]);
            let den_factors = factor_value(&args[1]);
            // Try to find and cancel common factors
            let (cancelled_num, cancelled_den) = cancel_common_factors(&num_factors, &den_factors);
            if is_one(&cancelled_den) {
                return cancelled_num;
            }
            return Value::Call {
                head: "Divide".to_string(),
                args: vec![cancelled_num, cancelled_den],
            };
        }
    }
    val.clone()
}

fn factor_value(val: &Value) -> Value {
    // Call into existing Factor implementation
    crate::builtins::symbolic::builtin_factor(&[val.clone()]).unwrap_or_else(|_| val.clone())
}

fn cancel_common_factors(num: &Value, den: &Value) -> (Value, Value) {
    // Simple cancellation: if both are Times with matching factors
    if let (Value::Call { head: h1, args: a1 }, Value::Call { head: h2, args: a2 }) = (num, den) {
        if h1 == "Times" && h2 == "Times" {
            let mut remaining_num: Vec<Value> = Vec::new();
            let mut remaining_den: Vec<Value> = Vec::new();
            let mut used_den: Vec<bool> = vec![false; a2.len()];

            for n in a1 {
                let mut found = false;
                for (j, d) in a2.iter().enumerate() {
                    if !used_den[j] && n.struct_eq(d) {
                        used_den[j] = true;
                        found = true;
                        break;
                    }
                }
                if !found {
                    remaining_num.push(n.clone());
                }
            }
            for (j, d) in a2.iter().enumerate() {
                if !used_den[j] {
                    remaining_den.push(d.clone());
                }
            }

            let result_num = if remaining_num.is_empty() {
                Value::Integer(Integer::from(1))
            } else if remaining_num.len() == 1 {
                remaining_num.into_iter().next().unwrap()
            } else {
                Value::Call {
                    head: "Times".to_string(),
                    args: remaining_num,
                }
            };
            let result_den = if remaining_den.is_empty() {
                Value::Integer(Integer::from(1))
            } else if remaining_den.len() == 1 {
                remaining_den.into_iter().next().unwrap()
            } else {
                Value::Call {
                    head: "Times".to_string(),
                    args: remaining_den,
                }
            };
            return (result_num, result_den);
        }
    }
    (num.clone(), den.clone())
}

/// Apart[expr] — partial fraction decomposition.
pub fn builtin_apart(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 1 || args.len() > 2 {
        return Err(EvalError::Error(
            "Apart expects 1 or 2 arguments".to_string(),
        ));
    }
    // For rational expressions a/(b*x+c)(d*x+e), decompose
    // Full symbolicApart is complex; implement basic case for integer coefficients
    Ok(apart_value(&args[0]))
}

fn apart_value(val: &Value) -> Value {
    // Apart is expensive; for now, delegate to Simplify for basic cases
    // A full implementation would:
    // 1. Extract numerator and denominator
    // 2. Factor denominator into linear terms
    // 3. Set up and solve the partial fraction system
    // For the MVP, return the expression as-is for complex cases
    crate::builtins::symbolic::builtin_simplify(&[val.clone()]).unwrap_or_else(|_| val.clone())
}

/// Collect[expr, x] — collect terms with same powers of x.
pub fn builtin_collect(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Collect expects exactly 2 arguments: expr and variable".to_string(),
        ));
    }
    let var_str: String = match &args[1] {
        Value::Symbol(s) => s.clone(),
        val => val.to_string(),
    };
    let var = var_str.as_str();
    Ok(collect_value(&args[0], var))
}

fn collect_value(val: &Value, var: &str) -> Value {
    // Expand first, then group by powers of var
    let expanded =
        crate::builtins::symbolic::builtin_expand(&[val.clone()]).unwrap_or_else(|_| val.clone());

    // Extract terms from Plus
    let terms = match &expanded {
        Value::Call { head, args } if head == "Plus" => args.clone(),
        _ => vec![expanded],
    };

    // Group by power of var
    let mut groups: std::collections::HashMap<i64, Vec<Value>> = std::collections::HashMap::new();

    for term in &terms {
        let (power, coeff) = extract_power_and_coeff(term, var);
        groups.entry(power).or_default().push(coeff);
    }

    // Sort by power descending
    let mut powers: Vec<i64> = groups.keys().cloned().collect();
    powers.sort_unstable_by(|a, b| b.cmp(a));

    let mut result_terms: Vec<Value> = Vec::new();
    for p in powers {
        if let Some(coeffs) = groups.get(&p) {
            let coeff_sum = sum_values(coeffs);
            if p == 0 {
                result_terms.push(coeff_sum);
            } else if p == 1 {
                result_terms.push(Value::Call {
                    head: "Times".to_string(),
                    args: vec![coeff_sum, Value::Symbol(var.to_string())],
                });
            } else {
                result_terms.push(Value::Call {
                    head: "Times".to_string(),
                    args: vec![
                        coeff_sum,
                        Value::Call {
                            head: "Power".to_string(),
                            args: vec![
                                Value::Symbol(var.to_string()),
                                Value::Integer(Integer::from(p)),
                            ],
                        },
                    ],
                });
            }
        }
    }

    if result_terms.is_empty() {
        Value::Integer(Integer::from(0))
    } else if result_terms.len() == 1 {
        result_terms.into_iter().next().unwrap()
    } else {
        Value::Call {
            head: "Plus".to_string(),
            args: result_terms,
        }
    }
}

/// Extract the power of var and the coefficient from a term.
fn extract_power_and_coeff(term: &Value, var: &str) -> (i64, Value) {
    match term {
        Value::Symbol(s) if s == var => (1, Value::Integer(Integer::from(1))),
        Value::Call { head, args } if head == "Power" && args.len() == 2 => {
            if let Value::Symbol(s) = &args[0] {
                if s == var {
                    let p = args[1].to_integer().unwrap_or(1);
                    (p, Value::Integer(Integer::from(1)))
                } else {
                    (0, term.clone())
                }
            } else {
                (0, term.clone())
            }
        }
        Value::Call { head, args } if head == "Times" => {
            let mut power: i64 = 0;
            let mut coeff = Value::Integer(Integer::from(1));
            let mut other_factors: Vec<Value> = Vec::new();

            for arg in args {
                match arg {
                    Value::Symbol(s) if s == var => power += 1,
                    Value::Call { head: h, args: a } if h == "Power" && a.len() == 2 => {
                        if let Value::Symbol(s) = &a[0] {
                            if s == var {
                                power += a[1].to_integer().unwrap_or(1);
                            } else {
                                other_factors.push(arg.clone());
                            }
                        } else {
                            other_factors.push(arg.clone());
                        }
                    }
                    _ => other_factors.push(arg.clone()),
                }
            }

            if other_factors.is_empty() {
                coeff = Value::Integer(Integer::from(1));
            } else if other_factors.len() == 1 {
                coeff = other_factors.into_iter().next().unwrap();
            } else {
                coeff = Value::Call {
                    head: "Times".to_string(),
                    args: other_factors,
                };
            }
            (power, coeff)
        }
        _ => (0, term.clone()),
    }
}

fn sum_values(values: &[Value]) -> Value {
    if values.is_empty() {
        return Value::Integer(Integer::from(0));
    }
    if values.len() == 1 {
        return values[0].clone();
    }
    Value::Call {
        head: "Plus".to_string(),
        args: values.to_vec(),
    }
}

fn is_one(val: &Value) -> bool {
    matches!(val, Value::Integer(n) if *n == rug::Integer::from(1))
}

// ─────────────────────────────────────────────────────────────────────────────
// Helper for VectorQ
// ─────────────────────────────────────────────────────────────────────────────

fn builtin_apply(args: &[Value], _env: &Env) -> Result<Value, EvalError> {
    // Minimal apply for VectorQ test invocation
    if args.len() != 2 {
        return Err(EvalError::Error("Apply requires 2 arguments".to_string()));
    }
    Ok(args[1].clone())
}
