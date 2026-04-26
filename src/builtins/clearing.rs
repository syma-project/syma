use crate::env::Env;
use crate::value::{EvalError, Value};

/// `Clear[sym1, sym2, ...]` — removes definitions, values, and attributes
/// for each symbol. Does nothing on protected symbols.
pub fn builtin_clear(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    for arg in args {
        let sym = match arg {
            Value::Symbol(s) | Value::Builtin(s, _) => s.clone(),
            Value::Str(s) => s.clone(),
            _ => continue,
        };
        if env.has_attribute(&sym, "Protected") {
            continue;
        }
        env.remove(&sym);
        env.clear_attributes(&sym);
    }
    Ok(Value::Null)
}

/// `ClearAll[sym1, sym2, ...]` — removes definitions, values, attributes,
/// and lazy providers for each symbol. Does nothing on protected symbols.
pub fn builtin_clear_all(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    for arg in args {
        let sym = match arg {
            Value::Symbol(s) | Value::Builtin(s, _) => s.clone(),
            Value::Str(s) => s.clone(),
            _ => continue,
        };
        if env.has_attribute(&sym, "Protected") {
            continue;
        }
        env.remove(&sym);
        env.clear_attributes(&sym);
        // Remove lazy provider so symbol can be re-loaded
        {
            let mut providers = env.lazy_providers.lock().unwrap();
            providers.remove(&sym);
        }
    }
    Ok(Value::Null)
}

/// `Unset[sym]` — removes the value/definition for a symbol.
/// Does not clear attributes. Returns Null even if symbol had no definition.
pub fn builtin_unset(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    for arg in args {
        let sym = match arg {
            Value::Symbol(s) | Value::Builtin(s, _) => s.clone(),
            Value::Str(s) => s.clone(),
            _ => continue,
        };
        if env.has_attribute(&sym, "Protected") {
            continue;
        }
        env.remove(&sym);
    }
    Ok(Value::Null)
}

/// `Remove[sym1, sym2, ...]` — completely removes symbols from the system,
/// including bindings, attributes, and lazy providers. Bypasses Protected
/// check (removes regardless).
pub fn builtin_remove(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    for arg in args {
        let sym = match arg {
            Value::Symbol(s) | Value::Builtin(s, _) => s.clone(),
            Value::Str(s) => s.clone(),
            _ => continue,
        };
        env.remove(&sym);
        env.clear_attributes(&sym);
        {
            let mut providers = env.lazy_providers.lock().unwrap();
            providers.remove(&sym);
        }
    }
    Ok(Value::Null)
}
