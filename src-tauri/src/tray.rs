//! 系统托盘：左键点击弹菜单（每5小时 / 每周配额分两项显示）。
//! 配额数据由主窗口 App 全局轮询（每 5s）并广播 quota://updated，托盘监听同步更新；
//! 「刷新配额」菜单转发 quota://refresh-requested，由主窗口统一发起查询。
//! 展示方案（已用 / 剩余）读取 prefs（kv），prefs://updated 时用最近一次配额即时重建。
use chrono::{DateTime, Local};
use std::sync::Mutex;
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    App, AppHandle, Emitter, Listener, Manager, Runtime,
};

/// 最近一次配额广播 payload（prefs 变更时用它即时重建菜单，无需等下一轮轮询）
static LAST_QUOTA: Mutex<Option<serde_json::Value>> = Mutex::new(None);

pub fn setup(app: &App) -> tauri::Result<()> {
    let menu = build_menu(app, &QuotaParts::new(), false);
    let _tray = TrayIconBuilder::with_id("main-tray")
        .icon(
            app.default_window_icon()
                .cloned()
                .expect("missing default window icon"),
        )
        .tooltip("zcode-assistant")
        .menu(&menu)
        // 左键点击托盘直接弹出菜单（看到当前配额）
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show_main" => {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.unminimize();
                    let _ = w.set_focus();
                }
            }
            "toggle_ball" => {
                // 与设置页共用 kv 偏好：切换后持久化 + 广播，设置页开关同步刷新
                if let Ok(visible) = crate::float_ball::toggle(app) {
                    if let Some(s) = app.try_state::<crate::state::AppState>() {
                        let _ = s.db.kv_set(
                            crate::commands::prefs_cmd::KV_FLOAT_BALL,
                            if visible { "1" } else { "0" },
                        );
                        let _ = app.emit(
                            "prefs://updated",
                            crate::commands::prefs_cmd::current_prefs(&s.db),
                        );
                    }
                }
            }
            "refresh_quota" => {
                // 转发给主窗口统一查询（保持单一数据源，与视图无关）
                let _ = app.emit("quota://refresh-requested", ());
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .build(app)?;

    // 监听前端广播的配额更新，同步托盘菜单 / tooltip
    // tooltip 多行：应用名 / 供应商 / 套餐 / 每5小时已用+重置 / 每周已用+重置
    let app_handle = app.handle().clone();
    app.listen("quota://updated", move |event| {
        let payload = event.payload();
        let Ok(v) = serde_json::from_str::<serde_json::Value>(payload) else {
            return;
        };
        if let Ok(mut g) = LAST_QUOTA.lock() {
            *g = Some(v.clone());
        }
        refresh_tray(&app_handle, &v);
    });

    // 偏好变更（展示方案 / 悬浮球）→ 用最近一次配额即时重建菜单，不等下一轮轮询
    let app_handle2 = app.handle().clone();
    app.listen("prefs://updated", move |_event| {
        if let Some(v) = LAST_QUOTA.lock().ok().and_then(|g| g.clone()) {
            refresh_tray(&app_handle2, &v);
        }
    });
    Ok(())
}

/// 按最近配额 + 当前偏好重建托盘菜单与 tooltip
fn refresh_tray(app: &AppHandle, v: &serde_json::Value) {
    let p = quota_parts(v.get("buckets"));
    let remaining = app
        .try_state::<crate::state::AppState>()
        .and_then(|s| s.db.kv_get(crate::commands::prefs_cmd::KV_USAGE_DISPLAY))
        .map(|m| m == "remaining")
        .unwrap_or(false);
    let plan = v.get("planName").and_then(|x| x.as_str());
    let provider = v.get("providerName").and_then(|x| x.as_str());
    if let Some(tray) = app.tray_by_id("main-tray") {
        let menu = build_menu(app, &p, remaining);
        let _ = tray.set_menu(Some(menu));
        let _ = tray.set_tooltip(Some(tooltip_text(provider, plan, &p, remaining)));
    }
}

fn build_menu<M: Manager<R>, R: Runtime>(
    app: &M,
    p: &QuotaParts,
    remaining: bool,
) -> Menu<R> {
    let m5 = MenuItem::with_id(
        app,
        "_q5h",
        &label(&p.h5_name, p.h5, remaining),
        false,
        None::<&str>,
    )
    .expect("menu item");
    let mw = MenuItem::with_id(
        app,
        "_qw",
        &label(&p.w_name, p.w, remaining),
        false,
        None::<&str>,
    )
    .expect("menu item");
    let show = MenuItem::with_id(app, "show_main", "显示主窗口", true, None::<&str>)
        .expect("menu item");
    let ball = MenuItem::with_id(app, "toggle_ball", "显示/隐藏悬浮球", true, None::<&str>)
        .expect("menu item");
    let refresh = MenuItem::with_id(app, "refresh_quota", "刷新配额", true, None::<&str>)
        .expect("menu item");
    let quit = MenuItem::with_id(app, "quit", "退出应用", true, None::<&str>)
        .expect("menu item");
    Menu::with_items(app, &[&m5, &mw, &show, &ball, &refresh, &quit]).expect("menu")
}

/// 菜单项文案：按展示方案显示「已用 / 剩余」占比（颜色语义始终按已用度，数值随方案切换）
fn label(name: &str, used_pct: Option<f64>, remaining: bool) -> String {
    let suffix = if remaining { "剩余" } else { "已用" };
    match used_pct {
        Some(v) => {
            let shown = if remaining { 100.0 - v } else { v };
            format!("{name}{suffix}: {shown:.0}%")
        }
        None => format!("{name}{suffix}: —"),
    }
}

#[derive(Default)]
struct QuotaParts {
    h5: Option<f64>,
    w: Option<f64>,
    h5_name: String,
    w_name: String,
    h5_end: Option<String>,
    w_end: Option<String>,
}

impl QuotaParts {
    fn new() -> Self {
        Self {
            h5_name: "每5小时".into(),
            w_name: "每周".into(),
            ..Default::default()
        }
    }
}

/// tooltip：多行（应用名 / 供应商 / 套餐 / 占比按展示方案 / 重置时间分行）
fn tooltip_text(
    provider: Option<&str>,
    plan: Option<&str>,
    p: &QuotaParts,
    remaining: bool,
) -> String {
    let mut lines: Vec<String> = Vec::new();
    lines.push("zcode-assistant".to_string());
    if let Some(pv) = provider.filter(|s| !s.is_empty()) {
        lines.push(pv.to_string());
    }
    if let Some(pl) = plan.filter(|s| !s.is_empty()) {
        lines.push(pl.to_string());
    }
    lines.push(label(&p.h5_name, p.h5, remaining));
    lines.push(label(&p.w_name, p.w, remaining));
    lines.push(format!("{}重置: {}", p.h5_name, fmt_reset(p.h5_end.as_deref())));
    lines.push(format!("{}重置: {}", p.w_name, fmt_reset(p.w_end.as_deref())));
    let tip = lines.join("\n");
    // Windows 托盘 tooltip 上限 128 字符，超长截断保护
    if tip.chars().count() > 120 {
        let mut t: String = tip.chars().take(120).collect();
        t.push('…');
        t
    } else {
        tip
    }
}

/// periodEnd(ISO) → 本地时间紧凑显示：今天 HH:mm，跨天 MM-dd HH:mm；解析失败返回 —
fn fmt_reset(iso: Option<&str>) -> String {
    let Some(iso) = iso else {
        return "—".to_string();
    };
    let Ok(dt) = DateTime::parse_from_rfc3339(iso) else {
        return "—".to_string();
    };
    let local = dt.with_timezone(&Local);
    if local.date_naive() == Local::now().date_naive() {
        local.format("%H:%M").to_string()
    } else {
        local.format("%m-%d %H:%M").to_string()
    }
}

/// 从 buckets 取已用占比 + 名称 + periodEnd：
/// 智谱套餐有「每5小时 / 每周」两个 bucket，展示名固定为「每5小时 / 每周」；
/// 其余供应商（用量模板）无此命名，回退用前两个 bucket（名称取模板名）
fn quota_parts(buckets: Option<&serde_json::Value>) -> QuotaParts {
    let mut p = QuotaParts::new();
    let Some(arr) = buckets.and_then(|b| b.as_array()) else {
        return p;
    };
    // (已用占比, 重置时间)；total<=0 视为无数据
    let pick = |needle: &str| -> Option<(f64, Option<String>)> {
        arr.iter().find_map(|b| {
            let name = b.get("name").and_then(|n| n.as_str())?;
            if !name.contains(needle) {
                return None;
            }
            bucket_parts(b)
        })
    };
    if let Some((v, e)) = pick("5小时") {
        p.h5 = Some(v);
        p.h5_end = e;
    }
    if let Some((v, e)) = pick("每周") {
        p.w = Some(v);
        p.w_end = e;
    }
    // 模板供应商回退：无 5小时/每周 bucket 时用前两个 bucket
    if p.h5.is_none() && p.w.is_none() {
        if let Some(b) = arr.first() {
            if let Some((n, v, e)) = bucket_parts_named(b) {
                p.h5_name = n;
                p.h5 = Some(v);
                p.h5_end = e;
            }
        }
        if let Some(b) = arr.get(1) {
            if let Some((n, v, e)) = bucket_parts_named(b) {
                p.w_name = n;
                p.w = Some(v);
                p.w_end = e;
            }
        }
    }
    p
}

/// 单个 bucket → (已用占比, 重置时间)
fn bucket_parts(b: &serde_json::Value) -> Option<(f64, Option<String>)> {
    let total = b.get("total").and_then(|v| v.as_f64())?;
    let used = b.get("used").and_then(|v| v.as_f64())?;
    if total <= 0.0 {
        return None;
    }
    let end = b
        .get("periodEnd")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    Some((used / total * 100.0, end))
}

/// 单个 bucket → (名称, 已用占比, 重置时间)（回退路径保留模板自定义桶名）
fn bucket_parts_named(b: &serde_json::Value) -> Option<(String, f64, Option<String>)> {
    let (v, e) = bucket_parts(b)?;
    let name = b
        .get("name")
        .and_then(|n| n.as_str())
        .unwrap_or("配额")
        .to_string();
    Some((name, v, e))
}
