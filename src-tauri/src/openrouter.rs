//! OpenRouter 模型目录：每日启动拉取一次，记录各模型真实上下文长度等元数据。
//! 拉取可用模型时按模型名模糊匹配兜底填充 context（优先于内置写死规格表）；
//! 拉取失败则沿用上一次目录，不阻塞任何功能。
use crate::db::Database;
use chrono::Local;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::Manager;

const CATALOG_URL: &str = "https://openrouter.ai/api/v1/models";
/// KV key 带版本后缀：换 key 名即让老用户触发一次全量重拉
/// （v3: 切到标准 API /api/v1/models，解析 top_provider.max_completion_tokens）
const KV_KEY: &str = "openrouter_catalog_v3";

/// 目录单条记录：模型名（slug/hf_slug 取「/」后段并转小写）+ 真实上下文 + 元数据
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CatalogEntry {
    pub name: String,
    pub context_length: i64,
    /// 输出长度上限（接口字段 max_completion_tokens；未提供时默认 131072）
    #[serde(default = "default_output_length")]
    pub output_length: i64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_modalities: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

/// 输出长度默认值（接口未提供 max_completion_tokens 时兜底）
pub const DEFAULT_OUTPUT_LENGTH: i64 = 131_072;

fn default_output_length() -> i64 {
    DEFAULT_OUTPUT_LENGTH
}

/// 完整目录（含拉取时间，按 updated_at 从新到旧排序）
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    pub fetched_at: String,
    pub models: Vec<CatalogEntry>,
}

/// 本地日期前缀（YYYY-MM-DD），用于「今天是否已拉取」判定
fn local_date(iso: &str) -> &str {
    iso.split('T').next().unwrap_or("")
}

fn today() -> String {
    Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}

/// 读取上一次保存的目录（无则 None）
pub fn load_catalog(db: &Database) -> Option<Catalog> {
    db.kv_get(KV_KEY)
        .and_then(|s| serde_json::from_str::<Catalog>(&s).ok())
}

/// 解析 OpenRouter 标准 API（/api/v1/models）响应为目录（按 created 从新到旧）
fn parse_catalog(v: &Value, fetched_at: String) -> Result<Catalog, String> {
    let arr = v
        .get("data")
        .and_then(|d| d.as_array())
        .ok_or_else(|| "响应无 data 数组".to_string())?;
    let mut models: Vec<CatalogEntry> = arr
        .iter()
        .filter_map(|m| {
            // 模型名：id 取「/」后段并转小写（如 "z-ai/glm-5.3" → "glm-5.3"）
            let raw = m.get("id").and_then(|s| s.as_str())?;
            let name = raw.rsplit('/').next()?.trim().to_lowercase();
            if name.is_empty() {
                return None;
            }
            let context_length = m.get("context_length").and_then(|x| x.as_i64())?;
            if context_length <= 0 {
                return None;
            }
            // 输出长度：top_provider.max_completion_tokens，未提供 / 非正时按默认 131072 兜底
            let output_length = m
                .get("top_provider")
                .and_then(|t| t.get("max_completion_tokens"))
                .and_then(|x| x.as_i64())
                .filter(|v| *v > 0)
                .unwrap_or(DEFAULT_OUTPUT_LENGTH);
            // input_modalities 在 architecture 下
            let input_modalities = m
                .get("architecture")
                .and_then(|a| a.get("input_modalities"))
                .and_then(|x| x.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|s| s.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            // author 取 id 前段（如 "z-ai"）
            let author = raw.split('/').next().map(String::from);
            // created 是 unix 秒
            let created_at = m
                .get("created")
                .and_then(|x| x.as_i64())
                .filter(|v| *v > 0)
                .and_then(|ts| chrono::DateTime::from_timestamp(ts, 0))
                .map(|d| d.to_rfc3339());
            Some(CatalogEntry {
                name,
                context_length,
                output_length,
                input_modalities,
                author,
                created_at: created_at.clone(),
                // 标准 API 无 updated_at，用 created_at 近似排序
                updated_at: created_at,
            })
        })
        .collect();
    // 按 created_at 从新到旧排序
    models.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(Catalog { fetched_at, models })
}

/// 每日启动拉取：今天已拉过则跳过；成功全量替换，失败沿用上一次目录。
pub async fn daily_fetch(handle: &tauri::AppHandle) -> Result<(), String> {
    let state = handle.state::<crate::state::AppState>();
    let existing = load_catalog(&state.db);
    let now = today();
    if let Some(c) = &existing {
        if local_date(&c.fetched_at) == local_date(&now) {
            return Ok(()); // 今天已拉取
        }
    }
    let client = state.client();
    let resp = client
        .get(CATALOG_URL)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status().as_u16()));
    }
    let v: Value = resp.json().await.map_err(|e| e.to_string())?;
    let catalog = parse_catalog(&v, now)?;
    if catalog.models.is_empty() {
        return Err("目录为空，保留上一次数据".to_string());
    }
    let s = serde_json::to_string(&catalog).map_err(|e| e.to_string())?;
    state.db.kv_set(KV_KEY, &s).map_err(|e| e.to_string())
}

/// 模糊匹配：目录按 updated_at 从新到旧遍历，取第一个与模型 id
/// 互相包含（小写、非空）的条目，返回其真实上下文长度。
pub fn fuzzy_context(catalog: &[CatalogEntry], model_id: &str) -> Option<i64> {
    let id = model_id.trim().to_lowercase();
    if id.is_empty() {
        return None;
    }
    catalog
        .iter()
        .find(|e| !e.name.is_empty() && (id.contains(&e.name) || e.name.contains(&id)))
        .map(|e| e.context_length)
}

/// 模糊匹配：与 fuzzy_context 同样规则，返回目录中的输出长度上限
/// （解析时已兜底为 DEFAULT_OUTPUT_LENGTH，所以命中必有值）。
pub fn fuzzy_output(catalog: &[CatalogEntry], model_id: &str) -> Option<i64> {
    let id = model_id.trim().to_lowercase();
    if id.is_empty() {
        return None;
    }
    catalog
        .iter()
        .find(|e| !e.name.is_empty() && (id.contains(&e.name) || e.name.contains(&id)))
        .map(|e| e.output_length)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, ctx: i64, out: i64, updated: &str) -> CatalogEntry {
        CatalogEntry {
            name: name.to_string(),
            context_length: ctx,
            output_length: out,
            input_modalities: vec![],
            author: None,
            created_at: None,
            updated_at: Some(updated.to_string()),
        }
    }

    #[test]
    fn fuzzy_prefers_newest_and_matches_case_insensitive() {
        let catalog = vec![
            entry("qwen3.8-27b", 262_144, 8192, "2026-08-14T17:00:00Z"),
            entry("qwen3.8-27b-20260801", 131_072, 4096, "2026-08-01T00:00:00Z"),
            entry("glm-4.6", 200_000, 131072, "2026-07-01T00:00:00Z"),
        ];
        // 精确命中（大小写不敏感）
        assert_eq!(
            fuzzy_context(&catalog, "Qwen/Qwen3.8-27B"),
            Some(262_144)
        );
        assert_eq!(
            fuzzy_output(&catalog, "Qwen/Qwen3.8-27B"),
            Some(8192)
        );
        // id 含目录名 → 命中最新的那条
        assert_eq!(
            fuzzy_context(&catalog, "qwen3.8-27b-instruct"),
            Some(262_144)
        );
        assert_eq!(
            fuzzy_output(&catalog, "qwen3.8-27b-instruct"),
            Some(8192)
        );
        // 目录名含 id（新版本号）→ 从新到旧取第一个
        assert_eq!(fuzzy_context(&catalog, "glm-4.6"), Some(200_000));
        assert_eq!(fuzzy_output(&catalog, "glm-4.6"), Some(131072));
        assert_eq!(fuzzy_context(&catalog, "no-such-model"), None);
        assert_eq!(fuzzy_output(&catalog, "no-such-model"), None);
    }

    #[test]
    fn parse_sorts_by_created_desc() {
        // 标准 API 形态：id + context_length + created（unix 秒），
        // 输出长度取 top_provider.max_completion_tokens（缺省兜底 131072）
        let v: Value = serde_json::json!({
            "data": [
                { "id": "vendor/old-model", "context_length": 1000,
                  "created": 1_700_000_000 },
                { "id": "Vendor/New-Model", "context_length": 2000,
                  "top_provider": { "max_completion_tokens": 4096 },
                  "created": 1_800_000_000 }
            ]
        });
        let c = parse_catalog(&v, "2026-08-18T09:00:00".into()).unwrap();
        assert_eq!(c.models.len(), 2);
        assert_eq!(c.models[0].name, "new-model"); // id 后段转小写
        assert_eq!(c.models[0].context_length, 2000);
        assert_eq!(c.models[0].output_length, 4096);
        assert_eq!(c.models[0].author.as_deref(), Some("Vendor")); // id 前段
        assert_eq!(c.models[1].name, "old-model"); // created 从新到旧
    }
}
