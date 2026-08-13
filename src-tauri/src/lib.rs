use std::sync::Mutex;
use tauri::{Manager, WindowEvent};

mod accounts;
mod autoswitch;
mod commands;
mod db;
mod float_ball;
mod http;
mod state;
mod tray;
mod types;
mod updater;
mod usage;
mod version;
mod zcode;

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // 数据目录 + SQLite + 共享 HTTP 客户端（注入代理）
            let data_dir = app.path().app_data_dir()?;
            let db = db::Database::open(data_dir.join("zcode-assistant.db"))?;
            let proxy_cfg = db
                .kv_get("proxy")
                .and_then(|s| serde_json::from_str::<types::ProxyConfig>(&s).ok());
            let pw = keyring::Entry::new("zcode-assistant", "proxy-password")
                .ok()
                .and_then(|e| e.get_password().ok());
            let client = http::build_client(proxy_cfg.as_ref(), pw.as_deref()).unwrap_or_default();
            app.manage(state::AppState {
                db,
                http: Mutex::new(client),
                data_dir,
            });

            // 主窗口显示
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
            tray::setup(app)?;

            // 悬浮球延迟创建
            let handle = app.handle().clone();
            tauri::async_runtime::spawn_blocking(move || {
                std::thread::sleep(std::time::Duration::from_millis(500));
                let _ = float_ball::ensure_float_ball(&handle);
            });

            // 自动切换调度
            autoswitch::spawn_scheduler(app.handle().clone());

            Ok(())
        })
        .on_window_event(|window, event| {
            // 主窗口关闭 → 最小化到托盘
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    let _ = window.hide();
                    api.prevent_close();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            // 窗口控制
            commands::window::show_main_window,
            commands::window::show_float_ball,
            commands::window::hide_float_ball,
            commands::window::toggle_float_ball,
            commands::window::show_float_panel,
            commands::window::hide_float_panel,
            commands::window::toggle_float_panel,
            commands::window::quit_app,
            commands::window::set_tray_icon,
            commands::window::fe_log,
            // zcode 配置 / 探测 / 重启 / 切换
            commands::zcode_cmd::get_zcode_config,
            commands::zcode_cmd::get_zcode_setting,
            commands::zcode_cmd::probe_zcode,
            commands::zcode_cmd::restart_zcode,
            commands::zcode_cmd::select_provider,
            // 模型管理
            commands::models_cmd::fetch_available_models,
            commands::models_cmd::builtin_model_specs,
            commands::models_cmd::add_provider,
            commands::models_cmd::remove_provider,
            commands::models_cmd::update_provider,
            commands::models_cmd::update_model_limit,
            commands::models_cmd::apply_models,
            commands::models_cmd::set_provider_enabled,
            commands::models_cmd::set_model_enabled,
            commands::models_cmd::remove_model,
            commands::models_cmd::get_provider_api_key,
            commands::models_cmd::set_provider_coding_plan,
            commands::models_cmd::list_coding_plan_providers,
            commands::models_cmd::test_provider_connection,
            // 导入配置
            commands::import_cmd::import_providers_from,
            commands::import_cmd::pick_config_file,
            // 配额
            commands::quota_cmd::get_coding_plan_quota,
            commands::quota_cmd::get_template_quota,
            commands::quota_cmd::get_provider_quota,
            // 账号
            commands::accounts_cmd::list_accounts,
            commands::accounts_cmd::capture_account,
            commands::accounts_cmd::switch_account,
            commands::accounts_cmd::remove_account,
            commands::accounts_cmd::rename_account,
            commands::accounts_cmd::current_account,
            // 代理
            commands::proxy_cmd::get_proxy,
            commands::proxy_cmd::set_proxy,
            commands::proxy_cmd::test_proxy,
            // 自动切换
            commands::autoswitch_cmd::list_rules,
            commands::autoswitch_cmd::upsert_rule,
            commands::autoswitch_cmd::delete_rule,
            // 配额模板
            commands::templates_cmd::list_templates,
            commands::templates_cmd::get_quota_template,
            commands::templates_cmd::upsert_template,
            commands::templates_cmd::remove_template,
            // 用量查询
            commands::usage_cmd::usage_sync,
            commands::usage_cmd::usage_filters,
            commands::usage_cmd::usage_overview,
            commands::usage_cmd::usage_aggregate,
            commands::usage_cmd::usage_records,
            // 版本号
            commands::version_cmd::get_app_version,
            // 自动更新
            commands::updater_cmd::check_for_updates,
            commands::updater_cmd::download_update,
            commands::updater_cmd::install_update,
            commands::updater_cmd::open_release_page,
        ])
        .run(tauri::generate_context!())
        .expect("error while running zcode-assistant");
}
