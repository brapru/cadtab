import type {
  Completions,
  CompileResult,
  LayoutConfig,
  PageConfig,
  PaginatedTree,
} from "./types";
import init, {
  compile as wasmCompile,
  completions as wasmCompletions,
  format as wasmFormat,
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

export async function completions(
  source: string,
  files: Record<string, string> = {},
): Promise<Completions> {
  await ensureReady();
  return wasmCompletions(source, files) as Completions;
}

export async function format(source: string): Promise<string> {
  await ensureReady();
  return wasmFormat(source) as string;
}
