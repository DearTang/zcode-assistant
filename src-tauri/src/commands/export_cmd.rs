//! 配置反向同步：把 zcode 的 provider 导出到 cc-switch（统一目标）。
//!
//! cc-switch 是各 agent 供应商配置的集中管理器，按 `providers.app_type` 区分目标
//! agent；当前写 `app_type='opencode'` 组，`settings_config` 即 opencode 格式的
//! provider 条目（name/npm/options/models），由 cc-switch 在切换供应商时落到
//! opencode.json。匹配口径与导入一致：baseURL（归一化）+ apiKey 完全一致视为
//! 同一条 → 覆盖更新；否则新增。写入前滚动备份数据库（.db.bak）。
use crate::zcode::config_file;
use rusqlite::Connection;
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::path::PathBuf;

/// 待导出的 zcode provider（已转 opencode 格式）。
/// 上下文等模型限制忠实携带 zcode 现有配置，不做自动纠正
/// （纠正入口统一在导入侧的「重新获取上下文」开关）。
struct ExportEntry {
    /// zcode provider key（作为目标侧新条目的 key / id 基准）
    key: String,
    name: String,
    entry: Value,
    base_url: String,
    api_key: String,
    enabled: bool,
    model_count: usize,
}

/// 预览：待导出的 provider（不写任何文件），供前端弹窗勾选
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ExportPreview {
    /// zcode provider key（同一次解析内唯一，勾选 / 导出过滤的标识）
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub model_count: usize,
    pub has_api_key: bool,
    pub enabled: bool,
    /// 目标侧命中的已有 key（导出时将覆盖更新该条）
    pub duplicate_of: Option<String>,
}

/// 单条导出结果
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub name: String,
    /// "success"（新增）| "updated"（覆盖）| "failed"
    pub status: String,
    pub target_key: String,
    pub message: String,
}

/// 导出整体结果（results + 需要提示的非致命警告）
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ExportOutcome {
    pub results: Vec<ExportResult>,
    pub warning: Option<String>,
}

fn ccswitch_db_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".cc-switch").join("cc-switch.db"))
}

/// zcode provider 条目 → opencode 格式条目。
/// 丢弃 zcode 专有字段（kind/source/enabled/zcode），kind 映射为 npm 包名；
/// 模型条目剥离 zcode 子对象与 enabled，保证至少有 name。
fn to_opencode_entry(key: &str, p: &Value) -> Option<ExportEntry> {
    let opts = p.get("options")?;
    let base_url = opts
        .get("baseURL")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if base_url.is_empty() {
        return None; // 无 baseURL 无法使用，跳过
    }
    let api_key = opts
        .get("apiKey")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let name = p
        .get("name")
        .and_then(|x| x.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| key.to_string());
    let kind = p.get("kind").and_then(|x| x.as_str()).unwrap_or("");
    let npm = if kind == "anthropic" {
        "@ai-sdk/anthropic"
    } else {
        "@ai-sdk/openai-compatible"
    };
    // options 保留原字段（apiKey/baseURL/setCacheKey 等），去掉 zcode 专有的 apiKeyRequired
    let mut clean_opts = Map::new();
    if let Some(o) = opts.as_object() {
        for (k, v) in o {
            if k != "apiKeyRequired" {
                clean_opts.insert(k.clone(), v.clone());
            }
        }
    }
    // models：按 opencode schema 白名单导出（zcode 的模型字段与其不完全兼容）：
    // - name：显示名（缺省用模型 id）
    // - limit：仅 context 与 output 均为正数时导出——opencode 要求 limit 内两者
    //   齐备，缺 output 会被判 invalid（如 "Missing key ...limit.output"）
    // - modalities：输入输出模态（可选，opencode 接受）
    // 其余全部丢弃：reasoning 在 zcode 是对象（enabled/variants/defaultVariant），
    // opencode 期望 boolean，原样导出必然校验失败；zcode / enabled 等专有字段同理
    let mut models = Map::new();
    if let Some(ms) = p.get("models").and_then(|m| m.as_object()) {
        for (mid, mv) in ms {
            let obj = mv.as_object();
            let mut m = Map::new();
            let display_name = obj
                .and_then(|o| o.get("name"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .unwrap_or(mid);
            m.insert("name".to_string(), json!(display_name));
            let limit = obj.and_then(|o| o.get("limit")).and_then(|l| l.as_object());
            // 只要 context 有效就导出 limit（output 缺失/非正时按 131072 兜底，
            // 不再要求两者都为正--opencode schema 仅要求 limit 内 context/output
            // 键齐备，值由我们保证为正）
            let ctx = limit
                .and_then(|l| l.get("context"))
                .and_then(|v| v.as_i64())
                .filter(|c| *c > 0);
            if let Some(c) = ctx {
                let out = limit
                    .and_then(|l| l.get("output"))
                    .and_then(|v| v.as_i64())
                    .filter(|o| *o > 0)
                    .unwrap_or(crate::openrouter::DEFAULT_OUTPUT_LENGTH);
                let mut lo = Map::new();
                lo.insert("context".to_string(), json!(c));
                lo.insert("output".to_string(), json!(out));
                m.insert("limit".to_string(), Value::Object(lo));
            }
            if let Some(mods) = obj.and_then(|o| o.get("modalities")) {
                m.insert("modalities".to_string(), mods.clone());
            }
            models.insert(mid.clone(), Value::Object(m));
        }
    }
    let enabled = p.get("enabled").and_then(|x| x.as_bool()).unwrap_or(true);
    let model_count = models.len();
    Some(ExportEntry {
        key: key.to_string(),
        name: name.clone(),
        entry: json!({
            "name": name,
            "npm": npm,
            "options": Value::Object(clean_opts),
            "models": Value::Object(models),
        }),
        base_url,
        api_key,
        enabled,
        model_count,
    })
}

/// 读取 zcode 配置中可导出的 provider（跳过 builtin: 与无 baseURL 的）
fn collect_entries() -> Result<Vec<ExportEntry>, String> {
    let config = config_file::read_config().map_err(|e| e.to_string())?;
    let providers = config
        .get("provider")
        .and_then(|p| p.as_object())
        .ok_or("zcode config.json 无 provider 对象")?;
    Ok(providers
        .iter()
        .filter(|(k, _)| !k.starts_with("builtin:"))
        .filter_map(|(k, p)| to_opencode_entry(k, p))
        .collect())
}

/// 条目匹配口径（与导入一致）：baseURL 归一化 + apiKey 完全一致（两者非空才可能命中）
fn entry_base_and_key(v: &Value) -> Option<(String, String)> {
    let opts = v.get("options")?;
    let base = opts
        .get("baseURL")
        .and_then(|x| x.as_str())?
        .trim()
        .trim_end_matches('/')
        .to_ascii_lowercase();
    let key = opts.get("apiKey").and_then(|x| x.as_str()).unwrap_or("").trim();
    if base.is_empty() || key.is_empty() {
        return None;
    }
    Some((base, key.to_string()))
}

fn matches_target(v: &Value, e: &ExportEntry) -> bool {
    match entry_base_and_key(v) {
        Some((b, k)) => {
            b == e.base_url.trim().trim_end_matches('/').to_ascii_lowercase() && k == e.api_key
        }
        None => false,
    }
}

/// 读取 cc-switch providers 表（app_type='opencode'）：id -> settings_config 条目
fn read_ccswitch_providers() -> Result<Map<String, Value>, String> {
    let path = ccswitch_db_path().ok_or("无 HOME")?;
    if !path.exists() {
        return Err("未找到 cc-switch 数据库（~/.cc-switch/cc-switch.db），请先安装 cc-switch".into());
    }
    let conn = Connection::open_with_flags(
        &path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| format!("打开 cc-switch 数据库失败: {e}"))?;
    let mut stmt = conn
        .prepare("SELECT id, settings_config FROM providers WHERE app_type='opencode'")
        .map_err(|e| format!("读取 cc-switch 供应商失败: {e}"))?;
    let rows = stmt
        .query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(|e| format!("读取 cc-switch 供应商失败: {e}"))?;
    let mut out = Map::new();
    for row in rows {
        let (id, cfg) = row.map_err(|e| format!("读取 cc-switch 供应商失败: {e}"))?;
        if let Ok(v) = serde_json::from_str::<Value>(&cfg) {
            out.insert(id, v);
        }
    }
    Ok(out)
}

/// 预览导出内容：解析 zcode provider 并标记 cc-switch 侧覆盖关系，不执行任何写入
#[tauri::command]
pub fn export_preview() -> Result<Vec<ExportPreview>, String> {
    let entries = collect_entries()?;
    if entries.is_empty() {
        return Err("zcode 配置中没有可导出的自定义供应商".into());
    }
    let target_providers = read_ccswitch_providers()?;
    Ok(entries
        .into_iter()
        .map(|e| {
            let duplicate_of = target_providers
                .iter()
                .find(|(_, v)| matches_target(v, &e))
                .map(|(k, _)| k.clone());
            ExportPreview {
                id: e.key,
                name: e.name,
                base_url: e.base_url,
                model_count: e.model_count,
                has_api_key: !e.api_key.is_empty(),
                enabled: e.enabled,
                duplicate_of,
            }
        })
        .collect())
}

/// 生成目标侧唯一 key：基准名被占用时加 -2/-3 后缀
fn unique_key(base: &str, taken: &dyn Fn(&str) -> bool) -> String {
    let mut id = base.to_string();
    let mut suffix = 1;
    while taken(&id) {
        suffix += 1;
        id = format!("{base}-{suffix}");
    }
    id
}

/// 执行导出：写入 cc-switch 的 opencode 供应商组（app_type='opencode'）
#[tauri::command]
pub fn export_providers_to(selected: Option<Vec<String>>) -> Result<ExportOutcome, String> {
    let mut entries = collect_entries()?;
    if let Some(sel) = selected {
        entries.retain(|e| sel.iter().any(|id| id == &e.key));
    }
    if entries.is_empty() {
        return Err("未选择任何要导出的供应商".into());
    }
    export_to_ccswitch(entries)
}

fn export_to_ccswitch(entries: Vec<ExportEntry>) -> Result<ExportOutcome, String> {
    let path = ccswitch_db_path().ok_or("无 HOME")?;
    if !path.exists() {
        return Err("未找到 cc-switch 数据库（~/.cc-switch/cc-switch.db），请先安装 cc-switch".into());
    }
    let conn = Connection::open(&path).map_err(|e| format!("打开 cc-switch 数据库失败: {e}"))?;
    // 滚动备份（VACUUM INTO 会连同 WAL 内容落盘，且要求目标不存在）
    let bak = path.with_extension("db.bak");
    let _ = std::fs::remove_file(&bak);
    let bak_str = bak
        .to_string_lossy()
        .replace('\'', "''");
    conn.execute_batch(&format!("VACUUM INTO '{bak_str}'"))
        .map_err(|e| format!("备份 cc-switch 数据库失败: {e}"))?;

    // 现有 opencode 条目（匹配覆盖）+ 全表 id（新增时保证主键唯一）
    let mut stmt = conn
        .prepare("SELECT id, settings_config FROM providers WHERE app_type='opencode'")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(|e| e.to_string())?;
    let mut existing: Vec<(String, Value)> = Vec::new();
    for row in rows {
        let (id, cfg) = row.map_err(|e| e.to_string())?;
        if let Ok(v) = serde_json::from_str::<Value>(&cfg) {
            existing.push((id, v));
        }
    }
    drop(stmt);
    let all_ids: Vec<String> = conn
        .prepare("SELECT id FROM providers")
        .and_then(|mut s| {
            s.query_map([], |r| r.get::<_, String>(0))
                .map(|rows| rows.filter_map(|x| x.ok()).collect())
        })
        .map_err(|e| e.to_string())?;

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let mut results = Vec::new();
    for e in entries {
        let cfg = serde_json::to_string(&e.entry).unwrap_or_default();
        let dup_id = existing
            .iter()
            .find(|(_, v)| matches_target(v, &e))
            .map(|(id, _)| id.clone());
        if let Some(id) = dup_id {
            conn.execute(
                "UPDATE providers SET name=?1, settings_config=?2 WHERE id=?3",
                rusqlite::params![e.name, cfg, id],
            )
            .map_err(|er| format!("更新 cc-switch 供应商失败: {er}"))?;
            results.push(ExportResult {
                name: e.name,
                status: "updated".into(),
                target_key: id.clone(),
                message: format!("已覆盖 cc-switch 供应商「{id}」"),
            });
            continue;
        }
        let id = unique_key(&e.key, &|k| all_ids.contains(&k.to_string()));
        conn.execute(
            "INSERT INTO providers(id, app_type, name, settings_config, category, created_at, meta, is_current, in_failover_queue)
             VALUES(?1, 'opencode', ?2, ?3, 'custom', ?4, '{}', 0, 0)",
            rusqlite::params![id, e.name, cfg, now_ms],
        )
        .map_err(|er| format!("写入 cc-switch 供应商失败: {er}"))?;
        results.push(ExportResult {
            name: e.name,
            status: "success".into(),
            target_key: id.clone(),
            message: format!("已新增 cc-switch 供应商「{id}」（{} 个模型）", e.model_count),
        });
    }
    // cc-switch 正在运行时其界面不会实时刷新，提醒重启
    let warning = if is_ccswitch_running() {
        Some("cc-switch 正在运行，同步已写入数据库；请重启 cc-switch 后查看".into())
    } else {
        None
    };
    Ok(ExportOutcome {
        results,
        warning,
    })
}

#[cfg(windows)]
fn is_ccswitch_running() -> bool {
    let mut cmd = std::process::Command::new("tasklist");
    crate::zcode::process::no_window(&mut cmd)
        .args(["/FI", "IMAGENAME eq cc-switch.exe", "/NH"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("cc-switch.exe"))
        .unwrap_or(false)
}

#[cfg(not(windows))]
fn is_ccswitch_running() -> bool {
    false
}
