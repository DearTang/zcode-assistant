//! Oh My Pi（Pi Coding Agent）原生 models.json 的双向同步。
//!
//! 只同步 providers 与其 models；不改 settings.json 的 defaultProvider/defaultModel，
//! 避免同步操作意外切换正在使用的模型。
use crate::commands::models_cmd::{add_provider, apply_models, update_provider, ModelSpec};
use crate::zcode::config_file;
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

const DEFAULT_OUTPUT_LENGTH: i64 = crate::openrouter::DEFAULT_OUTPUT_LENGTH;

#[derive(Clone)]
struct PiModel {
    id: String,
    name: Option<String>,
    context_window: Option<i64>,
    max_tokens: Option<i64>,
}

#[derive(Clone)]
struct PiProvider {
    id: String,
    name: String,
    base_url: String,
    api_key: String,
    api: String,
    models: Vec<PiModel>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OmpProviderPreview {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub base_url: String,
    pub models: Vec<String>,
    pub has_api_key: bool,
    pub duplicate_of: Option<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OmpImportResult {
    pub name: String,
    pub kind: String,
    pub base_url: String,
    pub models: Vec<String>,
    pub source: String,
    pub status: String,
    pub provider_key: String,
    pub message: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OmpExportPreview {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub model_count: usize,
    pub has_api_key: bool,
    pub enabled: bool,
    pub duplicate_of: Option<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OmpExportResult {
    pub name: String,
    pub status: String,
    pub target_key: String,
    pub message: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OmpExportOutcome {
    pub results: Vec<OmpExportResult>,
    pub warning: Option<String>,
}

/// Pi 配置目录定位：PI_CODING_AGENT_DIR 环境变量优先，否则 ~/.pi/agent
fn pi_agent_dir() -> Result<PathBuf, String> {
    if let Some(dir) = std::env::var_os("PI_CODING_AGENT_DIR").filter(|s| !s.is_empty()) {
        return Ok(PathBuf::from(dir));
    }
    dirs::home_dir()
        .map(|home| home.join(".pi").join("agent"))
        .ok_or_else(|| "无 HOME，无法定位 Oh My Pi 配置目录".to_string())
}

fn pi_models_path() -> Result<PathBuf, String> {
    Ok(pi_agent_dir()?.join("models.json"))
}

/// Pi 的 api 值 → zcode provider kind（anthropic 端点保留，其余按 OpenAI 兼容）
fn kind_for_pi_api(api: &str, base_url: &str) -> String {
    let api = api.to_ascii_lowercase();
    let base = base_url.to_ascii_lowercase();
    if api.contains("anthropic") || base.contains("/anthropic") || base.contains("anthropic.") {
        "anthropic".into()
    } else {
        "openai-compatible".into()
    }
}

/// zcode provider kind → Pi 的 api 值；目标侧已有非空 api 时优先保留
fn pi_api_for_zcode(kind: &str, existing_api: Option<&str>) -> String {
    if let Some(api) = existing_api.filter(|api| !api.trim().is_empty()) {
        return api.to_string();
    }
    if kind == "anthropic" {
        "anthropic-messages".into()
    } else {
        "openai-completions".into()
    }
}

fn read_pi_config(path: &Path) -> Result<Value, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("读取 Oh My Pi 配置失败（{}）: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("Oh My Pi models.json 解析失败: {e}"))
}

fn pi_providers(config: &Value) -> Result<Vec<PiProvider>, String> {
    let providers = config
        .get("providers")
        .and_then(Value::as_object)
        .ok_or("Oh My Pi models.json 无 providers 对象")?;
    Ok(providers
        .iter()
        .filter_map(|(id, value)| {
            let obj = value.as_object()?;
            let base_url = obj.get("baseUrl")?.as_str()?.trim().to_string();
            if base_url.is_empty() {
                return None;
            }
            let api = obj
                .get("api")
                .and_then(Value::as_str)
                .unwrap_or("openai-completions")
                .to_string();
            let name = obj
                .get("name")
                .and_then(Value::as_str)
                .filter(|name| !name.trim().is_empty())
                .unwrap_or(id)
                .to_string();
            let api_key = obj
                .get("apiKey")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string();
            let models = obj
                .get("models")
                .and_then(Value::as_array)
                .map(|models| {
                    models
                        .iter()
                        .filter_map(|model| {
                            let model = model.as_object()?;
                            let id = model.get("id")?.as_str()?.trim();
                            if id.is_empty() {
                                return None;
                            }
                            Some(PiModel {
                                id: id.to_string(),
                                name: model
                                    .get("name")
                                    .and_then(Value::as_str)
                                    .filter(|name| !name.trim().is_empty())
                                    .map(ToString::to_string),
                                context_window: model
                                    .get("contextWindow")
                                    .and_then(Value::as_i64)
                                    .filter(|value| *value > 0),
                                max_tokens: model
                                    .get("maxTokens")
                                    .and_then(Value::as_i64)
                                    .filter(|value| *value > 0),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            Some(PiProvider {
                id: id.to_string(),
                name,
                base_url,
                api_key,
                api,
                models,
            })
        })
        .collect())
}

fn normalize_base_url(base_url: &str) -> String {
    base_url.trim().trim_end_matches('/').to_ascii_lowercase()
}

/// 与既有导入口径一致：baseURL 归一化相等 + apiKey 非空且完全一致才命中
fn matching_zcode_provider(config: &Value, base_url: &str, api_key: &str) -> Option<String> {
    let api_key = api_key.trim();
    if api_key.is_empty() {
        return None;
    }
    let wanted_base = normalize_base_url(base_url);
    config
        .get("provider")?
        .as_object()?
        .iter()
        .find_map(|(key, provider)| {
            let options = provider.get("options")?;
            let candidate_key = options.get("apiKey")?.as_str()?.trim();
            let candidate_base = options.get("baseURL")?.as_str()?;
            (candidate_key == api_key
                && !candidate_key.is_empty()
                && normalize_base_url(candidate_base) == wanted_base)
                .then(|| key.clone())
        })
}

fn matching_pi_provider(
    providers: &Map<String, Value>,
    base_url: &str,
    api_key: &str,
) -> Option<String> {
    let api_key = api_key.trim();
    if api_key.is_empty() {
        return None;
    }
    let wanted_base = normalize_base_url(base_url);
    providers.iter().find_map(|(key, provider)| {
        let candidate_base = provider.get("baseUrl")?.as_str()?;
        let candidate_key = provider
            .get("apiKey")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        (candidate_key == api_key
            && !candidate_key.is_empty()
            && normalize_base_url(candidate_base) == wanted_base)
            .then(|| key.clone())
    })
}

fn slugify(value: &str) -> String {
    let result: String = value
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let result = result.trim_matches('-');
    if result.is_empty() {
        "provider".into()
    } else {
        result.into()
    }
}

fn unique_key(base: &str, taken: impl Fn(&str) -> bool) -> String {
    let mut key = base.to_string();
    let mut suffix = 1;
    while taken(&key) {
        suffix += 1;
        key = format!("{base}-{suffix}");
    }
    key
}

fn specs_from_pi(models: &[PiModel]) -> Vec<ModelSpec> {
    models
        .iter()
        .map(|model| ModelSpec {
            id: model.id.clone(),
            name: model.name.clone(),
            context_length: model.context_window,
            max_output: model.max_tokens,
        })
        .collect()
}

/// 只读解析 OMP 原生 models.json，并标记将覆盖的 zcode provider
#[tauri::command]
pub fn omp_import_preview(path: Option<String>) -> Result<Vec<OmpProviderPreview>, String> {
    let path = path.map(PathBuf::from).unwrap_or(pi_models_path()?);
    let pi = read_pi_config(&path)?;
    let config = config_file::read_config().map_err(|e| e.to_string())?;
    let providers = pi_providers(&pi)?;
    if providers.is_empty() {
        return Err("Oh My Pi models.json 中没有可导入的 provider".into());
    }
    Ok(providers
        .into_iter()
        .map(|provider| OmpProviderPreview {
            id: provider.id,
            name: provider.name,
            kind: kind_for_pi_api(&provider.api, &provider.base_url),
            base_url: provider.base_url.clone(),
            models: provider.models.into_iter().map(|model| model.id).collect(),
            has_api_key: !provider.api_key.is_empty(),
            duplicate_of: matching_zcode_provider(&config, &provider.base_url, &provider.api_key),
        })
        .collect())
}

/// 把选中的 OMP provider 导入 zcode（模型名 / contextWindow / maxTokens 一并写入）。
/// 不读取或修改 Pi 的 settings.json。
#[tauri::command]
pub fn import_providers_from_omp(
    path: Option<String>,
    selected: Option<Vec<String>>,
) -> Result<Vec<OmpImportResult>, String> {
    let path = path.map(PathBuf::from).unwrap_or(pi_models_path()?);
    let pi = read_pi_config(&path)?;
    let mut providers = pi_providers(&pi)?;
    if let Some(selected) = selected {
        providers.retain(|provider| selected.iter().any(|id| id == &provider.id));
    }
    if providers.is_empty() {
        return Err("未选择任何 Oh My Pi 供应商".into());
    }

    let mut results = Vec::new();
    for provider in providers {
        let kind = kind_for_pi_api(&provider.api, &provider.base_url);
        let model_ids: Vec<String> = provider.models.iter().map(|m| m.id.clone()).collect();
        let snapshot = config_file::read_config().map_err(|e| e.to_string())?;
        let duplicate = matching_zcode_provider(&snapshot, &provider.base_url, &provider.api_key);
        let (target_key, status, message) = if let Some(key) = duplicate {
            update_provider(
                key.clone(),
                Some(provider.name.clone()),
                Some(kind.clone()),
                Some(provider.base_url.clone()),
                Some(provider.api_key.clone()),
            )
            .map_err(|e| format!("覆盖更新 Oh My Pi 供应商失败: {e}"))?;
            let count = apply_models(key.clone(), specs_from_pi(&provider.models))
                .map_err(|e| format!("合并 Oh My Pi 模型失败: {e}"))?;
            (
                key,
                "updated".into(),
                format!("已覆盖并合并 {count} 个模型"),
            )
        } else {
            let taken: Vec<String> = snapshot
                .get("provider")
                .and_then(Value::as_object)
                .map(|providers| providers.keys().cloned().collect())
                .unwrap_or_default();
            let key = unique_key(&slugify(&provider.id), |candidate| {
                taken.iter().any(|id| id == candidate)
            });
            let key = add_provider(
                provider.name.clone(),
                kind.clone(),
                provider.base_url.clone(),
                provider.api_key.clone(),
                key,
            )
            .map_err(|e| format!("写入 Oh My Pi 供应商失败: {e}"))?;
            let count = apply_models(key.clone(), specs_from_pi(&provider.models))
                .map_err(|e| format!("写入 Oh My Pi 模型失败: {e}"))?;
            (key, "success".into(), format!("已写入 {count} 个模型"))
        };
        results.push(OmpImportResult {
            name: provider.name,
            kind,
            base_url: provider.base_url,
            models: model_ids,
            source: "omp".into(),
            status,
            provider_key: target_key,
            message,
        });
    }
    Ok(results)
}

#[derive(Clone)]
struct ZcodeExportEntry {
    id: String,
    name: String,
    base_url: String,
    api_key: String,
    kind: String,
    enabled: bool,
    models: Vec<Value>,
}

/// zcode 模型条目 → Pi 模型白名单字段（id / name / contextWindow / maxTokens）。
/// 仅 context 有效时写限制；output 缺失按 131072 兜底（Pi 侧两者需齐备）；
/// reasoning / zcode / enabled 等 zcode 专有字段一律丢弃。
fn to_pi_model(model_id: &str, model: &Value) -> Value {
    let object = model.as_object();
    let name = object
        .and_then(|object| object.get("name"))
        .and_then(Value::as_str)
        .filter(|name| !name.trim().is_empty());
    let limit = object
        .and_then(|object| object.get("limit"))
        .and_then(Value::as_object);
    let context = limit
        .and_then(|limit| limit.get("context"))
        .and_then(Value::as_i64)
        .filter(|value| *value > 0);
    let max_tokens = limit
        .and_then(|limit| limit.get("output"))
        .and_then(Value::as_i64)
        .filter(|value| *value > 0);
    let mut output = Map::new();
    output.insert("id".into(), json!(model_id));
    if let Some(name) = name {
        output.insert("name".into(), json!(name));
    }
    if let Some(context) = context {
        output.insert("contextWindow".into(), json!(context));
        output.insert(
            "maxTokens".into(),
            json!(max_tokens.unwrap_or(DEFAULT_OUTPUT_LENGTH)),
        );
    }
    Value::Object(output)
}

/// 读取 zcode 配置中可同步到 OMP 的 provider（跳过 builtin: 与无 baseURL 的）
fn zcode_export_entries() -> Result<Vec<ZcodeExportEntry>, String> {
    let config = config_file::read_config().map_err(|e| e.to_string())?;
    let providers = config
        .get("provider")
        .and_then(Value::as_object)
        .ok_or("zcode config.json 无 provider 对象")?;
    Ok(providers
        .iter()
        .filter(|(key, _)| !key.starts_with("builtin:"))
        .filter_map(|(id, provider)| {
            let options = provider.get("options")?;
            let base_url = options.get("baseURL")?.as_str()?.trim().to_string();
            if base_url.is_empty() {
                return None;
            }
            let api_key = options
                .get("apiKey")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string();
            let name = provider
                .get("name")
                .and_then(Value::as_str)
                .filter(|name| !name.is_empty())
                .unwrap_or(id)
                .to_string();
            let kind = provider
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or("openai-compatible")
                .to_string();
            let models = provider
                .get("models")
                .and_then(Value::as_object)
                .map(|models| {
                    models
                        .iter()
                        .map(|(model_id, model)| to_pi_model(model_id, model))
                        .collect()
                })
                .unwrap_or_default();
            Some(ZcodeExportEntry {
                id: id.clone(),
                name,
                base_url,
                api_key,
                kind,
                enabled: provider
                    .get("enabled")
                    .and_then(Value::as_bool)
                    .unwrap_or(true),
                models,
            })
        })
        .collect())
}

/// 目标侧 Pi provider 条目：保留未知扩展字段（如 createdAt 等），覆盖核心字段
/// name / baseUrl / api / apiKey / models；api 优先沿用目标侧已有值。
fn merge_pi_provider(existing: Option<&Value>, e: &ZcodeExportEntry) -> Map<String, Value> {
    let existing_obj = existing.and_then(Value::as_object);
    let api = pi_api_for_zcode(
        &e.kind,
        existing_obj
            .and_then(|provider| provider.get("api"))
            .and_then(Value::as_str),
    );
    let mut output = existing_obj.cloned().unwrap_or_default();
    output.insert("name".into(), json!(e.name));
    output.insert("baseUrl".into(), json!(e.base_url));
    output.insert("api".into(), json!(api));
    output.insert("apiKey".into(), json!(e.api_key));
    output.insert("models".into(), Value::Array(e.models.clone()));
    output
}

/// 只读预览 zcode → Oh My Pi 的覆盖关系
#[tauri::command]
pub fn omp_export_preview() -> Result<Vec<OmpExportPreview>, String> {
    let path = pi_models_path()?;
    let pi = read_pi_config(&path)?;
    let target = pi
        .get("providers")
        .and_then(Value::as_object)
        .ok_or("Oh My Pi models.json 无 providers 对象")?;
    let entries = zcode_export_entries()?;
    if entries.is_empty() {
        return Err("zcode 配置中没有可同步到 Oh My Pi 的自定义供应商".into());
    }
    Ok(entries
        .into_iter()
        .map(|entry| OmpExportPreview {
            id: entry.id,
            name: entry.name,
            base_url: entry.base_url.clone(),
            model_count: entry.models.len(),
            has_api_key: !entry.api_key.is_empty(),
            enabled: entry.enabled,
            duplicate_of: matching_pi_provider(target, &entry.base_url, &entry.api_key),
        })
        .collect())
}

/// 写入前滚动备份（models.json.bak），再原子替换 models.json
fn write_pi_config(path: &Path, config: &Value) -> Result<(), String> {
    let backup = path.with_extension("json.bak");
    std::fs::copy(path, &backup).map_err(|e| format!("备份 Oh My Pi models.json 失败: {e}"))?;
    let temp = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    std::fs::write(&temp, format!("{text}\n"))
        .map_err(|e| format!("写入 Oh My Pi 临时配置失败: {e}"))?;
    std::fs::rename(&temp, path).map_err(|e| format!("替换 Oh My Pi models.json 失败: {e}"))
}

/// 把选中的 zcode provider 写入 OMP 的原生 models.json（写前生成 .bak 备份）
#[tauri::command]
pub fn export_providers_to_omp(selected: Option<Vec<String>>) -> Result<OmpExportOutcome, String> {
    let path = pi_models_path()?;
    if !path.exists() {
        return Err(format!(
            "未找到 Oh My Pi 配置（{}），请先安装或初始化 Oh My Pi",
            path.display()
        ));
    }
    let mut config = read_pi_config(&path)?;
    let providers = config
        .get_mut("providers")
        .and_then(Value::as_object_mut)
        .ok_or("Oh My Pi models.json 无 providers 对象")?;
    let mut entries = zcode_export_entries()?;
    if let Some(selected) = selected {
        entries.retain(|entry| selected.iter().any(|id| id == &entry.id));
    }
    if entries.is_empty() {
        return Err("未选择任何要同步到 Oh My Pi 的供应商".into());
    }

    let mut results = Vec::new();
    for entry in entries {
        let existing_key = matching_pi_provider(providers, &entry.base_url, &entry.api_key);
        let target_key = existing_key.clone().unwrap_or_else(|| {
            unique_key(&entry.id, |candidate| providers.contains_key(candidate))
        });
        let output = merge_pi_provider(providers.get(&target_key), &entry);
        providers.insert(target_key.clone(), Value::Object(output));
        let (status, message) = if existing_key.is_some() {
            (
                "updated".into(),
                format!("已覆盖 Oh My Pi 供应商「{target_key}」"),
            )
        } else {
            (
                "success".into(),
                format!("已新增 Oh My Pi 供应商「{target_key}」"),
            )
        };
        results.push(OmpExportResult {
            name: entry.name,
            status,
            target_key,
            message,
        });
    }
    write_pi_config(&path, &config)?;
    Ok(OmpExportOutcome {
        results,
        warning: Some(
            "已写入 models.json（.bak 备份已生成）；请重启或重新加载 Oh My Pi 生效，未改动 settings.json 的默认供应商/模型".into(),
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pi_provider_and_model_limits() {
        let config: Value = serde_json::from_str(
            r#"{"providers":{"mini":{"name":"MiniMax","baseUrl":"https://api.example/v1","api":"openai-completions","apiKey":"key","models":[{"id":"m1","name":"Model 1","contextWindow":200000,"maxTokens":8192},{"id":"bad"},{"id":"","contextWindow":1}]}}}"#,
        )
        .unwrap();
        let providers = pi_providers(&config).unwrap();
        assert_eq!(providers.len(), 1);
        // 空 id 模型被跳过；无限制字段的模型（bad）合法保留
        assert_eq!(providers[0].models.len(), 2);
        assert_eq!(providers[0].models[0].id, "m1");
        assert_eq!(providers[0].models[0].name.as_deref(), Some("Model 1"));
        assert_eq!(providers[0].models[0].context_window, Some(200_000));
        assert_eq!(providers[0].models[0].max_tokens, Some(8_192));
        assert_eq!(providers[0].models[1].id, "bad");
        assert_eq!(providers[0].models[1].context_window, None);
    }

    #[test]
    fn pi_providers_requires_providers_object() {
        let config: Value = serde_json::from_str(r#"{"something":1}"#).unwrap();
        assert!(pi_providers(&config).is_err());
    }

    #[test]
    fn matches_only_nonempty_matching_credentials() {
        let config: Value = serde_json::from_str(
            r#"{"providers":{"one":{"baseUrl":"https://API.example/v1/","apiKey":"secret"},"empty":{"baseUrl":"https://api.example/v1","apiKey":""}}}"#,
        )
        .unwrap();
        let providers = config.get("providers").unwrap().as_object().unwrap();
        // 大小写 + 末尾斜杠归一化后命中
        assert_eq!(
            matching_pi_provider(providers, "https://api.example/v1", "secret"),
            Some("one".into())
        );
        // 空 key 不参与判定
        assert_eq!(
            matching_pi_provider(providers, "https://api.example/v1", ""),
            None
        );
        // key 不一致不命中
        assert_eq!(
            matching_pi_provider(providers, "https://api.example/v1", "other"),
            None
        );
    }

    #[test]
    fn zcode_matching_normalizes_base_url() {
        let config: Value = serde_json::from_str(
            r#"{"provider":{"custom":{"options":{"baseURL":"https://API.example/v1/","apiKey":"secret"}}}}"#,
        )
        .unwrap();
        assert_eq!(
            matching_zcode_provider(&config, "https://api.example/v1", "secret"),
            Some("custom".into())
        );
        assert_eq!(
            matching_zcode_provider(&config, "https://api.example/v1", ""),
            None
        );
    }

    #[test]
    fn to_pi_model_whitelists_and_fills_missing_output() {
        let model: Value = serde_json::from_str(
            r#"{"name":"Model 1","limit":{"context":200000},"reasoning":{"enabled":true},"zcode":{"modified":true},"enabled":false}"#,
        )
        .unwrap();
        let out = to_pi_model("m1", &model);
        assert_eq!(out.get("id").and_then(Value::as_str), Some("m1"));
        assert_eq!(out.get("name").and_then(Value::as_str), Some("Model 1"));
        assert_eq!(
            out.get("contextWindow").and_then(Value::as_i64),
            Some(200_000)
        );
        assert_eq!(
            out.get("maxTokens").and_then(Value::as_i64),
            Some(DEFAULT_OUTPUT_LENGTH)
        );
        // zcode 专有字段不外带
        assert!(out.get("reasoning").is_none());
        assert!(out.get("zcode").is_none());
        assert!(out.get("enabled").is_none());
    }

    #[test]
    fn to_pi_model_omits_limits_without_valid_context() {
        let model: Value = serde_json::from_str(r#"{"limit":{"output":100}}"#).unwrap();
        let out = to_pi_model("m1", &model);
        assert_eq!(out.get("id").and_then(Value::as_str), Some("m1"));
        assert!(out.get("contextWindow").is_none());
        assert!(out.get("maxTokens").is_none());
    }

    #[test]
    fn merge_preserves_unknown_fields_and_existing_api() {
        let existing: Value = serde_json::from_str(
            r#"{"name":"old","baseUrl":"https://api.example/v1","api":"custom-api","apiKey":"secret","createdAt":123,"models":[]}"#,
        )
        .unwrap();
        let entry = ZcodeExportEntry {
            id: "custom".into(),
            name: "Custom".into(),
            base_url: "https://api.example/v1".into(),
            api_key: "secret".into(),
            kind: "openai-compatible".into(),
            enabled: true,
            models: vec![json!({"id": "m1"})],
        };
        let merged = merge_pi_provider(Some(&existing), &entry);
        // 未知扩展字段保留
        assert_eq!(merged.get("createdAt").and_then(Value::as_i64), Some(123));
        // 目标侧已有 api 优先保留，不被默认值覆盖
        assert_eq!(
            merged.get("api").and_then(Value::as_str),
            Some("custom-api")
        );
        // 核心字段被 zcode 侧覆盖
        assert_eq!(merged.get("name").and_then(Value::as_str), Some("Custom"));
        assert_eq!(
            merged.get("models").and_then(Value::as_array).map(Vec::len),
            Some(1)
        );
    }

    #[test]
    fn merge_defaults_api_from_kind_when_missing() {
        let entry = ZcodeExportEntry {
            id: "custom".into(),
            name: "Custom".into(),
            base_url: "https://api.example/anthropic".into(),
            api_key: "secret".into(),
            kind: "anthropic".into(),
            enabled: true,
            models: vec![],
        };
        let merged = merge_pi_provider(None, &entry);
        assert_eq!(
            merged.get("api").and_then(Value::as_str),
            Some("anthropic-messages")
        );
    }

    #[test]
    fn unique_key_appends_suffixes() {
        let taken = |k: &str| k == "mini" || k == "mini-2";
        assert_eq!(unique_key("mini", taken), "mini-3");
        assert_eq!(unique_key("fresh", taken), "fresh");
    }
}
