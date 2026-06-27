//! The file-provider abstraction: how `import` paths become source text without
//! coupling the core to any filesystem. The core resolves imports through a
//! [`FileProvider`]; the desktop build backs it with the real filesystem
//! (multi-file projects), the web build with an in-memory map populated from a
//! project bundle. The embedded stdlib is ambient and does not go through the
//! provider (every score gets it by default).

use std::collections::HashMap;

/// Resolves an import path to its source text, or `None` when the path cannot be
/// found. Implementations are injected so the core stays IO-free.
pub trait FileProvider {
    fn resolve(&self, path: &str) -> Option<String>;
}

/// An in-memory provider: a `path -> contents` map. Used by the web build
/// (populated from a project bundle) and throughout the core's tests. An empty
/// map resolves nothing, which is the no-imports default for [`crate::compile`].
#[derive(Debug, Clone, Default)]
pub struct MapProvider {
    files: HashMap<String, String>,
}

impl MapProvider {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a file's contents, builder-style.
    pub fn with_file(mut self, path: impl Into<String>, contents: impl Into<String>) -> Self {
        self.files.insert(path.into(), contents.into());
        self
    }

    /// Register a file's contents in place.
    pub fn insert(&mut self, path: impl Into<String>, contents: impl Into<String>) {
        self.files.insert(path.into(), contents.into());
    }
}

impl FileProvider for MapProvider {
    fn resolve(&self, path: &str) -> Option<String> {
        self.files.get(path).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_provider_resolves_known_paths_only() {
        let p = MapProvider::new().with_file("rolls.ctab", "def roll() { 3:0 }");
        assert_eq!(
            p.resolve("rolls.ctab").as_deref(),
            Some("def roll() { 3:0 }")
        );
        assert_eq!(p.resolve("missing.ctab"), None);
    }

    #[test]
    fn insert_mutates_in_place() {
        let mut p = MapProvider::new();
        p.insert("a.ctab", "let x = 3:0");
        assert_eq!(p.resolve("a.ctab").as_deref(), Some("let x = 3:0"));
    }
}
