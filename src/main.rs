/// Syma: A Symbolic-First Language with OOP Structure
///
/// Phase 1: Tree-walk interpreter with REPL.

mod ast;
mod lexer;
mod parser;
mod value;
mod eval;
mod env;
mod pattern;
mod builtins;

use std::fs;

use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;

const VERSION: &str = "0.1.0";

fn print_usage() {
    println!("Syma v{} — Symbolic-First Language with OOP Structure", VERSION);
    println!();
    println!("Usage:");
    println!("  syma             Start the interactive REPL");
    println!("  syma <file>      Evaluate a Syma source file");
    println!("  syma --help      Show this help message");
    println!("  syma --version   Show version information");
}

fn print_repl_help() {
    println!("Syma REPL Commands:");
    println!("  help             Show this help message");
    println!("  quit, exit, :q   Exit the REPL");
    println!();
    println!("Syntax:");
    println!("  f[x_] := x^2    Function definition");
    println!("  x = 5            Assignment");
    println!("  {{1, 2, 3}}       List literal");
    println!("  \"a\" <> \"b\"      String concatenation");
    println!("  a -> b           Rule");
    println!("  a /. rules       Apply rules");
    println!("  expr // f        Postfix pipe");
    println!("  (* comment *)    Comment");
}

// ANSI color helpers
fn green(s: &str) -> String   { format!("\x1b[32m{}\x1b[0m", s) }
fn red(s: &str) -> String     { format!("\x1b[31m{}\x1b[0m", s) }
fn bold_red(s: &str) -> String { format!("\x1b[1;31m{}\x1b[0m", s) }
fn cyan(s: &str) -> String    { format!("\x1b[36m{}\x1b[0m", s) }
fn dim(s: &str) -> String     { format!("\x1b[2m{}\x1b[0m", s) }

fn print_error(label: &str, message: &str, source: &str) {
    eprintln!("{}: {}", bold_red(label), message);
    eprintln!("  {}", dim(source));
}

/// Returns the value if successful, None on error.
fn eval_input(input: &str, env: &env::Env) -> Option<value::Value> {
    let tokens = match lexer::tokenize(input) {
        Ok(tokens) => tokens,
        Err(e) => {
            print_error("LexError", &e.to_string(), input);
            return None;
        }
    };
    let ast = match parser::parse(tokens) {
        Ok(ast) => ast,
        Err(e) => {
            print_error("ParseError", &e.to_string(), input);
            return None;
        }
    };
    match eval::eval_program(&ast, env) {
        Ok(value) => Some(value),
        Err(e) => {
            print_error("Error", &e.to_string(), input);
            None
        }
    }
}

fn eval_input_silent(input: &str, env: &env::Env) {
    let tokens = match lexer::tokenize(input) {
        Ok(tokens) => tokens,
        Err(e) => {
            print_error("LexError", &e.to_string(), input);
            return;
        }
    };
    let ast = match parser::parse(tokens) {
        Ok(ast) => ast,
        Err(e) => {
            print_error("ParseError", &e.to_string(), input);
            return;
        }
    };
    if let Err(e) = eval::eval_program(&ast, env) {
        print_error("Error", &e.to_string(), input);
    }
}

fn run_file(path: &str) {
    let source = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading '{}': {}", path, e);
            std::process::exit(1);
        }
    };

    let env = env::Env::new();
    builtins::register_builtins(&env);

    for line in source.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("(*") {
            continue;
        }
        // Suppress output for lines ending with ';'
        if line.ends_with(';') {
            eval_input_silent(line, &env);
        } else if let Some(value) = eval_input(line, &env) {
            if value != value::Value::Null {
                println!("{}", value);
            }
        }
    }
}

const HISTORY_FILE: &str = ".syma_history";

fn run_repl() {
    println!("{} — Symbolic-First Language with OOP Structure",
        green(&format!("Syma v{}", VERSION)));
    println!("Type {} for commands, {} to exit.\n",
        cyan("'help'"), cyan("'quit'"));

    let env = env::Env::new();
    builtins::register_builtins(&env);

    let mut rl = match DefaultEditor::new() {
        Ok(rl) => rl,
        Err(e) => {
            eprintln!("Failed to initialize REPL: {}", e);
            std::process::exit(1);
        }
    };

    // Load history from file
    let history_path = dirs_or_default().map(|d| d.join(HISTORY_FILE));
    if let Some(ref path) = history_path {
        let _ = rl.load_history(path);
    }

    let mut counter: usize = 1;

    loop {
        // \x01 / \x02 bracket non-printing chars so rustyline measures width correctly
        let prompt = format!(
            "\x01\x1b[32m\x02In [{}]: \x01\x1b[0m\x02",
            counter
        );
        match rl.readline(&prompt) {
            Ok(line) => {
                let input = line.trim();
                if input.is_empty() {
                    continue;
                }

                // Add to history (skip duplicates)
                let _ = rl.add_history_entry(input);

                // REPL commands
                match input {
                    "quit" | "exit" | ":q" => {
                        println!("Goodbye!");
                        break;
                    }
                    "help" => {
                        print_repl_help();
                        continue;
                    }
                    _ => {}
                }

                if let Some(value) = eval_input(input, &env) {
                    if value != value::Value::Null {
                        println!("{} {}", red(&format!("Out[{}]:", counter)), value);
                    }
                }
                counter += 1;
            }
            Err(ReadlineError::Interrupted) => {
                // Ctrl+C: cancel current input
                println!("{}", dim("KeyboardInterrupt"));
                continue;
            }
            Err(ReadlineError::Eof) => {
                // Ctrl+D: exit
                println!("Goodbye!");
                break;
            }
            Err(e) => {
                eprintln!("Error reading input: {}", e);
                break;
            }
        }
    }

    // Save history
    if let Some(ref path) = history_path {
        let _ = rl.save_history(path);
    }
}

/// Get the home directory for history file storage.
fn dirs_or_default() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME").map(std::path::PathBuf::from)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    match args.get(1).map(|s| s.as_str()) {
        Some("--help") | Some("-h") => print_usage(),
        Some("--version") | Some("-v") => println!("syma {}", VERSION),
        Some(path) => run_file(path),
        None => run_repl(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rug::Integer;

    fn eval_str(input: &str) -> value::Value {
        let env = env::Env::new();
        builtins::register_builtins(&env);
        let tokens = lexer::tokenize(input).unwrap();
        let ast = parser::parse(tokens).unwrap();
        eval::eval_program(&ast, &env).unwrap()
    }

    #[test]
    fn test_arithmetic() {
        assert_eq!(eval_str("1 + 2"), value::Value::Integer(Integer::from(3)));
        assert_eq!(eval_str("3 * 4"), value::Value::Integer(Integer::from(12)));
        assert_eq!(eval_str("10 / 2"), value::Value::Integer(Integer::from(5)));
        assert_eq!(eval_str("2^3"), value::Value::Integer(Integer::from(8)));
    }

    #[test]
    fn test_variables() {
        assert_eq!(eval_str("x = 5; x"), value::Value::Integer(Integer::from(5)));
    }

    #[test]
    fn test_lists() {
        let val = eval_str("{1, 2, 3}");
        assert_eq!(val, value::Value::List(vec![
            value::Value::Integer(Integer::from(1)),
            value::Value::Integer(Integer::from(2)),
            value::Value::Integer(Integer::from(3)),
        ]));
    }

    #[test]
    fn test_function_def() {
        let val = eval_str("f[x_] := x^2; f[3]");
        assert_eq!(val, value::Value::Integer(Integer::from(9)));
    }

    #[test]
    fn test_if() {
        assert_eq!(eval_str("If[True, 1, 2]"), value::Value::Integer(Integer::from(1)));
        assert_eq!(eval_str("If[False, 1, 2]"), value::Value::Integer(Integer::from(2)));
    }

    #[test]
    fn test_comparison() {
        assert_eq!(eval_str("1 == 1"), value::Value::Bool(true));
        assert_eq!(eval_str("1 != 2"), value::Value::Bool(true));
        assert_eq!(eval_str("1 < 2"), value::Value::Bool(true));
    }
}
