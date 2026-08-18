//! 项目 / 会话管理命令（读写 zcode cli db 与任务索引）
use tauri::State;

use crate::sessions;
use crate::state::AppState;
use crate::types::{ZcDeleteResult, ZcProject, ZcSession};

/// 全部项目（含会话数 / 对话次数 / token 汇总 / 时间），按最近活跃倒序。
/// async：跨库聚合查询耗时，同步执行会冻结主线程（页面挂载即调用）。
#[tauri::command]
pub async fn zc_projects() -> Result<Vec<ZcProject>, String> {
    sessions::list_projects().map_err(|e| e.to_string())
}

/// 某项目下的顶层会话列表（消耗含子代理后代）。
/// async：跨库聚合查询耗时，避免阻塞主线程。
#[tauri::command]
pub async fn zc_sessions(project_id: String) -> Result<Vec<ZcSession>, String> {
    sessions::list_sessions(&project_id).map_err(|e| e.to_string())
}

/// 修改会话名称（写 session.title 并标记 custom，zcode 不再自动覆盖）
#[tauri::command]
pub fn zc_rename_session(session_id: String, title: String) -> Result<(), String> {
    sessions::rename_session(&session_id, &title).map_err(|e| e.to_string())
}

/// 归档会话（写 time_archived + 任务索引 archived=1，zcode 会话列表隐藏）
#[tauri::command]
pub fn zc_archive_session(session_id: String) -> Result<(), String> {
    sessions::archive_session(&session_id).map_err(|e| e.to_string())
}

/// 恢复归档会话（清任务索引 archived/deleted 与会话库 time_archived，
/// 回到 zcode 会话列表可继续对话）
#[tauri::command]
pub fn zc_restore_session(session_id: String) -> Result<(), String> {
    sessions::restore_session(&session_id).map_err(|e| e.to_string())
}

/// 归档整个项目（批量归档其全部活跃顶层会话），返回本次归档的会话数
#[tauri::command]
pub fn zc_archive_project(project_id: String) -> Result<usize, String> {
    sessions::archive_project(&project_id).map_err(|e| e.to_string())
}

/// 恢复整个项目（清该项目全部会话的归档标记），返回恢复的会话数
#[tauri::command]
pub fn zc_restore_project(project_id: String) -> Result<usize, String> {
    sessions::restore_project(&project_id).map_err(|e| e.to_string())
}

/// 批量删除会话 / 项目（级联删除消息与用量，连带清理任务索引、rollout 文件与本地用量记录）
#[tauri::command]
pub fn zc_delete(
    state: State<'_, AppState>,
    session_ids: Option<Vec<String>>,
    project_ids: Option<Vec<String>>,
) -> Result<ZcDeleteResult, String> {
    sessions::delete(
        &state.db,
        session_ids.as_deref().unwrap_or_default(),
        project_ids.as_deref().unwrap_or_default(),
    )
    .map_err(|e| e.to_string())
}
