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

/// What a piece of text means, so the painter can style it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextRole {
    FretNumber,
    StringLabel,
}

/// A positioned drawing primitive. Span-bearing variants carry the source span
/// that produced them, enabling bidirectional source<->render mapping.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Primitive {
    Line {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        weight: f32,
    },
    Text {
        x: f32,
        y: f32,
        content: String,
        role: TextRole,
        span: Option<Span>,
    },
    Path {
        cmds: String,
        span: Option<Span>,
    },
}

/// One laid-out measure: its box plus the primitives drawn inside it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeasureBox {
    pub bounds: Rect,
    pub prims: Vec<Primitive>,
    pub span: Option<Span>,
}

/// One horizontal system (line of music) holding a run of measures.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct System {
    pub bounds: Rect,
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
