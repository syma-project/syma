//! Symbol/context introspection: `Names`, `Symbol`, `SymbolName`, `Symbols`,
//! `Contexts`, plus the `$Context`/`$ContextPath` session variables.
//!
//! These builtins expose the session's symbol table and context stack. They
//! are deliberately conservative: any argument that is not applicable is
//! returned unevaluated as a `Value::Call`, matching Wolfram Language
//! symbolic behaviour.
//!
//! `Names` patterns use the Wolfram metacharacters `*` (zero or more chars),
//! `@` (one or more chars) and `?` (exactly one char). A list of patterns
//! matches names matching any of them.

use std::collections::BTreeSet;

use crate::ast::Expr;
use crate::env::Env;
use crate::value::{EvalError, Value};

/// Build an unevaluated call, unwrapping held `Value::Pattern` args so the
/// result looks like ordinary symbolic output.
fn unevaluated(head: &str, args: &[Value]) -> Value {
    Value::Call {
        head: head.to_string(),
        args: args
            .iter()
            .map(|a| match a {
                Value::Pattern(e) => crate::pattern::unwrap_expr_to_value(e),
                other => other.clone(),
            })
            .collect(),
    }
}

/// Extract a symbol/string name from an argument that may arrive as a bare
/// symbol/string or a held `Value::Pattern`.
fn string_arg(arg: &Value) -> Option<String> {
    match arg {
        Value::Symbol(s) | Value::Str(s) => Some(s.clone()),
        Value::Pattern(Expr::Symbol(s)) | Value::Pattern(Expr::Str(s)) => Some(s.clone()),
        _ => None,
    }
}

/// Every symbol name known to the session: scope bindings, lazy providers,
/// attribute entries (including the empty entry `Clear` leaves behind),
/// upvalue tags, subvalue heads, and registered options.
fn all_symbol_names(env: &Env) -> BTreeSet<String> {
    let mut names = BTreeSet::new();

    for (name, _) in env.all_bindings() {
        names.insert(name);
    }
    for name in env.lazy_providers.lock().unwrap().keys() {
        names.insert(name.clone());
    }
    for name in env.attributes.lock().unwrap().keys() {
        names.insert(name.clone());
    }
    for tag in env.upvalues.lock().unwrap().keys() {
        names.insert(tag.clone());
    }
    for head in env.subvalues.lock().unwrap().keys() {
        names.insert(head.clone());
    }
    for name in env.options.lock().unwrap().keys() {
        names.insert(name.clone());
    }

    names
}

/// A symbol is "defined" when it has a value/definition or a non-empty
/// attribute set. A bare self-symbol binding (`x` → `x`) does not count, and
/// neither does the empty attribute entry that `Clear` leaves behind.
fn is_defined(env: &Env, name: &str) -> bool {
    if !env.get_attributes(name).is_empty() {
        return true;
    }
    match env.get(name) {
        Some(Value::Symbol(s)) if s == name => false,
        Some(_) => true,
        None => false,
    }
}

/// Glob-style matcher for `Names` patterns. `*` matches zero or more
/// characters, `@` matches one or more, and `?` matches exactly one.
fn name_match(pattern: &str, text: &str) -> bool {
    let pat: Vec<char> = pattern.chars().collect();
    let txt: Vec<char> = text.chars().collect();
    name_match_chars(&pat, &txt)
}

fn name_match_chars(pat: &[char], txt: &[char]) -> bool {
    match (pat.first(), txt.first()) {
        (None, None) => true,
        (Some('*'), _) => {
            name_match_chars(&pat[1..], txt)
                || (!txt.is_empty() && name_match_chars(pat, &txt[1..]))
        }
        // `@` is one-or-more: consume a character, then either advance the
        // pattern (stopping) or keep matching (continuing).
        (Some('@'), Some(_)) => {
            name_match_chars(&pat[1..], &txt[1..]) || name_match_chars(pat, &txt[1..])
        }
        (Some('?'), Some(_)) => name_match_chars(&pat[1..], &txt[1..]),
        (Some(p), Some(t)) if p == t => name_match_chars(&pat[1..], &txt[1..]),
        _ => false,
    }
}

/// Extract one or more patterns from a `Names`/`Symbols`/`Contexts` argument.
/// `None` means the argument was inapplicable.
fn extract_patterns(arg: &Value) -> Option<Vec<String>> {
    match arg {
        Value::Str(s) => Some(vec![s.clone()]),
        Value::Pattern(Expr::Str(s)) => Some(vec![s.clone()]),
        Value::List(items) => {
            let mut patterns = Vec::with_capacity(items.len());
            for item in items {
                match item {
                    Value::Str(s) => patterns.push(s.clone()),
                    Value::Pattern(Expr::Str(s)) => patterns.push(s.clone()),
                    _ => return None,
                }
            }
            Some(patterns)
        }
        _ => None,
    }
}

/// Read `IgnoreCase -> True/False` option rules from trailing arguments.
fn ignore_case(args: &[Value]) -> bool {
    let mut flag = false;
    for arg in args.iter().skip(1) {
        let items: Vec<&Value> = match arg {
            Value::List(v) => v.iter().collect(),
            other => vec![other],
        };
        for item in items {
            if let Value::Rule { lhs, rhs, .. } = item
                && string_arg(lhs).as_deref() == Some("IgnoreCase")
            {
                match rhs.as_ref() {
                    Value::Bool(b) => flag = *b,
                    Value::Symbol(s) => flag = s == "True",
                    _ => {}
                }
            }
        }
    }
    flag
}

/// Test a name against any of the patterns, optionally case-insensitively.
fn matches_any(patterns: &[String], name: &str, case_insensitive: bool) -> bool {
    if case_insensitive {
        let lower = name.to_lowercase();
        patterns
            .iter()
            .any(|p| name_match(&p.to_lowercase(), &lower))
    } else {
        patterns.iter().any(|p| name_match(p, name))
    }
}

/// `Names[]` — all known symbol names, sorted.
/// `Names[pattern]` / `Names[{p1, p2}]` — names matching a pattern.
pub fn builtin_names(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    let all = all_symbol_names(env);

    if args.is_empty() {
        return Ok(Value::List(all.into_iter().map(Value::Str).collect()));
    }

    let Some(patterns) = extract_patterns(&args[0]) else {
        return Ok(unevaluated("Names", args));
    };
    let ci = ignore_case(args);
    let result = all
        .into_iter()
        .filter(|name| matches_any(&patterns, name, ci))
        .map(Value::Str)
        .collect();
    Ok(Value::List(result))
}

/// `Symbol["name"]` — create a symbol from a string. Threads over lists.
pub fn builtin_symbol(args: &[Value]) -> Result<Value, EvalError> {
    let Some(name) = args.first().and_then(string_arg) else {
        return Ok(unevaluated("Symbol", args));
    };
    Ok(Value::Symbol(name))
}

/// `SymbolName[sym]` — the name of a symbol, without its context.
pub fn builtin_symbol_name(args: &[Value]) -> Result<Value, EvalError> {
    let Some(name) = args.first().and_then(string_arg) else {
        return Ok(unevaluated("SymbolName", args));
    };
    Ok(Value::Str(base_name(&name)))
}

/// `Symbols[]` — the symbols that currently carry a value or definition.
/// `Symbols[pattern]` — only matching symbols.
pub fn builtin_symbols(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    let defined: Vec<String> = all_symbol_names(env)
        .into_iter()
        .filter(|name| is_defined(env, name))
        .collect();

    if args.is_empty() {
        return Ok(Value::List(
            defined.into_iter().map(Value::Symbol).collect(),
        ));
    }

    let Some(patterns) = extract_patterns(&args[0]) else {
        return Ok(unevaluated("Symbols", args));
    };
    let ci = ignore_case(args);
    let result = defined
        .into_iter()
        .filter(|name| matches_any(&patterns, name, ci))
        .map(Value::Symbol)
        .collect();
    Ok(Value::List(result))
}

/// `Contexts[]` — all contexts that appear in the session, sorted.
/// `Contexts[pattern]` — only matching contexts.
pub fn builtin_contexts(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    let mut contexts: BTreeSet<String> = BTreeSet::new();

    // Contexts on the (live) stack.
    for ctx in env.context_stack.lock().unwrap().iter() {
        contexts.insert(ctx.clone());
    }
    // Contexts named on the search path.
    if let Some(Value::List(path)) = env.get("$ContextPath") {
        for entry in path {
            if let Value::Str(ctx) = entry {
                contexts.insert(ctx.clone());
            }
        }
    }
    // Contexts inferred from fully-qualified symbol names.
    for name in all_symbol_names(env) {
        if let Some(idx) = name.rfind('`') {
            contexts.insert(name[..=idx].to_string());
        }
    }
    // Base contexts are always present in a Wolfram session.
    contexts.insert("Global`".to_string());
    contexts.insert("System`".to_string());

    if args.is_empty() {
        return Ok(Value::List(contexts.into_iter().map(Value::Str).collect()));
    }

    let Some(patterns) = extract_patterns(&args[0]) else {
        return Ok(unevaluated("Contexts", args));
    };
    let ci = ignore_case(args);
    let result = contexts
        .into_iter()
        .filter(|ctx| matches_any(&patterns, ctx, ci))
        .map(Value::Str)
        .collect();
    Ok(Value::List(result))
}

/// The final segment of a (possibly context-qualified) symbol name.
fn base_name(name: &str) -> String {
    match name.rfind('`') {
        Some(idx) => name[idx + 1..].to_string(),
        None => name.to_string(),
    }
}

/// Register the builtins implemented in this module.
pub fn register(env: &Env) {
    super::register_builtin_env(env, "Names", builtin_names);
    super::register_builtin_env(env, "Symbols", builtin_symbols);
    super::register_builtin_env(env, "Contexts", builtin_contexts);
    super::register_builtin(env, "SymbolName", builtin_symbol_name);
    super::register_builtin(env, "Symbol", builtin_symbol);

    // `Names`/`Symbols`/`Contexts` must not evaluate their argument (it is a
    // pattern string); `SymbolName` holds its argument so an assigned symbol
    // still yields its name. `Symbol` threads over lists.
    for name in ["Names", "Symbols", "Contexts"] {
        env.set_attributes(
            name,
            vec!["Locked".to_string(), "ReadProtected".to_string()],
        );
    }
    env.set_attributes(
        "SymbolName",
        vec![
            "HoldAll".to_string(),
            "Locked".to_string(),
            "ReadProtected".to_string(),
        ],
    );
    env.set_attributes(
        "Symbol",
        vec!["Listable".to_string(), "Protected".to_string()],
    );

    // Session context variables. `Begin`/`End` keep `$Context` in sync; the
    // baseline value here ensures it is defined before any context is pushed.
    env.set("$Context".to_string(), Value::Str(env.current_context()));
    env.set(
        "$ContextPath".to_string(),
        Value::List(vec![
            Value::Str("System`".to_string()),
            Value::Str("Global`".to_string()),
        ]),
    );
    env.set_attributes("$Context", vec!["Protected".to_string()]);
    env.set_attributes("$ContextPath", vec!["Protected".to_string()]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtins;
    use crate::lexer;
    use crate::parser;

    fn eval_str(input: &str) -> Value {
        std::thread::Builder::new()
            .stack_size(16 * 1024 * 1024)
            .spawn({
                let input = input.to_string();
                move || {
                    let env = Env::new();
                    builtins::register_builtins(&env);
                    let tokens = lexer::tokenize(&input).unwrap();
                    let ast = parser::parse(tokens).unwrap();
                    crate::eval::eval_program(&ast, &env).unwrap()
                }
            })
            .unwrap()
            .join()
            .unwrap()
    }

    fn names_of(env: &Env, pattern: &Value) -> Vec<String> {
        match builtin_names(std::slice::from_ref(pattern), env).unwrap() {
            Value::List(items) => items
                .into_iter()
                .filter_map(|v| match v {
                    Value::Str(s) => Some(s),
                    _ => None,
                })
                .collect(),
            _ => vec![],
        }
    }

    // ── Names pattern semantics ──────────────────────────────────────────

    #[test]
    fn test_names_filters_by_glob() {
        let env = Env::new();
        env.set("alpha".to_string(), Value::Null);
        env.set("beta".to_string(), Value::Null);
        env.set("alphabet".to_string(), Value::Null);

        let matched = names_of(&env, &Value::Str("alpha*".to_string()));
        assert_eq!(matched, vec!["alpha".to_string(), "alphabet".to_string()]);
    }

    #[test]
    fn test_names_single_char_wildcard() {
        let env = Env::new();
        env.set("ab".to_string(), Value::Null);
        env.set("abc".to_string(), Value::Null);
        let matched = names_of(&env, &Value::Str("a?".to_string()));
        assert_eq!(matched, vec!["ab".to_string()]);
    }

    #[test]
    fn test_names_at_least_one_wildcard() {
        let env = Env::new();
        env.set("a".to_string(), Value::Null);
        env.set("ab".to_string(), Value::Null);
        env.set("abc".to_string(), Value::Null);
        // `@` requires at least one following character.
        let matched = names_of(&env, &Value::Str("a@".to_string()));
        assert_eq!(matched, vec!["ab".to_string(), "abc".to_string()]);
        let anchored = names_of(&env, &Value::Str("a@c".to_string()));
        assert_eq!(anchored, vec!["abc".to_string()]);
    }

    #[test]
    fn test_names_list_of_patterns() {
        let env = Env::new();
        env.set("alpha".to_string(), Value::Null);
        env.set("beta".to_string(), Value::Null);
        env.set("gamma".to_string(), Value::Null);
        let pattern = Value::List(vec![
            Value::Str("alpha".to_string()),
            Value::Str("beta".to_string()),
        ]);
        let matched = names_of(&env, &pattern);
        assert_eq!(matched, vec!["alpha".to_string(), "beta".to_string()]);
    }

    #[test]
    fn test_names_ignore_case_option() {
        let env = Env::new();
        env.set("Alpha".to_string(), Value::Null);
        env.set("alpha".to_string(), Value::Null);
        let rule = Value::Rule {
            lhs: Box::new(Value::Symbol("IgnoreCase".to_string())),
            rhs: Box::new(Value::Bool(true)),
            delayed: false,
        };
        // Without the option an uppercase pattern matches neither name.
        assert!(names_of(&env, &Value::Str("ALPHA".to_string())).is_empty());
        // With `IgnoreCase -> True` both names match.
        let matched = match builtin_names(&[Value::Str("ALPHA".to_string()), rule], &env).unwrap() {
            Value::List(items) => items.len(),
            other => panic!("expected list, got {other}"),
        };
        assert_eq!(matched, 2);
    }

    #[test]
    fn test_names_removed_symbol_disappears() {
        let env = Env::new();
        env.set("zeta".to_string(), Value::Null);
        env.set_attributes("zeta", vec!["Listable".to_string()]);
        // Remove drops both binding and attribute entry.
        env.remove("zeta");
        env.attributes.lock().unwrap().remove("zeta");
        assert!(names_of(&env, &Value::Str("zeta".to_string())).is_empty());
    }

    #[test]
    fn test_names_bad_pattern_type_is_unevaluated() {
        let env = Env::new();
        let out = builtin_names(&[Value::Integer(rug::Integer::from(1))], &env).unwrap();
        assert_eq!(
            out,
            Value::Call {
                head: "Names".to_string(),
                args: vec![Value::Integer(rug::Integer::from(1))],
            }
        );
    }

    // ── Names / Remove / Clear interplay (end-to-end) ────────────────────

    #[test]
    fn test_clear_keeps_name_remove_drops_it() {
        // `Clear` leaves the symbol visible to `Names` (empty attribute entry);
        // `Remove` erases it entirely.
        assert_eq!(eval_str("x = 5; Length[Names[\"x\"]]"), val_int(1));
        assert_eq!(
            eval_str("x = 5; Clear[x]; Length[Names[\"x\"]]"),
            val_int(1)
        );
        assert_eq!(
            eval_str("x = 5; Remove[x]; Length[Names[\"x\"]]"),
            val_int(0)
        );
    }

    #[test]
    fn test_symbols_tracks_definitions() {
        assert_eq!(
            eval_str("x = 5; MemberQ[Symbols[], Symbol[\"x\"]]"),
            Value::Bool(true)
        );
        // `Clear` undefines x, so it leaves Symbols.
        assert_eq!(
            eval_str("x = 5; Clear[x]; MemberQ[Symbols[], Symbol[\"x\"]]"),
            Value::Bool(false)
        );
        // `Remove` erases it completely.
        assert_eq!(
            eval_str("x = 5; Remove[x]; MemberQ[Symbols[], Symbol[\"x\"]]"),
            Value::Bool(false)
        );
    }

    // ── Symbol / SymbolName ──────────────────────────────────────────────

    #[test]
    fn test_symbol_creates_symbol() {
        assert_eq!(
            eval_str("Symbol[\"abc\"]"),
            Value::Symbol("abc".to_string())
        );
        assert_eq!(
            eval_str("Symbol[{\"a\", \"b\"}]"),
            Value::List(vec![
                Value::Symbol("a".to_string()),
                Value::Symbol("b".to_string())
            ])
        );
    }

    #[test]
    fn test_symbol_inapplicable_unevaluated() {
        assert_eq!(
            eval_str("Symbol[5]"),
            Value::Call {
                head: "Symbol".to_string(),
                args: vec![Value::Integer(rug::Integer::from(5))],
            }
        );
    }

    #[test]
    fn test_symbol_name_round_trip() {
        assert_eq!(
            eval_str("SymbolName[Symbol[\"abc\"]]"),
            Value::Str("abc".to_string())
        );
        assert_eq!(eval_str("SymbolName[x]"), Value::Str("x".to_string()));
        // Assigned symbols still yield their own name (HoldAll).
        assert_eq!(
            eval_str("x = 5; SymbolName[x]"),
            Value::Str("x".to_string())
        );
        // Context is stripped.
        assert_eq!(
            eval_str("SymbolName[\"Foo`bar\"]"),
            Value::Str("bar".to_string())
        );
    }

    #[test]
    fn test_symbol_name_inapplicable_unevaluated() {
        assert_eq!(
            eval_str("SymbolName[5]"),
            Value::Call {
                head: "SymbolName".to_string(),
                args: vec![Value::Integer(rug::Integer::from(5))],
            }
        );
    }

    // ── ToExpression round-trip ──────────────────────────────────────────

    #[test]
    fn test_to_expression_round_trip_symbol() {
        assert_eq!(
            eval_str("ToExpression[\"abc\"]"),
            Value::Symbol("abc".to_string())
        );
        assert_eq!(
            eval_str("SymbolName[ToExpression[\"abc\"]]"),
            Value::Str("abc".to_string())
        );
    }

    #[test]
    fn test_to_expression_round_trip_expression() {
        assert_eq!(
            eval_str("ToExpression[\"1 + 2\"]"),
            Value::Integer(rug::Integer::from(3))
        );
        assert_eq!(eval_str("ToExpression[ToString[a + b]]"), eval_str("a + b"));
    }

    // ── Contexts / Context / $Context / $ContextPath ─────────────────────

    #[test]
    fn test_contexts_includes_base_and_pushed() {
        assert_eq!(
            eval_str("MemberQ[Contexts[], \"Global`\"]"),
            Value::Bool(true)
        );
        assert_eq!(
            eval_str("Begin[\"Foo`\"]; MemberQ[Contexts[], \"Foo`\"]"),
            Value::Bool(true)
        );
        assert_eq!(
            eval_str("Begin[\"Foo`\"]; End[]; MemberQ[Contexts[], \"Foo`\"]"),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_context_consistency() {
        assert_eq!(eval_str("Context[x]"), Value::Str("Global`".to_string()));
        assert_eq!(
            eval_str("Context[Symbol[\"Foo`y\"]]"),
            Value::Str("Foo`".to_string())
        );
        // Context tracks Begin/End and is restored on End.
        assert_eq!(
            eval_str("Begin[\"Foo`\"]; Context[x]"),
            Value::Str("Foo`".to_string())
        );
        assert_eq!(
            eval_str("Begin[\"Foo`\"]; End[]; Context[x]"),
            Value::Str("Global`".to_string())
        );
    }

    #[test]
    fn test_context_variables() {
        assert_eq!(eval_str("$Context"), Value::Str("Global`".to_string()));
        assert_eq!(
            eval_str("Begin[\"Foo`\"]; $Context"),
            Value::Str("Foo`".to_string())
        );
        assert_eq!(
            eval_str("Begin[\"Foo`\"]; End[]; $Context"),
            Value::Str("Global`".to_string())
        );
        assert_eq!(
            eval_str("MemberQ[$ContextPath, \"Global`\"]"),
            Value::Bool(true)
        );
    }

    fn val_int(n: i32) -> Value {
        Value::Integer(rug::Integer::from(n))
    }
}
