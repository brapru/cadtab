import type { CompileResult, LayoutConfig, ProjectContext } from "./types";

export type Backend = (
  source: string,
  config: LayoutConfig,
  ctx?: ProjectContext,
) => Promise<CompileResult>;

// True when running inside the Tauri webview (desktop), false in a plain browser.
export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

async function tauriBackend(
  source: string,
  config: LayoutConfig,
  ctx?: ProjectContext,
): Promise<CompileResult> {
  const { invoke } = await import("@tauri-apps/api/core");
  // Desktop resolves imports from the bundle map first, then the filesystem
  // (relative to the open document), so an opened bundle works on desktop too.
  return invoke<CompileResult>("compile", {
    source,
    config,
    basePath: ctx?.basePath ?? null,
    files: ctx?.files ?? {},
  });
}

async function wasmBackend(
  source: string,
  config: LayoutConfig,
  ctx?: ProjectContext,
): Promise<CompileResult> {
  const { compile } = await import("./wasm");
  // Web resolves imports from the in-memory bundle map.
  return compile(source, config, ctx?.files ?? {});
}

export function selectBackend(): Backend {
  return isTauri() ? tauriBackend : wasmBackend;
}

// Single backend-agnostic seam between the UI and the Rust core: the Tauri
// command on desktop, the wasm module in the browser.
export function compile(
  source: string,
  config: LayoutConfig,
  ctx?: ProjectContext,
): Promise<CompileResult> {
  return selectBackend()(source, config, ctx);
}
