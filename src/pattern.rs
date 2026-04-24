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
    if let Value::Call { head, args } = value {
        if args.len() == 1 && attr_checker.map_or(false, |c| c.has_attr(head, "OneIdentity")) {
            return Some(&args[0]);
        }
    }
    None
}

/// Try to match a value against a pattern.
///
/// Returns Match(bindings) on success, NoMatch on failure.
/// `attr_checker` provides access to symbol attributes (Flat, Orderless, OneIdentity)
/// for call pattern matching. Pass `None` to skip attribute-based matching.
pub fn match_pattern(
    pattern: &Expr,
    value: &Value,
    attr_checker: Option<&AttributeChecker>,
) -> MatchResult {
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
            ..
        } => {
            // Try normal named blank match first
            let normal_match = Expr::NamedBlank {
                name: name.clone(),
                type_constraint: type_constraint.clone(),
            };
            match match_pattern(&normal_match, value, attr_checker) {
                MatchResult::Match(bindings) => MatchResult::Match(bindings),
                MatchResult::NoMatch => {
                    // Optional: bind to Null
                    let mut bindings = HashMap::new();
                    bindings.insert(name.clone(), Value::Null);
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

        // ── Symbol (matches exact symbol or named blank) ──
        Expr::Symbol(s) => {
            // Handle x_ as a named blank pattern (parsed as Symbol("x_"))
            if s.ends_with('_') && s.len() > 1 {
                let name = &s[..s.len() - 1];
                let mut bindings = HashMap::new();
                bindings.insert(name.to_string(), value.clone());
                MatchResult::Match(bindings)
            } else if let Value::Symbol(v) = value {
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

            // Check for Except: Except[pat] matches anything not matching pat
            if matches!(head.as_ref(), Expr::Symbol(s) if s == "Except") && args.len() == 1 {
                return match match_pattern(&args[0], value, attr_checker) {
                    MatchResult::Match(_) => MatchResult::NoMatch,
                    MatchResult::NoMatch => MatchResult::Match(HashMap::new()),
                };
            }

            // Check for Repeated: Repeated[pat] matches pat 1+ times in a list.
            // Repeated[pat, n] matches pat exactly n times in a list.
            if matches!(head.as_ref(), Expr::Symbol(s) if s == "Repeated") {
                return match_repeated_pattern(args, value, attr_checker);
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

    // Base: all values consumed → check if remaining patterns are optional
    if val_idx == items.len() {
        let all_optional = patterns[pat_idx..]
            .iter()
            .all(|p| matches!(p, Expr::BlankNullSequence { .. }));
        return if all_optional {
            let mut b = bindings.clone();
            for p in &patterns[pat_idx..] {
                if let Expr::BlankNullSequence { name: Some(n), .. } = p {
                    b.insert(n.clone(), Value::Sequence(vec![]));
                }
            }
            MatchResult::Match(b)
        } else {
            MatchResult::NoMatch
        };
    }

    match &patterns[pat_idx] {
        Expr::BlankSequence {
            name,
            type_constraint,
        } => {
            // __ matches 1+ elements. Try from most-greedy to least (backtracking).
            let remaining = items.len() - val_idx;
            let trailing_count = patterns.len() - pat_idx - 1;
            // Leave at least 1 element for each trailing non-optional pattern
            let max_for_seq = remaining.saturating_sub(trailing_count);
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
            let trailing_count = patterns.len() - pat_idx - 1;
            // Leave at least one element per trailing non-___ pattern
            let max_for_seq = remaining.saturating_sub(trailing_count);
            for take in (0..=max_for_seq.min(remaining)).rev() {
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

/// Match a list pattern against a list value.
fn match_list_pattern(
    patterns: &[Expr],
    items: &[Value],
    attr_checker: Option<&AttributeChecker>,
) -> MatchResult {
    // Check if any sequence patterns exist
    let has_sequences = patterns.iter().any(|p| {
        matches!(
            p,
            Expr::BlankSequence { .. } | Expr::BlankNullSequence { .. }
        )
    });

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

/// Handle `Repeated[pat]` and `Repeated[pat, n]` in pattern matching.
///
/// `Repeated[pat]` matches `pat` one or more times consecutively in a list.
/// `Repeated[pat, n]` matches `pat` exactly `n` times in a list.
fn match_repeated_pattern(
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

    let count = if args.len() == 2 {
        match &args[1] {
            Expr::Integer(n) => match n.to_usize() {
                Some(v) if v > 0 => v,
                _ => return MatchResult::NoMatch,
            },
            _ => return MatchResult::NoMatch,
        }
    } else if args.len() == 1 {
        items.len() // match all items (1+)
    } else {
        return MatchResult::NoMatch;
    };

    if items.len() < count {
        return MatchResult::NoMatch;
    }

    // Try partitioning at `count` — only meaningful partition
    if count <= items.len() {
        let matched_items = &items[..count];
        let mut bindings = Bindings::new();
        for item in matched_items {
            match match_pattern(pat, item, attr_checker) {
                MatchResult::Match(b) => bindings.extend(b),
                MatchResult::NoMatch => return MatchResult::NoMatch,
            }
        }
        MatchResult::Match(bindings)
    } else {
        MatchResult::NoMatch
    }
}

/// Flatten nested calls with the same head (for Flat attribute).
/// e.g., `Plus[Plus[x_, y_], z_]` → `[x_, y_, z_]` when head is "Plus".
fn flatten_expr_args(head: &str, args: &[Expr]) -> Vec<Expr> {
    let mut result = Vec::new();
    for arg in args {
        if let Expr::Call { head: h, args: a } = arg {
            if let Expr::Symbol(s) = h.as_ref() {
                if s == head {
                    result.extend(flatten_expr_args(head, a));
                    continue;
                }
            }
        }
        result.push(arg.clone());
    }
    result
}

/// Flatten nested value calls with the same head (for Flat attribute).
fn flatten_value_args(head: &str, args: &[Value]) -> Vec<Value> {
    let mut result = Vec::new();
    for arg in args {
        if let Value::Call { head: h, args: a } = arg {
            if h == head {
                result.extend(flatten_value_args(head, a));
                continue;
            }
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
            MatchResult::Match(b) => bindings.extend(b),
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
    // Extract head name for attribute checks
    let head_name = match head {
        Expr::Symbol(s) => s.clone(),
        _ => return MatchResult::NoMatch,
    };

    match value {
        Value::Call {
            head: vhead,
            args: vargs,
        } => {
            // Head must match
            if head_name != *vhead {
                // OneIdentity: if head has OneIdentity and 1 pattern arg,
                // try matching the inner pattern directly against the value.
                if args.len() == 1
                    && attr_checker.map_or(false, |c| c.has_attr(&head_name, "OneIdentity"))
                {
                    if let MatchResult::Match(b) = match_pattern(&args[0], value, attr_checker) {
                        return MatchResult::Match(b);
                    }
                }
                return MatchResult::NoMatch;
            }

            // Determine attributes
            let is_flat = attr_checker.map_or(false, |c| c.has_attr(&head_name, "Flat"));
            let is_orderless = attr_checker.map_or(false, |c| c.has_attr(&head_name, "Orderless"));
            let is_one_identity =
                attr_checker.map_or(false, |c| c.has_attr(&head_name, "OneIdentity"));

            // Flatten args for Flat
            let pat_args: Vec<Expr> = if is_flat {
                flatten_expr_args(&head_name, args)
            } else {
                args.to_vec()
            };
            let val_args: Vec<Value> = if is_flat {
                flatten_value_args(&head_name, vargs)
            } else {
                vargs.clone()
            };

            // 1. Try direct ordered match
            if let MatchResult::Match(b) = try_ordered_match(&pat_args, &val_args, attr_checker) {
                return MatchResult::Match(b);
            }

            // 2. Orderless: try permutations of value args (max 6 elements)
            if is_orderless
                && pat_args.len() == val_args.len()
                && val_args.len() <= 6
                && val_args.len() >= 2
            {
                if let MatchResult::Match(b) =
                    try_orderless_match(&pat_args, &val_args, attr_checker)
                {
                    return MatchResult::Match(b);
                }
            }

            // 3. OneIdentity: for single arg, try matching inner pattern directly
            if is_one_identity && args.len() == 1 && vargs.len() == 1 {
                if let MatchResult::Match(b) = match_pattern(&args[0], &vargs[0], attr_checker) {
                    return MatchResult::Match(b);
                }
            }

            MatchResult::NoMatch
        }
        _ => {
            // Not a call value. With OneIdentity and single-arg pattern, try direct match.
            if args.len() == 1
                && attr_checker.map_or(false, |c| c.has_attr(&head_name, "OneIdentity"))
            {
                if let MatchResult::Match(b) = match_pattern(&args[0], value, attr_checker) {
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
}
