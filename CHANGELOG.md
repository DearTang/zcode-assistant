<!--
  本文件同时服务于两处：
  1. 仓库维护者：发版前在 [未发布] 下累积条目，发版时改为版本号 + 日期；
  2. 应用内「关于」弹窗：构建期以 `?raw` 打包进前端（src/components/AboutDialog.tsx），
     离线展示更新日志。HTML 注释与多余空行会在渲染前被剥离。
  格式基于 Keep a Changelog（https://keepachangelog.com/zh-CN/1.1.0/），
  版本号遵循语义化版本（https://semver.org/lang/zh-CN/）。
  发版流程见 AGENTS.md「Slash rule: 打包」；baseline 记录上一次发布的版本，
  用于步骤 3 的 `git diff --stat <baseline>..HEAD` 完整度核对。
  baseline: v0.5.0
-->

# 更新日志

## [未发布]

## v0.5.0 - 2026-08-21

### ✨ 新增

- 「登录 Token」方式新增「手动输入 Token」：跳过内嵌登录窗，直接粘贴从浏览器
  DevTools / 其他工具复制的 Token（≥8 字符），与自动捕获走同一 keyring 存储
  通道，模板 `{{token}}` 引用不变——不一定非要从登录窗获取。
- 用量查询模板内置预设对齐 cc-switch 最新实现：新增 智谱国际版（api.z.ai）与
  MiniMax 国际版（api.minimax.io）两组 Token Plan 预设（后端本就按 baseURL 自动
  识别查询，预设供「使用模板」下拉一键参考）。
- 用量查询模板升级为 cc-switch 四桶口径：主桶（余额/通用）之外新增「每5小时 /
  每周」窗口路径组与各组重置时间路径，同一响应可提取 每周+每月+余额 等多桶，
  配了 5小时/每周 的模板走双环展示；新增「配额单位 %」选项（提取值 ≤ 1.0 自动
  ×100，total 兜底 100），百分比类 API（如 Qwen Token Plan）可直接接入。
- Token 提取方式支持 `cookie:`（留空）或 `cookie:*`：提取完整 cookie 串（Windows
  含 HttpOnly），适合 Cookie 头认证的供应商，无需逐个猜测哪个 cookie 是认证票据。
- 内置「Qwen 通义千问 Token Plan」预设模板：Cookie 登录 + 周配额百分比 +
  重置时间一键配置，选中预设后点「登录获取 Token」即可。
- 启动引导主供应商：应用启动时 best-effort 自动选主（已设过且仍存在 → 不动；
  智谱 Coding Plan 内置订阅（账号登录态）→ 选中；否则第一个 enabled 且带模型的
  自定义供应商 → 选中；都没有 → 返回 null 不报错），并同步 setting.json 的
  family 选中（bigmodel）保证总览 / 悬浮窗有数据源。配合上一条「无主供应商且
  无智谱账号 → 不查询不报错」的修复，全新环境（未登录 / 无供应商）启动不再
  出现「配额查询失败」toast，总览展示引导文案。

### 🛠️ 变更

- OpenRouter 模型目录新增输出长度字段（`max_completion_tokens`，接口未提供时默认
  131072），与上下文长度同源同接口。`matched_spec` / `fetch_available_models` /
  导入未命中兜底均使用该值；导出到 cc-switch / opencode 时 limit.output 不再要求
  「必须为正」才导出——缺失时按 131072 兜底，解除此前 opencode 「Missing key
  limit.output」的校验失败。

### 🐛 修复

- 修复供应商卡片可用性误判：百分比桶剩余量为极小正数（如 0.4%）时显示「剩0%」
  却仍判「可用」——改用与显示一致的取整口径（%桶 Math.round ≤ 0 即耗尽）判定，
  月/周/5小时/余额桶全部参与。
- 修复供应商卡片重置时间始终优先 5 小时桶的问题：改为按「月→周→5小时」优先
  展示已耗尽桶的重置时间（未耗尽回退 5 小时），月配额耗尽时不再误导。
- 修复总览配额查询在「未设主供应商且未登录智谱 Coding Plan」时报错刷屏的问题：
  fetch_overview_quota 入口短路，双空时直接返回空 QuotaOverview（source="none"，
  error=None），不发网络请求、不触发错误 toast；总览空状态展示引导文案
  （设置主供应商或登录智谱账号后可查看用量）。
- 修复美化主题「颜色替换不彻底」：左侧栏与底部输入栏仍为默认浅色。preset_vars 现
  在补全 `--color-panel` / `--color-sidebar` / `--color-header` / `--color-input` 四
  个 token（默认引用 `var(--color-neutral-100/200)`，不覆盖会留下浅色侧栏/输入栏），
  六个预设主题一并加上。
- 修复毛玻璃与背景图只在启动屏可见、菜单/对话界面挂载后被盖住的问题。真机
  app.asar 实测根因有三：(1) 卡片/弹层/输入等表面 token（--color-card / popover /
  input / secondary）未做半透明覆盖，聊天气泡与弹窗仍为实心底色；(2) 主样式表把
  71+ 个工具类编译为字面量色值（如 `.bg-background/95 → #fafafae6`、
  `.dark:bg-[#484A58]`），CSS 变量覆写与类名枚举都够不着；(3) 调色板覆写的混色
  源 --color-surface 本身近透明（oklab … / 0.03），surface_opacity 形同虚设。
  修复：表面 token 补全四项；混色源改用主题/自定义背景色；新增注入
  zcode-custom.js 运行时补丁——由 CSS 变量 --zq-alpha / --zq-blur 作开关，React
  挂载后用 MutationObserver 把计算样式为不透明的背景持续改写为目标透明度，并给
  最大的 ≤6 个大面块加 backdrop-filter 真实磨砂（排除 #loading / 代码块 / 媒体
  元素）；未开启透明特性时 JS 自动空转。对已注入过 CSS 的旧 asar 原地升级补
  script 标签，重新「应用美化」即可生效。
- 修复同步到 cc-switch 后 opencode 报「Configuration is invalid」：zcode 的模型
  字段与 opencode schema 不兼容——reasoning 是对象（enabled/variants/
  defaultVariant）而 opencode 期望 boolean；limit 缺 output 会被判 Missing key。
  导出改为按 opencode schema 白名单（name + 完整的 limit{context,output} +
  modalities），不兼容字段全部丢弃。重新同步并在 cc-switch 里重新应用后即恢复
  （已损坏的 opencode.json 需删除对应 provider 条目或重新应用覆盖）。

## [0.4.1] - 2026-08-20

### 🐛 修复

- 修复应用内更新「启动安装器失败: 请求的操作需要提升 (os error 740)」：v0.4.0 起
  安装包为 perMachine（装 Program Files，需管理员），普通 spawn 无法启动安装器。
  现在用 ShellExecuteW("runas") 提权启动（弹 UAC 确认），用户点「否」时报错带提示。
- 修复「覆盖启动」在生产版不生效：app.restart() 与单实例插件存在竞态，新进程可能
  先于旧进程释放单实例锁启动、把自己判定为第二实例后随即退出。现在改由 cmd 延迟
  1 秒启动新实例（旧进程先退出释放锁），并继承原进程完整性级别。

## [0.4.0] - 2026-08-20

### ✨ 新增

- 设置新增「开机自启动」开关：开启后登录系统自动启动 zcode-assistant 并驻留托盘
  （通过 tauri-plugin-autostart 注册系统自启项；设置页打开时以系统实际状态为准，
  用户从系统设置中手动关闭后开关会同步回退）。
- 同步到 cc-switch：模型管理新增反向同步，把 zcode 的自定义供应商（含模型与
  上下文限制）统一写入 cc-switch 数据库的 opencode 供应商组（app_type='opencode'，
  切换供应商时由 cc-switch 落到 opencode.json，不再直写 opencode.json 避免被
  cc-switch 回写覆盖）。baseURL + apiKey 一致的条目覆盖更新、其余新增，写入前自动
  备份数据库（.db.bak）；同步前弹窗预览勾选（默认仅勾选 zcode 中启用的），
  cc-switch 正在运行时提示重启后查看。模型上下文等限制忠实携带 zcode 现有配置
  （纠正入口统一在导入侧的「重新获取上下文」开关）。
- 导入配置「重新获取上下文」开关（导入按钮旁，启动默认关）：
  开启后本次导入按 OpenRouter 目录 / 内置规格表匹配模型真实上下文并覆盖旧默认值
  （如 200k）；未命中的模型弹窗逐个确认（预填 200000，可修改）后再写入。
  仅当次导入生效，完成后自动还原为关；开关关闭时不改动任何上下文配置。

### 🛠️ 变更

- 应用图标重设计：从默认波形图标换为「Z 字母标识」（深色靛蓝+青绿渐变，圆角
  方形，发光滤镜）-- 主符号为居中放大的几何 `Z`，呼应 zcode 字母身份。
  设计源 `src-tauri/icons/source.svg`，通过 `npm run icon:gen` + `tauri icon`
  派生 `app-icon.png` 与全部 bundle / Windows Store / Android / iOS 图标；侧栏与
  「关于」弹窗的内联 SVG 与 `.za-logo-mark` 配色同步更新。
- 安装包改造：采用自定义 NSIS 模板 `src-tauri/nsis/installer.nsi`（基于 Tauri
  官方模板），实现两项关键改动 -- 升级时旧版卸载改为静默执行（加 `/S` 标志），
  不再弹完整卸载向导，把「先卸载、再安装」两轮交互降到一次「下一步」；
  主程序卸载 `Delete` 加 `/REBOOTOK`，进程占用无法立即删除时登记重启后删除，
  升级/卸载不再因此卡住或残留。同时 `tauri.conf.json` 的 `bundle.targets`
  收窄为 `["nsis"]`（仅 Windows），新增 `bundle.windows.nsis` 配置块：
  `installerIcon` 显式指向 `icons/icon.ico`（安装包 EXE 图标 = 新 Z 字母图标），
  `installMode: perMachine`（装到 `Program Files`，系统级），中英双语、无语言选择器。
- 供应商「⚡ 检测」改为「配置验证」口径：GET /models 探测只验证 baseURL + apiKey
  连通性（配置是否正确），提示文案与按钮说明均明确「不代表模型可用 / 额度充足」，
  可用性请以额度行徽标为准。
- 配额百分比统一取整显示：总览「主供应商配额」等处的百分比不再带小数
  （如 46.123112 % → 46%），消除原始浮点精度泄漏。
- 「模型用量展示方案」升级为全局口径并独立设置分区：原先只管悬浮球 / 悬浮面板 /
  托盘，现在总览「主供应商配额」与供应商管理的额度行同样跟随（数字与进度 /
  环弧长随方案切换已用 / 剩余，颜色始终按已用度分级）；设置页中从「悬浮球与托盘」
  拆分为独立分区。

### 🐛 修复

- 修复火山方舟填写账号级 AccessKey 后查询不到用量且报错含糊的问题（对齐
  cc-switch）：火山网关对签名 / 凭据错误常返 HTTP 400/403 且错误信息只在 body 的
  ResponseMetadata.Error 信封里，此前非 2xx 直接按状态码报错。现在解析信封并识别
  鉴权类错误码（auth/signature/accessdenied 等），给出「需账号级 AccessKey、
  到用量查询模板核对」的明确引导；Result 字段缺失时退回顶层解析；两个接口都解析
  不到额度时附带原始响应片段，便于区分「未订阅」与「结构变化」。
- 修复单实例弹窗「覆盖启动」在开发模式下弄断 `tauri dev` 会话的问题：dev 模式下
  应用进程退出会连带结束 tauri CLI 并杀掉 Vite，重新拉起的进程成为孤儿、窗口刷不出
  localhost。现在 debug 构建点「覆盖启动」仅重载主窗口页面，不再重启进程
  （生产安装包行为不变）。
- 修复用量统计与 zcode 对不上的问题：zcode 会自行清理 `model_usage` 行（如清除
  会话用量），而本地增量导入只增不删，被清理的记录会永久残留导致统计偏高。现在
  每次同步按 request_id 与 zcode 侧全量对账，顺带回收已删除的记录（同步信息行
  显示「对账回收 N 条」）。
- 修复模型拖拽排序在 zcode 内不生效的问题：此前只重排 config.json 中 models 的
  键顺序，但 zcode 按每个模型的 `zcode.priority` 升序展示、且与服务端下发的权威
  模型条目合并时仅保留 `zcode.modified=true` 的本地条目，键顺序仅作兜底。现在
  排序时按新顺序写入全部模型的 `zcode.priority`（0,1,2…）与 `zcode.modified=true`，
  zcode 重启后即按新顺序展示。

## [0.3.0] - 2026-08-18

### ✨ 新增

- 模型手动排序：供应商详情的模型列表支持拖动 ⠿ 手柄调整展示顺序（顺序写入
  config.json，与 zcode 同步）。
- 主供应商快捷切换：供应商列表卡片右侧新增「⭐ 设为主供应商」按钮，一键设置 /
  取消（原详情弹窗内的复选框移除，减少操作步骤）。
- Token 获取展示最终捕获内容：「Token 获取」区新增 Token 值行，默认掩码，
  点「显示」核对明文（可复制）；重新获取后明文自动收回。
- 模型真实上下文长度：每日启动拉取一次 OpenRouter 模型目录
  （记录模型名小写 / 上下文 / 输入模态 / 作者 / 创建与更新时间，按更新时间从新到旧
  排序，失败沿用上一次数据）；「拉取可用模型」时对缺上下文的模型按名称模糊匹配
  目录中最新一条，自动填充真实 context（优先于内置写死规格表）。

### 🐛 修复

- 修复火山方舟 AccessKey / 智谱团队版组织 ID 填写后「无法保存」的体验问题：「使用模板」
  一键套用预设不再静默清空已填的附加凭据；两个凭据框就近增加独立「保存」按钮并补充
  保存提示（原先需到面板顶部点「保存模板」，提示文案也未说明）。
- Token 获取支持 HttpOnly cookie（Windows）：登录窗在页面脚本提取之外，新增
  WebView2 原生 CookieManager 每秒轮询兜底，可读取 document.cookie 读不到的
  HttpOnly 票据（如阿里系 SSO 的 login ticket）；命中后同样写入系统凭证库并关窗。

## [0.2.0] - 2026-08-18

### 新增

- 用量查询模板支持月限额：新增「每月总额 / 已用 / 剩余 path」映射（部分供应商才有），
  配置后从同一响应追加「每月使用额度」桶；总览按桶展示（查到才显示对应行），
  模型管理供应商额度行追加「每月 剩xx%」，未配置或提取不到则不展示。
- 项目 / 会话管理：读取 zcode 会话库，按项目查看会话数（活跃 / 归档）、对话次数、
  token 消耗（含子代理，与用量页同口径）和创建 / 活跃时间；支持会话改名（直接写入
  zcode 会话库，zcode 端同步显示）、会话 / 项目一键归档与恢复（行内操作，对称于
  zcode 的归档语义）、项目 / 会话勾选批量删除（级联清理消息、任务索引、用量记录
  与本地缓存）。归档判定对齐 zcode 任务索引（tasks.archived/deleted），归档默认
  隐藏，「查看历史项目 / 查看历史会话」两级开关分别控制全归档项目与项目内归档
  会话的显隐。
- Token Plan 额度自动查询：识别 6 家供应商（Kimi / 智谱个人版 / MiniMax / ZenMux /
  火山方舟 / 智谱 Coding Plan 账号），按 baseURL 自动用该供应商 API Key + Base URL
  查额度，无需手配模板；其中智谱团队版需组织 / 项目 ID、火山方舟需账号级 AK / SK，
  由用户在「模型管理 → 用量查询模板」填写（存于 extraJson）。
- 供应商预设：「添加供应商」弹窗一键填充预设供应商配置（基于 cc-switch 已整理为
  anthropic 格式），免除重复手工输入。
- 应用内「关于」弹窗：离线展示 CHANGELOG（构建期以 `?raw` 打包进前端），无需联网
  即可查看更新历史。
- 单实例启动：第二个实例启动时弹出选择（覆盖启动 / 退出），避免多开造成的状态冲突。
- 全局 Toast 通知：成功绿 / 错误红 / 警告黄、4s 自动消失、悬停暂停（离开后再计时）、
  可复制、可手动关闭。
- 立即检测供应商连接：「模型管理」卡片顶栏「⚡」按钮（GET /models，不消耗 token）。

### 变更

- 自动切换执行方式：改为直写 zcode 会话库的模型选择记录（session_entry 的
  runtime/model_selection，会话恢复时按其还原模型）+ 直改 setting.json，对所有
  符合条件的会话生效（规则限定项目时仅该项目）；偏好「切换后重启 ZCode」开启
  （默认）时自动重启 ZCode 立即生效，关闭时各会话在恢复 / 新开时生效。全程不再
  依赖键盘模拟点击 ZCode 界面（该方式受菜单结构 / 前台焦点影响不可靠），修复了
  自动切换未实际切到所选模型的问题。
- ZCode 界面美化：自实现 asar 解包 / 重打包（精确保留 `unpacked` 标记，支持备份与还原），
  按配置生成 `zcode-custom.css` 注入 ZCode 渲染层，实现主题换肤。
- 供应商别名解析：扫描 zcode transcript，从系统消息中提取 UUID → 名称映射
  （`provider_aliases`），让用量页等处显示可读的供应商名称。
- 模型管理与自动切换页面改版，悬浮球面板交互优化。
- 用量页文案与口径微调。
- Umami 统计事件名统一加 app 前缀，与 myshell 区分。

## [0.1.0] - 2026-08-13

首个版本。

### 新增

- 总览：5 小时 / 每周配额双环监控、当前模型与账号状态一览。
- 模型管理：获取可用模型、配置上下文并写入 zcode。
- 自动切换：定时切换、配额耗尽自动切换。
- 用量查询：按供应商 / 模型 / 日期统计 token 用量与响应速度。
- 智谱账号：多账号捕获与一键切换。
- 网络代理：HTTP / SOCKS5 代理配置。
- 悬浮球与悬浮面板，托盘图标以双进度环实时显示配额（颜色随用量分级）。
- 应用内自动更新：检查新版本、流式下载安装器并重启安装（Windows），
  其他平台回退为浏览器下载。
- 匿名安装统计：每个新版本首次启动征求同意后上报，数据仅用于了解安装规模。
