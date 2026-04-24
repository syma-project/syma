use std::sync::Mutex;
/// Environment (scope) for variable bindings.
///
/// Supports nested scopes with lexical scoping rules.
/// Variables are looked up from innermost to outermost scope.
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use crate::value::{NativeLibHandle, Value};

/// Global module registry shared across all scopes in a session.
/// Maps module names (e.g. `"LinearAlgebra"`, `"Math.Stats"`) to their `Value::Module`.
pub type ModuleRegistry = Arc<Mutex<HashMap<String, Value>>>;

/// A scope frame containing variable bindings.
#[derive(Debug, Clone)]
pub struct Scope {
    bindings: HashMap<String, Value>,
    parent: Option<Arc<Mutex<Scope>>>,
}

/// Shared registry of opened native libraries, keyed by path.
/// Prevents double-dlopen when `LoadLibrary["x"]` is called twice.
pub type NativeLibRegistry = Arc<Mutex<HashMap<String, Arc<NativeLibHandle>>>>;

/// The evaluation environment, managing scopes.
#[derive(Debug, Clone)]
pub struct Env {
    /// Current scope chain.
    scope: Arc<Mutex<Scope>>,
    /// Module registry — shared (by `Arc` clone) across all child envs in a session.
    pub registry: ModuleRegistry,
    /// Directories searched when resolving `import Name` to a `.syma` file.
    pub search_paths: Arc<Mutex<Vec<PathBuf>>>,
    /// Native library handles shared across all child envs in a session.
    pub native_libs: NativeLibRegistry,
}

impl Scope {
    pub fn new(parent: Option<Arc<Mutex<Scope>>>) -> Self {
        Scope {
            bindings: HashMap::new(),
            parent,
        }
    }

    pub fn get(&self, name: &str) -> Option<Value> {
        if let Some(val) = self.bindings.get(name) {
            Some(val.clone())
        } else if let Some(ref parent) = self.parent {
            parent.lock().unwrap().get(name)
        } else {
            None
        }
    }

    pub fn set(&mut self, name: String, value: Value) {
        self.bindings.insert(name, value);
    }

    #[allow(dead_code)]
    pub fn set_local(&mut self, name: String, value: Value) {
        self.bindings.insert(name, value);
    }
}

impl Env {
    /// Create a new environment with a global scope.
    pub fn new() -> Self {
        Env {
            scope: Arc::new(Mutex::new(Scope::new(None))),
            registry: Arc::new(Mutex::new(HashMap::new())),
            search_paths: Arc::new(Mutex::new(vec![PathBuf::from(".")])),
            native_libs: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Create a child environment (new scope, shared registry and search paths).
    pub fn child(&self) -> Self {
        Env {
            scope: Arc::new(Mutex::new(Scope::new(Some(self.scope.clone())))),
            registry: self.registry.clone(),
            search_paths: self.search_paths.clone(),
            native_libs: self.native_libs.clone(),
        }
    }

    /// Register a module in the session-wide registry.
    pub fn register_module(&self, name: String, module: Value) {
        self.registry.lock().unwrap().insert(name, module);
    }

    /// Look up a module by its qualified name (e.g. `"LinearAlgebra"`).
    pub fn get_module(&self, name: &str) -> Option<Value> {
        self.registry.lock().unwrap().get(name).cloned()
    }

    /// Register a loaded native library handle under its path key.
    pub fn register_native_lib(&self, path: String, handle: Arc<NativeLibHandle>) {
        self.native_libs.lock().unwrap().insert(path, handle);
    }

    /// Look up a previously loaded native library by path.
    pub fn get_native_lib(&self, path: &str) -> Option<Arc<NativeLibHandle>> {
        self.native_libs.lock().unwrap().get(path).cloned()
    }

    /// Prepend a directory to the module search path.
    pub fn add_search_path(&self, path: PathBuf) {
        self.search_paths.lock().unwrap().insert(0, path);
    }

    /// Look up a variable by name.
    pub fn get(&self, name: &str) -> Option<Value> {
        self.scope.lock().unwrap().get(name)
    }

    /// Set a variable in the current scope.
    pub fn set(&self, name: String, value: Value) {
        self.scope.lock().unwrap().set(name, value);
    }

    /// Set a variable in the current (local) scope only.
    #[allow(dead_code)]
    pub fn set_local(&self, name: String, value: Value) {
        self.scope.lock().unwrap().set_local(name, value);
    }

    /// Check if a variable exists in the current scope (not parents).
    #[allow(dead_code)]
    pub fn has_local(&self, name: &str) -> bool {
        self.scope.lock().unwrap().bindings.contains_key(name)
    }

    /// Return all bindings in the current scope (not parents), as a Vec of (name, value) pairs.
    #[allow(dead_code)]
    pub fn bindings(&self) -> Vec<(String, Value)> {
        self.scope
            .lock()
            .unwrap()
            .bindings
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    /// Return all bindings including parent scopes, as a Vec of (name, value) pairs.
    /// Later entries shadow earlier ones.
    pub fn all_bindings(&self) -> Vec<(String, Value)> {
        let mut result = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let mut scope_opt = Some(self.scope.clone());
        while let Some(scope) = scope_opt {
            let s = scope.lock().unwrap();
            for (k, v) in &s.bindings {
                if seen.insert(k.clone()) {
                    result.push((k.clone(), v.clone()));
                }
            }
            scope_opt = s.parent.clone();
        }
        result
    }
}

impl Default for Env {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rug::Integer;

    #[test]
    fn test_new_env_is_empty() {
        let env = Env::new();
        assert!(env.get("x").is_none());
    }

    #[test]
    fn test_set_and_get() {
        let env = Env::new();
        env.set("x".to_string(), Value::Integer(Integer::from(42)));
        assert_eq!(env.get("x"), Some(Value::Integer(Integer::from(42))));
    }

    #[test]
    fn test_set_overwrites() {
        let env = Env::new();
        env.set("x".to_string(), Value::Integer(Integer::from(1)));
        env.set("x".to_string(), Value::Integer(Integer::from(2)));
        assert_eq!(env.get("x"), Some(Value::Integer(Integer::from(2))));
    }

    #[test]
    fn test_get_unknown_returns_none() {
        let env = Env::new();
        assert_eq!(env.get("nonexistent"), None);
    }

    #[test]
    fn test_child_inherits_parent() {
        let parent = Env::new();
        parent.set("x".to_string(), Value::Integer(Integer::from(42)));

        let child = parent.child();
        assert_eq!(child.get("x"), Some(Value::Integer(Integer::from(42))));
    }

    #[test]
    fn test_child_can_shadow_parent() {
        let parent = Env::new();
        parent.set("x".to_string(), Value::Integer(Integer::from(1)));

        let child = parent.child();
        child.set("x".to_string(), Value::Integer(Integer::from(2)));

        assert_eq!(child.get("x"), Some(Value::Integer(Integer::from(2))));
        // Parent is not affected
        assert_eq!(parent.get("x"), Some(Value::Integer(Integer::from(1))));
    }

    #[test]
    fn test_child_set_does_not_affect_parent() {
        let parent = Env::new();
        let child = parent.child();
        child.set("y".to_string(), Value::Str("hello".to_string()));

        assert_eq!(child.get("y"), Some(Value::Str("hello".to_string())));
        assert_eq!(parent.get("y"), None);
    }

    #[test]
    fn test_has_local() {
        let env = Env::new();
        assert!(!env.has_local("x"));
        env.set("x".to_string(), Value::Integer(Integer::from(1)));
        assert!(env.has_local("x"));
    }

    #[test]
    fn test_has_local_does_not_check_parent() {
        let parent = Env::new();
        parent.set("x".to_string(), Value::Integer(Integer::from(1)));

        let child = parent.child();
        assert!(!child.has_local("x"));
        assert!(child.get("x").is_some()); // but can still get it
    }

    #[test]
    fn test_nested_scopes() {
        let global = Env::new();
        global.set("a".to_string(), Value::Integer(Integer::from(1)));

        let outer = global.child();
        outer.set("b".to_string(), Value::Integer(Integer::from(2)));

        let inner = outer.child();
        inner.set("c".to_string(), Value::Integer(Integer::from(3)));

        assert_eq!(inner.get("a"), Some(Value::Integer(Integer::from(1))));
        assert_eq!(inner.get("b"), Some(Value::Integer(Integer::from(2))));
        assert_eq!(inner.get("c"), Some(Value::Integer(Integer::from(3))));
    }

    #[test]
    fn test_default_trait() {
        let env = Env::default();
        assert!(env.get("x").is_none());
    }
}
