//! 应用偏好命令：悬浮球可见性 / 配额展示方案 / 开机自启。
//! 持久化于 DB kv 表（重启后仍生效）；变更时广播 `prefs://updated`，
//! 主窗口 / 悬浮球 / 悬浮面板各自监听即时联动。
use crate::db::Database;
use crate::state::AppState;
use crate::types::AppPrefs;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt;

pub const KV_FLOAT_BALL: &str = "float_ball_visible";
pub const KV_USAGE_DISPLAY: &str = "usage_display";
pub const KV_SWITCH_RESTART: &str = "switch_restart_zcode";
pub const KV_AUTOSTART: &str = "autostart";

/// 从 DB 读取当前偏好（缺省：悬浮球显示、展示已用量、切换后提示重启、不自启）
pub fn current_prefs(db: &Database) -> AppPrefs {
    let float_ball_visible = db
        .kv_get(KV_FLOAT_BALL)
        .map(|v| v != "0")
        .unwrap_or(true);
    let usage_display = db
        .kv_get(KV_USAGE_DISPLAY)
        .filter(|v| v == "used" || v == "remaining")
        .unwrap_or_else(|| "used".to_string());
    let switch_restart_zcode = db
        .kv_get(KV_SWITCH_RESTART)
        .map(|v| v != "0")
        .unwrap_or(true);
    let autostart = db.kv_get(KV_AUTOSTART).map(|v| v != "0").unwrap_or(false);
    AppPrefs {
        float_ball_visible,
        usage_display,
        switch_restart_zcode,
        autostart,
    }
}

/// async：设置页挂载即调，避免同步执行占用主线程。
/// 开机自启以 OS 实际状态为准（用户可能从系统设置中手动关闭），同步回 kv 后再返回。
#[tauri::command]
pub async fn get_prefs(app: AppHandle, state: State<'_, AppState>) -> Result<AppPrefs, String> {
    if let Ok(actual) = app.autolaunch().is_enabled() {
        let _ = state.db.kv_set(KV_AUTOSTART, if actual { "1" } else { "0" });
    }
    Ok(current_prefs(&state.db))
}

/// 显示/隐藏悬浮球：持久化偏好 + 控制窗口 + 广播。
/// async：内部可能创建悬浮球窗口（build 需 dispatch 到主线程，同步命令会死锁）。
#[tauri::command]
pub async fn set_float_ball_visible(app: AppHandle, visible: bool) -> Result<(), String> {
    let prefs = {
        let state = app.state::<AppState>();
        state
            .db
            .kv_set(KV_FLOAT_BALL, if visible { "1" } else { "0" })
            .map_err(|e| e.to_string())?;
        current_prefs(&state.db)
    };
    if visible {
        crate::float_ball::show(&app).map_err(|e| e.to_string())?;
    } else {
        // 隐藏球时连带收起展开面板，避免面板悬空
        crate::float_ball::hide_panel(&app);
        crate::float_ball::hide(&app);
    }
    let _ = app.emit("prefs://updated", prefs);
    Ok(())
}

/// 设置配额展示方案（used=已用量 / remaining=剩余用量）：持久化 + 广播。
#[tauri::command]
pub fn set_usage_display(
    app: AppHandle,
    state: State<'_, AppState>,
    mode: String,
) -> Result<(), String> {
    if mode != "used" && mode != "remaining" {
        return Err("展示方案仅支持 used / remaining".to_string());
    }
    state
        .db
        .kv_set(KV_USAGE_DISPLAY, &mode)
        .map_err(|e| e.to_string())?;
    let _ = app.emit("prefs://updated", current_prefs(&state.db));
    Ok(())
}

/// 设置「切换后提示重启 ZCode」（自动切换 / 账号切换共用）：持久化 + 广播。
#[tauri::command]
pub fn set_switch_restart_zcode(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    state
        .db
        .kv_set(KV_SWITCH_RESTART, if enabled { "1" } else { "0" })
        .map_err(|e| e.to_string())?;
    let _ = app.emit("prefs://updated", current_prefs(&state.db));
    Ok(())
}

/// 设置开机自启动：注册 / 注销系统自启项 + 持久化偏好 + 广播。
#[tauri::command]
pub async fn set_autostart(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    let manager = app.autolaunch();
    if enabled {
        manager.enable().map_err(|e| e.to_string())?;
    } else {
        manager.disable().map_err(|e| e.to_string())?;
    }
    state
        .db
        .kv_set(KV_AUTOSTART, if enabled { "1" } else { "0" })
        .map_err(|e| e.to_string())?;
    let _ = app.emit("prefs://updated", current_prefs(&state.db));
    Ok(())
}
