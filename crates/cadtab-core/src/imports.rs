//! Import loading: walk a program's `import` declarations, pull each file's
//! source through a [`FileProvider`], and flatten the transitively-imported
//! `def`/`let` declarations into one pool. cadtab names are flat (no
//! namespacing), so an imported file contributes all its top-level names to both
//! resolution (so calls to them resolve) and evaluation (so the bodies run).
//!
//! Resolution is recursive — an imported library may itself `import` another —
//! with a visited set for dedup and an on-stack set to catch cycles. Every
//! diagnostic is anchored to the *entry* import that started the chain, so it
//! always maps to a location in the document the editor is showing (imported
//! files have their own coordinate space, which the single editor cannot map).

use std::collections::HashSet;

use crate::ast::{Item, ItemKind, Program};
use crate::diagnostics::Diagnostic;
use crate::parser::parse;
use crate::provider::FileProvider;
use crate::span::Span;

/// The flattened result of resolving a program's imports.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LoadedImports {
    /// Imported `def`/`let` items, in load order, for the evaluator.
    pub items: Vec<Item>,
    /// The names those items bind, for name resolution.
    pub names: Vec<String>,
    /// Unresolved-path, cyclic-import, and imported-file parse diagnostics.
    pub diagnostics: Vec<Diagnostic>,
}

/// Resolve every `import` reachable from `entry` through `provider`.
pub fn load_imports(entry: &Program, provider: &dyn FileProvider) -> LoadedImports {
    let mut loader = Loader {
        provider,
        visited: HashSet::new(),
        on_stack: HashSet::new(),
        out: LoadedImports::default(),
    };
    loader.load_program_imports(entry, None);
    loader.out
}

struct Loader<'a> {
    provider: &'a dyn FileProvider,
    /// Paths fully loaded, so a diamond import loads a file only once.
    visited: HashSet<String>,
    /// Paths currently being loaded, so a cycle is detected rather than looped.
    on_stack: HashSet<String>,
    out: LoadedImports,
}

impl Loader<'_> {
    /// Process the `import` items of one program. `anchor` is the entry import
    /// span to blame for failures deeper in the chain; `None` at the entry,
    /// where each import is blamed on its own span.
    fn load_program_imports(&mut self, program: &Program, anchor: Option<Span>) {
        for item in &program.items {
            if let ItemKind::Import(path) = &item.kind {
                let blame = anchor.unwrap_or(item.span);
                self.load_path(&path.value, blame);
            }
        }
    }

    fn load_path(&mut self, path: &str, blame: Span) {
        if self.on_stack.contains(path) {
            self.out.diagnostics.push(
                Diagnostic::error(blame, format!("import cycle through \"{path}\""))
                    .with_help("a file cannot import itself, directly or transitively"),
            );
            return;
        }
        if self.visited.contains(path) {
            return; // Already loaded via another path; its names/items are in.
        }

        let Some(source) = self.provider.resolve(path) else {
            self.out.diagnostics.push(
                Diagnostic::error(blame, format!("cannot resolve import \"{path}\"")).with_help(
                    "imports resolve to project files (desktop/bundle) or the standard library",
                ),
            );
            return;
        };

        let parsed = parse(&source);
        if !parsed.diagnostics.is_empty() {
            self.out.diagnostics.push(
                Diagnostic::error(blame, format!("imported file \"{path}\" contains errors"))
                    .with_help("fix the errors in the imported file"),
            );
            // Resilient: still pull in whatever parsed so valid licks stay usable.
        }

        self.on_stack.insert(path.to_string());
        // Load this file's own imports first, so a library's dependencies are
        // registered before the library that builds on them.
        self.load_program_imports(&parsed.program, Some(blame));
        self.on_stack.remove(path);
        self.visited.insert(path.to_string());

        for item in &parsed.program.items {
            match &item.kind {
                ItemKind::Def(def) => {
                    self.out.names.push(def.name.name.clone());
                    self.out.items.push(item.clone());
                }
                ItemKind::Let(l) => {
                    self.out.names.push(l.name.name.clone());
                    self.out.items.push(item.clone());
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::MapProvider;

    fn load(entry: &str, provider: &MapProvider) -> LoadedImports {
        load_imports(&parse(entry).program, provider)
    }

    #[test]
    fn no_imports_loads_nothing() {
        let loaded = load("score { 3:0 }", &MapProvider::new());
        assert_eq!(loaded, LoadedImports::default());
    }

    #[test]
    fn resolved_import_contributes_its_names_and_items() {
        let provider = MapProvider::new().with_file("rolls.ctab", "def my_roll(c) { c.0 .t }");
        let loaded = load(
            "import \"rolls.ctab\"\nscore { my_roll([3:0 2:0 1:0]) }",
            &provider,
        );
        assert_eq!(loaded.names, vec!["my_roll"]);
        assert_eq!(loaded.items.len(), 1);
        assert!(loaded.diagnostics.is_empty(), "{:?}", loaded.diagnostics);
    }

    #[test]
    fn missing_file_is_reported_at_the_import_span() {
        let loaded = load("import \"nope.ctab\"", &MapProvider::new());
        assert_eq!(loaded.names, Vec::<String>::new());
        assert_eq!(loaded.diagnostics.len(), 1);
        assert!(
            loaded.diagnostics[0]
                .message
                .contains("cannot resolve import \"nope.ctab\"")
        );
    }

    #[test]
    fn imported_file_with_errors_is_flagged_but_valid_defs_survive() {
        // The lib has one good def and a trailing bare event (a parse error).
        let provider = MapProvider::new().with_file("lib.ctab", "def good() { 3:0 }\n@@@");
        let loaded = load("import \"lib.ctab\"", &provider);
        assert!(
            loaded
                .diagnostics
                .iter()
                .any(|d| d.message.contains("contains errors"))
        );
        assert_eq!(loaded.names, vec!["good"]);
    }

    #[test]
    fn imports_resolve_recursively() {
        // entry -> a -> b; b's lick must be loaded for a to call it.
        let provider = MapProvider::new()
            .with_file("a.ctab", "import \"b.ctab\"\ndef a_lick() { base() }")
            .with_file("b.ctab", "def base() { 3:0 }");
        let loaded = load("import \"a.ctab\"\nscore { a_lick() }", &provider);
        assert!(loaded.diagnostics.is_empty(), "{:?}", loaded.diagnostics);
        // Deepest-first: b's `base` is registered before a's `a_lick`.
        assert_eq!(loaded.names, vec!["base", "a_lick"]);
    }

    #[test]
    fn a_diamond_loads_each_file_once() {
        // entry imports a and b; both import shared. `shared` loads once.
        let provider = MapProvider::new()
            .with_file("a.ctab", "import \"shared.ctab\"\ndef a() { s() }")
            .with_file("b.ctab", "import \"shared.ctab\"\ndef b() { s() }")
            .with_file("shared.ctab", "def s() { 3:0 }");
        let loaded = load("import \"a.ctab\"\nimport \"b.ctab\"", &provider);
        assert!(loaded.diagnostics.is_empty(), "{:?}", loaded.diagnostics);
        assert_eq!(loaded.names, vec!["s", "a", "b"]);
    }

    #[test]
    fn a_cycle_is_reported_not_looped() {
        let provider = MapProvider::new()
            .with_file("a.ctab", "import \"b.ctab\"\ndef a() { 3:0 }")
            .with_file("b.ctab", "import \"a.ctab\"\ndef b() { 3:0 }");
        let loaded = load("import \"a.ctab\"", &provider);
        assert!(
            loaded
                .diagnostics
                .iter()
                .any(|d| d.message.contains("import cycle")),
            "expected a cycle diagnostic, got {:?}",
            loaded.diagnostics
        );
        // Both files' defs still load (the cycle edge is just dropped).
        assert!(loaded.names.contains(&"a".to_string()));
        assert!(loaded.names.contains(&"b".to_string()));
    }
}
