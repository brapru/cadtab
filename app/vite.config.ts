import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    // Allow `?raw`-importing starter templates from the repo's examples/ dir,
    // which sits one level above this app root.
    fs: { allow: [".."] },
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
});
