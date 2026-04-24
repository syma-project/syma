/// Tier 2: Python interop via subprocess.
///
/// `ExternalEvaluate["Python", <|"module" -> "math", "func" -> "sqrt", "args" -> {2.0}|>]`
///
/// The bridge script (`bridge.py`) is embedded as a string literal.
/// On first call it is written to a temporary file and reused on subsequent calls.
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::OnceLock;

use crate::ffi::marshal::{json_to_value, values_to_json};
use crate::value::{EvalError, Value};

/// The embedded Python bridge script.
const BRIDGE_PY: &str = include_str!("bridge.py");

/// Path to the bridge script on disk (written on first use).
static BRIDGE_PATH: OnceLock<std::path::PathBuf> = OnceLock::new();

fn bridge_script_path() -> Result<&'static std::path::PathBuf, EvalError> {
    BRIDGE_PATH.get_or_init(|| {
        let mut path = std::env::temp_dir();
        path.push("syma_bridge.py");
        // Ignore error — if the write fails, the subprocess will fail with a clear message.
        let _ = std::fs::write(&path, BRIDGE_PY);
        path
    });
    Ok(BRIDGE_PATH.get().unwrap())
}

/// Call a Python function.
///
/// `module` — importable Python module name (e.g. `"math"`, `"numpy"`)
/// `func`   — function name within the module
/// `args`   — Syma values to pass as positional arguments
pub fn call_python(module: &str, func: &str, args: &[Value]) -> Result<Value, EvalError> {
    let script = bridge_script_path()?;

    // Build the JSON request.
    let req = {
        let json_args_str = values_to_json(args)?;
        let json_args: serde_json::Value = serde_json::from_str(&json_args_str)
            .map_err(|e| EvalError::FfiError(format!("JSON serialisation: {e}")))?;
        let obj = serde_json::json!({
            "module": module,
            "func": func,
            "args": json_args,
        });
        serde_json::to_string(&obj)
            .map_err(|e| EvalError::FfiError(format!("JSON serialisation: {e}")))?
    };

    let mut child = Command::new("python3")
        .arg(script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| EvalError::FfiError(format!("failed to launch python3: {e}")))?;

    // Write the request to stdin.
    if let Some(stdin) = child.stdin.take() {
        let mut stdin = stdin;
        stdin
            .write_all(req.as_bytes())
            .map_err(|e| EvalError::FfiError(format!("write to python3 stdin: {e}")))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|e| EvalError::FfiError(format!("python3 wait: {e}")))?;

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

    if !output.status.success() && stdout.is_empty() {
        return Err(EvalError::FfiError(format!(
            "python3 exited with status {}: {}",
            output.status,
            stderr.trim()
        )));
    }

    // Parse the JSON response.
    let resp: serde_json::Value = serde_json::from_str(&stdout).map_err(|e| {
        EvalError::FfiError(format!(
            "python3 response parse error: {e}\nstdout={stdout}"
        ))
    })?;

    if let Some(err_msg) = resp.get("error") {
        return Err(EvalError::FfiError(format!(
            "python3 {module}.{func}: {}",
            err_msg.as_str().unwrap_or("unknown error")
        )));
    }

    let result_jv = resp
        .get("ok")
        .ok_or_else(|| EvalError::FfiError(format!("unexpected python3 response: {stdout}")))?;

    let result_str = serde_json::to_string(result_jv)
        .map_err(|e| EvalError::FfiError(format!("JSON result re-serialisation: {e}")))?;

    json_to_value(&result_str)
}

/// Parse an `ExternalEvaluate` argument association into (module, func, args).
pub fn parse_external_evaluate_args(
    system: &str,
    opts: &Value,
    extra_args: &[Value],
) -> Result<(String, String, Vec<Value>), EvalError> {
    if system != "Python" {
        return Err(EvalError::FfiError(format!(
            "ExternalEvaluate: unsupported system \"{system}\". Supported: \"Python\""
        )));
    }

    let assoc = match opts {
        Value::Assoc(m) => m,
        _ => {
            return Err(EvalError::TypeError {
                expected: "Assoc".to_string(),
                got: opts.type_name().to_string(),
            });
        }
    };

    let module = assoc
        .get("module")
        .and_then(|v| {
            if let Value::Str(s) = v {
                Some(s.clone())
            } else {
                None
            }
        })
        .ok_or_else(|| {
            EvalError::FfiError("ExternalEvaluate: missing \"module\" key".to_string())
        })?;

    let func = assoc
        .get("func")
        .and_then(|v| {
            if let Value::Str(s) = v {
                Some(s.clone())
            } else {
                None
            }
        })
        .ok_or_else(|| EvalError::FfiError("ExternalEvaluate: missing \"func\" key".to_string()))?;

    let args = if let Some(a) = assoc.get("args") {
        match a {
            Value::List(items) => items.clone(),
            other => vec![other.clone()],
        }
    } else {
        extra_args.to_vec()
    };

    Ok((module, func, args))
}
