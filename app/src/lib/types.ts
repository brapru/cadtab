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
  | "rest";

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
