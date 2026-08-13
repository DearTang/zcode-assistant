#!/usr/bin/env node
/**
 * 发布 Gitee release 并上传打包好的 NSIS 安装器（参考 myshell）。
 *
 * Usage:
 *   node scripts/publish-gitee-release.mjs <version> <notes-file> [asset-path]
 *
 *   version     不带前缀 "v" 的版本号，如 "0.2.0"（tag 自动变 v0.2.0）
 *   notes-file  Markdown 文件，内容作为发行说明
 *   asset-path  可选；省略时自动取 src-tauri/target/release/bundle/nsis 下
 *               最新构建的 *-setup.exe
 *
 * Token 解析（按优先级取第一个非空）：
 *   1. $GITEE_TOKEN 环境变量
 *   2. 仓库根的 .gitee-token 文件（请加入 .gitignore）
 *
 * 需 Node >= 18（使用全局 fetch / FormData / Blob）。
 *
 * API 参考：
 *   - 创建 release:  POST /v5/repos/{owner}/{repo}/releases
 *   - 上传附件:      POST /v5/repos/{owner}/{repo}/releases/{id}/attach_files
 *     （multipart/form-data —— Gitee 的附件机制与 GitHub 的 upload_url 不同）
 */
import { readFile, readdir, stat } from "node:fs/promises";
import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join, basename } from "node:path";
import { setTimeout as sleep } from "node:timers/promises";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const OWNER = "argustang";
const REPO = "zcode-assistant";
const API = `https://gitee.com/api/v5/repos/${OWNER}/${REPO}`;

/**
 * Gitee API 偶发 ConnectTimeoutError / socket hangup，对每个网络调用加小重试，
 * 避免抖动链路拖垮一次本可完成的发布。仅在 fetch 抛 TypeError（网络类）时重试；
 * 4xx/5xx HTTP 响应原样返回给调用方（那是真实失败）。
 */
const MAX_ATTEMPTS = 4;
async function fetchWithRetry(url, init, label) {
  let lastErr;
  for (let attempt = 1; attempt <= MAX_ATTEMPTS; attempt++) {
    try {
      const res = await fetch(url, init);
      return res; // 成功或 HTTP 错误——交给调用方判断 res.ok
    } catch (err) {
      lastErr = err;
      if (attempt < MAX_ATTEMPTS) {
        const waitMs = 2000 * attempt; // 2s, 4s, 6s
        console.error(
          `  [${label}] 网络错误(${attempt}/${MAX_ATTEMPTS})，${waitMs}ms 后重试: ${err.message || err}`
        );
        await sleep(waitMs);
      }
    }
  }
  throw lastErr;
}

const [rawVersion, notesFile, assetOverride] = process.argv.slice(2);

if (!rawVersion || !notesFile) {
  console.error(
    "usage: node scripts/publish-gitee-release.mjs <version> <notes-file> [asset-path]"
  );
  process.exit(1);
}

// 去掉可能的前缀 "v"；tag 始终带 v。
const version = rawVersion.replace(/^v/i, "").trim();
const TAG = `v${version}`;

// ── 1. 解析 token ──────────────────────────────────────────────
function resolveToken() {
  if (process.env.GITEE_TOKEN) return process.env.GITEE_TOKEN.trim();
  const tokFile = join(ROOT, ".gitee-token");
  if (existsSync(tokFile)) return readFileSync(tokFile, "utf8").trim();
  return "";
}

// ── 2. 解析安装包 asset ────────────────────────────────────────
async function resolveAsset() {
  if (assetOverride) {
    if (!existsSync(assetOverride)) {
      throw new Error(`指定的资产路径不存在: ${assetOverride}`);
    }
    return assetOverride;
  }
  const nsisDir = join(ROOT, "src-tauri/target/release/bundle/nsis");
  if (!existsSync(nsisDir)) {
    throw new Error(
      `未找到安装包目录 ${nsisDir}。请先运行打包 (npm run tauri:build)。`
    );
  }
  const entries = await readdir(nsisDir);
  const exes = entries.filter((f) => f.endsWith("-setup.exe"));
  if (exes.length === 0) {
    throw new Error(`目录下没有 *-setup.exe: ${nsisDir}`);
  }
  // 取最近构建的安装器（与刚打的版本对应）。
  let newest = null;
  let newestMtime = 0;
  for (const f of exes) {
    const full = join(nsisDir, f);
    const mtime = (await stat(full)).mtimeMs;
    if (mtime > newestMtime) {
      newestMtime = mtime;
      newest = full;
    }
  }
  return newest;
}

async function main() {
  const token = resolveToken();
  if (!token) {
    console.error(
      "未找到 Gitee token。设置 $GITEE_TOKEN 环境变量，或在仓库根创建 .gitee-token 文件（内含私人令牌）。"
    );
    process.exit(1);
  }

  const notes = await readFile(notesFile, "utf8");
  const asset = await resolveAsset().catch((e) => {
    console.error(e.message);
    process.exit(1);
  });

  // ── 3. 创建 release（tag 由 target_commitish=main 创建）──────
  console.log(`=== 创建 Gitee release ${TAG} ===`);
  const createRes = await fetchWithRetry(
    `${API}/releases`,
    {
      method: "POST",
      headers: { "Content-Type": "application/json;charset=UTF-8" },
      body: JSON.stringify({
        access_token: token,
        tag_name: TAG,
        name: TAG,
        body: notes,
        target_commitish: "main",
        prerelease: false,
      }),
    },
    "create-release"
  );
  const createJson = await createRes.json().catch(() => ({}));
  if (!createRes.ok || !createJson.id) {
    console.error("创建 release 失败:", JSON.stringify(createJson));
    process.exit(1);
  }
  const releaseId = createJson.id;
  console.log(`release id = ${releaseId}`);

  // ── 4. 上传安装器作为 release 附件 ──────────────────────────
  // multipart body 是流、跨重试不可复用，故在重试 try 块内重建 FormData。
  console.log(`=== 上传资产 ${basename(asset)} ===`);
  const fileBuf = await readFile(asset);
  let upRes;
  let lastUploadErr;
  for (let attempt = 1; attempt <= MAX_ATTEMPTS; attempt++) {
    const form = new FormData();
    form.append("access_token", token);
    form.append("file", new Blob([fileBuf]), basename(asset));
    try {
      upRes = await fetch(`${API}/releases/${releaseId}/attach_files`, {
        method: "POST",
        body: form,
      });
      break; // 拿到响应（ok 或 HTTP 错误）即停止重试
    } catch (err) {
      lastUploadErr = err;
      if (attempt < MAX_ATTEMPTS) {
        const waitMs = 2000 * attempt;
        console.error(
          `  [upload] 网络错误(${attempt}/${MAX_ATTEMPTS})，${waitMs}ms 后重试: ${err.message || err}`
        );
        await sleep(waitMs);
      }
    }
  }
  if (!upRes) {
    throw lastUploadErr;
  }
  const upJson = await upRes.json().catch(() => ({}));
  if (!upRes.ok) {
    console.error("上传资产失败:", JSON.stringify(upJson));
    console.error(
      "release 已创建（含更新内容），但二进制未上传。可在网页端手动上传：" +
        `https://gitee.com/${OWNER}/${REPO}/releases/edit/${releaseId}`
    );
    process.exit(1);
  }
  console.log("上传完成");

  console.log(
    `=== 完成: https://gitee.com/${OWNER}/${REPO}/releases/tag/${TAG} ===`
  );
}

main().catch((e) => {
  console.error("发布异常:", e);
  process.exit(1);
});
