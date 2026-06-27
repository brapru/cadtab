//! Builtin instruments, named tuning overrides, and pitch derivation. A note's
//! truth is its `(string, fret)` position; the sounding pitch is computed as
//! `open_pitch[string] + fret` (position-canonical, pitch-derived). String
//! numbering is 1-based with `1` = highest line in tab; `Vec` index 0 is
//! string 1.

use serde::{Deserialize, Serialize};

use crate::diagnostics::Diagnostic;
use crate::model::Pitch;
use crate::span::Span;

/// One playable string: its open (unfretted) pitch and a display label.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StringDef {
    pub open_pitch: Pitch,
    pub label: String,
}

/// An instrument: an ordered set of strings, string 1 first, plus the display
/// name of its current tuning (e.g. "Open G", "Double C") for the sheet header.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Instrument {
    pub name: String,
    pub tuning: String,
    pub strings: Vec<StringDef>,
}

/// Frets past this are almost certainly a typo, not a real position.
pub const MAX_FRET: u32 = 24;

/// An open-string layout: `(label, MIDI semitone)` from string 1 to string n.
type TuningSpec = &'static [(&'static str, i16)];

/// A tuning's lookup key (the `tuning` directive name), its header display name,
/// and its open-string layout.
struct Tuning {
    key: &'static str,
    display: &'static str,
    spec: TuningSpec,
}

// MIDI semitone numbers (C4 = 60), listed string 1 → string n.
//
// Banjo numbering keeps the short 5th-string drone last even though it sounds
// high; this matches standard tab.
const BANJO_OPEN_G: TuningSpec = &[("D", 62), ("B", 59), ("G", 55), ("D", 50), ("g", 67)];
const BANJO_DOUBLE_C: TuningSpec = &[("D", 62), ("C", 60), ("G", 55), ("C", 48), ("g", 67)];
const BANJO_SAWMILL: TuningSpec = &[("D", 62), ("C", 60), ("G", 55), ("D", 50), ("g", 67)];
const GUITAR_STANDARD: TuningSpec = &[
    ("E", 64),
    ("B", 59),
    ("G", 55),
    ("D", 50),
    ("A", 45),
    ("E", 40),
];
const GUITAR_DROP_D: TuningSpec = &[
    ("E", 64),
    ("B", 59),
    ("G", 55),
    ("D", 50),
    ("A", 45),
    ("D", 38),
];

/// The known tunings available to a `tuning` override.
const TUNINGS: &[Tuning] = &[
    Tuning {
        key: "openG",
        display: "Open G",
        spec: BANJO_OPEN_G,
    },
    Tuning {
        key: "doubleC",
        display: "Double C",
        spec: BANJO_DOUBLE_C,
    },
    Tuning {
        key: "sawmill",
        display: "Sawmill",
        spec: BANJO_SAWMILL,
    },
    Tuning {
        key: "standard",
        display: "Standard",
        spec: GUITAR_STANDARD,
    },
    Tuning {
        key: "dropD",
        display: "Drop D",
        spec: GUITAR_DROP_D,
    },
];

fn named_tuning(name: &str) -> Option<&'static Tuning> {
    TUNINGS.iter().find(|t| t.key == name)
}

fn known_tuning_names() -> String {
    TUNINGS.iter().map(|t| t.key).collect::<Vec<_>>().join(", ")
}

impl Instrument {
    /// Resolve a builtin instrument (with its default tuning) by name.
    pub fn builtin(name: &str) -> Option<Instrument> {
        // Each builtin opens in a default tuning, named for the header.
        let default_tuning = match name {
            "banjo" => "openG",
            "guitar" => "standard",
            _ => return None,
        };
        let tuning = named_tuning(default_tuning).expect("builtin default tuning is known");
        Some(Instrument::from_spec(name, tuning))
    }

    fn from_spec(name: &str, tuning: &Tuning) -> Instrument {
        Instrument {
            name: name.to_string(),
            tuning: tuning.display.to_string(),
            strings: tuning
                .spec
                .iter()
                .map(|&(label, midi)| StringDef {
                    open_pitch: Pitch(midi),
                    label: label.to_string(),
                })
                .collect(),
        }
    }

    pub fn string_count(&self) -> usize {
        self.strings.len()
    }

    /// The open-string definition for 1-based `string`, if in range.
    pub fn string_def(&self, string: u32) -> Option<&StringDef> {
        let idx = string.checked_sub(1)?;
        self.strings.get(idx as usize)
    }

    /// Apply a named `tuning` override, replacing open-string pitches. Errors if
    /// the tuning is unknown or its string count differs from this instrument's.
    pub fn with_tuning(&self, name: &str, span: Span) -> Result<Instrument, Diagnostic> {
        let tuning = named_tuning(name).ok_or_else(|| {
            Diagnostic::error(span, format!("unknown tuning `{name}`"))
                .with_help(format!("known tunings: {}", known_tuning_names()))
        })?;
        if tuning.spec.len() != self.string_count() {
            return Err(Diagnostic::error(
                span,
                format!(
                    "tuning `{name}` is for {} strings but {} has {}",
                    tuning.spec.len(),
                    self.name,
                    self.string_count()
                ),
            ));
        }
        Ok(Instrument::from_spec(&self.name, tuning))
    }

    /// Derive the sounding pitch of a fretted position, validating bounds.
    /// `open_pitch[string] + fret`; the error carries a diagnostic at `span`.
    pub fn pitch_at(&self, string: u32, fret: u32, span: Span) -> Result<Pitch, Diagnostic> {
        let def =
            self.string_def(string).ok_or_else(|| {
                Diagnostic::error(span, format!("string {string} is out of range")).with_help(
                    format!("{} has strings 1–{}", self.name, self.string_count()),
                )
            })?;
        if fret > MAX_FRET {
            return Err(
                Diagnostic::error(span, format!("fret {fret} is unreasonably high"))
                    .with_help(format!("frets range 0–{MAX_FRET}")),
            );
        }
        Ok(def.open_pitch.transposed(fret as i16))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::Severity;
    use serde::de::DeserializeOwned;
    use std::fmt::Debug;

    fn sp() -> Span {
        Span::new(0, 3)
    }

    fn open_pitches(instr: &Instrument) -> Vec<i16> {
        (1..=instr.string_count() as u32)
            .map(|s| instr.pitch_at(s, 0, sp()).unwrap().0)
            .collect()
    }

    #[test]
    fn banjo_open_g_open_strings() {
        let banjo = Instrument::builtin("banjo").unwrap();
        assert_eq!(banjo.string_count(), 5);
        // strings 1..5: D4 B3 G3 D3 g4
        assert_eq!(open_pitches(&banjo), vec![62, 59, 55, 50, 67]);
    }

    #[test]
    fn guitar_standard_open_strings() {
        let guitar = Instrument::builtin("guitar").unwrap();
        assert_eq!(guitar.string_count(), 6);
        // strings 1..6: E4 B3 G3 D3 A2 E2
        assert_eq!(open_pitches(&guitar), vec![64, 59, 55, 50, 45, 40]);
    }

    #[test]
    fn fretted_pitch_adds_fret_to_open_string() {
        let banjo = Instrument::builtin("banjo").unwrap();
        // string 3 (G3=55) at fret 2 → A3=57.
        assert_eq!(banjo.pitch_at(3, 2, sp()).unwrap(), Pitch(57));
        // string 1 (D4=62) at fret 5 → G4=67.
        assert_eq!(banjo.pitch_at(1, 5, sp()).unwrap(), Pitch(67));
    }

    #[test]
    fn unknown_instrument_is_none() {
        assert!(Instrument::builtin("mandolin").is_none());
    }

    #[test]
    fn tuning_override_replaces_open_pitches() {
        let banjo = Instrument::builtin("banjo").unwrap();
        // double C: string 2 is C4 (60), string 4 is C3 (48).
        let double_c = banjo.with_tuning("doubleC", sp()).unwrap();
        assert_eq!(open_pitches(&double_c), vec![62, 60, 55, 48, 67]);
        // sawmill: string 2 is C4 (60), string 4 stays D3 (50).
        let sawmill = banjo.with_tuning("sawmill", sp()).unwrap();
        assert_eq!(open_pitches(&sawmill), vec![62, 60, 55, 50, 67]);
        // name is preserved across a retuning.
        assert_eq!(double_c.name, "banjo");
    }

    #[test]
    fn tuning_display_name_is_carried() {
        // Builtins open in their default tuning, named for the header.
        assert_eq!(Instrument::builtin("banjo").unwrap().tuning, "Open G");
        assert_eq!(Instrument::builtin("guitar").unwrap().tuning, "Standard");
        // A retuning swaps in the override's display name.
        let banjo = Instrument::builtin("banjo").unwrap();
        assert_eq!(
            banjo.with_tuning("doubleC", sp()).unwrap().tuning,
            "Double C"
        );
        assert_eq!(
            banjo.with_tuning("sawmill", sp()).unwrap().tuning,
            "Sawmill"
        );
    }

    #[test]
    fn redundant_default_tuning_is_identity() {
        let banjo = Instrument::builtin("banjo").unwrap();
        assert_eq!(banjo.with_tuning("openG", sp()).unwrap(), banjo);
    }

    #[test]
    fn unknown_tuning_diagnoses() {
        let banjo = Instrument::builtin("banjo").unwrap();
        let err = banjo.with_tuning("nashville", sp()).unwrap_err();
        assert_eq!(err.severity, Severity::Error);
        assert!(err.message.contains("unknown tuning"));
        assert!(err.help.unwrap().contains("openG"));
    }

    #[test]
    fn mismatched_string_count_diagnoses() {
        // dropD is a 6-string guitar tuning; it cannot retune the 5-string banjo.
        let banjo = Instrument::builtin("banjo").unwrap();
        let err = banjo.with_tuning("dropD", sp()).unwrap_err();
        assert!(err.message.contains("6 strings"));
    }

    #[test]
    fn string_out_of_range_diagnoses() {
        let banjo = Instrument::builtin("banjo").unwrap();
        for bad in [0, 6, 99] {
            let err = banjo.pitch_at(bad, 0, sp()).unwrap_err();
            assert!(err.message.contains("out of range"));
            assert!(err.help.unwrap().contains("1–5"));
        }
    }

    #[test]
    fn fret_too_high_diagnoses() {
        let banjo = Instrument::builtin("banjo").unwrap();
        assert!(banjo.pitch_at(1, MAX_FRET, sp()).is_ok());
        let err = banjo.pitch_at(1, MAX_FRET + 1, sp()).unwrap_err();
        assert!(err.message.contains("unreasonably high"));
    }

    fn round_trip<T: Serialize + DeserializeOwned + PartialEq + Debug>(value: &T) {
        let json = serde_json::to_string(value).unwrap();
        let back: T = serde_json::from_str(&json).unwrap();
        assert_eq!(value, &back);
    }

    #[test]
    fn instrument_round_trips() {
        round_trip(&Instrument::builtin("banjo").unwrap());
        round_trip(&Instrument::builtin("guitar").unwrap());
    }

    /// A readable fret-to-pitch table across the low frets of every builtin and
    /// tuning, pinned as a snapshot so retunings stay honest.
    #[test]
    fn pitch_derivation_table_snapshot() {
        let mut out = String::new();
        let cases: &[(&str, Option<Instrument>)] = &[
            ("banjo openG", Instrument::builtin("banjo")),
            (
                "banjo doubleC",
                Instrument::builtin("banjo")
                    .unwrap()
                    .with_tuning("doubleC", sp())
                    .ok(),
            ),
            (
                "banjo sawmill",
                Instrument::builtin("banjo")
                    .unwrap()
                    .with_tuning("sawmill", sp())
                    .ok(),
            ),
            ("guitar standard", Instrument::builtin("guitar")),
            (
                "guitar dropD",
                Instrument::builtin("guitar")
                    .unwrap()
                    .with_tuning("dropD", sp())
                    .ok(),
            ),
        ];
        for (title, instr) in cases {
            let instr = instr.as_ref().unwrap();
            out.push_str(title);
            out.push('\n');
            for (i, s) in instr.strings.iter().enumerate() {
                let string = i as u32 + 1;
                let frets: Vec<String> = (0..=4)
                    .map(|f| instr.pitch_at(string, f, sp()).unwrap().0.to_string())
                    .collect();
                out.push_str(&format!(
                    "  string {string} ({}): {}\n",
                    s.label,
                    frets.join(" ")
                ));
            }
        }
        insta::assert_snapshot!(out);
    }
}
