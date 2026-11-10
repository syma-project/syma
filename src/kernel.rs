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
use crate::value::{EvalError, Format, Value};

/// A single statement result in a multi-statement response.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StatementResult {
    /// Display string of the result.
    pub output: String,
    /// The serialised result value.
    pub value: serde_json::Value,
}

/// Structured result from a single evaluation request.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct KernelResult {
    /// Whether evaluation succeeded.
    pub success: bool,
    /// Per-statement results. A `null` entry means the statement was
    /// suppressed by `;`. Absent on parse/lex errors.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub results: Option<Vec<Option<StatementResult>>>,
    /// Warning/error messages generated during evaluation (e.g. "Power::infy: ...").
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub messages: Vec<String>,
    /// Error message if evaluation failed.
    pub error: Option<String>,
    /// Execution time in milliseconds.
    pub timing_ms: u64,
}

/// The Syma kernel — holds evaluation state across requests.
#[derive(Clone)]
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
    /// The input is lexed, parsed, and evaluated. All statements in the input
    /// are evaluated; results are returned as an array. Uses standard
    /// (FullForm-style) output format.
    pub fn eval(&self, input: &str) -> KernelResult {
        self.eval_inner(input, None)
    }

    /// Evaluate with an explicit display format.
    fn eval_inner(&self, input: &str, display_format: Option<Format>) -> KernelResult {
        let start = Instant::now();

        let tokens = match lexer::tokenize(input) {
            Ok(t) => t,
            Err(e) => {
                let elapsed = start.elapsed();
                return KernelResult {
                    success: false,
                    results: None,
                    messages: vec![],
                    error: Some(format!("Lexical error: {e}")),
                    timing_ms: elapsed.as_millis() as u64,
                };
            }
        };

        let ast = match parser::parse_with_suppress(tokens) {
            Ok(a) => a,
            Err(e) => {
                let elapsed = start.elapsed();
                return KernelResult {
                    success: false,
                    results: None,
                    messages: vec![],
                    error: Some(format!("Parse error: {e}")),
                    timing_ms: elapsed.as_millis() as u64,
                };
            }
        };

        let (val_result, messages) =
            crate::messages::with_buffer(|| eval::eval_program_with_results(&ast, &self.env));

        match val_result {
            Ok(results) => {
                let elapsed = start.elapsed();
                let json_results: Vec<Option<StatementResult>> = results
                    .into_iter()
                    .map(|opt_val| match opt_val {
                        None => None,
                        Some(val) => {
                            let json_val = value_to_json_full(&val);
                            let output = match &display_format {
                                Some(fmt) => Value::Formatted {
                                    format: fmt.clone(),
                                    value: Box::new(val),
                                }
                                .to_string(),
                                None => val.to_string(),
                            };
                            Some(StatementResult {
                                output,
                                value: json_val,
                            })
                        }
                    })
                    .collect();
                KernelResult {
                    success: true,
                    results: Some(json_results),
                    messages,
                    error: None,
                    timing_ms: elapsed.as_millis() as u64,
                }
            }
            Err(e) => {
                let elapsed = start.elapsed();
                KernelResult {
                    success: false,
                    results: None,
                    messages,
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
    ///
    /// Malformed JSON, non-object requests, non-string `input`, and unknown
    /// `format` values yield a structured `success: false` response rather than
    /// silently evaluating the empty string.
    pub fn eval_json(&self, request: &str) -> String {
        let start = Instant::now();

        let error_response = |msg: String| {
            let result = KernelResult {
                success: false,
                results: None,
                messages: vec![],
                error: Some(msg),
                timing_ms: start.elapsed().as_millis() as u64,
            };
            serde_json::to_string(&result).unwrap_or_else(|e| {
                format!(r#"{{"success":false,"error":"JSON serialisation failed: {e}"}}"#)
            })
        };

        let parsed: serde_json::Value = match serde_json::from_str(request) {
            Ok(v) => v,
            Err(e) => return error_response(format!("Invalid JSON request: {e}")),
        };

        let map = match &parsed {
            serde_json::Value::Object(m) => m,
            other => {
                return error_response(format!(
                    "Request must be a JSON object, got {}",
                    json_type_name(other)
                ));
            }
        };

        let input = match map.get("input") {
            None => "",
            Some(serde_json::Value::String(s)) => s.as_str(),
            Some(other) => {
                return error_response(format!(
                    "`input` must be a string, got {}",
                    json_type_name(other)
                ));
            }
        };

        let fmt = match map.get("format") {
            None => None,
            Some(serde_json::Value::String(s)) => match s.to_lowercase().as_str() {
                "inputform" => Some(Format::InputForm),
                "fullform" => Some(Format::FullForm),
                "standardform" => Some(Format::StandardForm),
                "outputform" => Some(Format::OutputForm),
                other => {
                    return error_response(format!(
                        "Unknown format '{other}' \
                         (expected inputform|fullform|standardform|outputform)"
                    ));
                }
            },
            Some(other) => {
                return error_response(format!(
                    "`format` must be a string, got {}",
                    json_type_name(other)
                ));
            }
        };

        let result = self.eval_inner(input, fmt);
        serde_json::to_string(&result).unwrap_or_else(|e| {
            format!(r#"{{"success":false,"error":"JSON serialisation failed: {e}"}}"#)
        })
    }
}

/// Human-readable JSON type name for protocol error messages.
fn json_type_name(v: &serde_json::Value) -> &'static str {
    match v {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "boolean",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
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
        assert!(result.error.is_none());
        let results = result.results.expect("expected results");
        assert_eq!(results.len(), 1);
        let stmt = results[0].as_ref().expect("expected a result");
        assert_eq!(stmt.output, "42");
        let json = &stmt.value;
        assert_eq!(json.get("t").and_then(|v| v.as_str()), Some("int"));
        assert_eq!(json.get("v").and_then(|v| v.as_str()), Some("42"));
    }

    #[test]
    fn test_eval_addition() {
        let kernel = make_kernel();
        let result = kernel.eval("1 + 2");
        assert!(result.success);
        let results = result.results.expect("expected results");
        let stmt = results[0].as_ref().expect("expected a result");
        assert_eq!(stmt.output, "3");
        assert_eq!(stmt.value.get("v").and_then(|v| v.as_str()), Some("3"));
    }

    #[test]
    fn test_eval_string() {
        let kernel = make_kernel();
        let result = kernel.eval(r#""hello world""#);
        assert!(result.success);
        let results = result.results.expect("expected results");
        let stmt = results[0].as_ref().expect("expected a result");
        assert_eq!(stmt.output, r#""hello world""#);
    }

    #[test]
    fn test_eval_list() {
        let kernel = make_kernel();
        let result = kernel.eval("{1, 2, 3}");
        assert!(result.success);
        let results = result.results.expect("expected results");
        let stmt = results[0].as_ref().expect("expected a result");
        assert_eq!(stmt.output, "{1, 2, 3}");
        // Verify tagged JSON: list
        let json = &stmt.value;
        assert_eq!(json.get("t").and_then(|v| v.as_str()), Some("list"));
    }

    #[test]
    fn test_eval_call_result() {
        let kernel = make_kernel();
        let result = kernel.eval("Integrate[x^2, x]");
        assert!(result.success);
        let results = result.results.expect("expected results");
        let stmt = results[0].as_ref().expect("expected a result");
        let json = &stmt.value;
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
        assert!(result.results.is_none());
    }

    #[test]
    fn test_eval_json_protocol() {
        let kernel = make_kernel();
        let response = kernel.eval_json(r#"{"input": "2+2"}"#);
        let parsed: serde_json::Value = serde_json::from_str(&response).unwrap();
        assert_eq!(parsed["success"], true);
        assert_eq!(parsed["results"][0]["output"], "4");
    }

    #[test]
    fn test_eval_json_empty() {
        let kernel = make_kernel();
        let response = kernel.eval_json(r#"{}"#);
        let parsed: serde_json::Value = serde_json::from_str(&response).unwrap();
        assert!(parsed["success"].as_bool().unwrap_or(false));
    }

    #[test]
    fn test_eval_json_malformed_is_error() {
        let kernel = make_kernel();
        let response = kernel.eval_json("2 + 2");
        let parsed: serde_json::Value = serde_json::from_str(&response).unwrap();
        assert_eq!(parsed["success"], false);
        assert!(
            parsed["error"]
                .as_str()
                .unwrap_or("")
                .contains("Invalid JSON"),
            "got: {response}"
        );
        assert!(parsed["results"].is_null());
    }

    #[test]
    fn test_eval_json_non_object_is_error() {
        let kernel = make_kernel();
        for req in ["[1, 2, 3]", r#""hello""#, "42", "null", "true"] {
            let response = kernel.eval_json(req);
            let parsed: serde_json::Value = serde_json::from_str(&response).unwrap();
            assert_eq!(parsed["success"], false, "request {req} should fail");
            assert!(
                parsed["error"]
                    .as_str()
                    .unwrap_or("")
                    .contains("must be a JSON object"),
                "request {req} got: {response}"
            );
        }
    }

    #[test]
    fn test_eval_json_non_string_input_is_error() {
        let kernel = make_kernel();
        let response = kernel.eval_json(r#"{"input": 42}"#);
        let parsed: serde_json::Value = serde_json::from_str(&response).unwrap();
        assert_eq!(parsed["success"], false);
        assert!(
            parsed["error"]
                .as_str()
                .unwrap_or("")
                .contains("must be a string"),
            "got: {response}"
        );
    }

    #[test]
    fn test_eval_json_unknown_format_is_error() {
        let kernel = make_kernel();
        let response = kernel.eval_json(r#"{"input": "1+1", "format": "bogus"}"#);
        let parsed: serde_json::Value = serde_json::from_str(&response).unwrap();
        assert_eq!(parsed["success"], false);
        assert!(
            parsed["error"]
                .as_str()
                .unwrap_or("")
                .contains("Unknown format"),
            "got: {response}"
        );
    }

    #[test]
    fn test_eval_json_non_string_format_is_error() {
        let kernel = make_kernel();
        let response = kernel.eval_json(r#"{"input": "1+1", "format": 7}"#);
        let parsed: serde_json::Value = serde_json::from_str(&response).unwrap();
        assert_eq!(parsed["success"], false);
        assert!(
            parsed["error"]
                .as_str()
                .unwrap_or("")
                .contains("must be a string"),
            "got: {response}"
        );
    }

    #[test]
    fn test_eval_json_format_case_insensitive() {
        let kernel = make_kernel();
        let response = kernel.eval_json(r#"{"input": "Plus[a, b]", "format": "FULLFORM"}"#);
        let parsed: serde_json::Value = serde_json::from_str(&response).unwrap();
        assert_eq!(parsed["success"], true);
        let out = parsed["results"][0]["output"].as_str().unwrap_or("");
        assert!(out.contains("Plus"), "got: {out}");
    }

    #[test]
    fn test_eval_json_single_line_framing() {
        // Every response must be a single line so the line-framed protocol works.
        let kernel = make_kernel();
        let response = kernel.eval_json(r#"{"input": "\"a\\nb\""}"#);
        assert!(
            !response.contains('\n'),
            "response contains a raw newline: {response:?}"
        );
        let parsed: serde_json::Value = serde_json::from_str(&response).unwrap();
        assert_eq!(parsed["success"], true);
    }

    #[test]
    fn test_eval_json_inputform_format() {
        let kernel = make_kernel();
        // Default (no format) returns FullForm: Plus[Power[x, 2], Times[2, x, y], Power[y, 2]]
        let default_response = kernel.eval_json(r#"{"input": "Expand[(x+y)^2]"}"#);
        let default_parsed: serde_json::Value = serde_json::from_str(&default_response).unwrap();
        let default_output = default_parsed["results"][0]["output"]
            .as_str()
            .unwrap_or("");

        // InputForm returns infix notation: x^2 + 2*x*y + y^2
        let inputform_response =
            kernel.eval_json(r#"{"input": "Expand[(x+y)^2]", "format": "inputform"}"#);
        let inputform_parsed: serde_json::Value =
            serde_json::from_str(&inputform_response).unwrap();
        let inputform_output = inputform_parsed["results"][0]["output"]
            .as_str()
            .unwrap_or("");

        // The two outputs should be different
        assert_ne!(
            default_output, inputform_output,
            "InputForm and default output should differ for symbolic expressions"
        );
        // InputForm should contain infix operators
        assert!(
            inputform_output.contains('+') || inputform_output.contains('^'),
            "InputForm output should contain infix operators, got: {inputform_output}"
        );
        // Default (FullForm) should contain '[' and ']'
        assert!(
            default_output.contains('['),
            "Default output should be FullForm with brackets, got: {default_output}"
        );
    }

    #[test]
    fn test_eval_holds_state() {
        let kernel = make_kernel();
        kernel.eval("x = 42");
        let result = kernel.eval("x + 1");
        assert!(result.success);
        let results = result.results.expect("expected results");
        let stmt = results[0].as_ref().expect("expected a result");
        assert_eq!(stmt.output, "43");
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
        assert!(result.timing_ms > 0 || result.results.is_some());
    }

    #[test]
    fn test_multi_statement() {
        let kernel = make_kernel();
        // 1\n2\n3;\n4 — three statements with third suppressed by ;
        let result = kernel.eval("1\n2\n3;\n4");
        assert!(result.success);
        let results = result.results.expect("expected results");
        assert_eq!(results.len(), 4);
        // First: 1
        let r0 = results[0].as_ref().expect("expected result");
        assert_eq!(r0.output, "1");
        assert_eq!(r0.value.get("v").and_then(|v| v.as_str()), Some("1"));
        // Second: 2
        let r1 = results[1].as_ref().expect("expected result");
        assert_eq!(r1.output, "2");
        assert_eq!(r1.value.get("v").and_then(|v| v.as_str()), Some("2"));
        // Third: suppressed by ;
        assert!(results[2].is_none());
        // Fourth: 4
        let r3 = results[3].as_ref().expect("expected result");
        assert_eq!(r3.output, "4");
        assert_eq!(r3.value.get("v").and_then(|v| v.as_str()), Some("4"));
    }
}
