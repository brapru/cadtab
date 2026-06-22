//! Beaming: grouping a measure's events into runs that share a beam, the first
//! step of rhythm rendering. This module is pure rhythm logic — it works from
//! durations and onsets alone and emits no geometry; stems, beams, and flags
//! are placed by the layout engine from the groups it returns.

use crate::model::{Duration, Event, EventKind, TimeSig};

/// A run of consecutive beamable events (eighths or shorter) that share one beat
/// and so are stemmed under a common beam. A singleton group is a lone beamable
/// note, which takes a flag rather than a beam.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BeamGroup {
    /// Indices into the measure's event slice, in order.
    pub members: Vec<usize>,
}

/// Whether an event takes a flag or beam: a note or chord shorter than a quarter.
/// Rests and quarter-or-longer notes carry neither and never join a group.
pub fn is_beamable(event: &Event) -> bool {
    match &event.kind {
        EventKind::Note(_) | EventKind::Chord(_) => event.duration() < Duration::new(1, 4),
        EventKind::Rest(_) => false,
    }
}

/// Partition a measure's events into beam groups by beat. Beamable events that
/// are contiguous and fall in the same beat group together; a rest, a longer
/// note, or a beat boundary closes the current group. Non-beamable events belong
/// to no group.
///
/// The beat is the meter denominator (`1/den`). Compound meters (6/8, 9/8) thus
/// beam per eighth for now rather than in dotted-quarter beats — a provisional
/// simplification; the common banjo case is 4/4.
pub fn beam_groups(events: &[Event], time: TimeSig) -> Vec<BeamGroup> {
    let beat_unit = Duration::new(1, u32::from(time.den.max(1)));
    let mut groups: Vec<BeamGroup> = Vec::new();
    let mut current: Option<(u64, Vec<usize>)> = None;
    let mut onset = Duration::zero();
    for (i, event) in events.iter().enumerate() {
        if is_beamable(event) {
            let beat = beat_index(onset, beat_unit);
            let same_beat = matches!(&current, Some((b, _)) if *b == beat);
            if same_beat {
                current.as_mut().unwrap().1.push(i);
            } else {
                close(&mut current, &mut groups);
                current = Some((beat, vec![i]));
            }
        } else {
            close(&mut current, &mut groups);
        }
        onset = onset.plus(event.duration());
    }
    close(&mut current, &mut groups);
    groups
}

/// Flush the open group, if any, into `groups`.
fn close(current: &mut Option<(u64, Vec<usize>)>, groups: &mut Vec<BeamGroup>) {
    if let Some((_, members)) = current.take() {
        groups.push(BeamGroup { members });
    }
}

/// Which beat an onset falls in: `floor(onset / beat_unit)`, computed in exact
/// integer arithmetic (widened so the products cannot overflow).
fn beat_index(onset: Duration, beat_unit: Duration) -> u64 {
    let num = u64::from(onset.num) * u64::from(beat_unit.den);
    let den = u64::from(onset.den) * u64::from(beat_unit.num);
    num / den
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Chord, ChordNote, Note, Position};
    use crate::span::Span;

    fn ev(kind: EventKind) -> Event {
        Event::new(kind, Span::new(0, 0))
    }

    fn note(den: u32) -> Event {
        ev(EventKind::Note(Note {
            pos: Position::new(3, 0),
            dur: Duration::from_denominator(den),
            right_hand: None,
            technique: None,
            tie: false,
        }))
    }

    fn chord(den: u32) -> Event {
        ev(EventKind::Chord(Chord {
            dur: Duration::from_denominator(den),
            notes: vec![ChordNote {
                pos: Position::new(1, 0),
                right_hand: None,
            }],
        }))
    }

    fn rest(den: u32) -> Event {
        ev(EventKind::Rest(Duration::from_denominator(den)))
    }

    fn group_sizes(groups: &[BeamGroup]) -> Vec<usize> {
        groups.iter().map(|g| g.members.len()).collect()
    }

    fn members(groups: &[BeamGroup]) -> Vec<Vec<usize>> {
        groups.iter().map(|g| g.members.clone()).collect()
    }

    #[test]
    fn two_eighths_in_a_beat_form_one_group() {
        let groups = beam_groups(&[note(8), note(8)], TimeSig::new(4, 4));
        assert_eq!(members(&groups), vec![vec![0, 1]]);
    }

    #[test]
    fn four_eighths_split_at_the_beat_boundary() {
        let m = [note(8), note(8), note(8), note(8)];
        let groups = beam_groups(&m, TimeSig::new(4, 4));
        assert_eq!(members(&groups), vec![vec![0, 1], vec![2, 3]]);
    }

    #[test]
    fn four_sixteenths_beam_within_one_beat() {
        let m = [note(16), note(16), note(16), note(16)];
        let groups = beam_groups(&m, TimeSig::new(4, 4));
        assert_eq!(members(&groups), vec![vec![0, 1, 2, 3]]);
    }

    #[test]
    fn quarter_notes_are_not_beamable() {
        let groups = beam_groups(&[note(4), note(4)], TimeSig::new(4, 4));
        assert!(groups.is_empty());
    }

    #[test]
    fn a_longer_note_closes_the_group() {
        // eighth, quarter, eighth, eighth: the quarter breaks beaming; onsets put
        // the trailing eighths in different beats, so each stands alone.
        let m = [note(8), note(4), note(8), note(8)];
        let groups = beam_groups(&m, TimeSig::new(4, 4));
        assert_eq!(members(&groups), vec![vec![0], vec![2], vec![3]]);
    }

    #[test]
    fn a_rest_splits_a_group_within_a_beat() {
        // Four sixteenth slots in beat 0: note, rest, note, note — the rest
        // breaks the run into a singleton and a surviving pair.
        let m = [note(16), rest(16), note(16), note(16)];
        let groups = beam_groups(&m, TimeSig::new(4, 4));
        assert_eq!(members(&groups), vec![vec![0], vec![2, 3]]);
    }

    #[test]
    fn a_lone_eighth_is_a_singleton_group() {
        let groups = beam_groups(&[note(8)], TimeSig::new(4, 4));
        assert_eq!(group_sizes(&groups), vec![1]);
    }

    #[test]
    fn chords_beam_like_notes() {
        let groups = beam_groups(&[chord(8), chord(8)], TimeSig::new(4, 4));
        assert_eq!(members(&groups), vec![vec![0, 1]]);
    }

    #[test]
    fn beat_unit_follows_the_meter_denominator() {
        // In 2/4 the beat is still a quarter, so eighths pair the same way.
        let m = [note(8), note(8), note(8), note(8)];
        let groups = beam_groups(&m, TimeSig::new(2, 4));
        assert_eq!(members(&groups), vec![vec![0, 1], vec![2, 3]]);
    }

    #[test]
    fn compound_meter_beams_per_eighth_for_now() {
        // Provisional: 6/8 beats per eighth, so six eighths are six singletons.
        let m = [note(8), note(8), note(8), note(8), note(8), note(8)];
        let groups = beam_groups(&m, TimeSig::new(6, 8));
        assert_eq!(group_sizes(&groups), vec![1, 1, 1, 1, 1, 1]);
    }

    #[test]
    fn an_empty_measure_has_no_groups() {
        assert!(beam_groups(&[], TimeSig::new(4, 4)).is_empty());
    }

    #[test]
    fn dotted_eighth_then_sixteenth_beam_together() {
        // 3/16 + 1/16 fills one beat; both are shorter than a quarter.
        let dotted = ev(EventKind::Note(Note {
            pos: Position::new(3, 0),
            dur: Duration::from_denominator(8).dotted(1),
            right_hand: None,
            technique: None,
            tie: false,
        }));
        let groups = beam_groups(&[dotted, note(16)], TimeSig::new(4, 4));
        assert_eq!(members(&groups), vec![vec![0, 1]]);
    }
}
