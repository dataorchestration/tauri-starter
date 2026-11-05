/// <reference types="vitest/config" />
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import process from "node:process";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

const host = process.env.TAURI_DEV_HOST;

// starter.config.json is the single source of feature toggles; the Rust side
// embeds the same file (src-tauri/src/config.rs). Exposed as a compile-time
// constant so the webview never has to ask the core for it.
const starterConfig = JSON.parse(
  readFileSync(resolve(process.cwd(), "starter.config.json"), "utf8"),
);

export default defineConfig(() => ({
  plugins: [react(), tailwindcss()],

  define: {
    __STARTER_CONFIG__: JSON.stringify(starterConfig),
  },

  // Vite options tailored for Tauri development (only applied in `tauri dev` /
  // `tauri build`).
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },

  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/test/setup.ts"],
    exclude: ["**/node_modules/**", "**/dist/**", "**/src-tauri/**", "**/scripts/**"],
  },
}));
