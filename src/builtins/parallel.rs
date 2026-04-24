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
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;

use crate::env::Env;
use crate::eval::apply_function;
use crate::value::{EvalError, Value};
use rug::Integer;

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
        let completion: Arc<(Mutex<usize>, Condvar)> =
            Arc::new((Mutex::new(0), Condvar::new()));

        for (i, job) in jobs.into_iter().enumerate() {
            let results = Arc::clone(&results);
            let completion = Arc::clone(&completion);
            pool.execute(Box::new(move || {
                let res = catch_unwind(AssertUnwindSafe(job));
                let res = match res {
                    Ok(Ok(v)) => Ok(v),
                    Ok(Err(e)) => Err(e),
                    Err(panic_info) => {
                        let msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                            s.to_string()
                        } else if let Some(s) = panic_info.downcast_ref::<String>() {
                            s.clone()
                        } else {
                            "Unknown panic".to_string()
                        };
                        Err(EvalError::Error(format!(
                            "Parallel job panicked: {}",
                            msg
                        )))
                    }
                };
                {
                    let mut guard = results.lock().unwrap();
                    guard[i] = Some(res);
                }
                let (lock, cvar) = &*completion;
                let mut count = lock.lock().unwrap();
                *count += 1;
                cvar.notify_one();
                Ok(Value::Null) // placeholder
            }));
        }

        // Wait for all jobs via Condvar — no busy-wait
        let (lock, cvar) = &*completion;
        let mut count = lock.lock().unwrap();
        while *count < n {
            count = cvar.wait(count).unwrap();
        }
        drop(count);

        let guard = results.lock().unwrap();
        guard.iter().map(|r| r.clone().unwrap()).collect()
    } else {
        // Sequential fallback: no pool active
        jobs.into_iter().map(|job| job()).collect()
    }
}

// ── Builtins ──

pub fn builtin_parallel_map(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "ParallelMap requires exactly 2 arguments".to_string(),
        ));
    }
    let f = &args[0];
    match &args[1] {
        Value::List(items) if items.is_empty() => Ok(Value::List(vec![])),
        Value::List(items) => {
            if items.len() < 4 {
                let mut result = Vec::with_capacity(items.len());
                for item in items {
                    result.push(apply_function(f, &[item.clone()], env)?);
                }
                return Ok(Value::List(result));
            }
            let jobs: Vec<Box<dyn FnOnce() -> Result<Value, EvalError> + Send>> = items
                .iter()
                .map(|item| {
                    let f = f.clone();
                    let item = item.clone();
                    let env = env.clone();
                    Box::new(move || apply_function(&f, &[item], &env))
                        as Box<dyn FnOnce() -> Result<Value, EvalError> + Send>
                })
                .collect();
            let results = parallel_batch(jobs);
            let mut out = Vec::with_capacity(results.len());
            for r in results {
                out.push(r?);
            }
            Ok(Value::List(out))
        }
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: args[1].type_name().to_string(),
        }),
    }
}

pub fn builtin_launch_kernels(args: &[Value]) -> Result<Value, EvalError> {
    match args.len() {
        0 => Ok(Value::Integer(Integer::from(pool_size() as i64))),
        1 => {
            let n = args[0].to_integer().ok_or_else(|| EvalError::TypeError {
                expected: "Integer".to_string(),
                got: args[0].type_name().to_string(),
            })?;
            if n < 1 {
                return Err(EvalError::Error(
                    "LaunchKernels requires a positive integer".to_string(),
                ));
            }
            launch_kernels(n as usize);
            Ok(Value::Integer(Integer::from(n)))
        }
        _ => Err(EvalError::Error(
            "LaunchKernels requires 0 or 1 arguments".to_string(),
        )),
    }
}

pub fn builtin_close_kernels(args: &[Value]) -> Result<Value, EvalError> {
    if !args.is_empty() {
        return Err(EvalError::Error(
            "CloseKernels takes no arguments".to_string(),
        ));
    }
    close_kernels();
    Ok(Value::Null)
}

pub fn builtin_parallel_table(_args: &[Value]) -> Result<Value, EvalError> {
    Err(EvalError::Error(
        "ParallelTable should be handled by evaluator".to_string(),
    ))
}

// ── Direct builtins ──

/// Stub for ParallelSum — actual implementation is in the evaluator special form.
pub fn builtin_parallel_sum(_args: &[Value]) -> Result<Value, EvalError> {
    Err(EvalError::Error(
        "ParallelSum should be handled by evaluator".to_string(),
    ))
}

/// Stub for ParallelEvaluate — actual implementation is in the evaluator special form.
pub fn builtin_parallel_evaluate(_args: &[Value]) -> Result<Value, EvalError> {
    Err(EvalError::Error(
        "ParallelEvaluate should be handled by evaluator".to_string(),
    ))
}

/// Stub for ParallelTry — actual implementation is in the evaluator special form.
pub fn builtin_parallel_try(_args: &[Value]) -> Result<Value, EvalError> {
    Err(EvalError::Error(
        "ParallelTry should be handled by evaluator".to_string(),
    ))
}

/// Stub for ParallelProduct — actual implementation is in the evaluator special form.
pub fn builtin_parallel_product(_args: &[Value]) -> Result<Value, EvalError> {
    Err(EvalError::Error(
        "ParallelProduct should be handled by evaluator".to_string(),
    ))
}

/// Stub for ParallelDo — actual implementation is in the evaluator special form.
pub fn builtin_parallel_do(_args: &[Value]) -> Result<Value, EvalError> {
    Err(EvalError::Error(
        "ParallelDo should be handled by evaluator".to_string(),
    ))
}

/// `ProcessorCount[]` — returns the number of processor cores on the current computer.
pub fn builtin_processor_count(args: &[Value]) -> Result<Value, EvalError> {
    if !args.is_empty() {
        return Err(EvalError::Error(
            "ProcessorCount takes no arguments".to_string(),
        ));
    }
    let n = std::thread::available_parallelism()
        .map(|p| p.get() as i64)
        .unwrap_or(1);
    Ok(Value::Integer(rug::Integer::from(n)))
}

/// `AbortKernels[]` — aborts all running kernel evaluations.
///
/// Currently a no-op — workers complete their current jobs.
/// Use `CloseKernels[]` and `LaunchKernels[]` to reset the pool.
pub fn builtin_abort_kernels(args: &[Value]) -> Result<Value, EvalError> {
    if !args.is_empty() {
        return Err(EvalError::Error(
            "AbortKernels takes no arguments".to_string(),
        ));
    }
    Ok(Value::Null)
}

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

/// `ParallelCombine[f, list]` — apply binary function f to combine list elements in parallel.
///
/// Partitions the list by kernel count, reduces each chunk sequentially with f,
/// then combines the partial results sequentially.
pub fn builtin_parallel_combine(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "ParallelCombine requires exactly 2 arguments".to_string(),
        ));
    }
    let f = &args[0];
    match &args[1] {
        Value::List(items) if items.is_empty() => Err(EvalError::Error(
            "ParallelCombine requires a non-empty list".to_string(),
        )),
        Value::List(items) if items.len() == 1 => Ok(items[0].clone()),
        Value::List(items) => {
            if items.len() < 8 {
                let mut acc = items[0].clone();
                for item in &items[1..] {
                    acc = apply_function(f, &[acc, item.clone()], env)?;
                }
                return Ok(acc);
            }
            let n_workers = pool_size();
            let chunk_size = items.len().div_ceil(n_workers);
            let jobs: Vec<Job> = items
                .chunks(chunk_size)
                .map(|chunk| {
                    let f = f.clone();
                    let chunk_vec = chunk.to_vec();
                    let env = env.clone();
                    Box::new(move || {
                        let mut acc = chunk_vec[0].clone();
                        for item in &chunk_vec[1..] {
                            acc = apply_function(&f, &[acc, item.clone()], &env)?;
                        }
                        Ok(acc)
                    }) as Job
                })
                .collect();
            let results = parallel_batch(jobs);
            let mut partials: Vec<Value> = Vec::with_capacity(results.len());
            for r in results {
                partials.push(r?);
            }
            let mut acc = partials.remove(0);
            for val in partials {
                acc = apply_function(f, &[acc, val], env)?;
            }
            Ok(acc)
        }
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: args[1].type_name().to_string(),
        }),
    }
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

    #[test]
    fn test_parallel_batch_panic_recovery() {
        cleanup_pool();
        launch_kernels(2);

        let jobs: Vec<Job> = vec![
            Box::new(|| Ok(Value::Integer(rug::Integer::from(1)))),
            Box::new(|| panic!("deliberate panic")),
            Box::new(|| Ok(Value::Integer(rug::Integer::from(3)))),
            Box::new(|| Ok(Value::Integer(rug::Integer::from(4)))),
        ];

        let results = parallel_batch(jobs);
        assert_eq!(results.len(), 4);
        assert!(results[0].is_ok());
        assert!(results[1].is_err());
        assert!(results[2].is_ok());
        assert!(results[3].is_ok());

        if let Err(EvalError::Error(msg)) = &results[1] {
            assert!(
                msg.contains("panicked"),
                "Error should mention panic: {}",
                msg
            );
        } else {
            panic!("Expected EvalError::Error for panicked job");
        }

        cleanup_pool();
    }
}
