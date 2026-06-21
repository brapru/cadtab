//! The musical model (IR): instrument-agnostic values the layout engine
//! renders. Grown incrementally; this slice defines pitch.

use serde::{Deserialize, Serialize};

/// A MIDI-style semitone number (C4 = 60, A4 = 69). Tab needs no enharmonic
/// spelling, so a single integer suffices; pitch is always *derived* from a
/// fretted position, never authored directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Pitch(pub i16);

impl Pitch {
    /// The pitch `semitones` above (or below, if negative) this one.
    pub fn transposed(self, semitones: i16) -> Pitch {
        Pitch(self.0 + semitones)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transpose_adds_semitones() {
        assert_eq!(Pitch(60).transposed(7), Pitch(67));
        assert_eq!(Pitch(60).transposed(-2), Pitch(58));
        assert_eq!(Pitch(55).transposed(0), Pitch(55));
    }

    #[test]
    fn serializes_as_a_bare_number() {
        assert_eq!(serde_json::to_string(&Pitch(62)).unwrap(), "62");
        assert_eq!(serde_json::from_str::<Pitch>("50").unwrap(), Pitch(50));
    }

    #[test]
    fn orders_by_semitone() {
        assert!(Pitch(50) < Pitch(62));
    }
}
