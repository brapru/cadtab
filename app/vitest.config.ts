import { defineConfig } from "vitest/config";
import { svelte } from "@sveltejs/vite-plugin-svelte";

export default defineConfig({
  plugins: [svelte({ hot: false })],
  resolve: {
    conditions: ["browser"],
  },
  // Allow tests to `?raw`-import shipped fixtures from the repo's examples/ dir,
  // which sits one level above this app root.
  server: { fs: { allow: [".."] } },
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./vitest-setup.ts"],
    include: ["src/**/*.{test,spec}.ts"],
    coverage: {
      provider: "v8",
      reporter: ["text", "html"],
      include: ["src/**/*.{ts,svelte}"],
      // Untestable glue: the entrypoint, the wasm backend (needs real wasm),
      // the type-only contract mirror, and generated/declaration files.
      exclude: [
        "src/main.ts",
        "src/lib/wasm.ts",
        // Browser-only canvas rasterization; exercised in the app, not jsdom.
        "src/lib/png.ts",
        "src/lib/types.ts",
        "src/**/*.d.ts",
        "src/wasm-gen/**",
      ],
      thresholds: { lines: 90 },
    },
  },
});
