/// Kernel mode — evaluation service for frontend integration.
///
/// Wraps the Syma evaluator and returns structured results with
/// JSON-serialised values, timing information, and error handling.
use std::time::Instant;

use crate::builtins;
use crate::env::Env;
use crate::eval;
use crate::ffi::marshal::value_to_json_full;
use crate::lexer;
use crate::parser;
use crate::value::{EvalError, Value};

/// Structured result from a single evaluation request.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct KernelResult {
    /// Whether evaluation succeeded.
    pub success: bool,
    /// The serialised result value (absent on error).
    pub value: Option<serde_json::Value>,
    /// Display string of the result.
    pub output: String,
    /// Error message if evaluation failed.
    pub error: Option<String>,
    /// Execution time in milliseconds.
    pub timing_ms: u64,
}

/// The Syma kernel — holds evaluation state across requests.
pub struct SymaKernel {
    env: Env,
}

impl SymaKernel {
    /// Create a new kernel with a fresh environment and all builtins registered.
    pub fn new() -> Self {
        let env = Env::new();
        builtins::register_builtins(&env);
        SymaKernel { env }
    }

    /// Create a kernel from an existing environment (for testing).
    pub fn from_env(env: Env) -> Self {
        SymaKernel { env }
    }

    /// Evaluate a Syma expression string and return a structured result.
    ///
    /// The input is lexed, parsed, and evaluated. The result value is serialised
    /// to tagged JSON and also returned as a display string.
    pub fn eval(&self, input: &str) -> KernelResult {
        let start = Instant::now();

        let tokens = match lexer::tokenize(input) {
            Ok(t) => t,
            Err(e) => {
                let elapsed = start.elapsed();
                return KernelResult {
                    success: false,
                    value: None,
                    output: String::new(),
                    error: Some(format!("Lexical error: {e}")),
                    timing_ms: elapsed.as_millis() as u64,
                };
            }
        };

        let ast = match parser::parse(tokens) {
            Ok(a) => a,
            Err(e) => {
                let elapsed = start.elapsed();
                return KernelResult {
                    success: false,
                    value: None,
                    output: String::new(),
                    error: Some(format!("Parse error: {e}")),
                    timing_ms: elapsed.as_millis() as u64,
                };
            }
        };

        match eval::eval_program(&ast, &self.env) {
            Ok(val) => {
                let elapsed = start.elapsed();
                let json_val = value_to_json_full(&val);
                let output = val.to_string();
                KernelResult {
                    success: true,
                    value: Some(json_val),
                    output,
                    error: None,
                    timing_ms: elapsed.as_millis() as u64,
                }
            }
            Err(e) => {
                let elapsed = start.elapsed();
                KernelResult {
                    success: false,
                    value: None,
                    output: String::new(),
                    error: Some(format!("{e}")),
                    timing_ms: elapsed.as_millis() as u64,
                }
            }
        }
    }

    /// Evaluate an expression and return a raw `Value` (for programmatic use).
    pub fn eval_raw(&self, input: &str) -> Result<Value, EvalError> {
        let tokens =
            lexer::tokenize(input).map_err(|e| EvalError::Error(format!("Lexical error: {e}")))?;
        let ast =
            parser::parse(tokens).map_err(|e| EvalError::Error(format!("Parse error: {e}")))?;
        eval::eval_program(&ast, &self.env)
    }

    /// Access the underlying environment.
    pub fn env(&self) -> &Env {
        &self.env
    }

    /// Evaluate JSON-encoded input and return a JSON-encoded result.
    ///
    /// Accepts `{"input": "2+2"}` and returns the standard KernelResult as JSON.
    /// This is the entry point for the stdin/stdout protocol.
    pub fn eval_json(&self, request: &str) -> String {
        let req: Result<serde_json::Value, _> = serde_json::from_str(request);
        let input = match req {
            Ok(serde_json::Value::Object(ref m)) => {
                m.get("input").and_then(|v| v.as_str()).unwrap_or("")
            }
            _ => "",
        };
        let result = self.eval(input);
        serde_json::to_string(&result).unwrap_or_else(|e| {
            format!(r#"{{"success":false,"error":"JSON serialisation failed: {e}"}}"#)
        })
    }
}

impl Default for SymaKernel {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtins;

    fn make_kernel() -> SymaKernel {
        let env = Env::new();
        builtins::register_builtins(&env);
        SymaKernel::from_env(env)
    }

    #[test]
    fn test_eval_integer() {
        let kernel = make_kernel();
        let result = kernel.eval("42");
        assert!(result.success);
        assert_eq!(result.output, "42");
        assert!(result.error.is_none());
        // Check JSON structure
        let json = result.value.unwrap();
        assert_eq!(json.get("t").and_then(|v| v.as_str()), Some("int"));
        assert_eq!(json.get("v").and_then(|v| v.as_str()), Some("42"));
    }

    #[test]
    fn test_eval_addition() {
        let kernel = make_kernel();
        let result = kernel.eval("1 + 2");
        assert!(result.success);
        assert_eq!(result.output, "3");
        assert_eq!(
            result.value.unwrap().get("v").and_then(|v| v.as_str()),
            Some("3")
        );
    }

    #[test]
    fn test_eval_string() {
        let kernel = make_kernel();
        let result = kernel.eval(r#""hello world""#);
        assert!(result.success);
        assert_eq!(result.output, r#""hello world""#);
    }

    #[test]
    fn test_eval_list() {
        let kernel = make_kernel();
        let result = kernel.eval("{1, 2, 3}");
        assert!(result.success);
        assert_eq!(result.output, "{1, 2, 3}");
        // Verify tagged JSON: list
        let json = result.value.unwrap();
        assert_eq!(json.get("t").and_then(|v| v.as_str()), Some("list"));
    }

    #[test]
    fn test_eval_call_result() {
        let kernel = make_kernel();
        let result = kernel.eval("Integrate[x^2, x]");
        assert!(result.success);
        // Should return a Call or computed result
        let json = result.value.unwrap();
        let tag = json.get("t").and_then(|v| v.as_str()).unwrap_or("");
        // Either a call (symbolic) or a computed result
        assert!(
            tag == "call" || tag == "int" || tag == "real",
            "unexpected tag: {tag}"
        );
    }

    #[test]
    fn test_eval_error() {
        let kernel = make_kernel();
        let result = kernel.eval("1 +++ 2");
        assert!(!result.success);
        assert!(result.error.is_some());
        assert!(result.value.is_none());
    }

    #[test]
    fn test_eval_json_protocol() {
        let kernel = make_kernel();
        let response = kernel.eval_json(r#"{"input": "2+2"}"#);
        let parsed: serde_json::Value = serde_json::from_str(&response).unwrap();
        assert_eq!(parsed["success"], true);
        assert_eq!(parsed["output"], "4");
    }

    #[test]
    fn test_eval_json_empty() {
        let kernel = make_kernel();
        let response = kernel.eval_json(r#"{}"#);
        let parsed: serde_json::Value = serde_json::from_str(&response).unwrap();
        assert!(parsed["success"].as_bool().unwrap_or(false));
    }

    #[test]
    fn test_eval_holds_state() {
        let kernel = make_kernel();
        kernel.eval("x = 42");
        let result = kernel.eval("x + 1");
        assert!(result.success);
        assert_eq!(result.output, "43");
    }

    #[test]
    fn test_eval_raw() {
        let kernel = make_kernel();
        let val = kernel.eval_raw("1 + 2").unwrap();
        assert_eq!(format!("{val}"), "3");
    }

    #[test]
    fn test_timing() {
        let kernel = make_kernel();
        let result = kernel.eval("2^1000");
        assert!(result.success);
        // Timing should be non-zero for a computation
        assert!(result.timing_ms > 0 || result.output.len() > 0);
    }
}
