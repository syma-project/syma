/// Parallel computation builtins for Syma.
///
/// Provides Wolfram-style parallel functions:
/// - `ParallelMap[f, list]` — parallel version of Map
/// - `ParallelTable[expr, {i, min, max}]` — parallel version of Table
/// - `$KernelCount` — number of available parallel workers
/// - `LaunchKernels[n]` — set the number of parallel workers
/// - `CloseKernels[]` — reset workers to default
///
/// Also provides the thread pool infrastructure:
/// - `ThreadPool` — reusable worker pool with job queue
/// - `KERNEL_POOL` — global pool instance (Arc-based for safe shared access)
/// - `parallel_batch()` — submit batch jobs to the pool (or sequential fallback)
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;

use crate::value::{EvalError, Value};

// ── Thread Pool ──

type Job = Box<dyn FnOnce() -> Result<Value, EvalError> + Send>;

struct PoolState {
    queue: VecDeque<Job>,
    shutdown: bool,
}

/// A reusable thread pool for parallel job execution.
pub struct ThreadPool {
    workers: Vec<JoinHandle<()>>,
    state: Arc<Mutex<PoolState>>,
    avail: Arc<Condvar>,
    size: usize,
}

impl ThreadPool {
    /// Create a new thread pool with `n` worker threads.
    pub fn new(n: usize) -> Self {
        let state = Arc::new(Mutex::new(PoolState {
            queue: VecDeque::new(),
            shutdown: false,
        }));
        let avail = Arc::new(Condvar::new());
        let mut workers = Vec::with_capacity(n);

        for _ in 0..n {
            let state = Arc::clone(&state);
            let avail = Arc::clone(&avail);
            workers.push(std::thread::spawn(move || {
                loop {
                    let mut guard = state.lock().unwrap();
                    // Wait until there's a job or we're shutting down
                    while guard.queue.is_empty() && !guard.shutdown {
                        guard = avail.wait(guard).unwrap();
                    }
                    if guard.shutdown && guard.queue.is_empty() {
                        break;
                    }
                    if let Some(job) = guard.queue.pop_front() {
                        // Drop the lock while executing
                        drop(guard);
                        let _ = job();
                        guard = state.lock().unwrap();
                    }
                }
            }));
        }

        ThreadPool {
            workers,
            state,
            avail,
            size: n,
        }
    }

    /// Submit a job to the pool. Returns immediately.
    pub fn execute(&self, job: Job) {
        let mut guard = self.state.lock().unwrap();
        guard.queue.push_back(job);
        self.avail.notify_one();
    }

    /// Number of worker threads.
    pub fn size(&self) -> usize {
        self.size
    }
}

impl Drop for ThreadPool {
    fn drop(&mut self) {
        {
            let mut guard = self.state.lock().unwrap();
            guard.shutdown = true;
        }
        self.avail.notify_all();
        // Join all workers
        while let Some(handle) = self.workers.pop() {
            let _ = handle.join();
        }
    }
}

// ── Global Kernel Pool ──
//
// Uses Arc<ThreadPool> so that parallel_batch can hold a reference that
// outlives a CloseKernels or test cleanup. Without Arc, dropping the global
// Option would shut down workers while another test's jobs are still running.

static KERNEL_POOL: std::sync::OnceLock<Mutex<Option<Arc<ThreadPool>>>> =
    std::sync::OnceLock::new();

fn get_pool() -> &'static Mutex<Option<Arc<ThreadPool>>> {
    KERNEL_POOL.get_or_init(|| Mutex::new(None))
}

/// Return the number of available workers (1 if no pool is active).
pub fn pool_size() -> usize {
    let guard = get_pool().lock().unwrap();
    guard.as_ref().map(|p| p.size()).unwrap_or(1)
}

/// Launch a thread pool with `n` workers. Replaces any existing pool.
pub fn launch_kernels(n: usize) {
    let mut guard = get_pool().lock().unwrap();
    *guard = Some(Arc::new(ThreadPool::new(n)));
}

/// Close the current thread pool. No-op if no pool is active.
pub fn close_kernels() {
    let mut guard = get_pool().lock().unwrap();
    *guard = None;
    // The old Arc is dropped here; if parallel_batch holds another clone,
    // the pool stays alive until that batch finishes.
}

/// Submit a batch of jobs for parallel execution.
///
/// If a thread pool is active, distributes jobs across workers.
/// Otherwise, runs jobs sequentially in the current thread.
/// Results are returned in order regardless of execution order.
pub fn parallel_batch(jobs: Vec<Job>) -> Vec<Result<Value, EvalError>> {
    // Clone the Arc (if any) so the pool stays alive for the full batch.
    let pool_opt = {
        let guard = get_pool().lock().unwrap();
        guard.as_ref().map(Arc::clone)
    };

    if let Some(pool) = pool_opt {
        let n = jobs.len();
        #[allow(clippy::type_complexity)]
        let results: Arc<Mutex<Vec<Option<Result<Value, EvalError>>>>> =
            Arc::new(Mutex::new(vec![None; n]));
        let completed = Arc::new(AtomicUsize::new(0));

        for (i, job) in jobs.into_iter().enumerate() {
            let results = Arc::clone(&results);
            let completed = Arc::clone(&completed);
            pool.execute(Box::new(move || {
                let res = job();
                let mut guard = results.lock().unwrap();
                guard[i] = Some(res);
                completed.fetch_add(1, Ordering::Release);
                Ok(Value::Null) // placeholder, actual results are in `results`
            }));
        }

        // Wait for all jobs to complete
        while completed.load(Ordering::Acquire) < n {
            std::thread::yield_now();
        }

        let guard = results.lock().unwrap();
        guard.iter().map(|r| r.clone().unwrap()).collect()
    } else {
        // Sequential fallback: no pool active
        jobs.into_iter().map(|job| job()).collect()
    }
}

// ── Stubs (evaluator-dependent, dispatched from eval.rs) ──

pub fn builtin_parallel_map(_args: &[Value]) -> Result<Value, EvalError> {
    Err(EvalError::Error(
        "ParallelMap should be handled by evaluator".to_string(),
    ))
}

pub fn builtin_parallel_table(_args: &[Value]) -> Result<Value, EvalError> {
    Err(EvalError::Error(
        "ParallelTable should be handled by evaluator".to_string(),
    ))
}

pub fn builtin_launch_kernels(_args: &[Value]) -> Result<Value, EvalError> {
    Err(EvalError::Error(
        "LaunchKernels should be handled by evaluator".to_string(),
    ))
}

pub fn builtin_close_kernels(_args: &[Value]) -> Result<Value, EvalError> {
    Err(EvalError::Error(
        "CloseKernels should be handled by evaluator".to_string(),
    ))
}

// ── Direct builtins ──

/// `$KernelCount` — returns the number of available parallel workers.
/// By default this is the number of CPU cores reported by the OS.
pub fn builtin_kernel_count(args: &[Value]) -> Result<Value, EvalError> {
    if !args.is_empty() {
        return Err(EvalError::Error(
            "$KernelCount takes no arguments".to_string(),
        ));
    }
    let n = std::thread::available_parallelism()
        .map(|p| p.get() as i64)
        .unwrap_or(1);
    Ok(Value::Integer(rug::Integer::from(n)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ensure the global pool is clean before each test.
    fn cleanup_pool() {
        *get_pool().lock().unwrap() = None;
    }

    #[test]
    fn test_pool_creation_and_size() {
        let pool = ThreadPool::new(4);
        assert_eq!(pool.size(), 4);
        // Pool drops at end of scope, joining all workers
    }

    #[test]
    fn test_pool_executes_jobs() {
        let pool = ThreadPool::new(2);
        let result = Arc::new(Mutex::new(0i32));
        for _ in 0..5 {
            let r = Arc::clone(&result);
            pool.execute(Box::new(move || {
                let mut guard = r.lock().unwrap();
                *guard += 1;
                Ok(Value::Null)
            }));
        }
        // Give workers time to finish
        drop(pool);
    }

    #[test]
    fn test_parallel_batch_sequential_fallback() {
        cleanup_pool();
        let jobs: Vec<Job> = (0..5)
            .map(|i| Box::new(move || Ok(Value::Integer(rug::Integer::from(i * 2)))) as Job)
            .collect();

        let results = parallel_batch(jobs);
        assert_eq!(results.len(), 5);
        for (i, r) in results.iter().enumerate() {
            assert!(r.is_ok());
            assert_eq!(
                r.as_ref().unwrap(),
                &Value::Integer(rug::Integer::from((i as i64) * 2))
            );
        }
    }

    #[test]
    fn test_parallel_batch_with_pool() {
        cleanup_pool();
        launch_kernels(4);

        let jobs: Vec<Job> = (0..10)
            .map(|i| Box::new(move || Ok(Value::Integer(rug::Integer::from(i)))) as Job)
            .collect();

        let results = parallel_batch(jobs);
        assert_eq!(results.len(), 10);
        for (i, r) in results.iter().enumerate() {
            assert!(r.is_ok());
            assert_eq!(
                r.as_ref().unwrap(),
                &Value::Integer(rug::Integer::from(i as i64))
            );
        }

        cleanup_pool();
    }

    #[test]
    fn test_pool_size_default() {
        cleanup_pool();
        assert_eq!(pool_size(), 1);
    }

    #[test]
    fn test_pool_size_with_pool() {
        cleanup_pool();
        launch_kernels(8);
        assert_eq!(pool_size(), 8);
        cleanup_pool();
        assert_eq!(pool_size(), 1);
    }
}
