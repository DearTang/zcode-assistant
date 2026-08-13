/**
 * Tauri command 调用封装 —— 统一入口，带类型
 * 后端 command 名与 src-tauri/src/commands/*.rs 的 #[tauri::command] 对应。
 */
import { invoke } from "@tauri-apps/api/core";
import { emit, listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AccountMeta,
  AutoSwitchRule,
  DownloadProgress,
  ModelSpec,
  ProxyConfig,
  QuotaBucket,
  QuotaTemplate,
  QuotaOverview,
  UpdateInfo,
  UsageAggRow,
  UsageFilters,
  UsageGroupBy,
  UsageOverview,
  UsageRecord,
  UsageSyncResult,
  ZcodeConfig,
  ZcodeSetting,
} from "./types";

/* ============ 窗口控制（M1）============ */
export const win = {
  showMain: () => invoke<void>("show_main_window"),
  showFloatBall: () => invoke<void>("show_float_ball"),
  hideFloatBall: () => invoke<void>("hide_float_ball"),
  toggleFloatBall: () => invoke<void>("toggle_float_ball"),
  showFloatPanel: () => invoke<void>("show_float_panel"),
  hideFloatPanel: () => invoke<void>("hide_float_panel"),
  toggleFloatPanel: () => invoke<void>("toggle_float_panel"),
  quitApp: () => invoke<void>("quit_app"),
  /** 用前端 canvas 绘制的 RGBA 像素替换托盘图标 */
  setTrayIcon: (rgba: number[], width: number, height: number) =>
    invoke<void>("set_tray_icon", { rgba, width, height }),
};

/** 前端日志桥：打到后端 stdout，便于在终端追踪悬浮窗交互链路（失败静默，不影响业务） */
export const feLog = (
  msg: string,
  level: "info" | "warn" | "error" = "info"
) => {
  // eslint-disable-next-line no-console
  console.log(`[FE:${level}] ${msg}`);
  invoke<void>("fe_log", { level, msg }).catch(() => {});
};

/* ============ zcode 配置读写（M2）============ */
export const zcode = {
  /** 读取 ~/.zcode/v2/config.json（apiKey 已脱敏） */
  getConfig: () => invoke<ZcodeConfig>("get_zcode_config"),
  /** 读取 setting.json */
  getSetting: () => invoke<ZcodeSetting>("get_zcode_setting"),
  /** 切换当前选中的 provider/model */
  selectProvider: (family: string, providerKey: string) =>
    invoke<void>("select_provider", { family, providerKey }),
  /** 探测 zcode 安装路径与运行状态 */
  probe: () =>
    invoke<{ exePath: string | null; running: boolean; configDir: string }>(
      "probe_zcode"
    ),
  /** 重启 zcode（kill + relaunch） */
  restartZcode: () => invoke<void>("restart_zcode"),
};

/* ============ 模型管理（M2）============ */
export const models = {
  /** 从智谱 /paas/v4/models 拉取可用模型列表 */
  fetchAvailable: (providerKey: string) =>
    invoke<ModelSpec[]>("fetch_available_models", { providerKey }),
  /** 内置模型规格表（兜底上下文长度） */
  builtinSpecs: () => invoke<ModelSpec[]>("builtin_model_specs"),
  /** 新增自定义 provider（providerId 为供应商标识，创建后不可改） */
  addProvider: (
    name: string,
    kind: string,
    baseURL: string,
    apiKey: string,
    providerId: string
  ) =>
    invoke<string>("add_provider", {
      name,
      kind,
      baseURL,
      apiKey,
      providerId,
    }),
  /** 删除 provider */
  removeProvider: (providerKey: string) =>
    invoke<void>("remove_provider", { providerKey }),
  /** 更新 provider 的 name / kind / baseURL / apiKey（传 undefined 的字段不变） */
  updateProvider: (
    providerKey: string,
    patch: {
      name?: string;
      kind?: string;
      baseURL?: string;
      apiKey?: string;
    }
  ) =>
    invoke<void>("update_provider", {
      providerKey,
      name: patch.name,
      kind: patch.kind,
      baseUrl: patch.baseURL,
      apiKey: patch.apiKey,
    }),
  /** 更新某 model 的上下文 / 输出上限（写回 config.json） */
  updateModelLimit: (
    providerKey: string,
    modelName: string,
    context?: number,
    output?: number
  ) =>
    invoke<void>("update_model_limit", {
      providerKey,
      modelName,
      context,
      output,
    }),
  /** 批量把拉取到的模型写入 provider.models（合并 + 设 limit） */
  applyModels: (providerKey: string, specs: ModelSpec[]) =>
    invoke<number>("apply_models", { providerKey, specs }),
  /** 切换 provider 启用状态 */
  setProviderEnabled: (providerKey: string, enabled: boolean) =>
    invoke<void>("set_provider_enabled", { providerKey, enabled }),
  /** 切换单个 model 启用状态 */
  setModelEnabled: (
    providerKey: string,
    modelName: string,
    enabled: boolean
  ) => invoke<void>("set_model_enabled", { providerKey, modelName, enabled }),
  /** 删除 provider 下的单个 model */
  removeModel: (providerKey: string, modelName: string) =>
    invoke<void>("remove_model", { providerKey, modelName }),
  /** 查看某 provider 的明文 apiKey（不脱敏） */
  getApiKey: (providerKey: string) =>
    invoke<string>("get_provider_api_key", { providerKey }),
  /** 标记/取消标记 provider 为 Coding Plan 订阅 */
  setCodingPlan: (providerKey: string, enabled: boolean) =>
    invoke<void>("set_provider_coding_plan", { providerKey, enabled }),
  /** 列出所有被标记为 Coding Plan 的 provider key */
  listCodingPlan: () => invoke<string[]>("list_coding_plan_providers"),
  /** 测试 baseURL + apiKey 能否连通（调 /models，不落盘）。用于「添加供应商」弹窗的测试按钮 */
  testConnection: (baseURL: string, apiKey: string, kind: string) =>
    invoke<{ ok: boolean; message: string; modelCount?: number }>(
      "test_provider_connection",
      { baseUrl: baseURL, apiKey, kind }
    ),
};

/* ============ 配额（M3）============ */
export const quota = {
  /** 查询 Coding Plan 配额（解密 token + 调 zcode.z.ai） */
  getCodingPlan: () => invoke<QuotaOverview>("get_coding_plan_quota"),
  /** 按模板查询某 provider 配额 */
  getByTemplate: (providerKey: string) =>
    invoke<QuotaOverview>("get_template_quota", { providerKey }),
  /** 按供应商查配额（统一入口：智谱走内置接口，其余走用量模板） */
  getProviderQuota: (providerKey: string) =>
    invoke<QuotaOverview>("get_provider_quota", { providerKey }),
};

/* ============ 账号（M4）============ */
export const accounts = {
  list: () => invoke<AccountMeta[]>("list_accounts"),
  capture: (label: string) => invoke<AccountMeta>("capture_account", { label }),
  use: (id: string) => invoke<void>("switch_account", { id }),
  remove: (id: string) => invoke<void>("remove_account", { id }),
  rename: (id: string, label: string) =>
    invoke<void>("rename_account", { id, label }),
  current: () => invoke<AccountMeta | null>("current_account"),
};

/* ============ 代理（M4）============ */
export const proxy = {
  get: () => invoke<ProxyConfig>("get_proxy"),
  set: (cfg: ProxyConfig, password?: string) =>
    invoke<void>("set_proxy", { cfg, password }),
  test: () => invoke<{ ok: boolean; latencyMs?: number; error?: string }>(
    "test_proxy"
  ),
};

/* ============ 自动切换规则（M5）============ */
export const autoswitch = {
  listRules: () => invoke<AutoSwitchRule[]>("list_rules"),
  upsertRule: (rule: AutoSwitchRule) =>
    invoke<string>("upsert_rule", { rule }),
  deleteRule: (id: string) => invoke<void>("delete_rule", { id }),
};

/* ============ 配额模板（M3-2）============ */
export const templates = {
  list: () => invoke<QuotaTemplate[]>("list_templates"),
  get: (providerKey: string) =>
    invoke<QuotaTemplate | null>("get_quota_template", { providerKey }),
  upsert: (template: QuotaTemplate) =>
    invoke<void>("upsert_template", { template }),
  /** 删除某 provider 的配额查询模板 */
  remove: (providerKey: string) =>
    invoke<void>("remove_template", { providerKey }),
};

/* ============ 用量查询（解析 zcode rollout 统计）============ */
export interface UsageQuery {
  from?: string;
  to?: string;
  provider?: string;
  model?: string;
  role?: string;
}
export const usage = {
  /** 同步：默认仅增量解析最近 30 天；full=true 回填全部历史 */
  sync: (full = false) => invoke<UsageSyncResult>("usage_sync", { full }),
  /** 筛选项（供应商 / 模型 / 角色 + 日期范围 + 总条数） */
  filters: () => invoke<UsageFilters>("usage_filters"),
  /** 整体汇总（随筛选条件） */
  overview: (q: UsageQuery) =>
    invoke<UsageOverview>("usage_overview", {
      from: q.from,
      to: q.to,
      provider: q.provider,
      model: q.model,
      role: q.role,
    }),
  /** 分组聚合（按供应商 / 模型 / 日期） */
  aggregate: (groupBy: UsageGroupBy, q: UsageQuery) =>
    invoke<UsageAggRow[]>("usage_aggregate", {
      groupBy,
      from: q.from,
      to: q.to,
      provider: q.provider,
      model: q.model,
      role: q.role,
    }),
  /** 明细记录（分页，按时间倒序） */
  records: (q: UsageQuery, limit = 200, offset = 0) =>
    invoke<UsageRecord[]>("usage_records", {
      from: q.from,
      to: q.to,
      provider: q.provider,
      model: q.model,
      role: q.role,
      limit,
      offset,
    }),
};

/* ============ 版本号 ============ */
export const app = {
  /** 当前应用版本号（取自 Cargo.toml，编译期确定） */
  getVersion: () => invoke<string>("get_app_version"),
};

/* ============ 自动更新（参考 myshell：自定义三命令，不集成 updater 插件）============ */
export const updater = {
  /** 检查是否有新版本（GET 发行版列表 + semver 比较）。失败编码进 UpdateInfo.error，不抛 */
  checkForUpdates: () => invoke<UpdateInfo>("check_for_updates"),
  /** 流式下载安装器到应用配置目录固定文件名，返回绝对路径 */
  downloadUpdate: (url: string) => invoke<string>("download_update", { url }),
  /** 启动下载好的安装器并退出本进程（让安装器替换运行中的文件） */
  installUpdate: (path: string) => invoke<void>("install_update", { path }),
  /** 在系统默认浏览器打开发行版页面（非 Windows 平台更新兜底） */
  openReleasePage: (url: string) => invoke<void>("open_release_page", { url }),
};

/** 订阅下载进度事件（update_download_progress） */
export const onUpdateDownloadProgress = (
  cb: (p: DownloadProgress) => void
): Promise<UnlistenFn> =>
  listen<DownloadProgress>("update_download_progress", (e) => cb(e.payload));

/* ============ 导入配置 ============ */
export type ImportStatus = "success" | "updated" | "duplicate" | "failed";
export interface ImportResult {
  name: string;
  kind: string;
  baseUrl: string;
  models: string[];
  source: string;
  status: ImportStatus;
  /** success: 新 key；duplicate: 命中的已有 key；failed: 空 */
  providerKey: string;
  message: string;
}
export const importer = {
  from: (source: string, path?: string) =>
    invoke<ImportResult[]>("import_providers_from", { source, path }),
  /** 弹出系统文件选择器，返回选中的文件路径（用户取消返回 null） */
  pickFile: (source: string, defaultPath?: string) =>
    invoke<string | null>("pick_config_file", { source, defaultPath }),
};

/* ============ 事件总线订阅 ============ */
export const events = {
  /** 配额更新广播（数据源：Dashboard 每 5s 查询后发出） */
  onQuotaUpdated: (cb: (q: QuotaOverview) => void): Promise<UnlistenFn> =>
    listen<QuotaOverview>("quota://updated", (e) => cb(e.payload)),
  /** 广播一次配额更新（Dashboard 作为唯一查询源调用） */
  emitQuotaUpdated: (q: QuotaOverview): Promise<void> =>
    emit("quota://updated", q),
  /** 请求重新查询配额（刷新按钮 / 托盘刷新菜单转发，由 Dashboard 统一发起） */
  emitRefreshRequested: (): Promise<void> =>
    emit("quota://refresh-requested", null),
  /** 监听刷新请求（Dashboard 注册，统一发起查询） */
  onRefreshRequested: (cb: () => void): Promise<UnlistenFn> =>
    listen("quota://refresh-requested", () => cb()),
  /** 模型切换广播 */
  onModelSwitched: (
    cb: (p: { providerKey: string }) => void
  ): Promise<UnlistenFn> =>
    listen<{ providerKey: string }>("model://switched", (e) => cb(e.payload)),
  /** 后端请求重启 zcode 确认 */
  onRestartRequested: (
    cb: (p: { reason: string }) => void
  ): Promise<UnlistenFn> =>
    listen<{ reason: string }>("zcode://restart-requested", (e) =>
      cb(e.payload)
    ),
};

/** 将 apiKey 脱敏为展示用字符串 */
export function maskApiKey(key?: string): string {
  if (!key) return "—";
  if (key.length <= 16) return "•".repeat(Math.max(4, key.length - 4));
  return `${key.slice(0, 6)}••••${key.slice(-4)}`;
}

/** 字节数/配额数值的人类可读格式 */
export function formatUnits(n: number, unit?: string): string {
  if (!Number.isFinite(n)) return "—";
  if (Math.abs(n) >= 1_000_000)
    return `${(n / 1_000_000).toFixed(2)}M ${unit ?? ""}`.trim();
  if (Math.abs(n) >= 1_000)
    return `${(n / 1_000).toFixed(1)}K ${unit ?? ""}`.trim();
  return `${n} ${unit ?? ""}`.trim();
}

/** 按名称关键词选取配额 bucket（如 "5小时"、"每周"） */
export function pickBucketByName(
  q: QuotaOverview | null | undefined,
  needle: string
): QuotaBucket | undefined {
  return q?.buckets?.find((b) => b.name.includes(needle));
}

/** 按已用占比返回用量色：<70% 绿 / ≥70% 黄 / ≥90% 红 */
export function usageColor(usedPct: number): string {
  if (usedPct >= 90) return "#EF4444";
  if (usedPct >= 70) return "#F59E0B";
  return "#22C55E";
}
