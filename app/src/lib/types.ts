// TypeScript mirror of the cadtab-core wire contract (serde camelCase).

export interface Span {
  start: number;
  end: number;
}

export type Severity = "error" | "warning" | "info";

export interface Diagnostic {
  severity: Severity;
  span: Span;
  message: string;
  help: string | null;
}

export type TokenClass =
  | "keyword"
  | "number"
  | "string"
  | "comment"
  | "ident"
  | "operator"
  | "punctuation";

export interface Token {
  class: TokenClass;
  span: Span;
}

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export type TextRole =
  | "fretNumber"
  | "stringLabel"
  | "timeSig"
  | "title"
  | "composer"
  | "tuningName"
  | "tuningString"
  | "tempo"
  | "capo"
  | "finger"
  | "strum"
  | "technique"
  | "ending"
  | "rest"
  | "sectionLabel"
  | "chordSymbol"
  | "barNumber"
  | "defHeading"
  | "defNote"
  | "pageNumber";

export type Primitive =
  | {
      kind: "line";
      x1: number;
      y1: number;
      x2: number;
      y2: number;
      weight: number;
    }
  | {
      kind: "text";
      x: number;
      y: number;
      content: string;
      role: TextRole;
      span: Span | null;
    }
  | { kind: "path"; cmds: string; span: Span | null };

export interface MeasureBox {
  bounds: Rect;
  prims: Primitive[];
  span: Span | null;
}

export interface System {
  bounds: Rect;
  prims: Primitive[];
  measures: MeasureBox[];
}

export interface LayoutMeta {
  width: number;
  height: number;
}

export interface RenderTree {
  meta: LayoutMeta;
  header: Primitive[];
  systems: System[];
}

export interface CompileResult {
  renderTree: RenderTree;
  diagnostics: Diagnostic[];
  tokens: Token[];
}

export interface LayoutConfig {
  width: number;
}

// A single paginated page (T7.19): its page box (logical units, origin top-left),
// the per-page header furniture (full title block on page one, a folio number
// after), and the systems placed within it. Each page is its own coordinate
// space starting at (0, 0).
export interface Page {
  bounds: Rect;
  header: Primitive[];
  systems: System[];
}

// A score laid out across fixed-size print pages, the input to PDF emission.
export interface PaginatedTree {
  pageWidth: number;
  pageHeight: number;
  pages: Page[];
}

export type PageSize = "letter" | "a4";

export interface PageConfig {
  size: PageSize;
  // Logical units across the printable content area (the justify target),
  // matching LayoutConfig.width.
  contentWidth: number;
}

// Completion vocabulary mirror (T7.24, D46). Sourced from the core's keyword
// table + stdlib/`def` registry, so the editor keeps no second copy.

// The operand shape an editor should offer after a keyword.
export type OperandKind = "none" | "string" | "number" | "values";

// One keyword's completion entry: its spelling, the operand it expects, and —
// for operand "values" — the closed set of values to offer (empty otherwise).
export interface KeywordInfo {
  name: string;
  operand: OperandKind;
  values: string[];
}

// The completion vocabulary for a document: the keyword table and the
// identifier registry (ambient stdlib licks + the document's own / imported
// top-level `def`/`let` names, sorted and deduplicated).
export interface Completions {
  keywords: KeywordInfo[];
  identifiers: string[];
}

// Per-compile project context: how `import`s resolve. Desktop uses `basePath`
// (the open document's path; imports resolve beside it on the real filesystem);
// web uses `files` (an in-memory path->contents map from the project bundle).
export interface ProjectContext {
  basePath?: string | null;
  files?: Record<string, string>;
}
