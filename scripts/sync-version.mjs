#!/usr/bin/env node
/**
 * 版本号单一真相源同步（参考 myshell）。
 *
 * `Cargo.toml` 的 `[package] version` 是唯一手写版本号的地方。本脚本读取它并
 * 同步到 npm 侧的 `package.json` / `package-lock.json`，保持一致、无需手动改。
 * `tauri.conf.json` 无需 version 字段——Tauri v2 在缺省时直接读 Cargo.toml。
 *
 * 已接入 `npm run build`（`tauri build` 通过 beforeBuildCommand 触发），保证发版
 * 构建各处版本一致；也可单独 `npm run version:sync` 运行。
 */
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

function readJSON(p) {
  return JSON.parse(readFileSync(p, "utf8"));
}

function writeJSON(p, data) {
  writeFileSync(p, JSON.stringify(data, null, 2) + "\n");
}

// 1. 真相源：Cargo.toml [package] version
const cargo = readFileSync(join(root, "src-tauri/Cargo.toml"), "utf8");
const m = cargo.match(/^version\s*=\s*"([^"]+)"/m);
if (!m) {
  console.error("[sync-version] could not find `version = \"…\"` in Cargo.toml");
  process.exit(1);
}
const version = m[1];

let touched = false;

// 2. package.json
const pkgPath = join(root, "package.json");
const pkg = readJSON(pkgPath);
if (pkg.version !== version) {
  pkg.version = version;
  writeJSON(pkgPath, pkg);
  console.log(`[sync-version] package.json -> ${version}`);
  touched = true;
}

// 3. package-lock.json（root version + packages[""]）
const lockPath = join(root, "package-lock.json");
const lock = readJSON(lockPath);
let lockChanged = false;
if (lock.version !== version) {
  lock.version = version;
  lockChanged = true;
}
if (lock.packages && lock.packages[""] && lock.packages[""].version !== version) {
  lock.packages[""].version = version;
  lockChanged = true;
}
if (lockChanged) {
  writeJSON(lockPath, lock);
  console.log(`[sync-version] package-lock.json -> ${version}`);
  touched = true;
}

if (!touched) {
  console.log(`[sync-version] already at ${version} (Cargo.toml is the source)`);
}
