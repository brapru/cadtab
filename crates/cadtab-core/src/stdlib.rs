//! The built-in lick library, embedded in the binary and available to every
//! score by default. The licks are ordinary, overridable `def`s — a user `def`
//! of the same name shadows the builtin. Provisional: the patterns will firm up
//! once tab renders and the language gets real use.

/// The standard rolls, as cadtab source.
const ROLLS: &str = include_str!("../stdlib/rolls.ctab");

/// The full embedded stdlib source. One file for now; concatenate as it grows.
pub fn source() -> &'static str {
    ROLLS
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    #[test]
    fn the_stdlib_parses_cleanly() {
        let parsed = parse(source());
        assert!(
            parsed.diagnostics.is_empty(),
            "embedded stdlib must parse cleanly: {:?}",
            parsed.diagnostics
        );
    }
}
