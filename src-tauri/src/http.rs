//! 全局 HTTP 客户端构建（注入代理，供配额查询/模型列表/模板查询复用）
use crate::types::ProxyConfig;
use anyhow::Result;
use std::time::Duration;

pub fn build_client(
    proxy: Option<&ProxyConfig>,
    password: Option<&str>,
) -> Result<reqwest::Client> {
    let mut b = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent(format!("zcode-assistant/{}", crate::version::APP_VERSION));
    let mut has_app_proxy = false;
    if let Some(p) = proxy {
        if p.enabled && p.ptype != "none" && !p.host.is_empty() && p.port > 0 {
            let scheme = if p.ptype == "socks5" {
                "socks5h"
            } else {
                "http"
            };
            let url = format!("{}://{}:{}", scheme, p.host, p.port);
            let mut proxy = reqwest::Proxy::all(&url)?;
            if let Some(u) = p.username.as_ref().filter(|s| !s.is_empty()) {
                proxy = proxy.basic_auth(u, password.unwrap_or(""));
            }
            b = b.proxy(proxy);
            has_app_proxy = true;
        }
    }
    // 未配置应用代理时，禁用系统环境变量代理（https_proxy/http_proxy 等）。
    // 否则 reqwest 会默认走系统代理，导致 bigmodel.cn 等国内接口在代理不通时请求失败。
    if !has_app_proxy {
        b = b.no_proxy();
    }
    Ok(b.build()?)
}
