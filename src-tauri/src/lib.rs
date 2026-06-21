use cadtab_core::{CompileResult, compile as core_compile, layout::LayoutConfig};

#[tauri::command]
fn compile(source: String, config: LayoutConfig) -> CompileResult {
    core_compile(&source, config)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![compile])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
