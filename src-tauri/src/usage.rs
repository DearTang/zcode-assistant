//! 用量记录解析：扫描 ~/.zcode/cli/rollout/model-io-sess_*.jsonl（每次模型调用一行），
//! 提取 token 用量 / 耗时 / 速度，去重写入 usage_records。
//!
//! 数据来源是只读的：本模块只读 zcode 自身的 rollout 文件，不改 zcode 任何配置。
//!
//! 同步策略（最小资源消耗）：
//! - 默认只处理 mtime 在最近 30 天内的文件（`full=true` 回填全部历史）；
//! - 逐文件记录已读字节偏移 offset，仅读取 offset→EOF 的新字节，绝不重复解析已入库行；
//! - 只解析以 `\n` 结尾的完整行，末尾半行延后；文件不再增长时才视末行为终结。
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use crate::db::Database;
use crate::types::{UsageRecord, UsageSyncResult};
use crate::zcode::paths;

const SYNC_KEY: &str = "usage_sync_state";
/// 时区修复一次性迁移标记：旧记录的 date 用 UTC 派生，需清空重解析为本地日期
const TZ_MIGRATED_KEY: &str = "usage_tz_migrated_v1";
/// 默认同步窗口（天）
const DEFAULT_WINDOW_DAYS: f64 = 30.0;

// ===== JSONL 行结构（只取需要的字段，serde 自动忽略巨型 request.body）=====

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelIoLine {
    request_id: Option<String>,
    started_at: Option<String>,
    completed_at: Option<String>,
    duration_ms: Option<f64>,
    #[serde(default)]
    model: Option<ModelMeta>,
    #[serde(default)]
    response: Option<ResponseBlock>,
    query_source: Option<String>,
    session_id: Option<String>,
    #[serde(default, rename = "turnId")]
    #[allow(dead_code)]
    turn_id: Option<String>,
    #[serde(default, rename = "type")]
    #[allow(dead_code)]
    ty: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ModelMeta {
    model_id: Option<String>,
    provider_id: Option<String>,
    role: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ResponseBlock {
    #[serde(default)]
    usage: Option<Usage>,
    finish_reason: Option<String>,
}

/// token 用量；同时兼容归一化 camelCase 与 anthropic 原始 snake_case 两种形态
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Usage {
    #[serde(default, alias = "input_tokens")]
    input_tokens: Option<i64>,
    #[serde(default, alias = "output_tokens")]
    output_tokens: Option<i64>,
    #[serde(default, alias = "total_tokens")]
    total_tokens: Option<i64>,
    #[serde(default, alias = "cache_read_tokens", alias = "cache_read_input_tokens")]
    cache_read_tokens: Option<i64>,
    #[serde(default, alias = "cache_write_tokens", alias = "cache_creation_input_tokens")]
    cache_write_tokens: Option<i64>,
}

// ===== 同步状态（存于 kv 表）=====

#[derive(Serialize, Deserialize, Clone, Default)]
struct SyncState {
    #[serde(default)]
    files: HashMap<String, FileSig>,
}

#[derive(Serialize, Deserialize, Clone)]
struct FileSig {
    size: u64,
    mtime: f64,
    /// 已解析到的字节偏移（下次从此处续读）
    #[serde(default)]
    offset: u64,
}

/// 扫描 rollout 目录：默认 30 天窗口 + 逐文件字节偏移增量解析。
/// - `full=false`：仅处理 mtime 在最近 30 天内的文件（资源最省，常态路径）；
/// - `full=true`：忽略窗口，回填所有未同步过的历史文件（一次性较重，之后仍增量）。
pub fn sync_rollout(db: &Database, full: bool) -> anyhow::Result<UsageSyncResult> {
    // 一次性时区迁移：旧版 date 按 UTC 派生，清空记录并重置同步状态，
    // 使本次重解析为本地日期（首次升级后执行一次）
    if db.kv_get(TZ_MIGRATED_KEY).is_none() {
        let _ = db.clear_usage();
        let _ = db.kv_set(SYNC_KEY, "");
        let _ = db.kv_set(TZ_MIGRATED_KEY, "1");
    }

    let total_before = db.count_usage().unwrap_or(0);

    let dir = match paths::rollout_dir() {
        Some(d) => d,
        None => {
            return Ok(UsageSyncResult {
                new_count: 0,
                total_count: total_before,
                scanned_files: 0,
                min_date: None,
                max_date: None,
            })
        }
    };

    let now = now_secs();
    // 30 天窗口（full 模式取 0，即不限制）
    let cutoff = if full {
        0.0
    } else {
        now - DEFAULT_WINDOW_DAYS * 86400.0
    };

    let state: SyncState = db
        .kv_get(SYNC_KEY)
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    let mut next_files: HashMap<String, FileSig> = HashMap::new();

    let mut recs: Vec<UsageRecord> = Vec::new();
    let mut scanned = 0usize;

    if let Ok(entries) = std::fs::read_dir(&dir) {
        for ent in entries.flatten() {
            let path = ent.path();
            if !is_model_io_file(&path) {
                continue;
            }
            let (size, mtime) = match file_meta(&path) {
                Some(m) => m,
                None => continue,
            };
            let key = path.to_string_lossy().to_string();
            let prev = state.files.get(&key);

            let in_window = full || mtime >= cutoff;

            // 起始偏移：旧状态有则取（并钳制到当前 size，防文件被截断），否则 0
            let start_offset = prev.map(|p| p.offset.min(p.size).min(size)).unwrap_or(0);

            // 文件未变化且已读到末尾 → 直接跳过（最常见路径，零解析）
            let fully_done = prev.map_or(false, |p| {
                p.size == size && p.mtime == mtime && p.offset >= size
            });

            // 仅窗口内且有增量才解析；窗口外或已完成都冻结 offset，不丢进度
            let new_offset = if !in_window || fully_done {
                start_offset
            } else {
                scanned += 1;
                let file_stable = prev.map_or(false, |p| p.size == size);
                parse_delta(&path, start_offset, file_stable, &mut recs)
            };

            next_files.insert(
                key,
                FileSig {
                    size,
                    mtime,
                    offset: new_offset,
                },
            );
        }
    }

    let new_count = db.insert_usage_ignore(&recs).unwrap_or(0);

    // 持久化同步状态（仅保留本次可见的文件，已删除/压缩的自动剔除）
    if let Ok(s) = serde_json::to_string(&SyncState {
        files: next_files,
    }) {
        let _ = db.kv_set(SYNC_KEY, &s);
    }

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

fn is_model_io_file(path: &Path) -> bool {
    let ext_ok = path.extension().and_then(|s| s.to_str()) == Some("jsonl");
    let name_ok = path
        .file_name()
        .and_then(|s| s.to_str())
        .map(|n| n.starts_with("model-io-sess"))
        .unwrap_or(false);
    ext_ok && name_ok
}

/// 取文件 (size, mtime_secs)
fn file_meta(path: &Path) -> Option<(u64, f64)> {
    let md = std::fs::metadata(path).ok()?;
    let size = md.len();
    let mtime = md
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    Some((size, mtime))
}

fn now_secs() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// 从 start_offset 处读取增量字节，解析**完整**行（以 `\n` 结尾）。
/// 末尾若无 `\n`（行未写完），延后到下次同步；但若 file_stable（文件不再增长），
/// 视末行为已终结，一并解析。返回新的偏移（已消费字节结尾位置）。
fn parse_delta(path: &Path, start_offset: u64, file_stable: bool, recs: &mut Vec<UsageRecord>) -> u64 {
    let mut file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return start_offset,
    };
    let len = match file.metadata() {
        Ok(m) => m.len(),
        Err(_) => return start_offset,
    };
    if len <= start_offset {
        return start_offset; // 无新数据（或被截断）
    }
    if file.seek(SeekFrom::Start(start_offset)).is_err() {
        return start_offset;
    }
    let mut buf = Vec::new();
    if file.read_to_end(&mut buf).is_err() {
        return start_offset;
    }

    let raw = path.to_string_lossy().to_string();
    let chunks: Vec<&[u8]> = buf.split_inclusive(|&b| b == b'\n').collect();
    let n = chunks.len();
    let mut consumed = 0usize;
    for (i, chunk) in chunks.iter().enumerate() {
        let is_last = i + 1 == n;
        let complete = chunk.ends_with(b"\n");
        // 末尾不完整行：仅当文件稳定（不再增长）才当终结行解析，否则延后
        if !complete && !(is_last && file_stable) {
            break;
        }
        // 去掉行尾换行（\n / \r\n）
        let mut end = chunk.len();
        if end > 0 && chunk[end - 1] == b'\n' {
            end -= 1;
        }
        if end > 0 && chunk[end - 1] == b'\r' {
            end -= 1;
        }
        if let Ok(s) = std::str::from_utf8(&chunk[..end]) {
            if !s.trim().is_empty() {
                if let Ok(parsed) = serde_json::from_str::<ModelIoLine>(s) {
                    if let Some(rec) = build_record(parsed, &raw) {
                        recs.push(rec);
                    }
                }
            }
        }
        consumed += chunk.len();
    }
    start_offset + consumed as u64
}

fn build_record(l: ModelIoLine, raw: &str) -> Option<UsageRecord> {
    let request_id = l.request_id?;
    // 必须有 usage（带 token 计数的模型调用）才记录
    let usage = l.response.as_ref().and_then(|r| r.usage.as_ref())?;

    let input = usage.input_tokens.unwrap_or(0);
    let output = usage.output_tokens.unwrap_or(0);
    let total = usage.total_tokens.unwrap_or(input + output);
    let cache_read = usage.cache_read_tokens.unwrap_or(0);
    let cache_write = usage.cache_write_tokens.unwrap_or(0);

    let model = l.model.as_ref();
    let model_id = model
        .and_then(|m| m.model_id.clone())
        .unwrap_or_else(|| "(未知)".into());
    let provider_id = model
        .and_then(|m| m.provider_id.clone())
        .unwrap_or_else(|| "(未知)".into());
    let role = model.and_then(|m| m.role.clone());

    let ts = l.started_at.clone().or(l.completed_at.clone());
    let date = ts
        .as_ref()
        .map(|s| local_date(s))
        .unwrap_or_else(|| "未知".into());

    let duration_ms = l.duration_ms;
    let tps = match duration_ms {
        Some(d) if d > 0.0 => Some(output as f64 / (d / 1000.0)),
        _ => None,
    };
    let finish_reason = l.response.as_ref().and_then(|r| r.finish_reason.clone());

    Some(UsageRecord {
        request_id,
        started_at: l.started_at,
        date,
        provider_id,
        model_id,
        role,
        query_source: l.query_source,
        input_tokens: input,
        output_tokens: output,
        total_tokens: total,
        cache_read_tokens: cache_read,
        cache_write_tokens: cache_write,
        duration_ms,
        tps,
        finish_reason,
        session_id: l.session_id,
        raw_path: Some(raw.to_string()),
    })
}

/// 把 ISO8601（UTC，带 Z）时间戳换算到本地时区，返回 YYYY-MM-DD；
/// 解析失败则退回取前 10 字符。保证与前端按本地日期筛选一致。
fn local_date(ts: &str) -> String {
    match chrono::DateTime::parse_from_rfc3339(ts) {
        Ok(dt) => dt
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d")
            .to_string(),
        Err(_) => ts.get(..10).unwrap_or(ts).to_string(),
    }
}
