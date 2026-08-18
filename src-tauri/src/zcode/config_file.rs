//! config.json / setting.json 读写（原子写）+ apiKey 脱敏
use crate::zcode::paths;
use anyhow::{Context, Result};
use serde_json::{Map, Value};
use std::path::PathBuf;

/// 读取 JSON 文件为 serde_json::Value
pub fn read_json(p: &Option<PathBuf>) -> Result<Value> {
    let p = p.as_ref().context("配置路径未定位")?;
    let txt =
        std::fs::read_to_string(p).with_context(|| format!("读取失败: {}", p.display()))?;
    Ok(serde_json::from_str(&txt).context("JSON 解析失败")?)
}

/// 原子写 JSON（先写 .tmp 再 rename）
pub fn write_json_atomic(p: &Option<PathBuf>, v: &Value) -> Result<()> {
    let p = p.as_ref().context("配置路径未定位")?;
    let txt = serde_json::to_string_pretty(v)?;
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, format!("{txt}\n"))
        .with_context(|| format!("写入临时文件失败: {}", tmp.display()))?;
    std::fs::rename(&tmp, p)
        .with_context(|| format!("替换文件失败: {}", p.display()))?;
    Ok(())
}

/// 读取 config.json
pub fn read_config() -> Result<Value> {
    read_json(&paths::config_path())
}

/// 读取 setting.json（可能不存在 → 返回空对象）
pub fn read_setting() -> Result<Value> {
    match read_json(&paths::setting_path()) {
        Ok(v) => Ok(v),
        Err(_) => Ok(Value::Object(Map::new())),
    }
}

/// 写回 config.json
pub fn write_config(v: &Value) -> Result<()> {
    write_json_atomic(&paths::config_path(), v)
}

/// 写回 setting.json
pub fn write_setting(v: &Value) -> Result<()> {
    write_json_atomic(&paths::setting_path(), v)
}

/// 把 config.json 中所有 provider 的 apiKey 替换为脱敏串，避免明文回传前端。
pub fn redact_config(mut config: Value) -> Value {
    if let Some(providers) = config.get_mut("provider").and_then(|p| p.as_object_mut()) {
        for (_key, prov) in providers.iter_mut() {
            if let Some(opts) = prov.get_mut("options").and_then(|o| o.as_object_mut()) {
                if opts.get("apiKey").is_some() {
                    opts.insert("apiKey".into(), Value::String("<REDACTED>".into()));
                }
            }
        }
    }
    config
}

/// 取某个 provider 的明文 apiKey（后端内部用，不回传）
pub fn provider_api_key(config: &Value, provider_key: &str) -> Option<String> {
    config
        .get("provider")?
        .get(provider_key)?
        .get("options")?
        .get("apiKey")?
        .as_str()
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty() && s != "<REDACTED>")
}

/// 取某个 provider 的 baseURL
pub fn provider_base_url(config: &Value, provider_key: &str) -> Option<String> {
    config
        .get("provider")?
        .get(provider_key)?
        .get("options")?
        .get("baseURL")?
        .as_str()
        .map(|s| s.to_string())
}

/// 取某个 provider 的展示名（无 name 字段时回退 key 本身）
pub fn provider_name(config: &Value, provider_key: &str) -> String {
    config
        .get("provider")
        .and_then(|p| p.get(provider_key))
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or(provider_key)
        .to_string()
}

/// 读取某 family 当前选中的 provider key（原始值，可能带 mode 前缀）
pub fn current_selected(setting: &Value, family: &str) -> Option<String> {
    setting
        .get("modelProviderFamilySelectedKeys")?
        .get(family)?
        .as_str()
        .map(String::from)
}

/// 把 setting 里的 selectedKey 还原为 config providerId。
/// selectedKey 形如 `coding-plan:builtin:bigmodel-coding-plan`（带 mode 前缀），
/// 也可能是裸 id 或 `builtin:xxx`（首段即 builtin）。
/// 规则：若首个 `:` 之前不是 `builtin`，则去掉首段前缀；否则原样返回。
pub fn selected_to_provider(selected: &str) -> String {
    match selected.split_once(':') {
        Some((prefix, rest)) if prefix != "builtin" && !rest.is_empty() => rest.to_string(),
        _ => selected.to_string(),
    }
}

/// 解析当前选中的 provider key：setting.json → family → selectedKey → 去 mode 前缀
pub fn current_provider_key() -> Option<String> {
    let setting = read_setting().ok()?;
    let family = setting.get("providerFamilyDomain")?.as_str()?;
    let selected = current_selected(&setting, family)?;
    Some(selected_to_provider(&selected))
}
