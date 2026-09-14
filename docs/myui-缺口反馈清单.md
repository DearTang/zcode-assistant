# myui 缺口反馈清单（基于 v0.7.0 消费实践，v0.10.0 已大部分落实）

> 项目：zcode-assistant 前端迁移（React → Vue 3 + myui，壳层对齐 unified-ui-vue 模板）
> 初版：2026-09-13（v0.7.0 消费评估）；**更新：2026-09-13——myui v0.8.0~v0.10.0 已落实绝大部分条目，本项目已升级到 v0.10.0 并删除对应本地兜底**。每节标注落实状态。

---

## 落实情况总览（v0.10.0）

| 本项目动作 | 对应 myui 版本 |
|---|---|
| ✅ 删除 `src/icons/register.ts` 运行时注册（26/27 图标进内置白名单） | 0.9.0 图标白名单 +45（12 个自绘线性图标取自本项目图标集 + 33 个 EP 标准名）；0.9.0 图标名类型开放 `IconName \| (string & {})` |
| ✅ 侧栏「设置」图标改名 `Settings` → `Setting`（唯一例外，见 0.9.0 Not adopted） | — |
| ✅ 删除本地 `Progress.vue` → `MyProgress :percentage + colorValue/warnAt/dangerAt` | 0.9.0 配额语义 |
| ✅ 删除本地 `DualRing.vue` → `MyDualRing`（value/innerValue + colorValue/innerColorValue 分档） | 0.9.0 |
| ✅ 删除本地 `fields/UnitSlider.vue` → `MySlider :unit`（Beautify 7 处 + Settings 3 处） | 0.9.0 |
| ✅ 删除本地 `fields/ColorField.vue` → `MyColorField`（清空回传 null，调用方转 undefined） | 0.9.0 |
| ✅ 删除本地 `fields/InlineEdit.vue` → `MyInlineEdit`（Accounts/Projects 点击即编辑，删除 editingId 状态与编辑按钮） | 0.9.0 |
| ✅ `fields/WeekdayPicker.vue` 内部改用 `MyPopover`（删除 document mousedown 手工收起） | 0.9.0 |
| ✅ ProviderAddModal 预设下拉：`el-select + el-option-group` → `MySelect :groups`；错误横幅 → `MyDialog :error` | 0.9.0 |
| ✅ ProviderEditModal「使用模板」原生 select → `MySelect :groups`（三组） | 0.9.0 |
| ✅ Projects 表头半选：`el-checkbox` → `MyCheckbox :indeterminate` | 0.9.0 |
| ✅ Projects 清理缓存弹窗：自定义 footer → 默认 footer + `:danger :confirm-loading :confirm-disabled :manual-close` | 0.9.0 / 0.10.0 |
| ⏸ 悬浮球/悬浮面板保留本地（透明窗口业务件，框架无此层；未挂 `html.no-glass`——面板半透明卡片是刻意视觉，挂了会降为实底） | 0.9.0 no-glass + --z-* 令牌（备用） |

---

## A. 缺失组件（原始清单，v0.7.0 时点）

| # | 组件 | 落实状态 |
|---|---|---|
| 1 | MyProgress 配额语义（colorValue 解耦 + warnAt/dangerAt） | ✅ 0.9.0 |
| 2 | DualRing 双环进度 | ✅ 0.9.0 MyDualRing |
| 3 | MyTooltip | ✅ 0.9.0（本项目暂无强调用点，未接入） |
| 4 | MyCheckbox indeterminate API 化 | ✅ 0.9.0 |
| 5 | Popover / 多选面板 | ✅ 0.9.0 MyPopover（星期多选业务件保留，内部已换用） |
| 6 | MyColorField | ✅ 0.9.0 |
| 7 | MyInlineEdit | ✅ 0.9.0 |
| 8 | MySkeleton | ✅ 0.9.0（独立导出；本项目暂无强调用点） |
| 9 | 拖拽排序封装 | ❌ 0.9.0 Not adopted（sortablejs 不在库 peer 边界内，维持应用层手写 draggable） |
| 10 | MySlider unit 后缀与数值显示 | ✅ 0.9.0 |
| 11 | MyNumberField / MyInput unit 后缀 | ✅ 0.9.0（本项目暂未改用，ZcodeSettings 保持 MyInput+后缀） |

## B. 现有组件 API 缺口

| # | 缺口 | 落实状态 |
|---|---|---|
| 12 | MyDialog header 插槽 / 错误横幅 / 宽度档位 | ✅ 0.9.0：#header 透传、`error` 模幅（ProviderAddModal 已用）、`size: sm\|md\|lg` |
| 13 | toast 队列上限 / action / 复制 | ❌ 0.9.0 Not adopted（EP 无 action API；`grouping` 缓解刷屏，维持现状） |
| 14 | MySelect options 分组 | ✅ 0.9.0 `groups` |
| 15 | MyStatCard error/empty 态 | ❌ 0.9.0 Not adopted（MyResultState 组合，本项目已如此使用） |
| 16 | MyProgress 自定义阈值 | ✅ 0.9.0（并入 #1） |

## C. 类型层缺口

| # | 问题 | 落实状态 |
|---|---|---|
| 17 | IconName 闭联合类型 | ✅ 0.9.0 开放为 `IconName \| (string & {})`（本项目已删除运行时注册与组件传递绕过） |

### C+. 内置图标白名单（16 项建议）

✅ **15 项已进 0.9.0 内置**：Dashboard、Cpu、Swap、Chart、Folder、Globe、Sparkle、Sliders、Trash、Zap、Activity、Power、External、Sun、Moon。
⚠️ `Settings` 未加（0.9.0 Not adopted：库内已有 EP `Setting`，建议消费方改名）——本项目已将侧栏图标改为 `Setting`。

## D. 横切约束

| # | 约束 | 落实状态 |
|---|---|---|
| 18 | 浮层非玻璃降级 | ✅ 0.9.0 `html.no-glass` 一键降级（本项目悬浮窗未挂载：面板半透明卡片为刻意视觉，且未用 myui 浮层；留作未来接入 myui 浮层时的开关） |
| 19 | --z-* 层级令牌 | ✅ 0.9.0 `--z-behind…--z-vault` |
| 20 | --space-* 运行时化 | ❌ 0.9.0 Not adopted（全组件重构，另立项目） |
| 21 | 主题切换仅 html.dark/light | 维持（本项目已迁移到该约定） |

---

## 已验证良好的部分（无需改动）

- `MyDialog` 的 `dismissable`（表单防误关默认）/ `manualClose` / `#footer` / `append-to-body` 嵌套 / 0.10.0 新增 `showClose / closeOnClickModal / closeOnPressEscape` 独立关闭策略。
- `confirmDialog() / toast() / notify()` 命令式、免 Provider、Promise 化，135+ 处调用点迁移零摩擦。
- `MyPanel / MyButton / MyBadge(success) / MyStatusDot / MySegmented / MyTabs / MyTree / MyFieldShell / MyInput(show-password 透传) / MyDataTable(排序/插槽) / MyFilterBar / MyResultState(#actions) / MyCommandPalette` 全部按预期工作。
- 样式分层（`element-plus → dark css-vars → myui/styles → myui/element-theme → app.css`）与模板一致，未出现令牌冲突。
- 0.8.1 修复的 MyIcon 字符串 size/icon 解析问题在本次升级中顺带受益。

---
