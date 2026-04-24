/// Environment (scope) for variable bindings.
///
/// Supports nested scopes with lexical scoping rules.
/// Variables are looked up from innermost to outermost scope.
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;

use crate::value::{EvalError, NativeLibHandle, Value};

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

/// A lazy-loading provider: when an undefined symbol is first used in a call,
/// the provider is fired once, then removed from the registry.
pub enum LazyProvider {
    /// Load a `.syma` file and evaluate it to define the symbol.
    /// The path is resolved against the environment's `search_paths`.
    File(PathBuf),
    /// Execute an arbitrary Rust closure. Must return the value to install
    /// for the registered symbol on success.
    #[allow(clippy::type_complexity)]
    Custom(Arc<dyn Fn(&Env) -> Result<Value, EvalError> + Send + Sync>),
}

impl std::fmt::Debug for LazyProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LazyProvider::File(p) => f.debug_tuple("File").field(p).finish(),
            LazyProvider::Custom(_) => f.debug_tuple("Custom").field(&"...").finish(),
        }
    }
}

impl Clone for LazyProvider {
    fn clone(&self) -> Self {
        match self {
            LazyProvider::File(p) => LazyProvider::File(p.clone()),
            LazyProvider::Custom(f) => LazyProvider::Custom(f.clone()),
        }
    }
}

/// The evaluation environment, managing scopes.
#[derive(Debug, Clone)]
pub struct Env {
    /// Current scope chain.
    scope: Arc<Mutex<Scope>>,
    /// Always points to the outermost (root) scope.
    root_scope: Arc<Mutex<Scope>>,
    /// Module registry — shared (by `Arc` clone) across all child envs in a session.
    pub registry: ModuleRegistry,
    /// Directories searched when resolving `import Name` to a `.syma` file.
    pub search_paths: Arc<Mutex<Vec<PathBuf>>>,
    /// Native library handles shared across all child envs in a session.
    pub native_libs: NativeLibRegistry,
    /// Per-symbol attributes (e.g., Listable, Flat, Orderless, HoldAll, Protected).
    /// Shared across all child envs in a session.
    pub attributes: Arc<Mutex<HashMap<String, Vec<String>>>>,
    /// Lazy-loading providers — registered per symbol, fired once on first use.
    /// Shared across all child envs in a session.
    pub lazy_providers: Arc<Mutex<HashMap<String, LazyProvider>>>,
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

    pub fn set_local(&mut self, name: String, value: Value) {
        self.bindings.insert(name, value);
    }
}

impl Env {
    /// Create a new environment with a global scope.
    pub fn new() -> Self {
        let scope = Arc::new(Mutex::new(Scope::new(None)));
        Env {
            scope: scope.clone(),
            root_scope: scope,
            registry: Arc::new(Mutex::new(HashMap::new())),
            search_paths: Arc::new(Mutex::new(vec![PathBuf::from(".")])),
            native_libs: Arc::new(Mutex::new(HashMap::new())),
            attributes: Arc::new(Mutex::new(HashMap::new())),
            lazy_providers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Create a child environment (new scope, shared registry, search paths, and attributes).
    pub fn child(&self) -> Self {
        Env {
            scope: Arc::new(Mutex::new(Scope::new(Some(self.scope.clone())))),
            root_scope: self.root_scope.clone(),
            registry: self.registry.clone(),
            search_paths: self.search_paths.clone(),
            native_libs: self.native_libs.clone(),
            attributes: self.attributes.clone(),
            lazy_providers: self.lazy_providers.clone(),
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

    /// Register a lazy provider for a symbol.
    ///
    /// When the symbol is first encountered as an undefined symbol in a call,
    /// the provider fires. After loading, the provider is removed (one-shot)
    /// and the loaded value is installed in the root scope.
    pub fn register_lazy_provider(&self, name: &str, provider: LazyProvider) {
        self.lazy_providers
            .lock()
            .unwrap()
            .insert(name.to_string(), provider);
    }

    /// Return an `Env` whose scope is the root (outermost) scope.
    /// Useful for lazy providers that need to load definitions into root scope.
    pub fn root_env(&self) -> Self {
        Env {
            scope: self.root_scope.clone(),
            root_scope: self.root_scope.clone(),
            registry: self.registry.clone(),
            search_paths: self.search_paths.clone(),
            native_libs: self.native_libs.clone(),
            attributes: self.attributes.clone(),
            lazy_providers: self.lazy_providers.clone(),
        }
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
    pub fn set_local(&self, name: String, value: Value) {
        self.scope.lock().unwrap().set_local(name, value);
    }

    /// Check if a variable exists in the current scope (not parents).
    pub fn has_local(&self, name: &str) -> bool {
        self.scope.lock().unwrap().bindings.contains_key(name)
    }

    /// Return all bindings in the current scope (not parents), as a Vec of (name, value) pairs.
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

    // ── Attribute system ──

    /// Set attributes for a symbol.
    pub fn set_attributes(&self, name: &str, attrs: Vec<String>) {
        self.attributes
            .lock()
            .unwrap()
            .insert(name.to_string(), attrs);
    }

    /// Get attributes for a symbol. Returns an empty vec if none set.
    pub fn get_attributes(&self, name: &str) -> Vec<String> {
        self.attributes
            .lock()
            .unwrap()
            .get(name)
            .cloned()
            .unwrap_or_default()
    }

    /// Check if a symbol has a specific attribute.
    pub fn has_attribute(&self, name: &str, attr: &str) -> bool {
        self.get_attributes(name).iter().any(|a| a == attr)
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

    // ── Lazy provider tests ──

    #[test]
    fn test_register_lazy_provider() {
        let env = Env::new();
        env.register_lazy_provider("Foo", LazyProvider::File(PathBuf::from("test.syma")));
        let providers = env.lazy_providers.lock().unwrap();
        assert!(providers.contains_key("Foo"));
    }

    #[test]
    fn test_lazy_provider_one_shot() {
        let env = Env::new();
        env.register_lazy_provider("Bar", LazyProvider::File(PathBuf::from("test.syma")));
        // Remove it (simulates firing)
        let removed = env.lazy_providers.lock().unwrap().remove("Bar");
        assert!(removed.is_some());
        // After removal, it should be gone
        let removed2 = env.lazy_providers.lock().unwrap().remove("Bar");
        assert!(removed2.is_none());
    }

    #[test]
    fn test_root_env() {
        let env = Env::new();
        env.set("x".to_string(), Value::Integer(Integer::from(42)));

        let child = env.child();
        child.set("y".to_string(), Value::Str("child".to_string()));

        // root_env still sees the root scope
        let root = child.root_env();
        assert_eq!(root.get("x"), Some(Value::Integer(Integer::from(42))));
        // y was set in child scope, not root scope
        assert_eq!(root.get("y"), None);
    }

    #[test]
    fn test_root_env_set_visible_through_child() {
        let env = Env::new();
        let root = env.root_env();
        root.set("z".to_string(), Value::Integer(Integer::from(99)));

        // Child envs see the root-set value
        let child = env.child();
        assert_eq!(child.get("z"), Some(Value::Integer(Integer::from(99))));
    }
}
