use crate::ast::Expr;
use crate::env::Env;
use crate::value::{EvalError, Value};

use super::eval;

pub(super) fn eval_information(expr: &Expr, env: &Env) -> Result<Value, EvalError> {
    match expr {
        Expr::Symbol(s) => {
            if let Some(val) = env.get(s) {
                if env.has_attribute(s, "ReadProtected") {
                    return Ok(Value::Str(format!("Symbol `{}` is read protected.", s)));
                }
                let info = match &val {
                    Value::Function(func_def) => {
                        let mut lines = vec![format!("User-defined function `{}`:", s)];
                        for def in &func_def.definitions {
                            let params: Vec<String> =
                                def.params.iter().map(|p| format!("{}", p)).collect();
                            lines.push(format!("    {}[{}] := {}", s, params.join(", "), def.body));
                        }
                        lines.join("\n")
                    }
                    Value::Builtin(_, _) => {
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
                        format!("{} = {}", s, val)
                    }
                };
                return Ok(Value::Str(info));
            }

            if let Some(help) = crate::builtins::get_help(s) {
                return Ok(Value::Str(help.to_string()));
            }

            Ok(Value::Call {
                head: "Missing".to_string(),
                args: vec![
                    Value::Symbol("UnknownSymbol".to_string()),
                    Value::Symbol(s.clone()),
                ],
            })
        }
        _ => {
            let val = eval(expr, env)?;
            Ok(Value::Str(format!(
                "{} is of type {}.",
                val,
                val.type_name()
            )))
        }
    }
}
