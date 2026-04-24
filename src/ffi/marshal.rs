/// Type marshalling between Syma `Value`s and native C types.
///
/// Two paths:
///   Direct C-ABI path (Tier 1): `value_to_c_arg` / `c_ret_to_value`
///   JSON wire path (Tier 2 Python, Tier 3 extensions): `values_to_json` / `json_to_value`
use std::ffi::{CStr, CString};

use crate::value::{DEFAULT_PRECISION, EvalError, NativeType, Value};
use rug::{Float, Integer};

// ── Direct C-ABI marshalling ──────────────────────────────────────────────────

/// An argument ready to be passed to a C function.
#[derive(Clone)]
pub enum CArg {
    I32(i32),
    I64(i64),
    U32(u32),
    U64(u64),
    F32(f32),
    F64(f64),
    Bool(i32),
    /// The `CString` is kept alive here so the `*const c_char` is valid during the call.
    CStr(CString),
}

/// Convert a Syma value to a C argument according to the declared type.
pub fn value_to_c_arg(v: &Value, ty: &NativeType) -> Result<CArg, EvalError> {
    match ty {
        NativeType::I32 => {
            let n = require_int(v, "Integer32")?;
            Ok(CArg::I32(n as i32))
        }
        NativeType::I64 => {
            let n = require_int(v, "Integer64")?;
            Ok(CArg::I64(n))
        }
        NativeType::U32 => {
            let n = require_int(v, "UnsignedInteger32")?;
            Ok(CArg::U32(n as u32))
        }
        NativeType::U64 => {
            let n = require_int(v, "UnsignedInteger64")?;
            Ok(CArg::U64(n as u64))
        }
        NativeType::F32 => {
            let r = require_real(v, "Real32")?;
            Ok(CArg::F32(r as f32))
        }
        NativeType::F64 => {
            let r = require_real(v, "Real64")?;
            Ok(CArg::F64(r))
        }
        NativeType::Bool => Ok(CArg::Bool(if v.to_bool() { 1 } else { 0 })),
        NativeType::CString => {
            if let Value::Str(s) = v {
                let cs = CString::new(s.as_str())
                    .map_err(|e| EvalError::FfiError(format!("CString conversion failed: {e}")))?;
                Ok(CArg::CStr(cs))
            } else {
                Err(EvalError::TypeError {
                    expected: "String".to_string(),
                    got: v.type_name().to_string(),
                })
            }
        }
        NativeType::Void => Err(EvalError::FfiError(
            "Void cannot be used as an input parameter".to_string(),
        )),
    }
}

/// Convert a raw return value (stored as bits in a `u64`) to a Syma `Value`.
pub fn c_ret_to_value(bits: u64, ty: &NativeType) -> Value {
    match ty {
        NativeType::Void => Value::Null,
        NativeType::I32 => Value::Integer(Integer::from(bits as i32 as i64)),
        NativeType::I64 => Value::Integer(Integer::from(bits as i64)),
        NativeType::U32 => Value::Integer(Integer::from(bits as u32 as i64)),
        NativeType::U64 => Value::Integer(Integer::from(bits as i64)),
        NativeType::F32 => {
            let f = f32::from_bits(bits as u32);
            Value::Real(Float::with_val(DEFAULT_PRECISION, f))
        }
        NativeType::F64 => {
            let f = f64::from_bits(bits);
            Value::Real(Float::with_val(DEFAULT_PRECISION, f))
        }
        NativeType::Bool => Value::Bool(bits != 0),
        NativeType::CString => {
            let ptr = bits as *const i8;
            if ptr.is_null() {
                Value::Null
            } else {
                let s = unsafe { CStr::from_ptr(ptr).to_string_lossy().into_owned() };
                Value::Str(s)
            }
        }
    }
}

// ── JSON wire marshalling (Tier 2 Python, Tier 3 extensions) ─────────────────

/// Serialise a slice of Syma values to a JSON string.
///
/// Only the following types can cross the JSON boundary:
/// `Integer`, `Real`, `Str`, `Bool`, `Null`, `List`, `Assoc`.
/// Anything else returns `EvalError::FfiError`.
pub fn values_to_json(args: &[Value]) -> Result<String, EvalError> {
    let json_vals: Vec<serde_json::Value> =
        args.iter().map(value_to_json).collect::<Result<_, _>>()?;
    serde_json::to_string(&json_vals)
        .map_err(|e| EvalError::FfiError(format!("JSON serialisation failed: {e}")))
}

/// Deserialise a JSON string to a single Syma `Value`.
pub fn json_to_value(json: &str) -> Result<Value, EvalError> {
    let jv: serde_json::Value = serde_json::from_str(json)
        .map_err(|e| EvalError::FfiError(format!("JSON parse failed: {e}")))?;
    json_val_to_value(&jv)
}

pub fn value_to_json(v: &Value) -> Result<serde_json::Value, EvalError> {
    match v {
        Value::Integer(n) => Ok(serde_json::Value::Number(serde_json::Number::from(
            n.to_i64().unwrap_or(0),
        ))),
        Value::Real(r) => {
            let f = r.to_f64();
            serde_json::Number::from_f64(f)
                .map(serde_json::Value::Number)
                .ok_or_else(|| EvalError::FfiError(format!("Real {f} is not JSON-serialisable")))
        }
        Value::Str(s) => Ok(serde_json::Value::String(s.clone())),
        Value::Bool(b) => Ok(serde_json::Value::Bool(*b)),
        Value::Null => Ok(serde_json::Value::Null),
        Value::List(items) => {
            let arr: Result<Vec<_>, _> = items.iter().map(value_to_json).collect();
            Ok(serde_json::Value::Array(arr?))
        }
        Value::Assoc(map) => {
            let obj: Result<serde_json::Map<_, _>, _> = map
                .iter()
                .map(|(k, v)| value_to_json(v).map(|jv| (k.clone(), jv)))
                .collect();
            Ok(serde_json::Value::Object(obj?))
        }
        other => Err(EvalError::FfiError(format!(
            "cannot marshal {} across FFI boundary",
            other.type_name()
        ))),
    }
}

fn json_val_to_value(jv: &serde_json::Value) -> Result<Value, EvalError> {
    match jv {
        serde_json::Value::Null => Ok(Value::Null),
        serde_json::Value::Bool(b) => Ok(Value::Bool(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(Value::Integer(Integer::from(i)))
            } else if let Some(f) = n.as_f64() {
                Ok(Value::Real(Float::with_val(DEFAULT_PRECISION, f)))
            } else {
                Err(EvalError::FfiError(format!(
                    "unrepresentable JSON number: {n}"
                )))
            }
        }
        serde_json::Value::String(s) => Ok(Value::Str(s.clone())),
        serde_json::Value::Array(arr) => {
            let items: Result<Vec<_>, _> = arr.iter().map(json_val_to_value).collect();
            Ok(Value::List(items?))
        }
        serde_json::Value::Object(obj) => {
            let map: Result<std::collections::HashMap<_, _>, _> = obj
                .iter()
                .map(|(k, v)| json_val_to_value(v).map(|val| (k.clone(), val)))
                .collect();
            Ok(Value::Assoc(map?))
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn require_int(v: &Value, ty_name: &str) -> Result<i64, EvalError> {
    match v {
        Value::Integer(n) => Ok(n.to_i64().unwrap_or(0)),
        Value::Real(r) => Ok(r.to_f64() as i64),
        _ => Err(EvalError::TypeError {
            expected: ty_name.to_string(),
            got: v.type_name().to_string(),
        }),
    }
}

fn require_real(v: &Value, ty_name: &str) -> Result<f64, EvalError> {
    match v {
        Value::Integer(n) => Ok(n.to_f64()),
        Value::Real(r) => Ok(r.to_f64()),
        _ => Err(EvalError::TypeError {
            expected: ty_name.to_string(),
            got: v.type_name().to_string(),
        }),
    }
}
