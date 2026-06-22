//! The layout engine: a pure function from the musical `Score` to a positioned
//! `RenderTree`. Coordinates are logical (1 unit = string spacing); the painter
//! scales them via the SVG viewBox.
//!
//! Vertical axis: the header block, the string lines, and the string->line
//! mapping that places fret numbers (string 1 = top line). Horizontal axis:
//! time-proportional spacing (x grows with an event's onset within its bar),
//! barlines between measures, repeat ornaments, and volta brackets. Everything
//! is laid out in a single system here; breaking into lines is a later pass.

use serde::{Deserialize, Serialize};

use crate::beam;
use crate::model::{Duration, EventKind, Measure, Score, TimeSig};
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
// Vertical gap between stacked systems (room below the numbers for stems/marks).
const SYSTEM_GAP: f32 = 3.0;
// Vertical room reserved above the staff for volta brackets, when any exist.
const VOLTA_SPACE: f32 = 1.2;
const VOLTA_GAP: f32 = 0.8;

// Horizontal metrics.
const LEFT_MARGIN: f32 = 2.0;
const RIGHT_MARGIN: f32 = 1.0;
const LABEL_X: f32 = 1.0;
// Logical width of one whole note; an event's x is proportional to its onset.
const UNITS_PER_WHOLE: f32 = 8.0;
// Padding inside a measure, before the first event and after the last.
const MEASURE_PAD: f32 = 0.8;

// The half-width of the gap a fret number opens in the string line behind it.
const NUMBER_GAP: f32 = 0.6;
const STRING_WEIGHT: f32 = 0.06;
const BARLINE_WEIGHT: f32 = 0.1;
const THICK_WEIGHT: f32 = 0.25;
// Spacing of the thin line and dots that ornament a repeat barline.
const REPEAT_GAP: f32 = 0.22;
const DOT_R: f32 = 0.12;
// Stems hang straight down below the staff (tab convention): a small gap below
// the bottom string line, then a fixed length.
const STEM_GAP: f32 = 0.3;
const STEM_LENGTH: f32 = 1.2;
const STEM_WEIGHT: f32 = 0.08;
// A beam is a thick flat bar joining a group's stem ends; tab has no pitch
// staff, so beams never slope.
const BEAM_WEIGHT: f32 = 0.3;
// A flag is a short beam-like stub at a lone note's stem end; repeated flags
// stack upward toward the staff.
const FLAG_LENGTH: f32 = 0.6;
const FLAG_SPACING: f32 = 0.35;

/// An event placed at a measure-relative x: its fretted positions, that x, and
/// the source span that produced it.
struct PlacedEvent {
    positions: Vec<(u8, u8)>,
    rel_x: f32,
    span: Span,
}

/// A measure resolved to its logical width and its events at measure-relative x,
/// before line-breaking decides which system (and absolute x) it lands in.
struct MeasurePlan {
    width: f32,
    events: Vec<PlacedEvent>,
    span: Option<Span>,
}

/// Lay a `Score` out into a positioned render tree, wrapping measures into
/// stacked systems no wider than `config.width`.
pub fn layout(score: &Score, config: LayoutConfig) -> RenderTree {
    let n_strings = score.instrument.string_count();
    let staff_height = ((n_strings.max(1) - 1) as f32) * STRING_SPACING;

    // Resolve each measure's width and event placement, then greedily pack the
    // measures into systems.
    let plans: Vec<MeasurePlan> = score.measures.iter().map(plan_measure).collect();
    let groups = pack_systems(&plans, config.width);
    let width = overall_width(&groups, &plans);

    // Beam grouping per measure, threading the running meter (default 4/4).
    let mut meter = TimeSig::new(4, 4);
    let beams: Vec<Vec<beam::BeamGroup>> = score
        .measures
        .iter()
        .map(|m| {
            if let Some(t) = m.meter {
                meter = t;
            }
            beam::beam_groups(&m.events, meter)
        })
        .collect();

    let (header, header_bottom) = build_header(score, width);

    // Stack the systems vertically, each restating the staff lines and labels.
    let mut systems = Vec::with_capacity(groups.len());
    let mut cursor = header_bottom + HEADER_GAP;
    let mut last_bottom = cursor;
    for &(start, end) in &groups {
        let measures = &score.measures[start..end];
        let has_volta = measures.iter().any(|m| m.ending.is_some());
        let staff_top = cursor + if has_volta { VOLTA_SPACE } else { 0.0 };
        let staff_bottom = staff_top + staff_height;
        systems.push(build_system(
            score,
            measures,
            &plans[start..end],
            &beams[start..end],
            width,
            staff_top,
            staff_height,
            n_strings,
        ));
        last_bottom = staff_bottom;
        cursor = staff_bottom + SYSTEM_GAP;
    }

    let height = last_bottom + BOTTOM_MARGIN;
    RenderTree {
        meta: LayoutMeta { width, height },
        header,
        systems,
    }
}

/// Resolve one measure to its width and measure-relative event placement.
fn plan_measure(measure: &Measure) -> MeasurePlan {
    let mut onset = Duration::zero();
    let mut events = Vec::with_capacity(measure.events.len());
    for event in &measure.events {
        events.push(PlacedEvent {
            positions: fretted_positions(&event.kind),
            rel_x: MEASURE_PAD + span_width(onset),
            span: event.span,
        });
        onset = onset.plus(event.duration());
    }
    let width = MEASURE_PAD + span_width(onset) + MEASURE_PAD;
    let span = measure
        .events
        .iter()
        .map(|e| e.span)
        .reduce(|acc, s| acc.merge(s));
    MeasurePlan {
        width,
        events,
        span,
    }
}

/// Greedily group measure indices into systems, each at most `width` wide (always
/// at least one measure per system, even if it alone exceeds the target).
fn pack_systems(plans: &[MeasurePlan], width: f32) -> Vec<(usize, usize)> {
    let mut groups = Vec::new();
    let mut start = 0;
    let mut acc = 0.0;
    for (i, plan) in plans.iter().enumerate() {
        let prospective = LEFT_MARGIN + acc + plan.width + RIGHT_MARGIN;
        if i > start && prospective > width {
            groups.push((start, i));
            start = i;
            acc = 0.0;
        }
        acc += plan.width;
    }
    if start < plans.len() {
        groups.push((start, plans.len()));
    }
    groups
}

/// The widest system's extent (with margins) — the viewBox width all systems
/// share. Falls back to a minimum for an empty score.
fn overall_width(groups: &[(usize, usize)], plans: &[MeasurePlan]) -> f32 {
    let widest = groups
        .iter()
        .map(|&(s, e)| plans[s..e].iter().map(|p| p.width).sum::<f32>())
        .fold(0.0_f32, f32::max);
    LEFT_MARGIN + widest.max(UNITS_PER_WHOLE) + RIGHT_MARGIN
}

/// Build one system: its measure boxes (fret numbers) plus the system-spanning
/// furniture (string lines, labels, barlines, volta brackets).
#[allow(clippy::too_many_arguments)]
fn build_system(
    score: &Score,
    measures: &[Measure],
    plans: &[MeasurePlan],
    beams: &[Vec<beam::BeamGroup>],
    width: f32,
    staff_top: f32,
    staff_height: f32,
    n_strings: usize,
) -> System {
    let staff_bottom = staff_top + staff_height;
    let beam_y = staff_bottom + STEM_GAP + STEM_LENGTH;
    let line_y = |string: u8| staff_top + (f32::from(string.saturating_sub(1))) * STRING_SPACING;

    let mut number_xs: Vec<Vec<f32>> = vec![Vec::new(); n_strings];
    let mut boxes = Vec::with_capacity(plans.len());
    let mut ranges: Vec<(f32, f32)> = Vec::with_capacity(plans.len());
    let mut mx0 = LEFT_MARGIN;
    for ((plan, measure), mbeams) in plans.iter().zip(measures).zip(beams) {
        let mx1 = mx0 + plan.width;
        let mut prims = Vec::new();
        for (placed, event) in plan.events.iter().zip(&measure.events) {
            let x = mx0 + placed.rel_x;
            for &(string, fret) in &placed.positions {
                if (1..=n_strings as u8).contains(&string) {
                    number_xs[(string - 1) as usize].push(x);
                }
                prims.push(Primitive::Text {
                    x,
                    y: line_y(string),
                    content: fret.to_string(),
                    role: TextRole::FretNumber,
                    span: Some(placed.span),
                });
            }
            if beam::has_stem(event) {
                prims.push(stem(x, staff_bottom));
            }
        }
        // A group of two or more shares one flat beam across its stem ends; a
        // lone beamable note takes flags at its stem end instead.
        for g in mbeams {
            if g.members.len() >= 2 {
                let x0 = mx0 + plan.events[g.members[0]].rel_x;
                let x1 = mx0 + plan.events[*g.members.last().unwrap()].rel_x;
                prims.push(beam_bar(x0, x1, beam_y));
            } else if let Some(&idx) = g.members.first() {
                let x = mx0 + plan.events[idx].rel_x;
                let count = beam::flag_count(measure.events[idx].duration());
                prims.extend(flags(x, beam_y, count));
            }
        }
        boxes.push(MeasureBox {
            bounds: Rect {
                x: mx0,
                y: staff_top,
                w: plan.width,
                h: staff_height.max(STRING_SPACING),
            },
            prims,
            span: plan.span,
        });
        ranges.push((mx0, mx1));
        mx0 = mx1;
    }
    let staff_x1 = if plans.is_empty() {
        LEFT_MARGIN + UNITS_PER_WHOLE
    } else {
        mx0
    };

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
    sys_prims.extend(barlines(measures, &ranges, staff_top, staff_bottom));
    sys_prims.extend(volta_brackets(measures, &ranges, staff_top));

    System {
        bounds: Rect {
            x: 0.0,
            y: staff_top,
            w: width,
            h: staff_height.max(STRING_SPACING),
        },
        prims: sys_prims,
        measures: boxes,
    }
}

/// The logical width of a duration: `dur` whole notes at the global time scale.
fn span_width(dur: Duration) -> f32 {
    (dur.num as f32 / dur.den as f32) * UNITS_PER_WHOLE
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
            segments.push(hline(cursor, x1.min(gap_start), y));
        }
        cursor = cursor.max(cx + NUMBER_GAP);
    }
    if cursor < x1 {
        segments.push(hline(cursor, x1, y));
    }
    segments
}

fn hline(x1: f32, x2: f32, y: f32) -> Primitive {
    Primitive::Line {
        x1,
        y1: y,
        x2,
        y2: y,
        weight: STRING_WEIGHT,
    }
}

fn vline(x: f32, y1: f32, y2: f32, weight: f32) -> Primitive {
    Primitive::Line {
        x1: x,
        y1,
        x2: x,
        y2,
        weight,
    }
}

/// A note's stem: a vertical line hanging down from just below the staff.
fn stem(x: f32, staff_bottom: f32) -> Primitive {
    let top = staff_bottom + STEM_GAP;
    vline(x, top, top + STEM_LENGTH, STEM_WEIGHT)
}

/// A primary beam: a thick flat bar joining the stem ends of a group.
fn beam_bar(x1: f32, x2: f32, y: f32) -> Primitive {
    Primitive::Line {
        x1,
        y1: y,
        x2,
        y2: y,
        weight: BEAM_WEIGHT,
    }
}

/// Flags for a lone note: `count` short stubs at the stem end (`beam_y`), each
/// stacked one step upward toward the staff.
fn flags(x: f32, beam_y: f32, count: u8) -> Vec<Primitive> {
    (0..count)
        .map(|i| {
            let y = beam_y - f32::from(i) * FLAG_SPACING;
            Primitive::Line {
                x1: x,
                y1: y,
                x2: x + FLAG_LENGTH,
                y2: y,
                weight: BEAM_WEIGHT,
            }
        })
        .collect()
}

/// A small filled dot (an SVG circle path) centered at `(cx, cy)`.
fn dot(cx: f32, cy: f32) -> Primitive {
    let r = DOT_R;
    Primitive::Path {
        cmds: format!(
            "M {} {cy} a {r} {r} 0 1 0 {} 0 a {r} {r} 0 1 0 {} 0 z",
            cx - r,
            2.0 * r,
            -2.0 * r
        ),
        span: None,
    }
}

/// The two repeat dots straddling the staff center.
fn repeat_dots(x: f32, top: f32, bottom: f32) -> Vec<Primitive> {
    let mid = (top + bottom) / 2.0;
    vec![dot(x, mid - 0.5), dot(x, mid + 0.5)]
}

/// Barlines at every measure boundary, choosing plain, repeat-start, repeat-end,
/// or both per the bounding measures' flags. The open left edge of the system
/// (and the edge before a pickup) carries no barline.
fn barlines(measures: &[Measure], ranges: &[(f32, f32)], top: f32, bottom: f32) -> Vec<Primitive> {
    let n = measures.len();
    let mut prims = Vec::new();
    for k in 0..=n {
        let x = if k < n { ranges[k].0 } else { ranges[n - 1].1 };
        let left = k.checked_sub(1).and_then(|i| measures.get(i));
        let right = measures.get(k);
        let rep_end = left.is_some_and(|m| m.repeat_end);
        let rep_start = right.is_some_and(|m| m.repeat_start);

        if rep_end && rep_start {
            prims.push(vline(x, top, bottom, THICK_WEIGHT));
            prims.push(vline(x - REPEAT_GAP, top, bottom, BARLINE_WEIGHT));
            prims.push(vline(x + REPEAT_GAP, top, bottom, BARLINE_WEIGHT));
            prims.extend(repeat_dots(x - 2.0 * REPEAT_GAP, top, bottom));
            prims.extend(repeat_dots(x + 2.0 * REPEAT_GAP, top, bottom));
        } else if rep_end {
            prims.push(vline(x, top, bottom, THICK_WEIGHT));
            prims.push(vline(x - REPEAT_GAP, top, bottom, BARLINE_WEIGHT));
            prims.extend(repeat_dots(x - 2.0 * REPEAT_GAP, top, bottom));
        } else if rep_start {
            prims.push(vline(x, top, bottom, THICK_WEIGHT));
            prims.push(vline(x + REPEAT_GAP, top, bottom, BARLINE_WEIGHT));
            prims.extend(repeat_dots(x + 2.0 * REPEAT_GAP, top, bottom));
        } else if k == 0 || right.is_some_and(|m| m.is_pickup) {
            // Open staff start, or the offset edge of a pickup: no barline.
        } else {
            prims.push(vline(x, top, bottom, BARLINE_WEIGHT));
        }
    }
    prims
}

/// Volta brackets over runs of consecutive measures sharing an `ending` number.
/// Each is an open bracket above the staff with a left tick and the number.
fn volta_brackets(measures: &[Measure], ranges: &[(f32, f32)], staff_top: f32) -> Vec<Primitive> {
    let bracket_y = staff_top - VOLTA_GAP;
    let mut prims = Vec::new();
    let mut i = 0;
    while i < measures.len() {
        let Some(n) = measures[i].ending else {
            i += 1;
            continue;
        };
        let start = i;
        while i < measures.len() && measures[i].ending == Some(n) {
            i += 1;
        }
        let x0 = ranges[start].0;
        let x1 = ranges[i - 1].1;
        // Horizontal span, left downward tick, and the ending number.
        prims.push(Primitive::Line {
            x1: x0,
            y1: bracket_y,
            x2: x1,
            y2: bracket_y,
            weight: BARLINE_WEIGHT,
        });
        prims.push(vline(x0, bracket_y, staff_top - 0.1, BARLINE_WEIGHT));
        prims.push(Primitive::Text {
            x: x0 + 0.5,
            y: bracket_y + 0.4,
            content: n.to_string(),
            role: TextRole::Ending,
            span: None,
        });
    }
    prims
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
        Chord, ChordNote, Duration, Event, EventKind, Note, Position, RightHand, ScoreMeta, TimeSig,
    };
    use serde::de::DeserializeOwned;
    use std::fmt::Debug as DebugTrait;

    fn cfg() -> LayoutConfig {
        LayoutConfig { width: 800.0 }
    }

    fn note_dur(string: u8, fret: u8, start: u32, dur: Duration) -> Event {
        Event::new(
            EventKind::Note(Note {
                pos: Position::new(string, fret),
                dur,
                right_hand: Some(RightHand::Finger(crate::model::Finger::Thumb)),
                technique: None,
                tie: false,
            }),
            Span::new(start, start + 3),
        )
    }

    fn note(string: u8, fret: u8, start: u32) -> Event {
        note_dur(string, fret, start, Duration::from_denominator(4))
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

    fn x_of(p: &Primitive) -> f32 {
        match p {
            Primitive::Text { x, .. } => *x,
            _ => unreachable!(),
        }
    }

    fn y_of(p: &Primitive) -> f32 {
        match p {
            Primitive::Text { y, .. } => *y,
            _ => unreachable!(),
        }
    }

    /// Lines in the first system that are horizontal (string lines) / vertical
    /// (barlines), distinguished by geometry since both are `Line`.
    fn horizontal_lines(tree: &RenderTree) -> usize {
        tree.systems[0]
            .prims
            .iter()
            .filter(|p| matches!(p, Primitive::Line { y1, y2, .. } if y1 == y2))
            .count()
    }

    fn vertical_lines(tree: &RenderTree) -> Vec<&Primitive> {
        tree.systems[0]
            .prims
            .iter()
            .filter(|p| matches!(p, Primitive::Line { x1, x2, .. } if x1 == x2))
            .collect()
    }

    #[test]
    fn maps_string_one_to_the_top_line() {
        let m = Measure::new(vec![note(1, 0, 0), note(5, 0, 4)]);
        let tree = layout(&banjo_score(vec![m]), cfg());
        let nums = fret_numbers(&tree);
        let y1 = y_of(nums[0]);
        let y5 = y_of(nums[1]);
        assert!(y1 < y5);
        assert_eq!(y5 - y1, 4.0 * STRING_SPACING);
    }

    #[test]
    fn draws_one_string_line_per_string() {
        let tree = layout(&banjo_score(vec![Measure::new(vec![])]), cfg());
        assert_eq!(horizontal_lines(&tree), 5);
    }

    #[test]
    fn a_fret_number_breaks_its_string_line() {
        let tree = layout(&banjo_score(vec![Measure::new(vec![note(3, 2, 0)])]), cfg());
        // String 3 splits into two segments; the other four stay whole.
        assert_eq!(horizontal_lines(&tree), 6);
    }

    #[test]
    fn spacing_is_time_proportional() {
        // quarter, half, quarter: onsets 0, 1/4, 3/4.
        let m = Measure::new(vec![
            note_dur(3, 0, 0, Duration::from_denominator(4)),
            note_dur(2, 1, 4, Duration::from_denominator(2)),
            note_dur(1, 2, 8, Duration::from_denominator(4)),
        ]);
        let tree = layout(&banjo_score(vec![m]), cfg());
        let nums = fret_numbers(&tree);
        let xs: Vec<f32> = nums.iter().map(|p| x_of(p)).collect();
        // A quarter advances 1/4 * 8 = 2.0; a half advances 4.0.
        assert!((xs[1] - xs[0] - 2.0).abs() < 1e-5);
        assert!((xs[2] - xs[1] - 4.0).abs() < 1e-5);
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
        assert_eq!(x_of(nums[0]), x_of(nums[1]));
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
    fn a_barline_separates_two_measures() {
        let one = Measure::new(vec![note(3, 0, 0)]);
        let two = Measure::new(vec![note(3, 0, 4)]);
        let tree = layout(&banjo_score(vec![one, two]), cfg());
        // No repeats: an interior barline plus the final one, no leading barline.
        assert_eq!(vertical_lines(&tree).len(), 2);
    }

    #[test]
    fn repeat_flags_draw_thick_lines_and_dots() {
        let mut m = Measure::new(vec![note(3, 0, 0)]);
        m.repeat_start = true;
        m.repeat_end = true;
        let tree = layout(&banjo_score(vec![m]), cfg());
        let thick = tree.systems[0]
            .prims
            .iter()
            .filter(|p| matches!(p, Primitive::Line { weight, .. } if *weight == THICK_WEIGHT))
            .count();
        let dots = tree.systems[0]
            .prims
            .iter()
            .filter(|p| matches!(p, Primitive::Path { .. }))
            .count();
        // A thick line on each edge; two dots per ornamented side.
        assert_eq!(thick, 2);
        assert_eq!(dots, 4);
    }

    #[test]
    fn a_pickup_has_no_leading_barline() {
        let mut pickup = Measure::new(vec![note(3, 0, 0)]);
        pickup.is_pickup = true;
        let bar = Measure::new(vec![note(3, 0, 4)]);
        let tree = layout(&banjo_score(vec![pickup, bar]), cfg());
        let xs: Vec<f32> = vertical_lines(&tree).iter().map(|p| x_of_line(p)).collect();
        // Only the barline after the pickup and the final one — none at the very
        // left edge (x == LEFT_MARGIN).
        assert!(xs.iter().all(|&x| x > LEFT_MARGIN));
    }

    fn x_of_line(p: &Primitive) -> f32 {
        match p {
            Primitive::Line { x1, .. } => *x1,
            _ => unreachable!(),
        }
    }

    #[test]
    fn an_ending_draws_a_volta_bracket_and_number() {
        let mut m = Measure::new(vec![note(3, 0, 0)]);
        m.ending = Some(1);
        let tree = layout(&banjo_score(vec![m]), cfg());
        let endings: Vec<&Primitive> = tree.systems[0]
            .prims
            .iter()
            .filter(|p| {
                matches!(
                    p,
                    Primitive::Text {
                        role: TextRole::Ending,
                        ..
                    }
                )
            })
            .collect();
        assert_eq!(endings.len(), 1);
        match endings[0] {
            Primitive::Text { content, .. } => assert_eq!(content, "1"),
            _ => unreachable!(),
        }
        // The bracket sits above the staff: the number is higher (smaller y)
        // than the fret number on the staff.
        let staff_y = y_of(fret_numbers(&tree)[0]);
        assert!(y_of(endings[0]) < staff_y);
    }

    #[test]
    fn voltas_reserve_room_above_the_staff() {
        let plain = layout(&banjo_score(vec![Measure::new(vec![note(3, 0, 0)])]), cfg());
        let mut m = Measure::new(vec![note(3, 0, 0)]);
        m.ending = Some(1);
        let with_volta = layout(&banjo_score(vec![m]), cfg());
        assert!(with_volta.meta.height > plain.meta.height);
    }

    #[test]
    fn consecutive_same_endings_share_one_bracket() {
        let mut a = Measure::new(vec![note(3, 0, 0)]);
        a.ending = Some(1);
        let mut b = Measure::new(vec![note(3, 0, 4)]);
        b.ending = Some(1);
        let tree = layout(&banjo_score(vec![a, b]), cfg());
        let endings = tree.systems[0]
            .prims
            .iter()
            .filter(|p| {
                matches!(
                    p,
                    Primitive::Text {
                        role: TextRole::Ending,
                        ..
                    }
                )
            })
            .count();
        assert_eq!(endings, 1);
    }

    #[test]
    fn meter_changes_do_not_affect_proportional_placement() {
        // Layout spaces by event onsets, not the meter stamp.
        let mut m = Measure::new(vec![note(3, 0, 0), note(2, 0, 4)]);
        m.meter = Some(TimeSig::new(3, 4));
        let tree = layout(&banjo_score(vec![m]), cfg());
        let nums = fret_numbers(&tree);
        assert!((x_of(nums[1]) - x_of(nums[0]) - 2.0).abs() < 1e-5);
    }

    /// Stems are the vertical line segments inside measure boxes (barlines live
    /// in the system's own prims, not the measure boxes).
    fn stems(tree: &RenderTree) -> Vec<&Primitive> {
        tree.systems
            .iter()
            .flat_map(|s| s.measures.iter())
            .flat_map(|m| m.prims.iter())
            .filter(|p| matches!(p, Primitive::Line { x1, x2, .. } if x1 == x2))
            .collect()
    }

    #[test]
    fn each_note_gets_one_downward_stem() {
        let m = Measure::new(vec![note(3, 0, 0), note(2, 1, 4)]);
        let tree = layout(&banjo_score(vec![m]), cfg());
        let stems = stems(&tree);
        assert_eq!(stems.len(), 2);
        for s in stems {
            match s {
                // Direction is down (y2 > y1) and the length is fixed.
                Primitive::Line { y1, y2, weight, .. } => {
                    assert!(y2 > y1);
                    assert!((y2 - y1 - STEM_LENGTH).abs() < 1e-5);
                    assert_eq!(*weight, STEM_WEIGHT);
                }
                _ => unreachable!(),
            }
        }
    }

    #[test]
    fn a_stem_hangs_below_the_staff() {
        let tree = layout(&banjo_score(vec![Measure::new(vec![note(1, 0, 0)])]), cfg());
        // The lowest string line is the largest y among the staff's lines.
        let bottom_line_y = tree.systems[0]
            .prims
            .iter()
            .filter_map(|p| match p {
                Primitive::Line { y1, y2, .. } if y1 == y2 => Some(*y1),
                _ => None,
            })
            .fold(f32::MIN, f32::max);
        match stems(&tree)[0] {
            Primitive::Line { y1, .. } => assert!(*y1 > bottom_line_y),
            _ => unreachable!(),
        }
    }

    #[test]
    fn a_chord_gets_a_single_stem() {
        let chord = Event::new(
            EventKind::Chord(Chord {
                dur: Duration::from_denominator(4),
                notes: vec![
                    ChordNote {
                        pos: Position::new(1, 0),
                        right_hand: None,
                    },
                    ChordNote {
                        pos: Position::new(3, 2),
                        right_hand: None,
                    },
                ],
            }),
            Span::new(0, 7),
        );
        let tree = layout(&banjo_score(vec![Measure::new(vec![chord])]), cfg());
        assert_eq!(stems(&tree).len(), 1);
    }

    #[test]
    fn a_rest_has_no_stem() {
        let rest = Event::new(
            EventKind::Rest(Duration::from_denominator(4)),
            Span::new(0, 1),
        );
        let tree = layout(&banjo_score(vec![Measure::new(vec![rest])]), cfg());
        assert!(stems(&tree).is_empty());
    }

    /// Beams are the thick horizontal segments inside measure boxes that span
    /// between notes — i.e. not the fixed-width flag stubs (which share the
    /// weight but are exactly FLAG_LENGTH wide).
    fn beams(tree: &RenderTree) -> Vec<&Primitive> {
        tree.systems
            .iter()
            .flat_map(|s| s.measures.iter())
            .flat_map(|m| m.prims.iter())
            .filter(|p| {
                matches!(p, Primitive::Line { x1, x2, y1, y2, weight }
                    if y1 == y2 && *weight == BEAM_WEIGHT && (x2 - x1 - FLAG_LENGTH).abs() >= 1e-5)
            })
            .collect()
    }

    fn eighth(string: u8, fret: u8, start: u32) -> Event {
        note_dur(string, fret, start, Duration::from_denominator(8))
    }

    #[test]
    fn two_eighths_share_one_beam() {
        let m = Measure::new(vec![eighth(3, 0, 0), eighth(2, 1, 4)]);
        let tree = layout(&banjo_score(vec![m]), cfg());
        let beams = beams(&tree);
        assert_eq!(beams.len(), 1);
        // Flat, and spanning the two note x-positions.
        let nums = fret_numbers(&tree);
        match beams[0] {
            Primitive::Line { x1, y1, x2, y2, .. } => {
                assert_eq!(y1, y2);
                assert!((x1 - x_of(nums[0])).abs() < 1e-5);
                assert!((x2 - x_of(nums[1])).abs() < 1e-5);
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn four_eighths_make_two_beams() {
        let m = Measure::new(vec![
            eighth(3, 0, 0),
            eighth(3, 0, 4),
            eighth(3, 0, 8),
            eighth(3, 0, 12),
        ]);
        let tree = layout(&banjo_score(vec![m]), cfg());
        assert_eq!(beams(&tree).len(), 2);
    }

    #[test]
    fn quarter_notes_have_no_beam() {
        let m = Measure::new(vec![note(3, 0, 0), note(2, 0, 4)]);
        let tree = layout(&banjo_score(vec![m]), cfg());
        assert!(beams(&tree).is_empty());
    }

    #[test]
    fn a_lone_eighth_has_no_beam() {
        let tree = layout(
            &banjo_score(vec![Measure::new(vec![eighth(3, 0, 0)])]),
            cfg(),
        );
        assert!(beams(&tree).is_empty());
    }

    #[test]
    fn a_beam_sits_at_the_stem_ends() {
        let m = Measure::new(vec![eighth(3, 0, 0), eighth(2, 0, 4)]);
        let tree = layout(&banjo_score(vec![m]), cfg());
        let beam_y = match beams(&tree)[0] {
            Primitive::Line { y1, .. } => *y1,
            _ => unreachable!(),
        };
        // Every stem's lower end reaches the beam.
        for s in stems(&tree) {
            match s {
                Primitive::Line { y2, .. } => assert!((y2 - beam_y).abs() < 1e-5),
                _ => unreachable!(),
            }
        }
    }

    /// Flag stubs are the short (FLAG_LENGTH-wide) horizontal beam-weight lines.
    fn flag_stubs(tree: &RenderTree) -> Vec<&Primitive> {
        tree.systems
            .iter()
            .flat_map(|s| s.measures.iter())
            .flat_map(|m| m.prims.iter())
            .filter(|p| {
                matches!(p, Primitive::Line { x1, x2, y1, y2, weight }
                    if y1 == y2 && *weight == BEAM_WEIGHT && (x2 - x1 - FLAG_LENGTH).abs() < 1e-5)
            })
            .collect()
    }

    #[test]
    fn a_lone_eighth_gets_one_flag() {
        let tree = layout(
            &banjo_score(vec![Measure::new(vec![eighth(3, 0, 0)])]),
            cfg(),
        );
        assert_eq!(flag_stubs(&tree).len(), 1);
        assert!(beams(&tree).is_empty());
    }

    #[test]
    fn a_lone_sixteenth_gets_two_flags() {
        let m = Measure::new(vec![note_dur(3, 0, 0, Duration::from_denominator(16))]);
        let tree = layout(&banjo_score(vec![m]), cfg());
        assert_eq!(flag_stubs(&tree).len(), 2);
    }

    #[test]
    fn flags_stack_upward_from_the_stem_end() {
        let m = Measure::new(vec![note_dur(3, 0, 0, Duration::from_denominator(16))]);
        let tree = layout(&banjo_score(vec![m]), cfg());
        let stem_end = match stems(&tree)[0] {
            Primitive::Line { y2, .. } => *y2,
            _ => unreachable!(),
        };
        let mut ys: Vec<f32> = flag_stubs(&tree)
            .iter()
            .map(|p| match p {
                Primitive::Line { y1, .. } => *y1,
                _ => unreachable!(),
            })
            .collect();
        ys.sort_by(|a, b| b.partial_cmp(a).unwrap());
        // The outermost flag is at the stem end; the next is higher (smaller y).
        assert!((ys[0] - stem_end).abs() < 1e-5);
        assert!(ys[1] < ys[0]);
    }

    #[test]
    fn a_beamed_eighth_has_no_flag() {
        let m = Measure::new(vec![eighth(3, 0, 0), eighth(2, 0, 4)]);
        let tree = layout(&banjo_score(vec![m]), cfg());
        assert!(flag_stubs(&tree).is_empty());
        assert_eq!(beams(&tree).len(), 1);
    }

    fn measures_of(n: u32) -> Vec<Measure> {
        (0..n)
            .map(|i| Measure::new(vec![note(3, 0, i * 4)]))
            .collect()
    }

    /// The count of distinct string lines in a system — the distinct y values
    /// among its horizontal line segments (a line broken behind numbers still
    /// counts once).
    fn string_line_count(system: &System) -> usize {
        let mut ys: Vec<u32> = system
            .prims
            .iter()
            .filter_map(|p| match p {
                Primitive::Line { y1, y2, .. } if y1 == y2 => Some(y1.to_bits()),
                _ => None,
            })
            .collect();
        ys.sort_unstable();
        ys.dedup();
        ys.len()
    }

    #[test]
    fn a_wide_target_keeps_everything_on_one_system() {
        let tree = layout(&banjo_score(measures_of(6)), LayoutConfig { width: 800.0 });
        assert_eq!(tree.systems.len(), 1);
    }

    #[test]
    fn a_narrow_target_wraps_into_more_systems() {
        let wide = layout(&banjo_score(measures_of(6)), LayoutConfig { width: 800.0 });
        let narrow = layout(&banjo_score(measures_of(6)), LayoutConfig { width: 12.0 });
        assert_eq!(wide.systems.len(), 1);
        assert!(narrow.systems.len() > 1);
    }

    #[test]
    fn systems_stack_top_to_bottom() {
        let tree = layout(&banjo_score(measures_of(6)), LayoutConfig { width: 12.0 });
        assert!(tree.systems.len() >= 2);
        for pair in tree.systems.windows(2) {
            assert!(pair[1].bounds.y > pair[0].bounds.y);
        }
    }

    #[test]
    fn each_system_restates_its_string_lines() {
        let tree = layout(&banjo_score(measures_of(6)), LayoutConfig { width: 12.0 });
        assert!(tree.systems.len() >= 2);
        for system in &tree.systems {
            assert_eq!(string_line_count(system), 5);
        }
    }

    #[test]
    fn a_measure_wider_than_the_target_still_stands_alone() {
        // A target narrower than a single measure cannot drop measures.
        let tree = layout(&banjo_score(measures_of(3)), LayoutConfig { width: 1.0 });
        assert_eq!(tree.systems.len(), 3);
        for system in &tree.systems {
            assert_eq!(system.measures.len(), 1);
        }
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

    #[test]
    fn repeat_with_endings_layout_snapshot() {
        let mut open = Measure::new(vec![note(3, 0, 0), note(2, 0, 4)]);
        open.repeat_start = true;
        let mut first = Measure::new(vec![note(1, 0, 8)]);
        first.ending = Some(1);
        first.repeat_end = true;
        let mut second = Measure::new(vec![note(1, 2, 12)]);
        second.ending = Some(2);
        let tree = layout(&banjo_score(vec![open, first, second]), cfg());
        insta::assert_snapshot!(serde_json::to_string_pretty(&tree).unwrap());
    }

    #[test]
    fn wrapped_systems_layout_snapshot() {
        let tree = layout(&banjo_score(measures_of(4)), LayoutConfig { width: 12.0 });
        insta::assert_snapshot!(serde_json::to_string_pretty(&tree).unwrap());
    }

    #[test]
    fn beamed_rhythm_layout_snapshot() {
        // One 4/4 bar mixing beat groupings: two eighths, four sixteenths, a
        // quarter, then two eighths.
        let m = Measure::new(vec![
            eighth(3, 0, 0),
            eighth(2, 0, 4),
            note_dur(1, 0, 8, Duration::from_denominator(16)),
            note_dur(1, 2, 12, Duration::from_denominator(16)),
            note_dur(1, 0, 16, Duration::from_denominator(16)),
            note_dur(2, 0, 20, Duration::from_denominator(16)),
            note(3, 0, 24),
            eighth(2, 1, 28),
            eighth(1, 0, 32),
        ]);
        let tree = layout(&banjo_score(vec![m]), cfg());
        insta::assert_snapshot!(serde_json::to_string_pretty(&tree).unwrap());
    }

    #[test]
    fn flagged_notes_layout_snapshot() {
        // A lone eighth and a lone sixteenth, kept apart by a quarter so neither
        // joins a beam — each takes flags instead (one, then two).
        let m = Measure::new(vec![
            eighth(2, 0, 0),
            note(3, 0, 4),
            note_dur(1, 0, 8, Duration::from_denominator(16)),
        ]);
        let tree = layout(&banjo_score(vec![m]), cfg());
        insta::assert_snapshot!(serde_json::to_string_pretty(&tree).unwrap());
    }
}
