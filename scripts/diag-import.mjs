// 读取各工具配置文件，输出 provider 提取结构（apiKey 脱敏）
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const HOME = os.homedir();
const candidates = [
  { tool: "Claude Code", file: path.join(HOME, ".claude.json") },
  { tool: "Claude Code", file: path.join(HOME, ".claude", "settings.json") },
  { tool: "Codex", file: path.join(HOME, ".codex", "config.toml") },
  { tool: "Codex", file: path.join(HOME, ".codex", "config.json") },
  { tool: "Qwen Code", file: path.join(HOME, ".qwen", "config.json") },
  { tool: "Qwen Code", file: path.join(HOME, ".qwen", "config.toml") },
  { tool: "Qwen Code", file: path.join(HOME, ".qwen", "code", "config.json") },
  { tool: "Qwen Code", file: path.join(HOME, ".qwen-code", "config.json") },
  { tool: "opencode", file: path.join(HOME, ".config", "opencode", "opencode.json") },
  { tool: "opencode", file: path.join(HOME, "AppData", "Roaming", "opencode", "opencode.json") },
  { tool: "ccswitch", file: path.join(HOME, ".ccswitch", "config.json") },
  { tool: "ccswitch", file: path.join(HOME, ".config", "ccswitch", "config.json") },
];

function mask(v) {
  if (typeof v === "string" && v.length > 0) return "<REDACTED len=" + v.length + ">";
  if (Array.isArray(v)) return v.slice(0, 3).map(mask).concat(v.length > 3 ? ["...+" + (v.length-3) + " more"] : []);
  if (v && typeof v === "object") {
    const out = {};
    for (const [k, val] of Object.entries(v)) {
      const kl = k.toLowerCase();
      if (kl.includes("key") || kl.includes("token") || kl.includes("secret") || kl.includes("password")) {
        out[k] = mask(val);
      } else {
        out[k] = val;
      }
    }
    return out;
  }
  return v;
}

for (const c of candidates) {
  console.log(`\n===== ${c.tool}: ${c.file} =====`);
  if (!fs.existsSync(c.file)) {
    console.log("  (不存在)");
    continue;
  }
  const st = fs.statSync(c.file);
  console.log("  size:", st.size, "bytes");
  try {
    const txt = fs.readFileSync(c.file, "utf8");
    if (c.file.endsWith(".toml")) {
      // TOML 原始输出前 60 行（含敏感值脱敏）
      const lines = txt.split("\n").slice(0, 80);
      console.log("  --- TOML 前 80 行（敏感值已脱敏）---");
      for (const ln of lines) {
        console.log("   ", ln.replace(/(key|token|secret|password)\s*=\s*"([^"]*)"/gi, "$1 = \"<REDACTED>\""));
      }
    } else {
      const j = JSON.parse(txt);
      console.log("  --- 脱敏结构 ---");
      console.log(JSON.stringify(mask(j), null, 2).slice(0, 2000));
    }
  } catch (e) {
    console.log("  解析失败:", e.message);
  }
}