/// Syma: A Symbolic-First Language with OOP Structure
///
/// CLI entry point. The language library lives in `lib.rs`.
use std::io::{self, BufRead, Write};
use std::process;

use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::{ValidationContext, ValidationResult, Validator};
use rustyline::{Context, Editor, Helper};
use syma::format::{cyan, dim, green, red};
use syma::kernel::SymaKernel;
use syma::value::{Format, Value};
use syma::{VERSION, eval_input, run_file};

/// Symbol completer for the REPL tab completion.
struct SymaCompleter {
    symbols: Vec<String>,
}

impl Completer for SymaCompleter {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> Result<(usize, Vec<Pair>), ReadlineError> {
        // Find the start of the current word
        let start = line[..pos]
            .rfind(|c: char| c.is_whitespace() || c == '(' || c == '[' || c == '{' || c == ',')
            .map(|i| i + 1)
            .unwrap_or(0);
        let prefix = &line[start..pos];

        if prefix.is_empty() {
            return Ok((start, vec![]));
        }

        let candidates: Vec<Pair> = self
            .symbols
            .iter()
            .filter(|s| s.starts_with(prefix))
            .take(100)
            .map(|s| Pair {
                display: s.clone(),
                replacement: s.clone(),
            })
            .collect();

        Ok((start, candidates))
    }
}

/// REPL helper providing tab completion via SymaCompleter.
struct SymaHelper {
    completer: SymaCompleter,
}

impl Helper for SymaHelper {}
impl Highlighter for SymaHelper {}
impl Hinter for SymaHelper {
    type Hint = String;
}
impl Validator for SymaHelper {
    fn validate(
        &self,
        _ctx: &mut ValidationContext<'_>,
    ) -> Result<ValidationResult, ReadlineError> {
        Ok(ValidationResult::Valid(None))
    }
}
impl Completer for SymaHelper {
    type Candidate = Pair;
    fn complete(
        &self,
        line: &str,
        pos: usize,
        ctx: &Context<'_>,
    ) -> Result<(usize, Vec<Pair>), ReadlineError> {
        self.completer.complete(line, pos, ctx)
    }
}

const HISTORY_FILE: &str = ".syma_history";
const MAX_OUTPUT_CHARS: usize = 2000;

fn print_usage() {
    println!(
        "Syma v{} — Symbolic-First Language with OOP Structure",
        VERSION
    );
    println!();
    println!("Usage:");
    println!("  syma                       Start the interactive REPL");
    println!("  syma <file>                Evaluate a Syma source file");
    println!("  syma -e <expr>             Evaluate an expression and print the result");
    println!(
        "  syma -e <expr> --format F  Evaluate and format output (inputform|fullform, default inputform)"
    );
    println!("  syma --dap <file>          Run a file in debug mode (DAP protocol)");
    println!("  syma --check <file>        Parse-only check (no evaluation)");
    println!("  syma --kernel              Run in kernel mode (JSON over stdin/stdout)");
    println!("  syma --help                Show this help");
    println!("  syma --version             Show version");
    println!();
    println!("Package commands:");
    println!("  syma new <name>            Create a binary package");
    println!("  syma new --lib <name>      Create a library package");
    println!("  syma run                   Run the package entry point");
    println!("  syma build                 Check syntax of all src/ files");
    println!("  syma check                 Same as build (no codegen yet)");
    println!("  syma test                  Run all files in tests/");
    println!();
    println!("Dependency commands:");
    println!("  syma add <pkg>[@ver]       Add a dependency to syma.toml");
    println!("  syma add <pkg> --dev       Add a dev-dependency");
    println!("  syma remove <pkg>          Remove a dependency");
    println!("  syma install               Install declared dependencies");
    println!("  syma update                Update dependencies (planned)");
    println!();
    println!("Registry commands (planned):");
    println!("  syma publish               Publish to packages.syma-lang.org");
    println!("  syma search <query>        Search the registry");
    println!("  syma info <pkg>            Show package metadata");
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

fn run_repl() {
    println!(
        "{} — Symbolic-First Language with OOP Structure",
        green(&format!("Syma v{}", VERSION))
    );
    println!(
        "Type {} for commands, {} to exit.\n",
        cyan("'help'"),
        cyan("'quit'")
    );

    let env = syma::env::Env::new();
    syma::builtins::register_builtins(&env);

    // Collect all symbols for tab completion
    let mut symbols: Vec<String> = env
        .all_bindings()
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    symbols.extend(["help", "quit", "exit"].map(String::from));
    symbols.sort();
    symbols.dedup();

    let helper = SymaHelper {
        completer: SymaCompleter { symbols },
    };

    let mut rl: Editor<SymaHelper, rustyline::history::FileHistory> = match Editor::new() {
        Ok(rl) => rl,
        Err(e) => {
            eprintln!("Failed to initialize REPL: {}", e);
            process::exit(1);
        }
    };
    rl.set_helper(Some(helper));

    // Load history from file
    let history_path = dirs_or_default().map(|d| d.join(HISTORY_FILE));
    if let Some(ref path) = history_path {
        let _ = rl.load_history(path);
    }

    let mut counter: usize = 1;

    loop {
        let prompt = format!("\x1b[32mIn [{}]: \x1b[0m", counter);
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

                if let Some(results) = syma::eval_input_with_results(input, &env) {
                    for opt_val in &results {
                        match opt_val {
                            Some(val) if val != &Value::Null => {
                                if input.starts_with('?') {
                                    let s = val.to_string();
                                    if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
                                        println!("{}", &s[1..s.len() - 1]);
                                    } else {
                                        println!("{}", s);
                                    }
                                } else {
                                    let display_val = Value::Formatted {
                                        format: Format::InputForm,
                                        value: Box::new(val.clone()),
                                    };
                                    let output = display_val.to_string();
                                    println!(
                                        "{} {}",
                                        red(&format!("Out[{}]:", counter)),
                                        truncate_output(&output)
                                    );
                                }
                                counter += 1;
                            }
                            _ => {
                                counter += 1;
                            }
                        }
                    }
                }
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

/// Truncate output that exceeds MAX_OUTPUT_CHARS.
/// Returns the original string if under the limit, or a truncated version
/// with a message like `... (N bytes omitted)` appended.
///
/// The cut is made on a UTF-8 character boundary so multi-byte output is not
/// split in the middle of a character.
fn truncate_output(s: &str) -> String {
    if s.len() <= MAX_OUTPUT_CHARS {
        return s.to_string();
    }
    let mut end = MAX_OUTPUT_CHARS;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    let omitted = s.len() - end;
    format!(
        "{}\n{}",
        &s[..end],
        dim(&format!("... ({} bytes omitted)", omitted))
    )
}

/// Run in kernel mode: read JSON requests from stdin, write JSON responses to stdout.
///
/// The protocol is line-framed: each non-empty input line is one JSON request
/// and each response is exactly one line of JSON. Responses are flushed after
/// every request so frontends see them promptly even through a pipe.
fn run_kernel() {
    let kernel = SymaKernel::new();
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();
    for line in stdin.lock().lines() {
        match line {
            Ok(input) => {
                let trimmed = input.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let response = kernel.eval_json(trimmed);
                if writeln!(out, "{response}").is_err() {
                    break;
                }
                let _ = out.flush();
            }
            Err(e) => {
                eprintln!("Error reading stdin: {}", e);
                break;
            }
        }
    }
}

/// Return the first positional argument (one that does not start with `-`)
/// after the program name. Used by `--check`/`--dap` to locate their file
/// operand without confusing it with a subcommand name (e.g. `syma --dap run`).
fn first_positional_arg(args: &[String]) -> Option<&str> {
    args.iter()
        .skip(1)
        .map(|s| s.as_str())
        .find(|a| !a.starts_with('-'))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Check for --check flag (single file parse-only syntax check)
    let has_check = args.iter().any(|a| a == "--check");
    let has_dap = args.iter().any(|a| a == "--dap");
    let file_arg = first_positional_arg(&args);

    if has_dap {
        if let Some(path) = file_arg {
            syma::debug::run_debug(path);
        } else {
            eprintln!("Usage: syma --dap <file>");
            process::exit(1);
        }
        return;
    }

    if has_check {
        if let Some(path) = file_arg {
            syma::cli::check_single_file(path);
        } else {
            eprintln!("Usage: syma --check <file>");
            process::exit(1);
        }
        return;
    }

    match args.get(1).map(|s| s.as_str()) {
        // ── Meta ──────────────────────────────────────────────────────────────
        Some("--help") | Some("-h") | Some("help") => print_usage(),
        Some("--version") | Some("-v") => println!("syma {}", VERSION),

        // ── Kernel mode ──────────────────────────────────────────────────────
        Some("--kernel") => run_kernel(),

        // ── Expression evaluation ──────────────────────────────────────────────
        Some("--eval") | Some("-e") => {
            let expr = args.get(2).map(|s| s.as_str()).unwrap_or_else(|| {
                eprintln!("Usage: syma --eval <expression>");
                process::exit(1);
            });
            // Parse an optional `--format F` flag.
            let format_flag = match args.iter().position(|a| a == "--format") {
                Some(i) => match args.get(i + 1) {
                    Some(v) => Some(v.as_str()),
                    None => {
                        eprintln!("error: --format requires a value (inputform|fullform)");
                        process::exit(1);
                    }
                },
                None => None,
            };
            let display_format = match format_flag.map(|s| s.to_ascii_lowercase()).as_deref() {
                // Default to InputForm for readability.
                None | Some("inputform") | Some("input") => Format::InputForm,
                Some("fullform") | Some("full") => Format::FullForm,
                Some("standardform") | Some("standard") => Format::StandardForm,
                Some("outputform") | Some("output") => Format::OutputForm,
                Some(other) => {
                    eprintln!(
                        "error: unknown --format value '{other}' \
                         (expected inputform|fullform|standardform|outputform)"
                    );
                    process::exit(1);
                }
            };
            let env = syma::env::Env::new();
            syma::builtins::register_builtins(&env);
            match eval_input(expr, &env) {
                Some(val) => {
                    let display_val = Value::Formatted {
                        format: display_format,
                        value: Box::new(val),
                    };
                    println!("{}", truncate_output(&display_val.to_string()));
                }
                None => process::exit(1),
            }
        }

        // ── Package scaffolding ───────────────────────────────────────────────
        Some("new") => {
            let is_lib = args.contains(&"--lib".to_string());
            let name = args
                .iter()
                .skip(2)
                .find(|a| a.as_str() != "--lib")
                .map(|s| s.as_str())
                .unwrap_or_else(|| {
                    eprintln!("Usage: syma new [--lib] <name>");
                    process::exit(1);
                });
            syma::cli::cmd_new(name, is_lib);
        }

        // ── Source execution ─────────────────────────────────────────────────
        Some("run") => syma::cli::cmd_run(),
        Some("build") => syma::cli::cmd_build(),
        Some("check") => syma::cli::cmd_check(),
        Some("test") => syma::cli::cmd_test(),

        // ── Dependency management ─────────────────────────────────────────────
        Some("add") => {
            let spec = args.get(2).map(|s| s.as_str()).unwrap_or_else(|| {
                eprintln!("Usage: syma add <package>[@version] [--dev]");
                process::exit(1);
            });
            let dev = args.contains(&"--dev".to_string());
            syma::cli::cmd_add(spec, dev);
        }
        Some("remove") | Some("rm") => {
            let name = args.get(2).map(|s| s.as_str()).unwrap_or_else(|| {
                eprintln!("Usage: syma remove <package>");
                process::exit(1);
            });
            syma::cli::cmd_remove(name);
        }
        Some("install") => syma::cli::cmd_install(),
        Some("update") => syma::cli::cmd_update(),

        // ── Registry (planned) ────────────────────────────────────────────────
        Some("publish") => syma::cli::cmd_publish(),
        Some("search") => {
            let query = args.get(2).map(|s| s.as_str()).unwrap_or("");
            syma::cli::cmd_search(query);
        }
        Some("info") => {
            let pkg = args.get(2).map(|s| s.as_str()).unwrap_or_else(|| {
                eprintln!("Usage: syma info <package>");
                process::exit(1);
            });
            syma::cli::cmd_info(pkg);
        }

        // ── Direct file execution and REPL ────────────────────────────────────
        Some(path) if !path.starts_with('-') => {
            if let Err(e) = run_file(path) {
                eprintln!("{}: {}", red("Error"), e);
                process::exit(1);
            }
        }
        Some(flag) => {
            eprintln!("Unknown option: {}. Try `syma --help`.", flag);
            process::exit(1);
        }
        None => run_repl(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn test_first_positional_arg() {
        // Subcommand-like file names must still be found after --check/--dap.
        assert_eq!(
            first_positional_arg(&args(&["syma", "--check", "run"])),
            Some("run")
        );
        assert_eq!(
            first_positional_arg(&args(&["syma", "--dap", "check"])),
            Some("check")
        );
        assert_eq!(
            first_positional_arg(&args(&["syma", "--dap", "file.syma"])),
            Some("file.syma")
        );
        assert_eq!(
            first_positional_arg(&args(&["syma", "-e", "1+1"])),
            Some("1+1")
        );
        assert_eq!(first_positional_arg(&args(&["syma", "--kernel"])), None);
        assert_eq!(first_positional_arg(&args(&["syma"])), None);
        assert_eq!(first_positional_arg(&args(&["syma", "--check"])), None);
    }

    #[test]
    fn test_truncate_output_short() {
        assert_eq!(truncate_output("hello"), "hello");
    }

    #[test]
    fn test_truncate_output_long_ascii() {
        let s = "x".repeat(MAX_OUTPUT_CHARS + 10);
        let out = truncate_output(&s);
        assert!(out.contains("10 bytes omitted"), "got: {out}");
        assert!(out.contains("..."));
    }

    #[test]
    fn test_truncate_output_respects_char_boundary() {
        // Two-byte 'é' characters; ensure the cut lands on a boundary.
        let s = "é".repeat(MAX_OUTPUT_CHARS + 5);
        let out = truncate_output(&s);
        let head = out.split('\n').next().unwrap();
        assert!(
            head.chars().all(|c| c == 'é'),
            "split mid-character: {head}"
        );
        assert!(out.contains("bytes omitted"));
    }
}
