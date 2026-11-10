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
use crate::pattern::{AttributeChecker, MatchResult, match_pattern};
use crate::value::{EvalError, Value};
use rug::{Integer, Rational};

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

/// Rewrite negative (from-the-bottom) level specs into absolute depths.
///
/// Wolfram-style level `-1` denotes the leaves, `-2` their parents, and so on.
/// `compute_depth` returns the maximum depth, so `-1` maps to that depth.
fn normalize_level_spec(spec: Vec<(i32, i32)>, expr: &Value) -> Vec<(i32, i32)> {
    let max_depth = compute_depth(expr) as i32;
    spec.into_iter()
        .map(|(lo, hi)| {
            let abs = |l: i32| if l < 0 { (max_depth + 1 + l).max(0) } else { l };
            (abs(lo), abs(hi))
        })
        .collect()
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
    let spec = normalize_level_spec(spec, &args[0]);
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
    let spec = normalize_level_spec(spec, &args[0]);
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
    let (lhs, rhs, delayed) = match rule {
        Value::Rule { lhs, rhs, delayed } => (lhs.as_ref(), rhs.as_ref(), *delayed),
        _ => return Ok(val.clone()),
    };

    // A rule's sides are held as `Pattern` expressions when it comes from the
    // evaluator, but may also be plain values when constructed directly.
    let matched = match lhs {
        Value::Pattern(pat_expr) => {
            let checker = AttributeChecker::new(env.attributes.clone());
            matches!(
                match_pattern(pat_expr, val, Some(&checker)),
                MatchResult::Match(_)
            )
        }
        _ => val.struct_eq(lhs),
    };
    if !matched {
        return Ok(val.clone());
    }

    // Immediate rules store a held rhs too, so evaluate it in both cases.
    match rhs {
        Value::Pattern(_) => eval(&convert_value_to_expr(rhs), env),
        _ if delayed => eval(&convert_value_to_expr(rhs), env),
        _ => Ok(rhs.clone()),
    }
}

/// Convert a Value back to an Expr for evaluation.
fn convert_value_to_expr(val: &Value) -> Expr {
    match val {
        // Held arguments arrive wrapped as `Pattern`; evaluate the inner expr.
        Value::Pattern(expr) => expr.clone(),
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
        // Every element must be numeric, not merely one of them.
        Value::List(items) => items.iter().all(is_numeric_value),
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

// `ExactNumberQ`, `VectorQ`, `MatrixQ` and `ArrayQ` are owned by `predicates`
// (registered last), so only the non-duplicated `ScalarQ` remains here.

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

// ─────────────────────────────────────────────────────────────────────────────
// Formula Simplification (beyond basic Simplify)
// ─────────────────────────────────────────────────────────────────────────────

/// FullSimplify[expr] — aggressive simplification.
///
/// Runs many transformation strategies (basic `Simplify`, `Together`,
/// `Cancel`, `Apart`, `Expand`, `Factor`, like-term combination, and common
/// trig identities), recurses bottom-up, and keeps the shortest equivalent
/// form it can find. It always seeds the search with `Simplify` so the result
/// is never weaker than plain simplification.
pub fn builtin_full_simplify(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Err(EvalError::Error(
            "FullSimplify expects 1 or 2 arguments".to_string(),
        ));
    }
    let simplified = full_simplify_value(&args[0]);
    if args.len() == 2 {
        let assum = crate::builtins::symbolic::effective_assumptions(Some(&args[1]), &Env::new());
        if crate::builtins::symbolic::assumptions_present(&assum) {
            return Ok(refine_full_simplify(&simplified, &assum));
        }
    }
    Ok(simplified)
}

/// Environment-aware `FullSimplify`: also honours `$Assumptions` (from
/// `Assuming` or a global setting).
pub fn builtin_full_simplify_env(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Err(EvalError::Error(
            "FullSimplify expects 1 or 2 arguments".to_string(),
        ));
    }
    let simplified = full_simplify_value(&args[0]);
    let assum = crate::builtins::symbolic::effective_assumptions(args.get(1), env);
    if !crate::builtins::symbolic::assumptions_present(&assum) {
        return Ok(simplified);
    }
    Ok(refine_full_simplify(&simplified, &assum))
}

/// Refine under assumptions, then re-run `FullSimplify` so the refinement can
/// expose further reductions (and vice-versa), keeping the shorter form.
///
/// The initial assumption-driven refinement is always kept: `Refine` is the
/// component that knows about assumptions, and its output can have a larger
/// leaf count yet still be the intended form (e.g. `Abs[x] -> -x` for `x < 0`).
fn refine_full_simplify(val: &Value, assum: &Value) -> Value {
    let refined = crate::builtins::domains::refine_expr(val, assum).unwrap_or_else(|_| val.clone());
    let mut best = full_simplify_value(&refined);
    for _ in 0..3 {
        let refined =
            crate::builtins::domains::refine_expr(&best, assum).unwrap_or_else(|_| best.clone());
        let cleaned = full_simplify_value(&refined);
        let cleaned = crate::builtins::domains::refine_expr(&cleaned, assum).unwrap_or(cleaned);
        if cleaned == best {
            break;
        }
        if value_complexity(&cleaned) < value_complexity(&best) {
            best = cleaned;
        } else {
            break;
        }
    }
    best
}

/// Recursively full-simplify `val`, descending into subexpressions first.
fn full_simplify_value(val: &Value) -> Value {
    let lowered = match val {
        Value::Call { head, args } => Value::Call {
            head: head.clone(),
            args: args.iter().map(full_simplify_value).collect(),
        },
        Value::List(items) => Value::List(items.iter().map(full_simplify_value).collect()),
        Value::Sequence(items) => Value::Sequence(items.iter().map(full_simplify_value).collect()),
        Value::Rule { lhs, rhs, delayed } => Value::Rule {
            lhs: Box::new(full_simplify_value(lhs)),
            rhs: Box::new(full_simplify_value(rhs)),
            delayed: *delayed,
        },
        _ => val.clone(),
    };
    best_simplification(&lowered)
}

/// Maximum number of refinement sweeps in `best_simplification`: bounded and
/// small, but deep enough for the rewrite chains these strategies trigger.
const FULL_SIMPLIFY_SWEEPS: usize = 6;

thread_local! {
    static FS_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// Repeatedly apply candidate rewrites, keeping only strictly simpler forms,
/// until a fixpoint or a small iteration bound.
///
/// The incumbent starts at `Simplify[val]` and a candidate must have strictly
/// lower [`value_complexity`] to replace it, so the result is *never* more
/// complex than plain `Simplify`.
fn best_simplification(val: &Value) -> Value {
    let depth = FS_DEPTH.get();
    if depth > 512 {
        return base_simplify(val);
    }
    FS_DEPTH.set(depth + 1);
    let mut best = base_simplify(val);
    for _ in 0..FULL_SIMPLIFY_SWEEPS {
        let mut improved = false;
        for cand in simplification_candidates(&best) {
            if value_complexity(&cand) < value_complexity(&best) {
                best = cand;
                improved = true;
            }
        }
        if !improved {
            break;
        }
    }
    FS_DEPTH.set(depth);
    best
}

/// Baseline `Simplify` used as a seed and to normalise every candidate.
fn base_simplify(val: &Value) -> Value {
    crate::builtins::symbolic::builtin_simplify(&[val.clone()]).unwrap_or_else(|_| val.clone())
}

/// Total-order complexity used to rank candidate rewrites: weighted operation
/// count first, then leaf count, then textual length.
///
/// Weighting the expensive heads makes the search prefer a canonical compact
/// form (e.g. `Cos[2 x]`) over an identity-expanded one
/// (`Cos[x]^2 - Sin[x]^2`), rather than merely the shortest string.
fn value_complexity(val: &Value) -> (usize, usize, usize) {
    (weighted_cost(val), count_leaves(val), val.to_string().len())
}

/// Weighted node count. `Plus`/`Times` are cheap; `Power`/`Sqrt`/`Log`/`Exp`
/// cost more; trigonometry costs most so identities that remove trig win.
fn weighted_cost(val: &Value) -> usize {
    match val {
        Value::Call { head, args } => {
            let head_cost = match head.as_str() {
                "Plus" | "Times" => 1,
                "Power" | "Sqrt" | "Log" | "Exp" => 2,
                "Sin" | "Cos" | "Tan" | "Cot" | "Sec" | "Csc" | "Sinh" | "Cosh" | "Tanh" => 3,
                _ => 2,
            };
            head_cost + args.iter().map(weighted_cost).sum::<usize>()
        }
        Value::List(items) | Value::Sequence(items) => {
            1 + items.iter().map(weighted_cost).sum::<usize>()
        }
        Value::Rule { lhs, rhs, .. } => 1 + weighted_cost(lhs) + weighted_cost(rhs),
        _ => 1,
    }
}

/// Build every candidate rewrite of `val`, each normalised through `Simplify`.
fn simplification_candidates(val: &Value) -> Vec<Value> {
    use crate::builtins::symbolic;
    use crate::builtins::symbolicmanip;

    let mut out: Vec<Value> = Vec::new();
    let mut push = |candidate: Value| {
        let normalised = base_simplify(&candidate);
        if !normalised.struct_eq(val) {
            out.push(normalised);
        }
    };

    // Combine sums of fractions over a common denominator.
    let mut together: Option<Value> = None;
    if let Ok(v) = symbolicmanip::builtin_together(&[val.clone()]) {
        push(v.clone());
        together = Some(v);
    }
    // Cancel common factors in rational expressions. Done locally because the
    // registered `Cancel` performs unsound polynomial division when a second
    // symbol appears (e.g. Cancel[(x + y)/(x y)] == 1/x).
    push(full_cancel(val));
    // Partial-fraction decomposition (pairs with Together).
    if let Ok(v) = symbolicmanip::builtin_apart(&[val.clone()]) {
        push(v);
    }
    // Distribute products over sums, then re-simplify.
    if let Ok(expanded) = symbolic::builtin_expand(&[val.clone()]) {
        push(expanded);
    }
    // Factor a genuine polynomial (the expanded form avoids `Factor`'s
    // mishandling of nested unexpanded products), then cancel any common
    // factor the factorisation exposes.
    if let Some(factored) = try_factor_polynomial(val) {
        push(factored.clone());
        push(full_cancel(&factored));
    }
    // Combine fractions (robust to `Times[…] * Divide[…]` terms), then cancel
    // the common denominator that introduced.
    let owned = together_value(val);
    if !owned.struct_eq(val) {
        push(owned);
    }
    if let Some(combined) = together {
        push(full_cancel(&combined));
    }
    // Pythagorean identity Sin[u]^2 + Cos[u]^2 -> 1.
    push(pythagorean_reduce(val));
    // Expand double angles so they can cancel against each other.
    push(expand_double_angles(val));
    // Log/exp normalisation: `Exp[a] Exp[b] -> Exp[a + b]`,
    // `Log[a] + Log[b] -> Log[a b]`, and their quotients.
    push(logexp_reduce(val));
    // Radical reduction: `Sqrt[u]^2 -> u` (and even roots of even powers).
    push(radical_reduce(val));
    // Route Tan/Cot/Sec/Csc through Sin/Cos so identities such as
    // `Tan[x] - Sin[x]/Cos[x]` can cancel.
    push(trig_to_sincos(val));

    out
}

/// Cancel common factors in the numerator/denominator of `val`.
///
/// Works by factorising both sides (when they are genuine polynomials) and
/// removing matching factors, then attempting an exact polynomial division as
/// a fallback. This is more reliable than the registered `Cancel`.
fn full_cancel(val: &Value) -> Value {
    let (num, den) = match val {
        Value::Call { head, args } if head == "Divide" && args.len() == 2 => {
            (args[0].clone(), args[1].clone())
        }
        _ => return base_simplify(val),
    };
    if is_zero_value(&base_simplify(&num)) {
        return Value::Integer(Integer::from(0));
    }
    if is_one(&den) {
        return base_simplify(&num);
    }

    let num_f = base_simplify(&try_factor_polynomial(&num).unwrap_or(num));
    let den_f = base_simplify(&try_factor_polynomial(&den).unwrap_or(den));
    let (cancelled_num, cancelled_den) = cancel_factor_lists(&num_f, &den_f);

    if is_one(&cancelled_den) {
        return base_simplify(&cancelled_num);
    }
    if let Some(quotient) = try_exact_poly_divide(&cancelled_num, &cancelled_den) {
        return base_simplify(&quotient);
    }
    base_simplify(&Value::Call {
        head: "Divide".to_string(),
        args: vec![cancelled_num, cancelled_den],
    })
}

/// Combine a sum of rational terms over a common denominator and cancel.
///
/// Unlike the registered `Together`, this understands terms shaped like
/// `Times[-1, Divide[a, b]]` (a fraction multiplied by extra factors), which
/// otherwise get left inside the numerator.
fn together_value(val: &Value) -> Value {
    let Value::Call { head, args } = val else {
        return val.clone();
    };
    if head != "Plus" {
        return full_cancel(val);
    }
    if !args.iter().any(term_has_denominator) {
        return val.clone();
    }

    let fractions: Vec<(Value, Value)> = args.iter().map(as_fraction).collect();
    let dens: Vec<Value> = fractions.iter().map(|(_, d)| d.clone()).collect();
    let common = build_product(dens.iter().filter(|d| !is_one(d)).cloned());

    let mut numerator_terms: Vec<Value> = Vec::new();
    for (n, d) in &fractions {
        let scale = if is_one(d) {
            Value::Integer(Integer::from(1))
        } else {
            base_simplify(&Value::Call {
                head: "Divide".to_string(),
                args: vec![common.clone(), d.clone()],
            })
        };
        numerator_terms.push(Value::Call {
            head: "Times".to_string(),
            args: vec![n.clone(), scale],
        });
    }
    let numerator = build_sum(numerator_terms);
    full_cancel(&Value::Call {
        head: "Divide".to_string(),
        args: vec![base_simplify(&numerator), base_simplify(&common)],
    })
}

/// True if `term` contains a fraction factor (a `Divide` or negative power).
fn term_has_denominator(term: &Value) -> bool {
    match term {
        Value::Call { head, args } if head == "Divide" && args.len() == 2 => true,
        Value::Call { head, args } if head == "Power" && args.len() == 2 => {
            matches!(&args[1], Value::Integer(n) if n.is_negative())
        }
        Value::Call { head, args } if head == "Times" => args.iter().any(term_has_denominator),
        _ => false,
    }
}

/// Split a term into `(numerator, denominator)`.
fn as_fraction(term: &Value) -> (Value, Value) {
    match term {
        Value::Call { head, args } if head == "Divide" && args.len() == 2 => {
            (args[0].clone(), args[1].clone())
        }
        Value::Call { head, args } if head == "Times" => {
            let mut num: Vec<Value> = Vec::new();
            let mut den: Vec<Value> = Vec::new();
            for a in args {
                let (n, d) = as_fraction(a);
                num.push(n);
                if !is_one(&d) {
                    den.push(d);
                }
            }
            (
                build_product(num.into_iter()),
                build_product(den.into_iter()),
            )
        }
        Value::Call { head, args } if head == "Power" && args.len() == 2 => {
            if let Value::Integer(n) = &args[1]
                && n.is_negative()
            {
                let positive = -n.clone();
                let d = if positive == 1 {
                    args[0].clone()
                } else {
                    Value::Call {
                        head: "Power".to_string(),
                        args: vec![args[0].clone(), Value::Integer(positive)],
                    }
                };
                return (Value::Integer(Integer::from(1)), d);
            }
            (term.clone(), Value::Integer(Integer::from(1)))
        }
        _ => (term.clone(), Value::Integer(Integer::from(1))),
    }
}

fn build_product(values: impl Iterator<Item = Value>) -> Value {
    let items: Vec<Value> = values.collect();
    match items.len() {
        0 => Value::Integer(Integer::from(1)),
        1 => items.into_iter().next().unwrap(),
        _ => Value::Call {
            head: "Times".to_string(),
            args: items,
        },
    }
}

fn build_sum(values: Vec<Value>) -> Value {
    match values.len() {
        0 => Value::Integer(Integer::from(0)),
        1 => values.into_iter().next().unwrap(),
        _ => Value::Call {
            head: "Plus".to_string(),
            args: values,
        },
    }
}

/// Collect a product into `(base, exponent)` pairs (exponents may be negative).
fn collect_factors(val: &Value, factors: &mut Vec<(Value, Integer)>) {
    match val {
        Value::Call { head, args } if head == "Times" => {
            for a in args {
                collect_factors(a, factors);
            }
        }
        Value::Call { head, args } if head == "Power" && args.len() == 2 => {
            if let Value::Integer(n) = &args[1] {
                factors.push((args[0].clone(), n.clone()));
            } else {
                factors.push((val.clone(), Integer::from(1)));
            }
        }
        _ => factors.push((val.clone(), Integer::from(1))),
    }
}

/// Rebuild a product from `(base, exponent)` pairs (all exponents positive).
fn rebuild_factors(factors: &[(Value, Integer)]) -> Value {
    let mut terms: Vec<Value> = Vec::new();
    for (base, exp) in factors {
        if exp.is_zero() {
            continue;
        }
        if *exp == 1 {
            terms.push(base.clone());
        } else {
            terms.push(Value::Call {
                head: "Power".to_string(),
                args: vec![base.clone(), Value::Integer(exp.clone())],
            });
        }
    }
    match terms.len() {
        0 => Value::Integer(Integer::from(1)),
        1 => terms.into_iter().next().unwrap(),
        _ => Value::Call {
            head: "Times".to_string(),
            args: terms,
        },
    }
}

/// Remove matching factors shared by `num` and `den`.
fn cancel_factor_lists(num: &Value, den: &Value) -> (Value, Value) {
    let mut num_factors: Vec<(Value, Integer)> = Vec::new();
    let mut den_factors: Vec<(Value, Integer)> = Vec::new();
    collect_factors(num, &mut num_factors);
    collect_factors(den, &mut den_factors);

    let mut used = vec![false; den_factors.len()];
    for (nb, ne) in num_factors.iter_mut() {
        for (j, (db, de)) in den_factors.iter().enumerate() {
            if !used[j] && factor_eq(db, nb) {
                used[j] = true;
                *ne -= de;
                break;
            }
        }
    }
    for (j, (db, de)) in den_factors.iter().enumerate() {
        if !used[j] {
            num_factors.push((db.clone(), -de.clone()));
        }
    }

    let mut num_positive = Vec::new();
    let mut den_positive = Vec::new();
    for (base, exp) in num_factors {
        if exp.is_negative() {
            den_positive.push((base, -exp));
        } else {
            num_positive.push((base, exp));
        }
    }
    (
        rebuild_factors(&num_positive),
        rebuild_factors(&den_positive),
    )
}

/// Structural equality up to `Plus`/`Times` commutativity, used so that
/// factors like `x + 1` and `1 + x` are recognised as the same.
fn factor_eq(a: &Value, b: &Value) -> bool {
    if a.struct_eq(b) {
        return true;
    }
    match (a, b) {
        (Value::Call { head: ha, args: aa }, Value::Call { head: hb, args: ab })
            if (ha == "Plus" || ha == "Times") && ha == hb && aa.len() == ab.len() =>
        {
            let mut used = vec![false; ab.len()];
            aa.iter().all(|x| {
                for (j, y) in ab.iter().enumerate() {
                    if !used[j] && factor_eq(x, y) {
                        used[j] = true;
                        return true;
                    }
                }
                false
            })
        }
        _ => false,
    }
}

/// Exact polynomial division, returning `None` unless the remainder is zero.
fn try_exact_poly_divide(num: &Value, den: &Value) -> Option<Value> {
    use crate::builtins::symbolic;
    let num_e = symbolic::builtin_expand(&[num.clone()]).ok()?;
    let den_e = symbolic::builtin_expand(&[den.clone()]).ok()?;
    let var = find_free_symbol(&num_e).or_else(|| find_free_symbol(&den_e))?;
    if !is_polynomial_in(&num_e, &var) || !is_polynomial_in(&den_e, &var) {
        return None;
    }
    let num_coeffs = symbolic::extract_polynomial_coeffs(&num_e, &var);
    let den_coeffs = symbolic::extract_polynomial_coeffs(&den_e, &var);
    if den_coeffs.is_empty() || num_coeffs.len() < den_coeffs.len() {
        return None;
    }
    let den_deg = den_coeffs.len() - 1;
    let den_lead = den_coeffs[den_deg].clone();
    if is_zero_value(&den_lead) {
        return None;
    }

    let mut remainder = num_coeffs;
    let quot_deg = remainder.len() - 1 - den_deg;
    let mut quot: Vec<Value> = vec![Value::Integer(Integer::from(0)); quot_deg + 1];

    for i in (0..=quot_deg).rev() {
        let j = i + den_deg;
        let r = remainder[j].clone();
        let q = base_simplify(&Value::Call {
            head: "Divide".to_string(),
            args: vec![r.clone(), den_lead.clone()],
        });
        quot[i] = q.clone();
        for (k, den_k) in den_coeffs.iter().enumerate().take(den_deg + 1) {
            let idx = i + k;
            let sub = base_simplify(&Value::Call {
                head: "Times".to_string(),
                args: vec![q.clone(), den_k.clone()],
            });
            remainder[idx] = base_simplify(&Value::Call {
                head: "Plus".to_string(),
                args: vec![
                    remainder[idx].clone(),
                    Value::Call {
                        head: "Times".to_string(),
                        args: vec![Value::Integer(Integer::from(-1)), sub],
                    },
                ],
            });
        }
    }

    if !remainder.iter().all(is_zero_value) {
        return None;
    }

    // Rebuild the quotient polynomial.
    let mut terms: Vec<Value> = Vec::new();
    for (i, c) in quot.iter().enumerate() {
        if is_zero_value(c) {
            continue;
        }
        if i == 0 {
            terms.push(c.clone());
        } else {
            let power = if i == 1 {
                Value::Symbol(var.clone())
            } else {
                Value::Call {
                    head: "Power".to_string(),
                    args: vec![Value::Symbol(var.clone()), Value::Integer(Integer::from(i))],
                }
            };
            terms.push(Value::Call {
                head: "Times".to_string(),
                args: vec![c.clone(), power],
            });
        }
    }
    Some(match terms.len() {
        0 => Value::Integer(Integer::from(0)),
        1 => terms.into_iter().next().unwrap(),
        _ => Value::Call {
            head: "Plus".to_string(),
            args: terms,
        },
    })
}

fn is_zero_value(val: &Value) -> bool {
    matches!(val, Value::Integer(n) if n.is_zero())
}

/// Factor `val` only when it is a genuine polynomial in a single symbol.
///
/// The underlying `Factor` builtin silently drops terms that are not polynomial
/// in the chosen variable (e.g. `Factor[x + Sin[x]] == x`) and mis-handles
/// products of parenthesised factors (e.g. `Factor[x (x + 2)] == 0`). By
/// expanding first and vetting every term we avoid both traps.
fn try_factor_polynomial(val: &Value) -> Option<Value> {
    let expanded = crate::builtins::symbolic::builtin_expand(&[val.clone()]).ok()?;
    let var = find_free_symbol(&expanded)?;
    if !is_polynomial_in(&expanded, &var) {
        return None;
    }
    crate::builtins::symbolic::builtin_factor(&[expanded]).ok()
}

/// First symbol that is not a known constant.
fn find_free_symbol(val: &Value) -> Option<String> {
    match val {
        Value::Symbol(s) => {
            if matches!(s.as_str(), "Pi" | "E" | "I" | "True" | "False" | "Null") {
                None
            } else {
                Some(s.clone())
            }
        }
        Value::Call { args, .. } | Value::List(args) | Value::Sequence(args) => {
            args.iter().find_map(find_free_symbol)
        }
        _ => None,
    }
}

/// True if every additive term is a non-negative integer power of `var`
/// multiplied by a `var`-free coefficient.
fn is_polynomial_in(val: &Value, var: &str) -> bool {
    match val {
        Value::Call { head, args } if head == "Plus" => {
            !args.is_empty() && args.iter().all(|t| poly_degree(t, var).is_some())
        }
        _ => poly_degree(val, var).is_some(),
    }
}

/// Degree of a single additive term in `var`, or `None` if the term is not a
/// clean `coeff * var^k` (k >= 0, coeff free of `var`).
fn poly_degree(term: &Value, var: &str) -> Option<i64> {
    match term {
        Value::Symbol(s) if s == var => Some(1),
        Value::Integer(_) | Value::Real(_) | Value::Rational(_) | Value::Symbol(_) => Some(0),
        Value::Call { head, args } => match head.as_str() {
            "Power" if args.len() == 2 && args[0].struct_eq(&Value::Symbol(var.to_string())) => {
                match &args[1] {
                    Value::Integer(n) if !n.is_negative() => n.to_i64(),
                    _ => None,
                }
            }
            "Times" => {
                let mut degree = 0;
                for a in args {
                    degree += poly_degree(a, var)?;
                }
                Some(degree)
            }
            _ => {
                if contains_symbol(term, var) {
                    None
                } else {
                    Some(0)
                }
            }
        },
        _ => None,
    }
}

/// True if `val` mentions the symbol `s` anywhere.
fn contains_symbol(val: &Value, s: &str) -> bool {
    match val {
        Value::Symbol(x) => x == s,
        Value::Call { args, .. } | Value::List(args) | Value::Sequence(args) => {
            args.iter().any(|a| contains_symbol(a, s))
        }
        Value::Rule { lhs, rhs, .. } => contains_symbol(lhs, s) || contains_symbol(rhs, s),
        _ => false,
    }
}

/// Rewrite `Sin[u]^2 + Cos[u]^2 (+ …)` groups into a constant within a `Plus`.
///
/// Handles numeric coefficients as long as a matching `Sin`/`Cos` pair shares
/// the same coefficient, e.g. `2 Sin[u]^2 + 2 Cos[u]^2 -> 2`.
fn pythagorean_reduce(val: &Value) -> Value {
    let Value::Call { head, args } = val else {
        return val.clone();
    };
    if head != "Plus" {
        return val.clone();
    }

    let mut sins: Vec<(Value, Value)> = Vec::new();
    let mut coss: Vec<(Value, Value)> = Vec::new();
    let mut others: Vec<Value> = Vec::new();

    for term in args {
        match match_trig_square(term) {
            Some((true, u, c)) => sins.push((u, c)),
            Some((false, u, c)) => coss.push((u, c)),
            None => others.push(term.clone()),
        }
    }
    if sins.is_empty() || coss.is_empty() {
        return val.clone();
    }

    let mut used_cos = vec![false; coss.len()];
    let mut constants: Vec<Value> = Vec::new();
    let mut consumed_sin: Vec<bool> = vec![false; sins.len()];
    let mut matched = false;

    for i in 0..sins.len() {
        for j in 0..coss.len() {
            if !used_cos[j] && coss[j].0.struct_eq(&sins[i].0) && coss[j].1.struct_eq(&sins[i].1) {
                used_cos[j] = true;
                consumed_sin[i] = true;
                constants.push(sins[i].1.clone());
                matched = true;
                break;
            }
        }
    }
    if !matched {
        return val.clone();
    }

    for (i, (u, c)) in sins.iter().enumerate() {
        if !consumed_sin[i] {
            others.push(rebuild_trig_square(true, u, c));
        }
    }
    for (j, (u, c)) in coss.iter().enumerate() {
        if !used_cos[j] {
            others.push(rebuild_trig_square(false, u, c));
        }
    }
    others.extend(constants);

    match others.len() {
        0 => Value::Integer(Integer::from(0)),
        1 => others.into_iter().next().unwrap(),
        _ => Value::Call {
            head: "Plus".to_string(),
            args: others,
        },
    }
}

/// Match `Sin[u]^2` / `Cos[u]^2`, optionally with a numeric coefficient.
/// Returns `(is_sin, u, coefficient)`.
fn match_trig_square(term: &Value) -> Option<(bool, Value, Value)> {
    let (coeff, base) = split_numeric_coeff(term);
    let Value::Call { head, args } = &base else {
        return None;
    };
    if head != "Power" || args.len() != 2 || !matches!(&args[1], Value::Integer(n) if *n == 2) {
        return None;
    }
    let Value::Call { head: fh, args: fa } = &args[0] else {
        return None;
    };
    if fa.len() != 1 {
        return None;
    }
    match fh.as_str() {
        "Sin" => Some((true, fa[0].clone(), coeff)),
        "Cos" => Some((false, fa[0].clone(), coeff)),
        _ => None,
    }
}

/// Rebuild `coeff * Sin[u]^2` (or `Cos`).
fn rebuild_trig_square(is_sin: bool, u: &Value, coeff: &Value) -> Value {
    let sq = Value::Call {
        head: "Power".to_string(),
        args: vec![
            Value::Call {
                head: if is_sin { "Sin" } else { "Cos" }.to_string(),
                args: vec![u.clone()],
            },
            Value::Integer(Integer::from(2)),
        ],
    };
    if is_one(coeff) {
        sq
    } else {
        Value::Call {
            head: "Times".to_string(),
            args: vec![coeff.clone(), sq],
        }
    }
}

/// Split a product into its numeric coefficient and remaining base.
fn split_numeric_coeff(term: &Value) -> (Value, Value) {
    if let Value::Call { head, args } = term
        && head == "Times"
    {
        let mut coeff = Value::Integer(Integer::from(1));
        let mut rest: Vec<Value> = Vec::new();
        for a in args {
            match a {
                Value::Integer(_) | Value::Rational(_) => {
                    coeff =
                        crate::builtins::arithmetic::mul_values_public(&coeff, a).unwrap_or(coeff);
                }
                _ => rest.push(a.clone()),
            }
        }
        let base = match rest.len() {
            0 => Value::Integer(Integer::from(1)),
            1 => rest.into_iter().next().unwrap(),
            _ => Value::Call {
                head: "Times".to_string(),
                args: rest,
            },
        };
        return (coeff, base);
    }
    (Value::Integer(Integer::from(1)), term.clone())
}

/// Rewrite `Cos[2 u]` -> `Cos[u]^2 - Sin[u]^2` and `Sin[2 u]` -> `2 Sin[u] Cos[u]`.
fn expand_double_angles(val: &Value) -> Value {
    match val {
        Value::Call { head, args } => {
            let new_args: Vec<Value> = args.iter().map(expand_double_angles).collect();
            if new_args.len() == 1
                && (head == "Cos" || head == "Sin")
                && let Some(u) = match_double_angle(&new_args[0])
            {
                let squared = |f: &str| Value::Call {
                    head: "Power".to_string(),
                    args: vec![
                        Value::Call {
                            head: f.to_string(),
                            args: vec![u.clone()],
                        },
                        Value::Integer(Integer::from(2)),
                    ],
                };
                if head == "Cos" {
                    return Value::Call {
                        head: "Plus".to_string(),
                        args: vec![
                            squared("Cos"),
                            Value::Call {
                                head: "Times".to_string(),
                                args: vec![Value::Integer(Integer::from(-1)), squared("Sin")],
                            },
                        ],
                    };
                }
                return Value::Call {
                    head: "Times".to_string(),
                    args: vec![
                        Value::Integer(Integer::from(2)),
                        Value::Call {
                            head: "Sin".to_string(),
                            args: vec![u.clone()],
                        },
                        Value::Call {
                            head: "Cos".to_string(),
                            args: vec![u],
                        },
                    ],
                };
            }
            Value::Call {
                head: head.clone(),
                args: new_args,
            }
        }
        Value::List(items) => Value::List(items.iter().map(expand_double_angles).collect()),
        _ => val.clone(),
    }
}

/// If `u` is `2 * w` or `w * 2`, return `w`.
fn match_double_angle(u: &Value) -> Option<Value> {
    if let Value::Call { head, args } = u
        && head == "Times"
        && args.len() == 2
    {
        if let Value::Integer(n) = &args[0]
            && *n == 2
        {
            return Some(args[1].clone());
        }
        if let Value::Integer(n) = &args[1]
            && *n == 2
        {
            return Some(args[0].clone());
        }
    }
    None
}

// ─────────────────────────────────────────────────────────────────────────────
// Additional FullSimplify strategies: log/exp, radical, trig normalisation
// ─────────────────────────────────────────────────────────────────────────────

fn vint(n: i64) -> Value {
    Value::Integer(Integer::from(n))
}

fn vcall(head: &str, args: Vec<Value>) -> Value {
    Value::Call {
        head: head.to_string(),
        args,
    }
}

/// Bottom-up normalisation of exponentials and logarithms:
/// `Exp[a] Exp[b] -> Exp[a + b]`, `Exp[a]^n -> Exp[n a]`,
/// `Log[a] + Log[b] -> Log[a b]` (and quotients).
fn logexp_reduce(val: &Value) -> Value {
    match val {
        Value::List(items) => Value::List(items.iter().map(logexp_reduce).collect()),
        Value::Call { head, args } if head == "Times" => {
            let mapped: Vec<Value> = args.iter().map(logexp_reduce).collect();
            combine_exp_factors(&mapped)
        }
        Value::Call { head, args } if head == "Plus" => {
            let mapped: Vec<Value> = args.iter().map(logexp_reduce).collect();
            combine_log_terms(&mapped)
        }
        Value::Call { head, args } if head == "Divide" && args.len() == 2 => {
            let mapped: Vec<Value> = args.iter().map(logexp_reduce).collect();
            let as_factors = vec![
                mapped[0].clone(),
                vcall("Power", vec![mapped[1].clone(), vint(-1)]),
            ];
            combine_exp_factors(&as_factors)
        }
        Value::Call { head, args } => vcall(head, args.iter().map(logexp_reduce).collect()),
        _ => val.clone(),
    }
}

/// `Exp[u]`, `E^u`, or a power of either — return the exponent `u`.
fn as_exp_arg(v: &Value) -> Option<Value> {
    match v {
        Value::Call { head, args } if head == "Exp" && args.len() == 1 => Some(args[0].clone()),
        Value::Call { head, args }
            if head == "Power"
                && args.len() == 2
                && args[0].struct_eq(&Value::Symbol("E".into())) =>
        {
            Some(args[1].clone())
        }
        Value::Call { head, args } if head == "Power" && args.len() == 2 => {
            let u = as_exp_arg(&args[0])?;
            match &args[1] {
                Value::Integer(n) => Some(vcall("Times", vec![Value::Integer(n.clone()), u])),
                _ => None,
            }
        }
        _ => None,
    }
}

fn combine_exp_factors(factors: &[Value]) -> Value {
    let mut exp_args: Vec<Value> = Vec::new();
    let mut others: Vec<Value> = Vec::new();
    for f in factors {
        match as_exp_arg(f) {
            Some(u) => exp_args.push(u),
            None => others.push(f.clone()),
        }
    }
    if exp_args.len() < 2 {
        return vcall("Times", factors.to_vec());
    }
    let sum = if exp_args.len() == 1 {
        exp_args.pop().unwrap()
    } else {
        vcall("Plus", exp_args)
    };
    others.push(vcall("Exp", vec![sum]));
    if others.len() == 1 {
        others.pop().unwrap()
    } else {
        vcall("Times", others)
    }
}

/// True for `Log[u]` or `c * Log[u]`; returns `(u, c)`.
fn as_scaled_log(term: &Value) -> Option<(Value, Value)> {
    match term {
        Value::Call { head, args } if head == "Log" && args.len() == 1 => {
            Some((args[0].clone(), vint(1)))
        }
        Value::Call { head, args } if head == "Times" => {
            let mut coeff = vint(1);
            let mut arg: Option<Value> = None;
            for f in args {
                if let Value::Call { head: lh, args: la } = f
                    && lh == "Log"
                    && la.len() == 1
                {
                    arg = Some(la[0].clone());
                    continue;
                }
                coeff = vcall("Times", vec![coeff, f.clone()]);
            }
            arg.map(|a| (a, base_simplify(&coeff)))
        }
        _ => None,
    }
}

fn combine_log_terms(terms: &[Value]) -> Value {
    let mut combined: Vec<(Value, Value)> = Vec::new();
    let mut others: Vec<Value> = Vec::new();
    for t in terms {
        match as_scaled_log(t) {
            Some(pair) => combined.push(pair),
            None => others.push(t.clone()),
        }
    }
    if combined.len() < 2 {
        return vcall("Plus", terms.to_vec());
    }
    let factors: Vec<Value> = combined
        .into_iter()
        .map(|(arg, coeff)| {
            if is_one(&coeff) {
                arg
            } else {
                vcall("Power", vec![arg, coeff])
            }
        })
        .collect();
    others.push(vcall("Log", vec![vcall("Times", factors)]));
    if others.len() == 1 {
        others.pop().unwrap()
    } else {
        vcall("Plus", others)
    }
}

/// Bottom-up radical reduction. `Sqrt[u]^2 -> u` is valid for every complex
/// `u`; `Sqrt[b^(2k)] -> b^k` is applied only for `k >= 2` so that the
/// ambiguous `Sqrt[x^2]` is left intact (matching `Simplify`).
fn radical_reduce(val: &Value) -> Value {
    match val {
        Value::List(items) => Value::List(items.iter().map(radical_reduce).collect()),
        Value::Call { head, args } if head == "Power" && args.len() == 2 => {
            let base = radical_reduce(&args[0]);
            if matches!(&args[1], Value::Integer(n) if *n == 2)
                && let Some(inner) = as_sqrt_arg(&base)
            {
                return inner;
            }
            vcall("Power", vec![base, args[1].clone()])
        }
        Value::Call { head, args } if head == "Sqrt" && args.len() == 1 => {
            let inner = radical_reduce(&args[0]);
            if let Value::Call { head: ph, args: pa } = &inner
                && ph == "Power"
                && pa.len() == 2
                && let Value::Integer(e) = &pa[1]
                && let Some(ei) = e.to_i64()
                && ei >= 4
                && ei % 2 == 0
            {
                return vcall("Power", vec![pa[0].clone(), vint(ei / 2)]);
            }
            vcall("Sqrt", vec![inner])
        }
        Value::Call { head, args } => vcall(head, args.iter().map(radical_reduce).collect()),
        _ => val.clone(),
    }
}

/// If `v` is `Sqrt[b]` or `b^(1/2)`, return `b`.
fn as_sqrt_arg(v: &Value) -> Option<Value> {
    match v {
        Value::Call { head, args } if head == "Sqrt" && args.len() == 1 => Some(args[0].clone()),
        Value::Call { head, args }
            if head == "Power"
                && args.len() == 2
                && matches!(&args[1], Value::Rational(r) if **r == Rational::from((1, 2))) =>
        {
            Some(args[0].clone())
        }
        _ => None,
    }
}

/// Rewrite Tan/Cot/Sec/Csc in terms of Sin/Cos (bottom-up).
fn trig_to_sincos(val: &Value) -> Value {
    match val {
        Value::List(items) => Value::List(items.iter().map(trig_to_sincos).collect()),
        Value::Call { head, args } if args.len() == 1 => {
            let inner = trig_to_sincos(&args[0]);
            let pair = |h: &str| vcall(h, vec![inner.clone()]);
            match head.as_str() {
                "Tan" => vcall("Divide", vec![pair("Sin"), pair("Cos")]),
                "Cot" => vcall("Divide", vec![pair("Cos"), pair("Sin")]),
                "Sec" => vcall("Divide", vec![vint(1), pair("Cos")]),
                "Csc" => vcall("Divide", vec![vint(1), pair("Sin")]),
                _ => vcall(head, vec![inner]),
            }
        }
        Value::Call { head, args } => vcall(head, args.iter().map(trig_to_sincos).collect()),
        _ => val.clone(),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// FunctionExpand
// ─────────────────────────────────────────────────────────────────────────────

/// `FunctionExpand[expr]` — expand special functions into elementary ones:
/// `Gamma[1 + n] -> n Gamma[n]`, `Beta[a, b] -> Gamma[a] Gamma[b]/Gamma[a + b]`,
/// `Factorial[n] -> Gamma[1 + n]`, logarithms of products/powers, multiple
/// angles, and half-integer Bessel functions.
pub fn builtin_function_expand(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "FunctionExpand expects exactly 1 argument".to_string(),
        ));
    }
    Ok(function_expand_value(&args[0]))
}

fn function_expand_value(val: &Value) -> Value {
    use crate::builtins::symbolic::simplify_call;
    match val {
        Value::List(items) => Value::List(items.iter().map(function_expand_value).collect()),
        Value::Call { head, args } => {
            let mapped: Vec<Value> = args.iter().map(function_expand_value).collect();
            match head.as_str() {
                "Plus" => simplify_call("Plus", &mapped),
                "Times" => simplify_call("Times", &mapped),
                "Power" if mapped.len() == 2 => {
                    if mapped[0].struct_eq(&Value::Symbol("E".into())) {
                        vcall("Exp", vec![mapped[1].clone()])
                    } else {
                        vcall("Power", mapped)
                    }
                }
                "Log" if mapped.len() == 1 => fe_log(&mapped[0]),
                "Sin" if mapped.len() == 1 => fe_sin(&mapped[0]),
                "Cos" if mapped.len() == 1 => fe_cos(&mapped[0]),
                "Gamma" if mapped.len() == 1 => fe_gamma(&mapped[0]),
                "Beta" if mapped.len() == 2 => fe_beta(&mapped[0], &mapped[1]),
                "Factorial" if mapped.len() == 1 => fe_factorial(&mapped[0]),
                "BesselJ" if mapped.len() == 2 => fe_bessel_j(&mapped[0], &mapped[1]),
                _ => vcall(head, mapped),
            }
        }
        _ => val.clone(),
    }
}

fn fe_log(arg: &Value) -> Value {
    use crate::builtins::symbolic::simplify_call;
    match arg {
        Value::Call { head, args } if head == "Times" => {
            let terms: Vec<Value> = args.iter().map(|a| vcall("Log", vec![a.clone()])).collect();
            simplify_call("Plus", &terms)
        }
        Value::Call { head, args } if head == "Divide" && args.len() == 2 => simplify_call(
            "Plus",
            &[
                vcall("Log", vec![args[0].clone()]),
                simplify_call("Times", &[vint(-1), vcall("Log", vec![args[1].clone()])]),
            ],
        ),
        Value::Call { head, args } if head == "Power" && args.len() == 2 => simplify_call(
            "Times",
            &[args[1].clone(), vcall("Log", vec![args[0].clone()])],
        ),
        _ => vcall("Log", vec![arg.clone()]),
    }
}

fn fe_sin(arg: &Value) -> Value {
    use crate::builtins::symbolic::simplify_call;
    if let Value::Call { head, args } = arg
        && head == "Times"
        && args.len() == 2
        && matches!(&args[0], Value::Integer(n) if *n == 2)
    {
        let x = args[1].clone();
        return simplify_call(
            "Times",
            &[
                vint(2),
                vcall("Sin", vec![x.clone()]),
                vcall("Cos", vec![x]),
            ],
        );
    }
    if let Value::Call { head, args } = arg
        && head == "Plus"
        && args.len() == 2
    {
        let (a, b) = (args[0].clone(), args[1].clone());
        return simplify_call(
            "Plus",
            &[
                simplify_call(
                    "Times",
                    &[vcall("Sin", vec![a.clone()]), vcall("Cos", vec![b.clone()])],
                ),
                simplify_call("Times", &[vcall("Cos", vec![a]), vcall("Sin", vec![b])]),
            ],
        );
    }
    vcall("Sin", vec![arg.clone()])
}

fn fe_cos(arg: &Value) -> Value {
    use crate::builtins::symbolic::simplify_call;
    if let Value::Call { head, args } = arg
        && head == "Times"
        && args.len() == 2
        && matches!(&args[0], Value::Integer(n) if *n == 2)
    {
        let x = args[1].clone();
        return simplify_call(
            "Plus",
            &[
                vcall("Power", vec![vcall("Cos", vec![x.clone()]), vint(2)]),
                simplify_call(
                    "Times",
                    &[
                        vint(-1),
                        vcall("Power", vec![vcall("Sin", vec![x]), vint(2)]),
                    ],
                ),
            ],
        );
    }
    if let Value::Call { head, args } = arg
        && head == "Plus"
        && args.len() == 2
    {
        let (a, b) = (args[0].clone(), args[1].clone());
        return simplify_call(
            "Plus",
            &[
                simplify_call(
                    "Times",
                    &[vcall("Cos", vec![a.clone()]), vcall("Cos", vec![b.clone()])],
                ),
                simplify_call(
                    "Times",
                    &[vint(-1), vcall("Sin", vec![a]), vcall("Sin", vec![b])],
                ),
            ],
        );
    }
    vcall("Cos", vec![arg.clone()])
}

/// `Gamma[1 + n] -> n Gamma[n]` (either addend order).
fn fe_gamma(arg: &Value) -> Value {
    use crate::builtins::symbolic::simplify_call;
    if let Value::Call { head, args } = arg
        && head == "Plus"
        && args.len() == 2
    {
        for (one, n) in [(&args[0], &args[1]), (&args[1], &args[0])] {
            if matches!(one, Value::Integer(v) if *v == 1) {
                return simplify_call("Times", &[n.clone(), vcall("Gamma", vec![n.clone()])]);
            }
        }
    }
    vcall("Gamma", vec![arg.clone()])
}

/// `Beta[a, b] -> Gamma[a] Gamma[b] / Gamma[a + b]`.
fn fe_beta(a: &Value, b: &Value) -> Value {
    use crate::builtins::symbolic::simplify_call;
    simplify_call(
        "Divide",
        &[
            simplify_call(
                "Times",
                &[
                    vcall("Gamma", vec![a.clone()]),
                    vcall("Gamma", vec![b.clone()]),
                ],
            ),
            vcall(
                "Gamma",
                vec![simplify_call("Plus", &[a.clone(), b.clone()])],
            ),
        ],
    )
}

/// `Factorial[n] -> Gamma[1 + n]`.
fn fe_factorial(n: &Value) -> Value {
    use crate::builtins::symbolic::simplify_call;
    vcall("Gamma", vec![simplify_call("Plus", &[vint(1), n.clone()])])
}

/// `BesselJ[±1/2, x] -> Sqrt[2/(Pi x)] {Sin,Cos}[x]`.
fn fe_bessel_j(param: &Value, arg: &Value) -> Value {
    use crate::builtins::symbolic::simplify_call;
    let root = vcall(
        "Sqrt",
        vec![simplify_call(
            "Times",
            &[
                vint(2),
                simplify_call(
                    "Divide",
                    &[
                        vint(1),
                        simplify_call("Times", &[Value::Symbol("Pi".into()), arg.clone()]),
                    ],
                ),
            ],
        )],
    );
    if let Value::Rational(r) = param
        && *r.numer() == 1
        && *r.denom() == 2
    {
        return simplify_call("Times", &[root, vcall("Sin", vec![arg.clone()])]);
    }
    if let Value::Rational(r) = param
        && *r.numer() == -1
        && *r.denom() == 2
    {
        return simplify_call("Times", &[root, vcall("Cos", vec![arg.clone()])]);
    }
    vcall("BesselJ", vec![param.clone(), arg.clone()])
}

/// `PowerExpand[expr]` / `PowerExpand[expr, Assumptions -> …]` — formal
/// expansion of powers and logs, optionally refined under assumptions
/// (e.g. `PowerExpand[Sqrt[x^2], Assumptions -> x > 0] == x`).
pub fn builtin_power_expand(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Err(EvalError::Error(
            "PowerExpand expects 1 or 2 arguments".to_string(),
        ));
    }
    let expanded = crate::builtins::symbolicmore::builtin_power_expand(&[args[0].clone()])?;
    if args.len() == 2 {
        let assum = crate::builtins::symbolic::effective_assumptions(Some(&args[1]), &Env::new());
        if crate::builtins::symbolic::assumptions_present(&assum) {
            return crate::builtins::domains::refine_expr(&expanded, &assum);
        }
    }
    Ok(expanded)
}

/// Late overrides owned by this module: registered after the gap-fill modules
/// so they are authoritative over the overlapping builtins.
pub fn register_overrides(env: &Env) {
    super::register_builtin(env, "FunctionExpand", builtin_function_expand);
    super::register_builtin(env, "PowerExpand", builtin_power_expand);
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
            let fractions: Vec<(Value, Value)> = args.iter().map(term_to_fraction).collect();

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
    if let Value::Call { head, args } = term
        && head == "Divide"
        && args.len() == 2
    {
        return (args[0].clone(), args[1].clone());
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
    if let Value::Call { head, args } = val
        && head == "Divide"
        && args.len() == 2
    {
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
    val.clone()
}

fn factor_value(val: &Value) -> Value {
    // Call into existing Factor implementation
    crate::builtins::symbolic::builtin_factor(&[val.clone()]).unwrap_or_else(|_| val.clone())
}

fn cancel_common_factors(num: &Value, den: &Value) -> (Value, Value) {
    // Simple cancellation: if both are Times with matching factors
    if let (Value::Call { head: h1, args: a1 }, Value::Call { head: h2, args: a2 }) = (num, den)
        && h1 == "Times"
        && h2 == "Times"
    {
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
    (num.clone(), den.clone())
}

/// Apart[expr] — partial fraction decomposition.
pub fn builtin_apart(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
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
            // `let coeff;` avoids the dead initializer origin silenced with
            // `#[allow(unused_assignments)]`.
            let coeff;
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
    matches!(val, Value::Integer(n) if *n == 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::Env;

    fn int(n: i64) -> Value {
        Value::Integer(Integer::from(n))
    }
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
    fn test_level_positive_spec() {
        let expr = list(vec![sym("a"), sym("b")]);
        assert_eq!(
            builtin_level(&[expr.clone(), int(0)]).unwrap(),
            list(vec![expr])
        );
    }

    #[test]
    fn test_level_negative_spec_selects_leaves() {
        // Regression: negative levels were previously never matched.
        let expr = list(vec![sym("a"), sym("b")]);
        assert_eq!(
            builtin_level(&[expr, int(-1)]).unwrap(),
            list(vec![sym("a"), sym("b")])
        );
    }

    #[test]
    fn test_level_negative_level_end_to_end() {
        assert_eq!(
            eval_str("Level[{a, b, c}, -1]"),
            list(vec![sym("a"), sym("b"), sym("c")])
        );
    }

    #[test]
    fn test_matrix_q_false_end_to_end() {
        assert_eq!(eval_str("MatrixQ[5]"), Value::Bool(false));
    }

    #[test]
    fn test_check_returns_fallback_on_error() {
        // `Check` holds its first argument, so it arrives wrapped as Value::Pattern.
        let bad = Value::Pattern(Expr::Call {
            head: Box::new(Expr::Symbol("Extract".to_string())),
            args: vec![
                Expr::List(vec![
                    Expr::Integer(Integer::from(1)),
                    Expr::Integer(Integer::from(2)),
                ]),
                Expr::Integer(Integer::from(5)),
            ],
        });
        let fallback = Value::Pattern(Expr::Integer(Integer::from(99)));
        let out = builtin_check(&[bad, fallback], &env_with_builtins()).unwrap();
        assert_eq!(out, int(99));
    }

    #[test]
    fn test_replace_default_level_replaces_leaves() {
        // Regression: the default (-1, -1) spec previously matched nothing.
        let expr = list(vec![sym("a"), sym("b")]);
        let rule = Value::Rule {
            lhs: Box::new(sym("a")),
            rhs: Box::new(sym("x")),
            delayed: false,
        };
        let out = builtin_replace(&[expr, rule], &Env::new()).unwrap();
        assert_eq!(out, list(vec![sym("x"), sym("b")]));
    }

    #[test]
    fn test_numeric_q_requires_all_elements() {
        assert_eq!(
            builtin_numeric_q(&[list(vec![int(1), sym("x")])]).unwrap(),
            Value::Bool(false)
        );
        assert_eq!(
            builtin_numeric_q(&[list(vec![int(1), int(2)])]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_numeric_q(&[Value::Call {
                head: "Sin".to_string(),
                args: vec![sym("x")],
            }])
            .unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_matrix_q_returns_false_not_error() {
        // The canonical implementation now lives in `predicates`.
        let env = Env::new();
        let mq = |v: Value| crate::builtins::predicates::builtin_matrix_q(&[v], &env).unwrap();
        // Regression: a non-matrix previously produced an EvalError.
        assert_eq!(mq(int(5)), Value::Bool(false));
        assert_eq!(mq(list(vec![])), Value::Bool(true));
        let ragged = list(vec![list(vec![int(1)]), list(vec![int(1), int(2)])]);
        assert_eq!(mq(ragged), Value::Bool(false));
        let matrix = list(vec![list(vec![int(1), int(2)]), list(vec![int(3), int(4)])]);
        assert_eq!(mq(matrix), Value::Bool(true));
    }

    #[test]
    fn test_extract_indices() {
        let expr = list(vec![int(10), int(20)]);
        assert_eq!(builtin_extract(&[expr.clone(), int(2)]).unwrap(), int(20));
        assert_eq!(
            builtin_extract(&[expr, list(vec![int(1)])]).unwrap(),
            int(10)
        );
    }

    #[test]
    fn test_replace_list_matches_pattern_rule() {
        // Regression: evaluator-produced rule sides are held as `Value::Pattern`,
        // so matching must go through the pattern engine, not `struct_eq`.
        assert_eq!(
            eval_str("ReplaceList[{a, b}, {a, b} -> z]"),
            list(vec![sym("z")])
        );
    }

    /// Assert two expressions are numerically equivalent at sample points.
    fn assert_num_equiv(a: &str, b: &str) {
        let cond = eval_str(&format!(
            "With[{{x = 0.7, y = 1.3, z = 2.1}}, Abs[(({a}) - ({b}))] < 10^-6]"
        ));
        assert!(
            matches!(cond, Value::Bool(true)) || matches!(&cond, Value::Symbol(s) if s == "True"),
            "`{a}` not numerically equivalent to `{b}`"
        );
    }

    /// Assert `FullSimplify[input]` is symbolically equivalent to `expected`.
    fn assert_full_simplify_equiv(input: &str, expected: &str) {
        let diff = eval_str(&format!("FullSimplify[({input}) - ({expected})]"));
        assert_eq!(
            diff,
            Value::Integer(Integer::from(0)),
            "FullSimplify[{input}] not equivalent to {expected}, got `{}`",
            eval_str(&format!("FullSimplify[{input}]"))
        );
    }

    #[test]
    fn test_full_simplify_cancels_rational() {
        assert_full_simplify_equiv("(x^2 - 1)/(x - 1)", "x + 1");
        assert_full_simplify_equiv("(x^2 + 2 x + 1)/(x + 1)", "x + 1");
        assert_full_simplify_equiv("(a^2 - b^2)/(a - b)", "a + b");
        // Factor ordering: `x + 1` and `1 + x` must still cancel.
        assert_full_simplify_equiv("(x + 1)/(x^2 - 1)", "1/(x - 1)");
        assert_full_simplify_equiv("(x^2 - 1)/(x + 1)", "x - 1");
    }

    #[test]
    fn test_full_simplify_does_not_drop_nonpolynomial_terms() {
        // Regression: the underlying `Factor` dropped `Sin[x]` here.
        assert_full_simplify_equiv("x + Sin[x]", "x + Sin[x]");
        assert_eq!(eval_str("FullSimplify[x + Sin[x]]"), eval_str("x + Sin[x]"));
    }

    #[test]
    fn test_full_simplify_combines_like_terms() {
        assert_full_simplify_equiv("x + x + 2 x", "4 x");
    }

    #[test]
    fn test_full_simplify_together_and_cancel() {
        // Wrong-gcd regression: Cancel[(x+y)/(x y)] used to yield 1/x.
        assert_full_simplify_equiv("1/x + 1/y", "(x + y)/(x y)");
        assert_full_simplify_equiv("1/x + 2/x", "3/x");
    }

    #[test]
    fn test_full_simplify_trig_identities() {
        assert_eq!(eval_str("FullSimplify[Sin[x]^2 + Cos[x]^2]"), int(1));
        assert_eq!(
            eval_str("FullSimplify[Cos[2 x] - (Cos[x]^2 - Sin[x]^2)]"),
            int(0)
        );
        assert_eq!(
            eval_str("FullSimplify[2 Sin[u]^2 + 2 Cos[u]^2 + 3]"),
            int(5)
        );
    }

    #[test]
    fn test_full_simplify_with_assumptions() {
        // With the sign assumption the square root collapses…
        assert_eq!(
            eval_str("FullSimplify[Sqrt[x^2], Assumptions -> x > 0]"),
            eval_str("x")
        );
        assert_eq!(eval_str("FullSimplify[Sqrt[x^2], x < 0]"), eval_str("-x"));
        // …and `Assuming` feeds `$Assumptions` through.
        assert_eq!(
            eval_str("Assuming[x > 0, FullSimplify[Sqrt[x^2]]]"),
            eval_str("x")
        );
        // Without an assumption it must NOT over-simplify.
        assert_eq!(eval_str("FullSimplify[Sqrt[x^2]]"), eval_str("Sqrt[x^2]"));
    }

    #[test]
    fn test_full_simplify_double_angle() {
        assert_eq!(
            eval_str("FullSimplify[Cos[x]^2 - Sin[x]^2]"),
            eval_str("Cos[2 x]")
        );
        assert_eq!(
            eval_str("FullSimplify[2 Sin[x] Cos[x]]"),
            eval_str("Sin[2 x]")
        );
    }

    #[test]
    fn test_full_simplify_rational_number() {
        assert_eq!(eval_str("FullSimplify[6/4]"), eval_str("3/2"));
    }

    #[test]
    fn test_full_simplify_at_least_as_strong_as_simplify() {
        // For any input FullSimplify must not be longer than Simplify's output.
        for input in [
            "(x^2 - 1)/(x - 1)",
            "Sin[x]^2 + Cos[x]^2",
            "1/x + 1/y + 1/z",
            "(x^3 - 1)/(x - 1)",
        ] {
            let simplified = eval_str(&format!("Simplify[{input}]"));
            let full = eval_str(&format!("FullSimplify[{input}]"));
            assert!(
                full.to_string().len() <= simplified.to_string().len(),
                "FullSimplify[{input}] = `{full}` longer than Simplify = `{simplified}`"
            );
        }
    }

    #[test]
    fn test_replace_at_level_applies_pattern_rule() {
        let expr = list(vec![sym("a"), sym("b")]);
        let rule = Value::Rule {
            lhs: Box::new(Value::Pattern(Expr::Symbol("a".to_string()))),
            rhs: Box::new(Value::Pattern(Expr::Symbol("x".to_string()))),
            delayed: false,
        };
        // Level 1 (the elements); the default leaves spec is normalized above.
        let out = builtin_replace(&[expr, rule, int(1)], &env_with_builtins()).unwrap();
        assert_eq!(out, list(vec![sym("x"), sym("b")]));
    }

    // ── FullSimplify: hardened multi-strategy search ──

    #[test]
    fn test_full_simplify_logexp() {
        assert_num_equiv("FullSimplify[Log[x] + Log[y]]", "Log[x y]");
        assert_num_equiv("FullSimplify[Log[x] - Log[y]]", "Log[x/y]");
        assert_eq!(eval_str("FullSimplify[Log[x y] - Log[x] - Log[y]]"), int(0));
        assert_num_equiv("FullSimplify[Exp[x] Exp[y]]", "Exp[x + y]");
        assert_num_equiv("FullSimplify[Exp[x]/Exp[y]]", "Exp[x - y]");
        assert_eq!(eval_str("FullSimplify[Exp[x] Exp[y] - Exp[x + y]]"), int(0));
    }

    #[test]
    fn test_full_simplify_radical() {
        // `Sqrt[x]^2 -> x` is unconditional; `Sqrt[x^2]` stays ambiguous.
        assert_eq!(eval_str("FullSimplify[Sqrt[x]^2]"), eval_str("x"));
        assert_full_simplify_equiv("Sqrt[x^4]", "x^2");
        assert_eq!(eval_str("FullSimplify[Sqrt[x^2]]"), eval_str("Sqrt[x^2]"));
    }

    #[test]
    fn test_full_simplify_trig_normalisation() {
        assert_eq!(eval_str("FullSimplify[Tan[x] - Sin[x]/Cos[x]]"), int(0));
        assert_eq!(eval_str("FullSimplify[Cot[x] - Cos[x]/Sin[x]]"), int(0));
        assert_eq!(eval_str("FullSimplify[Sec[x] - 1/Cos[x]]"), int(0));
        // A bare `Tan` must not be needlessly rewritten to `Sin/Cos`.
        assert_eq!(eval_str("FullSimplify[Tan[x]]"), eval_str("Tan[x]"));
    }

    #[test]
    fn test_full_simplify_required_assumption_identities() {
        assert_eq!(
            eval_str("Assuming[x < 0, FullSimplify[Abs[x]]]"),
            eval_str("-x")
        );
        assert_eq!(eval_str("FullSimplify[Sqrt[x^2], x > 0]"), eval_str("x"));
    }

    #[test]
    fn test_full_simplify_is_bounded_on_deep_input() {
        // (x + 1)^10 expanded and subtracted must collapse to 0 without hanging.
        assert_eq!(
            eval_str("FullSimplify[(x + 1)^10 - Expand[(x + 1)^10]]"),
            int(0)
        );
    }

    #[test]
    fn test_full_simplify_complexity_never_exceeds_simplify() {
        // Strict-improvement search seeded from `Simplify` guarantees the result
        // is never more complex, by both the internal metric and string length.
        for input in [
            "(x^2 - 1)/(x - 1)",
            "Sin[x]^2 + Cos[x]^2",
            "1/x + 1/y + 1/z",
            "(x^3 - 1)/(x - 1)",
            "Sqrt[x^2]",
            "Log[x] + Log[y]",
            "Exp[x] Exp[y]",
            "Tan[x]",
            "Cos[x]^2 - Sin[x]^2",
            "x^2 + 2 x + 1",
            "x + Sin[x]",
            "Sqrt[8]",
        ] {
            let simplified = eval_str(&format!("Simplify[{input}]"));
            let full = eval_str(&format!("FullSimplify[{input}]"));
            assert!(
                value_complexity(&full) <= value_complexity(&simplified),
                "FullSimplify[{input}] = `{full}` more complex than Simplify = `{simplified}`"
            );
            assert!(
                full.to_string().len() <= simplified.to_string().len(),
                "FullSimplify[{input}] = `{full}` longer than Simplify = `{simplified}`"
            );
        }
    }

    // ── Expanding / transforming family ──

    #[test]
    fn test_trig_expand_end_to_end() {
        assert_eq!(
            eval_str("TrigExpand[Sin[x + y]]"),
            eval_str("Sin[x] Cos[y] + Cos[x] Sin[y]")
        );
        assert_eq!(
            eval_str("TrigExpand[Sin[2 x]]"),
            eval_str("2 Sin[x] Cos[x]")
        );
        // Inapplicable input is returned unevaluated.
        assert_eq!(eval_str("TrigExpand[x]"), eval_str("x"));
        assert_eq!(eval_str("TrigExpand[f[x]]"), eval_str("f[x]"));
    }

    #[test]
    fn test_trig_reduce_end_to_end() {
        assert_eq!(
            eval_str("TrigReduce[Sin[x]^2]"),
            eval_str("1/2 - 1/2 Cos[2 x]")
        );
        assert_eq!(
            eval_str("TrigReduce[Sin[x] Cos[y]]"),
            eval_str("1/2 Sin[x + y] + 1/2 Sin[x - y]")
        );
        assert_eq!(eval_str("TrigReduce[x]"), eval_str("x"));
    }

    #[test]
    fn test_trig_factor_end_to_end() {
        assert_eq!(eval_str("TrigFactor[Sin[x]^2 + Cos[x]^2]"), int(1));
        assert_num_equiv(
            "TrigFactor[Sin[x] + Sin[y]]",
            "2 Sin[(x + y)/2] Cos[(x - y)/2]",
        );
        assert_eq!(eval_str("TrigFactor[x]"), eval_str("x"));
    }

    #[test]
    fn test_trig_to_exp_and_back() {
        assert_num_equiv("TrigToExp[Sin[x]]", "(Exp[I x] - Exp[-I x])/(2 I)");
        assert_eq!(
            eval_str("ExpToTrig[Exp[I x]]"),
            eval_str("Cos[x] + I Sin[x]")
        );
        assert_eq!(eval_str("ExpToTrig[Exp[x]]"), eval_str("Cosh[x] + Sinh[x]"));
        // Round-trip is an identity.
        assert_num_equiv("Simplify[ExpToTrig[TrigToExp[Sin[x + y]]]]", "Sin[x + y]");
    }

    #[test]
    fn test_complex_expand_real_symbols() {
        assert_eq!(
            eval_str("ComplexExpand[Exp[I x]]"),
            eval_str("Cos[x] + I Sin[x]")
        );
        assert_eq!(
            eval_str("ComplexExpand[Abs[x + I y]]"),
            eval_str("Sqrt[x^2 + y^2]")
        );
        assert_eq!(eval_str("ComplexExpand[Re[x + I y]]"), eval_str("x"));
        // A real expression is left untouched.
        assert_eq!(eval_str("ComplexExpand[x + y]"), eval_str("x + y"));
    }

    #[test]
    fn test_power_expand_formal_and_assumptions() {
        assert_eq!(eval_str("PowerExpand[(a b)^c]"), eval_str("a^c b^c"));
        assert_eq!(
            eval_str("PowerExpand[Log[a b]]"),
            eval_str("Log[a] + Log[b]")
        );
        // Formal expansion collapses the ambiguous root…
        assert_eq!(eval_str("PowerExpand[Sqrt[x^2]]"), eval_str("x"));
        // …and assumptions make it correct for the stated branch.
        assert_eq!(
            eval_str("PowerExpand[Sqrt[x^2], Assumptions -> x > 0]"),
            eval_str("x")
        );
        assert_eq!(eval_str("PowerExpand[y]"), eval_str("y"));
    }

    #[test]
    fn test_function_expand_gamma_beta_factorial() {
        assert_full_simplify_equiv("FunctionExpand[Gamma[1 + n]]", "n Gamma[n]");
        assert_eq!(
            eval_str("FunctionExpand[Factorial[n]]"),
            eval_str("Gamma[1 + n]")
        );
        assert_full_simplify_equiv(
            "FunctionExpand[Beta[a, b]]",
            "Gamma[a] Gamma[b]/Gamma[a + b]",
        );
        // Already-numeric arguments still evaluate.
        assert_eq!(eval_str("FunctionExpand[Beta[2, 3]]"), eval_str("1/12"));
        assert_eq!(eval_str("FunctionExpand[f[x]]"), eval_str("f[x]"));
    }

    #[test]
    fn test_expand_all_end_to_end() {
        assert_eq!(eval_str("ExpandAll[(x + 1)^2]"), eval_str("x^2 + 2 x + 1"));
        assert_eq!(
            eval_str("Simplify[ExpandAll[(x + 1)/y] - (x/y + 1/y)]"),
            int(0)
        );
        assert_eq!(eval_str("ExpandAll[x]"), eval_str("x"));
    }
}
