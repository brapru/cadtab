//! The musical model (IR): the instrument-agnostic values the layout engine
//! renders. A note's truth is its `(string, fret)` position; pitch is derived.
//! Source spans ride along through the model so a rendered primitive can map
//! back to the text that produced it.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use crate::instrument::Instrument;
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

    /// Parse scientific-pitch notation: a letter `A`–`G` (case-insensitive),
    /// optional accidentals (`#` raises, `b` lowers, repeatable), then a
    /// non-negative octave number. `C4` is middle C (MIDI 60). Returns `None`
    /// for anything malformed.
    pub fn from_name(text: &str) -> Option<Pitch> {
        let mut chars = text.chars();
        let letter = chars.next()?;
        let base = match letter.to_ascii_uppercase() {
            'C' => 0,
            'D' => 2,
            'E' => 4,
            'F' => 5,
            'G' => 7,
            'A' => 9,
            'B' => 11,
            _ => return None,
        };
        let mut chars = chars.peekable();
        let mut accidental: i16 = 0;
        loop {
            match chars.peek() {
                Some('#') => accidental += 1,
                Some('b') => accidental -= 1,
                _ => break,
            }
            chars.next();
        }
        let octave_str: String = chars.collect();
        if octave_str.is_empty() || !octave_str.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let octave: i16 = octave_str.parse().ok()?;
        // MIDI: octave -1 holds C=0; C4 = 60.
        Some(Pitch((octave + 1) * 12 + base + accidental))
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

    /// The zero duration.
    pub fn zero() -> Self {
        Self { num: 0, den: 1 }
    }

    /// Whether this is the zero duration.
    pub fn is_zero(self) -> bool {
        self.num == 0
    }

    /// The sum of two durations, reduced.
    pub fn plus(self, other: Duration) -> Duration {
        Duration::new(
            self.num * other.den + other.num * self.den,
            self.den * other.den,
        )
    }

    /// The difference `self - other`, reduced; saturates at zero when `other` is
    /// larger (callers subtract only a duration known not to exceed `self`).
    pub fn minus(self, other: Duration) -> Duration {
        let num = (self.num * other.den).saturating_sub(other.num * self.den);
        Duration::new(num, self.den * other.den)
    }
}

impl PartialOrd for Duration {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Duration {
    /// Compare as rationals by cross-multiplication (widened to avoid overflow).
    fn cmp(&self, other: &Self) -> Ordering {
        (u64::from(self.num) * u64::from(other.den))
            .cmp(&(u64::from(other.num) * u64::from(self.den)))
    }
}

fn gcd(a: u32, b: u32) -> u32 {
    let (mut a, mut b) = (a, b);
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

/// A time signature: `num` beats of a `den`-th note per bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimeSig {
    pub num: u8,
    pub den: u8,
}

impl TimeSig {
    pub fn new(num: u8, den: u8) -> Self {
        Self { num, den }
    }

    /// One bar's length as a fraction of a whole note (`num/den`).
    pub fn bar_len(self) -> Duration {
        Duration::new(u32::from(self.num), u32::from(self.den))
    }
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

    /// The duration this event occupies.
    pub fn duration(&self) -> Duration {
        self.kind.duration()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EventKind {
    Note(Note),
    Chord(Chord),
    Rest(Duration),
}

impl EventKind {
    /// The duration this event occupies.
    pub fn duration(&self) -> Duration {
        match self {
            EventKind::Note(n) => n.dur,
            EventKind::Chord(c) => c.dur,
            EventKind::Rest(d) => *d,
        }
    }
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

/// One bar of the score. The compiler owns barline placement, so a measure is a
/// run of events bounded by barlines. `meter` is set only where the meter
/// changes; the repeat/ending/pickup flags are populated by later passes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Measure {
    pub events: Vec<Event>,
    pub meter: Option<TimeSig>,
    pub repeat_start: bool,
    pub repeat_end: bool,
    pub ending: Option<u8>,
    pub is_pickup: bool,
}

impl Measure {
    /// A measure holding `events`, with no meter change and all flags clear.
    pub fn new(events: Vec<Event>) -> Self {
        Self {
            events,
            meter: None,
            repeat_start: false,
            repeat_end: false,
            ending: None,
            is_pickup: false,
        }
    }
}

/// Song metadata rendered into the sheet header.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoreMeta {
    pub title: Option<String>,
    pub composer: Option<String>,
    pub tempo: Option<u16>,
}

/// A fully evaluated score: metadata, the resolved instrument, display-only capo
/// labels, and the barred measures. The layout engine is a pure function of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Score {
    pub meta: ScoreMeta,
    pub instrument: Instrument,
    pub capo: Vec<String>,
    pub measures: Vec<Measure>,
}

/// Where an event falls in barred time: the bar it starts in and its onset (the
/// time already filled in that bar) when it begins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Beat {
    pub bar: usize,
    pub onset: Duration,
}

/// Walks an event stream accumulating rational time within fixed-length bars,
/// reporting each event's onset and rolling over into later bars as they fill.
/// The pure core of auto-barring: it counts time but neither splits nor
/// diagnoses (those build on the positions it reports).
#[derive(Debug, Clone)]
pub struct BeatAccumulator {
    bar_len: Duration,
    bar: usize,
    position: Duration,
}

impl BeatAccumulator {
    pub fn new(time: TimeSig) -> Self {
        Self {
            bar_len: time.bar_len(),
            bar: 0,
            position: Duration::zero(),
        }
    }

    /// Place an event of length `dur`: return where it begins, then advance,
    /// rolling past every bar the event completes.
    pub fn push(&mut self, dur: Duration) -> Beat {
        let beat = Beat {
            bar: self.bar,
            onset: self.position,
        };
        self.position = self.position.plus(dur);
        while !self.bar_len.is_zero() && self.position >= self.bar_len {
            self.position = self.position.minus(self.bar_len);
            self.bar += 1;
        }
        beat
    }

    /// Place an event, using its own duration.
    pub fn push_event(&mut self, event: &Event) -> Beat {
        self.push(event.duration())
    }

    /// The bar currently being filled.
    pub fn bar(&self) -> usize {
        self.bar
    }

    /// The time filled so far in the current bar.
    pub fn position(&self) -> Duration {
        self.position
    }

    /// The unfilled time remaining in the current bar.
    pub fn remaining(&self) -> Duration {
        self.bar_len.minus(self.position)
    }

    /// Whether the accumulator sits exactly on a barline (the current bar is
    /// empty) — false once any event has partly filled it.
    pub fn on_barline(&self) -> bool {
        self.position.is_zero()
    }
}

/// Split a flat event stream into measures under one time signature, inserting a
/// barline (a measure boundary) each time an event completes — or overflows — a
/// bar. An event that overflows stays whole in the bar it began; a trailing
/// partial bar becomes a final measure. Meter is left unset on every measure;
/// the caller, which tracks meter changes, stamps it.
pub fn split_measures(events: Vec<Event>, time: TimeSig) -> Vec<Measure> {
    let mut acc = BeatAccumulator::new(time);
    let mut measures = Vec::new();
    let mut current = Vec::new();
    for event in events {
        let start_bar = acc.bar();
        acc.push_event(&event);
        current.push(event);
        if acc.bar() != start_bar {
            measures.push(Measure::new(std::mem::take(&mut current)));
        }
    }
    if !current.is_empty() {
        measures.push(Measure::new(current));
    }
    measures
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
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
    fn pitch_from_name_parses_scientific_notation() {
        // Middle C and the A above it anchor the octave/letter math.
        assert_eq!(Pitch::from_name("C4"), Some(Pitch(60)));
        assert_eq!(Pitch::from_name("A4"), Some(Pitch(69)));
        // The open banjo strings, written out (D4 B3 G3 D3 g4).
        assert_eq!(Pitch::from_name("D4"), Some(Pitch(62)));
        assert_eq!(Pitch::from_name("B3"), Some(Pitch(59)));
        assert_eq!(Pitch::from_name("G3"), Some(Pitch(55)));
        assert_eq!(Pitch::from_name("D3"), Some(Pitch(50)));
        // Letter case is ignored (the high drone is conventionally lowercase).
        assert_eq!(Pitch::from_name("g4"), Some(Pitch(67)));
        // Accidentals raise/lower and stack.
        assert_eq!(Pitch::from_name("F#4"), Some(Pitch(66)));
        assert_eq!(Pitch::from_name("Bb3"), Some(Pitch(58)));
        assert_eq!(Pitch::from_name("C#4"), Pitch::from_name("Db4"));
        assert_eq!(Pitch::from_name("Fbb4"), Some(Pitch(63)));
    }

    #[test]
    fn pitch_from_name_rejects_malformed() {
        for bad in ["", "H4", "4", "C", "C#", "Cx4", "C4.5", "C-1"] {
            assert_eq!(Pitch::from_name(bad), None, "{bad} should not parse");
        }
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

    #[test]
    fn duration_adds_and_subtracts() {
        assert_eq!(
            Duration::new(1, 4).plus(Duration::new(1, 4)),
            Duration::new(1, 2)
        );
        assert_eq!(
            Duration::new(3, 8).plus(Duration::from_denominator(8)),
            Duration::new(1, 2)
        );
        assert_eq!(
            Duration::new(1, 2).minus(Duration::new(1, 4)),
            Duration::new(1, 4)
        );
        assert_eq!(
            Duration::zero().plus(Duration::new(1, 8)),
            Duration::new(1, 8)
        );
        // Subtraction past zero saturates.
        assert_eq!(
            Duration::new(1, 8).minus(Duration::new(1, 4)),
            Duration::zero()
        );
        assert!(Duration::zero().is_zero());
        assert!(!Duration::new(1, 16).is_zero());
    }

    #[test]
    fn duration_orders_as_a_rational() {
        assert!(Duration::new(1, 4) < Duration::new(1, 2));
        assert!(Duration::new(3, 4) > Duration::new(2, 3));
        assert_eq!(
            Duration::new(1, 2).cmp(&Duration::new(2, 4)),
            Ordering::Equal
        );
        let mut ds = vec![
            Duration::new(1, 2),
            Duration::new(1, 8),
            Duration::new(1, 4),
        ];
        ds.sort();
        assert_eq!(
            ds,
            vec![
                Duration::new(1, 8),
                Duration::new(1, 4),
                Duration::new(1, 2)
            ]
        );
    }

    #[test]
    fn time_sig_bar_length() {
        assert_eq!(TimeSig::new(4, 4).bar_len(), Duration::new(1, 1));
        assert_eq!(TimeSig::new(6, 8).bar_len(), Duration::new(3, 4));
        assert_eq!(TimeSig::new(3, 4).bar_len(), Duration::new(3, 4));
        assert_eq!(TimeSig::new(2, 2).bar_len(), Duration::new(1, 1));
    }

    #[test]
    fn event_reports_its_duration() {
        let note = Event::new(EventKind::Note(quarter_note(3, 0)), Span::new(0, 3));
        assert_eq!(note.duration(), Duration::from_denominator(4));
        let rest = Event::new(
            EventKind::Rest(Duration::from_denominator(8)),
            Span::new(0, 1),
        );
        assert_eq!(rest.duration(), Duration::from_denominator(8));
    }

    fn eighth() -> Duration {
        Duration::from_denominator(8)
    }

    #[test]
    fn accumulator_reports_onsets_within_a_bar() {
        let mut acc = BeatAccumulator::new(TimeSig::new(4, 4));
        let quarter = Duration::from_denominator(4);
        let onsets: Vec<Beat> = (0..4).map(|_| acc.push(quarter)).collect();
        assert_eq!(
            onsets,
            vec![
                Beat {
                    bar: 0,
                    onset: Duration::zero()
                },
                Beat {
                    bar: 0,
                    onset: Duration::new(1, 4)
                },
                Beat {
                    bar: 0,
                    onset: Duration::new(1, 2)
                },
                Beat {
                    bar: 0,
                    onset: Duration::new(3, 4)
                },
            ]
        );
        // The four quarters exactly fill the bar.
        assert_eq!(acc.bar(), 1);
        assert!(acc.on_barline());
    }

    #[test]
    fn accumulator_rolls_over_completed_bars() {
        let mut acc = BeatAccumulator::new(TimeSig::new(4, 4));
        let quarter = Duration::from_denominator(4);
        let bars: Vec<usize> = (0..8).map(|_| acc.push(quarter).bar).collect();
        assert_eq!(bars, vec![0, 0, 0, 0, 1, 1, 1, 1]);
        assert_eq!(acc.bar(), 2);
        assert!(acc.on_barline());
    }

    #[test]
    fn accumulator_handles_an_event_crossing_a_barline() {
        let mut acc = BeatAccumulator::new(TimeSig::new(4, 4));
        assert_eq!(acc.push(Duration::new(3, 4)).onset, Duration::zero());
        // A half note starting at 3/4 spills 1/4 into the next bar.
        let crossing = acc.push(Duration::new(1, 2));
        assert_eq!(
            crossing,
            Beat {
                bar: 0,
                onset: Duration::new(3, 4)
            }
        );
        assert_eq!(acc.bar(), 1);
        assert_eq!(acc.position(), Duration::new(1, 4));
    }

    #[test]
    fn accumulator_tracks_remaining_and_barline() {
        let mut acc = BeatAccumulator::new(TimeSig::new(4, 4));
        acc.push(eighth());
        assert_eq!(acc.remaining(), Duration::new(7, 8));
        assert!(!acc.on_barline());
        acc.push(Duration::new(7, 8));
        assert!(acc.on_barline());
        assert_eq!(acc.remaining(), Duration::new(1, 1));
    }

    proptest! {
        /// All pushed time is conserved: completed bars plus the leftover
        /// position always re-sum to the total of the event durations.
        #[test]
        fn beat_accumulation_conserves_total_time(
            num in 1u8..=12,
            den in prop::sample::select(vec![2u8, 4, 8]),
            durs in prop::collection::vec(
                (1u32..=8, prop::sample::select(vec![1u32, 2, 4, 8, 16]))
                    .prop_map(|(n, d)| Duration::new(n, d)),
                0..12,
            ),
        ) {
            let time = TimeSig::new(num, den);
            let mut acc = BeatAccumulator::new(time);
            for d in &durs {
                acc.push(*d);
            }
            let sum = durs.iter().fold(Duration::zero(), |a, d| a.plus(*d));
            let completed = Duration::new(time.bar_len().num * acc.bar() as u32, time.bar_len().den);
            prop_assert_eq!(completed.plus(acc.position()), sum);
        }
    }

    /// A rest event of duration `d` — splitting cares only about durations.
    fn ev(d: Duration) -> Event {
        Event::new(EventKind::Rest(d), Span::new(0, 0))
    }

    /// The number of events in each measure, in order.
    fn measure_sizes(measures: &[Measure]) -> Vec<usize> {
        measures.iter().map(|m| m.events.len()).collect()
    }

    #[test]
    fn splits_full_bars() {
        let quarter = Duration::from_denominator(4);
        let events: Vec<Event> = (0..8).map(|_| ev(quarter)).collect();
        let measures = split_measures(events, TimeSig::new(4, 4));
        assert_eq!(measure_sizes(&measures), vec![4, 4]);
    }

    #[test]
    fn an_exactly_full_bar_is_one_measure() {
        let quarter = Duration::from_denominator(4);
        let measures = split_measures(vec![ev(quarter); 4], TimeSig::new(4, 4));
        assert_eq!(measure_sizes(&measures), vec![4]);
    }

    #[test]
    fn a_trailing_partial_bar_becomes_a_measure() {
        let quarter = Duration::from_denominator(4);
        let measures = split_measures(vec![ev(quarter); 5], TimeSig::new(4, 4));
        assert_eq!(measure_sizes(&measures), vec![4, 1]);
    }

    #[test]
    fn splits_mixed_durations_on_beat_boundaries() {
        // 1/2 1/4 1/4 | 1/2 1/2
        let measures = split_measures(
            vec![
                ev(Duration::new(1, 2)),
                ev(Duration::new(1, 4)),
                ev(Duration::new(1, 4)),
                ev(Duration::new(1, 2)),
                ev(Duration::new(1, 2)),
            ],
            TimeSig::new(4, 4),
        );
        assert_eq!(measure_sizes(&measures), vec![3, 2]);
    }

    #[test]
    fn an_overflowing_event_stays_in_the_bar_it_began() {
        // 3/4 then 1/2 crosses the barline; it closes bar 0. 3/4 then fills bar 1.
        let measures = split_measures(
            vec![
                ev(Duration::new(3, 4)),
                ev(Duration::new(1, 2)),
                ev(Duration::new(3, 4)),
            ],
            TimeSig::new(4, 4),
        );
        assert_eq!(measure_sizes(&measures), vec![2, 1]);
    }

    #[test]
    fn an_empty_stream_has_no_measures() {
        assert!(split_measures(vec![], TimeSig::new(4, 4)).is_empty());
    }

    #[test]
    fn split_leaves_meter_unset_for_the_caller() {
        let quarter = Duration::from_denominator(4);
        let measures = split_measures(vec![ev(quarter); 8], TimeSig::new(3, 4));
        assert!(measures.iter().all(|m| m.meter.is_none()));
    }

    #[test]
    fn measure_round_trips() {
        let mut m = Measure::new(vec![ev(Duration::from_denominator(4))]);
        m.meter = Some(TimeSig::new(4, 4));
        round_trip(&m);
    }
}
