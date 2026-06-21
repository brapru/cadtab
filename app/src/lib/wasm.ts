import type { CompileResult, LayoutConfig } from "./types";
import init, { compile as wasmCompile } from "../wasm-gen/cadtab_wasm.js";

let ready: Promise<unknown> | null = null;

function ensureReady(): Promise<unknown> {
  if (!ready) {
    ready = init();
  }
  return ready;
}

export async function compile(
  source: string,
  config: LayoutConfig,
): Promise<CompileResult> {
  await ensureReady();
  return wasmCompile(source, config) as CompileResult;
}
