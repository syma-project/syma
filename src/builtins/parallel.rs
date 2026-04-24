/// Parallel computation builtins for Syma.
///
/// Provides Wolfram-style parallel functions:
/// - `ParallelMap[f, list]` — parallel version of Map
/// - `ParallelTable[expr, {i, min, max}]` — parallel version of Table
/// - `$KernelCount` — number of available parallel workers
/// - `LaunchKernels[n]` — set the number of parallel workers (no-op, stored)
/// - `CloseKernels[]` — reset workers to default (no-op)
///
/// All evaluator-dependent functions return a sentinel error so the
/// evaluator can dispatch them with access to `apply_function`.

use crate::value::{EvalError, Value};

// ── Stubs (evaluator-dependent) ──

pub fn builtin_parallel_map(_args: &[Value]) -> Result<Value, EvalError> {
    Err(EvalError::Error(
        "ParallelMap should be handled by evaluator".to_string(),
    ))
}

pub fn builtin_parallel_table(_args: &[Value]) -> Result<Value, EvalError> {
    Err(EvalError::Error(
        "ParallelTable should be handled by evaluator".to_string(),
    ))
}

// ── Direct builtins ──

/// `$KernelCount` — returns the number of available parallel workers.
/// By default this is the number of CPU cores reported by the OS.
pub fn builtin_kernel_count(args: &[Value]) -> Result<Value, EvalError> {
    if !args.is_empty() {
        return Err(EvalError::Error(
            "$KernelCount takes no arguments".to_string(),
        ));
    }
    let n = std::thread::available_parallelism()
        .map(|p| p.get() as i64)
        .unwrap_or(1);
    Ok(Value::Integer(rug::Integer::from(n)))
}

/// `LaunchKernels[n]` — sets the kernel count (stored as a session variable).
/// For now this is a no-op stub that returns the requested count.
pub fn builtin_launch_kernels(args: &[Value]) -> Result<Value, EvalError> {
    match args.len() {
        0 => {
            // Return current kernel count
            let n = std::thread::available_parallelism()
                .map(|p| p.get() as i64)
                .unwrap_or(1);
            Ok(Value::Integer(rug::Integer::from(n)))
        }
        1 => {
            let n = args[0].to_integer().ok_or_else(|| EvalError::TypeError {
                expected: "Integer".to_string(),
                got: args[0].type_name().to_string(),
            })?;
            if n < 1 {
                return Err(EvalError::Error(
                    "LaunchKernels requires a positive integer".to_string(),
                ));
            }
            // In a full implementation we'd store this and use it for thread pools.
            // For now, just validate and return the count.
            Ok(Value::Integer(rug::Integer::from(n)))
        }
        _ => Err(EvalError::Error(
            "LaunchKernels requires 0 or 1 arguments".to_string(),
        )),
    }
}

/// `CloseKernels[]` — resets the kernel pool. No-op, returns Null.
pub fn builtin_close_kernels(args: &[Value]) -> Result<Value, EvalError> {
    if !args.is_empty() {
        return Err(EvalError::Error(
            "CloseKernels takes no arguments".to_string(),
        ));
    }
    Ok(Value::Null)
}
