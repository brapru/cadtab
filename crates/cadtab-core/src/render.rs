//! The render tree: the positioned output of layout that the frontend paints
//! verbatim. Lightly hierarchical (`System -> MeasureBox -> Primitive`) in
//! logical coordinates (1 unit = string spacing), scaled by the painter via the
//! SVG `viewBox`. Everything serializes to JSON across the IPC/WASM boundary;
//! span-bearing nodes carry the source span that produced them for bidirectional
//! source<->render mapping.

use serde::{Deserialize, Serialize};

use crate::span::Span;

/// An axis-aligned box in logical coordinates (1 unit = string spacing).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// What a piece of text means, so the painter can style it by role (font size,
/// weight, anchoring). Geometry alone is in the coordinates; intent is here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextRole {
    /// A fretted-position number on a string line.
    FretNumber,
    /// An open-string tuning letter at the left of a system.
    StringLabel,
    /// A stacked time-signature digit (numerator or denominator) at a measure's
    /// left.
    TimeSig,
    /// The song title in the header.
    Title,
    /// The composer credit in the header.
    Composer,
    /// The tuning's display name in the header (e.g. "Open G").
    TuningName,
    /// A header tuning-grid cell: a circled string number and its note (`①=D`).
    TuningString,
    /// The tempo marking in the header (`♩ = N`).
    Tempo,
    /// The capo label in the header.
    Capo,
    /// A right-hand finger mark (T/I/M).
    Finger,
    /// A strum-direction glyph.
    Strum,
    /// A left-hand technique mark (h/p/sl).
    Technique,
    /// A volta (repeat-ending) bracket number.
    Ending,
    /// A rest glyph.
    Rest,
    /// A section label (rehearsal mark) above the staff (e.g. the "A" part).
    SectionLabel,
    /// A chord symbol above the staff at a beat (e.g. "G", "D7").
    ChordSymbol,
    /// A small measure number above the staff.
    BarNumber,
}

/// A positioned drawing primitive. Span-bearing variants carry the source span
/// that produced them, enabling bidirectional source<->render mapping.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Primitive {
    /// A straight stroke: string lines, barlines, stems, beams.
    Line {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        weight: f32,
    },
    /// Glyph text: fret numbers, finger/strum/technique marks, header labels.
    Text {
        x: f32,
        y: f32,
        content: String,
        role: TextRole,
        span: Option<Span>,
    },
    /// A free-form path (SVG path data): ties, slides, bends, choke arcs. The
    /// thin painter draws `cmds` verbatim, so style is baked into the geometry.
    Path { cmds: String, span: Option<Span> },
}

/// One laid-out measure: its box plus the primitives drawn inside it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeasureBox {
    pub bounds: Rect,
    pub prims: Vec<Primitive>,
    pub span: Option<Span>,
}

/// One horizontal system (line of music): its box, the measures it holds, and
/// the system-spanning furniture (`prims`) drawn behind them — the continuous
/// string lines and the leading string labels that run the system's full width.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct System {
    pub bounds: Rect,
    pub prims: Vec<Primitive>,
    pub measures: Vec<MeasureBox>,
}

/// Overall dimensions of the laid-out tree, used to set the SVG viewBox.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutMeta {
    pub width: f32,
    pub height: f32,
}

/// The complete, positioned output of layout; the frontend paints it verbatim.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderTree {
    pub meta: LayoutMeta,
    pub header: Vec<Primitive>,
    pub systems: Vec<System>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::de::DeserializeOwned;
    use std::fmt::Debug;

    fn round_trip<T: Serialize + DeserializeOwned + PartialEq + Debug>(value: &T) {
        let json = serde_json::to_string(value).unwrap();
        let back: T = serde_json::from_str(&json).unwrap();
        assert_eq!(value, &back);
    }

    const ALL_ROLES: &[TextRole] = &[
        TextRole::FretNumber,
        TextRole::StringLabel,
        TextRole::TimeSig,
        TextRole::Title,
        TextRole::Composer,
        TextRole::TuningName,
        TextRole::TuningString,
        TextRole::Tempo,
        TextRole::Capo,
        TextRole::Finger,
        TextRole::Strum,
        TextRole::Technique,
        TextRole::Ending,
        TextRole::Rest,
        TextRole::SectionLabel,
        TextRole::ChordSymbol,
        TextRole::BarNumber,
    ];

    fn rect() -> Rect {
        Rect {
            x: 0.0,
            y: 1.5,
            w: 12.0,
            h: 4.0,
        }
    }

    fn text(role: TextRole) -> Primitive {
        Primitive::Text {
            x: 1.0,
            y: 2.0,
            content: "0".to_string(),
            role,
            span: Some(Span::new(0, 3)),
        }
    }

    #[test]
    fn every_text_role_round_trips() {
        for &role in ALL_ROLES {
            round_trip(&text(role));
        }
    }

    #[test]
    fn each_primitive_variant_round_trips() {
        round_trip(&Primitive::Line {
            x1: 0.0,
            y1: 2.0,
            x2: 12.0,
            y2: 2.0,
            weight: 0.1,
        });
        round_trip(&text(TextRole::FretNumber));
        round_trip(&Primitive::Path {
            cmds: "M0 0 Q1 1 2 0".to_string(),
            span: None,
        });
    }

    #[test]
    fn render_tree_round_trips() {
        let measure = MeasureBox {
            bounds: rect(),
            prims: vec![
                Primitive::Line {
                    x1: 0.0,
                    y1: 2.0,
                    x2: 12.0,
                    y2: 2.0,
                    weight: 0.1,
                },
                text(TextRole::FretNumber),
            ],
            span: Some(Span::new(0, 3)),
        };
        let system = System {
            bounds: rect(),
            prims: vec![text(TextRole::StringLabel)],
            measures: vec![measure],
        };
        let tree = RenderTree {
            meta: LayoutMeta {
                width: 12.0,
                height: 4.0,
            },
            header: vec![text(TextRole::Title), text(TextRole::Tempo)],
            systems: vec![system],
        };
        round_trip(&tree);
    }

    #[test]
    fn text_role_serializes_as_camel_case() {
        let json = serde_json::to_string(&TextRole::FretNumber).unwrap();
        assert_eq!(json, "\"fretNumber\"");
        let json = serde_json::to_string(&TextRole::StringLabel).unwrap();
        assert_eq!(json, "\"stringLabel\"");
    }

    #[test]
    fn primitive_tags_its_kind() {
        let json = serde_json::to_string(&text(TextRole::FretNumber)).unwrap();
        assert!(json.contains("\"kind\":\"text\""));
    }
}
