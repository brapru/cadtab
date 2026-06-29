import type {
  Completions,
  CompileResult,
  LayoutConfig,
  PageConfig,
  PaginatedTree,
  ProjectContext,
} from "./types";

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

// The pagination seam for PDF export (T7.19): lays a document out across fixed
// Letter/A4 pages via the same backend split as `compile`.
export function paginate(
  source: string,
  config: PageConfig,
  ctx?: ProjectContext,
): Promise<PaginatedTree> {
  if (isTauri()) {
    return import("@tauri-apps/api/core").then(({ invoke }) =>
      invoke<PaginatedTree>("paginate", {
        source,
        config,
        basePath: ctx?.basePath ?? null,
        files: ctx?.files ?? {},
      }),
    );
  }
  return import("./wasm").then(({ paginate }) =>
    paginate(source, config, ctx?.files ?? {}),
  );
}

// The completion-vocabulary seam (T7.24, D46): the core's keyword table +
// stdlib/`def` registry, surfaced through the same backend split as `compile`.
// Imports resolve through `ctx` exactly as a compile would, so imported
// `def`/`let` names complete too.
export function completions(
  source: string,
  ctx?: ProjectContext,
): Promise<Completions> {
  if (isTauri()) {
    return import("@tauri-apps/api/core").then(({ invoke }) =>
      invoke<Completions>("completions", {
        source,
        basePath: ctx?.basePath ?? null,
        files: ctx?.files ?? {},
      }),
    );
  }
  return import("./wasm").then(({ completions }) =>
    completions(source, ctx?.files ?? {}),
  );
}
