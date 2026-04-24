use crate::value::{EvalError, Value};
use rug::Integer;
use std::path::{Component, Path, PathBuf};

/// Extract a single string argument.
fn arg_string(args: &[Value], index: usize, name: &str) -> Result<String, EvalError> {
    match args.get(index) {
        Some(Value::Str(s)) => Ok(s.clone()),
        Some(other) => Err(EvalError::TypeError {
            expected: "String".to_string(),
            got: other.type_name().to_string(),
        }),
        None => Err(EvalError::Error(format!(
            "{} requires more arguments",
            name
        ))),
    }
}

/// Extract a single integer argument as i64.
fn arg_int(args: &[Value], index: usize, name: &str) -> Result<i64, EvalError> {
    match args.get(index) {
        Some(Value::Integer(n)) => n
            .to_i64()
            .ok_or_else(|| EvalError::Error(format!("{}: integer out of range", name))),
        Some(other) => Err(EvalError::TypeError {
            expected: "Integer".to_string(),
            got: other.type_name().to_string(),
        }),
        None => Err(EvalError::Error(format!(
            "{} requires more arguments",
            name
        ))),
    }
}

/// Collect path components (excluding RootDir prefix) as strings.
fn path_components(p: &Path) -> Vec<String> {
    p.components()
        .filter_map(|c| match c {
            Component::Prefix(p) => Some(p.as_os_str().to_string_lossy().into_owned()),
            Component::RootDir => None, // skip leading "/"
            Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            Component::CurDir => Some(".".to_string()),
            Component::ParentDir => Some("..".to_string()),
        })
        .collect()
}

/// Reconstruct a path string from components, prepending "/" if the original was absolute.
fn components_to_path(comps: &[String], absolute: bool) -> String {
    let joined = comps.join(std::path::MAIN_SEPARATOR_STR);
    if absolute {
        format!("{}{}", std::path::MAIN_SEPARATOR, joined)
    } else {
        joined
    }
}

// ── FileNameSplit ──

/// FileNameSplit["path"] splits a file name into its path components.
pub fn builtin_file_name_split(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "FileNameSplit requires exactly 1 argument".to_string(),
        ));
    }
    let s = arg_string(args, 0, "FileNameSplit")?;
    let p = Path::new(&s);
    let comps: Vec<Value> = path_components(p).into_iter().map(Value::Str).collect();
    Ok(Value::List(comps))
}

// ── FileNameJoin ──

/// FileNameJoin[{"a", "b", "c"}] joins path components into a file name.
pub fn builtin_file_name_join(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "FileNameJoin requires exactly 1 argument (a list)".to_string(),
        ));
    }
    let list = match &args[0] {
        Value::List(v) => v,
        _ => {
            return Err(EvalError::TypeError {
                expected: "List".to_string(),
                got: args[0].type_name().to_string(),
            });
        }
    };
    if list.is_empty() {
        return Ok(Value::Str(String::new()));
    }
    let mut pb = PathBuf::new();
    for (i, item) in list.iter().enumerate() {
        match item {
            Value::Str(s) => {
                if i == 0 && s.starts_with(std::path::MAIN_SEPARATOR) {
                    pb.push(std::path::MAIN_SEPARATOR_STR);
                }
                pb.push(s);
            }
            other => {
                return Err(EvalError::TypeError {
                    expected: "String".to_string(),
                    got: other.type_name().to_string(),
                });
            }
        }
    }
    Ok(Value::Str(pb.to_string_lossy().into_owned()))
}

// ── FileNameTake ──

/// FileNameTake["path", n] gives the last n components of the path.
pub fn builtin_file_name_take(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "FileNameTake requires exactly 2 arguments".to_string(),
        ));
    }
    let s = arg_string(args, 0, "FileNameTake")?;
    let n = arg_int(args, 1, "FileNameTake")?;
    let p = Path::new(&s);
    let absolute = p.is_absolute();
    let comps = path_components(p);
    let n_usize = n.max(0) as usize;
    let start = comps.len().saturating_sub(n_usize);
    let taken = &comps[start..];
    Ok(Value::Str(components_to_path(taken, absolute)))
}

// ── FileNameDrop ──

/// FileNameDrop["path", n] gives the path with the last n components removed.
pub fn builtin_file_name_drop(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "FileNameDrop requires exactly 2 arguments".to_string(),
        ));
    }
    let s = arg_string(args, 0, "FileNameDrop")?;
    let n = arg_int(args, 1, "FileNameDrop")?;
    let p = Path::new(&s);
    let absolute = p.is_absolute();
    let comps = path_components(p);
    let n_usize = n.max(0) as usize;
    let end = comps.len().saturating_sub(n_usize);
    let kept = &comps[..end];
    Ok(Value::Str(components_to_path(kept, absolute)))
}

// ── FileBaseName ──

/// FileBaseName["path"] gives the file name without its extension.
pub fn builtin_file_base_name(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "FileBaseName requires exactly 1 argument".to_string(),
        ));
    }
    let s = arg_string(args, 0, "FileBaseName")?;
    let p = Path::new(&s);
    match p.file_stem() {
        Some(stem) => Ok(Value::Str(stem.to_string_lossy().into_owned())),
        None => Ok(Value::Str(String::new())),
    }
}

// ── FileExtension ──

/// FileExtension["path"] gives the file extension.
pub fn builtin_file_extension(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "FileExtension requires exactly 1 argument".to_string(),
        ));
    }
    let s = arg_string(args, 0, "FileExtension")?;
    let p = Path::new(&s);
    match p.extension() {
        Some(ext) => Ok(Value::Str(ext.to_string_lossy().into_owned())),
        None => Ok(Value::Str(String::new())),
    }
}

// ── FileNameDepth ──

/// FileNameDepth["path"] gives the number of path components.
pub fn builtin_file_name_depth(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "FileNameDepth requires exactly 1 argument".to_string(),
        ));
    }
    let s = arg_string(args, 0, "FileNameDepth")?;
    let p = Path::new(&s);
    let depth = path_components(p).len();
    Ok(Value::Integer(Integer::from(depth as i64)))
}

// ── DirectoryName ──

/// DirectoryName["path"] gives the directory portion of the path.
pub fn builtin_directory_name(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "DirectoryName requires exactly 1 argument".to_string(),
        ));
    }
    let s = arg_string(args, 0, "DirectoryName")?;
    let p = Path::new(&s);
    match p.parent() {
        Some(parent) => Ok(Value::Str(parent.to_string_lossy().into_owned())),
        None => Ok(Value::Str(String::new())),
    }
}

// ── ParentDirectory ──

/// ParentDirectory["path"] gives the parent directory (same as DirectoryName).
pub fn builtin_parent_directory(args: &[Value]) -> Result<Value, EvalError> {
    builtin_directory_name(args)
}

// ── ExpandFileName ──

/// ExpandFileName["path"] resolves the path to an absolute path.
pub fn builtin_expand_file_name(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "ExpandFileName requires exactly 1 argument".to_string(),
        ));
    }
    let s = arg_string(args, 0, "ExpandFileName")?;
    // Try canonicalize first; fall back to std::env::current_dir + join
    match std::fs::canonicalize(&s) {
        Ok(p) => Ok(Value::Str(p.to_string_lossy().into_owned())),
        Err(_) => {
            let p = Path::new(&s);
            if p.is_absolute() {
                Ok(Value::Str(s))
            } else {
                match std::env::current_dir() {
                    Ok(cwd) => Ok(Value::Str(cwd.join(p).to_string_lossy().into_owned())),
                    Err(e) => Err(EvalError::Error(format!(
                        "ExpandFileName: cannot resolve path: {}",
                        e
                    ))),
                }
            }
        }
    }
}

// ── FileExistsQ ──

/// FileExistsQ["path"] returns True if the file exists.
pub fn builtin_file_exists_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "FileExistsQ requires exactly 1 argument".to_string(),
        ));
    }
    let s = arg_string(args, 0, "FileExistsQ")?;
    Ok(Value::Bool(Path::new(&s).exists()))
}

// ── DirectoryQ ──

/// DirectoryQ["path"] returns True if the path is a directory.
pub fn builtin_directory_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "DirectoryQ requires exactly 1 argument".to_string(),
        ));
    }
    let s = arg_string(args, 0, "DirectoryQ")?;
    Ok(Value::Bool(Path::new(&s).is_dir()))
}

// ── FileNames ──

/// Simple glob matching: supports `*` (matches any sequence) and `?` (matches one char).
fn glob_match(pattern: &str, name: &str) -> bool {
    let p = pattern.as_bytes();
    let n = name.as_bytes();
    let plen = p.len();
    let nlen = n.len();

    // Simple DP approach
    let mut dp = vec![vec![false; nlen + 1]; plen + 1];
    dp[0][0] = true;

    // Handle leading * patterns
    for i in 1..=plen {
        if p[i - 1] == b'*' {
            dp[i][0] = dp[i - 1][0];
        }
    }

    for i in 1..=plen {
        for j in 1..=nlen {
            match p[i - 1] {
                b'*' => dp[i][j] = dp[i - 1][j] || dp[i][j - 1],
                b'?' => dp[i][j] = dp[i - 1][j - 1],
                _ => dp[i][j] = dp[i - 1][j - 1] && p[i - 1] == n[j - 1],
            }
        }
    }

    dp[plen][nlen]
}

/// FileNames[] lists files in the current directory.
/// FileNames["pattern"] lists files matching pattern in the current directory.
/// FileNames["pattern", {"dir1", "dir2"}] lists files matching pattern in given directories.
pub fn builtin_file_names(args: &[Value]) -> Result<Value, EvalError> {
    let pattern;
    let dirs: Vec<String>;

    match args.len() {
        0 => {
            pattern = "*".to_string();
            dirs = vec![".".to_string()];
        }
        1 => {
            pattern = arg_string(args, 0, "FileNames")?;
            dirs = vec![".".to_string()];
        }
        2 => {
            pattern = arg_string(args, 0, "FileNames")?;
            dirs = match &args[1] {
                Value::List(v) => {
                    let mut result = Vec::new();
                    for item in v {
                        match item {
                            Value::Str(s) => result.push(s.clone()),
                            other => {
                                return Err(EvalError::TypeError {
                                    expected: "String".to_string(),
                                    got: other.type_name().to_string(),
                                });
                            }
                        }
                    }
                    result
                }
                Value::Str(s) => vec![s.clone()],
                other => {
                    return Err(EvalError::TypeError {
                        expected: "List or String".to_string(),
                        got: other.type_name().to_string(),
                    });
                }
            };
        }
        _ => {
            return Err(EvalError::Error(
                "FileNames requires 0, 1, or 2 arguments".to_string(),
            ));
        }
    }

    let mut results = Vec::new();
    for dir in &dirs {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => continue, // skip non-existent directories
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if glob_match(&pattern, &name) {
                results.push(Value::Str(name));
            }
        }
    }
    results.sort_by(|a, b| a.to_string().cmp(&b.to_string()));
    Ok(Value::List(results))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn string(s: &str) -> Value {
        Value::Str(s.to_string())
    }
    fn int(n: i64) -> Value {
        Value::Integer(Integer::from(n))
    }
    fn list(v: Vec<Value>) -> Value {
        Value::List(v)
    }

    #[test]
    fn test_file_name_split() {
        let result = builtin_file_name_split(&[string("/home/user/doc.txt")]).unwrap();
        assert_eq!(
            result,
            list(vec![string("home"), string("user"), string("doc.txt")])
        );

        let result = builtin_file_name_split(&[string("a/b/c")]).unwrap();
        assert_eq!(result, list(vec![string("a"), string("b"), string("c")]));
    }

    #[test]
    fn test_file_name_join() {
        let result = builtin_file_name_join(&[list(vec![
            string("home"),
            string("user"),
            string("doc.txt"),
        ])])
        .unwrap();
        let sep = std::path::MAIN_SEPARATOR_STR;
        assert_eq!(result, string(&format!("home{}user{}doc.txt", sep, sep)));
    }

    #[test]
    fn test_file_name_take() {
        let result = builtin_file_name_take(&[string("/a/b/c/d.txt"), int(2)]).unwrap();
        assert_eq!(result, string("/c/d.txt"));
    }

    #[test]
    fn test_file_name_drop() {
        let result = builtin_file_name_drop(&[string("/a/b/c/d.txt"), int(2)]).unwrap();
        assert_eq!(result, string("/a/b"));
    }

    #[test]
    fn test_file_base_name() {
        assert_eq!(
            builtin_file_base_name(&[string("/home/user/doc.txt")]).unwrap(),
            string("doc")
        );
        assert_eq!(
            builtin_file_base_name(&[string("archive.tar.gz")]).unwrap(),
            string("archive.tar")
        );
    }

    #[test]
    fn test_file_extension() {
        assert_eq!(
            builtin_file_extension(&[string("/home/user/doc.txt")]).unwrap(),
            string("txt")
        );
        assert_eq!(
            builtin_file_extension(&[string("noext")]).unwrap(),
            string("")
        );
    }

    #[test]
    fn test_file_name_depth() {
        assert_eq!(
            builtin_file_name_depth(&[string("/a/b/c")]).unwrap(),
            int(3)
        );
        assert_eq!(
            builtin_file_name_depth(&[string("relative/path")]).unwrap(),
            int(2)
        );
    }

    #[test]
    fn test_directory_name() {
        assert_eq!(
            builtin_directory_name(&[string("/home/user/doc.txt")]).unwrap(),
            string("/home/user")
        );
        assert_eq!(
            builtin_directory_name(&[string("doc.txt")]).unwrap(),
            string("")
        );
    }

    #[test]
    fn test_expand_file_name_relative() {
        let result = builtin_expand_file_name(&[string("Cargo.toml")]).unwrap();
        if let Value::Str(s) = &result {
            assert!(s.ends_with("Cargo.toml"));
            assert!(Path::new(s).is_absolute());
        } else {
            panic!("expected string");
        }
    }

    #[test]
    fn test_file_exists_q() {
        // Cargo.toml should exist in the project root
        assert_eq!(
            builtin_file_exists_q(&[string("Cargo.toml")]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_file_exists_q(&[string("nonexistent_file_xyz")]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_directory_q() {
        assert_eq!(
            builtin_directory_q(&[string("src")]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_directory_q(&[string("Cargo.toml")]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_glob_match() {
        assert!(glob_match("*.rs", "main.rs"));
        assert!(glob_match("*.rs", ".rs"));
        assert!(!glob_match("*.rs", "main.txt"));
        assert!(glob_match("?.rs", "a.rs"));
        assert!(!glob_match("?.rs", "ab.rs"));
        assert!(glob_match("*", "anything"));
        assert!(glob_match("test_*_v2.*", "test_alpha_v2.rs"));
    }

    #[test]
    fn test_file_names() {
        // List all files in src directory
        let result = builtin_file_names(&[string("*"), list(vec![string("src")])]).unwrap();
        if let Value::List(items) = result {
            // src/ should contain main.rs or lib.rs
            assert!(!items.is_empty());
        } else {
            panic!("expected list");
        }
    }
}
