use syma::kernel::{KernelResult, SymaKernel};
use std::sync::Mutex;
use tauri::State;

struct KernelState(Mutex<SymaKernel>);

#[tauri::command]
fn eval(input: &str, state: State<KernelState>) -> KernelResult {
    state.0.lock().unwrap().eval(input)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(KernelState(Mutex::new(SymaKernel::new())))
        .invoke_handler(tauri::generate_handler![eval])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
