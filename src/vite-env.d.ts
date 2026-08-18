/// <reference types="vite/client" />

// Vite 的 `?raw` 后缀在构建期把文件内容作为字符串打包，
// 用于把 CHANGELOG.md 内置进前端（关于弹窗离线展示更新日志）。
declare module "*.md?raw" {
  const content: string;
  export default content;
}
