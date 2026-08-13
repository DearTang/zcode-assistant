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
}

/// 自动切换规则：cron（指定时段禁用某模型→切到指定）或 drain（配额耗尽→下一个）
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct AutoSwitchRule {
    pub id: String,
    pub name: String,
    pub kind: String, // "cron" | "drain"
    pub enabled: bool,
    pub time_start: Option<String>, // "HH:MM"
    pub time_end: Option<String>, // "HH:MM"
    pub weekdays: Option<String>, // "1,2,3,4,5,6,7"
    pub family: Option<String>,
    pub from_provider: Option<String>,
    pub to_provider: String,
    pub threshold: Option<f64>, // drain：剩余 ≤ 阈值
    pub priority: Option<i64>,
    pub created_at: String,
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
