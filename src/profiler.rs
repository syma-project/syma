/// Hotness tracking for JIT tiering.
///
/// Every call to a user-defined function increments an atomic counter.
/// When the counter passes the threshold, the function is promoted to
/// bytecode execution (phase 2) and later to native JIT (phase 3).
///
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

/// Configuration for compilation thresholds.
#[derive(Debug, Clone)]
pub struct TieringConfig {
    /// Number of calls before a function is compiled to bytecode.
    /// Default: 100.
    pub hot_function_threshold: u64,
}

impl Default for TieringConfig {
    fn default() -> Self {
        Self {
            hot_function_threshold: 100,
        }
    }
}

/// Per-function profiling counters.
#[derive(Debug)]
pub struct FunctionProfiler {
    /// Number of times this function has been called.
    pub call_count: AtomicU64,
}

/// Global profiler registry.
#[derive(Debug)]
pub struct Profiler {
    pub config: TieringConfig,
    functions: Mutex<HashMap<String, &'static FunctionProfiler>>,
}

impl Profiler {
    fn new() -> Self {
        Self {
            config: TieringConfig::default(),
            functions: Mutex::new(HashMap::new()),
        }
    }

    /// Get or create the profiler for a named function.
    pub fn get_or_create(name: &str) -> &'static FunctionProfiler {
        GLOBAL_PROFILER.get_or_create_inner(name)
    }

    fn get_or_create_inner(&self, name: &str) -> &FunctionProfiler {
        let mut map = self.functions.lock().unwrap();
        if !map.contains_key(name) {
            let leaked: &'static FunctionProfiler = Box::leak(Box::new(FunctionProfiler {
                call_count: AtomicU64::new(0),
            }));
            map.insert(name.to_string(), leaked);
        }
        map[name]
    }

    /// Increment the call counter for a function and return the new count.
    pub fn count_call(name: &str) -> u64 {
        let prof = Self::get_or_create(name);
        prof.call_count.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// Check if a function has crossed the threshold and is hot.
    pub fn check_hot(name: &str) -> bool {
        let prof = Self::get_or_create(name);
        let count = prof.call_count.load(Ordering::Relaxed);
        count >= GLOBAL_PROFILER.config.hot_function_threshold
    }

    /// Reset the counter for a function (e.g., after compilation).
    pub fn reset(name: &str) {
        let prof = Self::get_or_create(name);
        prof.call_count.store(0, Ordering::Relaxed);
    }
}

static GLOBAL_PROFILER: LazyLock<Profiler> = LazyLock::new(Profiler::new);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_counter_increment() {
        Profiler::reset("test_fn");
        assert_eq!(Profiler::count_call("test_fn"), 1);
        assert_eq!(Profiler::count_call("test_fn"), 2);
        assert_eq!(Profiler::count_call("test_fn"), 3);
    }

    #[test]
    fn test_threshold_check() {
        Profiler::reset("threshold_fn");
        // With default threshold of 100, 99 calls should not be hot
        for _ in 0..99 {
            Profiler::count_call("threshold_fn");
        }
        assert!(!Profiler::check_hot("threshold_fn"));
        // 100th call crosses threshold
        Profiler::count_call("threshold_fn");
        assert!(Profiler::check_hot("threshold_fn"));
    }

    #[test]
    fn test_separate_counters() {
        Profiler::reset("fn_a");
        Profiler::reset("fn_b");
        Profiler::count_call("fn_a");
        Profiler::count_call("fn_a");
        Profiler::count_call("fn_b");
        assert_eq!(Profiler::get_or_create("fn_a").call_count.load(Ordering::Relaxed), 2);
        assert_eq!(Profiler::get_or_create("fn_b").call_count.load(Ordering::Relaxed), 1);
    }
}
