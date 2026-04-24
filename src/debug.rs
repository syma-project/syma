/// Debug adapter protocol for Syma.
///
/// Implements a custom JSON-based debug protocol over stdin/stdout
/// that a VS Code debug adapter bridges to the DAP.
use std::collections::HashSet;
use std::io::{self, BufRead, Write};

use serde::{Deserialize, Serialize};

use crate::builtins;
use crate::env::Env;
use crate::eval;
use crate::lexer;
use crate::parser;
use crate::value::Value;

// ── Protocol messages ──────────────────────────────────────────────────────────

/// Messages received from the debug client (VS Code adapter) on stdin.
#[derive(Debug, Deserialize)]
#[serde(tag = "command", rename_all = "camelCase")]
enum ClientMessage {
    SetBreakpoints {
        breakpoints: Vec<BreakpointInfo>,
    },
    Continue,
    Next,
    StepIn,
    StepOut,
    Stop,
    GetVariables,
    Evaluate {
        expression: String,
    },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BreakpointInfo {
    line: usize,
}

/// Messages sent to the debug client on stdout.
#[derive(Debug, Serialize)]
#[serde(tag = "event", rename_all = "camelCase")]
enum ServerEvent {
    Stopped {
        reason: String,
        line: usize,
    },
    Terminated,
    Variables {
        variables: Vec<VariableInfo>,
    },
    EvaluateResult {
        result: String,
        #[serde(rename = "type")]
        value_type: String,
    },
    Output {
        category: String,
        output: String,
    },
    Error {
        message: String,
    },
    Initialized,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct VariableInfo {
    name: String,
    value: String,
    #[serde(rename = "type")]
    var_type: String,
}

// ── Debug state ────────────────────────────────────────────────────────────────

struct Debugger {
    breakpoints: HashSet<usize>,
    stepping: bool,
    stopped: bool,
    stop_requested: bool,
}

impl Debugger {
    fn new() -> Self {
        Debugger {
            breakpoints: HashSet::new(),
            stepping: false,
            stopped: false,
            stop_requested: false,
        }
    }

    /// Check whether execution should pause at the given line.
    fn should_pause(&self, line: usize) -> bool {
        if self.stop_requested {
            return true;
        }
        if self.stepping {
            return true;
        }
        self.breakpoints.contains(&line)
    }
}

// ── Public entry point ─────────────────────────────────────────────────────────

/// Run a file in debug mode. Communicates with the debug client over stdin/stdout.
pub fn run_debug(path: &str) {
    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            send_event(&ServerEvent::Error {
                message: format!("Error reading '{}': {}", path, e),
            });
            return;
        }
    };

    let tokens = match lexer::tokenize(&source) {
        Ok(t) => t,
        Err(e) => {
            send_event(&ServerEvent::Error {
                message: format!("LexError: {}", e),
            });
            return;
        }
    };

    let stmts = match parser::parse_with_debug_info(tokens) {
        Ok(s) => s,
        Err(e) => {
            send_event(&ServerEvent::Error {
                message: format!("ParseError: {}", e),
            });
            return;
        }
    };

    let env = Env::new();
    builtins::register_builtins(&env);

    if let Some(parent) = std::path::Path::new(path).parent() {
        if parent != std::path::Path::new("") {
            env.add_search_path(parent.to_path_buf());
        }
    }

    let mut dbg = Debugger::new();

    // Notify client we're ready
    send_event(&ServerEvent::Initialized);

    // Wait for initial configuration (breakpoints, etc.)
    // We loop reading commands until we get a Continue or StepIn
    loop {
        match read_client_message() {
            Some(msg) => match msg {
                ClientMessage::SetBreakpoints { breakpoints } => {
                    dbg.breakpoints = breakpoints.into_iter().map(|b| b.line).collect();
                }
                ClientMessage::Continue => {
                    dbg.stepping = false;
                    break;
                }
                ClientMessage::Next | ClientMessage::StepIn => {
                    dbg.stepping = true;
                    break;
                }
                ClientMessage::Stop => {
                    send_event(&ServerEvent::Terminated);
                    return;
                }
                _ => {}
            },
            None => {
                // EOF — client disconnected
                return;
            }
        }
    }

    // Execute statements
    for (stmt, suppress, line) in &stmts {
        // Check if we should pause before executing this statement
        if dbg.should_pause(*line) {
            dbg.stopped = true;
            dbg.stepping = false;

            send_event(&ServerEvent::Stopped {
                reason: if dbg.stop_requested {
                    "pause".to_string()
                } else if dbg.breakpoints.contains(line) {
                    "breakpoint".to_string()
                } else {
                    "step".to_string()
                },
                line: *line,
            });
            dbg.stop_requested = false;

            // Command loop while paused
            loop {
                match read_client_message() {
                    Some(msg) => match msg {
                        ClientMessage::Continue => {
                            dbg.stepping = false;
                            dbg.stopped = false;
                            break;
                        }
                        ClientMessage::Next | ClientMessage::StepIn => {
                            dbg.stepping = true;
                            dbg.stopped = false;
                            break;
                        }
                        ClientMessage::StepOut => {
                            // For Phase 1, treat stepOut same as continue
                            dbg.stepping = false;
                            dbg.stopped = false;
                            break;
                        }
                        ClientMessage::Stop => {
                            send_event(&ServerEvent::Terminated);
                            return;
                        }
                        ClientMessage::SetBreakpoints { breakpoints } => {
                            dbg.breakpoints = breakpoints.into_iter().map(|b| b.line).collect();
                        }
                        ClientMessage::GetVariables => {
                            let vars = collect_variables(&env);
                            send_event(&ServerEvent::Variables { variables: vars });
                        }
                        ClientMessage::Evaluate { expression } => {
                            evaluate_expression(&expression, &env);
                        }
                    },
                    None => return, // EOF
                }
            }
        }

        // Execute the statement
        match eval::eval(stmt, &env) {
            Ok(value) => {
                if !suppress && value != Value::Null {
                    send_event(&ServerEvent::Output {
                        category: "stdout".to_string(),
                        output: format!("{}\n", value),
                    });
                }
            }
            Err(e) => {
                send_event(&ServerEvent::Output {
                    category: "stderr".to_string(),
                    output: format!("Error at line {}: {}\n", line, e),
                });
            }
        }
    }

    // Program finished
    send_event(&ServerEvent::Terminated);
}

// ── Helpers ────────────────────────────────────────────────────────────────────

fn collect_variables(env: &Env) -> Vec<VariableInfo> {
    env.all_bindings()
        .into_iter()
        .filter(|(name, _)| !name.starts_with("__"))
        .map(|(name, val)| VariableInfo {
            var_type: val.type_name().to_string(),
            value: format_value(&val),
            name,
        })
        .collect()
}

fn format_value(val: &Value) -> String {
    match val {
        Value::Integer(n) => n.to_string(),
        Value::Real(r) => r.to_string(),
        Value::Str(s) => format!("\"{}\"", s),
        Value::Bool(b) => b.to_string(),
        Value::Null => "Null".to_string(),
        Value::Symbol(s) => s.clone(),
        Value::List(items) => {
            if items.len() <= 5 {
                let parts: Vec<String> = items.iter().map(format_value).collect();
                format!("{{{}}}", parts.join(", "))
            } else {
                format!("{{...{} items...}}", items.len())
            }
        }
        Value::Function(f) => format!("<function {}>", f.name),
        Value::Builtin(name, _) => format!("<builtin {}>", name),
        Value::PureFunction { .. } => "<pure function>".to_string(),
        Value::Call { head, args } => {
            format!("{}[...{}]", head, args.len())
        }
        Value::Assoc(_) => "<association>".to_string(),
        Value::Rule { .. } => "<rule>".to_string(),
        Value::RuleSet { name, .. } => format!("<ruleset {}>", name),
        Value::Pattern(_) => "<pattern>".to_string(),
        Value::Module { name, .. } => format!("<module {}>", name),
        Value::Object { class_name, .. } => format!("<object {}>", class_name),
        Value::Method { name, .. } => format!("<method {}>", name),
        Value::Hold(_) => "<hold>".to_string(),
        Value::HoldComplete(_) => "<hold>".to_string(),
        Value::Complex { re, im } => format!("{}+{}I", re, im),
    }
}

fn evaluate_expression(expr_str: &str, env: &Env) {
    match lexer::tokenize(expr_str) {
        Ok(tokens) => match parser::parse(tokens) {
            Ok(ast) => {
                if let Some(stmt) = ast.first() {
                    match eval::eval(stmt, env) {
                        Ok(val) => {
                            send_event(&ServerEvent::EvaluateResult {
                                result: format_value(&val),
                                value_type: val.type_name().to_string(),
                            });
                        }
                        Err(e) => {
                            send_event(&ServerEvent::EvaluateResult {
                                result: format!("Error: {}", e),
                                value_type: "error".to_string(),
                            });
                        }
                    }
                }
            }
            Err(e) => {
                send_event(&ServerEvent::EvaluateResult {
                    result: format!("ParseError: {}", e),
                    value_type: "error".to_string(),
                });
            }
        },
        Err(e) => {
            send_event(&ServerEvent::EvaluateResult {
                result: format!("LexError: {}", e),
                value_type: "error".to_string(),
            });
        }
    }
}

fn send_event(event: &ServerEvent) {
    if let Ok(json) = serde_json::to_string(event) {
        let stdout = io::stdout();
        let mut out = stdout.lock();
        let _ = writeln!(out, "{}", json);
        let _ = out.flush();
    }
}

fn read_client_message() -> Option<ClientMessage> {
    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();
    match lines.next() {
        Some(Ok(line)) => {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                return read_client_message();
            }
            match serde_json::from_str::<ClientMessage>(trimmed) {
                Ok(msg) => Some(msg),
                Err(e) => {
                    eprintln!("[syma debug] Failed to parse command: {}", e);
                    read_client_message() // skip bad lines
                }
            }
        }
        _ => None,
    }
}
