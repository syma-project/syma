/// Environment (scope) for variable bindings.
///
/// Supports nested scopes with lexical scoping rules.
/// Variables are looked up from innermost to outermost scope.
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;

use crate::ast::Expr;
use crate::value::{EvalError, NativeLibHandle, Value};

/// A stored subvalue definition (`f[outer...][inner...] := rhs`).
///
/// Kept separate from [`crate::value::FunctionDef`] so that `SubValues[f]` can
/// distinguish `f[a][b]` from a plain downvalue `f[a, b]`.
#[derive(Debug, Clone)]
pub struct SubValue {
    /// Pattern arguments of the inner (curried) call, e.g. `a` in `f[a][b]`.
    pub outer: Vec<Expr>,
    /// Pattern arguments of the outermost call, e.g. `b`.
    pub inner: Vec<Expr>,
    /// Definition body (unevaluated for delayed definitions).
    pub rhs: Expr,
    pub delayed: bool,
    pub guard: Option<Expr>,
}

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

/// Per-symbol default options: symbol name → ordered `(option name, value)` pairs.
pub type OptionsMap = HashMap<String, Vec<(String, Value)>>;

/// Per-tag upvalue rules: tag symbol → list of `HoldPattern[lhs] :> rhs` rules.
pub type UpValueMap = HashMap<String, Vec<Value>>;

/// Per-head subvalue definitions: head symbol → list of curried definitions.
pub type SubValueMap = HashMap<String, Vec<SubValue>>;

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

/// Fixity of a custom operator.
#[derive(Debug, Clone, PartialEq)]
pub enum Fixity {
    Infix,
    Prefix,
    Postfix,
}

/// Metadata for a registered custom operator.
#[derive(Debug, Clone)]
pub struct OperatorInfo {
    pub head: String,
    pub precedence: u32,
    pub fixity: Fixity,
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
    /// Custom operator table: maps operator strings (e.g. "⊕") to their metadata.
    /// Shared across all child envs in a session.
    pub operator_table: Arc<Mutex<HashMap<String, OperatorInfo>>>,
    /// Wolfram-style context stack (e.g. `["Global`"]`). Shared session-wide;
    /// manipulated by `Begin`/`End`/`BeginPackage`/`EndPackage`.
    pub context_stack: Arc<Mutex<Vec<String>>>,
    /// Per-symbol default options set via `SetOptions`/`Options[f] = ...`.
    /// Shared across all child envs in a session. Each entry is `(name, value)`.
    pub options: Arc<Mutex<OptionsMap>>,
    /// Per-tag upvalues (`f /: g[...] := ...`), keyed by the tag symbol.
    /// Shared across all child envs in a session.
    pub upvalues: Arc<Mutex<UpValueMap>>,
    /// Per-head subvalue definitions (`f[a][b] := ...`), keyed by head symbol.
    /// Shared across all child envs in a session.
    pub subvalues: Arc<Mutex<SubValueMap>>,
    /// Cache of symbol names that resolve to a binding in the *root* scope.
    ///
    /// Deeply nested scopes (e.g. recursive function calls) make `get` walk a
    /// long parent chain, hashing the name at every frame. Most names
    /// (`Plus`, user function heads, `$RecursionLimit`, ...) live in the root
    /// scope, so once resolved they can be served directly from here. Entries
    /// are keyed by name and invalidated whenever that name is (re)bound or
    /// removed in any scope, so a local shadowing binding correctly takes
    /// precedence. Only root hits are cached; local hits fall through to the
    /// normal chain walk.
    global_cache: Arc<Mutex<HashMap<String, Value>>>,
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

    /// Set a variable, walking up the parent chain to update an existing binding.
    /// If not found in any parent, creates a new binding in the current scope.
    pub fn set_propagate(&mut self, name: String, value: Value) {
        if self.bindings.contains_key(&name) {
            self.bindings.insert(name, value);
        } else if let Some(ref parent) = self.parent {
            let mut p = parent.lock().unwrap();
            if p.get(&name).is_some() {
                p.set_propagate(name, value);
                return;
            }
            drop(p);
            self.bindings.insert(name, value);
        } else {
            self.bindings.insert(name, value);
        }
    }

    pub fn set_local(&mut self, name: String, value: Value) {
        self.bindings.insert(name, value);
    }

    /// Remove a binding from this scope only.
    pub fn remove(&mut self, name: &str) -> Option<Value> {
        self.bindings.remove(name)
    }

    /// Remove a binding, walking up the parent chain.
    /// Returns true if the binding was found and removed.
    pub fn remove_propagate(&mut self, name: &str) -> bool {
        if self.bindings.contains_key(name) {
            self.bindings.remove(name);
            true
        } else if let Some(ref parent) = self.parent {
            parent.lock().unwrap().remove_propagate(name)
        } else {
            false
        }
    }

    /// Remove and return a binding, walking up the parent chain.
    pub fn take_propagate(&mut self, name: &str) -> Option<Value> {
        if let Some(val) = self.bindings.remove(name) {
            Some(val)
        } else if let Some(ref parent) = self.parent {
            parent.lock().unwrap().take_propagate(name)
        } else {
            None
        }
    }

    /// Get all bindings from this scope (without traversing parents).
    pub fn all_bindings(&self) -> HashMap<String, Value> {
        self.bindings.clone()
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
            operator_table: Arc::new(Mutex::new(HashMap::new())),
            context_stack: Arc::new(Mutex::new(vec!["Global`".to_string()])),
            options: Arc::new(Mutex::new(HashMap::new())),
            upvalues: Arc::new(Mutex::new(HashMap::new())),
            subvalues: Arc::new(Mutex::new(HashMap::new())),
            global_cache: Arc::new(Mutex::new(HashMap::new())),
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
            operator_table: self.operator_table.clone(),
            context_stack: self.context_stack.clone(),
            options: self.options.clone(),
            upvalues: self.upvalues.clone(),
            subvalues: self.subvalues.clone(),
            global_cache: self.global_cache.clone(),
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
            operator_table: self.operator_table.clone(),
            context_stack: self.context_stack.clone(),
            options: self.options.clone(),
            upvalues: self.upvalues.clone(),
            subvalues: self.subvalues.clone(),
            global_cache: self.global_cache.clone(),
        }
    }

    /// Drop any cached root resolution for `name`.
    ///
    /// Called on every binding mutation so that a newly created local shadow
    /// (or a changed/removed root binding) is picked up by the next `get`.
    fn invalidate_cached(&self, name: &str) {
        // Fast bail-out: only touch the cache mutex when it is non-empty.
        let mut cache = self.global_cache.lock().unwrap();
        if !cache.is_empty() {
            cache.remove(name);
        }
    }

    /// Look up a variable by name.
    pub fn get(&self, name: &str) -> Option<Value> {
        // Fast path: a previous lookup resolved `name` to a root-scope value
        // and no binding for `name` has been mutated since (mutations clear
        // the entry), so no ancestor can be shadowing it.
        if let Some(v) = self.global_cache.lock().unwrap().get(name) {
            return Some(v.clone());
        }
        // Slow path: walk the scope chain innermost → outermost.
        let mut current = Some(self.scope.clone());
        while let Some(scope) = current {
            let guard = scope.lock().unwrap();
            if let Some(val) = guard.bindings.get(name) {
                let val = val.clone();
                let is_root = Arc::ptr_eq(&scope, &self.root_scope);
                drop(guard);
                // Cache only root hits; local hits are cheap to re-walk and
                // may be shadowed differently from other call frames.
                if is_root {
                    self.global_cache
                        .lock()
                        .unwrap()
                        .insert(name.to_string(), val.clone());
                }
                return Some(val);
            }
            current = guard.parent.clone();
        }
        None
    }

    /// Set a variable in the current scope.
    pub fn set(&self, name: String, value: Value) {
        self.invalidate_cached(&name);
        self.scope.lock().unwrap().set(name, value);
    }

    /// Set a variable, propagating to the scope where it was originally defined.
    /// If not found in any ancestor scope, creates it in the current scope.
    pub fn set_propagate(&self, name: String, value: Value) {
        self.invalidate_cached(&name);
        self.scope.lock().unwrap().set_propagate(name, value);
    }

    /// Set a variable in the current (local) scope only.
    pub fn set_local(&self, name: String, value: Value) {
        self.invalidate_cached(&name);
        self.scope.lock().unwrap().set_local(name, value);
    }

    /// Remove a variable, propagating to the scope where it was originally defined.
    /// Returns true if the variable was found and removed.
    pub fn remove(&self, name: &str) -> bool {
        self.invalidate_cached(name);
        self.scope.lock().unwrap().remove_propagate(name)
    }

    /// Remove and return a variable, searching the scope where it was defined.
    ///
    /// Unlike [`Env::get`] this transfers ownership of the value, so an
    /// `Arc` held only by the environment can be uniquely owned afterwards.
    pub fn take(&self, name: &str) -> Option<Value> {
        self.invalidate_cached(name);
        self.scope.lock().unwrap().take_propagate(name)
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
    ///
    /// Avoids cloning the attribute vector (unlike routing through
    /// [`Env::get_attributes`]) so this stays cheap in the hot call path.
    pub fn has_attribute(&self, name: &str, attr: &str) -> bool {
        self.attributes
            .lock()
            .unwrap()
            .get(name)
            .is_some_and(|attrs| attrs.iter().any(|a| a == attr))
    }

    /// Clear (remove) all attributes for a symbol.
    pub fn clear_attributes(&self, name: &str) {
        self.attributes
            .lock()
            .unwrap()
            .insert(name.to_string(), vec![]);
    }

    // ── Custom operator table ──

    /// Register a custom operator with its metadata.
    pub fn register_operator(&self, op_name: &str, info: OperatorInfo) {
        self.operator_table
            .lock()
            .unwrap()
            .insert(op_name.to_string(), info);
    }

    /// Look up a registered custom operator.
    pub fn get_operator(&self, op_name: &str) -> Option<OperatorInfo> {
        self.operator_table.lock().unwrap().get(op_name).cloned()
    }

    // ── Context stack ──

    /// The current (innermost) context, e.g. `"Global`"`.
    pub fn current_context(&self) -> String {
        self.context_stack
            .lock()
            .unwrap()
            .last()
            .cloned()
            .unwrap_or_else(|| "Global`".to_string())
    }

    /// Push a new context onto the context stack.
    pub fn push_context(&self, ctx: String) {
        self.context_stack.lock().unwrap().push(ctx);
    }

    /// Pop the innermost context. Never pops below the base context.
    pub fn pop_context(&self) -> Option<String> {
        let mut stack = self.context_stack.lock().unwrap();
        if stack.len() > 1 {
            stack.pop()
        } else {
            stack.last().cloned()
        }
    }

    // ── Options ──

    /// Return the default options registered for `name`.
    pub fn get_options(&self, name: &str) -> Vec<(String, Value)> {
        self.options
            .lock()
            .unwrap()
            .get(name)
            .cloned()
            .unwrap_or_default()
    }

    /// Add or override a default option for `name`, preserving insertion order.
    pub fn set_option(&self, name: &str, key: String, value: Value) {
        let mut map = self.options.lock().unwrap();
        let entry = map.entry(name.to_string()).or_default();
        if let Some(slot) = entry.iter_mut().find(|(k, _)| *k == key) {
            slot.1 = value;
        } else {
            entry.push((key, value));
        }
    }

    /// Replace all default options for `name`.
    pub fn replace_options(&self, name: &str, opts: Vec<(String, Value)>) {
        self.options.lock().unwrap().insert(name.to_string(), opts);
    }

    // ── Upvalues ──

    /// Return the upvalue rules registered under `tag`.
    pub fn get_upvalues(&self, tag: &str) -> Vec<Value> {
        self.upvalues
            .lock()
            .unwrap()
            .get(tag)
            .cloned()
            .unwrap_or_default()
    }

    /// All upvalue rules across every tag (used for runtime dispatch).
    pub fn all_upvalues(&self) -> Vec<Value> {
        self.upvalues
            .lock()
            .unwrap()
            .values()
            .flat_map(|v| v.iter().cloned())
            .collect()
    }

    /// Register an upvalue rule under `tag`.
    pub fn add_upvalue(&self, tag: &str, rule: Value) {
        self.upvalues
            .lock()
            .unwrap()
            .entry(tag.to_string())
            .or_default()
            .push(rule);
    }

    // ── Subvalues ──

    /// Return the subvalue definitions registered under `head`.
    pub fn get_subvalues(&self, head: &str) -> Vec<SubValue> {
        self.subvalues
            .lock()
            .unwrap()
            .get(head)
            .cloned()
            .unwrap_or_default()
    }

    /// Register a subvalue definition under `head`.
    pub fn add_subvalue(&self, head: &str, sv: SubValue) {
        self.subvalues
            .lock()
            .unwrap()
            .entry(head.to_string())
            .or_default()
            .push(sv);
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
