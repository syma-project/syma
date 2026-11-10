use std::collections::BTreeSet;

use crate::env::Env;
use crate::value::{EvalError, Value};

/// Names[] — list symbol names matching a pattern.
///
/// Names[] returns all known symbol names as a sorted list of strings.
/// Names["pattern"] returns symbol names matching a glob pattern
/// where * matches any sequence of characters and ? matches any single
/// character. A list of patterns matches names matching any of them.
pub fn builtin_names(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    // Collect all symbol names from scope chain, lazy providers, and attributes
    let mut names = BTreeSet::new();

    // 1. Scope bindings (builtins + user-defined)
    for (name, _) in env.all_bindings() {
        names.insert(name);
    }

    // 2. Lazy providers
    {
        let providers = env.lazy_providers.lock().unwrap();
        for name in providers.keys() {
            names.insert(name.clone());
        }
    }

    // 3. Attributes map (symbols with attributes but potentially no binding)
    {
        let attrs = env.attributes.lock().unwrap();
        for name in attrs.keys() {
            names.insert(name.clone());
        }
    }

    let all: Vec<String> = names.into_iter().collect();

    let result: Vec<Value> = if args.is_empty() {
        all.into_iter().map(Value::Str).collect()
    } else {
        let patterns = extract_patterns(&args[0])?;
        all.into_iter()
            .filter(|name| {
                patterns
                    .iter()
                    .any(|pattern| super::string::glob_match(pattern, name))
            })
            .map(Value::Str)
            .collect()
    };

    Ok(Value::List(result))
}

/// Extract one or more glob patterns from a `Names` argument.
fn extract_patterns(arg: &Value) -> Result<Vec<String>, EvalError> {
    match arg {
        Value::Str(s) => Ok(vec![s.clone()]),
        Value::List(items) => {
            let mut patterns = Vec::with_capacity(items.len());
            for item in items {
                match item {
                    Value::Str(s) => patterns.push(s.clone()),
                    other => {
                        return Err(EvalError::TypeError {
                            expected: "String".to_string(),
                            got: other.type_name().to_string(),
                        });
                    }
                }
            }
            Ok(patterns)
        }
        other => Err(EvalError::TypeError {
            expected: "String or List of Strings".to_string(),
            got: other.type_name().to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_names_bad_pattern_type() {
        let env = Env::new();
        assert!(builtin_names(&[Value::Integer(rug::Integer::from(1))], &env).is_err());
    }
}
