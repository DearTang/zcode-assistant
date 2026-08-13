// 验证端点B（bigmodel.cn 真实用量）用 config.json 的 provider apiKey
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const cfg = JSON.parse(
  fs.readFileSync(path.join(os.homedir(), ".zcode", "v2", "config.json"), "utf8")
);
// 找 enabled 的 provider 的 apiKey（优先 bigmodel-coding-plan）
const providers = cfg.provider || {};
const enabledKey = Object.keys(providers).find(
  (k) => providers[k].enabled && providers[k].options?.apiKey
);
const anyKey = Object.keys(providers).find((k) => providers[k].options?.apiKey);
const apiKey = (providers[enabledKey] || providers[anyKey] || {}).options?.apiKey || "";
console.log("using provider:", enabledKey || anyKey, "| apiKey len:", apiKey.length, "| starts ey:", apiKey.startsWith("ey"));

async function probe(name, url, headers) {
  try {
    const r = await fetch(url, { headers });
    const txt = await r.text();
    console.log(`\n[${name}] ${url}`);
    console.log("  status:", r.status, "len:", txt.length);
    try {
      const j = JSON.parse(txt);
      console.log("  code:", j.code, "success:", j.success, "msg:", j.msg);
      if (j.data) {
        console.log("  data keys:", Object.keys(j.data).join(", "));
        console.log("  data:", JSON.stringify(j.data).slice(0, 400));
      }
    } catch {}
    if (r.status >= 400 || txt.length < 120) console.log("  body:", txt.slice(0, 300));
  } catch (e) {
    console.log(`[${name}] ERROR:`, e.message);
  }
}

// 端点B：bigmodel.cn（Bearer + 裸值都试）
await probe("bm-bearer", "https://bigmodel.cn/api/monitor/usage/quota/limit", {
  authorization: `Bearer ${apiKey}`,
});
await probe("bm-bare", "https://bigmodel.cn/api/monitor/usage/quota/limit", {
  authorization: apiKey,
});
// z.ai 变体
await probe("zai-bearer", "https://api.z.ai/api/monitor/usage/quota/limit", {
  authorization: `Bearer ${apiKey}`,
});
// 订阅列表（同样 GET）
await probe("subscription", "https://bigmodel.cn/api/biz/subscription/list", {
  authorization: `Bearer ${apiKey}`,
});
// coding-plan personal overview（renderer 用）
await probe("cp-overview", "https://bigmodel.cn/coding-plan/personal/overview", {
  authorization: `Bearer ${apiKey}`,
});
