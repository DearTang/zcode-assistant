// 把 src-tauri/icons/source.svg 渲染为 1024x1024 PNG，写到仓库根 app-icon.png。
// 设计源是单一真相源；改图标只需编辑 SVG，然后跑 `npm run icon:gen`。
// SVG → PNG 走 @resvg/resvg-js（Rust 内核 WASM，无原生编译依赖）。
import { Resvg } from "@resvg/resvg-js";
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const SIZE = 1024;
const __dirname = dirname(fileURLToPath(import.meta.url));
const svgPath = resolve(__dirname, "..", "src-tauri", "icons", "source.svg");
const outPath = resolve(__dirname, "..", "app-icon.png");

const svg = readFileSync(svgPath, "utf-8");
const resvg = new Resvg(svg, {
  fitTo: { mode: "width", value: SIZE },
  background: "rgba(0, 0, 0, 0)",
});
const rendered = resvg.render();
const png = rendered.asPng();

if (png.length === 0) {
  throw new Error("resvg returned empty PNG buffer");
}

writeFileSync(outPath, png);
console.log(`icon written: ${outPath} (${png.length} bytes, ${SIZE}x${SIZE})`);