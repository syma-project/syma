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
    if fn_ptr == 0 {
        return Err(EvalError::FfiError(
            "native function pointer is null".to_string(),
        ));
    }
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

    // Invoke — dispatch on the exact register classes (integer vs f32 vs f64)
    // and parameter count (0..=6 covers almost all real-world uses), transmute
    // the function pointer to the matching ABI signature, and collect the return
    // value as raw bits.
    //
    // SAFETY: the caller guarantees that `fn_ptr` points to a function whose
    // C ABI matches `sig`. Violating this contract causes undefined behaviour,
    // just as calling a C function with wrong types does in any language.
    let ret_bits: u64 = unsafe { dispatch_call(fn_ptr, &c_args, &sig.params, &sig.ret)? };

    Ok(c_ret_to_value(ret_bits, &sig.ret))
}

// ── Low-level call dispatch ───────────────────────────────────────────────────

// All scalar C-ABI arguments are either passed in general-purpose integer
// registers (integers, pointers, booleans) or in SSE registers (f32/f64). To
// call a function with the correct register classes we transmute `fn_ptr` to an
// `extern "C"` function whose parameter types match the declared signature and
// let the compiler place each argument in its ABI-mandated register.

/// Extract the raw bit pattern of an integer/pointer/boolean `CArg`.
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

fn arg_f32(a: &CArg) -> f32 {
    match a {
        CArg::F32(v) => *v,
        _ => f32::from_bits(arg_bits(a) as u32),
    }
}

fn arg_f64(a: &CArg) -> f64 {
    match a {
        CArg::F64(v) => *v,
        _ => f64::from_bits(arg_bits(a)),
    }
}

/// Fetch an argument as the concrete type the C ABI needs for its register class.
trait CArgValue: Copy {
    fn from_carg(a: &CArg) -> Self;
}
impl CArgValue for u64 {
    fn from_carg(a: &CArg) -> Self {
        arg_bits(a)
    }
}
impl CArgValue for f32 {
    fn from_carg(a: &CArg) -> Self {
        arg_f32(a)
    }
}
impl CArgValue for f64 {
    fn from_carg(a: &CArg) -> Self {
        arg_f64(a)
    }
}

/// Call `fn_ptr` as `extern "C" fn($($ty),*) -> $ret`, decoding arguments from
/// the `CArg` slice in order.
macro_rules! invoke {
    ($fn_ptr:expr, $ret:ty, $args:expr, [$($ty:ty),*]) => {{
        let __f: unsafe extern "C" fn($($ty),*) -> $ret =
            unsafe { std::mem::transmute($fn_ptr) };
        let mut __it = $args.iter();
        let __r = unsafe { __f($(<$ty as CArgValue>::from_carg(__it.next().unwrap())),*) };
        let _ = &mut __it;
        __r
    }};
}

/// Dispatch on the return type and invoke with the given parameter classes.
macro_rules! call_typed {
    ($fn_ptr:expr, $ret:expr, $args:expr, [$($ty:ty),*]) => {
        match *$ret {
            NativeType::F64 => invoke!($fn_ptr, f64, $args, [$($ty),*]).to_bits(),
            NativeType::F32 => invoke!($fn_ptr, f32, $args, [$($ty),*]).to_bits() as u64,
            _ => invoke!($fn_ptr, u64, $args, [$($ty),*]),
        }
    };
}

/// Register-class code for a parameter type: 0 = integer/pointer, 1 = f32,
/// 2 = f64. The dispatch key is the base-3 encoding of the parameter classes.
fn arg_class(ty: &NativeType) -> u32 {
    match ty {
        NativeType::F32 => 1,
        NativeType::F64 => 2,
        _ => 0,
    }
}

/// Base-3 encoding of the parameter register classes, most-significant first.
///
/// `wrapping_*` is used so that signatures wider than 20 parameters do not
/// panic in debug builds (3^21 exceeds `u32`); such signatures are reported as
/// unsupported after the key is computed.
fn sig_key(params: &[NativeType]) -> u32 {
    params
        .iter()
        .fold(0u32, |k, p| k.wrapping_mul(3).wrapping_add(arg_class(p)))
}

/// Call a C function via the correct ABI, returning raw return-value bits.
///
/// Supports up to six parameters (covering essentially all real-world uses)
/// with arbitrary integer/float mixtures. Longer signatures need an extension.
#[allow(unsafe_op_in_unsafe_fn, clippy::too_many_lines)]
unsafe fn dispatch_call(
    fn_ptr: usize,
    args: &[CArg],
    params: &[NativeType],
    ret: &NativeType,
) -> Result<u64, EvalError> {
    if params.len() > 6 {
        return Err(EvalError::FfiError(format!(
            "too many arguments for direct FFI call ({}); use an extension for wider signatures",
            params.len()
        )));
    }
    let key = sig_key(params);
    Ok(match (args.len(), key) {
        (0, 0) => call_typed!(fn_ptr, ret, args, []),
        (1, 0) => call_typed!(fn_ptr, ret, args, [u64]),
        (1, 1) => call_typed!(fn_ptr, ret, args, [f32]),
        (1, 2) => call_typed!(fn_ptr, ret, args, [f64]),
        (2, 0) => call_typed!(fn_ptr, ret, args, [u64, u64]),
        (2, 1) => call_typed!(fn_ptr, ret, args, [u64, f32]),
        (2, 2) => call_typed!(fn_ptr, ret, args, [u64, f64]),
        (2, 3) => call_typed!(fn_ptr, ret, args, [f32, u64]),
        (2, 4) => call_typed!(fn_ptr, ret, args, [f32, f32]),
        (2, 5) => call_typed!(fn_ptr, ret, args, [f32, f64]),
        (2, 6) => call_typed!(fn_ptr, ret, args, [f64, u64]),
        (2, 7) => call_typed!(fn_ptr, ret, args, [f64, f32]),
        (2, 8) => call_typed!(fn_ptr, ret, args, [f64, f64]),
        (3, 0) => call_typed!(fn_ptr, ret, args, [u64, u64, u64]),
        (3, 1) => call_typed!(fn_ptr, ret, args, [u64, u64, f32]),
        (3, 2) => call_typed!(fn_ptr, ret, args, [u64, u64, f64]),
        (3, 3) => call_typed!(fn_ptr, ret, args, [u64, f32, u64]),
        (3, 4) => call_typed!(fn_ptr, ret, args, [u64, f32, f32]),
        (3, 5) => call_typed!(fn_ptr, ret, args, [u64, f32, f64]),
        (3, 6) => call_typed!(fn_ptr, ret, args, [u64, f64, u64]),
        (3, 7) => call_typed!(fn_ptr, ret, args, [u64, f64, f32]),
        (3, 8) => call_typed!(fn_ptr, ret, args, [u64, f64, f64]),
        (3, 9) => call_typed!(fn_ptr, ret, args, [f32, u64, u64]),
        (3, 10) => call_typed!(fn_ptr, ret, args, [f32, u64, f32]),
        (3, 11) => call_typed!(fn_ptr, ret, args, [f32, u64, f64]),
        (3, 12) => call_typed!(fn_ptr, ret, args, [f32, f32, u64]),
        (3, 13) => call_typed!(fn_ptr, ret, args, [f32, f32, f32]),
        (3, 14) => call_typed!(fn_ptr, ret, args, [f32, f32, f64]),
        (3, 15) => call_typed!(fn_ptr, ret, args, [f32, f64, u64]),
        (3, 16) => call_typed!(fn_ptr, ret, args, [f32, f64, f32]),
        (3, 17) => call_typed!(fn_ptr, ret, args, [f32, f64, f64]),
        (3, 18) => call_typed!(fn_ptr, ret, args, [f64, u64, u64]),
        (3, 19) => call_typed!(fn_ptr, ret, args, [f64, u64, f32]),
        (3, 20) => call_typed!(fn_ptr, ret, args, [f64, u64, f64]),
        (3, 21) => call_typed!(fn_ptr, ret, args, [f64, f32, u64]),
        (3, 22) => call_typed!(fn_ptr, ret, args, [f64, f32, f32]),
        (3, 23) => call_typed!(fn_ptr, ret, args, [f64, f32, f64]),
        (3, 24) => call_typed!(fn_ptr, ret, args, [f64, f64, u64]),
        (3, 25) => call_typed!(fn_ptr, ret, args, [f64, f64, f32]),
        (3, 26) => call_typed!(fn_ptr, ret, args, [f64, f64, f64]),
        (4, 0) => call_typed!(fn_ptr, ret, args, [u64, u64, u64, u64]),
        (4, 1) => call_typed!(fn_ptr, ret, args, [u64, u64, u64, f32]),
        (4, 2) => call_typed!(fn_ptr, ret, args, [u64, u64, u64, f64]),
        (4, 3) => call_typed!(fn_ptr, ret, args, [u64, u64, f32, u64]),
        (4, 4) => call_typed!(fn_ptr, ret, args, [u64, u64, f32, f32]),
        (4, 5) => call_typed!(fn_ptr, ret, args, [u64, u64, f32, f64]),
        (4, 6) => call_typed!(fn_ptr, ret, args, [u64, u64, f64, u64]),
        (4, 7) => call_typed!(fn_ptr, ret, args, [u64, u64, f64, f32]),
        (4, 8) => call_typed!(fn_ptr, ret, args, [u64, u64, f64, f64]),
        (4, 9) => call_typed!(fn_ptr, ret, args, [u64, f32, u64, u64]),
        (4, 10) => call_typed!(fn_ptr, ret, args, [u64, f32, u64, f32]),
        (4, 11) => call_typed!(fn_ptr, ret, args, [u64, f32, u64, f64]),
        (4, 12) => call_typed!(fn_ptr, ret, args, [u64, f32, f32, u64]),
        (4, 13) => call_typed!(fn_ptr, ret, args, [u64, f32, f32, f32]),
        (4, 14) => call_typed!(fn_ptr, ret, args, [u64, f32, f32, f64]),
        (4, 15) => call_typed!(fn_ptr, ret, args, [u64, f32, f64, u64]),
        (4, 16) => call_typed!(fn_ptr, ret, args, [u64, f32, f64, f32]),
        (4, 17) => call_typed!(fn_ptr, ret, args, [u64, f32, f64, f64]),
        (4, 18) => call_typed!(fn_ptr, ret, args, [u64, f64, u64, u64]),
        (4, 19) => call_typed!(fn_ptr, ret, args, [u64, f64, u64, f32]),
        (4, 20) => call_typed!(fn_ptr, ret, args, [u64, f64, u64, f64]),
        (4, 21) => call_typed!(fn_ptr, ret, args, [u64, f64, f32, u64]),
        (4, 22) => call_typed!(fn_ptr, ret, args, [u64, f64, f32, f32]),
        (4, 23) => call_typed!(fn_ptr, ret, args, [u64, f64, f32, f64]),
        (4, 24) => call_typed!(fn_ptr, ret, args, [u64, f64, f64, u64]),
        (4, 25) => call_typed!(fn_ptr, ret, args, [u64, f64, f64, f32]),
        (4, 26) => call_typed!(fn_ptr, ret, args, [u64, f64, f64, f64]),
        (4, 27) => call_typed!(fn_ptr, ret, args, [f32, u64, u64, u64]),
        (4, 28) => call_typed!(fn_ptr, ret, args, [f32, u64, u64, f32]),
        (4, 29) => call_typed!(fn_ptr, ret, args, [f32, u64, u64, f64]),
        (4, 30) => call_typed!(fn_ptr, ret, args, [f32, u64, f32, u64]),
        (4, 31) => call_typed!(fn_ptr, ret, args, [f32, u64, f32, f32]),
        (4, 32) => call_typed!(fn_ptr, ret, args, [f32, u64, f32, f64]),
        (4, 33) => call_typed!(fn_ptr, ret, args, [f32, u64, f64, u64]),
        (4, 34) => call_typed!(fn_ptr, ret, args, [f32, u64, f64, f32]),
        (4, 35) => call_typed!(fn_ptr, ret, args, [f32, u64, f64, f64]),
        (4, 36) => call_typed!(fn_ptr, ret, args, [f32, f32, u64, u64]),
        (4, 37) => call_typed!(fn_ptr, ret, args, [f32, f32, u64, f32]),
        (4, 38) => call_typed!(fn_ptr, ret, args, [f32, f32, u64, f64]),
        (4, 39) => call_typed!(fn_ptr, ret, args, [f32, f32, f32, u64]),
        (4, 40) => call_typed!(fn_ptr, ret, args, [f32, f32, f32, f32]),
        (4, 41) => call_typed!(fn_ptr, ret, args, [f32, f32, f32, f64]),
        (4, 42) => call_typed!(fn_ptr, ret, args, [f32, f32, f64, u64]),
        (4, 43) => call_typed!(fn_ptr, ret, args, [f32, f32, f64, f32]),
        (4, 44) => call_typed!(fn_ptr, ret, args, [f32, f32, f64, f64]),
        (4, 45) => call_typed!(fn_ptr, ret, args, [f32, f64, u64, u64]),
        (4, 46) => call_typed!(fn_ptr, ret, args, [f32, f64, u64, f32]),
        (4, 47) => call_typed!(fn_ptr, ret, args, [f32, f64, u64, f64]),
        (4, 48) => call_typed!(fn_ptr, ret, args, [f32, f64, f32, u64]),
        (4, 49) => call_typed!(fn_ptr, ret, args, [f32, f64, f32, f32]),
        (4, 50) => call_typed!(fn_ptr, ret, args, [f32, f64, f32, f64]),
        (4, 51) => call_typed!(fn_ptr, ret, args, [f32, f64, f64, u64]),
        (4, 52) => call_typed!(fn_ptr, ret, args, [f32, f64, f64, f32]),
        (4, 53) => call_typed!(fn_ptr, ret, args, [f32, f64, f64, f64]),
        (4, 54) => call_typed!(fn_ptr, ret, args, [f64, u64, u64, u64]),
        (4, 55) => call_typed!(fn_ptr, ret, args, [f64, u64, u64, f32]),
        (4, 56) => call_typed!(fn_ptr, ret, args, [f64, u64, u64, f64]),
        (4, 57) => call_typed!(fn_ptr, ret, args, [f64, u64, f32, u64]),
        (4, 58) => call_typed!(fn_ptr, ret, args, [f64, u64, f32, f32]),
        (4, 59) => call_typed!(fn_ptr, ret, args, [f64, u64, f32, f64]),
        (4, 60) => call_typed!(fn_ptr, ret, args, [f64, u64, f64, u64]),
        (4, 61) => call_typed!(fn_ptr, ret, args, [f64, u64, f64, f32]),
        (4, 62) => call_typed!(fn_ptr, ret, args, [f64, u64, f64, f64]),
        (4, 63) => call_typed!(fn_ptr, ret, args, [f64, f32, u64, u64]),
        (4, 64) => call_typed!(fn_ptr, ret, args, [f64, f32, u64, f32]),
        (4, 65) => call_typed!(fn_ptr, ret, args, [f64, f32, u64, f64]),
        (4, 66) => call_typed!(fn_ptr, ret, args, [f64, f32, f32, u64]),
        (4, 67) => call_typed!(fn_ptr, ret, args, [f64, f32, f32, f32]),
        (4, 68) => call_typed!(fn_ptr, ret, args, [f64, f32, f32, f64]),
        (4, 69) => call_typed!(fn_ptr, ret, args, [f64, f32, f64, u64]),
        (4, 70) => call_typed!(fn_ptr, ret, args, [f64, f32, f64, f32]),
        (4, 71) => call_typed!(fn_ptr, ret, args, [f64, f32, f64, f64]),
        (4, 72) => call_typed!(fn_ptr, ret, args, [f64, f64, u64, u64]),
        (4, 73) => call_typed!(fn_ptr, ret, args, [f64, f64, u64, f32]),
        (4, 74) => call_typed!(fn_ptr, ret, args, [f64, f64, u64, f64]),
        (4, 75) => call_typed!(fn_ptr, ret, args, [f64, f64, f32, u64]),
        (4, 76) => call_typed!(fn_ptr, ret, args, [f64, f64, f32, f32]),
        (4, 77) => call_typed!(fn_ptr, ret, args, [f64, f64, f32, f64]),
        (4, 78) => call_typed!(fn_ptr, ret, args, [f64, f64, f64, u64]),
        (4, 79) => call_typed!(fn_ptr, ret, args, [f64, f64, f64, f32]),
        (4, 80) => call_typed!(fn_ptr, ret, args, [f64, f64, f64, f64]),
        (5, 0) => call_typed!(fn_ptr, ret, args, [u64, u64, u64, u64, u64]),
        (5, 1) => call_typed!(fn_ptr, ret, args, [u64, u64, u64, u64, f32]),
        (5, 2) => call_typed!(fn_ptr, ret, args, [u64, u64, u64, u64, f64]),
        (5, 3) => call_typed!(fn_ptr, ret, args, [u64, u64, u64, f32, u64]),
        (5, 4) => call_typed!(fn_ptr, ret, args, [u64, u64, u64, f32, f32]),
        (5, 5) => call_typed!(fn_ptr, ret, args, [u64, u64, u64, f32, f64]),
        (5, 6) => call_typed!(fn_ptr, ret, args, [u64, u64, u64, f64, u64]),
        (5, 7) => call_typed!(fn_ptr, ret, args, [u64, u64, u64, f64, f32]),
        (5, 8) => call_typed!(fn_ptr, ret, args, [u64, u64, u64, f64, f64]),
        (5, 9) => call_typed!(fn_ptr, ret, args, [u64, u64, f32, u64, u64]),
        (5, 10) => call_typed!(fn_ptr, ret, args, [u64, u64, f32, u64, f32]),
        (5, 11) => call_typed!(fn_ptr, ret, args, [u64, u64, f32, u64, f64]),
        (5, 12) => call_typed!(fn_ptr, ret, args, [u64, u64, f32, f32, u64]),
        (5, 13) => call_typed!(fn_ptr, ret, args, [u64, u64, f32, f32, f32]),
        (5, 14) => call_typed!(fn_ptr, ret, args, [u64, u64, f32, f32, f64]),
        (5, 15) => call_typed!(fn_ptr, ret, args, [u64, u64, f32, f64, u64]),
        (5, 16) => call_typed!(fn_ptr, ret, args, [u64, u64, f32, f64, f32]),
        (5, 17) => call_typed!(fn_ptr, ret, args, [u64, u64, f32, f64, f64]),
        (5, 18) => call_typed!(fn_ptr, ret, args, [u64, u64, f64, u64, u64]),
        (5, 19) => call_typed!(fn_ptr, ret, args, [u64, u64, f64, u64, f32]),
        (5, 20) => call_typed!(fn_ptr, ret, args, [u64, u64, f64, u64, f64]),
        (5, 21) => call_typed!(fn_ptr, ret, args, [u64, u64, f64, f32, u64]),
        (5, 22) => call_typed!(fn_ptr, ret, args, [u64, u64, f64, f32, f32]),
        (5, 23) => call_typed!(fn_ptr, ret, args, [u64, u64, f64, f32, f64]),
        (5, 24) => call_typed!(fn_ptr, ret, args, [u64, u64, f64, f64, u64]),
        (5, 25) => call_typed!(fn_ptr, ret, args, [u64, u64, f64, f64, f32]),
        (5, 26) => call_typed!(fn_ptr, ret, args, [u64, u64, f64, f64, f64]),
        (5, 27) => call_typed!(fn_ptr, ret, args, [u64, f32, u64, u64, u64]),
        (5, 28) => call_typed!(fn_ptr, ret, args, [u64, f32, u64, u64, f32]),
        (5, 29) => call_typed!(fn_ptr, ret, args, [u64, f32, u64, u64, f64]),
        (5, 30) => call_typed!(fn_ptr, ret, args, [u64, f32, u64, f32, u64]),
        (5, 31) => call_typed!(fn_ptr, ret, args, [u64, f32, u64, f32, f32]),
        (5, 32) => call_typed!(fn_ptr, ret, args, [u64, f32, u64, f32, f64]),
        (5, 33) => call_typed!(fn_ptr, ret, args, [u64, f32, u64, f64, u64]),
        (5, 34) => call_typed!(fn_ptr, ret, args, [u64, f32, u64, f64, f32]),
        (5, 35) => call_typed!(fn_ptr, ret, args, [u64, f32, u64, f64, f64]),
        (5, 36) => call_typed!(fn_ptr, ret, args, [u64, f32, f32, u64, u64]),
        (5, 37) => call_typed!(fn_ptr, ret, args, [u64, f32, f32, u64, f32]),
        (5, 38) => call_typed!(fn_ptr, ret, args, [u64, f32, f32, u64, f64]),
        (5, 39) => call_typed!(fn_ptr, ret, args, [u64, f32, f32, f32, u64]),
        (5, 40) => call_typed!(fn_ptr, ret, args, [u64, f32, f32, f32, f32]),
        (5, 41) => call_typed!(fn_ptr, ret, args, [u64, f32, f32, f32, f64]),
        (5, 42) => call_typed!(fn_ptr, ret, args, [u64, f32, f32, f64, u64]),
        (5, 43) => call_typed!(fn_ptr, ret, args, [u64, f32, f32, f64, f32]),
        (5, 44) => call_typed!(fn_ptr, ret, args, [u64, f32, f32, f64, f64]),
        (5, 45) => call_typed!(fn_ptr, ret, args, [u64, f32, f64, u64, u64]),
        (5, 46) => call_typed!(fn_ptr, ret, args, [u64, f32, f64, u64, f32]),
        (5, 47) => call_typed!(fn_ptr, ret, args, [u64, f32, f64, u64, f64]),
        (5, 48) => call_typed!(fn_ptr, ret, args, [u64, f32, f64, f32, u64]),
        (5, 49) => call_typed!(fn_ptr, ret, args, [u64, f32, f64, f32, f32]),
        (5, 50) => call_typed!(fn_ptr, ret, args, [u64, f32, f64, f32, f64]),
        (5, 51) => call_typed!(fn_ptr, ret, args, [u64, f32, f64, f64, u64]),
        (5, 52) => call_typed!(fn_ptr, ret, args, [u64, f32, f64, f64, f32]),
        (5, 53) => call_typed!(fn_ptr, ret, args, [u64, f32, f64, f64, f64]),
        (5, 54) => call_typed!(fn_ptr, ret, args, [u64, f64, u64, u64, u64]),
        (5, 55) => call_typed!(fn_ptr, ret, args, [u64, f64, u64, u64, f32]),
        (5, 56) => call_typed!(fn_ptr, ret, args, [u64, f64, u64, u64, f64]),
        (5, 57) => call_typed!(fn_ptr, ret, args, [u64, f64, u64, f32, u64]),
        (5, 58) => call_typed!(fn_ptr, ret, args, [u64, f64, u64, f32, f32]),
        (5, 59) => call_typed!(fn_ptr, ret, args, [u64, f64, u64, f32, f64]),
        (5, 60) => call_typed!(fn_ptr, ret, args, [u64, f64, u64, f64, u64]),
        (5, 61) => call_typed!(fn_ptr, ret, args, [u64, f64, u64, f64, f32]),
        (5, 62) => call_typed!(fn_ptr, ret, args, [u64, f64, u64, f64, f64]),
        (5, 63) => call_typed!(fn_ptr, ret, args, [u64, f64, f32, u64, u64]),
        (5, 64) => call_typed!(fn_ptr, ret, args, [u64, f64, f32, u64, f32]),
        (5, 65) => call_typed!(fn_ptr, ret, args, [u64, f64, f32, u64, f64]),
        (5, 66) => call_typed!(fn_ptr, ret, args, [u64, f64, f32, f32, u64]),
        (5, 67) => call_typed!(fn_ptr, ret, args, [u64, f64, f32, f32, f32]),
        (5, 68) => call_typed!(fn_ptr, ret, args, [u64, f64, f32, f32, f64]),
        (5, 69) => call_typed!(fn_ptr, ret, args, [u64, f64, f32, f64, u64]),
        (5, 70) => call_typed!(fn_ptr, ret, args, [u64, f64, f32, f64, f32]),
        (5, 71) => call_typed!(fn_ptr, ret, args, [u64, f64, f32, f64, f64]),
        (5, 72) => call_typed!(fn_ptr, ret, args, [u64, f64, f64, u64, u64]),
        (5, 73) => call_typed!(fn_ptr, ret, args, [u64, f64, f64, u64, f32]),
        (5, 74) => call_typed!(fn_ptr, ret, args, [u64, f64, f64, u64, f64]),
        (5, 75) => call_typed!(fn_ptr, ret, args, [u64, f64, f64, f32, u64]),
        (5, 76) => call_typed!(fn_ptr, ret, args, [u64, f64, f64, f32, f32]),
        (5, 77) => call_typed!(fn_ptr, ret, args, [u64, f64, f64, f32, f64]),
        (5, 78) => call_typed!(fn_ptr, ret, args, [u64, f64, f64, f64, u64]),
        (5, 79) => call_typed!(fn_ptr, ret, args, [u64, f64, f64, f64, f32]),
        (5, 80) => call_typed!(fn_ptr, ret, args, [u64, f64, f64, f64, f64]),
        (5, 81) => call_typed!(fn_ptr, ret, args, [f32, u64, u64, u64, u64]),
        (5, 82) => call_typed!(fn_ptr, ret, args, [f32, u64, u64, u64, f32]),
        (5, 83) => call_typed!(fn_ptr, ret, args, [f32, u64, u64, u64, f64]),
        (5, 84) => call_typed!(fn_ptr, ret, args, [f32, u64, u64, f32, u64]),
        (5, 85) => call_typed!(fn_ptr, ret, args, [f32, u64, u64, f32, f32]),
        (5, 86) => call_typed!(fn_ptr, ret, args, [f32, u64, u64, f32, f64]),
        (5, 87) => call_typed!(fn_ptr, ret, args, [f32, u64, u64, f64, u64]),
        (5, 88) => call_typed!(fn_ptr, ret, args, [f32, u64, u64, f64, f32]),
        (5, 89) => call_typed!(fn_ptr, ret, args, [f32, u64, u64, f64, f64]),
        (5, 90) => call_typed!(fn_ptr, ret, args, [f32, u64, f32, u64, u64]),
        (5, 91) => call_typed!(fn_ptr, ret, args, [f32, u64, f32, u64, f32]),
        (5, 92) => call_typed!(fn_ptr, ret, args, [f32, u64, f32, u64, f64]),
        (5, 93) => call_typed!(fn_ptr, ret, args, [f32, u64, f32, f32, u64]),
        (5, 94) => call_typed!(fn_ptr, ret, args, [f32, u64, f32, f32, f32]),
        (5, 95) => call_typed!(fn_ptr, ret, args, [f32, u64, f32, f32, f64]),
        (5, 96) => call_typed!(fn_ptr, ret, args, [f32, u64, f32, f64, u64]),
        (5, 97) => call_typed!(fn_ptr, ret, args, [f32, u64, f32, f64, f32]),
        (5, 98) => call_typed!(fn_ptr, ret, args, [f32, u64, f32, f64, f64]),
        (5, 99) => call_typed!(fn_ptr, ret, args, [f32, u64, f64, u64, u64]),
        (5, 100) => call_typed!(fn_ptr, ret, args, [f32, u64, f64, u64, f32]),
        (5, 101) => call_typed!(fn_ptr, ret, args, [f32, u64, f64, u64, f64]),
        (5, 102) => call_typed!(fn_ptr, ret, args, [f32, u64, f64, f32, u64]),
        (5, 103) => call_typed!(fn_ptr, ret, args, [f32, u64, f64, f32, f32]),
        (5, 104) => call_typed!(fn_ptr, ret, args, [f32, u64, f64, f32, f64]),
        (5, 105) => call_typed!(fn_ptr, ret, args, [f32, u64, f64, f64, u64]),
        (5, 106) => call_typed!(fn_ptr, ret, args, [f32, u64, f64, f64, f32]),
        (5, 107) => call_typed!(fn_ptr, ret, args, [f32, u64, f64, f64, f64]),
        (5, 108) => call_typed!(fn_ptr, ret, args, [f32, f32, u64, u64, u64]),
        (5, 109) => call_typed!(fn_ptr, ret, args, [f32, f32, u64, u64, f32]),
        (5, 110) => call_typed!(fn_ptr, ret, args, [f32, f32, u64, u64, f64]),
        (5, 111) => call_typed!(fn_ptr, ret, args, [f32, f32, u64, f32, u64]),
        (5, 112) => call_typed!(fn_ptr, ret, args, [f32, f32, u64, f32, f32]),
        (5, 113) => call_typed!(fn_ptr, ret, args, [f32, f32, u64, f32, f64]),
        (5, 114) => call_typed!(fn_ptr, ret, args, [f32, f32, u64, f64, u64]),
        (5, 115) => call_typed!(fn_ptr, ret, args, [f32, f32, u64, f64, f32]),
        (5, 116) => call_typed!(fn_ptr, ret, args, [f32, f32, u64, f64, f64]),
        (5, 117) => call_typed!(fn_ptr, ret, args, [f32, f32, f32, u64, u64]),
        (5, 118) => call_typed!(fn_ptr, ret, args, [f32, f32, f32, u64, f32]),
        (5, 119) => call_typed!(fn_ptr, ret, args, [f32, f32, f32, u64, f64]),
        (5, 120) => call_typed!(fn_ptr, ret, args, [f32, f32, f32, f32, u64]),
        (5, 121) => call_typed!(fn_ptr, ret, args, [f32, f32, f32, f32, f32]),
        (5, 122) => call_typed!(fn_ptr, ret, args, [f32, f32, f32, f32, f64]),
        (5, 123) => call_typed!(fn_ptr, ret, args, [f32, f32, f32, f64, u64]),
        (5, 124) => call_typed!(fn_ptr, ret, args, [f32, f32, f32, f64, f32]),
        (5, 125) => call_typed!(fn_ptr, ret, args, [f32, f32, f32, f64, f64]),
        (5, 126) => call_typed!(fn_ptr, ret, args, [f32, f32, f64, u64, u64]),
        (5, 127) => call_typed!(fn_ptr, ret, args, [f32, f32, f64, u64, f32]),
        (5, 128) => call_typed!(fn_ptr, ret, args, [f32, f32, f64, u64, f64]),
        (5, 129) => call_typed!(fn_ptr, ret, args, [f32, f32, f64, f32, u64]),
        (5, 130) => call_typed!(fn_ptr, ret, args, [f32, f32, f64, f32, f32]),
        (5, 131) => call_typed!(fn_ptr, ret, args, [f32, f32, f64, f32, f64]),
        (5, 132) => call_typed!(fn_ptr, ret, args, [f32, f32, f64, f64, u64]),
        (5, 133) => call_typed!(fn_ptr, ret, args, [f32, f32, f64, f64, f32]),
        (5, 134) => call_typed!(fn_ptr, ret, args, [f32, f32, f64, f64, f64]),
        (5, 135) => call_typed!(fn_ptr, ret, args, [f32, f64, u64, u64, u64]),
        (5, 136) => call_typed!(fn_ptr, ret, args, [f32, f64, u64, u64, f32]),
        (5, 137) => call_typed!(fn_ptr, ret, args, [f32, f64, u64, u64, f64]),
        (5, 138) => call_typed!(fn_ptr, ret, args, [f32, f64, u64, f32, u64]),
        (5, 139) => call_typed!(fn_ptr, ret, args, [f32, f64, u64, f32, f32]),
        (5, 140) => call_typed!(fn_ptr, ret, args, [f32, f64, u64, f32, f64]),
        (5, 141) => call_typed!(fn_ptr, ret, args, [f32, f64, u64, f64, u64]),
        (5, 142) => call_typed!(fn_ptr, ret, args, [f32, f64, u64, f64, f32]),
        (5, 143) => call_typed!(fn_ptr, ret, args, [f32, f64, u64, f64, f64]),
        (5, 144) => call_typed!(fn_ptr, ret, args, [f32, f64, f32, u64, u64]),
        (5, 145) => call_typed!(fn_ptr, ret, args, [f32, f64, f32, u64, f32]),
        (5, 146) => call_typed!(fn_ptr, ret, args, [f32, f64, f32, u64, f64]),
        (5, 147) => call_typed!(fn_ptr, ret, args, [f32, f64, f32, f32, u64]),
        (5, 148) => call_typed!(fn_ptr, ret, args, [f32, f64, f32, f32, f32]),
        (5, 149) => call_typed!(fn_ptr, ret, args, [f32, f64, f32, f32, f64]),
        (5, 150) => call_typed!(fn_ptr, ret, args, [f32, f64, f32, f64, u64]),
        (5, 151) => call_typed!(fn_ptr, ret, args, [f32, f64, f32, f64, f32]),
        (5, 152) => call_typed!(fn_ptr, ret, args, [f32, f64, f32, f64, f64]),
        (5, 153) => call_typed!(fn_ptr, ret, args, [f32, f64, f64, u64, u64]),
        (5, 154) => call_typed!(fn_ptr, ret, args, [f32, f64, f64, u64, f32]),
        (5, 155) => call_typed!(fn_ptr, ret, args, [f32, f64, f64, u64, f64]),
        (5, 156) => call_typed!(fn_ptr, ret, args, [f32, f64, f64, f32, u64]),
        (5, 157) => call_typed!(fn_ptr, ret, args, [f32, f64, f64, f32, f32]),
        (5, 158) => call_typed!(fn_ptr, ret, args, [f32, f64, f64, f32, f64]),
        (5, 159) => call_typed!(fn_ptr, ret, args, [f32, f64, f64, f64, u64]),
        (5, 160) => call_typed!(fn_ptr, ret, args, [f32, f64, f64, f64, f32]),
        (5, 161) => call_typed!(fn_ptr, ret, args, [f32, f64, f64, f64, f64]),
        (5, 162) => call_typed!(fn_ptr, ret, args, [f64, u64, u64, u64, u64]),
        (5, 163) => call_typed!(fn_ptr, ret, args, [f64, u64, u64, u64, f32]),
        (5, 164) => call_typed!(fn_ptr, ret, args, [f64, u64, u64, u64, f64]),
        (5, 165) => call_typed!(fn_ptr, ret, args, [f64, u64, u64, f32, u64]),
        (5, 166) => call_typed!(fn_ptr, ret, args, [f64, u64, u64, f32, f32]),
        (5, 167) => call_typed!(fn_ptr, ret, args, [f64, u64, u64, f32, f64]),
        (5, 168) => call_typed!(fn_ptr, ret, args, [f64, u64, u64, f64, u64]),
        (5, 169) => call_typed!(fn_ptr, ret, args, [f64, u64, u64, f64, f32]),
        (5, 170) => call_typed!(fn_ptr, ret, args, [f64, u64, u64, f64, f64]),
        (5, 171) => call_typed!(fn_ptr, ret, args, [f64, u64, f32, u64, u64]),
        (5, 172) => call_typed!(fn_ptr, ret, args, [f64, u64, f32, u64, f32]),
        (5, 173) => call_typed!(fn_ptr, ret, args, [f64, u64, f32, u64, f64]),
        (5, 174) => call_typed!(fn_ptr, ret, args, [f64, u64, f32, f32, u64]),
        (5, 175) => call_typed!(fn_ptr, ret, args, [f64, u64, f32, f32, f32]),
        (5, 176) => call_typed!(fn_ptr, ret, args, [f64, u64, f32, f32, f64]),
        (5, 177) => call_typed!(fn_ptr, ret, args, [f64, u64, f32, f64, u64]),
        (5, 178) => call_typed!(fn_ptr, ret, args, [f64, u64, f32, f64, f32]),
        (5, 179) => call_typed!(fn_ptr, ret, args, [f64, u64, f32, f64, f64]),
        (5, 180) => call_typed!(fn_ptr, ret, args, [f64, u64, f64, u64, u64]),
        (5, 181) => call_typed!(fn_ptr, ret, args, [f64, u64, f64, u64, f32]),
        (5, 182) => call_typed!(fn_ptr, ret, args, [f64, u64, f64, u64, f64]),
        (5, 183) => call_typed!(fn_ptr, ret, args, [f64, u64, f64, f32, u64]),
        (5, 184) => call_typed!(fn_ptr, ret, args, [f64, u64, f64, f32, f32]),
        (5, 185) => call_typed!(fn_ptr, ret, args, [f64, u64, f64, f32, f64]),
        (5, 186) => call_typed!(fn_ptr, ret, args, [f64, u64, f64, f64, u64]),
        (5, 187) => call_typed!(fn_ptr, ret, args, [f64, u64, f64, f64, f32]),
        (5, 188) => call_typed!(fn_ptr, ret, args, [f64, u64, f64, f64, f64]),
        (5, 189) => call_typed!(fn_ptr, ret, args, [f64, f32, u64, u64, u64]),
        (5, 190) => call_typed!(fn_ptr, ret, args, [f64, f32, u64, u64, f32]),
        (5, 191) => call_typed!(fn_ptr, ret, args, [f64, f32, u64, u64, f64]),
        (5, 192) => call_typed!(fn_ptr, ret, args, [f64, f32, u64, f32, u64]),
        (5, 193) => call_typed!(fn_ptr, ret, args, [f64, f32, u64, f32, f32]),
        (5, 194) => call_typed!(fn_ptr, ret, args, [f64, f32, u64, f32, f64]),
        (5, 195) => call_typed!(fn_ptr, ret, args, [f64, f32, u64, f64, u64]),
        (5, 196) => call_typed!(fn_ptr, ret, args, [f64, f32, u64, f64, f32]),
        (5, 197) => call_typed!(fn_ptr, ret, args, [f64, f32, u64, f64, f64]),
        (5, 198) => call_typed!(fn_ptr, ret, args, [f64, f32, f32, u64, u64]),
        (5, 199) => call_typed!(fn_ptr, ret, args, [f64, f32, f32, u64, f32]),
        (5, 200) => call_typed!(fn_ptr, ret, args, [f64, f32, f32, u64, f64]),
        (5, 201) => call_typed!(fn_ptr, ret, args, [f64, f32, f32, f32, u64]),
        (5, 202) => call_typed!(fn_ptr, ret, args, [f64, f32, f32, f32, f32]),
        (5, 203) => call_typed!(fn_ptr, ret, args, [f64, f32, f32, f32, f64]),
        (5, 204) => call_typed!(fn_ptr, ret, args, [f64, f32, f32, f64, u64]),
        (5, 205) => call_typed!(fn_ptr, ret, args, [f64, f32, f32, f64, f32]),
        (5, 206) => call_typed!(fn_ptr, ret, args, [f64, f32, f32, f64, f64]),
        (5, 207) => call_typed!(fn_ptr, ret, args, [f64, f32, f64, u64, u64]),
        (5, 208) => call_typed!(fn_ptr, ret, args, [f64, f32, f64, u64, f32]),
        (5, 209) => call_typed!(fn_ptr, ret, args, [f64, f32, f64, u64, f64]),
        (5, 210) => call_typed!(fn_ptr, ret, args, [f64, f32, f64, f32, u64]),
        (5, 211) => call_typed!(fn_ptr, ret, args, [f64, f32, f64, f32, f32]),
        (5, 212) => call_typed!(fn_ptr, ret, args, [f64, f32, f64, f32, f64]),
        (5, 213) => call_typed!(fn_ptr, ret, args, [f64, f32, f64, f64, u64]),
        (5, 214) => call_typed!(fn_ptr, ret, args, [f64, f32, f64, f64, f32]),
        (5, 215) => call_typed!(fn_ptr, ret, args, [f64, f32, f64, f64, f64]),
        (5, 216) => call_typed!(fn_ptr, ret, args, [f64, f64, u64, u64, u64]),
        (5, 217) => call_typed!(fn_ptr, ret, args, [f64, f64, u64, u64, f32]),
        (5, 218) => call_typed!(fn_ptr, ret, args, [f64, f64, u64, u64, f64]),
        (5, 219) => call_typed!(fn_ptr, ret, args, [f64, f64, u64, f32, u64]),
        (5, 220) => call_typed!(fn_ptr, ret, args, [f64, f64, u64, f32, f32]),
        (5, 221) => call_typed!(fn_ptr, ret, args, [f64, f64, u64, f32, f64]),
        (5, 222) => call_typed!(fn_ptr, ret, args, [f64, f64, u64, f64, u64]),
        (5, 223) => call_typed!(fn_ptr, ret, args, [f64, f64, u64, f64, f32]),
        (5, 224) => call_typed!(fn_ptr, ret, args, [f64, f64, u64, f64, f64]),
        (5, 225) => call_typed!(fn_ptr, ret, args, [f64, f64, f32, u64, u64]),
        (5, 226) => call_typed!(fn_ptr, ret, args, [f64, f64, f32, u64, f32]),
        (5, 227) => call_typed!(fn_ptr, ret, args, [f64, f64, f32, u64, f64]),
        (5, 228) => call_typed!(fn_ptr, ret, args, [f64, f64, f32, f32, u64]),
        (5, 229) => call_typed!(fn_ptr, ret, args, [f64, f64, f32, f32, f32]),
        (5, 230) => call_typed!(fn_ptr, ret, args, [f64, f64, f32, f32, f64]),
        (5, 231) => call_typed!(fn_ptr, ret, args, [f64, f64, f32, f64, u64]),
        (5, 232) => call_typed!(fn_ptr, ret, args, [f64, f64, f32, f64, f32]),
        (5, 233) => call_typed!(fn_ptr, ret, args, [f64, f64, f32, f64, f64]),
        (5, 234) => call_typed!(fn_ptr, ret, args, [f64, f64, f64, u64, u64]),
        (5, 235) => call_typed!(fn_ptr, ret, args, [f64, f64, f64, u64, f32]),
        (5, 236) => call_typed!(fn_ptr, ret, args, [f64, f64, f64, u64, f64]),
        (5, 237) => call_typed!(fn_ptr, ret, args, [f64, f64, f64, f32, u64]),
        (5, 238) => call_typed!(fn_ptr, ret, args, [f64, f64, f64, f32, f32]),
        (5, 239) => call_typed!(fn_ptr, ret, args, [f64, f64, f64, f32, f64]),
        (5, 240) => call_typed!(fn_ptr, ret, args, [f64, f64, f64, f64, u64]),
        (5, 241) => call_typed!(fn_ptr, ret, args, [f64, f64, f64, f64, f32]),
        (5, 242) => call_typed!(fn_ptr, ret, args, [f64, f64, f64, f64, f64]),
        (6, 0) => call_typed!(fn_ptr, ret, args, [u64, u64, u64, u64, u64, u64]),
        (6, 1) => call_typed!(fn_ptr, ret, args, [u64, u64, u64, u64, u64, f32]),
        (6, 2) => call_typed!(fn_ptr, ret, args, [u64, u64, u64, u64, u64, f64]),
        (6, 3) => call_typed!(fn_ptr, ret, args, [u64, u64, u64, u64, f32, u64]),
        (6, 4) => call_typed!(fn_ptr, ret, args, [u64, u64, u64, u64, f32, f32]),
        (6, 5) => call_typed!(fn_ptr, ret, args, [u64, u64, u64, u64, f32, f64]),
        (6, 6) => call_typed!(fn_ptr, ret, args, [u64, u64, u64, u64, f64, u64]),
        (6, 7) => call_typed!(fn_ptr, ret, args, [u64, u64, u64, u64, f64, f32]),
        (6, 8) => call_typed!(fn_ptr, ret, args, [u64, u64, u64, u64, f64, f64]),
        (6, 9) => call_typed!(fn_ptr, ret, args, [u64, u64, u64, f32, u64, u64]),
        (6, 10) => call_typed!(fn_ptr, ret, args, [u64, u64, u64, f32, u64, f32]),
        (6, 11) => call_typed!(fn_ptr, ret, args, [u64, u64, u64, f32, u64, f64]),
        (6, 12) => call_typed!(fn_ptr, ret, args, [u64, u64, u64, f32, f32, u64]),
        (6, 13) => call_typed!(fn_ptr, ret, args, [u64, u64, u64, f32, f32, f32]),
        (6, 14) => call_typed!(fn_ptr, ret, args, [u64, u64, u64, f32, f32, f64]),
        (6, 15) => call_typed!(fn_ptr, ret, args, [u64, u64, u64, f32, f64, u64]),
        (6, 16) => call_typed!(fn_ptr, ret, args, [u64, u64, u64, f32, f64, f32]),
        (6, 17) => call_typed!(fn_ptr, ret, args, [u64, u64, u64, f32, f64, f64]),
        (6, 18) => call_typed!(fn_ptr, ret, args, [u64, u64, u64, f64, u64, u64]),
        (6, 19) => call_typed!(fn_ptr, ret, args, [u64, u64, u64, f64, u64, f32]),
        (6, 20) => call_typed!(fn_ptr, ret, args, [u64, u64, u64, f64, u64, f64]),
        (6, 21) => call_typed!(fn_ptr, ret, args, [u64, u64, u64, f64, f32, u64]),
        (6, 22) => call_typed!(fn_ptr, ret, args, [u64, u64, u64, f64, f32, f32]),
        (6, 23) => call_typed!(fn_ptr, ret, args, [u64, u64, u64, f64, f32, f64]),
        (6, 24) => call_typed!(fn_ptr, ret, args, [u64, u64, u64, f64, f64, u64]),
        (6, 25) => call_typed!(fn_ptr, ret, args, [u64, u64, u64, f64, f64, f32]),
        (6, 26) => call_typed!(fn_ptr, ret, args, [u64, u64, u64, f64, f64, f64]),
        (6, 27) => call_typed!(fn_ptr, ret, args, [u64, u64, f32, u64, u64, u64]),
        (6, 28) => call_typed!(fn_ptr, ret, args, [u64, u64, f32, u64, u64, f32]),
        (6, 29) => call_typed!(fn_ptr, ret, args, [u64, u64, f32, u64, u64, f64]),
        (6, 30) => call_typed!(fn_ptr, ret, args, [u64, u64, f32, u64, f32, u64]),
        (6, 31) => call_typed!(fn_ptr, ret, args, [u64, u64, f32, u64, f32, f32]),
        (6, 32) => call_typed!(fn_ptr, ret, args, [u64, u64, f32, u64, f32, f64]),
        (6, 33) => call_typed!(fn_ptr, ret, args, [u64, u64, f32, u64, f64, u64]),
        (6, 34) => call_typed!(fn_ptr, ret, args, [u64, u64, f32, u64, f64, f32]),
        (6, 35) => call_typed!(fn_ptr, ret, args, [u64, u64, f32, u64, f64, f64]),
        (6, 36) => call_typed!(fn_ptr, ret, args, [u64, u64, f32, f32, u64, u64]),
        (6, 37) => call_typed!(fn_ptr, ret, args, [u64, u64, f32, f32, u64, f32]),
        (6, 38) => call_typed!(fn_ptr, ret, args, [u64, u64, f32, f32, u64, f64]),
        (6, 39) => call_typed!(fn_ptr, ret, args, [u64, u64, f32, f32, f32, u64]),
        (6, 40) => call_typed!(fn_ptr, ret, args, [u64, u64, f32, f32, f32, f32]),
        (6, 41) => call_typed!(fn_ptr, ret, args, [u64, u64, f32, f32, f32, f64]),
        (6, 42) => call_typed!(fn_ptr, ret, args, [u64, u64, f32, f32, f64, u64]),
        (6, 43) => call_typed!(fn_ptr, ret, args, [u64, u64, f32, f32, f64, f32]),
        (6, 44) => call_typed!(fn_ptr, ret, args, [u64, u64, f32, f32, f64, f64]),
        (6, 45) => call_typed!(fn_ptr, ret, args, [u64, u64, f32, f64, u64, u64]),
        (6, 46) => call_typed!(fn_ptr, ret, args, [u64, u64, f32, f64, u64, f32]),
        (6, 47) => call_typed!(fn_ptr, ret, args, [u64, u64, f32, f64, u64, f64]),
        (6, 48) => call_typed!(fn_ptr, ret, args, [u64, u64, f32, f64, f32, u64]),
        (6, 49) => call_typed!(fn_ptr, ret, args, [u64, u64, f32, f64, f32, f32]),
        (6, 50) => call_typed!(fn_ptr, ret, args, [u64, u64, f32, f64, f32, f64]),
        (6, 51) => call_typed!(fn_ptr, ret, args, [u64, u64, f32, f64, f64, u64]),
        (6, 52) => call_typed!(fn_ptr, ret, args, [u64, u64, f32, f64, f64, f32]),
        (6, 53) => call_typed!(fn_ptr, ret, args, [u64, u64, f32, f64, f64, f64]),
        (6, 54) => call_typed!(fn_ptr, ret, args, [u64, u64, f64, u64, u64, u64]),
        (6, 55) => call_typed!(fn_ptr, ret, args, [u64, u64, f64, u64, u64, f32]),
        (6, 56) => call_typed!(fn_ptr, ret, args, [u64, u64, f64, u64, u64, f64]),
        (6, 57) => call_typed!(fn_ptr, ret, args, [u64, u64, f64, u64, f32, u64]),
        (6, 58) => call_typed!(fn_ptr, ret, args, [u64, u64, f64, u64, f32, f32]),
        (6, 59) => call_typed!(fn_ptr, ret, args, [u64, u64, f64, u64, f32, f64]),
        (6, 60) => call_typed!(fn_ptr, ret, args, [u64, u64, f64, u64, f64, u64]),
        (6, 61) => call_typed!(fn_ptr, ret, args, [u64, u64, f64, u64, f64, f32]),
        (6, 62) => call_typed!(fn_ptr, ret, args, [u64, u64, f64, u64, f64, f64]),
        (6, 63) => call_typed!(fn_ptr, ret, args, [u64, u64, f64, f32, u64, u64]),
        (6, 64) => call_typed!(fn_ptr, ret, args, [u64, u64, f64, f32, u64, f32]),
        (6, 65) => call_typed!(fn_ptr, ret, args, [u64, u64, f64, f32, u64, f64]),
        (6, 66) => call_typed!(fn_ptr, ret, args, [u64, u64, f64, f32, f32, u64]),
        (6, 67) => call_typed!(fn_ptr, ret, args, [u64, u64, f64, f32, f32, f32]),
        (6, 68) => call_typed!(fn_ptr, ret, args, [u64, u64, f64, f32, f32, f64]),
        (6, 69) => call_typed!(fn_ptr, ret, args, [u64, u64, f64, f32, f64, u64]),
        (6, 70) => call_typed!(fn_ptr, ret, args, [u64, u64, f64, f32, f64, f32]),
        (6, 71) => call_typed!(fn_ptr, ret, args, [u64, u64, f64, f32, f64, f64]),
        (6, 72) => call_typed!(fn_ptr, ret, args, [u64, u64, f64, f64, u64, u64]),
        (6, 73) => call_typed!(fn_ptr, ret, args, [u64, u64, f64, f64, u64, f32]),
        (6, 74) => call_typed!(fn_ptr, ret, args, [u64, u64, f64, f64, u64, f64]),
        (6, 75) => call_typed!(fn_ptr, ret, args, [u64, u64, f64, f64, f32, u64]),
        (6, 76) => call_typed!(fn_ptr, ret, args, [u64, u64, f64, f64, f32, f32]),
        (6, 77) => call_typed!(fn_ptr, ret, args, [u64, u64, f64, f64, f32, f64]),
        (6, 78) => call_typed!(fn_ptr, ret, args, [u64, u64, f64, f64, f64, u64]),
        (6, 79) => call_typed!(fn_ptr, ret, args, [u64, u64, f64, f64, f64, f32]),
        (6, 80) => call_typed!(fn_ptr, ret, args, [u64, u64, f64, f64, f64, f64]),
        (6, 81) => call_typed!(fn_ptr, ret, args, [u64, f32, u64, u64, u64, u64]),
        (6, 82) => call_typed!(fn_ptr, ret, args, [u64, f32, u64, u64, u64, f32]),
        (6, 83) => call_typed!(fn_ptr, ret, args, [u64, f32, u64, u64, u64, f64]),
        (6, 84) => call_typed!(fn_ptr, ret, args, [u64, f32, u64, u64, f32, u64]),
        (6, 85) => call_typed!(fn_ptr, ret, args, [u64, f32, u64, u64, f32, f32]),
        (6, 86) => call_typed!(fn_ptr, ret, args, [u64, f32, u64, u64, f32, f64]),
        (6, 87) => call_typed!(fn_ptr, ret, args, [u64, f32, u64, u64, f64, u64]),
        (6, 88) => call_typed!(fn_ptr, ret, args, [u64, f32, u64, u64, f64, f32]),
        (6, 89) => call_typed!(fn_ptr, ret, args, [u64, f32, u64, u64, f64, f64]),
        (6, 90) => call_typed!(fn_ptr, ret, args, [u64, f32, u64, f32, u64, u64]),
        (6, 91) => call_typed!(fn_ptr, ret, args, [u64, f32, u64, f32, u64, f32]),
        (6, 92) => call_typed!(fn_ptr, ret, args, [u64, f32, u64, f32, u64, f64]),
        (6, 93) => call_typed!(fn_ptr, ret, args, [u64, f32, u64, f32, f32, u64]),
        (6, 94) => call_typed!(fn_ptr, ret, args, [u64, f32, u64, f32, f32, f32]),
        (6, 95) => call_typed!(fn_ptr, ret, args, [u64, f32, u64, f32, f32, f64]),
        (6, 96) => call_typed!(fn_ptr, ret, args, [u64, f32, u64, f32, f64, u64]),
        (6, 97) => call_typed!(fn_ptr, ret, args, [u64, f32, u64, f32, f64, f32]),
        (6, 98) => call_typed!(fn_ptr, ret, args, [u64, f32, u64, f32, f64, f64]),
        (6, 99) => call_typed!(fn_ptr, ret, args, [u64, f32, u64, f64, u64, u64]),
        (6, 100) => call_typed!(fn_ptr, ret, args, [u64, f32, u64, f64, u64, f32]),
        (6, 101) => call_typed!(fn_ptr, ret, args, [u64, f32, u64, f64, u64, f64]),
        (6, 102) => call_typed!(fn_ptr, ret, args, [u64, f32, u64, f64, f32, u64]),
        (6, 103) => call_typed!(fn_ptr, ret, args, [u64, f32, u64, f64, f32, f32]),
        (6, 104) => call_typed!(fn_ptr, ret, args, [u64, f32, u64, f64, f32, f64]),
        (6, 105) => call_typed!(fn_ptr, ret, args, [u64, f32, u64, f64, f64, u64]),
        (6, 106) => call_typed!(fn_ptr, ret, args, [u64, f32, u64, f64, f64, f32]),
        (6, 107) => call_typed!(fn_ptr, ret, args, [u64, f32, u64, f64, f64, f64]),
        (6, 108) => call_typed!(fn_ptr, ret, args, [u64, f32, f32, u64, u64, u64]),
        (6, 109) => call_typed!(fn_ptr, ret, args, [u64, f32, f32, u64, u64, f32]),
        (6, 110) => call_typed!(fn_ptr, ret, args, [u64, f32, f32, u64, u64, f64]),
        (6, 111) => call_typed!(fn_ptr, ret, args, [u64, f32, f32, u64, f32, u64]),
        (6, 112) => call_typed!(fn_ptr, ret, args, [u64, f32, f32, u64, f32, f32]),
        (6, 113) => call_typed!(fn_ptr, ret, args, [u64, f32, f32, u64, f32, f64]),
        (6, 114) => call_typed!(fn_ptr, ret, args, [u64, f32, f32, u64, f64, u64]),
        (6, 115) => call_typed!(fn_ptr, ret, args, [u64, f32, f32, u64, f64, f32]),
        (6, 116) => call_typed!(fn_ptr, ret, args, [u64, f32, f32, u64, f64, f64]),
        (6, 117) => call_typed!(fn_ptr, ret, args, [u64, f32, f32, f32, u64, u64]),
        (6, 118) => call_typed!(fn_ptr, ret, args, [u64, f32, f32, f32, u64, f32]),
        (6, 119) => call_typed!(fn_ptr, ret, args, [u64, f32, f32, f32, u64, f64]),
        (6, 120) => call_typed!(fn_ptr, ret, args, [u64, f32, f32, f32, f32, u64]),
        (6, 121) => call_typed!(fn_ptr, ret, args, [u64, f32, f32, f32, f32, f32]),
        (6, 122) => call_typed!(fn_ptr, ret, args, [u64, f32, f32, f32, f32, f64]),
        (6, 123) => call_typed!(fn_ptr, ret, args, [u64, f32, f32, f32, f64, u64]),
        (6, 124) => call_typed!(fn_ptr, ret, args, [u64, f32, f32, f32, f64, f32]),
        (6, 125) => call_typed!(fn_ptr, ret, args, [u64, f32, f32, f32, f64, f64]),
        (6, 126) => call_typed!(fn_ptr, ret, args, [u64, f32, f32, f64, u64, u64]),
        (6, 127) => call_typed!(fn_ptr, ret, args, [u64, f32, f32, f64, u64, f32]),
        (6, 128) => call_typed!(fn_ptr, ret, args, [u64, f32, f32, f64, u64, f64]),
        (6, 129) => call_typed!(fn_ptr, ret, args, [u64, f32, f32, f64, f32, u64]),
        (6, 130) => call_typed!(fn_ptr, ret, args, [u64, f32, f32, f64, f32, f32]),
        (6, 131) => call_typed!(fn_ptr, ret, args, [u64, f32, f32, f64, f32, f64]),
        (6, 132) => call_typed!(fn_ptr, ret, args, [u64, f32, f32, f64, f64, u64]),
        (6, 133) => call_typed!(fn_ptr, ret, args, [u64, f32, f32, f64, f64, f32]),
        (6, 134) => call_typed!(fn_ptr, ret, args, [u64, f32, f32, f64, f64, f64]),
        (6, 135) => call_typed!(fn_ptr, ret, args, [u64, f32, f64, u64, u64, u64]),
        (6, 136) => call_typed!(fn_ptr, ret, args, [u64, f32, f64, u64, u64, f32]),
        (6, 137) => call_typed!(fn_ptr, ret, args, [u64, f32, f64, u64, u64, f64]),
        (6, 138) => call_typed!(fn_ptr, ret, args, [u64, f32, f64, u64, f32, u64]),
        (6, 139) => call_typed!(fn_ptr, ret, args, [u64, f32, f64, u64, f32, f32]),
        (6, 140) => call_typed!(fn_ptr, ret, args, [u64, f32, f64, u64, f32, f64]),
        (6, 141) => call_typed!(fn_ptr, ret, args, [u64, f32, f64, u64, f64, u64]),
        (6, 142) => call_typed!(fn_ptr, ret, args, [u64, f32, f64, u64, f64, f32]),
        (6, 143) => call_typed!(fn_ptr, ret, args, [u64, f32, f64, u64, f64, f64]),
        (6, 144) => call_typed!(fn_ptr, ret, args, [u64, f32, f64, f32, u64, u64]),
        (6, 145) => call_typed!(fn_ptr, ret, args, [u64, f32, f64, f32, u64, f32]),
        (6, 146) => call_typed!(fn_ptr, ret, args, [u64, f32, f64, f32, u64, f64]),
        (6, 147) => call_typed!(fn_ptr, ret, args, [u64, f32, f64, f32, f32, u64]),
        (6, 148) => call_typed!(fn_ptr, ret, args, [u64, f32, f64, f32, f32, f32]),
        (6, 149) => call_typed!(fn_ptr, ret, args, [u64, f32, f64, f32, f32, f64]),
        (6, 150) => call_typed!(fn_ptr, ret, args, [u64, f32, f64, f32, f64, u64]),
        (6, 151) => call_typed!(fn_ptr, ret, args, [u64, f32, f64, f32, f64, f32]),
        (6, 152) => call_typed!(fn_ptr, ret, args, [u64, f32, f64, f32, f64, f64]),
        (6, 153) => call_typed!(fn_ptr, ret, args, [u64, f32, f64, f64, u64, u64]),
        (6, 154) => call_typed!(fn_ptr, ret, args, [u64, f32, f64, f64, u64, f32]),
        (6, 155) => call_typed!(fn_ptr, ret, args, [u64, f32, f64, f64, u64, f64]),
        (6, 156) => call_typed!(fn_ptr, ret, args, [u64, f32, f64, f64, f32, u64]),
        (6, 157) => call_typed!(fn_ptr, ret, args, [u64, f32, f64, f64, f32, f32]),
        (6, 158) => call_typed!(fn_ptr, ret, args, [u64, f32, f64, f64, f32, f64]),
        (6, 159) => call_typed!(fn_ptr, ret, args, [u64, f32, f64, f64, f64, u64]),
        (6, 160) => call_typed!(fn_ptr, ret, args, [u64, f32, f64, f64, f64, f32]),
        (6, 161) => call_typed!(fn_ptr, ret, args, [u64, f32, f64, f64, f64, f64]),
        (6, 162) => call_typed!(fn_ptr, ret, args, [u64, f64, u64, u64, u64, u64]),
        (6, 163) => call_typed!(fn_ptr, ret, args, [u64, f64, u64, u64, u64, f32]),
        (6, 164) => call_typed!(fn_ptr, ret, args, [u64, f64, u64, u64, u64, f64]),
        (6, 165) => call_typed!(fn_ptr, ret, args, [u64, f64, u64, u64, f32, u64]),
        (6, 166) => call_typed!(fn_ptr, ret, args, [u64, f64, u64, u64, f32, f32]),
        (6, 167) => call_typed!(fn_ptr, ret, args, [u64, f64, u64, u64, f32, f64]),
        (6, 168) => call_typed!(fn_ptr, ret, args, [u64, f64, u64, u64, f64, u64]),
        (6, 169) => call_typed!(fn_ptr, ret, args, [u64, f64, u64, u64, f64, f32]),
        (6, 170) => call_typed!(fn_ptr, ret, args, [u64, f64, u64, u64, f64, f64]),
        (6, 171) => call_typed!(fn_ptr, ret, args, [u64, f64, u64, f32, u64, u64]),
        (6, 172) => call_typed!(fn_ptr, ret, args, [u64, f64, u64, f32, u64, f32]),
        (6, 173) => call_typed!(fn_ptr, ret, args, [u64, f64, u64, f32, u64, f64]),
        (6, 174) => call_typed!(fn_ptr, ret, args, [u64, f64, u64, f32, f32, u64]),
        (6, 175) => call_typed!(fn_ptr, ret, args, [u64, f64, u64, f32, f32, f32]),
        (6, 176) => call_typed!(fn_ptr, ret, args, [u64, f64, u64, f32, f32, f64]),
        (6, 177) => call_typed!(fn_ptr, ret, args, [u64, f64, u64, f32, f64, u64]),
        (6, 178) => call_typed!(fn_ptr, ret, args, [u64, f64, u64, f32, f64, f32]),
        (6, 179) => call_typed!(fn_ptr, ret, args, [u64, f64, u64, f32, f64, f64]),
        (6, 180) => call_typed!(fn_ptr, ret, args, [u64, f64, u64, f64, u64, u64]),
        (6, 181) => call_typed!(fn_ptr, ret, args, [u64, f64, u64, f64, u64, f32]),
        (6, 182) => call_typed!(fn_ptr, ret, args, [u64, f64, u64, f64, u64, f64]),
        (6, 183) => call_typed!(fn_ptr, ret, args, [u64, f64, u64, f64, f32, u64]),
        (6, 184) => call_typed!(fn_ptr, ret, args, [u64, f64, u64, f64, f32, f32]),
        (6, 185) => call_typed!(fn_ptr, ret, args, [u64, f64, u64, f64, f32, f64]),
        (6, 186) => call_typed!(fn_ptr, ret, args, [u64, f64, u64, f64, f64, u64]),
        (6, 187) => call_typed!(fn_ptr, ret, args, [u64, f64, u64, f64, f64, f32]),
        (6, 188) => call_typed!(fn_ptr, ret, args, [u64, f64, u64, f64, f64, f64]),
        (6, 189) => call_typed!(fn_ptr, ret, args, [u64, f64, f32, u64, u64, u64]),
        (6, 190) => call_typed!(fn_ptr, ret, args, [u64, f64, f32, u64, u64, f32]),
        (6, 191) => call_typed!(fn_ptr, ret, args, [u64, f64, f32, u64, u64, f64]),
        (6, 192) => call_typed!(fn_ptr, ret, args, [u64, f64, f32, u64, f32, u64]),
        (6, 193) => call_typed!(fn_ptr, ret, args, [u64, f64, f32, u64, f32, f32]),
        (6, 194) => call_typed!(fn_ptr, ret, args, [u64, f64, f32, u64, f32, f64]),
        (6, 195) => call_typed!(fn_ptr, ret, args, [u64, f64, f32, u64, f64, u64]),
        (6, 196) => call_typed!(fn_ptr, ret, args, [u64, f64, f32, u64, f64, f32]),
        (6, 197) => call_typed!(fn_ptr, ret, args, [u64, f64, f32, u64, f64, f64]),
        (6, 198) => call_typed!(fn_ptr, ret, args, [u64, f64, f32, f32, u64, u64]),
        (6, 199) => call_typed!(fn_ptr, ret, args, [u64, f64, f32, f32, u64, f32]),
        (6, 200) => call_typed!(fn_ptr, ret, args, [u64, f64, f32, f32, u64, f64]),
        (6, 201) => call_typed!(fn_ptr, ret, args, [u64, f64, f32, f32, f32, u64]),
        (6, 202) => call_typed!(fn_ptr, ret, args, [u64, f64, f32, f32, f32, f32]),
        (6, 203) => call_typed!(fn_ptr, ret, args, [u64, f64, f32, f32, f32, f64]),
        (6, 204) => call_typed!(fn_ptr, ret, args, [u64, f64, f32, f32, f64, u64]),
        (6, 205) => call_typed!(fn_ptr, ret, args, [u64, f64, f32, f32, f64, f32]),
        (6, 206) => call_typed!(fn_ptr, ret, args, [u64, f64, f32, f32, f64, f64]),
        (6, 207) => call_typed!(fn_ptr, ret, args, [u64, f64, f32, f64, u64, u64]),
        (6, 208) => call_typed!(fn_ptr, ret, args, [u64, f64, f32, f64, u64, f32]),
        (6, 209) => call_typed!(fn_ptr, ret, args, [u64, f64, f32, f64, u64, f64]),
        (6, 210) => call_typed!(fn_ptr, ret, args, [u64, f64, f32, f64, f32, u64]),
        (6, 211) => call_typed!(fn_ptr, ret, args, [u64, f64, f32, f64, f32, f32]),
        (6, 212) => call_typed!(fn_ptr, ret, args, [u64, f64, f32, f64, f32, f64]),
        (6, 213) => call_typed!(fn_ptr, ret, args, [u64, f64, f32, f64, f64, u64]),
        (6, 214) => call_typed!(fn_ptr, ret, args, [u64, f64, f32, f64, f64, f32]),
        (6, 215) => call_typed!(fn_ptr, ret, args, [u64, f64, f32, f64, f64, f64]),
        (6, 216) => call_typed!(fn_ptr, ret, args, [u64, f64, f64, u64, u64, u64]),
        (6, 217) => call_typed!(fn_ptr, ret, args, [u64, f64, f64, u64, u64, f32]),
        (6, 218) => call_typed!(fn_ptr, ret, args, [u64, f64, f64, u64, u64, f64]),
        (6, 219) => call_typed!(fn_ptr, ret, args, [u64, f64, f64, u64, f32, u64]),
        (6, 220) => call_typed!(fn_ptr, ret, args, [u64, f64, f64, u64, f32, f32]),
        (6, 221) => call_typed!(fn_ptr, ret, args, [u64, f64, f64, u64, f32, f64]),
        (6, 222) => call_typed!(fn_ptr, ret, args, [u64, f64, f64, u64, f64, u64]),
        (6, 223) => call_typed!(fn_ptr, ret, args, [u64, f64, f64, u64, f64, f32]),
        (6, 224) => call_typed!(fn_ptr, ret, args, [u64, f64, f64, u64, f64, f64]),
        (6, 225) => call_typed!(fn_ptr, ret, args, [u64, f64, f64, f32, u64, u64]),
        (6, 226) => call_typed!(fn_ptr, ret, args, [u64, f64, f64, f32, u64, f32]),
        (6, 227) => call_typed!(fn_ptr, ret, args, [u64, f64, f64, f32, u64, f64]),
        (6, 228) => call_typed!(fn_ptr, ret, args, [u64, f64, f64, f32, f32, u64]),
        (6, 229) => call_typed!(fn_ptr, ret, args, [u64, f64, f64, f32, f32, f32]),
        (6, 230) => call_typed!(fn_ptr, ret, args, [u64, f64, f64, f32, f32, f64]),
        (6, 231) => call_typed!(fn_ptr, ret, args, [u64, f64, f64, f32, f64, u64]),
        (6, 232) => call_typed!(fn_ptr, ret, args, [u64, f64, f64, f32, f64, f32]),
        (6, 233) => call_typed!(fn_ptr, ret, args, [u64, f64, f64, f32, f64, f64]),
        (6, 234) => call_typed!(fn_ptr, ret, args, [u64, f64, f64, f64, u64, u64]),
        (6, 235) => call_typed!(fn_ptr, ret, args, [u64, f64, f64, f64, u64, f32]),
        (6, 236) => call_typed!(fn_ptr, ret, args, [u64, f64, f64, f64, u64, f64]),
        (6, 237) => call_typed!(fn_ptr, ret, args, [u64, f64, f64, f64, f32, u64]),
        (6, 238) => call_typed!(fn_ptr, ret, args, [u64, f64, f64, f64, f32, f32]),
        (6, 239) => call_typed!(fn_ptr, ret, args, [u64, f64, f64, f64, f32, f64]),
        (6, 240) => call_typed!(fn_ptr, ret, args, [u64, f64, f64, f64, f64, u64]),
        (6, 241) => call_typed!(fn_ptr, ret, args, [u64, f64, f64, f64, f64, f32]),
        (6, 242) => call_typed!(fn_ptr, ret, args, [u64, f64, f64, f64, f64, f64]),
        (6, 243) => call_typed!(fn_ptr, ret, args, [f32, u64, u64, u64, u64, u64]),
        (6, 244) => call_typed!(fn_ptr, ret, args, [f32, u64, u64, u64, u64, f32]),
        (6, 245) => call_typed!(fn_ptr, ret, args, [f32, u64, u64, u64, u64, f64]),
        (6, 246) => call_typed!(fn_ptr, ret, args, [f32, u64, u64, u64, f32, u64]),
        (6, 247) => call_typed!(fn_ptr, ret, args, [f32, u64, u64, u64, f32, f32]),
        (6, 248) => call_typed!(fn_ptr, ret, args, [f32, u64, u64, u64, f32, f64]),
        (6, 249) => call_typed!(fn_ptr, ret, args, [f32, u64, u64, u64, f64, u64]),
        (6, 250) => call_typed!(fn_ptr, ret, args, [f32, u64, u64, u64, f64, f32]),
        (6, 251) => call_typed!(fn_ptr, ret, args, [f32, u64, u64, u64, f64, f64]),
        (6, 252) => call_typed!(fn_ptr, ret, args, [f32, u64, u64, f32, u64, u64]),
        (6, 253) => call_typed!(fn_ptr, ret, args, [f32, u64, u64, f32, u64, f32]),
        (6, 254) => call_typed!(fn_ptr, ret, args, [f32, u64, u64, f32, u64, f64]),
        (6, 255) => call_typed!(fn_ptr, ret, args, [f32, u64, u64, f32, f32, u64]),
        (6, 256) => call_typed!(fn_ptr, ret, args, [f32, u64, u64, f32, f32, f32]),
        (6, 257) => call_typed!(fn_ptr, ret, args, [f32, u64, u64, f32, f32, f64]),
        (6, 258) => call_typed!(fn_ptr, ret, args, [f32, u64, u64, f32, f64, u64]),
        (6, 259) => call_typed!(fn_ptr, ret, args, [f32, u64, u64, f32, f64, f32]),
        (6, 260) => call_typed!(fn_ptr, ret, args, [f32, u64, u64, f32, f64, f64]),
        (6, 261) => call_typed!(fn_ptr, ret, args, [f32, u64, u64, f64, u64, u64]),
        (6, 262) => call_typed!(fn_ptr, ret, args, [f32, u64, u64, f64, u64, f32]),
        (6, 263) => call_typed!(fn_ptr, ret, args, [f32, u64, u64, f64, u64, f64]),
        (6, 264) => call_typed!(fn_ptr, ret, args, [f32, u64, u64, f64, f32, u64]),
        (6, 265) => call_typed!(fn_ptr, ret, args, [f32, u64, u64, f64, f32, f32]),
        (6, 266) => call_typed!(fn_ptr, ret, args, [f32, u64, u64, f64, f32, f64]),
        (6, 267) => call_typed!(fn_ptr, ret, args, [f32, u64, u64, f64, f64, u64]),
        (6, 268) => call_typed!(fn_ptr, ret, args, [f32, u64, u64, f64, f64, f32]),
        (6, 269) => call_typed!(fn_ptr, ret, args, [f32, u64, u64, f64, f64, f64]),
        (6, 270) => call_typed!(fn_ptr, ret, args, [f32, u64, f32, u64, u64, u64]),
        (6, 271) => call_typed!(fn_ptr, ret, args, [f32, u64, f32, u64, u64, f32]),
        (6, 272) => call_typed!(fn_ptr, ret, args, [f32, u64, f32, u64, u64, f64]),
        (6, 273) => call_typed!(fn_ptr, ret, args, [f32, u64, f32, u64, f32, u64]),
        (6, 274) => call_typed!(fn_ptr, ret, args, [f32, u64, f32, u64, f32, f32]),
        (6, 275) => call_typed!(fn_ptr, ret, args, [f32, u64, f32, u64, f32, f64]),
        (6, 276) => call_typed!(fn_ptr, ret, args, [f32, u64, f32, u64, f64, u64]),
        (6, 277) => call_typed!(fn_ptr, ret, args, [f32, u64, f32, u64, f64, f32]),
        (6, 278) => call_typed!(fn_ptr, ret, args, [f32, u64, f32, u64, f64, f64]),
        (6, 279) => call_typed!(fn_ptr, ret, args, [f32, u64, f32, f32, u64, u64]),
        (6, 280) => call_typed!(fn_ptr, ret, args, [f32, u64, f32, f32, u64, f32]),
        (6, 281) => call_typed!(fn_ptr, ret, args, [f32, u64, f32, f32, u64, f64]),
        (6, 282) => call_typed!(fn_ptr, ret, args, [f32, u64, f32, f32, f32, u64]),
        (6, 283) => call_typed!(fn_ptr, ret, args, [f32, u64, f32, f32, f32, f32]),
        (6, 284) => call_typed!(fn_ptr, ret, args, [f32, u64, f32, f32, f32, f64]),
        (6, 285) => call_typed!(fn_ptr, ret, args, [f32, u64, f32, f32, f64, u64]),
        (6, 286) => call_typed!(fn_ptr, ret, args, [f32, u64, f32, f32, f64, f32]),
        (6, 287) => call_typed!(fn_ptr, ret, args, [f32, u64, f32, f32, f64, f64]),
        (6, 288) => call_typed!(fn_ptr, ret, args, [f32, u64, f32, f64, u64, u64]),
        (6, 289) => call_typed!(fn_ptr, ret, args, [f32, u64, f32, f64, u64, f32]),
        (6, 290) => call_typed!(fn_ptr, ret, args, [f32, u64, f32, f64, u64, f64]),
        (6, 291) => call_typed!(fn_ptr, ret, args, [f32, u64, f32, f64, f32, u64]),
        (6, 292) => call_typed!(fn_ptr, ret, args, [f32, u64, f32, f64, f32, f32]),
        (6, 293) => call_typed!(fn_ptr, ret, args, [f32, u64, f32, f64, f32, f64]),
        (6, 294) => call_typed!(fn_ptr, ret, args, [f32, u64, f32, f64, f64, u64]),
        (6, 295) => call_typed!(fn_ptr, ret, args, [f32, u64, f32, f64, f64, f32]),
        (6, 296) => call_typed!(fn_ptr, ret, args, [f32, u64, f32, f64, f64, f64]),
        (6, 297) => call_typed!(fn_ptr, ret, args, [f32, u64, f64, u64, u64, u64]),
        (6, 298) => call_typed!(fn_ptr, ret, args, [f32, u64, f64, u64, u64, f32]),
        (6, 299) => call_typed!(fn_ptr, ret, args, [f32, u64, f64, u64, u64, f64]),
        (6, 300) => call_typed!(fn_ptr, ret, args, [f32, u64, f64, u64, f32, u64]),
        (6, 301) => call_typed!(fn_ptr, ret, args, [f32, u64, f64, u64, f32, f32]),
        (6, 302) => call_typed!(fn_ptr, ret, args, [f32, u64, f64, u64, f32, f64]),
        (6, 303) => call_typed!(fn_ptr, ret, args, [f32, u64, f64, u64, f64, u64]),
        (6, 304) => call_typed!(fn_ptr, ret, args, [f32, u64, f64, u64, f64, f32]),
        (6, 305) => call_typed!(fn_ptr, ret, args, [f32, u64, f64, u64, f64, f64]),
        (6, 306) => call_typed!(fn_ptr, ret, args, [f32, u64, f64, f32, u64, u64]),
        (6, 307) => call_typed!(fn_ptr, ret, args, [f32, u64, f64, f32, u64, f32]),
        (6, 308) => call_typed!(fn_ptr, ret, args, [f32, u64, f64, f32, u64, f64]),
        (6, 309) => call_typed!(fn_ptr, ret, args, [f32, u64, f64, f32, f32, u64]),
        (6, 310) => call_typed!(fn_ptr, ret, args, [f32, u64, f64, f32, f32, f32]),
        (6, 311) => call_typed!(fn_ptr, ret, args, [f32, u64, f64, f32, f32, f64]),
        (6, 312) => call_typed!(fn_ptr, ret, args, [f32, u64, f64, f32, f64, u64]),
        (6, 313) => call_typed!(fn_ptr, ret, args, [f32, u64, f64, f32, f64, f32]),
        (6, 314) => call_typed!(fn_ptr, ret, args, [f32, u64, f64, f32, f64, f64]),
        (6, 315) => call_typed!(fn_ptr, ret, args, [f32, u64, f64, f64, u64, u64]),
        (6, 316) => call_typed!(fn_ptr, ret, args, [f32, u64, f64, f64, u64, f32]),
        (6, 317) => call_typed!(fn_ptr, ret, args, [f32, u64, f64, f64, u64, f64]),
        (6, 318) => call_typed!(fn_ptr, ret, args, [f32, u64, f64, f64, f32, u64]),
        (6, 319) => call_typed!(fn_ptr, ret, args, [f32, u64, f64, f64, f32, f32]),
        (6, 320) => call_typed!(fn_ptr, ret, args, [f32, u64, f64, f64, f32, f64]),
        (6, 321) => call_typed!(fn_ptr, ret, args, [f32, u64, f64, f64, f64, u64]),
        (6, 322) => call_typed!(fn_ptr, ret, args, [f32, u64, f64, f64, f64, f32]),
        (6, 323) => call_typed!(fn_ptr, ret, args, [f32, u64, f64, f64, f64, f64]),
        (6, 324) => call_typed!(fn_ptr, ret, args, [f32, f32, u64, u64, u64, u64]),
        (6, 325) => call_typed!(fn_ptr, ret, args, [f32, f32, u64, u64, u64, f32]),
        (6, 326) => call_typed!(fn_ptr, ret, args, [f32, f32, u64, u64, u64, f64]),
        (6, 327) => call_typed!(fn_ptr, ret, args, [f32, f32, u64, u64, f32, u64]),
        (6, 328) => call_typed!(fn_ptr, ret, args, [f32, f32, u64, u64, f32, f32]),
        (6, 329) => call_typed!(fn_ptr, ret, args, [f32, f32, u64, u64, f32, f64]),
        (6, 330) => call_typed!(fn_ptr, ret, args, [f32, f32, u64, u64, f64, u64]),
        (6, 331) => call_typed!(fn_ptr, ret, args, [f32, f32, u64, u64, f64, f32]),
        (6, 332) => call_typed!(fn_ptr, ret, args, [f32, f32, u64, u64, f64, f64]),
        (6, 333) => call_typed!(fn_ptr, ret, args, [f32, f32, u64, f32, u64, u64]),
        (6, 334) => call_typed!(fn_ptr, ret, args, [f32, f32, u64, f32, u64, f32]),
        (6, 335) => call_typed!(fn_ptr, ret, args, [f32, f32, u64, f32, u64, f64]),
        (6, 336) => call_typed!(fn_ptr, ret, args, [f32, f32, u64, f32, f32, u64]),
        (6, 337) => call_typed!(fn_ptr, ret, args, [f32, f32, u64, f32, f32, f32]),
        (6, 338) => call_typed!(fn_ptr, ret, args, [f32, f32, u64, f32, f32, f64]),
        (6, 339) => call_typed!(fn_ptr, ret, args, [f32, f32, u64, f32, f64, u64]),
        (6, 340) => call_typed!(fn_ptr, ret, args, [f32, f32, u64, f32, f64, f32]),
        (6, 341) => call_typed!(fn_ptr, ret, args, [f32, f32, u64, f32, f64, f64]),
        (6, 342) => call_typed!(fn_ptr, ret, args, [f32, f32, u64, f64, u64, u64]),
        (6, 343) => call_typed!(fn_ptr, ret, args, [f32, f32, u64, f64, u64, f32]),
        (6, 344) => call_typed!(fn_ptr, ret, args, [f32, f32, u64, f64, u64, f64]),
        (6, 345) => call_typed!(fn_ptr, ret, args, [f32, f32, u64, f64, f32, u64]),
        (6, 346) => call_typed!(fn_ptr, ret, args, [f32, f32, u64, f64, f32, f32]),
        (6, 347) => call_typed!(fn_ptr, ret, args, [f32, f32, u64, f64, f32, f64]),
        (6, 348) => call_typed!(fn_ptr, ret, args, [f32, f32, u64, f64, f64, u64]),
        (6, 349) => call_typed!(fn_ptr, ret, args, [f32, f32, u64, f64, f64, f32]),
        (6, 350) => call_typed!(fn_ptr, ret, args, [f32, f32, u64, f64, f64, f64]),
        (6, 351) => call_typed!(fn_ptr, ret, args, [f32, f32, f32, u64, u64, u64]),
        (6, 352) => call_typed!(fn_ptr, ret, args, [f32, f32, f32, u64, u64, f32]),
        (6, 353) => call_typed!(fn_ptr, ret, args, [f32, f32, f32, u64, u64, f64]),
        (6, 354) => call_typed!(fn_ptr, ret, args, [f32, f32, f32, u64, f32, u64]),
        (6, 355) => call_typed!(fn_ptr, ret, args, [f32, f32, f32, u64, f32, f32]),
        (6, 356) => call_typed!(fn_ptr, ret, args, [f32, f32, f32, u64, f32, f64]),
        (6, 357) => call_typed!(fn_ptr, ret, args, [f32, f32, f32, u64, f64, u64]),
        (6, 358) => call_typed!(fn_ptr, ret, args, [f32, f32, f32, u64, f64, f32]),
        (6, 359) => call_typed!(fn_ptr, ret, args, [f32, f32, f32, u64, f64, f64]),
        (6, 360) => call_typed!(fn_ptr, ret, args, [f32, f32, f32, f32, u64, u64]),
        (6, 361) => call_typed!(fn_ptr, ret, args, [f32, f32, f32, f32, u64, f32]),
        (6, 362) => call_typed!(fn_ptr, ret, args, [f32, f32, f32, f32, u64, f64]),
        (6, 363) => call_typed!(fn_ptr, ret, args, [f32, f32, f32, f32, f32, u64]),
        (6, 364) => call_typed!(fn_ptr, ret, args, [f32, f32, f32, f32, f32, f32]),
        (6, 365) => call_typed!(fn_ptr, ret, args, [f32, f32, f32, f32, f32, f64]),
        (6, 366) => call_typed!(fn_ptr, ret, args, [f32, f32, f32, f32, f64, u64]),
        (6, 367) => call_typed!(fn_ptr, ret, args, [f32, f32, f32, f32, f64, f32]),
        (6, 368) => call_typed!(fn_ptr, ret, args, [f32, f32, f32, f32, f64, f64]),
        (6, 369) => call_typed!(fn_ptr, ret, args, [f32, f32, f32, f64, u64, u64]),
        (6, 370) => call_typed!(fn_ptr, ret, args, [f32, f32, f32, f64, u64, f32]),
        (6, 371) => call_typed!(fn_ptr, ret, args, [f32, f32, f32, f64, u64, f64]),
        (6, 372) => call_typed!(fn_ptr, ret, args, [f32, f32, f32, f64, f32, u64]),
        (6, 373) => call_typed!(fn_ptr, ret, args, [f32, f32, f32, f64, f32, f32]),
        (6, 374) => call_typed!(fn_ptr, ret, args, [f32, f32, f32, f64, f32, f64]),
        (6, 375) => call_typed!(fn_ptr, ret, args, [f32, f32, f32, f64, f64, u64]),
        (6, 376) => call_typed!(fn_ptr, ret, args, [f32, f32, f32, f64, f64, f32]),
        (6, 377) => call_typed!(fn_ptr, ret, args, [f32, f32, f32, f64, f64, f64]),
        (6, 378) => call_typed!(fn_ptr, ret, args, [f32, f32, f64, u64, u64, u64]),
        (6, 379) => call_typed!(fn_ptr, ret, args, [f32, f32, f64, u64, u64, f32]),
        (6, 380) => call_typed!(fn_ptr, ret, args, [f32, f32, f64, u64, u64, f64]),
        (6, 381) => call_typed!(fn_ptr, ret, args, [f32, f32, f64, u64, f32, u64]),
        (6, 382) => call_typed!(fn_ptr, ret, args, [f32, f32, f64, u64, f32, f32]),
        (6, 383) => call_typed!(fn_ptr, ret, args, [f32, f32, f64, u64, f32, f64]),
        (6, 384) => call_typed!(fn_ptr, ret, args, [f32, f32, f64, u64, f64, u64]),
        (6, 385) => call_typed!(fn_ptr, ret, args, [f32, f32, f64, u64, f64, f32]),
        (6, 386) => call_typed!(fn_ptr, ret, args, [f32, f32, f64, u64, f64, f64]),
        (6, 387) => call_typed!(fn_ptr, ret, args, [f32, f32, f64, f32, u64, u64]),
        (6, 388) => call_typed!(fn_ptr, ret, args, [f32, f32, f64, f32, u64, f32]),
        (6, 389) => call_typed!(fn_ptr, ret, args, [f32, f32, f64, f32, u64, f64]),
        (6, 390) => call_typed!(fn_ptr, ret, args, [f32, f32, f64, f32, f32, u64]),
        (6, 391) => call_typed!(fn_ptr, ret, args, [f32, f32, f64, f32, f32, f32]),
        (6, 392) => call_typed!(fn_ptr, ret, args, [f32, f32, f64, f32, f32, f64]),
        (6, 393) => call_typed!(fn_ptr, ret, args, [f32, f32, f64, f32, f64, u64]),
        (6, 394) => call_typed!(fn_ptr, ret, args, [f32, f32, f64, f32, f64, f32]),
        (6, 395) => call_typed!(fn_ptr, ret, args, [f32, f32, f64, f32, f64, f64]),
        (6, 396) => call_typed!(fn_ptr, ret, args, [f32, f32, f64, f64, u64, u64]),
        (6, 397) => call_typed!(fn_ptr, ret, args, [f32, f32, f64, f64, u64, f32]),
        (6, 398) => call_typed!(fn_ptr, ret, args, [f32, f32, f64, f64, u64, f64]),
        (6, 399) => call_typed!(fn_ptr, ret, args, [f32, f32, f64, f64, f32, u64]),
        (6, 400) => call_typed!(fn_ptr, ret, args, [f32, f32, f64, f64, f32, f32]),
        (6, 401) => call_typed!(fn_ptr, ret, args, [f32, f32, f64, f64, f32, f64]),
        (6, 402) => call_typed!(fn_ptr, ret, args, [f32, f32, f64, f64, f64, u64]),
        (6, 403) => call_typed!(fn_ptr, ret, args, [f32, f32, f64, f64, f64, f32]),
        (6, 404) => call_typed!(fn_ptr, ret, args, [f32, f32, f64, f64, f64, f64]),
        (6, 405) => call_typed!(fn_ptr, ret, args, [f32, f64, u64, u64, u64, u64]),
        (6, 406) => call_typed!(fn_ptr, ret, args, [f32, f64, u64, u64, u64, f32]),
        (6, 407) => call_typed!(fn_ptr, ret, args, [f32, f64, u64, u64, u64, f64]),
        (6, 408) => call_typed!(fn_ptr, ret, args, [f32, f64, u64, u64, f32, u64]),
        (6, 409) => call_typed!(fn_ptr, ret, args, [f32, f64, u64, u64, f32, f32]),
        (6, 410) => call_typed!(fn_ptr, ret, args, [f32, f64, u64, u64, f32, f64]),
        (6, 411) => call_typed!(fn_ptr, ret, args, [f32, f64, u64, u64, f64, u64]),
        (6, 412) => call_typed!(fn_ptr, ret, args, [f32, f64, u64, u64, f64, f32]),
        (6, 413) => call_typed!(fn_ptr, ret, args, [f32, f64, u64, u64, f64, f64]),
        (6, 414) => call_typed!(fn_ptr, ret, args, [f32, f64, u64, f32, u64, u64]),
        (6, 415) => call_typed!(fn_ptr, ret, args, [f32, f64, u64, f32, u64, f32]),
        (6, 416) => call_typed!(fn_ptr, ret, args, [f32, f64, u64, f32, u64, f64]),
        (6, 417) => call_typed!(fn_ptr, ret, args, [f32, f64, u64, f32, f32, u64]),
        (6, 418) => call_typed!(fn_ptr, ret, args, [f32, f64, u64, f32, f32, f32]),
        (6, 419) => call_typed!(fn_ptr, ret, args, [f32, f64, u64, f32, f32, f64]),
        (6, 420) => call_typed!(fn_ptr, ret, args, [f32, f64, u64, f32, f64, u64]),
        (6, 421) => call_typed!(fn_ptr, ret, args, [f32, f64, u64, f32, f64, f32]),
        (6, 422) => call_typed!(fn_ptr, ret, args, [f32, f64, u64, f32, f64, f64]),
        (6, 423) => call_typed!(fn_ptr, ret, args, [f32, f64, u64, f64, u64, u64]),
        (6, 424) => call_typed!(fn_ptr, ret, args, [f32, f64, u64, f64, u64, f32]),
        (6, 425) => call_typed!(fn_ptr, ret, args, [f32, f64, u64, f64, u64, f64]),
        (6, 426) => call_typed!(fn_ptr, ret, args, [f32, f64, u64, f64, f32, u64]),
        (6, 427) => call_typed!(fn_ptr, ret, args, [f32, f64, u64, f64, f32, f32]),
        (6, 428) => call_typed!(fn_ptr, ret, args, [f32, f64, u64, f64, f32, f64]),
        (6, 429) => call_typed!(fn_ptr, ret, args, [f32, f64, u64, f64, f64, u64]),
        (6, 430) => call_typed!(fn_ptr, ret, args, [f32, f64, u64, f64, f64, f32]),
        (6, 431) => call_typed!(fn_ptr, ret, args, [f32, f64, u64, f64, f64, f64]),
        (6, 432) => call_typed!(fn_ptr, ret, args, [f32, f64, f32, u64, u64, u64]),
        (6, 433) => call_typed!(fn_ptr, ret, args, [f32, f64, f32, u64, u64, f32]),
        (6, 434) => call_typed!(fn_ptr, ret, args, [f32, f64, f32, u64, u64, f64]),
        (6, 435) => call_typed!(fn_ptr, ret, args, [f32, f64, f32, u64, f32, u64]),
        (6, 436) => call_typed!(fn_ptr, ret, args, [f32, f64, f32, u64, f32, f32]),
        (6, 437) => call_typed!(fn_ptr, ret, args, [f32, f64, f32, u64, f32, f64]),
        (6, 438) => call_typed!(fn_ptr, ret, args, [f32, f64, f32, u64, f64, u64]),
        (6, 439) => call_typed!(fn_ptr, ret, args, [f32, f64, f32, u64, f64, f32]),
        (6, 440) => call_typed!(fn_ptr, ret, args, [f32, f64, f32, u64, f64, f64]),
        (6, 441) => call_typed!(fn_ptr, ret, args, [f32, f64, f32, f32, u64, u64]),
        (6, 442) => call_typed!(fn_ptr, ret, args, [f32, f64, f32, f32, u64, f32]),
        (6, 443) => call_typed!(fn_ptr, ret, args, [f32, f64, f32, f32, u64, f64]),
        (6, 444) => call_typed!(fn_ptr, ret, args, [f32, f64, f32, f32, f32, u64]),
        (6, 445) => call_typed!(fn_ptr, ret, args, [f32, f64, f32, f32, f32, f32]),
        (6, 446) => call_typed!(fn_ptr, ret, args, [f32, f64, f32, f32, f32, f64]),
        (6, 447) => call_typed!(fn_ptr, ret, args, [f32, f64, f32, f32, f64, u64]),
        (6, 448) => call_typed!(fn_ptr, ret, args, [f32, f64, f32, f32, f64, f32]),
        (6, 449) => call_typed!(fn_ptr, ret, args, [f32, f64, f32, f32, f64, f64]),
        (6, 450) => call_typed!(fn_ptr, ret, args, [f32, f64, f32, f64, u64, u64]),
        (6, 451) => call_typed!(fn_ptr, ret, args, [f32, f64, f32, f64, u64, f32]),
        (6, 452) => call_typed!(fn_ptr, ret, args, [f32, f64, f32, f64, u64, f64]),
        (6, 453) => call_typed!(fn_ptr, ret, args, [f32, f64, f32, f64, f32, u64]),
        (6, 454) => call_typed!(fn_ptr, ret, args, [f32, f64, f32, f64, f32, f32]),
        (6, 455) => call_typed!(fn_ptr, ret, args, [f32, f64, f32, f64, f32, f64]),
        (6, 456) => call_typed!(fn_ptr, ret, args, [f32, f64, f32, f64, f64, u64]),
        (6, 457) => call_typed!(fn_ptr, ret, args, [f32, f64, f32, f64, f64, f32]),
        (6, 458) => call_typed!(fn_ptr, ret, args, [f32, f64, f32, f64, f64, f64]),
        (6, 459) => call_typed!(fn_ptr, ret, args, [f32, f64, f64, u64, u64, u64]),
        (6, 460) => call_typed!(fn_ptr, ret, args, [f32, f64, f64, u64, u64, f32]),
        (6, 461) => call_typed!(fn_ptr, ret, args, [f32, f64, f64, u64, u64, f64]),
        (6, 462) => call_typed!(fn_ptr, ret, args, [f32, f64, f64, u64, f32, u64]),
        (6, 463) => call_typed!(fn_ptr, ret, args, [f32, f64, f64, u64, f32, f32]),
        (6, 464) => call_typed!(fn_ptr, ret, args, [f32, f64, f64, u64, f32, f64]),
        (6, 465) => call_typed!(fn_ptr, ret, args, [f32, f64, f64, u64, f64, u64]),
        (6, 466) => call_typed!(fn_ptr, ret, args, [f32, f64, f64, u64, f64, f32]),
        (6, 467) => call_typed!(fn_ptr, ret, args, [f32, f64, f64, u64, f64, f64]),
        (6, 468) => call_typed!(fn_ptr, ret, args, [f32, f64, f64, f32, u64, u64]),
        (6, 469) => call_typed!(fn_ptr, ret, args, [f32, f64, f64, f32, u64, f32]),
        (6, 470) => call_typed!(fn_ptr, ret, args, [f32, f64, f64, f32, u64, f64]),
        (6, 471) => call_typed!(fn_ptr, ret, args, [f32, f64, f64, f32, f32, u64]),
        (6, 472) => call_typed!(fn_ptr, ret, args, [f32, f64, f64, f32, f32, f32]),
        (6, 473) => call_typed!(fn_ptr, ret, args, [f32, f64, f64, f32, f32, f64]),
        (6, 474) => call_typed!(fn_ptr, ret, args, [f32, f64, f64, f32, f64, u64]),
        (6, 475) => call_typed!(fn_ptr, ret, args, [f32, f64, f64, f32, f64, f32]),
        (6, 476) => call_typed!(fn_ptr, ret, args, [f32, f64, f64, f32, f64, f64]),
        (6, 477) => call_typed!(fn_ptr, ret, args, [f32, f64, f64, f64, u64, u64]),
        (6, 478) => call_typed!(fn_ptr, ret, args, [f32, f64, f64, f64, u64, f32]),
        (6, 479) => call_typed!(fn_ptr, ret, args, [f32, f64, f64, f64, u64, f64]),
        (6, 480) => call_typed!(fn_ptr, ret, args, [f32, f64, f64, f64, f32, u64]),
        (6, 481) => call_typed!(fn_ptr, ret, args, [f32, f64, f64, f64, f32, f32]),
        (6, 482) => call_typed!(fn_ptr, ret, args, [f32, f64, f64, f64, f32, f64]),
        (6, 483) => call_typed!(fn_ptr, ret, args, [f32, f64, f64, f64, f64, u64]),
        (6, 484) => call_typed!(fn_ptr, ret, args, [f32, f64, f64, f64, f64, f32]),
        (6, 485) => call_typed!(fn_ptr, ret, args, [f32, f64, f64, f64, f64, f64]),
        (6, 486) => call_typed!(fn_ptr, ret, args, [f64, u64, u64, u64, u64, u64]),
        (6, 487) => call_typed!(fn_ptr, ret, args, [f64, u64, u64, u64, u64, f32]),
        (6, 488) => call_typed!(fn_ptr, ret, args, [f64, u64, u64, u64, u64, f64]),
        (6, 489) => call_typed!(fn_ptr, ret, args, [f64, u64, u64, u64, f32, u64]),
        (6, 490) => call_typed!(fn_ptr, ret, args, [f64, u64, u64, u64, f32, f32]),
        (6, 491) => call_typed!(fn_ptr, ret, args, [f64, u64, u64, u64, f32, f64]),
        (6, 492) => call_typed!(fn_ptr, ret, args, [f64, u64, u64, u64, f64, u64]),
        (6, 493) => call_typed!(fn_ptr, ret, args, [f64, u64, u64, u64, f64, f32]),
        (6, 494) => call_typed!(fn_ptr, ret, args, [f64, u64, u64, u64, f64, f64]),
        (6, 495) => call_typed!(fn_ptr, ret, args, [f64, u64, u64, f32, u64, u64]),
        (6, 496) => call_typed!(fn_ptr, ret, args, [f64, u64, u64, f32, u64, f32]),
        (6, 497) => call_typed!(fn_ptr, ret, args, [f64, u64, u64, f32, u64, f64]),
        (6, 498) => call_typed!(fn_ptr, ret, args, [f64, u64, u64, f32, f32, u64]),
        (6, 499) => call_typed!(fn_ptr, ret, args, [f64, u64, u64, f32, f32, f32]),
        (6, 500) => call_typed!(fn_ptr, ret, args, [f64, u64, u64, f32, f32, f64]),
        (6, 501) => call_typed!(fn_ptr, ret, args, [f64, u64, u64, f32, f64, u64]),
        (6, 502) => call_typed!(fn_ptr, ret, args, [f64, u64, u64, f32, f64, f32]),
        (6, 503) => call_typed!(fn_ptr, ret, args, [f64, u64, u64, f32, f64, f64]),
        (6, 504) => call_typed!(fn_ptr, ret, args, [f64, u64, u64, f64, u64, u64]),
        (6, 505) => call_typed!(fn_ptr, ret, args, [f64, u64, u64, f64, u64, f32]),
        (6, 506) => call_typed!(fn_ptr, ret, args, [f64, u64, u64, f64, u64, f64]),
        (6, 507) => call_typed!(fn_ptr, ret, args, [f64, u64, u64, f64, f32, u64]),
        (6, 508) => call_typed!(fn_ptr, ret, args, [f64, u64, u64, f64, f32, f32]),
        (6, 509) => call_typed!(fn_ptr, ret, args, [f64, u64, u64, f64, f32, f64]),
        (6, 510) => call_typed!(fn_ptr, ret, args, [f64, u64, u64, f64, f64, u64]),
        (6, 511) => call_typed!(fn_ptr, ret, args, [f64, u64, u64, f64, f64, f32]),
        (6, 512) => call_typed!(fn_ptr, ret, args, [f64, u64, u64, f64, f64, f64]),
        (6, 513) => call_typed!(fn_ptr, ret, args, [f64, u64, f32, u64, u64, u64]),
        (6, 514) => call_typed!(fn_ptr, ret, args, [f64, u64, f32, u64, u64, f32]),
        (6, 515) => call_typed!(fn_ptr, ret, args, [f64, u64, f32, u64, u64, f64]),
        (6, 516) => call_typed!(fn_ptr, ret, args, [f64, u64, f32, u64, f32, u64]),
        (6, 517) => call_typed!(fn_ptr, ret, args, [f64, u64, f32, u64, f32, f32]),
        (6, 518) => call_typed!(fn_ptr, ret, args, [f64, u64, f32, u64, f32, f64]),
        (6, 519) => call_typed!(fn_ptr, ret, args, [f64, u64, f32, u64, f64, u64]),
        (6, 520) => call_typed!(fn_ptr, ret, args, [f64, u64, f32, u64, f64, f32]),
        (6, 521) => call_typed!(fn_ptr, ret, args, [f64, u64, f32, u64, f64, f64]),
        (6, 522) => call_typed!(fn_ptr, ret, args, [f64, u64, f32, f32, u64, u64]),
        (6, 523) => call_typed!(fn_ptr, ret, args, [f64, u64, f32, f32, u64, f32]),
        (6, 524) => call_typed!(fn_ptr, ret, args, [f64, u64, f32, f32, u64, f64]),
        (6, 525) => call_typed!(fn_ptr, ret, args, [f64, u64, f32, f32, f32, u64]),
        (6, 526) => call_typed!(fn_ptr, ret, args, [f64, u64, f32, f32, f32, f32]),
        (6, 527) => call_typed!(fn_ptr, ret, args, [f64, u64, f32, f32, f32, f64]),
        (6, 528) => call_typed!(fn_ptr, ret, args, [f64, u64, f32, f32, f64, u64]),
        (6, 529) => call_typed!(fn_ptr, ret, args, [f64, u64, f32, f32, f64, f32]),
        (6, 530) => call_typed!(fn_ptr, ret, args, [f64, u64, f32, f32, f64, f64]),
        (6, 531) => call_typed!(fn_ptr, ret, args, [f64, u64, f32, f64, u64, u64]),
        (6, 532) => call_typed!(fn_ptr, ret, args, [f64, u64, f32, f64, u64, f32]),
        (6, 533) => call_typed!(fn_ptr, ret, args, [f64, u64, f32, f64, u64, f64]),
        (6, 534) => call_typed!(fn_ptr, ret, args, [f64, u64, f32, f64, f32, u64]),
        (6, 535) => call_typed!(fn_ptr, ret, args, [f64, u64, f32, f64, f32, f32]),
        (6, 536) => call_typed!(fn_ptr, ret, args, [f64, u64, f32, f64, f32, f64]),
        (6, 537) => call_typed!(fn_ptr, ret, args, [f64, u64, f32, f64, f64, u64]),
        (6, 538) => call_typed!(fn_ptr, ret, args, [f64, u64, f32, f64, f64, f32]),
        (6, 539) => call_typed!(fn_ptr, ret, args, [f64, u64, f32, f64, f64, f64]),
        (6, 540) => call_typed!(fn_ptr, ret, args, [f64, u64, f64, u64, u64, u64]),
        (6, 541) => call_typed!(fn_ptr, ret, args, [f64, u64, f64, u64, u64, f32]),
        (6, 542) => call_typed!(fn_ptr, ret, args, [f64, u64, f64, u64, u64, f64]),
        (6, 543) => call_typed!(fn_ptr, ret, args, [f64, u64, f64, u64, f32, u64]),
        (6, 544) => call_typed!(fn_ptr, ret, args, [f64, u64, f64, u64, f32, f32]),
        (6, 545) => call_typed!(fn_ptr, ret, args, [f64, u64, f64, u64, f32, f64]),
        (6, 546) => call_typed!(fn_ptr, ret, args, [f64, u64, f64, u64, f64, u64]),
        (6, 547) => call_typed!(fn_ptr, ret, args, [f64, u64, f64, u64, f64, f32]),
        (6, 548) => call_typed!(fn_ptr, ret, args, [f64, u64, f64, u64, f64, f64]),
        (6, 549) => call_typed!(fn_ptr, ret, args, [f64, u64, f64, f32, u64, u64]),
        (6, 550) => call_typed!(fn_ptr, ret, args, [f64, u64, f64, f32, u64, f32]),
        (6, 551) => call_typed!(fn_ptr, ret, args, [f64, u64, f64, f32, u64, f64]),
        (6, 552) => call_typed!(fn_ptr, ret, args, [f64, u64, f64, f32, f32, u64]),
        (6, 553) => call_typed!(fn_ptr, ret, args, [f64, u64, f64, f32, f32, f32]),
        (6, 554) => call_typed!(fn_ptr, ret, args, [f64, u64, f64, f32, f32, f64]),
        (6, 555) => call_typed!(fn_ptr, ret, args, [f64, u64, f64, f32, f64, u64]),
        (6, 556) => call_typed!(fn_ptr, ret, args, [f64, u64, f64, f32, f64, f32]),
        (6, 557) => call_typed!(fn_ptr, ret, args, [f64, u64, f64, f32, f64, f64]),
        (6, 558) => call_typed!(fn_ptr, ret, args, [f64, u64, f64, f64, u64, u64]),
        (6, 559) => call_typed!(fn_ptr, ret, args, [f64, u64, f64, f64, u64, f32]),
        (6, 560) => call_typed!(fn_ptr, ret, args, [f64, u64, f64, f64, u64, f64]),
        (6, 561) => call_typed!(fn_ptr, ret, args, [f64, u64, f64, f64, f32, u64]),
        (6, 562) => call_typed!(fn_ptr, ret, args, [f64, u64, f64, f64, f32, f32]),
        (6, 563) => call_typed!(fn_ptr, ret, args, [f64, u64, f64, f64, f32, f64]),
        (6, 564) => call_typed!(fn_ptr, ret, args, [f64, u64, f64, f64, f64, u64]),
        (6, 565) => call_typed!(fn_ptr, ret, args, [f64, u64, f64, f64, f64, f32]),
        (6, 566) => call_typed!(fn_ptr, ret, args, [f64, u64, f64, f64, f64, f64]),
        (6, 567) => call_typed!(fn_ptr, ret, args, [f64, f32, u64, u64, u64, u64]),
        (6, 568) => call_typed!(fn_ptr, ret, args, [f64, f32, u64, u64, u64, f32]),
        (6, 569) => call_typed!(fn_ptr, ret, args, [f64, f32, u64, u64, u64, f64]),
        (6, 570) => call_typed!(fn_ptr, ret, args, [f64, f32, u64, u64, f32, u64]),
        (6, 571) => call_typed!(fn_ptr, ret, args, [f64, f32, u64, u64, f32, f32]),
        (6, 572) => call_typed!(fn_ptr, ret, args, [f64, f32, u64, u64, f32, f64]),
        (6, 573) => call_typed!(fn_ptr, ret, args, [f64, f32, u64, u64, f64, u64]),
        (6, 574) => call_typed!(fn_ptr, ret, args, [f64, f32, u64, u64, f64, f32]),
        (6, 575) => call_typed!(fn_ptr, ret, args, [f64, f32, u64, u64, f64, f64]),
        (6, 576) => call_typed!(fn_ptr, ret, args, [f64, f32, u64, f32, u64, u64]),
        (6, 577) => call_typed!(fn_ptr, ret, args, [f64, f32, u64, f32, u64, f32]),
        (6, 578) => call_typed!(fn_ptr, ret, args, [f64, f32, u64, f32, u64, f64]),
        (6, 579) => call_typed!(fn_ptr, ret, args, [f64, f32, u64, f32, f32, u64]),
        (6, 580) => call_typed!(fn_ptr, ret, args, [f64, f32, u64, f32, f32, f32]),
        (6, 581) => call_typed!(fn_ptr, ret, args, [f64, f32, u64, f32, f32, f64]),
        (6, 582) => call_typed!(fn_ptr, ret, args, [f64, f32, u64, f32, f64, u64]),
        (6, 583) => call_typed!(fn_ptr, ret, args, [f64, f32, u64, f32, f64, f32]),
        (6, 584) => call_typed!(fn_ptr, ret, args, [f64, f32, u64, f32, f64, f64]),
        (6, 585) => call_typed!(fn_ptr, ret, args, [f64, f32, u64, f64, u64, u64]),
        (6, 586) => call_typed!(fn_ptr, ret, args, [f64, f32, u64, f64, u64, f32]),
        (6, 587) => call_typed!(fn_ptr, ret, args, [f64, f32, u64, f64, u64, f64]),
        (6, 588) => call_typed!(fn_ptr, ret, args, [f64, f32, u64, f64, f32, u64]),
        (6, 589) => call_typed!(fn_ptr, ret, args, [f64, f32, u64, f64, f32, f32]),
        (6, 590) => call_typed!(fn_ptr, ret, args, [f64, f32, u64, f64, f32, f64]),
        (6, 591) => call_typed!(fn_ptr, ret, args, [f64, f32, u64, f64, f64, u64]),
        (6, 592) => call_typed!(fn_ptr, ret, args, [f64, f32, u64, f64, f64, f32]),
        (6, 593) => call_typed!(fn_ptr, ret, args, [f64, f32, u64, f64, f64, f64]),
        (6, 594) => call_typed!(fn_ptr, ret, args, [f64, f32, f32, u64, u64, u64]),
        (6, 595) => call_typed!(fn_ptr, ret, args, [f64, f32, f32, u64, u64, f32]),
        (6, 596) => call_typed!(fn_ptr, ret, args, [f64, f32, f32, u64, u64, f64]),
        (6, 597) => call_typed!(fn_ptr, ret, args, [f64, f32, f32, u64, f32, u64]),
        (6, 598) => call_typed!(fn_ptr, ret, args, [f64, f32, f32, u64, f32, f32]),
        (6, 599) => call_typed!(fn_ptr, ret, args, [f64, f32, f32, u64, f32, f64]),
        (6, 600) => call_typed!(fn_ptr, ret, args, [f64, f32, f32, u64, f64, u64]),
        (6, 601) => call_typed!(fn_ptr, ret, args, [f64, f32, f32, u64, f64, f32]),
        (6, 602) => call_typed!(fn_ptr, ret, args, [f64, f32, f32, u64, f64, f64]),
        (6, 603) => call_typed!(fn_ptr, ret, args, [f64, f32, f32, f32, u64, u64]),
        (6, 604) => call_typed!(fn_ptr, ret, args, [f64, f32, f32, f32, u64, f32]),
        (6, 605) => call_typed!(fn_ptr, ret, args, [f64, f32, f32, f32, u64, f64]),
        (6, 606) => call_typed!(fn_ptr, ret, args, [f64, f32, f32, f32, f32, u64]),
        (6, 607) => call_typed!(fn_ptr, ret, args, [f64, f32, f32, f32, f32, f32]),
        (6, 608) => call_typed!(fn_ptr, ret, args, [f64, f32, f32, f32, f32, f64]),
        (6, 609) => call_typed!(fn_ptr, ret, args, [f64, f32, f32, f32, f64, u64]),
        (6, 610) => call_typed!(fn_ptr, ret, args, [f64, f32, f32, f32, f64, f32]),
        (6, 611) => call_typed!(fn_ptr, ret, args, [f64, f32, f32, f32, f64, f64]),
        (6, 612) => call_typed!(fn_ptr, ret, args, [f64, f32, f32, f64, u64, u64]),
        (6, 613) => call_typed!(fn_ptr, ret, args, [f64, f32, f32, f64, u64, f32]),
        (6, 614) => call_typed!(fn_ptr, ret, args, [f64, f32, f32, f64, u64, f64]),
        (6, 615) => call_typed!(fn_ptr, ret, args, [f64, f32, f32, f64, f32, u64]),
        (6, 616) => call_typed!(fn_ptr, ret, args, [f64, f32, f32, f64, f32, f32]),
        (6, 617) => call_typed!(fn_ptr, ret, args, [f64, f32, f32, f64, f32, f64]),
        (6, 618) => call_typed!(fn_ptr, ret, args, [f64, f32, f32, f64, f64, u64]),
        (6, 619) => call_typed!(fn_ptr, ret, args, [f64, f32, f32, f64, f64, f32]),
        (6, 620) => call_typed!(fn_ptr, ret, args, [f64, f32, f32, f64, f64, f64]),
        (6, 621) => call_typed!(fn_ptr, ret, args, [f64, f32, f64, u64, u64, u64]),
        (6, 622) => call_typed!(fn_ptr, ret, args, [f64, f32, f64, u64, u64, f32]),
        (6, 623) => call_typed!(fn_ptr, ret, args, [f64, f32, f64, u64, u64, f64]),
        (6, 624) => call_typed!(fn_ptr, ret, args, [f64, f32, f64, u64, f32, u64]),
        (6, 625) => call_typed!(fn_ptr, ret, args, [f64, f32, f64, u64, f32, f32]),
        (6, 626) => call_typed!(fn_ptr, ret, args, [f64, f32, f64, u64, f32, f64]),
        (6, 627) => call_typed!(fn_ptr, ret, args, [f64, f32, f64, u64, f64, u64]),
        (6, 628) => call_typed!(fn_ptr, ret, args, [f64, f32, f64, u64, f64, f32]),
        (6, 629) => call_typed!(fn_ptr, ret, args, [f64, f32, f64, u64, f64, f64]),
        (6, 630) => call_typed!(fn_ptr, ret, args, [f64, f32, f64, f32, u64, u64]),
        (6, 631) => call_typed!(fn_ptr, ret, args, [f64, f32, f64, f32, u64, f32]),
        (6, 632) => call_typed!(fn_ptr, ret, args, [f64, f32, f64, f32, u64, f64]),
        (6, 633) => call_typed!(fn_ptr, ret, args, [f64, f32, f64, f32, f32, u64]),
        (6, 634) => call_typed!(fn_ptr, ret, args, [f64, f32, f64, f32, f32, f32]),
        (6, 635) => call_typed!(fn_ptr, ret, args, [f64, f32, f64, f32, f32, f64]),
        (6, 636) => call_typed!(fn_ptr, ret, args, [f64, f32, f64, f32, f64, u64]),
        (6, 637) => call_typed!(fn_ptr, ret, args, [f64, f32, f64, f32, f64, f32]),
        (6, 638) => call_typed!(fn_ptr, ret, args, [f64, f32, f64, f32, f64, f64]),
        (6, 639) => call_typed!(fn_ptr, ret, args, [f64, f32, f64, f64, u64, u64]),
        (6, 640) => call_typed!(fn_ptr, ret, args, [f64, f32, f64, f64, u64, f32]),
        (6, 641) => call_typed!(fn_ptr, ret, args, [f64, f32, f64, f64, u64, f64]),
        (6, 642) => call_typed!(fn_ptr, ret, args, [f64, f32, f64, f64, f32, u64]),
        (6, 643) => call_typed!(fn_ptr, ret, args, [f64, f32, f64, f64, f32, f32]),
        (6, 644) => call_typed!(fn_ptr, ret, args, [f64, f32, f64, f64, f32, f64]),
        (6, 645) => call_typed!(fn_ptr, ret, args, [f64, f32, f64, f64, f64, u64]),
        (6, 646) => call_typed!(fn_ptr, ret, args, [f64, f32, f64, f64, f64, f32]),
        (6, 647) => call_typed!(fn_ptr, ret, args, [f64, f32, f64, f64, f64, f64]),
        (6, 648) => call_typed!(fn_ptr, ret, args, [f64, f64, u64, u64, u64, u64]),
        (6, 649) => call_typed!(fn_ptr, ret, args, [f64, f64, u64, u64, u64, f32]),
        (6, 650) => call_typed!(fn_ptr, ret, args, [f64, f64, u64, u64, u64, f64]),
        (6, 651) => call_typed!(fn_ptr, ret, args, [f64, f64, u64, u64, f32, u64]),
        (6, 652) => call_typed!(fn_ptr, ret, args, [f64, f64, u64, u64, f32, f32]),
        (6, 653) => call_typed!(fn_ptr, ret, args, [f64, f64, u64, u64, f32, f64]),
        (6, 654) => call_typed!(fn_ptr, ret, args, [f64, f64, u64, u64, f64, u64]),
        (6, 655) => call_typed!(fn_ptr, ret, args, [f64, f64, u64, u64, f64, f32]),
        (6, 656) => call_typed!(fn_ptr, ret, args, [f64, f64, u64, u64, f64, f64]),
        (6, 657) => call_typed!(fn_ptr, ret, args, [f64, f64, u64, f32, u64, u64]),
        (6, 658) => call_typed!(fn_ptr, ret, args, [f64, f64, u64, f32, u64, f32]),
        (6, 659) => call_typed!(fn_ptr, ret, args, [f64, f64, u64, f32, u64, f64]),
        (6, 660) => call_typed!(fn_ptr, ret, args, [f64, f64, u64, f32, f32, u64]),
        (6, 661) => call_typed!(fn_ptr, ret, args, [f64, f64, u64, f32, f32, f32]),
        (6, 662) => call_typed!(fn_ptr, ret, args, [f64, f64, u64, f32, f32, f64]),
        (6, 663) => call_typed!(fn_ptr, ret, args, [f64, f64, u64, f32, f64, u64]),
        (6, 664) => call_typed!(fn_ptr, ret, args, [f64, f64, u64, f32, f64, f32]),
        (6, 665) => call_typed!(fn_ptr, ret, args, [f64, f64, u64, f32, f64, f64]),
        (6, 666) => call_typed!(fn_ptr, ret, args, [f64, f64, u64, f64, u64, u64]),
        (6, 667) => call_typed!(fn_ptr, ret, args, [f64, f64, u64, f64, u64, f32]),
        (6, 668) => call_typed!(fn_ptr, ret, args, [f64, f64, u64, f64, u64, f64]),
        (6, 669) => call_typed!(fn_ptr, ret, args, [f64, f64, u64, f64, f32, u64]),
        (6, 670) => call_typed!(fn_ptr, ret, args, [f64, f64, u64, f64, f32, f32]),
        (6, 671) => call_typed!(fn_ptr, ret, args, [f64, f64, u64, f64, f32, f64]),
        (6, 672) => call_typed!(fn_ptr, ret, args, [f64, f64, u64, f64, f64, u64]),
        (6, 673) => call_typed!(fn_ptr, ret, args, [f64, f64, u64, f64, f64, f32]),
        (6, 674) => call_typed!(fn_ptr, ret, args, [f64, f64, u64, f64, f64, f64]),
        (6, 675) => call_typed!(fn_ptr, ret, args, [f64, f64, f32, u64, u64, u64]),
        (6, 676) => call_typed!(fn_ptr, ret, args, [f64, f64, f32, u64, u64, f32]),
        (6, 677) => call_typed!(fn_ptr, ret, args, [f64, f64, f32, u64, u64, f64]),
        (6, 678) => call_typed!(fn_ptr, ret, args, [f64, f64, f32, u64, f32, u64]),
        (6, 679) => call_typed!(fn_ptr, ret, args, [f64, f64, f32, u64, f32, f32]),
        (6, 680) => call_typed!(fn_ptr, ret, args, [f64, f64, f32, u64, f32, f64]),
        (6, 681) => call_typed!(fn_ptr, ret, args, [f64, f64, f32, u64, f64, u64]),
        (6, 682) => call_typed!(fn_ptr, ret, args, [f64, f64, f32, u64, f64, f32]),
        (6, 683) => call_typed!(fn_ptr, ret, args, [f64, f64, f32, u64, f64, f64]),
        (6, 684) => call_typed!(fn_ptr, ret, args, [f64, f64, f32, f32, u64, u64]),
        (6, 685) => call_typed!(fn_ptr, ret, args, [f64, f64, f32, f32, u64, f32]),
        (6, 686) => call_typed!(fn_ptr, ret, args, [f64, f64, f32, f32, u64, f64]),
        (6, 687) => call_typed!(fn_ptr, ret, args, [f64, f64, f32, f32, f32, u64]),
        (6, 688) => call_typed!(fn_ptr, ret, args, [f64, f64, f32, f32, f32, f32]),
        (6, 689) => call_typed!(fn_ptr, ret, args, [f64, f64, f32, f32, f32, f64]),
        (6, 690) => call_typed!(fn_ptr, ret, args, [f64, f64, f32, f32, f64, u64]),
        (6, 691) => call_typed!(fn_ptr, ret, args, [f64, f64, f32, f32, f64, f32]),
        (6, 692) => call_typed!(fn_ptr, ret, args, [f64, f64, f32, f32, f64, f64]),
        (6, 693) => call_typed!(fn_ptr, ret, args, [f64, f64, f32, f64, u64, u64]),
        (6, 694) => call_typed!(fn_ptr, ret, args, [f64, f64, f32, f64, u64, f32]),
        (6, 695) => call_typed!(fn_ptr, ret, args, [f64, f64, f32, f64, u64, f64]),
        (6, 696) => call_typed!(fn_ptr, ret, args, [f64, f64, f32, f64, f32, u64]),
        (6, 697) => call_typed!(fn_ptr, ret, args, [f64, f64, f32, f64, f32, f32]),
        (6, 698) => call_typed!(fn_ptr, ret, args, [f64, f64, f32, f64, f32, f64]),
        (6, 699) => call_typed!(fn_ptr, ret, args, [f64, f64, f32, f64, f64, u64]),
        (6, 700) => call_typed!(fn_ptr, ret, args, [f64, f64, f32, f64, f64, f32]),
        (6, 701) => call_typed!(fn_ptr, ret, args, [f64, f64, f32, f64, f64, f64]),
        (6, 702) => call_typed!(fn_ptr, ret, args, [f64, f64, f64, u64, u64, u64]),
        (6, 703) => call_typed!(fn_ptr, ret, args, [f64, f64, f64, u64, u64, f32]),
        (6, 704) => call_typed!(fn_ptr, ret, args, [f64, f64, f64, u64, u64, f64]),
        (6, 705) => call_typed!(fn_ptr, ret, args, [f64, f64, f64, u64, f32, u64]),
        (6, 706) => call_typed!(fn_ptr, ret, args, [f64, f64, f64, u64, f32, f32]),
        (6, 707) => call_typed!(fn_ptr, ret, args, [f64, f64, f64, u64, f32, f64]),
        (6, 708) => call_typed!(fn_ptr, ret, args, [f64, f64, f64, u64, f64, u64]),
        (6, 709) => call_typed!(fn_ptr, ret, args, [f64, f64, f64, u64, f64, f32]),
        (6, 710) => call_typed!(fn_ptr, ret, args, [f64, f64, f64, u64, f64, f64]),
        (6, 711) => call_typed!(fn_ptr, ret, args, [f64, f64, f64, f32, u64, u64]),
        (6, 712) => call_typed!(fn_ptr, ret, args, [f64, f64, f64, f32, u64, f32]),
        (6, 713) => call_typed!(fn_ptr, ret, args, [f64, f64, f64, f32, u64, f64]),
        (6, 714) => call_typed!(fn_ptr, ret, args, [f64, f64, f64, f32, f32, u64]),
        (6, 715) => call_typed!(fn_ptr, ret, args, [f64, f64, f64, f32, f32, f32]),
        (6, 716) => call_typed!(fn_ptr, ret, args, [f64, f64, f64, f32, f32, f64]),
        (6, 717) => call_typed!(fn_ptr, ret, args, [f64, f64, f64, f32, f64, u64]),
        (6, 718) => call_typed!(fn_ptr, ret, args, [f64, f64, f64, f32, f64, f32]),
        (6, 719) => call_typed!(fn_ptr, ret, args, [f64, f64, f64, f32, f64, f64]),
        (6, 720) => call_typed!(fn_ptr, ret, args, [f64, f64, f64, f64, u64, u64]),
        (6, 721) => call_typed!(fn_ptr, ret, args, [f64, f64, f64, f64, u64, f32]),
        (6, 722) => call_typed!(fn_ptr, ret, args, [f64, f64, f64, f64, u64, f64]),
        (6, 723) => call_typed!(fn_ptr, ret, args, [f64, f64, f64, f64, f32, u64]),
        (6, 724) => call_typed!(fn_ptr, ret, args, [f64, f64, f64, f64, f32, f32]),
        (6, 725) => call_typed!(fn_ptr, ret, args, [f64, f64, f64, f64, f32, f64]),
        (6, 726) => call_typed!(fn_ptr, ret, args, [f64, f64, f64, f64, f64, u64]),
        (6, 727) => call_typed!(fn_ptr, ret, args, [f64, f64, f64, f64, f64, f32]),
        (6, 728) => call_typed!(fn_ptr, ret, args, [f64, f64, f64, f64, f64, f64]),
        (n, _) => {
            return Err(EvalError::FfiError(format!(
                "too many arguments for direct FFI call ({n}); use an extension for wider signatures"
            )));
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::NativeSig;
    use rug::Integer;

    fn libc_name() -> &'static str {
        if cfg!(target_os = "macos") {
            "libSystem.B.dylib"
        } else {
            "libc.so.6"
        }
    }

    /// End-to-end smoke test of the Tier-1 C-ABI path. Skipped (returns) when
    /// libc cannot be dlopen-ed on the host.
    #[test]
    fn test_call_native_strlen_u64_return() {
        let env = Env::new();
        let Ok(lib) = load_native_library(libc_name(), &env) else {
            return;
        };
        let sig = NativeSig {
            params: vec![NativeType::CString],
            ret: NativeType::U64,
        };
        let f = library_function(&lib, "strlen", sig).expect("strlen");
        let Value::NativeFunction {
            fn_ptr, signature, ..
        } = f
        else {
            panic!("expected native function");
        };
        let out = call_native(fn_ptr, &signature, &[Value::Str("hello".to_string())]).unwrap();
        assert_eq!(out, Value::Integer(Integer::from(5)));
    }

    /// `double pow(double, double)` exercises floating-point parameters, which
    /// must be passed in SSE registers (not the integer registers used for the
    /// integer-only path).
    #[test]
    fn test_call_native_pow_f64_args() {
        let env = Env::new();
        let lib_name = if cfg!(target_os = "macos") {
            "libSystem.B.dylib"
        } else {
            "libm.so.6"
        };
        let Ok(lib) = load_native_library(lib_name, &env) else {
            return;
        };
        let sig = NativeSig {
            params: vec![NativeType::F64, NativeType::F64],
            ret: NativeType::F64,
        };
        let f = library_function(&lib, "pow", sig).expect("pow");
        let Value::NativeFunction {
            fn_ptr, signature, ..
        } = f
        else {
            panic!("expected native function");
        };
        let out = call_native(
            fn_ptr,
            &signature,
            &[
                Value::Real(rug::Float::with_val(64, 2.0)),
                Value::Real(rug::Float::with_val(64, 10.0)),
            ],
        )
        .unwrap();
        match out {
            Value::Real(r) => assert!((r.to_f64() - 1024.0).abs() < 1e-9),
            other => panic!("expected real, got {other:?}"),
        }
    }

    #[test]
    fn test_call_native_abs_i32_return() {
        let env = Env::new();
        let Ok(lib) = load_native_library(libc_name(), &env) else {
            return;
        };
        let sig = NativeSig {
            params: vec![NativeType::I32],
            ret: NativeType::I32,
        };
        let f = library_function(&lib, "abs", sig).expect("abs");
        let Value::NativeFunction {
            fn_ptr, signature, ..
        } = f
        else {
            panic!("expected native function");
        };
        let out = call_native(fn_ptr, &signature, &[Value::Integer(Integer::from(-7))]).unwrap();
        assert_eq!(out, Value::Integer(Integer::from(7)));
    }

    fn libm_name() -> &'static str {
        if cfg!(target_os = "macos") {
            "libSystem.B.dylib"
        } else {
            "libm.so.6"
        }
    }

    /// Build a callable for `symbol` with `sig`, or `None` if the library is
    /// unavailable on this host.  The caller must keep `env` alive so the
    /// returned function pointer stays valid.
    fn resolve(env: &Env, lib: &str, symbol: &str, sig: NativeSig) -> Option<usize> {
        let lib = load_native_library(lib, env).ok()?;
        let f = library_function(&lib, symbol, sig).ok()?;
        match f {
            Value::NativeFunction { fn_ptr, .. } => Some(fn_ptr),
            _ => None,
        }
    }

    #[test]
    fn test_call_native_sqrt_f64() {
        let env = Env::new();
        let sig = NativeSig {
            params: vec![NativeType::F64],
            ret: NativeType::F64,
        };
        let Some(fn_ptr) = resolve(&env, libm_name(), "sqrt", sig.clone()) else {
            return;
        };
        let out =
            call_native(fn_ptr, &sig, &[Value::Real(rug::Float::with_val(64, 2.25))]).unwrap();
        match out {
            Value::Real(r) => assert!((r.to_f64() - 1.5).abs() < 1e-12),
            other => panic!("expected real, got {other:?}"),
        }
    }

    /// `double ldexp(double, int)` mixes an SSE-register double with a
    /// general-purpose int, exercising the mixed-class dispatch arm.
    #[test]
    fn test_call_native_ldexp_mixed_f64_i32() {
        let env = Env::new();
        let sig = NativeSig {
            params: vec![NativeType::F64, NativeType::I32],
            ret: NativeType::F64,
        };
        let Some(fn_ptr) = resolve(&env, libm_name(), "ldexp", sig.clone()) else {
            return;
        };
        let out = call_native(
            fn_ptr,
            &sig,
            &[
                Value::Real(rug::Float::with_val(64, 1.5)),
                Value::Integer(Integer::from(3)),
            ],
        )
        .unwrap();
        match out {
            Value::Real(r) => assert!((r.to_f64() - 12.0).abs() < 1e-12),
            other => panic!("expected real, got {other:?}"),
        }
    }

    /// `long labs(long)` exercises the 64-bit integer path.
    #[test]
    fn test_call_native_labs_i64() {
        let env = Env::new();
        let sig = NativeSig {
            params: vec![NativeType::I64],
            ret: NativeType::I64,
        };
        let Some(fn_ptr) = resolve(&env, libc_name(), "labs", sig.clone()) else {
            return;
        };
        let out = call_native(
            fn_ptr,
            &sig,
            &[Value::Integer(Integer::from(-123_456_789_012i64))],
        )
        .unwrap();
        assert_eq!(out, Value::Integer(Integer::from(123_456_789_012i64)));
    }

    /// `char *strchr(const char *, int)` returns a pointer into the *argument*
    /// buffer, verifying both the string argument and the string return path
    /// (and that the argument buffer outlives the call).
    #[test]
    fn test_call_native_strchr_cstring_return() {
        let env = Env::new();
        let sig = NativeSig {
            params: vec![NativeType::CString, NativeType::I32],
            ret: NativeType::CString,
        };
        let Some(fn_ptr) = resolve(&env, libc_name(), "strchr", sig.clone()) else {
            return;
        };
        let out = call_native(
            fn_ptr,
            &sig,
            &[
                Value::Str("hello".to_string()),
                Value::Integer(Integer::from(b'l' as i32)),
            ],
        )
        .unwrap();
        assert_eq!(out, Value::Str("llo".to_string()));
    }

    #[test]
    fn test_call_native_wrong_arity_errors() {
        let env = Env::new();
        let sig = NativeSig {
            params: vec![NativeType::I32, NativeType::I32],
            ret: NativeType::I32,
        };
        let Some(fn_ptr) = resolve(&env, libc_name(), "abs", sig.clone()) else {
            return;
        };
        // `abs` only takes one argument; calling with one against a 2-param
        // signature must be rejected.
        let err = call_native(fn_ptr, &sig, &[Value::Integer(1.into())]).unwrap_err();
        assert!(err.to_string().contains("expected 2 arguments"));
    }

    #[test]
    fn test_sig_key_encoding() {
        assert_eq!(sig_key(&[]), 0);
        assert_eq!(sig_key(&[NativeType::F64]), 2);
        assert_eq!(sig_key(&[NativeType::F32, NativeType::U64]), 3);
        assert_eq!(sig_key(&[NativeType::F64, NativeType::F32]), 7);
        assert_eq!(
            sig_key(&[NativeType::F64, NativeType::F64, NativeType::F64]),
            26
        );
        // Wider than 20 parameters must not overflow/panic.
        let wide = vec![NativeType::F64; 21];
        let _ = sig_key(&wide);
    }

    #[test]
    fn test_call_native_null_fn_ptr_errors() {
        let sig = NativeSig {
            params: vec![],
            ret: NativeType::Void,
        };
        let err = call_native(0, &sig, &[]).unwrap_err();
        assert!(err.to_string().contains("null"), "got: {err}");
    }

    #[test]
    fn test_parse_sig_roundtrip() {
        use crate::value::Value;
        let rule = Value::Rule {
            lhs: Box::new(Value::List(vec![
                Value::Str("Real64".into()),
                Value::Str("Integer32".into()),
            ])),
            rhs: Box::new(Value::Str("CString".into())),
            delayed: false,
        };
        let sig = parse_sig(&rule).unwrap();
        assert_eq!(sig.params, vec![NativeType::F64, NativeType::I32]);
        assert_eq!(sig.ret, NativeType::CString);
    }

    #[test]
    fn test_parse_sig_errors() {
        use crate::value::Value;
        // Not a Rule.
        assert!(parse_sig(&Value::Integer(1.into())).is_err());
        // Unknown type name.
        let bad = Value::Rule {
            lhs: Box::new(Value::List(vec![Value::Str("Nope".into())])),
            rhs: Box::new(Value::Str("Void".into())),
            delayed: false,
        };
        assert!(parse_sig(&bad).is_err());
        // Non-string type.
        let bad2 = Value::Rule {
            lhs: Box::new(Value::Integer(3.into())),
            rhs: Box::new(Value::Str("Void".into())),
            delayed: false,
        };
        assert!(parse_sig(&bad2).is_err());
    }

    #[test]
    fn test_library_function_missing_symbol() {
        let env = Env::new();
        let Ok(lib) = load_native_library(libc_name(), &env) else {
            return;
        };
        let sig = NativeSig {
            params: vec![],
            ret: NativeType::Void,
        };
        assert!(library_function(&lib, "syma_no_such_symbol_xyz", sig).is_err());
    }

    #[test]
    fn test_load_native_library_reuses_handle() {
        let env = Env::new();
        let Ok(a) = load_native_library(libc_name(), &env) else {
            return;
        };
        let Ok(b) = load_native_library(libc_name(), &env) else {
            return;
        };
        match (a, b) {
            (Value::NativeLib { handle: ha, .. }, Value::NativeLib { handle: hb, .. }) => {
                assert!(std::sync::Arc::ptr_eq(&ha, &hb));
            }
            _ => panic!("expected native libs"),
        }
    }
}
