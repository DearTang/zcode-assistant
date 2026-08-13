// 打印 quota/limit 响应的完整 limits 结构，确认 nextResetTime 字段名
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const cfg = JSON.parse(
  fs.readFileSync(path.join(os.homedir(), ".zcode", "v2", "config.json"), "utf8")
);
const providers = cfg.provider || {};
const enabledKey = Object.keys(providers).find(
  (k) => providers[k].enabled && providers[k].options?.apiKey
);
const apiKey = providers[enabledKey]?.options?.apiKey || "";

const r = await fetch("https://bigmodel.cn/api/monitor/usage/quota/limit", {
  headers: { authorization: `Bearer ${apiKey}` },
});
const j = await r.json();
console.log("code:", j.code, "level:", j.data?.level);
console.log("limits count:", j.data?.limits?.length);
for (const [i, l] of (j.data?.limits || []).entries()) {
  console.log(`\n--- limit[${i}] ---`);
  console.log(JSON.stringify(l, null, 2));
}
