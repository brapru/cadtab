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
use crate::eval::GalleryDef;
use crate::instrument::Instrument;
use crate::model::{
    BarNumbers, Duration, Event, EventKind, Finger, Measure, RightHand, Score, Strum, Technique,
    TimeSig,
};
use crate::render::{
    LayoutMeta, MeasureBox, Page, PaginatedTree, Primitive, Rect, RenderTree, System, TextRole,
};
use crate::span::Span;

/// Inputs that parameterize layout. The same engine serves the screen (viewport
/// width) and export (fixed page width).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutConfig {
    pub width: f32,
}

/// A fixed print page size. The logical page box takes this size's portrait
/// aspect ratio; the absolute scale is set by `PageConfig::content_width`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PageSize {
    /// US Letter, 8.5 × 11 in.
    Letter,
    /// ISO A4, 210 × 297 mm.
    A4,
}

impl PageSize {
    /// Portrait page dimensions in inches `(width, height)`. Used only for the
    /// aspect ratio that shapes the logical page box — pagination never works in
    /// physical units; the exporter chooses the print DPI.
    fn inches(self) -> (f32, f32) {
        match self {
            // 210 mm × 297 mm in inches.
            PageSize::A4 => (8.268, 11.693),
            PageSize::Letter => (8.5, 11.0),
        }
    }
}

/// Inputs that parameterize pagination (T7.19). `content_width` is the justify
/// target in logical units — the same width `layout` packs systems to — so the
/// engine, justification, and per-system geometry are shared with the screen
/// render; the page just bounds the height and breaks systems across pages.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageConfig {
    pub size: PageSize,
    pub content_width: f32,
}

// Vertical metrics (logical units; 1 unit = string spacing).
const TOP_MARGIN: f32 = 0.5;
const TITLE_H: f32 = 2.0;
const COMPOSER_H: f32 = 1.2;
const META_LINE_H: f32 = 1.0;
const HEADER_GAP: f32 = 1.0;
// Column width for one cell of the header tuning grid (e.g. "①=D").
const TUNING_COL_W: f32 = 2.8;
const STRING_SPACING: f32 = 1.0;
const BOTTOM_MARGIN: f32 = 2.8;
// Vertical gap between stacked systems (room below the numbers for stems/marks).
const SYSTEM_GAP: f32 = 3.5;
// Vertical room a continuation page (T7.19, page 2+) reserves at the top for its
// folio number, before the first system. Page one uses its title block instead.
const FOLIO_SPACE: f32 = 1.4;
// Vertical room reserved above the staff for volta brackets, when any exist.
const VOLTA_SPACE: f32 = 1.2;
const VOLTA_GAP: f32 = 0.8;
// Vertical room reserved above the staff for a section label, when any system
// carries one. It sits at the top of the above-staff band, above any voltas.
const SECTION_SPACE: f32 = 1.4;
// Vertical room reserved above the staff for chord symbols, when any system
// carries one. It sits below the section-label row and above any voltas.
const CHORD_SPACE: f32 = 1.3;
// Vertical room reserved above the staff for measure numbers, when numbering is
// on. It sits at the very top of the above-staff band.
const BARNUM_SPACE: f32 = 0.9;
// Def-gallery (D49) metrics. The signature heading's row height, the gap from
// the heading down to the staff it labels, and the gap between one card and the
// next.
const GALLERY_HEADING_H: f32 = 1.6;
const GALLERY_HEADING_GAP: f32 = 1.0;
const GALLERY_DEF_GAP: f32 = 3.0;

// Horizontal metrics.
const LEFT_MARGIN: f32 = 2.0;
const RIGHT_MARGIN: f32 = 1.0;
// Logical width of one whole note; an event's x is proportional to its onset.
const UNITS_PER_WHOLE: f32 = 8.0;
// Padding inside a measure, before the first event and after the last.
const MEASURE_PAD: f32 = 0.8;
// Horizontal space a leading time-signature glyph reserves at a measure's left.
const TIMESIG_WIDTH: f32 = 1.6;
// Half the vertical gap between a time signature's stacked digits: each digit's
// centre sits this far from the staff midline, so the numerator's bottom and the
// denominator's top meet there. Fixed (not staff-height-scaled) so the pair
// stays tightly stacked on instruments with any number of strings.
const TIMESIG_GAP: f32 = 0.5;
// Minimum gap between consecutive events. Spacing is otherwise time-proportional,
// but sub-eighth values (16ths, 32nds) at their natural width pack the fret
// numbers on top of one another, so the gap never drops below this floor. Kept
// just under an eighth's width (1.0) so 16ths still read a touch tighter.
const MIN_EVENT_GAP: f32 = 0.9;

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
    /// The meter to draw as a leading time signature, set on the first measure
    /// and wherever the meter changes; `None` leaves the measure's left open.
    meter_mark: Option<TimeSig>,
}

/// A score resolved up to (but not including) vertical stacking: the pinned page
/// width, the per-measure plans/beams/bar-numbers, and the system groupings. Both
/// the continuous render (`layout`) and the paginated render (`paginate`) share
/// this prep, then differ only in how they stack the systems down the page(s).
struct Prepared {
    width: f32,
    plans: Vec<MeasurePlan>,
    beams: Vec<Vec<beam::BeamGroup>>,
    bar_nums: Vec<Option<u32>>,
    groups: Vec<(usize, usize)>,
    has_barnum: bool,
    staff_height: f32,
    n_strings: usize,
}

/// Resolve a score to its `Prepared` form: measure plans, beam groups, bar
/// numbers, the system groupings, and the page width pinned to the layout target.
fn prepare(score: &Score, config: LayoutConfig) -> Prepared {
    let n_strings = score.instrument.string_count();
    let staff_height = ((n_strings.max(1) - 1) as f32) * STRING_SPACING;

    // Decide where a time signature is drawn: at the first measure and at every
    // meter change (the running meter defaults to 4/4).
    let mut running_meter = TimeSig::new(4, 4);
    let meter_marks: Vec<Option<TimeSig>> = score
        .measures
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let prev = running_meter;
            if let Some(t) = m.meter {
                running_meter = t;
            }
            (i == 0 || running_meter != prev).then_some(running_meter)
        })
        .collect();

    // Resolve each measure's width and event placement, then greedily pack the
    // measures into systems.
    let plans: Vec<MeasurePlan> = score
        .measures
        .iter()
        .zip(&meter_marks)
        .map(|(m, &mark)| plan_measure(m, mark))
        .collect();
    let groups = pack_systems(&plans, config.width);
    // Pin the page to the layout target: at least `config.width`, growing only
    // when a single measure is wider than the target. The centred header and the
    // viewBox then stay put as measures are added, rather than reflowing with the
    // content. (The def-gallery pins the same way.)
    let width = overall_width(&groups, &plans).max(config.width);

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

    // Measure numbers, 1-based over the full bars (pickups are not numbered).
    let mut bar_count = 0u32;
    let bar_nums: Vec<Option<u32>> = score
        .measures
        .iter()
        .map(|m| {
            if m.is_pickup {
                None
            } else {
                bar_count += 1;
                Some(bar_count)
            }
        })
        .collect();

    Prepared {
        width,
        plans,
        beams,
        bar_nums,
        groups,
        has_barnum: score.bar_numbers != BarNumbers::Off,
        staff_height,
        n_strings,
    }
}

/// The height the above-staff band reserves for one system, summing the rows it
/// carries (top → staff): section label, chord symbols, bar number, and volta
/// bracket. Zero when the system carries none of them.
fn band_above(measures: &[Measure], has_barnum: bool) -> f32 {
    let has_section = measures.iter().any(|m| m.section.is_some());
    let has_chord = measures
        .iter()
        .any(|m| m.events.iter().any(|e| e.chord.is_some()));
    let has_volta = measures.iter().any(|m| m.ending.is_some());
    (if has_barnum { BARNUM_SPACE } else { 0.0 })
        + if has_section { SECTION_SPACE } else { 0.0 }
        + if has_chord { CHORD_SPACE } else { 0.0 }
        + if has_volta { VOLTA_SPACE } else { 0.0 }
}

/// Lay a `Score` out into a positioned render tree, wrapping measures into
/// stacked systems no wider than `config.width`.
pub fn layout(score: &Score, config: LayoutConfig) -> RenderTree {
    let prep = prepare(score, config);
    let (header, header_bottom) = build_header(score, prep.width, TOP_MARGIN);

    // Stack the systems vertically, each restating the staff lines.
    let mut systems = Vec::with_capacity(prep.groups.len());
    let mut cursor = header_bottom + HEADER_GAP;
    let mut last_bottom = cursor;
    for &(start, end) in &prep.groups {
        let measures = &score.measures[start..end];
        let band_top = cursor;
        let staff_top = band_top + band_above(measures, prep.has_barnum);
        let staff_bottom = staff_top + prep.staff_height;
        systems.push(build_system(
            measures,
            &prep.plans[start..end],
            &prep.beams[start..end],
            &prep.bar_nums[start..end],
            score.bar_numbers,
            prep.width,
            true,
            band_top,
            staff_top,
            prep.staff_height,
            prep.n_strings,
        ));
        last_bottom = staff_bottom;
        cursor = staff_bottom + SYSTEM_GAP;
    }

    let height = last_bottom + BOTTOM_MARGIN;
    RenderTree {
        meta: LayoutMeta {
            width: prep.width,
            height,
        },
        header,
        systems,
    }
}

/// Geometry of one print page in logical units, derived from a `PageConfig` and
/// the pinned content width.
struct PageGeometry {
    page_width: f32,
    page_height: f32,
    /// The y a continuation page's content band starts from (the top margin).
    top: f32,
    /// The lowest y a system's footprint may reach before it spills to the next
    /// page (the page height less the bottom margin).
    bottom_limit: f32,
}

/// Resolve a page's logical box from its size and the pinned content width. The
/// page is `width` wide (the content already inset by LEFT/RIGHT_MARGIN within
/// it) and takes the size's portrait aspect ratio for its height.
fn page_geometry(config: PageConfig, width: f32) -> PageGeometry {
    let (w_in, h_in) = config.size.inches();
    let page_height = width * (h_in / w_in);
    PageGeometry {
        page_width: width,
        page_height,
        top: TOP_MARGIN,
        bottom_limit: page_height - BOTTOM_MARGIN,
    }
}

/// Lay a `Score` out across fixed-size print pages (T7.19): the same engine and
/// justified systems as `layout`, packed top-to-bottom onto pages of the chosen
/// `PageSize` and broken to a new page when the next system would overflow. Page
/// one carries the full title block; later pages carry a folio number. Each page
/// is its own coordinate space (origin top-left), so the painter draws a page
/// exactly like a single-page tree.
pub fn paginate(score: &Score, config: PageConfig) -> PaginatedTree {
    let prep = prepare(
        score,
        LayoutConfig {
            width: config.content_width,
        },
    );
    let geom = page_geometry(config, prep.width);
    let page_box = Rect {
        x: 0.0,
        y: 0.0,
        w: geom.page_width,
        h: geom.page_height,
    };

    // Page one carries the full title block, laid out from the top margin.
    let (title_block, title_bottom) = build_header(score, prep.width, TOP_MARGIN);

    let mut pages: Vec<Page> = Vec::new();
    let mut i = 0;
    while i < prep.groups.len() {
        let first_page = pages.is_empty();
        // Page one's first system clears the title block; later pages clear the
        // folio band at the top margin.
        let mut cursor = if first_page {
            title_bottom + HEADER_GAP
        } else {
            geom.top + FOLIO_SPACE
        };
        let mut page_systems: Vec<System> = Vec::new();

        // Pack systems down the page until the next one's footprint — its band,
        // staff, and the below-staff room SYSTEM_GAP reserves for stems/marks —
        // would cross the bottom margin. Always place at least one system per
        // page, even if it alone overflows (mirrors `pack_systems` horizontally).
        while i < prep.groups.len() {
            let (start, end) = prep.groups[i];
            let measures = &score.measures[start..end];
            let band_top = cursor;
            let staff_top = band_top + band_above(measures, prep.has_barnum);
            let staff_bottom = staff_top + prep.staff_height;
            if !page_systems.is_empty() && staff_bottom + SYSTEM_GAP > geom.bottom_limit {
                break;
            }
            page_systems.push(build_system(
                measures,
                &prep.plans[start..end],
                &prep.beams[start..end],
                &prep.bar_nums[start..end],
                score.bar_numbers,
                prep.width,
                true,
                band_top,
                staff_top,
                prep.staff_height,
                prep.n_strings,
            ));
            cursor = staff_bottom + SYSTEM_GAP;
            i += 1;
        }

        let header = if first_page {
            title_block.clone()
        } else {
            // Folio number, top-right within the top-margin band (right-anchored
            // by the painter). Pages are 1-based; page one omits its folio.
            vec![Primitive::Text {
                x: prep.width - RIGHT_MARGIN,
                y: geom.top + FOLIO_SPACE / 2.0,
                content: (pages.len() + 1).to_string(),
                role: TextRole::PageNumber,
                span: None,
            }]
        };

        pages.push(Page {
            bounds: page_box,
            header,
            systems: page_systems,
        });
    }

    // An empty score still emits one page, carrying just its (possibly empty)
    // title block — a render is never blank.
    if pages.is_empty() {
        pages.push(Page {
            bounds: page_box,
            header: title_block,
            systems: Vec::new(),
        });
    }

    PaginatedTree {
        page_width: geom.page_width,
        page_height: geom.page_height,
        pages,
    }
}

/// Lay out a library's def-gallery (D49): each previewed `def` is a card — its
/// signature heading over the staff its sample invocation rendered to — stacked
/// down the page. A def with no rendered measures (the signature-card fallback)
/// shows its heading and a muted note. The page is pinned to `config.width` so
/// every card's staff lines align. Heading/fallback text lands in `header` and
/// the staves in `systems`; both are absolute-positioned, so the painter draws
/// them verbatim like any tree.
pub fn layout_gallery(
    instrument: &Instrument,
    defs: &[GalleryDef],
    config: LayoutConfig,
) -> RenderTree {
    let n_strings = instrument.string_count();
    let staff_height = ((n_strings.max(1) - 1) as f32) * STRING_SPACING;
    let width = config.width;

    let mut header: Vec<Primitive> = Vec::new();
    let mut systems: Vec<System> = Vec::new();
    let mut cursor = TOP_MARGIN;

    for def in defs {
        header.push(Primitive::Text {
            x: LEFT_MARGIN,
            y: cursor + GALLERY_HEADING_H / 2.0,
            content: def.signature.clone(),
            role: TextRole::DefHeading,
            span: None,
        });
        cursor += GALLERY_HEADING_H;

        if def.measures.is_empty() {
            // Signature card: the heading plus a muted note that this def takes
            // arguments and so has no sample preview.
            header.push(Primitive::Text {
                x: LEFT_MARGIN,
                y: cursor + META_LINE_H / 2.0,
                content: "parameterized — no preview".to_string(),
                role: TextRole::DefNote,
                span: None,
            });
            cursor += META_LINE_H + GALLERY_DEF_GAP;
            continue;
        }

        // Breathing room between the heading and the staff it labels, so the
        // signature doesn't crowd the top string line.
        cursor += GALLERY_HEADING_GAP;

        // Lay the def's measures out into stacked systems, reusing the engine.
        // The gallery shows no time signatures, bar numbers, or above-staff band,
        // and is not justified — a short sample lick hugs the left at its natural
        // width rather than smearing across the full page.
        let plans: Vec<MeasurePlan> = def.measures.iter().map(|m| plan_measure(m, None)).collect();
        let groups = pack_systems(&plans, width);
        let beams: Vec<Vec<beam::BeamGroup>> = def
            .measures
            .iter()
            .map(|m| beam::beam_groups(&m.events, TimeSig::new(4, 4)))
            .collect();
        let bar_nums: Vec<Option<u32>> = vec![None; def.measures.len()];

        let mut last_bottom = cursor;
        for &(start, end) in &groups {
            let staff_top = cursor;
            let staff_bottom = staff_top + staff_height;
            systems.push(build_system(
                &def.measures[start..end],
                &plans[start..end],
                &beams[start..end],
                &bar_nums[start..end],
                BarNumbers::Off,
                width,
                false,
                staff_top,
                staff_top,
                staff_height,
                n_strings,
            ));
            last_bottom = staff_bottom;
            cursor = staff_bottom + SYSTEM_GAP;
        }
        cursor = last_bottom + GALLERY_DEF_GAP;
    }

    let height = cursor + BOTTOM_MARGIN;
    RenderTree {
        meta: LayoutMeta { width, height },
        header,
        systems,
    }
}

/// Resolve one measure to its width and measure-relative event placement. A
/// `meter_mark` reserves leading space for a time signature drawn at the left.
fn plan_measure(measure: &Measure, meter_mark: Option<TimeSig>) -> MeasurePlan {
    let lead = if meter_mark.is_some() {
        TIMESIG_WIDTH
    } else {
        0.0
    };
    let mut x = MEASURE_PAD + lead;
    let mut events = Vec::with_capacity(measure.events.len());
    for event in &measure.events {
        events.push(PlacedEvent {
            positions: fretted_positions(&event.kind),
            rel_x: x,
            span: event.span,
        });
        // Advance to the next event: time-proportional, but never less than the
        // minimum gap so dense rhythms keep their numbers legible.
        x += span_width(event.duration()).max(MIN_EVENT_GAP);
    }
    let width = x + MEASURE_PAD;
    let span = measure
        .events
        .iter()
        .map(|e| e.span)
        .reduce(|acc, s| acc.merge(s));
    MeasurePlan {
        width,
        events,
        span,
        meter_mark,
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

/// The widest system's extent (with margins) — the content-derived floor for
/// the viewBox width all systems share. Falls back to a minimum for an empty
/// score; the caller pins this up to the layout target (`config.width`).
fn overall_width(groups: &[(usize, usize)], plans: &[MeasurePlan]) -> f32 {
    let widest = groups
        .iter()
        .map(|&(s, e)| plans[s..e].iter().map(|p| p.width).sum::<f32>())
        .fold(0.0_f32, f32::max);
    LEFT_MARGIN + widest.max(UNITS_PER_WHOLE) + RIGHT_MARGIN
}

/// Build one system: its measure boxes (fret numbers) plus the system-spanning
/// furniture (string lines, barlines, volta brackets). When `justify` is set the
/// measures are stretched to fill the page (see the `scale` below); otherwise
/// they sit at their natural widths, left-aligned.
#[allow(clippy::too_many_arguments)]
fn build_system(
    measures: &[Measure],
    plans: &[MeasurePlan],
    beams: &[Vec<beam::BeamGroup>],
    bar_nums: &[Option<u32>],
    bar_mode: BarNumbers,
    width: f32,
    justify: bool,
    band_top: f32,
    staff_top: f32,
    staff_height: f32,
    n_strings: usize,
) -> System {
    let staff_bottom = staff_top + staff_height;
    let beam_y = staff_bottom + BEAM_DROP;
    let line_y = |string: u8| staff_top + (f32::from(string.saturating_sub(1))) * STRING_SPACING;

    // Justify: stretch the measures to fill the page width. The natural content
    // (sum of measure widths) is scaled up uniformly so the last measure's right
    // edge meets `width - RIGHT_MARGIN`. Only base positions — measure widths and
    // event onsets — scale; glyph-relative offsets (augmentation dots, ties, beam
    // overhang, the time-signature column) stay fixed so notes don't smear. Never
    // compress (scale >= 1): a system already at or past the target keeps its
    // spacing, so the widest system (which pins the page) is left untouched.
    let content_w: f32 = plans.iter().map(|p| p.width).sum();
    let scale = if justify && content_w > 0.0 {
        ((width - LEFT_MARGIN - RIGHT_MARGIN) / content_w).max(1.0)
    } else {
        1.0
    };

    // The above-staff band rows, top → staff: section, chord, bar number.
    let has_section = measures.iter().any(|m| m.section.is_some());
    let has_chord = measures
        .iter()
        .any(|m| m.events.iter().any(|e| e.chord.is_some()));
    let rows = band_rows(band_top, has_section, has_chord);
    let chord_row_y = rows.chord_y;

    let mut number_xs: Vec<Vec<f32>> = vec![Vec::new(); n_strings];
    let mut boxes = Vec::with_capacity(plans.len());
    let mut ranges: Vec<(f32, f32)> = Vec::with_capacity(plans.len());
    let mut mx0 = LEFT_MARGIN;
    for ((plan, measure), mbeams) in plans.iter().zip(measures).zip(beams) {
        let mwidth = plan.width * scale;
        let mx1 = mx0 + mwidth;
        let mut prims = Vec::new();
        if let Some(meter) = plan.meter_mark {
            // Centre the glyph in its reserved column so it clears the left
            // barline and the first note each by the measure's leading pad: the
            // column spans [MEASURE_PAD, TIMESIG_WIDTH], the first event sits at
            // MEASURE_PAD + TIMESIG_WIDTH (see plan_measure).
            let tsx = mx0 + (MEASURE_PAD + TIMESIG_WIDTH) / 2.0;
            prims.extend(time_signature(meter, tsx, staff_top, staff_height));
        }
        for (j, (placed, event)) in plan.events.iter().zip(&measure.events).enumerate() {
            let x = mx0 + placed.rel_x * scale;
            // A chord symbol landing on this onset sits above the staff at its x.
            if let Some(sym) = &event.chord {
                prims.push(Primitive::Text {
                    x,
                    y: chord_row_y,
                    content: sym.text.clone(),
                    role: TextRole::ChordSymbol,
                    span: Some(sym.span),
                });
            }
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
            let next_x = plan.events.get(j + 1).map(|p| mx0 + p.rel_x * scale);
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
                let xs: Vec<f32> = g
                    .members
                    .iter()
                    .map(|&m| mx0 + plan.events[m].rel_x * scale)
                    .collect();
                let flag_counts: Vec<u8> = g
                    .members
                    .iter()
                    .map(|&m| beam::flag_count(measure.events[m].duration()))
                    .collect();
                prims.extend(group_beams(&xs, &flag_counts, beam_render_y, beam_overhang));
            } else if let Some(&idx) = g.members.first() {
                let x = mx0 + plan.events[idx].rel_x * scale;
                let count = beam::flag_count(measure.events[idx].duration());
                prims.extend(flags(x, beam_render_y, count));
            }
        }
        boxes.push(MeasureBox {
            bounds: Rect {
                x: mx0,
                y: staff_top,
                w: mwidth,
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
        // Per-line string labels are intentionally omitted: the tuning is shown
        // once in the header (build_header) rather than repeated at every system.
        sys_prims.extend(string_line(LEFT_MARGIN, staff_x1, y, xs));
    }
    sys_prims.extend(barlines(measures, &ranges, staff_top, staff_bottom));
    sys_prims.extend(volta_brackets(measures, &ranges, staff_top));
    sys_prims.extend(section_labels(measures, &ranges, rows.section_y));
    sys_prims.extend(bar_numbers(bar_nums, bar_mode, &ranges, rows.barnum_y));

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

/// All beams for a beamed group of two or more members, given each member's x
/// (`xs`) and beam count (`flag_counts`). One primary beam spans the whole group
/// at `primary_y`; each higher level (2 = sixteenths, 3 = thirty-seconds…) adds a
/// beam over every maximal run of members carrying that level, stacked toward the
/// numbers (above the primary, since stems hang down). A run of one becomes a
/// short partial-beam stub pointing into the group. Bars overhang their outer
/// stems by `overhang` so they reach the stems' outer edges.
fn group_beams(xs: &[f32], flag_counts: &[u8], primary_y: f32, overhang: f32) -> Vec<Primitive> {
    let last = xs.len() - 1;
    let mut out = vec![beam_bar(xs[0] - overhang, xs[last] + overhang, primary_y)];
    let max_level = flag_counts.iter().copied().max().unwrap_or(1);
    for level in 2..=max_level {
        let y = primary_y - f32::from(level - 1) * FLAG_SPACING;
        let mut i = 0;
        while i < xs.len() {
            if flag_counts[i] < level {
                i += 1;
                continue;
            }
            let start = i;
            let mut end = i;
            while end + 1 < xs.len() && flag_counts[end + 1] >= level {
                end += 1;
            }
            if end > start {
                out.push(beam_bar(xs[start] - overhang, xs[end] + overhang, y));
            } else {
                let (x1, x2) = partial_beam(xs, start, overhang);
                out.push(beam_bar(x1, x2, y));
            }
            i = end + 1;
        }
    }
    out
}

/// A partial (fractional) beam stub for a lone member at some level: it points
/// right from the group's first member, otherwise left toward the previous one,
/// its length capped at half the gap to that neighbour so it never collides with
/// it. The far end overhangs the member's stem like a full beam.
fn partial_beam(xs: &[f32], idx: usize, overhang: f32) -> (f32, f32) {
    let x = xs[idx];
    if idx == 0 {
        let len = FLAG_LENGTH.min((xs[1] - x) * 0.5);
        (x - overhang, x + len)
    } else {
        let len = FLAG_LENGTH.min((x - xs[idx - 1]) * 0.5);
        (x - len, x + overhang)
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
        } else if right.is_some_and(|m| m.is_pickup) {
            // The offset edge of a pickup measure stays open.
        } else {
            // Every other barline, including the left edge of each system
            // (k == 0), which closes the staff on the left.
            prims.push(vline(x, top, bottom, BARLINE_WEIGHT));
        }
    }
    prims
}

/// A stacked time signature: the numerator over the denominator, tightly stacked
/// on the staff midline (centred on `x`), independent of the staff's height.
fn time_signature(meter: TimeSig, x: f32, staff_top: f32, staff_height: f32) -> Vec<Primitive> {
    let mid = staff_top + staff_height / 2.0;
    let glyph = |content: String, y: f32| Primitive::Text {
        x,
        y,
        content,
        role: TextRole::TimeSig,
        span: None,
    };
    vec![
        glyph(meter.num.to_string(), mid - TIMESIG_GAP),
        glyph(meter.den.to_string(), mid + TIMESIG_GAP),
    ]
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

/// Baselines for the above-staff band rows, stacked top → staff: bar number,
/// section label, chord symbol. A row's height only pushes the rows below it
/// down when that row is actually present in this system.
struct BandRows {
    barnum_y: f32,
    section_y: f32,
    chord_y: f32,
}

/// Stacked top → staff: section label, chord symbol, then bar number (the
/// bar number sits closest to the staff, just above any volta). A row's height
/// only pushes the rows below it down when that row is present in this system.
fn band_rows(band_top: f32, has_section: bool, has_chord: bool) -> BandRows {
    let mut y = band_top;
    let section_y = y + SECTION_SPACE / 2.0;
    if has_section {
        y += SECTION_SPACE;
    }
    let chord_y = y + CHORD_SPACE / 2.0;
    if has_chord {
        y += CHORD_SPACE;
    }
    let barnum_y = y + BARNUM_SPACE / 2.0;
    BandRows {
        barnum_y,
        section_y,
        chord_y,
    }
}

/// Section labels (rehearsal marks) above the staff: each measure carrying a
/// `section` mark gets its label left-anchored at the measure's start, on the
/// band's section row (`baseline`). Span-tagged so it maps back to its source.
fn section_labels(measures: &[Measure], ranges: &[(f32, f32)], baseline: f32) -> Vec<Primitive> {
    measures
        .iter()
        .zip(ranges)
        .filter_map(|(m, &(x0, _))| {
            m.section.as_ref().map(|label| Primitive::Text {
                x: x0,
                y: baseline,
                content: label.text.clone(),
                role: TextRole::SectionLabel,
                span: Some(label.span),
            })
        })
        .collect()
}

/// Measure numbers above the staff. `off` draws none; `lines` numbers only the
/// first measure of the system; `all` numbers every measure. Pickups carry no
/// number (`None`) and are skipped. Left-anchored at the measure's start.
fn bar_numbers(
    bar_nums: &[Option<u32>],
    mode: BarNumbers,
    ranges: &[(f32, f32)],
    baseline: f32,
) -> Vec<Primitive> {
    let number = |i: usize, n: u32| Primitive::Text {
        x: ranges[i].0,
        y: baseline,
        content: n.to_string(),
        role: TextRole::BarNumber,
        span: None,
    };
    // Pair each measure with its number, dropping unnumbered pickups.
    let numbered = bar_nums
        .iter()
        .enumerate()
        .filter_map(|(i, n)| n.map(|n| (i, n)));
    match mode {
        BarNumbers::Off => Vec::new(),
        // Only the first numbered measure of the system.
        BarNumbers::Lines => numbered.take(1).map(|(i, n)| number(i, n)).collect(),
        BarNumbers::All => numbered.map(|(i, n)| number(i, n)).collect(),
    }
}

/// Build the header block: a centred title/composer at the top, then a
/// left-aligned tuning block (tuning name over a circled-number string grid), a
/// tempo line, and a capo line. Returns the primitives and the y it ends at.
fn build_header(score: &Score, width: f32, top: f32) -> (Vec<Primitive>, f32) {
    let cx = width / 2.0;
    let mut prims = Vec::new();
    let mut y = top;
    let line =
        |prims: &mut Vec<Primitive>, x: f32, baseline: f32, content: String, role: TextRole| {
            prims.push(Primitive::Text {
                x,
                y: baseline,
                content,
                role,
                span: None,
            });
        };

    // Title and composer, centred at the top.
    if let Some(title) = &score.meta.title {
        prims.push(Primitive::Text {
            x: cx,
            y: y + TITLE_H / 2.0,
            content: title.clone(),
            role: TextRole::Title,
            span: None,
        });
        y += TITLE_H;
    }
    if let Some(composer) = &score.meta.composer {
        prims.push(Primitive::Text {
            x: cx,
            y: y + COMPOSER_H / 2.0,
            content: composer.clone(),
            role: TextRole::Composer,
            span: None,
        });
        y += COMPOSER_H;
    }
    y += HEADER_GAP;

    // Left-aligned tuning block: the tuning name (omitted for an unnamed custom
    // tuning) over a circled-number grid that pairs strings into columns — odds
    // on the top row, evens below.
    if let Some(name) = &score.instrument.tuning {
        line(
            &mut prims,
            LEFT_MARGIN,
            y + META_LINE_H / 2.0,
            name.clone(),
            TextRole::TuningName,
        );
        y += META_LINE_H;
    }

    let n = score.instrument.string_count();
    let cols = n.div_ceil(2);
    let grid_top = y;
    for col in 0..cols {
        let col_x = LEFT_MARGIN + col as f32 * TUNING_COL_W;
        let top = 2 * col + 1;
        let bottom = 2 * col + 2;
        prims.push(tuning_cell(
            &score.instrument,
            top,
            col_x,
            grid_top + META_LINE_H / 2.0,
        ));
        if bottom <= n {
            prims.push(tuning_cell(
                &score.instrument,
                bottom,
                col_x,
                grid_top + META_LINE_H + META_LINE_H / 2.0,
            ));
        }
    }
    y = grid_top + 2.0 * META_LINE_H;

    // Tempo, centred under the grid.
    if let Some(tempo) = score.meta.tempo {
        let grid_cx = LEFT_MARGIN + cols as f32 * TUNING_COL_W / 2.0;
        line(
            &mut prims,
            grid_cx,
            y + META_LINE_H / 2.0,
            format!("♩ = {tempo}"),
            TextRole::Tempo,
        );
        y += META_LINE_H;
    }
    // Capo, left-aligned below.
    if !score.capo.is_empty() {
        line(
            &mut prims,
            LEFT_MARGIN,
            y + META_LINE_H / 2.0,
            format!("Capo {}", score.capo.join(", ")),
            TextRole::Capo,
        );
        y += META_LINE_H;
    }

    (prims, y)
}

/// One cell of the header tuning grid: a circled string number and its open-note
/// label (e.g. `①=D`), left-anchored at `(x, y)`.
fn tuning_cell(
    instrument: &crate::instrument::Instrument,
    string: usize,
    x: f32,
    y: f32,
) -> Primitive {
    let label = instrument.strings[string - 1].label.clone();
    Primitive::Text {
        x,
        y,
        content: format!("{}={}", circled_digit(string), label),
        role: TextRole::TuningString,
        span: None,
    }
}

/// A string number as a circled digit (①..⑳); falls back to `(n)` beyond 20.
fn circled_digit(n: usize) -> String {
    if (1..=20).contains(&n) {
        char::from_u32(0x2460 + (n as u32 - 1))
            .expect("0x2460..0x2473 are circled digits")
            .to_string()
    } else {
        format!("({n})")
    }
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

    /// A target so narrow any real measure overflows it, so the page pins to the
    /// content's own width and justification is a no-op (scale = 1). Tests that
    /// assert *natural* intra-measure spacing use this, both to read the
    /// unstretched geometry and to keep coordinates small (large justified x's
    /// lose float precision in `x2 - x1` reconstructions).
    fn cfg_natural() -> LayoutConfig {
        LayoutConfig { width: 1.0 }
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
        // Bar numbers off by default in unit tests so they don't perturb other
        // assertions; the `lines` default is covered by eval + the showcase.
        Score {
            meta: ScoreMeta::default(),
            instrument: Instrument::builtin("banjo").unwrap(),
            capo: vec![],
            bar_numbers: BarNumbers::Off,
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

    /// The numerator/denominator digits of every time signature drawn, as a
    /// flat list of their `content` strings across all measure boxes.
    fn time_sig_digits(tree: &RenderTree) -> Vec<String> {
        tree.systems
            .iter()
            .flat_map(|s| s.measures.iter())
            .flat_map(|m| m.prims.iter())
            .filter_map(|p| match p {
                Primitive::Text {
                    role: TextRole::TimeSig,
                    content,
                    ..
                } => Some(content.clone()),
                _ => None,
            })
            .collect()
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
        let tree = layout(&banjo_score(vec![m]), cfg_natural());
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
        // No repeats: a leading barline closing the staff, an interior barline,
        // and the final one.
        assert_eq!(vertical_lines(&tree).len(), 3);
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
    fn the_first_measure_shows_the_default_time_signature() {
        // No explicit meter: the opening measure still states the 4/4 default,
        // and a following measure does not repeat it.
        let tree = layout(
            &banjo_score(vec![
                Measure::new(vec![note(3, 0, 0)]),
                Measure::new(vec![note(3, 0, 4)]),
            ]),
            cfg(),
        );
        assert_eq!(time_sig_digits(&tree), vec!["4", "4"]);
    }

    #[test]
    fn a_meter_change_redraws_the_time_signature() {
        let first = Measure::new(vec![note(3, 0, 0)]);
        let mut changed = Measure::new(vec![note(3, 0, 0)]);
        changed.meter = Some(TimeSig::new(3, 4));
        let unchanged = Measure::new(vec![note(3, 0, 0)]);
        let tree = layout(&banjo_score(vec![first, changed, unchanged]), cfg());
        // 4/4 at the start, 3/4 at the change, nothing on the third measure.
        assert_eq!(time_sig_digits(&tree), vec!["4", "4", "3", "4"]);
    }

    /// The y-distance between a time signature's two digits, from its first
    /// measure box. Panics if no time signature was drawn.
    fn time_sig_digit_gap(tree: &RenderTree) -> f32 {
        let ys: Vec<f32> = tree.systems[0].measures[0]
            .prims
            .iter()
            .filter_map(|p| match p {
                Primitive::Text {
                    role: TextRole::TimeSig,
                    y,
                    ..
                } => Some(*y),
                _ => None,
            })
            .collect();
        assert_eq!(ys.len(), 2, "expected a numerator and denominator");
        (ys[1] - ys[0]).abs()
    }

    #[test]
    fn the_time_signature_gap_is_independent_of_string_count() {
        // The stacked digits stay tightly spaced regardless of how many strings
        // the staff has, so wider instruments don't blow the pair apart.
        let measure = || Measure::new(vec![note(3, 0, 0)]);
        let banjo = layout(&banjo_score(vec![measure()]), cfg());
        let guitar = layout(
            &Score {
                meta: ScoreMeta::default(),
                instrument: Instrument::builtin("guitar").unwrap(),
                capo: vec![],
                bar_numbers: BarNumbers::Off,
                measures: vec![measure()],
            },
            cfg(),
        );
        assert_eq!(time_sig_digit_gap(&banjo), time_sig_digit_gap(&guitar));
        assert_eq!(time_sig_digit_gap(&banjo), 2.0 * TIMESIG_GAP);
    }

    #[test]
    fn the_first_note_clears_the_time_signature() {
        let tree = layout(
            &banjo_score(vec![Measure::new(vec![note(3, 0, 0)])]),
            cfg_natural(),
        );
        let box0 = &tree.systems[0].measures[0];
        let mx0 = box0.bounds.x;
        let glyph_x = box0
            .prims
            .iter()
            .find_map(|p| match p {
                Primitive::Text {
                    role: TextRole::TimeSig,
                    x,
                    ..
                } => Some(*x),
                _ => None,
            })
            .unwrap();
        let note_x = x_of(fret_numbers(&tree)[0]);
        // The glyph is centred in its reserved column (a fixed offset, unscaled by
        // justification), and the first note sits at least a full leading pad past
        // it — so neither the barline nor the note crowds the time signature. The
        // page stretch only ever pushes the note further right, never nearer.
        assert!((glyph_x - mx0 - (MEASURE_PAD + TIMESIG_WIDTH) / 2.0).abs() < 1e-3);
        assert!(note_x - mx0 >= (MEASURE_PAD + TIMESIG_WIDTH) - 1e-3);
        // The note clears the glyph's centre by more than the bare leading pad.
        assert!(note_x - glyph_x > MEASURE_PAD);
    }

    #[test]
    fn a_time_signature_reserves_leading_width() {
        // A meter mark widens the measure plan by exactly the reserved
        // time-signature column. Tested on `plan_measure` directly: the planned
        // width is pre-justification, so it isn't perturbed by the page stretch.
        let m = Measure::new(vec![note(3, 0, 0)]);
        let plain_w = plan_measure(&m, None).width;
        let marked_w = plan_measure(&m, Some(TimeSig::new(3, 4))).width;
        assert!((marked_w - plain_w - TIMESIG_WIDTH).abs() < 1e-3);
    }

    /// The content of the first header text prim carrying `role`, if any.
    fn header_text(tree: &RenderTree, role: TextRole) -> Option<String> {
        tree.header.iter().find_map(|p| match p {
            Primitive::Text {
                role: r, content, ..
            } if *r == role => Some(content.clone()),
            _ => None,
        })
    }

    /// Count the header text prims carrying `role`.
    fn header_role_count(tree: &RenderTree, role: TextRole) -> usize {
        tree.header
            .iter()
            .filter(|p| matches!(p, Primitive::Text { role: r, .. } if *r == role))
            .count()
    }

    #[test]
    fn the_header_lays_out_the_lead_sheet_blocks() {
        let score = Score {
            meta: ScoreMeta {
                title: Some("Spotted Pony".into()),
                composer: Some("Eli Gilbert".into()),
                tempo: Some(100),
            },
            instrument: Instrument::builtin("banjo")
                .unwrap()
                .with_tuning("doubleC", Span::new(0, 0))
                .unwrap(),
            capo: vec!["2".into()],
            bar_numbers: BarNumbers::Off,
            measures: vec![Measure::new(vec![note(3, 0, 0)])],
        };
        let tree = layout(&score, cfg());
        assert_eq!(
            header_text(&tree, TextRole::Title).as_deref(),
            Some("Spotted Pony")
        );
        assert_eq!(
            header_text(&tree, TextRole::Composer).as_deref(),
            Some("Eli Gilbert")
        );
        // The tuning name sits over a circled-number grid — one cell per string,
        // the first being string 1 (Double C → ①=D).
        assert_eq!(
            header_text(&tree, TextRole::TuningName).as_deref(),
            Some("Double C")
        );
        assert_eq!(header_role_count(&tree, TextRole::TuningString), 5);
        assert_eq!(
            header_text(&tree, TextRole::TuningString).as_deref(),
            Some("①=D")
        );
        // Tempo and capo each on their own line.
        assert_eq!(
            header_text(&tree, TextRole::Tempo).as_deref(),
            Some("♩ = 100")
        );
        assert_eq!(
            header_text(&tree, TextRole::Capo).as_deref(),
            Some("Capo 2")
        );
    }

    #[test]
    fn the_header_omits_absent_title_tempo_and_capo() {
        let score = Score {
            meta: ScoreMeta::default(),
            instrument: Instrument::builtin("banjo").unwrap(),
            capo: vec![],
            bar_numbers: BarNumbers::Off,
            measures: vec![Measure::new(vec![note(3, 0, 0)])],
        };
        let tree = layout(&score, cfg());
        // The tuning block always renders; title/tempo/capo drop out cleanly.
        assert_eq!(
            header_text(&tree, TextRole::TuningName).as_deref(),
            Some("Open G")
        );
        assert_eq!(header_role_count(&tree, TextRole::TuningString), 5);
        assert!(header_text(&tree, TextRole::Title).is_none());
        assert!(header_text(&tree, TextRole::Tempo).is_none());
        assert!(header_text(&tree, TextRole::Capo).is_none());
    }

    #[test]
    fn an_unnamed_custom_tuning_drops_the_name_caption() {
        // A custom tuning with no display name renders the string grid but no
        // tuning-name line.
        let strings = ["D4", "B3", "G3", "D3", "g4"]
            .map(|p| crate::instrument::StringDef {
                open_pitch: crate::model::Pitch::from_name(p).unwrap(),
                label: p.trim_end_matches(|c: char| c.is_ascii_digit()).into(),
            })
            .to_vec();
        let score = Score {
            meta: ScoreMeta::default(),
            instrument: Instrument::builtin("banjo")
                .unwrap()
                .with_custom_strings(None, strings, Span::new(0, 0))
                .unwrap(),
            capo: vec![],
            bar_numbers: BarNumbers::Off,
            measures: vec![Measure::new(vec![note(3, 0, 0)])],
        };
        let tree = layout(&score, cfg());
        assert!(header_text(&tree, TextRole::TuningName).is_none());
        // The circled-number grid still renders one cell per string.
        assert_eq!(header_role_count(&tree, TextRole::TuningString), 5);
    }

    #[test]
    fn voltas_reserve_room_above_the_staff() {
        let plain = layout(&banjo_score(vec![Measure::new(vec![note(3, 0, 0)])]), cfg());
        let mut m = Measure::new(vec![note(3, 0, 0)]);
        m.ending = Some(1);
        let with_volta = layout(&banjo_score(vec![m]), cfg());
        assert!(with_volta.meta.height > plain.meta.height);
    }

    fn section_prims(tree: &RenderTree) -> Vec<&Primitive> {
        tree.systems
            .iter()
            .flat_map(|s| s.prims.iter())
            .filter(|p| {
                matches!(
                    p,
                    Primitive::Text {
                        role: TextRole::SectionLabel,
                        ..
                    }
                )
            })
            .collect()
    }

    fn labeled(text: &str, string: u8, fret: u8) -> Measure {
        let mut m = Measure::new(vec![note(string, fret, 0)]);
        m.section = Some(crate::model::SectionLabel {
            text: text.into(),
            span: Span::new(0, 1),
        });
        m
    }

    #[test]
    fn a_section_label_draws_above_the_staff_at_the_measure_start() {
        let tree = layout(&banjo_score(vec![labeled("A", 3, 0)]), cfg());
        let labels = section_prims(&tree);
        assert_eq!(labels.len(), 1);
        match labels[0] {
            Primitive::Text {
                content, x, span, ..
            } => {
                assert_eq!(content, "A");
                // Left-anchored at the system's left margin (first measure start).
                assert!((x - LEFT_MARGIN).abs() < 1e-5);
                assert!(span.is_some()); // span-tagged for bidi mapping
            }
            _ => unreachable!(),
        }
        // Above the staff: smaller y than the fret number.
        assert!(y_of(labels[0]) < y_of(fret_numbers(&tree)[0]));
    }

    #[test]
    fn a_section_label_reserves_room_above_the_staff() {
        let plain = layout(&banjo_score(vec![Measure::new(vec![note(3, 0, 0)])]), cfg());
        let labeled = layout(&banjo_score(vec![labeled("A", 3, 0)]), cfg());
        assert!(labeled.meta.height > plain.meta.height);
    }

    #[test]
    fn a_section_label_stacks_above_a_volta_bracket() {
        let mut m = labeled("A", 3, 0);
        m.ending = Some(1);
        let tree = layout(&banjo_score(vec![m]), cfg());
        let label_y = y_of(section_prims(&tree)[0]);
        let ending_y = tree.systems[0]
            .prims
            .iter()
            .find_map(|p| match p {
                Primitive::Text {
                    role: TextRole::Ending,
                    y,
                    ..
                } => Some(*y),
                _ => None,
            })
            .unwrap();
        // The rehearsal mark sits at the very top, above the volta number.
        assert!(label_y < ending_y);
    }

    fn chord_prims(tree: &RenderTree) -> Vec<&Primitive> {
        tree.systems
            .iter()
            .flat_map(|s| s.measures.iter())
            .flat_map(|m| m.prims.iter())
            .filter(|p| {
                matches!(
                    p,
                    Primitive::Text {
                        role: TextRole::ChordSymbol,
                        ..
                    }
                )
            })
            .collect()
    }

    fn note_with_chord(text: &str, string: u8, fret: u8) -> Event {
        let mut e = note(string, fret, 0);
        e.chord = Some(crate::model::ChordSymbol {
            text: text.into(),
            span: Span::new(0, 1),
        });
        e
    }

    #[test]
    fn a_chord_symbol_draws_above_the_staff_at_its_beat() {
        let m = Measure::new(vec![note_with_chord("G", 3, 0), note(2, 0, 4)]);
        let tree = layout(&banjo_score(vec![m]), cfg());
        let chords = chord_prims(&tree);
        assert_eq!(chords.len(), 1);
        match chords[0] {
            Primitive::Text { content, span, .. } => {
                assert_eq!(content, "G");
                assert!(span.is_some()); // span-tagged
            }
            _ => unreachable!(),
        }
        // Above the staff, and aligned to its note's column (same x as the fret).
        assert!(y_of(chords[0]) < y_of(fret_numbers(&tree)[0]));
        assert!((x_of(chords[0]) - x_of(fret_numbers(&tree)[0])).abs() < 1e-5);
    }

    #[test]
    fn chord_symbols_reserve_room_above_the_staff() {
        let plain = layout(&banjo_score(vec![Measure::new(vec![note(3, 0, 0)])]), cfg());
        let with_chord = layout(
            &banjo_score(vec![Measure::new(vec![note_with_chord("G", 3, 0)])]),
            cfg(),
        );
        assert!(with_chord.meta.height > plain.meta.height);
    }

    #[test]
    fn chord_symbols_sit_below_section_labels_and_above_voltas() {
        let mut m = Measure::new(vec![note_with_chord("G", 3, 0)]);
        m.section = Some(crate::model::SectionLabel {
            text: "A".into(),
            span: Span::new(0, 1),
        });
        m.ending = Some(1);
        let tree = layout(&banjo_score(vec![m]), cfg());
        let section_y = y_of(section_prims(&tree)[0]);
        let chord_y = y_of(chord_prims(&tree)[0]);
        let ending_y = tree.systems[0]
            .prims
            .iter()
            .find_map(|p| match p {
                Primitive::Text {
                    role: TextRole::Ending,
                    y,
                    ..
                } => Some(*y),
                _ => None,
            })
            .unwrap();
        // Top → staff: section label, chord symbol, volta number.
        assert!(section_y < chord_y);
        assert!(chord_y < ending_y);
    }

    fn numbered_score(measures: Vec<Measure>, mode: BarNumbers) -> Score {
        Score {
            bar_numbers: mode,
            ..banjo_score(measures)
        }
    }

    fn bar_prims(tree: &RenderTree) -> Vec<String> {
        tree.systems
            .iter()
            .flat_map(|s| s.prims.iter())
            .filter_map(|p| match p {
                Primitive::Text {
                    role: TextRole::BarNumber,
                    content,
                    ..
                } => Some(content.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn lines_mode_numbers_only_the_first_bar_of_the_system() {
        let measures = vec![
            Measure::new(vec![note(3, 0, 0)]),
            Measure::new(vec![note(2, 0, 0)]),
            Measure::new(vec![note(1, 0, 0)]),
        ];
        let tree = layout(&numbered_score(measures, BarNumbers::Lines), cfg());
        assert_eq!(bar_prims(&tree), vec!["1"]);
    }

    #[test]
    fn all_mode_numbers_every_bar() {
        let measures = vec![
            Measure::new(vec![note(3, 0, 0)]),
            Measure::new(vec![note(2, 0, 0)]),
            Measure::new(vec![note(1, 0, 0)]),
        ];
        let tree = layout(&numbered_score(measures, BarNumbers::All), cfg());
        assert_eq!(bar_prims(&tree), vec!["1", "2", "3"]);
    }

    #[test]
    fn off_mode_draws_no_bar_numbers() {
        let measures = vec![
            Measure::new(vec![note(3, 0, 0)]),
            Measure::new(vec![note(2, 0, 0)]),
        ];
        let tree = layout(&numbered_score(measures, BarNumbers::Off), cfg());
        assert!(bar_prims(&tree).is_empty());
    }

    #[test]
    fn a_pickup_bar_is_not_numbered() {
        let mut pickup = Measure::new(vec![note(1, 0, 0)]);
        pickup.is_pickup = true;
        let measures = vec![
            pickup,
            Measure::new(vec![note(3, 0, 0)]),
            Measure::new(vec![note(2, 0, 0)]),
        ];
        // `all` would number every full bar; the leading pickup is skipped, so the
        // first full bar is "1".
        let tree = layout(&numbered_score(measures, BarNumbers::All), cfg());
        assert_eq!(bar_prims(&tree), vec!["1", "2"]);
    }

    #[test]
    fn bar_numbers_reserve_room_and_sit_closest_to_the_staff() {
        let plain = layout(
            &numbered_score(vec![Measure::new(vec![note(3, 0, 0)])], BarNumbers::Off),
            cfg(),
        );
        let mut m = Measure::new(vec![note_with_chord("G", 3, 0)]);
        m.section = Some(crate::model::SectionLabel {
            text: "A".into(),
            span: Span::new(0, 1),
        });
        let numbered = layout(&numbered_score(vec![m], BarNumbers::Lines), cfg());
        // Adding the bar-number row grows the sheet.
        assert!(numbered.meta.height > plain.meta.height);
        let bar_y = numbered
            .systems
            .iter()
            .flat_map(|s| s.prims.iter())
            .find_map(|p| match p {
                Primitive::Text {
                    role: TextRole::BarNumber,
                    y,
                    ..
                } => Some(*y),
                _ => None,
            })
            .unwrap();
        // The number sits below the section label and chord symbol — closest to
        // the staff, but still above it.
        assert!(bar_y > y_of(section_prims(&numbered)[0]));
        assert!(bar_y > y_of(chord_prims(&numbered)[0]));
        assert!(bar_y < y_of(fret_numbers(&numbered)[0]));
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
        // Spacing follows event onsets, not the meter stamp. Tested on the plan so
        // the page-justify stretch (which scales every gap equally) can't mask it.
        let mut m = Measure::new(vec![note(3, 0, 0), note(2, 0, 4)]);
        m.meter = Some(TimeSig::new(3, 4));
        let plan = plan_measure(&m, Some(TimeSig::new(3, 4)));
        assert!((plan.events[1].rel_x - plan.events[0].rel_x - 2.0).abs() < 1e-5);
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

    #[test]
    fn four_sixteenths_get_a_primary_and_a_secondary_beam() {
        let d16 = Duration::from_denominator(16);
        let m = Measure::new(vec![
            note_dur(3, 0, 0, d16),
            note_dur(2, 0, 4, d16),
            note_dur(1, 0, 8, d16),
            note_dur(5, 0, 12, d16),
        ]);
        let tree = layout(&banjo_score(vec![m]), cfg());
        let beams = beams(&tree);
        // A primary beam plus one secondary beam, both spanning the group.
        assert_eq!(beams.len(), 2);
        let ys: Vec<f32> = beams
            .iter()
            .map(|b| match b {
                Primitive::Line { y1, .. } => *y1,
                _ => unreachable!(),
            })
            .collect();
        let primary = ys.iter().copied().fold(f32::MIN, f32::max);
        let secondary = ys.iter().copied().fold(f32::MAX, f32::min);
        // The secondary sits one flag-spacing above the primary (toward numbers).
        assert!((primary - secondary - FLAG_SPACING).abs() < 1e-5);
    }

    #[test]
    fn an_isolated_sixteenth_in_a_group_gets_a_partial_beam() {
        // 16th, 8th, 16th fill one beat and beam together; each 16th needs a
        // second beam but has no 16th neighbour, so each gets a partial stub.
        let d16 = Duration::from_denominator(16);
        let d8 = Duration::from_denominator(8);
        let m = Measure::new(vec![
            note_dur(3, 0, 0, d16),
            note_dur(2, 0, 4, d8),
            note_dur(1, 0, 8, d16),
        ]);
        let tree = layout(&banjo_score(vec![m]), cfg());
        let beams = beams(&tree);
        let primary = beams
            .iter()
            .map(|b| match b {
                Primitive::Line { y1, .. } => *y1,
                _ => unreachable!(),
            })
            .fold(f32::MIN, f32::max);
        // One primary across all three, plus two partial stubs one level above.
        let secondary = beams
            .iter()
            .filter(|b| matches!(b, Primitive::Line { y1, .. } if *y1 < primary - 1e-5))
            .count();
        assert_eq!(secondary, 2);
    }

    #[test]
    fn sub_eighth_events_keep_a_minimum_gap() {
        let d16 = Duration::from_denominator(16);
        let sixteenths = Measure::new(vec![
            note_dur(3, 0, 0, d16),
            note_dur(2, 0, 4, d16),
            note_dur(1, 0, 8, d16),
        ]);
        // Tested on the plan, before the page-justify stretch scales the gaps.
        let plan = plan_measure(&sixteenths, None);
        // Time-proportionally 16ths are 0.5 apart; the floor lifts each gap to
        // MIN_EVENT_GAP so the fret numbers do not overlap.
        assert!((plan.events[1].rel_x - plan.events[0].rel_x - MIN_EVENT_GAP).abs() < 1e-5);
        assert!((plan.events[2].rel_x - plan.events[1].rel_x - MIN_EVENT_GAP).abs() < 1e-5);

        // Eighths are wider than the floor, so they keep proportional spacing.
        let eighths = Measure::new(vec![eighth(3, 0, 0), eighth(2, 0, 4)]);
        let plan = plan_measure(&eighths, None);
        assert!(
            (plan.events[1].rel_x
                - plan.events[0].rel_x
                - span_width(Duration::from_denominator(8)))
            .abs()
                < 1e-5
        );
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
        let tree = layout(&banjo_score(vec![m]), cfg_natural());
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

    fn banjo() -> Instrument {
        Instrument::builtin("banjo").unwrap()
    }

    fn heading_text(tree: &RenderTree, role: TextRole) -> Vec<&str> {
        tree.header
            .iter()
            .filter_map(|p| match p {
                Primitive::Text {
                    content, role: r, ..
                } if *r == role => Some(content.as_str()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn page_pins_to_the_target_width_when_content_is_narrower() {
        // A lone short measure is far narrower than the 800-unit target, so the
        // page width is the target, not the content-derived width.
        let tree = layout(&banjo_score(vec![Measure::new(vec![note(3, 0, 0)])]), cfg());
        assert_eq!(tree.meta.width, 800.0);
    }

    #[test]
    fn page_grows_past_the_target_when_a_measure_overflows() {
        // A single measure packed with events exceeds a tiny target; the page
        // grows to hold it rather than clipping.
        let events: Vec<Event> = (0..12)
            .map(|i| note_dur(3, 0, i * 2, Duration::from_denominator(8)))
            .collect();
        let narrow = LayoutConfig { width: 10.0 };
        let tree = layout(&banjo_score(vec![Measure::new(events)]), narrow);
        assert!(tree.meta.width > 10.0);
    }

    #[test]
    fn a_short_system_justifies_to_fill_the_page() {
        // One small measure on an 800-unit page: justification stretches it so the
        // final barline lands at the page's right edge (width - RIGHT_MARGIN).
        let m = Measure::new(vec![note(3, 0, 0), note(2, 0, 4)]);
        let tree = layout(&banjo_score(vec![m]), cfg());
        let last_barline = vertical_lines(&tree)
            .iter()
            .map(|p| x_of_line(p))
            .fold(f32::MIN, f32::max);
        assert!((last_barline - (800.0 - RIGHT_MARGIN)).abs() < 1e-3);
    }

    #[test]
    fn justification_preserves_time_proportional_spacing() {
        // quarter, half, quarter: a half advances twice a quarter. Stretching the
        // system scales every gap uniformly, so that 2:1 ratio survives.
        let m = Measure::new(vec![
            note_dur(3, 0, 0, Duration::from_denominator(4)),
            note_dur(2, 1, 4, Duration::from_denominator(2)),
            note_dur(1, 2, 8, Duration::from_denominator(4)),
        ]);
        let tree = layout(&banjo_score(vec![m]), cfg());
        let xs: Vec<f32> = fret_numbers(&tree).iter().map(|p| x_of(p)).collect();
        let first = xs[1] - xs[0];
        let second = xs[2] - xs[1];
        assert!((second / first - 2.0).abs() < 1e-4);
        // And the spacing is genuinely stretched past its natural (unjustified) gap.
        assert!(first > 2.0);
    }

    #[test]
    fn the_widest_system_is_not_stretched() {
        // A measure wide enough to pin the page (a tiny target) sits at scale 1:
        // its first two onsets keep their natural 2.0-unit quarter-note advance.
        let events: Vec<Event> = (0..12)
            .map(|i| note_dur(3, 0, i * 2, Duration::from_denominator(8)))
            .collect();
        let narrow = LayoutConfig { width: 10.0 };
        let tree = layout(&banjo_score(vec![Measure::new(events)]), narrow);
        let xs: Vec<f32> = fret_numbers(&tree).iter().map(|p| x_of(p)).collect();
        // Eighth notes advance 1/8 * 8 = 1.0 at natural scale, unchanged here.
        assert!((xs[1] - xs[0] - 1.0).abs() < 1e-4);
    }

    #[test]
    fn gallery_stacks_a_heading_and_staff_per_rendered_def() {
        let rendered = GalleryDef {
            signature: "lick".to_string(),
            measures: vec![Measure::new(vec![note(3, 0, 0), note(2, 0, 2)])],
        };
        let tree = layout_gallery(&banjo(), std::slice::from_ref(&rendered), cfg());

        // One signature heading, and the def's measures became a staff system.
        assert_eq!(heading_text(&tree, TextRole::DefHeading), vec!["lick"]);
        assert!(heading_text(&tree, TextRole::DefNote).is_empty());
        assert_eq!(tree.systems.len(), 1);
        // The page is pinned to the config width so cards align.
        assert_eq!(tree.meta.width, 800.0);
        assert!(tree.systems[0].measures[0].prims.iter().any(|p| matches!(
            p,
            Primitive::Text {
                role: TextRole::FretNumber,
                ..
            }
        )));
    }

    #[test]
    fn gallery_falls_back_to_a_signature_card_when_a_def_has_no_preview() {
        let card = GalleryDef {
            signature: "roll(c)".to_string(),
            measures: vec![],
        };
        let tree = layout_gallery(&banjo(), std::slice::from_ref(&card), cfg());

        // Heading plus a muted note, and no staff system for an unrendered def.
        assert_eq!(heading_text(&tree, TextRole::DefHeading), vec!["roll(c)"]);
        assert_eq!(heading_text(&tree, TextRole::DefNote).len(), 1);
        assert!(tree.systems.is_empty());
    }

    #[test]
    fn gallery_cards_stack_top_to_bottom() {
        let defs = vec![
            GalleryDef {
                signature: "a".to_string(),
                measures: vec![Measure::new(vec![note(3, 0, 0)])],
            },
            GalleryDef {
                signature: "b".to_string(),
                measures: vec![Measure::new(vec![note(2, 0, 0)])],
            },
        ];
        let tree = layout_gallery(&banjo(), &defs, cfg());
        assert_eq!(heading_text(&tree, TextRole::DefHeading), vec!["a", "b"]);
        // The second card's staff sits below the first's.
        assert!(tree.systems.len() == 2);
        assert!(tree.systems[1].bounds.y > tree.systems[0].bounds.y);
        assert!(tree.meta.height > tree.systems[1].bounds.y);
    }

    // --- Pagination (T7.19a) ---

    fn page_cfg(content_width: f32) -> PageConfig {
        PageConfig {
            size: PageSize::Letter,
            content_width,
        }
    }

    fn paginate_banjo(n: u32, content_width: f32) -> PaginatedTree {
        paginate(&banjo_score(measures_of(n)), page_cfg(content_width))
    }

    fn folios(page: &Page) -> Vec<String> {
        page.header
            .iter()
            .filter_map(|p| match p {
                Primitive::Text {
                    role: TextRole::PageNumber,
                    content,
                    ..
                } => Some(content.clone()),
                _ => None,
            })
            .collect()
    }

    fn has_role(prims: &[Primitive], role: TextRole) -> bool {
        prims
            .iter()
            .any(|p| matches!(p, Primitive::Text { role: r, .. } if *r == role))
    }

    #[test]
    fn a_short_score_emits_a_single_page() {
        let doc = paginate(&banjo_score(measures_of(2)), page_cfg(80.0));
        assert_eq!(doc.pages.len(), 1);
        // The lone page carries the title block, never a folio.
        assert!(folios(&doc.pages[0]).is_empty());
    }

    #[test]
    fn an_empty_score_still_emits_one_page() {
        let doc = paginate(&banjo_score(vec![]), page_cfg(80.0));
        assert_eq!(doc.pages.len(), 1);
        assert!(doc.pages[0].systems.is_empty());
    }

    #[test]
    fn a_long_score_emits_multiple_pages() {
        let doc = paginate_banjo(400, 50.0);
        assert!(doc.pages.len() >= 3, "got {} pages", doc.pages.len());
    }

    #[test]
    fn pagination_preserves_every_system_in_order() {
        let score = banjo_score(measures_of(400));
        let flat = layout(&score, LayoutConfig { width: 50.0 });
        let doc = paginate(&score, page_cfg(50.0));
        let paged: usize = doc.pages.iter().map(|p| p.systems.len()).sum();
        assert_eq!(paged, flat.systems.len());
        // The flattened sequence of measure spans is identical to the single-page
        // render's: pagination only repartitions systems, never drops, dupes, or
        // reorders them.
        let flat_spans: Vec<Option<Span>> = flat
            .systems
            .iter()
            .flat_map(|s| s.measures.iter().map(|m| m.span))
            .collect();
        let paged_spans: Vec<Option<Span>> = doc
            .pages
            .iter()
            .flat_map(|p| p.systems.iter())
            .flat_map(|s| s.measures.iter().map(|m| m.span))
            .collect();
        assert_eq!(flat_spans, paged_spans);
    }

    #[test]
    fn pages_pack_systems_tightly() {
        // The page-break golden: every page but the last is full — the next page's
        // first system would not have fit after the current page's last one.
        let doc = paginate_banjo(400, 50.0);
        assert!(doc.pages.len() >= 2);
        let limit = doc.page_height - BOTTOM_MARGIN;
        for pair in doc.pages.windows(2) {
            let last = pair[0].systems.last().unwrap();
            let next = &pair[1].systems[0];
            // Re-place the next page's first system right after the prior page's
            // last (band height is 0 for these measures, so staff_top == cursor).
            let cursor = last.bounds.y + last.bounds.h + SYSTEM_GAP;
            let projected_bottom = cursor + next.bounds.h;
            assert!(
                projected_bottom + SYSTEM_GAP > limit,
                "page left room for another system"
            );
        }
    }

    #[test]
    fn every_system_sits_within_its_page() {
        let doc = paginate_banjo(400, 50.0);
        for page in &doc.pages {
            for sys in &page.systems {
                assert!(sys.bounds.y >= 0.0);
                assert!(sys.bounds.y + sys.bounds.h <= page.bounds.h);
            }
        }
    }

    #[test]
    fn page_one_carries_the_title_block_continuation_pages_a_folio() {
        let mut score = banjo_score(measures_of(400));
        score.meta.title = Some("Cripple Creek".to_string());
        let doc = paginate(&score, page_cfg(50.0));
        assert!(doc.pages.len() >= 2);

        // Page one: the title, no folio.
        assert!(has_role(&doc.pages[0].header, TextRole::Title));
        assert!(folios(&doc.pages[0]).is_empty());

        // Later pages: a folio numbered by position (1-based), no title block.
        for (i, page) in doc.pages.iter().enumerate().skip(1) {
            assert_eq!(folios(page), vec![(i + 1).to_string()]);
            assert!(!has_role(&page.header, TextRole::Title));
        }
    }

    #[test]
    fn a_continuation_page_starts_above_page_one() {
        // With no title block to clear, page two's first system sits higher than
        // page one's.
        let mut score = banjo_score(measures_of(400));
        score.meta.title = Some("T".to_string());
        let doc = paginate(&score, page_cfg(50.0));
        assert!(doc.pages.len() >= 2);
        assert!(doc.pages[1].systems[0].bounds.y < doc.pages[0].systems[0].bounds.y);
    }

    #[test]
    fn every_page_shares_one_letter_proportioned_box() {
        let doc = paginate_banjo(400, 50.0);
        let (w, h) = (doc.page_width, doc.page_height);
        assert!((h / w - 11.0 / 8.5).abs() < 1e-3);
        for page in &doc.pages {
            assert_eq!(
                page.bounds,
                Rect {
                    x: 0.0,
                    y: 0.0,
                    w,
                    h
                }
            );
        }
    }

    #[test]
    fn a4_is_taller_than_letter_at_the_same_width() {
        let measures = measures_of(4);
        let letter = paginate(&banjo_score(measures.clone()), page_cfg(80.0));
        let a4 = paginate(
            &banjo_score(measures),
            PageConfig {
                size: PageSize::A4,
                content_width: 80.0,
            },
        );
        assert_eq!(letter.page_width, a4.page_width);
        assert!(a4.page_height > letter.page_height);
    }
}
