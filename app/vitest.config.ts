import { defineConfig } from "vitest/config";
import { svelte } from "@sveltejs/vite-plugin-svelte";

export default defineConfig({
  plugins: [svelte({ hot: false })],
  resolve: {
    conditions: ["browser"],
  },
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./vitest-setup.ts"],
    include: ["src/**/*.{test,spec}.ts"],
  },
});
