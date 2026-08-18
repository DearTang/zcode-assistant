//! 智谱账号管理命令
use crate::accounts;
use crate::state::AppState;
use crate::types::AccountMeta;
use tauri::{AppHandle, Emitter, Manager, State};

/// async：账号页挂载即调，避免同步执行占用主线程。
#[tauri::command]
pub async fn list_accounts(state: State<'_, AppState>) -> Result<Vec<AccountMeta>, String> {
    state.db.list_accounts().map_err(|e| e.to_string())
}

/// async：捕获账号涉及文件读写。
#[tauri::command]
pub async fn capture_account(
    state: State<'_, AppState>,
    label: String,
) -> Result<AccountMeta, String> {
    accounts::capture(&state.db, &state.data_dir, &label).map_err(|e| e.to_string())
}

/// 切换账号：kill → 写 credentials/config（accounts::switch 不拉起）→
/// 按「切换后提示重启」偏好决定：开启=弹 RestartDialog 由用户确认重启（默认）；
/// 关闭=直接拉起 ZCode（旧行为，免提示）。
/// async：内部含 sleep + spawn 进程，同步执行会冻结主线程数百毫秒以上。
#[tauri::command]
pub async fn switch_account(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<AccountMeta, String> {
    let meta =
        accounts::switch(&state.db, &state.data_dir, &id).map_err(|e| e.to_string())?;
    if crate::commands::prefs_cmd::current_prefs(&state.db).switch_restart_zcode {
        // 弹窗挂在主窗口，主窗口可能在托盘里——先带出来再发事件
        // （async 上下文里窗口操作经 run_on_main_thread 派发，安全）
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.show();
            let _ = w.unminimize();
            let _ = w.set_focus();
        }
        let _ = app.emit(
            "zcode://restart-requested",
            serde_json::json!({
                "reason": "账号已切换（ZCode 已关闭），重启 ZCode 后以新账号运行",
            }),
        );
    } else {
        std::thread::sleep(std::time::Duration::from_millis(500));
        crate::zcode::process::launch_zcode().map_err(|e| {
            // 文件已写入，但 ZCode 没能自动重启——明确告知用户手动启动
            format!("账号已切换，但 ZCode 未能自动重新启动：{e}（请手动打开 ZCode）")
        })?;
    }
    Ok(meta)
}

/// async：涉及凭据文件删除。
#[tauri::command]
pub async fn remove_account(state: State<'_, AppState>, id: String) -> Result<(), String> {
    accounts::remove(&state.db, &state.data_dir, &id).map_err(|e| e.to_string())
}

/// async：页面挂载即调，避免同步执行占用主线程。
#[tauri::command]
pub async fn rename_account(
    state: State<'_, AppState>,
    id: String,
    label: String,
) -> Result<(), String> {
    state
        .db
        .rename_account(&id, &label)
        .map_err(|e| e.to_string())
}

/// async：页面挂载即调（读凭据文件比对当前账号）。
#[tauri::command]
pub async fn current_account(state: State<'_, AppState>) -> Result<Option<AccountMeta>, String> {
    Ok(accounts::current(&state.db))
}
