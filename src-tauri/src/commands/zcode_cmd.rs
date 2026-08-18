//! zcode 配置读写 / 探测 / 重启 / 切换 provider
use crate::zcode::{config_file, paths, process};
use serde_json::Value;
use std::sync::OnceLock;
use tauri::{AppHandle, Emitter};

/// 读取 config.json（apiKey 已脱敏，安全回传前端）。async：页面挂载即调。
#[tauri::command]
pub async fn get_zcode_config() -> Result<Value, String> {
    let cfg = config_file::read_config().map_err(|e| e.to_string())?;
    Ok(config_file::redact_config(cfg))
}

/// 读取 setting.json。async：页面挂载即调。
#[tauri::command]
pub async fn get_zcode_setting() -> Result<Value, String> {
    config_file::read_setting().map_err(|e| e.to_string())
}

/// 探测 ZCode 安装路径 / 运行状态 / 配置目录。
/// async + 结果缓存：内部 spawn wmic/tasklist/reg 同步等待外部进程（1~5s），
/// 即使跑在线程池，从 GUI 进程反复 spawn console 子进程仍会造成窗口卡顿
/// （设置页每次挂载都会调用）；安装路径几乎不变，首次探测后缓存复用。
#[tauri::command]
pub async fn probe_zcode() -> Value {
    static CACHE: OnceLock<Value> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            serde_json::json!({
                "exePath": paths::find_zcode_exe(),
                "running": paths::is_zcode_running(),
                "configDir": paths::config_dir_str(),
            })
        })
        .clone()
}

/// 重启 ZCode（kill + relaunch）。async：内部 spawn 进程，避免阻塞主线程。
#[tauri::command]
pub async fn restart_zcode() -> Result<(), String> {
    process::restart()
}

/// 触发 ZCode「Developer: Reload Window」温和重读配置（不杀进程，保留打开的文件）。
/// 供配置变更后自动让 ZCode 用上新 provider/模型，无需用户手动重启。
/// async：内部枚举窗口 + 键盘模拟耗时，避免阻塞主线程。
#[tauri::command]
pub async fn reload_zcode_window() -> Result<(), String> {
    if process::reload_window() {
        Ok(())
    } else {
        Err("ZCode 重载窗口失败（未找到 ZCode 进程/窗口，或键盘模拟未成功）".into())
    }
}

/// 键盘模拟切换 ZCode 当前会话的模型（免重启，下一轮对话生效）。
/// 位置计算见 process::model_menu_position（与自动切换共用）；
/// model_key 仅 builtin 套餐目标需要（定位套餐内具体模型），自定义供应商可省略。
/// async：内部枚举窗口 + 键盘模拟耗时，避免阻塞主线程。
#[tauri::command]
pub async fn switch_zcode_model(
    provider_key: String,
    model_key: Option<String>,
) -> Result<(), String> {
    let pos = process::model_menu_position(&provider_key, model_key.as_deref())?;
    process::switch_model_window(pos)
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
