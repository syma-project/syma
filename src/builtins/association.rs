use crate::value::EvalError;
use crate::value::Value;

pub fn builtin_keys(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Keys requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Assoc(map) => Ok(Value::List(
            map.keys().map(|k| Value::Str(k.clone())).collect(),
        )),
        _ => Err(EvalError::TypeError {
            expected: "Assoc".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

pub fn builtin_values(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Values requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Assoc(map) => Ok(Value::List(map.values().cloned().collect())),
        _ => Err(EvalError::TypeError {
            expected: "Assoc".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

pub fn builtin_lookup(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(EvalError::Error(
            "Lookup requires 2 or 3 arguments".to_string(),
        ));
    }
    match &args[0] {
        Value::Assoc(map) => {
            let key = match &args[1] {
                Value::Str(s) => s.clone(),
                _ => {
                    return Err(EvalError::TypeError {
                        expected: "String".to_string(),
                        got: args[1].type_name().to_string(),
                    });
                }
            };
            match map.get(&key) {
                Some(val) => Ok(val.clone()),
                None => {
                    if args.len() == 3 {
                        Ok(args[2].clone())
                    } else {
                        Ok(Value::Null)
                    }
                }
            }
        }
        _ => Err(EvalError::TypeError {
            expected: "Assoc".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

pub fn builtin_key_exists_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "KeyExistsQ requires exactly 2 arguments".to_string(),
        ));
    }
    match &args[0] {
        Value::Assoc(map) => {
            let key = match &args[1] {
                Value::Str(s) => s.clone(),
                _ => {
                    return Err(EvalError::TypeError {
                        expected: "String".to_string(),
                        got: args[1].type_name().to_string(),
                    });
                }
            };
            Ok(Value::Bool(map.contains_key(&key)))
        }
        _ => Err(EvalError::TypeError {
            expected: "Assoc".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}
