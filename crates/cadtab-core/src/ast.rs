//! Abstract syntax tree. Every node carries a source span; recursive node
//! categories use a `{ kind, span }` wrapper and each enum has an `Error`
//! variant so a resilient parse can emit a partial tree.

use serde::{Deserialize, Serialize};

use crate::span::Span;

// ---------------------------------------------------------------------------
// Leaf nodes
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ident {
    pub name: String,
    pub span: Span,
}

impl Ident {
    pub fn new(name: impl Into<String>, span: Span) -> Self {
        Self {
            name: name.into(),
            span,
        }
    }
}

/// An unsigned integer literal (string number, fret, denominator, count…).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntLit {
    pub value: u32,
    pub span: Span,
}

impl IntLit {
    pub fn new(value: u32, span: Span) -> Self {
        Self { value, span }
    }
}

/// A string literal. `value` is the inner text; escapes are not decoded here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StringLit {
    pub value: String,
    pub span: Span,
}

impl StringLit {
    pub fn new(value: impl Into<String>, span: Span) -> Self {
        Self {
            value: value.into(),
            span,
        }
    }
}

/// Right-hand execution mark suffix (`.t/.i/.m`, `.d/.u`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mark {
    pub kind: MarkKind,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MarkKind {
    Thumb,
    Index,
    Middle,
    StrumDown,
    StrumUp,
}

/// A `_N` duration suffix with `dots` trailing augmentation dots. Tuplet forms
/// are not represented yet (provisional).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Duration {
    pub denom: IntLit,
    pub dots: u8,
    pub span: Span,
}

/// A `string:fret` note position literal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Position {
    pub string: IntLit,
    pub fret: IntLit,
    pub span: Span,
}

/// A `num/den` time signature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimeSig {
    pub num: IntLit,
    pub den: IntLit,
    pub span: Span,
}

/// A `num/den` fraction, e.g. the sticky-default duration `1/8`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fraction {
    pub num: IntLit,
    pub den: IntLit,
    pub span: Span,
}

// ---------------------------------------------------------------------------
// Program & top-level items
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Program {
    pub items: Vec<Item>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub kind: ItemKind,
    pub span: Span,
}

impl Item {
    pub fn new(kind: ItemKind, span: Span) -> Self {
        Self { kind, span }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ItemKind {
    Title(StringLit),
    Composer(StringLit),
    Capo(StringLit),
    Import(StringLit),
    Tempo(IntLit),
    Instrument(Ident),
    Tuning(TuningRef),
    Def(Def),
    Let(Let),
    Score(Score),
    Error,
}

/// A `tuning` directive's argument: a named builtin (`tuning openG`) or an
/// inline per-string spec (`tuning { D4 B3 G3 D3 g4 }`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum TuningRef {
    Named(Ident),
    Custom(CustomTuning),
}

/// An inline `tuning { ... }` block: an optional display name and one pitch
/// token per string, string 1 first. Pitch text (e.g. `F#4`) is parsed and
/// validated in eval, not here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomTuning {
    pub name: Option<StringLit>,
    pub strings: Vec<Ident>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Def {
    pub name: Ident,
    pub params: Vec<Ident>,
    pub body: Block,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Let {
    pub name: Ident,
    pub value: Expr,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Score {
    pub items: Vec<ScoreItem>,
}

/// A braced sequence of events (`pickup`/`measure`/`loop`/`ending` bodies, and
/// `def` bodies).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Block {
    pub events: Vec<Event>,
    pub span: Span,
}

// ---------------------------------------------------------------------------
// Score-level items
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoreItem {
    pub kind: ScoreItemKind,
    pub span: Span,
}

impl ScoreItem {
    pub fn new(kind: ScoreItemKind, span: Span) -> Self {
        Self { kind, span }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ScoreItemKind {
    Time(TimeSig),
    Default(Fraction),
    Pickup(Block),
    Repeat(Repeat),
    Loop(LoopBlock),
    Measure(Block),
    /// A `section "A"` rehearsal mark attaching a label to the next measure.
    Section(StringLit),
    Event(Event),
    Error,
}

/// `repeat { body… ending(n){…}… }`: the body events precede the voltas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Repeat {
    pub body: Vec<Event>,
    pub endings: Vec<Ending>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ending {
    pub number: IntLit,
    pub body: Block,
    pub span: Span,
}

/// `loop N { body }` — unrolled at evaluation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoopBlock {
    pub count: IntLit,
    pub body: Block,
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

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
    Rest(Rest),
    /// A bare expression event (call/ident/spread) that splices a phrase.
    Phrase(Expr),
    Tie(Tie),
    /// A `chord "G"` marker: zero-duration, attaches its name to the next event.
    ChordSymbol(StringLit),
    Error,
}

/// A single note: a head expression (`3:2`, `chord.0`) plus optional right-hand
/// mark and duration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub head: Expr,
    pub mark: Option<Mark>,
    pub duration: Option<Duration>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Chord {
    pub notes: Vec<ChordNote>,
    pub duration: Option<Duration>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChordNote {
    pub position: Position,
    pub mark: Option<Mark>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rest {
    pub duration: Option<Duration>,
}

/// `a ~ b` tie. Left-associative chains nest in `left`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tie {
    pub left: Box<Event>,
    pub right: Box<Event>,
}

// ---------------------------------------------------------------------------
// Expressions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

impl Expr {
    pub fn new(kind: ExprKind, span: Span) -> Self {
        Self { kind, span }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExprKind {
    Int(u32),
    Str(String),
    Ident(String),
    Position(Position),
    Chord(Chord),
    /// `base.N` phrase index.
    Index {
        base: Box<Expr>,
        index: IntLit,
    },
    /// `callee(args)`.
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    /// `...expr` spread in argument position.
    Spread(Box<Expr>),
    /// `(expr)`.
    Paren(Box<Expr>),
    Error,
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

    fn s(a: u32, b: u32) -> Span {
        Span::new(a, b)
    }

    fn dur(denom: u32) -> Duration {
        Duration {
            denom: IntLit::new(denom, s(0, 1)),
            dots: 0,
            span: s(0, 1),
        }
    }

    /// Build a tree exercising most node kinds: metadata, a score with a meter,
    /// a marked+durated note, a pinch chord, a rest, a tie, and a phrase call.
    fn sample_program() -> Program {
        let note = Event::new(
            EventKind::Note(Note {
                head: Expr::new(
                    ExprKind::Position(Position {
                        string: IntLit::new(3, s(0, 1)),
                        fret: IntLit::new(0, s(2, 3)),
                        span: s(0, 3),
                    }),
                    s(0, 3),
                ),
                mark: Some(Mark {
                    kind: MarkKind::Thumb,
                    span: s(3, 5),
                }),
                duration: Some(dur(8)),
            }),
            s(0, 7),
        );

        let chord = Event::new(
            EventKind::Chord(Chord {
                notes: vec![ChordNote {
                    position: Position {
                        string: IntLit::new(1, s(0, 1)),
                        fret: IntLit::new(0, s(2, 3)),
                        span: s(0, 3),
                    },
                    mark: None,
                    span: s(0, 3),
                }],
                duration: Some(dur(4)),
            }),
            s(0, 9),
        );

        let rest = Event::new(
            EventKind::Rest(Rest {
                duration: Some(dur(8)),
            }),
            s(0, 3),
        );

        let tie = Event::new(
            EventKind::Tie(Tie {
                left: Box::new(rest.clone()),
                right: Box::new(rest.clone()),
            }),
            s(0, 9),
        );

        let call = Event::new(
            EventKind::Phrase(Expr::new(
                ExprKind::Call {
                    callee: Box::new(Expr::new(ExprKind::Ident("forward_roll".into()), s(0, 12))),
                    args: vec![Expr::new(ExprKind::Ident("g_chord".into()), s(13, 20))],
                },
                s(0, 21),
            )),
            s(0, 21),
        );

        let score = ScoreItem::new(ScoreItemKind::Event(note), s(0, 7));
        let meter = ScoreItem::new(
            ScoreItemKind::Time(TimeSig {
                num: IntLit::new(4, s(0, 1)),
                den: IntLit::new(4, s(2, 3)),
                span: s(0, 3),
            }),
            s(0, 3),
        );

        Program {
            items: vec![
                Item::new(ItemKind::Title(StringLit::new("X", s(0, 3))), s(0, 3)),
                Item::new(ItemKind::Instrument(Ident::new("banjo", s(0, 5))), s(0, 5)),
                Item::new(
                    ItemKind::Score(Score {
                        items: vec![
                            meter,
                            score,
                            ScoreItem::new(ScoreItemKind::Event(chord), s(0, 9)),
                            ScoreItem::new(ScoreItemKind::Event(rest), s(0, 3)),
                            ScoreItem::new(ScoreItemKind::Event(tie), s(0, 9)),
                            ScoreItem::new(ScoreItemKind::Event(call), s(0, 21)),
                        ],
                    }),
                    s(0, 40),
                ),
            ],
            span: s(0, 40),
        }
    }

    #[test]
    fn sample_program_round_trips() {
        round_trip(&sample_program());
    }

    #[test]
    fn leaf_constructors_set_fields() {
        assert_eq!(Ident::new("x", s(0, 1)).name, "x");
        assert_eq!(IntLit::new(7, s(0, 1)).value, 7);
        assert_eq!(StringLit::new("hi", s(0, 4)).value, "hi");
    }

    #[test]
    fn sample_program_snapshot() {
        insta::assert_debug_snapshot!(sample_program());
    }
}
