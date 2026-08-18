//! 应用全局状态：DB 连接 + 共享 HTTP 客户端 + 数据目录 + 健康检测缓存
use crate::{db::Database, http, types::ProxyConfig};
use std::path::PathBuf;
use std::sync::Mutex;

pub struct AppState {
    pub db: Database,
    pub http: Mutex<reqwest::Client>,
    pub data_dir: PathBuf,
    /// 当前供应商可用性检测的最新报告（冷却判断 + 状态展示共用）
    pub health: Mutex<Option<crate::commands::health_cmd::HealthReport>>,
}

impl AppState {
    /// 获取当前共享 HTTP 客户端（clone，已注入代理）
    pub fn client(&self) -> reqwest::Client {
        self.http.lock().map(|c| c.clone()).unwrap_or_default()
    }

    /// 代理变更后重建客户端
    pub fn rebuild_http(&self, proxy: Option<&ProxyConfig>, password: Option<&str>) {
        if let Ok(c) = http::build_client(proxy, password) {
            if let Ok(mut h) = self.http.lock() {
                *h = c;
            }
        }
    }
}
