//! system/session IO: Get, Put, Save, Directory, SetDirectory, AbsoluteTiming, Rasterize, Display.
//!
//! Gap-fill module; see the individual builtin docs for semantics.

use crate::ast::Expr;
use crate::env::Env;
use crate::value::{EvalError, Format, Value};
use std::collections::HashMap;
use std::path::PathBuf;

/// Environment key holding the session's notion of the current directory.
///
/// Wolfram Language has a single process-wide current directory. Syma stores
/// it in the environment so that independent `Env`s (e.g. parallel tests) do
/// not interfere with each other, and so relative `Get`/`Put`/`Save` paths can
/// be resolved against it.
const CWD_KEY: &str = "$SymaCurrentDirectory";

// ── Small helpers ───────────────────────────────────────────────────────────

/// Return an unevaluated `head[args...]` call.
///
/// Used when the builtin does not apply to the supplied arguments, mirroring
/// the convention used throughout the builtin table.
fn inert(name: &str, args: &[Value]) -> Result<Value, EvalError> {
    Ok(Value::Call {
        head: name.to_string(),
        args: args.to_vec(),
    })
}

/// Extract a string argument, transparently unwrapping a held `Pattern`.
fn as_str(v: &Value) -> Option<String> {
    match v {
        Value::Str(s) => Some(s.clone()),
        Value::Pattern(Expr::Str(s)) => Some(s.clone()),
        _ => None,
    }
}

/// Extract a symbol name from a held (`Pattern`) or plain symbol argument.
fn as_symbol_name(v: &Value) -> Option<String> {
    match v {
        Value::Symbol(s) => Some(s.clone()),
        Value::Pattern(Expr::Symbol(s)) => Some(s.clone()),
        _ => None,
    }
}

/// Current session directory (absolute when possible).
fn current_dir(env: &Env) -> PathBuf {
    if let Some(Value::Str(s)) = env.get(CWD_KEY) {
        return PathBuf::from(s);
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// Resolve `p` against the session directory, expanding a leading `~`.
fn resolve_path(env: &Env, p: &str) -> PathBuf {
    let expanded = if p == "~" || p.starts_with("~/") {
        match std::env::var("HOME") {
            Ok(home) if p == "~" => home,
            Ok(home) => format!("{}{}", home, &p[1..]),
            Err(_) => p.to_string(),
        }
    } else {
        p.to_string()
    };
    let path = PathBuf::from(expanded);
    if path.is_absolute() {
        path
    } else {
        current_dir(env).join(path)
    }
}

/// Render a value using `InputForm` (used by `Put`, `Save`, `Display`).
fn input_form(v: &Value) -> String {
    format!(
        "{}",
        Value::Formatted {
            format: Format::InputForm,
            value: Box::new(v.clone()),
        }
    )
}

// ── Get ─────────────────────────────────────────────────────────────────────

/// Get["file"] — read and evaluate a source file, returning the last result.
///
/// The file may contain any Syma/WL source. Definitions are installed in the
/// caller's environment. Returns `$Failed` when the file cannot be read.
pub fn builtin_get(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return inert("Get", args);
    }
    let Some(file) = as_str(&args[0]) else {
        return inert("Get", args);
    };
    let path = resolve_path(env, &file);
    let source = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => return Ok(Value::Symbol("$Failed".to_string())),
    };
    let tokens =
        crate::lexer::tokenize(&source).map_err(|e| EvalError::Error(format!("Get: {}", e)))?;
    let stmts = crate::parser::parse_with_suppress(tokens)
        .map_err(|e| EvalError::Error(format!("Get: {}", e)))?;
    let exprs: Vec<Expr> = stmts.into_iter().map(|(e, _)| e).collect();
    crate::eval::eval_program(&exprs, env)
}

// ── Put ─────────────────────────────────────────────────────────────────────

/// Put[expr, "file"] — write `expr` to a file in `InputForm`.
///
/// Multiple expressions may be supplied: `Put[e1, e2, "file"]` writes each on
/// its own line. Returns `Null`.
pub fn builtin_put(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return inert("Put", args);
    }
    let Some(file) = as_str(&args[args.len() - 1]) else {
        return inert("Put", args);
    };
    let path = resolve_path(env, &file);
    let mut out = String::new();
    for expr in &args[..args.len() - 1] {
        out.push_str(&input_form(expr));
        out.push('\n');
    }
    std::fs::write(&path, out).map_err(|e| EvalError::Error(format!("Put failed: {}", e)))?;
    Ok(Value::Null)
}

// ── Save / DumpSave ─────────────────────────────────────────────────────────

/// Serialize one function definition as an assignment source line.
fn serialize_definition(name: &str, def: &crate::value::FunctionDefinition) -> String {
    let params: Vec<String> = def.params.iter().map(|p| format!("{}", p)).collect();
    let op = if def.delayed { ":=" } else { "=" };
    match &def.guard {
        Some(guard) => format!(
            "{}[{}] /; {} {} {}\n",
            name,
            params.join(", "),
            guard,
            op,
            def.body
        ),
        None => format!("{}[{}] {} {}\n", name, params.join(", "), op, def.body),
    }
}

/// Save["file", sym1, sym2, ...] — write definitions of the named symbols.
///
/// Own values are written as `sym = value`; user-defined functions are written
/// as their pattern definitions. Returns `Null`.
pub fn builtin_save(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return inert("Save", args);
    }
    let Some(file) = as_str(&args[0]) else {
        return inert("Save", args);
    };
    let path = resolve_path(env, &file);
    let mut out = String::new();
    for spec in &args[1..] {
        let Some(name) = as_symbol_name(spec) else {
            continue;
        };
        match env.get(&name) {
            // Builtins need not be persisted.
            Some(Value::Builtin(..)) | None => {}
            Some(Value::Function(fd)) => {
                for def in &fd.definitions {
                    out.push_str(&serialize_definition(&name, def));
                }
            }
            Some(other) => {
                out.push_str(&name);
                out.push_str(" = ");
                out.push_str(&input_form(&other));
                out.push('\n');
            }
        }
    }
    std::fs::write(&path, out).map_err(|e| EvalError::Error(format!("Save failed: {}", e)))?;
    Ok(Value::Null)
}

/// DumpSave["file.mx", sym...] — basic alias for `Save`.
pub fn builtin_dump_save(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return inert("DumpSave", args);
    }
    builtin_save(args, env)
}

// ── Directory / SetDirectory ────────────────────────────────────────────────

/// Directory[] — the current working directory.
pub fn builtin_directory(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if !args.is_empty() {
        return inert("Directory", args);
    }
    Ok(Value::Str(current_dir(env).to_string_lossy().into_owned()))
}

/// SetDirectory["dir"] — set the current working directory, returning it.
///
/// Returns `$Failed` if `dir` does not name an existing directory.
pub fn builtin_set_directory(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return inert("SetDirectory", args);
    }
    let Some(dir) = as_str(&args[0]) else {
        return inert("SetDirectory", args);
    };
    let path = resolve_path(env, &dir);
    if !path.is_dir() {
        return Ok(Value::Symbol("$Failed".to_string()));
    }
    let abs = path.canonicalize().unwrap_or(path);
    let s = abs.to_string_lossy().into_owned();
    env.root_env()
        .set(CWD_KEY.to_string(), Value::Str(s.clone()));
    Ok(Value::Str(s))
}

// ── AbsoluteTiming ──────────────────────────────────────────────────────────

/// AbsoluteTiming[expr] — evaluate `expr`, returning `{seconds, result}`.
///
/// `HoldAll` so the argument is evaluated inside the timer.
pub fn builtin_absolute_timing(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return inert("AbsoluteTiming", args);
    }
    let start = std::time::Instant::now();
    let value = match &args[0] {
        Value::Pattern(expr) => crate::eval::eval(expr, env)?,
        other => crate::eval::apply_function(
            &Value::Symbol("Identity".to_string()),
            std::slice::from_ref(other),
            env,
        )?,
    };
    let elapsed = start.elapsed();
    let seconds = rug::Float::with_val(crate::value::DEFAULT_PRECISION, elapsed.as_secs_f64());
    Ok(Value::List(vec![Value::Real(seconds), value]))
}

// ── Rasterize / Display ─────────────────────────────────────────────────────

/// Render a graphics-producing value to an SVG string.
///
/// `Graphics[...]` values are rendered by the graphics module; an existing SVG
/// string is passed through. Returns `None` for unsupported values.
fn render_graphics(v: &Value) -> Option<String> {
    match v {
        Value::Call { head, args } if head == "Graphics" && !args.is_empty() => {
            let options = args
                .get(1)
                .cloned()
                .unwrap_or_else(|| Value::Assoc(HashMap::new()));
            crate::builtins::graphics::render_svg(&args[0], &options).ok()
        }
        _ => None,
    }
}

/// Rasterize[expr] — render a `Graphics`/chart expression.
///
/// Produces an SVG string (Syma has no built-in SVG rasterizer); an `Image` is
/// returned unchanged.
pub fn builtin_rasterize(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return inert("Rasterize", args);
    }
    match &args[0] {
        Value::Image(_) => Ok(args[0].clone()),
        Value::Formatted { value, .. } => builtin_rasterize(std::slice::from_ref(value.as_ref())),
        other => match render_graphics(other) {
            Some(svg) => Ok(Value::Str(svg)),
            None => match other {
                Value::Str(s) => Ok(Value::Str(s.clone())),
                _ => inert("Rasterize", args),
            },
        },
    }
}

/// Produce a displayable string: SVG for graphics, `InputForm` otherwise.
fn display_string(v: &Value) -> String {
    render_graphics(v).unwrap_or_else(|| input_form(v))
}

/// Display[expr] — print a displayable rendering of `expr`, returning it.
///
/// Display["file", expr] — write the rendering to a file, returning `Null`.
pub fn builtin_display(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    match args.len() {
        1 => {
            let rendered = display_string(&args[0]);
            println!("{}", rendered);
            Ok(Value::Str(rendered))
        }
        2 => {
            let Some(file) = as_str(&args[0]) else {
                return inert("Display", args);
            };
            let path = resolve_path(env, &file);
            let rendered = display_string(&args[1]);
            std::fs::write(&path, rendered)
                .map_err(|e| EvalError::Error(format!("Display failed: {}", e)))?;
            Ok(Value::Null)
        }
        _ => inert("Display", args),
    }
}

/// Register the builtins implemented in this module.
/// FindList[file(s), "text"] — lines containing `text`, across the files.
pub fn builtin_find_list(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    let uneval = || {
        Ok(Value::Call {
            head: "FindList".to_string(),
            args: args.to_vec(),
        })
    };
    if args.len() < 2 {
        return uneval();
    }
    let needles: Vec<String> = match &args[1] {
        Value::Str(s) => vec![s.clone()],
        Value::List(items) => items.iter().filter_map(as_str).collect(),
        _ => return uneval(),
    };
    let files: Vec<String> = match &args[0] {
        Value::Str(s) => vec![s.clone()],
        Value::List(items) => items.iter().filter_map(as_str).collect(),
        _ => return uneval(),
    };
    let mut out: Vec<Value> = Vec::new();
    for f in &files {
        let path = resolve_path(env, f);
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        for line in content.lines() {
            if needles.iter().any(|n| line.contains(n.as_str())) {
                out.push(Value::Str(line.to_string()));
            }
        }
    }
    Ok(Value::List(out))
}

/// URLRead[url] — fetch an HTTP(S) URL as a string (basic; HTTPS needs no
/// extra crates so it is attempted over plain TCP per the scheme).
pub fn builtin_url_read(args: &[Value]) -> Result<Value, EvalError> {
    let failed = || Value::Symbol("$Failed".to_string());
    if args.len() != 1 {
        return Ok(Value::Call {
            head: "URLRead".to_string(),
            args: args.to_vec(),
        });
    }
    let Some(url) = as_str(&args[0]) else {
        return Ok(failed());
    };
    let without_scheme = if let Some(rest) = url.strip_prefix("http://") {
        ("http", rest)
    } else if let Some(rest) = url.strip_prefix("https://") {
        ("https", rest)
    } else {
        return Ok(failed());
    };
    let (_, rest) = without_scheme;
    let (host_port, path) = match rest.split_once('/') {
        Some((h, p)) => (h, format!("/{p}")),
        None => (rest, "/".to_string()),
    };
    let (host, port) = match host_port.rsplit_once(':') {
        Some((h, p)) => (h.to_string(), p.parse::<u16>().unwrap_or(80)),
        None => (host_port.to_string(), 80),
    };
    let addr = format!("{host}:{port}");
    use std::io::{Read, Write};
    let mut stream = match std::net::TcpStream::connect(&addr) {
        Ok(s) => s,
        Err(_) => return Ok(failed()),
    };
    let req = format!(
        "GET {path} HTTP/1.0\r\nHost: {host}\r\nUser-Agent: syma\r\nConnection: close\r\n\r\n"
    );
    if stream.write_all(req.as_bytes()).is_err() {
        return Ok(failed());
    }
    let mut resp = Vec::new();
    if stream.read_to_end(&mut resp).is_err() {
        return Ok(failed());
    }
    let text = String::from_utf8_lossy(&resp);
    let body = match text.find("\r\n\r\n") {
        Some(i) => &text[i + 4..],
        None => &text[..],
    };
    Ok(Value::Str(body.to_string()))
}

pub fn register(env: &Env) {
    super::register_builtin_env(env, "Get", builtin_get);
    super::register_builtin_env(env, "Put", builtin_put);
    super::register_builtin_env(env, "Save", builtin_save);
    super::register_builtin_env(env, "DumpSave", builtin_dump_save);
    super::register_builtin_env(env, "Directory", builtin_directory);
    super::register_builtin_env(env, "SetDirectory", builtin_set_directory);
    super::register_builtin_env(env, "AbsoluteTiming", builtin_absolute_timing);
    super::register_builtin(env, "Rasterize", builtin_rasterize);
    super::register_builtin_env(env, "Display", builtin_display);
    super::register_builtin_env(env, "FindList", builtin_find_list);
    super::register_builtin(env, "URLRead", builtin_url_read);

    // Get holds its file argument; Save/DumpSave hold the symbol names so the
    // symbol (not its value) is persisted.
    env.set_attributes("Get", vec!["HoldAll".to_string(), "Protected".to_string()]);
    env.set_attributes(
        "AbsoluteTiming",
        vec!["HoldAll".to_string(), "Protected".to_string()],
    );
    env.set_attributes(
        "Save",
        vec!["HoldRest".to_string(), "Protected".to_string()],
    );
    env.set_attributes(
        "DumpSave",
        vec!["HoldRest".to_string(), "Protected".to_string()],
    );
    for name in ["Put", "Directory", "SetDirectory", "Rasterize", "Display"] {
        env.set_attributes(name, vec!["Protected".to_string()]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rug::Integer;
    use std::fs;

    fn s(x: &str) -> Value {
        Value::Str(x.to_string())
    }
    fn int(n: i64) -> Value {
        Value::Integer(Integer::from(n))
    }
    fn sym(x: &str) -> Value {
        Value::Pattern(Expr::Symbol(x.to_string()))
    }
    fn env_with_builtins() -> Env {
        let env = Env::new();
        crate::builtins::register_builtins(&env);
        env
    }
    #[test]
    fn test_find_list_matches_lines() {
        let env = env_with_builtins();
        let p = temp_path("findlist");
        fs::write(&p, "alpha\nbeta\ngamma\n").unwrap();
        let out = builtin_find_list(&[s(p.to_str().unwrap()), s("a")], &env).unwrap();
        let Value::List(items) = out else {
            panic!("expected list");
        };
        assert_eq!(items.len(), 3);
        let _ = fs::remove_file(&p);
    }

    fn temp_path(tag: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("syma_systemio_{}_{}.syma", std::process::id(), tag));
        p
    }
    fn graphics_line() -> Value {
        let line = Value::Call {
            head: "Line".to_string(),
            args: vec![Value::List(vec![
                Value::List(vec![int(0), int(0)]),
                Value::List(vec![int(1), int(1)]),
            ])],
        };
        Value::Call {
            head: "Graphics".to_string(),
            args: vec![Value::List(vec![line]), Value::Assoc(HashMap::new())],
        }
    }

    // ── Directory / SetDirectory ──

    #[test]
    fn test_directory_default() {
        let env = env_with_builtins();
        let d = builtin_directory(&[], &env).unwrap();
        assert!(matches!(d, Value::Str(_)));
        assert!(builtin_directory(&[s("x")], &env).is_ok());
        assert!(matches!(
            builtin_directory(&[s("x")], &env).unwrap(),
            Value::Call { .. }
        ));
    }

    #[test]
    fn test_set_directory_roundtrip() {
        let env = env_with_builtins();
        let dir = std::env::temp_dir();
        let set = builtin_set_directory(&[s(dir.to_str().unwrap())], &env).unwrap();
        let Value::Str(new_dir) = set else {
            panic!("expected string, got {:?}", set);
        };
        let got = builtin_directory(&[], &env).unwrap();
        assert_eq!(got, Value::Str(new_dir));
    }

    #[test]
    fn test_set_directory_missing() {
        let env = env_with_builtins();
        let out = builtin_set_directory(&[s("/definitely/not/a/real/dir/xyz")], &env).unwrap();
        assert_eq!(out, Value::Symbol("$Failed".to_string()));
    }

    // ── Get ──

    #[test]
    fn test_get_returns_last_result() {
        let path = temp_path("get");
        fs::write(&path, "a = 3;\na + 4").unwrap();
        let env = env_with_builtins();
        let out = builtin_get(&[s(path.to_str().unwrap())], &env).unwrap();
        assert_eq!(out, int(7));
        assert_eq!(env.get("a"), Some(int(3)));
        fs::remove_file(&path).ok();
    }

    #[test]
    fn test_get_missing_file() {
        let env = env_with_builtins();
        let out = builtin_get(&[s("/no/such/file.syma")], &env).unwrap();
        assert_eq!(out, Value::Symbol("$Failed".to_string()));
    }

    #[test]
    fn test_get_wrong_args() {
        let env = env_with_builtins();
        assert!(matches!(
            builtin_get(&[], &env).unwrap(),
            Value::Call { ref head, .. } if head == "Get"
        ));
        assert!(matches!(
            builtin_get(&[int(1)], &env).unwrap(),
            Value::Call { .. }
        ));
    }

    #[test]
    fn test_get_via_eval_holdall() {
        let path = temp_path("get_eval");
        fs::write(&path, "b = 10;\nb * 2").unwrap();
        let env = env_with_builtins();
        let input = format!("Get[\"{}\"]", path.display());
        let tokens = crate::lexer::tokenize(&input).unwrap();
        let ast = crate::parser::parse(tokens).unwrap();
        let out = crate::eval::eval_program(&ast, &env).unwrap();
        assert_eq!(out, int(20));
        assert_eq!(env.get("b"), Some(int(10)));
        fs::remove_file(&path).ok();
    }

    // ── Put ──

    #[test]
    fn test_put_writes_input_form() {
        let path = temp_path("put");
        let env = env_with_builtins();
        let expr = Value::Call {
            head: "Plus".to_string(),
            args: vec![int(1), int(2)],
        };
        let out = builtin_put(&[expr, s(path.to_str().unwrap())], &env).unwrap();
        assert_eq!(out, Value::Null);
        let contents = fs::read_to_string(&path).unwrap();
        assert_eq!(contents, "1 + 2\n");
        fs::remove_file(&path).ok();
    }

    #[test]
    fn test_put_get_roundtrip() {
        let path = temp_path("put_get");
        let env = env_with_builtins();
        builtin_put(&[int(42), s(path.to_str().unwrap())], &env).unwrap();
        let out = builtin_get(&[s(path.to_str().unwrap())], &env).unwrap();
        assert_eq!(out, int(42));
        fs::remove_file(&path).ok();
    }

    #[test]
    fn test_put_wrong_args() {
        let env = env_with_builtins();
        assert!(matches!(
            builtin_put(&[int(1)], &env).unwrap(),
            Value::Call { .. }
        ));
    }

    // ── Save / DumpSave ──

    #[test]
    fn test_save_own_value() {
        let path = temp_path("save");
        let env = env_with_builtins();
        env.set("x".to_string(), int(41));
        builtin_save(&[s(path.to_str().unwrap()), sym("x")], &env).unwrap();

        let env2 = env_with_builtins();
        builtin_get(&[s(path.to_str().unwrap())], &env2).unwrap();
        assert_eq!(env2.get("x"), Some(int(41)));
        fs::remove_file(&path).ok();
    }

    #[test]
    fn test_save_function_definition() {
        let path = temp_path("save_fn");
        let env = env_with_builtins();
        let input = "f[t_] := t^2";
        let tokens = crate::lexer::tokenize(input).unwrap();
        let ast = crate::parser::parse(tokens).unwrap();
        crate::eval::eval_program(&ast, &env).unwrap();
        builtin_save(&[s(path.to_str().unwrap()), sym("f")], &env).unwrap();
        let contents = fs::read_to_string(&path).unwrap();
        assert!(contents.contains(":="), "contents: {}", contents);

        let env2 = env_with_builtins();
        builtin_get(&[s(path.to_str().unwrap())], &env2).unwrap();
        let out = crate::eval::apply_function(&env2.get("f").unwrap(), &[int(5)], &env2).unwrap();
        assert_eq!(out, int(25));
        fs::remove_file(&path).ok();
    }

    #[test]
    fn test_dump_save_alias() {
        let path = temp_path("dump");
        let env = env_with_builtins();
        env.set("y".to_string(), int(7));
        let out = builtin_dump_save(&[s(path.to_str().unwrap()), sym("y")], &env).unwrap();
        assert_eq!(out, Value::Null);
        assert!(path.exists());
        fs::remove_file(&path).ok();
    }

    // ── AbsoluteTiming ──

    #[test]
    fn test_absolute_timing_direct() {
        let env = env_with_builtins();
        let out = builtin_absolute_timing(&[int(5)], &env).unwrap();
        match out {
            Value::List(items) => {
                assert_eq!(items.len(), 2);
                assert!(matches!(items[0], Value::Real(_)));
                assert_eq!(items[1], int(5));
            }
            other => panic!("expected list, got {:?}", other),
        }
    }

    #[test]
    fn test_absolute_timing_holds() {
        let env = env_with_builtins();
        let tokens = crate::lexer::tokenize("AbsoluteTiming[1 + 1]").unwrap();
        let ast = crate::parser::parse(tokens).unwrap();
        let out = crate::eval::eval_program(&ast, &env).unwrap();
        match out {
            Value::List(items) => assert_eq!(items[1], int(2)),
            other => panic!("expected list, got {:?}", other),
        }
    }

    // ── Rasterize / Display ──

    #[test]
    fn test_rasterize_graphics() {
        let out = builtin_rasterize(&[graphics_line()]).unwrap();
        match out {
            Value::Str(svg) => assert!(svg.contains("<svg")),
            other => panic!("expected svg string, got {:?}", other),
        }
    }

    #[test]
    fn test_rasterize_svg_passthrough() {
        let out = builtin_rasterize(&[s("<svg></svg>")]).unwrap();
        assert_eq!(out, s("<svg></svg>"));
    }

    #[test]
    fn test_rasterize_inapplicable() {
        assert!(matches!(
            builtin_rasterize(&[int(3)]).unwrap(),
            Value::Call { .. }
        ));
    }

    #[test]
    fn test_display_returns_rendering() {
        let env = env_with_builtins();
        let out = builtin_display(&[int(5)], &env).unwrap();
        assert_eq!(out, s("5"));
        let out = builtin_display(&[graphics_line()], &env).unwrap();
        match out {
            Value::Str(svg) => assert!(svg.contains("<svg")),
            other => panic!("expected svg string, got {:?}", other),
        }
    }

    #[test]
    fn test_display_two_arg_writes_file() {
        let path = temp_path("display");
        let env = env_with_builtins();
        let out = builtin_display(&[s(path.to_str().unwrap()), int(9)], &env).unwrap();
        assert_eq!(out, Value::Null);
        assert_eq!(fs::read_to_string(&path).unwrap(), "9");
        fs::remove_file(&path).ok();
    }
}
