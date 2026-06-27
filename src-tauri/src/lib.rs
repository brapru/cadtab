use std::collections::HashMap;
use std::path::{Path, PathBuf};

use cadtab_core::provider::{FileProvider, MapProvider};
use cadtab_core::{CompileResult, compile_with_provider, layout::LayoutConfig};

/// Resolves `import`s for the desktop build: an in-memory bundle map first (so an
/// opened `.ctabz` works on desktop too), then the real filesystem relative to
/// the directory holding the open document. With neither a bundle nor an open
/// document, it resolves nothing — imports report as unresolvable rather than
/// reading arbitrary cwd files.
struct ProjectProvider {
    files: MapProvider,
    base_dir: Option<PathBuf>,
}

impl ProjectProvider {
    /// `base_path` is the open document's path (imports resolve beside it);
    /// `files` is the in-memory project bundle, checked first.
    fn new(base_path: Option<String>, files: HashMap<String, String>) -> Self {
        let mut map = MapProvider::new();
        for (path, contents) in files {
            map.insert(path, contents);
        }
        let base_dir = base_path
            .map(PathBuf::from)
            .and_then(|p| p.parent().map(Path::to_path_buf));
        Self {
            files: map,
            base_dir,
        }
    }
}

impl FileProvider for ProjectProvider {
    fn resolve(&self, path: &str) -> Option<String> {
        self.files.resolve(path).or_else(|| {
            let base = self.base_dir.as_ref()?;
            std::fs::read_to_string(base.join(path)).ok()
        })
    }
}

#[tauri::command]
fn compile(
    source: String,
    config: LayoutConfig,
    base_path: Option<String>,
    files: Option<HashMap<String, String>>,
) -> CompileResult {
    let provider = ProjectProvider::new(base_path, files.unwrap_or_default());
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
