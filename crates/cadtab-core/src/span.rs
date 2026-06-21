use serde::{Deserialize, Serialize};

/// A half-open byte range `[start, end)` into the source text.
///
/// Spans are mandatory on every node across the whole pipeline. Well-formed
/// spans have `start <= end`; the arithmetic below treats `start >= end` as
/// empty.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub const fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }

    /// A zero-width span at `offset` (e.g. an insertion point for a diagnostic).
    pub const fn point(offset: u32) -> Self {
        Self {
            start: offset,
            end: offset,
        }
    }

    /// Byte length; saturates to `0` for a malformed (`end < start`) span.
    pub fn len(&self) -> u32 {
        self.end.saturating_sub(self.start)
    }

    /// True when the span covers no bytes (`start >= end`).
    pub fn is_empty(&self) -> bool {
        self.start >= self.end
    }

    /// True when `offset` lies inside the half-open range `[start, end)`.
    pub fn contains(&self, offset: u32) -> bool {
        self.start <= offset && offset < self.end
    }

    /// True when `other` is fully enclosed by `self`.
    pub fn contains_span(&self, other: Span) -> bool {
        self.start <= other.start && other.end <= self.end
    }

    /// The smallest span covering both `self` and `other`.
    pub fn merge(&self, other: Span) -> Span {
        Span::new(self.start.min(other.start), self.end.max(other.end))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn point_is_empty_and_zero_len() {
        let p = Span::point(7);
        assert!(p.is_empty());
        assert_eq!(p.len(), 0);
        assert_eq!(p, Span::new(7, 7));
    }

    #[test]
    fn malformed_span_len_saturates() {
        assert_eq!(Span::new(9, 4).len(), 0);
        assert!(Span::new(9, 4).is_empty());
    }

    // A strategy yielding well-formed spans (`start <= end`) plus a probe offset.
    fn span_and_offset() -> impl Strategy<Value = (Span, u32)> {
        (0u32..200, 0u32..200, 0u32..200).prop_map(|(a, b, o)| {
            let (start, end) = if a <= b { (a, b) } else { (b, a) };
            (Span::new(start, end), o)
        })
    }

    proptest! {
        #[test]
        fn len_equals_end_minus_start((span, _o) in span_and_offset()) {
            prop_assert_eq!(span.len(), span.end - span.start);
        }

        #[test]
        fn contains_matches_range((span, o) in span_and_offset()) {
            prop_assert_eq!(span.contains(o), span.start <= o && o < span.end);
        }

        #[test]
        fn merge_is_commutative(
            (a, _o1) in span_and_offset(),
            (b, _o2) in span_and_offset(),
        ) {
            prop_assert_eq!(a.merge(b), b.merge(a));
        }

        #[test]
        fn merge_covers_both_operands(
            (a, _o1) in span_and_offset(),
            (b, _o2) in span_and_offset(),
        ) {
            let m = a.merge(b);
            prop_assert!(m.contains_span(a));
            prop_assert!(m.contains_span(b));
        }

        #[test]
        fn merge_is_associative(
            (a, _o1) in span_and_offset(),
            (b, _o2) in span_and_offset(),
            (c, _o3) in span_and_offset(),
        ) {
            prop_assert_eq!(a.merge(b).merge(c), a.merge(b.merge(c)));
        }

        #[test]
        fn merge_is_idempotent((a, _o) in span_and_offset()) {
            prop_assert_eq!(a.merge(a), a);
        }
    }
}
