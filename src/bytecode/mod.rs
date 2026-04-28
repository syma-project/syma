/// Syma Bytecode VM (Phase 2 of the JIT pipeline).
///
/// Hot user-defined functions are compiled from AST to a register-based
/// bytecode instruction stream and executed by the VM in [`vm`].  The
/// bytecode is designed to feed into the Cranelift native compiler
/// (Phase 3, behind the `"jit"` feature flag).
///
pub mod compiler;
pub mod instruction;
pub mod vm;

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicPtr, AtomicU64};

use crate::value::Value;

/// The type of a JIT-compiled function: `extern "C" fn(ctx: *mut JitContext)`.
pub type JitFnPtr = AtomicPtr<()>;

/// Tracks a block of executable memory allocated for JIT-compiled code.
///
/// This struct owns the underlying allocation and ensures it is properly
/// deallocated when the `BytecodeFunctionDef` that owns it is dropped.
/// On Unix, memory is allocated via `mmap`; on Windows, via `VirtualAlloc`.
#[cfg(feature = "jit")]
pub struct JitModule {
    /// Pointer to the executable code block.
    ptr: *mut u8,
    /// Total allocated size in bytes (page-aligned).
    size: usize,
}

#[cfg(feature = "jit")]
impl JitModule {
    /// Create a new JitModule wrapping an existing allocation.
    ///
    /// # Safety
    /// The caller must ensure `ptr` was obtained from the matching
    /// platform allocation function (`mmap` on Unix, `VirtualAlloc` on Windows)
    /// and `size` matches the allocation size.
    pub(crate) unsafe fn new(ptr: *mut u8, size: usize) -> Self {
        Self { ptr, size }
    }

    /// Get the function pointer for this module.
    pub fn fn_ptr(&self) -> *mut () {
        self.ptr as *mut ()
    }
}

#[cfg(feature = "jit")]
impl Drop for JitModule {
    fn drop(&mut self) {
        if self.ptr.is_null() || self.size == 0 {
            return;
        }
        #[cfg(unix)]
        unsafe {
            libc::munmap(self.ptr as *mut libc::c_void, self.size);
        }
        #[cfg(windows)]
        unsafe {
            use std::ptr::null_mut;
            // VirtualFree with MEM_RELEASE to free the entire allocation
            extern "system" {
                fn VirtualAlloc(
                    lp_address: *mut std::ffi::c_void,
                    dw_size: usize,
                    fl_allocation_type: u32,
                    fl_protect: u32,
                ) -> *mut std::ffi::c_void;
            }
            // MEM_RELEASE = 0x8000
            let _ = VirtualAlloc(self.ptr as *mut std::ffi::c_void, 0, 0x8000, 0);
        }
        self.ptr = std::ptr::null_mut();
        self.size = 0;
    }
}

// SAFETY: JitModule owns a block of mmap/VirtualAlloc memory. The pointer
// itself is not dereferenced from multiple threads — only the jit_fn_ptr
// (AtomicPtr) is read from other threads for function invocation. The
// Drop implementation runs on the owning thread when BytecodeFunctionDef
// is dropped.
#[cfg(feature = "jit")]
unsafe impl Send for JitModule {}

#[cfg(not(feature = "jit"))]
pub struct JitModule {
    _private: (),
}

#[cfg(not(feature = "jit"))]
impl JitModule {
    #[allow(dead_code)]
    pub(crate) unsafe fn new(_ptr: *mut u8, _size: usize) -> Self {
        Self { _private: () }
    }

    pub fn fn_ptr(&self) -> *mut () {
        std::ptr::null_mut()
    }
}

#[cfg(not(feature = "jit"))]
impl Drop for JitModule {
    fn drop(&mut self) {}
}

/// A function whose body has been compiled to Syma bytecode.
pub struct BytecodeFunctionDef {
    /// The name of the function.
    pub name: String,
    /// The compiled bytecode body.
    pub bytecode: CompiledBytecode,
    /// How many times this function has been called
    /// (used for Phase 3 promotion).
    pub call_count: Arc<AtomicU64>,
    /// Pointer to JIT-compiled native code (null = not compiled yet).
    pub jit_fn_ptr: Arc<JitFnPtr>,
    /// Owning handle for JIT-allocated executable memory.
    /// When this is dropped, the executable memory is released.
    pub jit_module: Mutex<Option<JitModule>>,
}

impl BytecodeFunctionDef {
    /// Create a new BytecodeFunctionDef with no JIT module.
    pub fn new(name: String, bytecode: CompiledBytecode, call_count: Arc<AtomicU64>) -> Self {
        Self {
            name,
            bytecode,
            call_count,
            jit_fn_ptr: Arc::new(AtomicPtr::new(std::ptr::null_mut())),
            jit_module: Mutex::new(None),
        }
    }

    /// Set the JIT module, replacing any existing one.
    /// The old module (if any) is dropped, freeing its executable memory.
    pub fn set_jit_module(&self, module: Option<JitModule>) {
        *self.jit_module.lock().unwrap() = module;
    }
}

impl std::fmt::Debug for BytecodeFunctionDef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BytecodeFunctionDef")
            .field("name", &self.name)
            .field("bytecode", &self.bytecode)
            .field("call_count", &self.call_count)
            .field("jit_fn_ptr", &self.jit_fn_ptr)
            .field("jit_module", &"<JitModule>")
            .finish()
    }
}

impl Clone for BytecodeFunctionDef {
    fn clone(&self) -> Self {
        // When cloning, we do NOT clone the JitModule — the original owner
        // retains exclusive ownership of the executable memory. The clone
        // shares the jit_fn_ptr (AtomicPtr) via Arc, so it can still call
        // the JIT function, but only the original drops the allocation.
        Self {
            name: self.name.clone(),
            bytecode: self.bytecode.clone(),
            call_count: self.call_count.clone(),
            jit_fn_ptr: self.jit_fn_ptr.clone(),
            jit_module: Mutex::new(None),
        }
    }
}

/// Compiled bytecode for a single function body.
#[derive(Debug, Clone)]
pub struct CompiledBytecode {
    /// Instructions in linear order.
    pub instructions: Vec<instruction::Instruction>,
    /// Constant pool — literal values referenced by `LoadConst`.
    pub constants: Vec<Value>,
    /// Number of virtual registers this function needs.
    pub nregs: u16,
    /// Number of parameters the function expects.
    pub nparams: u8,
}
