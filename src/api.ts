/**
 * Tauri command 调用封装 —— 统一入口，带类型
 * 后端 command 名与 src-tauri/src/commands/*.rs 的 #[tauri::command] 对应。
 */
import { invoke } from "@tauri-apps/api/core";
import { emit, listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AccountMeta,
  AppPrefs,
  AutoSwitchLog,
  AutoSwitchProject,
  AutoSwitchRule,
  BeautifyConfig,
  BeautifyPreset,
  BeautifyStatus,
  BeautifyTemplate,
  DownloadProgress,
  HealthReport,
  ModelSpec,
  ModelRetryConfig,
  ProxyConfig,
  QuotaBucket,
  QuotaTemplate,
  QuotaTokenStatus,
  QuotaOverview,
  UpdateInfo,
  UsageAggRow,
  UsageDisplayMode,
  UsageFilters,
  UsageGroupBy,
  UsageOverview,
  UsageRecord,
  UsageSyncResult,
  ZcDeleteResult,
  ZcProject,
  ZcSession,
  ZcodeConfig,
  ZcodeSetting,
} from "./types";

/* ============ 窗口控制（M1）============ */
export const win = {
  showMain: () => invoke<void>("show_main_window"),
  showFloatPanel: () => invoke<void>("show_float_panel"),
  hideFloatPanel: () => invoke<void>("hide_float_panel"),
  toggleFloatPanel: () => invoke<void>("toggle_float_panel"),
  /** 固定/取消固定展开面板（固定 = 鼠标移开、点击窗口外都不收起，单击悬浮球触发） */
  toggleFloatPanelPin: () => invoke<void>("toggle_float_panel_pin"),
  quitApp: () => invoke<void>("quit_app"),
  /** 覆盖启动：结束当前进程并重新拉起（单实例弹窗用） */
  restartApp: () => invoke<void>("restart_app"),
  /** 用前端 canvas 绘制的 RGBA 像素替换托盘图标 */
  setTrayIcon: (rgba: number[], width: number, height: number) =>
    invoke<void>("set_tray_icon", { rgba, width, height }),
};

/* ============ 应用偏好（悬浮球显隐 / 用量展示方案）============ */
export const prefs = {
  /** 读取当前偏好（缺省：悬浮球显示、展示已用量） */
  get: () => invoke<AppPrefs>("get_prefs"),
  /** 显示/隐藏悬浮球（持久化，重启后仍生效；后端同时广播 prefs://updated） */
  setFloatBallVisible: (visible: boolean) =>
    invoke<void>("set_float_ball_visible", { visible }),
  /** 设置用量展示方案：used=已用量 / remaining=剩余用量 */
  setUsageDisplay: (mode: UsageDisplayMode) =>
    invoke<void>("set_usage_display", { mode }),
  /** 切换后重启 ZCode（开=自动切换直改配置并自动重启；关=自动切换免重启模拟，默认开） */
  setSwitchRestart: (enabled: boolean) =>
    invoke<void>("set_switch_restart_zcode", { enabled }),
  /** 开机自启动（注册 / 注销系统自启项，重启后生效） */
  setAutostart: (enabled: boolean) => invoke<void>("set_autostart", { enabled }),
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

/** 在系统默认浏览器打开 http(s) 链接（复用后端 open_release_page 的零依赖实现） */
export const openUrl = (url: string) =>
  invoke<void>("open_release_page", { url });

/* ============ zcode 配置读写（M2）============ */
export const zcode = {
  /** 读取 ~/.zcode/v2/config.json（apiKey 已脱敏） */
  getConfig: () => invoke<ZcodeConfig>("get_zcode_config"),
  /** 读取 setting.json */
  getSetting: () => invoke<ZcodeSetting>("get_zcode_setting"),
  /** 探测 zcode 安装路径与运行状态 */
  probe: () =>
    invoke<{ exePath: string | null; running: boolean; configDir: string }>(
      "probe_zcode"
    ),
  /** 重启 zcode（kill + relaunch） */
  restartZcode: () => invoke<void>("restart_zcode"),
  /** 触发 ZCode「Developer: Reload Window」温和重读配置（不杀进程，保留打开的文件） */
  reloadWindow: () => invoke<void>("reload_zcode_window"),
  /**
   * 键盘模拟切换 ZCode 当前会话的模型（免重启，下一轮对话生效）。
   * 传 providerKey，后端自动计算菜单中的位置（套餐模型偏移 + 供应商索引）；
   * modelKey 仅 builtin 套餐目标需要（定位套餐内具体模型）。
   */
  switchModel: (providerKey: string, modelKey?: string) =>
    invoke<void>("switch_zcode_model", { providerKey, modelKey }),
  /** 模型调用重试配置（ZCODE_MODEL_RETRY_* 用户环境变量；null = 跟随 ZCode 默认） */
  getRetryConfig: () => invoke<ModelRetryConfig>("get_model_retry_config"),
  /** 写入重试配置（reg add/delete + 广播环境变更；ZCode 运行中会弹重启确认） */
  setRetryConfig: (config: ModelRetryConfig) =>
    invoke<ModelRetryConfig>("set_model_retry_config", { config }),
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
  /** 拖拽排序后重排 provider 顺序（写回 config.json） */
  reorderProviders: (orderedKeys: string[]) =>
    invoke<void>("reorder_providers", { orderedKeys }),
  /** 拖拽排序后重排某 provider 下模型顺序（写回 config.json） */
  reorderModels: (providerKey: string, orderedNames: string[]) =>
    invoke<void>("reorder_models", { providerKey, orderedNames }),
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
  /** 设置/取消主供应商（全局唯一；总览 / 悬浮窗 / 托盘展示其配额） */
  setPrimary: (providerKey: string, enabled: boolean) =>
    invoke<void>("set_provider_primary", { providerKey, enabled }),
  /** 取主供应商 key（未设置为 null，展示回退自动识别的智谱 Coding Plan） */
  getPrimary: () => invoke<string | null>("get_primary_provider"),
  /**
   * 启动引导：best-effort 选一个主供应商（已设过且仍存在 → 不动；否则优先智谱
   * Coding Plan 内置；再否则第一个 enabled 且带模型的自定义供应商；都没有返回 null），
   * 并同步写入 setting.json 的 family 选中（bigmodel），保证总览/悬浮窗有数据源。
   */
  bootstrapPrimary: () => invoke<string | null>("bootstrap_primary"),
  /** 测试 baseURL + apiKey 能否连通（调 /models，不落盘）。用于「添加供应商」弹窗的测试按钮 */
  testConnection: (baseURL: string, apiKey: string, kind: string) =>
    invoke<{ ok: boolean; message: string; modelCount?: number }>(
      "test_provider_connection",
      { baseUrl: baseURL, apiKey, kind }
    ),
};

/* ============ 配额（M3）============ */
export const quota = {
  /** 总览配额（主供应商优先，未设置则自动识别智谱 Coding Plan；悬浮窗 / 托盘共用） */
  getOverview: () => invoke<QuotaOverview>("get_overview_quota"),
  /** 查询 Coding Plan 配额（自动识别订阅 provider） */
  getCodingPlan: () => invoke<QuotaOverview>("get_coding_plan_quota"),
  /** 按模板查询某 provider 配额 */
  getByTemplate: (providerKey: string) =>
    invoke<QuotaOverview>("get_template_quota", { providerKey }),
  /** 按供应商查配额（统一入口：Token Plan 供应商按 baseURL 自动识别并用其 API Key + Base URL
   *  查询，其余走用量模板） */
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
  /** 按传入顺序重排规则优先级（拖拽后调用） */
  reorder: (ids: string[]) => invoke<void>("reorder_rules", { ids }),
  /** 可限定项目列表（当前打开且有具体对话的项目，按最近活跃倒序） */
  projects: () => invoke<AutoSwitchProject[]>("autoswitch_projects"),
  /** 手动测试：跳过触发条件立即执行切换，返回结果说明（同时记入执行日志） */
  testRule: (id: string) => invoke<string>("test_rule", { id }),
  /** 执行日志（最近 200 条，时间倒序） */
  logs: () => invoke<AutoSwitchLog[]>("autoswitch_logs"),
};

/* ============ 配额模板（M3-2）============ */
export const templates = {
  list: () => invoke<QuotaTemplate[]>("list_templates"),
  /** 内置预设模板：Token Plan 额度 6 家（Kimi/智谱/智谱团队/MiniMax/ZenMux/火山，preset:cp- 前缀）
   *  + 余额查询 5 家（DeepSeek/StepFun/SiliconFlow×2/OpenRouter），端点均取自 cc-switch 实测实现 */
  builtin: () => invoke<QuotaTemplate[]>("builtin_quota_templates"),
  get: (providerKey: string) =>
    invoke<QuotaTemplate | null>("get_quota_template", { providerKey }),
  upsert: (template: QuotaTemplate) =>
    invoke<void>("upsert_template", { template }),
  /** 删除某 provider 的配额查询模板 */
  remove: (providerKey: string) =>
    invoke<void>("remove_template", { providerKey }),
};

/* ============ 配额 Token 获取（弹登录窗，keyring 存储）============ */
export const quotaToken = {
  /** 弹内嵌登录窗口获取 token（需模板已配置 loginUrl / tokenSource） */
  startLogin: (providerKey: string) =>
    invoke<void>("start_quota_token_login", { providerKey }),
  /** token 状态：是否已获取 + 获取时间 */
  status: (providerKey: string) =>
    invoke<QuotaTokenStatus>("quota_token_status", { providerKey }),
  /** 读取明文 token（UI「显示」核对用，未获取返回 null） */
  value: (providerKey: string) =>
    invoke<string | null>("get_quota_token_value", { providerKey }),
  /** 手动写入 token：从浏览器 DevTools 等复制后粘贴到 UI 保存 */
  set: (providerKey: string, token: string) =>
    invoke<void>("set_quota_token", { providerKey, token }),
  /** 清除已存的 token */
  clear: (providerKey: string) =>
    invoke<void>("clear_quota_token", { providerKey }),
  /** 保存登录密码（自动填充用，keyring 存储；空串=删除） */
  setPassword: (providerKey: string, password: string) =>
    invoke<void>("set_quota_login_password", { providerKey, password }),
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
  /** 供应商别名映射（provider_id -> 可读名，解析自 transcript），用于把 UUID 显示成真名 */
  providerLabels: () => invoke<Record<string, string>>("usage_provider_labels"),
};

/* ============ 项目管理（读写 zcode cli db）============ */
export const projects = {
  /** 全部项目（含会话数 / 对话次数 / token 汇总 / 时间），按最近活跃倒序 */
  list: () => invoke<ZcProject[]>("zc_projects"),
  /** 某项目下的顶层会话列表（消耗含子代理后代） */
  sessions: (projectId: string) =>
    invoke<ZcSession[]>("zc_sessions", { projectId }),
  /** 修改会话名称（title_source 标记 custom，zcode 不再自动覆盖） */
  renameSession: (sessionId: string, title: string) =>
    invoke<void>("zc_rename_session", { sessionId, title }),
  /** 归档会话（写 time_archived + 任务索引 archived=1，zcode 会话列表隐藏，可恢复） */
  archiveSession: (sessionId: string) =>
    invoke<void>("zc_archive_session", { sessionId }),
  /** 恢复归档会话（清任务索引与会话库的归档标记，回 zcode 会话列表继续对话） */
  restoreSession: (sessionId: string) =>
    invoke<void>("zc_restore_session", { sessionId }),
  /** 归档整个项目（批量归档其全部活跃顶层会话），返回本次归档的会话数 */
  archiveProject: (projectId: string) =>
    invoke<number>("zc_archive_project", { projectId }),
  /** 恢复整个项目（清该项目全部会话的归档标记），返回恢复的会话数 */
  restoreProject: (projectId: string) =>
    invoke<number>("zc_restore_project", { projectId }),
  /** 批量删除会话 / 项目（级联删除消息与用量，连带清理 rollout 与本地用量记录） */
  delete: (sessionIds: string[], projectIds: string[]) =>
    invoke<ZcDeleteResult>("zc_delete", { sessionIds, projectIds }),
};

/* ============ ZCode 美化（侵入式改造 app.asar）============ */
export const beautify = {
  /** 美化状态：是否已注入 / 有无备份 / 当前配置 / ZCode 版本 */
  getStatus: () => invoke<BeautifyStatus>("get_beautify_status"),
  /** 可选预设主题列表 */
  getPresets: () => invoke<BeautifyPreset[]>("get_beautify_presets"),
  /** 弹出系统文件选择器挑选背景图（取消返回 null） */
  pickImage: () => invoke<string | null>("pick_beautify_image"),
  /** 读取背景图 base64 data URL 供预览（>8MB 或格式不支持返回 null） */
  readImagePreview: (path: string) =>
    invoke<string | null>("read_beautify_image_preview", { path }),
  /** 应用美化（备份→原地补丁→替换 app.asar），完成后后端会请求重启 zcode */
  apply: (config: BeautifyConfig) =>
    invoke<void>("apply_beautify", { config }),
  /** 还原官方 app.asar，完成后后端会请求重启 zcode */
  restore: () => invoke<void>("restore_beautify"),
  /** 全部美化模板列表 */
  listTemplates: () => invoke<BeautifyTemplate[]>("list_beautify_templates"),
  /** 保存模板（同名覆盖），返回最新列表 */
  saveTemplate: (name: string, config: BeautifyConfig) =>
    invoke<BeautifyTemplate[]>("save_beautify_template", { name, config }),
  /** 删除模板，返回最新列表 */
  deleteTemplate: (name: string) =>
    invoke<BeautifyTemplate[]>("delete_beautify_template", { name }),
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
/** 预览：解析出的待导入 provider（未写入），供弹窗勾选 */
export interface ProviderPreview {
  /** 解析出的条目 id（同一次解析内唯一，作为勾选 / 导入过滤的标识） */
  id: string;
  name: string;
  kind: string;
  baseUrl: string;
  models: string[];
  hasApiKey: boolean;
  /** 命中的已有 provider key（导入时将覆盖更新该条） */
  duplicateOf?: string | null;
}
/** 目录匹配的单个模型（真实上下文） */
export interface ResolvedModel {
  id: string;
  context: number;
  /** 输出上限（仅内置规格表命中时有） */
  output?: number | null;
}
/** 上下文预解析结果：unmatched 需弹窗让用户逐个确认 */
export interface ResolveContextsResult {
  matched: ResolvedModel[];
  unmatched: string[];
}
export const importer = {
  /** 预览导入内容：解析来源配置并标记覆盖关系，不执行任何写入 */
  preview: (source: string, path?: string) =>
    invoke<ProviderPreview[]>("preview_providers_from", { source, path }),
  /** 预解析导入模型的上下文（「重新获取上下文」开启、导入确认前调用），不写入 */
  resolveContexts: (source: string, path?: string, selected?: string[]) =>
    invoke<ResolveContextsResult>("resolve_import_contexts", {
      source,
      path,
      selected,
    }),
  /**
   * 执行导入；selected=预览弹窗勾选的条目 id（不传=全部，向后兼容）；
   * refetchContext=「重新获取上下文」开关（开启时命中的模型覆盖为真实值，
   * 未命中的用 contextOverrides 携带的用户确认值）
   */
  from: (
    source: string,
    path?: string,
    selected?: string[],
    refetchContext?: boolean,
    contextOverrides?: Record<string, number>
  ) =>
    invoke<ImportResult[]>("import_providers_from", {
      source,
      path,
      selected,
      refetchContext,
      contextOverrides,
    }),
  /** 弹出系统文件选择器，返回选中的文件路径（用户取消返回 null） */
  pickFile: (source: string, defaultPath?: string) =>
    invoke<string | null>("pick_config_file", { source, defaultPath }),
};

/* ============ 反向同步（zcode → cc-switch，app_type='opencode'）============ */
export type ExportStatus = "success" | "updated" | "failed";
/** 预览：待导出的 zcode provider（未写入），供弹窗勾选 */
export interface ExportPreview {
  /** zcode provider key（勾选 / 导出过滤的标识） */
  id: string;
  name: string;
  baseUrl: string;
  modelCount: number;
  hasApiKey: boolean;
  enabled: boolean;
  /** 目标侧命中的已有 key（导出时将覆盖更新该条） */
  duplicateOf?: string | null;
}
export interface ExportResult {
  name: string;
  status: ExportStatus;
  targetKey: string;
  message: string;
}
export interface ExportOutcome {
  results: ExportResult[];
  /** 非致命警告（如 cc-switch 正在运行需重启后查看） */
  warning?: string | null;
}
export const exporter = {
  /** 预览导出内容：解析 zcode provider 并标记 cc-switch 侧覆盖关系，不执行任何写入 */
  preview: () => invoke<ExportPreview[]>("export_preview"),
  /** 执行导出到 cc-switch（opencode 供应商组）；selected=预览弹窗勾选的条目 id（不传=全部） */
  to: (selected?: string[]) =>
    invoke<ExportOutcome>("export_providers_to", { selected }),
};

/* ============ 当前模型可用性检测 ============ */
export const health = {
  /**
   * 检测当前选中供应商可用性（GET /models，0 token）。
   * force=false 时冷却期内直接返回缓存、不发网络请求；force=true 绕过冷却立即探测。
   */
  check: (force = false) =>
    invoke<HealthReport>("check_current_health", { force }),
  /** 手动检测指定供应商连接（模型卡片 ⚡）：绕过冷却直接探测，结果由调用方 toast 通知 */
  checkProvider: (providerKey: string) =>
    invoke<HealthReport>("check_provider_health", { providerKey }),
};

/* ============ 事件总线订阅 ============ */
export const events = {
  /** 配额更新广播（数据源：主窗口 App 全局轮询，每 5s 查询后发出） */
  onQuotaUpdated: (cb: (q: QuotaOverview) => void): Promise<UnlistenFn> =>
    listen<QuotaOverview>("quota://updated", (e) => cb(e.payload)),
  /** 广播一次配额更新（App 作为唯一查询源调用） */
  emitQuotaUpdated: (q: QuotaOverview): Promise<void> =>
    emit("quota://updated", q),
  /** 请求重新查询配额（刷新按钮 / 托盘刷新菜单转发，由 App 统一发起） */
  emitRefreshRequested: (): Promise<void> =>
    emit("quota://refresh-requested", null),
  /** 监听刷新请求（App 注册，统一发起查询） */
  onRefreshRequested: (cb: () => void): Promise<UnlistenFn> =>
    listen("quota://refresh-requested", () => cb()),
  /** 偏好变更广播（悬浮球显隐 / 用量展示方案；后端在 set_* 命令与托盘切换时发出） */
  onPrefsUpdated: (cb: (p: AppPrefs) => void): Promise<UnlistenFn> =>
    listen<AppPrefs>("prefs://updated", (e) => cb(e.payload)),
  /** 配额 Token 获取成功 / 清除 广播（登录窗口提取到 token 后发出） */
  onTokenUpdated: (
    cb: (p: { providerKey: string }) => void
  ): Promise<UnlistenFn> =>
    listen<{ providerKey: string }>("quota://token-updated", (e) =>
      cb(e.payload)
    ),
  /** 悬浮球鼠标离开 → 通知展开面板延迟隐藏（跨窗口 hover 联动） */
  emitBallLeave: (): Promise<void> => emit("float://ball-leave", null),
  onBallLeave: (cb: () => void): Promise<UnlistenFn> =>
    listen("float://ball-leave", () => cb()),
  /** 面板固定态广播（后端固定/收起面板时发出；悬浮球指示灯与面板自身监听联动） */
  onPanelPinned: (cb: (pinned: boolean) => void): Promise<UnlistenFn> =>
    listen<boolean>("float://panel-pinned", (e) => cb(e.payload)),
  /** 模型切换广播 */
  onModelSwitched: (
    cb: (p: { providerKey: string }) => void
  ): Promise<UnlistenFn> =>
    listen<{ providerKey: string }>("model://switched", (e) => cb(e.payload)),
  /** 当前供应商可用性检测更新（后端每次检测后广播） */
  onHealthUpdated: (cb: (r: HealthReport) => void): Promise<UnlistenFn> =>
    listen<HealthReport>("health://updated", (e) => cb(e.payload)),
  /** 后端请求重启 zcode 确认 */
  onRestartRequested: (
    cb: (p: { reason: string }) => void
  ): Promise<UnlistenFn> =>
    listen<{ reason: string }>("zcode://restart-requested", (e) =>
      cb(e.payload)
    ),
  /** 第二个应用实例尝试启动（单实例插件在已有实例中广播；新实例已自动退出） */
  onSecondInstance: (cb: () => void): Promise<UnlistenFn> =>
    listen("app://second-instance", () => cb()),
};

/** 将 apiKey 脱敏为展示用字符串 */
export function maskApiKey(key?: string): string {
  if (!key) return "—";
  if (key.length <= 16) return "•".repeat(Math.max(4, key.length - 4));
  return `${key.slice(0, 6)}••••${key.slice(-4)}`;
}

/** 字节数/配额数值的人类可读格式（K → M → B 逐级进位）；百分比取整显示 */
export function formatUnits(n: number, unit?: string): string {
  if (!Number.isFinite(n)) return "—";
  if (unit === "%") return `${Math.round(n)}%`;
  if (Math.abs(n) >= 1_000_000_000)
    return `${(n / 1_000_000_000).toFixed(2)}B ${unit ?? ""}`.trim();
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

/** 取双环展示的两个 bucket（悬浮球 / 悬浮面板 / 托盘图标共用）：
 *  智谱套餐取「每5小时 / 每周」；其余供应商（用量模板）无此命名，
 *  回退用前两个 bucket，保证模板供应商也有环可显 */
export function pickRingBuckets(q: QuotaOverview | null | undefined): {
  b5: QuotaBucket | undefined;
  bW: QuotaBucket | undefined;
} {
  const b5 = pickBucketByName(q, "5小时");
  const bW = pickBucketByName(q, "每周");
  if (b5 || bW) return { b5, bW };
  return { b5: q?.buckets?.[0], bW: q?.buckets?.[1] };
}

/** 按已用占比返回用量色：<70% 绿 / ≥70% 黄 / ≥90% 红 */
export function usageColor(usedPct: number): string {
  if (usedPct >= 90) return "#EF4444";
  if (usedPct >= 70) return "#F59E0B";
  return "#22C55E";
}

/* ============ ZCode 重载防抖 ============ */
/**
 * 防抖触发 ZCode 重启（kill + relaunch），让外部改动的 config.json 被重新读取。
 * 连续的配置变更（加/删/改 provider、模型、切账号）只在停顿后重启一次，
 * 避免每次小改都重启 ZCode。失败静默（用户仍可手动重启）。
 *
 * 说明：曾尝试用键盘模拟触发「Reload Window」（温和），但 ZCode 命令面板入口
 * 非 F1（实测 F1 无反应，命令行入口是 Ctrl+K），且无 reload 命令，键盘模拟路线
 * 走不通。故退回到最可靠的 kill + relaunch；ZCode 重启后会自动恢复会话。
 */
let zcodeReloadTimer: ReturnType<typeof setTimeout> | null = null;
export function scheduleZcodeReload(delay = 1500): void {
  if (zcodeReloadTimer) clearTimeout(zcodeReloadTimer);
  zcodeReloadTimer = setTimeout(() => {
    zcodeReloadTimer = null;
    invoke<void>("restart_zcode").catch(() => {
      /* 重启失败静默：用户可手动点「重启 zcode」 */
    });
  }, delay);
}
