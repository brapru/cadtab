use std::path::{Path, PathBuf};

use cadtab_core::provider::FileProvider;
use cadtab_core::{CompileResult, compile_with_provider, layout::LayoutConfig};

/// Resolves `import`s on the real filesystem, relative to the directory holding
/// the open document. With no open document (the default buffer), it resolves
/// nothing — so imports report as unresolvable rather than reading arbitrary cwd
/// files.
struct FsProvider {
    base_dir: Option<PathBuf>,
}

impl FsProvider {
    /// `base_path` is the open document's path; imports resolve beside it.
    fn new(base_path: Option<String>) -> Self {
        let base_dir = base_path
            .map(PathBuf::from)
            .and_then(|p| p.parent().map(Path::to_path_buf));
        Self { base_dir }
    }
}

impl FileProvider for FsProvider {
    fn resolve(&self, path: &str) -> Option<String> {
        let base = self.base_dir.as_ref()?;
        std::fs::read_to_string(base.join(path)).ok()
    }
}

#[tauri::command]
fn compile(source: String, config: LayoutConfig, base_path: Option<String>) -> CompileResult {
    let provider = FsProvider::new(base_path);
    compile_with_provider(&source, config, &provider)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
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
