/// Tier 1: Load raw C/C++/Rust dynamic libraries and call their functions.
use crate::env::Env;
use crate::ffi::marshal::{CArg, c_ret_to_value, value_to_c_arg};
use crate::ffi::{lib_open, lib_sym};
use crate::value::{EvalError, NativeSig, NativeType, Value};

/// Open a dynamic library and return a `Value::NativeLib`.
pub fn load_native_library(path: &str, env: &Env) -> Result<Value, EvalError> {
    // Re-use an already-opened handle if available.
    if let Some(handle) = env.get_native_lib(path) {
        return Ok(Value::NativeLib {
            name: path.to_string(),
            handle,
        });
    }

    let handle = lib_open(path).map_err(|e| EvalError::FfiError(format!("LoadLibrary: {e}")))?;

    env.register_native_lib(path.to_string(), handle.clone());

    Ok(Value::NativeLib {
        name: path.to_string(),
        handle,
    })
}

/// Resolve a symbol from a `NativeLib` and return a `Value::NativeFunction`.
pub fn library_function(lib: &Value, symbol: &str, sig: NativeSig) -> Result<Value, EvalError> {
    let (lib_name, handle) = match lib {
        Value::NativeLib { name, handle } => (name.clone(), handle.clone()),
        _ => {
            return Err(EvalError::TypeError {
                expected: "NativeLib".to_string(),
                got: lib.type_name().to_string(),
            });
        }
    };

    let fn_ptr = lib_sym(&handle, symbol).ok_or_else(|| {
        EvalError::FfiError(format!("symbol \"{symbol}\" not found in \"{lib_name}\""))
    })?;

    Ok(Value::NativeFunction {
        lib_name,
        symbol_name: symbol.to_string(),
        fn_ptr,
        signature: sig,
    })
}

/// Call a `Value::NativeFunction` with Syma arguments.
pub fn call_native(fn_ptr: usize, sig: &NativeSig, args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != sig.params.len() {
        return Err(EvalError::Error(format!(
            "expected {} arguments, got {}",
            sig.params.len(),
            args.len()
        )));
    }

    // Marshal arguments.
    let c_args: Vec<CArg> = args
        .iter()
        .zip(&sig.params)
        .map(|(v, ty)| value_to_c_arg(v, ty))
        .collect::<Result<_, _>>()?;

    // Invoke — we dispatch on parameter count (0..=6 covers almost all real-world uses).
    // Each variant transmutes the function pointer to the right ABI signature,
    // passes the arguments, and collects the return value as raw bits.
    //
    // SAFETY: the caller guarantees that `fn_ptr` points to a function whose
    // C ABI matches `sig`. Violating this contract causes undefined behaviour,
    // just as calling a C function with wrong types does in any language.
    let ret_bits: u64 = unsafe { dispatch_call(fn_ptr, &c_args, &sig.ret)? };

    Ok(c_ret_to_value(ret_bits, &sig.ret))
}

// ── Low-level call dispatch ───────────────────────────────────────────────────

#[allow(unsafe_op_in_unsafe_fn)]
unsafe fn dispatch_call(fn_ptr: usize, args: &[CArg], ret: &NativeType) -> Result<u64, EvalError> {
    // We represent all return types as u64 bits and reinterpret in c_ret_to_value.
    // The union of all integer/pointer/float return ABIs fits in a 64-bit register
    // on amd64 and aarch64 (the only targets we support for now).

    match args.len() {
        0 => call0(fn_ptr, ret),
        1 => call1(fn_ptr, ret, &args[0]),
        2 => call2(fn_ptr, ret, &args[0], &args[1]),
        3 => call3(fn_ptr, ret, &args[0], &args[1], &args[2]),
        4 => call4(fn_ptr, ret, &args[0], &args[1], &args[2], &args[3]),
        5 => call5(
            fn_ptr, ret, &args[0], &args[1], &args[2], &args[3], &args[4],
        ),
        6 => call6(
            fn_ptr, ret, &args[0], &args[1], &args[2], &args[3], &args[4], &args[5],
        ),
        n => Err(EvalError::FfiError(format!(
            "too many arguments for direct FFI call ({n}); use an extension for wider signatures"
        ))),
    }
}

// Each `callN` function picks the right register type for the single return category.
// Integer/pointer returns go through a `fn(...) -> u64` cast.
// Float returns go through `fn(...) -> f64` / `fn(...) -> f32` casts, then to bits.

/// Extract the "bits" representation of a `CArg` as a `u64`.
/// This works correctly on little-endian amd64/aarch64 for all scalar types.
fn arg_bits(a: &CArg) -> u64 {
    match a {
        CArg::I32(v) => *v as i64 as u64,
        CArg::I64(v) => *v as u64,
        CArg::U32(v) => *v as u64,
        CArg::U64(v) => *v,
        CArg::F32(v) => v.to_bits() as u64,
        CArg::F64(v) => v.to_bits(),
        CArg::Bool(v) => *v as u64,
        CArg::CStr(cs) => cs.as_ptr() as usize as u64,
    }
}

/// Helper: transmute fn_ptr and call with given raw `u64` arguments.
/// We standardise on `extern "C" fn(u64, ...) -> u64` for integer-returning functions,
/// and separately handle float returns.
#[allow(unsafe_op_in_unsafe_fn)]
unsafe fn raw_call_int_ret(fn_ptr: usize, raw_args: &[u64]) -> u64 {
    // All scalar C-ABI arguments fit in 64-bit integer registers on amd64/aarch64.
    match raw_args.len() {
        0 => {
            let f: unsafe extern "C" fn() -> u64 = std::mem::transmute(fn_ptr);
            f()
        }
        1 => {
            let f: unsafe extern "C" fn(u64) -> u64 = std::mem::transmute(fn_ptr);
            f(raw_args[0])
        }
        2 => {
            let f: unsafe extern "C" fn(u64, u64) -> u64 = std::mem::transmute(fn_ptr);
            f(raw_args[0], raw_args[1])
        }
        3 => {
            let f: unsafe extern "C" fn(u64, u64, u64) -> u64 = std::mem::transmute(fn_ptr);
            f(raw_args[0], raw_args[1], raw_args[2])
        }
        4 => {
            let f: unsafe extern "C" fn(u64, u64, u64, u64) -> u64 = std::mem::transmute(fn_ptr);
            f(raw_args[0], raw_args[1], raw_args[2], raw_args[3])
        }
        5 => {
            let f: unsafe extern "C" fn(u64, u64, u64, u64, u64) -> u64 =
                std::mem::transmute(fn_ptr);
            f(
                raw_args[0],
                raw_args[1],
                raw_args[2],
                raw_args[3],
                raw_args[4],
            )
        }
        6 => {
            let f: unsafe extern "C" fn(u64, u64, u64, u64, u64, u64) -> u64 =
                std::mem::transmute(fn_ptr);
            f(
                raw_args[0],
                raw_args[1],
                raw_args[2],
                raw_args[3],
                raw_args[4],
                raw_args[5],
            )
        }
        _ => unreachable!(),
    }
}

#[allow(unsafe_op_in_unsafe_fn)]
unsafe fn raw_call_f64_ret(fn_ptr: usize, raw_args: &[u64]) -> u64 {
    match raw_args.len() {
        0 => {
            let f: unsafe extern "C" fn() -> f64 = std::mem::transmute(fn_ptr);
            f().to_bits()
        }
        1 => {
            let f: unsafe extern "C" fn(u64) -> f64 = std::mem::transmute(fn_ptr);
            f(raw_args[0]).to_bits()
        }
        2 => {
            let f: unsafe extern "C" fn(u64, u64) -> f64 = std::mem::transmute(fn_ptr);
            f(raw_args[0], raw_args[1]).to_bits()
        }
        3 => {
            let f: unsafe extern "C" fn(u64, u64, u64) -> f64 = std::mem::transmute(fn_ptr);
            f(raw_args[0], raw_args[1], raw_args[2]).to_bits()
        }
        4 => {
            let f: unsafe extern "C" fn(u64, u64, u64, u64) -> f64 = std::mem::transmute(fn_ptr);
            f(raw_args[0], raw_args[1], raw_args[2], raw_args[3]).to_bits()
        }
        5 => {
            let f: unsafe extern "C" fn(u64, u64, u64, u64, u64) -> f64 =
                std::mem::transmute(fn_ptr);
            f(
                raw_args[0],
                raw_args[1],
                raw_args[2],
                raw_args[3],
                raw_args[4],
            )
            .to_bits()
        }
        6 => {
            let f: unsafe extern "C" fn(u64, u64, u64, u64, u64, u64) -> f64 =
                std::mem::transmute(fn_ptr);
            f(
                raw_args[0],
                raw_args[1],
                raw_args[2],
                raw_args[3],
                raw_args[4],
                raw_args[5],
            )
            .to_bits()
        }
        _ => unreachable!(),
    }
}

#[allow(unsafe_op_in_unsafe_fn)]
unsafe fn raw_call_f32_ret(fn_ptr: usize, raw_args: &[u64]) -> u64 {
    match raw_args.len() {
        0 => {
            let f: unsafe extern "C" fn() -> f32 = std::mem::transmute(fn_ptr);
            f().to_bits() as u64
        }
        1 => {
            let f: unsafe extern "C" fn(u64) -> f32 = std::mem::transmute(fn_ptr);
            f(raw_args[0]).to_bits() as u64
        }
        2 => {
            let f: unsafe extern "C" fn(u64, u64) -> f32 = std::mem::transmute(fn_ptr);
            f(raw_args[0], raw_args[1]).to_bits() as u64
        }
        3 => {
            let f: unsafe extern "C" fn(u64, u64, u64) -> f32 = std::mem::transmute(fn_ptr);
            f(raw_args[0], raw_args[1], raw_args[2]).to_bits() as u64
        }
        4 => {
            let f: unsafe extern "C" fn(u64, u64, u64, u64) -> f32 = std::mem::transmute(fn_ptr);
            f(raw_args[0], raw_args[1], raw_args[2], raw_args[3]).to_bits() as u64
        }
        5 => {
            let f: unsafe extern "C" fn(u64, u64, u64, u64, u64) -> f32 =
                std::mem::transmute(fn_ptr);
            f(
                raw_args[0],
                raw_args[1],
                raw_args[2],
                raw_args[3],
                raw_args[4],
            )
            .to_bits() as u64
        }
        6 => {
            let f: unsafe extern "C" fn(u64, u64, u64, u64, u64, u64) -> f32 =
                std::mem::transmute(fn_ptr);
            f(
                raw_args[0],
                raw_args[1],
                raw_args[2],
                raw_args[3],
                raw_args[4],
                raw_args[5],
            )
            .to_bits() as u64
        }
        _ => unreachable!(),
    }
}

// Wrappers that convert CArgs to raw bits and delegate.

#[allow(unsafe_op_in_unsafe_fn)]
unsafe fn call0(fn_ptr: usize, ret: &NativeType) -> Result<u64, EvalError> {
    Ok(match ret {
        NativeType::F64 => raw_call_f64_ret(fn_ptr, &[]),
        NativeType::F32 => raw_call_f32_ret(fn_ptr, &[]),
        _ => raw_call_int_ret(fn_ptr, &[]),
    })
}

#[allow(unsafe_op_in_unsafe_fn)]
unsafe fn call1(fn_ptr: usize, ret: &NativeType, a0: &CArg) -> Result<u64, EvalError> {
    let r = [arg_bits(a0)];
    Ok(match ret {
        NativeType::F64 => raw_call_f64_ret(fn_ptr, &r),
        NativeType::F32 => raw_call_f32_ret(fn_ptr, &r),
        _ => raw_call_int_ret(fn_ptr, &r),
    })
}

#[allow(unsafe_op_in_unsafe_fn)]
unsafe fn call2(fn_ptr: usize, ret: &NativeType, a0: &CArg, a1: &CArg) -> Result<u64, EvalError> {
    let r = [arg_bits(a0), arg_bits(a1)];
    Ok(match ret {
        NativeType::F64 => raw_call_f64_ret(fn_ptr, &r),
        NativeType::F32 => raw_call_f32_ret(fn_ptr, &r),
        _ => raw_call_int_ret(fn_ptr, &r),
    })
}

#[allow(unsafe_op_in_unsafe_fn)]
unsafe fn call3(
    fn_ptr: usize,
    ret: &NativeType,
    a0: &CArg,
    a1: &CArg,
    a2: &CArg,
) -> Result<u64, EvalError> {
    let r = [arg_bits(a0), arg_bits(a1), arg_bits(a2)];
    Ok(match ret {
        NativeType::F64 => raw_call_f64_ret(fn_ptr, &r),
        NativeType::F32 => raw_call_f32_ret(fn_ptr, &r),
        _ => raw_call_int_ret(fn_ptr, &r),
    })
}

#[allow(unsafe_op_in_unsafe_fn)]
unsafe fn call4(
    fn_ptr: usize,
    ret: &NativeType,
    a0: &CArg,
    a1: &CArg,
    a2: &CArg,
    a3: &CArg,
) -> Result<u64, EvalError> {
    let r = [arg_bits(a0), arg_bits(a1), arg_bits(a2), arg_bits(a3)];
    Ok(match ret {
        NativeType::F64 => raw_call_f64_ret(fn_ptr, &r),
        NativeType::F32 => raw_call_f32_ret(fn_ptr, &r),
        _ => raw_call_int_ret(fn_ptr, &r),
    })
}

#[allow(unsafe_op_in_unsafe_fn)]
unsafe fn call5(
    fn_ptr: usize,
    ret: &NativeType,
    a0: &CArg,
    a1: &CArg,
    a2: &CArg,
    a3: &CArg,
    a4: &CArg,
) -> Result<u64, EvalError> {
    let r = [
        arg_bits(a0),
        arg_bits(a1),
        arg_bits(a2),
        arg_bits(a3),
        arg_bits(a4),
    ];
    Ok(match ret {
        NativeType::F64 => raw_call_f64_ret(fn_ptr, &r),
        NativeType::F32 => raw_call_f32_ret(fn_ptr, &r),
        _ => raw_call_int_ret(fn_ptr, &r),
    })
}

#[allow(unsafe_op_in_unsafe_fn)]
unsafe fn call6(
    fn_ptr: usize,
    ret: &NativeType,
    a0: &CArg,
    a1: &CArg,
    a2: &CArg,
    a3: &CArg,
    a4: &CArg,
    a5: &CArg,
) -> Result<u64, EvalError> {
    let r = [
        arg_bits(a0),
        arg_bits(a1),
        arg_bits(a2),
        arg_bits(a3),
        arg_bits(a4),
        arg_bits(a5),
    ];
    Ok(match ret {
        NativeType::F64 => raw_call_f64_ret(fn_ptr, &r),
        NativeType::F32 => raw_call_f32_ret(fn_ptr, &r),
        _ => raw_call_int_ret(fn_ptr, &r),
    })
}

// ── Signature parsing ─────────────────────────────────────────────────────────

/// Parse a Syma signature expression:
///   `{"Real64", "Integer32"} -> "Real64"`  (already evaluated to a Rule value)
pub fn parse_sig(sig_val: &Value) -> Result<NativeSig, EvalError> {
    match sig_val {
        Value::Rule { lhs, rhs, .. } => {
            let params = parse_type_list(lhs)?;
            let ret = parse_type_str(rhs)?;
            Ok(NativeSig { params, ret })
        }
        _ => Err(EvalError::FfiError(
            "LibraryFunction signature must be a Rule: {types} -> returnType".to_string(),
        )),
    }
}

fn parse_type_list(v: &Value) -> Result<Vec<NativeType>, EvalError> {
    match v {
        Value::List(items) => items.iter().map(parse_type_str).collect(),
        single => parse_type_str(single).map(|t| vec![t]),
    }
}

fn parse_type_str(v: &Value) -> Result<NativeType, EvalError> {
    if let Value::Str(s) = v {
        NativeType::from_syma_name(s)
            .ok_or_else(|| EvalError::FfiError(format!("unknown native type \"{s}\"")))
    } else {
        Err(EvalError::FfiError(format!(
            "native type must be a string, got {}",
            v.type_name()
        )))
    }
}
