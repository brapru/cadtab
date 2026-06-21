//! Source map: turn byte offsets / spans into human-facing `line:column`
//! locations for diagnostic rendering (tests, CLI). The live editor consumes
//! raw byte spans; this is the Rust-side index for pretty-printing.

use serde::{Deserialize, Serialize};

use crate::span::Span;

/// A 1-based line/column location. Columns count Unicode scalar values (chars)
/// from the line start, so multibyte source still reports sensible columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    pub line: u32,
    pub column: u32,
}

/// An owned copy of the source plus a precomputed index of line-start byte
/// offsets, enabling O(log n) offset → `Location` lookup.
#[derive(Debug, Clone)]
pub struct SourceMap {
    text: String,
    /// Byte offset of the start of each line; `line_starts[0]` is always `0`.
    line_starts: Vec<u32>,
}

impl SourceMap {
    pub fn new(source: &str) -> Self {
        let mut line_starts = vec![0u32];
        for (i, b) in source.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push((i + 1) as u32);
            }
        }
        Self {
            text: source.to_string(),
            line_starts,
        }
    }

    pub fn source(&self) -> &str {
        &self.text
    }

    pub fn len(&self) -> u32 {
        self.text.len() as u32
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Number of lines (always >= 1).
    pub fn line_count(&self) -> u32 {
        self.line_starts.len() as u32
    }

    /// 0-based index of the line containing `offset` (clamped to the source).
    fn line_index(&self, offset: u32) -> usize {
        match self.line_starts.binary_search(&offset) {
            Ok(i) => i,
            // line_starts[0] == 0 <= offset, so `i` is always >= 1 here.
            Err(i) => i - 1,
        }
    }

    /// Map a byte `offset` to its 1-based `line:column`. Offsets past the end
    /// clamp to the end; offsets landing mid-char count that char as not-yet-
    /// reached (no panic on non-boundary input).
    pub fn location(&self, offset: u32) -> Location {
        let offset = offset.min(self.len());
        let li = self.line_index(offset);
        let line_start = self.line_starts[li];

        // line_start is always a char boundary (file start or just after '\n').
        let mut column = 1u32;
        for (i, _ch) in self.text[line_start as usize..].char_indices() {
            if line_start + i as u32 >= offset {
                break;
            }
            column += 1;
        }

        Location {
            line: li as u32 + 1,
            column,
        }
    }

    /// The `(start, end)` locations bracketing a span.
    pub fn span_location(&self, span: Span) -> (Location, Location) {
        (self.location(span.start), self.location(span.end))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn empty_source_is_one_line() {
        let sm = SourceMap::new("");
        assert!(sm.is_empty());
        assert_eq!(sm.line_count(), 1);
        assert_eq!(sm.location(0), Location { line: 1, column: 1 });
    }

    #[test]
    fn offsets_map_to_expected_line_col() {
        //              0123 4567
        let sm = SourceMap::new("ab\ncd\n");
        assert_eq!(sm.location(0), Location { line: 1, column: 1 }); // 'a'
        assert_eq!(sm.location(1), Location { line: 1, column: 2 }); // 'b'
        assert_eq!(sm.location(2), Location { line: 1, column: 3 }); // '\n'
        assert_eq!(sm.location(3), Location { line: 2, column: 1 }); // 'c'
        assert_eq!(sm.location(5), Location { line: 2, column: 3 }); // '\n'
        assert_eq!(sm.location(6), Location { line: 3, column: 1 }); // EOF (trailing line)
        assert_eq!(sm.line_count(), 3);
    }

    #[test]
    fn multibyte_columns_count_chars_not_bytes() {
        // 'é' is 2 bytes (U+00E9); 'x' follows at byte offset 2.
        let sm = SourceMap::new("éx");
        assert_eq!(sm.location(0), Location { line: 1, column: 1 });
        assert_eq!(sm.location(2), Location { line: 1, column: 2 }); // 'x'
    }

    #[test]
    fn offset_past_end_clamps() {
        let sm = SourceMap::new("ab");
        assert_eq!(sm.location(999), Location { line: 1, column: 3 });
    }

    #[test]
    fn span_location_brackets_a_range() {
        // "abc\ndef": offset 1 = 'b' (1:2), offset 5 = 'e' (2:2).
        let sm = SourceMap::new("abc\ndef");
        let (lo, hi) = sm.span_location(Span::new(1, 5));
        assert_eq!(lo, Location { line: 1, column: 2 });
        assert_eq!(hi, Location { line: 2, column: 2 });
    }

    proptest! {
        // For any ASCII source and offset, location is well-formed and lookup
        // never panics (incl. arbitrary, possibly out-of-range offsets).
        #[test]
        fn location_is_well_formed(s in "[a-z\n]{0,80}", offset in 0u32..120) {
            let sm = SourceMap::new(&s);
            let loc = sm.location(offset);
            prop_assert!(loc.line >= 1);
            prop_assert!(loc.column >= 1);
            prop_assert!(loc.line <= sm.line_count());
        }

        // Offsets are monotonic in (line, column): later offset never maps
        // before an earlier one.
        #[test]
        fn location_is_monotonic(s in "[a-z\n]{0,80}", a in 0u32..80, b in 0u32..80) {
            let sm = SourceMap::new(&s);
            let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
            let la = sm.location(lo);
            let lb = sm.location(hi);
            prop_assert!((la.line, la.column) <= (lb.line, lb.column));
        }
    }
}
