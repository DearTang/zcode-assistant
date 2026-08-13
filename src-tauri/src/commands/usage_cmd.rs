//! 用量查询命令：解析 zcode rollout → 聚合统计
use crate::state::AppState;
use crate::types::{UsageAggRow, UsageFilters, UsageOverview, UsageRecord, UsageSyncResult};
use crate::usage;
use tauri::State;

/// 同步：扫描 rollout 文件，增量写入 usage_records，返回同步结果。
/// `full=true` 时忽略 30 天窗口、回填全部历史；默认仅同步最近 30 天。
#[tauri::command]
pub fn usage_sync(state: State<'_, AppState>, full: Option<bool>) -> Result<UsageSyncResult, String> {
    usage::sync_rollout(&state.db, full.unwrap_or(false)).map_err(|e| e.to_string())
}

/// 筛选项（去重的供应商/模型/角色 + 日期范围 + 总条数）
#[tauri::command]
pub fn usage_filters(state: State<'_, AppState>) -> Result<UsageFilters, String> {
    state.db.usage_filters().map_err(|e| e.to_string())
}

/// 整体汇总（随筛选条件）
#[tauri::command]
pub fn usage_overview(
    state: State<'_, AppState>,
    from: Option<String>,
    to: Option<String>,
    provider: Option<String>,
    model: Option<String>,
    role: Option<String>,
) -> Result<UsageOverview, String> {
    state
        .db
        .usage_overview(
            from.as_deref(),
            to.as_deref(),
            provider.as_deref(),
            model.as_deref(),
            role.as_deref(),
        )
        .map_err(|e| e.to_string())
}

/// 分组聚合（按供应商 / 模型 / 日期）
#[tauri::command]
pub fn usage_aggregate(
    state: State<'_, AppState>,
    group_by: String,
    from: Option<String>,
    to: Option<String>,
    provider: Option<String>,
    model: Option<String>,
    role: Option<String>,
) -> Result<Vec<UsageAggRow>, String> {
    state
        .db
        .usage_aggregate(
            &group_by,
            from.as_deref(),
            to.as_deref(),
            provider.as_deref(),
            model.as_deref(),
            role.as_deref(),
        )
        .map_err(|e| e.to_string())
}

/// 明细记录（分页，按时间倒序）
#[tauri::command]
pub fn usage_records(
    state: State<'_, AppState>,
    from: Option<String>,
    to: Option<String>,
    provider: Option<String>,
    model: Option<String>,
    role: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Vec<UsageRecord>, String> {
    state
        .db
        .usage_records_list(
            from.as_deref(),
            to.as_deref(),
            provider.as_deref(),
            model.as_deref(),
            role.as_deref(),
            limit.unwrap_or(200),
            offset.unwrap_or(0),
        )
        .map_err(|e| e.to_string())
}
