//! 系统托盘：左键点击弹菜单（每5小时 / 每周配额分两项显示）。
//! 配额数据由前端 Dashboard 每 5s 查询并广播 quota://updated，托盘监听同步更新；
//! 「刷新配额」菜单转发 quota://refresh-requested，由 Dashboard 统一发起查询。
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    App, Emitter, Listener, Manager, Runtime,
};

pub fn setup(app: &App) -> tauri::Result<()> {
    let menu = build_menu(app, None, None);
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
                let _ = crate::float_ball::toggle(app);
            }
            "refresh_quota" => {
                // 转发给前端 Dashboard 统一查询（保持单一数据源）
                let _ = app.emit("quota://refresh-requested", ());
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .build(app)?;

    // 监听前端广播的配额更新，同步托盘菜单 / tooltip
    let app_handle = app.handle().clone();
    app.listen("quota://updated", move |event| {
        let payload = event.payload();
        let Ok(v) = serde_json::from_str::<serde_json::Value>(payload) else { return };
        let (h, w) = quota_parts(v.get("buckets"));
        if let Some(tray) = app_handle.tray_by_id("main-tray") {
            let menu = build_menu(&app_handle, h, w);
            let _ = tray.set_menu(Some(menu));
            let _ = tray.set_tooltip(Some(tooltip_text(h, w)));
        }
    });
    Ok(())
}

fn build_menu<M: Manager<R>, R: Runtime>(app: &M, h: Option<f64>, w: Option<f64>) -> Menu<R> {
    let m5 = MenuItem::with_id(app, "_q5h", &label("每5小时", h), false, None::<&str>)
        .expect("menu item");
    let mw = MenuItem::with_id(app, "_qw", &label("每周", w), false, None::<&str>)
        .expect("menu item");
    let show = MenuItem::with_id(app, "show_main", "显示主窗口", true, None::<&str>)
        .expect("menu item");
    let ball = MenuItem::with_id(app, "toggle_ball", "显示/隐藏悬浮球", true, None::<&str>)
        .expect("menu item");
    let refresh = MenuItem::with_id(app, "refresh_quota", "刷新配额", true, None::<&str>)
        .expect("menu item");
    let quit = MenuItem::with_id(app, "quit", "退出 zcode-assistant", true, None::<&str>)
        .expect("menu item");
    Menu::with_items(app, &[&m5, &mw, &show, &ball, &refresh, &quit]).expect("menu")
}

fn label(name: &str, pct: Option<f64>) -> String {
    match pct {
        Some(v) => format!("{name}: {v:.0}%"),
        None => format!("{name}: —"),
    }
}

/// tooltip：两行（每5小时 / 每周，已用占比）
fn tooltip_text(h: Option<f64>, w: Option<f64>) -> String {
    match (h, w) {
        (Some(a), Some(b)) => format!("每5小时: {a:.0}%\n每周: {b:.0}%"),
        (Some(a), None) => format!("每5小时: {a:.0}%"),
        (None, Some(b)) => format!("每周: {b:.0}%"),
        (None, None) => "无配额数据".to_string(),
    }
}

/// 从 buckets 取 (每5小时已用%, 每周已用%)
fn quota_parts(buckets: Option<&serde_json::Value>) -> (Option<f64>, Option<f64>) {
    let arr = match buckets.and_then(|b| b.as_array()) {
        Some(a) => a,
        None => return (None, None),
    };
    let used_pct = |needle: &str| -> Option<f64> {
        arr.iter().find_map(|b| {
            let name = b.get("name").and_then(|n| n.as_str())?;
            if !name.contains(needle) {
                return None;
            }
            let total = b.get("total").and_then(|v| v.as_f64())?;
            let used = b.get("used").and_then(|v| v.as_f64())?;
            if total > 0.0 {
                Some(used / total * 100.0)
            } else {
                None
            }
        })
    };
    (used_pct("5小时"), used_pct("每周"))
}
