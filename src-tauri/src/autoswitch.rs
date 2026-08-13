//! 自动切换调度器：每 60s 检查规则
//! - cron：当前时间在窗口内 + 当前 provider==from → 切到 to
//! - drain：当前 Coding Plan 配额剩余 ≤ 阈值 → 切到 to
//! 切换 = 改 setting.json + emit "zcode://restart-requested"（前端确认后重启 zcode）
use crate::commands::quota_cmd::fetch_coding_plan;
use crate::state::AppState;
use crate::zcode::config_file;
use chrono::{DateTime, Datelike, Local, NaiveTime};
use std::collections::HashSet;
use tauri::{AppHandle, Emitter, Manager};

pub fn spawn_scheduler(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            if let Err(e) = tick(&app).await {
                log::warn!("autoswitch tick error: {e}");
            }
        }
    });
}

async fn tick(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let rules = state.db.list_rules().map_err(|e| e.to_string())?;
    let now = Local::now();
    for rule in rules.iter().filter(|r| r.enabled) {
        let Some(family) = rule.family.as_ref() else {
            continue;
        };
        if rule.to_provider.is_empty() {
            continue;
        }
        let current = current_selected(family);
        // 已是目标 provider 则跳过（避免重复切换）
        if current.as_deref() == Some(rule.to_provider.as_str()) {
            continue;
        }

        let from_match = rule.from_provider.is_none()
            || rule.from_provider.as_deref() == current.as_deref();

        let need = match rule.kind.as_str() {
            "cron" => {
                from_match
                    && in_window(
                        &now,
                        rule.time_start.as_deref(),
                        rule.time_end.as_deref(),
                        rule.weekdays.as_deref(),
                    )
            }
            "drain" => {
                if !from_match {
                    false
                } else {
                    let threshold = rule.threshold.unwrap_or(0.0);
                    match fetch_coding_plan(state.inner()).await {
                        Ok(q) => q
                            .buckets
                            .first()
                            .map(|b| b.remaining <= threshold)
                            .unwrap_or(false),
                        Err(_) => false,
                    }
                }
            }
            _ => false,
        };

        if need {
            switch_provider(app, family, &rule.to_provider, &rule.name).await;
        }
    }
    Ok(())
}

fn current_selected(family: &str) -> Option<String> {
    let s = config_file::read_setting().ok()?;
    s.get("modelProviderFamilySelectedKeys")?
        .get(family)?
        .as_str()
        .map(String::from)
}

fn in_window(
    now: &DateTime<Local>,
    start: Option<&str>,
    end: Option<&str>,
    weekdays: Option<&str>,
) -> bool {
    if let Some(wd) = weekdays {
        if !wd.is_empty() {
            let set: HashSet<u32> = wd
                .split(',')
                .filter_map(|x| x.trim().parse::<u32>().ok())
                .collect();
            if !set.is_empty() {
                let day = now.weekday().num_days_from_monday() + 1; // 周一=1
                if !set.contains(&day) {
                    return false;
                }
            }
        }
    }
    let (Some(s), Some(e)) = (start, end) else {
        return true;
    };
    let Ok(st) = NaiveTime::parse_from_str(s, "%H:%M") else {
        return true;
    };
    let Ok(et) = NaiveTime::parse_from_str(e, "%H:%M") else {
        return true;
    };
    let t = now.time();
    t >= st && t < et
}

async fn switch_provider(app: &AppHandle, family: &str, to: &str, reason: &str) {
    if let Ok(mut setting) = config_file::read_setting() {
        if let Some(obj) = setting.as_object_mut() {
            let keys = obj
                .entry("modelProviderFamilySelectedKeys".to_string())
                .or_insert_with(|| serde_json::json!({}));
            if let Some(m) = keys.as_object_mut() {
                m.insert(family.to_string(), serde_json::Value::String(to.to_string()));
            }
            let _ = config_file::write_setting(&setting);
        }
    }
    let _ = app.emit(
        "zcode://restart-requested",
        serde_json::json!({
            "reason": format!("自动切换规则「{reason}」已切到 {to}，需重启 zcode 生效")
        }),
    );
}
