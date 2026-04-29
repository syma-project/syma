use crate::ast::Expr;
use crate::env::Env;
use crate::value::{Value, EvalError};
use crate::eval::apply_function;

pub(super) fn eval_control_flow(s: &str, args: &[Expr], env: &Env) -> Result<Option<Value>, EvalError> {
    match s {
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
            Ok(Some(result))
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
            Ok(Some(result))
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
            Ok(Some(result))
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
            Ok(Some(result))
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
            Ok(Some(result))
        }
        _ => Ok(None),
    }
}
