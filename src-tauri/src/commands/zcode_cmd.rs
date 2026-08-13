//! zcode 配置读写 / 探测 / 重启 / 切换 provider
use crate::zcode::{config_file, paths, process};
use serde_json::Value;
use tauri::{AppHandle, Emitter};

/// 读取 config.json（apiKey 已脱敏，安全回传前端）
#[tauri::command]
pub fn get_zcode_config() -> Result<Value, String> {
    let cfg = config_file::read_config().map_err(|e| e.to_string())?;
    Ok(config_file::redact_config(cfg))
}

/// 读取 setting.json
#[tauri::command]
pub fn get_zcode_setting() -> Result<Value, String> {
    config_file::read_setting().map_err(|e| e.to_string())
}

/// 探测 ZCode 安装路径 / 运行状态 / 配置目录
#[tauri::command]
pub fn probe_zcode() -> Value {
    serde_json::json!({
        "exePath": paths::find_zcode_exe(),
        "running": paths::is_zcode_running(),
        "configDir": paths::config_dir_str(),
    })
}

/// 重启 ZCode（kill + relaunch）
#[tauri::command]
pub fn restart_zcode() -> Result<(), String> {
    if process::restart() {
        Ok(())
    } else {
        Err("ZCode 重启失败（未找到 exe 或未能关闭）".into())
    }
}

/// 切换当前选中 provider（写 setting.json）
#[tauri::command]
pub fn select_provider(
    app: AppHandle,
    family: String,
    provider_key: String,
) -> Result<(), String> {
    let mut setting = config_file::read_setting().map_err(|e| e.to_string())?;
    let obj = setting
        .as_object_mut()
        .ok_or_else(|| "setting.json 非对象".to_string())?;
    obj.insert("providerFamilyDomain".into(), Value::String(family.clone()));
    let keys = obj
        .entry("modelProviderFamilySelectedKeys".to_string())
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    if let Some(m) = keys.as_object_mut() {
        m.insert(family, Value::String(provider_key.clone()));
    }
    config_file::write_setting(&setting).map_err(|e| e.to_string())?;
    // 广播“当前模型已切换”：供前端多窗口同步 / 后续“已生效”提示驱动
    let _ = app.emit(
        "model://switched",
        serde_json::json!({ "providerKey": provider_key }),
    );
    Ok(())
}
