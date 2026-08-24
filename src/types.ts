/**
 * 共享类型定义 —— 对齐 zcode 的 config.json / setting.json 结构
 * 敏感字段（apiKey/token）在 UI 层脱敏，不外传。
 */

/** provider 协议类型 */
export type ProviderKind = "anthropic" | "openai" | "openai-compatible";

/** 单个 model 配置（config.json -> provider.<id>.models.<name>） */
export interface ZcModel {
  /** 实际发给 API 的 model id（缺省时用 key 本身） */
  name?: string;
  /** 是否启用（zcode-assistant 管理；缺省视为启用） */
  enabled?: boolean;
  limit?: {
    context?: number; // 上下文窗口 token 数
    output?: number; // 最大输出 token 数
  };
  reasoning?: {
    enabled?: boolean;
    variants?: string[];
    defaultVariant?: string;
  };
  modalities?: {
    input?: string[];
    output?: string[];
  };
  zcode?: {
    modified?: boolean;
    priority?: number;
  };
}

/** provider 配置 */
export interface ZcProvider {
  name: string;
  kind: ProviderKind;
  options: {
    apiKey?: string;
    baseURL?: string;
    apiKeyRequired?: boolean;
  };
  enabled?: boolean;
  source?: string; // "custom" | 内置
  systemDisabledReason?: string;
  models: Record<string, ZcModel>;
}

/** config.json 顶层 */
export interface ZcodeConfig {
  provider: Record<string, ZcProvider>;
}

/** setting.json 顶层（当前选中状态） */
export interface ZcodeSetting {
  providerFamilyDomain?: string;
  modelProviderFamilyModes?: Record<string, string>;
  modelProviderFamilySelectedKeys?: Record<string, string>;
}

/** provider id 类型 */
export type ProviderId = string;

/** 配额单位 */
export type QuotaUnit = "quota" | "tokens" | "requests" | string;

/** 单个资源包/窗口的配额 */
export interface QuotaBucket {
  name: string;
  total: number;
  used: number;
  remaining: number;
  unit?: QuotaUnit;
  periodEnd?: string; // ISO 时间
}

/** 聚合配额概览 */
export interface QuotaOverview {
  source: "coding-plan" | "template" | "unknown" | "none";
  /** 配额所属供应商显示名（总览 / 悬浮窗 / 托盘标注数据来源） */
  providerName?: string;
  accountLabel?: string;
  planName?: string;
  buckets: QuotaBucket[];
  fetchedAt: string; // ISO 时间
  error?: string;
}

/** 智谱账号快照元数据 */
export interface AccountMeta {
  id: string;
  shortId: string;
  userId?: string;
  provider?: string;
  label: string;
  email?: string;
  name?: string;
  avatar?: string;
  customerId?: string;
  note?: string;
  capturedAt: string;
}

/** 网络代理配置 */
export interface ProxyConfig {
  enabled: boolean;
  type: "none" | "http" | "socks5";
  host: string;
  port: number;
  username?: string;
  /** 密码单独存 keyring，此处仅占位 */
  hasPassword?: boolean;
}

/** 模型规格（用于自动填充上下文长度） */
export interface ModelSpec {
  id: string;
  name?: string;
  contextLength?: number;
  maxOutput?: number;
}

/** 配额展示方案：used=展示已用量，remaining=展示剩余用量 */
export type UsageDisplayMode = "used" | "remaining";

/** 应用偏好（持久化于后端 kv，悬浮球 / 悬浮面板 / 托盘共用） */
export interface AppPrefs {
  /** 悬浮球是否显示 */
  floatBallVisible: boolean;
  /** 模型用量展示方案 */
  usageDisplay: UsageDisplayMode;
  /** 自动切换 / 账号切换完成后是否提示重启 ZCode（默认 true） */
  switchRestartZcode: boolean;
  /** 是否开机自启动（默认 false） */
  autostart: boolean;
}

/** 主窗口视图 id */
export type ViewId =
  | "dashboard"
  | "models"
  | "autoswitch"
  | "usage"
  | "projects"
  | "accounts"
  | "proxy"
  | "beautify"
  | "settings";

/* ============ 项目管理（数据源：zcode cli db）============ */

/** 项目（会话按 project_id 分组；会话数只计顶层窗口） */
export interface ZcProject {
  id: string;
  directory: string;
  /** 会话数（顶层会话总数，含归档） */
  sessions: number;
  /** 已归档的顶层会话数（活跃 = sessions - archivedSessions） */
  archivedSessions: number;
  /** 对话次数（turn_usage 轮次，含子代理） */
  turns: number;
  /** 模型调用次数 */
  calls: number;
  inputTokens: number;
  outputTokens: number;
  /** 总 token（model_usage 口径，与用量页一致） */
  totalTokens: number;
  timeCreatedMs?: number | null;
  timeUpdatedMs?: number | null;
}

/** 会话（顶层；消耗统计含其全部子代理后代） */
export interface ZcSession {
  id: string;
  projectId: string;
  title: string;
  titleSource: string;
  directory: string;
  taskType: string;
  /** 是否已归档（zcode 任务索引 tasks.archived/deleted 或 time_archived） */
  archived: boolean;
  turns: number;
  calls: number;
  inputTokens: number;
  outputTokens: number;
  totalTokens: number;
  timeCreatedMs: number;
  timeUpdatedMs: number;
  timeArchivedMs?: number | null;
}

/** 批量删除结果 */
export interface ZcDeleteResult {
  deletedSessions: number;
  deletedProjects: number;
  freedRolloutFiles: number;
}

/* ============ ZCode 美化（侵入式改造 app.asar）============ */

/** 美化配置（持久化于 zcode-assistant app data，不写 ZCode 的 setting.json） */
export interface BeautifyConfig {
  /** 是否启用美化 */
  enabled: boolean;
  /** 预设主题 id（midnight/nord/dracula/gruvbox/tokyo-night/rose-pine），none=不套主题 */
  theme?: string;
  /** UI 字体族（覆盖 --font-sans） */
  ui_font?: string;
  /** 等宽字体族（覆盖 --font-mono） */
  mono_font?: string;
  /** 自定义背景色（覆盖 --color-background） */
  bg_color?: string;
  /** 自定义主色调（覆盖 --color-primary） */
  primary_color?: string;
  /** 毛玻璃：表面半透明，透出 Windows acrylic 原生模糊 */
  acrylic?: boolean;
  /** 表面不透明度（0.2–1），毛玻璃或背景图启用时生效，越大越实 */
  surface_opacity?: number;
  /** 背景图（本地绝对路径，应用时复制进 app.asar） */
  bg_image?: string;
  /** 背景图图层不透明度（0.1–1） */
  bg_image_opacity?: number;
}

/** 美化状态 */
export interface BeautifyStatus {
  /** app.asar 当前是否已注入美化 */
  installed: boolean;
  /** 是否已有原始备份（可还原） */
  has_backup: boolean;
  /** 当前配置 */
  config: BeautifyConfig;
  /** ZCode 版本号 */
  zcode_version?: string;
  /** 原始备份对应的 ZCode 版本；与 zcode_version 不一致 = 备份已过期（升级后未重建） */
  backup_version?: string | null;
  /** app.asar 绝对路径 */
  asar_path?: string;
}

/** 预设主题项 */
export interface BeautifyPreset {
  id: string;
  name: string;
}

/** 美化模板（命名配置快照，可一键保存/载入） */
export interface BeautifyTemplate {
  name: string;
  config: BeautifyConfig;
}

/** 配额查询模板（用户可自定义，用于非 Coding Plan 的 provider） */
export interface QuotaTemplate {
  providerKey: string;
  name?: string;
  method?: string; // GET | POST
  url?: string;
  headersJson?: string;
  body?: string;
  totalPath?: string;
  usedPath?: string;
  remainingPath?: string;
  /** 月限额（可选，部分供应商才有）：配置后同一响应里再提取「每月使用额度」桶 */
  monthlyTotalPath?: string;
  monthlyUsedPath?: string;
  monthlyRemainingPath?: string;
  /** 每5小时窗口（可选）：配置后同一响应里再提取「每5小时使用额度」桶 */
  fiveHourTotalPath?: string;
  fiveHourUsedPath?: string;
  fiveHourRemainingPath?: string;
  /** 每周窗口（可选）：配置后同一响应里再提取「每周使用额度」桶 */
  weeklyTotalPath?: string;
  weeklyUsedPath?: string;
  weeklyRemainingPath?: string;
  /** 登录页 URL：供「登录获取 Token」弹内嵌窗口加载 */
  loginUrl?: string;
  /** Token 提取方式：cookie:<名称> | cookie:（留空取完整 cookie 串）| localstorage:<key>[#<dot.path>] */
  tokenSource?: string;
  /** 用量查询方式：token=登录会话 Token | appkey=API Key（默认/旧数据）| coding_plan=Token Plan 内置预设 */
  authMode?: "token" | "appkey" | "coding_plan" | string;
  /** 登录账号（自动填充用；密码另存系统凭证库不回显） */
  loginUsername?: string;
  /** 附加凭据 JSON：智谱团队版 {organizationId,projectId}；火山方舟 {accessKeyId,secretAccessKey} */
  extraJson?: string;
  /** 配额单位："%" → 百分比（值 ≤ 1.0 自动 ×100）；缺省 = 绝对值 */
  unit?: string;
  /** 重置时间 dot path（毫秒/秒时间戳或 ISO 字符串）→ 提取到 bucket.periodEnd */
  resetTimePath?: string;
  /** 每5小时窗口重置时间 dot path */
  fiveHourResetTimePath?: string;
  /** 每周窗口重置时间 dot path */
  weeklyResetTimePath?: string;
  /** 月限额重置时间 dot path */
  monthlyResetTimePath?: string;
}

/** 配额 Token 状态（keyring 是否已存 + 获取时间 + 登录密码是否已保存） */
export interface QuotaTokenStatus {
  hasToken: boolean;
  fetchedAt?: string;
  hasPassword: boolean;
}

/** 自动切换规则 */
export interface AutoSwitchRule {
  id: string;
  name: string;
  kind: "cron" | "drain" | "appstart";
  enabled: boolean;
  timeStart?: string; // "HH:MM"（cron 执行时间）
  timeEnd?: string; // 兼容旧数据，不再使用
  weekdays?: string; // "1,2,3,4,5,6,7"
  fromProvider?: string; // 源供应商 id（留空=任意）
  fromModel?: string; // 源模型（可选，仅展示）
  toProvider: string; // 目标供应商 id
  toModel?: string; // 目标模型（必填，仅展示）
  threshold?: number; // drain 阈值
  priority?: number;
  createdAt: string;
  /** 项目目录（限定：仅该项目为最近对话项目时触发；留空=全部项目） */
  projectDir?: string;
}

/** 自动切换可选项目（当前打开且有具体对话的项目） */
export interface AutoSwitchProject {
  /** 项目目录绝对路径 */
  dir: string;
  /** 目录名（展示用） */
  name: string;
  /** 会话数 */
  sessions: number;
  /** 最近一次对话时间（毫秒） */
  lastActiveMs?: number;
}

/** 自动切换触发方式 */
export type AutoSwitchTrigger = "manual" | "cron" | "drain" | "appstart";

/** 自动切换执行日志 */
export interface AutoSwitchLog {
  id: number;
  ruleId: string;
  ruleName: string;
  triggerType: AutoSwitchTrigger | string;
  success: boolean;
  /** 失败原因 / 成功备注 */
  message?: string;
  createdAt: string;
}

/** 当前 zcode 选中状态（来自 setting.json） */
export interface SelectedState {
  family?: string;
  providerKey?: string;
}

/** 当前供应商可用性检测报告（GET /models 免费探测，0 token） */
export interface HealthReport {
  providerKey: string;
  providerName: string;
  ok: boolean;
  message: string;
  /** 上次真实检测时间（unix 秒） */
  checkedAt: number;
  /** 下次允许自动检测的时间（unix 秒；失败后的冷却截止） */
  nextCheckAt: number;
  /** 连续失败次数（成功后清零，决定退避档位） */
  failCount: number;
  /** true=本次未发起网络请求（冷却中/缺凭据），为缓存或跳过态 */
  skipped: boolean;
}

/* ============ 用量查询 ============ */

/** 聚合维度 */
export type UsageGroupBy = "provider" | "model" | "date";

/** 单次模型调用记录 */
export interface UsageRecord {
  requestId: string;
  startedAt?: string;
  date: string;
  providerId: string;
  modelId: string;
  role?: string;
  querySource?: string;
  inputTokens: number;
  outputTokens: number;
  totalTokens: number;
  cacheReadTokens: number;
  cacheWriteTokens: number;
  durationMs?: number;
  tps?: number;
  finishReason?: string;
  sessionId?: string;
  rawPath?: string;
}

/** 同步扫描结果 */
export interface UsageSyncResult {
  newCount: number;
  /** 对账回收数：zcode 侧已清理、本地顺带删除的记录条数 */
  removedCount: number;
  totalCount: number;
  scannedFiles: number;
  minDate?: string;
  maxDate?: string;
}

/** 筛选项 */
export interface UsageFilters {
  providers: string[];
  models: string[];
  roles: string[];
  minDate?: string;
  maxDate?: string;
  totalRecords: number;
}

/** 整体汇总 */
export interface UsageOverview {
  calls: number;
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheWriteTokens: number;
  totalTokens: number;
  avgTps?: number;
  maxTps?: number;
  minTps?: number;
  avgDurationMs?: number;
}

/** 分组聚合行 */
export interface UsageAggRow {
  key: string;
  label: string;
  calls: number;
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheWriteTokens: number;
  totalTokens: number;
  avgTps?: number;
  maxTps?: number;
  minTps?: number;
  avgDurationMs?: number;
}

/* ============ 版本号 / 自动更新 ============ */

/** 更新策略：auto=应用内下载安装（Windows）；browser=打开浏览器手动下载 */
export type UpdateStrategy = "auto" | "browser";

/** 后端 check_for_updates 返回的更新信息 */
export interface UpdateInfo {
  currentVersion: string;
  latestVersion: string;
  hasUpdate: boolean;
  /** release 页面地址（下载按钮兜底） */
  releaseUrl: string;
  /** 匹配平台 asset 的下载地址，缺省回退 releaseUrl */
  downloadUrl: string;
  /** 截断后的发行说明（Markdown） */
  notes: string;
  /** 发行时间（API 原始字符串） */
  publishedAt: string;
  /** 本次检查的 unix 秒 */
  checkedAt: number;
  /** 检查失败时非空；遇非空 error 视为「无更新，保持沉默」 */
  error?: string;
  updateStrategy: UpdateStrategy;
}

/** 下载进度事件 update_download_progress 的 payload */
export interface DownloadProgress {
  downloaded: number;
  total: number;
}
