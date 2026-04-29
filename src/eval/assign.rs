use std::sync::Arc;

use crate::ast::*;
use crate::env::Env;
use crate::eval::table;
use crate::value::*;

use super::set_part;
use super::{eval, specificity};

/// Evaluate `x = y` (Set) and `x := y` (SetDelayed) from the assignment expression.
/// `delayed` is `false` for `=` (immediate), `true` for `:=` (delayed).
pub(super) fn eval_assign(
    lhs: &Expr,
    rhs: &Expr,
    delayed: bool,
    env: &Env,
) -> Result<Value, EvalError> {
    let val = if delayed {
        // Delayed assignment: RHS is stored unevaluated as a Pattern
        Value::Pattern(rhs.clone())
    } else {
        eval(rhs, env)?
    };
    match lhs {
        Expr::Symbol(s) => {
            if env.has_attribute(s, "Protected") && env.get(s).is_some() {
                return Err(EvalError::Error(format!(
                    "Symbol {} is protected; cannot assign",
                    s
                )));
            }
            env.set_propagate(s.clone(), val.clone());
            Ok(val)
        }
        // LocalSymbol["name"] = value — write to local symbol store
        Expr::Call {
            head,
            args: call_args,
        } if call_args.len() == 1
            && matches!(head.as_ref(), Expr::Symbol(s) if s == "LocalSymbol") =>
        {
            let name = eval(&call_args[0], env)?;
            match name {
                Value::Str(s) => crate::builtins::localsymbol::write_local_symbol(&s, &val),
                _ => Err(EvalError::Error(
                    "LocalSymbol requires a string name".to_string(),
                )),
            }
        }
        // Attributes[sym] = {attr1, attr2} — set attributes
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
                return Ok(Value::Null);
            }
            let attrs = match &val {
                Value::List(items) => items.iter().map(|v| v.to_string()).collect(),
                other => vec![other.to_string()],
            };
            env.set_attributes(&sym_name, attrs);
            Ok(val)
        }
        // f[args] = value — set a function definition with immediate RHS
        // Mathematica: Set[f[args], val]  defines a specific rule for f.
        // Also handles desugared OOP field access: this.field = val
        // is parsed as Assign(Call(field[this]), val). When the target
        // evaluates to an Object, treat as field access.
        Expr::Call {
            head,
            args: call_args,
        } if !matches!(head.as_ref(), Expr::Symbol(s) if s == "Part") => {
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
                        return Ok(val);
                    }
                }
                // Otherwise: function definition via assignment
                let body_expr = if delayed {
                    rhs.clone()
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
                    delayed,
                    guard: None,
                });
                func.definitions
                    .sort_by_key(|a| std::cmp::Reverse(specificity(&a.params)));
                env.set(name.clone(), Value::Function(Arc::new(func)));
                return Ok(val);
            }
            Err(EvalError::Error("Invalid assignment target".to_string()))
        }
        // x[[i]] = val  (desugared to Assign(Part[x, i], val))
        Expr::Call {
            head,
            args: part_args,
        } if matches!(head.as_ref(), Expr::Symbol(s) if s == "Part") && !part_args.is_empty() => {
            let var_name = match &part_args[0] {
                Expr::Symbol(s) => s.clone(),
                _ => {
                    return Err(EvalError::Error(
                        "Part assignment: collection must be a symbol".to_string(),
                    ));
                }
            };
            let current = env
                .get(&var_name)
                .ok_or_else(|| EvalError::Error(format!("Symbol {} is not defined", var_name)))?;
            let indices: Vec<i64> = part_args[1..]
                .iter()
                .map(|idx| {
                    eval(idx, env)?
                        .to_integer()
                        .ok_or_else(|| EvalError::TypeError {
                            expected: "Integer".to_string(),
                            got: "non-Integer".to_string(),
                        })
                })
                .collect::<Result<Vec<_>, _>>()?;
            let updated = set_part(current, &indices, val.clone())?;
            env.set_propagate(var_name, updated);
            Ok(val)
        }
        _ => Err(EvalError::Error("Invalid assignment target".to_string())),
    }
}
