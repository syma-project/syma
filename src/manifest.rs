/// `syma.toml` manifest — reading, locating, and dependency editing.
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// A native dependency entry in `syma.toml`.
#[derive(Debug, Clone)]
pub struct NativeDep {
    /// Path to the compiled native library (`.so` / `.dylib` / `.dll`).
    pub path: String,
}

/// Parsed representation of a `syma.toml` package manifest.
#[derive(Debug, Clone)]
pub struct Manifest {
    /// Absolute path to the `syma.toml` file.
    pub path: PathBuf,
    pub name: String,
    pub version: String,
    pub description: String,
    /// Explicit entry point (`entry` key in `[package]`).
    pub entry: Option<String>,
    pub dependencies: HashMap<String, String>,
    pub dev_dependencies: HashMap<String, String>,
    /// Native library dependencies (Tier 1/3).
    pub native_dependencies: HashMap<String, NativeDep>,
}

impl Manifest {
    /// Parse a `syma.toml` file.
    pub fn read(path: &Path) -> Result<Self, String> {
        let content = fs::read_to_string(path)
            .map_err(|e| format!("cannot read {}: {}", path.display(), e))?;

        let mut section = String::new();
        let mut name = String::new();
        let mut version = String::from("0.1.0");
        let mut description = String::new();
        let mut entry: Option<String> = None;
        let mut deps: HashMap<String, String> = HashMap::new();
        let mut dev_deps: HashMap<String, String> = HashMap::new();
        let mut native_deps: HashMap<String, NativeDep> = HashMap::new();

        for raw_line in content.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                section = line[1..line.len() - 1].to_string();
                continue;
            }
            if let Some((k, v)) = parse_kv(line) {
                match section.as_str() {
                    "package" => match k {
                        "name" => name = v.to_string(),
                        "version" => version = v.to_string(),
                        "description" => description = v.to_string(),
                        "entry" => entry = Some(v.to_string()),
                        _ => {}
                    },
                    "dependencies" => {
                        deps.insert(k.to_string(), v.to_string());
                    }
                    "dev-dependencies" => {
                        dev_deps.insert(k.to_string(), v.to_string());
                    }
                    "native-dependencies" => {
                        // Value is either `"path"` or `{ path = "path" }`.
                        let dep_path = if v.starts_with('{') {
                            // Inline table: extract `path = "..."`.
                            parse_inline_path(v).unwrap_or_else(|| v.to_string())
                        } else {
                            v.to_string()
                        };
                        native_deps.insert(k.to_string(), NativeDep { path: dep_path });
                    }
                    _ => {}
                }
            }
        }

        Ok(Manifest {
            path: path.to_path_buf(),
            name,
            version,
            description,
            entry,
            dependencies: deps,
            dev_dependencies: dev_deps,
            native_dependencies: native_deps,
        })
    }

    /// Walk up from `dir` until a `syma.toml` is found; return its path.
    pub fn find_from(dir: &Path) -> Option<PathBuf> {
        let mut current = dir.to_path_buf();
        loop {
            let candidate = current.join("syma.toml");
            if candidate.exists() {
                return Some(candidate);
            }
            if !current.pop() {
                return None;
            }
        }
    }

    /// Walk up from the current working directory.
    pub fn find() -> Option<PathBuf> {
        let cwd = std::env::current_dir().ok()?;
        Self::find_from(&cwd)
    }

    /// Package root — directory that contains `syma.toml`.
    pub fn root(&self) -> &Path {
        self.path.parent().unwrap_or(Path::new("."))
    }

    /// Resolved entry-point path (absolute).
    ///
    /// Uses the explicit `entry` key when present. Otherwise prefers
    /// `src/main.syma` and falls back to `src/lib.syma` (so `syma run` also
    /// works for library packages created with `syma new --lib`).
    pub fn entry_path(&self) -> PathBuf {
        if let Some(rel) = self.entry.as_deref() {
            return self.root().join(rel);
        }
        let main = self.root().join("src/main.syma");
        if main.exists() {
            return main;
        }
        let lib = self.root().join("src/lib.syma");
        if lib.exists() {
            return lib;
        }
        main
    }
}

// ── Dependency editing ────────────────────────────────────────────────────────

/// Add or update a dependency in `syma.toml`.
///
/// If the package is already listed, its version is updated in place.
/// If the section doesn't exist yet it is appended.
pub fn add_dep(manifest_path: &Path, name: &str, version: &str, dev: bool) -> Result<(), String> {
    let header = if dev {
        "[dev-dependencies]"
    } else {
        "[dependencies]"
    };
    let new_line = format!("{} = \"{}\"", name, version);

    let content = fs::read_to_string(manifest_path).map_err(|e| e.to_string())?;
    let mut lines: Vec<String> = content.lines().map(String::from).collect();

    // Try to update an existing entry in the section.
    let mut in_section = false;
    let mut updated_idx: Option<usize> = None;
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim();
        if t == header {
            in_section = true;
            continue;
        }
        if t.starts_with('[') {
            in_section = false;
        }
        if in_section
            && let Some(eq) = t.find('=')
            && t[..eq].trim() == name
        {
            updated_idx = Some(i);
            break;
        }
    }

    if let Some(i) = updated_idx {
        lines[i] = new_line;
    } else {
        // Find where to insert: right before the next section header after `header`,
        // or at the end of the section, or append the whole section.
        let mut section_end: Option<usize> = None;
        let mut found_section = false;
        for (i, line) in lines.iter().enumerate() {
            let t = line.trim();
            if t == header {
                found_section = true;
                continue;
            }
            if found_section && t.starts_with('[') {
                section_end = Some(i);
                break;
            }
        }

        if found_section {
            // Insert just before the next section (or at end of file).
            let insert_at = section_end.unwrap_or(lines.len());
            lines.insert(insert_at, new_line);
        } else {
            // Section absent — append it.
            if lines.last().is_some_and(|l| !l.is_empty()) {
                lines.push(String::new());
            }
            lines.push(header.to_string());
            lines.push(new_line);
        }
    }

    fs::write(manifest_path, lines.join("\n") + "\n").map_err(|e| e.to_string())
}

/// Remove a dependency from `syma.toml`.
///
/// Returns `true` if the entry was found and removed.
pub fn remove_dep(manifest_path: &Path, name: &str, dev: bool) -> Result<bool, String> {
    let header = if dev {
        "[dev-dependencies]"
    } else {
        "[dependencies]"
    };
    let content = fs::read_to_string(manifest_path).map_err(|e| e.to_string())?;
    let mut lines: Vec<String> = content.lines().map(String::from).collect();

    let mut in_section = false;
    let mut remove_idx: Option<usize> = None;
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim();
        if t == header {
            in_section = true;
            continue;
        }
        if t.starts_with('[') {
            in_section = false;
        }
        if in_section
            && let Some(eq) = t.find('=')
            && t[..eq].trim() == name
        {
            remove_idx = Some(i);
            break;
        }
    }

    if let Some(i) = remove_idx {
        lines.remove(i);
        fs::write(manifest_path, lines.join("\n") + "\n").map_err(|e| e.to_string())?;
        Ok(true)
    } else {
        Ok(false)
    }
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Extract the `path` value from a TOML inline table like `{ path = "vendor/foo.so" }`.
fn parse_inline_path(s: &str) -> Option<String> {
    // Find `path = "..."` inside the braces.
    let inner = s.trim_start_matches('{').trim_end_matches('}').trim();
    for part in inner.split(',') {
        let part = part.trim();
        if let Some((k, v)) = parse_kv(part)
            && k.trim() == "path"
        {
            return Some(v.to_string());
        }
    }
    None
}

/// Parse a TOML `key = "value"` (or `key = { ... }`) line.
/// Returns `(key, value)` with surrounding quotes stripped for plain strings
/// and any trailing inline comment removed.
fn parse_kv(line: &str) -> Option<(&str, &str)> {
    let eq = line.find('=')?;
    let key = line[..eq].trim();
    let raw = line[eq + 1..].trim();
    let val = strip_quoted(raw).unwrap_or(raw);
    Some((key, val))
}

/// If `raw` begins with `"` or `'`, return the contents up to the matching
/// closing quote (honouring `\"` escapes inside double quotes). Returns `None`
/// when `raw` is not a quoted string (e.g. an inline table or bare value).
fn strip_quoted(raw: &str) -> Option<&str> {
    let bytes = raw.as_bytes();
    let quote = *bytes.first()?;
    if quote != b'"' && quote != b'\'' {
        return None;
    }
    let mut i = 1;
    while i < bytes.len() {
        if quote == b'"' && bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if bytes[i] == quote {
            return Some(&raw[1..i]);
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TempPkg {
        root: PathBuf,
    }

    impl TempPkg {
        fn new(toml: &str) -> Self {
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!("syma_manifest_test_{n}"));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(root.join("src")).unwrap();
            fs::write(root.join("syma.toml"), toml).unwrap();
            TempPkg { root }
        }

        fn manifest(&self) -> Manifest {
            Manifest::read(&self.root.join("syma.toml")).unwrap()
        }
    }

    impl Drop for TempPkg {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn test_entry_path_explicit() {
        let pkg = TempPkg::new("[package]\nname = \"x\"\nentry = \"src/other.syma\"\n");
        assert!(pkg.manifest().entry_path().ends_with("src/other.syma"));
    }

    #[test]
    fn test_entry_path_defaults_to_lib_for_library_package() {
        let pkg = TempPkg::new("[package]\nname = \"x\"\n");
        // No main.syma, but a lib.syma exists → use lib.syma.
        fs::write(pkg.root.join("src/lib.syma"), "x = 1\n").unwrap();
        assert!(pkg.manifest().entry_path().ends_with("src/lib.syma"));
    }

    #[test]
    fn test_entry_path_prefers_main() {
        let pkg = TempPkg::new("[package]\nname = \"x\"\n");
        fs::write(pkg.root.join("src/main.syma"), "x = 1\n").unwrap();
        fs::write(pkg.root.join("src/lib.syma"), "x = 1\n").unwrap();
        assert!(pkg.manifest().entry_path().ends_with("src/main.syma"));
    }

    #[test]
    fn test_read_manifest_sections() {
        let pkg = TempPkg::new(
            "[package]\nname = \"demo\"\nversion = \"1.2.3\"\nentry = \"src/main.syma\"\n\n\
             [dependencies]\nfoo = \"1.0\"\nbar = \"2.0\"\n\n\
             [dev-dependencies]\ntestkit = \"0.5\"\n",
        );
        let m = pkg.manifest();
        assert_eq!(m.name, "demo");
        assert_eq!(m.version, "1.2.3");
        assert_eq!(m.entry.as_deref(), Some("src/main.syma"));
        assert_eq!(m.dependencies.get("foo").map(String::as_str), Some("1.0"));
        assert_eq!(m.dependencies.get("bar").map(String::as_str), Some("2.0"));
        assert_eq!(
            m.dev_dependencies.get("testkit").map(String::as_str),
            Some("0.5")
        );
    }

    #[test]
    fn test_read_manifest_trailing_comment() {
        let pkg =
            TempPkg::new("[package]\nname = \"demo\"\n\n[dependencies]\nfoo = \"1.0\" # pinned\n");
        let m = pkg.manifest();
        assert_eq!(m.dependencies.get("foo").map(String::as_str), Some("1.0"));
    }

    #[test]
    fn test_read_native_inline_dep() {
        let pkg = TempPkg::new(
            "[package]\nname = \"demo\"\n\n\
             [native-dependencies]\nlibfoo = { path = \"vendor/libfoo.so\" }\n",
        );
        let m = pkg.manifest();
        assert_eq!(
            m.native_dependencies.get("libfoo").map(|d| d.path.as_str()),
            Some("vendor/libfoo.so")
        );
    }

    #[test]
    fn test_add_dep_updates_existing() {
        let pkg = TempPkg::new("[package]\nname = \"x\"\n\n[dependencies]\nfoo = \"1.0\"\n");
        let mpath = pkg.root.join("syma.toml");
        add_dep(&mpath, "foo", "2.0", false).unwrap();
        let m = pkg.manifest();
        assert_eq!(m.dependencies.get("foo").map(String::as_str), Some("2.0"));
        assert_eq!(m.dependencies.len(), 1);
    }

    #[test]
    fn test_add_dep_inserts_into_existing_section_before_next() {
        let pkg = TempPkg::new(
            "[package]\nname = \"x\"\n\n[dependencies]\nfoo = \"1.0\"\n\n[dev-dependencies]\n",
        );
        let mpath = pkg.root.join("syma.toml");
        add_dep(&mpath, "bar", "3.0", false).unwrap();
        let m = pkg.manifest();
        assert_eq!(m.dependencies.get("bar").map(String::as_str), Some("3.0"));
        assert_eq!(m.dependencies.get("foo").map(String::as_str), Some("1.0"));
        // Must not have leaked into the dev section.
        assert!(m.dev_dependencies.is_empty());
    }

    #[test]
    fn test_add_dep_appends_missing_section() {
        let pkg = TempPkg::new("[package]\nname = \"x\"\n");
        let mpath = pkg.root.join("syma.toml");
        add_dep(&mpath, "bar", "3.0", true).unwrap();
        let m = pkg.manifest();
        assert_eq!(
            m.dev_dependencies.get("bar").map(String::as_str),
            Some("3.0")
        );
    }

    #[test]
    fn test_remove_dep_roundtrip() {
        let pkg = TempPkg::new(
            "[package]\nname = \"x\"\n\n[dependencies]\nfoo = \"1.0\"\nbar = \"2.0\"\n",
        );
        let mpath = pkg.root.join("syma.toml");
        assert!(remove_dep(&mpath, "foo", false).unwrap());
        let m = pkg.manifest();
        assert!(!m.dependencies.contains_key("foo"));
        assert_eq!(m.dependencies.get("bar").map(String::as_str), Some("2.0"));
        // Removing again reports false.
        assert!(!remove_dep(&mpath, "foo", false).unwrap());
    }

    #[test]
    fn test_strip_quoted() {
        assert_eq!(strip_quoted("\"hello\""), Some("hello"));
        assert_eq!(strip_quoted("'world'"), Some("world"));
        assert_eq!(strip_quoted("\"a\" # note"), Some("a"));
        assert_eq!(strip_quoted(r#""a\"b""#), Some(r#"a\"b"#));
        assert_eq!(strip_quoted("{ path = \"x\" }"), None);
        assert_eq!(strip_quoted("\"unterminated"), None);
    }

    #[test]
    fn test_find_from() {
        let pkg = TempPkg::new("[package]\nname = \"x\"\n");
        let nested = pkg.root.join("src");
        let found = Manifest::find_from(&nested).expect("should find manifest in ancestor");
        assert_eq!(found, pkg.root.join("syma.toml"));
    }
}
