use crate::env::Env;
use crate::value::Value;

use super::flatten_flat_args;

pub(super) fn normalize_flat_result(name: &str, result: Value, env: &Env) -> Value {
    if !env.has_attribute(name, "Flat") {
        return result;
    }
    if let Value::Call {
        ref head,
        args: ref a,
    } = result
        && head == name
    {
        let flat = flatten_flat_args(name, a);
        if flat.len() != a.len() || flat.as_slice() != a.as_slice() {
            return Value::Call {
                head: head.clone(),
                args: flat,
            };
        }
    }
    result
}
