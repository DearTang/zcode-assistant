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

// 悬浮球显隐命令已移除：统一走 prefs_cmd::set_float_ball_visible（持久化偏好），
// 托盘菜单切换则直接调 float_ball::toggle 并回写偏好。

#[tauri::command]
pub async fn show_float_panel(app: AppHandle) -> Result<(), String> {
    crate::float_ball::show_panel(&app).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn hide_float_panel(app: AppHandle) {
    crate::float_ball::hide_panel(&app);
}

#[tauri::command]
pub async fn toggle_float_panel(app: AppHandle) -> Result<(), String> {
    crate::float_ball::toggle_panel(&app).map_err(|e| e.to_string())
}

/// 单击悬浮球触发：固定 / 取消固定展开面板（固定 = 鼠标移开、点击面板窗口外都不收起）。
/// 详见 float_ball::toggle_pin_panel。
#[tauri::command]
pub async fn toggle_float_panel_pin(app: AppHandle) -> Result<(), String> {
    crate::float_ball::toggle_pin_panel(&app).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}

/// 覆盖启动：结束当前进程并重新启动（单实例弹窗「覆盖启动」按钮调用）。
/// dev 模式（tauri dev）下应用进程退出会连带结束整个 dev 会话——tauri CLI
/// 随之关闭并杀掉 Vite，重新拉起的进程成为孤儿、页面再也刷不出 localhost。
/// 因此 debug 构建改为仅重载主窗口页面（后端代码本就会由 watcher 热重编译重启）。
#[tauri::command]
pub fn restart_app(app: AppHandle) {
    if cfg!(debug_assertions) {
        log::info!("dev 模式：跳过进程重启，仅重载主窗口页面");
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.eval("window.location.reload()");
        }
        return;
    }
    // 生产：app.restart() 与单实例插件存在竞态——新进程可能先于旧进程退出
    // 释放单实例锁启动，把自己判定为第二实例后随即退出（表现为重启失败）。
    // 改由 cmd 延迟 1 秒再启动新实例：旧进程先退出释放锁，新实例正常启动；
    // cmd 由旧进程派生，新实例继承相同的完整性级别（装完安装器自动启动的
    // 提升态场景也保持一致）。
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        if let Ok(exe) = std::env::current_exe() {
            let exe_str = exe.to_string_lossy();
            let mut c = std::process::Command::new("cmd");
            crate::zcode::process::no_window(&mut c);
            let ok = c
                .raw_arg(format!(
                    "/C timeout /t 1 /nobreak >nul & start \"\" \"{exe_str}\""
                ))
                .spawn()
                .is_ok();
            if ok {
                app.exit(0);
                return;
            }
        }
    }
    // 兜底：cmd 代理不可用时退回 tauri 自带重启
    app.restart();
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
