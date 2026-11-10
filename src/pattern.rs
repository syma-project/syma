/// Pattern matching engine for Syma language.
///
/// Matches values against patterns, binding variables as needed.
/// Supports:
/// - Blanks: _, x_, _Integer, x_Integer
/// - Sequences: __, x__, ___, x___
/// - List patterns: {x_, y_}
/// - Literal patterns: 0, "hello", True
/// - Compound patterns: x_^n_, a_ + b_
/// - Guards: pattern /; condition
/// - Alternatives: (pat1 | pat2)
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;

use rug::Float;

use crate::ast::Expr;
use crate::value::Value;

/// A set of variable bindings produced by pattern matching.
pub type Bindings = HashMap<String, Value>;

/// Identity element for a `OneIdentity` head, when it has a known numeric
/// identity (`Times`→1, `Plus`→0). Used so that a bare value such as `x^2`
/// can match a `b_*x^2` pattern with `b` defaulting to the identity.
fn one_identity_element(head: &str) -> Option<Value> {
    match head {
        "Times" | "Power" => Some(Value::Integer(rug::Integer::from(1))),
        "Plus" => Some(Value::Integer(rug::Integer::from(0))),
        _ => None,
    }
}

/// Whether a head should be treated as `OneIdentity` for pattern matching.
/// `Power` is special-cased because `x` must be allowed to match `x_^n_`
/// (with `n` = 1) even though `Power` lacks the attribute.
fn head_is_one_identity(head: &str, attr_checker: Option<&AttributeChecker>) -> bool {
    head == "Power" || attr_checker.is_some_and(|c| c.has_attr(head, "OneIdentity"))
}

/// OneIdentity fallback for call patterns: a bare value can stand in for a
/// synthesized `h[...]` in which the slot absorbing the value is the only
/// "real" one and every other slot is an `Optional` pattern filled by its
/// default (or the head's identity element). This matches Mathematica:
/// `n_. x_` matches `x`, but `n_ x_` does not.
///
/// `Power` is special-cased (`x` stands for `x^1`).
fn match_one_identity_pattern(
    head_name: &str,
    pat_args: &[Expr],
    value: &Value,
    attr_checker: Option<&AttributeChecker>,
) -> Option<Bindings> {
    let pat_args: Vec<Expr> = flatten_expr_args(head_name, pat_args);
    if pat_args.is_empty() {
        return None;
    }
    let is_orderless = attr_checker.is_some_and(|c| c.has_attr(head_name, "Orderless"));

    let id = one_identity_element(head_name)?;
    // Try each slot as the one absorbing `value`; every other slot must be an
    // `Optional` pattern (filled by its default / the identity element) so that
    // `n_. x_` matches `x` but `n_ x_` does not (Mathematica semantics).
    for value_slot in 0..pat_args.len() {
        let mut synth_args: Vec<Value> = Vec::with_capacity(pat_args.len());
        let mut fillable = true;
        for (j, p) in pat_args.iter().enumerate() {
            if j == value_slot {
                synth_args.push(value.clone());
            } else if let Some(fill) = optional_fill(p, &id) {
                synth_args.push(fill);
            } else {
                fillable = false;
                break;
            }
        }
        if !fillable {
            continue;
        }
        if let Some(b) = match_one_identity_try(&pat_args, &synth_args, is_orderless, attr_checker)
        {
            return Some(b);
        }
    }
    None
}

/// If `p` is an `Optional` pattern, the value filling its slot when absent:
/// its explicit default, or the head's identity element `id`.
fn optional_fill(p: &Expr, id: &Value) -> Option<Value> {
    match p {
        Expr::OptionalBlank { default_value, .. }
        | Expr::OptionalNamedBlank { default_value, .. } => Some(
            default_value
                .as_ref()
                .map(|d| unwrap_expr_to_value(d))
                .unwrap_or_else(|| id.clone()),
        ),
        _ => None,
    }
}

fn match_one_identity_try(
    pat_args: &[Expr],
    synth_args: &[Value],
    is_orderless: bool,
    attr_checker: Option<&AttributeChecker>,
) -> Option<Bindings> {
    if is_orderless
        && synth_args.len() >= 2
        && synth_args.len() <= 6
        && let MatchResult::Match(b) = try_orderless_match(pat_args, synth_args, attr_checker)
    {
        return Some(b);
    }
    if let MatchResult::Match(b) = try_ordered_match(pat_args, synth_args, attr_checker) {
        return Some(b);
    }
    None
}

/// Lightweight attribute checker for the pattern engine.
///
/// Wraps the shared attribute map so the pattern engine can query
/// symbol attributes (Flat, Orderless, OneIdentity) without depending
/// on `Env`. Pass `None` to skip attribute-based matching.
pub struct AttributeChecker {
    attributes: Arc<Mutex<HashMap<String, Vec<String>>>>,
}

impl AttributeChecker {
    pub fn new(attributes: Arc<Mutex<HashMap<String, Vec<String>>>>) -> Self {
        AttributeChecker { attributes }
    }

    /// Returns a checker with no attributes (all lookups return false).
    pub fn empty() -> Self {
        AttributeChecker {
            attributes: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn has_attr(&self, name: &str, attr: &str) -> bool {
        self.attributes
            .lock()
            .unwrap()
            .get(name)
            .map(|attrs| attrs.iter().any(|a| a == attr))
            .unwrap_or(false)
    }
}

/// Result of a pattern match attempt.
#[derive(Debug)]
pub enum MatchResult {
    /// Match succeeded with bindings.
    Match(Bindings),
    /// Match failed.
    NoMatch,
}

/// Try to unwrap a OneIdentity value: if the value is a single-arg Call
/// whose head has the OneIdentity attribute, return the inner argument.
fn try_unwrap_one_identity<'a>(
    value: &'a Value,
    attr_checker: Option<&AttributeChecker>,
) -> Option<&'a Value> {
    if let Value::Call { head, args } = value
        && args.len() == 1
        && attr_checker.is_some_and(|c| c.has_attr(head, "OneIdentity"))
    {
        return Some(&args[0]);
    }
    None
}

/// Try to match a value against a pattern.
///
/// Convert an AST expression to a runtime Value.
/// Used to unwrap `Value::Pattern(Expr)` for pattern matching
/// when HoldAll/HoldAllComplete prevents argument evaluation.
pub fn unwrap_expr_to_value(expr: &Expr) -> Value {
    match expr {
        Expr::Symbol(s) => Value::Symbol(s.clone()),
        Expr::Integer(n) => Value::Integer(n.clone()),
        Expr::Real(r) => Value::Real(r.clone()),
        Expr::Bool(b) => Value::Bool(*b),
        Expr::Str(s) => Value::Str(s.clone()),
        Expr::Null => Value::Null,
        Expr::List(items) => Value::List(items.iter().map(unwrap_expr_to_value).collect()),
        Expr::Call { head, args } => {
            let h = unwrap_expr_to_value(head);
            let a: Vec<Value> = args.iter().map(unwrap_expr_to_value).collect();
            match h {
                Value::Symbol(name) => Value::Call {
                    head: name,
                    args: a,
                },
                _ => Value::Call {
                    head: h.to_string(),
                    args: a,
                },
            }
        }
        _ => Value::Pattern(expr.clone()),
    }
}

/// Returns Match(bindings) on success, NoMatch on failure.
/// `attr_checker` provides access to symbol attributes (Flat, Orderless, OneIdentity)
/// for call pattern matching. Pass `None` to skip attribute-based matching.
pub fn match_pattern(
    pattern: &Expr,
    value: &Value,
    attr_checker: Option<&AttributeChecker>,
) -> MatchResult {
    // Unwrap Value::Pattern/Value::Hold/Value::HoldComplete for HoldAll/HoldAllComplete arguments
    if let Value::Pattern(expr) = value {
        let unwrapped = unwrap_expr_to_value(expr);
        // `unwrap_expr_to_value` maps pattern nodes (Blank/NamedBlank/...) back to
        // Value::Pattern, so unwrapping again would not terminate. Only recurse
        // when the unwrapped value is not an identical Pattern wrapper.
        if !matches!(&unwrapped, Value::Pattern(e) if e == expr) {
            return match_pattern(pattern, &unwrapped, attr_checker);
        }
    }
    if let Value::Hold(inner) = value {
        return match_pattern(pattern, inner, attr_checker);
    }
    if let Value::HoldComplete(inner) = value {
        return match_pattern(pattern, inner, attr_checker);
    }

    // ── HoldPattern[p]: match the content as a pattern (unwraps the hold) ──
    // ── Verbatim[e]:   match e literally (no pattern interpretation) ──
    if let Expr::Call { head, args } = pattern
        && let Expr::Symbol(h) = head.as_ref()
    {
        if h == "HoldPattern" && args.len() == 1 {
            return match_pattern(&args[0], value, attr_checker);
        }
        if h == "Verbatim" && args.len() == 1 {
            let literal = unwrap_expr_to_value(&args[0]);
            return if literal.struct_eq(value) {
                MatchResult::Match(HashMap::new())
            } else {
                MatchResult::NoMatch
            };
        }
        if h == "KeyValuePattern" {
            return match_key_value_pattern(args, value, attr_checker);
        }
    }

    match pattern {
        // ── Blank patterns ──
        Expr::Blank { type_constraint } => {
            if let Some(tc) = type_constraint {
                if value.matches_type(tc) {
                    MatchResult::Match(HashMap::new())
                } else if let Some(unwrapped) = try_unwrap_one_identity(value, attr_checker) {
                    // OneIdentity: retry with unwrapped value
                    match_pattern(pattern, unwrapped, attr_checker)
                } else {
                    MatchResult::NoMatch
                }
            } else {
                MatchResult::Match(HashMap::new())
            }
        }

        Expr::NamedBlank {
            name,
            type_constraint,
        } => {
            if let Some(tc) = type_constraint {
                if value.matches_type(tc) {
                    let mut bindings = HashMap::new();
                    bindings.insert(name.clone(), value.clone());
                    MatchResult::Match(bindings)
                } else if let Some(unwrapped) = try_unwrap_one_identity(value, attr_checker) {
                    // OneIdentity: retry with unwrapped value
                    match_pattern(pattern, unwrapped, attr_checker)
                } else {
                    MatchResult::NoMatch
                }
            } else {
                let mut bindings = HashMap::new();
                bindings.insert(name.clone(), value.clone());
                MatchResult::Match(bindings)
            }
        }

        // ── Sequence patterns: __ (BlankSequence) and ___ (BlankNullSequence) ──
        // As top-level patterns, they match any single value (sequence of length 1).
        Expr::BlankSequence {
            name,
            type_constraint,
        } => {
            if let Some(tc) = type_constraint {
                if value.matches_type(tc) {
                    let mut bindings = HashMap::new();
                    if let Some(n) = name {
                        bindings.insert(n.clone(), value.clone());
                    }
                    MatchResult::Match(bindings)
                } else {
                    MatchResult::NoMatch
                }
            } else {
                let mut bindings = HashMap::new();
                if let Some(n) = name {
                    bindings.insert(n.clone(), value.clone());
                }
                MatchResult::Match(bindings)
            }
        }

        Expr::BlankNullSequence {
            name,
            type_constraint,
        } => {
            if let Some(tc) = type_constraint {
                if value.matches_type(tc) {
                    let mut bindings = HashMap::new();
                    if let Some(n) = name {
                        bindings.insert(n.clone(), value.clone());
                    }
                    MatchResult::Match(bindings)
                } else {
                    MatchResult::NoMatch
                }
            } else {
                let mut bindings = HashMap::new();
                if let Some(n) = name {
                    bindings.insert(n.clone(), value.clone());
                }
                MatchResult::Match(bindings)
            }
        }

        // ── Optional patterns: _. and x_. ──
        // Try normal match first; if it fails, treat the pattern as matched
        // with Null as the default value.
        Expr::OptionalBlank {
            type_constraint, ..
        } => {
            // Try normal blank match first
            let normal_match = Expr::Blank {
                type_constraint: type_constraint.clone(),
            };
            match match_pattern(&normal_match, value, attr_checker) {
                MatchResult::Match(_) => MatchResult::Match(HashMap::new()),
                MatchResult::NoMatch => {
                    // Optional: match succeeded with no bindings
                    MatchResult::Match(HashMap::new())
                }
            }
        }

        Expr::OptionalNamedBlank {
            name,
            type_constraint,
            default_value,
        } => {
            // Try normal named blank match first
            let normal_match = Expr::NamedBlank {
                name: name.clone(),
                type_constraint: type_constraint.clone(),
            };
            match match_pattern(&normal_match, value, attr_checker) {
                MatchResult::Match(bindings) => MatchResult::Match(bindings),
                MatchResult::NoMatch => {
                    // Optional: fall back to the supplied default, else Null.
                    let mut bindings = HashMap::new();
                    bindings.insert(name.clone(), optional_default_binding(default_value));
                    MatchResult::Match(bindings)
                }
            }
        }

        // ── Literal patterns ──
        Expr::Integer(n) => {
            if let Value::Integer(v) = value {
                if n == v {
                    MatchResult::Match(HashMap::new())
                } else {
                    MatchResult::NoMatch
                }
            } else {
                MatchResult::NoMatch
            }
        }

        Expr::Real(r) => {
            if let Value::Real(v) = value {
                if r == v {
                    MatchResult::Match(HashMap::new())
                } else {
                    MatchResult::NoMatch
                }
            } else if let Value::Integer(n) = value {
                let n_f = Float::with_val(crate::value::DEFAULT_PRECISION, n);
                if *r == n_f {
                    MatchResult::Match(HashMap::new())
                } else {
                    MatchResult::NoMatch
                }
            } else {
                MatchResult::NoMatch
            }
        }

        Expr::Bool(b) => {
            if let Value::Bool(v) = value {
                if b == v {
                    MatchResult::Match(HashMap::new())
                } else {
                    MatchResult::NoMatch
                }
            } else {
                MatchResult::NoMatch
            }
        }

        Expr::Str(s) => {
            if let Value::Str(v) = value {
                if s == v {
                    MatchResult::Match(HashMap::new())
                } else {
                    MatchResult::NoMatch
                }
            } else {
                MatchResult::NoMatch
            }
        }

        Expr::Null => {
            if matches!(value, Value::Null) {
                MatchResult::Match(HashMap::new())
            } else {
                MatchResult::NoMatch
            }
        }

        // ── Symbol (matches exact symbol or blank pattern) ──
        // A symbol containing `_` is a blank pattern when it reaches the matcher
        // unevaluated, e.g. as a rule LHS, a Switch/match case, or a held
        // argument: `_`, `x_`, `_Integer`, `x_Integer`, `__`, `x___`, ...
        // Normalize it to the corresponding pattern node and retry.
        Expr::Symbol(s) => {
            if s.contains('_') {
                return match_pattern(&crate::eval::convert_blank_pattern(s), value, attr_checker);
            }
            if let Value::Symbol(v) = value {
                if s == v {
                    MatchResult::Match(HashMap::new())
                } else {
                    MatchResult::NoMatch
                }
            } else {
                MatchResult::NoMatch
            }
        }

        // ── List pattern ──
        Expr::List(patterns) => {
            if let Value::List(items) = value {
                match_list_pattern(patterns, items, attr_checker)
            } else {
                MatchResult::NoMatch
            }
        }

        // ── Guard pattern: pattern /; condition ──
        Expr::PatternGuard {
            pattern,
            condition: _,
        } => {
            // For now, just match the inner pattern.
            // Guard evaluation requires the evaluator, so it's handled
            // in the evaluator's dispatch logic.
            match_pattern(pattern, value, attr_checker)
        }

        // ── Call pattern: f[x_, y_], negated -expr, and alternatives (pat1 | pat2) ──
        Expr::Call { head, args } => {
            // Check for negated pattern: Times[-1, expr]
            if matches!(head.as_ref(), Expr::Symbol(s) if s == "Times")
                && args.len() == 2
                && matches!(&args[0], Expr::Integer(n) if *n == -1)
            {
                // Handle concrete negative integer: -x_ matches Value::Integer(-n)
                if let Value::Integer(v) = value
                    && v.is_negative()
                {
                    return match_pattern(&args[1], &Value::Integer(-v.clone()), attr_checker);
                }
                // Handle structural Times[-1, x]
                if let Value::Call { head: h, args: a } = value
                    && h == "Times"
                    && a.len() == 2
                    && let Value::Integer(n) = &a[0]
                    && *n == -1
                {
                    return match_pattern(&args[1], &a[1], attr_checker);
                }
                return MatchResult::NoMatch;
            }

            // Check for alternatives: Alternatives[pat1, pat2, ...]
            if matches!(head.as_ref(), Expr::Symbol(s) if s == "Alternatives") {
                for alt in args {
                    if let MatchResult::Match(bindings) = match_pattern(alt, value, attr_checker) {
                        return MatchResult::Match(bindings);
                    }
                }
                return MatchResult::NoMatch;
            }

            // Check for Except:
            //   Except[pat]     matches anything not matching pat
            //   Except[pat, p]  matches anything not matching pat but matching p
            if matches!(head.as_ref(), Expr::Symbol(s) if s == "Except")
                && (args.len() == 1 || args.len() == 2)
            {
                if let MatchResult::Match(_) = match_pattern(&args[0], value, attr_checker) {
                    return MatchResult::NoMatch;
                }
                return if args.len() == 2 {
                    // Bindings from the "should match" pattern are kept.
                    match_pattern(&args[1], value, attr_checker)
                } else {
                    MatchResult::Match(HashMap::new())
                };
            }

            // Condition[pat, cond] / PatternTest[pat, test] written as calls.
            // The guard itself is evaluated by the caller (match_pattern has no
            // environment); here we only need to match the inner pattern.
            if let Expr::Symbol(s) = head.as_ref()
                && (s == "Condition" || s == "PatternTest")
                && args.len() == 2
            {
                return match_pattern(&args[0], value, attr_checker);
            }

            // Optional[pat] / Optional[pat, default]: matches pat, or (when it
            // fails) matches nothing.
            if call_head_is(head, "Optional") && (1..=2).contains(&args.len()) {
                return match match_pattern(&args[0], value, attr_checker) {
                    MatchResult::Match(b) => MatchResult::Match(b),
                    MatchResult::NoMatch => {
                        let mut b = HashMap::new();
                        bind_optional_call_default(args, &mut b);
                        MatchResult::Match(b)
                    }
                };
            }

            // Check for Repeated/RepeatedNull: consume a run of list elements.
            //   Repeated[pat]         1+ times
            //   Repeated[pat, max]    1..=max times
            //   Repeated[pat, min, max]
            //   RepeatedNull[...]     0+ (or bounded) times
            if matches!(head.as_ref(), Expr::Symbol(s) if s == "Repeated" || s == "RepeatedNull") {
                let name = match head.as_ref() {
                    Expr::Symbol(s) => s.as_str(),
                    _ => "Repeated",
                };
                return match_repeated_pattern(name, args, value, attr_checker);
            }

            // Regular call pattern: f[x_, y_]
            match_call_pattern(head, args, value, attr_checker)
        }

        // ── Rule patterns ──
        Expr::Rule { lhs, rhs } => {
            if let Value::Rule {
                lhs: vl,
                rhs: vr,
                delayed,
            } = value
            {
                if !delayed {
                    let left_match = match_pattern(lhs, vl, attr_checker);
                    if let MatchResult::Match(mut bindings) = left_match
                        && let MatchResult::Match(right_bindings) =
                            match_pattern(rhs, vr, attr_checker)
                    {
                        bindings.extend(right_bindings);
                        return MatchResult::Match(bindings);
                    }
                }
                MatchResult::NoMatch
            } else {
                MatchResult::NoMatch
            }
        }

        Expr::RuleDelayed { lhs, rhs } => {
            if let Value::Rule {
                lhs: vl,
                rhs: vr,
                delayed,
            } = value
            {
                if *delayed {
                    let left_match = match_pattern(lhs, vl, attr_checker);
                    if let MatchResult::Match(mut bindings) = left_match
                        && let MatchResult::Match(right_bindings) =
                            match_pattern(rhs, vr, attr_checker)
                    {
                        bindings.extend(right_bindings);
                        return MatchResult::Match(bindings);
                    }
                }
                MatchResult::NoMatch
            } else {
                MatchResult::NoMatch
            }
        }

        // ── Default: no match ──
        _ => MatchResult::NoMatch,
    }
}

/// Recursively collect guard expressions from a pattern tree.
/// Guards nested inside lists, calls, and alternatives are collected
/// so that the evaluator can evaluate them after pattern matching.
pub fn collect_nested_guards(expr: &Expr, guards: &mut Vec<Expr>) {
    match expr {
        Expr::PatternGuard { pattern, condition } => {
            guards.push(condition.as_ref().clone());
            collect_nested_guards(pattern, guards);
        }
        Expr::List(items) => {
            for item in items {
                collect_nested_guards(item, guards);
            }
        }
        // `Condition[pat, cond]` is a guard written as a call.
        Expr::Call { head, args } if call_head_is(head, "Condition") && args.len() == 2 => {
            guards.push(args[1].clone());
            collect_nested_guards(&args[0], guards);
        }
        // `PatternTest[pat, test]` applies `test` to the matched value.
        Expr::Call { head, args } if call_head_is(head, "PatternTest") && args.len() == 2 => {
            guards.push(Expr::Call {
                head: Box::new(args[1].clone()),
                args: vec![Expr::Slot(None)],
            });
            collect_nested_guards(&args[0], guards);
        }
        Expr::Call { head, args } => {
            collect_nested_guards(head, guards);
            for arg in args {
                collect_nested_guards(arg, guards);
            }
        }
        Expr::BlankSequence {
            type_constraint, ..
        }
        | Expr::BlankNullSequence {
            type_constraint, ..
        } => {
            // type_constraint is a string (type name), not an expression — no guards to collect.
            let _ = type_constraint;
        }
        _ => {}
    }
}

/// A shallow binding for an unmatched optional pattern: the supplied default
/// expression converted to a value when it is an atom, otherwise a
/// `Value::Pattern` (which the evaluator can evaluate later), or `Null`.
fn optional_default_binding(default_value: &Option<Box<Expr>>) -> Value {
    match default_value {
        None => Value::Null,
        Some(d) => match d.as_ref() {
            Expr::Integer(n) => Value::Integer(n.clone()),
            Expr::Real(r) => Value::Real(r.clone()),
            Expr::Str(s) => Value::Str(s.clone()),
            Expr::Bool(b) => Value::Bool(*b),
            Expr::Null => Value::Null,
            other => Value::Pattern(other.clone()),
        },
    }
}

/// If an `Optional[named_, default]` call falls back to matching nothing, bind
/// the name to the default (or `Null`).
fn bind_optional_call_default(args: &[Expr], bindings: &mut Bindings) {
    if let Some(Expr::NamedBlank { name, .. }) = args.first() {
        let default = optional_default_binding(&args.get(1).cloned().map(Box::new));
        bindings.insert(name.clone(), default);
    }
}

/// True when a `Call` head is the given symbol.
fn call_head_is(head: &Expr, name: &str) -> bool {
    matches!(head, Expr::Symbol(s) if s == name)
}

/// Parse an integer expression as a `usize`.
fn expr_usize(expr: &Expr) -> Option<usize> {
    match expr {
        Expr::Integer(n) => n.to_usize(),
        _ => None,
    }
}

/// Bounds `(min, max)` for `Repeated`/`RepeatedNull`.
///
/// `Repeated[p]` = 1+, `Repeated[p, max]` = 1..=max, `Repeated[p, min, max]`
/// = min..=max. `RepeatedNull` variants have a default minimum of 0. `max`
/// is `None` for an unbounded run. Returns `None` for malformed argument lists.
fn repeated_bounds(head: &str, args: &[Expr]) -> Option<(usize, Option<usize>)> {
    let default_min = if head == "RepeatedNull" { 0 } else { 1 };
    let (min, max) = match args.len() {
        1 => (default_min, None),
        2 => (default_min, Some(expr_usize(&args[1])?)),
        3 => (expr_usize(&args[1])?, Some(expr_usize(&args[2])?)),
        _ => return None,
    };
    if let Some(m) = max
        && m < min
    {
        return None;
    }
    Some((min, max))
}

/// Minimum number of list elements a pattern must consume. Optional and null
/// patterns consume zero; this lets the backtracker reserve room for trailing
/// mandatory patterns instead of assuming one element each.
fn min_consume(pat: &Expr) -> usize {
    match pat {
        Expr::BlankSequence { .. } => 1,
        Expr::BlankNullSequence { .. } => 0,
        Expr::OptionalBlank { .. } | Expr::OptionalNamedBlank { .. } => 0,
        Expr::Call { head, args } => match head.as_ref() {
            Expr::Symbol(s) if s == "Repeated" => {
                repeated_bounds(s, args).map(|(mn, _)| mn).unwrap_or(1)
            }
            Expr::Symbol(s) if s == "RepeatedNull" => {
                repeated_bounds(s, args).map(|(mn, _)| mn).unwrap_or(0)
            }
            Expr::Symbol(s) if s == "Optional" => 0,
            Expr::Symbol(s) if s == "Alternatives" => {
                args.iter().map(min_consume).min().unwrap_or(0)
            }
            Expr::Symbol(s) if s == "Condition" || s == "PatternTest" => {
                args.first().map(min_consume).unwrap_or(1)
            }
            _ => 1,
        },
        _ => 1,
    }
}

/// Total minimum consumption of a slice of patterns.
fn min_consume_of(patterns: &[Expr]) -> usize {
    patterns.iter().map(min_consume).sum()
}

/// Copy of `patterns` with position `idx` replaced by `replacement`.
fn replace_pattern(patterns: &[Expr], idx: usize, replacement: &Expr) -> Vec<Expr> {
    let mut out = Vec::with_capacity(patterns.len());
    out.extend_from_slice(&patterns[..idx]);
    out.push(replacement.clone());
    out.extend_from_slice(&patterns[idx + 1..]);
    out
}

/// Recursive backtracking pattern matcher for lists with sequence patterns.
/// Tries different partition points for BlankSequence (__) and BlankNullSequence (___).
fn match_sequence_pattern(
    patterns: &[Expr],
    items: &[Value],
    pat_idx: usize,
    val_idx: usize,
    bindings: &mut Bindings,
    attr_checker: Option<&AttributeChecker>,
) -> MatchResult {
    // Base: all patterns consumed → check all values consumed too
    if pat_idx == patterns.len() {
        return if val_idx == items.len() {
            MatchResult::Match(bindings.clone())
        } else {
            MatchResult::NoMatch
        };
    }

    match &patterns[pat_idx] {
        Expr::BlankSequence {
            name,
            type_constraint,
        } => {
            // __ matches 1+ elements. Try from most-greedy to least
            // (backtracking), reserving room for the minimum consumption of the
            // trailing patterns.
            let remaining = items.len() - val_idx;
            let tail_min = min_consume_of(&patterns[pat_idx + 1..]);
            if remaining < tail_min {
                return MatchResult::NoMatch;
            }
            let max_for_seq = remaining - tail_min;
            if max_for_seq < 1 {
                return MatchResult::NoMatch;
            }
            for take in (1..=max_for_seq).rev() {
                let seq_items = &items[val_idx..val_idx + take];
                // Type constraint check
                if let Some(tc) = type_constraint
                    && !seq_items.iter().all(|v| v.matches_type(tc))
                {
                    continue;
                }
                let mut new_bindings = bindings.clone();
                let seq_val = Value::Sequence(seq_items.to_vec());
                if let Some(n) = name {
                    new_bindings.insert(n.clone(), seq_val);
                }
                if let MatchResult::Match(b) = match_sequence_pattern(
                    patterns,
                    items,
                    pat_idx + 1,
                    val_idx + take,
                    &mut new_bindings,
                    attr_checker,
                ) {
                    return MatchResult::Match(b);
                }
            }
            MatchResult::NoMatch
        }

        Expr::BlankNullSequence {
            name,
            type_constraint,
        } => {
            // ___ matches 0+ elements.
            let remaining = items.len() - val_idx;
            let tail_min = min_consume_of(&patterns[pat_idx + 1..]);
            if remaining < tail_min {
                return MatchResult::NoMatch;
            }
            let max_for_seq = remaining - tail_min;
            for take in (0..=max_for_seq).rev() {
                if take == 0 {
                    // Match zero elements
                    let mut new_bindings = bindings.clone();
                    if let Some(n) = name {
                        new_bindings.insert(n.clone(), Value::Sequence(vec![]));
                    }
                    if let MatchResult::Match(b) = match_sequence_pattern(
                        patterns,
                        items,
                        pat_idx + 1,
                        val_idx,
                        &mut new_bindings,
                        attr_checker,
                    ) {
                        return MatchResult::Match(b);
                    }
                } else {
                    let seq_items = &items[val_idx..val_idx + take];
                    if let Some(tc) = type_constraint
                        && !seq_items.iter().all(|v| v.matches_type(tc))
                    {
                        continue;
                    }
                    let mut new_bindings = bindings.clone();
                    let seq_val = Value::Sequence(seq_items.to_vec());
                    if let Some(n) = name {
                        new_bindings.insert(n.clone(), seq_val);
                    }
                    if let MatchResult::Match(b) = match_sequence_pattern(
                        patterns,
                        items,
                        pat_idx + 1,
                        val_idx + take,
                        &mut new_bindings,
                        attr_checker,
                    ) {
                        return MatchResult::Match(b);
                    }
                }
            }
            MatchResult::NoMatch
        }

        // Repeated[pat] / Repeated[pat, ...] / RepeatedNull[...] — consume a
        // run of consecutive elements that each match `pat`.
        Expr::Call { head, args }
            if call_head_is(head, "Repeated") || call_head_is(head, "RepeatedNull") =>
        {
            let head_name = if call_head_is(head, "RepeatedNull") {
                "RepeatedNull"
            } else {
                "Repeated"
            };
            let pat = match args.first() {
                Some(p) => p,
                None => return MatchResult::NoMatch,
            };
            let (min_take, max_take) = match repeated_bounds(head_name, args) {
                Some(b) => b,
                None => return MatchResult::NoMatch,
            };
            let remaining = items.len() - val_idx;
            let tail_min = min_consume_of(&patterns[pat_idx + 1..]);
            if remaining < tail_min {
                return MatchResult::NoMatch;
            }
            let room = remaining - tail_min;
            let max_take = max_take.map(|m| m.min(room)).unwrap_or(room);
            if max_take < min_take {
                return MatchResult::NoMatch;
            }
            for take in (min_take..=max_take).rev() {
                let mut ok = true;
                let mut new_bindings = bindings.clone();
                for item in &items[val_idx..val_idx + take] {
                    match match_pattern(pat, item, attr_checker) {
                        MatchResult::Match(b) => new_bindings.extend(b),
                        MatchResult::NoMatch => {
                            ok = false;
                            break;
                        }
                    }
                }
                if ok
                    && let MatchResult::Match(b) = match_sequence_pattern(
                        patterns,
                        items,
                        pat_idx + 1,
                        val_idx + take,
                        &mut new_bindings,
                        attr_checker,
                    )
                {
                    return MatchResult::Match(b);
                }
            }
            MatchResult::NoMatch
        }

        // Optional blank in a sequence: consume one element if it matches,
        // otherwise fall back to the default (consuming nothing).
        Expr::OptionalBlank {
            type_constraint, ..
        } => {
            if val_idx < items.len() {
                let normal = Expr::Blank {
                    type_constraint: type_constraint.clone(),
                };
                if let MatchResult::Match(b) = match_pattern(&normal, &items[val_idx], attr_checker)
                {
                    let mut nb = bindings.clone();
                    nb.extend(b);
                    if let MatchResult::Match(r) = match_sequence_pattern(
                        patterns,
                        items,
                        pat_idx + 1,
                        val_idx + 1,
                        &mut nb,
                        attr_checker,
                    ) {
                        return MatchResult::Match(r);
                    }
                }
            }
            let mut nb = bindings.clone();
            if let MatchResult::Match(r) =
                match_sequence_pattern(patterns, items, pat_idx + 1, val_idx, &mut nb, attr_checker)
            {
                return MatchResult::Match(r);
            }
            MatchResult::NoMatch
        }

        Expr::OptionalNamedBlank {
            name,
            type_constraint,
            default_value,
        } => {
            if val_idx < items.len() {
                let normal = Expr::NamedBlank {
                    name: name.clone(),
                    type_constraint: type_constraint.clone(),
                };
                if let MatchResult::Match(b) = match_pattern(&normal, &items[val_idx], attr_checker)
                {
                    let mut nb = bindings.clone();
                    nb.extend(b);
                    if let MatchResult::Match(r) = match_sequence_pattern(
                        patterns,
                        items,
                        pat_idx + 1,
                        val_idx + 1,
                        &mut nb,
                        attr_checker,
                    ) {
                        return MatchResult::Match(r);
                    }
                }
            }
            let mut nb = bindings.clone();
            nb.insert(name.clone(), optional_default_binding(default_value));
            if let MatchResult::Match(r) =
                match_sequence_pattern(patterns, items, pat_idx + 1, val_idx, &mut nb, attr_checker)
            {
                return MatchResult::Match(r);
            }
            MatchResult::NoMatch
        }

        // Alternatives in sequence position: inline each branch so that
        // branches of varying length (e.g. `__ | a_`) are handled.
        Expr::Call { head, args } if call_head_is(head, "Alternatives") => {
            for alt in args {
                let new_patterns = replace_pattern(patterns, pat_idx, alt);
                let mut nb = bindings.clone();
                if let MatchResult::Match(r) = match_sequence_pattern(
                    &new_patterns,
                    items,
                    pat_idx,
                    val_idx,
                    &mut nb,
                    attr_checker,
                ) {
                    return MatchResult::Match(r);
                }
            }
            MatchResult::NoMatch
        }

        // Condition/PatternTest wrapping a sequence pattern: unwrap and retry.
        Expr::Call { head, args }
            if (call_head_is(head, "Condition") || call_head_is(head, "PatternTest"))
                && args.len() == 2 =>
        {
            let new_patterns = replace_pattern(patterns, pat_idx, &args[0]);
            let mut nb = bindings.clone();
            match_sequence_pattern(
                &new_patterns,
                items,
                pat_idx,
                val_idx,
                &mut nb,
                attr_checker,
            )
        }

        // Optional[pat] as a call: try pat, else consume nothing.
        Expr::Call { head, args } if call_head_is(head, "Optional") && !args.is_empty() => {
            let new_patterns = replace_pattern(patterns, pat_idx, &args[0]);
            let mut nb = bindings.clone();
            if let MatchResult::Match(r) = match_sequence_pattern(
                &new_patterns,
                items,
                pat_idx,
                val_idx,
                &mut nb,
                attr_checker,
            ) {
                return MatchResult::Match(r);
            }
            let mut nb = bindings.clone();
            bind_optional_call_default(args, &mut nb);
            match_sequence_pattern(patterns, items, pat_idx + 1, val_idx, &mut nb, attr_checker)
        }

        // Regular pattern: match one element
        _ => {
            if val_idx >= items.len() {
                return MatchResult::NoMatch;
            }
            match match_pattern(&patterns[pat_idx], &items[val_idx], attr_checker) {
                MatchResult::Match(b) => {
                    bindings.extend(b);
                    match_sequence_pattern(
                        patterns,
                        items,
                        pat_idx + 1,
                        val_idx + 1,
                        bindings,
                        attr_checker,
                    )
                }
                MatchResult::NoMatch => MatchResult::NoMatch,
            }
        }
    }
}

/// True for patterns that can consume a variable number of list elements:
/// BlankSequence (`__`), BlankNullSequence (`___`), Optional (`_.`/`x_.`),
/// Repeated, RepeatedNull, and alternatives/condition wrappers of these.
fn is_sequence_like(p: &Expr) -> bool {
    match p {
        Expr::BlankSequence { .. }
        | Expr::BlankNullSequence { .. }
        | Expr::OptionalBlank { .. }
        | Expr::OptionalNamedBlank { .. } => true,
        Expr::Call { head, args } => match head.as_ref() {
            Expr::Symbol(s) if s == "Repeated" || s == "RepeatedNull" => true,
            Expr::Symbol(s) if s == "Optional" => true,
            Expr::Symbol(s) if s == "Alternatives" => args.iter().any(is_sequence_like),
            Expr::Symbol(s) if s == "Condition" || s == "PatternTest" => {
                args.first().is_some_and(is_sequence_like)
            }
            _ => false,
        },
        _ => false,
    }
}

/// Match a list pattern against a list value.
fn match_list_pattern(
    patterns: &[Expr],
    items: &[Value],
    attr_checker: Option<&AttributeChecker>,
) -> MatchResult {
    // Check if any sequence patterns exist
    let has_sequences = patterns.iter().any(is_sequence_like);

    if !has_sequences {
        // Fast path: no sequences, direct matching
        if patterns.len() != items.len() {
            return MatchResult::NoMatch;
        }
        let mut bindings = HashMap::new();
        for (pat, val) in patterns.iter().zip(items.iter()) {
            match match_pattern(pat, val, attr_checker) {
                MatchResult::Match(b) => bindings.extend(b),
                MatchResult::NoMatch => return MatchResult::NoMatch,
            }
        }
        return MatchResult::Match(bindings);
    }

    // Slow path: sequences with backtracking
    let mut bindings = Bindings::new();
    match_sequence_pattern(patterns, items, 0, 0, &mut bindings, attr_checker)
}

/// Handle `Repeated[pat]`, `Repeated[pat, max]`, `Repeated[pat, min, max]`
/// and the `RepeatedNull` variants against a whole list value.
///
/// `Repeated[pat]` matches `pat` one or more times consecutively in a list;
/// a `max` bound caps the run. `RepeatedNull` allows a zero-length run. A
/// single-argument pattern must match the whole list, while a bounded pattern
/// matches a prefix of the requested length (consistent with sequence use).
fn match_repeated_pattern(
    head_name: &str,
    args: &[Expr],
    value: &Value,
    attr_checker: Option<&AttributeChecker>,
) -> MatchResult {
    let items = match value {
        Value::List(items) => items,
        _ => return MatchResult::NoMatch,
    };

    let pat = match args.first() {
        Some(p) => p,
        None => return MatchResult::NoMatch,
    };

    let (min, max) = match repeated_bounds(head_name, args) {
        Some(b) => b,
        None => return MatchResult::NoMatch,
    };

    let n = items.len();
    if n < min {
        return MatchResult::NoMatch;
    }

    // A single-argument pattern must match the entire list; bounded patterns
    // try the largest permitted prefix first, then shrink.
    let candidates: Vec<usize> = if args.len() == 1 {
        vec![n]
    } else {
        let upper = max.map(|m| m.min(n)).unwrap_or(n);
        if upper < min {
            return MatchResult::NoMatch;
        }
        (min..=upper).rev().collect()
    };

    for take in candidates {
        let mut bindings = Bindings::new();
        let mut ok = true;
        for item in &items[..take] {
            match match_pattern(pat, item, attr_checker) {
                MatchResult::Match(b) => bindings.extend(b),
                MatchResult::NoMatch => {
                    ok = false;
                    break;
                }
            }
        }
        if ok {
            return MatchResult::Match(bindings);
        }
    }
    MatchResult::NoMatch
}

/// Handle `KeyValuePattern[{k -> v, ...}]` (and the single-rule form).
///
/// Matches an association — or a list of rules, which is its normalized form —
/// that contains at least the given key/value patterns. Values are matched as
/// patterns, so named blanks bind as usual. Keys may themselves be patterns.
fn match_key_value_pattern(
    args: &[Expr],
    value: &Value,
    attr_checker: Option<&AttributeChecker>,
) -> MatchResult {
    let rules: &[Expr] = match args {
        [Expr::List(items)] => items,
        [single] => std::slice::from_ref(single),
        _ => return MatchResult::NoMatch,
    };

    // Normalize the candidate value into ordered key/value pairs.
    let pairs: Vec<(String, Value)> = match value {
        Value::Assoc(map) => map.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        Value::List(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                match item {
                    Value::Rule {
                        lhs,
                        rhs,
                        delayed: false,
                    } => out.push((value_key_string(lhs), rhs.as_ref().clone())),
                    _ => return MatchResult::NoMatch,
                }
            }
            out
        }
        _ => return MatchResult::NoMatch,
    };

    let mut bindings = Bindings::new();
    for rule in rules {
        let (key_pat, val_pat) = match rule {
            Expr::Rule { lhs, rhs } | Expr::RuleDelayed { lhs, rhs } => {
                (lhs.as_ref(), rhs.as_ref())
            }
            _ => return MatchResult::NoMatch,
        };

        let mut matched_rule = false;
        if let Some(literal) = literal_key(key_pat) {
            if let Some((_, stored)) = pairs.iter().find(|(k, _)| *k == literal)
                && let MatchResult::Match(b) = match_pattern(val_pat, stored, attr_checker)
            {
                bindings.extend(b);
                matched_rule = true;
            }
        } else {
            for (k, stored) in &pairs {
                if let MatchResult::Match(key_bindings) =
                    match_pattern(key_pat, &Value::Str(k.clone()), attr_checker)
                    && let MatchResult::Match(val_bindings) =
                        match_pattern(val_pat, stored, attr_checker)
                {
                    bindings.extend(key_bindings);
                    bindings.extend(val_bindings);
                    matched_rule = true;
                    break;
                }
            }
        }

        if !matched_rule {
            return MatchResult::NoMatch;
        }
    }
    MatchResult::Match(bindings)
}

/// If a key expression is a literal, return its association-key string.
fn literal_key(key: &Expr) -> Option<String> {
    match key {
        Expr::Str(s) | Expr::Symbol(s) => Some(s.clone()),
        Expr::Integer(n) => Some(n.to_string()),
        Expr::Real(r) => Some(r.to_string()),
        _ => None,
    }
}

/// String form of a value used as an association key.
fn value_key_string(v: &Value) -> String {
    match v {
        Value::Str(s) | Value::Symbol(s) => s.clone(),
        Value::Integer(n) => n.to_string(),
        Value::Real(r) => r.to_string(),
        other => other.to_string(),
    }
}

/// Flatten nested calls with same head (for Flat attribute).
/// e.g., `Plus[Plus[x_, y_], z_]` → `[x_, y_, z_]` when head is "Plus".
fn flatten_expr_args(head: &str, args: &[Expr]) -> Vec<Expr> {
    let mut result = Vec::new();
    for arg in args {
        if let Expr::Call { head: h, args: a } = arg
            && let Expr::Symbol(s) = h.as_ref()
            && s == head
        {
            result.extend(flatten_expr_args(head, a));
            continue;
        }
        result.push(arg.clone());
    }
    result
}

/// Generate all permutations of a slice of indices (Heap's algorithm).
fn generate_permutations(indices: &[usize]) -> Vec<Vec<usize>> {
    if indices.is_empty() {
        return vec![vec![]];
    }
    let n = indices.len();
    let mut result = Vec::new();
    let mut c = vec![0usize; n];
    let mut p = indices.to_vec();
    result.push(p.clone());
    let mut i = 0;
    while i < n {
        if c[i] < i {
            if i % 2 == 0 {
                p.swap(0, i);
            } else {
                p.swap(c[i], i);
            }
            result.push(p.clone());
            c[i] += 1;
            i = 0;
        } else {
            c[i] = 0;
            i += 1;
        }
    }
    result
}

/// Merge `src` into `dst`, enforcing consistency of repeated pattern
/// variables (e.g. `f[x_, x_]` requires both slots to be equal). Returns
/// false on conflict.
fn merge_bindings_checked(dst: &mut Bindings, src: Bindings) -> bool {
    for (k, v) in src {
        match dst.get(&k) {
            Some(existing) if !existing.struct_eq(&v) => return false,
            _ => {
                dst.insert(k, v);
            }
        }
    }
    true
}

/// Check if there's a permutation of `val_args` that matches `pat_args`.
fn try_orderless_match(
    pat_args: &[Expr],
    val_args: &[Value],
    attr_checker: Option<&AttributeChecker>,
) -> MatchResult {
    let n = pat_args.len();
    let indices: Vec<usize> = (0..n).collect();
    for perm in generate_permutations(&indices) {
        let mut bindings = Bindings::new();
        let mut ok = true;
        for (i, &pi) in perm.iter().enumerate() {
            match match_pattern(&pat_args[i], &val_args[pi], attr_checker) {
                MatchResult::Match(b) => {
                    if !merge_bindings_checked(&mut bindings, b) {
                        ok = false;
                        break;
                    }
                }
                MatchResult::NoMatch => {
                    ok = false;
                    break;
                }
            }
        }
        if ok {
            return MatchResult::Match(bindings);
        }
    }
    MatchResult::NoMatch
}

/// Try a direct ordered match of pattern args against value args.
fn try_ordered_match(
    pat_args: &[Expr],
    val_args: &[Value],
    attr_checker: Option<&AttributeChecker>,
) -> MatchResult {
    if pat_args.len() != val_args.len() {
        return MatchResult::NoMatch;
    }
    let mut bindings = Bindings::new();
    for (pat, val) in pat_args.iter().zip(val_args.iter()) {
        match match_pattern(pat, val, attr_checker) {
            MatchResult::Match(b) => {
                if !merge_bindings_checked(&mut bindings, b) {
                    return MatchResult::NoMatch;
                }
            }
            MatchResult::NoMatch => return MatchResult::NoMatch,
        }
    }
    MatchResult::Match(bindings)
}

/// Match a call pattern (e.g., f[x_, y_]) against a value.
///
/// Respects Flat, Orderless, and OneIdentity attributes when an
/// `attr_checker` is provided.
fn match_call_pattern(
    head: &Expr,
    args: &[Expr],
    value: &Value,
    attr_checker: Option<&AttributeChecker>,
) -> MatchResult {
    // The parser represents `a/b` as `Divide[a, b]`, but `Integrate` (and WL
    // generally) express division as `Times[a, Power[b, -1]]`. Rewrite a
    // `Divide` pattern to its `Times` form so `/` rule patterns match the
    // normalized values produced by the integrator.
    if let Expr::Symbol(h) = head
        && h == "Divide"
        && args.len() == 2
        && !matches!(value, Value::Call { head: vh, .. } if vh == "Divide")
    {
        let rewritten = Expr::Call {
            head: Box::new(Expr::Symbol("Times".to_string())),
            args: vec![
                args[0].clone(),
                Expr::Call {
                    head: Box::new(Expr::Symbol("Power".to_string())),
                    args: vec![args[1].clone(), Expr::Integer(rug::Integer::from(-1))],
                },
            ],
        };
        return match_pattern(&rewritten, value, attr_checker);
    }

    // Match pattern head against value head.
    // The head can be a Symbol ("Plus"), a NamedBlank ("f_"), or a Blank ("_").
    let head_match: Option<(String, Bindings)> = match head {
        Expr::Symbol(s) => Some((s.clone(), HashMap::new())),
        _ => {
            // Pattern head (e.g., f_ or _) — match against any call's head symbol
            if let Value::Call { head: vhead, .. } = value {
                let mut b = HashMap::new();
                // For NamedBlank, bind the name; for Blank, ignore
                if let Expr::NamedBlank { name, .. } = head {
                    b.insert(name.clone(), Value::Symbol(vhead.clone()));
                }
                Some((vhead.clone(), b))
            } else {
                None
            }
        }
    };

    match value {
        Value::Call {
            head: vhead,
            args: vargs,
        } => {
            let (head_name, head_bindings) = match head_match {
                Some(hm) => hm,
                None => return MatchResult::NoMatch,
            };

            // For Symbol heads (not patterns), verify head matches value head
            if matches!(head, Expr::Symbol(_)) && head_name != *vhead {
                // OneIdentity fallback: try matching inner pattern directly
                if args.len() == 1
                    && head_is_one_identity(&head_name, attr_checker)
                    && let MatchResult::Match(b) = match_pattern(&args[0], value, attr_checker)
                {
                    return MatchResult::Match(b);
                }
                // OneIdentity fallback: treat the bare value as `head[value]`
                // with identity elements for the remaining pattern slots.
                if head_is_one_identity(&head_name, attr_checker)
                    && let Some(b) =
                        match_one_identity_pattern(&head_name, args, value, attr_checker)
                {
                    return MatchResult::Match(b);
                }
                return MatchResult::NoMatch;
            }

            // Determine attributes
            let is_flat = attr_checker.is_some_and(|c| c.has_attr(&head_name, "Flat"));
            let is_orderless = attr_checker.is_some_and(|c| c.has_attr(&head_name, "Orderless"));
            let is_one_identity =
                attr_checker.is_some_and(|c| c.has_attr(&head_name, "OneIdentity"));

            // Flatten args for Flat
            let pat_args: Vec<Expr> = if is_flat {
                flatten_expr_args(&head_name, args)
            } else {
                args.to_vec()
            };
            let val_args: Vec<Value> = if is_flat {
                crate::eval::flatten_flat_args(&head_name, vargs)
            } else {
                vargs.clone()
            };

            // Check if any pattern args contain sequences (__, ___, Repeated)
            let has_sequences = pat_args.iter().any(is_sequence_like);

            if has_sequences {
                // Use recursive backtracking sequence matcher for __ and ___
                let mut bindings = head_bindings;
                return match_sequence_pattern(
                    &pat_args,
                    &val_args,
                    0,
                    0,
                    &mut bindings,
                    attr_checker,
                );
            }

            // 1. Try direct ordered match
            if let MatchResult::Match(mut b) = try_ordered_match(&pat_args, &val_args, attr_checker)
            {
                b.extend(head_bindings);
                return MatchResult::Match(b);
            }

            // 2. Orderless: try permutations of value args (max 6 elements)
            if is_orderless
                && pat_args.len() == val_args.len()
                && val_args.len() <= 6
                && val_args.len() >= 2
                && let MatchResult::Match(mut b) =
                    try_orderless_match(&pat_args, &val_args, attr_checker)
            {
                b.extend(head_bindings);
                return MatchResult::Match(b);
            }

            // 3. OneIdentity: for single arg, try matching inner pattern directly
            if is_one_identity
                && args.len() == 1
                && vargs.len() == 1
                && let MatchResult::Match(b) = match_pattern(&args[0], &vargs[0], attr_checker)
            {
                return MatchResult::Match(b);
            }

            MatchResult::NoMatch
        }
        _ => {
            // Not a call value. With OneIdentity and single-arg pattern, try direct match.
            if let Expr::Symbol(s) = head
                && head_is_one_identity(s, attr_checker)
            {
                if args.len() == 1
                    && let MatchResult::Match(b) = match_pattern(&args[0], value, attr_checker)
                {
                    return MatchResult::Match(b);
                }
                // OneIdentity: treat the bare value as `head[value]` with
                // identity elements filling the remaining pattern slots.
                if let Some(b) = match_one_identity_pattern(s, args, value, attr_checker) {
                    return MatchResult::Match(b);
                }
            }
            MatchResult::NoMatch
        }
    }
}

/// Try to match a value against a list of patterns and return the first match.
///
/// Returns (index, bindings) for the first matching pattern, or None.
pub fn match_first(
    patterns: &[Expr],
    value: &Value,
    attr_checker: Option<&AttributeChecker>,
) -> Option<(usize, Bindings)> {
    for (i, pattern) in patterns.iter().enumerate() {
        if let MatchResult::Match(bindings) = match_pattern(pattern, value, attr_checker) {
            return Some((i, bindings));
        }
    }
    None
}

/// Apply a set of rules to a value.
///
/// Returns the first matching rule's RHS with bindings substituted, or None.
pub fn apply_rules(
    rules: &[(Expr, Expr)],
    value: &Value,
    attr_checker: Option<&AttributeChecker>,
) -> Option<Value> {
    for (lhs, rhs) in rules {
        if let MatchResult::Match(bindings) = match_pattern(lhs, value, attr_checker) {
            return Some(substitute(rhs, &bindings));
        }
    }
    None
}

/// Substitute bindings into an expression.
fn substitute(expr: &Expr, bindings: &Bindings) -> Value {
    match expr {
        Expr::Symbol(s) => {
            if let Some(val) = bindings.get(s) {
                val.clone()
            } else {
                Value::Symbol(s.clone())
            }
        }
        Expr::Integer(n) => Value::Integer(n.clone()),
        Expr::Real(r) => Value::Real(r.clone()),
        Expr::Bool(b) => Value::Bool(*b),
        Expr::Str(s) => Value::Str(s.clone()),
        Expr::Null => Value::Null,
        Expr::List(items) => Value::List(
            items
                .iter()
                .map(|item| substitute(item, bindings))
                .collect(),
        ),
        Expr::Call { head, args } => {
            let h = substitute(head, bindings);
            let a: Vec<Value> = args.iter().map(|arg| substitute(arg, bindings)).collect();
            match h {
                Value::Symbol(name) => Value::Call {
                    head: name,
                    args: a,
                },
                _ => Value::Call {
                    head: h.to_string(),
                    args: a,
                },
            }
        }
        _ => Value::Pattern(expr.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rug::Integer;

    #[test]
    fn test_match_integer() {
        let pattern = Expr::Integer(Integer::from(42));
        let value = Value::Integer(Integer::from(42));
        assert!(matches!(
            match_pattern(&pattern, &value, None),
            MatchResult::Match(_)
        ));

        let value = Value::Integer(Integer::from(43));
        assert!(matches!(
            match_pattern(&pattern, &value, None),
            MatchResult::NoMatch
        ));
    }

    #[test]
    fn test_match_blank() {
        let pattern = Expr::Blank {
            type_constraint: None,
        };
        let value = Value::Integer(Integer::from(42));
        assert!(matches!(
            match_pattern(&pattern, &value, None),
            MatchResult::Match(_)
        ));
    }

    #[test]
    fn test_match_named_blank() {
        let pattern = Expr::NamedBlank {
            name: "x".to_string(),
            type_constraint: None,
        };
        let value = Value::Integer(Integer::from(42));
        if let MatchResult::Match(bindings) = match_pattern(&pattern, &value, None) {
            assert_eq!(bindings.get("x"), Some(&Value::Integer(Integer::from(42))));
        } else {
            panic!("Expected match");
        }
    }

    #[test]
    fn test_match_typed_blank() {
        let pattern = Expr::NamedBlank {
            name: "x".to_string(),
            type_constraint: Some("Integer".to_string()),
        };

        let value = Value::Integer(Integer::from(42));
        assert!(matches!(
            match_pattern(&pattern, &value, None),
            MatchResult::Match(_)
        ));

        let value = Value::Str("hello".to_string());
        assert!(matches!(
            match_pattern(&pattern, &value, None),
            MatchResult::NoMatch
        ));
    }

    #[test]
    fn test_match_list() {
        let pattern = Expr::List(vec![
            Expr::NamedBlank {
                name: "a".to_string(),
                type_constraint: None,
            },
            Expr::NamedBlank {
                name: "b".to_string(),
                type_constraint: None,
            },
        ]);
        let value = Value::List(vec![
            Value::Integer(Integer::from(1)),
            Value::Integer(Integer::from(2)),
        ]);
        if let MatchResult::Match(bindings) = match_pattern(&pattern, &value, None) {
            assert_eq!(bindings.get("a"), Some(&Value::Integer(Integer::from(1))));
            assert_eq!(bindings.get("b"), Some(&Value::Integer(Integer::from(2))));
        } else {
            panic!("Expected match");
        }
    }

    #[test]
    fn test_match_call() {
        let pattern = Expr::Call {
            head: Box::new(Expr::Symbol("f".to_string())),
            args: vec![Expr::NamedBlank {
                name: "x".to_string(),
                type_constraint: None,
            }],
        };
        let value = Value::Call {
            head: "f".to_string(),
            args: vec![Value::Integer(Integer::from(42))],
        };
        if let MatchResult::Match(bindings) = match_pattern(&pattern, &value, None) {
            assert_eq!(bindings.get("x"), Some(&Value::Integer(Integer::from(42))));
        } else {
            panic!("Expected match");
        }
    }

    // ── Sequence pattern backtracking ──

    #[test]
    fn test_blank_sequence_simple() {
        // {a__} should match all remaining elements
        let pattern = Expr::List(vec![Expr::BlankSequence {
            name: Some("a".to_string()),
            type_constraint: None,
        }]);
        let value = Value::List(vec![
            Value::Integer(Integer::from(1)),
            Value::Integer(Integer::from(2)),
            Value::Integer(Integer::from(3)),
        ]);
        if let MatchResult::Match(bindings) = match_pattern(&pattern, &value, None) {
            assert_eq!(
                bindings.get("a"),
                Some(&Value::Sequence(vec![
                    Value::Integer(Integer::from(1)),
                    Value::Integer(Integer::from(2)),
                    Value::Integer(Integer::from(3)),
                ]))
            );
        } else {
            panic!("Expected match");
        }
    }

    #[test]
    fn test_blank_sequence_with_trailing() {
        // {a__, b_} should backtrack: a__ takes 2, b_ takes 1
        let pattern = Expr::List(vec![
            Expr::BlankSequence {
                name: Some("a".to_string()),
                type_constraint: None,
            },
            Expr::NamedBlank {
                name: "b".to_string(),
                type_constraint: None,
            },
        ]);
        let value = Value::List(vec![
            Value::Integer(Integer::from(1)),
            Value::Integer(Integer::from(2)),
            Value::Integer(Integer::from(3)),
        ]);
        if let MatchResult::Match(bindings) = match_pattern(&pattern, &value, None) {
            assert_eq!(
                bindings.get("a"),
                Some(&Value::Sequence(vec![
                    Value::Integer(Integer::from(1)),
                    Value::Integer(Integer::from(2)),
                ]))
            );
            assert_eq!(bindings.get("b"), Some(&Value::Integer(Integer::from(3))));
        } else {
            panic!("Expected match");
        }
    }

    #[test]
    fn test_blank_sequence_multiple_trailing() {
        // {a__, b_, c_} — a__ takes 1, leaving one each for b_, c_
        let pattern = Expr::List(vec![
            Expr::BlankSequence {
                name: Some("a".to_string()),
                type_constraint: None,
            },
            Expr::NamedBlank {
                name: "b".to_string(),
                type_constraint: None,
            },
            Expr::NamedBlank {
                name: "c".to_string(),
                type_constraint: None,
            },
        ]);
        let value = Value::List(vec![
            Value::Integer(Integer::from(1)),
            Value::Integer(Integer::from(2)),
            Value::Integer(Integer::from(3)),
        ]);
        assert!(matches!(
            match_pattern(&pattern, &value, None),
            MatchResult::Match(_)
        ));
    }

    #[test]
    fn test_blank_null_sequence_with_trailing() {
        // {a___, b_} — a___ takes 0 elements (empty list), b_ takes first
        let pattern = Expr::List(vec![
            Expr::BlankNullSequence {
                name: Some("a".to_string()),
                type_constraint: None,
            },
            Expr::NamedBlank {
                name: "b".to_string(),
                type_constraint: None,
            },
        ]);
        let value = Value::List(vec![Value::Integer(Integer::from(1))]);
        if let MatchResult::Match(bindings) = match_pattern(&pattern, &value, None) {
            assert_eq!(bindings.get("a"), Some(&Value::Sequence(vec![])));
            assert_eq!(bindings.get("b"), Some(&Value::Integer(Integer::from(1))));
        } else {
            panic!("Expected match");
        }
    }

    #[test]
    fn test_blank_null_sequence_more() {
        // {a___, b_, c_} — a___ takes 0, b_ takes first, c_ takes second
        let pattern = Expr::List(vec![
            Expr::BlankNullSequence {
                name: Some("a".to_string()),
                type_constraint: None,
            },
            Expr::NamedBlank {
                name: "b".to_string(),
                type_constraint: None,
            },
            Expr::NamedBlank {
                name: "c".to_string(),
                type_constraint: None,
            },
        ]);
        let value = Value::List(vec![
            Value::Integer(Integer::from(1)),
            Value::Integer(Integer::from(2)),
        ]);
        if let MatchResult::Match(bindings) = match_pattern(&pattern, &value, None) {
            assert_eq!(bindings.get("a"), Some(&Value::Sequence(vec![])));
            assert_eq!(bindings.get("b"), Some(&Value::Integer(Integer::from(1))));
            assert_eq!(bindings.get("c"), Some(&Value::Integer(Integer::from(2))));
        } else {
            panic!("Expected match");
        }
    }

    #[test]
    fn test_multiple_sequences() {
        // {a__, b__} — a__ takes 0+? actually __ needs at least 1
        // a__ takes 2, b__ takes 1 (remaining)
        let pattern = Expr::List(vec![
            Expr::BlankSequence {
                name: Some("a".to_string()),
                type_constraint: None,
            },
            Expr::BlankSequence {
                name: Some("b".to_string()),
                type_constraint: None,
            },
        ]);
        let value = Value::List(vec![
            Value::Integer(Integer::from(1)),
            Value::Integer(Integer::from(2)),
            Value::Integer(Integer::from(3)),
        ]);
        if let MatchResult::Match(bindings) = match_pattern(&pattern, &value, None) {
            assert_eq!(
                bindings.get("a"),
                Some(&Value::Sequence(vec![
                    Value::Integer(Integer::from(1)),
                    Value::Integer(Integer::from(2)),
                ]))
            );
            assert_eq!(
                bindings.get("b"),
                Some(&Value::Sequence(vec![Value::Integer(Integer::from(3))]))
            );
        } else {
            panic!("Expected match");
        }
    }

    #[test]
    fn test_blank_sequence_no_match() {
        // {a__, b_} on list with 1 element — a__ needs at least 1, leaving 0 for b_
        let pattern = Expr::List(vec![
            Expr::BlankSequence {
                name: Some("a".to_string()),
                type_constraint: None,
            },
            Expr::NamedBlank {
                name: "b".to_string(),
                type_constraint: None,
            },
        ]);
        let value = Value::List(vec![Value::Integer(Integer::from(1))]);
        assert!(matches!(
            match_pattern(&pattern, &value, None),
            MatchResult::NoMatch
        ));
    }

    // ── Optional pattern (_. / x_.) tests ──

    #[test]
    fn test_optional_blank_matches() {
        // _. should match any value
        let pattern = Expr::OptionalBlank {
            type_constraint: None,
            default_value: None,
        };
        let value = Value::Integer(Integer::from(42));
        assert!(matches!(
            match_pattern(&pattern, &value, None),
            MatchResult::Match(_)
        ));
    }

    #[test]
    fn test_optional_named_blank_matches() {
        // x_. should match any value
        let pattern = Expr::OptionalNamedBlank {
            name: "x".to_string(),
            type_constraint: None,
            default_value: None,
        };
        let value = Value::Integer(Integer::from(42));
        if let MatchResult::Match(bindings) = match_pattern(&pattern, &value, None) {
            assert_eq!(bindings.get("x"), Some(&Value::Integer(Integer::from(42))));
        } else {
            panic!("Expected match");
        }
    }

    #[test]
    fn test_optional_named_blank_with_type() {
        // x_Integer. should only match integers
        let pattern = Expr::OptionalNamedBlank {
            name: "x".to_string(),
            type_constraint: Some("Integer".to_string()),
            default_value: None,
        };
        // Integer matches
        let val_int = Value::Integer(Integer::from(42));
        if let MatchResult::Match(bindings) = match_pattern(&pattern, &val_int, None) {
            assert_eq!(bindings.get("x"), Some(&Value::Integer(Integer::from(42))));
        } else {
            panic!("Expected match");
        }
        // String does NOT match (type constraint fails), but Optional still
        // succeeds by binding Null
        let val_str = Value::Str("hello".to_string());
        if let MatchResult::Match(bindings) = match_pattern(&pattern, &val_str, None) {
            assert_eq!(bindings.get("x"), Some(&Value::Null));
        } else {
            panic!("Expected optional match with Null default");
        }
    }

    #[test]
    fn test_optional_in_call_pattern() {
        // f[x_.] should match f[42] (binding x to 42)
        let pattern = Expr::Call {
            head: Box::new(Expr::Symbol("f".to_string())),
            args: vec![Expr::OptionalNamedBlank {
                name: "x".to_string(),
                type_constraint: None,
                default_value: None,
            }],
        };

        // f[42]
        let val = Value::Call {
            head: "f".to_string(),
            args: vec![Value::Integer(Integer::from(42))],
        };
        if let MatchResult::Match(bindings) = match_pattern(&pattern, &val, None) {
            assert_eq!(bindings.get("x"), Some(&Value::Integer(Integer::from(42))));
        } else {
            panic!("Expected match");
        }
    }

    // ── Except pattern tests ──

    #[test]
    fn test_except_matches_non_matching() {
        // Except[0] should match any value that is NOT 0
        let pat = Expr::Call {
            head: Box::new(Expr::Symbol("Except".to_string())),
            args: vec![Expr::Integer(Integer::from(0))],
        };
        // 1 != 0 → match
        assert!(matches!(
            match_pattern(&pat, &Value::Integer(Integer::from(1)), None),
            MatchResult::Match(_)
        ));
    }

    #[test]
    fn test_except_does_not_match_excluded() {
        // Except[0] should NOT match 0
        let pat = Expr::Call {
            head: Box::new(Expr::Symbol("Except".to_string())),
            args: vec![Expr::Integer(Integer::from(0))],
        };
        // 0 → no match
        assert!(matches!(
            match_pattern(&pat, &Value::Integer(Integer::from(0)), None),
            MatchResult::NoMatch
        ));
    }

    #[test]
    fn test_except_with_blank() {
        // Except[x_] matches any value but doesn't capture x
        let pat = Expr::Call {
            head: Box::new(Expr::Symbol("Except".to_string())),
            args: vec![Expr::NamedBlank {
                name: "x".to_string(),
                type_constraint: None,
            }],
        };
        // 42 would match x_, so Except[x_] should NOT match 42
        assert!(matches!(
            match_pattern(&pat, &Value::Integer(Integer::from(42)), None),
            MatchResult::NoMatch
        ));
    }

    // ── Repeated pattern tests ──

    #[test]
    fn test_repeated_matches_consecutive() {
        // Repeated[0] on list [0, 0, 0] should match all three
        let pat = Expr::Call {
            head: Box::new(Expr::Symbol("Repeated".to_string())),
            args: vec![Expr::Integer(Integer::from(0))],
        };
        let val = Value::List(vec![
            Value::Integer(Integer::from(0)),
            Value::Integer(Integer::from(0)),
            Value::Integer(Integer::from(0)),
        ]);
        assert!(matches!(
            match_pattern(&pat, &val, None),
            MatchResult::Match(_)
        ));
    }

    #[test]
    fn test_repeated_n_matches_exactly() {
        // Repeated[0, 2] on list [0, 0, 0] should match first two
        let pat = Expr::Call {
            head: Box::new(Expr::Symbol("Repeated".to_string())),
            args: vec![
                Expr::Integer(Integer::from(0)),
                Expr::Integer(Integer::from(2)),
            ],
        };
        let val = Value::List(vec![
            Value::Integer(Integer::from(0)),
            Value::Integer(Integer::from(0)),
            Value::Integer(Integer::from(0)),
        ]);
        assert!(matches!(
            match_pattern(&pat, &val, None),
            MatchResult::Match(_)
        ));
    }

    #[test]
    fn test_repeated_mismatch_returns_nomatch() {
        // Repeated[0] on list [1, 2] should not match
        let pat = Expr::Call {
            head: Box::new(Expr::Symbol("Repeated".to_string())),
            args: vec![Expr::Integer(Integer::from(0))],
        };
        let val = Value::List(vec![
            Value::Integer(Integer::from(1)),
            Value::Integer(Integer::from(2)),
        ]);
        assert!(matches!(
            match_pattern(&pat, &val, None),
            MatchResult::NoMatch
        ));
    }

    #[test]
    fn test_repeated_non_list_returns_nomatch() {
        // Repeated[0] on non-list should not match
        let pat = Expr::Call {
            head: Box::new(Expr::Symbol("Repeated".to_string())),
            args: vec![Expr::Integer(Integer::from(0))],
        };
        assert!(matches!(
            match_pattern(&pat, &Value::Integer(Integer::from(0)), None),
            MatchResult::NoMatch
        ));
    }

    // ── Unevaluated blank symbols (rule LHS / Switch cases / held args) ──

    #[test]
    fn test_symbol_typed_blank_pattern() {
        // "x_Integer" reaching the matcher as a raw symbol must be treated as a
        // typed named blank, not as a literal symbol.
        let pattern = Expr::Symbol("x_Integer".to_string());
        if let MatchResult::Match(bindings) =
            match_pattern(&pattern, &Value::Integer(Integer::from(42)), None)
        {
            assert_eq!(bindings.get("x"), Some(&Value::Integer(Integer::from(42))));
        } else {
            panic!("Expected typed blank to match Integer");
        }
        assert!(matches!(
            match_pattern(&pattern, &Value::Str("hi".to_string()), None),
            MatchResult::NoMatch
        ));
    }

    #[test]
    fn test_symbol_plain_and_typed_blank_pattern() {
        // "_" is an anonymous blank; "_Integer" is a type-constrained blank.
        let blank = Expr::Symbol("_".to_string());
        assert!(matches!(
            match_pattern(&blank, &Value::Integer(Integer::from(1)), None),
            MatchResult::Match(_)
        ));
        let typed = Expr::Symbol("_Integer".to_string());
        assert!(matches!(
            match_pattern(&typed, &Value::Integer(Integer::from(1)), None),
            MatchResult::Match(_)
        ));
        assert!(matches!(
            match_pattern(&typed, &Value::Str("s".to_string()), None),
            MatchResult::NoMatch
        ));
    }

    #[test]
    fn test_symbol_sequence_pattern() {
        // "x___" reaching the matcher raw must become a BlankNullSequence.
        let pattern = Expr::Symbol("x___".to_string());
        if let MatchResult::Match(bindings) =
            match_pattern(&pattern, &Value::Integer(Integer::from(7)), None)
        {
            assert_eq!(bindings.get("x"), Some(&Value::Integer(Integer::from(7))));
        } else {
            panic!("Expected blank null sequence to match");
        }
    }

    #[test]
    fn test_plain_symbol_still_exact_matches() {
        // A symbol without '_' remains a literal symbol pattern.
        let pattern = Expr::Symbol("foo".to_string());
        assert!(matches!(
            match_pattern(&pattern, &Value::Symbol("foo".to_string()), None),
            MatchResult::Match(_)
        ));
        assert!(matches!(
            match_pattern(&pattern, &Value::Symbol("bar".to_string()), None),
            MatchResult::NoMatch
        ));
    }

    // ── Repeated inside list patterns ──

    fn repeated_pat(inner: Expr, count: Option<i64>) -> Expr {
        let mut args = vec![inner];
        if let Some(n) = count {
            args.push(Expr::Integer(Integer::from(n)));
        }
        Expr::Call {
            head: Box::new(Expr::Symbol("Repeated".to_string())),
            args,
        }
    }

    #[test]
    fn test_repeated_in_list_pattern() {
        let pat = Expr::List(vec![repeated_pat(
            Expr::NamedBlank {
                name: "x".to_string(),
                type_constraint: Some("Integer".to_string()),
            },
            None,
        )]);
        // All integers -> match
        let val = Value::List(vec![
            Value::Integer(Integer::from(1)),
            Value::Integer(Integer::from(2)),
            Value::Integer(Integer::from(3)),
        ]);
        assert!(matches!(
            match_pattern(&pat, &val, None),
            MatchResult::Match(_)
        ));
        // Non-integer element -> no match
        let val2 = Value::List(vec![
            Value::Integer(Integer::from(1)),
            Value::Str("a".to_string()),
        ]);
        assert!(matches!(
            match_pattern(&pat, &val2, None),
            MatchResult::NoMatch
        ));
        // Repeated matches 1+ elements; empty list does not match
        assert!(matches!(
            match_pattern(&pat, &Value::List(vec![]), None),
            MatchResult::NoMatch
        ));
    }

    #[test]
    fn test_repeated_n_in_list_pattern_with_trailing() {
        let pat = Expr::List(vec![
            repeated_pat(Expr::Integer(Integer::from(1)), Some(2)),
            Expr::NamedBlank {
                name: "x".to_string(),
                type_constraint: None,
            },
        ]);
        let val = Value::List(vec![
            Value::Integer(Integer::from(1)),
            Value::Integer(Integer::from(1)),
            Value::Integer(Integer::from(5)),
        ]);
        assert!(matches!(
            match_pattern(&pat, &val, None),
            MatchResult::Match(_)
        ));
    }

    #[test]
    fn test_match_pattern_value_terminates() {
        // A Pattern value whose unwrap is another identical Pattern must not
        // recurse forever.
        let pattern = Expr::Symbol("x_".to_string());
        let value = Value::Pattern(Expr::NamedBlank {
            name: "x".to_string(),
            type_constraint: None,
        });
        if let MatchResult::Match(bindings) = match_pattern(&pattern, &value, None) {
            assert!(bindings.contains_key("x"));
        } else {
            panic!("Expected match against pattern value");
        }
    }

    // ── Helpers for the hardening tests ──

    fn int(n: i64) -> Value {
        Value::Integer(Integer::from(n))
    }

    fn named(name: &str) -> Expr {
        Expr::NamedBlank {
            name: name.to_string(),
            type_constraint: None,
        }
    }

    fn opt_named(name: &str, default: Option<i64>) -> Expr {
        Expr::OptionalNamedBlank {
            name: name.to_string(),
            type_constraint: None,
            default_value: default.map(|d| Box::new(Expr::Integer(Integer::from(d)))),
        }
    }

    fn alt(branches: Vec<Expr>) -> Expr {
        Expr::Call {
            head: Box::new(Expr::Symbol("Alternatives".to_string())),
            args: branches,
        }
    }

    fn repeat(head: &str, inner: Expr, bounds: &[i64]) -> Expr {
        let mut args = vec![inner];
        args.extend(bounds.iter().map(|n| Expr::Integer(Integer::from(*n))));
        Expr::Call {
            head: Box::new(Expr::Symbol(head.to_string())),
            args,
        }
    }

    fn call(head: &str, args: Vec<Expr>) -> Expr {
        Expr::Call {
            head: Box::new(Expr::Symbol(head.to_string())),
            args,
        }
    }

    // ── Alternatives ──

    #[test]
    fn test_alternatives_ordered_first_match_wins() {
        let pat = alt(vec![int_expr(0), named("x")]);
        // First branch matches 0.
        if let MatchResult::Match(b) = match_pattern(&pat, &int(0), None) {
            assert!(b.is_empty(), "literal branch binds nothing");
        } else {
            panic!("expected literal alternative to match");
        }
        // Second branch is used for non-zero values and binds x.
        if let MatchResult::Match(b) = match_pattern(&pat, &int(5), None) {
            assert_eq!(b.get("x"), Some(&int(5)));
        } else {
            panic!("expected fallback alternative to match");
        }
    }

    #[test]
    fn test_alternatives_sequence_branch_consumes_all() {
        // {(a__ | b_)} on a 3-element list: the sequence branch must be able
        // to consume the whole run rather than a single element.
        let pat = Expr::List(vec![alt(vec![
            Expr::BlankSequence {
                name: Some("a".to_string()),
                type_constraint: None,
            },
            named("b"),
        ])]);
        let val = Value::List(vec![int(1), int(2), int(3)]);
        if let MatchResult::Match(b) = match_pattern(&pat, &val, None) {
            assert_eq!(
                b.get("a"),
                Some(&Value::Sequence(vec![int(1), int(2), int(3)]))
            );
        } else {
            panic!("expected the sequence branch of the alternative to match");
        }
    }

    fn int_expr(n: i64) -> Expr {
        Expr::Integer(Integer::from(n))
    }

    // ── Except ──

    #[test]
    fn test_except_two_arg_matches_inner_pattern() {
        // Except[_Integer, _String] matches strings (not integers).
        let pat = call(
            "Except",
            vec![
                Expr::Blank {
                    type_constraint: Some("Integer".to_string()),
                },
                Expr::Blank {
                    type_constraint: Some("String".to_string()),
                },
            ],
        );
        assert!(matches!(
            match_pattern(&pat, &Value::Str("a".to_string()), None),
            MatchResult::Match(_)
        ));
        // An integer is excluded by the first pattern.
        assert!(matches!(
            match_pattern(&pat, &int(1), None),
            MatchResult::NoMatch
        ));
        // A real matches neither the exclusion nor the required pattern.
        assert!(matches!(
            match_pattern(&pat, &Value::Real(rug::Float::with_val(53, 1.5)), None),
            MatchResult::NoMatch
        ));
    }

    #[test]
    fn test_except_two_arg_keeps_bindings() {
        let pat = call("Except", vec![Expr::Integer(Integer::from(0)), named("x")]);
        if let MatchResult::Match(b) = match_pattern(&pat, &int(7), None) {
            assert_eq!(b.get("x"), Some(&int(7)));
        } else {
            panic!("expected Except to keep bindings from its second pattern");
        }
    }

    // ── Repeated / RepeatedNull ──

    #[test]
    fn test_repeated_null_matches_empty_run() {
        let pat = Expr::List(vec![repeat("RepeatedNull", int_expr(0), &[])]);
        assert!(matches!(
            match_pattern(&pat, &Value::List(vec![]), None),
            MatchResult::Match(_)
        ));
        // Still matches a run of zeros.
        let val = Value::List(vec![int(0), int(0)]);
        assert!(matches!(
            match_pattern(&pat, &val, None),
            MatchResult::Match(_)
        ));
    }

    #[test]
    fn test_repeated_null_bounded_with_trailing() {
        // {RepeatedNull[0, 2], x_} on {0, 0, 5}: the bounded run takes the two
        // zeros and x_ takes 5.
        let pat = Expr::List(vec![repeat("RepeatedNull", int_expr(0), &[2]), named("x")]);
        let val = Value::List(vec![int(0), int(0), int(5)]);
        if let MatchResult::Match(b) = match_pattern(&pat, &val, None) {
            assert_eq!(b.get("x"), Some(&int(5)));
        } else {
            panic!("expected bounded RepeatedNull to match");
        }
    }

    #[test]
    fn test_repeated_min_max_bounds() {
        // Repeated[_, 2, 3] on a 2- and 3-element list matches, on 4 does not
        // (as a prefix it must fit the trailing mandatory pattern).
        let pat = Expr::List(vec![repeat(
            "Repeated",
            Expr::Blank {
                type_constraint: None,
            },
            &[2, 3],
        )]);
        assert!(matches!(
            match_pattern(&pat, &Value::List(vec![int(1), int(2)]), None),
            MatchResult::Match(_)
        ));
        assert!(matches!(
            match_pattern(&pat, &Value::List(vec![int(1), int(2), int(3)]), None),
            MatchResult::Match(_)
        ));
        assert!(matches!(
            match_pattern(&pat, &Value::List(vec![int(1)]), None),
            MatchResult::NoMatch
        ));
    }

    #[test]
    fn test_repeated_respects_trailing_mandatory() {
        // {Repeated[_Integer, 3], x_} on {1,2,3}: the repeated run must leave
        // one element for x_.
        let pat = Expr::List(vec![
            repeat("Repeated", Expr::Symbol("_Integer".to_string()), &[3]),
            named("x"),
        ]);
        let val = Value::List(vec![int(1), int(2), int(3)]);
        if let MatchResult::Match(b) = match_pattern(&pat, &val, None) {
            assert_eq!(b.get("x"), Some(&int(3)));
        } else {
            panic!("expected Repeated to backtrack and leave room for x_");
        }
    }

    // ── Sequence blanks / optional interaction ──

    #[test]
    fn test_blank_sequence_before_optional_trailing() {
        // {a__, b_.} on {1}: a__ takes the element, b_. falls back to Null.
        let pat = Expr::List(vec![
            Expr::BlankSequence {
                name: Some("a".to_string()),
                type_constraint: None,
            },
            opt_named("b", None),
        ]);
        let val = Value::List(vec![int(1)]);
        if let MatchResult::Match(b) = match_pattern(&pat, &val, None) {
            assert_eq!(b.get("a"), Some(&Value::Sequence(vec![int(1)])));
            assert_eq!(b.get("b"), Some(&Value::Null));
        } else {
            panic!("expected sequence followed by optional to match");
        }
    }

    #[test]
    fn test_optional_in_list_uses_default() {
        // {a_, b_:5} on {1}: b uses its default 5.
        let pat = Expr::List(vec![named("a"), opt_named("b", Some(5))]);
        let val = Value::List(vec![int(1)]);
        if let MatchResult::Match(b) = match_pattern(&pat, &val, None) {
            assert_eq!(b.get("a"), Some(&int(1)));
            assert_eq!(b.get("b"), Some(&int(5)));
        } else {
            panic!("expected optional to apply its default");
        }
    }

    #[test]
    fn test_optional_prefers_consuming_element() {
        // {a_:9, b_} on {1, 2}: a consumes 1 rather than defaulting.
        let pat = Expr::List(vec![opt_named("a", Some(9)), named("b")]);
        let val = Value::List(vec![int(1), int(2)]);
        if let MatchResult::Match(b) = match_pattern(&pat, &val, None) {
            assert_eq!(b.get("a"), Some(&int(1)));
            assert_eq!(b.get("b"), Some(&int(2)));
        } else {
            panic!("expected optional to consume an available element");
        }
    }

    #[test]
    fn test_optional_backtracks_for_mandatory_pattern() {
        // {a_:9, b_} on {1}: a must default so b_ can take 1.
        let pat = Expr::List(vec![opt_named("a", Some(9)), named("b")]);
        let val = Value::List(vec![int(1)]);
        if let MatchResult::Match(b) = match_pattern(&pat, &val, None) {
            assert_eq!(b.get("a"), Some(&int(9)));
            assert_eq!(b.get("b"), Some(&int(1)));
        } else {
            panic!("expected optional to backtrack for the trailing pattern");
        }
    }

    #[test]
    fn test_multiple_null_sequences_with_trailing() {
        // {___, ___, x_} on {1,2,3}: both null sequences empty, x_ takes 3.
        let pat = Expr::List(vec![
            Expr::BlankNullSequence {
                name: Some("a".to_string()),
                type_constraint: None,
            },
            Expr::BlankNullSequence {
                name: Some("b".to_string()),
                type_constraint: None,
            },
            named("x"),
        ]);
        let val = Value::List(vec![int(1), int(2), int(3)]);
        if let MatchResult::Match(b) = match_pattern(&pat, &val, None) {
            assert_eq!(b.get("x"), Some(&int(3)));
        } else {
            panic!("expected null sequences to leave room for x_");
        }
    }

    #[test]
    fn test_optional_call_form_binds_default() {
        // {a_, Optional[b_, 5]} on {1}: the Optional fallback binds b to 5.
        let pat = Expr::List(vec![
            named("a"),
            call("Optional", vec![named("b"), int_expr(5)]),
        ]);
        let val = Value::List(vec![int(1)]);
        if let MatchResult::Match(bindings) = match_pattern(&pat, &val, None) {
            assert_eq!(bindings.get("b"), Some(&int(5)));
        } else {
            panic!("expected Optional call to fall back to its default");
        }
        // With a spare element it consumes and binds that element instead.
        let val2 = Value::List(vec![int(1), int(2)]);
        if let MatchResult::Match(bindings) = match_pattern(&pat, &val2, None) {
            assert_eq!(bindings.get("b"), Some(&int(2)));
        } else {
            panic!("expected Optional call to consume an available element");
        }
    }

    // ── Condition / PatternTest as calls ──

    #[test]
    fn test_condition_call_matches_inner_pattern() {
        let pat = call(
            "Condition",
            vec![int_expr(1), Expr::Symbol("True".to_string())],
        );
        assert!(matches!(
            match_pattern(&pat, &int(1), None),
            MatchResult::Match(_)
        ));
        assert!(matches!(
            match_pattern(&pat, &int(2), None),
            MatchResult::NoMatch
        ));
    }

    #[test]
    fn test_patterntest_call_matches_inner_pattern() {
        let pat = call(
            "PatternTest",
            vec![named("x"), Expr::Symbol("EvenQ".to_string())],
        );
        if let MatchResult::Match(b) = match_pattern(&pat, &int(4), None) {
            assert_eq!(b.get("x"), Some(&int(4)));
        } else {
            panic!("expected PatternTest to match its inner pattern");
        }
    }

    #[test]
    fn test_collect_nested_guards_condition_and_patterntest() {
        let pat = Expr::List(vec![
            call(
                "Condition",
                vec![named("a"), Expr::Symbol("g1".to_string())],
            ),
            call(
                "PatternTest",
                vec![named("b"), Expr::Symbol("g2".to_string())],
            ),
        ]);
        let mut guards = Vec::new();
        collect_nested_guards(&pat, &mut guards);
        assert_eq!(guards.len(), 2);
        // First guard is `g1`; second is the application `g2[#]`.
        assert_eq!(guards[0], Expr::Symbol("g1".to_string()));
        match &guards[1] {
            Expr::Call { head, args } => {
                assert_eq!(head.as_ref(), &Expr::Symbol("g2".to_string()));
                assert_eq!(args.len(), 1);
                assert!(matches!(args[0], Expr::Slot(None)));
            }
            other => panic!("expected g2[#], got {other:?}"),
        }
    }

    // ── Structural equality edges ──

    #[test]
    fn test_nested_literal_structural_match() {
        let pat = Expr::List(vec![
            int_expr(1),
            call("f", vec![Expr::Str("a".to_string())]),
        ]);
        let good = Value::List(vec![
            int(1),
            Value::Call {
                head: "f".to_string(),
                args: vec![Value::Str("a".to_string())],
            },
        ]);
        let bad = Value::List(vec![
            int(1),
            Value::Call {
                head: "f".to_string(),
                args: vec![Value::Str("b".to_string())],
            },
        ]);
        assert!(matches!(
            match_pattern(&pat, &good, None),
            MatchResult::Match(_)
        ));
        assert!(matches!(
            match_pattern(&pat, &bad, None),
            MatchResult::NoMatch
        ));
    }

    #[test]
    fn test_verbatim_structural_edge() {
        let pat = call("Verbatim", vec![call("f", vec![int_expr(1)])]);
        let good = Value::Call {
            head: "f".to_string(),
            args: vec![int(1)],
        };
        // f[1] matches, f[2] does not, and the match binds nothing.
        if let MatchResult::Match(b) = match_pattern(&pat, &good, None) {
            assert!(b.is_empty());
        } else {
            panic!("expected Verbatim to match literally");
        }
        let bad = Value::Call {
            head: "f".to_string(),
            args: vec![int(2)],
        };
        assert!(matches!(
            match_pattern(&pat, &bad, None),
            MatchResult::NoMatch
        ));
    }

    // ── Pathological backtracking ──

    #[test]
    fn test_many_sequences_terminates() {
        // Six consecutive null sequences over 30 elements must resolve.
        let mut patterns = Vec::new();
        for name in ["a", "b", "c", "d", "e", "f"] {
            patterns.push(Expr::BlankNullSequence {
                name: Some(name.to_string()),
                type_constraint: None,
            });
        }
        patterns.push(named("last"));
        let pat = Expr::List(patterns);
        let val = Value::List((0..30).map(int).collect());
        if let MatchResult::Match(b) = match_pattern(&pat, &val, None) {
            assert_eq!(b.get("last"), Some(&int(29)));
        } else {
            panic!("expected many-sequence pattern to match");
        }
    }

    #[test]
    fn test_many_sequences_no_match_backtracks() {
        // A trailing literal that can never be reached forces full backtracking.
        let mut patterns = Vec::new();
        for name in ["a", "b", "c", "d"] {
            patterns.push(Expr::BlankSequence {
                name: Some(name.to_string()),
                type_constraint: None,
            });
        }
        patterns.push(int_expr(99));
        let pat = Expr::List(patterns);
        let val = Value::List((0..20).map(int).collect());
        assert!(matches!(
            match_pattern(&pat, &val, None),
            MatchResult::NoMatch
        ));
    }
}
