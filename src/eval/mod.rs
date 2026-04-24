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
use crate::pattern::{Bindings, MatchResult, collect_nested_guards, match_pattern};
use crate::profiler;
use crate::value::*;

pub(crate) mod rules;
pub(crate) mod numeric;
pub(crate) mod table;
pub(crate) mod plot;

/// Evaluate a program (list of statements) in the given environment.
pub fn eval_program(stmts: &[Expr], env: &Env) -> Result<Value, EvalError> {
    let mut result = Value::Null;
    for stmt in stmts {
        result = eval(stmt, env)?;
    }
    Ok(result)
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
            Ok(Value::List(flatten_sequences(values?)))
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
            rules::apply_rules_value(&val, &rules_val, env)        }

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
                if let MatchResult::Match(_) = match_pattern(&rules::pat_to_expr(&pat_val), &val) {
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
            });

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
                    env.set(s.clone(), val.clone());
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
                        Value::Str(s) => {
                            crate::builtins::localsymbol::write_local_symbol(&s, &val)
                        }
                        _ => Err(EvalError::Error(
                            "LocalSymbol requires a string name".to_string(),
                        )),
                    }
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
                                if let Expr::Symbol(s) = &call_args[0]
                                    && s == "this"
                                {
                                    env.set("this".to_string(), updated.clone());
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
pub(super) fn extract_guard_expr(expr: &Expr) -> (&Expr, Option<&Expr>) {
    match expr {
        Expr::PatternGuard { pattern, condition } => (pattern.as_ref(), Some(condition.as_ref())),
        _ => (expr, None),
    }
}

/// Check if an expression looks like a pattern that should not be evaluated.
pub(super) fn is_pattern_like(expr: &Expr) -> bool {
    match expr {
        Expr::Symbol(s) => s.ends_with('_') || s == "_",
        Expr::Blank { .. }
        | Expr::NamedBlank { .. }
        | Expr::BlankSequence { .. }
        | Expr::BlankNullSequence { .. }
        | Expr::OptionalBlank { .. }
        | Expr::OptionalNamedBlank { .. }
        | Expr::PatternGuard { .. } => true,
        _ => false,
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
        let hold_all = env.has_attribute(name, "HoldAll");
        let hold_first = env.has_attribute(name, "HoldFirst");
        let hold_rest = env.has_attribute(name, "HoldRest");

        if hold_all {
            return Ok(args.iter().map(|a| Value::Pattern(a.clone())).collect());
        }
        if hold_first {
            let mut vals = Vec::with_capacity(args.len());
            if let Some(first) = args.first() {
                vals.push(Value::Pattern(first.clone()));
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
                vals.push(Value::Pattern(rest.clone()));
            }
            return Ok(vals);
        }
    }

    // Default: evaluate all arguments
    args.iter().map(|a| eval(a, env)).collect()
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
                                    if let Expr::Symbol(s) = &call_args[0]
                                        && s == "this"
                                    {
                                        env.set("this".to_string(), updated.clone());
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
            "Sum" => {
                // Sum[expr, {i, min, max}] — iterator spec has unevaluated symbols
                table::eval_sum(args, env)
            }
            "Plot" => {
                // Plot[f, {x, xmin, xmax}] — needs unevaluated expr for sampling
                plot::eval_plot(args, env)
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
            _ => {
                let head_val = eval(head, env)?;
                let arg_vals = eval_args_with_attributes(head, args, &head_val, env)?;

                apply_function(&head_val, &flatten_sequences(arg_vals), env)
            }
        }
    } else {
        let head_val = eval(head, env)?;
        let arg_vals = eval_args_with_attributes(head, args, &head_val, env)?;
        apply_function(&head_val, &flatten_sequences(arg_vals), env)
    }
}

/// Apply a function value to arguments.
pub(crate) fn apply_function(func: &Value, args: &[Value], env: &Env) -> Result<Value, EvalError> {
    // ── Listable attribute: auto-thread over lists ──
    let func_name = match func {
        Value::Builtin(name, _) => Some(name.as_str()),
        Value::Function(fd) => Some(fd.name.as_str()),
        Value::Symbol(s) => Some(s.as_str()),
        _ => None,
    };
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
                            result.push(apply_function(func, &thread_args, env)?);
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
                        result.push(apply_function(func, &thread_args, env)?);
                    }
                    return Ok(Value::List(result));
                }
            }
        }
    }

    match func {
        Value::Builtin(name, f) => match f {
            BuiltinFn::Pure(f) => {
                // Extension-registered builtin: trampoline through ext registry
                if std::ptr::fn_addr_eq(
                    *f,
                    ffi::extension::EXT_DISPATCH_PTR,
                ) {
                    return ffi::extension::call_ext_fn(name, args);
                }
                f(args).map_err(|e| match e {
                    EvalError::NoMatch { .. } => EvalError::NoMatch {
                        head: name.clone(),
                        args: args.to_vec(),
                    },
                    other => other,
                })
            }
            BuiltinFn::Env(f) => f(args, env),
        },

        Value::NativeFunction {
            fn_ptr, signature, ..
        } => ffi::loader::call_native(*fn_ptr, signature, args),

        Value::BytecodeFunction(bc_def) => {
            bytecode::vm::execute_bytecode(&bc_def.bytecode, args, env)
        }

        Value::Function(func_def) => {
            // ── Hotness check: compile to bytecode if frequently called ──
            if profiler::Profiler::check_hot(&func_def.name) {
                if let Ok(bc) = bytecode::compiler::BytecodeCompiler::compile_multi(
                    &func_def.definitions,
                    &func_def.name,
                ) {
                    let bc_val = Value::BytecodeFunction(std::sync::Arc::new(
                        crate::bytecode::BytecodeFunctionDef {
                            name: func_def.name.clone(),
                            bytecode: bc,
                            call_count: std::sync::Arc::new(
                                std::sync::atomic::AtomicU64::new(0),
                            ),
                        },
                    ));
                    env.set(func_def.name.clone(), bc_val.clone());
                    profiler::Profiler::reset(&func_def.name);
                    return apply_function(&bc_val, args, env);
                }
                // If compilation fails, reset counter and fall back to tree-walk
                profiler::Profiler::reset(&func_def.name);
            }

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
///
/// Handles sequence patterns (__ / ___) in function parameters using backtracking:
///   f[x__] := Total[x]             — sequence of 1+ elements
///   f[x___] := {x}                 — sequence of 0+ elements
///   f[a_, b__, c_] := {a, b, c}    — mixed fixed and sequence parameters
fn try_match_params(
    params: &[Expr],
    args: &[Value],
    env: &Env,
) -> Result<Option<Bindings>, EvalError> {
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

        match match_pattern(&list_pattern, &list_value) {
            MatchResult::Match(bindings) => {
                // Evaluate guards with the collected bindings
                for guard in &guard_exprs {
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
            MatchResult::NoMatch => Ok(None),
        }
    } else {
        // Fast path: no sequence patterns, direct matching
        if params.len() != args.len() {
            return Ok(None);
        }

        let mut bindings = HashMap::new();

        for (param, arg) in params.iter().zip(args.iter()) {
            let (inner_pat, guard) = extract_guard_expr(param);
            match match_pattern(inner_pat, arg) {
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
            if !eval(guard, &guard_env)?.to_bool() {
                return Ok(None);
            }
        }

        Ok(Some(bindings))
    }
}

/// Flatten Sequence values into the surrounding list or call arguments.
/// In Wolfram Language semantics, Sequence[...] automatically splats.
fn flatten_sequences(items: Vec<Value>) -> Vec<Value> {
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
        assert_eq!(
            eval_str("f[x___] := {x}; f[]"),
            Value::List(vec![])
        );
        assert_eq!(
            eval_str("f[x___] := {x}; f[1, 2]"),
            Value::List(vec![
                Value::Integer(Integer::from(1)),
                Value::Integer(Integer::from(2)),
            ])
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
        assert_eq!(
            result,
            Value::List(vec![Value::Integer(Integer::from(3))])
        );
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
        assert_eq!(eval_str("And[False, Error[\"should not fire\"]]"), Value::Bool(false));
    }

    #[test]
    fn test_or_short_circuit() {
        // Or should short-circuit: True || (error) should not evaluate the error
        assert_eq!(eval_str("Or[True, Error[\"should not fire\"]]"), Value::Bool(true));
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
            Value::List(vec![
                Value::Bool(false),
                Value::Bool(true),
            ])
        );
    }
}
