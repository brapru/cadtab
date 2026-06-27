use wasm_bindgen::prelude::*;

/// Installs a panic hook that forwards Rust panics to the browser console.
#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
}

/// Returns the core crate version.
#[wasm_bindgen]
pub fn version() -> String {
    cadtab_core::version().to_string()
}

/// Compiles source text into a `CompileResult`, marshalled to a JS object.
/// `config` is a `LayoutConfig` JS object; `files` is the project bundle as a
/// `{ path: contents }` map (or null/undefined for none) backing `import`
/// resolution. The return value mirrors the Tauri `compile` command so the
/// frontend can dispatch to either backend.
#[wasm_bindgen]
pub fn compile(source: &str, config: JsValue, files: JsValue) -> Result<JsValue, JsValue> {
    let config: cadtab_core::layout::LayoutConfig =
        serde_wasm_bindgen::from_value(config).map_err(|e| JsValue::from_str(&e.to_string()))?;

    let files: std::collections::HashMap<String, String> =
        if files.is_undefined() || files.is_null() {
            std::collections::HashMap::new()
        } else {
            serde_wasm_bindgen::from_value(files).map_err(|e| JsValue::from_str(&e.to_string()))?
        };
    let mut provider = cadtab_core::provider::MapProvider::new();
    for (path, contents) in files {
        provider.insert(path, contents);
    }

    let result = cadtab_core::compile_with_provider(source, config, &provider);
    serde_wasm_bindgen::to_value(&result).map_err(|e| JsValue::from_str(&e.to_string()))
}

#[cfg(test)]
mod tests {
    use cadtab_core::{CompileResult, compile, layout::LayoutConfig};

    #[test]
    fn compile_result_round_trips_through_json() {
        let result = compile("score { 3:0 2:0 1:0 5:0 }", LayoutConfig { width: 800.0 });
        let json = serde_json::to_string(&result).unwrap();
        let back: CompileResult = serde_json::from_str(&json).unwrap();
        assert_eq!(result, back);
    }
}

#[cfg(target_arch = "wasm32")]
#[cfg(test)]
mod wasm_tests {
    use super::*;
    use wasm_bindgen_test::*;

    #[wasm_bindgen_test]
    fn compile_marshals_a_render_tree() {
        let config =
            serde_wasm_bindgen::to_value(&cadtab_core::layout::LayoutConfig { width: 800.0 })
                .unwrap();
        let value = compile("score { 3:0 2:0 1:0 5:0 }", config, JsValue::UNDEFINED).unwrap();
        let result: cadtab_core::CompileResult = serde_wasm_bindgen::from_value(value).unwrap();
        assert_eq!(result.render_tree.systems.len(), 1);
    }
}
