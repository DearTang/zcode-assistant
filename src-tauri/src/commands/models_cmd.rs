//! 模型管理：拉取可用模型、内置规格表、增删 provider、改 model limit
use crate::state::AppState;
use crate::zcode::config_file;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::State;

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ModelSpec {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_length: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output: Option<i64>,
}

/// 内置智谱模型规格表（兜底上下文/输出，文档未全列，常用型号）
const BUILTIN: &[(&str, i64, i64)] = &[
    ("glm-4.6", 200_000, 128_000),
    ("glm-4.5", 128_000, 96_000),
    ("glm-4.5-air", 128_000, 4_096),
    ("glm-4.5v", 128_000, 4_096),
    ("glm-4-plus", 128_000, 4_096),
    ("glm-4-air", 128_000, 4_096),
    ("glm-4-airx", 128_000, 4_096),
    ("glm-4-flash", 128_000, 4_096),
    ("glm-4-flashx", 128_000, 4_096),
    ("glm-4-long", 1_000_000, 4_096),
];

#[tauri::command]
pub fn builtin_model_specs() -> Vec<ModelSpec> {
    BUILTIN
        .iter()
        .map(|(id, ctx, out)| ModelSpec {
            id: id.to_string(),
            name: None,
            context_length: Some(*ctx),
            max_output: Some(*out),
        })
        .collect()
}

/// 拼 /models 端点（智谱 anthropic 端点用 paas/v4，其余按 OpenAI 兼容约定）
fn models_endpoint(base: &str) -> String {
    let b = base.trim_end_matches('/');
    if b.contains("open.bigmodel.cn") || b.contains("bigmodel.cn") {
        return "https://open.bigmodel.cn/api/paas/v4/models".into();
    }
    if b.ends_with("/v1") || b.ends_with("/v4") {
        format!("{b}/models")
    } else if b.ends_with("/anthropic") || b.ends_with("/coding") {
        format!("{b}/v1/models")
    } else {
        format!("{b}/models")
    }
}

/// 用 provider 的 baseURL + apiKey 调 /models 拉取可用模型
#[tauri::command]
pub async fn fetch_available_models(
    state: State<'_, AppState>,
    provider_key: String,
) -> Result<Vec<ModelSpec>, String> {
    let config = config_file::read_config().map_err(|e| e.to_string())?;
    let base = config_file::provider_base_url(&config, &provider_key)
        .ok_or_else(|| "provider 无 baseURL".to_string())?;
    let key = config_file::provider_api_key(&config, &provider_key)
        .ok_or_else(|| "provider 无可用 apiKey".to_string())?;

    let url = models_endpoint(&base);
    let client = state.client();
    let resp = client
        .get(&url)
        .bearer_auth(&key)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let v: Value = resp.json().await.map_err(|e| e.to_string())?;
    let arr = v
        .get("data")
        .and_then(|d| d.as_array())
        .or_else(|| v.as_array())
        .ok_or_else(|| "响应无 data 数组".to_string())?;

    // 内置规格兜底映射
    let builtin_map: std::collections::HashMap<String, (i64, i64)> = BUILTIN
        .iter()
        .map(|(id, c, o)| (id.to_string(), (*c, *o)))
        .collect();

    let specs: Vec<ModelSpec> = arr
        .iter()
        .filter_map(|m| {
            let id = m.get("id").and_then(|x| x.as_str())?.to_string();
            let ctx = m
                .get("context_length")
                .and_then(|x| x.as_i64())
                .or_else(|| m.get("max_context_length").and_then(|x| x.as_i64()))
                .or_else(|| builtin_map.get(&id).map(|(c, _)| *c));
            let out = m
                .get("max_output")
                .and_then(|x| x.as_i64())
                .or_else(|| builtin_map.get(&id).map(|(_, o)| *o));
            Some(ModelSpec {
                id,
                name: None,
                context_length: ctx,
                max_output: out,
            })
        })
        .collect();
    Ok(specs)
}

/// 新增自定义 provider，返回新 provider key（由调用方指定的可读标识）
#[tauri::command]
pub fn add_provider(
    name: String,
    kind: String,
    base_url: String,
    api_key: String,
    provider_id: String,
) -> Result<String, String> {
    let mut config = config_file::read_config().map_err(|e| e.to_string())?;
    let providers = config
        .get_mut("provider")
        .and_then(|p| p.as_object_mut())
        .ok_or_else(|| "config.json 无 provider 对象".to_string())?;
    let id = provider_id.trim().to_string();
    if id.is_empty() {
        return Err("供应商标识不能为空".into());
    }
    if providers.contains_key(&id) {
        return Err(format!("供应商标识「{id}」已存在"));
    }
    let provider = json!({
        "name": name,
        "kind": kind,
        "options": { "apiKey": api_key, "baseURL": base_url, "apiKeyRequired": true },
        "source": "custom",
        "enabled": true,
        "models": {}
    });
    providers.insert(id.clone(), provider);
    config_file::write_config(&config).map_err(|e| e.to_string())?;
    Ok(id)
}

/// 删除 provider
#[tauri::command]
pub fn remove_provider(provider_key: String) -> Result<(), String> {
    let mut config = config_file::read_config().map_err(|e| e.to_string())?;
    let providers = config
        .get_mut("provider")
        .and_then(|p| p.as_object_mut())
        .ok_or_else(|| "config.json 无 provider 对象".to_string())?;
    if providers.remove(&provider_key).is_none() {
        return Err("provider 不存在".into());
    }
    config_file::write_config(&config).map_err(|e| e.to_string())
}

/// 更新 provider 的 name / kind / baseURL / apiKey（写回 config.json）
/// 传 None 的字段保持原值不变；apiKey 传空串视为不改，传 "<REDACTED>" 也忽略。
#[tauri::command]
pub fn update_provider(
    provider_key: String,
    name: Option<String>,
    kind: Option<String>,
    base_url: Option<String>,
    api_key: Option<String>,
) -> Result<(), String> {
    let mut config = config_file::read_config().map_err(|e| e.to_string())?;
    let provider = config
        .get_mut("provider")
        .and_then(|p| p.get_mut(&provider_key))
        .ok_or_else(|| "provider 不存在".to_string())?;
    let obj = provider
        .as_object_mut()
        .ok_or_else(|| "provider 非对象".to_string())?;
    if let Some(n) = name {
        if !n.is_empty() {
            obj.insert("name".into(), json!(n));
        }
    }
    if let Some(k) = kind {
        if !k.is_empty() {
            obj.insert("kind".into(), json!(k));
        }
    }
    if let Some(b) = base_url {
        if !b.is_empty() {
            let opts = obj
                .entry("options".to_string())
                .or_insert_with(|| json!({}))
                .as_object_mut()
                .ok_or_else(|| "options 非对象".to_string())?;
            opts.insert("baseURL".into(), json!(b));
        }
    }
    if let Some(k) = api_key {
        // 空串 / 脱敏占位都视为不改
        if !k.is_empty() && k != "<REDACTED>" {
            let opts = obj
                .entry("options".to_string())
                .or_insert_with(|| json!({}))
                .as_object_mut()
                .ok_or_else(|| "options 非对象".to_string())?;
            opts.insert("apiKey".into(), json!(k));
        }
    }
    config_file::write_config(&config).map_err(|e| e.to_string())
}

/// 更新某 model 的上下文 / 输出上限（写入 config.json）
#[tauri::command]
pub fn update_model_limit(
    provider_key: String,
    model_name: String,
    context: Option<i64>,
    output: Option<i64>,
) -> Result<(), String> {
    let mut config = config_file::read_config().map_err(|e| e.to_string())?;
    let models = config
        .get_mut("provider")
        .and_then(|p| p.get_mut(&provider_key))
        .and_then(|prov| prov.get_mut("models"))
        .and_then(|m| m.as_object_mut())
        .ok_or_else(|| "未找到 provider / models".to_string())?;
    let entry = models
        .entry(model_name)
        .or_insert_with(|| json!({}));
    let obj = entry
        .as_object_mut()
        .ok_or_else(|| "model 项非对象".to_string())?;
    let limit = obj
        .entry("limit".to_string())
        .or_insert_with(|| json!({}));
    let limit_obj = limit
        .as_object_mut()
        .ok_or_else(|| "limit 非对象".to_string())?;
    if let Some(c) = context {
        limit_obj.insert("context".into(), json!(c));
    }
    if let Some(o) = output {
        limit_obj.insert("output".into(), json!(o));
    }
    config_file::write_config(&config).map_err(|e| e.to_string())
}

/// 批量把拉取到的模型写入 provider.models（合并并设 limit/modalities，单次写盘）
#[tauri::command]
pub fn apply_models(provider_key: String, specs: Vec<ModelSpec>) -> Result<usize, String> {
    let mut config = config_file::read_config().map_err(|e| e.to_string())?;
    let models_obj = config
        .get_mut("provider")
        .and_then(|p| p.get_mut(&provider_key))
        .and_then(|prov| prov.get_mut("models"))
        .and_then(|m| m.as_object_mut())
        .ok_or_else(|| "未找到 provider / models".to_string())?;
    let mut n = 0;
    for s in &specs {
        let entry = models_obj
            .entry(s.id.clone())
            .or_insert_with(|| json!({}));
        if let Some(obj) = entry.as_object_mut() {
            let limit = obj
                .entry("limit".to_string())
                .or_insert_with(|| json!({}));
            if let Some(limit_obj) = limit.as_object_mut() {
                if let Some(c) = s.context_length {
                    limit_obj.insert("context".into(), json!(c));
                }
                if let Some(o) = s.max_output {
                    limit_obj.insert("output".into(), json!(o));
                }
            }
            obj.entry("modalities".to_string())
                .or_insert(json!({ "input": ["text"], "output": ["text"] }));
            n += 1;
        }
    }
    config_file::write_config(&config).map_err(|e| e.to_string())?;
    Ok(n)
}

/// 切换 provider 的启用状态（写入 config.json 的 enabled 字段）
#[tauri::command]
pub fn set_provider_enabled(provider_key: String, enabled: bool) -> Result<(), String> {
    let mut config = config_file::read_config().map_err(|e| e.to_string())?;
    let provider = config
        .get_mut("provider")
        .and_then(|p| p.get_mut(&provider_key))
        .ok_or_else(|| "provider 不存在".to_string())?;
    let obj = provider
        .as_object_mut()
        .ok_or_else(|| "provider 非对象".to_string())?;
    obj.insert("enabled".into(), json!(enabled));
    config_file::write_config(&config).map_err(|e| e.to_string())
}

/// 切换单个 model 的启用状态（写入 config.json 的 model.enabled 字段）
#[tauri::command]
pub fn set_model_enabled(
    provider_key: String,
    model_name: String,
    enabled: bool,
) -> Result<(), String> {
    let mut config = config_file::read_config().map_err(|e| e.to_string())?;
    let model = config
        .get_mut("provider")
        .and_then(|p| p.get_mut(&provider_key))
        .and_then(|prov| prov.get_mut("models"))
        .and_then(|m| m.get_mut(&model_name))
        .ok_or_else(|| "未找到 provider / model".to_string())?;
    let obj = model
        .as_object_mut()
        .ok_or_else(|| "model 项非对象".to_string())?;
    obj.insert("enabled".into(), json!(enabled));
    config_file::write_config(&config).map_err(|e| e.to_string())
}

/// 删除 provider 下的单个 model（写回 config.json）
#[tauri::command]
pub fn remove_model(provider_key: String, model_name: String) -> Result<(), String> {
    let mut config = config_file::read_config().map_err(|e| e.to_string())?;
    let models = config
        .get_mut("provider")
        .and_then(|p| p.get_mut(&provider_key))
        .and_then(|prov| prov.get_mut("models"))
        .and_then(|m| m.as_object_mut())
        .ok_or_else(|| "未找到 provider / models".to_string())?;
    if models.remove(&model_name).is_none() {
        return Err("model 不存在".into());
    }
    config_file::write_config(&config).map_err(|e| e.to_string())
}

/// 取某 provider 的明文 apiKey（用于 UI「查看 key」，不脱敏）
#[tauri::command]
pub fn get_provider_api_key(provider_key: String) -> Result<String, String> {
    let config = config_file::read_config().map_err(|e| e.to_string())?;
    Ok(config_file::provider_api_key(&config, &provider_key).unwrap_or_default())
}

/// 标记/取消标记某 provider 为 Coding Plan 订阅（存 db，独立于 config.json）
#[tauri::command]
pub fn set_provider_coding_plan(
    state: State<'_, AppState>,
    provider_key: String,
    enabled: bool,
) -> Result<(), String> {
    state
        .db
        .set_provider_coding_plan(&provider_key, enabled)
        .map_err(|e| e.to_string())
}

/// 列出所有被标记为 Coding Plan 的 provider key
#[tauri::command]
pub fn list_coding_plan_providers(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    state
        .db
        .list_coding_plan_provider_keys()
        .map_err(|e| e.to_string())
}

/// 测试 provider 连接结果（供「添加供应商」弹窗的「测试」按钮）
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestResult {
    pub ok: bool,
    pub message: String,
    pub model_count: Option<usize>,
}

/// 用填写的 baseURL + apiKey 调 /models 验证连通性（不落盘、不依赖已保存的 provider）。
/// 供「添加供应商」弹窗在保存前测试配置是否可用。
#[tauri::command]
pub async fn test_provider_connection(
    state: State<'_, AppState>,
    base_url: String,
    api_key: String,
    kind: String,
) -> Result<TestResult, String> {
    let _ = &kind; // 预留：不同协议未来可用不同测试端点
    let base = base_url.trim();
    if base.is_empty() {
        return Err("Base URL 不能为空".into());
    }
    let url = models_endpoint(base);
    let client = state.client();
    let resp = match client.get(&url).bearer_auth(&api_key).send().await {
        Ok(r) => r,
        Err(e) => {
            return Ok(TestResult {
                ok: false,
                message: format!("连接失败：{e}"),
                model_count: None,
            })
        }
    };
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        let snippet: String = body.chars().take(200).collect();
        return Ok(TestResult {
            ok: false,
            message: format!("HTTP {} {}", status.as_u16(), snippet),
            model_count: None,
        });
    }
    let v: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            return Ok(TestResult {
                ok: false,
                message: format!("响应解析失败：{e}"),
                model_count: None,
            })
        }
    };
    let arr = v
        .get("data")
        .and_then(|d| d.as_array())
        .or_else(|| v.as_array());
    let count = arr.map(|a| a.len());
    Ok(TestResult {
        ok: true,
        message: match count {
            Some(n) => format!("连接成功，发现 {} 个模型", n),
            None => "连接成功".to_string(),
        },
        model_count: count,
    })
}
