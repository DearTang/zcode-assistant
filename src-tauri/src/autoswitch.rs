//! 自动切换调度器：每 60s 检查规则
//! - cron：选定星期的执行时间点 → 切到目标
//! - drain：当前 Coding Plan 配额剩余 ≤ 阈值 → 切到目标
//! - appstart：本应用每次启动后触发一次（不进周期调度）
//! 切换 = 直写会话模型选择（cli db 的 runtime/model_selection，会话恢复时还原
//! 模型——setting.json 只决定新会话默认供应商）+ 直改 setting.json 的 family
//! 选中键，再按偏好生效：偏好「切换后重启 ZCode」开（默认）→ kill + relaunch
//! ZCode，全部对话恢复时统一到达目标；关闭 → 免重启回退：键盘模拟在 ZCode 界面
//! 选中（下一轮对话生效）。成功后广播 model://switched。
//! 每次执行（含手动测试）写入 autoswitch_logs 执行日志。
use crate::commands::quota_cmd::fetch_coding_plan;
use crate::db::Database;
use crate::state::AppState;
use crate::types::{AutoSwitchLog, AutoSwitchRule};
use crate::zcode::{config_file, process};
use chrono::{DateTime, Datelike, Local, NaiveTime, Timelike};
use std::collections::{HashMap, HashSet};
use tauri::{AppHandle, Emitter, Manager};

/// 最近一次自动切换命中的目标（"provider|model"），供「已是目标则跳过」在
/// 供应商相同+带目标模型时判断是否真的切到过该模型（setting.json 只存供应商级选中）
const KV_LAST_TARGET: &str = "autoswitch_last_target";

fn target_key(provider: &str, model: &str) -> String {
    format!("{provider}|{model}")
}

pub fn spawn_scheduler(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            if let Err(e) = eval_rules(&app, &["cron", "drain"]).await {
                log::warn!("autoswitch tick error: {e}");
            }
        }
    });
}

/// 应用启动触发：启动后延迟数秒评估 appstart 规则（每次启动执行一次；
/// 延迟是为了等前端就绪，切换后的重启确认弹窗能立刻被看到）
pub fn spawn_startup_trigger(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        if let Err(e) = eval_rules(&app, &["appstart"]).await {
            log::warn!("autoswitch startup error: {e}");
        }
    });
}

/// 评估并执行规则：周期调度传 cron/drain，应用启动传 appstart。
/// 规则已按 priority 升序返回，遍历即按优先级触发。
async fn eval_rules(app: &AppHandle, kinds: &[&str]) -> Result<(), String> {
    let state = app.state::<AppState>();
    let rules = state.db.list_rules().map_err(|e| e.to_string())?;
    let now = Local::now();

    // 全局当前 family（切换目标维度），读一次复用；缺省则无法定位当前选中项
    let setting = config_file::read_setting().map_err(|e| e.to_string())?;
    let Some(family) = setting
        .get("providerFamilyDomain")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
    else {
        return Ok(());
    };
    // 当前选中的原始 key（如 `coding-plan:builtin:bigmodel-coding-plan`）→ 规范化出 providerId
    // （current_selected / selected_to_provider 抽到了 zcode::config_file，健康检测等复用）
    let current_provider = config_file::current_selected(&setting, &family)
        .as_deref()
        .map(config_file::selected_to_provider);

    let in_scope: Vec<&AutoSwitchRule> = rules
        .iter()
        .filter(|r| r.enabled && kinds.contains(&r.kind.as_str()))
        .collect();

    // 项目限定（空=全部项目）：仅当最近一次对话发生在规则所选项目时才触发。
    // 切换本身是全局的（zcode 只有全局 provider 选择），该门槛保证正在其他
    // 项目工作时不会被切走。有带项目的规则才查一次活跃项目（读 zcode db）。
    let active_project = in_scope
        .iter()
        .any(|r| r.project_dir.as_deref().map(|s| !s.is_empty()).unwrap_or(false))
        .then(latest_active_project)
        .flatten();

    for rule in in_scope {
        if rule.to_provider.is_empty() {
            continue;
        }
        // 已是目标 provider 则跳过（避免重复切换）。规则若指定了套餐内目标模型，
        // 供应商相同不代表会话已在目标模型上（模型是会话内状态，setting.json
        // 不存），还需确认最近一次切换已到达该模型（两种生效模式均能到达：
        // 重启模式重启后按库恢复，非重启模式恢复 / 新开时按库到达）
        if current_provider.as_deref() == Some(rule.to_provider.as_str()) {
            let model = rule.to_model.as_deref().unwrap_or("");
            let hit = if model.is_empty() {
                true
            } else {
                state
                    .db
                    .kv_get(KV_LAST_TARGET)
                    .is_some_and(|last| last == target_key(&rule.to_provider, model))
            };
            if hit {
                continue;
            }
        }

        // 项目限定（空=全部项目）：仅当最近对话项目 == 规则项目才触发
        if let Some(pd) = rule.project_dir.as_deref().filter(|s| !s.is_empty()) {
            if active_project.as_deref() != Some(norm_dir(pd).as_str()) {
                continue;
            }
        }

        // 空字符串（前端"任意"）视为 None：不限制源 provider
        let from = rule.from_provider.as_deref().filter(|s| !s.is_empty());
        let from_match = from.is_none() || current_provider.as_deref() == from;

        let need = match rule.kind.as_str() {
            "cron" => {
                from_match
                    && at_time_point(&now, rule.time_start.as_deref(), rule.weekdays.as_deref())
                    && !fired_today(&state.db, &rule.id, rule.time_start.as_deref(), &now)
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
            // 启动触发：门槛（目标非空/非已目标/项目/源）已过，直接执行
            "appstart" => from_match,
            _ => false,
        };

        if need {
            // 切换含 kill/重启/等窗口就绪（最长数十秒）的同步阻塞，放阻塞线程池，
            // 避免占住 tokio worker（drain 规则的配额查询等异步任务还会继续调度）
            let app2 = app.clone();
            let family2 = family.clone();
            let rule2 = (*rule).clone();
            let result = tauri::async_runtime::spawn_blocking(move || {
                switch_provider(&app2, &family2, &rule2)
            })
            .await
            .map_err(|e| format!("切换任务异常: {e}"))
            .and_then(|r| r);
            match result {
                Ok(()) => {
                    // cron 为瞬时触发，成功后置位防当日重复（失败不置位，90s 窗口内下轮重试）；
                    // drain 自身已有「已是目标则跳过」保护
                    if rule.kind == "cron" {
                        mark_fired(&state.db, &rule.id, rule.time_start.as_deref(), &now);
                    }
                    log_attempt(&state.db, rule, &rule.kind, true, None);
                }
                Err(e) => {
                    log::warn!("autoswitch switch failed: {e}");
                    log_attempt(&state.db, rule, &rule.kind, false, Some(&e));
                }
            }
            // 一次切换（重启 / 键盘模拟）需数秒，一轮只执行优先级最高的命中规则，
            // 其余留待下一轮（60s 后）按需触发
            break;
        }
    }
    Ok(())
}

/// cron 执行时间点匹配：当前时刻落在 [start, start+90s] 且星期命中。
/// tick 每 60s 一次，90s 窗口保证不漏同一分钟；重复触发由 fired_today 兜底。
fn at_time_point(now: &DateTime<Local>, start: Option<&str>, weekdays: Option<&str>) -> bool {
    if !weekday_hit(now, weekdays) {
        return false;
    }
    let Some(s) = start else {
        return true;
    };
    let Ok(st) = NaiveTime::parse_from_str(s, "%H:%M") else {
        return true;
    };
    let start_sec = st.num_seconds_from_midnight() as i64;
    let now_sec = now.time().num_seconds_from_midnight() as i64;
    now_sec >= start_sec && now_sec < start_sec + 90
}

fn weekday_hit(now: &DateTime<Local>, weekdays: Option<&str>) -> bool {
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
    true
}

/// 当天该执行时间点是否已触发（键 = 规则id + 日期 + HH:MM）
fn fired_key(rule_id: &str, start: Option<&str>, now: &DateTime<Local>) -> String {
    format!(
        "autoswitch_fired_{}_{}_{}",
        rule_id,
        now.format("%Y%m%d"),
        start.unwrap_or("00:00")
    )
}

fn fired_today(db: &Database, rule_id: &str, start: Option<&str>, now: &DateTime<Local>) -> bool {
    db.kv_get(&fired_key(rule_id, start, now)).is_some()
}

fn mark_fired(db: &Database, rule_id: &str, start: Option<&str>, now: &DateTime<Local>) {
    let _ = db.kv_set(&fired_key(rule_id, start, now), "1");
}

/// 规则开启「同时切换主供应商」时：把 zcode-assistant 自己的主供应商标记
/// （provider_meta.is_primary）也切到规则目标供应商——总览 / 悬浮球 / 托盘的
/// 配额展示随之跟随。非致命：失败只记日志，不影响 ZCode 侧切换流程。
/// 放在 switch_provider 入口与 test_rule 的「无事可做」早退分支调用，
/// 保证即使 ZCode 已在目标供应商、主供应商标记也会被同步。
pub(crate) fn sync_primary_if_enabled(state: &AppState, rule: &AutoSwitchRule) {
    if !rule.switch_primary {
        return;
    }
    if let Err(e) = state.db.set_provider_primary(&rule.to_provider, true) {
        log::warn!("autoswitch: 联动切换主供应商失败: {e}");
    }
}

/// 执行切换（对全部符合条件的会话生效）：
///   1) 直写目标会话的模型选择（cli db 的 runtime/model_selection）：ZCode 会话
///      恢复时按该条目还原模型（免 UI 模拟的核心，setting.json 只决定新会话的
///      默认供应商）；作用域随规则的项目限定（空 = 全部活跃会话）；
///   2) 换供应商时再直改 setting.json 的 family 选中键；
///   3) 生效：偏好「切换后重启 ZCode」开（默认）→ kill + relaunch，重启后所有
///      会话恢复时统一到达目标；关 → 不重启，各会话在恢复 / 新开时到达目标
///      （不做单会话键盘模拟——只点一个会话与其余会话状态不一致）。
/// 完成后广播 model://switched 同步界面，并记录本次到达的目标（供跳过判断）。
/// 规则开启 switch_primary 时，先联动切换 zcode-assistant 主供应商标记。
pub(crate) fn switch_provider(
    app: &AppHandle,
    family: &str,
    rule: &AutoSwitchRule,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    let restart_mode =
        crate::commands::prefs_cmd::current_prefs(&state.db).switch_restart_zcode;

    // 0) 主供应商标记联动（无论后续 ZCode 配置是否需要变动都执行）
    sync_primary_if_enabled(&state, rule);

    // 收尾：广播「当前模型已切换」+ 记录本次到达的目标（供跳过判断）
    let finish = || {
        let _ = app.emit(
            "model://switched",
            serde_json::json!({ "providerKey": rule.to_provider }),
        );
        let _ = state.db.kv_set(
            KV_LAST_TARGET,
            &target_key(&rule.to_provider, rule.to_model.as_deref().unwrap_or("")),
        );
    };

    let mut setting = config_file::read_setting().map_err(|e| e.to_string())?;

    // 0) 已是目标供应商且无模型目标：无事可做（避免无谓写库 / 重启）
    let already = config_file::current_selected(&setting, family)
        .as_deref()
        .map(config_file::selected_to_provider)
        .as_deref()
        == Some(rule.to_provider.as_str());
    let target_model = rule.to_model.as_deref().filter(|m| !m.is_empty());
    if already && target_model.is_none() {
        finish();
        return Ok(());
    }

    // 1) 直写目标会话的模型选择（全部符合条件的会话，见函数头注释）
    let scope_dir = rule.project_dir.as_deref().filter(|s| !s.is_empty());
    let n = crate::sessions::write_model_selection(&rule.to_provider, target_model, scope_dir)
        .map_err(|e| format!("写入会话模型选择失败: {e}"))?;
    log::info!(
        "autoswitch: 已写入 {n} 个会话的模型选择（{} / {}）",
        rule.to_provider,
        target_model.unwrap_or("默认首个模型")
    );

    // 2) 换供应商：直改 setting.json 的 family 选中键（builtin 目标保留现有 mode
    //    前缀，如 coding-plan:，与 ZCode 实际存储格式一致）
    if !already {
        let mode_prefix = config_file::current_selected(&setting, family)
            .and_then(|cur| cur.split_once(':').map(|(p, _)| p.to_string()))
            .filter(|p| p != "builtin" && !p.is_empty());
        let key = match (&mode_prefix, rule.to_provider.starts_with("builtin:")) {
            (Some(p), true) => format!("{p}:{}", rule.to_provider),
            _ => rule.to_provider.clone(),
        };
        if let Some(obj) = setting.as_object_mut() {
            let keys = obj
                .entry("modelProviderFamilySelectedKeys".to_string())
                .or_insert_with(|| serde_json::json!({}));
            if let Some(m) = keys.as_object_mut() {
                m.insert(family.to_string(), serde_json::Value::String(key));
            }
        }
        config_file::write_setting(&setting).map_err(|e| format!("写入 setting.json 失败: {e}"))?;
    }

    // 3) 生效：重启模式下 kill + relaunch，重启后全部会话恢复时到达目标；
    //    关闭偏好则不重启（会话恢复 / 新开时到达），不再键盘模拟单个会话
    if restart_mode {
        process::restart()?;
    }

    finish();
    Ok(())
}

/// 记录一次切换执行日志（trigger: manual | cron | drain | appstart；
/// message 存失败原因或成功备注）。写日志失败只打日志，不影响切换流程。
pub(crate) fn log_attempt(
    db: &Database,
    rule: &AutoSwitchRule,
    trigger: &str,
    success: bool,
    message: Option<&str>,
) {
    let log = AutoSwitchLog {
        id: 0,
        rule_id: rule.id.clone(),
        rule_name: rule.name.clone(),
        trigger_type: trigger.to_string(),
        success,
        message: message.map(String::from),
        created_at: Local::now().to_rfc3339(),
    };
    if let Err(e) = db.insert_switch_log(&log) {
        log::warn!("写入自动切换执行日志失败: {e}");
    }
}

// ===== 项目相关（数据源：zcode cli db 的 session 表）=====

/// 路径规范化（小写、/ → \），用于跨数据源（setting.json / session 表）比较项目目录
pub(crate) fn norm_dir(p: &str) -> String {
    p.to_lowercase().replace('/', "\\")
}

/// 各项目会话统计：规范化目录 → (会话数, 最近更新毫秒)。db 打不开时返回空表
pub(crate) fn project_session_stats() -> HashMap<String, (i64, Option<i64>)> {
    let mut m = HashMap::new();
    let Some(path) = crate::zcode::paths::zcode_cli_db_path() else {
        return m;
    };
    let Ok(conn) = crate::usage::open_readonly(&path) else {
        return m;
    };
    let Ok(mut stmt) = conn.prepare(
        "SELECT directory, COUNT(*), MAX(time_updated) FROM session GROUP BY directory",
    ) else {
        return m;
    };
    let Ok(rows) = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, Option<i64>>(2)?,
        ))
    }) else {
        return m;
    };
    for (dir, cnt, last) in rows.flatten() {
        m.insert(norm_dir(&dir), (cnt, last));
    }
    m
}

/// 最近一次对话所在项目（规范化目录）；无会话或 db 打不开时返回 None
fn latest_active_project() -> Option<String> {
    let path = crate::zcode::paths::zcode_cli_db_path()?;
    let conn = crate::usage::open_readonly(&path).ok()?;
    conn.query_row(
        "SELECT directory FROM session ORDER BY time_updated DESC LIMIT 1",
        [],
        |r| r.get::<_, String>(0),
    )
    .ok()
    .map(|d| norm_dir(&d))
}
