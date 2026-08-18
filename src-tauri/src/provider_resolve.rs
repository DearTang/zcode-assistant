//! 供应商 UUID→名称解析：扫描 zcode transcript 文件，从系统消息里嵌入的
//! 工作区 provider 注册表（"id":{"name":"X"}）提取映射，写入本地 provider_aliases。
//!
//! 这是 UUID→可读名的唯一来源（config/message 表都没有自定义供应商名）。
//! 结果独立于 zcode 配置：删除渠道不会清掉已保存的别名。
//! 每次启动增量扫描（按文件 mtime/size 跟踪），只解析新增/变更的 transcript。
use regex::Regex;
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::Path;

use crate::db::Database;

const SCAN_STATE_KEY: &str = "provider_transcript_scan_state";

#[derive(serde::Serialize, serde::Deserialize, Clone, Default)]
struct ScanState {
    #[serde(default)]
    files: HashMap<String, FileSig>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
struct FileSig {
    size: u64,
    mtime: f64,
}

/// 扫描 transcript 目录，增量提取 UUID/builtin id → name，写入 provider_aliases。
/// 返回本次实际解析的文件数。
pub fn scan_transcripts(db: &Database) -> anyhow::Result<usize> {
    let re_strip = Regex::new(r"(?m)^[ \t]*\d+\t")?;
    // 匹配 "某个key": { "name": "名字"  —— provider 注册表的固定结构
    let re_name = Regex::new(r#""([^"]{1,80})"\s*:\s*\{\s*"name"\s*:\s*"([^"]+)""#)?;

    let state: ScanState = db
        .kv_get(SCAN_STATE_KEY)
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();

    let mut next_files = state.files.clone();
    let mut found: HashMap<String, String> = HashMap::new();
    let mut scanned = 0usize;

    if let Some(base) = dirs::home_dir().map(|h| h.join(".zcode").join("cli").join("agents")) {
        walk(
            &base,
            &state,
            &mut next_files,
            &mut found,
            &mut scanned,
            &re_strip,
            &re_name,
        );
    }

    for (id, name) in &found {
        let _ = db.upsert_provider_alias(id, name, "transcript");
    }
    if let Ok(s) = serde_json::to_string(&ScanState {
        files: next_files,
    }) {
        let _ = db.kv_set(SCAN_STATE_KEY, &s);
    }
    Ok(scanned)
}

fn walk(
    base: &Path,
    state: &ScanState,
    next_files: &mut HashMap<String, FileSig>,
    found: &mut HashMap<String, String>,
    scanned: &mut usize,
    re_strip: &Regex,
    re_name: &Regex,
) {
    let sess_dirs = match std::fs::read_dir(base) {
        Ok(d) => d,
        Err(_) => return,
    };
    for sess in sess_dirs.flatten() {
        let agent_dirs = match std::fs::read_dir(sess.path()) {
            Ok(d) => d,
            Err(_) => continue,
        };
        for agent in agent_dirs.flatten() {
            let tf = agent.path().join("transcript.jsonl");
            if !tf.is_file() {
                continue;
            }
            let (size, mtime) = match file_meta(&tf) {
                Some(m) => m,
                None => continue,
            };
            let key = tf.to_string_lossy().to_string();
            let changed = match state.files.get(&key) {
                Some(p) => p.size != size || p.mtime != mtime,
                None => true,
            };
            next_files.insert(key, FileSig { size, mtime });
            if !changed {
                continue;
            }
            *scanned += 1;
            extract_from_file(&tf, found, re_strip, re_name);
        }
    }
}

fn extract_from_file(
    path: &Path,
    found: &mut HashMap<String, String>,
    re_strip: &Regex,
    re_name: &Regex,
) {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return,
    };
    for line in BufReader::new(file).lines().flatten() {
        // 廉价预筛：原始行（引号被转义）里仍含 provider/name 子串
        if !line.contains("provider") || !line.contains("name") {
            continue;
        }
        let obj: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let mut strings: Vec<String> = Vec::new();
        collect_provider_strings(&obj, &mut strings);
        for s in strings {
            // transcript 里 config 预览带行号前缀（\n4\t...），先去掉
            let cleaned = re_strip.replace_all(&s, "");
            for cap in re_name.captures_iter(&cleaned) {
                let id = cap.get(1).unwrap().as_str();
                let nm = cap.get(2).unwrap().as_str();
                // 只收 uuid / builtin: / 短键，避免误收模型名等
                if id.contains('-') || id.starts_with("builtin:") || id.len() <= 40 {
                    found.insert(id.to_string(), nm.to_string());
                }
            }
        }
    }
}

/// 递归收集所有含 "provider" 与 "name" 的字符串值（系统消息里嵌入的注册表）
fn collect_provider_strings(o: &serde_json::Value, out: &mut Vec<String>) {
    match o {
        serde_json::Value::String(s) => {
            if s.contains("provider") && s.contains("\"name\"") {
                out.push(s.clone());
            }
        }
        serde_json::Value::Object(m) => {
            for v in m.values() {
                collect_provider_strings(v, out);
            }
        }
        serde_json::Value::Array(a) => {
            for v in a {
                collect_provider_strings(v, out);
            }
        }
        _ => {}
    }
}

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
