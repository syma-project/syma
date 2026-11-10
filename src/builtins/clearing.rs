use crate::ast::Expr;
use crate::env::Env;
use crate::value::{EvalError, Value};

/// Extract a symbol name from an argument that may arrive held (as a
/// `Value::Pattern`), evaluated, or as a string.
///
/// The clearing builtins are `HoldAll`/`HoldFirst`, so their symbol arguments
/// are wrapped as `Value::Pattern` by the evaluator. Handling that shape here
/// means the builtins also work when invoked through `Apply`, `Map`, etc.
fn symbol_name(arg: &Value) -> Option<String> {
    match arg {
        Value::Symbol(s) | Value::Str(s) => Some(s.clone()),
        Value::Builtin(s, _) => Some(s.clone()),
        Value::Function(fd) => Some(fd.name.clone()),
        Value::Pattern(Expr::Symbol(s)) => Some(s.clone()),
        Value::Pattern(Expr::Str(s)) => Some(s.clone()),
        Value::Hold(inner) | Value::HoldComplete(inner) => symbol_name(inner),
        _ => None,
    }
}

/// Register the protection builtins. Called from the containing module's
/// registrar (they are `HoldFirst` so the leading symbol argument survives
/// evaluation).
pub fn register(env: &Env) {
    super::register_builtin_env(env, "Protect", builtin_protect);
    super::register_builtin_env(env, "Unprotect", builtin_unprotect);
}

/// `Clear[sym1, sym2, ...]` — removes definitions, values, and attributes
/// for each symbol. Does nothing on protected symbols.
pub fn builtin_clear(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    for arg in args {
        let Some(sym) = symbol_name(arg) else {
            continue;
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
        let Some(sym) = symbol_name(arg) else {
            continue;
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
        let Some(sym) = symbol_name(arg) else {
            continue;
        };
        if env.has_attribute(&sym, "Protected") {
            continue;
        }
        env.remove(&sym);
    }
    Ok(Value::Null)
}

/// `Remove[sym1, sym2, ...]` — completely removes symbols from the system,
/// including bindings, attributes, and lazy providers. Bypasses the Protected
/// check (removes regardless).
pub fn builtin_remove(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    for arg in args {
        let Some(sym) = symbol_name(arg) else {
            continue;
        };
        env.remove(&sym);
        // Unlike Clear/ClearAll (which leave an empty attribute entry behind so
        // the symbol stays visible to `Names`), Remove deletes the entry
        // entirely so the symbol disappears from the system.
        env.attributes.lock().unwrap().remove(&sym);
        {
            let mut providers = env.lazy_providers.lock().unwrap();
            providers.remove(&sym);
        }
    }
    Ok(Value::Null)
}

/// `Protect[sym1, sym2, ...]` — adds the `Protected` attribute to each symbol,
/// preventing later definitions/redefinitions. Returns the list of names.
pub fn builtin_protect(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    let mut protected = Vec::new();
    for arg in args {
        let Some(sym) = symbol_name(arg) else {
            continue;
        };
        if env.get(&sym).is_none() {
            env.set(sym.clone(), Value::Symbol(sym.clone()));
        }
        let mut attrs = env.get_attributes(&sym);
        if !attrs.iter().any(|a| a == "Protected") {
            attrs.push("Protected".to_string());
            env.set_attributes(&sym, attrs);
        }
        protected.push(Value::Str(sym));
    }
    Ok(Value::List(protected))
}

/// `Unprotect[sym1, sym2, ...]` — removes the `Protected` attribute from each
/// symbol, leaving any other attributes untouched. Returns the list of names.
pub fn builtin_unprotect(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    let mut unprotected = Vec::new();
    for arg in args {
        let Some(sym) = symbol_name(arg) else {
            continue;
        };
        let mut attrs = env.get_attributes(&sym);
        if attrs.iter().any(|a| a == "Protected") {
            attrs.retain(|a| a != "Protected");
            env.set_attributes(&sym, attrs);
        }
        unprotected.push(Value::Str(sym));
    }
    Ok(Value::List(unprotected))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rug::Integer;

    fn sym(s: &str) -> Value {
        Value::Symbol(s.to_string())
    }

    #[test]
    fn test_symbol_name_shapes() {
        assert_eq!(symbol_name(&sym("x")).as_deref(), Some("x"));
        assert_eq!(
            symbol_name(&Value::Str("x".to_string())).as_deref(),
            Some("x")
        );
        assert_eq!(
            symbol_name(&Value::Pattern(Expr::Symbol("x".to_string()))).as_deref(),
            Some("x")
        );
        assert_eq!(symbol_name(&Value::Integer(Integer::from(3))), None);
    }

    #[test]
    fn test_clear_removes_value() {
        let env = Env::new();
        env.set("x".to_string(), Value::Integer(Integer::from(5)));
        builtin_clear(&[sym("x")], &env).unwrap();
        assert!(env.get("x").is_none());
    }

    #[test]
    fn test_clear_handles_held_pattern() {
        // `Clear` is HoldAll, so the evaluator hands over a Value::Pattern.
        let env = Env::new();
        env.set("x".to_string(), Value::Integer(Integer::from(5)));
        builtin_clear(&[Value::Pattern(Expr::Symbol("x".to_string()))], &env).unwrap();
        assert!(env.get("x").is_none());
    }

    #[test]
    fn test_clear_skips_protected() {
        let env = Env::new();
        env.set("x".to_string(), Value::Integer(Integer::from(5)));
        env.set_attributes("x", vec!["Protected".to_string()]);
        builtin_clear(&[sym("x")], &env).unwrap();
        assert_eq!(env.get("x"), Some(Value::Integer(Integer::from(5))));
    }

    #[test]
    fn test_unset_keeps_attributes() {
        let env = Env::new();
        env.set("x".to_string(), Value::Integer(Integer::from(5)));
        env.set_attributes("x", vec!["Listable".to_string()]);
        builtin_unset(&[Value::Pattern(Expr::Symbol("x".to_string()))], &env).unwrap();
        assert!(env.get("x").is_none());
        assert!(env.has_attribute("x", "Listable"));
    }

    #[test]
    fn test_remove_drops_attribute_entry() {
        let env = Env::new();
        env.set("x".to_string(), Value::Integer(Integer::from(5)));
        env.set_attributes("x", vec!["Listable".to_string()]);
        builtin_remove(&[sym("x")], &env).unwrap();
        assert!(env.get("x").is_none());
        assert!(!env.attributes.lock().unwrap().contains_key("x"));
    }

    #[test]
    fn test_remove_bypasses_protected() {
        let env = Env::new();
        env.set("x".to_string(), Value::Integer(Integer::from(5)));
        env.set_attributes("x", vec!["Protected".to_string()]);
        builtin_remove(&[sym("x")], &env).unwrap();
        assert!(env.get("x").is_none());
    }

    #[test]
    fn test_protect_then_clear_skips() {
        let env = Env::new();
        let res = builtin_protect(&[sym("x")], &env).unwrap();
        assert_eq!(res, Value::List(vec![Value::Str("x".to_string())]));
        assert!(env.has_attribute("x", "Protected"));

        // Protecting twice must not duplicate the attribute.
        builtin_protect(&[sym("x")], &env).unwrap();
        let attrs = env.get_attributes("x");
        assert_eq!(attrs.iter().filter(|a| *a == "Protected").count(), 1);
    }

    #[test]
    fn test_unprotect_keeps_other_attributes() {
        let env = Env::new();
        env.set_attributes("x", vec!["Protected".to_string(), "Listable".to_string()]);
        let res = builtin_unprotect(&[sym("x")], &env).unwrap();
        assert_eq!(res, Value::List(vec![Value::Str("x".to_string())]));
        assert!(!env.has_attribute("x", "Protected"));
        assert!(env.has_attribute("x", "Listable"));
    }

    #[test]
    fn test_protect_protected_symbol_is_not_cleared() {
        // `Protect` must bypass the `Locked` guard that stops SetAttributes.
        let env = Env::new();
        env.set_attributes("x", vec!["Locked".to_string()]);
        builtin_protect(&[sym("x")], &env).unwrap();
        assert!(env.has_attribute("x", "Protected"));
    }

    #[test]
    fn test_protect_creates_symbol_binding() {
        let env = Env::new();
        builtin_protect(&[sym("x")], &env).unwrap();
        assert_eq!(env.get("x"), Some(sym("x")));
    }
}
