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

/// 内置智谱规格表查询（大小写不敏感）：命中返回 (context, output)
pub(crate) fn builtin_spec(model_id: &str) -> Option<(i64, i64)> {
    let id = model_id.trim().to_lowercase();
    BUILTIN
        .iter()
        .find(|(b, _, _)| *b == id)
        .map(|(_, c, o)| (*c, *o))
}

/// 解析模型真实规格（权威值，命中即应覆盖已有配置）：
/// OpenRouter 目录模糊匹配 → 内置智谱规格表。
/// 命中返回 Some((context, output))；output 仅内置表命中时有值。
pub(crate) fn matched_spec(
    catalog: &Option<crate::openrouter::Catalog>,
    model_id: &str,
) -> Option<(i64, Option<i64>)> {
    if let Some(ctx) = catalog
        .as_ref()
        .and_then(|c| crate::openrouter::fuzzy_context(&c.models, model_id))
    {
        // 目录命中：上下文 + 输出都取自目录（输出已在解析时兜底为 131072）
        let out = catalog
            .as_ref()
            .and_then(|c| crate::openrouter::fuzzy_output(&c.models, model_id));
        return Some((ctx, out));
    }
    builtin_spec(model_id).map(|(c, o)| (c, Some(o)))
}

/// 拼 /models 端点（智谱 anthropic 端点用 paas/v4，其余按 OpenAI 兼容约定）
/// 供拉取模型 / 连接测试 / 可用性健康检测共用
pub(crate) fn models_endpoint(base: &str) -> String {
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
    // OpenRouter 目录：真实上下文（按模型名模糊匹配，优先于内置写死表）
    let or_catalog = crate::openrouter::load_catalog(&state.db);

    let specs: Vec<ModelSpec> = arr
        .iter()
        .filter_map(|m| {
            let id = m.get("id").and_then(|x| x.as_str())?.to_string();
            let ctx = m
                .get("context_length")
                .and_then(|x| x.as_i64())
                .or_else(|| m.get("max_context_length").and_then(|x| x.as_i64()))
                .or_else(|| {
                    or_catalog
                        .as_ref()
                        .and_then(|c| crate::openrouter::fuzzy_context(&c.models, &id))
                })
                .or_else(|| builtin_map.get(&id).map(|(c, _)| *c));
            let out = m
                .get("max_output")
                .and_then(|x| x.as_i64())
                .or_else(|| m.get("max_completion_tokens").and_then(|x| x.as_i64()))
                .or_else(|| {
                    or_catalog
                        .as_ref()
                        .and_then(|c| crate::openrouter::fuzzy_output(&c.models, &id))
                })
                .or_else(|| builtin_map.get(&id).map(|(_, o)| *o))
                .unwrap_or(crate::openrouter::DEFAULT_OUTPUT_LENGTH);
            Some(ModelSpec {
                id,
                name: None,
                context_length: ctx,
                max_output: Some(out),
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

/// 删除 provider（若是主供应商则同步清除标记，总览回退自动识别）
#[tauri::command]
pub fn remove_provider(
    state: State<'_, AppState>,
    provider_key: String,
) -> Result<(), String> {
    let mut config = config_file::read_config().map_err(|e| e.to_string())?;
    let providers = config
        .get_mut("provider")
        .and_then(|p| p.as_object_mut())
        .ok_or_else(|| "config.json 无 provider 对象".to_string())?;
    if providers.remove(&provider_key).is_none() {
        return Err("provider 不存在".into());
    }
    config_file::write_config(&config).map_err(|e| e.to_string())?;
    let _ = state.db.clear_primary_if(&provider_key);
    Ok(())
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

/// 拖拽排序后重排 provider 的键顺序（写回 config.json）。
///
/// ordered_keys 是用户可见供应商（非 builtin）的新顺序子集。
/// 采用「槽位重排」：ordered_keys 中的项在其原有槽位里按新顺序填入，
/// 其余 provider（builtin:、systemDisabled 等）保持原位不变。
/// 依赖 serde_json 的 preserve_order feature（IndexMap 保留插入顺序）。
#[tauri::command]
pub fn reorder_providers(ordered_keys: Vec<String>) -> Result<(), String> {
    let mut config = config_file::read_config().map_err(|e| e.to_string())?;
    let providers = config
        .get("provider")
        .and_then(|p| p.as_object())
        .ok_or_else(|| "config.json 无 provider 对象".to_string())?;

    // 全量 key 当前顺序
    let all_keys: Vec<String> = providers.keys().cloned().collect();
    // ordered_keys 中各 key 的目标序号
    let order_map: std::collections::HashMap<&String, usize> =
        ordered_keys.iter().enumerate().map(|(i, k)| (k, i)).collect();

    // 校验：ordered_keys 中的 key 必须都存在于 config
    for k in &ordered_keys {
        if !providers.contains_key(k) {
            return Err(format!("排序清单含未知 provider「{k}」"));
        }
    }

    // 槽位重排：找出 all_keys 中属于 ordered_keys 子集的位置，按新顺序填入
    let mut new_all = all_keys.clone();
    let slots: Vec<usize> = all_keys
        .iter()
        .enumerate()
        .filter(|(_, k)| order_map.contains_key(k))
        .map(|(i, _)| i)
        .collect();
    for (slot_idx, new_key) in slots.iter().zip(ordered_keys.iter()) {
        new_all[*slot_idx] = new_key.clone();
    }

    // 按新顺序重建 Map
    let mut new_map = serde_json::Map::new();
    for key in &new_all {
        if let Some(v) = providers.get(key) {
            new_map.insert(key.clone(), v.clone());
        }
    }
    config
        .as_object_mut()
        .ok_or_else(|| "config 顶层非对象".to_string())?
        .insert("provider".into(), Value::Object(new_map));
    config_file::write_config(&config).map_err(|e| e.to_string())
}

/// 拖拽排序后重排某 provider 下 models 的顺序（写回 config.json）。
///
/// 与 reorder_providers 同款「槽位重排」：ordered_names 中的模型在原有槽位里
/// 按新顺序填入，未包含的模型保持原位。依赖 serde_json 的 preserve_order。
/// 同时按最终顺序写入每个模型的 zcode.priority + zcode.modified=true——
/// ZCode 按这两个字段决定模型展示顺序与本地条目存留，仅键顺序不生效。
#[tauri::command]
pub fn reorder_models(
    provider_key: String,
    ordered_names: Vec<String>,
) -> Result<(), String> {
    let mut config = config_file::read_config().map_err(|e| e.to_string())?;
    let provider = config
        .get_mut("provider")
        .and_then(|p| p.get_mut(&provider_key))
        .ok_or_else(|| "provider 不存在".to_string())?;
    let models = provider
        .get("models")
        .and_then(|m| m.as_object())
        .ok_or_else(|| "未找到 provider / models".to_string())?;

    for n in &ordered_names {
        if !models.contains_key(n) {
            return Err(format!("排序清单含未知模型「{n}」"));
        }
    }

    // 全量模型名当前顺序
    let all_names: Vec<String> = models.keys().cloned().collect();
    let order_map: std::collections::HashMap<&String, usize> = ordered_names
        .iter()
        .enumerate()
        .map(|(i, n)| (n, i))
        .collect();

    let mut new_all = all_names.clone();
    let slots: Vec<usize> = all_names
        .iter()
        .enumerate()
        .filter(|(_, n)| order_map.contains_key(n))
        .map(|(i, _)| i)
        .collect();
    for (slot_idx, new_name) in slots.iter().zip(ordered_names.iter()) {
        new_all[*slot_idx] = new_name.clone();
    }

    // 按新顺序重建 Map 并整体替换 models
    let mut new_map = serde_json::Map::new();
    for name in &new_all {
        if let Some(v) = models.get(name) {
            new_map.insert(name.clone(), v.clone());
        }
    }

    // 除键顺序外，还要写入每个模型的 zcode.priority（按新顺序 0,1,2...）与
    // zcode.modified=true：ZCode 读取后按 sortModelEntriesByPriority 以
    // zcode.priority 升序展示（无 priority 的排在最后，键顺序仅作兜底），且
    // 与服务端下发的权威模型条目合并时，只有 modified=true 的本地条目会保留。
    // 只调键顺序不写 priority，ZCode 侧不会生效。
    for (i, name) in new_all.iter().enumerate() {
        let Some(v) = new_map.get_mut(name) else { continue };
        let Some(obj) = v.as_object_mut() else { continue };
        let zcode = obj
            .entry("zcode".to_string())
            .or_insert_with(|| json!({}));
        let Some(zobj) = zcode.as_object_mut() else { continue };
        zobj.insert("priority".into(), json!(i as i64));
        zobj.insert("modified".into(), json!(true));
    }

    let obj = provider
        .as_object_mut()
        .ok_or_else(|| "provider 非对象".to_string())?;
    obj.insert("models".into(), Value::Object(new_map));
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

/// 设置/取消主供应商（全局唯一，存 db，独立于 config.json）。
/// 总览 / 悬浮窗 / 托盘的配额展示均跟随主供应商。
#[tauri::command]
pub fn set_provider_primary(
    state: State<'_, AppState>,
    provider_key: String,
    enabled: bool,
) -> Result<(), String> {
    state
        .db
        .set_provider_primary(&provider_key, enabled)
        .map_err(|e| e.to_string())
}

/// 取主供应商 key（未设置返回 null，总览回退自动识别智谱 Coding Plan）
#[tauri::command]
pub fn get_primary_provider(state: State<'_, AppState>) -> Result<Option<String>, String> {
    state
        .db
        .primary_provider_key()
        .map_err(|e| e.to_string())
}

/// 启动引导：best-effort 选一个主供应商写入 DB + setting.json，确保总览/悬浮窗有数据源。
/// 优先级：
///   1) 已设过主供应商且仍存在 → 不动（仅同步 setting.json 的 family 选中）
///   2) 智谱 Coding Plan 内置订阅 provider（账号登录态写入 config）→ 选中
///   3) 否则取第一个 enabled 且带至少一个模型的非 builtin 供应商 → 选中
///   4) 都没有 → 返回 None（保持主供应商为空，外部走空概览，不报错）
#[tauri::command]
pub fn bootstrap_primary(state: State<'_, AppState>) -> Result<Option<String>, String> {
    let current = state.db.primary_provider_key().ok().flatten();
    let cfg = config_file::read_config().map_err(|e| e.to_string())?;
    let providers = cfg
        .get("provider")
        .and_then(|p| p.as_object());

    let exists = |k: &str| providers.is_some_and(|m| m.contains_key(k));

    if let Some(k) = current.clone() {
        if exists(&k) {
            ensure_setting_family_selected(&k)?;
            return Ok(Some(k));
        }
    }

    let pick_coding_plan = || {
        providers.and_then(|m| {
            m.keys()
                .find(|k| k.contains("coding-plan"))
                .cloned()
        })
    };
    let pick_first_user_provider = || {
        providers.and_then(|m| {
            m.iter()
                .find(|(k, v)| {
                    !k.starts_with("builtin:")
                        && v.get("enabled").and_then(|x| x.as_bool()).unwrap_or(false)
                        && v.get("models")
                            .and_then(|x| x.as_object())
                            .map(|o| !o.is_empty())
                            .unwrap_or(false)
                })
                .map(|(k, _)| k.clone())
        })
    };

    let chosen = pick_coding_plan().or_else(pick_first_user_provider);
    if let Some(k) = chosen {
        state
            .db
            .set_provider_primary(&k, true)
            .map_err(|e| e.to_string())?;
        ensure_setting_family_selected(&k)?;
        Ok(Some(k))
    } else {
        Ok(None)
    }
}

/// 把 setting.json 的 providerFamilyDomain 与 modelProviderFamilySelectedKeys[family]
/// 指向给定 provider（已对应该 provider 时不动）。
fn ensure_setting_family_selected(provider_key: &str) -> Result<(), String> {
    let mut setting: Value = config_file::read_setting().unwrap_or_else(|_| json!({}));
    let obj = setting
        .as_object_mut()
        .ok_or_else(|| "setting.json 顶层非对象".to_string())?;
    let family = "bigmodel";

    let already_points_to = obj
        .get("modelProviderFamilySelectedKeys")
        .and_then(|m| m.get(family))
        .and_then(|v| v.as_str())
        .map(|s| {
            // 兼容带 mode 前缀（coding-plan:builtin:xxx）的 selectedKey
            s.rsplit(':').next().unwrap_or(s) == provider_key
        })
        .unwrap_or(false);

    if obj.get("providerFamilyDomain").and_then(|v| v.as_str()) == Some(family)
        && already_points_to
    {
        return Ok(());
    }

    obj.insert("providerFamilyDomain".into(), json!(family));
    obj.entry("modelProviderFamilySelectedKeys".to_string())
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .map(|m| m.insert(family.into(), json!(provider_key)))
        .ok_or_else(|| "setting.json 的 modelProviderFamilySelectedKeys 非对象".to_string())?;
    config_file::write_setting(&setting).map_err(|e| e.to_string())
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
