import type { CompileResult, LayoutConfig, ProjectContext } from "./types";

export type CompileFn = (
  source: string,
  config: LayoutConfig,
  ctx?: ProjectContext,
) => Promise<CompileResult>;

// Serializes compile requests with latest-wins semantics: each run is tagged
// with a sequence number, and an outcome is applied only if no newer run has
// started since. Guards against out-of-order async resolution — including
// errors, so a stale rejection (e.g. a missing backend) never clobbers a fresh
// render. `onError` receives whatever the backend threw.
export function createLiveCompiler(
  compileFn: CompileFn,
  onResult: (result: CompileResult) => void,
  onError?: (error: unknown) => void,
) {
  let seq = 0;

  async function run(
    source: string,
    config: LayoutConfig,
    ctx?: ProjectContext,
  ): Promise<boolean> {
    const mine = ++seq;
    try {
      const result = await compileFn(source, config, ctx);
      if (mine !== seq) {
        return false;
      }
      onResult(result);
    } catch (error) {
      if (mine !== seq) {
        return false;
      }
      onError?.(error);
    }
    return true;
  }

  return { run };
}
