//! The layout engine: a pure function from the musical `Score` to a positioned
//! `RenderTree`. Coordinates are logical (1 unit = string spacing); the painter
//! scales them via the SVG viewBox.
//!
//! This pass owns the vertical axis — the header block, the string lines, and
//! the string->line mapping that places fret numbers (string 1 = top line). The
//! horizontal axis here is provisional even spacing in a single system; later
//! passes make it time-proportional, add barlines, and break into systems.

use serde::{Deserialize, Serialize};

use crate::model::{EventKind, Measure, Score};
use crate::render::{LayoutMeta, MeasureBox, Primitive, Rect, RenderTree, System, TextRole};
use crate::span::Span;

/// Inputs that parameterize layout. The same engine serves the screen (viewport
/// width) and export (fixed page width).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutConfig {
    pub width: f32,
}

// Vertical metrics (logical units; 1 unit = string spacing).
const TOP_MARGIN: f32 = 0.5;
const TITLE_H: f32 = 2.0;
const COMPOSER_H: f32 = 1.2;
const META_LINE_H: f32 = 1.0;
const HEADER_GAP: f32 = 1.0;
const STRING_SPACING: f32 = 1.0;
const BOTTOM_MARGIN: f32 = 2.0;

// Horizontal metrics.
const LEFT_MARGIN: f32 = 2.0;
const RIGHT_MARGIN: f32 = 1.0;
const LABEL_X: f32 = 1.0;
const EVENT_ADVANCE: f32 = 2.0;

// The half-width of the gap a fret number opens in the string line behind it.
const NUMBER_GAP: f32 = 0.6;
const STRING_WEIGHT: f32 = 0.06;

/// Lay a `Score` out into a positioned render tree.
pub fn layout(score: &Score, _config: LayoutConfig) -> RenderTree {
    let n_strings = score.instrument.string_count();
    let event_count: usize = score.measures.iter().map(|m| m.events.len()).sum();

    let staff_x1 = LEFT_MARGIN + (event_count.max(1) as f32) * EVENT_ADVANCE;
    let width = staff_x1 + RIGHT_MARGIN;

    let (header, header_bottom) = build_header(score, width);
    let staff_top = header_bottom + HEADER_GAP;
    let staff_height = ((n_strings.max(1) - 1) as f32) * STRING_SPACING;
    let height = staff_top + staff_height + BOTTOM_MARGIN;

    let line_y = |string: u8| staff_top + (f32::from(string.saturating_sub(1))) * STRING_SPACING;

    // Place every event left-to-right, collecting fret-number primitives per
    // measure and the x of each number on each string (for line breaking).
    let mut number_xs: Vec<Vec<f32>> = vec![Vec::new(); n_strings];
    let mut measures = Vec::with_capacity(score.measures.len());
    let mut index = 0usize;
    for measure in &score.measures {
        let start = index;
        let mut prims = Vec::new();
        for event in &measure.events {
            let x = event_x(index);
            index += 1;
            for (string, fret) in fretted_positions(&event.kind) {
                if (1..=n_strings as u8).contains(&string) {
                    number_xs[(string - 1) as usize].push(x);
                }
                prims.push(Primitive::Text {
                    x,
                    y: line_y(string),
                    content: fret.to_string(),
                    role: TextRole::FretNumber,
                    span: Some(event.span),
                });
            }
        }
        let span = measure_span(measure);
        let bounds = Rect {
            x: event_x(start) - EVENT_ADVANCE / 2.0,
            y: staff_top,
            w: (index - start).max(1) as f32 * EVENT_ADVANCE,
            h: staff_height.max(STRING_SPACING),
        };
        measures.push(MeasureBox {
            bounds,
            prims,
            span,
        });
    }

    // System furniture: the string lines (broken behind their numbers) and the
    // leading open-string labels.
    let mut sys_prims = Vec::new();
    for (i, xs) in number_xs.iter().enumerate() {
        let y = line_y((i + 1) as u8);
        sys_prims.extend(string_line(LEFT_MARGIN, staff_x1, y, xs));
        sys_prims.push(Primitive::Text {
            x: LABEL_X,
            y,
            content: score.instrument.strings[i].label.clone(),
            role: TextRole::StringLabel,
            span: None,
        });
    }

    let system = System {
        bounds: Rect {
            x: 0.0,
            y: staff_top,
            w: width,
            h: staff_height.max(STRING_SPACING),
        },
        prims: sys_prims,
        measures,
    };

    RenderTree {
        meta: LayoutMeta { width, height },
        header,
        systems: vec![system],
    }
}

/// The x center of the `index`-th event in the (single, provisional) system.
fn event_x(index: usize) -> f32 {
    LEFT_MARGIN + (index as f32 + 0.5) * EVENT_ADVANCE
}

/// The `(string, fret)` positions an event draws as numbers: a note's own, every
/// chord member's, or none for a rest.
fn fretted_positions(kind: &EventKind) -> Vec<(u8, u8)> {
    match kind {
        EventKind::Note(n) => vec![(n.pos.string, n.pos.fret)],
        EventKind::Chord(c) => c
            .notes
            .iter()
            .map(|cn| (cn.pos.string, cn.pos.fret))
            .collect(),
        EventKind::Rest(_) => Vec::new(),
    }
}

/// The smallest span covering a measure's events, or `None` if it is empty.
fn measure_span(measure: &Measure) -> Option<Span> {
    measure
        .events
        .iter()
        .map(|e| e.span)
        .reduce(|acc, s| acc.merge(s))
}

/// One string line from `x0` to `x1` at height `y`, broken into segments that
/// skip a gap behind each fret-number x in `xs`.
fn string_line(x0: f32, x1: f32, y: f32, xs: &[f32]) -> Vec<Primitive> {
    let mut breaks: Vec<f32> = xs.to_vec();
    breaks.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mut segments = Vec::new();
    let mut cursor = x0;
    for cx in breaks {
        let gap_start = cx - NUMBER_GAP;
        if gap_start > cursor {
            segments.push(line(cursor, x1.min(gap_start), y));
        }
        cursor = cursor.max(cx + NUMBER_GAP);
    }
    if cursor < x1 {
        segments.push(line(cursor, x1, y));
    }
    segments
}

fn line(x1: f32, x2: f32, y: f32) -> Primitive {
    Primitive::Line {
        x1,
        y1: y,
        x2,
        y2: y,
        weight: STRING_WEIGHT,
    }
}

/// Build the header block of metadata text, centered on `width`, returning the
/// primitives and the y at which the header ends.
fn build_header(score: &Score, width: f32) -> (Vec<Primitive>, f32) {
    let cx = width / 2.0;
    let mut prims = Vec::new();
    let mut y = TOP_MARGIN;
    let mut row = |prims: &mut Vec<Primitive>, content: String, role: TextRole, h: f32| {
        prims.push(Primitive::Text {
            x: cx,
            y: y + h / 2.0,
            content,
            role,
            span: None,
        });
        y += h;
    };

    if let Some(title) = &score.meta.title {
        row(&mut prims, title.clone(), TextRole::Title, TITLE_H);
    }
    if let Some(composer) = &score.meta.composer {
        row(&mut prims, composer.clone(), TextRole::Composer, COMPOSER_H);
    }
    if let Some(tempo) = score.meta.tempo {
        row(
            &mut prims,
            format!("♩ = {tempo}"),
            TextRole::Tempo,
            META_LINE_H,
        );
    }
    row(
        &mut prims,
        tuning_label(score),
        TextRole::Tuning,
        META_LINE_H,
    );
    if !score.capo.is_empty() {
        let label = format!("Capo {}", score.capo.join(", "));
        row(&mut prims, label, TextRole::Capo, META_LINE_H);
    }

    (prims, y)
}

/// The open-string letters as conventional tuning notation: highest-numbered
/// string first (banjo `gDGBD`, guitar `EADGBE`).
fn tuning_label(score: &Score) -> String {
    score
        .instrument
        .strings
        .iter()
        .rev()
        .map(|s| s.label.as_str())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instrument::Instrument;
    use crate::model::{
        Chord, ChordNote, Duration, Event, EventKind, Note, Position, RightHand, ScoreMeta,
    };
    use serde::de::DeserializeOwned;
    use std::fmt::Debug as DebugTrait;

    fn cfg() -> LayoutConfig {
        LayoutConfig { width: 800.0 }
    }

    fn note(string: u8, fret: u8, start: u32) -> Event {
        Event::new(
            EventKind::Note(Note {
                pos: Position::new(string, fret),
                dur: Duration::from_denominator(4),
                right_hand: Some(RightHand::Finger(crate::model::Finger::Thumb)),
                technique: None,
                tie: false,
            }),
            Span::new(start, start + 3),
        )
    }

    fn banjo_score(measures: Vec<Measure>) -> Score {
        Score {
            meta: ScoreMeta::default(),
            instrument: Instrument::builtin("banjo").unwrap(),
            capo: vec![],
            measures,
        }
    }

    fn fret_numbers(tree: &RenderTree) -> Vec<&Primitive> {
        tree.systems
            .iter()
            .flat_map(|s| s.measures.iter())
            .flat_map(|m| m.prims.iter())
            .filter(|p| {
                matches!(
                    p,
                    Primitive::Text {
                        role: TextRole::FretNumber,
                        ..
                    }
                )
            })
            .collect()
    }

    #[test]
    fn maps_string_one_to_the_top_line() {
        // Strings 1 and 5 a spacing apart: string 1 is the top (smaller y).
        let m = Measure::new(vec![note(1, 0, 0), note(5, 0, 4)]);
        let tree = layout(&banjo_score(vec![m]), cfg());
        let nums = fret_numbers(&tree);
        let y_string1 = match nums[0] {
            Primitive::Text { y, .. } => *y,
            _ => unreachable!(),
        };
        let y_string5 = match nums[1] {
            Primitive::Text { y, .. } => *y,
            _ => unreachable!(),
        };
        assert!(y_string1 < y_string5);
        assert_eq!(y_string5 - y_string1, 4.0 * STRING_SPACING);
    }

    #[test]
    fn draws_one_string_line_per_string() {
        let tree = layout(&banjo_score(vec![Measure::new(vec![])]), cfg());
        // No numbers => one unbroken line per string.
        let lines = tree.systems[0]
            .prims
            .iter()
            .filter(|p| matches!(p, Primitive::Line { .. }))
            .count();
        assert_eq!(lines, 5);
    }

    #[test]
    fn a_fret_number_breaks_its_string_line() {
        // One note on string 3 splits that line into two segments; the other
        // four strings stay whole. 4 + 2 = 6 line segments.
        let tree = layout(&banjo_score(vec![Measure::new(vec![note(3, 2, 0)])]), cfg());
        let lines = tree.systems[0]
            .prims
            .iter()
            .filter(|p| matches!(p, Primitive::Line { .. }))
            .count();
        assert_eq!(lines, 6);
    }

    #[test]
    fn chord_places_a_number_per_member_at_one_x() {
        let chord = Event::new(
            EventKind::Chord(Chord {
                dur: Duration::from_denominator(4),
                notes: vec![
                    ChordNote {
                        pos: Position::new(1, 0),
                        right_hand: None,
                    },
                    ChordNote {
                        pos: Position::new(2, 1),
                        right_hand: None,
                    },
                ],
            }),
            Span::new(0, 7),
        );
        let tree = layout(&banjo_score(vec![Measure::new(vec![chord])]), cfg());
        let nums = fret_numbers(&tree);
        assert_eq!(nums.len(), 2);
        let xs: Vec<f32> = nums
            .iter()
            .map(|p| match p {
                Primitive::Text { x, .. } => *x,
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(xs[0], xs[1]);
    }

    #[test]
    fn fret_numbers_carry_their_event_span() {
        let tree = layout(
            &banjo_score(vec![Measure::new(vec![note(2, 5, 10)])]),
            cfg(),
        );
        match fret_numbers(&tree)[0] {
            Primitive::Text { span, content, .. } => {
                assert_eq!(*span, Some(Span::new(10, 13)));
                assert_eq!(content, "5");
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn header_renders_present_metadata() {
        let mut score = banjo_score(vec![Measure::new(vec![note(1, 0, 0)])]);
        score.meta = ScoreMeta {
            title: Some("Cripple Creek".to_string()),
            composer: Some("Trad.".to_string()),
            tempo: Some(120),
        };
        let tree = layout(&score, cfg());
        let roles: Vec<TextRole> = tree
            .header
            .iter()
            .map(|p| match p {
                Primitive::Text { role, .. } => *role,
                _ => unreachable!(),
            })
            .collect();
        // Title, composer, tempo, and the always-present tuning line.
        assert_eq!(
            roles,
            vec![
                TextRole::Title,
                TextRole::Composer,
                TextRole::Tempo,
                TextRole::Tuning,
            ]
        );
    }

    #[test]
    fn tuning_label_is_conventional_order() {
        let banjo = banjo_score(vec![]);
        assert_eq!(tuning_label(&banjo), "gDGBD");
        let guitar = Score {
            instrument: Instrument::builtin("guitar").unwrap(),
            ..banjo_score(vec![])
        };
        assert_eq!(tuning_label(&guitar), "EADGBE");
    }

    fn round_trip<T: Serialize + DeserializeOwned + PartialEq + DebugTrait>(value: &T) {
        let json = serde_json::to_string(value).unwrap();
        let back: T = serde_json::from_str(&json).unwrap();
        assert_eq!(value, &back);
    }

    #[test]
    fn layout_output_round_trips() {
        let m = Measure::new(vec![note(3, 0, 0), note(2, 1, 4), note(1, 2, 8)]);
        round_trip(&layout(&banjo_score(vec![m]), cfg()));
    }

    #[test]
    fn simple_measure_layout_snapshot() {
        let mut score = banjo_score(vec![Measure::new(vec![
            note(3, 0, 0),
            note(2, 1, 4),
            note(1, 2, 8),
            note(5, 0, 12),
        ])]);
        score.meta = ScoreMeta {
            title: Some("Cripple Creek".to_string()),
            composer: None,
            tempo: Some(120),
        };
        let tree = layout(&score, cfg());
        insta::assert_snapshot!(serde_json::to_string_pretty(&tree).unwrap());
    }
}
