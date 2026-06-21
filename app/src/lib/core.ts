import type { CompileResult, LayoutConfig } from "./types";

export type Backend = (
  source: string,
  config: LayoutConfig,
) => Promise<CompileResult>;

// True when running inside the Tauri webview (desktop), false in a plain browser.
export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

async function tauriBackend(
  source: string,
  config: LayoutConfig,
): Promise<CompileResult> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<CompileResult>("compile", { source, config });
}

async function wasmBackend(
  source: string,
  config: LayoutConfig,
): Promise<CompileResult> {
  const { compile } = await import("./wasm");
  return compile(source, config);
}

export function selectBackend(): Backend {
  return isTauri() ? tauriBackend : wasmBackend;
}

// Single backend-agnostic seam between the UI and the Rust core: the Tauri
// command on desktop, the wasm module in the browser.
export function compile(
  source: string,
  config: LayoutConfig,
): Promise<CompileResult> {
  return selectBackend()(source, config);
}
