/// Type marshalling between Syma `Value`s and native C types.
///
/// Two paths:
///   Direct C-ABI path (Tier 1): `value_to_c_arg` / `c_ret_to_value`
///   JSON wire path (Tier 2 Python, Tier 3 extensions): `values_to_json` / `json_to_value`
use std::ffi::{CStr, CString};

use crate::value::{DEFAULT_PRECISION, EvalError, NativeType, PackedArrayType, Value};
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

// ── Full tagged-JSON serialisation (frontend/kernel) ───────────────────────────

/// Serialise a `Value` to a tagged-JSON representation suitable for the frontend.
///
/// All variants are supported. Complex types that contain `Expr` (Pattern,
/// PureFunction body, ClassDef internals) are represented via their Display
/// string rather than a full structural serialisation.
///
/// Format: `{"t": "<type-tag>", ...variant-specific fields...}`
pub fn value_to_json_full(v: &Value) -> serde_json::Value {
    use serde_json::{Map, Number, Value as JVal};
    match v {
        Value::Image(img) => {
            let mut m = Map::new();
            m.insert("t".into(), JVal::String("img".into()));
            m.insert("w".into(), JVal::Number((img.width() as u64).into()));
            m.insert("h".into(), JVal::Number((img.height() as u64).into()));
            m.insert("c".into(), JVal::String(format!("{:?}", img.color())));
            let cs = match img.color() {
                image::ColorType::L8 | image::ColorType::L16 => "Grayscale",
                image::ColorType::La8 | image::ColorType::La16 => "GrayAlpha",
                image::ColorType::Rgb8 | image::ColorType::Rgb16 | image::ColorType::Rgb32F => {
                    "RGB"
                }
                image::ColorType::Rgba8 | image::ColorType::Rgba16 | image::ColorType::Rgba32F => {
                    "RGBA"
                }
                _ => "Unknown",
            };
            m.insert("cs".into(), JVal::String(cs.into()));
            JVal::Object(m)
        }
        Value::Dataset(inner) => {
            // Serialize Dataset by its inner data
            let mut m = Map::new();
            m.insert("t".into(), JVal::String("dataset".into()));
            m.insert("v".into(), value_to_json_full(inner));
            JVal::Object(m)
        }
        Value::Integer(n) => {
            // Serialise as a string to avoid i64 precision loss for huge integers.
            // The frontend can parse small values as numbers when needed.
            let mut m = Map::new();
            m.insert("t".into(), JVal::String("int".into()));
            m.insert("v".into(), JVal::String(n.to_string()));
            JVal::Object(m)
        }
        Value::Real(r) => {
            let mut m = Map::new();
            m.insert("t".into(), JVal::String("real".into()));
            m.insert("v".into(), Number::from_f64(r.to_f64()).into());
            JVal::Object(m)
        }
        Value::Rational(r) => {
            let mut m = Map::new();
            m.insert("t".into(), JVal::String("rat".into()));
            m.insert("n".into(), JVal::String(r.numer().to_string()));
            m.insert("d".into(), JVal::String(r.denom().to_string()));
            JVal::Object(m)
        }
        Value::Complex { re, im } => {
            let mut m = Map::new();
            m.insert("t".into(), JVal::String("cpx".into()));
            m.insert("re".into(), Number::from_f64(*re).into());
            m.insert("im".into(), Number::from_f64(*im).into());
            JVal::Object(m)
        }
        Value::Str(s) => {
            let mut m = Map::new();
            m.insert("t".into(), JVal::String("str".into()));
            m.insert("v".into(), JVal::String(s.clone()));
            JVal::Object(m)
        }
        Value::Bool(b) => {
            let mut m = Map::new();
            m.insert("t".into(), JVal::String("bool".into()));
            m.insert("v".into(), JVal::Bool(*b));
            JVal::Object(m)
        }
        Value::Null => {
            let mut m = Map::new();
            m.insert("t".into(), JVal::String("null".into()));
            JVal::Object(m)
        }
        Value::Symbol(s) => {
            let mut m = Map::new();
            m.insert("t".into(), JVal::String("sym".into()));
            m.insert("v".into(), JVal::String(s.clone()));
            JVal::Object(m)
        }
        Value::List(items) => {
            let mut m = Map::new();
            m.insert("t".into(), JVal::String("list".into()));
            m.insert(
                "v".into(),
                JVal::Array(items.iter().map(value_to_json_full).collect()),
            );
            JVal::Object(m)
        }
        Value::Call { head, args } => {
            let mut m = Map::new();
            m.insert("t".into(), JVal::String("call".into()));
            m.insert("h".into(), JVal::String(head.clone()));
            m.insert(
                "v".into(),
                JVal::Array(args.iter().map(value_to_json_full).collect()),
            );
            JVal::Object(m)
        }
        Value::Assoc(map) => {
            let mut m = Map::new();
            m.insert("t".into(), JVal::String("assoc".into()));
            let obj: Map<String, JVal> = map
                .iter()
                .map(|(k, val)| (k.clone(), value_to_json_full(val)))
                .collect();
            m.insert("v".into(), JVal::Object(obj));
            JVal::Object(m)
        }
        Value::Rule { lhs, rhs, delayed } => {
            let mut m = Map::new();
            m.insert("t".into(), JVal::String("rule".into()));
            m.insert("l".into(), value_to_json_full(lhs));
            m.insert("r".into(), value_to_json_full(rhs));
            m.insert("d".into(), JVal::Bool(*delayed));
            JVal::Object(m)
        }
        Value::Function(fd) => {
            let mut m = Map::new();
            m.insert("t".into(), JVal::String("func".into()));
            m.insert("n".into(), JVal::String(fd.name.clone()));
            // Include definitions as display strings for reference
            let defs: Vec<JVal> = fd
                .definitions
                .iter()
                .map(|d| {
                    let params: Vec<String> = d.params.iter().map(|p| format!("{p}")).collect();
                    JVal::String(format!(
                        "{}[{:?}] := {}",
                        fd.name,
                        params.join(", "),
                        d.body
                    ))
                })
                .collect();
            m.insert("def".into(), JVal::Array(defs));
            JVal::Object(m)
        }
        Value::Builtin(name, _) => {
            let mut m = Map::new();
            m.insert("t".into(), JVal::String("builtin".into()));
            m.insert("n".into(), JVal::String(name.clone()));
            JVal::Object(m)
        }
        Value::PureFunction { slot_count, .. } => {
            let mut m = Map::new();
            m.insert("t".into(), JVal::String("purefn".into()));
            m.insert("n".into(), JVal::Number((*slot_count as u64).into()));
            JVal::Object(m)
        }
        Value::Method { name, object } => {
            let mut m = Map::new();
            m.insert("t".into(), JVal::String("method".into()));
            m.insert("n".into(), JVal::String(name.clone()));
            m.insert("o".into(), value_to_json_full(object));
            JVal::Object(m)
        }
        Value::Object { class_name, fields } => {
            let mut m = Map::new();
            m.insert("t".into(), JVal::String("obj".into()));
            m.insert("c".into(), JVal::String(class_name.clone()));
            let f_map: Map<String, JVal> = fields
                .iter()
                .map(|(k, val)| (k.clone(), value_to_json_full(val)))
                .collect();
            m.insert("f".into(), JVal::Object(f_map));
            JVal::Object(m)
        }
        Value::Class(cd) => {
            let mut m = Map::new();
            m.insert("t".into(), JVal::String("class".into()));
            m.insert("n".into(), JVal::String(cd.name.clone()));
            JVal::Object(m)
        }
        Value::RuleSet { name, rules } => {
            let mut m = Map::new();
            m.insert("t".into(), JVal::String("ruleset".into()));
            m.insert("n".into(), JVal::String(name.clone()));
            // Rules are Vec<(Value, Value)>
            let pairs: Vec<JVal> = rules
                .iter()
                .map(|(lhs, rhs)| {
                    JVal::Array(vec![value_to_json_full(lhs), value_to_json_full(rhs)])
                })
                .collect();
            m.insert("r".into(), JVal::Array(pairs));
            JVal::Object(m)
        }
        Value::DispatchedRules { rules, .. } => {
            let mut m = Map::new();
            m.insert("t".into(), JVal::String("dispatched".into()));
            let pairs: Vec<JVal> = rules
                .iter()
                .map(|(lhs, rhs)| {
                    JVal::Array(vec![value_to_json_full(lhs), value_to_json_full(rhs)])
                })
                .collect();
            m.insert("r".into(), JVal::Array(pairs));
            JVal::Object(m)
        }
        Value::Pattern(expr) => {
            let mut m = Map::new();
            m.insert("t".into(), JVal::String("pat".into()));
            m.insert("v".into(), JVal::String(format!("{expr}")));
            JVal::Object(m)
        }
        Value::Module { name, exports, .. } => {
            let mut m = Map::new();
            m.insert("t".into(), JVal::String("mod".into()));
            m.insert("n".into(), JVal::String(name.clone()));
            let e_map: Map<String, JVal> = exports
                .iter()
                .map(|(k, val)| (k.clone(), value_to_json_full(val)))
                .collect();
            m.insert("e".into(), JVal::Object(e_map));
            JVal::Object(m)
        }
        Value::Hold(inner) => {
            let mut m = Map::new();
            m.insert("t".into(), JVal::String("hold".into()));
            m.insert("v".into(), value_to_json_full(inner));
            JVal::Object(m)
        }
        Value::HoldComplete(inner) => {
            let mut m = Map::new();
            m.insert("t".into(), JVal::String("holdc".into()));
            m.insert("v".into(), value_to_json_full(inner));
            JVal::Object(m)
        }
        Value::NativeLib { name, .. } => {
            let mut m = Map::new();
            m.insert("t".into(), JVal::String("nativelib".into()));
            m.insert("n".into(), JVal::String(name.clone()));
            JVal::Object(m)
        }
        Value::NativeFunction {
            lib_name,
            symbol_name,
            ..
        } => {
            let mut m = Map::new();
            m.insert("t".into(), JVal::String("nativefn".into()));
            m.insert("l".into(), JVal::String(lib_name.clone()));
            m.insert("s".into(), JVal::String(symbol_name.clone()));
            JVal::Object(m)
        }
        Value::Sequence(items) => {
            let vs: Vec<JVal> = items.iter().map(value_to_json_full).collect();
            JVal::Array(vs)
        }
        Value::Formatted { value, .. } => value_to_json_full(value),
        Value::BytecodeFunction(bc) => {
            let mut m = Map::new();
            m.insert("t".into(), JVal::String("bytecode".into()));
            m.insert("name".into(), JVal::String(bc.name.clone()));
            JVal::Object(m)
        }
        Value::PackedArray(pa) => {
            let mut m = Map::new();
            m.insert("t".into(), JVal::String("packedarray".into()));
            let vs: Vec<JVal> = pa.to_values().iter().map(value_to_json_full).collect();
            m.insert("v".into(), JVal::Array(vs));
            let type_name = match pa {
                PackedArrayType::Integer64(_) => "Integer64",
                PackedArrayType::Real64(_) => "Real64",
            };
            m.insert("type".into(), JVal::String(type_name.into()));
            JVal::Object(m)
        }
        Value::SeriesData {
            variable,
            expansion_point,
            coefficients,
            min_exponent,
            max_exponent,
            denominator,
        } => {
            let mut m = Map::new();
            m.insert("t".into(), JVal::String("seriesdata".into()));
            m.insert("var".into(), value_to_json_full(variable));
            m.insert("pt".into(), value_to_json_full(expansion_point));
            m.insert(
                "coeffs".into(),
                JVal::Array(coefficients.iter().map(value_to_json_full).collect()),
            );
            m.insert("nmin".into(), JVal::Number((*min_exponent as i64).into()));
            m.insert("nmax".into(), JVal::Number((*max_exponent as i64).into()));
            m.insert("den".into(), JVal::Number((*denominator as i64).into()));
            JVal::Object(m)
        }
        Value::Root { coeffs, index } => {
            let mut m = Map::new();
            m.insert("t".into(), JVal::String("root".into()));
            m.insert(
                "coeffs".into(),
                JVal::Array(
                    coeffs
                        .iter()
                        .map(|c| {
                            JVal::Array(vec![
                                JVal::String(c.numer().to_string()),
                                JVal::String(c.denom().to_string()),
                            ])
                        })
                        .collect(),
                ),
            );
            m.insert("idx".into(), JVal::Number((*index as u64).into()));
            JVal::Object(m)
        }
    }
}

/// Deserialise a tagged-JSON value back into a `Value`.
pub fn json_val_to_value_full(jv: &serde_json::Value) -> Result<Value, EvalError> {
    use serde_json::Value as JVal;
    let obj = match jv {
        JVal::Object(m) => m,
        _ => return json_to_value_fallback(jv), // Untagged JSON: fallback to old parser
    };
    let tag = match obj.get("t").and_then(|t| t.as_str()) {
        Some(t) => t,
        None => return json_to_value_fallback(jv),
    };
    match tag {
        "int" => {
            let s = obj
                .get("v")
                .and_then(|v| v.as_str())
                .ok_or_else(|| EvalError::FfiError("int missing 'v' field".into()))?;
            let n = Integer::parse(s)
                .map_err(|e| EvalError::FfiError(format!("invalid integer '{s}': {e}")))?;
            Ok(Value::Integer(rug::Integer::from(n)))
        }
        "real" => {
            let f = obj
                .get("v")
                .and_then(|v| v.as_f64())
                .ok_or_else(|| EvalError::FfiError("real missing 'v' field".into()))?;
            Ok(Value::Real(Float::with_val(DEFAULT_PRECISION, f)))
        }
        "cpx" => {
            let re = obj.get("re").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let im = obj.get("im").and_then(|v| v.as_f64()).unwrap_or(0.0);
            Ok(Value::Complex { re, im })
        }
        "str" => {
            let s = obj
                .get("v")
                .and_then(|v| v.as_str())
                .ok_or_else(|| EvalError::FfiError("str missing 'v' field".into()))?;
            Ok(Value::Str(s.to_string()))
        }
        "bool" => {
            let b = obj
                .get("v")
                .and_then(|v| v.as_bool())
                .ok_or_else(|| EvalError::FfiError("bool missing 'v' field".into()))?;
            Ok(Value::Bool(b))
        }
        "null" => Ok(Value::Null),
        "sym" => {
            let s = obj
                .get("v")
                .and_then(|v| v.as_str())
                .ok_or_else(|| EvalError::FfiError("sym missing 'v' field".into()))?;
            Ok(Value::Symbol(s.to_string()))
        }
        "list" => {
            let arr = obj
                .get("v")
                .and_then(|v| v.as_array())
                .ok_or_else(|| EvalError::FfiError("list missing 'v' field".into()))?;
            let items: Result<Vec<_>, _> = arr.iter().map(json_val_to_value_full).collect();
            Ok(Value::List(items?))
        }
        "call" => {
            let head = obj
                .get("h")
                .and_then(|v| v.as_str())
                .ok_or_else(|| EvalError::FfiError("call missing 'h' field".into()))?;
            let arr = obj
                .get("v")
                .and_then(|v| v.as_array())
                .ok_or_else(|| EvalError::FfiError("call missing 'v' field".into()))?;
            let args: Result<Vec<_>, _> = arr.iter().map(json_val_to_value_full).collect();
            Ok(Value::Call {
                head: head.to_string(),
                args: args?,
            })
        }
        "assoc" => {
            let map_obj = obj
                .get("v")
                .and_then(|v| v.as_object())
                .ok_or_else(|| EvalError::FfiError("assoc missing 'v' field".into()))?;
            let map: Result<std::collections::HashMap<_, _>, _> = map_obj
                .iter()
                .map(|(k, val)| json_val_to_value_full(val).map(|v| (k.clone(), v)))
                .collect();
            Ok(Value::Assoc(map?))
        }
        "rule" => {
            let lhs = obj
                .get("l")
                .ok_or_else(|| EvalError::FfiError("rule missing 'l' field".into()))?;
            let rhs = obj
                .get("r")
                .ok_or_else(|| EvalError::FfiError("rule missing 'r' field".into()))?;
            let delayed = obj.get("d").and_then(|v| v.as_bool()).unwrap_or(false);
            Ok(Value::Rule {
                lhs: Box::new(json_val_to_value_full(lhs)?),
                rhs: Box::new(json_val_to_value_full(rhs)?),
                delayed,
            })
        }
        "func" | "builtin" | "purefn" | "method" | "class" | "ruleset" | "pat" | "mod"
        | "nativelib" | "nativefn" | "obj" | "hold" | "holdc" | "img" => {
            // These types are useful as output but generally shouldn't be
            // round-tripped through JSON. Return a display-string representation.
            Ok(Value::Str(format!("{jv}")))
        }
        _ => Err(EvalError::FfiError(format!(
            "unknown type tag '{tag}' in JSON value"
        ))),
    }
}

/// Fallback: parse an untagged JSON value using the original simple format
/// (numbers → Integer/Real, strings → Str, arrays → List, objects → Assoc).
fn json_to_value_fallback(jv: &serde_json::Value) -> Result<Value, EvalError> {
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
            let items: Result<Vec<_>, _> = arr.iter().map(json_val_to_value_full).collect();
            Ok(Value::List(items?))
        }
        serde_json::Value::Object(_) => {
            // Untagged objects are treated as associations
            let map: Result<std::collections::HashMap<_, _>, _> = jv
                .as_object()
                .unwrap()
                .iter()
                .map(|(k, val)| json_val_to_value_full(val).map(|v| (k.clone(), v)))
                .collect();
            Ok(Value::Assoc(map?))
        }
    }
}

/// Serialise a slice of Values to a tagged-JSON string (frontend format).
pub fn values_to_json_full(args: &[Value]) -> Result<String, EvalError> {
    let json_vals: Vec<serde_json::Value> = args.iter().map(value_to_json_full).collect();
    serde_json::to_string(&json_vals)
        .map_err(|e| EvalError::FfiError(format!("JSON serialisation failed: {e}")))
}

/// Parse a tagged-JSON string back into a Value.
pub fn json_to_value_full(json: &str) -> Result<Value, EvalError> {
    let jv: serde_json::Value = serde_json::from_str(json)
        .map_err(|e| EvalError::FfiError(format!("JSON parse failed: {e}")))?;
    json_val_to_value_full(&jv)
}

// ── Legacy FFI JSON marshalling (kept for backward compatibility) ──────────────

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
        Value::Rational(r) => {
            let f = r.to_f64();
            serde_json::Number::from_f64(f)
                .map(serde_json::Value::Number)
                .ok_or_else(|| {
                    EvalError::FfiError(format!("Rational {r} is not JSON-serialisable"))
                })
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
        Value::Dataset(inner) => {
            // Unwrap Dataset and serialize inner data
            value_to_json(inner)
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
        Value::Rational(r) => {
            if r.denom() == &Integer::from(1) {
                r.numer().to_i64().ok_or_else(|| EvalError::TypeError {
                    expected: ty_name.to_string(),
                    got: v.type_name().to_string(),
                })
            } else {
                Ok(r.to_f64() as i64)
            }
        }
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
        Value::Rational(r) => Ok(r.to_f64()),
        _ => Err(EvalError::TypeError {
            expected: ty_name.to_string(),
            got: v.type_name().to_string(),
        }),
    }
}
