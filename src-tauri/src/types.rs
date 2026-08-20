//! 后端共享类型（与前端 types.ts 对齐）
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProxyConfig {
    pub enabled: bool,
    #[serde(rename = "type")]
    pub ptype: String, // none | http | socks5
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub has_password: bool,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AccountMeta {
    pub id: String,
    pub short_id: String,
    pub user_id: Option<String>,
    pub provider: Option<String>,
    pub label: String,
    pub email: Option<String>,
    pub name: Option<String>,
    pub avatar: Option<String>,
    pub customer_id: Option<String>,
    pub note: Option<String>,
    pub captured_at: String,
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct QuotaTemplate {
    pub provider_key: String,
    pub name: Option<String>,
    pub method: Option<String>, // GET | POST
    pub url: Option<String>,
    pub headers_json: Option<String>,
    pub body: Option<String>,
    pub total_path: Option<String>,
    pub used_path: Option<String>,
    pub remaining_path: Option<String>,
    /// 月限额（可选，部分供应商才有）：配置后同一响应里再提取一个「每月使用额度」桶
    #[serde(default)]
    pub monthly_total_path: Option<String>,
    #[serde(default)]
    pub monthly_used_path: Option<String>,
    #[serde(default)]
    pub monthly_remaining_path: Option<String>,
    /// 登录页 URL：配「登录获取 Token」弹内嵌窗口加载（用户登录含 2FA）
    #[serde(default)]
    pub login_url: Option<String>,
    /// Token 提取规则：`cookie:<名称>` 或 `localstorage:<key>[#<dot.path>]
    /// （localStorage 值为 JSON 时用 # 后的 dot path 取子字段）
    #[serde(default)]
    pub token_source: Option<String>,
    /// 用量查询方式："token"（登录会话 Token）| "appkey"（API Key，默认/旧数据）
    ///   | "coding_plan"（Token Plan 内置预设标记）
    #[serde(default)]
    pub auth_mode: Option<String>,
    /// 登录账号（自动填充用，密码另存系统凭证库不落库）
    #[serde(default)]
    pub login_username: Option<String>,
    /// 附加凭据 JSON：智谱团队版 {organizationId,projectId}；
    /// 火山方舟 {accessKeyId,secretAccessKey}（仅存本机数据库）
    #[serde(default)]
    pub extra_json: Option<String>,
}

/// 自动切换规则：cron（指定执行时间/星期→切到目标）或 drain（配额耗尽→切到目标）
/// family 字段已废弃（切换改用全局 setting.providerFamilyDomain）；DB 列保留以兼容旧库。
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct AutoSwitchRule {
    pub id: String,
    pub name: String,
    pub kind: String, // "cron" | "drain"
    pub enabled: bool,
    pub time_start: Option<String>, // "HH:MM"（cron 执行时间）
    #[serde(default)]
    pub time_end: Option<String>, // 兼容旧数据，不再使用
    pub weekdays: Option<String>, // "1,2,3,4,5,6,7"
    pub from_provider: Option<String>, // 源供应商 id（None=任意）
    pub from_model: Option<String>, // 源模型（可选，仅展示）
    pub to_provider: String, // 目标供应商 id
    pub to_model: Option<String>, // 目标模型（必填，仅展示）
    pub threshold: Option<f64>, // drain：剩余 ≤ 阈值
    pub priority: Option<i64>,
    pub created_at: String,
    #[serde(default)]
    pub project_dir: Option<String>, // 项目目录（限定：仅该项目为最近对话项目时触发；None=全部项目）
}

/// 自动切换可选项目（当前打开且有具体对话的项目）
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ProjectOption {
    pub dir: String,               // 项目目录绝对路径
    pub name: String,              // 目录名（展示用）
    pub sessions: i64,             // 会话数
    pub last_active_ms: Option<i64>, // 最近一次对话时间（毫秒）
}

/// 自动切换执行日志（手动测试 / 定时 / 配额耗尽 / 应用启动）
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AutoSwitchLog {
    pub id: i64,
    pub rule_id: String,
    pub rule_name: String,
    pub trigger_type: String, // manual | cron | drain | appstart
    pub success: bool,
    pub message: Option<String>, // 失败原因 / 成功备注
    pub created_at: String,
}

/// 应用偏好：悬浮球可见性 + 配额展示方案 + 切换后是否提示重启 + 开机自启（持久化于 DB kv 表）
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AppPrefs {
    /// 悬浮球是否显示
    pub float_ball_visible: bool,
    /// 配额展示方案："used" 展示已用量（默认）| "remaining" 展示剩余用量
    pub usage_display: String,
    /// 自动切换 / 账号切换完成后是否提示重启 ZCode（默认 true：
    /// ZCode 机制限制下免重启仅当前会话生效，重启后全部会话生效）
    pub switch_restart_zcode: bool,
    /// 是否开机自启动（默认 false：用户需主动在设置中开启）
    pub autostart: bool,
}

// ===== 用量查询（解析 ~/.zcode/cli/rollout 逐次模型调用记录）=====

/// 单次模型调用记录（持久化 + 返回前端共用）
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct UsageRecord {
    pub request_id: String,
    pub started_at: Option<String>, // ISO8601
    pub date: String,               // YYYY-MM-DD（派生，用于按日聚合）
    pub provider_id: String,
    pub model_id: String,
    pub role: Option<String>,             // main / lite / subagent
    pub query_source: Option<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub total_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
    pub duration_ms: Option<f64>,
    pub tps: Option<f64>, // output_tokens / (duration_ms/1000)
    pub finish_reason: Option<String>,
    pub session_id: Option<String>,
    pub raw_path: Option<String>,
}

/// 同步扫描结果
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct UsageSyncResult {
    pub new_count: usize,
    /// 对账回收数：zcode 侧已清理、本地顺带删除的记录条数
    pub removed_count: usize,
    pub total_count: i64,
    pub scanned_files: usize,
    pub min_date: Option<String>,
    pub max_date: Option<String>,
}

/// 筛选项（去重后的供应商/模型/角色 + 日期范围）
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct UsageFilters {
    pub providers: Vec<String>,
    pub models: Vec<String>,
    pub roles: Vec<String>,
    pub min_date: Option<String>,
    pub max_date: Option<String>,
    pub total_records: i64,
}

/// 整体汇总（随筛选条件）
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct UsageOverview {
    pub calls: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
    pub total_tokens: i64,
    pub avg_tps: Option<f64>,
    pub max_tps: Option<f64>,
    pub min_tps: Option<f64>,
    pub avg_duration_ms: Option<f64>,
}

/// 分组聚合行（按供应商 / 模型 / 日期）
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct UsageAggRow {
    pub key: String,
    pub label: String,
    pub calls: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
    pub total_tokens: i64,
    pub avg_tps: Option<f64>,
    pub max_tps: Option<f64>,
    pub min_tps: Option<f64>,
    pub avg_duration_ms: Option<f64>,
}

/* ============ 项目 / 会话管理（数据源：zcode cli db 的 session 等表）============ */

/// 项目（session 表按 project_id 分组；会话数只计顶层，子代理会话归入其根会话）
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct ZcProject {
    pub id: String,                 // project_id（proj_ 前缀）
    pub directory: String,          // 项目目录（组内任一会话的 directory）
    pub sessions: i64,              // 会话数（顶层会话总数，含归档）
    pub archived_sessions: i64,     // 已归档的顶层会话数（活跃 = sessions - archived_sessions）
    pub turns: i64,                 // 对话次数（turn_usage 轮次，含子代理）
    pub calls: i64,                 // 模型调用次数（model_usage 行数）
    pub input_tokens: i64,          // 与「用量查询」同口径：model_usage 汇总
    pub output_tokens: i64,
    pub total_tokens: i64,
    pub time_created_ms: Option<i64>, // 最早会话创建时间
    pub time_updated_ms: Option<i64>, // 最近活跃时间
}

/// 会话（顶层；消耗统计含其全部子代理后代会话）
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct ZcSession {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub title_source: String,       // default / first_input / generated / custom
    pub directory: String,
    pub task_type: String,          // interactive / subagent_child / ...
    /// 是否已归档（zcode 任务索引 tasks.archived/deleted，或 session.time_archived）
    #[serde(default)]
    pub archived: bool,
    pub turns: i64,                 // 对话次数（含子代理轮次）
    pub calls: i64,                 // 模型调用次数（含子代理）
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub total_tokens: i64,
    pub time_created_ms: i64,
    pub time_updated_ms: i64,
    pub time_archived_ms: Option<i64>,
}

/// 批量删除结果（会话数为含子代理后代的展开数量）
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct ZcDeleteResult {
    pub deleted_sessions: usize,
    pub deleted_projects: usize,
    pub freed_rollout_files: usize, // 顺带清理的 model-io-sess_*.jsonl 数量
}
