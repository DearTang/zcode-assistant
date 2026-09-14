import { resolve } from "node:path";
import vue from "@vitejs/plugin-vue";
import { defineConfig } from "vite";

// Tauri 期望前端 dev server 固定端口，构建产物输出到 dist/
export default defineConfig({
  plugins: [vue()],
  resolve: {
    alias: { "@": resolve(import.meta.dirname, "./src") },
  },
  // Tauri 内部通过 stdout 与 dev server 通信，不能清屏
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      // 忽略 Rust 端改动，避免触发 vite HMR
      ignored: ["**/src-tauri/**"],
    },
  },
  envPrefix: ["VITE_", "TAURI_ENV_*"],
  build: {
    target: "es2021",
    minify: "esbuild",
    sourcemap: false,
  },
});
