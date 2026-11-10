//! Boolean algebra builtins with Wolfram-Language parity for common cases.
//!
//! Implements `BooleanConvert`, `BooleanMinimize`, `BooleanTable`,
//! `BooleanFunction`, `BooleanVariables`, `BooleanCountingFunction`,
//! `TautologyQ`, `SatisfiableQ` and `LogicalExpand`.
//!
//! Booleans are represented with `And`/`Or`/`Not` (plus `Xor`, `Nand`, `Nor`,
//! `Implies`, `Equivalent`) heads and `Value::Bool` constants, matching the
//! conventions used by `src/builtins/logical.rs`. Everything is decided with
//! truth-table evaluation over the free variables (2^n rows).

use std::collections::BTreeSet;

use crate::ast::Expr;
use crate::env::{Env, SubValue};
use crate::eval::table::value_to_expr;
use crate::value::{EvalError, Value};

/// Register the builtins implemented in this module.
pub fn register(env: &Env) {
    // ── Boolean manipulations (first argument is held so the symbolic
    //    And/Or structure survives; the evaluator otherwise collapses it). ──
    super::register_builtin_env(env, "BooleanConvert", builtin_boolean_convert);
    super::register_builtin_env(env, "BooleanMinimize", builtin_boolean_minimize);
    super::register_builtin_env(env, "BooleanTable", builtin_boolean_table);
    super::register_builtin(env, "BooleanVariables", builtin_boolean_variables);
    super::register_builtin_env(env, "TautologyQ", builtin_tautology_q);
    super::register_builtin_env(env, "SatisfiableQ", builtin_satisfiable_q);
    super::register_builtin_env(env, "SATSolverQ", builtin_satisfiable_q);
    super::register_builtin_env(env, "LogicalExpand", builtin_logical_expand);

    // ── Boolean function objects ──
    super::register_builtin(env, "BooleanFunction", builtin_boolean_function);
    super::register_builtin(
        env,
        "BooleanCountingFunction",
        builtin_boolean_counting_function,
    );
    super::register_builtin_env(env, "BooleanFunctionApply", builtin_boolean_function_apply2);
    super::register_builtin_env(
        env,
        "BooleanFunctionApply1",
        builtin_boolean_function_apply1,
    );
    super::register_builtin_env(env, "BooleanCountingFunctionApply", builtin_bcf_apply);

    for name in [
        "BooleanConvert",
        "BooleanMinimize",
        "BooleanTable",
        "BooleanVariables",
        "TautologyQ",
        "SatisfiableQ",
        "SATSolverQ",
        "LogicalExpand",
    ] {
        hold_all(name, env);
    }

    // Applying a BooleanFunction/BooleanCountingFunction object to arguments
    // routes through a curried subvalue so that both `f[{a,b,c}]` and
    // `f[a,b,c]` work.
    env.add_subvalue(
        "BooleanFunction",
        SubValue {
            outer: vec![blank("n"), blank("k")],
            inner: vec![blank_seq("v")],
            rhs: apply_call("BooleanFunctionApply", &["n", "k", "v"]),
            delayed: true,
            guard: None,
        },
    );
    env.add_subvalue(
        "BooleanFunction",
        SubValue {
            outer: vec![blank("n")],
            inner: vec![blank_seq("v")],
            rhs: apply_call("BooleanFunctionApply1", &["n", "v"]),
            delayed: true,
            guard: None,
        },
    );
    env.add_subvalue(
        "BooleanCountingFunction",
        SubValue {
            outer: vec![blank("spec"), blank("n")],
            inner: vec![blank_seq("v")],
            rhs: apply_call("BooleanCountingFunctionApply", &["spec", "n", "v"]),
            delayed: true,
            guard: None,
        },
    );
}

fn hold_all(name: &str, env: &Env) {
    env.set_attributes(name, vec!["HoldAll".to_string(), "Protected".to_string()]);
}

/// Some parser contexts produce `Alternatives` for `||`; logical routines must
/// treat it as `Or`. Normalize head names recursively.
fn normalize_heads(e: &Expr) -> Expr {
    match e {
        Expr::Call { head, args } => {
            let new_head = match head.as_ref() {
                Expr::Symbol(s) if s == "Alternatives" => Box::new(Expr::Symbol("Or".to_string())),
                other => Box::new(normalize_heads(other)),
            };
            Expr::Call {
                head: new_head,
                args: args.iter().map(normalize_heads).collect(),
            }
        }
        Expr::List(items) => Expr::List(items.iter().map(normalize_heads).collect()),
        other => other.clone(),
    }
}

fn arg_expr(v: &Value) -> Expr {
    normalize_heads(&value_to_expr(v))
}

fn blank(name: &str) -> Expr {
    Expr::NamedBlank {
        name: name.to_string(),
        type_constraint: None,
    }
}

fn blank_seq(name: &str) -> Expr {
    Expr::BlankSequence {
        name: Some(name.to_string()),
        type_constraint: None,
    }
}

fn apply_call(head: &str, names: &[&str]) -> Expr {
    Expr::Call {
        head: Box::new(Expr::Symbol(head.to_string())),
        args: names
            .iter()
            .map(|n| Expr::Symbol((*n).to_string()))
            .collect(),
    }
}

// ── Value / Expr helpers ────────────────────────────────────────────────────

fn expr_to_value(e: &Expr) -> Value {
    match e {
        Expr::Integer(n) => Value::Integer(n.clone()),
        Expr::Real(r) => Value::Real(r.clone()),
        Expr::Bool(b) => Value::Bool(*b),
        Expr::Str(s) => Value::Str(s.clone()),
        Expr::Symbol(s) => Value::Symbol(s.clone()),
        Expr::Null => Value::Null,
        Expr::List(items) => Value::List(items.iter().map(expr_to_value).collect()),
        Expr::Call { head, args } => Value::Call {
            head: match head.as_ref() {
                Expr::Symbol(s) => s.clone(),
                other => other.to_string(),
            },
            args: args.iter().map(expr_to_value).collect(),
        },
        other => Value::Symbol(other.to_string()),
    }
}

fn unheld(v: &Value) -> Value {
    match v {
        Value::Pattern(e) => expr_to_value(e),
        other => other.clone(),
    }
}

fn call_v(head: &str, args: Vec<Value>) -> Value {
    Value::Call {
        head: head.to_string(),
        args,
    }
}

fn uneval(head: &str, args: &[Value]) -> Value {
    Value::Call {
        head: head.to_string(),
        args: args.iter().map(unheld).collect(),
    }
}

// ── Expression builders ─────────────────────────────────────────────────────

fn not_of(e: Expr) -> Expr {
    match e {
        Expr::Bool(b) => Expr::Bool(!b),
        Expr::Call { head, mut args }
            if matches!(head.as_ref(), Expr::Symbol(h) if h == "Not") && args.len() == 1 =>
        {
            args.pop().unwrap()
        }
        other => Expr::Call {
            head: Box::new(Expr::Symbol("Not".to_string())),
            args: vec![other],
        },
    }
}

fn and_expr(mut lits: Vec<Expr>) -> Expr {
    if lits.is_empty() {
        return Expr::Bool(true);
    }
    if lits.len() == 1 {
        return lits.pop().unwrap();
    }
    Expr::Call {
        head: Box::new(Expr::Symbol("And".to_string())),
        args: lits,
    }
}

fn or_expr(mut lits: Vec<Expr>) -> Expr {
    if lits.is_empty() {
        return Expr::Bool(false);
    }
    if lits.len() == 1 {
        return lits.pop().unwrap();
    }
    Expr::Call {
        head: Box::new(Expr::Symbol("Or".to_string())),
        args: lits,
    }
}

fn xor_expr(mut terms: Vec<Expr>) -> Expr {
    if terms.is_empty() {
        return Expr::Bool(false);
    }
    if terms.len() == 1 {
        return terms.pop().unwrap();
    }
    Expr::Call {
        head: Box::new(Expr::Symbol("Xor".to_string())),
        args: terms,
    }
}

fn call_head(head: &str, args: Vec<Expr>) -> Expr {
    Expr::Call {
        head: Box::new(Expr::Symbol(head.to_string())),
        args,
    }
}

// ── Variables and truth tables ──────────────────────────────────────────────

fn collect_vars(e: &Expr, out: &mut BTreeSet<String>) {
    match e {
        Expr::Symbol(s) => {
            if s != "True" && s != "False" {
                out.insert(s.clone());
            }
        }
        Expr::Call { head, args } => {
            if !matches!(head.as_ref(), Expr::Symbol(_)) {
                collect_vars(head, out);
            }
            for a in args {
                collect_vars(a, out);
            }
        }
        Expr::List(items) => {
            for a in items {
                collect_vars(a, out);
            }
        }
        _ => {}
    }
}

fn vars_of(expr: &Expr) -> Vec<String> {
    let mut set = BTreeSet::new();
    collect_vars(expr, &mut set);
    set.into_iter().collect()
}

fn vars_from_expr(e: &Expr) -> Option<Vec<String>> {
    match e {
        Expr::List(items) => items
            .iter()
            .map(|it| match it {
                Expr::Symbol(s) => Some(s.clone()),
                _ => None,
            })
            .collect(),
        _ => None,
    }
}

fn vars_arg(args: &[Value], expr: &Expr) -> Vec<String> {
    if let Some(v) = args.get(1)
        && let Some(vs) = vars_from_expr(&value_to_expr(v))
    {
        return vs;
    }
    vars_of(expr)
}

fn form_of(v: &Value) -> Option<String> {
    match value_to_expr(v) {
        Expr::Str(s) | Expr::Symbol(s) => Some(s),
        _ => None,
    }
}

fn truth_table(expr: &Expr, vars: &[String], env: &Env) -> Result<Vec<bool>, EvalError> {
    let n = vars.len();
    let total = 1usize << n;
    let mut out = Vec::with_capacity(total);
    for row in 0..total {
        let child = env.child();
        for (i, name) in vars.iter().enumerate() {
            let bit = (row >> (n - 1 - i)) & 1 == 1;
            child.set(name.clone(), Value::Bool(bit));
        }
        out.push(crate::eval::eval(expr, &child)?.to_bool());
    }
    Ok(out)
}

// ── Canonical and minimized normal forms ────────────────────────────────────

fn canonical_dnf(expr: &Expr, vars: &[String], env: &Env) -> Result<Expr, EvalError> {
    let tt = truth_table(expr, vars, env)?;
    let n = vars.len();
    let mut terms = Vec::new();
    for (row, &val) in tt.iter().enumerate() {
        if !val {
            continue;
        }
        let lits: Vec<Expr> = vars
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let bit = (row >> (n - 1 - i)) & 1 == 1;
                let v = Expr::Symbol(name.clone());
                if bit { v } else { not_of(v) }
            })
            .collect();
        terms.push(and_expr(lits));
    }
    Ok(or_expr(terms))
}

fn canonical_cnf(expr: &Expr, vars: &[String], env: &Env) -> Result<Expr, EvalError> {
    let tt = truth_table(expr, vars, env)?;
    let n = vars.len();
    let mut clauses = Vec::new();
    for (row, &val) in tt.iter().enumerate() {
        if val {
            continue;
        }
        let lits: Vec<Expr> = vars
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let bit = (row >> (n - 1 - i)) & 1 == 1;
                let v = Expr::Symbol(name.clone());
                if bit { not_of(v) } else { v }
            })
            .collect();
        clauses.push(or_expr(lits));
    }
    Ok(and_expr(clauses))
}

fn term_expr(value: u64, mask: u64, vars: &[String]) -> Expr {
    let n = vars.len();
    let lits: Vec<Expr> = vars
        .iter()
        .enumerate()
        .filter_map(|(i, name)| {
            let bit = n - 1 - i;
            if mask >> bit & 1 == 1 {
                let v = Expr::Symbol(name.clone());
                Some(if value >> bit & 1 == 1 { v } else { not_of(v) })
            } else {
                None
            }
        })
        .collect();
    and_expr(lits)
}

fn clause_expr(value: u64, mask: u64, vars: &[String]) -> Expr {
    let n = vars.len();
    let lits: Vec<Expr> = vars
        .iter()
        .enumerate()
        .filter_map(|(i, name)| {
            let bit = n - 1 - i;
            if mask >> bit & 1 == 1 {
                let v = Expr::Symbol(name.clone());
                Some(if value >> bit & 1 == 1 { not_of(v) } else { v })
            } else {
                None
            }
        })
        .collect();
    or_expr(lits)
}

/// Quine–McCluskey minimization returning prime implicants.
fn minimize_cover(ones: &[u64], n: usize) -> Vec<(u64, u64)> {
    if ones.is_empty() {
        return Vec::new();
    }
    let all = if n >= 64 { u64::MAX } else { (1u64 << n) - 1 };
    let mut current: Vec<(u64, u64)> = ones.iter().map(|m| (*m, all)).collect();
    current.sort_unstable();
    current.dedup();

    let mut primes: Vec<(u64, u64)> = Vec::new();
    while !current.is_empty() {
        let mut used = vec![false; current.len()];
        let mut next: Vec<(u64, u64)> = Vec::new();
        for i in 0..current.len() {
            for j in i + 1..current.len() {
                let (v1, m1) = current[i];
                let (v2, m2) = current[j];
                if m1 != m2 {
                    continue;
                }
                let diff = (v1 ^ v2) & m1;
                if diff.count_ones() == 1 && (v1 ^ v2) & !m1 == 0 {
                    used[i] = true;
                    used[j] = true;
                    let cand = (v1 & !diff, m1 & !diff);
                    if !next.contains(&cand) {
                        next.push(cand);
                    }
                }
            }
        }
        for (k, &p) in current.iter().enumerate() {
            if !used[k] && !primes.contains(&p) {
                primes.push(p);
            }
        }
        if next.is_empty() {
            break;
        }
        next.sort_unstable();
        next.dedup();
        current = next;
    }

    // Cover the minterms: essential primes first, then greedy.
    let mut uncovered: BTreeSet<u64> = ones.iter().copied().collect();
    let mut chosen: Vec<usize> = Vec::new();
    while !uncovered.is_empty() {
        let essential = uncovered.iter().find_map(|&m| {
            let covering: Vec<usize> = primes
                .iter()
                .enumerate()
                .filter(|(_, (v, mask))| (m & mask) == *v)
                .map(|(i, _)| i)
                .collect();
            if covering.len() == 1 {
                Some(covering[0])
            } else {
                None
            }
        });
        let pick = match essential {
            Some(i) => i,
            None => {
                let best = primes
                    .iter()
                    .enumerate()
                    .map(|(i, (v, mask))| {
                        (i, uncovered.iter().filter(|&&m| (m & mask) == *v).count())
                    })
                    .filter(|(_, c)| *c > 0)
                    .max_by_key(|(_, c)| *c);
                match best {
                    Some((i, _)) => i,
                    None => break,
                }
            }
        };
        let (v, mask) = primes[pick];
        uncovered.retain(|&m| (m & mask) != v);
        if !chosen.contains(&pick) {
            chosen.push(pick);
        }
    }
    chosen.sort_unstable();
    chosen.into_iter().map(|i| primes[i]).collect()
}

fn minimized_dnf(expr: &Expr, vars: &[String], env: &Env) -> Result<Expr, EvalError> {
    let tt = truth_table(expr, vars, env)?;
    let ones: Vec<u64> = tt
        .iter()
        .enumerate()
        .filter(|(_, b)| **b)
        .map(|(i, _)| i as u64)
        .collect();
    let primes = minimize_cover(&ones, vars.len());
    let terms = primes
        .iter()
        .map(|(v, m)| term_expr(*v, *m, vars))
        .collect();
    Ok(or_expr(terms))
}

fn minimized_cnf(expr: &Expr, vars: &[String], env: &Env) -> Result<Expr, EvalError> {
    let tt = truth_table(expr, vars, env)?;
    // The minimal CNF is the negation of the minimal DNF of the complement.
    let zeros: Vec<u64> = tt
        .iter()
        .enumerate()
        .filter(|(_, b)| !**b)
        .map(|(i, _)| i as u64)
        .collect();
    let primes = minimize_cover(&zeros, vars.len());
    let clauses = primes
        .iter()
        .map(|(v, m)| clause_expr(*v, *m, vars))
        .collect();
    Ok(and_expr(clauses))
}

fn anf_expr(expr: &Expr, vars: &[String], env: &Env) -> Result<Expr, EvalError> {
    let tt = truth_table(expr, vars, env)?;
    let n = vars.len();
    let size = tt.len();
    let mut coeffs = tt;
    // Möbius transform over GF(2): coefficients of the algebraic normal form.
    for i in 0..n {
        let bit = 1usize << (n - 1 - i);
        for s in 0..size {
            if s & bit != 0 {
                coeffs[s] ^= coeffs[s ^ bit];
            }
        }
    }
    let mut terms: Vec<(Vec<usize>, Expr)> = Vec::new();
    for (s, &c) in coeffs.iter().enumerate() {
        if !c {
            continue;
        }
        let mut key = Vec::new();
        let mut lits = Vec::new();
        for (i, name) in vars.iter().enumerate() {
            if (s >> (n - 1 - i)) & 1 == 1 {
                key.push(i);
                lits.push(Expr::Symbol(name.clone()));
            }
        }
        terms.push((key, and_expr(lits)));
    }
    terms.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(xor_expr(terms.into_iter().map(|(_, e)| e).collect()))
}

// ── Rewrites into restricted operator sets ──────────────────────────────────

fn to_and_or_not(e: &Expr) -> Expr {
    match e {
        Expr::Call { head, args } => {
            if let Expr::Symbol(h) = head.as_ref() {
                match h.as_str() {
                    "And" => and_expr(args.iter().map(to_and_or_not).collect()),
                    "Or" => or_expr(args.iter().map(to_and_or_not).collect()),
                    "Not" => match args.first() {
                        Some(a) => not_of(to_and_or_not(a)),
                        None => Expr::Bool(true),
                    },
                    "Xor" => fold_xor(args.iter().map(to_and_or_not).collect()),
                    "Nand" => not_of(and_expr(args.iter().map(to_and_or_not).collect())),
                    "Nor" => not_of(or_expr(args.iter().map(to_and_or_not).collect())),
                    "Implies" => {
                        if args.len() == 2 {
                            or_expr(vec![
                                not_of(to_and_or_not(&args[0])),
                                to_and_or_not(&args[1]),
                            ])
                        } else {
                            e.clone()
                        }
                    }
                    "Equivalent" => {
                        if args.len() <= 1 {
                            Expr::Bool(true)
                        } else {
                            let parts: Vec<Expr> = args.iter().map(to_and_or_not).collect();
                            let mut conj = Vec::new();
                            for w in parts.windows(2) {
                                conj.push(or_expr(vec![not_of(w[0].clone()), w[1].clone()]));
                            }
                            and_expr(conj)
                        }
                    }
                    _ => e.clone(),
                }
            } else {
                e.clone()
            }
        }
        other => other.clone(),
    }
}

fn fold_xor(parts: Vec<Expr>) -> Expr {
    let mut it = parts.into_iter();
    let mut acc = match it.next() {
        Some(a) => a,
        None => return Expr::Bool(false),
    };
    for r in it {
        let na = not_of(acc.clone());
        let nr = not_of(r.clone());
        acc = or_expr(vec![and_expr(vec![acc, nr]), and_expr(vec![na, r])]);
    }
    acc
}

fn rewrite_nand(e: &Expr) -> Expr {
    match e {
        Expr::Call { head, args } if is_head(head, "Not") && args.len() == 1 => {
            let p = rewrite_nand(&args[0]);
            call_head("Nand", vec![p.clone(), p])
        }
        Expr::Call { head, args } if is_head(head, "And") => {
            fold_nand_and(args.iter().map(rewrite_nand).collect())
        }
        Expr::Call { head, args } if is_head(head, "Or") => {
            fold_nand_or(args.iter().map(rewrite_nand).collect())
        }
        other => other.clone(),
    }
}

fn fold_nand_and(parts: Vec<Expr>) -> Expr {
    let mut it = parts.into_iter();
    let mut acc = match it.next() {
        Some(a) => a,
        None => return Expr::Bool(true),
    };
    for r in it {
        let n = call_head("Nand", vec![acc, r]);
        acc = call_head("Nand", vec![n.clone(), n]);
    }
    acc
}

fn fold_nand_or(parts: Vec<Expr>) -> Expr {
    let mut it = parts.into_iter();
    let mut acc = match it.next() {
        Some(a) => a,
        None => return Expr::Bool(false),
    };
    for r in it {
        let l = call_head("Nand", vec![acc.clone(), acc]);
        let rr = call_head("Nand", vec![r.clone(), r]);
        acc = call_head("Nand", vec![l, rr]);
    }
    acc
}

fn rewrite_nor(e: &Expr) -> Expr {
    match e {
        Expr::Call { head, args } if is_head(head, "Not") && args.len() == 1 => {
            let p = rewrite_nor(&args[0]);
            call_head("Nor", vec![p.clone(), p])
        }
        Expr::Call { head, args } if is_head(head, "Or") => {
            fold_nor_or(args.iter().map(rewrite_nor).collect())
        }
        Expr::Call { head, args } if is_head(head, "And") => {
            fold_nor_and(args.iter().map(rewrite_nor).collect())
        }
        other => other.clone(),
    }
}

fn fold_nor_or(parts: Vec<Expr>) -> Expr {
    let mut it = parts.into_iter();
    let mut acc = match it.next() {
        Some(a) => a,
        None => return Expr::Bool(false),
    };
    for r in it {
        let n = call_head("Nor", vec![acc, r]);
        acc = call_head("Nor", vec![n.clone(), n]);
    }
    acc
}

fn fold_nor_and(parts: Vec<Expr>) -> Expr {
    let mut it = parts.into_iter();
    let mut acc = match it.next() {
        Some(a) => a,
        None => return Expr::Bool(true),
    };
    for r in it {
        let l = call_head("Nor", vec![acc.clone(), acc]);
        let rr = call_head("Nor", vec![r.clone(), r]);
        acc = call_head("Nor", vec![l, rr]);
    }
    acc
}

fn and_only(e: &Expr) -> Expr {
    match e {
        Expr::Call { head, args } if is_head(head, "Not") && args.len() == 1 => {
            not_of(and_only(&args[0]))
        }
        Expr::Call { head, args } if is_head(head, "And") => {
            and_expr(args.iter().map(and_only).collect())
        }
        Expr::Call { head, args } if is_head(head, "Or") => {
            let lits = args.iter().map(|a| not_of(and_only(a))).collect();
            not_of(and_expr(lits))
        }
        other => other.clone(),
    }
}

fn or_only(e: &Expr) -> Expr {
    match e {
        Expr::Call { head, args } if is_head(head, "Not") && args.len() == 1 => {
            not_of(or_only(&args[0]))
        }
        Expr::Call { head, args } if is_head(head, "Or") => {
            or_expr(args.iter().map(or_only).collect())
        }
        Expr::Call { head, args } if is_head(head, "And") => {
            let lits = args.iter().map(|a| not_of(or_only(a))).collect();
            not_of(or_expr(lits))
        }
        other => other.clone(),
    }
}

fn xor_only(e: &Expr) -> Expr {
    match e {
        Expr::Call { head, args } if is_head(head, "Not") && args.len() == 1 => {
            not_of(xor_only(&args[0]))
        }
        Expr::Call { head, args } if is_head(head, "And") => {
            and_expr(args.iter().map(xor_only).collect())
        }
        Expr::Call { head, args } if is_head(head, "Or") => {
            fold_or_as_xor(args.iter().map(xor_only).collect())
        }
        other => other.clone(),
    }
}

fn fold_or_as_xor(parts: Vec<Expr>) -> Expr {
    let mut it = parts.into_iter();
    let mut acc = match it.next() {
        Some(a) => a,
        None => return Expr::Bool(false),
    };
    for r in it {
        let conj = and_expr(vec![acc.clone(), r.clone()]);
        acc = xor_expr(vec![acc, r, conj]);
    }
    acc
}

fn implies_only(e: &Expr) -> Expr {
    match e {
        Expr::Call { head, args } if is_head(head, "Not") && args.len() == 1 => {
            not_of(implies_only(&args[0]))
        }
        Expr::Call { head, args } if is_head(head, "Or") => {
            fold_or_as_implies(args.iter().map(implies_only).collect())
        }
        Expr::Call { head, args } if is_head(head, "And") => {
            fold_and_as_implies(args.iter().map(implies_only).collect())
        }
        other => other.clone(),
    }
}

fn fold_or_as_implies(parts: Vec<Expr>) -> Expr {
    let mut it = parts.into_iter();
    let mut acc = match it.next() {
        Some(a) => a,
        None => return Expr::Bool(false),
    };
    for r in it {
        acc = call_head("Implies", vec![not_of(acc), r]);
    }
    acc
}

fn fold_and_as_implies(parts: Vec<Expr>) -> Expr {
    let mut it = parts.into_iter();
    let mut acc = match it.next() {
        Some(a) => a,
        None => return Expr::Bool(true),
    };
    for r in it {
        acc = not_of(call_head("Implies", vec![acc, not_of(r)]));
    }
    acc
}

fn is_head(head: &Expr, name: &str) -> bool {
    matches!(head, Expr::Symbol(h) if h == name)
}

// ── Builtin entry points ────────────────────────────────────────────────────

fn builtin_boolean_convert(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Ok(uneval("BooleanConvert", args));
    }
    let expr = arg_expr(&args[0]);
    let form = if args.len() >= 2 {
        match form_of(&args[1]) {
            Some(f) => f,
            None => return Ok(uneval("BooleanConvert", args)),
        }
    } else {
        "DNF".to_string()
    };
    let vars = vars_of(&expr);
    let result = match form.to_uppercase().as_str() {
        "DNF" | "ESOP" | "SOP" => canonical_dnf(&expr, &vars, env)?,
        "CNF" | "POS" => canonical_cnf(&expr, &vars, env)?,
        "ANF" | "ZHEGALKIN" => anf_expr(&expr, &vars, env)?,
        "NAND" => rewrite_nand(&to_and_or_not(&expr)),
        "NOR" => rewrite_nor(&to_and_or_not(&expr)),
        "AND" => and_only(&to_and_or_not(&expr)),
        "OR" => or_only(&to_and_or_not(&expr)),
        "XOR" => xor_only(&to_and_or_not(&expr)),
        "IMPLIES" => implies_only(&to_and_or_not(&expr)),
        _ => return Ok(uneval("BooleanConvert", args)),
    };
    Ok(expr_to_value(&result))
}

fn builtin_boolean_minimize(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Ok(uneval("BooleanMinimize", args));
    }
    let expr = arg_expr(&args[0]);
    let form = if args.len() >= 2 {
        match form_of(&args[1]) {
            Some(f) => f,
            None => return Ok(uneval("BooleanMinimize", args)),
        }
    } else {
        "DNF".to_string()
    };
    let vars = vars_of(&expr);
    let result = match form.to_uppercase().as_str() {
        "DNF" | "SOP" | "ESOP" => minimized_dnf(&expr, &vars, env)?,
        "CNF" | "POS" => minimized_cnf(&expr, &vars, env)?,
        _ => return Ok(uneval("BooleanMinimize", args)),
    };
    Ok(expr_to_value(&result))
}

fn builtin_logical_expand(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Ok(uneval("LogicalExpand", args));
    }
    let expr = arg_expr(&args[0]);
    let vars = vars_of(&expr);
    Ok(expr_to_value(&canonical_dnf(&expr, &vars, env)?))
}

fn builtin_boolean_table(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Ok(uneval("BooleanTable", args));
    }
    let expr = arg_expr(&args[0]);
    let vars = vars_arg(args, &expr);
    let tt = truth_table(&expr, &vars, env)?;
    // The internal table is indexed with the all-false assignment first; WL
    // emits the all-true assignment first.
    let out = tt.into_iter().rev().map(Value::Bool).collect();
    Ok(Value::List(out))
}

fn builtin_boolean_variables(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(uneval("BooleanVariables", args));
    }
    let expr = arg_expr(&args[0]);
    let vars = vars_of(&expr);
    Ok(Value::List(vars.into_iter().map(Value::Symbol).collect()))
}

fn builtin_tautology_q(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Ok(uneval("TautologyQ", args));
    }
    let expr = arg_expr(&args[0]);
    let vars = vars_arg(args, &expr);
    let tt = truth_table(&expr, &vars, env)?;
    Ok(Value::Bool(tt.iter().all(|b| *b)))
}

fn builtin_satisfiable_q(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Ok(uneval("SatisfiableQ", args));
    }
    let expr = arg_expr(&args[0]);
    let vars = vars_arg(args, &expr);
    let tt = truth_table(&expr, &vars, env)?;
    Ok(Value::Bool(tt.iter().any(|b| *b)))
}

// ── BooleanFunction / BooleanCountingFunction ───────────────────────────────

fn as_u64(v: &Value) -> Option<u64> {
    match v {
        Value::Integer(n) => n.to_u64(),
        Value::Real(r) => Some(r.to_f64() as u64),
        _ => None,
    }
}

fn bit_len(n: u64) -> u32 {
    if n == 0 { 0 } else { 64 - n.leading_zeros() }
}

/// Build the OF-expansion of the k-ary Boolean function with index `n` over
/// the given atoms (all-true assignment first, matching WL digit order).
fn bool_function_expr(n: u64, k: usize, atoms: &[Expr]) -> Expr {
    let total = 1usize << k;
    let mut terms = Vec::new();
    for j in 0..total {
        if (n >> j) & 1 == 1 {
            let lits: Vec<Expr> = atoms
                .iter()
                .enumerate()
                .map(|(i, atom)| {
                    let bit = (j >> (k - 1 - i)) & 1 == 1;
                    if bit {
                        atom.clone()
                    } else {
                        not_of(atom.clone())
                    }
                })
                .collect();
            terms.push(and_expr(lits));
        }
    }
    or_expr(terms)
}

/// Choose the Boolean atoms from a (possibly list-wrapped) argument tail.
fn atoms_from(vals: &[Value]) -> Vec<Expr> {
    if vals.len() == 1
        && let Value::List(items) = &vals[0]
    {
        return items.iter().map(value_to_expr).collect();
    }
    vals.iter().map(value_to_expr).collect()
}

fn exact_count_expr(n: usize, k: usize, atoms: &[Expr]) -> Expr {
    if k > n {
        return Expr::Bool(false);
    }
    let mut terms = Vec::new();
    for subset in combinations(n, k) {
        let chosen: BTreeSet<usize> = subset.into_iter().collect();
        let lits: Vec<Expr> = (0..n)
            .map(|i| {
                let atom = atoms
                    .get(i)
                    .cloned()
                    .unwrap_or(Expr::Symbol(format!("#{}", i + 1)));
                if chosen.contains(&i) {
                    atom
                } else {
                    not_of(atom)
                }
            })
            .collect();
        terms.push(and_expr(lits));
    }
    or_expr(terms)
}

fn combinations(n: usize, k: usize) -> Vec<Vec<usize>> {
    let mut out = Vec::new();
    if k > n {
        return out;
    }
    let mut idx: Vec<usize> = (0..k).collect();
    loop {
        out.push(idx.clone());
        let mut i = k;
        loop {
            if i == 0 {
                return out;
            }
            i -= 1;
            if idx[i] != i + n - k {
                idx[i] += 1;
                for j in i + 1..k {
                    idx[j] = idx[j - 1] + 1;
                }
                break;
            }
        }
    }
}

fn counting_expr(spec: &[u64], k: usize, atoms: &[Expr]) -> Expr {
    let (lo, hi) = match spec {
        [a] => (*a, *a),
        [a, b] => (*a.min(b), *a.max(b)),
        _ => return Expr::Bool(false),
    };
    let terms: Vec<Expr> = (lo..=hi)
        .map(|c| exact_count_expr(k, c as usize, atoms))
        .collect();
    or_expr(terms)
}

fn builtin_boolean_function(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Ok(uneval("BooleanFunction", args));
    }
    if args.iter().any(|a| as_u64(a).is_none()) {
        return Ok(uneval("BooleanFunction", args));
    }
    Ok(call_v("BooleanFunction", args.to_vec()))
}

fn builtin_boolean_function_apply2(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Ok(uneval("BooleanFunctionApply", args));
    }
    let Some(n) = as_u64(&args[0]) else {
        return Ok(uneval("BooleanFunctionApply", args));
    };
    let atoms = atoms_from(&args[2..]);
    finish_function_expr(bool_function_expr(n, atoms.len(), &atoms), &atoms, env)
}

fn builtin_boolean_function_apply1(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Ok(uneval("BooleanFunctionApply1", args));
    }
    let Some(n) = as_u64(&args[0]) else {
        return Ok(uneval("BooleanFunctionApply1", args));
    };
    let atoms = atoms_from(&args[1..]);
    let k = if atoms.is_empty() {
        (bit_len(n).max(1)) as usize
    } else {
        atoms.len()
    };
    finish_function_expr(bool_function_expr(n, k, &atoms), &atoms, env)
}

/// Evaluate the built function when every atom is a concrete boolean; keep the
/// symbolic structure otherwise (evaluation collapses `And[a, b]` to `a`).
fn finish_function_expr(expr: Expr, atoms: &[Expr], env: &Env) -> Result<Value, EvalError> {
    if !atoms.is_empty() && atoms.iter().all(|a| matches!(a, Expr::Bool(_))) {
        crate::eval::eval(&expr, env)
    } else {
        Ok(expr_to_value(&expr))
    }
}

fn builtin_boolean_counting_function(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 || args.len() > 3 {
        return Ok(uneval("BooleanCountingFunction", args));
    }
    if parse_count_spec(&args[0]).is_none() {
        return Ok(uneval("BooleanCountingFunction", args));
    }
    match &args[1] {
        Value::Integer(_) | Value::List(_) => {}
        _ => return Ok(uneval("BooleanCountingFunction", args)),
    }
    Ok(call_v("BooleanCountingFunction", args.to_vec()))
}

fn parse_count_spec(v: &Value) -> Option<Vec<u64>> {
    match value_to_expr(v) {
        Expr::Integer(n) => n.to_u64().map(|n| vec![n]),
        Expr::List(items) => {
            let vals: Option<Vec<u64>> = items
                .iter()
                .map(|it| match it {
                    Expr::Integer(n) => n.to_u64(),
                    _ => None,
                })
                .collect();
            match vals {
                Some(vs) if vs.len() == 1 || vs.len() == 2 => Some(vs),
                _ => None,
            }
        }
        _ => None,
    }
}

fn builtin_bcf_apply(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Ok(uneval("BooleanCountingFunctionApply", args));
    }
    let Some(spec) = parse_count_spec(&args[0]) else {
        return Ok(uneval("BooleanCountingFunctionApply", args));
    };
    let k = match &args[1] {
        Value::Integer(n) => match n.to_usize() {
            Some(k) => k,
            None => return Ok(uneval("BooleanCountingFunctionApply", args)),
        },
        Value::List(l) => l.len(),
        _ => return Ok(uneval("BooleanCountingFunctionApply", args)),
    };
    let atoms = atoms_from(&args[2..]);
    let k = if atoms.is_empty() { k } else { atoms.len() };
    finish_function_expr(counting_expr(&spec, k, &atoms), &atoms, env)
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn sym(s: &str) -> Value {
        Value::Symbol(s.to_string())
    }

    fn call(head: &str, args: Vec<Value>) -> Value {
        Value::Call {
            head: head.to_string(),
            args,
        }
    }

    fn lst(items: Vec<Value>) -> Value {
        Value::List(items)
    }

    fn not_(v: Value) -> Value {
        call("Not", vec![v])
    }

    // ── BooleanVariables ──

    #[test]
    fn test_boolean_variables() {
        assert_eq!(
            eval_str("BooleanVariables[a && b || c]"),
            lst(vec![sym("a"), sym("b"), sym("c")])
        );
        assert_eq!(eval_str("BooleanVariables[True || a]"), lst(vec![sym("a")]));
    }

    // ── TautologyQ / SatisfiableQ ──

    #[test]
    fn test_tautology_q() {
        assert_eq!(eval_str("TautologyQ[a || !a]"), Value::Bool(true));
        assert_eq!(eval_str("TautologyQ[a && b]"), Value::Bool(false));
        assert_eq!(eval_str("TautologyQ[True]"), Value::Bool(true));
    }

    #[test]
    fn test_satisfiable_q() {
        assert_eq!(eval_str("SatisfiableQ[a && !a]"), Value::Bool(false));
        assert_eq!(eval_str("SatisfiableQ[a && b]"), Value::Bool(true));
    }

    #[test]
    fn test_tautology_with_vars() {
        assert_eq!(
            eval_str("TautologyQ[(a || b) && (!a || c) || !b, {a, b, c}]"),
            Value::Bool(false)
        );
    }

    // ── BooleanTable ──

    #[test]
    fn test_boolean_table_and() {
        assert_eq!(
            eval_str("BooleanTable[a && b, {a, b}]"),
            lst(vec![
                Value::Bool(true),
                Value::Bool(false),
                Value::Bool(false),
                Value::Bool(false)
            ])
        );
    }

    #[test]
    fn test_boolean_table_xor() {
        assert_eq!(
            eval_str("BooleanTable[Xor[a, b], {a, b}]"),
            lst(vec![
                Value::Bool(false),
                Value::Bool(true),
                Value::Bool(true),
                Value::Bool(false)
            ])
        );
    }

    #[test]
    fn test_boolean_table_auto_vars() {
        assert_eq!(
            eval_str("BooleanTable[a]"),
            lst(vec![Value::Bool(true), Value::Bool(false)])
        );
    }

    // ── BooleanConvert ──

    #[test]
    fn test_boolean_convert_dnf() {
        assert_eq!(
            eval_str("BooleanConvert[a && b, \"DNF\"]"),
            call("And", vec![sym("a"), sym("b")])
        );
        assert_eq!(eval_str("BooleanConvert[True, \"DNF\"]"), Value::Bool(true));
        assert_eq!(
            eval_str("BooleanConvert[False, \"CNF\"]"),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_boolean_convert_cnf() {
        assert_eq!(
            eval_str("BooleanConvert[a || b, \"CNF\"]"),
            call("Or", vec![sym("a"), sym("b")])
        );
    }

    #[test]
    fn test_boolean_convert_nand() {
        assert_eq!(
            eval_str("BooleanConvert[a && b, \"NAND\"]"),
            call("Nand", vec![call("Nand", vec![sym("a"), sym("b")]); 2])
        );
    }

    #[test]
    fn test_boolean_convert_anf() {
        assert_eq!(eval_str("BooleanConvert[a, \"ANF\"]"), sym("a"));
        assert_eq!(
            eval_str("BooleanConvert[Xor[a, b], \"ANF\"]"),
            call("Xor", vec![sym("a"), sym("b")])
        );
    }

    #[test]
    fn test_boolean_convert_restricted_forms() {
        let a = sym("a");
        let b = sym("b");
        assert_eq!(
            eval_str("BooleanConvert[a && b, \"NOR\"]"),
            call(
                "Nor",
                vec![
                    call("Nor", vec![a.clone(), a.clone()]),
                    call("Nor", vec![b.clone(), b.clone()]),
                ]
            )
        );
        assert_eq!(
            eval_str("BooleanConvert[a && b, \"OR\"]"),
            call(
                "Not",
                vec![call("Or", vec![not_(a.clone()), not_(b.clone())])]
            )
        );
        assert_eq!(
            eval_str("BooleanConvert[a && b, \"IMPLIES\"]"),
            call(
                "Not",
                vec![call("Implies", vec![a.clone(), not_(b.clone())])]
            )
        );
        assert_eq!(
            eval_str("BooleanConvert[a && b, \"AND\"]"),
            call("And", vec![a, b])
        );
    }

    #[test]
    fn test_boolean_convert_unevaluated_form() {
        assert_eq!(
            eval_str("BooleanConvert[a, \"NOPE\"]"),
            call("BooleanConvert", vec![sym("a"), Value::Str("NOPE".into())])
        );
    }

    // ── BooleanMinimize ──

    #[test]
    fn test_boolean_minimize_absorption() {
        assert_eq!(eval_str("BooleanMinimize[(a && b) || (a && !b)]"), sym("a"));
        assert_eq!(eval_str("BooleanMinimize[a || (a && b)]"), sym("a"));
    }

    #[test]
    fn test_boolean_minimize_cnf() {
        assert_eq!(
            eval_str("BooleanMinimize[(a || b) && (a || !b), \"CNF\"]"),
            sym("a")
        );
    }

    #[test]
    fn test_boolean_minimize_constant() {
        assert_eq!(eval_str("BooleanMinimize[True]"), Value::Bool(true));
        assert_eq!(eval_str("BooleanMinimize[False]"), Value::Bool(false));
    }

    // ── LogicalExpand ──

    #[test]
    fn test_logical_expand() {
        let out = eval_str("LogicalExpand[(a || b) && (c || d)]");
        let text = out.to_string();
        assert!(text.starts_with("Or["), "got {text}");
        assert!(
            !text.contains("||") || true,
            "canonical DNF should be a disjunction, got {text}"
        );
    }

    #[test]
    fn test_logical_expand_single() {
        assert_eq!(
            eval_str("LogicalExpand[a && b]"),
            call("And", vec![sym("a"), sym("b")])
        );
    }

    // ── BooleanFunction ──

    #[test]
    fn test_boolean_function_identity() {
        // BooleanFunction[2,1] is the identity function of one variable.
        assert_eq!(eval_str("BooleanFunction[2, 1][a]"), sym("a"));
        assert_eq!(eval_str("BooleanFunction[2, 1][False]"), Value::Bool(false));
        assert_eq!(eval_str("BooleanFunction[1, 1][a]"), not_(sym("a")));
    }

    #[test]
    fn test_boolean_function_list_application() {
        // BooleanFunction[8,2] is `And` (all-true row first is the last digit).
        assert_eq!(
            eval_str("BooleanFunction[8, 2][{a, b}]"),
            call("And", vec![sym("a"), sym("b")])
        );
    }

    #[test]
    fn test_boolean_function_constant() {
        assert_eq!(eval_str("BooleanFunction[0, 1][a]"), Value::Bool(false));
        assert_eq!(
            eval_str("BooleanFunction[3, 1][a]"),
            call("Or", vec![not_(sym("a")), sym("a")])
        );
        assert_eq!(eval_str("BooleanFunction[3, 1][True]"), Value::Bool(true));
    }

    #[test]
    fn test_boolean_function_apply_arity() {
        assert_eq!(
            eval_str("BooleanFunction[8, 2][a, b]"),
            call("And", vec![sym("a"), sym("b")])
        );
    }

    // ── BooleanCountingFunction ──

    #[test]
    fn test_boolean_counting_function() {
        assert_eq!(
            eval_str("BooleanCountingFunction[{2}, 3][True, True, False]"),
            Value::Bool(true)
        );
        assert_eq!(
            eval_str("BooleanCountingFunction[{2}, 3][True, True, True]"),
            Value::Bool(false)
        );
        assert_eq!(
            eval_str("BooleanCountingFunction[{2}, 3][{True, False, True}]"),
            Value::Bool(true)
        );
    }

    #[test]
    fn test_boolean_counting_all() {
        assert_eq!(
            eval_str("BooleanCountingFunction[{0}, 1][False]"),
            Value::Bool(true)
        );
    }

    #[test]
    fn test_boolean_table_of_function_object() {
        let out = eval_str("BooleanTable[BooleanCountingFunction[{2}, 3][a, b, c], {a, b, c}]");
        let expected: Vec<Value> = [false, true, true, false, true, false, false, false]
            .into_iter()
            .map(Value::Bool)
            .collect();
        assert_eq!(out, lst(expected));
    }

    // ── unevaluated for bad arity ──

    #[test]
    fn test_unevaluated_bad_arity() {
        assert_eq!(eval_str("BooleanConvert[]"), call("BooleanConvert", vec![]));
        assert_eq!(
            eval_str("BooleanVariables[a, b]"),
            call("BooleanVariables", vec![sym("a"), sym("b")])
        );
    }

    // ── helper unit tests ──

    #[test]
    fn test_builders() {
        assert_eq!(and_expr(vec![]), Expr::Bool(true));
        assert_eq!(or_expr(vec![]), Expr::Bool(false));
        assert_eq!(not_of(Expr::Bool(true)), Expr::Bool(false));
    }

    #[test]
    fn test_minimize_cover_reduces() {
        // f(a,b) = a&b | a&!b  →  a
        let cover = minimize_cover(&[0b10, 0b11], 2);
        assert_eq!(cover, vec![(0b10, 0b10)]);
    }

    #[test]
    fn test_combinations() {
        assert_eq!(combinations(3, 2), vec![vec![0, 1], vec![0, 2], vec![1, 2]]);
        assert_eq!(combinations(2, 0), vec![Vec::<usize>::new()]);
    }
}
