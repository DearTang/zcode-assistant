//! 悬浮球窗口：固定显示（贴右侧初始位置，可拖拽，不自动隐藏）
//! + 点击展开的迷你面板窗口。
//!
//! 诊断日志走 eprintln!（直写 stderr，无需 logger backend），定位「点击不展开 / 拖不动」类问题。
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

const LOGICAL_BALL: f64 = 64.0; // 球视觉尺寸
const LOGICAL_WIN_W: f64 = 100.0; // 窗口宽（容纳下方 tooltip，避免裁剪）
const LOGICAL_WIN_H: f64 = 112.0; // 窗口高：球 64 + tooltip 区
const LOGICAL_PANEL_W: f64 = 280.0;
const LOGICAL_PANEL_H: f64 = 300.0;

/// 面板固定态：固定（true）时忽略球/面板鼠标离开的自动收起联动，
/// 点击面板窗口外也不会关闭；再次单击球或点面板 ✕ 才收起。
/// 任何隐藏面板的路径都走 hide_panel，在那里统一清除并广播。
static PANEL_PINNED: AtomicBool = AtomicBool::new(false);

fn broadcast_pin_state(app: &AppHandle) {
    let pinned = PANEL_PINNED.load(Ordering::SeqCst);
    let _ = app.emit("float://panel-pinned", pinned);
}

/// 创建悬浮球（幂等），初始贴右侧偏上，固定显示。
pub fn ensure_float_ball(app: &AppHandle) -> tauri::Result<()> {
    eprintln!("[float-ball] ensure_float_ball: start");
    if app.get_webview_window("float-ball").is_some() {
        eprintln!("[float-ball] already exists, skip");
        return Ok(());
    }
    let win = WebviewWindowBuilder::new(app, "float-ball", WebviewUrl::App("index.html".into()))
        .title("")
        .inner_size(LOGICAL_WIN_W, LOGICAL_WIN_H)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .closable(false)
        .decorations(false)
        .transparent(true)
        // 置顶：悬浮球作为常驻配额监控，需始终可见且不被主窗口遮挡（否则点不到/拖不动）
        .always_on_top(true)
        .skip_taskbar(true)
        .shadow(false)
        .visible(false)
        .build()
        .map_err(|e| {
            eprintln!("[float-ball] build failed: {e}");
            e
        })?;
    eprintln!("[float-ball] window built (hidden)");

    dock_to_right(&win);

    // 延迟 show，等 CSS 渲染完，避免不透明闪烁
    let app2 = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        std::thread::sleep(Duration::from_millis(200));
        if let Some(w) = app2.get_webview_window("float-ball") {
            match w.show() {
                Ok(_) => eprintln!("[float-ball] shown after delay"),
                Err(e) => eprintln!("[float-ball] show failed: {e}"),
            }
        } else {
            eprintln!("[float-ball] window gone before delayed show");
        }
    });

    Ok(())
}

/// 初始定位到屏幕右侧偏上 1/3
fn dock_to_right(win: &WebviewWindow) {
    let Some(monitor) = win.current_monitor().ok().flatten() else {
        eprintln!("[float-ball] dock_to_right: no monitor");
        return;
    };
    let scale = monitor.scale_factor();
    let size = monitor.size();
    let win_w = (LOGICAL_WIN_W * scale).round() as i32;
    let ball_phys = (LOGICAL_BALL * scale).round() as i32;
    let mw = size.width as i32;
    let mh = size.height as i32;
    let x = mw - win_w;
    let y = (mh - ball_phys) / 3;
    eprintln!(
        "[float-ball] dock_to_right: monitor {mw}x{mh} scale={scale} win_w={win_w} ball_phys={ball_phys} → ({x},{y})"
    );
    if let Err(e) = win.set_position(PhysicalPosition::new(x, y)) {
        eprintln!("[float-ball] dock_to_right set_position failed: {e}");
    }
}

// ===== 对外控制 =====
pub fn show(app: &AppHandle) -> tauri::Result<()> {
    eprintln!("[float-ball] show");
    if let Some(w) = app.get_webview_window("float-ball") {
        if let Err(e) = w.show() {
            eprintln!("[float-ball] show failed: {e}");
        }
        Ok(())
    } else {
        eprintln!("[float-ball] not exist, ensure...");
        ensure_float_ball(app)
    }
}

pub fn hide(app: &AppHandle) {
    eprintln!("[float-ball] hide");
    if let Some(w) = app.get_webview_window("float-ball") {
        if let Err(e) = w.hide() {
            eprintln!("[float-ball] hide failed: {e}");
        }
    }
}

/// 切换悬浮球显隐，返回切换后是否可见（供调用方持久化偏好）
pub fn toggle(app: &AppHandle) -> tauri::Result<bool> {
    eprintln!("[float-ball] toggle");
    if let Some(w) = app.get_webview_window("float-ball") {
        if w.is_visible().unwrap_or(false) {
            if let Err(e) = w.hide() {
                eprintln!("[float-ball] toggle hide failed: {e}");
            }
            Ok(false)
        } else {
            if let Err(e) = w.show() {
                eprintln!("[float-ball] toggle show failed: {e}");
            }
            Ok(true)
        }
    } else {
        eprintln!("[float-ball] not exist, ensure...");
        ensure_float_ball(app)?;
        Ok(true)
    }
}

// ===== 展开面板 =====
pub fn toggle_panel(app: &AppHandle) -> tauri::Result<()> {
    eprintln!("[float-panel] toggle_panel: start");
    if app.get_webview_window("float-panel").is_none() {
        eprintln!("[float-panel] not exist, creating...");
        create_panel(app)?;
    } else {
        eprintln!("[float-panel] already exists");
    }
    if let Some(panel) = app.get_webview_window("float-panel") {
        let visible = panel.is_visible().unwrap_or(false);
        eprintln!("[float-panel] current visible={visible}");
        if visible {
            match panel.hide() {
                Ok(_) => eprintln!("[float-panel] hidden"),
                Err(e) => eprintln!("[float-panel] hide failed: {e}"),
            }
        } else {
            position_panel_next_to_ball(app);
            match panel.show() {
                Ok(_) => eprintln!("[float-panel] shown"),
                Err(e) => eprintln!("[float-panel] show failed: {e}"),
            }
            match panel.set_focus() {
                Ok(_) => eprintln!("[float-panel] focused"),
                Err(e) => eprintln!("[float-panel] set_focus failed: {e}"),
            }
        }
    } else {
        eprintln!("[float-panel] panel still None after create");
    }
    Ok(())
}

pub fn hide_panel(app: &AppHandle) {
    eprintln!("[float-panel] hide_panel");
    // 面板已隐藏即不存在「固定显示」；清除并广播，让悬浮球指示灯、面板状态同步复位
    if PANEL_PINNED.swap(false, Ordering::SeqCst) {
        broadcast_pin_state(app);
    }
    if let Some(p) = app.get_webview_window("float-panel") {
        if let Err(e) = p.hide() {
            eprintln!("[float-panel] hide_panel failed: {e}");
        }
    }
}

/// 单击悬浮球触发的「固定 / 取消固定」面板：
/// - 未固定 → 展开并固定（常驻：鼠标移开、点击面板窗口外都不收起）
/// - 已固定 → 收起（hide_panel 顺带清固定态）
/// hover 快速查看的自动收起行为不受影响（固定态仅由本函数置位）。
pub fn toggle_pin_panel(app: &AppHandle) -> tauri::Result<()> {
    let pinned = PANEL_PINNED.load(Ordering::SeqCst);
    if pinned {
        eprintln!("[float-panel] unpin → hide");
        hide_panel(app);
    } else {
        eprintln!("[float-panel] pin → show");
        PANEL_PINNED.store(true, Ordering::SeqCst);
        show_panel(app)?;
        broadcast_pin_state(app);
    }
    Ok(())
}

/// 展开面板：幂等纯显示（不存在则创建后显示），**不 toggle、不抢焦点**。
/// 供悬浮球 hover 触发 —— set_focus 会让球窗口失焦闪烁，故仅 show。
pub fn show_panel(app: &AppHandle) -> tauri::Result<()> {
    eprintln!("[float-panel] show_panel: start");
    if app.get_webview_window("float-panel").is_none() {
        eprintln!("[float-panel] not exist, creating...");
        create_panel(app)?;
    }
    if let Some(panel) = app.get_webview_window("float-panel") {
        let visible = panel.is_visible().unwrap_or(false);
        eprintln!("[float-panel] show_panel current visible={visible}");
        if !visible {
            position_panel_next_to_ball(app);
            match panel.show() {
                Ok(_) => eprintln!("[float-panel] show_panel shown"),
                Err(e) => eprintln!("[float-panel] show_panel show failed: {e}"),
            }
        }
    } else {
        eprintln!("[float-panel] panel still None after create");
    }
    Ok(())
}

fn create_panel(app: &AppHandle) -> tauri::Result<()> {
    eprintln!(
        "[float-panel] create_panel: building {LOGICAL_PANEL_W}x{LOGICAL_PANEL_H}..."
    );
    WebviewWindowBuilder::new(app, "float-panel", WebviewUrl::App("index.html".into()))
        .title("")
        .inner_size(LOGICAL_PANEL_W, LOGICAL_PANEL_H)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .shadow(false)
        .visible(false)
        .build()
        .map_err(|e| {
            eprintln!("[float-panel] build failed: {e}");
            e
        })?;
    eprintln!("[float-panel] created (hidden, awaiting show)");
    Ok(())
}

/// 面板定位到悬浮球左侧
fn position_panel_next_to_ball(app: &AppHandle) {
    let panel = app.get_webview_window("float-panel");
    let ball = app.get_webview_window("float-ball");
    let (Some(panel), Some(ball)) = (panel, ball) else {
        eprintln!(
            "[float-panel] position: panel={} ball={} (skipped)",
            app.get_webview_window("float-panel").is_some(),
            app.get_webview_window("float-ball").is_some()
        );
        return;
    };
    let scale = panel.scale_factor().unwrap_or(1.0);
    let pw = (LOGICAL_PANEL_W * scale).round() as i32;
    match ball.outer_position() {
        Ok(bpos) => {
            let x = bpos.x - pw - 4;
            let y = bpos.y.max(8);
            eprintln!(
                "[float-panel] position: ball outer=({},{}) pw={pw} scale={scale} → ({x},{y})",
                bpos.x,
                bpos.y
            );
            if let Err(e) = panel.set_position(PhysicalPosition::new(x, y)) {
                eprintln!("[float-panel] set_position failed: {e}");
            }
        }
        Err(e) => eprintln!("[float-panel] ball.outer_position failed: {e}"),
    }
}
