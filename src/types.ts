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
  source: "coding-plan" | "template" | "unknown";
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

/** 主窗口视图 id */
export type ViewId =
  | "dashboard"
  | "models"
  | "autoswitch"
  | "usage"
  | "accounts"
  | "proxy"
  | "settings";

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
}

/** 自动切换规则 */
export interface AutoSwitchRule {
  id: string;
  name: string;
  kind: "cron" | "drain";
  enabled: boolean;
  timeStart?: string; // "HH:MM"
  timeEnd?: string; // "HH:MM"
  weekdays?: string; // "1,2,3,4,5"
  family?: string;
  fromProvider?: string;
  toProvider: string;
  threshold?: number; // drain 阈值
  priority?: number;
  createdAt: string;
}

/** 当前 zcode 选中状态（来自 setting.json） */
export interface SelectedState {
  family?: string;
  providerKey?: string;
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
