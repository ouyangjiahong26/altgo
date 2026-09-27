import { resolve } from "path";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
/// <reference types="vitest/config" />

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    // Bind IPv4 explicitly: on Linux, `localhost` in the WebView often resolves to ::1 while
    // Vite's default can be IPv4-only, causing "connection refused" for http://localhost:1420.
    host: host || "127.0.0.1",
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
  build: {
    rollupOptions: {
      input: {
        main: resolve(__dirname, "index.html"),
        overlay: resolve(__dirname, "overlay.html"),
        "update-notes": resolve(__dirname, "update-notes.html"),
      },
    },
  },
  test: {
    globals: true,
    environment: "jsdom",
    setupFiles: "./src/test-setup.ts",
    css: true,
  },
});
