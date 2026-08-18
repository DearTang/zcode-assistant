# zcode-assistant

> zcode 使用增强工具：配额监控、模型管理、自动切换、悬浮球、账号切换、用量统计。

一个基于 **Tauri 2 + React 18 + TypeScript + Vite** 的桌面应用，采用 Sequoia-X 液态玻璃设计风格，所有数据本地读写 `~/.zcode/v2`，不外传。

## ✨ 功能特性

| 模块 | 说明 |
| --- | --- |
| 📊 总览 | 配额监控（每 5 小时 / 每周）、当前模型、账号状态，托盘双环图标实时反映用量 |
| 🧩 模型管理 | 拉取可用模型、配置上下文 / 输出上限、写回 zcode、自定义供应商 + 预设一键填充；Token Plan 6 家供应商自动额度查询 |
| 🔀 自动切换 | 定时切换、配额耗尽自动切换到下一个供应商（直写 zcode 会话模型选择记录 + 直改 setting.json） |
| 📁 项目 / 会话 | 读取 zcode 会话库，按项目查看会话数 / 对话次数 / token 消耗；行内改名、归档 / 恢复、批量删除 |
| 👤 智谱账号 | 多账号捕获与一键切换 |
| 🌐 网络代理 | HTTP / SOCKS5 代理 |
| 📈 用量查询 | 解析 zcode rollout 日志，按供应商 / 模型 / 日期统计 token 用量与速度，支持月限额桶 |
| 🎨 美化 | 自实现 asar 解包 / 重打包，注入 `zcode-custom.css` 实现 ZCode 主题换肤 |
| ⚙️ 设置 | 主题（深色液态玻璃 / 浅色）、配额查询模板、关于、检查更新 |
| 🎈 悬浮球 | 桌面悬浮快捷面板 |

## 🚀 快速开始

### 环境要求

- [Node.js](https://nodejs.org/) ≥ 18
- [Rust](https://www.rust-lang.org/)（stable 工具链）
- Tauri 2 系统依赖：参见 [Tauri Prerequisites](https://tauri.app/start/prerequisites/)

### 安装依赖

```bash
npm install
```

### 开发模式

```bash
npm run tauri:dev
```

### 打包发布

```bash
npm run tauri:build      # 产物在 src-tauri/target/release/bundle/
```

## 🧱 技术栈

- **前端**：React 18 · TypeScript · Vite 6
- **后端**：Rust · Tauri 2
- **存储**：SQLite（rusqlite，bundled）+ 系统 keyring（凭证安全）
- **加密**：AES-256-GCM（zcode `credentials.json` 的 `enc:v1` 解密）
- **网络**：reqwest（rustls，免 OpenSSL）

## 🔢 版本号与发版

版本号的**单一真相源**是 [`src-tauri/Cargo.toml`](./src-tauri/Cargo.toml) 的 `[package] version`：

- 编译期通过 `env!("CARGO_PKG_VERSION")` 注入（见 [`version.rs`](./src-tauri/src/version.rs)）
- `npm run build` 会自动通过 [`scripts/sync-version.mjs`](./scripts/sync-version.mjs) 同步到 `package.json` / `package-lock.json`
- `tauri.conf.json` 不再写 `version` 字段，Tauri v2 直接读 Cargo.toml
- 单独同步：`npm run version:sync`

发版流程：

```bash
# 1. 改 Cargo.toml 的 version
# 2. 同步 + 打包
npm run version:sync
npm run tauri:build

# 3. 发布到 Gitee / GitHub（需设置对应 TOKEN 环境变量，或仓库根 .gitee-token / .github-token 文件）
npm run release:gitee  -- <version> <notes.md>
npm run release:github -- <version> <notes.md>
# 例：npm run release:gitee -- 0.2.0 CHANGELOG.md
```

## 📦 仓库

- Gitee：<https://gitee.com/argustang/zcode-assistant>
- GitHub：<https://github.com/DearTang/zcode-assistant>

## 🔄 自动更新

采用**自定义轻量更新**方案（不集成 `tauri-plugin-updater`，无签名密钥 / CI manifest）：

- 后端 `check_for_updates` 拉取 Gitee 发行版列表，按 semver 取最大 tag 与当前版本比较
- 发现新版本后弹窗：`更新` → 流式下载安装包（进度条）→ `安装并重启`
- 非 Windows 平台走「打开下载页」浏览器兜底
- 配置项在 [`src-tauri/src/updater.rs`](./src-tauri/src/updater.rs) 顶部的 `RELEASES_ENDPOINT`

## 📊 匿名使用统计（opt-in）

- 默认**关闭**，每个新版本首次启动询问一次（同意 / 暂不）
- 仅上报：**匿名设备 ID + 应用版本 + 操作系统**，绝不收集账号 / 凭据 / 连接内容
- 经 [Umami Cloud](https://cloud.umami.is/) 上报，每版本一次
- 启用：在 [`src/lib/usageStats.ts`](./src/lib/usageStats.ts) 填入你注册的 `UMAMI_WEBSITE_ID`（留空则跳过上报）

## 🔒 隐私与安全

- 所有配置本地读写，不外传
- API Key 等凭证通过系统 keyring 存储，界面层脱敏展示
- 仓库已通过敏感信息扫描，无硬编码密钥 / 令牌

## 📁 项目结构

```
zcode-assistant/
├── src/                    # 前端（React + TS）
│   ├── views/              # 各功能页面
│   ├── components/         # 复用组件（弹窗、图标、进度等）
│   ├── hooks/              # 自定义 hooks（主题、更新检查）
│   ├── lib/                # 工具库（使用统计等）
│   └── api.ts              # Tauri command 调用封装
├── src-tauri/              # 后端（Rust + Tauri）
│   ├── src/
│   │   ├── commands/       # #[tauri::command] 命令模块
│   │   ├── zcode/          # zcode 配置读写 / 解密 / 进程
│   │   ├── updater.rs      # 自动更新逻辑
│   │   ├── version.rs      # 版本号常量
│   │   └── db.rs           # SQLite 数据层
│   └── tauri.conf.json
└── scripts/                # 版本同步 / 发版脚本
```

## 📄 License

私有项目。

## 🔄 更新日志

完整版本历史见 [CHANGELOG.md](./CHANGELOG.md)。最新版要点摘录：

### v0.3.0 (2026-08-18)

**新增**：模型拖拽排序（供应商详情 ⠿ 手柄）· 主供应商快捷切换（列表右侧 ⭐ 按钮，详情弹窗复选框移除）· Token 获取展示最终捕获内容（默认掩码，点「显示」可复制）· 模型真实上下文长度（每日拉取 OpenRouter 目录，模糊匹配兜底，优先于内置写死规格）。

**修复**：火山方舟 AccessKey / 智谱团队版组织 ID 保存体验（「使用模板」不再清空附加凭据，凭据框独立「保存」按钮 + 提示）· Token 获取支持 HttpOnly cookie（Windows WebView2 CookieManager 轮询兜底，可读阿里系 SSO 等 HttpOnly 票据）。

### v0.2.0 (2026-08-18)

**新增**：月度额度查询 · 项目 / 会话管理 · Token Plan 6 家供应商自动额度查询 · 供应商预设 · 应用内「关于」弹窗 · 单实例启动 · 全局 Toast 通知 · 立即检测供应商连接。

**变更**：自动切换改为直写 zcode 会话模型选择记录（修复「未实际切到所选模型」的回归）· ZCode 主题换肤（asar 自实现解 / 重打包，保留 `unpacked` 标记）· 供应商 UUID 别名解析（用量页等显示真名）· 模型管理 / 自动切换页面改版 · 悬浮球面板交互优化 · 用量页文案与口径微调 · Umami 事件名加 `app` 前缀，与 myshell 区分。
