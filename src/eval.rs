/// Tree-walk evaluator for Syma language.
///
/// Evaluates AST expressions in an environment. Handles:
/// - Symbol lookup and function application
/// - Pattern-matched function definitions
/// - Rule application (/. and //.)
/// - Class instantiation and method dispatch
/// - Control flow (If, Which, Switch, match, loops)
use std::collections::HashMap;
use std::sync::Arc;

use rug::float::Constant;
use rug::ops::Pow;
use rug::{Float, Integer};

use crate::ast::*;
use crate::env::Env;
use crate::ffi;
use crate::pattern::{Bindings, MatchResult, match_pattern};
use crate::value::*;

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
        Expr::Symbol(s) => Ok(env.get(s).unwrap_or_else(|| Value::Symbol(s.clone()))),

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
        Expr::Call { head, args } => eval_call(head, args, env),

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
        Expr::For {
            init,
            condition,
            step,
            body,
        } => {
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
                        _ => {
                            return Err(EvalError::TypeError {
                                expected: "List".to_string(),
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
        } => {
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
            });

            env.set(name.clone(), Value::Function(Arc::new(func)));
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
                // this.field = value  (desugared to Assign(field[this], value))
                Expr::Call {
                    head,
                    args: call_args,
                } if call_args.len() == 1 => {
                    if let Expr::Symbol(field_name) = head.as_ref() {
                        let target = eval(&call_args[0], env)?;
                        match target {
                            Value::Object {
                                class_name,
                                mut fields,
                            } => {
                                fields.insert(field_name.clone(), val.clone());
                                let updated = Value::Object { class_name, fields };
                                if let Expr::Symbol(s) = &call_args[0] {
                                    if s == "this" {
                                        env.set("this".to_string(), updated.clone());
                                    }
                                }
                                Ok(val)
                            }
                            _ => Err(EvalError::Error("Invalid assignment target".to_string())),
                        }
                    } else {
                        Err(EvalError::Error("Invalid assignment target".to_string()))
                    }
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
                                if let MatchResult::Match(bindings) = match_pattern(pat, item) {
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
            if let Some(ref parent_name) = parent_name {
                if let Some(parent_val) = env.get(parent_name) {
                    if let Value::Class(parent_class) = parent_val {
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
                }
            }

            // Resolve mixins and merge their methods
            for mixin_name in mixins {
                if let Some(mixin_val) = env.get(mixin_name) {
                    if let Value::Class(mixin_class) = mixin_val {
                        for (method_name, method) in &mixin_class.methods {
                            methods
                                .entry(method_name.clone())
                                .or_insert_with(|| method.clone());
                        }
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
            let module_val = Value::Module {
                name: name.clone(),
                exports: export_map,
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
        Expr::Blank { .. }
        | Expr::NamedBlank { .. }
        | Expr::BlankSequence { .. }
        | Expr::BlankNullSequence { .. }
        | Expr::PatternGuard { .. } => Ok(Value::Pattern(expr.clone())),

        // ── Slot (only meaningful inside pure functions) ──
        Expr::Slot(_) => Err(EvalError::Error(
            "Slot # used outside of pure function".to_string(),
        )),

        // ── Information (help) ──
        Expr::Information(inner) => eval_information(inner, env),

        // ── Function constructor ──
        Expr::Function { params, body } => Ok(Value::PureFunction {
            body: body.as_ref().clone(),
            slot_count: params.len(),
        }),
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
        Expr::Blank { .. }
        | Expr::NamedBlank { .. }
        | Expr::BlankSequence { .. }
        | Expr::BlankNullSequence { .. }
        | Expr::PatternGuard { .. } => true,
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
                    return Err(EvalError::Error(
                        "Set requires exactly 2 arguments".to_string(),
                    ));
                }
                let val = eval(&args[1], env)?;
                match &args[0] {
                    Expr::Symbol(name) => {
                        env.set(name.clone(), val.clone());
                        Ok(val)
                    }
                    // this.field = value  (desugared to Set[field[this], value])
                    Expr::Call {
                        head,
                        args: call_args,
                    } if call_args.len() == 1 => {
                        if let Expr::Symbol(field_name) = head.as_ref() {
                            let target = eval(&call_args[0], env)?;
                            match target {
                                Value::Object {
                                    class_name,
                                    mut fields,
                                } => {
                                    fields.insert(field_name.clone(), val.clone());
                                    let updated = Value::Object { class_name, fields };
                                    // If target is 'this', update it in the environment
                                    if let Expr::Symbol(s) = &call_args[0] {
                                        if s == "this" {
                                            env.set("this".to_string(), updated.clone());
                                        }
                                    }
                                    Ok(val)
                                }
                                _ => Err(EvalError::Error("Invalid assignment target".to_string())),
                            }
                        } else {
                            Err(EvalError::Error("Invalid assignment target".to_string()))
                        }
                    }
                    _ => Err(EvalError::Error("Invalid assignment target".to_string())),
                }
            }
            "Hold" => {
                // Don't evaluate arguments
                Ok(Value::Hold(Box::new(Value::List(
                    args.iter().map(|a| Value::Pattern(a.clone())).collect(),
                ))))
            }
            "Table" => {
                // Table[expr, {i, min, max}] — iterator spec has unevaluated symbols
                eval_table(args, env)
            }
            "ParallelTable" => {
                // ParallelTable[expr, {i, min, max}] — parallel version of Table
                eval_parallel_table(args, env)
            }
            "Sum" => {
                // Sum[expr, {i, min, max}] — iterator spec has unevaluated symbols
                eval_sum(args, env)
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
                    Err(EvalError::Thrown(v)) => Ok(v),
                    Err(e) => Err(e),
                }
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
                numeric_eval_expr(&args[0], prec_bits, env)
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
                "ParallelMap" => return builtin_parallel_map_eval(args, env),
                // ── FFI builtins (need env access) ──
                "LoadLibrary" => {
                    if args.len() != 1 {
                        return Err(EvalError::Error(
                            "LoadLibrary requires exactly 1 argument".to_string(),
                        ));
                    }
                    if let Value::Str(path) = &args[0] {
                        return ffi::loader::load_native_library(path, env);
                    }
                    return Err(EvalError::TypeError {
                        expected: "String".to_string(),
                        got: args[0].type_name().to_string(),
                    });
                }
                "LoadExtension" => {
                    if args.len() != 1 {
                        return Err(EvalError::Error(
                            "LoadExtension requires exactly 1 argument".to_string(),
                        ));
                    }
                    if let Value::Str(path) = &args[0] {
                        ffi::extension::load_extension(path, env)?;
                        return Ok(Value::Null);
                    }
                    return Err(EvalError::TypeError {
                        expected: "String".to_string(),
                        got: args[0].type_name().to_string(),
                    });
                }
                "LibraryFunction" => {
                    // LibraryFunction[lib, "symbol", {types} -> retType]
                    if args.len() != 3 {
                        return Err(EvalError::Error(
                            "LibraryFunction requires 3 arguments: lib, symbol, signature"
                                .to_string(),
                        ));
                    }
                    let sym = match &args[1] {
                        Value::Str(s) => s.clone(),
                        _ => {
                            return Err(EvalError::TypeError {
                                expected: "String".to_string(),
                                got: args[1].type_name().to_string(),
                            });
                        }
                    };
                    let sig = ffi::loader::parse_sig(&args[2])?;
                    return ffi::loader::library_function(&args[0], &sym, sig);
                }
                "LibraryFunctionLoad" => {
                    // LibraryFunctionLoad["path", "symbol", {types} -> retType]
                    if args.len() != 3 {
                        return Err(EvalError::Error(
                            "LibraryFunctionLoad requires 3 arguments: path, symbol, signature"
                                .to_string(),
                        ));
                    }
                    let path = match &args[0] {
                        Value::Str(s) => s.clone(),
                        _ => {
                            return Err(EvalError::TypeError {
                                expected: "String".to_string(),
                                got: args[0].type_name().to_string(),
                            });
                        }
                    };
                    let sym = match &args[1] {
                        Value::Str(s) => s.clone(),
                        _ => {
                            return Err(EvalError::TypeError {
                                expected: "String".to_string(),
                                got: args[1].type_name().to_string(),
                            });
                        }
                    };
                    let sig = ffi::loader::parse_sig(&args[2])?;
                    let lib = ffi::loader::load_native_library(&path, env)?;
                    return ffi::loader::library_function(&lib, &sym, sig);
                }
                "ExternalEvaluate" => {
                    if args.len() < 2 {
                        return Err(EvalError::Error(
                            "ExternalEvaluate requires at least 2 arguments: system, opts"
                                .to_string(),
                        ));
                    }
                    let system = match &args[0] {
                        Value::Str(s) => s.clone(),
                        _ => {
                            return Err(EvalError::TypeError {
                                expected: "String".to_string(),
                                got: args[0].type_name().to_string(),
                            });
                        }
                    };
                    let (module, func, call_args) =
                        ffi::python::parse_external_evaluate_args(&system, &args[1], &args[2..])?;
                    return ffi::python::call_python(&module, &func, &call_args);
                }
                _ => {
                    // Extension-registered builtin: trampoline through ext registry.
                    if std::ptr::fn_addr_eq(
                        *f as fn(&[Value]) -> _,
                        ffi::extension::EXT_DISPATCH_FN as fn(&[Value]) -> _,
                    ) {
                        return ffi::extension::call_ext_fn(name, args);
                    }
                }
            }
            f(args).map_err(|e| match e {
                EvalError::NoMatch { .. } => EvalError::NoMatch {
                    head: name.clone(),
                    args: args.to_vec(),
                },
                other => other,
            })
        }

        Value::NativeFunction {
            fn_ptr, signature, ..
        } => ffi::loader::call_native(*fn_ptr, signature, args),

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

        Value::PureFunction {
            body,
            slot_count: _,
        } => {
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
            } else if !args.is_empty() {
                // Check if first arg is an object — field access or method call
                if let Value::Object { class_name, fields } = &args[0] {
                    // Field access: single arg
                    if args.len() == 1 {
                        if let Some(val) = fields.get(name) {
                            return Ok(val.clone());
                        }
                    }
                    // Method dispatch: look up method on the class
                    if let Some(class_val) = env.get(class_name) {
                        if let Value::Class(class_def) = class_val {
                            if let Some(method) = class_def.methods.get(name) {
                                let child_env = env.child();
                                // Bind 'this' to the object
                                child_env.set("this".to_string(), args[0].clone());
                                // Bind field names to their values
                                for (field_name, field_val) in fields {
                                    child_env.set(field_name.clone(), field_val.clone());
                                }
                                // Match method params to remaining args
                                let method_args = &args[1..];
                                if let Some(bindings) =
                                    try_match_params(&method.params, method_args, env)?
                                {
                                    for (bind_name, bind_val) in &bindings {
                                        child_env.set(bind_name.clone(), bind_val.clone());
                                    }
                                    return eval(&method.body, &child_env);
                                }
                            }
                        }
                    }
                }
                // Return unevaluated
                Ok(Value::Call {
                    head: name.clone(),
                    args: args.to_vec(),
                })
            } else {
                // Return unevaluated
                Ok(Value::Call {
                    head: name.clone(),
                    args: args.to_vec(),
                })
            }
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
fn try_match_params(
    params: &[Expr],
    args: &[Value],
    env: &Env,
) -> Result<Option<Bindings>, EvalError> {
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
        Value::RuleSet {
            rules: rule_pairs, ..
        } => {
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
            let values: Result<Vec<Value>, _> = items
                .iter()
                .map(|item| substitute_value(item, bindings))
                .collect();
            Ok(Value::List(values?))
        }
        Expr::Call { head, args } => {
            let h = substitute_value(head, bindings)?;
            let a: Result<Vec<Value>, _> = args
                .iter()
                .map(|arg| substitute_value(arg, bindings))
                .collect();
            match h {
                Value::Symbol(name) => Ok(Value::Call {
                    head: name,
                    args: a?,
                }),
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

/// Evaluate an expression numerically at the given bit precision.
/// Keeps symbolic constants (Pi, E) alive so they can be computed at
/// arbitrary precision instead of using the pre-stored DEFAULT_PRECISION value.
fn numeric_eval_expr(expr: &Expr, prec_bits: u32, env: &Env) -> Result<Value, EvalError> {
    match expr {
        Expr::Symbol(s) => match s.as_str() {
            "Pi" => Ok(Value::Real(Float::with_val(prec_bits, Constant::Pi))),
            "E" => {
                let one = Float::with_val(prec_bits, 1u32);
                Ok(Value::Real(one.exp()))
            }
            _ => {
                let v = eval(expr, env)?;
                coerce_to_float(v, prec_bits)
            }
        },
        Expr::Integer(n) => Ok(Value::Real(Float::with_val(prec_bits, n))),
        Expr::Real(r) => Ok(Value::Real(Float::with_val(prec_bits, r))),
        // Recursively evaluate calls at the requested precision.
        // Each argument is numeric-evaluated first, then the operation is
        // performed on high-precision floats.
        Expr::Call { head, args } => {
            if let Expr::Symbol(name) = head.as_ref() {
                let evaluated_args: Result<Vec<Value>, _> = args
                    .iter()
                    .map(|a| numeric_eval_expr(a, prec_bits, env))
                    .collect();
                let evaluated_args = evaluated_args?;
                match name.as_str() {
                    "Plus" => numeric_fold_op(evaluated_args, prec_bits, |a, b| a + b),
                    "Times" => numeric_fold_op(evaluated_args, prec_bits, |a, b| a * b),
                    "Power" if evaluated_args.len() == 2 => {
                        let (b, e) = (&evaluated_args[0], &evaluated_args[1]);
                        let bf = to_float(b, prec_bits);
                        let ef = to_float(e, prec_bits);
                        match (bf, ef) {
                            (Some(b), Some(e)) => Ok(Value::Real(b.pow(e))),
                            _ => apply_function(&eval(head, env)?, &evaluated_args, env),
                        }
                    }
                    "Divide" if evaluated_args.len() == 2 => {
                        let bf = to_float(&evaluated_args[0], prec_bits);
                        let ef = to_float(&evaluated_args[1], prec_bits);
                        match (bf, ef) {
                            (Some(a), Some(b)) => Ok(Value::Real(a / b)),
                            _ => apply_function(&eval(head, env)?, &evaluated_args, env),
                        }
                    }
                    "Log" => {
                        let ln = |v: &Value| -> Option<Float> {
                            to_float(v, prec_bits).map(|f| Float::with_val(prec_bits, f.ln()))
                        };
                        match evaluated_args.len() {
                            1 => match ln(&evaluated_args[0]) {
                                Some(r) => Ok(Value::Real(r)),
                                None => apply_function(&eval(head, env)?, &evaluated_args, env),
                            },
                            2 => match (ln(&evaluated_args[1]), ln(&evaluated_args[0])) {
                                (Some(lx), Some(lb)) => {
                                    Ok(Value::Real(Float::with_val(prec_bits, &lx) / lb))
                                }
                                _ => apply_function(&eval(head, env)?, &evaluated_args, env),
                            },
                            _ => apply_function(&eval(head, env)?, &evaluated_args, env),
                        }
                    }
                    "Sin" if evaluated_args.len() == 1 => {
                        match to_float(&evaluated_args[0], prec_bits) {
                            Some(f) => Ok(Value::Real(f.sin())),
                            None => apply_function(&eval(head, env)?, &evaluated_args, env),
                        }
                    }
                    "Cos" if evaluated_args.len() == 1 => {
                        match to_float(&evaluated_args[0], prec_bits) {
                            Some(f) => Ok(Value::Real(f.cos())),
                            None => apply_function(&eval(head, env)?, &evaluated_args, env),
                        }
                    }
                    "Tan" if evaluated_args.len() == 1 => {
                        match to_float(&evaluated_args[0], prec_bits) {
                            Some(f) => Ok(Value::Real(f.tan())),
                            None => apply_function(&eval(head, env)?, &evaluated_args, env),
                        }
                    }
                    "Exp" if evaluated_args.len() == 1 => {
                        match to_float(&evaluated_args[0], prec_bits) {
                            Some(f) => Ok(Value::Real(f.exp())),
                            None => apply_function(&eval(head, env)?, &evaluated_args, env),
                        }
                    }
                    "Sqrt" if evaluated_args.len() == 1 => {
                        match to_float(&evaluated_args[0], prec_bits) {
                            Some(f) => Ok(Value::Real(f.sqrt())),
                            None => apply_function(&eval(head, env)?, &evaluated_args, env),
                        }
                    }
                    _ => apply_function(&eval(head, env)?, &evaluated_args, env),
                }
            } else {
                let v = eval(expr, env)?;
                coerce_to_float(v, prec_bits)
            }
        }
        _ => {
            let v = eval(expr, env)?;
            coerce_to_float(v, prec_bits)
        }
    }
}

fn to_float(v: &Value, prec_bits: u32) -> Option<Float> {
    match v {
        Value::Integer(n) => Some(Float::with_val(prec_bits, n)),
        Value::Real(r) => Some(Float::with_val(prec_bits, r)),
        _ => None,
    }
}

fn numeric_fold_op<F>(args: Vec<Value>, prec_bits: u32, op: F) -> Result<Value, EvalError>
where
    F: Fn(Float, Float) -> Float,
{
    let mut acc: Option<Float> = None;
    for v in &args {
        match to_float(v, prec_bits) {
            Some(f) => {
                acc = Some(match acc {
                    None => f,
                    Some(a) => op(a, f),
                })
            }
            None => {
                return Ok(Value::Call {
                    head: "Unknown".to_string(),
                    args,
                });
            }
        }
    }
    Ok(acc
        .map(Value::Real)
        .unwrap_or(Value::Integer(Integer::from(0))))
}

fn coerce_to_float(v: Value, prec_bits: u32) -> Result<Value, EvalError> {
    match v {
        Value::Integer(n) => Ok(Value::Real(Float::with_val(prec_bits, n))),
        Value::Real(r) => Ok(Value::Real(Float::with_val(prec_bits, r))),
        other => Ok(other),
    }
}

/// Map[f, list] — apply f to each element of list.
fn builtin_map_eval(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Map requires exactly 2 arguments".to_string(),
        ));
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
                Value::List(list) if !list.is_empty() => (&args[0], list[0].clone(), &list[1..]),
                Value::List(_) => {
                    return Err(EvalError::Error(
                        "Fold on empty list requires initial value".to_string(),
                    ));
                }
                _ => {
                    return Err(EvalError::TypeError {
                        expected: "List".to_string(),
                        got: args[1].type_name().to_string(),
                    });
                }
            }
        }
        3 => {
            // Fold[f, init, list]
            match &args[2] {
                Value::List(list) => (&args[0], args[1].clone(), list.as_slice()),
                _ => {
                    return Err(EvalError::TypeError {
                        expected: "List".to_string(),
                        got: args[2].type_name().to_string(),
                    });
                }
            }
        }
        _ => {
            return Err(EvalError::Error(
                "Fold requires 2 or 3 arguments".to_string(),
            ));
        }
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
        return Err(EvalError::Error(
            "Select requires exactly 2 arguments".to_string(),
        ));
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
        return Err(EvalError::Error(
            "Scan requires exactly 2 arguments".to_string(),
        ));
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
        return Err(EvalError::Error(
            "Nest requires exactly 3 arguments".to_string(),
        ));
    }
    let n = args[2].to_integer().ok_or_else(|| EvalError::TypeError {
        expected: "Integer".to_string(),
        got: args[2].type_name().to_string(),
    })?;
    if n < 0 {
        return Err(EvalError::Error(
            "Nest count must be non-negative".to_string(),
        ));
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
        return Err(EvalError::Error(
            "MatchQ requires exactly 2 arguments".to_string(),
        ));
    }
    // The pattern arg is stored as a Value::Pattern(Expr)
    let result = match &args[1] {
        Value::Pattern(pat_expr) => {
            matches!(
                crate::pattern::match_pattern(pat_expr, &args[0]),
                crate::pattern::MatchResult::Match(_)
            )
        }
        // If the pattern is a regular value, do structural equality
        _ => args[0].struct_eq(&args[1]),
    };
    Ok(Value::Bool(result))
}

/// FreeQ[expr, pattern] — returns True if pattern does not appear anywhere in expr.
fn builtin_free_q_eval(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "FreeQ requires exactly 2 arguments".to_string(),
        ));
    }
    fn contains_pattern(value: &Value, pattern: &Value) -> bool {
        // Check if value matches pattern
        let matched = match pattern {
            Value::Pattern(pat_expr) => {
                matches!(
                    crate::pattern::match_pattern(pat_expr, value),
                    crate::pattern::MatchResult::Match(_)
                )
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

/// Table[expr, n] or Table[expr, {i, ...}] or Table[expr, {i, ...}, {j, ...}, ...].
/// Called from eval_call as a special form (iterator spec has unevaluated symbols).
fn eval_table(args: &[Expr], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "Table requires at least 2 arguments".to_string(),
        ));
    }

    let expr = &args[0];

    // Case 1: Table[expr, n] — n copies of expr (no variable binding)
    if args.len() == 2 {
        // Check if second arg is a plain integer (not a list)
        if let Expr::Integer(_) = &args[1] {
            let n = eval(&args[1], env)?
                .to_integer()
                .ok_or_else(|| EvalError::TypeError {
                    expected: "Integer".to_string(),
                    got: "non-Integer".to_string(),
                })?;
            if n < 0 {
                return Err(EvalError::Error(
                    "Table count must be non-negative".to_string(),
                ));
            }
            let mut result = Vec::new();
            for _ in 0..n {
                result.push(eval(expr, env)?);
            }
            return Ok(Value::List(result));
        }
    }

    // Case 2: Table[expr, {i, ...}] or Table[expr, {i, ...}, {j, ...}, ...]
    let iter_specs = &args[1..];
    eval_table_recursive(expr, iter_specs, env, 0)
}

/// Recursive helper for nested Table iteration.
fn eval_table_recursive(
    expr: &Expr,
    iter_specs: &[Expr],
    env: &Env,
    depth: usize,
) -> Result<Value, EvalError> {
    if depth >= iter_specs.len() {
        // Base case: all iterators processed, evaluate expression
        return eval(expr, env);
    }

    let iter_items = match &iter_specs[depth] {
        Expr::List(items) => items,
        _ => {
            return Err(EvalError::Error(
                "Table iterator spec must be a list".to_string(),
            ));
        }
    };

    // Parse the iterator spec and generate values
    let (var_name, values) =
        match iter_items.len() {
            2 => {
                // {var, n} or {var, {values}}
                let var = match &iter_items[0] {
                    Expr::Symbol(s) => s.clone(),
                    _ => {
                        return Err(EvalError::Error(
                            "Table iterator variable must be a symbol".to_string(),
                        ));
                    }
                };

                // Check if second element is a list (explicit values)
                if let Expr::List(_) = &iter_items[1] {
                    let list_val = eval(&iter_items[1], env)?;
                    match list_val {
                        Value::List(items) => (var, items),
                        _ => {
                            return Err(EvalError::TypeError {
                                expected: "List".to_string(),
                                got: list_val.type_name().to_string(),
                            });
                        }
                    }
                } else {
                    // {var, n} — iterate 1..=n
                    let n = eval(&iter_items[1], env)?.to_integer().ok_or_else(|| {
                        EvalError::TypeError {
                            expected: "Integer".to_string(),
                            got: "non-Integer".to_string(),
                        }
                    })?;
                    let values: Vec<Value> =
                        (1..=n).map(|i| Value::Integer(Integer::from(i))).collect();
                    (var, values)
                }
            }
            3 => {
                // {var, min, max}
                let var = match &iter_items[0] {
                    Expr::Symbol(s) => s.clone(),
                    _ => {
                        return Err(EvalError::Error(
                            "Table iterator variable must be a symbol".to_string(),
                        ));
                    }
                };
                let min = eval(&iter_items[1], env)?.to_integer().ok_or_else(|| {
                    EvalError::TypeError {
                        expected: "Integer".to_string(),
                        got: "non-Integer".to_string(),
                    }
                })?;
                let max = eval(&iter_items[2], env)?.to_integer().ok_or_else(|| {
                    EvalError::TypeError {
                        expected: "Integer".to_string(),
                        got: "non-Integer".to_string(),
                    }
                })?;
                let values: Vec<Value> = (min..=max)
                    .map(|i| Value::Integer(Integer::from(i)))
                    .collect();
                (var, values)
            }
            4 => {
                // {var, min, max, step}
                let var = match &iter_items[0] {
                    Expr::Symbol(s) => s.clone(),
                    _ => {
                        return Err(EvalError::Error(
                            "Table iterator variable must be a symbol".to_string(),
                        ));
                    }
                };
                let min = eval(&iter_items[1], env)?.to_integer().ok_or_else(|| {
                    EvalError::TypeError {
                        expected: "Integer".to_string(),
                        got: "non-Integer".to_string(),
                    }
                })?;
                let max = eval(&iter_items[2], env)?.to_integer().ok_or_else(|| {
                    EvalError::TypeError {
                        expected: "Integer".to_string(),
                        got: "non-Integer".to_string(),
                    }
                })?;
                let step = eval(&iter_items[3], env)?.to_integer().ok_or_else(|| {
                    EvalError::TypeError {
                        expected: "Integer".to_string(),
                        got: "non-Integer".to_string(),
                    }
                })?;
                if step == 0 {
                    return Err(EvalError::Error("Table step cannot be zero".to_string()));
                }
                let mut values = Vec::new();
                if step > 0 {
                    let mut i = min;
                    while i <= max {
                        values.push(Value::Integer(Integer::from(i)));
                        i += step;
                    }
                } else {
                    let mut i = min;
                    while i >= max {
                        values.push(Value::Integer(Integer::from(i)));
                        i += step;
                    }
                }
                (var, values)
            }
            _ => {
                return Err(EvalError::Error(
                    "Table iterator spec must have 2-4 elements".to_string(),
                ));
            }
        };

    // Generate results for this iterator level
    let child_env = env.child();
    let mut result = Vec::new();
    for val in values {
        child_env.set(var_name.clone(), val);
        result.push(eval_table_recursive(
            expr,
            iter_specs,
            &child_env,
            depth + 1,
        )?);
    }

    Ok(Value::List(result))
}

/// Sum[expr, {i, min, max}] — like Table but adds results.
/// Called from eval_call as a special form (iterator spec has unevaluated symbols).
fn eval_sum(args: &[Expr], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Sum requires exactly 2 arguments".to_string(),
        ));
    }
    let iter_items = match &args[1] {
        Expr::List(items) => items,
        _ => {
            return Err(EvalError::Error(
                "Sum iterator spec must be a list".to_string(),
            ));
        }
    };

    let (var_name, min, max) =
        match iter_items.len() {
            3 => {
                let var = match &iter_items[0] {
                    Expr::Symbol(s) => s.clone(),
                    _ => {
                        return Err(EvalError::Error(
                            "Sum iterator variable must be a symbol".to_string(),
                        ));
                    }
                };
                let min = eval(&iter_items[1], env)?.to_integer().ok_or_else(|| {
                    EvalError::TypeError {
                        expected: "Integer".to_string(),
                        got: "non-Integer".to_string(),
                    }
                })?;
                let max = eval(&iter_items[2], env)?.to_integer().ok_or_else(|| {
                    EvalError::TypeError {
                        expected: "Integer".to_string(),
                        got: "non-Integer".to_string(),
                    }
                })?;
                (var, min, max)
            }
            _ => {
                return Err(EvalError::Error(
                    "Sum iterator spec must have 3 elements {i, min, max}".to_string(),
                ));
            }
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

// ── Parallel evaluation builtins ──

/// ParallelMap[f, list] — apply f to each element of list using multiple threads.
fn builtin_parallel_map_eval(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "ParallelMap requires exactly 2 arguments".to_string(),
        ));
    }
    let f = &args[0];
    match &args[1] {
        Value::List(items) if items.is_empty() => Ok(Value::List(vec![])),
        Value::List(items) => {
            // For small lists, sequential is faster than thread overhead
            if items.len() < 4 {
                let mut result = Vec::with_capacity(items.len());
                for item in items {
                    result.push(apply_function(f, &[item.clone()], env)?);
                }
                return Ok(Value::List(result));
            }

            let mut results: Vec<Option<Value>> = vec![None; items.len()];
            std::thread::scope(|s| {
                let handles: Vec<_> = items
                    .iter()
                    .enumerate()
                    .map(|(_i, item)| {
                        let item = item.clone();
                        let f = f.clone();
                        let env = env.clone();
                        s.spawn(move || -> Result<Value, EvalError> {
                            apply_function(&f, &[item], &env)
                        })
                    })
                    .collect();

                for (i, handle) in handles.into_iter().enumerate() {
                    let val = handle.join().map_err(|_| {
                        EvalError::Error("ParallelMap worker panicked".to_string())
                    })??;
                    results[i] = Some(val);
                }
                Ok::<(), EvalError>(())
            })?;

            Ok(Value::List(
                results.into_iter().map(|r| r.unwrap()).collect(),
            ))
        }
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: args[1].type_name().to_string(),
        }),
    }
}

/// ParallelTable[expr, {i, ...}] — evaluate expr for each iterator value in parallel.
fn eval_parallel_table(args: &[Expr], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "ParallelTable requires exactly 2 arguments".to_string(),
        ));
    }

    let expr = &args[0];

    // Parse the iterator spec (reuse the same logic as eval_table)
    let iter_items = match &args[1] {
        Expr::List(items) => items,
        _ => {
            return Err(EvalError::Error(
                "ParallelTable iterator spec must be a list".to_string(),
            ));
        }
    };

    let (var_name, values) =
        match iter_items.len() {
            2 => {
                let var = match &iter_items[0] {
                    Expr::Symbol(s) => s.clone(),
                    _ => {
                        return Err(EvalError::Error(
                            "ParallelTable iterator variable must be a symbol".to_string(),
                        ));
                    }
                };
                if let Expr::List(_) = &iter_items[1] {
                    let list_val = eval(&iter_items[1], env)?;
                    match list_val {
                        Value::List(items) => (var, items),
                        _ => {
                            return Err(EvalError::TypeError {
                                expected: "List".to_string(),
                                got: list_val.type_name().to_string(),
                            });
                        }
                    }
                } else {
                    let n = eval(&iter_items[1], env)?.to_integer().ok_or_else(|| {
                        EvalError::TypeError {
                            expected: "Integer".to_string(),
                            got: "non-Integer".to_string(),
                        }
                    })?;
                    let values: Vec<Value> =
                        (1..=n).map(|i| Value::Integer(Integer::from(i))).collect();
                    (var, values)
                }
            }
            3 => {
                let var = match &iter_items[0] {
                    Expr::Symbol(s) => s.clone(),
                    _ => {
                        return Err(EvalError::Error(
                            "ParallelTable iterator variable must be a symbol".to_string(),
                        ));
                    }
                };
                let min = eval(&iter_items[1], env)?.to_integer().ok_or_else(|| {
                    EvalError::TypeError {
                        expected: "Integer".to_string(),
                        got: "non-Integer".to_string(),
                    }
                })?;
                let max = eval(&iter_items[2], env)?.to_integer().ok_or_else(|| {
                    EvalError::TypeError {
                        expected: "Integer".to_string(),
                        got: "non-Integer".to_string(),
                    }
                })?;
                let values: Vec<Value> = (min..=max)
                    .map(|i| Value::Integer(Integer::from(i)))
                    .collect();
                (var, values)
            }
            4 => {
                let var = match &iter_items[0] {
                    Expr::Symbol(s) => s.clone(),
                    _ => {
                        return Err(EvalError::Error(
                            "ParallelTable iterator variable must be a symbol".to_string(),
                        ));
                    }
                };
                let min = eval(&iter_items[1], env)?.to_integer().ok_or_else(|| {
                    EvalError::TypeError {
                        expected: "Integer".to_string(),
                        got: "non-Integer".to_string(),
                    }
                })?;
                let max = eval(&iter_items[2], env)?.to_integer().ok_or_else(|| {
                    EvalError::TypeError {
                        expected: "Integer".to_string(),
                        got: "non-Integer".to_string(),
                    }
                })?;
                let step = eval(&iter_items[3], env)?.to_integer().ok_or_else(|| {
                    EvalError::TypeError {
                        expected: "Integer".to_string(),
                        got: "non-Integer".to_string(),
                    }
                })?;
                if step == 0 {
                    return Err(EvalError::Error(
                        "ParallelTable step cannot be zero".to_string(),
                    ));
                }
                let mut values = Vec::new();
                if step > 0 {
                    let mut i = min;
                    while i <= max {
                        values.push(Value::Integer(Integer::from(i)));
                        i += step;
                    }
                } else {
                    let mut i = min;
                    while i >= max {
                        values.push(Value::Integer(Integer::from(i)));
                        i += step;
                    }
                }
                (var, values)
            }
            _ => {
                return Err(EvalError::Error(
                    "ParallelTable iterator spec must have 2-4 elements".to_string(),
                ));
            }
        };

    // For small iteration counts, sequential is faster
    if values.len() < 4 {
        let child_env = env.child();
        let mut result = Vec::with_capacity(values.len());
        for val in values {
            child_env.set(var_name.clone(), val);
            result.push(eval(expr, &child_env)?);
        }
        return Ok(Value::List(result));
    }

    // Parallel evaluation using scoped threads
    let mut results: Vec<Option<Value>> = vec![None; values.len()];
    std::thread::scope(|s| {
        let handles: Vec<_> = values
            .into_iter()
            .map(|val| {
                let expr = expr.clone();
                let var_name = var_name.clone();
                let env = env.clone();
                s.spawn(move || -> Result<Value, EvalError> {
                    let child_env = env.child();
                    child_env.set(var_name, val);
                    eval(&expr, &child_env)
                })
            })
            .collect();

        for (i, handle) in handles.into_iter().enumerate() {
            let val = handle
                .join()
                .map_err(|_| EvalError::Error("ParallelTable worker panicked".to_string()))??;
            results[i] = Some(val);
        }
        Ok::<(), EvalError>(())
    })?;

    Ok(Value::List(
        results.into_iter().map(|r| r.unwrap()).collect(),
    ))
}

/// FixedPoint[f, x] — apply f until result stops changing.
fn builtin_fixed_point_eval(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(EvalError::Error(
            "FixedPoint requires 2 or 3 arguments".to_string(),
        ));
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

/// Evaluate `?expr` — display help information for a symbol.
fn eval_information(expr: &Expr, env: &Env) -> Result<Value, EvalError> {
    match expr {
        Expr::Symbol(s) => {
            // 1. Check if the symbol has a user-defined value
            if let Some(val) = env.get(s) {
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
/// Searches `env.search_paths` for `{Name}.syma` (then `{name}.syma`).
/// The file must contain a top-level `module <Name> { ... }` definition.
fn load_module_from_file(name: &str, env: &Env) -> Result<Value, EvalError> {
    use crate::{lexer, parser};

    // Candidates: exact case first, then lowercase fallback.
    let candidates = [
        format!("{}.syma", name),
        format!("{}.syma", name.to_lowercase()),
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

    env.get_module(name).ok_or_else(|| {
        EvalError::Error(format!(
            "File '{}.syma' was loaded but did not define a module named '{}'.\n\
             Tip: wrap its contents in `module {} {{ export ...; ... }}`",
            name, name, name
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtins;
    use crate::lexer;
    use crate::parser;

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
}
