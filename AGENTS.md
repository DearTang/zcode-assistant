# AGENTS.md

面向 zcode-assistant 仓库的 AI 助手工作约定。本文件规则与 `myshell/AGENTS.md` 同源；本项目尚未引入 `progress.md` / `findings.md` / `task_plan.md`，因此本仓库的「文档随更新」只覆盖 `CHANGELOG.md` 与 `README.md`。

## Project

zcode-assistant 是面向 zcode CLI 的桌面增强工具（Tauri v2 + React + TypeScript），包含配额监控、模型管理、自动切换、悬浮球、智谱账号切换、项目/会话管理等功能。

## Commands（与现有 npm scripts 对齐）

Frontend:
- `npm run dev` — Vite dev server
- `npm run build` — 同步版本号 + `tsc` + `vite build`，产物输出到 `dist/`
- `npm run test:ts` — `tsc --noEmit` 类型检查
- `npm run preview` — 预览生产构建

Full app（从 `src-tauri/`）：
- `cargo tauri dev` / `npm run tauri:dev` — 启动桌面应用 + HMR
- `cargo tauri build` / `npm run tauri:build` — 构建 NSIS 安装包（会先调 `beforeBuildCommand`，即 `npm run build`）
- `cargo check` — 快速类型/错误检查（无产物）

版本号单一真相源：`src-tauri/Cargo.toml` 的 `version`；`npm run version:sync`（`scripts/sync-version.mjs`）会把它同步到 `package.json` / `package-lock.json`。`tauri.conf.json` 无 version 字段——Tauri v2 直接读 Cargo.toml。

Git remotes:
- `origin` = Gitee (`gitee.com/argustang/zcode-assistant`)
- `github` = GitHub (`github.com/DearTang/zcode-assistant`)

---

## Slash rule: `打包` / `帮我打包`（沿用 myshell，适配本项目）

用户发出 **`打包`** 或 **`帮我打包`**（或核心意图为「发版」）时，按下面的发布管线自动执行——**只在 changelog 写入后保留一道强制确认闸口**（步骤 3.5）。版本号变更、构建、git commit/push、Gitee/GitHub 发版均已通过这条指令获得一次性预授权；但**版本号 + 更新内容**必须在构建/推送前由用户复核确认。每一步开始时主动播报，透明可追，然后继续——到闸口处停下。

### 前置条件（一次性，由用户准备）

两边平台的 Token 都需就绪：

**Gitee** — 具备 `projects`/release 权限的私人访问令牌：
- 环境变量 `$GITEE_TOKEN`，或
- 仓库根的 `.gitee-token` 文件（**加入 .gitignore，永远不要 commit、永远不要在聊天/提交信息中粘贴值**）。

**GitHub** — 对 `DearTang/zcode-assistant` 拥有 Contents (read/write) 的 fine-grained PAT：
- 环境变量 `$GITHUB_TOKEN`，或
- 仓库根的 `.github-token` 文件（gitignored）。

发版步骤到达发布阶段时若任一 Token 缺失，**立即停下**，告诉用户如何生成 Token 放在哪里——**不要**继续走一遍失败的发版，也不要凭空捏造 Token。

### 管线（按顺序执行）

1. **决定版本号 bump（自动）**。首要信号 = `CHANGELOG.md` 的 `[未发布]` 段落里的条目**类型**（出现任意 `### 新增` → **minor**；只有 `### 修复` / `### 变更` / `### 安全` → **patch**）。读取 `src-tauri/Cargo.toml` 当前 version。执行前先给出「选定版本 + 一句话理由」。
2. **Bump 版本号** — 仅编辑 `src-tauri/Cargo.toml` 的 `version = "..."`（单一真相源），然后 `npm run version:sync`。
3. **生成 release notes** — `CHANGELOG.md` 的 `[未发布]` 是**首要来源**：把它的「新增 / 变更 / 修复 / 安全」按 `## vX.Y.Z（YYYY-MM-DD）` 的格式展开为正式 CHANGELOG 段落，分组保留（✨新增 / 🛠️变更 / 🐛修复 / 🔒安全）。再做一次**完整度核对**：`git diff --stat <baseline>..HEAD`（baseline 取 CHANGELOG `[未发布]` 段上方的 `baseline:` 注释行）——若变更的文件未被任何 staging 条目覆盖，主动补条目（兜住「忘了写更新日志」或「跨会话改动」）。保留文件头注释。镜像到 `README.md` 的「更新日志」小节。把新段落写入发布脚本所需的临时 notes 文件。
3.5. **⚠️ 确认闸口 — 在这里停下。** 出示：(a) 选定的版本号 + 理由；(b) 新的 CHANGELOG 段落全文。**未获用户确认前不要 build/commit/push/publish。** 用户若改动，先修订再重出。确认后步骤 4–9 自动推进。
4. **预检** — `npm run test:ts` 与 `cargo check`（在 `src-tauri/`）。两者都必须通过。
5. **构建安装包** — 仓库根 `npm run tauri:build`。首次较久（10+ 分钟），后台跑。产物：`src-tauri/target/release/bundle/nsis/zcode-assistant_X.Y.Z_x64-setup.exe`。
6. **Commit + push（两个 remote）** — 暂存 `Cargo.toml`、`package.json`、`package-lock.json`、`CHANGELOG.md`、`README.md`、特性代码，提交信息 `release: vX.Y.Z`，再 push 到**两个** remote（`origin` Gitee + `github` GitHub）。不要提交 `.gitee-token` / `.github-token`。host 端策略可能拦 push 到默认分支——授权用户或跑 `! git push origin main && git push github main`。
7. **发布 release（两个平台）** — 顺序执行两个脚本（顺序无所谓）：
   - `node scripts/publish-gitee-release.mjs <version> <temp-notes-file>`
   - `node scripts/publish-github-release.mjs <version> <temp-notes-file>`
   回报两条 URL。任一失败仍试另一边，并把失败回报清楚。
8. **清理 staging** — 在 `CHANGELOG.md`：清空 `[未发布]` 段下所有条目（保留 header/baseline 骨架），把 `baseline: v<X.Y.Z>` 更新为刚发布的版本。commit + push 到**两个** remote（`chore: clear release staging after vX.Y.Z`）。
9. **Doc-after-feature** — 确认 `README.md` 的「更新日志」与 CHANGELOG 一致。本项目无 `progress.md`，跳过该步骤。

### 备注 / 安全
- 若任一平台的 tag 已经存在，创建 release 会报错——这是预期行为，回报即可，**不要**静默删了重建。
- 这条规则授权的 git commit + push + 公开发版**仅限** `打包` 流程；其他任务不享受此授权。
- 不要读取、记录、打印 Token 值；脚本会自己读。
- 本项目目前没有 `progress.md` / `findings.md` / `task_plan.md`——以后若新增，记得同时在 AGENTS.md 的「Living planning docs」段补一条规则（参照 myshell）。