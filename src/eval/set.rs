use crate::ast::Expr;
use crate::env::Env;
use crate::value::{Value, EvalError};
use crate::eval::apply_function;

pub(super) fn eval_set(s: &str, args: &[Expr], env: &Env) -> Result<Option<Value>, EvalError> {
    if s != "Set" && s != "SetDelayed" {
        return Ok(None);
    }
    // Handle assignment specially
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Set requires exactly 2 arguments".to_string(),
        ));
    }
    let val = eval(&args[1], env)?;
    let result = match &args[0] {
        Expr::Symbol(name) => {
            env.set_propagate(name.clone(), val.clone());
            val
        }
        // Attributes[sym] = value  via Set[Attributes[sym], value]
        Expr::Call {
            head,
            args: call_args,
        } if call_args.len() == 1
            && matches!(head.as_ref(), Expr::Symbol(s) if s == "Attributes") =>
        {
            let sym_name = match &call_args[0] {
                Expr::Symbol(s) => s.clone(),
                _ => {
                    return Err(EvalError::Error(
                        "Attributes assignment requires a symbol name".to_string(),
                    ));
                }
            };
            if env.has_attribute(&sym_name, "Locked") {
                return Ok(Some(Value::Null));
            }
            let attrs = match &val {
                Value::List(items) => items.iter().map(|v| v.to_string()).collect(),
                other => vec![other.to_string()],
            };
            env.set_attributes(&sym_name, attrs);
            val
        }
        // f[args] = value  or SetDelayed[f[args], val]
        // — function definition with immediate or delayed RHS.
        // Also handles desugared OOP field access: this.field = val
        // is parsed as Set[field[this], val]. When target is an
        // Object, treat as field access.
        Expr::Call {
            head,
            args: call_args,
        } if !matches!(head.as_ref(), Expr::Symbol(s) if s == "Part")
            && !matches!(head.as_ref(), Expr::Symbol(s) if s == "Attributes") =>
        {
            if let Expr::Symbol(name) = head.as_ref() {
                // Check for OOP field access: field[object] = value
                // where object evaluates to an Object
                if call_args.len() == 1 {
                    let target = eval(&call_args[0], env)?;
                    if let Value::Object {
                        class_name,
                        mut fields,
                    } = target
                    {
                        fields.insert(name.clone(), val.clone());
                        let updated = Value::Object { class_name, fields };
                        if let Expr::Symbol(s) = &call_args[0]
                            && s == "this"
                        {
                            env.set("this".to_string(), updated.clone());
                        }
                        return Ok(Some(val));
                    }
                }
                // Otherwise: function definition via Set/SetDelayed
                // SetDelayed[f[args], body] — use body as-is
                // Set[f[args], val] — evaluate RHS then store as Expr
                let body_expr = if s == "SetDelayed" {
                    args[1].clone()
                } else {
                    table::value_to_expr(&val)
                };
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
                    params: call_args.clone(),
                    body: body_expr,
                    delayed: s == "SetDelayed",
                    guard: None,
                });
                // Sort definitions so more specific ones match first
                func.definitions.sort_by(|a, b| {
                    specificity(&b.params).cmp(&specificity(&a.params))
                });
                env.set(name.clone(), Value::Function(Arc::new(func)));
                return Ok(Some(val));
            }
            return Err(EvalError::Error("Invalid assignment target".to_string()));
        }
        _ => return Err(EvalError::Error("Invalid assignment target".to_string())),
    };
    Ok(Some(result))
}

