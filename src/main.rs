/// Syma: A Symbolic-First Language with OOP Structure
///
/// CLI entry point. The language library lives in `lib.rs`.
use std::io::{self, BufRead};
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
    fn validate(&self, _ctx: &mut ValidationContext<'_>) -> Result<ValidationResult, ReadlineError> {
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
fn truncate_output(s: &str) -> String {
    if s.len() <= MAX_OUTPUT_CHARS {
        s.to_string()
    } else {
        let omitted = s.len() - MAX_OUTPUT_CHARS;
        let truncated: String = s.chars().take(MAX_OUTPUT_CHARS).collect();
        format!(
            "{}\n{}",
            truncated,
            dim(&format!("... ({} bytes omitted)", omitted))
        )
    }
}

/// Run in kernel mode: read JSON requests from stdin, write JSON responses to stdout.
fn run_kernel() {
    let kernel = SymaKernel::new();
    let stdin = io::stdin().lock();
    for line in stdin.lines() {
        match line {
            Ok(input) => {
                let trimmed = input.trim().to_string();
                if trimmed.is_empty() {
                    continue;
                }
                let response = kernel.eval_json(&trimmed);
                println!("{response}");
            }
            Err(e) => {
                eprintln!("Error reading stdin: {}", e);
                break;
            }
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Check for --check flag (single file parse-only syntax check)
    let has_check = args.iter().any(|a| a == "--check");
    let has_dap = args.iter().any(|a| a == "--dap");
    let file_arg = args
        .iter()
        .skip(1)
        .find(|a| {
            !a.starts_with('-')
                && a.as_str() != "new"
                && a.as_str() != "run"
                && a.as_str() != "build"
                && a.as_str() != "check"
                && a.as_str() != "test"
                && a.as_str() != "add"
                && a.as_str() != "remove"
                && a.as_str() != "rm"
                && a.as_str() != "install"
                && a.as_str() != "update"
                && a.as_str() != "publish"
                && a.as_str() != "search"
                && a.as_str() != "info"
        })
        .map(|s| s.as_str());

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
        Some("--help") | Some("-h") => print_usage(),
        Some("--version") | Some("-v") => println!("syma {}", VERSION),

        // ── Kernel mode ──────────────────────────────────────────────────────
        Some("--kernel") => run_kernel(),

        // ── Expression evaluation ──────────────────────────────────────────────
        Some("--eval") | Some("-e") => {
            let expr = args.get(2).map(|s| s.as_str()).unwrap_or_else(|| {
                eprintln!("Usage: syma --eval <expression>");
                process::exit(1);
            });
            // Check for --format flag
            let format_flag = args
                .iter()
                .position(|a| a == "--format")
                .and_then(|i| args.get(i + 1).map(|s| s.as_str()));
            let env = syma::env::Env::new();
            syma::builtins::register_builtins(&env);
            match eval_input(expr, &env) {
                Some(val) => {
                    let display_val = match format_flag {
                        Some("fullform") | Some("full") => Value::Formatted {
                            format: Format::FullForm,
                            value: Box::new(val),
                        },
                        // Default to InputForm for readability
                        _ => Value::Formatted {
                            format: Format::InputForm,
                            value: Box::new(val),
                        },
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
