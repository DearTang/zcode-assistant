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
