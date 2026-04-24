/// FFI builtins exposed to Syma code.
use crate::env::Env;
use crate::ffi;
use crate::value::{EvalError, Value};

// ── LoadLibrary ───────────────────────────────────────────────────────────────

/// `LoadLibrary["path/to/lib.so"]` — load a native shared library.
pub fn builtin_load_library(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "LoadLibrary requires exactly 1 argument".to_string(),
        ));
    }
    if let Value::Str(path) = &args[0] {
        return ffi::loader::load_native_library(path, env);
    }
    Err(EvalError::TypeError {
        expected: "String".to_string(),
        got: args[0].type_name().to_string(),
    })
}

// ── LoadExtension ─────────────────────────────────────────────────────────────

/// `LoadExtension["path/to/ext.so"]` — load a Syma extension package.
pub fn builtin_load_extension(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "LoadExtension requires exactly 1 argument".to_string(),
        ));
    }
    if let Value::Str(path) = &args[0] {
        ffi::extension::load_extension(path, env)?;
        return Ok(Value::Null);
    }
    Err(EvalError::TypeError {
        expected: "String".to_string(),
        got: args[0].type_name().to_string(),
    })
}

// ── LibraryFunction ───────────────────────────────────────────────────────────

/// `LibraryFunction[lib, "symbol", {types} -> retType]` — create a callable from a loaded library.
pub fn builtin_library_function(args: &[Value], _env: &Env) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Err(EvalError::Error(
            "LibraryFunction requires 3 arguments: lib, symbol, signature".to_string(),
        ));
    }
    let sym = match &args[1] {
        Value::Str(s) => s.clone(),
        _ => {
            return Err(EvalError::TypeError {
                expected: "String".to_string(),
                got: args[1].type_name().to_string(),
            });
        }
    };
    let sig = ffi::loader::parse_sig(&args[2])?;
    ffi::loader::library_function(&args[0], &sym, sig)
}

// ── LibraryFunctionLoad ───────────────────────────────────────────────────────

/// `LibraryFunctionLoad["path", "symbol", {types} -> retType]` — load lib and create function.
pub fn builtin_library_function_load(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Err(EvalError::Error(
            "LibraryFunctionLoad requires 3 arguments: path, symbol, signature".to_string(),
        ));
    }
    let path = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => {
            return Err(EvalError::TypeError {
                expected: "String".to_string(),
                got: args[0].type_name().to_string(),
            });
        }
    };
    let sym = match &args[1] {
        Value::Str(s) => s.clone(),
        _ => {
            return Err(EvalError::TypeError {
                expected: "String".to_string(),
                got: args[1].type_name().to_string(),
            });
        }
    };
    let sig = ffi::loader::parse_sig(&args[2])?;
    let lib = ffi::loader::load_native_library(&path, env)?;
    ffi::loader::library_function(&lib, &sym, sig)
}

// ── ExternalEvaluate ──────────────────────────────────────────────────────────

/// `ExternalEvaluate["Python", opts]` — evaluate code in an external system.
pub fn builtin_external_evaluate(args: &[Value], _env: &Env) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "ExternalEvaluate requires at least 2 arguments: system, opts".to_string(),
        ));
    }
    let system = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => {
            return Err(EvalError::TypeError {
                expected: "String".to_string(),
                got: args[0].type_name().to_string(),
            });
        }
    };
    let (module, func, call_args) =
        ffi::python::parse_external_evaluate_args(&system, &args[1], &args[2..])?;
    ffi::python::call_python(&module, &func, &call_args)
}
