// `cloned_ref_to_slice_refs` — suggestion `std::slice::from_ref` gives `&[&T]` but
// apply_function/simplify_call take `&[T]`, so the fix doesn't apply in this codebase.
#![allow(clippy::cloned_ref_to_slice_refs)]

pub mod ast;
pub mod builtins;
pub mod bytecode;
pub mod cli;
pub mod debug;
pub mod env;
pub mod eval;
pub mod ffi;
pub mod format;
pub mod kernel;
pub mod lexer;
pub mod manifest;
pub mod messages;
pub mod parser;
pub mod pattern;
pub mod polynomial;
pub mod profiler;
pub mod value;

#[cfg(feature = "jit")]
pub mod jit;

use std::fs;

use crate::format::{bold_red, dim};

pub const VERSION: &str = "0.1.0";

/// Helper: tokenize and parse input. `suppress=true` uses parse_with_suppress.
type ParseResult = Vec<(ast::Expr, bool)>;

fn tokenize_and_parse<'a>(input: &'a str, suppress: bool) -> Option<ParseResult> {
    let tokens = match lexer::tokenize(input) {
        Ok(tokens) => tokens,
        Err(e) => {
            print_error("LexError", &e.to_string(), input);
            return None;
        }
    };
    let result = if suppress {
        match parser::parse_with_suppress(tokens) {
            Ok(ast) => ast,
            Err(e) => {
                print_error("ParseError", &e.to_string(), input);
                return None;
            }
        }
    } else {
        match parser::parse(tokens) {
            Ok(ast) => ast.into_iter().map(|e| (e, false)).collect(),
            Err(e) => {
                print_error("ParseError", &e.to_string(), input);
                return None;
            }
        }
    };
    Some(result)
}

/// Returns the value if successful, None on error.
pub fn eval_input(input: &str, env: &env::Env) -> Option<value::Value> {
    let parsed = tokenize_and_parse(input, false)?;
    let exprs: Vec<ast::Expr> = parsed.into_iter().map(|(e, _)| e).collect();
    match eval::eval_program(&exprs, env) {
        Ok(value) => Some(value),
        Err(e) => {
            print_error("Error", &e.to_string(), input);
            None
        }
    }
}

/// Evaluate multi-statement input, returning the result of each statement.
/// `None` in the returned vector means the statement was suppressed by `;`.
pub fn eval_input_with_results(input: &str, env: &env::Env) -> Option<Vec<Option<value::Value>>> {
    let parsed = tokenize_and_parse(input, true)?;
    match eval::eval_program_with_results(&parsed, env) {
        Ok(results) => Some(results),
        Err(e) => {
            print_error("Error", &e.to_string(), input);
            None
        }
    }
}

pub fn run_file(path: &str) -> Result<(), String> {
    let source =
        fs::read_to_string(path).map_err(|e| format!("Error reading '{}': {}", path, e))?;

    let env = env::Env::new();
    builtins::register_builtins(&env);

    // Add the file's directory to the module search path so that
    // `import Foo` can find sibling files (e.g. `Foo.syma`).
    if let Some(parent) = std::path::Path::new(path).parent()
        && parent != std::path::Path::new("")
    {
        env.add_search_path(parent.to_path_buf());
    }

    let tokens = lexer::tokenize(&source).map_err(|e| format!("{}", e))?;
    let stmts = parser::parse_with_suppress(tokens).map_err(|e| format!("{}", e))?;

    for (stmt, suppress) in &stmts {
        match eval::eval(stmt, &env) {
            Ok(value::Value::Null) => {}
            Ok(value) if !suppress => println!("{}", value),
            Ok(_) => {}
            Err(e) => print_error("Error", &e.to_string(), path),
        }
    }
    Ok(())
}

pub fn print_error(label: &str, message: &str, source: &str) {
    eprintln!("{}: {}", bold_red(label), message);
    eprintln!("  {}", dim(source));
}
