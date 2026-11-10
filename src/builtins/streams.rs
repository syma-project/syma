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
use std::io::Seek;
use std::io::Write as IoWrite;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

/// Environment key holding the session's notion of the current directory.
/// Mirrors the key used by `systemio.rs` so relative paths resolve the same way.
const CWD_KEY: &str = "$SymaCurrentDirectory";

/// Live state for one open stream.
enum StreamEntry {
    /// A read stream buffering the whole file as raw bytes plus a byte cursor.
    Read { content: Vec<u8>, pos: usize },
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

/// Render a value for output: strings verbatim, everything else via `Display`.
fn display_form(v: &Value) -> String {
    match v {
        Value::Str(s) => s.clone(),
        other => format!("{}", other),
    }
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

/// Byte cursor of the open stream `handle`, if it can be determined.
///
/// Read streams track an in-memory cursor; write streams are queried through
/// the underlying file handle.
pub(crate) fn stream_position(handle: u64) -> Option<u64> {
    with_stream(handle, |entry| match entry {
        StreamEntry::Read { pos, .. } => Some(*pos as u64),
        StreamEntry::Write(file) => file.stream_position().ok(),
    })?
}

/// Set the byte cursor of an open stream. Returns `false` for unknown handles
/// or write streams that cannot seek.
pub(crate) fn set_stream_position(handle: u64, pos: u64) -> bool {
    with_stream(handle, |entry| match entry {
        StreamEntry::Read { content, pos: p } => {
            *p = (pos as usize).min(content.len());
            true
        }
        StreamEntry::Write(file) => file.seek(std::io::SeekFrom::Start(pos)).is_ok(),
    })
    .unwrap_or(false)
}

/// Read all remaining characters from a read stream.
fn take_all(handle: u64) -> Option<String> {
    with_stream(handle, |entry| match entry {
        StreamEntry::Read { content, pos } => {
            let rest = String::from_utf8_lossy(&content[*pos..]).into_owned();
            *pos = content.len();
            Some(rest)
        }
        StreamEntry::Write(_) => None,
    })?
}

/// Read up to `n` characters from a read stream.
fn take_chars(handle: u64, n: usize) -> Option<String> {
    with_stream(handle, |entry| match entry {
        StreamEntry::Read { content, pos } => {
            let rest = String::from_utf8_lossy(&content[*pos..]);
            let taken: String = rest.chars().take(n).collect();
            // Advance by the number of bytes that produced the taken chars
            // (exact for valid UTF-8; conservative otherwise).
            let advance = taken.len().min(content.len() - *pos);
            *pos += advance;
            Some(taken)
        }
        StreamEntry::Write(_) => None,
    })?
}

/// Read up to `n` bytes from a read stream (advancing the cursor).
fn take_bytes(handle: u64, n: usize) -> Option<Vec<u8>> {
    with_stream(handle, |entry| match entry {
        StreamEntry::Read { content, pos } => {
            if *pos >= content.len() {
                return Some(Vec::new());
            }
            let end = pos.saturating_add(n).min(content.len());
            let out = content[*pos..end].to_vec();
            *pos = end;
            Some(out)
        }
        StreamEntry::Write(_) => None,
    })?
}

/// Discard up to `n` lines from a read stream.
fn skip_lines(handle: u64, n: usize) -> Option<()> {
    with_stream(handle, |entry| match entry {
        StreamEntry::Read { content, pos } => {
            for _ in 0..n {
                if *pos >= content.len() {
                    break;
                }
                let rest = &content[*pos..];
                let end = rest
                    .iter()
                    .position(|&b| b == b'\n')
                    .map(|i| i + 1)
                    .unwrap_or(rest.len());
                *pos += end;
            }
            Some(())
        }
        StreamEntry::Write(_) => None,
    })?
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
    match std::fs::read(&path) {
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
            let end = rest.iter().position(|&b| b == b'\n').unwrap_or(rest.len());
            let line = String::from_utf8_lossy(&rest[..end]).into_owned();
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

// ── ReadString / ReadList / Skip / SetStreamPosition ───────────────────────

/// ReadString[stream] / ReadString[stream, n] — read the rest of a read stream
/// (or the next `n` characters) as a single string.
///
/// `ReadString["file"]` falls back to reading the whole file.
pub fn builtin_read_string(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.first().and_then(as_handle).is_some()
        && let Some(handle) = args.first().and_then(as_handle)
    {
        let n = match args.get(1) {
            None => None,
            Some(Value::Integer(n)) => Some(n.to_usize().unwrap_or(0)),
            Some(_) => return inert("ReadString", args),
        };
        let text = match n {
            None => take_all(handle),
            Some(n) => take_chars(handle, n),
        };
        return match text {
            Some(t) => Ok(Value::Str(t)),
            None => Ok(Value::Symbol("EndOfFile".to_string())),
        };
    }
    // Path fallback: only the 1-argument form writes/reads whole files.
    if args.len() == 1 {
        return super::io::builtin_read_string(args);
    }
    let _ = env;
    inert("ReadString", args)
}

/// ReadList[stream] / ReadList[stream, n] — read the remaining lines (or the
/// next `n` lines) of a read stream as a list of strings.
pub fn builtin_read_list(args: &[Value], _env: &Env) -> Result<Value, EvalError> {
    if let Some(handle) = args.first().and_then(as_handle) {
        let count = match args.get(1) {
            None => None,
            Some(Value::Integer(n)) => Some(n.to_usize().unwrap_or(0)),
            Some(_) => return inert("ReadList", args),
        };
        let mut items = Vec::new();
        loop {
            if let Some(n) = count
                && items.len() >= n
            {
                break;
            }
            match next_line(handle) {
                Some(mut line) => {
                    if line.ends_with('\r') {
                        line.pop();
                    }
                    items.push(Value::Str(line));
                }
                None => break,
            }
        }
        return Ok(Value::List(items));
    }
    if args.len() == 1 {
        return super::io::builtin_read_list(args);
    }
    inert("ReadList", args)
}

/// Skip[stream] / Skip[stream, n] — skip `n` lines of a read stream.
pub fn builtin_skip(args: &[Value], _env: &Env) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return inert("Skip", args);
    }
    let Some(handle) = as_handle(&args[0]) else {
        return inert("Skip", args);
    };
    let n = match args.get(1) {
        None => 1usize,
        Some(Value::Integer(n)) => n.to_usize().unwrap_or(1),
        Some(_) => return inert("Skip", args),
    };
    match skip_lines(handle, n) {
        Some(()) => Ok(Value::Null),
        None => Ok(Value::Symbol("$Failed".to_string())),
    }
}

/// SetStreamPosition[stream, pos] — move a stream's cursor to byte `pos`.
pub fn builtin_set_stream_position(args: &[Value], _env: &Env) -> Result<Value, EvalError> {
    let [stream, Value::Integer(pos)] = args else {
        return inert("SetStreamPosition", args);
    };
    let Some(handle) = as_handle(stream) else {
        return inert("SetStreamPosition", args);
    };
    let pos = pos.to_u64().unwrap_or(0);
    if set_stream_position(handle, pos) {
        Ok(Value::Integer(rug::Integer::from(pos)))
    } else {
        Ok(Value::Symbol("$Failed".to_string()))
    }
}

// ── WriteLine ───────────────────────────────────────────────────────────────

/// WriteLine[stream, expr...] — write `expr`s in `InputForm`, newline-terminated.
///
/// Falls back to stdout when the first argument is not a stream handle.
pub fn builtin_write_line(args: &[Value], _env: &Env) -> Result<Value, EvalError> {
    if let Some(handle) = args.first().and_then(as_handle)
        && let Some(()) = with_stream(handle, |entry| {
            if let StreamEntry::Write(file) = entry {
                let rendered: Vec<String> = args[1..].iter().map(display_form).collect();
                let mut line = rendered.join(" ");
                line.push('\n');
                let _ = file.write_all(line.as_bytes());
            }
        })
    {
        return Ok(Value::Null);
    }
    for arg in args {
        match arg {
            Value::Str(s) => print!("{}", s),
            other => print!("{}", other),
        }
    }
    println!();
    Ok(Value::Null)
}

// ── BinaryRead / BinaryWrite ────────────────────────────────────────────────

/// BinaryRead[stream] / BinaryRead[stream, n] — read bytes from a read stream as
/// a list of integers.
pub fn builtin_binary_read_stream(args: &[Value], _env: &Env) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return inert("BinaryRead", args);
    }
    let Some(handle) = as_handle(&args[0]) else {
        return inert("BinaryRead", args);
    };
    let n = match args.get(1) {
        None => usize::MAX,
        Some(Value::Integer(n)) => n.to_usize().unwrap_or(0),
        Some(_) => return inert("BinaryRead", args),
    };
    match take_bytes(handle, n) {
        Some(bytes) => Ok(Value::List(
            bytes
                .iter()
                .map(|b| Value::Integer(rug::Integer::from(*b)))
                .collect(),
        )),
        None => Ok(Value::Symbol("$Failed".to_string())),
    }
}

/// BinaryWrite[stream, {b1, b2, ...}] — write raw bytes to a write stream.
pub fn builtin_binary_write_stream(args: &[Value], _env: &Env) -> Result<Value, EvalError> {
    let [stream, Value::List(bytes)] = args else {
        return inert("BinaryWrite", args);
    };
    let Some(handle) = as_handle(stream) else {
        return inert("BinaryWrite", args);
    };
    let mut out = Vec::with_capacity(bytes.len());
    for b in bytes {
        match b {
            Value::Integer(n) => out.push(n.to_u8().unwrap_or(0)),
            _ => return inert("BinaryWrite", args),
        }
    }
    match with_stream(handle, |entry| {
        if let StreamEntry::Write(file) = entry {
            file.write_all(&out).is_ok()
        } else {
            false
        }
    }) {
        Some(true) => Ok(Value::Null),
        _ => Ok(Value::Symbol("$Failed".to_string())),
    }
}

// ── Put / Get via streams ───────────────────────────────────────────────────

/// Put[expr..., stream] — write `expr`s to a stream/file in `InputForm`.
pub fn builtin_put(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return inert("Put", args);
    }
    let last = &args[args.len() - 1];
    if let Some(handle) = as_handle(last) {
        let ok = with_stream(handle, |entry| {
            if let StreamEntry::Write(file) = entry {
                let mut out = String::new();
                for expr in &args[..args.len() - 1] {
                    out.push_str(&input_form(expr));
                    out.push('\n');
                }
                file.write_all(out.as_bytes()).is_ok()
            } else {
                false
            }
        });
        if ok == Some(true) {
            return Ok(Value::Null);
        }
    }
    crate::builtins::systemio::builtin_put(args, env)
}

/// PutAppend[expr..., stream] — append `expr`s to a stream/file in `InputForm`.
pub fn builtin_put_append(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return inert("PutAppend", args);
    }
    let last = &args[args.len() - 1];
    if let Some(handle) = as_handle(last) {
        let ok = with_stream(handle, |entry| {
            if let StreamEntry::Write(file) = entry {
                let mut out = String::new();
                for expr in &args[..args.len() - 1] {
                    out.push_str(&input_form(expr));
                    out.push('\n');
                }
                file.write_all(out.as_bytes()).is_ok()
            } else {
                false
            }
        });
        if ok == Some(true) {
            return Ok(Value::Null);
        }
    }
    crate::builtins::notebook::builtin_put_append(args, env)
}

/// Get[stream] — read and evaluate the remaining expressions of a read stream.
///
/// Falls back to `systemio::Get` for a file name.
pub fn builtin_get(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return inert("Get", args);
    }
    // `Get` is `HoldAll`, so the argument may still be an unevaluated pattern.
    let arg = match &args[0] {
        Value::Pattern(e) => crate::eval::eval(e, env)?,
        other => other.clone(),
    };
    if let Some(handle) = as_handle(&arg) {
        let Some(source) = take_all(handle) else {
            return Ok(Value::Symbol("$Failed".to_string()));
        };
        if source.trim().is_empty() {
            return Ok(Value::Null);
        }
        let tokens =
            crate::lexer::tokenize(&source).map_err(|e| EvalError::Error(format!("Get: {}", e)))?;
        let stmts = crate::parser::parse_with_suppress(tokens)
            .map_err(|e| EvalError::Error(format!("Get: {}", e)))?;
        let exprs: Vec<crate::ast::Expr> = stmts.into_iter().map(|(e, _)| e).collect();
        return crate::eval::eval_program(&exprs, env);
    }
    crate::builtins::systemio::builtin_get(&[arg], env)
}

/// Register the builtins implemented in this module.
/// StringToStream[s] — a read stream over the string `s`.
pub fn builtin_string_to_stream(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "StringToStream requires exactly 1 argument".to_string(),
        ));
    }
    let Some(s) = as_str(&args[0]) else {
        return Err(EvalError::TypeError {
            expected: "String".to_string(),
            got: args[0].type_name().to_string(),
        });
    };
    let handle = NEXT_HANDLE.fetch_add(1, Ordering::Relaxed);
    STREAMS.lock().unwrap().insert(
        handle,
        StreamEntry::Read {
            content: s.into_bytes(),
            pos: 0,
        },
    );
    Ok(Value::Integer(rug::Integer::from(handle as i64)))
}

pub fn register(env: &Env) {
    super::register_builtin_env(env, "OpenWrite", builtin_open_write);
    super::register_builtin(env, "StringToStream", builtin_string_to_stream);
    super::register_builtin_env(env, "OpenAppend", builtin_open_append);
    super::register_builtin_env(env, "OpenRead", builtin_open_read);
    super::register_builtin_env(env, "Close", builtin_close);
    super::register_builtin_env(env, "Streams", builtin_streams);
    // `Write`/`WriteString` are re-registered here so stream handles are
    // recognised; the stdout/path fallbacks preserve the legacy behaviour.
    super::register_builtin_env(env, "Write", builtin_write);
    super::register_builtin_env(env, "WriteString", builtin_write_string);
    super::register_builtin_env(env, "WriteLine", builtin_write_line);
    super::register_builtin_env(env, "Read", builtin_read);
    super::register_builtin_env(env, "ReadLine", builtin_read_line);
    super::register_builtin_env(env, "ReadString", builtin_read_string);
    super::register_builtin_env(env, "ReadList", builtin_read_list);
    super::register_builtin_env(env, "EndOfFile", builtin_end_of_file);
    super::register_builtin_env(env, "Skip", builtin_skip);
    super::register_builtin_env(env, "SetStreamPosition", builtin_set_stream_position);
    // `Put`/`Get` are re-registered so they accept open stream handles too.
    super::register_builtin_env(env, "Put", builtin_put);
    super::register_builtin_env(env, "Get", builtin_get);

    for name in [
        "OpenWrite",
        "OpenAppend",
        "OpenRead",
        "StringToStream",
        "Close",
        "Streams",
        "Write",
        "WriteString",
        "WriteLine",
        "Read",
        "ReadLine",
        "ReadString",
        "ReadList",
        "EndOfFile",
        "Skip",
        "SetStreamPosition",
        "Put",
        "Get",
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

    #[test]
    fn test_string_to_stream_readline() {
        let env = env_with_builtins();
        let handle = builtin_string_to_stream(&[s("one\ntwo\n")]).unwrap();
        let l1 = builtin_read_line(&[handle.clone()], &env).unwrap();
        let l2 = builtin_read_line(&[handle.clone()], &env).unwrap();
        assert_eq!(l1, s("one"));
        assert_eq!(l2, s("two"));
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

    // ── Stream cursor / reading breadth ──────────────────────────────────

    #[test]
    fn test_set_stream_position_then_read() {
        let env = env_with_builtins();
        let handle = builtin_string_to_stream(&[s("hello\nworld\n")]).unwrap();
        assert_eq!(
            builtin_set_stream_position(&[handle.clone(), int(6)], &env).unwrap(),
            int(6)
        );
        assert_eq!(
            builtin_read_line(&[handle.clone()], &env).unwrap(),
            s("world")
        );
        builtin_close(&[handle], &env).unwrap();
    }

    #[test]
    fn test_skip_lines() {
        let env = env_with_builtins();
        let handle = builtin_string_to_stream(&[s("a\nb\nc\n")]).unwrap();
        assert_eq!(
            builtin_skip(&[handle.clone(), int(1)], &env).unwrap(),
            Value::Null
        );
        assert_eq!(builtin_read_line(&[handle.clone()], &env).unwrap(), s("b"));
        // Default `Skip[stream]` skips one line.
        assert_eq!(builtin_skip(&[handle.clone()], &env).unwrap(), Value::Null);
        assert_eq!(
            builtin_read_line(&[handle.clone()], &env).unwrap(),
            Value::Symbol("EndOfFile".to_string())
        );
        builtin_close(&[handle], &env).unwrap();
    }

    #[test]
    fn test_read_string_chars_and_rest() {
        let env = env_with_builtins();
        let handle = builtin_string_to_stream(&[s("hello world\nsecond\n")]).unwrap();
        assert_eq!(
            builtin_read_string(&[handle.clone(), int(5)], &env).unwrap(),
            s("hello")
        );
        assert_eq!(
            builtin_read_string(&[handle.clone()], &env).unwrap(),
            s(" world\nsecond\n")
        );
        builtin_close(&[handle], &env).unwrap();
    }

    #[test]
    fn test_read_list_stream() {
        let env = env_with_builtins();
        let handle = builtin_string_to_stream(&[s("one\ntwo\nthree\n")]).unwrap();
        assert_eq!(
            builtin_read_list(&[handle.clone(), int(2)], &env).unwrap(),
            Value::List(vec![s("one"), s("two")])
        );
        assert_eq!(
            builtin_read_list(&[handle.clone()], &env).unwrap(),
            Value::List(vec![s("three")])
        );
        builtin_close(&[handle], &env).unwrap();
    }

    #[test]
    fn test_write_line_to_stream() {
        let path = temp_path("writeline");
        let env = env_with_builtins();
        let handle = builtin_open_write(&[s(&path)], &env).unwrap();
        builtin_write_line(&[handle.clone(), s("x"), int(1)], &env).unwrap();
        builtin_close(&[handle], &env).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "x 1\n");
        fs::remove_file(&path).ok();
    }

    #[test]
    fn test_binary_read_write_via_stream() {
        let path = temp_path("binstream");
        let env = env_with_builtins();
        let handle = builtin_open_write(&[s(&path)], &env).unwrap();
        builtin_binary_write_stream(
            &[
                handle.clone(),
                Value::List(vec![int(10), int(20), int(255)]),
            ],
            &env,
        )
        .unwrap();
        builtin_close(&[handle], &env).unwrap();
        assert_eq!(fs::read(&path).unwrap(), vec![10u8, 20, 255]);

        let read = builtin_open_read(&[s(&path)], &env).unwrap();
        assert_eq!(
            builtin_binary_read_stream(&[read.clone(), int(2)], &env).unwrap(),
            Value::List(vec![int(10), int(20)])
        );
        assert_eq!(
            builtin_binary_read_stream(&[read.clone()], &env).unwrap(),
            Value::List(vec![int(255)])
        );
        builtin_close(&[read], &env).unwrap();
        fs::remove_file(&path).ok();
    }

    #[test]
    fn test_put_and_get_via_stream() {
        let path = temp_path("putstream");
        let env = env_with_builtins();
        let handle = builtin_open_write(&[s(&path)], &env).unwrap();
        builtin_put(&[int(41), int(1), handle.clone()], &env).unwrap();
        builtin_close(&[handle], &env).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "41\n1\n");

        let read = builtin_open_read(&[s(&path)], &env).unwrap();
        assert_eq!(builtin_get(&[read.clone()], &env).unwrap(), int(1));
        builtin_close(&[read], &env).unwrap();
        fs::remove_file(&path).ok();
    }

    #[test]
    fn test_put_append_via_stream() {
        let path = temp_path("putappendstream");
        let env = env_with_builtins();
        let handle = builtin_open_write(&[s(&path)], &env).unwrap();
        builtin_put_append(&[int(7), handle.clone()], &env).unwrap();
        builtin_put_append(&[int(8), handle.clone()], &env).unwrap();
        builtin_close(&[handle], &env).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "7\n8\n");
        fs::remove_file(&path).ok();
    }
}
