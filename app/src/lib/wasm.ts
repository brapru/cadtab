import type {
  CompileResult,
  LayoutConfig,
  PageConfig,
  PaginatedTree,
} from "./types";
import init, {
  compile as wasmCompile,
  paginate as wasmPaginate,
} from "../wasm-gen/cadtab_wasm.js";

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
  files: Record<string, string> = {},
): Promise<CompileResult> {
  await ensureReady();
  return wasmCompile(source, config, files) as CompileResult;
}

export async function paginate(
  source: string,
  config: PageConfig,
  files: Record<string, string> = {},
): Promise<PaginatedTree> {
  await ensureReady();
  return wasmPaginate(source, config, files) as PaginatedTree;
}
