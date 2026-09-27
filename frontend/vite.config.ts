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
    // 明确绑定 IPv4：Linux 下 WebView 里的 `localhost` 常解析到 ::1，而 Vite 默认
    // 可能只监听 IPv4，导致 http://localhost:1420 报“连接被拒绝”。
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
