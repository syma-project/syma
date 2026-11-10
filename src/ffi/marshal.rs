/// Type marshalling between Syma `Value`s and native C types.
///
/// Two paths:
///   Direct C-ABI path (Tier 1): `value_to_c_arg` / `c_ret_to_value`
///   JSON wire path (Tier 2 Python, Tier 3 extensions): `values_to_json` / `json_to_value`
use std::ffi::{CStr, CString};
use std::os::raw::c_char;

use crate::value::{DEFAULT_PRECISION, EvalError, NativeType, PackedArrayType, Value};
use rug::{Float, Integer};

// ── Direct C-ABI marshalling ──────────────────────────────────────────────────

/// An argument ready to be passed to a C function.
#[derive(Clone)]
pub enum CArg {
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    F32(f32),
    F64(f64),
    Bool(i32),
    /// Raw pointer (`*mut c_void`), carried as an unsigned integer.
    Ptr(u64),
    /// The `CString` is kept alive here so the `*const c_char` is valid during the call.
    CStr(CString),
}

/// Convert a Syma value to a C argument according to the declared type.
pub fn value_to_c_arg(v: &Value, ty: &NativeType) -> Result<CArg, EvalError> {
    match ty {
        NativeType::I8 => {
            let n = require_int(v, "Integer8")?;
            let n = i8::try_from(n).map_err(|_| out_of_range("Integer8", v, n))?;
            Ok(CArg::I8(n))
        }
        NativeType::I16 => {
            let n = require_int(v, "Integer16")?;
            let n = i16::try_from(n).map_err(|_| out_of_range("Integer16", v, n))?;
            Ok(CArg::I16(n))
        }
        NativeType::I32 => {
            let n = require_int(v, "Integer32")?;
            let n = i32::try_from(n).map_err(|_| out_of_range("Integer32", v, n))?;
            Ok(CArg::I32(n))
        }
        NativeType::I64 => {
            let n = require_int(v, "Integer64")?;
            Ok(CArg::I64(n))
        }
        NativeType::U8 => {
            let n = require_uint(v, "UnsignedInteger8")?;
            let n = u8::try_from(n).map_err(|_| out_of_range("UnsignedInteger8", v, n))?;
            Ok(CArg::U8(n))
        }
        NativeType::U16 => {
            let n = require_uint(v, "UnsignedInteger16")?;
            let n = u16::try_from(n).map_err(|_| out_of_range("UnsignedInteger16", v, n))?;
            Ok(CArg::U16(n))
        }
        NativeType::U32 => {
            let n = require_uint(v, "UnsignedInteger32")?;
            let n = u32::try_from(n).map_err(|_| out_of_range("UnsignedInteger32", v, n))?;
            Ok(CArg::U32(n))
        }
        NativeType::U64 => {
            let n = require_uint(v, "UnsignedInteger64")?;
            Ok(CArg::U64(n))
        }
        NativeType::F32 => {
            let r = require_real(v, "Real32")?;
            Ok(CArg::F32(r as f32))
        }
        NativeType::F64 => {
            let r = require_real(v, "Real64")?;
            Ok(CArg::F64(r))
        }
        NativeType::Bool => Ok(CArg::Bool(i32::from(v.to_bool()))),
        NativeType::Pointer => Ok(CArg::Ptr(require_pointer(v, "Pointer")?)),
        NativeType::UTF8String => {
            if let Value::Str(s) = v {
                let cs = CString::new(s.as_str()).map_err(|e| {
                    EvalError::FfiError(format!("UTF8String conversion failed: {e}"))
                })?;
                Ok(CArg::CStr(cs))
            } else {
                Err(EvalError::TypeError {
                    expected: "String".to_string(),
                    got: v.type_name().to_string(),
                })
            }
        }
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
        NativeType::Struct(_) => Err(EvalError::FfiError(
            "Struct arguments are not supported (return-only, basic support)".to_string(),
        )),
    }
}

fn out_of_range(ty_name: &str, v: &Value, n: impl std::fmt::Display) -> EvalError {
    EvalError::TypeError {
        expected: ty_name.to_string(),
        got: format!("{} {n} out of range", v.type_name()),
    }
}

/// Convert a raw return value (stored as bits in a `u64`) to a Syma `Value`.
pub fn c_ret_to_value(bits: u64, ty: &NativeType) -> Value {
    match ty {
        NativeType::Void => Value::Null,
        NativeType::I8 => Value::Integer(Integer::from(bits as u8 as i8 as i64)),
        NativeType::I16 => Value::Integer(Integer::from(bits as u16 as i16 as i64)),
        NativeType::I32 => Value::Integer(Integer::from(bits as u32 as i32 as i64)),
        NativeType::I64 => Value::Integer(Integer::from(bits as i64)),
        NativeType::U8 => Value::Integer(Integer::from(bits as u8)),
        NativeType::U16 => Value::Integer(Integer::from(bits as u16)),
        NativeType::U32 => Value::Integer(Integer::from(bits as u32)),
        NativeType::U64 => Value::Integer(Integer::from(bits)),
        NativeType::F32 => {
            let f = f32::from_bits(bits as u32);
            Value::Real(Float::with_val(DEFAULT_PRECISION, f))
        }
        NativeType::F64 => {
            let f = f64::from_bits(bits);
            Value::Real(Float::with_val(DEFAULT_PRECISION, f))
        }
        NativeType::Bool => Value::Bool(bits != 0),
        NativeType::Pointer => Value::Integer(Integer::from(bits)),
        NativeType::CString | NativeType::UTF8String => {
            let ptr = bits as *const c_char;
            if ptr.is_null() {
                Value::Null
            } else {
                let s = unsafe { CStr::from_ptr(ptr).to_string_lossy().into_owned() };
                Value::Str(s)
            }
        }
        NativeType::Struct(fields) => decode_struct(&[bits, 0], fields),
    }
}

/// Convert a native return value split across two machine words into a `Value`.
///
/// Only `Struct` returns consume the second word; everything else ignores it.
/// This mirrors the SysV convention where an aggregate of two words is
/// returned in a register pair.
pub fn c_ret_to_value2(lo: u64, hi: u64, ty: &NativeType) -> Value {
    match ty {
        NativeType::Struct(fields) => decode_struct(&[lo, hi], fields),
        _ => c_ret_to_value(lo, ty),
    }
}

/// Decode a by-value struct from its raw little-endian machine words.
///
/// Fields are taken in declaration order from a flat little-endian byte
/// stream. Aggregate/nested fields are not supported beyond one level.
pub fn decode_struct(words: &[u64], fields: &[NativeType]) -> Value {
    let mut bytes = Vec::with_capacity(words.len() * 8);
    for w in words {
        bytes.extend_from_slice(&w.to_le_bytes());
    }
    let mut offset = 0usize;
    let mut out = Vec::with_capacity(fields.len());
    for f in fields {
        let size = f.byte_size().unwrap_or(8).max(1);
        let mut buf = [0u8; 8];
        let end = (offset + size).min(bytes.len());
        if offset < bytes.len() {
            buf[..end - offset].copy_from_slice(&bytes[offset..end]);
        }
        let bits = u64::from_le_bytes(buf);
        out.push(c_ret_to_value(bits, f));
        // Round up to the field's alignment (natural size), as C would.
        offset += size;
    }
    Value::List(out)
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
        Value::Integer(n) => {
            // Prefer an exact integer representation; `to_i64().unwrap_or(0)`
            // previously collapsed any value outside i64 to 0.
            if let Some(i) = n.to_i64() {
                Ok(serde_json::Value::Number(serde_json::Number::from(i)))
            } else if let Some(u) = n.to_u64() {
                Ok(serde_json::Value::Number(serde_json::Number::from(u)))
            } else {
                let f = n.to_f64();
                serde_json::Number::from_f64(f)
                    .map(serde_json::Value::Number)
                    .ok_or_else(|| {
                        EvalError::FfiError(format!("Integer {n} is not JSON-serialisable"))
                    })
            }
        }
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
        // Report an error rather than silently coercing an out-of-range
        // arbitrary-precision integer to 0.
        Value::Integer(n) => n.to_i64().ok_or_else(|| EvalError::TypeError {
            expected: ty_name.to_string(),
            got: format!("Integer {n} out of 64-bit range"),
        }),
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

/// Convert a value to a `u64` for an unsigned C parameter.
///
/// Unlike `require_int`, this accepts the full 64-bit unsigned range (values
/// above `i64::MAX` previously failed to marshal at all), and it rejects
/// negative integers so that `UnsignedInteger*` is not silently reinterpreted.
fn require_uint(v: &Value, ty_name: &str) -> Result<u64, EvalError> {
    let out_of_range = |v: &Value| EvalError::TypeError {
        expected: ty_name.to_string(),
        got: v.type_name().to_string(),
    };
    match v {
        Value::Integer(n) => {
            if let Some(u) = n.to_u64() {
                Ok(u)
            } else if let Some(i) = n.to_i64() {
                // Negative-but-fits: reinterpret as two's complement, matching C.
                Ok(i as u64)
            } else {
                Err(EvalError::TypeError {
                    expected: ty_name.to_string(),
                    got: format!("Integer {n} out of 64-bit range"),
                })
            }
        }
        Value::Real(r) => {
            let f = r.to_f64();
            if f < 0.0 {
                Err(out_of_range(v))
            } else {
                Ok(f as u64)
            }
        }
        Value::Rational(r) => {
            if r.denom() == &Integer::from(1) {
                let n = r.numer();
                if let Some(u) = n.to_u64() {
                    Ok(u)
                } else if let Some(i) = n.to_i64() {
                    Ok(i as u64)
                } else {
                    Err(out_of_range(v))
                }
            } else {
                let f = r.to_f64();
                if f < 0.0 {
                    Err(out_of_range(v))
                } else {
                    Ok(f as u64)
                }
            }
        }
        _ => Err(out_of_range(v)),
    }
}

/// Convert a value to a raw pointer address carried in a `u64`.
///
/// Accepts arbitrary-precision integers (including `0`/null and negative
/// values reinterpreted two's-complement) and non-negative reals.
fn require_pointer(v: &Value, ty_name: &str) -> Result<u64, EvalError> {
    let bad = || EvalError::TypeError {
        expected: ty_name.to_string(),
        got: v.type_name().to_string(),
    };
    match v {
        Value::Integer(n) => {
            if let Some(u) = n.to_u64() {
                Ok(u)
            } else if let Some(i) = n.to_i64() {
                Ok(i as u64)
            } else {
                Err(EvalError::TypeError {
                    expected: ty_name.to_string(),
                    got: format!("Integer {n} out of 64-bit range"),
                })
            }
        }
        Value::Real(r) => {
            let f = r.to_f64();
            if f < 0.0 { Err(bad()) } else { Ok(f as u64) }
        }
        _ => Err(bad()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_c_ret_u64_not_truncated() {
        // Regression: `bits as i64` wrapped values above i64::MAX to negative.
        let big = u64::MAX;
        assert_eq!(
            c_ret_to_value(big, &NativeType::U64),
            Value::Integer(Integer::from(big))
        );
        let mid = (i64::MAX as u64) + 1;
        assert_eq!(
            c_ret_to_value(mid, &NativeType::U64),
            Value::Integer(Integer::from(mid))
        );
    }

    #[test]
    fn test_value_to_c_arg_huge_integer_errors() {
        // 2^100 does not fit in i64; previously it silently became 0.
        let huge = Value::Integer(Integer::from(1) << 100);
        assert!(value_to_c_arg(&huge, &NativeType::I64).is_err());
        // A value within range still works.
        assert!(matches!(
            value_to_c_arg(&Value::Integer(5.into()), &NativeType::I64),
            Ok(CArg::I64(5))
        ));
    }

    #[test]
    fn test_value_to_json_big_integer_not_zero() {
        // 2^63 fits in u64 but not i64; previously it serialised as 0.
        let v = Value::Integer(Integer::from(1u64) << 63);
        let j = value_to_json(&v).unwrap();
        assert_eq!(j.as_u64(), Some(1u64 << 63));
        assert_ne!(j, serde_json::Value::Number(0.into()));
    }

    #[test]
    fn test_c_ret_u32_not_sign_extended() {
        let big = u32::MAX;
        assert_eq!(
            c_ret_to_value(big as u64, &NativeType::U32),
            Value::Integer(Integer::from(big))
        );
    }

    #[test]
    fn test_value_to_c_arg_i32_out_of_range_errors() {
        // 2^40 does not fit in i32; previously it silently wrapped to 0.
        let big = Value::Integer(Integer::from(1) << 40);
        assert!(value_to_c_arg(&big, &NativeType::I32).is_err());
        // Boundary values still marshal.
        assert!(matches!(
            value_to_c_arg(&Value::Integer(i32::MIN.into()), &NativeType::I32),
            Ok(CArg::I32(i32::MIN))
        ));
        assert!(matches!(
            value_to_c_arg(&Value::Integer(i32::MAX.into()), &NativeType::I32),
            Ok(CArg::I32(i32::MAX))
        ));
    }

    #[test]
    fn test_value_to_c_arg_u32_out_of_range_errors() {
        let big = Value::Integer(Integer::from(1) << 40);
        assert!(value_to_c_arg(&big, &NativeType::U32).is_err());
        assert!(matches!(
            value_to_c_arg(&Value::Integer(u32::MAX.into()), &NativeType::U32),
            Ok(CArg::U32(u32::MAX))
        ));
    }

    #[test]
    fn test_value_to_c_arg_u64_accepts_above_i64_max() {
        // Values in (i64::MAX, u64::MAX] must be representable as UnsignedInteger64.
        let big = Value::Integer(Integer::from(u64::MAX));
        assert!(matches!(
            value_to_c_arg(&big, &NativeType::U64),
            Ok(CArg::U64(v)) if v == u64::MAX
        ));
        // A negative integer is reinterpreted two's-complement, as C does.
        assert!(matches!(
            value_to_c_arg(&Value::Integer(Integer::from(-1)), &NativeType::U64),
            Ok(CArg::U64(u64::MAX))
        ));
    }

    #[test]
    fn test_value_to_c_arg_cstring_interior_nul_errors() {
        let v = Value::Str("a\0b".to_string());
        assert!(value_to_c_arg(&v, &NativeType::CString).is_err());
        let ok = Value::Str("hello".to_string());
        match value_to_c_arg(&ok, &NativeType::CString).unwrap() {
            CArg::CStr(cs) => assert_eq!(cs.to_str().unwrap(), "hello"),
            _ => panic!("expected CStr"),
        }
    }

    #[test]
    fn test_c_ret_float_roundtrip() {
        let f = 1.5f64;
        assert_eq!(
            c_ret_to_value(f.to_bits(), &NativeType::F64),
            Value::Real(Float::with_val(DEFAULT_PRECISION, f))
        );
        let g = -2.25f32;
        assert_eq!(
            c_ret_to_value(g.to_bits() as u64, &NativeType::F32),
            Value::Real(Float::with_val(DEFAULT_PRECISION, g))
        );
    }

    #[test]
    fn test_c_ret_i32_sign_extends() {
        let bits = (-7i32) as u64; // upper bits set by the cast
        assert_eq!(
            c_ret_to_value(bits, &NativeType::I32),
            Value::Integer(Integer::from(-7))
        );
    }

    #[test]
    fn test_c_ret_cstring_null_is_null() {
        assert_eq!(c_ret_to_value(0, &NativeType::CString), Value::Null);
        let s = CString::new("hello").unwrap();
        assert_eq!(
            c_ret_to_value(s.as_ptr() as usize as u64, &NativeType::CString),
            Value::Str("hello".to_string())
        );
    }

    #[test]
    fn test_c_ret_bool_and_void() {
        assert_eq!(c_ret_to_value(0, &NativeType::Bool), Value::Bool(false));
        assert_eq!(c_ret_to_value(7, &NativeType::Bool), Value::Bool(true));
        assert_eq!(c_ret_to_value(1234, &NativeType::Void), Value::Null);
    }

    #[test]
    fn test_legacy_json_roundtrip() {
        let args = vec![
            Value::Integer(Integer::from(42)),
            Value::Real(Float::with_val(DEFAULT_PRECISION, 2.5)),
            Value::Str("hi".to_string()),
            Value::Bool(true),
            Value::Null,
            Value::List(vec![Value::Integer(Integer::from(1))]),
        ];
        let json = values_to_json(&args).unwrap();
        let json_vals: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
        assert_eq!(json_vals.len(), args.len());
        assert_eq!(json_to_value("[1,2,3]").unwrap().type_name(), "List");
    }

    #[test]
    fn test_legacy_values_to_json_rejects_unsupported() {
        let v = Value::Complex { re: 1.0, im: 2.0 };
        assert!(values_to_json(&[v]).is_err());
    }

    // ── Narrow integer / pointer / string-breadth marshalling ──

    #[test]
    fn test_c_ret_narrow_signed_extends() {
        assert_eq!(
            c_ret_to_value((-1i8) as u64, &NativeType::I8),
            Value::Integer(Integer::from(-1))
        );
        assert_eq!(
            c_ret_to_value((-3i16) as u64, &NativeType::I16),
            Value::Integer(Integer::from(-3))
        );
        // Upper garbage bits must be ignored (truncate to the declared width).
        let dirty = 0xDEAD_BEEF_0000_007Fu64;
        assert_eq!(
            c_ret_to_value(dirty, &NativeType::I8),
            Value::Integer(Integer::from(0x7Fi64))
        );
    }

    #[test]
    fn test_c_ret_narrow_unsigned_zero_extends() {
        assert_eq!(
            c_ret_to_value(0xFFu64, &NativeType::U8),
            Value::Integer(Integer::from(255))
        );
        assert_eq!(
            c_ret_to_value(0xFFFFu64, &NativeType::U16),
            Value::Integer(Integer::from(65535))
        );
        // Signed -1 reinterpreted as its low 8 bits (255).
        assert_eq!(
            c_ret_to_value((-1i64) as u64, &NativeType::U8),
            Value::Integer(Integer::from(255))
        );
    }

    #[test]
    fn test_value_to_c_arg_narrow_range_checks() {
        assert!(matches!(
            value_to_c_arg(&Value::Integer(Integer::from(127)), &NativeType::I8),
            Ok(CArg::I8(127))
        ));
        assert!(value_to_c_arg(&Value::Integer(Integer::from(128)), &NativeType::I8).is_err());
        assert!(value_to_c_arg(&Value::Integer(Integer::from(-129)), &NativeType::I16).is_ok());
        assert!(value_to_c_arg(&Value::Integer(Integer::from(256)), &NativeType::U8).is_err());
        assert!(matches!(
            value_to_c_arg(&Value::Integer(Integer::from(255)), &NativeType::U8),
            Ok(CArg::U8(255))
        ));
        assert!(value_to_c_arg(&Value::Integer(Integer::from(65536)), &NativeType::U16).is_err());
    }

    #[test]
    fn test_value_to_c_arg_pointer() {
        assert!(matches!(
            value_to_c_arg(&Value::Integer(Integer::from(0)), &NativeType::Pointer),
            Ok(CArg::Ptr(0))
        ));
        assert!(matches!(
            value_to_c_arg(&Value::Integer(Integer::from(0x1000)), &NativeType::Pointer),
            Ok(CArg::Ptr(0x1000))
        ));
        // Negative reinterpreted two's-complement, like an address.
        assert!(matches!(
            value_to_c_arg(&Value::Integer(Integer::from(-1)), &NativeType::Pointer),
            Ok(CArg::Ptr(u64::MAX))
        ));
        assert!(value_to_c_arg(&Value::Str("x".into()), &NativeType::Pointer).is_err());
    }

    #[test]
    fn test_c_ret_pointer_is_integer() {
        assert_eq!(
            c_ret_to_value(0x4000, &NativeType::Pointer),
            Value::Integer(Integer::from(0x4000))
        );
    }

    #[test]
    fn test_value_to_c_arg_utf8string() {
        let ok = Value::Str("héllo".to_string());
        match value_to_c_arg(&ok, &NativeType::UTF8String).unwrap() {
            CArg::CStr(cs) => assert_eq!(cs.to_str().unwrap(), "héllo"),
            _ => panic!("expected CStr"),
        }
        // Interior NUL is rejected just like CString.
        let bad = Value::Str("a\0b".to_string());
        assert!(value_to_c_arg(&bad, &NativeType::UTF8String).is_err());
    }

    #[test]
    fn test_utf8string_is_an_alias_of_cstring_marshalling() {
        let s = Value::Str("abc".to_string());
        assert!(value_to_c_arg(&s, &NativeType::CString).is_ok());
        assert!(value_to_c_arg(&s, &NativeType::UTF8String).is_ok());
        let cs = CString::new("abc").unwrap();
        let bits = cs.as_ptr() as usize as u64;
        assert_eq!(
            c_ret_to_value(bits, &NativeType::UTF8String),
            Value::Str("abc".to_string())
        );
    }

    #[test]
    fn test_struct_return_error_as_input() {
        let sig = NativeType::Struct(vec![NativeType::I32, NativeType::I32]);
        assert!(value_to_c_arg(&Value::List(vec![]), &sig).is_err());
    }

    #[test]
    fn test_decode_struct_div_t_layout() {
        let fields = vec![NativeType::I32, NativeType::I32];
        // Low word holds quotient, high word holds remainder.
        let word = 7u64 | (2u64 << 32);
        assert_eq!(
            decode_struct(&[word, 0], &fields),
            Value::List(vec![
                Value::Integer(Integer::from(7)),
                Value::Integer(Integer::from(2))
            ])
        );
    }

    #[test]
    fn test_decode_struct_ldiv_t_two_words() {
        let fields = vec![NativeType::I64, NativeType::I64];
        assert_eq!(
            decode_struct(&[10, 3], &fields),
            Value::List(vec![
                Value::Integer(Integer::from(10)),
                Value::Integer(Integer::from(3))
            ])
        );
    }

    #[test]
    fn test_c_ret_to_value2_non_struct_ignores_high() {
        assert_eq!(
            c_ret_to_value2(5, 999, &NativeType::I64),
            Value::Integer(Integer::from(5))
        );
    }
}
