//! The completion vocabulary the editor offers (D46): the keyword table with
//! each keyword's operand shape and value set, plus the identifier registry
//! (ambient stdlib licks and the document's own / imported `def`/`let` names).
//!
//! Everything here is *derived* from the existing core knowledge — the keyword
//! enum, the builtin instruments, the named tunings, the bar-number modes, and
//! the parsed program — so the editor never keeps a second copy of the grammar
//! (mirrors D27). The query is `source → candidates`; turning a cursor position
//! into the right subset is the editor's job (T7.24b).

use serde::{Deserialize, Serialize};

use crate::ast::ItemKind;
use crate::imports::load_imports;
use crate::instrument;
use crate::model;
use crate::parser::parse;
use crate::provider::{FileProvider, MapProvider};
use crate::stdlib;
use crate::token::Keyword;

/// The operand shape an editor should offer after a keyword. Sourced from the
/// parser's grammar so the editor hints the right thing without its own table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OperandKind {
    /// No operand to hint — a structural keyword (`score`, `def`, `measure`).
    None,
    /// A free string operand (`title "…"`).
    String,
    /// A numeric operand (`tempo 120`).
    Number,
    /// A fixed set of identifier values, listed in [`KeywordInfo::values`]
    /// (`instrument` → `banjo`/`guitar`).
    Values,
}

/// One keyword's completion entry: its spelling, the operand it expects, and —
/// for [`OperandKind::Values`] — the closed set of values to offer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeywordInfo {
    /// The keyword spelling (`instrument`).
    pub name: String,
    /// The operand shape the editor should hint after the keyword.
    pub operand: OperandKind,
    /// The closed value set for `operand == Values` (`["banjo", "guitar"]`),
    /// empty for every other operand shape.
    pub values: Vec<String>,
}

/// The completion vocabulary for a document: the keyword table and the
/// identifier registry. Crosses the wire to the editor (T7.24).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Completions {
    /// Every keyword with its operand shape and value set.
    pub keywords: Vec<KeywordInfo>,
    /// Identifier completions, sorted and deduplicated: the ambient stdlib licks
    /// plus the document's own and imported top-level `def`/`let` names.
    pub identifiers: Vec<String>,
}

/// The completion entry for one keyword, pulling its value set from the
/// authoritative core list (the instrument/tuning/bar-number registries).
fn keyword_info(kw: Keyword) -> KeywordInfo {
    use Keyword::*;
    let (operand, values): (OperandKind, Vec<&'static str>) = match kw {
        Title | Composer | Capo | Import => (OperandKind::String, vec![]),
        Tempo => (OperandKind::Number, vec![]),
        Instrument => (OperandKind::Values, instrument::builtin_names()),
        Tuning => (OperandKind::Values, instrument::named_tuning_keys()),
        BarNumbers => (OperandKind::Values, model::BarNumbers::keywords()),
        Score | Time | Default | Pickup | Repeat | Ending | Loop | Measure | Section | Def
        | Let => (OperandKind::None, vec![]),
    };
    KeywordInfo {
        name: kw.as_str().to_string(),
        operand,
        values: values.into_iter().map(String::from).collect(),
    }
}

/// The completion vocabulary for `source` with no import resolution. The
/// embedded stdlib is still ambient; an `import` simply contributes no names.
pub fn completions(source: &str) -> Completions {
    completions_with_provider(source, &MapProvider::new())
}

/// The completion vocabulary for `source`, resolving `import`s through
/// `provider` exactly as `compile_with_provider` does so imported `def`/`let`
/// names complete too. The keyword table is static; the identifier registry is
/// the ambient stdlib plus the document's own and imported top-level names.
pub fn completions_with_provider(source: &str, provider: &dyn FileProvider) -> Completions {
    let keywords = Keyword::ALL.iter().map(|&kw| keyword_info(kw)).collect();

    let parsed = parse(source);
    let loaded = load_imports(&parsed.program, provider);

    let mut identifiers = stdlib::names();
    identifiers.extend(loaded.names.iter().cloned());
    for item in &parsed.program.items {
        match &item.kind {
            ItemKind::Def(d) => identifiers.push(d.name.name.clone()),
            ItemKind::Let(l) => identifiers.push(l.name.name.clone()),
            _ => {}
        }
    }
    identifiers.sort();
    identifiers.dedup();

    Completions {
        keywords,
        identifiers,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::MapProvider;

    fn keyword<'a>(c: &'a Completions, name: &str) -> &'a KeywordInfo {
        c.keywords
            .iter()
            .find(|k| k.name == name)
            .unwrap_or_else(|| panic!("keyword `{name}` missing from completions"))
    }

    #[test]
    fn every_keyword_is_listed_once() {
        let c = completions("");
        assert_eq!(c.keywords.len(), Keyword::ALL.len());
        for &kw in Keyword::ALL {
            assert_eq!(keyword(&c, kw.as_str()).name, kw.as_str());
        }
    }

    #[test]
    fn value_set_keywords_carry_their_core_values() {
        // The value sets come straight from the core registries (no JS copy):
        // instruments, named tunings, bar-number modes.
        let c = completions("");
        let instrument = keyword(&c, "instrument");
        assert_eq!(instrument.operand, OperandKind::Values);
        assert_eq!(instrument.values, vec!["banjo", "guitar"]);

        let tuning = keyword(&c, "tuning");
        assert_eq!(tuning.operand, OperandKind::Values);
        assert!(tuning.values.contains(&"openG".to_string()));
        assert!(tuning.values.contains(&"dropD".to_string()));

        let bars = keyword(&c, "barnumbers");
        assert_eq!(bars.operand, OperandKind::Values);
        assert_eq!(bars.values, vec!["lines", "all", "off"]);
    }

    #[test]
    fn operand_shapes_match_the_grammar() {
        let c = completions("");
        assert_eq!(keyword(&c, "title").operand, OperandKind::String);
        assert_eq!(keyword(&c, "composer").operand, OperandKind::String);
        assert_eq!(keyword(&c, "tempo").operand, OperandKind::Number);
        assert_eq!(keyword(&c, "score").operand, OperandKind::None);
        // A structural keyword carries no value set.
        assert!(keyword(&c, "score").values.is_empty());
        assert!(keyword(&c, "title").values.is_empty());
    }

    #[test]
    fn identifiers_include_the_ambient_stdlib() {
        // Calling a builtin lick must complete: the stdlib names are ambient.
        let c = completions("");
        assert!(
            c.identifiers.contains(&"forward_roll".to_string()),
            "expected stdlib licks among identifiers, got {:?}",
            c.identifiers
        );
    }

    #[test]
    fn identifiers_include_the_documents_own_defs_and_lets() {
        let c = completions("def my_roll(c) { c.0 }\nlet riff = [3:0 2:0]\nscore { }");
        assert!(c.identifiers.contains(&"my_roll".to_string()));
        assert!(c.identifiers.contains(&"riff".to_string()));
    }

    #[test]
    fn identifiers_include_imported_names() {
        // With a provider supplying the library, the imported def completes.
        let provider =
            MapProvider::new().with_file("licks.ctab", "def lib_lick(c) { c.0 .t  c.1 .i }");
        let c = completions_with_provider("import \"licks.ctab\"\nscore { }", &provider);
        assert!(
            c.identifiers.contains(&"lib_lick".to_string()),
            "expected the imported def among identifiers, got {:?}",
            c.identifiers
        );
    }

    #[test]
    fn identifiers_are_sorted_and_deduplicated() {
        // A document def shadowing a stdlib name must not appear twice.
        let c = completions("def forward_roll(c) { c.0 }\nscore { }");
        let occurrences = c
            .identifiers
            .iter()
            .filter(|n| *n == "forward_roll")
            .count();
        assert_eq!(occurrences, 1);
        let mut sorted = c.identifiers.clone();
        sorted.sort();
        assert_eq!(c.identifiers, sorted);
    }
}
