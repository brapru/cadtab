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
use crate::model::{
    Duration, Event, EventKind, Finger, Measure, RightHand, Score, Strum, Technique, TimeSig,
};
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
const BOTTOM_MARGIN: f32 = 2.8;
// Vertical gap between stacked systems (room below the numbers for stems/marks).
const SYSTEM_GAP: f32 = 3.5;
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
// A stem connects an event to its rhythm: it hangs from a small gap below the
// event's lowest fret number down to the beam line. STEM_NOTE_GAP is measured
// from the number's center; a fret digit reaches ~0.45 below center, so 0.8
// leaves a slight visible gap between the glyph and the stem. The beam sits a
// fixed drop below the bottom string line, and a stem is never shorter than
// STEM_MIN so a bottom (5th) string note still reads as stemmed.
const STEM_NOTE_GAP: f32 = 0.8;
const STEM_MIN: f32 = 0.6;
const BEAM_DROP: f32 = 1.5;
const STEM_WEIGHT: f32 = 0.08;
// A beam is a flat bar joining a group's stem ends (tab has no pitch staff, so
// beams never slope). Kept fairly thin — a heavy beam reads as a block against
// the hairline stems and string lines.
const BEAM_WEIGHT: f32 = 0.18;
// A flag is a short beam-like stub at a lone note's stem end; repeated flags
// stack upward toward the staff.
const FLAG_LENGTH: f32 = 0.6;
const FLAG_SPACING: f32 = 0.35;
// Augmentation dots sit just right of a fret number, spaced along the line.
const AUG_DOT_AFTER: f32 = 0.45;
const AUG_DOT_GAP: f32 = 0.3;
// Right-hand finger/strum marks sit in a row below the stems; chord members are
// staggered so they do not overlap.
const RH_ROW_GAP: f32 = 0.6;
const RH_STAGGER: f32 = 0.5;
// Technique text (h/p/sl) sits just above and right of the note number.
const TECH_DX: f32 = 0.4;
const TECH_DY: f32 = 0.45;
// A tie arcs above the two notes it joins.
const TIE_HEIGHT: f32 = 0.8;

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
    let beam_y = staff_bottom + BEAM_DROP;
    let line_y = |string: u8| staff_top + (f32::from(string.saturating_sub(1))) * STRING_SPACING;

    let mut number_xs: Vec<Vec<f32>> = vec![Vec::new(); n_strings];
    let mut boxes = Vec::with_capacity(plans.len());
    let mut ranges: Vec<(f32, f32)> = Vec::with_capacity(plans.len());
    let mut mx0 = LEFT_MARGIN;
    for ((plan, measure), mbeams) in plans.iter().zip(measures).zip(beams) {
        let mx1 = mx0 + plan.width;
        let mut prims = Vec::new();
        for (j, (placed, event)) in plan.events.iter().zip(&measure.events).enumerate() {
            let x = mx0 + placed.rel_x;
            let dots = beam::augmentation_dots(event.duration());
            for &(string, fret) in &placed.positions {
                if (1..=n_strings as u8).contains(&string) {
                    number_xs[(string - 1) as usize].push(x);
                }
                let y = line_y(string);
                prims.push(Primitive::Text {
                    x,
                    y,
                    content: fret.to_string(),
                    role: TextRole::FretNumber,
                    span: Some(placed.span),
                });
                for i in 0..dots {
                    prims.push(dot(x + AUG_DOT_AFTER + f32::from(i) * AUG_DOT_GAP, y));
                }
            }
            if matches!(event.kind, EventKind::Rest(_)) {
                // A rest sits centered on the staff, with its own augmentation
                // dots; it breaks beaming, so it is never stemmed.
                let y = (staff_top + staff_bottom) / 2.0;
                prims.push(Primitive::Text {
                    x,
                    y,
                    content: rest_glyph(event.duration()).to_string(),
                    role: TextRole::Rest,
                    span: Some(placed.span),
                });
                for i in 0..dots {
                    prims.push(dot(x + AUG_DOT_AFTER + f32::from(i) * AUG_DOT_GAP, y));
                }
            }
            if beam::has_stem(event) {
                // Hang the stem from just below this event's lowest fret number
                // (largest y), clamped so it keeps a minimum visible length.
                let low_y = placed
                    .positions
                    .iter()
                    .map(|&(string, _)| line_y(string))
                    .fold(f32::MIN, f32::max);
                let stem_top = (low_y + STEM_NOTE_GAP).min(beam_y - STEM_MIN);
                prims.push(stem(x, stem_top, beam_y));
            }
            let next_x = plan.events.get(j + 1).map(|p| mx0 + p.rel_x);
            prims.extend(marks_for(event, x, next_x, staff_top, staff_bottom));
        }
        // A group of two or more shares one flat beam across its stem ends; a
        // lone beamable note takes flags at its stem end instead. The beam's top
        // edge sits flush with the stem ends (beam_y) and the bar hangs below, so
        // stems meet the beam cleanly rather than poking through its middle. The
        // bar overhangs each outer stem by half a stem-width so it reaches the
        // outer edge of the stem rather than stopping at its centerline.
        let beam_render_y = beam_y + BEAM_WEIGHT / 2.0;
        let beam_overhang = STEM_WEIGHT / 2.0;
        for g in mbeams {
            if g.members.len() >= 2 {
                let x0 = mx0 + plan.events[g.members[0]].rel_x - beam_overhang;
                let x1 = mx0 + plan.events[*g.members.last().unwrap()].rel_x + beam_overhang;
                prims.push(beam_bar(x0, x1, beam_render_y));
            } else if let Some(&idx) = g.members.first() {
                let x = mx0 + plan.events[idx].rel_x;
                let count = beam::flag_count(measure.events[idx].duration());
                prims.extend(flags(x, beam_render_y, count));
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

/// A note's stem: a vertical line from just below its lowest fret number (`top`)
/// down to the shared beam line (`beam_y`).
fn stem(x: f32, top: f32, beam_y: f32) -> Primitive {
    vline(x, top, beam_y, STEM_WEIGHT)
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

/// The rest glyph for a duration (by base value; dotted rests reuse the base
/// glyph and add augmentation dots). Sub-quarter values pick by flag count.
fn rest_glyph(dur: Duration) -> &'static str {
    if dur >= Duration::new(1, 1) {
        "\u{1D13B}" // whole rest
    } else if dur >= Duration::new(1, 2) {
        "\u{1D13C}" // half rest
    } else if dur >= Duration::new(1, 4) {
        "\u{1D13D}" // quarter rest
    } else {
        match beam::flag_count(dur) {
            1 => "\u{1D13E}", // eighth rest
            2 => "\u{1D13F}", // sixteenth rest
            _ => "\u{1D140}", // thirty-second (and shorter) rest
        }
    }
}

/// All marks an event draws: right-hand finger/strum marks, technique marks
/// (h/p/sl text, bend/choke paths), and a tie arc to the following note. Each
/// carries the event's source span.
fn marks_for(
    event: &Event,
    x: f32,
    next_x: Option<f32>,
    staff_top: f32,
    staff_bottom: f32,
) -> Vec<Primitive> {
    let rh_y = staff_bottom + BEAM_DROP + RH_ROW_GAP;
    let line_y = |s: u8| staff_top + f32::from(s.saturating_sub(1)) * STRING_SPACING;
    let mut out = Vec::new();
    match &event.kind {
        EventKind::Note(n) => {
            if let Some(rh) = n.right_hand {
                out.push(rh_mark(rh, x, rh_y, event.span));
            }
            if let Some(t) = n.technique {
                out.extend(technique_mark(t, x, line_y(n.pos.string), event.span));
            }
            if let (true, Some(nx)) = (n.tie, next_x) {
                out.push(tie_arc(x, nx, line_y(n.pos.string), event.span));
            }
        }
        EventKind::Chord(c) => {
            for (i, cn) in c.notes.iter().enumerate() {
                if let Some(rh) = cn.right_hand {
                    out.push(rh_mark(rh, x + i as f32 * RH_STAGGER, rh_y, event.span));
                }
            }
        }
        EventKind::Rest(_) => {}
    }
    out
}

/// A right-hand finger letter (T/I/M) or strum arrow.
fn rh_mark(rh: RightHand, x: f32, y: f32, span: Span) -> Primitive {
    let (content, role) = match rh {
        RightHand::Finger(Finger::Thumb) => ("T", TextRole::Finger),
        RightHand::Finger(Finger::Index) => ("I", TextRole::Finger),
        RightHand::Finger(Finger::Middle) => ("M", TextRole::Finger),
        RightHand::Strum(Strum::Down) => ("\u{2193}", TextRole::Strum),
        RightHand::Strum(Strum::Up) => ("\u{2191}", TextRole::Strum),
    };
    Primitive::Text {
        x,
        y,
        content: content.to_string(),
        role,
        span: Some(span),
    }
}

/// A left-hand technique mark: h/p/sl as text above the note; bend and choke as
/// short path arcs rising from it. Ghost notes draw no mark here.
fn technique_mark(t: Technique, x: f32, note_y: f32, span: Span) -> Vec<Primitive> {
    let text = |content: &str| Primitive::Text {
        x: x + TECH_DX,
        y: note_y - TECH_DY,
        content: content.to_string(),
        role: TextRole::Technique,
        span: Some(span),
    };
    match t {
        Technique::HammerOn => vec![text("h")],
        Technique::PullOff => vec![text("p")],
        Technique::SlideTo => vec![text("sl")],
        Technique::Bend => vec![Primitive::Path {
            cmds: format!(
                "M {x} {note_y} Q {} {} {} {}",
                x + 0.3,
                note_y - 1.0,
                x + 0.6,
                note_y - 1.0
            ),
            span: Some(span),
        }],
        Technique::Choke => vec![Primitive::Path {
            cmds: format!("M {x} {note_y} q 0.3 -0.9 0.6 0"),
            span: Some(span),
        }],
        Technique::Ghost => vec![],
    }
}

/// A tie: a slur arc from one note to the next, bowed above the line.
fn tie_arc(x1: f32, x2: f32, y: f32, span: Span) -> Primitive {
    Primitive::Path {
        cmds: format!(
            "M {x1} {y} Q {} {} {x2} {y}",
            (x1 + x2) / 2.0,
            y - TIE_HEIGHT
        ),
        span: Some(span),
    }
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
        Chord, ChordNote, Duration, Event, EventKind, Finger, Note, Position, RightHand, ScoreMeta,
        Strum, Technique, TimeSig,
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
    fn each_note_gets_one_connected_downward_stem() {
        // Notes on strings 3 then 2 (string 2 sits higher on the staff).
        let m = Measure::new(vec![note(3, 0, 0), note(2, 1, 4)]);
        let tree = layout(&banjo_score(vec![m]), cfg());
        let stems = stems(&tree);
        assert_eq!(stems.len(), 2);
        let mut ends = Vec::new();
        let mut lens = Vec::new();
        for s in &stems {
            match s {
                Primitive::Line { y1, y2, weight, .. } => {
                    assert!(y2 > y1, "stem points down");
                    assert_eq!(*weight, STEM_WEIGHT);
                    ends.push(*y2);
                    lens.push(*y2 - *y1);
                }
                _ => unreachable!(),
            }
        }
        // Every stem hangs to the same beam line...
        assert!((ends[0] - ends[1]).abs() < 1e-5);
        // ...but the higher note (string 2) gets the longer stem: stems connect
        // to their own number rather than being a fixed length.
        assert!(lens[1] > lens[0]);
    }

    #[test]
    fn a_stem_connects_its_note_to_a_beam_below_the_staff() {
        // A top-string note: its stem starts just below that number (near the top
        // of the staff) and runs down past the bottom string line to the beam.
        let tree = layout(&banjo_score(vec![Measure::new(vec![note(1, 0, 0)])]), cfg());
        let lines: Vec<f32> = tree.systems[0]
            .prims
            .iter()
            .filter_map(|p| match p {
                Primitive::Line { y1, y2, .. } if y1 == y2 => Some(*y1),
                _ => None,
            })
            .collect();
        let top_line_y = lines.iter().copied().fold(f32::MAX, f32::min);
        let bottom_line_y = lines.iter().copied().fold(f32::MIN, f32::max);
        match stems(&tree)[0] {
            Primitive::Line { y1, y2, .. } => {
                // Starts just below the top string's number...
                assert!((*y1 - (top_line_y + STEM_NOTE_GAP)).abs() < 1e-5);
                // ...and ends on the beam, below the bottom string line.
                assert!(*y2 > bottom_line_y);
            }
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
        // Flat, and spanning the two note x-positions — overhanging each outer
        // stem by half a stem-width so the bar reaches the stems' outer edges.
        let nums = fret_numbers(&tree);
        let overhang = STEM_WEIGHT / 2.0;
        match beams[0] {
            Primitive::Line { x1, y1, x2, y2, .. } => {
                assert_eq!(y1, y2);
                assert!((x1 - (x_of(nums[0]) - overhang)).abs() < 1e-5);
                assert!((x2 - (x_of(nums[1]) + overhang)).abs() < 1e-5);
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
    fn a_beam_top_edge_sits_flush_with_the_stem_ends() {
        let m = Measure::new(vec![eighth(3, 0, 0), eighth(2, 0, 4)]);
        let tree = layout(&banjo_score(vec![m]), cfg());
        let beam_center = match beams(&tree)[0] {
            Primitive::Line { y1, .. } => *y1,
            _ => unreachable!(),
        };
        // The beam hangs below the stem ends: its top edge (center minus half its
        // weight) is flush with every stem's lower end.
        let beam_top = beam_center - BEAM_WEIGHT / 2.0;
        for s in stems(&tree) {
            match s {
                Primitive::Line { y2, .. } => assert!((y2 - beam_top).abs() < 1e-5),
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
        // The outermost flag's top edge is flush with the stem end; the next is
        // higher (smaller y), stacking toward the staff.
        assert!((ys[0] - (stem_end + BEAM_WEIGHT / 2.0)).abs() < 1e-5);
        assert!(ys[1] < ys[0]);
    }

    #[test]
    fn a_beamed_eighth_has_no_flag() {
        let m = Measure::new(vec![eighth(3, 0, 0), eighth(2, 0, 4)]);
        let tree = layout(&banjo_score(vec![m]), cfg());
        assert!(flag_stubs(&tree).is_empty());
        assert_eq!(beams(&tree).len(), 1);
    }

    /// Augmentation dots are the path primitives inside measure boxes (repeat
    /// dots live in the system's own prims).
    fn aug_dots(tree: &RenderTree) -> Vec<&Primitive> {
        tree.systems
            .iter()
            .flat_map(|s| s.measures.iter())
            .flat_map(|m| m.prims.iter())
            .filter(|p| matches!(p, Primitive::Path { .. }))
            .collect()
    }

    fn dotted(string: u8, fret: u8, start: u32, den: u32, dots: u8) -> Event {
        note_dur(
            string,
            fret,
            start,
            Duration::from_denominator(den).dotted(dots),
        )
    }

    #[test]
    fn a_dotted_quarter_draws_one_augmentation_dot() {
        let tree = layout(
            &banjo_score(vec![Measure::new(vec![dotted(3, 0, 0, 4, 1)])]),
            cfg(),
        );
        assert_eq!(aug_dots(&tree).len(), 1);
    }

    #[test]
    fn a_double_dotted_note_draws_two_dots() {
        let tree = layout(
            &banjo_score(vec![Measure::new(vec![dotted(3, 0, 0, 4, 2)])]),
            cfg(),
        );
        assert_eq!(aug_dots(&tree).len(), 2);
    }

    #[test]
    fn an_undotted_note_has_no_augmentation_dot() {
        let tree = layout(&banjo_score(vec![Measure::new(vec![note(3, 0, 0)])]), cfg());
        assert!(aug_dots(&tree).is_empty());
    }

    #[test]
    fn a_dotted_chord_dots_each_member() {
        let chord = Event::new(
            EventKind::Chord(Chord {
                dur: Duration::from_denominator(4).dotted(1),
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
        assert_eq!(aug_dots(&tree).len(), 2);
    }

    /// Rest glyphs are the rest-role text primitives inside measure boxes.
    fn rests(tree: &RenderTree) -> Vec<&Primitive> {
        tree.systems
            .iter()
            .flat_map(|s| s.measures.iter())
            .flat_map(|m| m.prims.iter())
            .filter(|p| {
                matches!(
                    p,
                    Primitive::Text {
                        role: TextRole::Rest,
                        ..
                    }
                )
            })
            .collect()
    }

    fn rest(start: u32, den: u32) -> Event {
        Event::new(
            EventKind::Rest(Duration::from_denominator(den)),
            Span::new(start, start + 2),
        )
    }

    #[test]
    fn a_rest_draws_a_glyph_with_no_stem() {
        let tree = layout(&banjo_score(vec![Measure::new(vec![rest(0, 4)])]), cfg());
        assert_eq!(rests(&tree).len(), 1);
        assert!(stems(&tree).is_empty());
    }

    #[test]
    fn rest_glyphs_match_their_duration() {
        assert_eq!(rest_glyph(Duration::new(1, 1)), "\u{1D13B}");
        assert_eq!(rest_glyph(Duration::new(1, 2)), "\u{1D13C}");
        assert_eq!(rest_glyph(Duration::from_denominator(4)), "\u{1D13D}");
        assert_eq!(rest_glyph(Duration::from_denominator(8)), "\u{1D13E}");
        assert_eq!(rest_glyph(Duration::from_denominator(16)), "\u{1D13F}");
        // A dotted quarter rest keeps the quarter glyph.
        assert_eq!(
            rest_glyph(Duration::from_denominator(4).dotted(1)),
            "\u{1D13D}"
        );
    }

    #[test]
    fn a_rest_carries_its_span() {
        let tree = layout(&banjo_score(vec![Measure::new(vec![rest(5, 4)])]), cfg());
        match rests(&tree)[0] {
            Primitive::Text { span, .. } => assert_eq!(*span, Some(Span::new(5, 7))),
            _ => unreachable!(),
        }
    }

    #[test]
    fn a_dotted_rest_draws_augmentation_dots() {
        let dotted_rest = Event::new(
            EventKind::Rest(Duration::from_denominator(4).dotted(1)),
            Span::new(0, 2),
        );
        let tree = layout(&banjo_score(vec![Measure::new(vec![dotted_rest])]), cfg());
        assert_eq!(rests(&tree).len(), 1);
        assert_eq!(aug_dots(&tree).len(), 1);
    }

    #[test]
    fn a_rest_breaks_a_beam() {
        // Eighth, eighth-rest, eighth in one beat: the rest splits the eighths so
        // neither pair beams — both eighths stand alone (flags), no beam.
        let m = Measure::new(vec![eighth(3, 0, 0), rest(4, 8), eighth(2, 0, 8)]);
        let tree = layout(&banjo_score(vec![m]), cfg());
        assert!(beams(&tree).is_empty());
        assert_eq!(rests(&tree).len(), 1);
    }

    fn texts_with(tree: &RenderTree, role: TextRole) -> Vec<&Primitive> {
        tree.systems
            .iter()
            .flat_map(|s| s.measures.iter())
            .flat_map(|m| m.prims.iter())
            .filter(move |p| matches!(p, Primitive::Text { role: r, .. } if *r == role))
            .collect()
    }

    fn paths(tree: &RenderTree) -> Vec<&Primitive> {
        tree.systems
            .iter()
            .flat_map(|s| s.measures.iter())
            .flat_map(|m| m.prims.iter())
            .filter(|p| matches!(p, Primitive::Path { .. }))
            .collect()
    }

    fn marked_note(
        string: u8,
        fret: u8,
        rh: Option<RightHand>,
        tech: Option<Technique>,
        tie: bool,
    ) -> Event {
        Event::new(
            EventKind::Note(Note {
                pos: Position::new(string, fret),
                dur: Duration::from_denominator(4),
                right_hand: rh,
                technique: tech,
                tie,
            }),
            Span::new(0, 3),
        )
    }

    #[test]
    fn a_finger_mark_is_a_letter_below_the_staff() {
        let n = marked_note(3, 0, Some(RightHand::Finger(Finger::Thumb)), None, false);
        let tree = layout(&banjo_score(vec![Measure::new(vec![n])]), cfg());
        let fingers = texts_with(&tree, TextRole::Finger);
        assert_eq!(fingers.len(), 1);
        let staff_bottom = 6.5; // banjo: staff_top 2.5 + 4
        match fingers[0] {
            Primitive::Text {
                content, y, span, ..
            } => {
                assert_eq!(content, "T");
                assert!(*y > staff_bottom);
                assert_eq!(*span, Some(Span::new(0, 3)));
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn finger_letters_map_thumb_index_middle() {
        for (f, want) in [
            (Finger::Thumb, "T"),
            (Finger::Index, "I"),
            (Finger::Middle, "M"),
        ] {
            let n = marked_note(3, 0, Some(RightHand::Finger(f)), None, false);
            let tree = layout(&banjo_score(vec![Measure::new(vec![n])]), cfg());
            match texts_with(&tree, TextRole::Finger)[0] {
                Primitive::Text { content, .. } => assert_eq!(content, want),
                _ => unreachable!(),
            }
        }
    }

    #[test]
    fn a_strum_is_an_arrow_glyph() {
        let n = marked_note(1, 0, Some(RightHand::Strum(Strum::Down)), None, false);
        let tree = layout(&banjo_score(vec![Measure::new(vec![n])]), cfg());
        let strums = texts_with(&tree, TextRole::Strum);
        assert_eq!(strums.len(), 1);
        match strums[0] {
            Primitive::Text { content, .. } => assert_eq!(content, "\u{2193}"),
            _ => unreachable!(),
        }
    }

    #[test]
    fn technique_text_marks_render_hps() {
        for (t, want) in [
            (Technique::HammerOn, "h"),
            (Technique::PullOff, "p"),
            (Technique::SlideTo, "sl"),
        ] {
            let n = marked_note(3, 0, None, Some(t), false);
            let tree = layout(&banjo_score(vec![Measure::new(vec![n])]), cfg());
            let techs = texts_with(&tree, TextRole::Technique);
            assert_eq!(techs.len(), 1);
            match techs[0] {
                Primitive::Text { content, .. } => assert_eq!(content, want),
                _ => unreachable!(),
            }
        }
    }

    #[test]
    fn bend_and_choke_are_paths() {
        for t in [Technique::Bend, Technique::Choke] {
            let n = marked_note(3, 0, None, Some(t), false);
            let tree = layout(&banjo_score(vec![Measure::new(vec![n])]), cfg());
            assert_eq!(paths(&tree).len(), 1);
        }
    }

    #[test]
    fn a_tie_draws_a_path_to_the_next_note() {
        let m = Measure::new(vec![
            marked_note(3, 2, None, None, true),
            marked_note(3, 2, None, None, false),
        ]);
        let tree = layout(&banjo_score(vec![m]), cfg());
        let paths = paths(&tree);
        assert_eq!(paths.len(), 1);
        match paths[0] {
            Primitive::Path { span, .. } => assert_eq!(*span, Some(Span::new(0, 3))),
            _ => unreachable!(),
        }
    }

    #[test]
    fn a_tie_with_no_following_note_draws_nothing() {
        let m = Measure::new(vec![marked_note(3, 2, None, None, true)]);
        let tree = layout(&banjo_score(vec![m]), cfg());
        assert!(paths(&tree).is_empty());
    }

    #[test]
    fn a_chord_marks_each_member() {
        let chord = Event::new(
            EventKind::Chord(Chord {
                dur: Duration::from_denominator(4),
                notes: vec![
                    ChordNote {
                        pos: Position::new(4, 0),
                        right_hand: Some(RightHand::Finger(Finger::Thumb)),
                    },
                    ChordNote {
                        pos: Position::new(1, 0),
                        right_hand: Some(RightHand::Finger(Finger::Index)),
                    },
                ],
            }),
            Span::new(0, 7),
        );
        let tree = layout(&banjo_score(vec![Measure::new(vec![chord])]), cfg());
        assert_eq!(texts_with(&tree, TextRole::Finger).len(), 2);
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
            title: Some("Syntax Showcase".to_string()),
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

    #[test]
    fn dotted_rhythm_layout_snapshot() {
        // A dotted quarter (one dot, stem only) then a lone eighth (a flag).
        let m = Measure::new(vec![dotted(3, 0, 0, 4, 1), eighth(2, 1, 6)]);
        let tree = layout(&banjo_score(vec![m]), cfg());
        insta::assert_snapshot!(serde_json::to_string_pretty(&tree).unwrap());
    }

    #[test]
    fn rests_layout_snapshot() {
        // Quarter rest, a beamed eighth pair, then another quarter rest.
        let m = Measure::new(vec![
            rest(0, 4),
            eighth(3, 0, 4),
            eighth(2, 0, 8),
            rest(12, 4),
        ]);
        let tree = layout(&banjo_score(vec![m]), cfg());
        insta::assert_snapshot!(serde_json::to_string_pretty(&tree).unwrap());
    }

    #[test]
    fn showcase_render_tree_snapshot() {
        // The canonical example, end to end: source -> Score -> render tree.
        let src = include_str!("../../../examples/showcase.ctab");
        let parsed = crate::parser::parse(src);
        let (score, _) = crate::eval::eval_program(&parsed.program);
        let tree = layout(&score, LayoutConfig { width: 80.0 });
        insta::assert_snapshot!(serde_json::to_string_pretty(&tree).unwrap());
    }

    #[test]
    fn marks_layout_snapshot() {
        // A thumb-picked note, a hammer-on, a tied pair, and a down-strum chord.
        let chord = Event::new(
            EventKind::Chord(Chord {
                dur: Duration::from_denominator(4),
                notes: vec![ChordNote {
                    pos: Position::new(2, 1),
                    right_hand: Some(RightHand::Strum(Strum::Down)),
                }],
            }),
            Span::new(20, 27),
        );
        let m = Measure::new(vec![
            marked_note(3, 0, Some(RightHand::Finger(Finger::Thumb)), None, false),
            marked_note(2, 2, None, Some(Technique::HammerOn), false),
            marked_note(1, 0, None, None, true),
            chord,
        ]);
        let tree = layout(&banjo_score(vec![m]), cfg());
        insta::assert_snapshot!(serde_json::to_string_pretty(&tree).unwrap());
    }
}
