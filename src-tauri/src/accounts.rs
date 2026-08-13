//! 智谱账号切换：捕获快照 / 切换 / 回滚（参考 zcode-account-switcher）
//! 快照存 data_dir/accounts/<id>.snap.json = { credentials, config }（原文件文本）
//! 切换 = kill zcode → 备份 .last → 原子写回两文件 → 重启
use crate::db::Database;
use crate::types::AccountMeta;
use crate::zcode::{crypto, paths, process};
use anyhow::{anyhow, Context, Result};
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub fn accounts_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("accounts")
}

fn snap_path(data_dir: &Path, id: &str) -> PathBuf {
    accounts_dir(data_dir).join(format!("{id}.snap.json"))
}

fn atomic_write(path: &Path, content: &str) -> Result<()> {
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, content).with_context(|| format!("写临时文件失败: {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("替换文件失败: {}", path.display()))?;
    Ok(())
}

/// 从当前 credentials.json 提取账号身份指纹
///
/// 真正的 zcode 登录态在 credentials.json：
///   - `zcodejwttoken`：zcode 登录 JWT（enc:v1 加密），其 payload 含 user_id/sub
///   - `oauth:bigmodel:user_info`：含 username/displayName（enc:v1 加密）
///
/// 返回 (user_id, 合成信息 Value)，信息 Value 携带
/// user_id/username/displayName/email/name 供 capture 展示。
pub fn current_fingerprint() -> Option<(String, serde_json::Value)> {
    let cred_path = paths::credentials_path()?;
    let cred_txt = std::fs::read_to_string(&cred_path).ok()?;
    let cred: serde_json::Value = serde_json::from_str(&cred_txt).ok()?;

    // 1) zcodejwttoken：解密 → 解码 JWT → 取 user_id（指纹主键）
    let jwt = crypto::read_zcode_jwt_token(&cred).ok()?;
    let jwt_payload = crypto::decode_jwt_payload(&jwt)?;
    let uid = jwt_payload
        .get("user_id")
        .or_else(|| jwt_payload.get("sub"))
        .and_then(|v| v.as_str())
        .map(String::from)?;

    // 2) user_info：解密 → 取展示字段（username/displayName/email/name）
    let info = cred
        .get("oauth:bigmodel:user_info")
        .and_then(|v| v.as_str())
        .and_then(|s| {
            if crypto::is_encrypted(s) {
                crypto::decrypt(s).ok()
            } else {
                Some(s.to_string())
            }
        })
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .unwrap_or(serde_json::Value::Null);

    let mut info_map = serde_json::Map::new();
    info_map.insert("user_id".into(), serde_json::Value::String(uid.clone()));
    for key in ["username", "displayName", "email", "name"] {
        if let Some(v) = info.get(key).cloned() {
            info_map.insert(key.into(), v);
        }
    }

    Some((uid, serde_json::Value::Object(info_map)))
}

/// 捕获当前 zcode 登录态为快照
pub fn capture(db: &Database, data_dir: &Path, label: &str) -> Result<AccountMeta> {
    let cred_path = paths::credentials_path().ok_or_else(|| anyhow!("无 credentials 路径"))?;
    let cfg_path = paths::config_path().ok_or_else(|| anyhow!("无 config 路径"))?;
    let cred = std::fs::read_to_string(&cred_path)
        .with_context(|| format!("读取失败: {}", cred_path.display()))?;
    let cfg = std::fs::read_to_string(&cfg_path)
        .with_context(|| format!("读取失败: {}", cfg_path.display()))?;

    let (uid, payload) = current_fingerprint().unwrap_or(("unknown".into(), serde_json::Value::Null));
    let short_id = uid.get(..8).unwrap_or(&uid).to_string();
    let id = Uuid::new_v4().to_string();

    let meta = AccountMeta {
        id: id.clone(),
        short_id,
        user_id: Some(uid),
        provider: None,
        label: if label.is_empty() {
            payload
                .get("displayName")
                .or_else(|| payload.get("username"))
                .or_else(|| payload.get("email"))
                .and_then(|v| v.as_str())
                .unwrap_or("账号")
                .to_string()
        } else {
            label.to_string()
        },
        email: payload.get("email").and_then(|v| v.as_str()).map(String::from),
        name: payload
            .get("displayName")
            .or_else(|| payload.get("username"))
            .and_then(|v| v.as_str())
            .map(String::from),
        avatar: None,
        customer_id: payload.get("customer_id").and_then(|v| v.as_str()).map(String::from),
        note: None,
        captured_at: chrono::Utc::now().to_rfc3339(),
    };

    std::fs::create_dir_all(accounts_dir(data_dir))?;
    let snap = serde_json::json!({ "credentials": cred, "config": cfg });
    std::fs::write(snap_path(data_dir, &id), serde_json::to_string_pretty(&snap)?)?;
    db.upsert_account(&meta)?;
    Ok(meta)
}

/// 切换到指定账号（kill → 备份 → 原子写回 → 重启）
/// credentials 整体覆盖；config 仅用快照里的内置(builtin:)provider 覆盖，
/// 保留用户自定义 provider，避免切换账号冲掉手动添加的供应商。
pub fn switch(db: &Database, data_dir: &Path, id: &str) -> Result<AccountMeta> {
    let meta = db
        .get_account(id)?
        .ok_or_else(|| anyhow!("账号不存在: {id}"))?;
    let sp = snap_path(data_dir, id);
    let snap_txt = std::fs::read_to_string(&sp)
        .with_context(|| format!("读取快照失败: {}", sp.display()))?;
    let snap: serde_json::Value = serde_json::from_str(&snap_txt)?;
    let new_cred = snap
        .get("credentials")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("快照无 credentials"))?;
    let new_cfg = snap
        .get("config")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("快照无 config"))?;

    let cred_path = paths::credentials_path().ok_or_else(|| anyhow!("无 credentials 路径"))?;
    let cfg_path = paths::config_path().ok_or_else(|| anyhow!("无 config 路径"))?;
    let v2 = paths::zcode_v2_dir().ok_or_else(|| anyhow!("无 v2 路径"))?;

    // 关闭运行中的 zcode（运行时改两文件不可靠）
    let _ = process::kill_zcode();

    // 备份当前到 .last（用于回滚）
    let last = v2.join(".last");
    let _ = std::fs::create_dir_all(&last);
    if let Ok(b) = std::fs::read(&cred_path) {
        let _ = std::fs::write(last.join("credentials.json"), b);
    }
    if let Ok(b) = std::fs::read(&cfg_path) {
        let _ = std::fs::write(last.join("config.json"), b);
    }

    // 原子写回（失败回滚）
    // credentials 整体覆盖：登录态必须随账号切换
    if atomic_write(&cred_path, new_cred).is_err() {
        rollback(&last, &cred_path, &cfg_path)?;
        return Err(anyhow!("写 credentials 失败，已回滚"));
    }
    // config 合并：只换快照里的内置(builtin:)订阅 provider，保留自定义 provider
    let merged_cfg = match std::fs::read_to_string(&cfg_path) {
        Ok(curr) => merge_config(&curr, new_cfg),
        Err(_) => new_cfg.to_string(),
    };
    if atomic_write(&cfg_path, &merged_cfg).is_err() {
        rollback(&last, &cred_path, &cfg_path)?;
        return Err(anyhow!("写 config 失败，已回滚"));
    }

    // 重启
    let _ = process::launch_zcode();
    Ok(meta)
}

fn rollback(last: &Path, cred_path: &Path, cfg_path: &Path) -> Result<()> {
    if let Ok(b) = std::fs::read(last.join("credentials.json")) {
        let _ = atomic_write(cred_path, &String::from_utf8_lossy(&b));
    }
    if let Ok(b) = std::fs::read(last.join("config.json")) {
        let _ = atomic_write(cfg_path, &String::from_utf8_lossy(&b));
    }
    Ok(())
}

/// 合并 config：保留当前的自定义 provider，仅用快照里的内置(builtin:)provider 覆盖。
/// 这样切换账号只换内置订阅 key，不冲掉用户手动添加/导入的自定义 provider。
/// 任一端 JSON 非法时退化为整体使用另一端，保证不丢数据。
fn merge_config(curr: &str, snap: &str) -> String {
    let mut curr_v: serde_json::Value = match serde_json::from_str(curr) {
        Ok(v) => v,
        Err(_) => return snap.to_string(),
    };
    let snap_v: serde_json::Value = match serde_json::from_str(snap) {
        Ok(v) => v,
        Err(_) => return curr.to_string(),
    };
    let snap_providers = match snap_v.get("provider").and_then(|p| p.as_object()) {
        Some(p) => p,
        None => return curr.to_string(),
    };
    let curr_providers = match curr_v.get_mut("provider").and_then(|p| p.as_object_mut()) {
        Some(p) => p,
        None => return curr.to_string(),
    };
    for (k, v) in snap_providers {
        if k.starts_with("builtin:") {
            curr_providers.insert(k.clone(), v.clone());
        }
        // 非 builtin（用户自定义 provider）保留当前不动
    }
    serde_json::to_string_pretty(&curr_v).unwrap_or_else(|_| curr.to_string())
}

/// 删除账号（db + 快照文件）
pub fn remove(db: &Database, data_dir: &Path, id: &str) -> Result<()> {
    db.delete_account(id)?;
    let _ = std::fs::remove_file(snap_path(data_dir, id));
    Ok(())
}

/// 当前账号（按指纹匹配）
pub fn current(db: &Database) -> Option<AccountMeta> {
    let (uid, _) = current_fingerprint()?;
    let list = db.list_accounts().ok()?;
    list.into_iter().find(|a| a.user_id.as_deref() == Some(&uid))
}
