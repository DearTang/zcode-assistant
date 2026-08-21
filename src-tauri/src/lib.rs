use std::sync::Mutex;
use tauri::{Emitter, Manager, WindowEvent};

mod accounts;
mod autoswitch;
mod coding_plan;
mod commands;
mod db;
mod float_ball;
mod http;
mod openrouter;
mod provider_resolve;
mod sessions;
mod state;
mod tray;
mod types;
mod updater;
mod usage;
mod version;
mod zcode;

pub fn run() {
    tauri::Builder::default()
        // 单实例：必须在所有插件之前注册。第二个实例启动时自动退出，
        // 已有实例收到回调：唤出主窗口并通知前端弹窗，由用户选择
        // 「覆盖启动」（restart_app 重启进程）或「退出」（保持现有实例）。
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
            let _ = app.emit("app://second-instance", ());
        }))
        // 开机自启动：注册 / 注销系统自启项（Windows 注册表 / macOS LaunchAgent / Linux .desktop）
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
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
                health: Mutex::new(None),
            });

            // 主窗口显示
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
            tray::setup(app)?;

            // 悬浮球延迟创建（遵循偏好：设置里隐藏过则启动时不弹，需要时再按需创建）
            let handle = app.handle().clone();
            tauri::async_runtime::spawn_blocking(move || {
                std::thread::sleep(std::time::Duration::from_millis(500));
                let visible = handle
                    .try_state::<state::AppState>()
                    .and_then(|s| s.db.kv_get(commands::prefs_cmd::KV_FLOAT_BALL))
                    .map(|v| v != "0")
                    .unwrap_or(true);
                if visible {
                    let _ = float_ball::ensure_float_ball(&handle);
                }
            });

            // 自动切换调度 + 应用启动触发（appstart 规则）
            autoswitch::spawn_scheduler(app.handle().clone());
            autoswitch::spawn_startup_trigger(app.handle().clone());

            // 启动时后台扫描 transcript，补全供应商 UUID→名称别名（增量，不阻塞启动）
            // 别名独立于 zcode 配置，删除渠道不影响已保存的映射
            let handle = app.handle().clone();
            tauri::async_runtime::spawn_blocking(move || {
                let state = handle.state::<state::AppState>();
                let _ = provider_resolve::scan_transcripts(&state.db);
            });

            // OpenRouter 模型目录：每日启动拉取一次真实上下文（今天已拉过则跳过；
            // 失败沿用上一次目录），供「拉取可用模型」模糊匹配填充 context
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = openrouter::daily_fetch(&handle).await {
                    log::warn!("OpenRouter 模型目录拉取失败（沿用上一次）：{e}");
                }
            });

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
            commands::window::show_float_panel,
            commands::window::hide_float_panel,
            commands::window::toggle_float_panel,
            commands::window::quit_app,
            commands::window::restart_app,
            commands::window::set_tray_icon,
            commands::window::fe_log,
            // 应用偏好（悬浮球显隐 / 用量展示方案 / 切换后提示重启 / 开机自启）
            commands::prefs_cmd::get_prefs,
            commands::prefs_cmd::set_float_ball_visible,
            commands::prefs_cmd::set_usage_display,
            commands::prefs_cmd::set_switch_restart_zcode,
            commands::prefs_cmd::set_autostart,
            // zcode 配置 / 探测 / 重启 / 切换
            commands::zcode_cmd::get_zcode_config,
            commands::zcode_cmd::get_zcode_setting,
            commands::zcode_cmd::probe_zcode,
            commands::zcode_cmd::restart_zcode,
            commands::zcode_cmd::reload_zcode_window,
            commands::zcode_cmd::switch_zcode_model,
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
            commands::models_cmd::reorder_providers,
            commands::models_cmd::reorder_models,
            commands::models_cmd::set_model_enabled,
            commands::models_cmd::remove_model,
            commands::models_cmd::get_provider_api_key,
            commands::models_cmd::set_provider_primary,
            commands::models_cmd::get_primary_provider,
            commands::models_cmd::bootstrap_primary,
            commands::models_cmd::test_provider_connection,
            // 导入配置
            commands::import_cmd::preview_providers_from,
            commands::import_cmd::import_providers_from,
            commands::import_cmd::resolve_import_contexts,
            commands::import_cmd::pick_config_file,
            // 反向同步（导出到 opencode / cc-switch）
            commands::export_cmd::export_preview,
            commands::export_cmd::export_providers_to,
            // 当前模型可用性检测（check_provider_health：模型卡片 ⚡ 手动检测指定供应商）
            commands::health_cmd::check_current_health,
            commands::health_cmd::check_provider_health,
            // 配额
            commands::quota_cmd::get_coding_plan_quota,
            commands::quota_cmd::get_template_quota,
            commands::quota_cmd::get_provider_quota,
            commands::quota_cmd::get_overview_quota,
            // 配额查询 Token 获取（弹登录窗，keyring 存储）
            commands::token_cmd::start_quota_token_login,
            commands::token_cmd::quota_token_status,
            commands::token_cmd::get_quota_token_value,
            commands::token_cmd::clear_quota_token,
            commands::token_cmd::set_quota_token,
            commands::token_cmd::set_quota_login_password,
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
            commands::autoswitch_cmd::reorder_rules,
            commands::autoswitch_cmd::autoswitch_projects,
            commands::autoswitch_cmd::test_rule,
            commands::autoswitch_cmd::autoswitch_logs,
            // 配额模板
            commands::templates_cmd::list_templates,
            commands::templates_cmd::builtin_quota_templates,
            commands::templates_cmd::get_quota_template,
            commands::templates_cmd::upsert_template,
            commands::templates_cmd::remove_template,
            // 用量查询
            commands::usage_cmd::usage_sync,
            commands::usage_cmd::usage_filters,
            commands::usage_cmd::usage_overview,
            commands::usage_cmd::usage_aggregate,
            commands::usage_cmd::usage_records,
            commands::usage_cmd::usage_provider_labels,
            // 项目 / 会话管理
            commands::sessions_cmd::zc_projects,
            commands::sessions_cmd::zc_sessions,
            commands::sessions_cmd::zc_rename_session,
            commands::sessions_cmd::zc_restore_session,
            commands::sessions_cmd::zc_archive_session,
            commands::sessions_cmd::zc_archive_project,
            commands::sessions_cmd::zc_restore_project,
            commands::sessions_cmd::zc_delete,
            // ZCode 美化
            commands::beautify_cmd::get_beautify_status,
            commands::beautify_cmd::get_beautify_presets,
            commands::beautify_cmd::pick_beautify_image,
            commands::beautify_cmd::read_beautify_image_preview,
            commands::beautify_cmd::apply_beautify,
            commands::beautify_cmd::restore_beautify,
            commands::beautify_cmd::list_beautify_templates,
            commands::beautify_cmd::save_beautify_template,
            commands::beautify_cmd::delete_beautify_template,
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
