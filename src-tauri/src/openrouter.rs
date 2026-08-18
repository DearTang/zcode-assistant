//! OpenRouter 模型目录：每日启动拉取一次，记录各模型真实上下文长度等元数据。
//! 拉取可用模型时按模型名模糊匹配兜底填充 context（优先于内置写死规格表）；
//! 拉取失败则沿用上一次目录，不阻塞任何功能。
use crate::db::Database;
use chrono::Local;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::Manager;

const CATALOG_URL: &str = "https://openrouter.ai/api/frontend/v1/catalog/models";
const KV_KEY: &str = "openrouter_catalog";

/// 目录单条记录：模型名（slug/hf_slug 取「/」后段并转小写）+ 真实上下文 + 元数据
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CatalogEntry {
    pub name: String,
    pub context_length: i64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_modalities: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
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

/// 解析 OpenRouter catalog 响应为目录（按 updated_at 从新到旧）
fn parse_catalog(v: &Value, fetched_at: String) -> Result<Catalog, String> {
    let arr = v
        .get("data")
        .and_then(|d| d.as_array())
        .ok_or_else(|| "响应无 data 数组".to_string())?;
    let mut models: Vec<CatalogEntry> = arr
        .iter()
        .filter_map(|m| {
            // 模型名：slug（小写）优先，退回 hf_slug；均取「/」后段并转小写
            let raw = m
                .get("slug")
                .and_then(|s| s.as_str())
                .or_else(|| m.get("hf_slug").and_then(|s| s.as_str()))?;
            let name = raw.rsplit('/').next()?.trim().to_lowercase();
            if name.is_empty() {
                return None;
            }
            let context_length = m.get("context_length").and_then(|x| x.as_i64())?;
            if context_length <= 0 {
                return None;
            }
            Some(CatalogEntry {
                name,
                context_length,
                input_modalities: m
                    .get("input_modalities")
                    .and_then(|x| x.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|s| s.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default(),
                author: m.get("author").and_then(|x| x.as_str()).map(String::from),
                created_at: m
                    .get("created_at")
                    .and_then(|x| x.as_str())
                    .map(String::from),
                updated_at: m
                    .get("updated_at")
                    .and_then(|x| x.as_str())
                    .map(String::from),
            })
        })
        .collect();
    // ISO-8601 UTC 字符串的字典序即时间序，从最新到最早
    models.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, ctx: i64, updated: &str) -> CatalogEntry {
        CatalogEntry {
            name: name.to_string(),
            context_length: ctx,
            input_modalities: vec![],
            author: None,
            created_at: None,
            updated_at: Some(updated.to_string()),
        }
    }

    #[test]
    fn fuzzy_prefers_newest_and_matches_case_insensitive() {
        let catalog = vec![
            entry("qwen3.8-27b", 262_144, "2026-08-14T17:00:00Z"),
            entry("qwen3.8-27b-20260801", 131_072, "2026-08-01T00:00:00Z"),
            entry("glm-4.6", 200_000, "2026-07-01T00:00:00Z"),
        ];
        // 精确命中（大小写不敏感）
        assert_eq!(
            fuzzy_context(&catalog, "Qwen/Qwen3.8-27B"),
            Some(262_144)
        );
        // id 含目录名 → 命中最新的那条
        assert_eq!(
            fuzzy_context(&catalog, "qwen3.8-27b-instruct"),
            Some(262_144)
        );
        // 目录名含 id（新版本号）→ 从新到旧取第一个
        assert_eq!(fuzzy_context(&catalog, "glm-4.6"), Some(200_000));
        assert_eq!(fuzzy_context(&catalog, "no-such-model"), None);
    }

    #[test]
    fn parse_sorts_by_updated_desc() {
        let v: Value = serde_json::json!({
            "data": [
                { "slug": "a/old-model", "context_length": 1000,
                  "updated_at": "2026-08-01T00:00:00Z", "created_at": "2026-08-01T00:00:00Z",
                  "input_modalities": ["text"] },
                { "hf_slug": "A/New-Model", "context_length": 2000,
                  "updated_at": "2026-08-14T00:00:00Z" }
            ]
        });
        let c = parse_catalog(&v, "2026-08-18T09:00:00".into()).unwrap();
        assert_eq!(c.models.len(), 2);
        assert_eq!(c.models[0].name, "new-model"); // hf_slug 后段转小写
        assert_eq!(c.models[0].context_length, 2000);
        assert_eq!(c.models[0].author, None);
    }
}
