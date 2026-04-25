use std::collections::BTreeSet;

use crate::env::Env;
use crate::value::{EvalError, Value};

/// Names[] — list symbol names matching a pattern.
///
/// Names[] returns all known symbol names as a sorted list of strings.
/// Names["pattern"] returns symbol names matching a glob pattern
/// where * matches any sequence of characters and ? matches any single character.
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
        let pattern = match &args[0] {
            Value::Str(s) => s.clone(),
            other => return Err(EvalError::TypeError {
                expected: "String".to_string(),
                got: other.type_name().to_string(),
            }),
        };
        all.into_iter()
            .filter(|name| super::string::glob_match(&pattern, name))
            .map(Value::Str)
            .collect()
    };

    Ok(Value::List(result))
}
