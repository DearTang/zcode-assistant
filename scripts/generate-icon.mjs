// 生成 1024x1024 应用图标 PNG（青绿圆角方块 + 白色上升折线），无外部依赖
// Node 内置 zlib 完成 PNG 编码。
import { deflateSync, crc32 } from "node:zlib";
import { writeFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const W = 1024,
  H = 1024;
const R = 230; // 圆角半径
const __dirname = dirname(fileURLToPath(import.meta.url));
const out = resolve(__dirname, "..", "app-icon.png");

const buf = Buffer.alloc(W * H * 4);
const lerp = (a, b, t) => a + (b - a) * t;

// 圆角矩形的带符号距离（<0 在内）
function cornerSdf(x, y) {
  const w = W,
    h = H,
    r = R;
  // 最近角中心
  const cx = x < r ? r : x > w - r ? w - r : x;
  const cy = y < r ? r : y > h - r ? h - r : y;
  if ((x >= r && x <= w - r) || (y >= r && y <= h - r)) return -r; // 在直边区域，视为深处
  return Math.hypot(x - cx, y - cy) - r;
}

// 折线段（归一化 → 像素）
const segs = [
  [0.18, 0.64, 0.4, 0.45],
  [0.4, 0.45, 0.55, 0.57],
  [0.55, 0.57, 0.82, 0.3],
].map((s) => s.map((v) => v * 1024));
const lineHalf = 26;

function distToSeg(px, py, ax, ay, bx, by) {
  const dx = bx - ax,
    dy = by - ay;
  const len2 = dx * dx + dy * dy;
  let t = len2 > 0 ? ((px - ax) * dx + (py - ay) * dy) / len2 : 0;
  t = Math.max(0, Math.min(1, t));
  return Math.hypot(px - (ax + t * dx), py - (ay + t * dy));
}

for (let y = 0; y < H; y++) {
  for (let x = 0; x < W; x++) {
    const idx = (y * W + x) * 4;
    const sdf = cornerSdf(x, y);
    // 圆角边缘抗锯齿（2px 过渡）
    let alpha = 255;
    if (sdf > 1.5) {
      buf[idx + 3] = 0;
      continue;
    } else if (sdf > -0.5) {
      alpha = Math.round(255 * Math.max(0, 1 - (sdf + 0.5) / 2));
    }

    const t = (x + y) / (W + H);
    let r = Math.round(lerp(45, 20, t));
    let g = Math.round(lerp(212, 150, t));
    let b = Math.round(lerp(191, 170, t));

    let onLine = false;
    for (const s of segs) {
      if (distToSeg(x, y, s[0], s[1], s[2], s[3]) < lineHalf) {
        onLine = true;
        break;
      }
    }
    if (onLine) {
      r = g = b = 255;
    }
    buf[idx] = r;
    buf[idx + 1] = g;
    buf[idx + 2] = b;
    buf[idx + 3] = alpha;
  }
}

function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length, 0);
  const typeBuf = Buffer.from(type, "ascii");
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(Buffer.concat([typeBuf, data])) >>> 0, 0);
  return Buffer.concat([len, typeBuf, data, crc]);
}

const sig = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);
const ihdr = Buffer.alloc(13);
ihdr.writeUInt32BE(W, 0);
ihdr.writeUInt32BE(H, 4);
ihdr[8] = 8; // bit depth
ihdr[9] = 6; // color type RGBA
ihdr[10] = 0;
ihdr[11] = 0;
ihdr[12] = 0;

const stride = W * 4 + 1;
const raw = Buffer.alloc(stride * H);
for (let y = 0; y < H; y++) {
  raw[y * stride] = 0; // filter: none
  buf.copy(raw, y * stride + 1, y * W * 4, (y + 1) * W * 4);
}
const idat = deflateSync(raw, { level: 9 });

const png = Buffer.concat([
  sig,
  chunk("IHDR", ihdr),
  chunk("IDAT", idat),
  chunk("IEND", Buffer.alloc(0)),
]);
writeFileSync(out, png);
console.log("icon written:", out, png.length, "bytes");
