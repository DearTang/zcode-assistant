//! 用量记录：从 zcode 的 cli/db/db.sqlite（model_usage 表）增量导入到本地 usage_records。
//!
//! 为什么不用 rollout 文件：rollout 目录（model-io-sess_*.jsonl）会被 zcode 定期清理，
//! 通常只保留近期会话；而 model_usage 表保留了**完整历史**（含 input/output/cache token、
//! duration、time_to_first_token 等），是用量统计的权威来源。
//!
//! 本模块只读打开 zcode 的 SQLite（WAL 模式允许并发读），按 rowid 增量拉取新行，
//! 写入本地 usage_records 供聚合查询。全程不改 zcode 任何数据。
use chrono::DateTime;
use rusqlite::{params, Connection, OpenFlags};
use std::path::Path;

use crate::db::Database;
use crate::types::{UsageRecord, UsageSyncResult};
use crate::zcode::paths;

/// 增量游标：已导入的最大 rowid（存于 kv）
const CURSOR_KEY: &str = "usage_import_cursor";
/// 数据源切换一次性迁移：清空旧 rollout 数据，从 model_usage 全量重导
const SOURCE_MIGRATED_KEY: &str = "usage_source_model_usage_v1";

/// 每批拉取行数
const BATCH: i64 = 2000;

/// 同步：从 zcode model_usage 增量导入。`full=true` 时清空并从 rowid=0 全量重导。
pub fn sync_usage(db: &Database, full: bool) -> anyhow::Result<UsageSyncResult> {
    let total_before = db.count_usage().unwrap_or(0);

    // 数据源切换迁移（或用户主动 full）：清空 + 重置游标
    if db.kv_get(SOURCE_MIGRATED_KEY).is_none() || full {
        let _ = db.clear_usage();
        let _ = db.kv_set(CURSOR_KEY, "0");
        let _ = db.kv_set(SOURCE_MIGRATED_KEY, "1");
    }

    let zc_path = match paths::zcode_cli_db_path() {
        Some(p) => p,
        None => {
            return Ok(UsageSyncResult {
                new_count: 0,
                total_count: total_before,
                scanned_files: 0,
                min_date: None,
                max_date: None,
            });
        }
    };

    let conn = match open_readonly(&zc_path) {
        Ok(c) => c,
        Err(e) => {
            // zcode 库打不开（未运行/被独占）→ 不影响已有数据
            log::warn!("打开 zcode 用量库失败: {e}");
            return Ok(UsageSyncResult {
                new_count: 0,
                total_count: total_before,
                scanned_files: 0,
                min_date: None,
                max_date: None,
            });
        }
    };

    let mut cursor: i64 = db
        .kv_get(CURSOR_KEY)
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0);

    let sql = "SELECT rowid, id, started_at, provider_id, model_id, query_source, \
        input_tokens, output_tokens, cache_read_input_tokens, cache_creation_input_tokens, \
        computed_total_tokens, provider_total_tokens, duration_ms, time_to_first_token_ms, \
        finish_reason, session_id \
        FROM model_usage WHERE rowid > ? ORDER BY rowid LIMIT 2000";

    let mut stmt = conn.prepare(sql)?;
    let mut recs: Vec<UsageRecord> = Vec::new();
    loop {
        let mut batch: Vec<(i64, UsageRecord)> = Vec::new();
        let rows = stmt.query_map(params![cursor], map_row)?;
        for x in rows {
            batch.push(x?);
        }
        if batch.is_empty() {
            break;
        }
        let batch_len = batch.len();
        let new_cursor = batch.last().unwrap().0;
        for (_, rec) in batch {
            recs.push(rec);
        }
        if new_cursor > cursor {
            cursor = new_cursor;
        } else {
            break;
        }
        // 不足一批 → 已到末尾
        if (batch_len as i64) < BATCH {
            break;
        }
    }
    drop(stmt);
    drop(conn);

    let scanned = recs.len();
    let new_count = db.insert_usage_ignore(&recs).unwrap_or(0);
    // 游标推进到本轮最大 rowid（即使本轮插入被去重忽略，也推进，避免重复扫描）
    let _ = db.kv_set(CURSOR_KEY, &cursor.to_string());

    let total_count = db.count_usage().unwrap_or(total_before + new_count as i64);
    let (min_date, max_date) = match db.usage_filters() {
        Ok(f) => (f.min_date, f.max_date),
        Err(_) => (None, None),
    };

    Ok(UsageSyncResult {
        new_count,
        total_count,
        scanned_files: scanned,
        min_date,
        max_date,
    })
}

/// 只读打开 zcode 的 SQLite（WAL 允许并发读，不与 zcode 抢写锁）
fn open_readonly(path: &Path) -> anyhow::Result<Connection> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    Ok(conn)
}

/// model_usage 一行 → UsageRecord（同时返回 rowid 用于增量游标）
fn map_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<(i64, UsageRecord)> {
    let rowid: i64 = r.get(0)?;
    let id: String = r.get::<_, Option<String>>(1)?.unwrap_or_default();
    let started_ms: Option<i64> = r.get::<_, Option<i64>>(2)?;
    let provider_id: String = r
        .get::<_, Option<String>>(3)?
        .unwrap_or_else(|| "(未知)".into());
    let model_id: String = r
        .get::<_, Option<String>>(4)?
        .unwrap_or_else(|| "(未知)".into());
    let query_source: Option<String> = r.get(5)?;
    let input_tokens: i64 = r.get::<_, Option<i64>>(6)?.unwrap_or(0);
    let output_tokens: i64 = r.get::<_, Option<i64>>(7)?.unwrap_or(0);
    let cache_read: i64 = r.get::<_, Option<i64>>(8)?.unwrap_or(0);
    let cache_write: i64 = r.get::<_, Option<i64>>(9)?.unwrap_or(0);
    let computed_total: Option<i64> = r.get(10)?;
    let provider_total: Option<i64> = r.get(11)?;
    let duration_ms: Option<i64> = r.get(12)?;
    let ttfb_ms: Option<i64> = r.get(13)?;
    let finish_reason: Option<String> = r.get(14)?;
    let session_id: Option<String> = r.get(15)?;

    let total = computed_total
        .or(provider_total)
        .unwrap_or(input_tokens + output_tokens);

    // ISO（UTC，带 Z）便于前端 new Date() 转本地显示；date 为本地日期
    let started_at = started_ms.and_then(|ms| {
        DateTime::from_timestamp_millis(ms).map(|dt| dt.to_rfc3339())
    });
    let date = started_ms
        .and_then(|ms| {
            DateTime::from_timestamp_millis(ms)
                .map(|dt| dt.with_timezone(&chrono::Local).format("%Y-%m-%d").to_string())
        })
        .unwrap_or_else(|| "未知".into());

    // 输出速度：优先用「生成耗时 = duration - TTFB」（真实吐字速度），
    // TTFB 缺失或异常时退化为总耗时
    let gen_ms = match (duration_ms, ttfb_ms) {
        (Some(d), Some(t)) if t >= 0 && t < d => Some(d - t),
        _ => duration_ms,
    };
    let tps = gen_ms
        .filter(|&g| g > 0)
        .map(|g| output_tokens as f64 / (g as f64 / 1000.0));

    Ok((
        rowid,
        UsageRecord {
            request_id: id,
            started_at,
            date,
            provider_id,
            model_id,
            role: None, // model_usage 无 role（main/lite/subagent 在 rollout，历史表不区分）
            query_source,
            input_tokens,
            output_tokens,
            total_tokens: total,
            cache_read_tokens: cache_read,
            cache_write_tokens: cache_write,
            duration_ms: duration_ms.map(|d| d as f64),
            tps,
            finish_reason,
            session_id,
            raw_path: Some("model_usage".into()),
        },
    ))
}
