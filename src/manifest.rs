/// `syma.toml` manifest — reading, locating, and dependency editing.
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

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
    /// Defaults to `src/main.syma` when `entry` is not set.
    pub fn entry_path(&self) -> PathBuf {
        let rel = self.entry.as_deref().unwrap_or("src/main.syma");
        self.root().join(rel)
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
        if in_section {
            if let Some(eq) = t.find('=') {
                if t[..eq].trim() == name {
                    updated_idx = Some(i);
                    break;
                }
            }
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
            if lines.last().map_or(false, |l| !l.is_empty()) {
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
        if in_section {
            if let Some(eq) = t.find('=') {
                if t[..eq].trim() == name {
                    remove_idx = Some(i);
                    break;
                }
            }
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

/// Parse a TOML `key = "value"` (or `key = { ... }`) line.
/// Returns `(key, value)` with surrounding quotes stripped for plain strings.
fn parse_kv(line: &str) -> Option<(&str, &str)> {
    let eq = line.find('=')?;
    let key = line[..eq].trim();
    let raw = line[eq + 1..].trim();
    let val = if raw.starts_with('"') && raw.ends_with('"') && raw.len() >= 2 {
        &raw[1..raw.len() - 1]
    } else {
        raw
    };
    Some((key, val))
}
