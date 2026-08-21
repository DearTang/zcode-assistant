//! 配额查询模板 CRUD + 内置预设
use crate::state::AppState;
use crate::types::QuotaTemplate;
use tauri::State;

/// 内置预设模板，分两组：
///   1) Token Plan 额度（providerKey 前缀 preset:cp-）：Kimi/智谱个人/智谱团队/智谱国际
///      /MiniMax/MiniMax 国际/ZenMux/火山方舟，端点取自 cc-switch services/coding_plan.rs
///      实测实现。这些供应商由后端按 baseURL 自动识别并自动使用其 API Key + Base URL 查询，
///      模板仅作「使用模板」下拉预设展示（智谱团队版的组织/项目 ID、火山的 AK/SK 在模板
///      extra_json 中填写）；
///   2) 余额查询：DeepSeek/StepFun/SiliconFlow/OpenRouter，端点与提取路径取自 cc-switch
///      services/balance.rs，全部 Bearer apiKey + GET + dot path 提取，与模板引擎
///      （run_template_quota）直接兼容，选中后一键复制内容即可保存使用。
/// 注：Novita AI 余额原始单位为 0.0001 USD（需换算），模板引擎不支持除法，故未收录。
pub fn builtin_templates() -> Vec<QuotaTemplate> {
    // Token Plan 预设（自动查询标记，url 仅作展示参考）
    let cp = |key: &str, name: &str, url: &str| QuotaTemplate {
        provider_key: format!("preset:cp-{key}"),
        name: Some(name.to_string()),
        method: Some("GET".to_string()),
        url: Some(url.to_string()),
        auth_mode: Some("coding_plan".to_string()),
        ..Default::default()
    };
    // 余额查询预设（模板引擎可直接执行）
    let t = |key: &str,
             name: &str,
             url: &str,
             remaining: Option<&str>,
             total: Option<&str>,
             used: Option<&str>| QuotaTemplate {
        provider_key: format!("preset:{key}"),
        name: Some(name.to_string()),
        method: Some("GET".to_string()),
        url: Some(url.to_string()),
        total_path: total.map(String::from),
        used_path: used.map(String::from),
        remaining_path: remaining.map(String::from),
        auth_mode: Some("appkey".to_string()),
        ..Default::default()
    };
    vec![
        // ===== Token Plan 额度（自动，对齐 cc-switch coding_plan.rs）=====
        cp(
            "kimi",
            "Kimi For Coding",
            "https://api.kimi.com/coding/v1/usages",
        ),
        cp(
            "zhipu",
            "Zhipu GLM(智谱)",
            "https://open.bigmodel.cn/api/monitor/usage/quota/limit",
        ),
        cp(
            "zhipu-team",
            "Zhipu GLM Team (智谱团队)",
            "https://open.bigmodel.cn/api/monitor/usage/quota/limit?type=2",
        ),
        cp(
            "zhipu-en",
            "Zhipu GLM (智谱国际 z.ai)",
            "https://api.z.ai/api/monitor/usage/quota/limit",
        ),
        cp(
            "minimax",
            "MiniMax",
            "https://api.minimaxi.com/v1/api/openplatform/coding_plan/remains",
        ),
        cp(
            "minimax-en",
            "MiniMax 国际 (minimax.io)",
            "https://api.minimax.io/v1/api/openplatform/coding_plan/remains",
        ),
        cp("zenmux", "ZenMux", "{{baseURL}}"),
        cp(
            "volcengine",
            "火山方舟(Volcengine)",
            "https://open.volcengineapi.com/?Action=GetCodingPlanUsage",
        ),
        // ===== 余额查询（对齐 cc-switch balance.rs）=====
        // DeepSeek：{balance_infos:[{total_balance(字符串数字),...}]}，多币种取第一条
        t(
            "deepseek",
            "DeepSeek 余额",
            "https://api.deepseek.com/user/balance",
            Some("balance_infos.0.total_balance"),
            None,
            None,
        ),
        // StepFun：{balance, ...}
        t(
            "stepfun",
            "StepFun 余额",
            "https://api.stepfun.com/v1/accounts",
            Some("balance"),
            None,
            None,
        ),
        // SiliconFlow 国内：{code, data:{totalBalance,...}}
        t(
            "siliconflow-cn",
            "SiliconFlow 余额（国内）",
            "https://api.siliconflow.cn/v1/user/info",
            Some("data.totalBalance"),
            None,
            None,
        ),
        // SiliconFlow 国际版（USD）
        t(
            "siliconflow-en",
            "SiliconFlow 余额（国际）",
            "https://api.siliconflow.com/v1/user/info",
            Some("data.totalBalance"),
            None,
            None,
        ),
        // OpenRouter：{data:{total_credits, total_usage}}；remaining 留空 = total-used
        t(
            "openrouter",
            "OpenRouter 余额",
            "https://openrouter.ai/api/v1/credits",
            None,
            Some("data.total_credits"),
            Some("data.total_usage"),
        ),
        // Qwen 通义千问 Token Plan：Cookie 认证 + POST 百分比 API（仅每周窗口）
        QuotaTemplate {
            provider_key: "preset:qwen".to_string(),
            name: Some("Qwen 通义千问 Token Plan".to_string()),
            method: Some("POST".to_string()),
            url: Some("https://cs-data.qianwenai.com/data/api.json?product=sfm_bailian&action=BroadScopeAspnGateway&api=zeldaHttp.apikeyMgr.%252Ftokenplan%252Fpersonal%252Fapi%252Fv2%252Fusage&region=cn-beijing&params=%7B%22Api%22%3A%22zeldaHttp.apikeyMgr.%2Ftokenplan%2Fpersonal%2Fapi%2Fv2%2Fusage%22%2C%22Data%22%3A%7B%22cornerstoneParam%22%3A%7B%22domain%22%3A%22platform.qianwenai.com%22%2C%22consoleSite%22%3A%22QIANWENAI%22%2C%22console%22%3A%22ONE_CONSOLE%22%2C%22xsp_lang%22%3A%22zh-CN%22%2C%22protocol%22%3A%22V2%22%2C%22productCode%22%3A%22p_efm%22%7D%7D%2C%22V%22%3A%221.0%22%7D".to_string()),
            headers_json: Some(r#"{"Cookie":"{{token}}","Content-Type":"application/x-www-form-urlencoded","Origin":"https://platform.qianwenai.com","Referer":"https://platform.qianwenai.com/home/billing/subscription/token-plan-individual"}"#.to_string()),
            weekly_remaining_path: Some("data.DataV2.data.data.per1WeekPercentage".to_string()),
            weekly_reset_time_path: Some("data.DataV2.data.data.per1WeekResetTime".to_string()),
            login_url: Some("https://platform.qianwenai.com".to_string()),
            token_source: Some("cookie:".to_string()),
            auth_mode: Some("token".to_string()),
            unit: Some("%".to_string()),
            ..Default::default()
        },
    ]
}

/// 内置预设配额模板（供「使用模板」下拉内置分组一键复制）
#[tauri::command]
pub fn builtin_quota_templates() -> Vec<QuotaTemplate> {
    builtin_templates()
}

#[tauri::command]
pub fn list_templates(state: State<'_, AppState>) -> Result<Vec<QuotaTemplate>, String> {
    state.db.list_templates().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_quota_template(
    state: State<'_, AppState>,
    provider_key: String,
) -> Result<Option<QuotaTemplate>, String> {
    state
        .db
        .get_template(&provider_key)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn upsert_template(
    state: State<'_, AppState>,
    template: QuotaTemplate,
) -> Result<(), String> {
    state
        .db
        .upsert_template(&template)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn remove_template(state: State<'_, AppState>, provider_key: String) -> Result<(), String> {
    state
        .db
        .delete_template(&provider_key)
        .map_err(|e| e.to_string())
}
