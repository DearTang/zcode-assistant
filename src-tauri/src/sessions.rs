//! 项目管理：直接读写 zcode 的 cli/db/db.sqlite（session / turn_usage / model_usage 表）。
//!
//! 口径说明：
//! - 「会话窗口」= session 表中 parent_id 为空的顶层会话；subagent_child 子会话归入其根会话统计；
//! - 「对话次数」= turn_usage 轮次数；「token 消耗」= model_usage 汇总（与「用量查询」页同口径）；
//! - 读走 WAL 只读连接；改名 / 删除走短事务读写连接（busy_timeout 兜底，不与 zcode 长期抢锁）。
//!
//! 删除采用 schema 自带的 ON DELETE CASCADE（message / part / model_usage / turn_usage /
//! tool_usage / todo / session_entry / session_input 随会话级联），并顺带清理 rollout
//! 的 model-io-sess_*.jsonl 与本地 usage_records，保持用量页一致。
//!
//! 注意：本文件 SQL 多用 `\` 续行拼接，该写法会吞掉换行与下一行前导空格，
//! 因此行尾必须保留一个空格（同 usage.rs 的约定），否则关键字会粘连。
use chrono::Local;
use rusqlite::{params, Connection};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Duration;

use crate::types::{ZcDeleteResult, ZcProject, ZcSession};
use crate::zcode::{config_file, paths};

/// SQLite IN 列表单批参数上限（999 限制留余量）
const CHUNK: usize = 400;

fn min_opt(a: Option<i64>, b: Option<i64>) -> Option<i64> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x.min(y)),
        (x, y) => x.or(y),
    }
}

fn max_opt(a: Option<i64>, b: Option<i64>) -> Option<i64> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x.max(y)),
        (x, y) => x.or(y),
    }
}

/// 任务索引库（tasks-index.sqlite）中已归档 / 已删除的会话 id 集合。
/// zcode 界面的「归档」写的就是这里的 tasks.archived（task_id = session.id）；
/// 打不开索引库时返回空集（退化为仅用 session.time_archived 判定）。
fn task_archived_set() -> anyhow::Result<HashSet<String>> {
    let mut set = HashSet::new();
    let Some(path) = paths::tasks_index_db_path() else {
        return Ok(set);
    };
    let Ok(conn) = crate::usage::open_readonly(Path::new(&path)) else {
        return Ok(set);
    };
    let mut stmt = conn.prepare("SELECT task_id FROM tasks WHERE archived=1 OR deleted=1")?;
    for id in stmt.query_map([], |r| r.get::<_, String>(0))?.flatten() {
        set.insert(id);
    }
    Ok(set)
}

/// 任务索引库读写连接（清归档 / 删索引行用）
fn open_tasks_rw() -> anyhow::Result<Connection> {
    let path = paths::tasks_index_db_path()
        .ok_or_else(|| anyhow::anyhow!("未找到 zcode 任务索引（~/.zcode/v2/tasks-index.sqlite）"))?;
    let conn = Connection::open(path)?;
    conn.busy_timeout(Duration::from_millis(5000))?;
    Ok(conn)
}

/// 全部项目（含汇总统计），按最近活跃倒序。
pub fn list_projects() -> anyhow::Result<Vec<ZcProject>> {
    let conn = open_ro()?;
    // 归档标记：zcode 界面的真实归档存在任务索引库（tasks.archived/deleted），
    // 会话库的 time_archived 作为兜底口径合并
    let archived_ids = task_archived_set()?;

    // 顶层会话逐行取出后在 Rust 侧聚合（量级仅百级，便于跨库合并归档标记）
    let mut stmt = conn.prepare(
        "SELECT project_id, directory, id, time_archived IS NOT NULL, time_created, time_updated \
         FROM session WHERE parent_id IS NULL",
    )?;
    struct Acc {
        directory: String,
        sessions: i64,
        archived: i64,
        created: Option<i64>,
        updated: Option<i64>,
    }
    let mut map: HashMap<String, Acc> = HashMap::new();
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, bool>(3)?,
            r.get::<_, Option<i64>>(4)?,
            r.get::<_, Option<i64>>(5)?,
        ))
    })?;
    for (pid, dir, sid, ts_archived, created, updated) in rows.flatten() {
        let archived = ts_archived || archived_ids.contains(&sid);
        let acc = map.entry(pid).or_insert(Acc {
            directory: dir,
            sessions: 0,
            archived: 0,
            created: None,
            updated: None,
        });
        acc.sessions += 1;
        if archived {
            acc.archived += 1;
        }
        acc.created = min_opt(acc.created, created);
        acc.updated = max_opt(acc.updated, updated);
    }
    drop(stmt);

    let mut list: Vec<ZcProject> = map
        .into_iter()
        .map(|(pid, a)| ZcProject {
            id: pid.clone(),
            directory: a.directory,
            sessions: a.sessions,
            archived_sessions: a.archived,
            time_created_ms: a.created,
            time_updated_ms: a.updated,
            ..Default::default()
        })
        .collect();

    // 对话轮次（turn_usage 按项目汇总）
    let mut stmt = conn.prepare(
        "SELECT s.project_id, COUNT(t.turn_id) FROM turn_usage t \
         JOIN session s ON t.session_id = s.id GROUP BY s.project_id",
    )?;
    for (pid, turns) in stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
        .flatten()
    {
        if let Some(p) = list.iter_mut().find(|p| p.id == pid) {
            p.turns = turns;
        }
    }
    drop(stmt);

    // token 消耗（model_usage 按项目汇总，与用量页同口径）
    let mut stmt = conn.prepare(
        "SELECT s.project_id, COUNT(*), COALESCE(SUM(m.input_tokens),0), \
         COALESCE(SUM(m.output_tokens),0), COALESCE(SUM(m.computed_total_tokens),0) \
         FROM model_usage m JOIN session s ON m.session_id = s.id GROUP BY s.project_id",
    )?;
    for (pid, calls, i, o, t) in stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
            ))
        })?
        .flatten()
    {
        if let Some(p) = list.iter_mut().find(|p| p.id == pid) {
            p.calls = calls;
            p.input_tokens = i;
            p.output_tokens = o;
            p.total_tokens = t;
        }
    }

    list.sort_by(|a, b| b.time_updated_ms.cmp(&a.time_updated_ms));
    Ok(list)
}

/// 某项目下的顶层会话（消耗含子代理后代），按最近活跃倒序
pub fn list_sessions(project_id: &str) -> anyhow::Result<Vec<ZcSession>> {
    let conn = open_ro()?;
    let archived_ids = task_archived_set()?;

    let mut stmt = conn.prepare(
        "SELECT id, project_id, title, title_source, directory, task_type, \
         time_created, time_updated, time_archived \
         FROM session WHERE project_id = ?1 AND parent_id IS NULL ORDER BY time_updated DESC",
    )?;
    let mut list: Vec<ZcSession> = stmt
        .query_map(params![project_id], |r| {
            let id: String = r.get(0)?;
            let ts_archived: Option<i64> = r.get(8)?;
            let archived = ts_archived.is_some() || archived_ids.contains(&id);
            Ok(ZcSession {
                id,
                project_id: r.get(1)?,
                title: r.get(2)?,
                title_source: r.get(3)?,
                directory: r.get(4)?,
                task_type: r.get(5)?,
                archived,
                time_created_ms: r.get(6)?,
                time_updated_ms: r.get(7)?,
                time_archived_ms: ts_archived,
                ..Default::default()
            })
        })?
        .flatten()
        .collect();
    drop(stmt);

    // 根会话 → 全部后代（含自身）的树，两个聚合分别按根汇总
    let tree = "WITH RECURSIVE tree(root, sid) AS ( \
        SELECT id, id FROM session WHERE project_id = ?1 AND parent_id IS NULL \
        UNION ALL SELECT t.root, s.id FROM session s JOIN tree t ON s.parent_id = t.sid)";

    let mut stmt = conn.prepare(&format!(
        "{tree} SELECT t.root, COUNT(tu.turn_id) \
         FROM tree t LEFT JOIN turn_usage tu ON tu.session_id = t.sid GROUP BY t.root"
    ))?;
    for (root, turns) in stmt
        .query_map(params![project_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?
        .flatten()
    {
        if let Some(s) = list.iter_mut().find(|s| s.id == root) {
            s.turns = turns;
        }
    }
    drop(stmt);

    let mut stmt = conn.prepare(&format!(
        "{tree} SELECT t.root, COUNT(m.id), COALESCE(SUM(m.input_tokens),0), \
         COALESCE(SUM(m.output_tokens),0), COALESCE(SUM(m.computed_total_tokens),0) \
         FROM tree t LEFT JOIN model_usage m ON m.session_id = t.sid GROUP BY t.root"
    ))?;
    for (root, calls, i, o, t) in stmt
        .query_map(params![project_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
            ))
        })?
        .flatten()
    {
        if let Some(s) = list.iter_mut().find(|s| s.id == root) {
            s.calls = calls;
            s.input_tokens = i;
            s.output_tokens = o;
            s.total_tokens = t;
        }
    }
    Ok(list)
}

/// 直写会话模型选择（cli db session_entry，type=runtime/model_selection）。
///
/// 诊断结论（app.asar host/glm 源码 + 库内实测）：用户在 ZCode 菜单选模型时 agent
/// 写入该条目（id=`{sessionId}:runtime-model-selection`，data 含 modelId/providerId/
/// thoughtLevel），会话恢复时读该条目还原模型；setting.json 的 family 选中键只决定
/// 新会话的默认供应商。因此免 UI 模拟的可靠切换 = 改 setting.json + 改写目标会话的
/// model_selection + 重启（让 agent 内存态失效后按库恢复）。
///
/// 目标会话：顶层 interactive 且未归档（归档判定对齐列表口径）；project_dir 非空时
/// 仅该项目（路径规范化比较）。model 缺省取该 provider 在 config.json 的第一个模型，
/// 已有 thoughtLevel 沿用。返回写入的会话数。
pub fn write_model_selection(
    provider_key: &str,
    model_key: Option<&str>,
    project_dir: Option<&str>,
) -> anyhow::Result<usize> {
    let model = match model_key.filter(|m| !m.is_empty()) {
        Some(m) => m.to_string(),
        None => {
            let config = config_file::read_config()?;
            config
                .get("provider")
                .and_then(|p| p.get(provider_key))
                .and_then(|p| p.get("models"))
                .and_then(|m| m.as_object())
                .and_then(|m| m.keys().next().cloned())
                .ok_or_else(|| anyhow::anyhow!("供应商「{provider_key}」无可用模型"))?
        }
    };

    let conn = open_rw()?;
    let archived = task_archived_set()?;
    let mut stmt = conn.prepare(
        "SELECT id, directory FROM session \
         WHERE parent_id IS NULL AND task_type='interactive' AND time_archived IS NULL",
    )?;
    let rows: Vec<(String, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .flatten()
        .collect();
    drop(stmt);
    let want_dir = project_dir
        .filter(|s| !s.is_empty())
        .map(crate::autoswitch::norm_dir);
    let targets: Vec<String> = rows
        .iter()
        .filter(|(sid, dir)| {
            !archived.contains(sid)
                && want_dir
                    .as_ref()
                    .map_or(true, |w| crate::autoswitch::norm_dir(dir) == *w)
        })
        .map(|(sid, _)| sid.clone())
        .collect();

    let tx = conn.unchecked_transaction()?;
    let now = Local::now().timestamp_millis();
    for sid in &targets {
        let id = format!("{sid}:runtime-model-selection");
        // 沿用已有 thoughtLevel（high/max 等），避免降级用户的思考档位
        let thought: Option<String> = tx
            .query_row(
                "SELECT data FROM session_entry WHERE id=?1",
                params![id],
                |r| r.get::<_, String>(0),
            )
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .and_then(|v| {
                v.get("thoughtLevel")
                    .and_then(|t| t.as_str())
                    .map(String::from)
            });
        let data = match thought.as_deref() {
            Some(t) => format!(
                r#"{{"modelId":{:?},"providerId":{:?},"thoughtLevel":{:?}}}"#,
                model, provider_key, t
            ),
            None => format!(r#"{{"modelId":{:?},"providerId":{:?}}}"#, model, provider_key),
        };
        tx.execute(
            "INSERT INTO session_entry(id,session_id,type,time_created,time_updated,data) \
             VALUES(?1,?2,'runtime/model_selection',?3,?3,?4) \
             ON CONFLICT(id) DO UPDATE SET data=excluded.data, time_updated=excluded.time_updated",
            params![id, sid, now, data],
        )?;
    }
    tx.commit()?;
    Ok(targets.len())
}

/// 修改会话名称：写 session.title 并标记 title_source='custom'（zcode 不再自动覆盖）
pub fn rename_session(session_id: &str, title: &str) -> anyhow::Result<()> {
    let title = title.trim();
    if title.is_empty() {
        anyhow::bail!("名称不能为空");
    }
    let conn = open_rw()?;
    let now = Local::now().timestamp_millis();
    let n = conn.execute(
        "UPDATE session SET title=?2, title_source='custom', time_title_updated=?3, time_updated=?3 \
         WHERE id=?1",
        params![session_id, title, now],
    )?;
    if n == 0 {
        anyhow::bail!("会话不存在（可能已被删除）");
    }
    Ok(())
}

/// 归档单个会话：对称于 restore_session，写 session.time_archived 并置
/// 任务索引 tasks.archived=1（zcode 会话列表随即隐藏，可随时恢复）。
/// 不动 time_updated（归档不算活跃，排序仍按真实最后活跃时间）。
pub fn archive_session(session_id: &str) -> anyhow::Result<()> {
    let conn = open_rw()?;
    let now = Local::now().timestamp_millis();
    let n = conn.execute(
        "UPDATE session SET time_archived=?2 WHERE id=?1 AND parent_id IS NULL",
        params![session_id, now],
    )?;
    if n == 0 {
        anyhow::bail!("会话不存在（可能已被删除）");
    }
    drop(conn);
    let tasks = open_tasks_rw()?;
    tasks.execute(
        "UPDATE tasks SET archived=1 WHERE task_id=?1",
        params![session_id],
    )?;
    Ok(())
}

/// 恢复归档会话：清掉两处归档标记（任务索引 tasks.archived/deleted + 会话库
/// time_archived），会话重新回到 zcode 的会话列表，可继续对话。
/// 不动 time_updated（恢复不算活跃，排序仍按真实最后活跃时间）。
pub fn restore_session(session_id: &str) -> anyhow::Result<()> {
    let conn = open_rw()?;
    let n = conn.execute(
        "UPDATE session SET time_archived=NULL WHERE id=?1",
        params![session_id],
    )?;
    if n == 0 {
        anyhow::bail!("会话不存在（可能已被删除）");
    }
    drop(conn);
    let tasks = open_tasks_rw()?;
    tasks.execute(
        "UPDATE tasks SET archived=0, deleted=0 WHERE task_id=?1",
        params![session_id],
    )?;
    Ok(())
}

/// 归档整个项目：对称于 restore_project / zcode 界面的「归档项目」——
/// 批量归档该项目全部活跃的顶层会话（已归档不动）。返回本次归档的会话数。
pub fn archive_project(project_id: &str) -> anyhow::Result<usize> {
    let conn = open_rw()?;
    let now = Local::now().timestamp_millis();
    let n = conn.execute(
        "UPDATE session SET time_archived=?2 \
         WHERE project_id=?1 AND parent_id IS NULL AND time_archived IS NULL",
        params![project_id, now],
    )? as usize;
    drop(conn);
    let tasks = open_tasks_rw()?;
    // 项目在任务索引里按 workspace_path（= 会话库 directory）关联
    let dir: Option<String> = open_ro()?
        .query_row(
            "SELECT directory FROM session WHERE project_id=?1 LIMIT 1",
            params![project_id],
            |r| r.get(0),
        )
        .ok();
    if let Some(dir) = dir {
        tasks.execute(
            "UPDATE tasks SET archived=1 WHERE workspace_path=?1 AND archived=0",
            params![dir],
        )?;
    }
    Ok(n)
}

/// 恢复整个项目：清掉该项目全部会话的归档标记（含子代理），对称于
/// zcode 界面的「归档项目」（批量归档该项目所有任务）。返回恢复的会话数。
pub fn restore_project(project_id: &str) -> anyhow::Result<usize> {
    let conn = open_rw()?;
    let n = conn.execute(
        "UPDATE session SET time_archived=NULL WHERE project_id=?1",
        params![project_id],
    )? as usize;
    drop(conn);
    let tasks = open_tasks_rw()?;
    // 项目在任务索引里按 workspace_path（= 会话库 directory）关联
    let dir: Option<String> = open_ro()?
        .query_row(
            "SELECT directory FROM session WHERE project_id=?1 LIMIT 1",
            params![project_id],
            |r| r.get(0),
        )
        .ok();
    if let Some(dir) = dir {
        tasks.execute(
            "UPDATE tasks SET archived=0, deleted=0 WHERE workspace_path=?1",
            params![dir],
        )?;
    }
    Ok(n)
}

/// 批量删除：会话（自动连带子代理后代）与项目（含其全部会话）。
/// 级联清掉 message / model_usage 等关联表，并同步清理 rollout 文件与本地用量记录。
pub fn delete(
    db: &crate::db::Database,
    session_ids: &[String],
    project_ids: &[String],
) -> anyhow::Result<ZcDeleteResult> {
    let mut conn = open_rw()?;
    // SQLite 默认关闭外键约束，必须显式开启才有 ON DELETE CASCADE
    conn.execute_batch("PRAGMA foreign_keys=ON")?;

    // 展开删除目标：所选会话 + 其全部后代；所选项目 + 其全部会话（及后代）
    let mut targets: HashSet<String> = HashSet::new();
    if !session_ids.is_empty() {
        for sid in expand_descendants(&conn, session_ids)? {
            targets.insert(sid);
        }
    }
    for pid in project_ids {
        let mut stmt = conn.prepare("SELECT id FROM session WHERE project_id = ?1")?;
        let ids: Vec<String> = stmt
            .query_map(params![pid], |r| r.get(0))?
            .flatten()
            .collect();
        drop(stmt);
        for sid in expand_descendants(&conn, &ids)? {
            targets.insert(sid);
        }
    }
    if targets.is_empty() {
        return Ok(ZcDeleteResult::default());
    }
    let ids: Vec<String> = targets.into_iter().collect();

    let tx = conn.transaction()?;
    for chunk in ids.chunks(CHUNK) {
        let placeholders = vec!["?"; chunk.len()].join(",");
        let sql = format!("DELETE FROM session WHERE id IN ({placeholders})");
        tx.execute(&sql, rusqlite::params_from_iter(chunk))?;
    }
    tx.commit()?;
    let deleted_sessions = ids.len();

    // 同步清理任务索引（tasks 表按 task_id 关联会话），避免 zcode 界面残留幽灵条目；
    // 失败仅记日志——索引残留不影响会话库数据完整性
    if let Ok(tasks) = open_tasks_rw() {
        for chunk in ids.chunks(CHUNK) {
            let placeholders = vec!["?"; chunk.len()].join(",");
            let sql = format!("DELETE FROM tasks WHERE task_id IN ({placeholders})");
            if let Err(e) = tasks.execute(&sql, rusqlite::params_from_iter(chunk)) {
                log::warn!("清理任务索引失败: {e}");
            }
        }
    }

    // 顺带清理 rollout 用量文件与本地 usage_records（用量页同步收敛）
    let freed = cleanup_rollout_files(&ids);
    let _ = db.delete_usage_by_sessions(&ids);

    Ok(ZcDeleteResult {
        deleted_sessions,
        deleted_projects: project_ids.len(),
        freed_rollout_files: freed,
    })
}

/// 展开会话 id 列表 → 自身 + 全部后代（子代理会话可能与父会话不同项目，按 id 追而不是按项目）
fn expand_descendants(conn: &Connection, roots: &[String]) -> anyhow::Result<Vec<String>> {
    if roots.is_empty() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for chunk in roots.chunks(CHUNK) {
        let placeholders = vec!["?"; chunk.len()].join(",");
        let sql = format!(
            "WITH RECURSIVE tree(sid) AS (SELECT id FROM session WHERE id IN ({placeholders}) \
             UNION ALL SELECT s.id FROM session s JOIN tree t ON s.parent_id = t.sid) \
             SELECT sid FROM tree"
        );
        let mut stmt = conn.prepare(&sql)?;
        for sid in stmt.query_map(rusqlite::params_from_iter(chunk), |r| r.get::<_, String>(0))? {
            out.push(sid?);
        }
    }
    Ok(out)
}

/// 删除各会话对应的 rollout 用量文件（model-io-<session_id>.jsonl，不存在则跳过）
fn cleanup_rollout_files(session_ids: &[String]) -> usize {
    let Some(dir) = paths::rollout_dir() else {
        return 0;
    };
    let mut n = 0;
    for sid in session_ids {
        let p = dir.join(format!("model-io-{sid}.jsonl"));
        if p.is_file() && std::fs::remove_file(&p).is_ok() {
            n += 1;
        }
    }
    n
}

fn db_path() -> anyhow::Result<std::path::PathBuf> {
    paths::zcode_cli_db_path().ok_or_else(|| anyhow::anyhow!("未找到 zcode 数据库（~/.zcode/cli/db/db.sqlite）"))
}

fn open_ro() -> anyhow::Result<Connection> {
    let p = db_path()?;
    crate::usage::open_readonly(Path::new(&p))
}

/// 读写连接（WAL 允许与 zcode 并发，busy_timeout 等待短暂锁）
fn open_rw() -> anyhow::Result<Connection> {
    let conn = Connection::open(db_path()?)?;
    conn.busy_timeout(Duration::from_millis(5000))?;
    Ok(conn)
}
