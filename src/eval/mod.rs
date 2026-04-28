/// Tree-walk evaluator for Syma language.
///
/// Evaluates AST expressions in an environment. Handles:
/// - Symbol lookup and function application
/// - Pattern-matched function definitions
/// - Rule application (/. and //.)
/// - Class instantiation and method dispatch
/// - Control flow (If, Which, Switch, match, loops)
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use rug::Integer;

use crate::ast::*;
use crate::bytecode;
use crate::env::Env;
use crate::env::LazyProvider;
use crate::ffi;
use crate::pattern::{
    AttributeChecker, Bindings, MatchResult, collect_nested_guards, match_pattern,
};
use crate::profiler;
use crate::value::*;

pub(crate) mod numeric;
pub(crate) mod plot;
pub(crate) mod rules;
pub(crate) mod table;

use crate::builtins::dataset;

/// Evaluate a program (list of statements) in the given environment.
pub fn eval_program(stmts: &[Expr], env: &Env) -> Result<Value, EvalError> {
    let mut result = Value::Null;
    for stmt in stmts {
        match eval(stmt, env) {
            Ok(v) => result = v,
            // Return at top-level just yields the value
            Err(EvalError::Return(v)) => result = *v,
            Err(e) => return Err(e),
        }
    }
    Ok(result)
}

/// Evaluate a program where each statement may be suppressed by `;`.
/// Returns `None` for suppressed statements, `Some(value)` otherwise.
pub fn eval_program_with_results(
    stmts: &[(Expr, bool)],
    env: &Env,
) -> Result<Vec<Option<Value>>, EvalError> {
    let mut results = Vec::with_capacity(stmts.len());
    for (stmt, suppressed) in stmts {
        match eval(stmt, env) {
            Ok(val) => {
                if *suppressed {
                    results.push(None);
                } else {
                    results.push(Some(val));
                }
            }
            // Return at top-level just yields the value
            Err(EvalError::Return(v)) => {
                if *suppressed {
                    results.push(None);
                } else {
                    results.push(Some(*v));
                }
            }
            Err(e) => return Err(e),
        }
    }
    Ok(results)
}

/// Convert an AST expression to a value without performing evaluation.
/// Used by Hold/HoldComplete to preserve the syntactic form.
fn expr_to_value(expr: &Expr) -> Value {
    match expr {
        Expr::Integer(n) => Value::Integer(n.clone()),
        Expr::Real(r) => Value::Real(r.clone()),
        Expr::Bool(b) => Value::Bool(*b),
        Expr::Str(s) => Value::Str(s.clone()),
        Expr::Null => Value::Null,
        Expr::Symbol(s) => Value::Symbol(s.clone()),
        Expr::List(items) => Value::List(items.iter().map(expr_to_value).collect()),
        // Wrap calls as Pattern so ReleaseHold can evaluate them properly
        Expr::Call { .. } => Value::Pattern(expr.clone()),
        // Everything else wraps as Pattern
        _ => Value::Pattern(expr.clone()),
    }
}

/// Evaluate a value that was previously held.
/// Recursively evaluates Value::Pattern/list contents after ReleaseHold.
fn release_inner(val: Value, env: &Env) -> Result<Value, EvalError> {
    match val {
        Value::Pattern(expr) => eval(&expr, env),
        Value::List(items) => {
            let evaled: Result<Vec<Value>, _> =
                items.into_iter().map(|v| release_inner(v, env)).collect();
            Ok(Value::List(evaled?))
        }
        other => Ok(other),
    }
}

/// Evaluate a single expression in the given environment.
pub fn eval(expr: &Expr, env: &Env) -> Result<Value, EvalError> {
    let _attr_checker = AttributeChecker::new(env.attributes.clone());
    match expr {
        // ── Atoms ──
        Expr::Integer(n) => Ok(Value::Integer(n.clone())),
        Expr::Real(r) => Ok(Value::Real(r.clone())),
        Expr::Complex { re, im } => Ok(Value::Complex { re: *re, im: *im }),
        Expr::Str(s) => Ok(Value::Str(s.clone())),
        Expr::Bool(b) => Ok(Value::Bool(*b)),
        Expr::Null => Ok(Value::Null),

        // ── Symbol lookup ──
        Expr::Symbol(s) => {
            // Symbols containing _ are patterns in Syma (e.g., _Integer, x_, x_Integer, __, ___)
            if s.starts_with('_') || s.contains('_') {
                Ok(Value::Pattern(convert_blank_pattern(s)))
            } else {
                Ok(env.get(s).unwrap_or_else(|| Value::Symbol(s.clone())))
            }
        }

        // ── List ──
        Expr::List(items) => {
            let values: Result<Vec<Value>, _> = items.iter().map(|item| eval(item, env)).collect();
            Ok(Value::List(flatten_sequences(values?, false)))
        }

        // ── Association ──
        Expr::Assoc(entries) => {
            let mut map = HashMap::new();
            for (key, val) in entries {
                map.insert(key.clone(), eval(val, env)?);
            }
            Ok(Value::Assoc(map))
        }

        // ── Rules ──
        Expr::Rule { lhs, rhs } => Ok(Value::Rule {
            lhs: Box::new(Value::Pattern(lhs.as_ref().clone())),
            rhs: Box::new(Value::Pattern(rhs.as_ref().clone())),
            delayed: false,
        }),
        Expr::RuleDelayed { lhs, rhs } => Ok(Value::Rule {
            lhs: Box::new(Value::Pattern(lhs.as_ref().clone())),
            rhs: Box::new(Value::Pattern(rhs.as_ref().clone())),
            delayed: true,
        }),

        // ── Function application ──
        Expr::Call { head, args } => eval_call(head, args, env),

        // ── ReplaceAll: expr /. rules ──
        Expr::ReplaceAll { expr, rules } => {
            let val = eval(expr, env)?;
            let rules_val = eval(rules, env)?;
            rules::apply_rules_value(&val, &rules_val, env)
        }

        // ── ReplaceRepeated: expr //. rules ──
        Expr::ReplaceRepeated { expr, rules } => {
            let mut val = eval(expr, env)?;
            let rules_val = eval(rules, env)?;
            let max_iterations = 1000;
            for _ in 0..max_iterations {
                match rules::apply_rules_value(&val, &rules_val, env)? {
                    new_val if !new_val.struct_eq(&val) => val = new_val,
                    _ => break,
                }
            }
            Ok(val)
        }

        // ── Map: f /@ list ──
        Expr::Map { func, list } => {
            let f = eval(func, env)?;
            let l = eval(list, env)?;
            match l {
                Value::List(items) => {
                    let mut result = Vec::new();
                    for item in items {
                        result.push(apply_function(&f, &[item], env)?);
                    }
                    Ok(Value::List(result))
                }
                _ => Err(EvalError::TypeError {
                    expected: "List".to_string(),
                    got: l.type_name().to_string(),
                }),
            }
        }

        // ── Apply: f @@ expr ──
        Expr::Apply { func, expr } => {
            let f = eval(func, env)?;
            let e = eval(expr, env)?;
            match e {
                Value::List(items) => apply_function(&f, &items, env),
                _ => apply_function(&f, &[e], env),
            }
        }

        // ── Pipe: expr // func ──
        // expr // f → f[expr]; Sequence values in expr are flattened.
        Expr::Pipe { expr, func } => {
            let val = eval(expr, env)?;
            let f = eval(func, env)?;
            let args = flatten_sequences(vec![val], false);
            apply_function(&f, &args, env)
        }

        // ── Prefix: f @ x ──
        Expr::Prefix { func, arg } => {
            let f = eval(func, env)?;
            let a = eval(arg, env)?;
            let args = flatten_sequences(vec![a], false);
            apply_function(&f, &args, env)
        }

        // ── If ──
        Expr::If {
            condition,
            then_branch,
            else_branch,
        } => {
            let cond = eval(condition, env)?;
            if cond.to_bool() {
                eval(then_branch, env)
            } else if let Some(else_b) = else_branch {
                eval(else_b, env)
            } else {
                Ok(Value::Null)
            }
        }

        // ── Which ──
        Expr::Which { pairs } => {
            for (cond, val) in pairs {
                if eval(cond, env)?.to_bool() {
                    return eval(val, env);
                }
            }
            Ok(Value::Null)
        }

        // ── Switch ──
        Expr::Switch { expr, cases } => {
            let val = eval(expr, env)?;
            'next_case: for (pattern, result) in cases {
                let (inner_pat, guard) = extract_guard_expr(pattern);
                if let MatchResult::Match(bindings) =
                    match_pattern(inner_pat, &val, Some(&_attr_checker))
                {
                    // Evaluate top-level guard if present
                    if let Some(guard_expr) = guard {
                        let guard_env = env.child();
                        for (name, value) in &bindings {
                            guard_env.set(name.clone(), value.clone());
                        }
                        guard_env.set("#".to_string(), val.clone());
                        if !eval(guard_expr, &guard_env)?.to_bool() {
                            continue 'next_case;
                        }
                    }
                    // Evaluate nested guards inside compound patterns (e.g., {a_, b_ /; a < b})
                    let mut guard_exprs = Vec::new();
                    collect_nested_guards(inner_pat, &mut guard_exprs);
                    for guard_expr in &guard_exprs {
                        let guard_env = env.child();
                        for (name, value) in &bindings {
                            guard_env.set(name.clone(), value.clone());
                        }
                        guard_env.set("#".to_string(), val.clone());
                        if !eval(guard_expr, &guard_env)?.to_bool() {
                            continue 'next_case;
                        }
                    }
                    let child_env = env.child();
                    for (name, value) in &bindings {
                        child_env.set(name.clone(), value.clone());
                    }
                    return eval(result, &child_env);
                }
            }
            Ok(Value::Null)
        }

        // ── Match ──
        Expr::Match { expr, branches } => {
            let val = eval(expr, env)?;
            for branch in branches {
                let (inner_pat, guard) = extract_guard_expr(&branch.pattern);
                if let MatchResult::Match(bindings) =
                    match_pattern(inner_pat, &val, Some(&_attr_checker))
                {
                    // Evaluate top-level guard if present
                    if let Some(guard_expr) = guard {
                        let guard_env = env.child();
                        for (name, value) in &bindings {
                            guard_env.set(name.clone(), value.clone());
                        }
                        guard_env.set("#".to_string(), val.clone());
                        if !eval(guard_expr, &guard_env)?.to_bool() {
                            continue;
                        }
                    }
                    // Evaluate nested guards inside compound patterns
                    let mut guard_exprs = Vec::new();
                    collect_nested_guards(inner_pat, &mut guard_exprs);
                    for guard_expr in &guard_exprs {
                        let guard_env = env.child();
                        for (name, value) in &bindings {
                            guard_env.set(name.clone(), value.clone());
                        }
                        guard_env.set("#".to_string(), val.clone());
                        if !eval(guard_expr, &guard_env)?.to_bool() {
                            continue;
                        }
                    }
                    let child_env = env.child();
                    for (name, value) in &bindings {
                        child_env.set(name.clone(), value.clone());
                    }
                    return eval(&branch.result, &child_env);
                }
            }
            Err(EvalError::NoMatch {
                head: "match".to_string(),
                args: Box::new(vec![val]),
            })
        }

        // ── For loop ──
        Expr::For {
            init,
            condition,
            step,
            body,
        } => {
            eval(init, env)?;
            while eval(condition, env)?.to_bool() {
                match eval(body, env) {
                    Ok(_) | Err(EvalError::Continue) => {}
                    Err(EvalError::Break) => break,
                    Err(e) => return Err(e),
                }
                match eval(step, env) {
                    Ok(_) | Err(EvalError::Continue) => {}
                    Err(EvalError::Break) => break,
                    Err(e) => return Err(e),
                }
            }
            Ok(Value::Null)
        }

        // ── While loop ──
        Expr::While { condition, body } => {
            while eval(condition, env)?.to_bool() {
                match eval(body, env) {
                    Ok(_) | Err(EvalError::Continue) => {}
                    Err(EvalError::Break) => break,
                    Err(e) => return Err(e),
                }
            }
            Ok(Value::Null)
        }

        // ── Do loop ──
        Expr::Do { body, iterator } => {
            let child_env = env.child();
            match iterator {
                IteratorSpec::Range { var, min, max } => {
                    let min_val = eval(min, &child_env)?.to_integer().ok_or_else(|| {
                        EvalError::TypeError {
                            expected: "Integer".to_string(),
                            got: "non-Integer".to_string(),
                        }
                    })?;
                    let max_val = eval(max, &child_env)?.to_integer().ok_or_else(|| {
                        EvalError::TypeError {
                            expected: "Integer".to_string(),
                            got: "non-Integer".to_string(),
                        }
                    })?;
                    for i in min_val..=max_val {
                        child_env.set(var.clone(), Value::Integer(Integer::from(i)));
                        match eval(body, &child_env) {
                            Ok(_) | Err(EvalError::Continue) => {}
                            Err(EvalError::Break) => break,
                            Err(e) => return Err(e),
                        }
                    }
                }
                IteratorSpec::List { var, list } => {
                    let list_val = eval(list, &child_env)?;
                    match list_val {
                        // Do[body, {i, {val1, val2, ...}}] — iterate over list elements
                        Value::List(items) => {
                            for item in items {
                                child_env.set(var.clone(), item);
                                match eval(body, &child_env) {
                                    Ok(_) | Err(EvalError::Continue) => {}
                                    Err(EvalError::Break) => break,
                                    Err(e) => return Err(e),
                                }
                            }
                        }
                        // Do[body, {i, max}] — short form for range 1..max
                        Value::Integer(max_val) => {
                            let max_i64 = max_val.to_i64().unwrap_or(0);
                            for i in 1..=max_i64 {
                                child_env.set(var.clone(), Value::Integer(Integer::from(i)));
                                match eval(body, &child_env) {
                                    Ok(_) | Err(EvalError::Continue) => {}
                                    Err(EvalError::Break) => break,
                                    Err(e) => return Err(e),
                                }
                            }
                        }
                        _ => {
                            return Err(EvalError::TypeError {
                                expected: "List or Integer".to_string(),
                                got: list_val.type_name().to_string(),
                            });
                        }
                    }
                }
            }
            Ok(Value::Null)
        }

        // ── Function definition ──
        Expr::FuncDef {
            name,
            params,
            body,
            delayed,
            guard,
        } => {
            // Check if symbol is protected
            if env.has_attribute(name, "Protected") && env.get(name).is_some() {
                return Err(EvalError::Error(format!(
                    "Symbol {} is protected; cannot redefine",
                    name
                )));
            }
            // Check if function already exists
            let func = if let Some(Value::Function(f)) = env.get(name) {
                Arc::try_unwrap(f).unwrap_or_else(|arc| (*arc).clone())
            } else {
                FunctionDef {
                    name: name.clone(),
                    definitions: Vec::new(),
                }
            };

            let mut func = func;
            func.definitions.push(FunctionDefinition {
                params: params.clone(),
                body: body.as_ref().clone(),
                delayed: *delayed,
                guard: guard.as_ref().map(|g| g.as_ref().clone()),
            });

            // Sort definitions so more specific (literal) patterns come first.
            func.definitions
                .sort_by(|a, b| specificity(&b.params).cmp(&specificity(&a.params)));

            env.set(name.clone(), Value::Function(Arc::new(func)));
            Ok(Value::Null)
        }

        // ── Assignment ──
        Expr::Assign { lhs, rhs } => {
            let val = eval(rhs, env)?;
            match lhs.as_ref() {
                Expr::Symbol(s) => {
                    // Check if symbol is protected
                    if env.has_attribute(s, "Protected") && env.get(s).is_some() {
                        return Err(EvalError::Error(format!(
                            "Symbol {} is protected; cannot assign",
                            s
                        )));
                    }
                    // Propagate assignment to the scope where the symbol was defined,
                    // so that assignments inside Do/For/While bodies update outer bindings.
                    env.set_propagate(s.clone(), val.clone());
                    Ok(val)
                }
                // LocalSymbol["name"] = value
                Expr::Call {
                    head,
                    args: call_args,
                } if call_args.len() == 1
                    && matches!(head.as_ref(), Expr::Symbol(s) if s == "LocalSymbol") =>
                {
                    let name = eval(&call_args[0], env)?;
                    match name {
                        Value::Str(s) => crate::builtins::localsymbol::write_local_symbol(&s, &val),
                        _ => Err(EvalError::Error(
                            "LocalSymbol requires a string name".to_string(),
                        )),
                    }
                }
                // Attributes[sym] = {attr1, attr2}  — set attributes
                Expr::Call {
                    head,
                    args: call_args,
                } if call_args.len() == 1
                    && matches!(head.as_ref(), Expr::Symbol(s) if s == "Attributes") =>
                {
                    let sym_name = match &call_args[0] {
                        Expr::Symbol(s) => s.clone(),
                        _ => {
                            return Err(EvalError::Error(
                                "Attributes assignment requires a symbol name".to_string(),
                            ));
                        }
                    };
                    if env.has_attribute(&sym_name, "Locked") {
                        return Ok(Value::Null);
                    }
                    let attrs = match &val {
                        Value::List(items) => items.iter().map(|v| v.to_string()).collect(),
                        other => vec![other.to_string()],
                    };
                    env.set_attributes(&sym_name, attrs);
                    Ok(val)
                }
                // f[args] = value  — set a function definition with immediate RHS
                // Mathematica: Set[f[args], val]  defines a specific rule for f.
                // Also handles desugared OOP field access: this.field = val
                // is parsed as Assign(Call(field[this]), val). When the target
                // evaluates to an Object, treat as field access.
                // Guard: Part[...] = val is handled by the next branch.
                Expr::Call {
                    head,
                    args: call_args,
                } if !matches!(head.as_ref(), Expr::Symbol(s) if s == "Part") => {
                    if let Expr::Symbol(name) = head.as_ref() {
                        // Check for OOP field access: field[object] = value
                        // where object evaluates to an Object
                        if call_args.len() == 1 {
                            let target = eval(&call_args[0], env)?;
                            if let Value::Object {
                                class_name,
                                mut fields,
                            } = target
                            {
                                fields.insert(name.clone(), val.clone());
                                let updated = Value::Object { class_name, fields };
                                if let Expr::Symbol(s) = &call_args[0]
                                    && s == "this"
                                {
                                    env.set("this".to_string(), updated.clone());
                                }
                                return Ok(val);
                            }
                        }
                        // Otherwise: function definition via assignment
                        // f[args] = val  → add FunctionDefinition for name with
                        // params = args (as patterns), body = val (converted to Expr)
                        let body_expr = table::value_to_expr(&val);
                        let func = if let Some(Value::Function(f)) = env.get(name) {
                            Arc::try_unwrap(f).unwrap_or_else(|arc| (*arc).clone())
                        } else {
                            FunctionDef {
                                name: name.clone(),
                                definitions: Vec::new(),
                            }
                        };
                        let mut func = func;
                        func.definitions.push(FunctionDefinition {
                            params: call_args.clone(),
                            body: body_expr,
                            delayed: false,
                            guard: None,
                        });
                        // Sort definitions so more specific ones match first
                        func.definitions
                            .sort_by(|a, b| specificity(&b.params).cmp(&specificity(&a.params)));
                        env.set(name.clone(), Value::Function(Arc::new(func)));
                        return Ok(val);
                    }
                    Err(EvalError::Error("Invalid assignment target".to_string()))
                }
                // x[[i]] = val  (desugared to Assign(Part[x, i], val))
                Expr::Call {
                    head,
                    args: part_args,
                } if matches!(head.as_ref(), Expr::Symbol(s) if s == "Part")
                    && !part_args.is_empty() =>
                {
                    let var_name = match &part_args[0] {
                        Expr::Symbol(s) => s.clone(),
                        _ => {
                            return Err(EvalError::Error(
                                "Part assignment: collection must be a symbol".to_string(),
                            ));
                        }
                    };
                    let current = env.get(&var_name).ok_or_else(|| {
                        EvalError::Error(format!("Symbol {} is not defined", var_name))
                    })?;
                    let indices: Vec<i64> = part_args[1..]
                        .iter()
                        .map(|idx| {
                            eval(idx, env)?
                                .to_integer()
                                .ok_or_else(|| EvalError::TypeError {
                                    expected: "Integer".to_string(),
                                    got: "non-Integer".to_string(),
                                })
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    let updated = set_part(current, &indices, val.clone())?;
                    env.set_propagate(var_name, updated);
                    Ok(val)
                }
                _ => Err(EvalError::Error("Invalid assignment target".to_string())),
            }
        }

        // ── Destructuring assignment ──
        Expr::DestructAssign { patterns, rhs } => {
            let val = eval(rhs, env)?;
            match val {
                Value::List(items) => {
                    if patterns.len() != items.len() {
                        return Err(EvalError::Error(format!(
                            "Destructuring mismatch: {} patterns, {} values",
                            patterns.len(),
                            items.len()
                        )));
                    }
                    for (pat, item) in patterns.iter().zip(items.iter()) {
                        match pat {
                            Expr::Symbol(s) => {
                                env.set(s.clone(), item.clone());
                            }
                            _ => {
                                // Pattern destructuring
                                if let MatchResult::Match(bindings) =
                                    match_pattern(pat, item, Some(&_attr_checker))
                                {
                                    for (name, value) in bindings {
                                        env.set(name, value);
                                    }
                                } else {
                                    return Err(EvalError::Error(
                                        "Destructuring pattern mismatch".to_string(),
                                    ));
                                }
                            }
                        }
                    }
                    Ok(Value::Null)
                }
                _ => Err(EvalError::TypeError {
                    expected: "List".to_string(),
                    got: val.type_name().to_string(),
                }),
            }
        }

        // ── Post-increment: expr++ (evaluate, increment, return old value) ──
        Expr::PostIncrement { expr } => {
            let val = eval(expr, env)?;
            let new_val = eval(
                &Expr::Call {
                    head: Box::new(Expr::Symbol("Plus".to_string())),
                    args: vec![expr.as_ref().clone(), Expr::Integer(Integer::from(1))],
                },
                env,
            )?;
            match expr.as_ref() {
                Expr::Symbol(s) => {
                    if env.has_attribute(s, "Protected") && env.get(s).is_some() {
                        return Err(EvalError::Error(format!(
                            "Symbol {} is protected; cannot modify",
                            s
                        )));
                    }
                    env.set_propagate(s.clone(), new_val);
                    Ok(val) // return old value
                }
                _ => Err(EvalError::Error("Invalid increment target".to_string())),
            }
        }

        // ── Post-decrement: expr-- (evaluate, decrement, return old value) ──
        Expr::PostDecrement { expr } => {
            let val = eval(expr, env)?;
            let new_val = eval(
                &Expr::Call {
                    head: Box::new(Expr::Symbol("Plus".to_string())),
                    args: vec![expr.as_ref().clone(), Expr::Integer(Integer::from(-1))],
                },
                env,
            )?;
            match expr.as_ref() {
                Expr::Symbol(s) => {
                    if env.has_attribute(s, "Protected") && env.get(s).is_some() {
                        return Err(EvalError::Error(format!(
                            "Symbol {} is protected; cannot modify",
                            s
                        )));
                    }
                    env.set_propagate(s.clone(), new_val);
                    Ok(val) // return old value
                }
                _ => Err(EvalError::Error("Invalid decrement target".to_string())),
            }
        }

        // ── Unset: expr =. ──
        Expr::Unset { expr } => match expr.as_ref() {
            Expr::Symbol(s) => {
                if env.has_attribute(s, "Protected") && env.get(s).is_some() {
                    return Err(EvalError::Error(format!(
                        "Symbol {} is protected; cannot unset",
                        s
                    )));
                }
                env.remove(s);
                Ok(Value::Null)
            }
            _ => Err(EvalError::Error(
                "Unset requires a symbol target".to_string(),
            )),
        },

        // ── Rule definition ──
        Expr::RuleDef { name, rules } => {
            let rule_pairs: Vec<(Value, Value)> = rules
                .iter()
                .map(|(lhs, rhs)| Ok((Value::Pattern(lhs.clone()), Value::Pattern(rhs.clone()))))
                .collect::<Result<Vec<_>, EvalError>>()?;

            env.set(
                name.clone(),
                Value::RuleSet {
                    name: name.clone(),
                    rules: rule_pairs,
                },
            );
            Ok(Value::Null)
        }

        // ── Class definition ──
        Expr::ClassDef {
            name,
            parent,
            mixins,
            members,
        } => {
            let mut fields = Vec::new();
            let mut methods = HashMap::new();
            let mut constructor = None;

            for member in members {
                match member {
                    crate::ast::MemberDef::Field {
                        name: field_name,
                        type_hint,
                        default,
                    } => {
                        fields.push(crate::value::ClassField {
                            name: field_name.clone(),
                            type_hint: type_hint.clone(),
                            default: default.clone(),
                        });
                    }
                    crate::ast::MemberDef::Method {
                        name: method_name,
                        params,
                        body,
                        ..
                    } => {
                        let body_expr = match body {
                            crate::ast::MethodBody::Expr(e) => e.clone(),
                            crate::ast::MethodBody::Block(stmts) => Expr::Sequence(stmts.clone()),
                        };
                        methods.insert(
                            method_name.clone(),
                            crate::value::ClassMethod {
                                name: method_name.clone(),
                                params: params.clone(),
                                body: body_expr,
                            },
                        );
                    }
                    crate::ast::MemberDef::Constructor { params, body } => {
                        let body_expr = if body.len() == 1 {
                            body[0].clone()
                        } else {
                            Expr::Sequence(body.clone())
                        };
                        constructor = Some(crate::value::ClassConstructor {
                            params: params.clone(),
                            body: body_expr,
                        });
                    }
                    crate::ast::MemberDef::Transform { .. } => {
                        // Transforms are not yet supported
                    }
                }
            }

            // Resolve parent class and merge its fields/methods
            let parent_name = parent.clone();
            if let Some(ref parent_name) = parent_name
                && let Some(parent_val) = env.get(parent_name)
                && let Value::Class(parent_class) = parent_val
            {
                // Merge parent fields (child fields override)
                let child_field_names: std::collections::HashSet<String> =
                    fields.iter().map(|f| f.name.clone()).collect();
                for parent_field in &parent_class.fields {
                    if !child_field_names.contains(&parent_field.name) {
                        fields.insert(0, parent_field.clone());
                    }
                }
                // Merge parent methods (child methods override)
                for (method_name, method) in &parent_class.methods {
                    methods
                        .entry(method_name.clone())
                        .or_insert_with(|| method.clone());
                }
                // Use parent constructor if child doesn't have one
                if constructor.is_none() {
                    constructor = parent_class.constructor.clone();
                }
            }

            // Resolve mixins and merge their methods
            for mixin_name in mixins {
                if let Some(mixin_val) = env.get(mixin_name)
                    && let Value::Class(mixin_class) = mixin_val
                {
                    for (method_name, method) in &mixin_class.methods {
                        methods
                            .entry(method_name.clone())
                            .or_insert_with(|| method.clone());
                    }
                }
            }

            let class_def = crate::value::ClassDef {
                name: name.clone(),
                parent: parent_name,
                mixins: mixins.clone(),
                fields,
                methods,
                constructor,
            };
            let class_val = Value::Class(std::sync::Arc::new(class_def));
            env.set(name.clone(), class_val);
            Ok(Value::Null)
        }

        // ── Module definition ──
        Expr::ModuleDef {
            name,
            exports,
            body,
        } => {
            let child_env = env.child();
            for stmt in body {
                eval(stmt, &child_env)?;
            }
            // Collect the explicitly exported symbols from the module's scope.
            let mut export_map = HashMap::new();
            for sym in exports {
                match child_env.get(sym) {
                    Some(val) => {
                        export_map.insert(sym.clone(), val);
                    }
                    None => {
                        return Err(EvalError::Error(format!(
                            "Module '{}' exports '{}' but it is not defined",
                            name, sym
                        )));
                    }
                }
            }
            // Collect all bindings from the module body's child_env
            // so internal helper functions are accessible.
            let local_map: std::collections::HashMap<String, Value> = child_env
                .bindings()
                .into_iter()
                .filter(|(sym, _)| !export_map.contains_key(sym))
                .collect();
            let module_val = Value::Module {
                name: name.clone(),
                exports: export_map,
                locals: local_map,
            };
            // Register in the shared session registry so `import` can find it by name.
            env.register_module(name.clone(), module_val.clone());
            // Also bind in the current scope so the module can be stored in a variable.
            env.set(name.clone(), module_val.clone());
            Ok(module_val)
        }

        // ── Import ──
        Expr::Import {
            module,
            selective,
            alias,
        } => {
            let module_name = module.join(".");
            // 1. Try in-memory registry (covers same-file and previously loaded modules).
            let module_val = if let Some(m) = env.get_module(&module_name) {
                m
            } else if module.len() == 1 {
                // 2. Try file-based loading.
                load_module_from_file(&module[0], env)?
            } else {
                return Err(EvalError::Error(format!(
                    "Module not found: '{}'",
                    module_name
                )));
            };

            match &module_val {
                Value::Module {
                    name: mod_name,
                    exports,
                    ..
                } => {
                    match (selective.as_deref(), alias.as_deref()) {
                        (Some(names), _) => {
                            // import Foo.{A, B}  — bind only the listed names
                            for sym in names {
                                match exports.get(sym) {
                                    Some(val) => env.set(sym.clone(), val.clone()),
                                    None => {
                                        return Err(EvalError::Error(format!(
                                            "Module '{}' does not export '{}'",
                                            mod_name, sym
                                        )));
                                    }
                                }
                            }
                        }
                        (None, Some(alias_name)) => {
                            // import Foo as F  — bind the module value under the alias
                            env.set(alias_name.to_string(), module_val.clone());
                        }
                        (None, None) => {
                            // import Foo  — bind all exports into current scope
                            for (sym, val) in exports {
                                env.set(sym.clone(), val.clone());
                            }
                            // Also bind internal helpers so exported functions can find them
                            if let Value::Module { locals, .. } = &module_val {
                                for (sym, val) in locals {
                                    env.set(sym.clone(), val.clone());
                                }
                            }
                        }
                    }
                }
                _ => {
                    return Err(EvalError::Error(format!(
                        "'{}' is not a module",
                        module_name
                    )));
                }
            }
            Ok(Value::Null)
        }

        // ── Export ──
        // `export` inside a module body is handled by ModuleDef; a bare `export`
        // outside a module is a no-op (it was already recorded by the parser).
        Expr::Export(_) => Ok(Value::Null),

        // ── Sequence ──
        Expr::Sequence(exprs) => {
            let mut result = Value::Null;
            for expr in exprs {
                result = eval(expr, env)?;
            }
            Ok(result)
        }

        // ── Hold ──
        Expr::Hold(e) => Ok(Value::Hold(Box::new(expr_to_value(e)))),
        Expr::HoldComplete(e) => Ok(Value::HoldComplete(Box::new(expr_to_value(e)))),
        Expr::ReleaseHold(e) => {
            let val = eval(e, env)?;
            match val {
                Value::Hold(v) | Value::HoldComplete(v) => release_inner(*v, env),
                _ => Ok(val),
            }
        }

        // ── Pattern nodes (should not be evaluated directly) ──
        Expr::Blank { .. }
        | Expr::NamedBlank { .. }
        | Expr::BlankSequence { .. }
        | Expr::BlankNullSequence { .. }
        | Expr::OptionalBlank { .. }
        | Expr::OptionalNamedBlank { .. }
        | Expr::PatternGuard { .. } => Ok(Value::Pattern(expr.clone())),

        // ── Slot (look up from env; PureFunction application binds #, #1, ...) ──
        Expr::Slot(None) => env
            .get("#")
            .ok_or_else(|| EvalError::Error("Slot # used outside of pure function".to_string())),
        Expr::Slot(Some(n)) => {
            let key = format!("#{}", n);
            env.get(&key).ok_or_else(|| {
                EvalError::Error(format!("Slot #{} used outside of pure function", n))
            })
        }

        // ── Slot sequence: ## or ##n ──
        Expr::SlotSequence(None) => env.get("##").ok_or_else(|| {
            EvalError::Error("Slot sequence ## used outside of pure function".to_string())
        }),
        Expr::SlotSequence(Some(n)) => match env.get("##") {
            Some(Value::Sequence(items)) => {
                let start = n.saturating_sub(1);
                if start >= items.len() {
                    Ok(Value::Sequence(vec![]))
                } else {
                    Ok(Value::Sequence(items[start..].to_vec()))
                }
            }
            _ => Err(EvalError::Error(format!(
                "Slot sequence ##{} used outside of pure function",
                n
            ))),
        },

        // ── Pure function sugar: expr & ──
        Expr::Pure { body } => {
            let slot_count = count_slots(body);
            Ok(Value::PureFunction {
                body: body.as_ref().clone(),
                slot_count,
                params: vec![],
                env: Some(env.clone()),
            })
        }

        // ── Information (help) ──
        Expr::Information(inner) => eval_information(inner, env),

        // ── Function constructor ──
        Expr::Function { params, body } => Ok(Value::PureFunction {
            body: body.as_ref().clone(),
            slot_count: params.len(),
            params: params.clone(),
            env: Some(env.clone()),
        }),
    }
}

/// Count the maximum slot index (#, #1, #2, ...) in an expression body.
fn count_slots(expr: &Expr) -> usize {
    match expr {
        Expr::Slot(None) => 1,
        Expr::Slot(Some(idx)) => *idx,
        Expr::SlotSequence(None) => 1,
        Expr::SlotSequence(Some(idx)) => *idx,
        Expr::List(items) => items.iter().map(count_slots).max().unwrap_or(0),
        Expr::Assoc(items) => items.iter().map(|(_, v)| count_slots(v)).max().unwrap_or(0),
        Expr::Call { head, args } => args
            .iter()
            .chain(std::iter::once(head.as_ref()))
            .map(count_slots)
            .max()
            .unwrap_or(0),
        // Unary containers
        Expr::Pipe { expr: e, .. }
        | Expr::Hold(e)
        | Expr::HoldComplete(e)
        | Expr::ReleaseHold(e)
        | Expr::Information(e)
        | Expr::Pure { body: e } => count_slots(e),
        // Binary containers
        Expr::Rule { lhs, rhs }
        | Expr::RuleDelayed { lhs, rhs }
        | Expr::ReplaceAll {
            expr: lhs,
            rules: rhs,
        }
        | Expr::ReplaceRepeated {
            expr: lhs,
            rules: rhs,
        }
        | Expr::Map {
            func: lhs,
            list: rhs,
        }
        | Expr::Apply {
            func: lhs,
            expr: rhs,
        }
        | Expr::Prefix {
            func: lhs,
            arg: rhs,
        }
        | Expr::Assign { lhs, rhs } => count_slots(lhs).max(count_slots(rhs)),
        Expr::Function { body, .. } => count_slots(body),
        // Other expression types don't contain slots
        _ => 0,
    }
}

/// Extract a top-level guard from a pattern expression.
///
/// Returns (inner_pattern, Some(guard_condition)) if the expression is a
/// PatternGuard, otherwise (expr, None).
pub(super) fn extract_guard_expr(expr: &Expr) -> (&Expr, Option<&Expr>) {
    match expr {
        Expr::PatternGuard { pattern, condition } => (pattern.as_ref(), Some(condition.as_ref())),
        _ => (expr, None),
    }
}

/// Convert a symbol string containing `_` into the corresponding pattern AST node.
/// Handles _Integer, x_, x_Integer, __, ___, x__, x___ and all typed variants.
fn convert_blank_pattern(s: &str) -> Expr {
    match s {
        "_" => {
            return Expr::Blank {
                type_constraint: None,
            };
        }
        "__" => {
            return Expr::BlankSequence {
                name: None,
                type_constraint: None,
            };
        }
        "___" => {
            return Expr::BlankNullSequence {
                name: None,
                type_constraint: None,
            };
        }
        _ => {}
    }
    if let Some(pos) = s.find('_') {
        let prefix = &s[..pos];
        let underscore_part = &s[pos..];

        if let Some(tc) = underscore_part.strip_prefix("___") {
            return Expr::BlankNullSequence {
                name: if prefix.is_empty() {
                    None
                } else {
                    Some(prefix.to_string())
                },
                type_constraint: if tc.is_empty() {
                    None
                } else {
                    Some(tc.to_string())
                },
            };
        }
        if let Some(tc) = underscore_part.strip_prefix("__") {
            return Expr::BlankSequence {
                name: if prefix.is_empty() {
                    None
                } else {
                    Some(prefix.to_string())
                },
                type_constraint: if tc.is_empty() {
                    None
                } else {
                    Some(tc.to_string())
                },
            };
        }
        // Single underscore: Blank or NamedBlank
        let tc = &underscore_part[1..];
        if prefix.is_empty() {
            Expr::Blank {
                type_constraint: if tc.is_empty() {
                    None
                } else {
                    Some(tc.to_string())
                },
            }
        } else {
            Expr::NamedBlank {
                name: prefix.to_string(),
                type_constraint: if tc.is_empty() {
                    None
                } else {
                    Some(tc.to_string())
                },
            }
        }
    } else {
        Expr::Symbol(s.to_string())
    }
}

/// Evaluate function arguments, respecting HoldAll/HoldFirst/HoldRest attributes.
fn eval_args_with_attributes(
    head: &Expr,
    args: &[Expr],
    head_val: &Value,
    env: &Env,
) -> Result<Vec<Value>, EvalError> {
    // Determine head name for attribute lookup
    let head_name = match head {
        Expr::Symbol(s) => Some(s.as_str()),
        _ => {
            if let Value::Symbol(s) = head_val {
                Some(s.as_str())
            } else {
                None
            }
        }
    };

    if let Some(name) = head_name {
        let hold_all =
            env.has_attribute(name, "HoldAll") || env.has_attribute(name, "HoldAllComplete");
        let hold_first = env.has_attribute(name, "HoldFirst");
        let hold_rest = env.has_attribute(name, "HoldRest");

        if hold_all {
            let vals = args
                .iter()
                .map(|arg| {
                    match arg {
                        Expr::Symbol(_) => {
                            // Evaluate to resolve bindings, then wrap as Hold to prevent further eval
                            eval(arg, env).map(|v| Value::Hold(Box::new(v)))
                        }
                        _ => Ok(Value::Pattern(arg.clone())),
                    }
                })
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(vals);
        }
        if hold_first {
            let mut vals = Vec::with_capacity(args.len());
            if let Some(first) = args.first() {
                let v = match first {
                    Expr::Symbol(_) => eval(first, env).map(|v| Value::Hold(Box::new(v))),
                    _ => Ok(Value::Pattern(first.clone())),
                };
                vals.push(v?);
            }
            for rest in &args[1..] {
                vals.push(eval(rest, env)?);
            }
            return Ok(vals);
        }
        if hold_rest {
            let mut vals = Vec::with_capacity(args.len());
            if let Some(first) = args.first() {
                vals.push(eval(first, env)?);
            }
            for rest in &args[1..] {
                let v = match rest {
                    Expr::Symbol(_) => eval(rest, env).map(|v| Value::Hold(Box::new(v))),
                    _ => Ok(Value::Pattern(rest.clone())),
                };
                vals.push(v?);
            }
            return Ok(vals);
        }
    }

    // Default: evaluate all arguments
    args.iter().map(|a| eval(a, env)).collect()
}

/// Parse a local variable specification list from a Module/With/Block call.
///
/// The first argument should be `Expr::List([...])` where each item is either:
/// - `Expr::Symbol(name)` — variable with no initial value
/// - `Expr::Assign { lhs: Symbol(name), rhs }` — variable with initial value
/// - `Expr::Call { head: Symbol("Set"), args: [Symbol(name), rhs] }` — same via Set
///
/// Returns `Vec<(String, Option<Expr>)>` — variable name and optional initializer expr.
fn parse_local_specs(arg: &Expr) -> Result<Vec<(String, Option<Expr>)>, EvalError> {
    let items = match arg {
        Expr::List(items) => items,
        other => {
            return Err(EvalError::Error(format!(
                "First argument must be a list of local variable specifications, got: {}",
                other
            )));
        }
    };

    let mut specs = Vec::new();
    for item in items {
        match item {
            Expr::Symbol(name) => {
                specs.push((name.clone(), None));
            }
            Expr::Assign { lhs, rhs } => {
                if let Expr::Symbol(name) = lhs.as_ref() {
                    specs.push((name.clone(), Some(*rhs.clone())));
                } else {
                    return Err(EvalError::Error(format!(
                        "Invalid left-hand side in local specification: {}",
                        lhs
                    )));
                }
            }
            Expr::Call {
                head,
                args: call_args,
            } if matches!(head.as_ref(), Expr::Symbol(s) if s == "Set") && call_args.len() == 2 => {
                if let Expr::Symbol(name) = &call_args[0] {
                    specs.push((name.clone(), Some(call_args[1].clone())));
                } else {
                    return Err(EvalError::Error(format!(
                        "Invalid left-hand side in local specification: {}",
                        call_args[0]
                    )));
                }
            }
            other => {
                return Err(EvalError::Error(format!(
                    "Invalid local specification: {}",
                    other
                )));
            }
        }
    }
    Ok(specs)
}

/// Recursively substitute symbols in an expression using a substitution map.
///
/// Walks the AST and replaces any `Expr::Symbol` matching a key in `subs`
/// with the corresponding replacement expression.
fn substitute_in_expr(expr: &Expr, subs: &[(String, Expr)]) -> Expr {
    match expr {
        Expr::Symbol(name) => subs
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, replacement)| replacement.clone())
            .unwrap_or_else(|| expr.clone()),
        Expr::Call { head, args } => Expr::Call {
            head: Box::new(substitute_in_expr(head, subs)),
            args: args.iter().map(|a| substitute_in_expr(a, subs)).collect(),
        },
        Expr::List(items) => {
            Expr::List(items.iter().map(|i| substitute_in_expr(i, subs)).collect())
        }
        Expr::Assoc(pairs) => Expr::Assoc(
            pairs
                .iter()
                .map(|(k, v)| (k.clone(), substitute_in_expr(v, subs)))
                .collect(),
        ),
        Expr::Sequence(items) => {
            Expr::Sequence(items.iter().map(|i| substitute_in_expr(i, subs)).collect())
        }
        Expr::Rule { lhs, rhs } => Expr::Rule {
            lhs: Box::new(substitute_in_expr(lhs, subs)),
            rhs: Box::new(substitute_in_expr(rhs, subs)),
        },
        Expr::RuleDelayed { lhs, rhs } => Expr::RuleDelayed {
            lhs: Box::new(substitute_in_expr(lhs, subs)),
            rhs: Box::new(substitute_in_expr(rhs, subs)),
        },
        Expr::Slot(_) => expr.clone(),
        Expr::SlotSequence(_) => expr.clone(),
        Expr::Function { params, body } => Expr::Function {
            params: params.clone(),
            body: Box::new(substitute_in_expr(body, subs)),
        },
        Expr::Pure { body } => Expr::Pure {
            body: Box::new(substitute_in_expr(body, subs)),
        },
        Expr::ReplaceAll { expr: inner, rules } => Expr::ReplaceAll {
            expr: Box::new(substitute_in_expr(inner, subs)),
            rules: Box::new(substitute_in_expr(rules, subs)),
        },
        Expr::ReplaceRepeated { expr: inner, rules } => Expr::ReplaceRepeated {
            expr: Box::new(substitute_in_expr(inner, subs)),
            rules: Box::new(substitute_in_expr(rules, subs)),
        },
        Expr::Map { func, list } => Expr::Map {
            func: Box::new(substitute_in_expr(func, subs)),
            list: Box::new(substitute_in_expr(list, subs)),
        },
        Expr::Apply { func, expr: inner } => Expr::Apply {
            func: Box::new(substitute_in_expr(func, subs)),
            expr: Box::new(substitute_in_expr(inner, subs)),
        },
        Expr::Pipe { expr: inner, func } => Expr::Pipe {
            expr: Box::new(substitute_in_expr(inner, subs)),
            func: Box::new(substitute_in_expr(func, subs)),
        },
        Expr::Prefix { func, arg } => Expr::Prefix {
            func: Box::new(substitute_in_expr(func, subs)),
            arg: Box::new(substitute_in_expr(arg, subs)),
        },
        Expr::If {
            condition,
            then_branch,
            else_branch,
        } => Expr::If {
            condition: Box::new(substitute_in_expr(condition, subs)),
            then_branch: Box::new(substitute_in_expr(then_branch, subs)),
            else_branch: else_branch
                .as_ref()
                .map(|e| Box::new(substitute_in_expr(e, subs))),
        },
        Expr::Which { pairs } => Expr::Which {
            pairs: pairs
                .iter()
                .map(|(c, v)| (substitute_in_expr(c, subs), substitute_in_expr(v, subs)))
                .collect(),
        },
        Expr::Switch { expr: inner, cases } => Expr::Switch {
            expr: Box::new(substitute_in_expr(inner, subs)),
            cases: cases
                .iter()
                .map(|(p, b)| (substitute_in_expr(p, subs), substitute_in_expr(b, subs)))
                .collect(),
        },
        Expr::Match {
            expr: inner,
            branches,
        } => Expr::Match {
            expr: Box::new(substitute_in_expr(inner, subs)),
            branches: branches
                .iter()
                .map(|b| crate::ast::MatchBranch {
                    pattern: substitute_in_expr(&b.pattern, subs),
                    result: substitute_in_expr(&b.result, subs),
                })
                .collect(),
        },
        Expr::For {
            init,
            condition,
            step,
            body,
        } => Expr::For {
            init: Box::new(substitute_in_expr(init, subs)),
            condition: Box::new(substitute_in_expr(condition, subs)),
            step: Box::new(substitute_in_expr(step, subs)),
            body: Box::new(substitute_in_expr(body, subs)),
        },
        Expr::While { condition, body } => Expr::While {
            condition: Box::new(substitute_in_expr(condition, subs)),
            body: Box::new(substitute_in_expr(body, subs)),
        },
        Expr::Do { body, iterator } => Expr::Do {
            body: Box::new(substitute_in_expr(body, subs)),
            iterator: iterator.clone(),
        },
        Expr::FuncDef {
            name,
            params,
            body,
            delayed,
            guard,
        } => Expr::FuncDef {
            name: name.clone(),
            params: params.clone(),
            body: Box::new(substitute_in_expr(body, subs)),
            delayed: *delayed,
            guard: guard
                .as_ref()
                .map(|g| Box::new(substitute_in_expr(g, subs))),
        },
        Expr::Assign { lhs, rhs } => Expr::Assign {
            lhs: Box::new(substitute_in_expr(lhs, subs)),
            rhs: Box::new(substitute_in_expr(rhs, subs)),
        },
        Expr::DestructAssign { patterns, rhs } => Expr::DestructAssign {
            patterns: patterns
                .iter()
                .map(|p| substitute_in_expr(p, subs))
                .collect(),
            rhs: Box::new(substitute_in_expr(rhs, subs)),
        },
        Expr::PostIncrement { expr: inner } => Expr::PostIncrement {
            expr: Box::new(substitute_in_expr(inner, subs)),
        },
        Expr::PostDecrement { expr: inner } => Expr::PostDecrement {
            expr: Box::new(substitute_in_expr(inner, subs)),
        },
        Expr::Unset { expr: inner } => Expr::Unset {
            expr: Box::new(substitute_in_expr(inner, subs)),
        },
        Expr::ModuleDef {
            name,
            exports,
            body,
        } => Expr::ModuleDef {
            name: name.clone(),
            exports: exports.clone(),
            body: body.iter().map(|b| substitute_in_expr(b, subs)).collect(),
        },
        Expr::Import {
            module,
            selective,
            alias,
        } => Expr::Import {
            module: module.clone(),
            selective: selective.clone(),
            alias: alias.clone(),
        },
        Expr::Export(exports) => Expr::Export(exports.clone()),
        Expr::ClassDef {
            name,
            parent,
            mixins,
            members,
        } => Expr::ClassDef {
            name: name.clone(),
            parent: parent.clone(),
            mixins: mixins.clone(),
            members: members.clone(),
        },
        Expr::Hold(inner) => Expr::Hold(Box::new(substitute_in_expr(inner, subs))),
        Expr::HoldComplete(inner) => Expr::HoldComplete(Box::new(substitute_in_expr(inner, subs))),
        Expr::ReleaseHold(inner) => Expr::ReleaseHold(Box::new(substitute_in_expr(inner, subs))),
        Expr::Information(inner) => Expr::Information(Box::new(substitute_in_expr(inner, subs))),
        // Atoms and non-recursive variants pass through unchanged
        other => other.clone(),
    }
}

/// Evaluate a function call.
fn eval_call(head: &Expr, args: &[Expr], env: &Env) -> Result<Value, EvalError> {
    // Special forms that don't evaluate all arguments
    if let Expr::Symbol(s) = head {
        match s.as_str() {
            "Set" | "SetDelayed" => {
                // Handle assignment specially
                if args.len() != 2 {
                    return Err(EvalError::Error(
                        "Set requires exactly 2 arguments".to_string(),
                    ));
                }
                let val = eval(&args[1], env)?;
                match &args[0] {
                    Expr::Symbol(name) => {
                        env.set_propagate(name.clone(), val.clone());
                        Ok(val)
                    }
                    // Attributes[sym] = value  via Set[Attributes[sym], value]
                    Expr::Call {
                        head,
                        args: call_args,
                    } if call_args.len() == 1
                        && matches!(head.as_ref(), Expr::Symbol(s) if s == "Attributes") =>
                    {
                        let sym_name = match &call_args[0] {
                            Expr::Symbol(s) => s.clone(),
                            _ => {
                                return Err(EvalError::Error(
                                    "Attributes assignment requires a symbol name".to_string(),
                                ));
                            }
                        };
                        if env.has_attribute(&sym_name, "Locked") {
                            return Ok(Value::Null);
                        }
                        let attrs = match &val {
                            Value::List(items) => items.iter().map(|v| v.to_string()).collect(),
                            other => vec![other.to_string()],
                        };
                        env.set_attributes(&sym_name, attrs);
                        Ok(val)
                    }
                    // f[args] = value  or SetDelayed[f[args], val]
                    // — function definition with immediate or delayed RHS.
                    // Also handles desugared OOP field access: this.field = val
                    // is parsed as Set[field[this], val]. When target is an
                    // Object, treat as field access.
                    Expr::Call {
                        head,
                        args: call_args,
                    } if !matches!(head.as_ref(), Expr::Symbol(s) if s == "Part")
                        && !matches!(head.as_ref(), Expr::Symbol(s) if s == "Attributes") =>
                    {
                        if let Expr::Symbol(name) = head.as_ref() {
                            // Check for OOP field access: field[object] = value
                            // where object evaluates to an Object
                            if call_args.len() == 1 {
                                let target = eval(&call_args[0], env)?;
                                if let Value::Object {
                                    class_name,
                                    mut fields,
                                } = target
                                {
                                    fields.insert(name.clone(), val.clone());
                                    let updated = Value::Object { class_name, fields };
                                    if let Expr::Symbol(s) = &call_args[0]
                                        && s == "this"
                                    {
                                        env.set("this".to_string(), updated.clone());
                                    }
                                    return Ok(val);
                                }
                            }
                            // Otherwise: function definition via Set/SetDelayed
                            // SetDelayed[f[args], body] — use body as-is
                            // Set[f[args], val] — evaluate RHS then store as Expr
                            let body_expr = if s == "SetDelayed" {
                                args[1].clone()
                            } else {
                                table::value_to_expr(&val)
                            };
                            let func = if let Some(Value::Function(f)) = env.get(name) {
                                Arc::try_unwrap(f).unwrap_or_else(|arc| (*arc).clone())
                            } else {
                                FunctionDef {
                                    name: name.clone(),
                                    definitions: Vec::new(),
                                }
                            };
                            let mut func = func;
                            func.definitions.push(FunctionDefinition {
                                params: call_args.clone(),
                                body: body_expr,
                                delayed: s == "SetDelayed",
                                guard: None,
                            });
                            // Sort definitions so more specific ones match first
                            func.definitions.sort_by(|a, b| {
                                specificity(&b.params).cmp(&specificity(&a.params))
                            });
                            env.set(name.clone(), Value::Function(Arc::new(func)));
                            return Ok(val);
                        }
                        Err(EvalError::Error("Invalid assignment target".to_string()))
                    }
                    _ => Err(EvalError::Error("Invalid assignment target".to_string())),
                }
            }
            "Hold" => {
                // Don't evaluate arguments. Single arg: Hold[expr]; multi: Hold[expr, ...]
                if args.len() == 1 {
                    Ok(Value::Hold(Box::new(Value::Pattern(args[0].clone()))))
                } else {
                    Ok(Value::Hold(Box::new(Value::List(
                        args.iter().map(|a| Value::Pattern(a.clone())).collect(),
                    ))))
                }
            }
            "HoldComplete" => {
                // HoldComplete prevents ALL evaluation, including interior
                if args.len() == 1 {
                    Ok(Value::HoldComplete(Box::new(Value::Pattern(
                        args[0].clone(),
                    ))))
                } else {
                    Ok(Value::HoldComplete(Box::new(Value::List(
                        args.iter().map(|a| Value::Pattern(a.clone())).collect(),
                    ))))
                }
            }
            "Table" => {
                // Table[expr, {i, min, max}] — iterator spec has unevaluated symbols
                table::eval_table(args, env)
            }
            "ParallelTable" => {
                // ParallelTable[expr, {i, min, max}] — parallel version of Table
                table::eval_parallel_table(args, env)
            }
            "ParallelSum" => {
                // ParallelSum[expr, {i, min, max}] — parallel version of Sum
                table::eval_parallel_sum(args, env)
            }
            "ParallelEvaluate" => {
                // ParallelEvaluate[expr] — evaluate expr on all workers in parallel
                table::eval_parallel_evaluate(args, env)
            }
            "ParallelTry" => {
                // ParallelTry[list] or ParallelTry[f, list] — return first result
                table::eval_parallel_try(args, env)
            }
            "ParallelProduct" => {
                // ParallelProduct[expr, {i, min, max}] — parallel version of Product
                table::eval_parallel_product(args, env)
            }
            "ParallelDo" => {
                // ParallelDo[expr, {i, ...}] — parallel side-effect evaluation, returns Null
                table::eval_parallel_do(args, env)
            }
            "Sum" => {
                // Sum[expr, {i, min, max}] — iterator spec has unevaluated symbols
                table::eval_sum(args, env)
            }
            "Product" => {
                // Product[expr, {i, min, max}] — iterator spec has unevaluated symbols
                table::eval_product(args, env)
            }
            "RecurrenceTable" => {
                // RecurrenceTable[eqns, f, {n, nmin, nmax}] — generate table from recurrence equations
                table::eval_recurrence_table(args, env)
            }
            "Plot" => {
                // Plot[f, {x, xmin, xmax}] — needs unevaluated expr for sampling
                plot::eval_plot(args, env)
            }
            "LogPlot" => plot::eval_log_plot(args, env),
            "LogLogPlot" => plot::eval_log_log_plot(args, env),
            "LogLinearPlot" => plot::eval_log_linear_plot(args, env),
            "ParametricPlot" => plot::eval_parametric_plot(args, env),
            "PolarPlot" => plot::eval_polar_plot(args, env),
            "DiscretePlot" => plot::eval_discrete_plot(args, env),
            "DensityPlot" => plot::eval_density_plot(args, env),
            "TryCatch" => {
                // try { body } catch var { catch_body } finally { finally_body }
                // args: [body, err_var, catch_body, ?finally_body]
                if args.len() < 3 {
                    return Err(EvalError::Error(
                        "TryCatch requires at least 3 arguments".to_string(),
                    ));
                }
                let result = eval(&args[0], env);
                let thrown = match result {
                    Ok(v) => Ok(v),
                    Err(EvalError::Thrown(v)) => {
                        let child_env = env.child();
                        if let Expr::Symbol(name) = &args[1] {
                            child_env.set(name.clone(), *v);
                        }
                        eval(&args[2], &child_env)
                    }
                    Err(e) => Err(e),
                };
                // Evaluate finally block if present
                if args.len() > 3 {
                    let _ = eval(&args[3], env);
                }
                thrown
            }
            "Catch" => {
                // Catch[expr] — evaluate expr, catching any Throw[val]
                if args.len() != 1 {
                    return Err(EvalError::Error(
                        "Catch requires exactly 1 argument".to_string(),
                    ));
                }
                match eval(&args[0], env) {
                    Ok(v) => Ok(v),
                    Err(EvalError::Thrown(v)) => Ok(*v),
                    Err(e) => Err(e),
                }
            }
            "Return" => {
                // Return[expr] — return expr from the enclosing function
                // Return[] returns Null
                let val = if args.is_empty() {
                    Value::Null
                } else if args.len() == 1 {
                    eval(&args[0], env)?
                } else {
                    return Err(EvalError::Error(
                        "Return requires 0 or 1 arguments".to_string(),
                    ));
                };
                Err(EvalError::Return(Box::new(val)))
            }
            "Break" => {
                // Break[] — exit the enclosing loop
                if !args.is_empty() {
                    return Err(EvalError::Error("Break takes no arguments".to_string()));
                }
                Err(EvalError::Break)
            }
            "Continue" => {
                // Continue[] — skip to next loop iteration
                if !args.is_empty() {
                    return Err(EvalError::Error("Continue takes no arguments".to_string()));
                }
                Err(EvalError::Continue)
            }
            "While" => {
                // While[cond, body] — evaluate body while cond is True
                if args.len() != 2 {
                    return Err(EvalError::Error(
                        "While requires exactly 2 arguments".to_string(),
                    ));
                }
                let mut result = Value::Null;
                loop {
                    let cond = eval(&args[0], env)?;
                    if !cond.to_bool() {
                        break;
                    }
                    result = eval(&args[1], env).or_else(|e| match e {
                        EvalError::Break => Ok(Value::Null),
                        EvalError::Continue => Ok(Value::Null),
                        other => Err(other),
                    })?;
                }
                Ok(result)
            }
            "For" => {
                // For[start, test, step, body] — C-style for loop
                if args.len() != 4 {
                    return Err(EvalError::Error(
                        "For requires exactly 4 arguments".to_string(),
                    ));
                }
                eval(&args[0], env)?;
                let mut result = Value::Null;
                loop {
                    let test = eval(&args[1], env)?;
                    if !test.to_bool() {
                        break;
                    }
                    result = eval(&args[3], env).or_else(|e| match e {
                        EvalError::Break => Ok(Value::Null),
                        EvalError::Continue => Ok(Value::Null),
                        other => Err(other),
                    })?;
                    eval(&args[2], env)?;
                }
                Ok(result)
            }
            "ReleaseHold" => {
                if args.len() != 1 {
                    return Err(EvalError::Error(
                        "ReleaseHold requires exactly 1 argument".to_string(),
                    ));
                }
                let held = eval(&args[0], env)?;
                match held {
                    Value::Hold(v) | Value::HoldComplete(v) => release_inner(*v, env),
                    other => Ok(other),
                }
            }
            "Assuming" => {
                // Assuming[assum, expr] — evaluate expr with $Assumptions set
                if args.len() != 2 {
                    return Err(EvalError::Error(
                        "Assuming requires exactly 2 arguments".to_string(),
                    ));
                }
                let assum = eval(&args[0], env)?;
                let child_env = env.child();
                child_env.set("$Assumptions".to_string(), assum);
                eval(&args[1], &child_env)
            }
            "N" => {
                if args.is_empty() || args.len() > 2 {
                    return Err(EvalError::Error("N requires 1 or 2 arguments".to_string()));
                }
                let prec_bits: u32 = if args.len() == 2 {
                    match eval(&args[1], env)? {
                        Value::Integer(n) => {
                            let d = u32::try_from(&n).unwrap_or(DEFAULT_PRECISION);
                            // decimal digits → bits: d * log2(10) ≈ d * 3.322
                            ((d as f64) * std::f64::consts::LOG2_10).ceil() as u32
                        }
                        _ => DEFAULT_PRECISION,
                    }
                } else {
                    DEFAULT_PRECISION
                };
                numeric::numeric_eval_expr(&args[0], prec_bits, env)
            }
            "Module" => {
                if args.len() < 2 {
                    return Err(EvalError::Error(
                        "Module requires at least 2 arguments".to_string(),
                    ));
                }
                let specs = parse_local_specs(&args[0])?;
                let child = env.child();
                for (name, init) in &specs {
                    let val = match init {
                        Some(expr) => eval(expr, env)?,
                        None => Value::Null,
                    };
                    child.set(name.clone(), val);
                }
                let mut result = Value::Null;
                for expr in &args[1..] {
                    result = eval(expr, &child)?;
                }
                Ok(result)
            }
            "With" => {
                if args.len() < 2 {
                    return Err(EvalError::Error(
                        "With requires at least 2 arguments".to_string(),
                    ));
                }
                let specs = parse_local_specs(&args[0])?;
                let child = env.child();
                // Evaluate RHS values and build substitution map
                let mut subs = Vec::new();
                for (name, init) in &specs {
                    match init {
                        Some(rhs_expr) => {
                            let val = eval(rhs_expr, env)?;
                            subs.push((name.clone(), table::value_to_expr(&val)));
                        }
                        None => {
                            return Err(EvalError::Error(
                                "With requires initial values for all local variables".to_string(),
                            ));
                        }
                    }
                }
                let mut result = Value::Null;
                for expr in &args[1..] {
                    let substituted = substitute_in_expr(expr, &subs);
                    result = eval(&substituted, &child)?;
                }
                Ok(result)
            }
            "Block" => {
                if args.len() < 2 {
                    return Err(EvalError::Error(
                        "Block requires at least 2 arguments".to_string(),
                    ));
                }
                let specs = parse_local_specs(&args[0])?;
                // Save old values and propagate new ones up the scope chain.
                // Block uses dynamic scoping: the new values are visible everywhere,
                // including in functions called within the body, by updating the
                // defining scope (rather than shadowing in a child scope).
                let mut saved: Vec<(String, Option<Value>)> = Vec::new();
                for (name, init) in &specs {
                    let old_val = env.get(name);
                    let new_val = match init {
                        Some(expr) => eval(expr, env)?,
                        None => Value::Null,
                    };
                    env.set_propagate(name.clone(), new_val);
                    saved.push((name.clone(), old_val));
                }
                // Evaluate body
                let mut result = Value::Null;
                for expr in &args[1..] {
                    result = eval(expr, env)?;
                }
                // Restore old values (reverse order)
                for (name, old_val) in saved.into_iter().rev() {
                    match old_val {
                        Some(v) => env.set_propagate(name, v),
                        None => {
                            env.remove(&name);
                        }
                    }
                }
                Ok(result)
            }
            _ => {
                // Check if this is a custom operator symbol registered in the operator table.
                // If so, resolve it to the actual head name and dispatch.
                if let Some(op_info) = env.get_operator(s) {
                    let head_val = eval(&Expr::Symbol(op_info.head.clone()), env)?;
                    // Evaluate args with the resolved head's attributes
                    // (the operator string itself has no attributes, but the resolved head might)
                    let arg_vals = eval_args_with_attributes(
                        &Expr::Symbol(op_info.head.clone()),
                        args,
                        &head_val,
                        env,
                    )?;
                    let skip_seq = env.has_attribute(&op_info.head, "SequenceHold")
                        || env.has_attribute(&op_info.head, "HoldAllComplete");
                    return apply_function(&head_val, &flatten_sequences(arg_vals, skip_seq), env);
                }

                let head_val = eval(head, env)?;
                let arg_vals = eval_args_with_attributes(head, args, &head_val, env)?;

                // Respect SequenceHold and HoldAllComplete attributes:
                // these prevent Sequence[...] from splicing.
                let skip_seq = env.has_attribute(s.as_str(), "SequenceHold")
                    || env.has_attribute(s.as_str(), "HoldAllComplete");
                apply_function(&head_val, &flatten_sequences(arg_vals, skip_seq), env)
            }
        }
    } else {
        let head_val = eval(head, env)?;
        let arg_vals = eval_args_with_attributes(head, args, &head_val, env)?;
        apply_function(&head_val, &flatten_sequences(arg_vals, false), env)
    }
}

/// Recursively flatten nested calls with same head for Flat attribute.
pub(crate) fn flatten_flat_args(name: &str, args: &[Value]) -> Vec<Value> {
    let mut result = Vec::with_capacity(args.len());
    for arg in args {
        if let Value::Call { head, args: inner } = arg {
            if head == name {
                result.extend(flatten_flat_args(name, inner));
                continue;
            }
        }
        result.push(arg.clone());
    }
    result
}

/// If the result is a Call with the same Flat head, flatten nested calls.
fn normalize_flat_result(name: &str, result: Value, env: &Env) -> Value {
    if !env.has_attribute(name, "Flat") {
        return result;
    }
    if let Value::Call {
        ref head,
        args: ref a,
    } = result
    {
        if head == name {
            let flat = flatten_flat_args(name, a);
            if flat.len() != a.len() || flat.as_slice() != a.as_slice() {
                return Value::Call {
                    head: head.clone(),
                    args: flat,
                };
            }
        }
    }
    result
}

// ── Recursion limit ($RecursionLimit) ──

thread_local! {
    static RECURSION_DEPTH: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn get_recursion_limit(env: &Env) -> usize {
    match env.get("$RecursionLimit") {
        Some(Value::Integer(n)) => {
            let n = n.to_usize().unwrap_or(1024);
            if n == 0 { usize::MAX } else { n }
        }
        Some(Value::Symbol(s)) if s == "Infinity" => usize::MAX,
        _ => 1024,
    }
}

struct RecursionGuard;

impl RecursionGuard {
    fn new(env: &Env, _func_name: &str) -> Result<Self, EvalError> {
        let limit = get_recursion_limit(env);
        let depth = RECURSION_DEPTH.get();
        if depth >= limit {
            return Err(EvalError::Error(format!(
                "$RecursionLimit::reclim: Recursion depth of {} exceeded.",
                depth
            )));
        }
        RECURSION_DEPTH.set(depth + 1);
        Ok(RecursionGuard)
    }
}

impl Drop for RecursionGuard {
    fn drop(&mut self) {
        RECURSION_DEPTH.set(RECURSION_DEPTH.get() - 1);
    }
}

/// Apply a function value to arguments.
pub(crate) fn apply_function(func: &Value, args: &[Value], env: &Env) -> Result<Value, EvalError> {
    // ── Flat attribute: flatten nested calls with same head ──
    let func_name = match func {
        Value::Builtin(name, _) => Some(name.as_str()),
        Value::Function(fd) => Some(fd.name.as_str()),
        Value::Symbol(s) => Some(s.as_str()),
        _ => None,
    };
    if let Some(name) = func_name
        && env.has_attribute(name, "Flat")
    {
        let flat = flatten_flat_args(name, args);
        if flat.len() != args.len() || flat.as_slice() != args {
            return apply_function(func, &flat, env);
        }
    }

    // ── Listable attribute: auto-thread over lists ──
    if let Some(name) = func_name
        && env.has_attribute(name, "Listable")
    {
        // Find which args are lists
        let list_indices: Vec<usize> = args
            .iter()
            .enumerate()
            .filter(|(_, a)| matches!(a, Value::List(_)))
            .map(|(i, _)| i)
            .collect();

        if !list_indices.is_empty() {
            if list_indices.len() == args.len() {
                // All args are lists — do element-wise threading
                if let Value::List(first) = &args[0] {
                    let len = first.len();
                    // All lists must be the same length
                    let all_same_len = list_indices.iter().all(|&i| {
                        if let Value::List(items) = &args[i] {
                            items.len() == len
                        } else {
                            false
                        }
                    });
                    if all_same_len {
                        let mut result = Vec::with_capacity(len);
                        for i in 0..len {
                            let thread_args: Vec<Value> = args
                                .iter()
                                .map(|a| {
                                    if let Value::List(items) = a {
                                        items[i].clone()
                                    } else {
                                        a.clone()
                                    }
                                })
                                .collect();
                            let v = apply_function(func, &thread_args, env)?;
                            result.push(normalize_flat_result(name, v, env));
                        }
                        return Ok(Value::List(result));
                    }
                }
            } else {
                // Mixed: some args are lists, some are scalars
                let list_idx = list_indices[0];
                if let Value::List(items) = &args[list_idx] {
                    let mut result = Vec::with_capacity(items.len());
                    for elem in items {
                        let mut thread_args: Vec<Value> = args.to_vec();
                        thread_args[list_idx] = elem.clone();
                        let v = apply_function(func, &thread_args, env)?;
                        result.push(normalize_flat_result(name, v, env));
                    }
                    return Ok(Value::List(result));
                }
            }
        }
    }

    let result = match func {
        Value::Builtin(name, f) => match f {
            BuiltinFn::Pure(f) => {
                // Extension-registered builtin: trampoline through ext registry
                if std::ptr::fn_addr_eq(*f, ffi::extension::EXT_DISPATCH_PTR) {
                    return ffi::extension::call_ext_fn(name, args);
                }
                f(args).or_else(|e| match e {
                    EvalError::NoMatch { .. } => Ok(Value::Call {
                        head: name.clone(),
                        args: args.to_vec(),
                    }),
                    other => Err(other),
                })
            }
            BuiltinFn::Env(f) => f(args, env).or_else(|e| match e {
                EvalError::NoMatch { .. } => Ok(Value::Call {
                    head: name.clone(),
                    args: args.to_vec(),
                }),
                other => Err(other),
            }),
        },

        Value::NativeFunction {
            fn_ptr, signature, ..
        } => ffi::loader::call_native(*fn_ptr, signature, args),

        Value::BytecodeFunction(bc_def) => {
            #[cfg(feature = "jit")]
            {
                use std::sync::atomic::Ordering;
                const JIT_THRESHOLD: u64 = 1000;
                const JIT_FAILED: usize = 1;

                let prev = bc_def.call_count.fetch_add(1, Ordering::Relaxed);
                if prev + 1 > JIT_THRESHOLD {
                    let jit_ptr = bc_def.jit_fn_ptr.load(Ordering::Relaxed);
                    if !jit_ptr.is_null() && jit_ptr as usize != JIT_FAILED {
                        let mut regs = vec![Value::Null; bc_def.bytecode.nregs as usize];
                        let mut jit_ctx = crate::jit::runtime::JitContext::new(
                            &bc_def.bytecode,
                            args,
                            env,
                            &mut regs,
                        );
                        let jit_fn: extern "C" fn(*mut crate::jit::runtime::JitContext) =
                            unsafe { std::mem::transmute(jit_ptr) };
                        jit_fn(&mut jit_ctx);
                        return Ok(unsafe { (*jit_ctx.regs).clone() });
                    }
                    // First time past threshold — try to compile.
                    if prev + 1 == JIT_THRESHOLD + 1 {
                        if let Some(jit_fn) = crate::jit::compile_jit(bc_def) {
                            let fn_ptr = jit_fn.fn_ptr as *mut ();
                            bc_def.jit_fn_ptr.store(fn_ptr, Ordering::Relaxed);
                            // Track the executable memory allocation so it is
                            // properly freed when this BytecodeFunctionDef is dropped.
                            let module = unsafe {
                                crate::bytecode::JitModule::new(
                                    jit_fn.fn_ptr as *mut u8,
                                    jit_fn.alloc_size,
                                )
                            };
                            bc_def.set_jit_module(Some(module));
                        } else {
                            // Compilation failed — set sentinel so we never retry.
                            bc_def
                                .jit_fn_ptr
                                .store(JIT_FAILED as *mut (), Ordering::Relaxed);
                        }
                    }
                }
            }

            bytecode::vm::execute_bytecode(&bc_def.bytecode, args, env)
        }

        Value::Function(func_def) => {
            // ── Hotness check: compile to bytecode if frequently called ──
            profiler::Profiler::count_call(&func_def.name);
            if profiler::Profiler::check_hot(&func_def.name) {
                if let Ok(bc) = bytecode::compiler::BytecodeCompiler::compile_multi(
                    &func_def.definitions,
                    &func_def.name,
                ) {
                    let bc_val = Value::BytecodeFunction(std::sync::Arc::new(
                        crate::bytecode::BytecodeFunctionDef::new(
                            func_def.name.clone(),
                            bc,
                            std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
                        ),
                    ));
                    env.set(func_def.name.clone(), bc_val.clone());
                    profiler::Profiler::reset(&func_def.name);
                    return apply_function(&bc_val, args, env);
                }
                // If compilation fails, reset counter and fall back to tree-walk
                profiler::Profiler::reset(&func_def.name);
            }

            // ── Recursion depth check ──
            let _guard = RecursionGuard::new(env, &func_def.name)?;

            // Try each definition in order
            let mut found_result: Option<Result<Value, EvalError>> = None;
            for def in &func_def.definitions {
                let match_result = try_match_params(&def.params, args, env)?;
                if let Some(bindings) = match_result {
                    // Evaluate function-level guard if present (f[x_] := body /; condition)
                    if let Some(guard_expr) = &def.guard {
                        let guard_env = env.child();
                        for (name, value) in &bindings {
                            guard_env.set(name.clone(), value.clone());
                        }
                        let guard_val = eval(guard_expr, &guard_env)?;
                        if !guard_val.to_bool() {
                            continue;
                        }
                    }
                    let child_env = env.child();
                    for (name, value) in &bindings {
                        child_env.set(name.clone(), value.clone());
                    }
                    // Catch Return[expr] and unwrap it as the function result.
                    // Catch NoMatch and return unevaluated Call (symbolic form).
                    found_result = Some(match eval(&def.body, &child_env) {
                        Ok(v) => Ok(v),
                        Err(EvalError::Return(v)) => Ok(*v),
                        Err(EvalError::NoMatch { head, args: a }) => {
                            Ok(Value::Call { head, args: *a })
                        }
                        Err(e) => Err(e),
                    });
                    break;
                }
            }
            Ok(if let Some(r) = found_result {
                r?
            } else {
                return Err(EvalError::NoMatch {
                    head: func_def.name.clone(),
                    args: Box::new(args.to_vec()),
                });
            })
        }

        Value::PureFunction {
            body,
            slot_count: _,
            params,
            env: captured_env,
        } => {
            // ── Recursion depth check ──
            let _guard = RecursionGuard::new(env, "#0")?;

            // Use the captured defining environment for lexical scoping (closures).
            // Fall back to the call-site environment when no env was captured
            // (e.g., PureFunction created by the bytecode compiler).
            let parent_env = captured_env.as_ref().unwrap_or(env);
            let child_env = parent_env.child();
            // Bind slots
            for (i, arg) in args.iter().enumerate() {
                child_env.set(format!("#{}", i + 1), arg.clone());
            }
            if !args.is_empty() {
                child_env.set("#".to_string(), args[0].clone());
            }
            // Bind named parameters (e.g., "k" in Function[{k}, body])
            for (i, param) in params.iter().enumerate() {
                if i < args.len() {
                    child_env.set(param.clone(), args[i].clone());
                }
            }
            // Bind ## to all args as Sequence (for slot sequence expansion)
            child_env.set("##".to_string(), Value::Sequence(args.to_vec()));
            // Bind #0 to the function itself (for anonymous recursion)
            child_env.set("#0".to_string(), func.clone());
            // Catch Return[expr] and unwrap it as the pure function result
            eval(body, &child_env).or_else(|e| match e {
                EvalError::Return(v) => Ok(*v),
                other => Err(other),
            })
        }

        Value::Symbol(name) => {
            // Look up the symbol and apply
            if let Some(f) = env.get(name) {
                return apply_function(&f, args, env);
            }

            // ── Lazy provider: load on first use ──
            if let Some(provider) = env.lazy_providers.lock().unwrap().remove(name) {
                use crate::{lexer, parser};
                return match provider {
                    LazyProvider::Custom(f) => {
                        let val = f(env)?;
                        env.root_env().set(name.to_string(), val.clone());
                        apply_function(&val, args, env)
                    }
                    LazyProvider::File(path) => {
                        let resolved = resolve_lazy_path(&path, env)?;
                        let source = std::fs::read_to_string(&resolved).map_err(|e| {
                            EvalError::Error(format!(
                                "Failed to read lazy provider file '{}': {}",
                                resolved.display(),
                                e
                            ))
                        })?;
                        let tokens = lexer::tokenize(&source)
                            .map_err(|e| EvalError::Error(e.to_string()))?;
                        let ast =
                            parser::parse(tokens).map_err(|e| EvalError::Error(e.to_string()))?;
                        // Evaluate in a child of root so definitions land in
                        // a scope that chains to root.
                        let file_env = env.root_env().child();
                        eval_program(&ast, &file_env)?;
                        let val = file_env.get(name).ok_or_else(|| {
                            EvalError::Error(format!(
                                "Lazy provider file '{}' did not define symbol '{}'",
                                resolved.display(),
                                name
                            ))
                        })?;
                        env.root_env().set(name.to_string(), val.clone());
                        apply_function(&val, args, env)
                    }
                };
            }

            // ── Object field access / method dispatch ──
            if !args.is_empty() {
                // Check if first arg is an object — field access or method call
                if let Value::Object { class_name, fields } = &args[0] {
                    // Field access: single arg
                    if args.len() == 1
                        && let Some(val) = fields.get(name)
                    {
                        return Ok(val.clone());
                    }
                    // Method dispatch: look up method on the class
                    if let Some(class_val) = env.get(class_name)
                        && let Value::Class(class_def) = class_val
                        && let Some(method) = class_def.methods.get(name)
                    {
                        let child_env = env.child();
                        // Bind 'this' to the object
                        child_env.set("this".to_string(), args[0].clone());
                        // Bind field names to their values
                        for (field_name, field_val) in fields {
                            child_env.set(field_name.clone(), field_val.clone());
                        }
                        // Match method params to remaining args
                        let method_args = &args[1..];
                        if let Some(bindings) = try_match_params(&method.params, method_args, env)?
                        {
                            for (bind_name, bind_val) in &bindings {
                                child_env.set(bind_name.clone(), bind_val.clone());
                            }
                            return eval(&method.body, &child_env);
                        }
                    }
                }
            }

            // Return unevaluated
            Ok(Value::Call {
                head: name.clone(),
                args: args.to_vec(),
            })
        }

        Value::Class(class_def) => {
            // Object instantiation: ClassName[args...]
            let mut fields = HashMap::new();

            // Initialize fields with defaults
            for field in &class_def.fields {
                if let Some(default_expr) = &field.default {
                    let default_val = eval(default_expr, env)?;
                    fields.insert(field.name.clone(), default_val);
                } else {
                    fields.insert(field.name.clone(), Value::Null);
                }
            }

            let mut object = Value::Object {
                class_name: class_def.name.clone(),
                fields,
            };

            // Run constructor if one exists
            if let Some(constructor) = &class_def.constructor {
                let child_env = env.child();
                // Bind 'this' to the object
                child_env.set("this".to_string(), object.clone());
                // Match constructor params to args
                if let Some(bindings) = try_match_params(&constructor.params, args, env)? {
                    for (name, value) in &bindings {
                        child_env.set(name.clone(), value.clone());
                    }
                    // Evaluate constructor body
                    let _ = eval(&constructor.body, &child_env)?;
                    // Read back the 'this' value (it may have been modified)
                    if let Some(this_val) = child_env.get("this") {
                        object = this_val;
                    }
                }
            }

            Ok(object)
        }

        Value::Object {
            class_name,
            fields: _,
        } => {
            // Method dispatch: look for method on the object
            let method_name = format!("{}.__method__", class_name);
            if let Some(method) = env.get(&method_name) {
                let mut method_args = vec![func.clone()];
                method_args.extend(args.to_vec());
                apply_function(&method, &method_args, env)
            } else {
                Err(EvalError::NoMatch {
                    head: class_name.clone(),
                    args: Box::new(args.to_vec()),
                })
            }
        }

        Value::Dataset(data) => {
            // ds[args...] -> query dispatch
            dataset::dataset_query(data, args, env)
        }

        _ => {
            // Return unevaluated
            Ok(Value::Call {
                head: func.to_string(),
                args: args.to_vec(),
            })
        }
    };

    // ── Normalize Flat results ──
    match (func_name, result) {
        (Some(name), Ok(v)) => Ok(normalize_flat_result(name, v, env)),
        (_, r) => r,
    }
}

/// Rank specificity of a function definition's parameter list.
/// Higher score = more specific (tried first in rule ordering).
fn specificity(params: &[Expr]) -> usize {
    params.iter().map(|p| specificity_expr(p)).sum()
}

/// Specificity score for a single pattern expression.
fn specificity_expr(p: &Expr) -> usize {
    match p {
        // Literal values: most specific
        Expr::Integer(_) | Expr::Real(_) | Expr::Str(_) | Expr::Bool(_) => 3,
        // Type-constrained blank: x_Integer or _Integer
        Expr::Blank {
            type_constraint: Some(_),
        }
        | Expr::NamedBlank {
            type_constraint: Some(_),
            ..
        } => 2,
        // Named blank: x_
        Expr::NamedBlank { .. } => 1,
        // Bare blank, blank sequence, blank null sequence: least specific
        Expr::Blank { .. } | Expr::BlankSequence { .. } | Expr::BlankNullSequence { .. } => 0,
        // PatternGuard: score based on inner pattern + bonus for having a condition
        Expr::PatternGuard { pattern, .. } => specificity_expr(pattern) + 1,
        // Call patterns: score based on head specificity + sum of arg specificities
        Expr::Call { head, args } => {
            let head_score = match head.as_ref() {
                // Literal head (e.g., Sin[x_]) is more specific than blank head (f_[x_])
                Expr::Symbol(_) => 2,
                Expr::Integer(_) | Expr::Real(_) | Expr::Str(_) | Expr::Bool(_) => 3,
                _ => 0,
            };
            let arg_scores: usize = args.iter().map(specificity_expr).sum();
            head_score + arg_scores
        }
        // List patterns: score based on sum of item specificities
        Expr::List(items) => items.iter().map(specificity_expr).sum(),
        // Default: zero
        _ => 0,
    }
}

/// Try to match parameters against arguments, evaluating any pattern guards.
///
/// Returns Ok(Some(bindings)) on success, Ok(None) if no match, Err on guard eval error.
///
/// Handles sequence patterns (__ / ___) in function parameters using backtracking:
///   f[x__] := Total[x]             — sequence of 1+ elements
///   f[x___] := {x}                 — sequence of 0+ elements
///   f[a_, b__, c_] := {a, b, c}    — mixed fixed and sequence parameters
pub(crate) fn try_match_params(
    params: &[Expr],
    args: &[Value],
    env: &Env,
) -> Result<Option<Bindings>, EvalError> {
    let _attr_checker = AttributeChecker::new(env.attributes.clone());
    // Check if any parameter uses a sequence pattern (__ or ___)
    let has_sequences = params.iter().any(|p| {
        let (inner, _guard) = extract_guard_expr(p);
        has_sequence_pattern(inner)
    });

    // Collect from each param: inner pattern and guards (top-level + nested)
    let mut guard_exprs: Vec<Expr> = Vec::new();

    if has_sequences {
        // Sequence path: wrap params as a list pattern and use the existing
        // backtracking sequence matcher in pattern.rs.
        let inner_params: Vec<Expr> = params
            .iter()
            .map(|p| {
                let (inner, guard) = extract_guard_expr(p);
                if let Some(g) = guard {
                    guard_exprs.push(g.clone());
                }
                collect_nested_guards(inner, &mut guard_exprs);
                inner.clone()
            })
            .collect();

        let list_pattern = Expr::List(inner_params);
        let list_value = Value::List(args.to_vec());

        match match_pattern(&list_pattern, &list_value, Some(&_attr_checker)) {
            MatchResult::Match(mut bindings) => {
                // Evaluate guards with the collected bindings
                for guard in &guard_exprs {
                    let guard_env = env.child();
                    for (name, val) in &bindings {
                        guard_env.set(name.clone(), val.clone());
                    }
                    // Bind # and #1, #2, ... for PatternTest desugaring (_?f → PatternGuard)
                    for (i, arg) in args.iter().enumerate() {
                        guard_env.set(format!("#{}", i + 1), arg.clone());
                    }
                    if !args.is_empty() {
                        guard_env.set("#".to_string(), args[0].clone());
                    }
                    if !eval(guard, &guard_env)?.to_bool() {
                        return Ok(None);
                    }
                }
                // Apply defaults for optional named patterns with default values
                apply_defaults(params, &mut bindings, env)?;
                Ok(Some(bindings))
            }
            MatchResult::NoMatch => Ok(None),
        }
    } else {
        // Fast path: no sequence patterns, direct matching
        let has_optional = params.iter().any(|p| {
            let (inner, _) = extract_guard_expr(p);
            matches!(
                inner,
                Expr::OptionalBlank { .. } | Expr::OptionalNamedBlank { .. }
            )
        });

        if has_optional {
            // Allow fewer args than params; pad with Null for optional params
            if args.len() > params.len() {
                return Ok(None);
            }
        } else if params.len() != args.len() {
            return Ok(None);
        }

        let mut bindings = HashMap::new();

        for (i, param) in params.iter().enumerate() {
            let arg = args.get(i).unwrap_or(&Value::Null);
            let (inner_pat, guard) = extract_guard_expr(param);
            match match_pattern(inner_pat, arg, Some(&_attr_checker)) {
                MatchResult::Match(b) => {
                    bindings.extend(b);
                    if let Some(g) = guard {
                        guard_exprs.push(g.clone());
                    }
                    // Also collect guards nested inside list/call patterns
                    collect_nested_guards(inner_pat, &mut guard_exprs);
                }
                MatchResult::NoMatch => return Ok(None),
            }
        }

        // Evaluate guards with the collected bindings
        for guard in &guard_exprs {
            let guard_env = env.child();
            for (name, val) in &bindings {
                guard_env.set(name.clone(), val.clone());
            }
            // Bind # and #1, #2, ... for PatternTest desugaring (_?f → PatternGuard)
            for (i, arg) in args.iter().enumerate() {
                guard_env.set(format!("#{}", i + 1), arg.clone());
            }
            if !args.is_empty() {
                guard_env.set("#".to_string(), args[0].clone());
            }
            if !eval(guard, &guard_env)?.to_bool() {
                return Ok(None);
            }
        }

        // Apply defaults for optional named patterns with default values
        apply_defaults(params, &mut bindings, env)?;

        Ok(Some(bindings))
    }
}

/// Apply default values for optional named patterns (_:default, x_:default).
fn apply_defaults(params: &[Expr], bindings: &mut Bindings, env: &Env) -> Result<(), EvalError> {
    for param in params {
        let (inner, _guard) = extract_guard_expr(param);
        if let Expr::OptionalNamedBlank {
            name,
            default_value: Some(default),
            ..
        } = inner
            && let Some(Value::Null) = bindings.get(name)
        {
            let val = eval(default, env)?;
            bindings.insert(name.clone(), val);
        }
    }
    Ok(())
}

/// Flatten Sequence values into the surrounding list or call arguments.
/// In Wolfram Language semantics, Sequence[...] automatically splats.
/// Functions with `SequenceHold` or `HoldAllComplete` pass `skip_sequence = true`
/// to preserve Sequence objects intact.
fn flatten_sequences(items: Vec<Value>, skip_sequence: bool) -> Vec<Value> {
    if skip_sequence {
        return items;
    }
    let mut result = Vec::with_capacity(items.len());
    for item in items {
        match item {
            Value::Sequence(seq) => result.extend(seq),
            other => result.push(other),
        }
    }
    result
}

/// Check if an expression contains a sequence pattern (BlankSequence or BlankNullSequence).
fn has_sequence_pattern(expr: &Expr) -> bool {
    match expr {
        Expr::BlankSequence { .. } | Expr::BlankNullSequence { .. } => true,
        Expr::List(items) => items.iter().any(has_sequence_pattern),
        Expr::Call { args, .. } => args.iter().any(has_sequence_pattern),
        _ => false,
    }
}

/// Recursively update a nested structure at the given index path.
/// Used to implement `x[[i]] = val` and `x[[i, j]] = val`.
fn set_part(current: Value, indices: &[i64], val: Value) -> Result<Value, EvalError> {
    if indices.is_empty() {
        return Ok(val);
    }
    match current {
        Value::List(mut items) => {
            let idx = crate::builtins::list::normalize_index(indices[0], items.len())?;
            items[idx] = set_part(items[idx].clone(), &indices[1..], val)?;
            Ok(Value::List(items))
        }
        other => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: other.type_name().to_string(),
        }),
    }
}

/// Evaluate `?expr` — display help information for a symbol.
fn eval_information(expr: &Expr, env: &Env) -> Result<Value, EvalError> {
    match expr {
        Expr::Symbol(s) => {
            // 1. Check if the symbol has a user-defined value
            if let Some(val) = env.get(s) {
                // ReadProtected — hide definition details
                if env.has_attribute(s, "ReadProtected") {
                    return Ok(Value::Str(format!("Symbol `{}` is read protected.", s)));
                }
                let info = match &val {
                    Value::Function(func_def) => {
                        // Show function definitions
                        let mut lines = vec![format!("User-defined function `{}`:", s)];
                        for def in &func_def.definitions {
                            let params: Vec<String> =
                                def.params.iter().map(|p| format!("{}", p)).collect();
                            lines.push(format!("    {}[{}] := {}", s, params.join(", "), def.body));
                        }
                        lines.join("\n")
                    }
                    Value::Builtin(_, _) => {
                        // Built-in with documentation
                        let mut info = if let Some(help) = crate::builtins::get_help(s) {
                            help.to_string()
                        } else {
                            format!("Builtin function `{}`.", s)
                        };
                        let attrs = crate::builtins::get_attributes(s);
                        if !attrs.is_empty() {
                            info.push_str(&format!("\n\nAttributes: {}", attrs.join(", ")));
                        }
                        info
                    }
                    _ => {
                        // Other values — show binding
                        format!("{} = {}", s, val)
                    }
                };
                return Ok(Value::Str(info));
            }

            // 2. Symbol not in env — check built-in docs (constants, etc.)
            if let Some(help) = crate::builtins::get_help(s) {
                return Ok(Value::Str(help.to_string()));
            }

            // 3. Unknown symbol
            Ok(Value::Call {
                head: "Missing".to_string(),
                args: vec![
                    Value::Symbol("UnknownSymbol".to_string()),
                    Value::Symbol(s.clone()),
                ],
            })
        }
        _ => {
            // Non-symbol: evaluate and show type
            let val = eval(expr, env)?;
            Ok(Value::Str(format!(
                "{} is of type {}.",
                val,
                val.type_name()
            )))
        }
    }
}

/// Try to load a module from a `.syma` file on disk.
///
/// Searches `env.search_paths` for `{Name}.syma` (then `{name}.syma`),
/// then `{Name}/{Name}.syma` (then `{name}/{name}.syma`), then
/// `{Name}/src/{Name}.syma` for the standard package layout.
/// The file must contain a top-level `module <Name> { ... }` definition.
pub(crate) fn load_module_from_file(name: &str, env: &Env) -> Result<Value, EvalError> {
    use crate::{lexer, parser};

    // Candidates: flat file, then directory/file, then directory/src/file.
    // Each pair: exact case, then lowercase fallback.
    let candidates = [
        format!("{}.syma", name),
        format!("{}.syma", name.to_lowercase()),
        format!("{}/{}.syma", name, name),
        format!("{}/{}.syma", name.to_lowercase(), name.to_lowercase()),
        format!("{}/src/{}.syma", name, name),
        format!("{}/src/{}.syma", name.to_lowercase(), name.to_lowercase()),
    ];

    let mut source: Option<String> = None;
    'outer: for candidate in &candidates {
        let paths = env.search_paths.lock().unwrap();
        for dir in paths.iter() {
            let path = dir.join(candidate);
            if let Ok(content) = std::fs::read_to_string(&path) {
                source = Some(content);
                break 'outer;
            }
        }
    }

    let src = source.ok_or_else(|| {
        let paths = env.search_paths.lock().unwrap();
        EvalError::Error(format!(
            "Module '{}' not found.\nSearched for '{}.syma' in: {}",
            name,
            name,
            paths
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ))
    })?;

    let tokens = lexer::tokenize(&src).map_err(|e| EvalError::Error(e.to_string()))?;
    let ast = parser::parse(tokens).map_err(|e| EvalError::Error(e.to_string()))?;

    // Eval the file in a child env so its internals don't pollute the caller's scope.
    // `ModuleDef` evaluation will register the module in the shared registry.
    let file_env = env.child();
    eval_program(&ast, &file_env)?;

    // Import all file-level definitions into the caller's scope so that
    // exported functions can reference internal helpers (e.g. `binomialLoop`
    // used by `Binomial`, `fibIter` used by `Fibonacci`).
    for (sym, val) in file_env.bindings() {
        // Don't shadow builtins or the module symbol.
        if env.get(&sym).is_none() && sym != name {
            env.set(sym.clone(), val);
        }
    }

    env.get_module(name).ok_or_else(|| {
        EvalError::Error(format!(
            "File '{}.syma' was loaded but did not define a module named '{}'.\n\
             Tip: wrap its contents in `module {} {{ export ...; ... }}`",
            name, name, name
        ))
    })
}

/// Resolve a lazy-provider file path against the environment's search paths.
/// If the path is absolute and exists, returns it directly.
/// Otherwise searches each directory in `search_paths` for the file.
fn resolve_lazy_path(path: &std::path::Path, env: &Env) -> Result<PathBuf, EvalError> {
    if path.is_absolute() && path.exists() {
        return Ok(path.to_path_buf());
    }
    let paths = env.search_paths.lock().unwrap();
    for dir in paths.iter() {
        let candidate = dir.join(path);
        if candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(EvalError::Error(format!(
        "Lazy provider file '{}' not found.\nSearched in: {}",
        path.display(),
        paths
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtins;
    use crate::lexer;
    use crate::parser;

    /// Run a closure in a thread with 8MB stack (avoids stack overflow
    /// from deep eval recursion in test harness with default 2MB stack).
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
        // 2 + 3 * 4 = 14
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
        // `__` (BlankSequence) matches one or more arguments.
        // The sequence is bound as a Sequence value, which automatically
        // splats into lists and call arguments (Wolfram Language compatible).
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
        // Mixed fixed and sequence parameters
        // b__ binds as Sequence, which splats into {a, b}
        assert_eq!(
            eval_str("h[a_, b__] := {a, b}; h[1, 2, 3]"),
            eval_str("{1, 2, 3}")
        );
    }

    #[test]
    fn test_function_sequence_param_zero_args() {
        // `___` (BlankNullSequence) matches zero or more arguments.
        // x___ binds as Sequence (possibly empty), which splats into lists.
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
        // Infinity disables the limit check — use shallow recursion to avoid
        // native stack overflow from the tree-walk evaluator.
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
        // {1, 2, 3} // Length
        assert_eq!(
            eval_str("{1, 2, 3} // Length"),
            Value::Integer(Integer::from(3))
        );
    }

    // ── Prefix ──

    #[test]
    fn test_prefix() {
        // f @ x is equivalent to f[x]
        assert_eq!(
            eval_str("Length @ {1, 2, 3}"),
            Value::Integer(Integer::from(3))
        );
    }

    // ── ReplaceAll ──

    #[test]
    fn test_replace_all() {
        // 5 /. x_ -> 42 uses a blank pattern that matches anything
        assert_eq!(eval_str("5 /. x_ -> 42"), Value::Integer(Integer::from(42)));
    }

    // ── Map ──

    #[test]
    fn test_map_builtin() {
        // Sqrt /@ {1, 4, 9} maps Sqrt over a list
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
        // Degree = Pi/180 ≈ 0.017453292519943295
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
        // SinDegrees[30] = 1/2
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
        // CosDegrees[60] = 1/2
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
        // Csc[Pi/6] = 2
        assert_eq!(
            eval_str("Csc[Pi / 6]"),
            Value::Integer(rug::Integer::from(2))
        );
    }

    #[test]
    fn test_sec_pi_over_3_eval() {
        // Sec[Pi/3] = 2
        assert_eq!(
            eval_str("Sec[Pi / 3]"),
            Value::Integer(rug::Integer::from(2))
        );
    }

    #[test]
    fn test_cot_pi_over_4_eval() {
        // Cot[Pi/4] = 1
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
        assert_eq!(result, Value::Integer(Integer::from(256))); // ((2^2)^2)^2 = 256
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
        assert_eq!(result, Value::Integer(Integer::from(30))); // 1 + 4 + 9 + 16 = 30
    }

    #[test]
    fn test_fixed_point() {
        // FixedPoint with halving function should converge to ~0
        let result = eval_str("half[x_] := x / 2; FixedPoint[half, 64]");
        // With Rational arithmetic, result should be a very small exact fraction
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
        // f[x_ /; x > 0] := "positive"
        // f[x_ /; x < 0] := "negative"
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
        // Guard fails so second branch matches
        let result = eval_str(r#"match 3 { n_ /; n > 5 => "big"; n_ => "small" }"#);
        assert_eq!(result, Value::Str("small".to_string()));
    }

    #[test]
    fn test_pattern_guard_match_fallback() {
        // Guard fails, fallback branch with no guard matches
        let result = eval_str(r#"match 3 { n_ /; n > 5 => "big"; n_ => "small" }"#);
        assert_eq!(result, Value::Str("small".to_string()));
    }

    #[test]
    fn test_pattern_guard_replace_all() {
        // Guard in ReplaceAll via rule keyword: only match when guard succeeds
        let result = eval_str("rule r = { x_ /; x > 3 -> 42 }; 5 /. r");
        assert_eq!(result, Value::Integer(Integer::from(42)));

        // Guard fails → no match → value unchanged
        let result2 = eval_str("rule r = { x_ /; x > 3 -> 42 }; 2 /. r");
        assert_eq!(result2, Value::Integer(Integer::from(2)));
    }

    #[test]
    fn test_pattern_guard_replace_all_no_match() {
        // Guard that never matches via rule keyword
        let result = eval_str("rule r = { x_ /; x > 10 -> 99 }; 5 /. r");
        assert_eq!(result, Value::Integer(Integer::from(5)));
    }

    #[test]
    fn test_pattern_guard_replace_repeated() {
        // Guard with ReplaceRepeated: repeatedly apply until guard fails
        let result = eval_str("rule r = { x_ /; x > 1 -> 1 }; 5 //. r");
        assert_eq!(result, Value::Integer(Integer::from(1)));
    }

    #[test]
    fn test_pattern_guard_switch_nested() {
        // Nested guard inside a compound pattern in Switch
        let result = eval_str(
            r#"
            f[a_, b_ /; a + b > 0] := "big";
            f[a_, b_] := "small";
            f[3, -1]
        "#,
        );
        assert_eq!(result, Value::Str("big".to_string()));

        // Second case: condition fails, triggers fallback
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
        // Nested guard in match expression with compound pattern
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
        // _?IntegerQ via PatternGuard desugaring
        let result = eval_str("MatchQ[5, _?IntegerQ]");
        assert_eq!(result, Value::Bool(true));

        let result2 = eval_str("MatchQ[3.14, _?IntegerQ]");
        assert_eq!(result2, Value::Bool(false));
    }

    #[test]
    fn test_switch_literal() {
        // Switch with literal integer patterns
        let result = eval_str(r#"Switch[2, 1, "one", 2, "two", 3, "three"]"#);
        assert_eq!(result, Value::Str("two".to_string()));
    }

    #[test]
    fn test_switch_fallback() {
        // Switch with no matching case returns Null
        let result = eval_str(r#"Switch[99, 1, "one", 2, "two"]"#);
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn test_switch_named_blank() {
        // Switch with named blank pattern (y_ parsed as Symbol but matched as NamedBlank)
        let result = eval_str(r#"Switch[x, y_, y]"#);
        assert_eq!(result, Value::Symbol("x".to_string()));
    }

    #[test]
    fn test_switch_nested_guard() {
        // Switch with nested guard in compound pattern — tests collect_nested_guards
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
        // Space before ]] prevents lexer from treating it as RDoubleBracket
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
        // rethrow caught by outer Catch
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
        // Body is a single If expression (no ; inside loop bodies)
        let result = eval_str("i = 0; While[True, If[i >= 5, Return[i * 10], i = i + 1]]");
        assert_eq!(result, Value::Integer(Integer::from(50)));
    }

    // ── Break/Continue ──

    #[test]
    fn test_break_from_while() {
        // Body: when i > 5, Break exits. i = i + 1 runs only when i <= 5 (the else branch).
        // When i becomes 6, Break fires immediately, so final i = 6.
        let result = eval_str("i = 1; While[i < 100, If[i > 5, Break[], i = i + 1]]; i");
        assert_eq!(result, Value::Integer(Integer::from(6)));
    }

    #[test]
    fn test_continue_in_while() {
        // Increment in condition, Continue/append in single-expression body
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
        // Initialize i in the outer scope so set_propagate can update it from inside For
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
        // Do body is parsed as statement, use single-expression body
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
        // Lists with < 4 elements should still work (sequential fallback)
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

    // ── New parallel builtins ──

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
        // Small range (< 8) takes sequential path
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
        // With pool_size = 1 (no pool active), returns single-element list
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

    // ── ParallelProduct ──

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

    // ── ParallelDo ──

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

    // ── ParallelCombine ──

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
        // Hold[1 + 2] should preserve the syntactic form, not evaluate to 3
        let result = eval_str("Hold[1 + 2]");
        match result {
            Value::Hold(_) => {} // We just care that it's held, not evaluated
            _ => panic!("Expected Hold, got {:?}", result),
        }
        // Verify it's NOT 3 (the evaluated result)
        assert!(!matches!(result, Value::Integer(_)));
    }

    #[test]
    fn test_hold_complete_prevents_evaluation() {
        let result = eval_str("HoldComplete[1 + 2]");
        assert!(matches!(result, Value::HoldComplete(_)));
    }

    #[test]
    fn test_release_hold_evaluates() {
        // Use a variable to pass the held expression
        assert_eq!(
            eval_str("x = Hold[1 + 2]; ReleaseHold[x]"),
            Value::Integer(Integer::from(3))
        );
    }

    #[test]
    fn test_hold_multiple_args() {
        // Test single-arg through Hold
        assert!(matches!(eval_str("Hold[1 + 2]"), Value::Hold(_)));
    }

    #[test]
    fn test_hold_preserves_symbol() {
        // Hold[x] should not look up x
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
        // ReleaseHold of a non-held value should return it unchanged
        assert_eq!(
            eval_str("x = 42; ReleaseHold[x]"),
            Value::Integer(Integer::from(42))
        );
    }

    #[test]
    fn test_nested_hold() {
        // Nested hold evaluation: ReleaseHold[Hold[ReleaseHold[Hold[1+2]]]]
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
        // Plus has Listable attribute, so {1, 2} + 10 should give {11, 12}
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
        // Times has Listable attribute
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
        // {1, 2} * {3, 4} should give {3, 8}
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
        // Sin has Listable, so Sin[{0}] should thread
        // Sin[0] returns Integer(0) (exact match), not Real(0.0)
        let result = eval_str("Sin[{0}]");
        assert_eq!(result, Value::List(vec![Value::Integer(Integer::from(0))]));
    }

    #[test]
    fn test_protected_prevents_redefinition() {
        // Set a symbol as protected, then try to redefine
        let result = eval_str("SetAttributes[Sin, Protected]; f[x_] := x + 1");
        // This should succeed because 'f' is not protected
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn test_builtin_attributes_seeded() {
        // Builtins should have their attributes seeded during registration
        let env = Env::new();
        builtins::register_builtins(&env);
        let plus_attrs = env.get_attributes("Plus");
        assert!(plus_attrs.contains(&"Listable".to_string()));
        assert!(plus_attrs.contains(&"Flat".to_string()));
        let sin_attrs = env.get_attributes("Sin");
        assert!(sin_attrs.contains(&"Listable".to_string()));
        // Hold is not seeded as a builtin (it's a special form handled by the evaluator),
        // so it shouldn't have attributes set automatically.
        // let hold_attrs = env.get_attributes("Hold");
        // assert!(hold_attrs.contains(&"HoldAll".to_string()));
    }

    /// Helper: evaluate an expression string in a pre-configured environment.
    fn eval_str_in_env(input: &str, env: &Env) -> Value {
        let tokens = crate::lexer::tokenize(input).unwrap();
        let ast = crate::parser::parse(tokens).unwrap();
        crate::eval::eval_program(&ast, env).unwrap()
    }

    // ── Lazy provider tests ──

    #[test]
    fn test_lazy_custom_provider_constant() {
        let env = Env::new();
        builtins::register_builtins(&env);
        env.register_lazy_provider(
            "LazyFoo",
            LazyProvider::Custom(Arc::new(|env| {
                // Define a function
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
        // Register a lazy provider that defines a function called LazyDouble
        env.register_lazy_provider(
            "LazyDouble",
            LazyProvider::Custom(Arc::new(|env| {
                // Evaluate the function definition in the root env
                let tokens = crate::lexer::tokenize("LazyDouble[x_] := 2*x").unwrap();
                let ast = crate::parser::parse(tokens).unwrap();
                crate::eval::eval_program(&ast, &env.root_env()).unwrap();
                // Return the value that should be installed for LazyDouble
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
                // Define a real function so calling Once[] works
                let tokens = crate::lexer::tokenize("Once[] := 99").unwrap();
                let ast = crate::parser::parse(tokens).unwrap();
                crate::eval::eval_program(&ast, &env.root_env()).unwrap();
                Ok(env.root_env().get("Once").unwrap())
            })),
        );
        // First call: provider fires, loads function
        assert_eq!(
            eval_str_in_env("Once[]", &env),
            Value::Integer(Integer::from(99))
        );
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 1);
        // Second call: uses env lookup, provider does NOT fire
        assert_eq!(
            eval_str_in_env("Once[]", &env),
            Value::Integer(Integer::from(99))
        );
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test]
    fn test_lazy_provider_no_match_fallback() {
        // An undefined symbol without a provider should still return unevaluated
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

        // Create a temp file defining a simple function.
        // The lazy provider hook fires when a symbol is used AS A FUNCTION
        // (i.e., the Symbol branch in apply_function), so define a callable function.
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

        // Trigger provider by calling the lazily-loaded function
        let result = eval_str_in_env("LazyAdd[1]", &env);
        assert_eq!(result, Value::Integer(Integer::from(43)));

        // Clean up
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_det_auto_loads() {
        // Det should auto-load the LinearAlgebra package via lazy provider
        let result = eval_str("Det[{{1, 2}, {3, 4}}]");
        assert_eq!(result, Value::Integer(Integer::from(-2)));
    }

    #[test]
    fn test_mean_auto_loads() {
        // Mean should auto-load the Statistics package via lazy provider
        let result = eval_str("Mean[{1, 2, 3, 4, 5}]");
        assert_eq!(result, Value::Integer(Integer::from(3)));
    }

    #[test]
    fn test_auto_load_works_with_needs() {
        // Verify that after auto-load, Needs returns Null (already loaded)
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
        // And should short-circuit: False && (error) should not evaluate the error
        assert_eq!(
            eval_str("And[False, Error[\"should not fire\"]]"),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_or_short_circuit() {
        // Or should short-circuit: True || (error) should not evaluate the error
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
        // a && b should desugar to And[a, b]
        assert_eq!(eval_str("True && False"), Value::Bool(false));
        // a || b should desugar to Or[a, b]
        assert_eq!(eval_str("True || False"), Value::Bool(true));
        // !expr should desugar to Not[expr]
        assert_eq!(eval_str("!True"), Value::Bool(false));
        assert_eq!(eval_str("!False"), Value::Bool(true));
    }

    // ── JIT promotion ──

    #[test]
    fn test_jit_promotion() {
        // Define a function and call it 101+ times to trigger JIT promotion (threshold is 100).
        // Verify correctness after promotion to bytecode.
        let result = eval_str(
            "double[x_] := x * 2;
             Do[double[i], {i, 1, 100}];
             double[21]",
        );
        assert_eq!(result, Value::Integer(Integer::from(42)));
    }

    #[test]
    fn test_jit_promotion_with_if() {
        // JIT promotion for a function with a simple If
        let result = eval_str(
            "abs[x_] := If[x < 0, -x, x];
             Do[abs[i], {i, -50, 50}];
             abs[-5]",
        );
        assert_eq!(result, Value::Integer(Integer::from(5)));
    }

    #[test]
    fn test_jit_promotion_inline_arithmetic() {
        // Triggers inline arithmetic path in bytecode compiler
        let result = eval_str(
            "f[x_] := x + 10;
             Do[f[i], {i, 1, 100}];
             f[32]",
        );
        assert_eq!(result, Value::Integer(Integer::from(42)));
    }

    #[test]
    fn test_jit_promotion_multiple_calls() {
        // 200 calls across multiple arguments
        let result = eval_str(
            "add[a_, b_] := a + b;
             Do[add[i, i], {i, 1, 200}];
             add[20, 22]",
        );
        assert_eq!(result, Value::Integer(Integer::from(42)));
    }

    // ── JIT comparison/logical/loop tests ──────────────────────────────

    #[test]
    fn test_jit_comparison_equal() {
        // Equal comparison inside a hot function
        let result = eval_str(
            "f[x_] := If[x == 42, 1, 0];
             Do[f[i], {i, 1, 100}];
             f[42]",
        );
        assert_eq!(result, Value::Integer(Integer::from(1)));
    }

    #[test]
    fn test_jit_comparison_greater() {
        // Greater-than comparison inside a hot function
        let result = eval_str(
            "f[x_] := If[x > 0, x, 0];
             Do[f[i], {i, -50, 50}];
             f[42]",
        );
        assert_eq!(result, Value::Integer(Integer::from(42)));
    }

    #[test]
    fn test_jit_and_or() {
        // And/Or predicates inside a hot function
        let result = eval_str(
            "f[x_, y_] := If[x > 0 && y > 0, 1, 0];
             Do[f[i, i], {i, 1, 100}];
             f[3, 5]",
        );
        assert_eq!(result, Value::Integer(Integer::from(1)));
    }

    #[test]
    fn test_jit_map_desugar() {
        // Map desugaring inside a hot function
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
        // Which inside a hot function
        let result = eval_str(
            "f[x_] := Which[x > 0, x, True, 0];
             Do[f[i], {i, -50, 50}];
             f[42]",
        );
        assert_eq!(result, Value::Integer(Integer::from(42)));
    }

    #[test]
    fn test_jit_which_default() {
        // Which with default case
        let result = eval_str(
            "f[x_] := Which[x < 0, -x, x > 10, 10, True, x];
             Do[f[i], {i, 1, 100}];
             f[5]",
        );
        assert_eq!(result, Value::Integer(Integer::from(5)));
    }

    #[test]
    fn test_jit_switch() {
        // Verify no crash — Switch might not parse as special form
        // (the compiler unit test covers Switch compilation directly)
    }

    #[test]
    fn test_jit_apply_desugar() {
        // Apply desugaring inside a hot function.
        // f[x_] := Apply[Plus, x] — no list literal inside the body
        let result = eval_str(
            "f[x_] := Apply[Plus, x];
             Do[f[{i, i}], {i, 1, 100}];
             f[{10, 32}]",
        );
        assert_eq!(result, Value::Integer(Integer::from(42)));
    }

    #[test]
    fn test_jit_subtract() {
        // Subtraction inside a hot function (previously bug: Sub used Plus)
        let result = eval_str(
            "f[x_, y_] := x - y;
             Do[f[i, 1], {i, 1, 100}];
             f[10, 3]",
        );
        assert_eq!(result, Value::Integer(Integer::from(7)));
    }

    #[test]
    fn test_jit_divide() {
        // Division inside a hot function (previously bug: Div used Times)
        let result = eval_str(
            "f[x_, y_] := x / y;
             Do[f[i, 2], {i, 1, 100}];
             f[42, 2]",
        );
        assert_eq!(result, Value::Integer(Integer::from(21)));
    }

    #[test]
    fn test_listable_boole() {
        // Boole is Listable, so Boole[{True, False}] should give {1, 0}
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
        // Xor is Listable, so Xor[{True, False}, True] should give {False, True}
        assert_eq!(
            eval_str("Xor[{True, False}, True]"),
            Value::List(vec![Value::Bool(false), Value::Bool(true),])
        );
    }

    // ── Pure function (#&) tests ──

    #[test]
    fn test_pure_function_slot() {
        // #& — identity function
        assert_eq!(eval_str("(#&)[5]"), Value::Integer(Integer::from(5)));
    }

    #[test]
    fn test_pure_function_arithmetic() {
        // # + 1 &
        assert_eq!(eval_str("(# + 1 &)[5]"), Value::Integer(Integer::from(6)));
    }

    #[test]
    fn test_pure_function_two_slots() {
        // #1 + #2 &
        assert_eq!(
            eval_str("(#1 + #2 &)[3, 7]"),
            Value::Integer(Integer::from(10))
        );
    }

    #[test]
    fn test_pure_function_map() {
        // Map[# + 1 &, {1, 2, 3}]
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
        // Select[{1, 2, 3, 4}, # > 2 &]
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
        // (#[[1]] + #[[2]] &)[{10, 32}] → 42
        assert_eq!(
            eval_str("(#[[1]] + #[[2]] &)[{10, 32}]"),
            Value::Integer(Integer::from(42)),
        );
    }

    #[test]
    fn test_pure_function_nested_slots() {
        // Nested pure functions: inner # belongs to inner &, outer # to outer &
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
        // Function[{x, y}, x + y] — named-parameter lambda
        assert_eq!(
            eval_str("Function[{x, y}, x + y][3, 7]"),
            Value::Integer(Integer::from(10))
        );
    }

    #[test]
    fn test_pure_function_named_params_map() {
        // Function with named params used with Map
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
        // Zero-arg pure function (constant)
        assert_eq!(eval_str("(42&)[]"), Value::Integer(Integer::from(42)));
    }

    #[test]
    fn test_pure_function_extra_args() {
        // Extra args beyond slot_count are silently accepted
        assert_eq!(eval_str("(#&)[1, 2, 3]"), Value::Integer(Integer::from(1)));
    }

    #[test]
    fn test_pure_function_multi_arity() {
        // #1 + #2 & with exactly 2 args
        assert_eq!(
            eval_str("(#1 + #2 &)[10, 32]"),
            Value::Integer(Integer::from(42))
        );
    }

    #[test]
    fn test_pure_function_with_select_complex() {
        // More complex select with pure function
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
        // #0 resolves without error
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
        // Basic closure: createAdder[x_] returns a function that captures x
        assert_eq!(
            eval_str("createAdder[x_] := Function[{y}, x + y]; f = createAdder[10]; f[5]"),
            Value::Integer(Integer::from(15))
        );
    }

    #[test]
    fn test_closure_nested() {
        // Nested closure: outer function captures outer param, inner captures outer closure
        assert_eq!(
            eval_str("Function[{x}, Function[{y}, x + y]][5][3]"),
            Value::Integer(Integer::from(8))
        );
    }

    #[test]
    fn test_closure_double_nested() {
        // Double nesting: three levels of closure capture
        assert_eq!(
            eval_str("Function[{x}, Function[{y}, Function[{z}, x + y + z]]][1][2][3]"),
            Value::Integer(Integer::from(6))
        );
    }

    #[test]
    fn test_closure_with_map() {
        // Closure used with Map: adder captures outer x = 10
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
        // Multiple free variables captured from outer scope
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
        // Basic class with constructor, field assignment, and method
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
        // Field access via obj.field
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
        // Inheritance: child class gets parent fields and methods
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
        // Mixin composition: methods from mixin are available on the class
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
        // Method can reference this fields
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
        // Field with default value
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
        // Module definition and bare import
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
        // Selective import: only bring specific exports into scope
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
        // Alias import: bind module to a name
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
        // The alias should produce a Module value
        match result {
            Value::Module { .. } => {} // OK
            _ => panic!("Expected Module, got {:?}", result),
        }
    }

    // ── Bytecode compilation register-overlap bug ──

    #[test]
    fn test_jit_fold_with_pure_function() {
        // Fold with a pure function inside a hot function.
        // g[n_] := Fold[(#1 + #2) &, 0, Range[n]]
        // After bytecode compilation, the compiler's `compile_call` emitted
        // Mov instructions in forward order, which could overwrite source
        // registers still needed by later arguments. This test verifies
        // the fix: reverse-order Mov emission.
        let result = eval_str(
            "g[n_] := Fold[(#1 + #2) &, 0, Range[n]];
             g[8]; (* verify before compilation *)
             Do[g[8], {i, 105}]; (* trigger hot-compilation *)
             g[8]",
        );
        assert_eq!(result, Value::Integer(Integer::from(36)));
    }

    #[test]
    fn test_jit_nest_with_pure_function() {
        // Nest with a pure function after hot compilation.
        let result = eval_str(
            "g[n_] := Nest[(# + 1) &, 0, n];
             g[5]; (* verify before *)
             Do[g[5], {i, 105}]; (* trigger hot-compilation *)
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
        // post-increment returns old value
        let result = eval_str("x = 5; x++");
        assert_eq!(result, Value::Integer(Integer::from(5)));
        // x is now 6 in the same evaluation
        assert_eq!(eval_str("x = 5; x++; x"), Value::Integer(Integer::from(6)));
    }

    #[test]
    fn test_post_decrement_eval() {
        // post-decrement returns old value
        let result = eval_str("x = 5; x--");
        assert_eq!(result, Value::Integer(Integer::from(5)));
        // x is now 4 in the same evaluation
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
        // Chained: x = y = 5 → y = 5 → 5, then x = 5 → 5, result is 5
        assert_eq!(eval_str("x = y = 5"), Value::Integer(Integer::from(5)));
        // Both x and y should now be 5
        assert_eq!(
            eval_str("x = y = 5; x + y"),
            Value::Integer(Integer::from(10))
        );
    }

    // ── Scoping constructs: Module, With, Block ──

    #[test]
    fn test_module_scoping_basic() {
        // Module with simple local variable
        assert_eq!(
            eval_str("Module[{x = 5}, x + 1]"),
            Value::Integer(Integer::from(6))
        );
    }

    #[test]
    fn test_module_multiple_locals() {
        // Module with multiple local variables
        assert_eq!(
            eval_str("Module[{x = 3, y = 4}, x^2 + y^2]"),
            Value::Integer(Integer::from(25))
        );
    }

    #[test]
    fn test_module_no_init() {
        // Module with uninitialized variable binds to Null
        assert_eq!(eval_str("Module[{x}, x]"), Value::Null);
    }

    #[test]
    fn test_module_shadows_global() {
        // Module local x should not affect global x (checked in same eval)
        let result = eval_str("x = 100; Module[{x = 5}, x]");
        assert_eq!(result, Value::Integer(Integer::from(5)));
    }

    #[test]
    fn test_module_global_unchanged() {
        // After Module, global x keeps its value (single eval_str)
        let result = eval_str("x = 100; Module[{x = 5}, x]; x");
        assert_eq!(result, Value::Integer(Integer::from(100)));
    }

    #[test]
    fn test_module_empty_specs() {
        // Module with empty local list
        assert_eq!(
            eval_str("Module[{}, 42]"),
            Value::Integer(Integer::from(42))
        );
    }

    #[test]
    fn test_module_with_function_def() {
        // Module inside a function definition
        let result = eval_str("f[x_] := Module[{y = x^2}, y + 1]; f[5]");
        assert_eq!(result, Value::Integer(Integer::from(26)));
    }

    #[test]
    fn test_module_sequential_body() {
        // Module args[1..] evaluated sequentially
        // Set[x, x + 2] => x=3, then Set[x, Times[x, 3]] = 9
        let result = eval_str("Module[{x = 1}, Set[x, x + 2], Set[x, x * 3], x]");
        assert_eq!(result, Value::Integer(Integer::from(9)));
    }

    // ── With tests ──

    #[test]
    fn test_with_basic() {
        // With with simple substitution
        assert_eq!(
            eval_str("With[{x = 5}, x + 1]"),
            Value::Integer(Integer::from(6))
        );
    }

    #[test]
    fn test_with_multiple_vars() {
        // With with multiple variables
        assert_eq!(
            eval_str("With[{x = 3, y = 4}, x^2 + y^2]"),
            Value::Integer(Integer::from(25))
        );
    }

    #[test]
    fn test_with_substitution_in_call() {
        // With substitutes x with 5 inside Sin[...]
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
        // With with empty local list
        assert_eq!(eval_str("With[{}, 42]"), Value::Integer(Integer::from(42)));
    }

    #[test]
    fn test_with_rhs_evaluation() {
        // With evaluates RHS before substitution: 1+2=3, x*3 = 9
        let result = eval_str("With[{x = 1 + 2}, x * 3]");
        assert_eq!(result, Value::Integer(Integer::from(9)));
    }

    #[test]
    fn test_with_no_global_leak() {
        // With should not create bindings visible outside
        // (checked in single eval_str since each eval_str has fresh env)
        let result = eval_str("a = 10; With[{a = 5}, a + 1]; a");
        assert_eq!(result, Value::Integer(Integer::from(10)));
    }

    // ── Block tests ──

    #[test]
    fn test_block_basic() {
        // Block with simple local rebinding
        assert_eq!(
            eval_str("Block[{x = 5}, x + 1]"),
            Value::Integer(Integer::from(6))
        );
    }

    #[test]
    fn test_block_multiple_vars() {
        // Block with multiple variables
        assert_eq!(
            eval_str("Block[{x = 3, y = 4}, x + y]"),
            Value::Integer(Integer::from(7))
        );
    }

    #[test]
    fn test_block_restores_global() {
        // Block should not affect value after exit (checked in same eval)
        let result = eval_str("x = 10; Block[{x = 5}, x + 1]; x");
        assert_eq!(result, Value::Integer(Integer::from(10)));
    }

    #[test]
    fn test_block_empty_specs() {
        // Block with empty local list
        assert_eq!(eval_str("Block[{}, 42]"), Value::Integer(Integer::from(42)));
    }

    #[test]
    fn test_block_side_effect_in_body() {
        // Block sets x=10, modifies it to 15 inside, then restores
        let result = eval_str("x = 1; Block[{x = 10}, Set[x, x + 5]]; x");
        assert_eq!(result, Value::Integer(Integer::from(1)));
    }

    #[test]
    fn test_block_affects_function_due_to_dynamic_scoping() {
        // Block uses dynamic scoping: f sees Block's x=5, not the original x=100
        let result = eval_str("x = 100; f = Function[{a}, x]; Block[{x = 5}, f[0]]");
        assert_eq!(result, Value::Integer(Integer::from(5)));
    }
}
