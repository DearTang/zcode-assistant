//! 窗口控制命令（前端 invoke）
//!
//! 涉及窗口创建/显示的命令声明为 async：在异步 runtime 线程执行，
//! 内部 WebviewWindowBuilder::build() 会 dispatch 到主线程完成。
//! 若用同步命令，build() 在主线程同步执行会占用消息循环 → 死锁
//! （toggle_panel 创建 float-panel 时实测卡在 create_panel 不返回）。
use tauri::{AppHandle, Manager};

#[tauri::command]
pub async fn show_main_window(app: AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

#[tauri::command]
pub async fn show_float_ball(app: AppHandle) -> Result<(), String> {
    crate::float_ball::show(&app).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn hide_float_ball(app: AppHandle) {
    crate::float_ball::hide(&app);
}

#[tauri::command]
pub async fn toggle_float_ball(app: AppHandle) -> Result<(), String> {
    crate::float_ball::toggle(&app).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn show_float_panel(app: AppHandle) -> Result<(), String> {
    crate::float_ball::toggle_panel(&app).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn hide_float_panel(app: AppHandle) {
    crate::float_ball::hide_panel(&app);
}

#[tauri::command]
pub async fn toggle_float_panel(app: AppHandle) -> Result<(), String> {
    crate::float_ball::toggle_panel(&app).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}

/// 用前端 canvas 绘制的 RGBA 像素替换托盘图标，实现"配额直接显示在任务栏"
#[tauri::command]
pub fn set_tray_icon(
    app: AppHandle,
    rgba: Vec<u8>,
    width: u32,
    height: u32,
) -> Result<(), String> {
    let img = tauri::image::Image::new_owned(rgba, width, height);
    if let Some(tray) = app.tray_by_id("main-tray") {
        tray.set_icon(Some(img)).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 前端日志桥：把 webview 的日志打到后端 stderr，便于在终端追踪悬浮窗交互链路
#[tauri::command]
pub fn fe_log(level: String, msg: String) {
    eprintln!("[FE][{level}] {msg}");
}
