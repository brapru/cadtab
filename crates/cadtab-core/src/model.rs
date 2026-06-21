//! The musical model (IR): the instrument-agnostic values the layout engine
//! renders. A note's truth is its `(string, fret)` position; pitch is derived.
//! Source spans ride along through the model so a rendered primitive can map
//! back to the text that produced it.

use serde::{Deserialize, Serialize};

use crate::span::Span;

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

/// A duration as an exact fraction of a whole note (`1/8` = an eighth). Rational
/// so dotted notes and tuplets stay exact under accumulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Duration {
    pub num: u32,
    pub den: u32,
}

impl Duration {
    /// A fraction reduced to lowest terms. `den` must be nonzero.
    pub fn new(num: u32, den: u32) -> Self {
        debug_assert!(den != 0, "duration denominator must be nonzero");
        let g = gcd(num, den).max(1);
        Self {
            num: num / g,
            den: den / g,
        }
    }

    /// `1/den` of a whole note — the value of a bare `_den` suffix.
    pub fn from_denominator(den: u32) -> Self {
        Self::new(1, den)
    }

    /// Apply `dots` augmentation dots. Each dot adds half of the running value,
    /// so `k` dots scale by `(2^(k+1) − 1) / 2^k` (one dot = ×3/2).
    pub fn dotted(self, dots: u8) -> Self {
        let k = u32::from(dots.min(8));
        let factor_num = (1u32 << (k + 1)) - 1;
        let factor_den = 1u32 << k;
        Self::new(self.num * factor_num, self.den * factor_den)
    }
}

fn gcd(a: u32, b: u32) -> u32 {
    let (mut a, mut b) = (a, b);
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

/// A fretted position. `string` is 1-based, `1` = highest line in tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Position {
    pub string: u8,
    pub fret: u8,
}

impl Position {
    pub fn new(string: u8, fret: u8) -> Self {
        Self { string, fret }
    }
}

/// A right-hand finger (banjo / fingerstyle).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Finger {
    Thumb,
    Index,
    Middle,
}

/// A strum direction (guitar / chordal).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Strum {
    Down,
    Up,
}

/// An optional right-hand execution mark on a note.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RightHand {
    Finger(Finger),
    Strum(Strum),
}

/// A left-hand / expressive technique that draws a mark in the tab. Set only by
/// the surface technique functions, never authored as a raw field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Technique {
    HammerOn,
    PullOff,
    SlideTo,
    Bend,
    Choke,
    Ghost,
}

/// A single sounding note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub pos: Position,
    pub dur: Duration,
    pub right_hand: Option<RightHand>,
    pub technique: Option<Technique>,
    /// Ties into the following note.
    pub tie: bool,
}

/// One member of a chord/pinch: a position with an optional right-hand mark.
/// The duration is shared by the whole chord, so it is not repeated here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChordNote {
    pub pos: Position,
    pub right_hand: Option<RightHand>,
}

/// A chord/pinch: simultaneous notes under one shared duration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Chord {
    pub dur: Duration,
    pub notes: Vec<ChordNote>,
}

/// A timed event in a phrase, carrying the span of the source that produced it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub kind: EventKind,
    pub span: Span,
}

impl Event {
    pub fn new(kind: EventKind, span: Span) -> Self {
        Self { kind, span }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EventKind {
    Note(Note),
    Chord(Chord),
    Rest(Duration),
}

/// A sequence of events — the unit that licks produce and calls splice.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Phrase {
    pub events: Vec<Event>,
}

impl Phrase {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn push(&mut self, event: Event) {
        self.events.push(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::de::DeserializeOwned;
    use std::fmt::Debug as DebugTrait;

    fn round_trip<T: Serialize + DeserializeOwned + PartialEq + DebugTrait>(value: &T) {
        let json = serde_json::to_string(value).unwrap();
        let back: T = serde_json::from_str(&json).unwrap();
        assert_eq!(value, &back);
    }

    #[test]
    fn transpose_adds_semitones() {
        assert_eq!(Pitch(60).transposed(7), Pitch(67));
        assert_eq!(Pitch(60).transposed(-2), Pitch(58));
        assert_eq!(Pitch(55).transposed(0), Pitch(55));
    }

    #[test]
    fn pitch_serializes_as_a_bare_number() {
        assert_eq!(serde_json::to_string(&Pitch(62)).unwrap(), "62");
        assert_eq!(serde_json::from_str::<Pitch>("50").unwrap(), Pitch(50));
    }

    #[test]
    fn pitch_orders_by_semitone() {
        assert!(Pitch(50) < Pitch(62));
    }

    #[test]
    fn duration_reduces_to_lowest_terms() {
        assert_eq!(Duration::new(2, 8), Duration::new(1, 4));
        assert_eq!(Duration::new(4, 4), Duration::new(1, 1));
        assert_eq!(Duration::from_denominator(8), Duration { num: 1, den: 8 });
    }

    #[test]
    fn dotted_durations_are_exact() {
        // A dotted quarter is 3/8; double-dotted is 7/16.
        assert_eq!(Duration::from_denominator(4).dotted(0), Duration::new(1, 4));
        assert_eq!(Duration::from_denominator(4).dotted(1), Duration::new(3, 8));
        assert_eq!(
            Duration::from_denominator(4).dotted(2),
            Duration::new(7, 16)
        );
    }

    fn quarter_note(string: u8, fret: u8) -> Note {
        Note {
            pos: Position::new(string, fret),
            dur: Duration::from_denominator(4),
            right_hand: Some(RightHand::Finger(Finger::Thumb)),
            technique: None,
            tie: false,
        }
    }

    #[test]
    fn phrase_collects_events() {
        let mut p = Phrase::new();
        assert!(p.is_empty());
        p.push(Event::new(
            EventKind::Note(quarter_note(3, 0)),
            Span::new(0, 3),
        ));
        p.push(Event::new(
            EventKind::Rest(Duration::from_denominator(8)),
            Span::new(4, 6),
        ));
        assert_eq!(p.len(), 2);
        assert!(!p.is_empty());
    }

    #[test]
    fn model_types_round_trip() {
        round_trip(&Duration::new(3, 8));
        round_trip(&Position::new(1, 7));
        round_trip(&quarter_note(2, 0));
        round_trip(&Event::new(
            EventKind::Chord(Chord {
                dur: Duration::from_denominator(4),
                notes: vec![
                    ChordNote {
                        pos: Position::new(1, 0),
                        right_hand: Some(RightHand::Finger(Finger::Middle)),
                    },
                    ChordNote {
                        pos: Position::new(5, 0),
                        right_hand: Some(RightHand::Strum(Strum::Down)),
                    },
                ],
            }),
            Span::new(0, 9),
        ));
        round_trip(&Phrase {
            events: vec![Event::new(
                EventKind::Note(quarter_note(3, 2)),
                Span::new(0, 3),
            )],
        });
    }
}
