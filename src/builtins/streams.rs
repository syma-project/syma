//! streams and notebook: Streams, OpenWrite/OpenRead/Close, Notebook, CreateWindow, and << / >> infix operators.
//!
//! Gap-fill module; see the individual builtin docs for semantics.
//!
//! Streams are small integer handles handed back by `OpenWrite`/`OpenAppend`/
//! `OpenRead`. The live stream state (an open file or an in-memory read buffer)
//! lives in a process-wide registry keyed by that handle; the handle itself is
//! an ordinary value the caller may bind in the environment (e.g.
//! `s = OpenWrite["f"]`).

use crate::env::Env;
use crate::value::{EvalError, Format, Value};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::Write as IoWrite;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

/// Environment key holding the session's notion of the current directory.
/// Mirrors the key used by `systemio.rs` so relative paths resolve the same way.
const CWD_KEY: &str = "$SymaCurrentDirectory";

/// Live state for one open stream.
enum StreamEntry {
    /// A read stream buffering the whole file plus a byte cursor.
    Read { content: String, pos: usize },
    /// A write/append stream backed by an open file.
    Write(File),
}

/// Process-wide registry of open streams, keyed by handle.
static STREAMS: LazyLock<Mutex<HashMap<u64, StreamEntry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Monotonic handle allocator.
static NEXT_HANDLE: AtomicU64 = AtomicU64::new(1);

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Return an unevaluated `head[args...]` call.
fn inert(name: &str, args: &[Value]) -> Result<Value, EvalError> {
    Ok(Value::Call {
        head: name.to_string(),
        args: args.to_vec(),
    })
}

/// Extract an integer stream handle.
fn as_handle(v: &Value) -> Option<u64> {
    match v {
        Value::Integer(n) => n.to_u64(),
        _ => None,
    }
}

/// Extract a string argument, transparently unwrapping a held `Pattern`.
fn as_str(v: &Value) -> Option<String> {
    match v {
        Value::Str(s) => Some(s.clone()),
        Value::Pattern(crate::ast::Expr::Str(s)) => Some(s.clone()),
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

/// Render a value using `InputForm` (used by `Write`).
fn input_form(v: &Value) -> String {
    format!(
        "{}",
        Value::Formatted {
            format: Format::InputForm,
            value: Box::new(v.clone()),
        }
    )
}

/// Allocate a fresh handle and insert `entry` under it.
fn insert_stream(entry: StreamEntry) -> u64 {
    let handle = NEXT_HANDLE.fetch_add(1, Ordering::Relaxed);
    STREAMS.lock().unwrap().insert(handle, entry);
    handle
}

/// Run `f` against the open stream `handle`.
fn with_stream<R>(handle: u64, f: impl FnOnce(&mut StreamEntry) -> R) -> Option<R> {
    STREAMS.lock().unwrap().get_mut(&handle).map(f)
}

// ── Open / Close / Streams ──────────────────────────────────────────────────

/// OpenWrite["file"] — open `file` for writing, truncating it.
///
/// With no argument a fresh temporary file is opened. Returns an integer
/// stream handle, or `$Failed` if the file cannot be opened.
pub fn builtin_open_write(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() > 1 {
        return inert("OpenWrite", args);
    }
    let path = match args.first() {
        Some(v) => match as_str(v) {
            Some(s) => resolve_path(env, &s),
            None => return inert("OpenWrite", args),
        },
        None => std::env::temp_dir().join("syma_stream.tmp"),
    };
    match OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&path)
    {
        Ok(file) => Ok(Value::Integer(rug::Integer::from(insert_stream(
            StreamEntry::Write(file),
        )))),
        Err(_) => Ok(Value::Symbol("$Failed".to_string())),
    }
}

/// OpenAppend["file"] — open `file` for appending, creating it if needed.
pub fn builtin_open_append(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 1 {
        return inert("OpenAppend", args);
    }
    let Some(file) = as_str(&args[0]) else {
        return inert("OpenAppend", args);
    };
    let path = resolve_path(env, &file);
    match OpenOptions::new().create(true).append(true).open(&path) {
        Ok(file) => Ok(Value::Integer(rug::Integer::from(insert_stream(
            StreamEntry::Write(file),
        )))),
        Err(_) => Ok(Value::Symbol("$Failed".to_string())),
    }
}

/// OpenRead["file"] — open `file` for reading. Returns a handle or `$Failed`.
pub fn builtin_open_read(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return inert("OpenRead", args);
    }
    let Some(file) = as_str(&args[0]) else {
        return inert("OpenRead", args);
    };
    let path = resolve_path(env, &file);
    match std::fs::read_to_string(&path) {
        Ok(content) => Ok(Value::Integer(rug::Integer::from(insert_stream(
            StreamEntry::Read { content, pos: 0 },
        )))),
        Err(_) => Ok(Value::Symbol("$Failed".to_string())),
    }
}

/// Close[stream] — close a stream handle. Returns `Null`.
pub fn builtin_close(args: &[Value], _env: &Env) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return inert("Close", args);
    }
    let Some(handle) = as_handle(&args[0]) else {
        return inert("Close", args);
    };
    if STREAMS.lock().unwrap().remove(&handle).is_some() {
        Ok(Value::Null)
    } else {
        Ok(Value::Symbol("$Failed".to_string()))
    }
}

/// Streams[] — list the handles of all currently open streams.
pub fn builtin_streams(args: &[Value], _env: &Env) -> Result<Value, EvalError> {
    if !args.is_empty() {
        return inert("Streams", args);
    }
    let mut handles: Vec<u64> = STREAMS.lock().unwrap().keys().copied().collect();
    handles.sort_unstable();
    Ok(Value::List(
        handles
            .into_iter()
            .map(|h| Value::Integer(rug::Integer::from(h)))
            .collect(),
    ))
}

// ── Write / WriteString ─────────────────────────────────────────────────────

/// Write[stream, expr...] — write `expr`s in `InputForm`, space-separated,
/// followed by a newline.
///
/// When the first argument is not a stream handle this falls back to the
/// stdout behaviour of `Write[expr...]` (concatenated, no newline).
pub fn builtin_write(args: &[Value], _env: &Env) -> Result<Value, EvalError> {
    if let Some(handle) = args.first().and_then(as_handle)
        && let Some(()) = with_stream(handle, |entry| {
            if let StreamEntry::Write(file) = entry {
                let rendered: Vec<String> = args[1..].iter().map(input_form).collect();
                let mut line = rendered.join(" ");
                line.push('\n');
                let _ = file.write_all(line.as_bytes());
            }
        })
    {
        return Ok(Value::Null);
    }
    // stdout fallback (matches the legacy pure `Write`).
    for arg in args {
        match arg {
            Value::Str(s) => print!("{}", s),
            other => print!("{}", other),
        }
    }
    std::io::stdout().flush().ok();
    Ok(Value::Null)
}

/// WriteString[stream, data...] — write `data` verbatim, without separators or
/// a trailing newline.
///
/// `WriteString["file", data]` (a path rather than a stream) writes/overwrites
/// the file, matching the legacy behaviour.
pub fn builtin_write_string(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.first().and_then(as_handle).is_some()
        && let Some(handle) = args.first().and_then(as_handle)
    {
        let data: String = args[1..]
            .iter()
            .map(|v| match v {
                Value::Str(s) => s.clone(),
                other => format!("{}", other),
            })
            .collect();
        let ok = with_stream(handle, |entry| {
            if let StreamEntry::Write(file) = entry {
                file.write_all(data.as_bytes()).is_ok()
            } else {
                false
            }
        });
        if ok == Some(true) {
            return Ok(Value::Null);
        }
    }
    if args.len() == 2
        && let Some(path) = as_str(&args[0])
    {
        let path = resolve_path(env, &path);
        let data = match &args[1] {
            Value::Str(s) => s.clone(),
            other => format!("{}", other),
        };
        std::fs::write(&path, data)
            .map_err(|e| EvalError::Error(format!("WriteString failed: {}", e)))?;
        return Ok(Value::Null);
    }
    inert("WriteString", args)
}

// ── Read / ReadLine / EndOfFile ─────────────────────────────────────────────

/// Take the next line from a read stream, advancing its cursor.
///
/// Returns `None` at end of file. The returned string has no trailing newline.
fn next_line(handle: u64) -> Option<String> {
    with_stream(handle, |entry| match entry {
        StreamEntry::Read { content, pos } => {
            if *pos >= content.len() {
                return None;
            }
            let rest = &content[*pos..];
            let end = rest.find('\n').unwrap_or(rest.len());
            let line = rest[..end].to_string();
            *pos += end + 1;
            Some(line)
        }
        StreamEntry::Write(_) => None,
    })?
}

/// Read[stream] — read and evaluate the next expression from a read stream.
pub fn builtin_read(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return inert("Read", args);
    }
    let Some(handle) = as_handle(&args[0]) else {
        return inert("Read", args);
    };
    let Some(line) = next_line(handle) else {
        return Ok(Value::Symbol("EndOfFile".to_string()));
    };
    let text = line.trim();
    if text.is_empty() {
        return Ok(Value::Null);
    }
    let tokens =
        crate::lexer::tokenize(text).map_err(|e| EvalError::Error(format!("Read: {}", e)))?;
    let stmts =
        crate::parser::parse(tokens).map_err(|e| EvalError::Error(format!("Read: {}", e)))?;
    crate::eval::eval_program(&stmts, env)
}

/// ReadLine[stream] — read the next line of a read stream as a string.
pub fn builtin_read_line(args: &[Value], _env: &Env) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return inert("ReadLine", args);
    }
    let Some(handle) = as_handle(&args[0]) else {
        return inert("ReadLine", args);
    };
    match next_line(handle) {
        Some(mut line) => {
            if line.ends_with('\r') {
                line.pop();
            }
            Ok(Value::Str(line))
        }
        None => Ok(Value::Symbol("EndOfFile".to_string())),
    }
}

/// EndOfFile[stream] — `True` when a read stream's cursor is at end of file.
pub fn builtin_end_of_file(args: &[Value], _env: &Env) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return inert("EndOfFile", args);
    }
    let Some(handle) = as_handle(&args[0]) else {
        return inert("EndOfFile", args);
    };
    match with_stream(handle, |entry| match entry {
        StreamEntry::Read { content, pos } => *pos >= content.len(),
        StreamEntry::Write(_) => true,
    }) {
        Some(at_end) => Ok(Value::Bool(at_end)),
        None => Ok(Value::Symbol("$Failed".to_string())),
    }
}

/// Register the builtins implemented in this module.
pub fn register(env: &Env) {
    super::register_builtin_env(env, "OpenWrite", builtin_open_write);
    super::register_builtin_env(env, "OpenAppend", builtin_open_append);
    super::register_builtin_env(env, "OpenRead", builtin_open_read);
    super::register_builtin_env(env, "Close", builtin_close);
    super::register_builtin_env(env, "Streams", builtin_streams);
    // `Write`/`WriteString` are re-registered here so stream handles are
    // recognised; the stdout/path fallbacks preserve the legacy behaviour.
    super::register_builtin_env(env, "Write", builtin_write);
    super::register_builtin_env(env, "WriteString", builtin_write_string);
    super::register_builtin_env(env, "Read", builtin_read);
    super::register_builtin_env(env, "ReadLine", builtin_read_line);
    super::register_builtin_env(env, "EndOfFile", builtin_end_of_file);

    for name in [
        "OpenWrite",
        "OpenAppend",
        "OpenRead",
        "Close",
        "Streams",
        "Write",
        "WriteString",
        "Read",
        "ReadLine",
        "EndOfFile",
    ] {
        if env.get_attributes(name).is_empty() {
            env.set_attributes(name, vec!["Protected".to_string()]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::Value;
    use rug::Integer;
    use std::fs;

    fn env_with_builtins() -> Env {
        let env = Env::new();
        crate::builtins::register_builtins(&env);
        env
    }

    fn s(x: &str) -> Value {
        Value::Str(x.to_string())
    }

    fn int(n: i64) -> Value {
        Value::Integer(Integer::from(n))
    }

    fn temp_path(tag: &str) -> String {
        let mut p = std::env::temp_dir();
        p.push(format!("syma_streams_{}_{}.txt", std::process::id(), tag));
        p.to_string_lossy().into_owned()
    }

    #[test]
    fn test_open_write_close_roundtrip() {
        let path = temp_path("rw");
        let env = env_with_builtins();

        let handle = builtin_open_write(&[s(&path)], &env).unwrap();
        assert!(matches!(handle, Value::Integer(_)));
        builtin_write_string(&[handle.clone(), s("hello ")], &env).unwrap();
        builtin_write(&[handle.clone(), int(42)], &env).unwrap();
        assert_eq!(builtin_close(&[handle], &env).unwrap(), Value::Null);

        let contents = fs::read_to_string(&path).unwrap();
        assert_eq!(contents, "hello 42\n");
        fs::remove_file(&path).ok();
    }

    #[test]
    fn test_open_append() {
        let path = temp_path("append");
        fs::write(&path, "first\n").unwrap();
        let env = env_with_builtins();
        let handle = builtin_open_append(&[s(&path)], &env).unwrap();
        builtin_write_string(&[handle.clone(), s("second\n")], &env).unwrap();
        builtin_close(&[handle], &env).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "first\nsecond\n");
        fs::remove_file(&path).ok();
    }

    #[test]
    fn test_read_readline_eof() {
        let path = temp_path("read");
        fs::write(&path, "line1\n42\n").unwrap();
        let env = env_with_builtins();

        let handle = builtin_open_read(&[s(&path)], &env).unwrap();
        assert_eq!(
            builtin_read_line(&[handle.clone()], &env).unwrap(),
            s("line1")
        );
        assert_eq!(builtin_read(&[handle.clone()], &env).unwrap(), int(42));
        assert_eq!(
            builtin_end_of_file(&[handle.clone()], &env).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_read_line(&[handle.clone()], &env).unwrap(),
            Value::Symbol("EndOfFile".to_string())
        );
        builtin_close(&[handle], &env).unwrap();
        fs::remove_file(&path).ok();
    }

    #[test]
    fn test_missing_file_returns_failed() {
        let env = env_with_builtins();
        assert_eq!(
            builtin_open_read(&[s("/no/such/file.xyz")], &env).unwrap(),
            Value::Symbol("$Failed".to_string())
        );
    }

    #[test]
    fn test_streams_lists_open_handles() {
        let path = temp_path("list");
        let env = env_with_builtins();
        let handle = builtin_open_write(&[s(&path)], &env).unwrap();
        let listed = builtin_streams(&[], &env).unwrap();
        match listed {
            Value::List(items) => assert!(items.contains(&handle)),
            other => panic!("expected list, got {:?}", other),
        }
        builtin_close(&[handle], &env).unwrap();
        fs::remove_file(&path).ok();
    }

    #[test]
    fn test_wrong_args_are_inert() {
        let env = env_with_builtins();
        assert!(matches!(
            builtin_open_read(&[int(1)], &env).unwrap(),
            Value::Call { .. }
        ));
        assert!(matches!(
            builtin_close(&[], &env).unwrap(),
            Value::Call { .. }
        ));
    }

    #[test]
    fn test_put_get_operators_via_eval() {
        let path = temp_path("ops");
        let env = env_with_builtins();
        let input = format!("1 + 2 >> \"{}\"", path);
        let tokens = crate::lexer::tokenize(&input).unwrap();
        let ast = crate::parser::parse(tokens).unwrap();
        crate::eval::eval_program(&ast, &env).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "3\n");

        let input = format!("<< \"{}\"", path);
        let tokens = crate::lexer::tokenize(&input).unwrap();
        let ast = crate::parser::parse(tokens).unwrap();
        assert_eq!(crate::eval::eval_program(&ast, &env).unwrap(), int(3));
        fs::remove_file(&path).ok();
    }
}
