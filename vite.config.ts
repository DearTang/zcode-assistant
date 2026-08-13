import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri 期望前端 dev server 固定端口，构建产物输出到 dist/
export default defineConfig({
  plugins: [react()],
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
