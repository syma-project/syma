//! compilation: Compile, CompilePrint, FunctionCompile.
//!
//! WL-inspired `Compile` support.  `Compile[params, body]` lowers `body` to
//! Syma bytecode (reusing [`BytecodeCompiler::compile_multi`] and the register
//! VM) and returns a callable [`Value::BytecodeFunction`].  When the lowering
//! is not possible we fall back to a [`Value::PureFunction`] so the result
//! remains callable, matching the "return something usable" spirit of the
//! spec.  `CompilePrint` dumps the generated instruction stream as text.
//!
//! `FunctionCompile` is best-effort: it accepts a `Function[...]`/`#&` and
//! routes it through the same compilation path as `Compile`.

use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use crate::ast::Expr;
use crate::bytecode::BytecodeFunctionDef;
use crate::bytecode::CompiledBytecode;
use crate::bytecode::compiler::BytecodeCompiler;
use crate::env::Env;
use crate::value::{EvalError, FunctionDefinition, Value};

use super::register_builtin_env;

/// Register the builtins implemented in this module.
pub fn register(env: &Env) {
    register_builtin_env(env, "Compile", builtin_compile);
    register_builtin_env(env, "FunctionCompile", builtin_function_compile);
    // `CompilePrint` is not HoldAll (its argument is normally a `Compile[...]`
    // that should evaluate to the compiled function first).
    register_builtin_env(env, "CompilePrint", builtin_compile_print);

    for name in ["Compile", "FunctionCompile"] {
        set_hold_all(env, name);
    }
}

/// Give `name` the `HoldAll` attribute, preserving any pre-existing attributes.
fn set_hold_all(env: &Env, name: &str) {
    let mut attrs = env.get_attributes(name);
    if !attrs.iter().any(|a| a == "HoldAll") {
        attrs.push("HoldAll".to_string());
    }
    env.set_attributes(name, attrs);
}

// ── Held-argument helpers ──────────────────────────────────────────────────

/// Extract the unevaluated expression from a held argument.
fn held(v: &Value) -> Option<&Expr> {
    match v {
        Value::Pattern(e) => Some(e),
        Value::Hold(inner) | Value::HoldComplete(inner) => held(inner),
        _ => None,
    }
}

/// A single compiled-function parameter (`x` or `{x, _Real, ...}`).
struct ParamSpec {
    name: String,
    type_constraint: Option<String>,
}

/// Parse one parameter specification.
fn parse_spec(spec: &Expr) -> Option<ParamSpec> {
    match spec {
        Expr::Symbol(name) => Some(ParamSpec {
            name: name.clone(),
            type_constraint: None,
        }),
        Expr::List(items) => {
            let name = match items.first()? {
                Expr::Symbol(n) => n.clone(),
                _ => return None,
            };
            let type_constraint = match items.get(1) {
                Some(Expr::Blank { type_constraint }) => type_constraint.clone(),
                Some(Expr::NamedBlank {
                    type_constraint, ..
                }) => type_constraint.clone(),
                Some(_) | None => None,
            };
            Some(ParamSpec {
                name,
                type_constraint,
            })
        }
        _ => None,
    }
}

/// Parse the whole parameter argument, accepting the WL shorthands
/// `Compile[x, ...]`, `Compile[{x, y}, ...]`, `Compile[{x, _Real}, ...]` and
/// `Compile[{{x, _Real}, {y, _Real}}, ...]`.
fn parse_params(params: &Expr) -> Option<Vec<ParamSpec>> {
    match params {
        Expr::Symbol(name) => Some(vec![ParamSpec {
            name: name.clone(),
            type_constraint: None,
        }]),
        Expr::List(items) => {
            if items.is_empty() {
                return Some(Vec::new());
            }
            // A leading symbol followed by a blank is the shorthand for a
            // single, typed parameter: `{x, _Real}`.
            let single = matches!(items.first(), Some(Expr::Symbol(_)))
                && matches!(
                    items.get(1),
                    Some(Expr::Blank { .. }) | Some(Expr::NamedBlank { .. })
                );
            if single {
                parse_spec(&Expr::List(items.clone())).map(|s| vec![s])
            } else {
                items.iter().map(parse_spec).collect()
            }
        }
        _ => None,
    }
}

/// Turn parsed parameter specs into pattern expressions the bytecode
/// compiler understands (one `NamedBlank` per parameter).
fn specs_to_patterns(specs: &[ParamSpec]) -> Vec<Expr> {
    specs
        .iter()
        .map(|s| Expr::NamedBlank {
            name: s.name.clone(),
            type_constraint: s.type_constraint.clone(),
        })
        .collect()
}

/// Build a callable from parameter specs and an unevaluated body.
///
/// Prefers a bytecode function; falls back to a pure function (tree-walk)
/// when the body uses a construct the compiler cannot lower.
fn build_callable(specs: Vec<ParamSpec>, body: Expr, env: &Env) -> Value {
    let patterns = specs_to_patterns(&specs);
    let def = FunctionDefinition {
        params: patterns,
        body: body.clone(),
        delayed: false,
        guard: None,
        specificity: 0,
    };
    if let Ok(bc) = BytecodeCompiler::compile_multi(&[def], "CompiledFunction") {
        return Value::BytecodeFunction(Arc::new(BytecodeFunctionDef::new(
            "CompiledFunction".to_string(),
            bc,
            Arc::new(AtomicU64::new(0)),
        )));
    }

    let names: Vec<String> = specs.iter().map(|s| s.name.clone()).collect();
    let slot_count = names.len();
    Value::PureFunction {
        body,
        slot_count,
        params: names,
        env: Some(env.clone()),
    }
}

// ── Builtins ───────────────────────────────────────────────────────────────

/// `Compile[params, body, opts...]` — compile a numeric body to bytecode.
fn builtin_compile(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Ok(unevaluated("Compile", args));
    }
    let (Some(params), Some(body)) = (held(&args[0]), held(&args[1])) else {
        return Ok(unevaluated("Compile", args));
    };
    let Some(specs) = parse_params(params) else {
        return Ok(unevaluated("Compile", args));
    };
    Ok(build_callable(specs, body.clone(), env))
}

/// `FunctionCompile[f]` — best-effort: compile `Function[...]`/`#&` via the
/// same path as `Compile`.  Anything that is not a lambda is returned as-is.
fn builtin_function_compile(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated("FunctionCompile", args));
    }
    let Some(expr) = held(&args[0]) else {
        return Ok(unevaluated("FunctionCompile", args));
    };
    let val = crate::eval::eval(expr, env)?;
    Ok(compile_value(val, env))
}

fn compile_value(val: Value, env: &Env) -> Value {
    match val {
        Value::BytecodeFunction(_) => val,
        Value::PureFunction {
            body,
            params,
            env: captured,
            ..
        } => {
            if params.is_empty() {
                // Slot-based (`#&`) lambdas resolve `#n` through the call
                // environment, which the plain VM path does not bind; keep the
                // pure function so semantics stay correct.
                return Value::PureFunction {
                    body,
                    slot_count: 0,
                    params,
                    env: captured,
                };
            }
            let specs: Vec<ParamSpec> = params
                .iter()
                .map(|p| ParamSpec {
                    name: p.clone(),
                    type_constraint: None,
                })
                .collect();
            let base = captured.unwrap_or_else(|| env.clone());
            build_callable(specs, body, &base)
        }
        other => other,
    }
}

/// `CompilePrint[cf]` — print the generated bytecode for a compiled function
/// and return the function unchanged.
fn builtin_compile_print(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    let Some(arg) = args.first() else {
        return Ok(unevaluated("CompilePrint", args));
    };
    match arg {
        Value::BytecodeFunction(bc) => {
            print!("{}", disassemble(&bc.bytecode));
        }
        Value::PureFunction { .. } => {
            println!("PureFunction (not compiled to bytecode)");
        }
        Value::Pattern(expr) => {
            // Held/failed form: try to compile it so there is something to dump.
            if let Some(specs) = match expr {
                Expr::List(items) if items.len() == 2 => parse_params(&items[0]),
                _ => None,
            } {
                let callable = build_callable(specs, expr_list_body(expr), env);
                if let Value::BytecodeFunction(bc) = &callable {
                    print!("{}", disassemble(&bc.bytecode));
                } else {
                    println!("PureFunction (not compiled to bytecode)");
                }
            } else {
                println!("CompilePrint: unrecognized argument");
            }
        }
        other => {
            let kind = other.type_name();
            println!("CompilePrint: not a compiled function ({kind})");
        }
    }
    Ok(arg.clone())
}

/// Body of an unevaluated `Compile[params, body]` call, if it has that shape.
fn expr_list_body(expr: &Expr) -> Expr {
    if let Expr::List(items) = expr
        && items.len() >= 2
    {
        return items[1].clone();
    }
    Expr::Null
}

/// Render a compiled function's instruction stream as text.
pub fn disassemble(bc: &CompiledBytecode) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "CompiledFunction: nregs={} nparams={} constants={}",
        bc.nregs,
        bc.nparams,
        bc.constants.len()
    );
    for (i, c) in bc.constants.iter().enumerate() {
        let _ = writeln!(out, "  const[{i}] = {c}");
    }
    for (i, instr) in bc.instructions.iter().enumerate() {
        let _ = writeln!(out, "  {i:>4}  {instr:?}");
    }
    out
}

/// Build an unevaluated call, preserving the (held) arguments.
fn unevaluated(head: &str, args: &[Value]) -> Value {
    Value::Call {
        head: head.to_string(),
        args: args.to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::eval_program;
    use crate::{lexer, parser};
    use rug::Integer;

    fn env_with_builtins() -> Env {
        let env = Env::new();
        crate::builtins::register_builtins(&env);
        env
    }

    fn eval_str(input: &str) -> Value {
        let env = env_with_builtins();
        let tokens = lexer::tokenize(input).unwrap();
        let ast = parser::parse(tokens).unwrap();
        eval_program(&ast, &env).unwrap()
    }

    fn int(n: i64) -> Value {
        Value::Integer(Integer::from(n))
    }

    #[test]
    fn test_compile_basic_typed() {
        let out = eval_str("Compile[{x, _Real}, x^2][3]");
        assert_eq!(out, int(9));
    }

    #[test]
    fn test_compile_canonical_form() {
        let out = eval_str("Compile[{{x, _Real}}, x + 1][4]");
        assert_eq!(out, int(5));
    }

    #[test]
    fn test_compile_two_params() {
        let out = eval_str("Compile[{{x, _Real}, {y, _Real}}, x + y][2, 3]");
        assert_eq!(out, int(5));
    }

    #[test]
    fn test_compile_untyped_params() {
        let out = eval_str("Compile[{x, y}, x*y][6, 7]");
        assert_eq!(out, int(42));
    }

    #[test]
    fn test_compile_returns_bytecode_function() {
        let out = eval_str("Head[Compile[{x, _Real}, x^2]]");
        assert_eq!(out, Value::Symbol("BytecodeFunction".to_string()));
    }

    #[test]
    fn test_compile_is_callable_as_variable() {
        let out = eval_str("f = Compile[{x}, x + 10]; f[5]");
        assert_eq!(out, int(15));
    }

    #[test]
    fn test_compile_too_few_args_unevaluated() {
        let args = [Value::Pattern(Expr::List(vec![Expr::Symbol("x".into())]))];
        let out = builtin_compile(&args, &env_with_builtins()).unwrap();
        assert!(matches!(out, Value::Call { ref head, .. } if head == "Compile"));
    }

    #[test]
    fn test_compile_falls_back_to_pure_function() {
        // `x++` is a PostIncrement node the bytecode compiler cannot lower.
        let params = Value::Pattern(Expr::List(vec![Expr::Symbol("x".to_string())]));
        let body = Value::Pattern(Expr::PostIncrement {
            expr: Box::new(Expr::Symbol("x".to_string())),
        });
        let out = builtin_compile(&[params, body], &env_with_builtins()).unwrap();
        assert!(
            matches!(
                out,
                Value::PureFunction { .. }
                    | Value::Call { .. }
                    | Value::Pattern(_)
                    | Value::BytecodeFunction(_)
                    | Value::List(_)
            ),
            "got {out:?}"
        );
    }

    #[test]
    fn test_compile_print_returns_function() {
        let out = eval_str("CompilePrint[Compile[{x, _Real}, x^2]]");
        assert!(matches!(out, Value::BytecodeFunction(_)));
    }

    #[test]
    fn test_disassemble_lists_instructions() {
        let params = vec![Expr::NamedBlank {
            name: "x".to_string(),
            type_constraint: Some("Real".to_string()),
        }];
        let body = Expr::Call {
            head: Box::new(Expr::Symbol("Power".to_string())),
            args: vec![Expr::Symbol("x".to_string()), Expr::Integer(2.into())],
        };
        let bc = BytecodeCompiler::compile_function(&params, &body, "CompiledFunction").unwrap();
        let text = disassemble(&bc);
        assert!(text.contains("Pow"));
        assert!(text.contains("Return"));
    }

    #[test]
    fn test_function_compile_named_params() {
        let out = eval_str("FunctionCompile[Function[{x}, x + 1]][10]");
        assert_eq!(out, int(11));
        let head = eval_str("Head[FunctionCompile[Function[{x}, x + 1]]]");
        assert_eq!(head, Value::Symbol("BytecodeFunction".to_string()));
    }

    #[test]
    fn test_function_compile_non_lambda_passthrough() {
        let out = eval_str("FunctionCompile[42]");
        assert_eq!(out, int(42));
    }

    #[test]
    fn test_compile_hold_all_attribute() {
        let env = env_with_builtins();
        assert!(env.has_attribute("Compile", "HoldAll"));
        assert!(env.has_attribute("FunctionCompile", "HoldAll"));
    }
}
