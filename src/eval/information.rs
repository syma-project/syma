use crate::ast::Expr;
use crate::env::Env;
use crate::value::{Value, EvalError};
use crate::eval::apply_function;

pub(super) fn eval_information(expr: &Expr, env: &Env) -> Result<Value, EvalError> {
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

