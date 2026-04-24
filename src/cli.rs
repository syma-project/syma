/// Package management subcommands — `syma new`, `syma run`, `syma test`, etc.

use std::fs;
use std::path::{Path, PathBuf};

use crate::manifest::{self, Manifest};

// ── ANSI helpers (local copies — keeps cli.rs self-contained) ─────────────────
fn green(s: &str) -> String   { format!("\x1b[32m{}\x1b[0m", s) }
fn red(s: &str) -> String     { format!("\x1b[31m{}\x1b[0m", s) }
fn cyan(s: &str) -> String    { format!("\x1b[36m{}\x1b[0m", s) }
fn bold(s: &str) -> String    { format!("\x1b[1m{}\x1b[0m", s) }
fn dim(s: &str) -> String     { format!("\x1b[2m{}\x1b[0m", s) }

// ── syma new ──────────────────────────────────────────────────────────────────

/// Create a new package directory with a `syma.toml` and a starter source file.
pub fn cmd_new(name: &str, is_lib: bool) {
    let root = Path::new(name);
    if root.exists() {
        eprintln!("{}: directory '{}' already exists", red("error"), name);
        std::process::exit(1);
    }

    let src_dir = root.join("src");
    fs::create_dir_all(&src_dir).unwrap_or_else(|e| {
        eprintln!("{}: {}", red("error"), e);
        std::process::exit(1);
    });
    fs::create_dir_all(root.join("tests")).unwrap();
    fs::create_dir_all(root.join("examples")).unwrap();

    // syma.toml
    let entry_rel = if is_lib { "src/lib.syma" } else { "src/main.syma" };
    let toml = format!(
        "[package]\n\
         name        = \"{name}\"\n\
         version     = \"0.1.0\"\n\
         description = \"\"\n\
         {entry_line}\n\
         \n\
         [dependencies]\n",
        name       = name,
        entry_line = if is_lib { String::new() } else { format!("entry       = \"{}\"", entry_rel) },
    );
    fs::write(root.join("syma.toml"), toml).unwrap();

    // src/main.syma  or  src/lib.syma
    let (entry_file, entry_src) = if is_lib {
        let mod_name = to_pascal_case(name);
        (
            "lib.syma",
            format!(
                "(* {name} — library entry point *)\n\
                 \n\
                 module {mod_name} {{\n\
                 \x20   export greet\n\
                 \n\
                 \x20   greet[name_] := \"Hello, \" <> name <> \"!\"\n\
                 }}\n",
                name = name,
                mod_name = mod_name,
            ),
        )
    } else {
        (
            "main.syma",
            format!("(* {name} — main entry point *)\n\nPrint[\"Hello from {name}!\"]\n", name = name),
        )
    };
    fs::write(src_dir.join(entry_file), entry_src).unwrap();

    println!("{} {} `{}`", green("Created"), if is_lib { "library" } else { "binary" }, bold(name));
    println!("   {}", dim(&format!("{}/syma.toml", name)));
    println!("   {}", dim(&format!("{}/src/{}", name, entry_file)));
    println!("   {}", dim(&format!("{}/tests/", name)));
    println!("\nRun {} to get started.", cyan(&format!("cd {} && syma run", name)));
}

// ── syma run ──────────────────────────────────────────────────────────────────

/// Run the package's entry point (`entry` in `[package]`, default `src/main.syma`).
pub fn cmd_run() {
    let manifest_path = require_manifest();
    let manifest = read_manifest(&manifest_path);
    let entry = manifest.entry_path();
    if !entry.exists() {
        eprintln!(
            "{}: entry file '{}' not found\n  \
             hint: set `entry` in [package] or create {}",
            red("error"),
            entry.display(),
            entry.display(),
        );
        std::process::exit(1);
    }
    crate::run_file(entry.to_str().unwrap());
}

// ── syma test ─────────────────────────────────────────────────────────────────

/// Run all `*.syma` files under `tests/` and report results.
pub fn cmd_test() {
    let manifest_path = require_manifest();
    let manifest = read_manifest(&manifest_path);
    let tests_dir = manifest.root().join("tests");

    if !tests_dir.exists() {
        println!("{}", dim("no tests/ directory — nothing to run"));
        return;
    }

    let files = collect_syma_files(&tests_dir);
    if files.is_empty() {
        println!("{}", dim("no .syma test files found in tests/"));
        return;
    }

    let mut passed = 0usize;
    let mut failed = 0usize;

    for file in &files {
        let rel = file.strip_prefix(manifest.root()).unwrap_or(file.as_path());
        print!("test {} ... ", cyan(&rel.display().to_string()));
        match run_file_silent(file) {
            Ok(()) => { println!("{}", green("ok")); passed += 1; }
            Err(e) => {
                println!("{}", red("FAILED"));
                for line in e.lines() {
                    eprintln!("    {}", dim(line));
                }
                failed += 1;
            }
        }
    }

    let status = if failed == 0 { green("ok") } else { red("FAILED") };
    println!("\ntest result: {}. {} passed; {} failed.", status, passed, failed);
    if failed > 0 {
        std::process::exit(1);
    }
}

// ── syma check / syma build ───────────────────────────────────────────────────

/// Parse and type-check all `*.syma` files under `src/` without evaluating.
pub fn cmd_check() {
    check_or_build(false);
}

/// Same as `check` at Phase 1 (tree-walk interpreter, no code generation yet).
pub fn cmd_build() {
    check_or_build(true);
}

fn check_or_build(building: bool) {
    let manifest_path = require_manifest();
    let manifest = read_manifest(&manifest_path);
    let src_dir = manifest.root().join("src");

    if !src_dir.exists() {
        eprintln!("{}: src/ directory not found", red("error"));
        std::process::exit(1);
    }

    let verb = if building { "Building" } else { "Checking" };
    println!("   {} {} v{}", dim(verb), bold(&manifest.name), manifest.version);

    let files = collect_syma_files(&src_dir);
    let mut errors = 0usize;

    for file in &files {
        let source = match fs::read_to_string(file) {
            Ok(s) => s,
            Err(e) => {
                let rel = file.strip_prefix(manifest.root()).unwrap_or(file.as_path());
                eprintln!("{} [{}]: {}", red("error"), rel.display(), e);
                errors += 1;
                continue;
            }
        };
        let rel = file.strip_prefix(manifest.root()).unwrap_or(file.as_path());
        match crate::lexer::tokenize(&source) {
            Err(e) => {
                eprintln!("{} [{}]: {}", red("lex error"), rel.display(), e);
                errors += 1;
            }
            Ok(tokens) => {
                if let Err(e) = crate::parser::parse(tokens) {
                    eprintln!("{} [{}]: {}", red("parse error"), rel.display(), e);
                    errors += 1;
                }
            }
        }
    }

    if errors == 0 {
        println!("    {} {} v{}", green("Finished"), manifest.name, manifest.version);
    } else {
        eprintln!("\n{}: could not compile `{}` ({} error(s))",
            red("error"), manifest.name, errors);
        std::process::exit(1);
    }
}

// ── syma add ──────────────────────────────────────────────────────────────────

/// Add (or update) a dependency in `syma.toml`.
///
/// `spec` is `<name>` or `<name>@<version>`.
/// Pass `dev = true` to add under `[dev-dependencies]`.
pub fn cmd_add(spec: &str, dev: bool) {
    let manifest_path = require_manifest();
    let (name, version) = parse_dep_spec(spec);

    manifest::add_dep(&manifest_path, name, version, dev).unwrap_or_else(|e| {
        eprintln!("{}: {}", red("error"), e);
        std::process::exit(1);
    });

    let section = if dev { "dev-dependencies" } else { "dependencies" };
    println!("{} `{}` {} to `{}`",
        green("Added"), bold(name), dim(&format!("({})", version)), section);
    println!("  {}", dim("run `syma install` to download the package"));
}

// ── syma remove ───────────────────────────────────────────────────────────────

/// Remove a dependency from `syma.toml` (searches both sections).
pub fn cmd_remove(name: &str) {
    let manifest_path = require_manifest();

    // Try runtime deps first, then dev-deps.
    let removed = manifest::remove_dep(&manifest_path, name, false)
        .unwrap_or_else(|e| { eprintln!("{}: {}", red("error"), e); std::process::exit(1); });

    if removed {
        println!("{} `{}` from dependencies", green("Removed"), bold(name));
        return;
    }

    let removed_dev = manifest::remove_dep(&manifest_path, name, true)
        .unwrap_or_else(|e| { eprintln!("{}: {}", red("error"), e); std::process::exit(1); });

    if removed_dev {
        println!("{} `{}` from dev-dependencies", green("Removed"), bold(name));
    } else {
        eprintln!("{}: `{}` is not listed as a dependency", red("error"), bold(name));
        std::process::exit(1);
    }
}

// ── syma install ──────────────────────────────────────────────────────────────

/// Resolve and install all dependencies declared in `syma.toml`.
///
/// Local `path = "..."` dependencies are resolved at import time.
/// Registry downloads are not yet implemented.
pub fn cmd_install() {
    let manifest_path = require_manifest();
    let manifest = read_manifest(&manifest_path);

    let all_empty = manifest.dependencies.is_empty() && manifest.dev_dependencies.is_empty();
    if all_empty {
        println!("{}", dim("no dependencies declared — nothing to install"));
        return;
    }

    println!("   {} dependencies for `{}` v{}",
        dim("Resolving"), manifest.name, manifest.version);

    for (name, ver) in &manifest.dependencies {
        println!("     {} {} {}", dim("•"), bold(name), dim(ver));
    }
    for (name, ver) in &manifest.dev_dependencies {
        println!("     {} {} {} {}", dim("•"), bold(name), dim(ver), dim("(dev)"));
    }

    println!();
    println!("  {} the package registry is not yet operational.", cyan("note:"));
    println!("  Local {} dependencies are resolved at import time.",
        cyan("path = \"...\""));
    println!("  Registry-hosted packages will be supported in a future release.");
}

// ── syma update ───────────────────────────────────────────────────────────────

pub fn cmd_update() {
    require_manifest();
    println!("  {} dependency updating requires the package registry,", cyan("note:"));
    println!("  which is planned for a future release.");
    println!("  To update a local-path dependency, edit its source directly.");
}

// ── syma publish ─────────────────────────────────────────────────────────────

pub fn cmd_publish() {
    let manifest_path = require_manifest();
    let manifest = read_manifest(&manifest_path);
    println!("  {} publishing `{}` v{} requires the registry at",
        cyan("note:"), bold(&manifest.name), manifest.version);
    println!("  {}, which is planned for a future release.",
        cyan("packages.syma-lang.org"));
}

// ── syma search ───────────────────────────────────────────────────────────────

pub fn cmd_search(query: &str) {
    if query.is_empty() {
        eprintln!("Usage: syma search <query>");
        std::process::exit(1);
    }
    println!("  {} searching for '{}' ...", cyan("note:"), query);
    println!("  The package registry is not yet operational — no results available.");
}

// ── syma info ────────────────────────────────────────────────────────────────

pub fn cmd_info(name: &str) {
    println!("  {} fetching info for '{}' ...", cyan("note:"), name);
    println!("  The package registry is not yet operational.");
}

// ── Internal helpers ─────────────────────────────────────────────────────────

/// Exit with a helpful error if no `syma.toml` is found.
fn require_manifest() -> PathBuf {
    Manifest::find().unwrap_or_else(|| {
        eprintln!(
            "{}: no `syma.toml` found in the current directory or any parent\n  \
             hint: run `syma new <name>` to create a new package",
            red("error")
        );
        std::process::exit(1);
    })
}

fn read_manifest(path: &Path) -> Manifest {
    Manifest::read(path).unwrap_or_else(|e| {
        eprintln!("{}: {}", red("error"), e);
        std::process::exit(1);
    })
}

/// Collect all `*.syma` files under `dir` (sorted, recursive).
fn collect_syma_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(|e| e.path());
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                files.extend(collect_syma_files(&path));
            } else if path.extension().map_or(false, |e| e == "syma") {
                files.push(path);
            }
        }
    }
    files
}

/// Evaluate a `.syma` file silently; return `Err(message)` on any error.
fn run_file_silent(path: &Path) -> Result<(), String> {
    let source = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let tokens = crate::lexer::tokenize(&source).map_err(|e| e.to_string())?;
    let stmts  = crate::parser::parse_with_suppress(tokens).map_err(|e| e.to_string())?;
    let env    = crate::env::Env::new();
    crate::builtins::register_builtins(&env);
    if let Some(parent) = path.parent() {
        env.add_search_path(parent.to_path_buf());
    }
    for (stmt, _suppress) in &stmts {
        crate::eval::eval(stmt, &env).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Parse `<name>` or `<name>@<version>` into `(name, version)`.
fn parse_dep_spec(spec: &str) -> (&str, &str) {
    if let Some(pos) = spec.find('@') {
        (&spec[..pos], &spec[pos + 1..])
    } else {
        (spec, "*")
    }
}

/// Convert `"my-package"` → `"MyPackage"`.
fn to_pascal_case(s: &str) -> String {
    s.split(['-', '_', ' '])
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None    => String::new(),
                Some(f) => f.to_uppercase().to_string() + chars.as_str(),
            }
        })
        .collect()
}
