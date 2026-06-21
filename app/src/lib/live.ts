import type { CompileResult, LayoutConfig } from "./types";

export type CompileFn = (
  source: string,
  config: LayoutConfig,
) => Promise<CompileResult>;

// Serializes compile requests with latest-wins semantics: each run is tagged
// with a sequence number, and a result is applied only if no newer run has
// started since. Guards against out-of-order async resolution.
export function createLiveCompiler(
  compileFn: CompileFn,
  onResult: (result: CompileResult) => void,
) {
  let seq = 0;

  async function run(source: string, config: LayoutConfig): Promise<boolean> {
    const mine = ++seq;
    const result = await compileFn(source, config);
    const isLatest = mine === seq;
    if (isLatest) {
      onResult(result);
    }
    return isLatest;
  }

  return { run };
}
