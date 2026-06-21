// Fallback declaration for the wasm-pack output so the type-check and tests do
// not require the generated package to be present. When it is generated, its
// own declarations take precedence.
declare module "*/wasm-gen/cadtab_wasm.js" {
  export default function init(input?: unknown): Promise<unknown>;
  export function compile(source: string, config: unknown): unknown;
  export function version(): string;
}
