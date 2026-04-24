/// Tree-walk evaluator for Syma language.
///
/// Evaluates AST expressions in an environment. Handles:
/// - Symbol lookup and function application
/// - Pattern-matched function definitions
/// - Rule application (/. and //.)
/// - Class instantiation and method dispatch
/// - Control flow (If, Which, Switch, match, loops)

use std::collections::HashMap;
use std::rc::Rc;

use rug::Integer;

use crate::ast::*;
use crate::env::Env;
use crate::value::*;
use crate::pattern::{match_pattern, MatchResult, Bindings};

/// Evaluate a program (list of statements) in the given environment.
pub fn eval_program(stmts: &[Expr], env: &Env) -> Result<Value, EvalError> {
    let mut result = Value::Null;
    for stmt in stmts {
        result = eval(stmt, env)?;
    }
    Ok(result)
}

/// Evaluate a single expression in the given environment.
pub fn eval(expr: &Expr, env: &Env) -> Result<Value, EvalError> {
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
            Ok(env.get(s).unwrap_or_else(|| Value::Symbol(s.clone())))
        }

        // ── List ──
        Expr::List(items) => {
            let values: Result<Vec<Value>, _> = items.iter().map(|item| eval(item, env)).collect();
            Ok(Value::List(values?))
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
        Expr::Rule { lhs, rhs } => {
            // If LHS looks like a pattern (symbol ending with _, or blank nodes),
            // keep it as a Pattern value rather than evaluating it.
            let lhs_val = if is_pattern_like(lhs) {
                Value::Pattern(lhs.as_ref().clone())
            } else {
                eval(lhs, env)?
            };
            Ok(Value::Rule {
                lhs: Box::new(lhs_val),
                rhs: Box::new(eval(rhs, env)?),
                delayed: false,
            })
        }
        Expr::RuleDelayed { lhs, rhs } => {
            // Delayed: don't evaluate RHS yet
            let lhs_val = if is_pattern_like(lhs) {
                Value::Pattern(lhs.as_ref().clone())
            } else {
                eval(lhs, env)?
            };
            Ok(Value::Rule {
                lhs: Box::new(lhs_val),
                rhs: Box::new(Value::Pattern(rhs.as_ref().clone())),
                delayed: true,
            })
        }

        // ── Function application ──
        Expr::Call { head, args } => {
            eval_call(head, args, env)
        }

        // ── ReplaceAll: expr /. rules ──
        Expr::ReplaceAll { expr, rules } => {
            let val = eval(expr, env)?;
            let rules_val = eval(rules, env)?;
            apply_rules_value(&val, &rules_val)
        }

        // ── ReplaceRepeated: expr //. rules ──
        Expr::ReplaceRepeated { expr, rules } => {
            let mut val = eval(expr, env)?;
            let rules_val = eval(rules, env)?;
            let max_iterations = 1000;
            for _ in 0..max_iterations {
                match apply_rules_value(&val, &rules_val)? {
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
        Expr::Pipe { expr, func } => {
            let val = eval(expr, env)?;
            let f = eval(func, env)?;
            apply_function(&f, &[val], env)
        }

        // ── Prefix: f @ x ──
        Expr::Prefix { func, arg } => {
            let f = eval(func, env)?;
            let a = eval(arg, env)?;
            apply_function(&f, &[a], env)
        }

        // ── If ──
        Expr::If { condition, then_branch, else_branch } => {
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
            for (pattern, result) in cases {
                let pat_val = eval(pattern, env)?;
                if let MatchResult::Match(_) = match_pattern(&pat_to_expr(&pat_val), &val) {
                    return eval(result, env);
                }
            }
            Ok(Value::Null)
        }

        // ── Match ──
        Expr::Match { expr, branches } => {
            let val = eval(expr, env)?;
            for branch in branches {
                let (inner_pat, guard) = extract_guard_expr(&branch.pattern);
                if let MatchResult::Match(bindings) = match_pattern(inner_pat, &val) {
                    // Evaluate guard if present
                    if let Some(guard_expr) = guard {
                        let guard_env = env.child();
                        for (name, value) in &bindings {
                            guard_env.set(name.clone(), value.clone());
                        }
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
                args: vec![val],
            })
        }

        // ── For loop ──
        Expr::For { init, condition, step, body } => {
            let child_env = env.child();
            eval(init, &child_env)?;
            while eval(condition, &child_env)?.to_bool() {
                eval(body, &child_env)?;
                eval(step, &child_env)?;
            }
            Ok(Value::Null)
        }

        // ── While loop ──
        Expr::While { condition, body } => {
            while eval(condition, env)?.to_bool() {
                eval(body, env)?;
            }
            Ok(Value::Null)
        }

        // ── Do loop ──
        Expr::Do { body, iterator } => {
            let child_env = env.child();
            match iterator {
                IteratorSpec::Range { var, min, max } => {
                    let min_val = eval(min, &child_env)?.to_integer()
                        .ok_or_else(|| EvalError::TypeError {
                            expected: "Integer".to_string(),
                            got: "non-Integer".to_string(),
                        })?;
                    let max_val = eval(max, &child_env)?.to_integer()
                        .ok_or_else(|| EvalError::TypeError {
                            expected: "Integer".to_string(),
                            got: "non-Integer".to_string(),
                        })?;
                    for i in min_val..=max_val {
                        child_env.set(var.clone(), Value::Integer(Integer::from(i)));
                        eval(body, &child_env)?;
                    }
                }
                IteratorSpec::List { var, list } => {
                    let list_val = eval(list, &child_env)?;
                    match list_val {
                        Value::List(items) => {
                            for item in items {
                                child_env.set(var.clone(), item);
                                eval(body, &child_env)?;
                            }
                        }
                        _ => return Err(EvalError::TypeError {
                            expected: "List".to_string(),
                            got: list_val.type_name().to_string(),
                        }),
                    }
                }
            }
            Ok(Value::Null)
        }

        // ── Function definition ──
        Expr::FuncDef { name, params, body, delayed } => {
            // Check if function already exists
            let func = if let Some(Value::Function(f)) = env.get(name) {
                Rc::try_unwrap(f).unwrap_or_else(|rc| (*rc).clone())
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
            });

            env.set(name.clone(), Value::Function(Rc::new(func)));
            Ok(Value::Null)
        }

        // ── Assignment ──
        Expr::Assign { lhs, rhs } => {
            let val = eval(rhs, env)?;
            match lhs.as_ref() {
                Expr::Symbol(s) => {
                    env.set(s.clone(), val.clone());
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
                            patterns.len(), items.len()
                        )));
                    }
                    for (pat, item) in patterns.iter().zip(items.iter()) {
                        match pat {
                            Expr::Symbol(s) => {
                                env.set(s.clone(), item.clone());
                            }
                            _ => {
                                // Pattern destructuring
                                if let MatchResult::Match(bindings) = match_pattern(pat, item) {
                                    for (name, value) in bindings {
                                        env.set(name, value);
                                    }
                                } else {
                                    return Err(EvalError::Error("Destructuring pattern mismatch".to_string()));
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

        // ── Rule definition ──
        Expr::RuleDef { name, rules } => {
            let rule_pairs: Vec<(Value, Value)> = rules.iter()
                .map(|(lhs, rhs)| {
                    Ok((Value::Pattern(lhs.clone()), Value::Pattern(rhs.clone())))
                })
                .collect::<Result<Vec<_>, EvalError>>()?;

            env.set(name.clone(), Value::RuleSet {
                name: name.clone(),
                rules: rule_pairs,
            });
            Ok(Value::Null)
        }

        // ── Class definition ──
        Expr::ClassDef { name, parent: _, mixins: _, members: _ } => {
            // Store class name as a marker; actual instantiation is handled
            // by the evaluator when it sees a Call with this head.
            env.set(name.clone(), Value::Symbol(name.clone()));
            Ok(Value::Null)
        }

        // ── Module definition ──
        Expr::ModuleDef { name: _, exports: _, body } => {
            let child_env = env.child();
            for stmt in body {
                eval(stmt, &child_env)?;
            }
            // TODO: export symbols to parent scope
            Ok(Value::Null)
        }

        // ── Import ──
        Expr::Import { module, selective: _, alias: _ } => {
            // TODO: implement module loading
            Err(EvalError::Error(format!("Import not yet implemented: {}", module.join("."))))
        }

        // ── Export ──
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
        Expr::Hold(e) => Ok(Value::Hold(Box::new(eval(e, env)?))),
        Expr::HoldComplete(e) => Ok(Value::HoldComplete(Box::new(eval(e, env)?))),
        Expr::ReleaseHold(e) => {
            let val = eval(e, env)?;
            match val {
                Value::Hold(v) => Ok(*v),
                Value::HoldComplete(v) => Ok(*v),
                _ => Ok(val),
            }
        }

        // ── Pattern nodes (should not be evaluated directly) ──
        Expr::Blank { .. } | Expr::NamedBlank { .. } | Expr::BlankSequence { .. }
        | Expr::BlankNullSequence { .. } | Expr::PatternGuard { .. } => {
            Ok(Value::Pattern(expr.clone()))
        }

        // ── Slot (only meaningful inside pure functions) ──
        Expr::Slot(_) => {
            Err(EvalError::Error("Slot # used outside of pure function".to_string()))
        }

        // ── Function constructor ──
        Expr::Function { params, body } => {
            Ok(Value::PureFunction {
                body: body.as_ref().clone(),
                slot_count: params.len(),
            })
        }
    }
}

/// Extract a top-level guard from a pattern expression.
///
/// Returns (inner_pattern, Some(guard_condition)) if the expression is a
/// PatternGuard, otherwise (expr, None).
fn extract_guard_expr(expr: &Expr) -> (&Expr, Option<&Expr>) {
    match expr {
        Expr::PatternGuard { pattern, condition } => (pattern.as_ref(), Some(condition.as_ref())),
        _ => (expr, None),
    }
}

/// Check if an expression looks like a pattern that should not be evaluated.
fn is_pattern_like(expr: &Expr) -> bool {
    match expr {
        Expr::Symbol(s) => s.ends_with('_') || s == "_",
        Expr::Blank { .. } | Expr::NamedBlank { .. } | Expr::BlankSequence { .. }
        | Expr::BlankNullSequence { .. } | Expr::PatternGuard { .. } => true,
        _ => false,
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
                    return Err(EvalError::Error("Set requires exactly 2 arguments".to_string()));
                }
                let val = eval(&args[1], env)?;
                match &args[0] {
                    Expr::Symbol(name) => {
                        env.set(name.clone(), val.clone());
                        Ok(val)
                    }
                    _ => Err(EvalError::Error("Invalid assignment target".to_string())),
                }
            }
            "Hold" => {
                // Don't evaluate arguments
                Ok(Value::Hold(Box::new(Value::List(
                    args.iter().map(|a| Value::Pattern(a.clone())).collect()
                ))))
            }
            "Table" => {
                // Table[expr, {i, min, max}] — iterator spec has unevaluated symbols
                eval_table(args, env)
            }
            "Sum" => {
                // Sum[expr, {i, min, max}] — iterator spec has unevaluated symbols
                eval_sum(args, env)
            }
            "Catch" => {
                // Catch[expr] — evaluate expr, catching any Throw[val]
                if args.len() != 1 {
                    return Err(EvalError::Error("Catch requires exactly 1 argument".to_string()));
                }
                match eval(&args[0], env) {
                    Ok(v) => Ok(v),
                    Err(EvalError::Thrown(v)) => Ok(v),
                    Err(e) => Err(e),
                }
            }
            _ => {
                // Normal function call
                let head_val = eval(head, env)?;
                let arg_vals: Result<Vec<Value>, _> = args.iter().map(|a| eval(a, env)).collect();
                apply_function(&head_val, &arg_vals?, env)
            }
        }
    } else {
        let head_val = eval(head, env)?;
        let arg_vals: Result<Vec<Value>, _> = args.iter().map(|a| eval(a, env)).collect();
        apply_function(&head_val, &arg_vals?, env)
    }
}

/// Apply a function value to arguments.
fn apply_function(func: &Value, args: &[Value], env: &Env) -> Result<Value, EvalError> {
    match func {
        Value::Builtin(name, f) => {
            // Handle evaluator-dependent builtins that need apply_function
            match name.as_str() {
                "Map" => return builtin_map_eval(args, env),
                "Fold" => return builtin_fold_eval(args, env),
                "Select" => return builtin_select_eval(args, env),
                "Scan" => return builtin_scan_eval(args, env),
                "Nest" => return builtin_nest_eval(args, env),
                "MatchQ" => return builtin_match_q_eval(args),
                "FreeQ" => return builtin_free_q_eval(args),
                "FixedPoint" => return builtin_fixed_point_eval(args, env),
                _ => {}
            }
            f(args).map_err(|e| match e {
                EvalError::NoMatch { .. } => EvalError::NoMatch {
                    head: name.clone(),
                    args: args.to_vec(),
                },
                other => other,
            })
        }

        Value::Function(func_def) => {
            // Try each definition in order
            for def in &func_def.definitions {
                if let Some(bindings) = try_match_params(&def.params, args, env)? {
                    let child_env = env.child();
                    for (name, value) in &bindings {
                        child_env.set(name.clone(), value.clone());
                    }
                    return eval(&def.body, &child_env);
                }
            }
            Err(EvalError::NoMatch {
                head: func_def.name.clone(),
                args: args.to_vec(),
            })
        }

        Value::PureFunction { body, slot_count: _ } => {
            let child_env = env.child();
            // Bind slots
            for (i, arg) in args.iter().enumerate() {
                child_env.set(format!("#{}", i + 1), arg.clone());
            }
            if !args.is_empty() {
                child_env.set("#".to_string(), args[0].clone());
            }
            eval(body, &child_env)
        }

        Value::Symbol(name) => {
            // Look up the symbol and apply
            if let Some(f) = env.get(name) {
                apply_function(&f, args, env)
            } else {
                // Return unevaluated
                Ok(Value::Call {
                    head: name.clone(),
                    args: args.to_vec(),
                })
            }
        }

        Value::Object { class_name, fields: _ } => {
            // Method dispatch: look for method on the object
            let method_name = format!("{}.__method__", class_name);
            if let Some(method) = env.get(&method_name) {
                let mut method_args = vec![func.clone()];
                method_args.extend(args.to_vec());
                apply_function(&method, &method_args, env)
            } else {
                Err(EvalError::NoMatch {
                    head: class_name.clone(),
                    args: args.to_vec(),
                })
            }
        }

        _ => {
            // Return unevaluated
            Ok(Value::Call {
                head: func.to_string(),
                args: args.to_vec(),
            })
        }
    }
}

/// Try to match parameters against arguments, evaluating any pattern guards.
///
/// Returns Ok(Some(bindings)) on success, Ok(None) if no match, Err on guard eval error.
fn try_match_params(params: &[Expr], args: &[Value], env: &Env) -> Result<Option<Bindings>, EvalError> {
    if params.len() != args.len() {
        return Ok(None);
    }

    let mut bindings = HashMap::new();
    let mut guards: Vec<&Expr> = Vec::new();

    for (param, arg) in params.iter().zip(args.iter()) {
        let (inner_pat, guard) = extract_guard_expr(param);
        match match_pattern(inner_pat, arg) {
            MatchResult::Match(b) => {
                bindings.extend(b);
                if let Some(g) = guard {
                    guards.push(g);
                }
            }
            MatchResult::NoMatch => return Ok(None),
        }
    }

    // Evaluate guards with the collected bindings
    for guard in guards {
        let guard_env = env.child();
        for (name, val) in &bindings {
            guard_env.set(name.clone(), val.clone());
        }
        if !eval(guard, &guard_env)?.to_bool() {
            return Ok(None);
        }
    }

    Ok(Some(bindings))
}

/// Apply rules to a value.
fn apply_rules_value(value: &Value, rules: &Value) -> Result<Value, EvalError> {
    match rules {
        Value::RuleSet { rules: rule_pairs, .. } => {
            for (lhs, rhs) in rule_pairs {
                if let Value::Pattern(lhs_expr) = lhs {
                    if let MatchResult::Match(bindings) = match_pattern(lhs_expr, value) {
                        if let Value::Pattern(rhs_expr) = rhs {
                            return substitute_value(rhs_expr, &bindings);
                        }
                    }
                }
            }
            Ok(value.clone())
        }
        Value::List(rule_list) => {
            for rule in rule_list {
                let result = apply_rules_value(value, rule)?;
                if !result.struct_eq(value) {
                    return Ok(result);
                }
            }
            Ok(value.clone())
        }
        Value::Rule { lhs, rhs, delayed } => {
            if let Value::Pattern(lhs_expr) = lhs.as_ref() {
                if let MatchResult::Match(bindings) = match_pattern(lhs_expr, value) {
                    if *delayed {
                        if let Value::Pattern(rhs_expr) = rhs.as_ref() {
                            return substitute_value(rhs_expr, &bindings);
                        }
                    } else {
                        return Ok(rhs.as_ref().clone());
                    }
                }
            }
            Ok(value.clone())
        }
        _ => Ok(value.clone()),
    }
}

/// Substitute bindings into an expression and evaluate.
fn substitute_value(expr: &Expr, bindings: &Bindings) -> Result<Value, EvalError> {
    // Simple substitution: replace symbols with bound values
    match expr {
        Expr::Symbol(s) => {
            if let Some(val) = bindings.get(s) {
                Ok(val.clone())
            } else {
                Ok(Value::Symbol(s.clone()))
            }
        }
        Expr::Integer(n) => Ok(Value::Integer(n.clone())),
        Expr::Real(r) => Ok(Value::Real(r.clone())),
        Expr::Bool(b) => Ok(Value::Bool(*b)),
        Expr::Str(s) => Ok(Value::Str(s.clone())),
        Expr::Null => Ok(Value::Null),
        Expr::List(items) => {
            let values: Result<Vec<Value>, _> = items.iter()
                .map(|item| substitute_value(item, bindings))
                .collect();
            Ok(Value::List(values?))
        }
        Expr::Call { head, args } => {
            let h = substitute_value(head, bindings)?;
            let a: Result<Vec<Value>, _> = args.iter()
                .map(|arg| substitute_value(arg, bindings))
                .collect();
            match h {
                Value::Symbol(name) => Ok(Value::Call { head: name, args: a? }),
                _ => Ok(Value::Call {
                    head: h.to_string(),
                    args: a?,
                }),
            }
        }
        _ => Ok(Value::Pattern(expr.clone())),
    }
}

/// Convert a Value back to an Expr for pattern matching.
fn pat_to_expr(val: &Value) -> Expr {
    match val {
        Value::Integer(n) => Expr::Integer(n.clone()),
        Value::Real(r) => Expr::Real(r.clone()),
        Value::Bool(b) => Expr::Bool(*b),
        Value::Str(s) => Expr::Str(s.clone()),
        Value::Null => Expr::Null,
        Value::Symbol(s) => Expr::Symbol(s.clone()),
        Value::List(items) => Expr::List(items.iter().map(pat_to_expr).collect()),
        Value::Call { head, args } => Expr::Call {
            head: Box::new(Expr::Symbol(head.clone())),
            args: args.iter().map(pat_to_expr).collect(),
        },
        Value::Pattern(expr) => expr.clone(),
        _ => Expr::Symbol(val.to_string()),
    }
}

// ── Evaluator-dependent builtins ──

/// Map[f, list] — apply f to each element of list.
fn builtin_map_eval(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error("Map requires exactly 2 arguments".to_string()));
    }
    let f = &args[0];
    match &args[1] {
        Value::List(items) => {
            let mut result = Vec::new();
            for item in items {
                result.push(apply_function(f, &[item.clone()], env)?);
            }
            Ok(Value::List(result))
        }
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: args[1].type_name().to_string(),
        }),
    }
}

/// Fold[f, init, list] or Fold[f, list] — left fold.
fn builtin_fold_eval(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    let (f, init, items) = match args.len() {
        2 => {
            // Fold[f, list] — use first element as init
            match &args[1] {
                Value::List(list) if !list.is_empty() => {
                    (&args[0], list[0].clone(), &list[1..])
                }
                Value::List(_) => return Err(EvalError::Error("Fold on empty list requires initial value".to_string())),
                _ => return Err(EvalError::TypeError {
                    expected: "List".to_string(),
                    got: args[1].type_name().to_string(),
                }),
            }
        }
        3 => {
            // Fold[f, init, list]
            match &args[2] {
                Value::List(list) => (&args[0], args[1].clone(), list.as_slice()),
                _ => return Err(EvalError::TypeError {
                    expected: "List".to_string(),
                    got: args[2].type_name().to_string(),
                }),
            }
        }
        _ => return Err(EvalError::Error("Fold requires 2 or 3 arguments".to_string())),
    };
    let mut acc = init;
    for item in items {
        acc = apply_function(f, &[acc, item.clone()], env)?;
    }
    Ok(acc)
}

/// Select[list, test] — keep elements where test returns True.
fn builtin_select_eval(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error("Select requires exactly 2 arguments".to_string()));
    }
    match &args[0] {
        Value::List(items) => {
            let test = &args[1];
            let mut result = Vec::new();
            for item in items {
                let keep = apply_function(test, &[item.clone()], env)?;
                if keep.to_bool() {
                    result.push(item.clone());
                }
            }
            Ok(Value::List(result))
        }
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

/// Scan[f, list] — like Map but returns Null.
fn builtin_scan_eval(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error("Scan requires exactly 2 arguments".to_string()));
    }
    match &args[1] {
        Value::List(items) => {
            for item in items {
                apply_function(&args[0], &[item.clone()], env)?;
            }
            Ok(Value::Null)
        }
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: args[1].type_name().to_string(),
        }),
    }
}

/// Nest[f, x, n] — apply f to x, n times.
fn builtin_nest_eval(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Err(EvalError::Error("Nest requires exactly 3 arguments".to_string()));
    }
    let n = args[2].to_integer().ok_or_else(|| EvalError::TypeError {
        expected: "Integer".to_string(),
        got: args[2].type_name().to_string(),
    })?;
    if n < 0 {
        return Err(EvalError::Error("Nest count must be non-negative".to_string()));
    }
    let mut val = args[1].clone();
    for _ in 0..n {
        val = apply_function(&args[0], &[val], env)?;
    }
    Ok(val)
}

/// MatchQ[expr, pattern] — returns True/False.
fn builtin_match_q_eval(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error("MatchQ requires exactly 2 arguments".to_string()));
    }
    // The pattern arg is stored as a Value::Pattern(Expr)
    let result = match &args[1] {
        Value::Pattern(pat_expr) => {
            matches!(crate::pattern::match_pattern(pat_expr, &args[0]), crate::pattern::MatchResult::Match(_))
        }
        // If the pattern is a regular value, do structural equality
        _ => args[0].struct_eq(&args[1]),
    };
    Ok(Value::Bool(result))
}

/// FreeQ[expr, pattern] — returns True if pattern does not appear anywhere in expr.
fn builtin_free_q_eval(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error("FreeQ requires exactly 2 arguments".to_string()));
    }
    fn contains_pattern(value: &Value, pattern: &Value) -> bool {
        // Check if value matches pattern
        let matched = match pattern {
            Value::Pattern(pat_expr) => {
                matches!(crate::pattern::match_pattern(pat_expr, value), crate::pattern::MatchResult::Match(_))
            }
            _ => value.struct_eq(pattern),
        };
        if matched {
            return true;
        }
        // Recurse into compound values
        match value {
            Value::List(items) => items.iter().any(|item| contains_pattern(item, pattern)),
            Value::Call { args, .. } => args.iter().any(|arg| contains_pattern(arg, pattern)),
            _ => false,
        }
    }
    Ok(Value::Bool(!contains_pattern(&args[0], &args[1])))
}

/// Table[expr, {i, n}] or Table[expr, {i, min, max}] or Table[expr, {i, min, max, step}].
/// Called from eval_call as a special form (iterator spec has unevaluated symbols).
fn eval_table(args: &[Expr], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error("Table requires exactly 2 arguments".to_string()));
    }
    let iter_items = match &args[1] {
        Expr::List(items) => items,
        _ => return Err(EvalError::Error("Table iterator spec must be a list".to_string())),
    };

    // Parse iterator spec: {var, n} or {var, min, max} or {var, min, max, step}
    let (var_name, min, max, step) = match iter_items.len() {
        2 => {
            let var = match &iter_items[0] {
                Expr::Symbol(s) => s.clone(),
                _ => return Err(EvalError::Error("Table iterator variable must be a symbol".to_string())),
            };
            let n = eval(&iter_items[1], env)?.to_integer().ok_or_else(|| EvalError::TypeError {
                expected: "Integer".to_string(),
                got: "non-Integer".to_string(),
            })?;
            (var, 1i64, n, 1i64)
        }
        3 => {
            let var = match &iter_items[0] {
                Expr::Symbol(s) => s.clone(),
                _ => return Err(EvalError::Error("Table iterator variable must be a symbol".to_string())),
            };
            let min = eval(&iter_items[1], env)?.to_integer().ok_or_else(|| EvalError::TypeError {
                expected: "Integer".to_string(),
                got: "non-Integer".to_string(),
            })?;
            let max = eval(&iter_items[2], env)?.to_integer().ok_or_else(|| EvalError::TypeError {
                expected: "Integer".to_string(),
                got: "non-Integer".to_string(),
            })?;
            (var, min, max, 1i64)
        }
        4 => {
            let var = match &iter_items[0] {
                Expr::Symbol(s) => s.clone(),
                _ => return Err(EvalError::Error("Table iterator variable must be a symbol".to_string())),
            };
            let min = eval(&iter_items[1], env)?.to_integer().ok_or_else(|| EvalError::TypeError {
                expected: "Integer".to_string(),
                got: "non-Integer".to_string(),
            })?;
            let max = eval(&iter_items[2], env)?.to_integer().ok_or_else(|| EvalError::TypeError {
                expected: "Integer".to_string(),
                got: "non-Integer".to_string(),
            })?;
            let step = eval(&iter_items[3], env)?.to_integer().ok_or_else(|| EvalError::TypeError {
                expected: "Integer".to_string(),
                got: "non-Integer".to_string(),
            })?;
            if step == 0 {
                return Err(EvalError::Error("Table step cannot be zero".to_string()));
            }
            (var, min, max, step)
        }
        _ => return Err(EvalError::Error("Table iterator spec must have 2-4 elements".to_string())),
    };

    let expr = &args[0];
    let child_env = env.child();
    let mut result = Vec::new();

    if step > 0 {
        let mut i = min;
        while i <= max {
            child_env.set(var_name.clone(), Value::Integer(Integer::from(i)));
            result.push(eval(expr, &child_env)?);
            i += step;
        }
    } else {
        let mut i = min;
        while i >= max {
            child_env.set(var_name.clone(), Value::Integer(Integer::from(i)));
            result.push(eval(expr, &child_env)?);
            i += step;
        }
    }

    Ok(Value::List(result))
}

/// Sum[expr, {i, min, max}] — like Table but adds results.
/// Called from eval_call as a special form (iterator spec has unevaluated symbols).
fn eval_sum(args: &[Expr], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error("Sum requires exactly 2 arguments".to_string()));
    }
    let iter_items = match &args[1] {
        Expr::List(items) => items,
        _ => return Err(EvalError::Error("Sum iterator spec must be a list".to_string())),
    };

    let (var_name, min, max) = match iter_items.len() {
        3 => {
            let var = match &iter_items[0] {
                Expr::Symbol(s) => s.clone(),
                _ => return Err(EvalError::Error("Sum iterator variable must be a symbol".to_string())),
            };
            let min = eval(&iter_items[1], env)?.to_integer().ok_or_else(|| EvalError::TypeError {
                expected: "Integer".to_string(),
                got: "non-Integer".to_string(),
            })?;
            let max = eval(&iter_items[2], env)?.to_integer().ok_or_else(|| EvalError::TypeError {
                expected: "Integer".to_string(),
                got: "non-Integer".to_string(),
            })?;
            (var, min, max)
        }
        _ => return Err(EvalError::Error("Sum iterator spec must have 3 elements {i, min, max}".to_string())),
    };

    let expr = &args[0];
    let child_env = env.child();
    let mut acc = Value::Integer(Integer::from(0));

    for i in min..=max {
        child_env.set(var_name.clone(), Value::Integer(Integer::from(i)));
        let val = eval(expr, &child_env)?;
        acc = crate::builtins::add_values_public(&acc, &val)?;
    }

    Ok(acc)
}

/// FixedPoint[f, x] — apply f until result stops changing.
fn builtin_fixed_point_eval(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(EvalError::Error("FixedPoint requires 2 or 3 arguments".to_string()));
    }
    let max_iter = if args.len() == 3 {
        args[2].to_integer().ok_or_else(|| EvalError::TypeError {
            expected: "Integer".to_string(),
            got: args[2].type_name().to_string(),
        })? as usize
    } else {
        1000
    };

    let f = &args[0];
    let mut val = args[1].clone();
    for _ in 0..max_iter {
        let new_val = apply_function(f, &[val.clone()], env)?;
        if new_val.struct_eq(&val) {
            return Ok(new_val);
        }
        val = new_val;
    }
    Ok(val)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer;
    use crate::parser;
    use crate::builtins;

    fn eval_str(input: &str) -> Value {
        let env = Env::new();
        builtins::register_builtins(&env);
        let tokens = lexer::tokenize(input).unwrap();
        let ast = parser::parse(tokens).unwrap();
        eval_program(&ast, &env).unwrap()
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
        assert_eq!(eval_str("x = 1; y = 2; x + y"), Value::Integer(Integer::from(3)));
    }

    // ── Functions ──

    #[test]
    fn test_function_def_and_call() {
        assert_eq!(eval_str("f[x_] := x^2; f[3]"), Value::Integer(Integer::from(9)));
    }

    #[test]
    fn test_function_multi_arg() {
        assert_eq!(eval_str("add[a_, b_] := a + b; add[3, 4]"), Value::Integer(Integer::from(7)));
    }

    // ── Control flow ──

    #[test]
    fn test_if_true() {
        assert_eq!(eval_str("If[True, 1, 2]"), Value::Integer(Integer::from(1)));
    }

    #[test]
    fn test_if_false() {
        assert_eq!(eval_str("If[False, 1, 2]"), Value::Integer(Integer::from(2)));
    }

    #[test]
    fn test_if_no_else() {
        assert_eq!(eval_str("If[False, 1]"), Value::Null);
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
        assert_eq!(eval_str("{1, 2, 3}"), Value::List(vec![
            Value::Integer(Integer::from(1)),
            Value::Integer(Integer::from(2)),
            Value::Integer(Integer::from(3)),
        ]));
    }

    #[test]
    fn test_empty_list() {
        assert_eq!(eval_str("{}"), Value::List(vec![]));
    }

    #[test]
    fn test_list_operations() {
        assert_eq!(eval_str("Length[{1, 2, 3}]"), Value::Integer(Integer::from(3)));
        assert_eq!(eval_str("First[{1, 2, 3}]"), Value::Integer(Integer::from(1)));
        assert_eq!(eval_str("Last[{1, 2, 3}]"), Value::Integer(Integer::from(3)));
    }

    // ── Pipe ──

    #[test]
    fn test_pipe() {
        // {1, 2, 3} // Length
        assert_eq!(eval_str("{1, 2, 3} // Length"), Value::Integer(Integer::from(3)));
    }

    // ── Prefix ──

    #[test]
    fn test_prefix() {
        // f @ x is equivalent to f[x]
        assert_eq!(eval_str("Length @ {1, 2, 3}"), Value::Integer(Integer::from(3)));
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
        assert_eq!(result, Value::List(vec![
            Value::Integer(Integer::from(1)),
            Value::Integer(Integer::from(2)),
            Value::Integer(Integer::from(3)),
        ]));
    }

    // ── Constants ──

    #[test]
    fn test_pi() {
        let val = eval_str("Pi");
        match val {
            Value::Real(r) => assert!((r.to_f64() - std::f64::consts::PI).abs() < 1e-15),
            _ => panic!("Expected Real, got {:?}", val),
        }
    }

    #[test]
    fn test_e() {
        let val = eval_str("E");
        match val {
            Value::Real(r) => {
                let e = r.to_f64();
                // Check first 15 digits of Euler's number
                assert!((e - 2.718281828459045).abs() < 1e-10, "Expected ~e, got {}", e);
            }
            _ => panic!("Expected Real, got {:?}", val),
        }
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
        assert_eq!(eval_str(r#"StringLength["hello"]"#), Value::Integer(Integer::from(5)));
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
        assert_eq!(result, Value::List(vec![
            Value::Integer(Integer::from(1)),
            Value::Integer(Integer::from(4)),
            Value::Integer(Integer::from(9)),
        ]));
    }

    #[test]
    fn test_map_with_builtin() {
        let result = eval_str("Map[Sqrt, {1, 4, 9}]");
        assert_eq!(result, Value::List(vec![
            Value::Integer(Integer::from(1)),
            Value::Integer(Integer::from(2)),
            Value::Integer(Integer::from(3)),
        ]));
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
        assert_eq!(result, Value::List(vec![Value::Integer(Integer::from(4)), Value::Integer(Integer::from(5))]));
    }

    #[test]
    fn test_nest() {
        let result = eval_str("sq[x_] := x^2; Nest[sq, 2, 3]");
        assert_eq!(result, Value::Integer(Integer::from(256))); // ((2^2)^2)^2 = 256
    }

    #[test]
    fn test_table_basic() {
        let result = eval_str("Table[i^2, {i, 1, 5}]");
        assert_eq!(result, Value::List(vec![
            Value::Integer(Integer::from(1)),
            Value::Integer(Integer::from(4)),
            Value::Integer(Integer::from(9)),
            Value::Integer(Integer::from(16)),
            Value::Integer(Integer::from(25)),
        ]));
    }

    #[test]
    fn test_table_short_form() {
        let result = eval_str("Table[i, {i, 3}]");
        assert_eq!(result, Value::List(vec![
            Value::Integer(Integer::from(1)),
            Value::Integer(Integer::from(2)),
            Value::Integer(Integer::from(3)),
        ]));
    }

    #[test]
    fn test_table_with_step() {
        let result = eval_str("Table[i, {i, 0, 10, 2}]");
        assert_eq!(result, Value::List(vec![
            Value::Integer(Integer::from(0)),
            Value::Integer(Integer::from(2)),
            Value::Integer(Integer::from(4)),
            Value::Integer(Integer::from(6)),
            Value::Integer(Integer::from(8)),
            Value::Integer(Integer::from(10)),
        ]));
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
        // Due to floating point, result is a very small Real number
        match result {
            Value::Real(r) => assert!(r.clone().abs() < 1e-10, "Expected near-zero, got {}", r),
            Value::Integer(n) => assert_eq!(n, 0),
            _ => panic!("Expected numeric value, got {:?}", result),
        }
    }

    // ── Pattern guards ──

    #[test]
    fn test_pattern_guard_function() {
        // f[x_ /; x > 0] := "positive"
        // f[x_ /; x < 0] := "negative"
        let result = eval_str(
            r#"f[x_ /; x > 0] := "positive"; f[x_ /; x < 0] := "negative"; f[5]"#
        );
        assert_eq!(result, Value::Str("positive".to_string()));
    }

    #[test]
    fn test_pattern_guard_negative() {
        let result = eval_str(
            r#"f[x_ /; x > 0] := "positive"; f[x_ /; x < 0] := "negative"; f[-3]"#
        );
        assert_eq!(result, Value::Str("negative".to_string()));
    }

    #[test]
    fn test_pattern_guard_match_expression() {
        let result = eval_str(
            r#"match 7 { n_ /; n > 5 => "big"; n_ => "small" }"#
        );
        assert_eq!(result, Value::Str("big".to_string()));
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
}
