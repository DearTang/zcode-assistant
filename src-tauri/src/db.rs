//! SQLite 数据层（代理配置 / 配额模板 / 账号元数据 / 自动切换规则 / 用量记录）
use anyhow::{anyhow, Result};
use rusqlite::{params, Connection};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use crate::types::{AccountMeta, AutoSwitchLog, AutoSwitchRule, QuotaTemplate, UsageAggRow, UsageFilters, UsageOverview, UsageRecord};

pub struct Database {
    pub conn: Mutex<Connection>,
}

const MIGRATION: &str = r#"
CREATE TABLE IF NOT EXISTS kv (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS quota_templates (
  provider_key TEXT PRIMARY KEY,
  name TEXT, method TEXT, url TEXT, headers_json TEXT, body TEXT,
  total_path TEXT, used_path TEXT, remaining_path TEXT,
  monthly_total_path TEXT, monthly_used_path TEXT, monthly_remaining_path TEXT,
  login_url TEXT, token_source TEXT, auth_mode TEXT, login_username TEXT,
  extra_json TEXT,
  unit TEXT, reset_time_path TEXT, monthly_reset_time_path TEXT,
  five_hour_total_path TEXT, five_hour_used_path TEXT, five_hour_remaining_path TEXT, five_hour_reset_time_path TEXT,
  weekly_total_path TEXT, weekly_used_path TEXT, weekly_remaining_path TEXT, weekly_reset_time_path TEXT
);
CREATE TABLE IF NOT EXISTS accounts (
  id TEXT PRIMARY KEY,
  short_id TEXT, user_id TEXT, provider TEXT, label TEXT,
  email TEXT, name TEXT, avatar TEXT, customer_id TEXT, note TEXT, captured_at TEXT
);
CREATE TABLE IF NOT EXISTS autoswitch_rules (
  id TEXT PRIMARY KEY,
  name TEXT, kind TEXT, enabled INTEGER,
  time_start TEXT, time_end TEXT, weekdays TEXT,
  family TEXT, from_provider TEXT, to_provider TEXT,
  from_model TEXT, to_model TEXT, project_dir TEXT,
  threshold REAL, priority INTEGER, created_at TEXT
);
CREATE TABLE IF NOT EXISTS provider_meta (
  provider_key TEXT PRIMARY KEY,
  is_coding_plan INTEGER DEFAULT 0
);
CREATE TABLE IF NOT EXISTS usage_records (
  request_id   TEXT PRIMARY KEY,
  started_at   TEXT,
  date         TEXT NOT NULL,
  provider_id  TEXT NOT NULL,
  model_id     TEXT NOT NULL,
  role         TEXT,
  query_source TEXT,
  input_tokens        INTEGER NOT NULL,
  output_tokens       INTEGER NOT NULL,
  total_tokens        INTEGER NOT NULL,
  cache_read_tokens   INTEGER NOT NULL,
  cache_write_tokens  INTEGER NOT NULL,
  duration_ms  REAL,
  tps          REAL,
  finish_reason TEXT,
  session_id   TEXT,
  raw_path     TEXT
);
CREATE INDEX IF NOT EXISTS idx_usage_date     ON usage_records(date);
CREATE INDEX IF NOT EXISTS idx_usage_provider ON usage_records(provider_id);
CREATE INDEX IF NOT EXISTS idx_usage_model    ON usage_records(model_id);
CREATE TABLE IF NOT EXISTS provider_aliases (
  provider_id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  source TEXT,
  updated_at TEXT
);
CREATE TABLE IF NOT EXISTS autoswitch_logs (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  rule_id TEXT NOT NULL,
  rule_name TEXT NOT NULL,
  trigger_type TEXT NOT NULL,
  success INTEGER NOT NULL,
  message TEXT,
  created_at TEXT NOT NULL
);
"#;

impl Database {
    pub fn open(path: PathBuf) -> Result<Self> {
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(MIGRATION)?;
        // 老库补列：autoswitch_rules.from_model / to_model / project_dir；
        // quota_templates.login_url / token_source（登录获取 Token 功能）；
        // provider_meta.is_primary（主供应商标记）
        // SQLite 无 ADD COLUMN IF NOT EXISTS，重复列错误忽略，其余报错上抛
        for col in ["from_model", "to_model", "project_dir"] {
            let sql = format!("ALTER TABLE autoswitch_rules ADD COLUMN {col} TEXT");
            if let Err(e) = conn.execute_batch(&sql) {
                let msg = e.to_string();
                if !msg.contains("duplicate column") {
                    return Err(anyhow!("迁移失败: {msg}"));
                }
            }
        }
        for col in [
            "login_url",
            "token_source",
            "auth_mode",
            "login_username",
            "extra_json",
            "monthly_total_path",
            "monthly_used_path",
            "monthly_remaining_path",
            "unit",
            "reset_time_path",
            "monthly_reset_time_path",
            "five_hour_total_path",
            "five_hour_used_path",
            "five_hour_remaining_path",
            "five_hour_reset_time_path",
            "weekly_total_path",
            "weekly_used_path",
            "weekly_remaining_path",
            "weekly_reset_time_path",
        ] {
            let sql = format!("ALTER TABLE quota_templates ADD COLUMN {col} TEXT");
            if let Err(e) = conn.execute_batch(&sql) {
                let msg = e.to_string();
                if !msg.contains("duplicate column") {
                    return Err(anyhow!("迁移失败: {msg}"));
                }
            }
        }
        {
            let sql = "ALTER TABLE provider_meta ADD COLUMN is_primary INTEGER DEFAULT 0";
            if let Err(e) = conn.execute_batch(sql) {
                let msg = e.to_string();
                if !msg.contains("duplicate column") {
                    return Err(anyhow!("迁移失败: {msg}"));
                }
            }
        }
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// 取连接锁（PoisonError 手动转 anyhow，避免 Connection 非 Sync 导致 ? 转换失败）
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_err(|e| anyhow!("db lock poisoned: {e}"))
    }

    pub fn kv_get(&self, key: &str) -> Option<String> {
        let c = self.conn.lock().ok()?;
        c.query_row("SELECT value FROM kv WHERE key=?1", params![key], |r| r.get(0))
            .ok()
    }

    pub fn kv_set(&self, key: &str, value: &str) -> Result<()> {
        let c = self.lock()?;
        c.execute(
            "INSERT INTO kv(key,value) VALUES(?1,?2)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    // ===== 账号 =====
    pub fn upsert_account(&self, a: &AccountMeta) -> Result<()> {
        let c = self.lock()?;
        c.execute(
            "INSERT INTO accounts(id,short_id,user_id,provider,label,email,name,avatar,customer_id,note,captured_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)
             ON CONFLICT(id) DO UPDATE SET label=excluded.label, note=excluded.note",
            params![a.id, a.short_id, a.user_id, a.provider, a.label, a.email, a.name, a.avatar, a.customer_id, a.note, a.captured_at],
        )?;
        Ok(())
    }

    pub fn list_accounts(&self) -> Result<Vec<AccountMeta>> {
        let c = self.lock()?;
        let mut stmt = c.prepare(
            "SELECT id,short_id,user_id,provider,label,email,name,avatar,customer_id,note,captured_at FROM accounts ORDER BY captured_at DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(AccountMeta {
                id: r.get(0)?,
                short_id: r.get(1)?,
                user_id: r.get(2)?,
                provider: r.get(3)?,
                label: r.get(4)?,
                email: r.get(5)?,
                name: r.get(6)?,
                avatar: r.get(7)?,
                customer_id: r.get(8)?,
                note: r.get(9)?,
                captured_at: r.get(10)?,
            })
        })?;
        let mut v = Vec::new();
        for row in rows {
            v.push(row?);
        }
        Ok(v)
    }

    pub fn get_account(&self, id: &str) -> Result<Option<AccountMeta>> {
        let c = self.lock()?;
        let r = c.query_row(
            "SELECT id,short_id,user_id,provider,label,email,name,avatar,customer_id,note,captured_at FROM accounts WHERE id=?1",
            params![id],
            |r| {
                Ok(AccountMeta {
                    id: r.get(0)?,
                    short_id: r.get(1)?,
                    user_id: r.get(2)?,
                    provider: r.get(3)?,
                    label: r.get(4)?,
                    email: r.get(5)?,
                    name: r.get(6)?,
                    avatar: r.get(7)?,
                    customer_id: r.get(8)?,
                    note: r.get(9)?,
                    captured_at: r.get(10)?,
                })
            },
        );
        match r {
            Ok(a) => Ok(Some(a)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn rename_account(&self, id: &str, label: &str) -> Result<()> {
        let c = self.lock()?;
        c.execute("UPDATE accounts SET label=?2 WHERE id=?1", params![id, label])?;
        Ok(())
    }

    pub fn delete_account(&self, id: &str) -> Result<()> {
        let c = self.lock()?;
        c.execute("DELETE FROM accounts WHERE id=?1", params![id])?;
        Ok(())
    }

    // ===== 配额模板 =====
    pub fn upsert_template(&self, t: &QuotaTemplate) -> Result<()> {
        let c = self.lock()?;
        c.execute(
            "INSERT INTO quota_templates(provider_key,name,method,url,headers_json,body,total_path,used_path,remaining_path,monthly_total_path,monthly_used_path,monthly_remaining_path,login_url,token_source,auth_mode,login_username,extra_json,unit,reset_time_path,monthly_reset_time_path,five_hour_total_path,five_hour_used_path,five_hour_remaining_path,five_hour_reset_time_path,weekly_total_path,weekly_used_path,weekly_remaining_path,weekly_reset_time_path)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25,?26,?27,?28)
             ON CONFLICT(provider_key) DO UPDATE SET name=excluded.name,method=excluded.method,url=excluded.url,
             headers_json=excluded.headers_json,body=excluded.body,total_path=excluded.total_path,
             used_path=excluded.used_path,remaining_path=excluded.remaining_path,
             monthly_total_path=excluded.monthly_total_path,monthly_used_path=excluded.monthly_used_path,
             monthly_remaining_path=excluded.monthly_remaining_path,
             login_url=excluded.login_url,token_source=excluded.token_source,
             auth_mode=excluded.auth_mode,login_username=excluded.login_username,
             extra_json=excluded.extra_json,
             unit=excluded.unit,reset_time_path=excluded.reset_time_path,
             monthly_reset_time_path=excluded.monthly_reset_time_path,
             five_hour_total_path=excluded.five_hour_total_path,five_hour_used_path=excluded.five_hour_used_path,
             five_hour_remaining_path=excluded.five_hour_remaining_path,five_hour_reset_time_path=excluded.five_hour_reset_time_path,
             weekly_total_path=excluded.weekly_total_path,weekly_used_path=excluded.weekly_used_path,
             weekly_remaining_path=excluded.weekly_remaining_path,weekly_reset_time_path=excluded.weekly_reset_time_path",
            params![t.provider_key, t.name, t.method, t.url, t.headers_json, t.body, t.total_path, t.used_path, t.remaining_path, t.monthly_total_path, t.monthly_used_path, t.monthly_remaining_path, t.login_url, t.token_source, t.auth_mode, t.login_username, t.extra_json, t.unit, t.reset_time_path, t.monthly_reset_time_path, t.five_hour_total_path, t.five_hour_used_path, t.five_hour_remaining_path, t.five_hour_reset_time_path, t.weekly_total_path, t.weekly_used_path, t.weekly_remaining_path, t.weekly_reset_time_path],
        )?;
        Ok(())
    }

    pub fn get_template(&self, provider_key: &str) -> Result<Option<QuotaTemplate>> {
        let c = self.lock()?;
        let r = c.query_row(
            "SELECT provider_key,name,method,url,headers_json,body,total_path,used_path,remaining_path,monthly_total_path,monthly_used_path,monthly_remaining_path,login_url,token_source,auth_mode,login_username,extra_json,unit,reset_time_path,monthly_reset_time_path,five_hour_total_path,five_hour_used_path,five_hour_remaining_path,five_hour_reset_time_path,weekly_total_path,weekly_used_path,weekly_remaining_path,weekly_reset_time_path FROM quota_templates WHERE provider_key=?1",
            params![provider_key],
            map_template_row,
        );
        match r {
            Ok(t) => Ok(Some(t)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn list_templates(&self) -> Result<Vec<QuotaTemplate>> {
        let c = self.lock()?;
        let mut stmt = c.prepare(
            "SELECT provider_key,name,method,url,headers_json,body,total_path,used_path,remaining_path,monthly_total_path,monthly_used_path,monthly_remaining_path,login_url,token_source,auth_mode,login_username,extra_json,unit,reset_time_path,monthly_reset_time_path,five_hour_total_path,five_hour_used_path,five_hour_remaining_path,five_hour_reset_time_path,weekly_total_path,weekly_used_path,weekly_remaining_path,weekly_reset_time_path FROM quota_templates",
        )?;
        let rows = stmt.query_map([], map_template_row)?;
        let mut v = Vec::new();
        for row in rows {
            v.push(row?);
        }
        Ok(v)
    }

    /// 删除某 provider 的配额查询模板
    pub fn delete_template(&self, provider_key: &str) -> Result<()> {
        let c = self.lock()?;
        c.execute(
            "DELETE FROM quota_templates WHERE provider_key=?1",
            params![provider_key],
        )?;
        Ok(())
    }

    // ===== 自动切换规则 =====
    pub fn list_rules(&self) -> Result<Vec<AutoSwitchRule>> {
        let c = self.lock()?;
        let mut stmt = c.prepare(
            "SELECT id,name,kind,enabled,time_start,time_end,weekdays,from_provider,from_model,to_provider,to_model,project_dir,threshold,priority,created_at FROM autoswitch_rules ORDER BY priority IS NULL, priority ASC, created_at ASC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(AutoSwitchRule {
                id: r.get(0)?,
                name: r.get(1)?,
                kind: r.get(2)?,
                enabled: r.get::<_, i64>(3)? != 0,
                time_start: r.get(4)?,
                time_end: r.get(5)?,
                weekdays: r.get(6)?,
                from_provider: r.get(7)?,
                from_model: r.get(8)?,
                to_provider: r.get(9)?,
                to_model: r.get(10)?,
                project_dir: r.get(11)?,
                threshold: r.get(12)?,
                priority: r.get(13)?,
                created_at: r.get(14)?,
            })
        })?;
        let mut v = Vec::new();
        for row in rows {
            v.push(row?);
        }
        Ok(v)
    }

    pub fn upsert_rule(&self, r: &AutoSwitchRule) -> Result<()> {
        let c = self.lock()?;
        c.execute(
            "INSERT INTO autoswitch_rules(id,name,kind,enabled,time_start,time_end,weekdays,from_provider,from_model,to_provider,to_model,project_dir,threshold,priority,created_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)
             ON CONFLICT(id) DO UPDATE SET name=excluded.name,kind=excluded.kind,enabled=excluded.enabled,
             time_start=excluded.time_start,time_end=excluded.time_end,weekdays=excluded.weekdays,
             from_provider=excluded.from_provider,from_model=excluded.from_model,
             to_provider=excluded.to_provider,to_model=excluded.to_model,
             project_dir=excluded.project_dir,
             threshold=excluded.threshold,priority=excluded.priority",
            params![r.id, r.name, r.kind, r.enabled as i64, r.time_start, r.time_end, r.weekdays, r.from_provider, r.from_model, r.to_provider, r.to_model, r.project_dir, r.threshold, r.priority, r.created_at],
        )?;
        Ok(())
    }

    /// 批量重排优先级：按 ordered_ids 顺序写 priority=0,1,2...
    pub fn reorder_rules(&self, ordered_ids: &[String]) -> Result<()> {
        let c = self.lock()?;
        let tx = c.unchecked_transaction()?;
        for (i, id) in ordered_ids.iter().enumerate() {
            tx.execute(
                "UPDATE autoswitch_rules SET priority=?1 WHERE id=?2",
                params![i as i64, id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn delete_rule(&self, id: &str) -> Result<()> {
        let c = self.lock()?;
        c.execute("DELETE FROM autoswitch_rules WHERE id=?1", params![id])?;
        Ok(())
    }

    // ===== 自动切换执行日志 =====

    pub fn insert_switch_log(&self, l: &AutoSwitchLog) -> Result<()> {
        let c = self.lock()?;
        c.execute(
            "INSERT INTO autoswitch_logs(rule_id,rule_name,trigger_type,success,message,created_at)
             VALUES(?1,?2,?3,?4,?5,?6)",
            params![l.rule_id, l.rule_name, l.trigger_type, l.success as i64, l.message, l.created_at],
        )?;
        Ok(())
    }

    /// 最近执行日志（时间倒序）
    pub fn list_switch_logs(&self, limit: i64) -> Result<Vec<AutoSwitchLog>> {
        let c = self.lock()?;
        let mut stmt = c.prepare(
            "SELECT id,rule_id,rule_name,trigger_type,success,message,created_at
             FROM autoswitch_logs ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], |r| {
            Ok(AutoSwitchLog {
                id: r.get(0)?,
                rule_id: r.get(1)?,
                rule_name: r.get(2)?,
                trigger_type: r.get(3)?,
                success: r.get::<_, i64>(4)? != 0,
                message: r.get(5)?,
                created_at: r.get(6)?,
            })
        })?;
        let mut v = Vec::new();
        for row in rows {
            v.push(row?);
        }
        Ok(v)
    }

    // ===== provider 元数据（主供应商标记）=====
    /// 设置/取消主供应商（全局唯一：设置时先清除已有标记）
    pub fn set_provider_primary(&self, provider_key: &str, enabled: bool) -> Result<()> {
        let c = self.lock()?;
        if enabled {
            c.execute("UPDATE provider_meta SET is_primary=0 WHERE is_primary=1", [])?;
            c.execute(
                "INSERT INTO provider_meta(provider_key,is_primary) VALUES(?1,1)
                 ON CONFLICT(provider_key) DO UPDATE SET is_primary=1",
                params![provider_key],
            )?;
        } else {
            c.execute(
                "INSERT INTO provider_meta(provider_key,is_primary) VALUES(?1,0)
                 ON CONFLICT(provider_key) DO UPDATE SET is_primary=0",
                params![provider_key],
            )?;
        }
        Ok(())
    }

    /// 取主供应商 key（未设置返回 None）
    pub fn primary_provider_key(&self) -> Result<Option<String>> {
        let c = self.lock()?;
        let r = c.query_row(
            "SELECT provider_key FROM provider_meta WHERE is_primary=1",
            [],
            |r| r.get::<_, String>(0),
        );
        match r {
            Ok(k) => Ok(Some(k)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// 删除 provider 时清除其主供应商标记（若是），总览回退自动识别
    pub fn clear_primary_if(&self, provider_key: &str) -> Result<()> {
        let c = self.lock()?;
        c.execute(
            "UPDATE provider_meta SET is_primary=0 WHERE provider_key=?1 AND is_primary=1",
            params![provider_key],
        )?;
        Ok(())
    }

    // ===== 用量记录（解析自 ~/.zcode/cli/rollout）=====

    /// 批量写入用量记录（按 request_id 去重，返回实际新增数）
    pub fn insert_usage_ignore(&self, recs: &[UsageRecord]) -> Result<usize> {
        let mut c = self.lock()?;
        let tx = c.transaction()?;
        let mut inserted = 0usize;
        for r in recs {
            let n = tx.execute(
                "INSERT OR IGNORE INTO usage_records(
                    request_id,started_at,date,provider_id,model_id,role,query_source,
                    input_tokens,output_tokens,total_tokens,cache_read_tokens,cache_write_tokens,
                    duration_ms,tps,finish_reason,session_id,raw_path)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)",
                params![
                    r.request_id, r.started_at, r.date, r.provider_id, r.model_id, r.role, r.query_source,
                    r.input_tokens, r.output_tokens, r.total_tokens, r.cache_read_tokens, r.cache_write_tokens,
                    r.duration_ms, r.tps, r.finish_reason, r.session_id, r.raw_path
                ],
            )?;
            inserted += n;
        }
        tx.commit()?;
        Ok(inserted)
    }

    pub fn count_usage(&self) -> Result<i64> {
        let c = self.lock()?;
        Ok(c.query_row("SELECT COUNT(*) FROM usage_records", [], |r| r.get(0))?)
    }

    /// 清空所有用量记录（用于一次性迁移后重新解析）
    pub fn clear_usage(&self) -> Result<()> {
        let c = self.lock()?;
        c.execute("DELETE FROM usage_records", [])?;
        Ok(())
    }

    /// 全部用量记录的 request_id（同步对账用）
    pub fn list_usage_request_ids(&self) -> Result<Vec<String>> {
        let c = self.lock()?;
        let mut stmt = c.prepare("SELECT request_id FROM usage_records")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        let mut v = Vec::new();
        for row in rows {
            v.push(row?);
        }
        Ok(v)
    }

    /// 按 request_id 批量删除本地用量记录（zcode 侧已清理的行，同步时对账回收）
    pub fn delete_usage_by_request_ids(&self, ids: &[String]) -> Result<usize> {
        if ids.is_empty() {
            return Ok(0);
        }
        let c = self.lock()?;
        let mut n = 0;
        for chunk in ids.chunks(400) {
            let placeholders = vec!["?"; chunk.len()].join(",");
            let sql = format!("DELETE FROM usage_records WHERE request_id IN ({placeholders})");
            n += c.execute(&sql, rusqlite::params_from_iter(chunk))? as usize;
        }
        Ok(n)
    }

    /// 按会话 id 删除本地用量记录（会话在 zcode 库被删除后调用，保持用量页口径一致）
    pub fn delete_usage_by_sessions(&self, session_ids: &[String]) -> Result<usize> {
        if session_ids.is_empty() {
            return Ok(0);
        }
        let c = self.lock()?;
        let mut n = 0;
        for chunk in session_ids.chunks(400) {
            let placeholders = vec!["?"; chunk.len()].join(",");
            let sql = format!("DELETE FROM usage_records WHERE session_id IN ({placeholders})");
            n += c.execute(&sql, rusqlite::params_from_iter(chunk))? as usize;
        }
        Ok(n)
    }

    /// 筛选项：去重后的供应商 / 模型 / 角色 + 日期范围 + 总条数
    pub fn usage_filters(&self) -> Result<UsageFilters> {
        let c = self.lock()?;
        let providers = col_strs(&c, "SELECT DISTINCT provider_id FROM usage_records ORDER BY provider_id")?;
        let models = col_strs(&c, "SELECT DISTINCT model_id FROM usage_records ORDER BY model_id")?;
        let roles = col_strs(&c, "SELECT DISTINCT role FROM usage_records WHERE role IS NOT NULL ORDER BY role")?;
        let total_records: i64 = c.query_row("SELECT COUNT(*) FROM usage_records", [], |r| r.get(0))?;
        let min_date: Option<String> =
            c.query_row("SELECT MIN(date) FROM usage_records", [], |r| r.get::<_, Option<String>>(0))
                .unwrap_or(None);
        let max_date: Option<String> =
            c.query_row("SELECT MAX(date) FROM usage_records", [], |r| r.get::<_, Option<String>>(0))
                .unwrap_or(None);
        Ok(UsageFilters {
            providers,
            models,
            roles,
            min_date,
            max_date,
            total_records,
        })
    }

    /// 整体汇总（随筛选条件）
    pub fn usage_overview(
        &self,
        from: Option<&str>,
        to: Option<&str>,
        provider: Option<&str>,
        model: Option<&str>,
        role: Option<&str>,
    ) -> Result<UsageOverview> {
        let c = self.lock()?;
        let (clause, params) = usage_filter_params(from, to, provider, model, role);
        let sql = format!(
            "SELECT COUNT(*), \
             COALESCE(SUM(input_tokens),0), COALESCE(SUM(output_tokens),0), \
             COALESCE(SUM(cache_read_tokens),0), COALESCE(SUM(cache_write_tokens),0), \
             COALESCE(SUM(total_tokens),0), \
             AVG(tps), MAX(tps), MIN(tps), AVG(duration_ms) \
             FROM usage_records{clause}"
        );
        let row = c.query_row(&sql, rusqlite::params_from_iter(params.iter()), |r| {
            Ok(UsageOverview {
                calls: r.get::<_, i64>(0)?,
                input_tokens: r.get::<_, i64>(1)?,
                output_tokens: r.get::<_, i64>(2)?,
                cache_read_tokens: r.get::<_, i64>(3)?,
                cache_write_tokens: r.get::<_, i64>(4)?,
                total_tokens: r.get::<_, i64>(5)?,
                avg_tps: r.get::<_, Option<f64>>(6)?,
                max_tps: r.get::<_, Option<f64>>(7)?,
                min_tps: r.get::<_, Option<f64>>(8)?,
                avg_duration_ms: r.get::<_, Option<f64>>(9)?,
            })
        });
        match row {
            Ok(o) => Ok(o),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(UsageOverview::default()),
            Err(e) => Err(e.into()),
        }
    }

    /// 分组聚合（按供应商 / 模型 / 日期）
    pub fn usage_aggregate(
        &self,
        group_by: &str,
        from: Option<&str>,
        to: Option<&str>,
        provider: Option<&str>,
        model: Option<&str>,
        role: Option<&str>,
    ) -> Result<Vec<UsageAggRow>> {
        // 白名单映射，避免拼接前端输入
        let col = match group_by {
            "model" => "model_id",
            "date" => "date",
            _ => "provider_id",
        };
        let c = self.lock()?;
        let (clause, params) = usage_filter_params(from, to, provider, model, role);
        let sql = format!(
            "SELECT {col} AS k, COUNT(*), \
             COALESCE(SUM(input_tokens),0), COALESCE(SUM(output_tokens),0), \
             COALESCE(SUM(cache_read_tokens),0), COALESCE(SUM(cache_write_tokens),0), \
             COALESCE(SUM(total_tokens),0), \
             AVG(tps), MAX(tps), MIN(tps), AVG(duration_ms) \
             FROM usage_records{clause} GROUP BY {col} ORDER BY SUM(total_tokens) DESC"
        );
        let mut stmt = c.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), |r| {
            let key: String = r.get(0)?;
            Ok(UsageAggRow {
                label: key.clone(),
                key,
                calls: r.get(1)?,
                input_tokens: r.get(2)?,
                output_tokens: r.get(3)?,
                cache_read_tokens: r.get(4)?,
                cache_write_tokens: r.get(5)?,
                total_tokens: r.get(6)?,
                avg_tps: r.get::<_, Option<f64>>(7)?,
                max_tps: r.get::<_, Option<f64>>(8)?,
                min_tps: r.get::<_, Option<f64>>(9)?,
                avg_duration_ms: r.get::<_, Option<f64>>(10)?,
            })
        })?;
        let mut v = Vec::new();
        for row in rows {
            v.push(row?);
        }
        Ok(v)
    }

    /// 明细记录（分页，按时间倒序）
    pub fn usage_records_list(
        &self,
        from: Option<&str>,
        to: Option<&str>,
        provider: Option<&str>,
        model: Option<&str>,
        role: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<UsageRecord>> {
        let c = self.lock()?;
        let (clause, mut params) = usage_filter_params(from, to, provider, model, role);
        params.push(Box::new(limit));
        params.push(Box::new(offset));
        let sql = format!(
            "SELECT request_id,started_at,date,provider_id,model_id,role,query_source,\
             input_tokens,output_tokens,total_tokens,cache_read_tokens,cache_write_tokens,\
             duration_ms,tps,finish_reason,session_id,raw_path \
             FROM usage_records{clause} ORDER BY started_at DESC LIMIT ? OFFSET ?"
        );
        let mut stmt = c.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), map_usage_record)?;
        let mut v = Vec::new();
        for row in rows {
            v.push(row?);
        }
        Ok(v)
    }

    // ===== 供应商别名（解析自 transcript，独立于 zcode 配置，删渠道不影响）=====

    /// 写入/更新一条供应商别名（来源：transcript）
    pub fn upsert_provider_alias(&self, provider_id: &str, name: &str, source: &str) -> Result<()> {
        let c = self.lock()?;
        let now = chrono::Utc::now().to_rfc3339();
        c.execute(
            "INSERT INTO provider_aliases(provider_id,name,source,updated_at)
             VALUES(?1,?2,?3,?4)
             ON CONFLICT(provider_id) DO UPDATE SET name=excluded.name, source=excluded.source, updated_at=excluded.updated_at",
            params![provider_id, name, source, now],
        )?;
        Ok(())
    }

    /// 取全部别名映射 provider_id -> name
    pub fn provider_alias_map(&self) -> Result<HashMap<String, String>> {
        let c = self.lock()?;
        let mut stmt = c.prepare("SELECT provider_id, name FROM provider_aliases")?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        let mut m = HashMap::new();
        for row in rows {
            let (k, v) = row?;
            m.insert(k, v);
        }
        Ok(m)
    }
}

/// 单行 → UsageRecord
fn map_usage_record(r: &rusqlite::Row<'_>) -> rusqlite::Result<UsageRecord> {
    Ok(UsageRecord {
        request_id: r.get(0)?,
        started_at: r.get(1)?,
        date: r.get(2)?,
        provider_id: r.get(3)?,
        model_id: r.get(4)?,
        role: r.get(5)?,
        query_source: r.get(6)?,
        input_tokens: r.get(7)?,
        output_tokens: r.get(8)?,
        total_tokens: r.get(9)?,
        cache_read_tokens: r.get(10)?,
        cache_write_tokens: r.get(11)?,
        duration_ms: r.get(12)?,
        tps: r.get(13)?,
        finish_reason: r.get(14)?,
        session_id: r.get(15)?,
        raw_path: r.get(16)?,
    })
}

/// 单行 → QuotaTemplate（login_url/token_source 等可空列可能为 NULL）
fn map_template_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<QuotaTemplate> {
    Ok(QuotaTemplate {
        provider_key: r.get(0)?,
        name: r.get(1)?,
        method: r.get(2)?,
        url: r.get(3)?,
        headers_json: r.get(4)?,
        body: r.get(5)?,
        total_path: r.get(6)?,
        used_path: r.get(7)?,
        remaining_path: r.get(8)?,
        monthly_total_path: r.get(9)?,
        monthly_used_path: r.get(10)?,
        monthly_remaining_path: r.get(11)?,
        login_url: r.get(12)?,
        token_source: r.get(13)?,
        auth_mode: r.get(14)?,
        login_username: r.get(15)?,
        extra_json: r.get(16)?,
        unit: r.get(17)?,
        reset_time_path: r.get(18)?,
        monthly_reset_time_path: r.get(19)?,
        five_hour_total_path: r.get(20)?,
        five_hour_used_path: r.get(21)?,
        five_hour_remaining_path: r.get(22)?,
        five_hour_reset_time_path: r.get(23)?,
        weekly_total_path: r.get(24)?,
        weekly_used_path: r.get(25)?,
        weekly_remaining_path: r.get(26)?,
        weekly_reset_time_path: r.get(27)?,
    })
}

/// 取某列去重后的字符串列表
fn col_strs(c: &Connection, sql: &str) -> Result<Vec<String>> {
    let mut stmt = c.prepare(sql)?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    let mut v = Vec::new();
    for row in rows {
        v.push(row?);
    }
    Ok(v)
}

/// 构造用量查询的 WHERE 子句 + 绑定参数（所有值以 Box<dyn ToSql> 持有）
fn usage_filter_params(
    from: Option<&str>,
    to: Option<&str>,
    provider: Option<&str>,
    model: Option<&str>,
    role: Option<&str>,
) -> (String, Vec<Box<dyn rusqlite::ToSql>>) {
    let mut parts: Vec<String> = Vec::new();
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if let Some(v) = from {
        parts.push("date >= ?".to_string());
        params.push(Box::new(v.to_string()));
    }
    if let Some(v) = to {
        parts.push("date <= ?".to_string());
        params.push(Box::new(v.to_string()));
    }
    if let Some(v) = provider {
        parts.push("provider_id = ?".to_string());
        params.push(Box::new(v.to_string()));
    }
    if let Some(v) = model {
        parts.push("model_id = ?".to_string());
        params.push(Box::new(v.to_string()));
    }
    if let Some(v) = role {
        parts.push("role = ?".to_string());
        params.push(Box::new(v.to_string()));
    }
    let clause = if parts.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", parts.join(" AND "))
    };
    (clause, params)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 前端 invoke 载荷 → serde 反序列化 → upsert → get 的完整回路。
    /// 复现「火山 AK/SK 填写后无法保存」：弹窗初始模板只带 providerKey/method，
    /// 载荷缺 name/url 等字段，若 serde 不容忍缺失 Option 字段则 upsert 直接报错。
    #[test]
    fn template_extra_json_roundtrip_minimal_payload() {
        let dir = std::env::temp_dir().join(format!("za-db-test-{}", uuid::Uuid::new_v4()));
        let db = Database::open(dir.join("t.db")).expect("open");

        let payload = r#"{"providerKey":"volc","method":"GET","extraJson":"{\"accessKeyId\":\"AK123\",\"secretAccessKey\":\"SK456\"}"}"#;
        let t: crate::types::QuotaTemplate =
            serde_json::from_str(payload).expect("serde 应容忍缺失的 Option 字段");
        assert_eq!(
            t.extra_json.as_deref(),
            Some(r#"{"accessKeyId":"AK123","secretAccessKey":"SK456"}"#)
        );
        db.upsert_template(&t).expect("upsert");
        let got = db.get_template("volc").expect("get").expect("row missing");
        assert_eq!(got.extra_json, t.extra_json, "extra_json 应完整落库");

        std::fs::remove_dir_all(&dir).ok();
    }
}
