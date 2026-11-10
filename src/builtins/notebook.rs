//! notebook/session: Notebook, NotebookObject, CreateWindow, BinaryRead/Write, TimeUsed, SymbolName.
//!
//! Gap-fill module; see the individual builtin docs for semantics.
//!
//! The "notebook" builtins are deliberately symbolic: `Notebook`, `NotebookObject`,
//! `CreateWindow` and `CurrentNotebooks` round-trip through `Value::Call` because
//! there is no live front-end attached to a kernel `Env`. The remaining builtins
//! cover session introspection (`SymbolName`, `Symbols`, `Contexts`), timing,
//! binary file I/O, protection queries, and small filesystem helpers.

use crate::ast::Expr;
use crate::env::Env;
use crate::value::{EvalError, Format, Value};
use rug::Integer;
use std::collections::{BTreeSet, HashMap};
use std::io::Write as IoWrite;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Environment key holding the session's notion of the current directory.
/// Mirrors the key used by `systemio.rs`/`streams.rs` so relative paths resolve
/// the same way everywhere.
const CWD_KEY: &str = "$SymaCurrentDirectory";

/// Wall-clock instant at which the process (session) started.
static SESSION_START: LazyLock<Instant> = LazyLock::new(Instant::now);

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Return an unevaluated `head[args...]` call.
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

/// Extract a symbol name from a plain, held (`Pattern`), or string argument.
fn symbol_name(v: &Value) -> Option<String> {
    match v {
        Value::Symbol(s) | Value::Str(s) | Value::Builtin(s, _) => Some(s.clone()),
        Value::Pattern(Expr::Symbol(s)) | Value::Pattern(Expr::Str(s)) => Some(s.clone()),
        _ => None,
    }
}

/// Extract an integer stream handle.
fn as_handle(v: &Value) -> Option<u64> {
    match v {
        Value::Integer(n) => n.to_u64(),
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

/// Render a value using `InputForm` (used by `PutAppend`).
fn input_form(v: &Value) -> String {
    format!(
        "{}",
        Value::Formatted {
            format: Format::InputForm,
            value: Box::new(v.clone()),
        }
    )
}

/// Every symbol name known to `env` (bindings, lazy providers, attributes).
fn collect_symbol_names(env: &Env) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for (name, _) in env.all_bindings() {
        names.insert(name);
    }
    {
        let providers = env.lazy_providers.lock().unwrap();
        names.extend(providers.keys().cloned());
    }
    {
        let attrs = env.attributes.lock().unwrap();
        names.extend(attrs.keys().cloned());
    }
    names
}

/// Recursively copy the directory tree rooted at `src` into `dst`.
fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_all(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

// ── Symbol / context introspection ──────────────────────────────────────────

/// The final segment of a (possibly context-qualified) symbol name.
fn base_name(name: &str) -> String {
    match name.rfind('`') {
        Some(idx) => name[idx + 1..].to_string(),
        None => name.to_string(),
    }
}

/// A symbol is "defined" when it carries a value/definition or attributes.
/// A bare self-symbol binding and the empty attribute entry `Clear` leaves
/// behind do not count.
fn is_defined(env: &Env, name: &str) -> bool {
    if !env.get_attributes(name).is_empty() {
        return true;
    }
    match env.get(name) {
        Some(Value::Symbol(s)) if s == name => false,
        Some(_) => true,
        None => false,
    }
}

/// `SymbolName[sym]` — the name of `sym`, without its context. `HoldFirst`, so
/// `x = 5; SymbolName[x]` yields `"x"` while a held expression such as
/// `SymbolName[Symbol["abc"]]` still resolves to `"abc"`.
pub fn builtin_symbol_name(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    let name = match args.first() {
        Some(Value::Pattern(Expr::Symbol(s))) | Some(Value::Symbol(s)) => Some(s.clone()),
        Some(Value::Pattern(Expr::Str(s))) | Some(Value::Str(s)) => Some(s.clone()),
        Some(Value::Pattern(e)) => match crate::eval::eval(e, env) {
            Ok(Value::Symbol(s)) | Ok(Value::Str(s)) => Some(s),
            _ => None,
        },
        Some(other) => symbol_name(other),
        None => None,
    };
    match name {
        Some(name) => Ok(Value::Str(base_name(&name))),
        None => inert("SymbolName", args),
    }
}

/// `Symbols[]` / `Symbols[pattern]` — sorted list of defined symbols (as
/// `Symbol` values), optionally restricted to names matching `pattern`.
pub fn builtin_symbols(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() > 1 {
        return inert("Symbols", args);
    }
    let defined: Vec<String> = collect_symbol_names(env)
        .into_iter()
        .filter(|name| is_defined(env, name))
        .collect();
    if args.is_empty() {
        return Ok(Value::List(
            defined.into_iter().map(Value::Symbol).collect(),
        ));
    }
    let Some(pattern) = as_str(&args[0]) else {
        return inert("Symbols", args);
    };
    Ok(Value::List(
        defined
            .into_iter()
            .filter(|name| super::string::glob_match(&pattern, name))
            .map(Value::Symbol)
            .collect(),
    ))
}

/// `Contexts[]` / `Contexts[pattern]` — sorted list of contexts known to the
/// session, optionally restricted to those matching `pattern`.
pub fn builtin_contexts(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    let context_filter = match args.first() {
        Some(v) => match as_str(v) {
            Some(p) => Some(p),
            None => return inert("Contexts", args),
        },
        None => None,
    };
    if args.len() > 1 {
        return inert("Contexts", args);
    }
    let mut contexts = BTreeSet::new();
    for ctx in env.context_stack.lock().unwrap().iter() {
        contexts.insert(ctx.clone());
    }
    contexts.insert("Global`".to_string());
    contexts.insert("System`".to_string());
    for name in collect_symbol_names(env) {
        if let Some(idx) = name.rfind('`') {
            contexts.insert(name[..=idx].to_string());
        }
    }
    let filtered: Vec<Value> = contexts
        .into_iter()
        .filter(|ctx| match &context_filter {
            Some(p) => super::string::glob_match(p, ctx),
            None => true,
        })
        .map(Value::Str)
        .collect();
    Ok(Value::List(filtered))
}

// ── Notebook / front-end stubs ──────────────────────────────────────────────

/// `Notebook[expr]` — symbolic notebook expression.
pub fn builtin_notebook(args: &[Value]) -> Result<Value, EvalError> {
    inert("Notebook", args)
}

/// `NotebookObject[id]` — symbolic handle to a notebook.
pub fn builtin_notebook_object(args: &[Value]) -> Result<Value, EvalError> {
    inert("NotebookObject", args)
}

/// `CreateWindow[...]` — symbolic request to open a front-end window.
pub fn builtin_create_window(args: &[Value]) -> Result<Value, EvalError> {
    inert("CreateWindow", args)
}

/// Registry of open (in-memory) notebooks, keyed by id.
static OPEN_NOTEBOOKS: LazyLock<Mutex<HashMap<u64, Value>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static NEXT_NOTEBOOK: AtomicU64 = AtomicU64::new(1);

/// `CreateDocument[content?]` — create a notebook and return its handle.
pub fn builtin_create_document(args: &[Value]) -> Result<Value, EvalError> {
    let id = NEXT_NOTEBOOK.fetch_add(1, Ordering::Relaxed);
    let content = args.first().cloned().unwrap_or(Value::Null);
    OPEN_NOTEBOOKS.lock().unwrap().insert(id, content);
    Ok(Value::Call {
        head: "NotebookObject".to_string(),
        args: vec![Value::Integer(rug::Integer::from(id as i64))],
    })
}

/// `NotebookSave[nb]` or `NotebookSave[nb, path]` — save a notebook.
pub fn builtin_notebook_save(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Ok(Value::Call {
            head: "NotebookSave".to_string(),
            args: args.to_vec(),
        });
    }
    if let Some(Value::Str(path)) = args.get(1) {
        let id = match &args[0] {
            Value::Call { head, args: a } if head == "NotebookObject" => {
                a.first().and_then(|v| v.to_integer()).map(|i| i as u64)
            }
            _ => None,
        };
        let content = id
            .and_then(|i| OPEN_NOTEBOOKS.lock().unwrap().get(&i).cloned())
            .unwrap_or(Value::Null);
        let text = format!("Notebook[{{Cell[{}]}}]", content);
        std::fs::write(path, text)
            .map_err(|e| EvalError::Error(format!("NotebookSave: cannot write '{path}': {e}")))?;
    }
    Ok(args[0].clone())
}

/// `Cell[...]` / `BoxData[...]` — symbolic notebook constructs (kept inert).
pub fn builtin_cell(args: &[Value]) -> Result<Value, EvalError> {
    inert("Cell", args)
}

pub fn builtin_box_data(args: &[Value]) -> Result<Value, EvalError> {
    inert("BoxData", args)
}

/// `TextCell[...]` — symbolic notebook text cell (kept inert).
pub fn builtin_text_cell(args: &[Value]) -> Result<Value, EvalError> {
    inert("TextCell", args)
}

/// `SetSelectedNotebook[nb]` — select `nb`, returning it.
pub fn builtin_set_selected_notebook(args: &[Value]) -> Result<Value, EvalError> {
    match args {
        [nb] => Ok(nb.clone()),
        _ => inert("SetSelectedNotebook", args),
    }
}

/// `CurrentNotebooks[]` — list of open notebook objects (empty in a headless kernel).
pub fn builtin_current_notebooks(args: &[Value]) -> Result<Value, EvalError> {
    if !args.is_empty() {
        return inert("CurrentNotebooks", args);
    }
    Ok(Value::List(vec![]))
}

// ── Binary file I/O ─────────────────────────────────────────────────────────

/// `BinaryRead["file"]` / `BinaryRead["file", n]` — read all (or the first `n`)
/// bytes as a list of byte values. Returns `$Failed` when the file is missing.
pub fn builtin_binary_read(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    // A leading stream handle takes precedence over a file-name path.
    if args.first().and_then(as_handle).is_some() {
        return super::streams::builtin_binary_read_stream(args, env);
    }
    let (file, count) = match args {
        [p] => (as_str(p), None),
        [p, Value::Integer(n)] => (as_str(p), n.to_usize()),
        _ => return inert("BinaryRead", args),
    };
    let Some(file) = file else {
        return inert("BinaryRead", args);
    };
    let path = resolve_path(env, &file);
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(_) => return Ok(Value::Symbol("$Failed".to_string())),
    };
    let end = count.map(|n| n.min(bytes.len())).unwrap_or(bytes.len());
    Ok(Value::List(
        bytes[..end]
            .iter()
            .map(|b| Value::Integer(Integer::from(*b)))
            .collect(),
    ))
}

/// `BinaryWrite["file", {b1, b2, ...}]` — write raw bytes to a file.
pub fn builtin_binary_write(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    // A leading stream handle writes to the open stream instead of a path.
    if args.first().and_then(as_handle).is_some() {
        return super::streams::builtin_binary_write_stream(args, env);
    }
    let (file, bytes) = match args {
        [p, Value::List(bytes)] => (as_str(p), bytes),
        _ => return inert("BinaryWrite", args),
    };
    let Some(file) = file else {
        return inert("BinaryWrite", args);
    };
    let mut out = Vec::with_capacity(bytes.len());
    for b in bytes {
        match b {
            Value::Integer(n) => out.push(n.to_u8().unwrap_or(0)),
            _ => return inert("BinaryWrite", args),
        }
    }
    let path = resolve_path(env, &file);
    std::fs::write(&path, out)
        .map_err(|e| EvalError::Error(format!("BinaryWrite failed: {}", e)))?;
    Ok(Value::Null)
}

// ── Streams ─────────────────────────────────────────────────────────────────

/// `StreamPosition[stream]` — the byte cursor of an open stream.
///
/// Read streams report their cursor; write streams report the underlying file
/// position. When no cursor is available (a closed or unknown handle) the
/// stream object is returned unchanged, and a non-handle argument leaves the
/// call unevaluated.
pub fn builtin_stream_position(args: &[Value]) -> Result<Value, EvalError> {
    let [stream] = args else {
        return inert("StreamPosition", args);
    };
    let Some(handle) = as_handle(stream) else {
        return inert("StreamPosition", args);
    };
    match super::streams::stream_position(handle) {
        Some(pos) => Ok(Value::Integer(Integer::from(pos))),
        None => Ok(stream.clone()),
    }
}

// ── Timing ──────────────────────────────────────────────────────────────────

/// `TimeUsed[]` — CPU time consumed by the session, in seconds.
pub fn builtin_time_used(args: &[Value]) -> Result<Value, EvalError> {
    if !args.is_empty() {
        return inert("TimeUsed", args);
    }
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    let cpu = unsafe {
        if libc::clock_gettime(libc::CLOCK_PROCESS_CPUTIME_ID, &mut ts) == 0 {
            ts.tv_sec as f64 + ts.tv_nsec as f64 / 1e9
        } else {
            0.0
        }
    };
    Ok(super::real(cpu))
}

/// `SessionTime[]` — wall-clock seconds since the session started.
pub fn builtin_session_time(args: &[Value]) -> Result<Value, EvalError> {
    if !args.is_empty() {
        return inert("SessionTime", args);
    }
    Ok(super::real(SESSION_START.elapsed().as_secs_f64()))
}

/// `TimeObject[]` — object holding the current time of day (UTC).
pub fn builtin_time_object(args: &[Value]) -> Result<Value, EvalError> {
    if !args.is_empty() {
        return inert("TimeObject", args);
    }
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let day = secs % 86_400;
    let mut fields = HashMap::new();
    fields.insert(
        "hour".to_string(),
        Value::Integer(Integer::from(day / 3_600)),
    );
    fields.insert(
        "minute".to_string(),
        Value::Integer(Integer::from((day % 3_600) / 60)),
    );
    fields.insert("second".to_string(), super::real((day % 60) as f64));
    Ok(Value::Object {
        class_name: "TimeObject".to_string(),
        fields,
    })
}

// ── Protection / attributes ─────────────────────────────────────────────────

/// `ProtectedQ[sym]` — `True` when `sym` carries the `Protected` attribute.
pub fn builtin_protected_q(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    let Some(name) = args.first().and_then(symbol_name) else {
        return inert("ProtectedQ", args);
    };
    Ok(Value::Bool(env.has_attribute(&name, "Protected")))
}

/// `HoldAttributes[]` — the list of hold-related attributes.
pub fn builtin_hold_attributes(args: &[Value]) -> Result<Value, EvalError> {
    if !args.is_empty() {
        return inert("HoldAttributes", args);
    }
    Ok(Value::List(
        [
            "HoldAll",
            "HoldAllComplete",
            "HoldFirst",
            "HoldRest",
            "SequenceHold",
        ]
        .iter()
        .map(|s| Value::Symbol((*s).to_string()))
        .collect(),
    ))
}

/// `Default[f]` / `Default[f, i]` — default value for the optional argument
/// slot `i` of `f`. Consults `Default[f]`/`Default[f, i]` bindings first, then
/// a few built-in heads, otherwise stays symbolic.
pub fn builtin_default(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    let Some(name) = args.first().and_then(symbol_name) else {
        return inert("Default", args);
    };
    let idx = match args.get(1) {
        Some(Value::Integer(n)) => n.to_i64().unwrap_or(1),
        Some(_) => return inert("Default", args),
        None => 1,
    };
    if let Some(v) = env.get(&format!("Default[{}, {}]", name, idx)) {
        return Ok(v);
    }
    if let Some(v) = env.get(&format!("Default[{}]", name)) {
        return Ok(v);
    }
    match name.as_str() {
        "Plus" => Ok(Value::Integer(Integer::from(0))),
        "Times" | "Power" | "Dot" | "TimesBy" => Ok(Value::Integer(Integer::from(1))),
        _ => inert("Default", args),
    }
}

// ── PutAppend / filesystem helpers ──────────────────────────────────────────

/// `PutAppend[expr, "file"]` — append `expr` to `file` in `InputForm`. Multiple
/// expressions may be supplied before the trailing file name. A trailing stream
/// handle is written to directly.
pub fn builtin_put_append(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() >= 2 && args.last().and_then(as_handle).is_some() {
        return super::streams::builtin_put_append(args, env);
    }
    if args.len() < 2 {
        return inert("PutAppend", args);
    }
    let Some(file) = as_str(&args[args.len() - 1]) else {
        return inert("PutAppend", args);
    };
    let path = resolve_path(env, &file);
    let mut out = String::new();
    for expr in &args[..args.len() - 1] {
        out.push_str(&input_form(expr));
        out.push('\n');
    }
    let mut handle = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| EvalError::Error(format!("PutAppend failed: {}", e)))?;
    handle
        .write_all(out.as_bytes())
        .map_err(|e| EvalError::Error(format!("PutAppend failed: {}", e)))?;
    Ok(Value::Null)
}

/// `FileType[path]` — `File`, `Directory`, `SymbolicLink`, or `None`.
pub fn builtin_file_type(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    let Some(p) = args.first().and_then(as_str) else {
        return inert("FileType", args);
    };
    let path = resolve_path(env, &p);
    match std::fs::symlink_metadata(&path) {
        Ok(meta) if meta.file_type().is_symlink() => Ok(Value::Symbol("SymbolicLink".to_string())),
        Ok(meta) if meta.is_dir() => Ok(Value::Symbol("Directory".to_string())),
        Ok(_) => Ok(Value::Symbol("File".to_string())),
        Err(_) => Ok(Value::Symbol("None".to_string())),
    }
}

/// `FileByteCount[path]` — size of the file in bytes.
pub fn builtin_file_byte_count(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    let Some(p) = args.first().and_then(as_str) else {
        return inert("FileByteCount", args);
    };
    let path = resolve_path(env, &p);
    match std::fs::metadata(&path) {
        Ok(meta) => Ok(Value::Integer(Integer::from(meta.len()))),
        Err(_) => Ok(Value::Symbol("$Failed".to_string())),
    }
}

/// `AbsoluteFileName[path]` / `AbsoluteFileName[path, base]` — absolute path,
/// resolved against the session directory (or `base`) and canonicalized.
pub fn builtin_absolute_file_name(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    let (p, base) = match args {
        [p] => (as_str(p), None),
        [p, b] => (as_str(p), as_str(b)),
        _ => return inert("AbsoluteFileName", args),
    };
    let Some(p) = p else {
        return inert("AbsoluteFileName", args);
    };
    let joined = match base {
        Some(base) if !Path::new(&p).is_absolute() => resolve_path(env, &base).join(Path::new(&p)),
        _ => resolve_path(env, &p),
    };
    let absolute = std::fs::canonicalize(&joined).unwrap_or(joined);
    Ok(Value::Str(absolute.to_string_lossy().into_owned()))
}

/// `RenameDirectory[src, dst]` — rename/move a directory, returning `dst`.
pub fn builtin_rename_directory(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    let (src, dst) = match args {
        [s, d] => (as_str(s), as_str(d)),
        _ => return inert("RenameDirectory", args),
    };
    let (Some(src), Some(dst)) = (src, dst) else {
        return inert("RenameDirectory", args);
    };
    std::fs::rename(resolve_path(env, &src), resolve_path(env, &dst))
        .map_err(|e| EvalError::Error(format!("RenameDirectory failed: {}", e)))?;
    Ok(Value::Str(dst))
}

/// `CopyDirectory[src, dst]` — recursively copy a directory, returning `dst`.
pub fn builtin_copy_directory(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    let (src, dst) = match args {
        [s, d] => (as_str(s), as_str(d)),
        _ => return inert("CopyDirectory", args),
    };
    let (Some(src), Some(dst)) = (src, dst) else {
        return inert("CopyDirectory", args);
    };
    copy_dir_all(&resolve_path(env, &src), &resolve_path(env, &dst))
        .map_err(|e| EvalError::Error(format!("CopyDirectory failed: {}", e)))?;
    Ok(Value::Str(dst))
}

/// Register the builtins implemented in this module.
pub fn register(env: &Env) {
    // Pure builtins.
    super::register_builtin(env, "Notebook", builtin_notebook);
    super::register_builtin(env, "NotebookObject", builtin_notebook_object);
    super::register_builtin(env, "CreateWindow", builtin_create_window);
    super::register_builtin(env, "CreateDocument", builtin_create_document);
    super::register_builtin(env, "NotebookSave", builtin_notebook_save);
    super::register_builtin(env, "Cell", builtin_cell);
    super::register_builtin(env, "BoxData", builtin_box_data);
    super::register_builtin(env, "TextCell", builtin_text_cell);
    super::register_builtin(env, "SetSelectedNotebook", builtin_set_selected_notebook);
    super::register_builtin(env, "CurrentNotebooks", builtin_current_notebooks);
    super::register_builtin(env, "StreamPosition", builtin_stream_position);
    super::register_builtin(env, "TimeUsed", builtin_time_used);
    super::register_builtin(env, "SessionTime", builtin_session_time);
    super::register_builtin(env, "TimeObject", builtin_time_object);
    super::register_builtin(env, "HoldAttributes", builtin_hold_attributes);

    // Env-aware builtins.
    // `SymbolName`/`Symbols`/`Contexts` are owned by `names::register`, which
    // runs later and is authoritative over these session helpers.
    super::register_builtin_env(env, "BinaryRead", builtin_binary_read);
    super::register_builtin_env(env, "BinaryWrite", builtin_binary_write);
    super::register_builtin_env(env, "ProtectedQ", builtin_protected_q);
    super::register_builtin_env(env, "Default", builtin_default);
    super::register_builtin_env(env, "PutAppend", builtin_put_append);
    super::register_builtin_env(env, "FileType", builtin_file_type);
    super::register_builtin_env(env, "FileByteCount", builtin_file_byte_count);
    super::register_builtin_env(env, "AbsoluteFileName", builtin_absolute_file_name);
    super::register_builtin_env(env, "RenameDirectory", builtin_rename_directory);
    super::register_builtin_env(env, "CopyDirectory", builtin_copy_directory);

    // HoldFirst keeps the leading symbol argument unevaluated.
    for name in ["SymbolName", "ProtectedQ", "Default"] {
        env.set_attributes(name, vec!["HoldFirst".to_string(), "Protected".to_string()]);
    }
    for name in [
        "Symbols",
        "Contexts",
        "Notebook",
        "NotebookObject",
        "CreateWindow",
        "CreateDocument",
        "NotebookSave",
        "BoxData",
        "TextCell",
        "SetSelectedNotebook",
        "CurrentNotebooks",
        "StreamPosition",
        "BinaryRead",
        "BinaryWrite",
        "TimeUsed",
        "SessionTime",
        "TimeObject",
        "HoldAttributes",
        "PutAppend",
        "FileType",
        "FileByteCount",
        "AbsoluteFileName",
        "RenameDirectory",
        "CopyDirectory",
    ] {
        if env.get_attributes(name).is_empty() {
            env.set_attributes(name, vec!["Protected".to_string()]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Expr;
    use rug::Integer;

    fn env_with_builtins() -> Env {
        let env = Env::new();
        crate::builtins::register_builtins(&env);
        env
    }

    #[test]
    fn test_create_document_head() {
        let doc = builtin_create_document(&[]).unwrap();
        assert!(matches!(&doc, Value::Call { head, .. } if head == "NotebookObject"));
    }

    #[test]
    fn test_cell_and_boxdata_symbolic() {
        assert_eq!(
            builtin_cell(&[s("hi")]).unwrap().to_string(),
            "Cell[\"hi\"]"
        );
        assert_eq!(
            builtin_box_data(&[int(1)]).unwrap().to_string(),
            "BoxData[1]"
        );
    }

    fn s(x: &str) -> Value {
        Value::Str(x.to_string())
    }

    fn int(n: i64) -> Value {
        Value::Integer(Integer::from(n))
    }

    fn run(input: &str, env: &Env) -> Value {
        let tokens = crate::lexer::tokenize(input).unwrap();
        let ast = crate::parser::parse(tokens).unwrap();
        crate::eval::eval_program(&ast, env).unwrap()
    }

    fn temp_path(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("syma_notebook_{}_{}", std::process::id(), tag))
    }

    #[test]
    fn test_symbol_name_plain_and_held() {
        let env = env_with_builtins();
        assert_eq!(
            builtin_symbol_name(&[Value::Symbol("x".into())], &env).unwrap(),
            s("x")
        );
        let held = Value::Pattern(Expr::Symbol("y".to_string()));
        assert_eq!(builtin_symbol_name(&[held], &env).unwrap(), s("y"));
        assert!(matches!(
            builtin_symbol_name(&[int(1)], &env).unwrap(),
            Value::Call { .. }
        ));
    }

    #[test]
    fn test_symbol_name_hold_first_via_eval() {
        let env = env_with_builtins();
        assert_eq!(run("x = 5; SymbolName[x]", &env), s("x"));
        // Context stripped and held calls resolved.
        assert_eq!(run("SymbolName[\"Foo`bar\"]", &env), s("bar"));
        assert_eq!(run("SymbolName[Symbol[\"abc\"]]", &env), s("abc"));
        assert_eq!(run("SymbolName[ToExpression[\"abc\"]]", &env), s("abc"));
        assert!(matches!(run("SymbolName[5]", &env), Value::Call { .. }));
    }

    #[test]
    fn test_symbols_lists_known_symbols() {
        let env = env_with_builtins();
        env.set("mySymbol_zzz".to_string(), Value::Null);
        let listed = builtin_symbols(&[], &env).unwrap();
        match listed {
            Value::List(items) => assert!(items.contains(&Value::Symbol("mySymbol_zzz".into()))),
            other => panic!("expected list, got {other:?}"),
        }
        // Mirror the session behaviour used elsewhere: defined => member.
        assert_eq!(
            run("x = 5; MemberQ[Symbols[], Symbol[\"x\"]]", &env),
            Value::Bool(true)
        );
        assert_eq!(
            run("x = 5; Clear[x]; MemberQ[Symbols[], Symbol[\"x\"]]", &env),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_contexts_nonempty() {
        let env = env_with_builtins();
        let listed = builtin_contexts(&[], &env).unwrap();
        match listed {
            Value::List(items) => assert!(items.contains(&s("Global`"))),
            other => panic!("expected list, got {other:?}"),
        }
        assert_eq!(
            run("Begin[\"Foo`\"]; MemberQ[Contexts[], \"Foo`\"]", &env),
            Value::Bool(true)
        );
    }

    #[test]
    fn test_notebook_builtins_are_symbolic() {
        let env = env_with_builtins();
        assert!(matches!(
            builtin_notebook(&[int(1)]).unwrap(),
            Value::Call { .. }
        ));
        assert!(matches!(
            builtin_notebook_object(&[int(1)]).unwrap(),
            Value::Call { .. }
        ));
        assert!(matches!(
            builtin_create_window(&[]).unwrap(),
            Value::Call { .. }
        ));
        let nb = Value::Call {
            head: "NotebookObject".to_string(),
            args: vec![int(1)],
        };
        assert_eq!(builtin_set_selected_notebook(&[nb.clone()]).unwrap(), nb);
        assert_eq!(builtin_current_notebooks(&[]).unwrap(), Value::List(vec![]));
        // Registered names stay symbolic through evaluation too.
        assert!(matches!(run("Notebook[{1, 2}]", &env), Value::Call { .. }));
    }

    #[test]
    fn test_binary_read_write_roundtrip() {
        let env = env_with_builtins();
        let path = temp_path("bin");
        let bytes = Value::List(vec![int(0), int(1), int(255), int(42)]);
        builtin_binary_write(&[s(path.to_str().unwrap()), bytes], &env).unwrap();
        let read = builtin_binary_read(&[s(path.to_str().unwrap())], &env).unwrap();
        assert_eq!(read, Value::List(vec![int(0), int(1), int(255), int(42)]));
        let head = builtin_binary_read(&[s(path.to_str().unwrap()), int(2)], &env).unwrap();
        assert_eq!(head, Value::List(vec![int(0), int(1)]));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_binary_read_missing_file() {
        let env = env_with_builtins();
        assert_eq!(
            builtin_binary_read(&[s("/no/such/file.bin")], &env).unwrap(),
            Value::Symbol("$Failed".to_string())
        );
    }

    #[test]
    fn test_time_builtins() {
        assert!(matches!(builtin_time_used(&[]).unwrap(), Value::Real(_)));
        assert!(matches!(builtin_session_time(&[]).unwrap(), Value::Real(_)));
        match builtin_time_object(&[]).unwrap() {
            Value::Object { class_name, .. } => assert_eq!(class_name, "TimeObject"),
            other => panic!("expected object, got {other:?}"),
        }
    }

    #[test]
    fn test_protected_q() {
        let env = env_with_builtins();
        env.set_attributes("foo", vec!["Protected".to_string()]);
        let held = Value::Pattern(Expr::Symbol("foo".to_string()));
        assert_eq!(
            builtin_protected_q(&[held], &env).unwrap(),
            Value::Bool(true)
        );
        let held2 = Value::Pattern(Expr::Symbol("bar".to_string()));
        assert_eq!(
            builtin_protected_q(&[held2], &env).unwrap(),
            Value::Bool(false)
        );
        assert_eq!(run("Protect[pp]; ProtectedQ[pp]", &env), Value::Bool(true));
    }

    #[test]
    fn test_hold_attributes_contains_holds() {
        match builtin_hold_attributes(&[]).unwrap() {
            Value::List(items) => {
                assert!(items.contains(&Value::Symbol("HoldFirst".to_string())));
                assert!(items.contains(&Value::Symbol("HoldAll".to_string())));
            }
            other => panic!("expected list, got {other:?}"),
        }
    }

    #[test]
    fn test_default_builtin_heads() {
        let env = env_with_builtins();
        assert_eq!(
            builtin_default(&[Value::Symbol("Plus".into())], &env).unwrap(),
            int(0)
        );
        assert_eq!(
            builtin_default(&[Value::Symbol("Times".into())], &env).unwrap(),
            int(1)
        );
        assert!(matches!(
            builtin_default(&[Value::Symbol("UnknownFn".into())], &env).unwrap(),
            Value::Call { .. }
        ));
    }

    #[test]
    fn test_put_append() {
        let env = env_with_builtins();
        let path = temp_path("putappend.syma");
        std::fs::write(&path, "").unwrap();
        builtin_put_append(&[int(1), s(path.to_str().unwrap())], &env).unwrap();
        builtin_put_append(&[int(2), s(path.to_str().unwrap())], &env).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "1\n2\n");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_file_type_byte_count() {
        let env = env_with_builtins();
        let file = temp_path("type.txt");
        let dir = temp_path("typedir");
        std::fs::write(&file, "hello").unwrap();
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(
            builtin_file_type(&[s(file.to_str().unwrap())], &env).unwrap(),
            Value::Symbol("File".to_string())
        );
        assert_eq!(
            builtin_file_type(&[s(dir.to_str().unwrap())], &env).unwrap(),
            Value::Symbol("Directory".to_string())
        );
        assert_eq!(
            builtin_file_type(&[s("/no/such/thing")], &env).unwrap(),
            Value::Symbol("None".to_string())
        );
        assert_eq!(
            builtin_file_byte_count(&[s(file.to_str().unwrap())], &env).unwrap(),
            int(5)
        );
        std::fs::remove_file(&file).ok();
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_absolute_file_name() {
        let env = env_with_builtins();
        let abs = builtin_absolute_file_name(&[s("Cargo.toml")], &env).unwrap();
        match abs {
            Value::Str(p) => assert!(Path::new(&p).is_absolute(), "not absolute: {p}"),
            other => panic!("expected string, got {other:?}"),
        }
    }

    #[test]
    fn test_rename_and_copy_directory() {
        let env = env_with_builtins();
        let src = temp_path("srcdir");
        let renamed = temp_path("renameddir");
        let copied = temp_path("copieddir");
        std::fs::create_dir_all(src.join("nested")).unwrap();
        std::fs::write(src.join("nested").join("f.txt"), "data").unwrap();

        builtin_copy_directory(
            &[s(src.to_str().unwrap()), s(copied.to_str().unwrap())],
            &env,
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(copied.join("nested").join("f.txt")).unwrap(),
            "data"
        );

        builtin_rename_directory(
            &[s(src.to_str().unwrap()), s(renamed.to_str().unwrap())],
            &env,
        )
        .unwrap();
        assert!(!src.exists());
        assert!(renamed.join("nested").join("f.txt").exists());

        std::fs::remove_dir_all(&renamed).ok();
        std::fs::remove_dir_all(&copied).ok();
    }

    #[test]
    fn test_wrong_args_are_inert() {
        let env = env_with_builtins();
        assert!(matches!(
            builtin_binary_read(&[], &env).unwrap(),
            Value::Call { .. }
        ));
        assert!(matches!(
            builtin_put_append(&[int(1)], &env).unwrap(),
            Value::Call { .. }
        ));
        assert!(matches!(
            builtin_symbols(&[s("a"), s("b")], &env).unwrap(),
            Value::Call { .. }
        ));
    }

    #[test]
    fn test_stream_position_tracks_read_cursor() {
        let env = env_with_builtins();
        let path = temp_path("streampos");
        std::fs::write(&path, "hello\nworld\n").unwrap();
        let handle =
            crate::builtins::streams::builtin_open_read(&[s(path.to_str().unwrap())], &env)
                .unwrap();
        assert_eq!(builtin_stream_position(&[handle.clone()]).unwrap(), int(0));

        crate::builtins::streams::builtin_read_line(&[handle.clone()], &env).unwrap();
        assert_eq!(builtin_stream_position(&[handle.clone()]).unwrap(), int(6));

        crate::builtins::streams::builtin_close(&[handle.clone()], &env).unwrap();
        // A closed handle has no cursor, so the stream object is echoed back.
        assert_eq!(builtin_stream_position(&[handle.clone()]).unwrap(), handle);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_stream_position_write_stream_echoes_object() {
        let env = env_with_builtins();
        let path = temp_path("streamposwrite");
        let handle =
            crate::builtins::streams::builtin_open_write(&[s(path.to_str().unwrap())], &env)
                .unwrap();
        // The handle is a live write stream; its position is reported numerically.
        assert!(matches!(
            builtin_stream_position(&[handle.clone()]).unwrap(),
            Value::Integer(_)
        ));
        crate::builtins::streams::builtin_close(&[handle], &env).unwrap();
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_stream_position_inapplicable_is_inert() {
        assert!(matches!(
            builtin_stream_position(&[]).unwrap(),
            Value::Call { .. }
        ));
        assert!(matches!(
            builtin_stream_position(&[s("not-a-stream")]).unwrap(),
            Value::Call { .. }
        ));
        assert!(matches!(
            builtin_stream_position(&[int(1), int(2)]).unwrap(),
            Value::Call { .. }
        ));
    }

    // ── Notebook / cell breadth ──────────────────────────────────────────

    #[test]
    fn test_text_cell_symbolic() {
        assert_eq!(
            builtin_text_cell(&[s("hello")]).unwrap().to_string(),
            "TextCell[\"hello\"]"
        );
        assert!(matches!(
            run("TextCell[\"a\"]", &env_with_builtins()),
            Value::Call { .. }
        ));
    }

    #[test]
    fn test_notebook_symbolic_roundtrip() {
        let env = env_with_builtins();
        assert!(matches!(
            run("Notebook[{Cell[1]}]", &env),
            Value::Call { .. }
        ));
        assert!(matches!(run("NotebookObject[1]", &env), Value::Call { .. }));
        assert!(matches!(run("BoxData[\"x\"]", &env), Value::Call { .. }));
    }

    #[test]
    fn test_create_document_and_notebook_save() {
        let env = env_with_builtins();
        let doc = run("doc = CreateDocument[BoxData[1]]", &env);
        assert!(matches!(&doc, Value::Call { head, .. } if head == "NotebookObject"));
        let path = temp_path("created.nb");
        run(
            &format!("NotebookSave[doc, \"{}\"]", path.to_str().unwrap()),
            &env,
        );
        let written = std::fs::read_to_string(&path).unwrap();
        assert!(written.contains("Notebook["), "written: {written}");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_binary_read_write_stream_via_notebook() {
        let env = env_with_builtins();
        let path = temp_path("nbstream.bin");
        let handle =
            crate::builtins::streams::builtin_open_write(&[s(path.to_str().unwrap())], &env)
                .unwrap();
        builtin_binary_write(
            &[handle.clone(), Value::List(vec![int(1), int(2), int(3)])],
            &env,
        )
        .unwrap();
        crate::builtins::streams::builtin_close(&[handle], &env).unwrap();
        let read = builtin_binary_read(&[s(path.to_str().unwrap())], &env).unwrap();
        assert_eq!(read, Value::List(vec![int(1), int(2), int(3)]));
        std::fs::remove_file(&path).ok();
    }
}
