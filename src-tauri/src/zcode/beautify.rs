//! ZCode 美化：根据配置生成 zcode-custom.css 并注入解包后的 index.html。
//!
//! 注入策略（最小侵入、最易还原）：在 `out/renderer/index.html` 的 `</head>` 前
//! 插入 `<link rel="stylesheet" href="./assets/zcode-custom.css">`，并把自定义 CSS
//! 写到 `out/renderer/assets/zcode-custom.css`。因该 link 在主样式表之后加载，
//! CSS 源顺序取胜，可覆盖 ZCode 自带样式。
//!
//! 换肤靠覆盖 CSS 变量（ZCode 主样式表用 ~501 个 token）。preset_vars 同时覆盖
//! 背景/表面/卡片/前景/主色/边框/品牌色/强调色 + 面板/侧栏/头部/输入栏（侧栏与
//! 输入栏默认引用 neutral-100/200 等浅色 token，仅覆盖 background/-surface 等
//! 不足以让整页主题一致）；输入栏加 `--color-input`。
//!
//! ZCode 的 React 子组件大量用 `bg-neutral-50/100` 等 Tailwind 直写背景色（不
//! 引用 CSS 变量），CSS 变量无法覆盖字面量。开启毛玻璃 / 背景图时把这些调色板
//! token 按配置透明度混色覆写（混色源 = 主题/自定义背景色，亮/暗分档避免误伤
//! 文字色），让 acrylic + 背景图透出；未开毛玻璃 / 背景图时不输出这些覆写。
//!
//! **运行时 JS 补丁（zcode-custom.js）**：ZCode 主样式表里大量工具类被 Tailwind
//! 编译为字面量色值（如 `.bg-background/95 → #fafafae6`、`.dark:bg-[#484A58]`），
//! CSS 变量覆写与类名枚举都够不着——这是「背景图只在启动屏可见、界面挂载后被
//! 盖住」的根因。注入的 JS 在 React 挂载后用 MutationObserver 持续把计算样式
//! 为不透明的背景改写为配置透明度，并给大面块加 backdrop-filter（真磨砂）。
//! JS 由 CSS 里的 `--zq-alpha` / `--zq-blur` 开关：未开启透明特性时变量缺失，
//! JS 自动空转。
//!
//! 完整性校验已关闭，重打包安全（详见 asar.rs）。
//!
//! 毛玻璃：ZCode 在 Windows 上默认启用 acrylic 窗口材质，但被不透明的表面 token
//! 盖住；这里用 color-mix 混入透明让其透出。
//! 背景图：复制到 assets/zcode-bg.<ext> 后用 html::before 固定图层承载，期望
//! 内容区使用引用 CSS 变量的半透明色时透出图。注意：被写死 tailwind 颜色遮
//! 挡的部分不可穿透（见上）。
//!
//! 原则上同时覆盖 `:root,:host` 与 `.dark` 两个作用域，保证亮/暗模式都生效。
use crate::zcode::asar;
use crate::zcode::process;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

const INDEX_HTML: &str = "out/renderer/index.html";
const CUSTOM_CSS_NAME: &str = "zcode-custom.css";
const CUSTOM_JS_NAME: &str = "zcode-custom.js";
const INJECT_MARK: &str = "zcode-custom.css"; // 用于判断是否已注入

/// 美化配置。持久化到 zcode-assistant app data，不写 ZCode 的 setting.json。
#[derive(Serialize, Deserialize, Clone)]
pub struct BeautifyConfig {
    /// 是否启用（应用美化）。
    #[serde(default)]
    pub enabled: bool,
    /// 预设主题 id（midnight / nord / dracula / gruvbox / tokyo-night / rose-pine / none）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    /// UI 字体族（覆盖 --font-sans），如 "Microsoft YaHei UI"。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui_font: Option<String>,
    /// 等宽字体族（覆盖 --font-mono），如 "Cascadia Code"。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mono_font: Option<String>,
    /// 自定义背景色（覆盖 --color-background），如 "#0d1117"。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bg_color: Option<String>,
    /// 自定义主色调（覆盖 --color-primary），如 "#58a6ff"。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_color: Option<String>,
    /// 毛玻璃：让主要表面半透明，透出 ZCode 在 Windows 上默认启用的 acrylic 材质。
    #[serde(default)]
    pub acrylic: bool,
    /// 表面不透明度（0.2–1.0），毛玻璃或背景图启用时生效；越大越实。
    #[serde(default = "default_surface_opacity")]
    pub surface_opacity: f32,
    /// 背景图源文件（本地绝对路径），应用时复制进 asar 的 assets/zcode-bg.<ext>。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bg_image: Option<String>,
    /// 背景图图层不透明度（0.1–1.0）。
    #[serde(default = "default_bg_image_opacity")]
    pub bg_image_opacity: f32,
}

fn default_surface_opacity() -> f32 {
    0.72
}

fn default_bg_image_opacity() -> f32 {
    1.0
}

impl Default for BeautifyConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            theme: None,
            ui_font: None,
            mono_font: None,
            bg_color: None,
            primary_color: None,
            acrylic: false,
            surface_opacity: default_surface_opacity(),
            bg_image: None,
            bg_image_opacity: default_bg_image_opacity(),
        }
    }
}

// ───────────────────────── 预设主题 ─────────────────────────

/// 返回某预设主题的 (CSS 变量名, 值) 列表。覆盖背景 / 背景alt / 表面 / 卡片 /
/// 前景 / 主色 / 边框 / 品牌色 / 强调色 + 面板 / 侧栏 / 头部 / 输入栏
/// （后四者默认引用 neutral-100/200 等浅色 token，不覆盖会留下浅色侧栏/输入栏）。
fn preset_vars(theme: &str) -> Option<Vec<(&'static str, &'static str)>> {
    // 各 token: 背景 / 背景alt / 表面 / 卡片 / 前景 / 主色 / 边框 / 品牌色 / 强调色
    //          / 面板 / 侧栏 / 头部 / 输入栏
    let v = match theme {
        "midnight" => vec![
            ("--color-background", "#0b1020"),
            ("--color-background-alt", "#121a33"),
            ("--color-surface", "#151d3b"),
            ("--color-card", "#1a2348"),
            ("--color-foreground", "#c9d4f0"),
            ("--color-primary", "#e8ecff"),
            ("--color-border", "#2a3558"),
            ("--color-brand", "#7c8cff"),
            ("--color-accent", "#5b8cff"),
            ("--color-panel", "var(--color-surface)"),
            ("--color-sidebar", "var(--color-surface)"),
            ("--color-header", "var(--color-surface)"),
            ("--color-input", "var(--color-surface)"),
        ],
        "nord" => vec![
            ("--color-background", "#2e3440"),
            ("--color-background-alt", "#3b4252"),
            ("--color-surface", "#3b4252"),
            ("--color-card", "#434c5e"),
            ("--color-foreground", "#d8dee9"),
            ("--color-primary", "#eceff4"),
            ("--color-border", "#434c5e"),
            ("--color-brand", "#88c0d0"),
            ("--color-accent", "#81a1c1"),
            ("--color-panel", "var(--color-surface)"),
            ("--color-sidebar", "var(--color-surface)"),
            ("--color-header", "var(--color-surface)"),
            ("--color-input", "var(--color-surface)"),
        ],
        "dracula" => vec![
            ("--color-background", "#282a36"),
            ("--color-background-alt", "#2e3140"),
            ("--color-surface", "#343746"),
            ("--color-card", "#383b4d"),
            ("--color-foreground", "#f8f8f2"),
            ("--color-primary", "#ffffff"),
            ("--color-border", "#44475a"),
            ("--color-brand", "#bd93f9"),
            ("--color-accent", "#ff79c6"),
            ("--color-panel", "var(--color-surface)"),
            ("--color-sidebar", "var(--color-surface)"),
            ("--color-header", "var(--color-surface)"),
            ("--color-input", "var(--color-surface)"),
        ],
        "gruvbox" => vec![
            ("--color-background", "#282828"),
            ("--color-background-alt", "#32302f"),
            ("--color-surface", "#3c3836"),
            ("--color-card", "#45403d"),
            ("--color-foreground", "#ebdbb2"),
            ("--color-primary", "#fbf1c7"),
            ("--color-border", "#504945"),
            ("--color-brand", "#fabd2f"),
            ("--color-accent", "#fe8019"),
            ("--color-panel", "var(--color-surface)"),
            ("--color-sidebar", "var(--color-surface)"),
            ("--color-header", "var(--color-surface)"),
            ("--color-input", "var(--color-surface)"),
        ],
        "tokyo-night" => vec![
            ("--color-background", "#1a1b26"),
            ("--color-background-alt", "#16161e"),
            ("--color-surface", "#24283b"),
            ("--color-card", "#292e42"),
            ("--color-foreground", "#c0caf5"),
            ("--color-primary", "#c0caf5"),
            ("--color-border", "#414868"),
            ("--color-brand", "#7aa2f7"),
            ("--color-accent", "#bb9af7"),
            ("--color-panel", "var(--color-surface)"),
            ("--color-sidebar", "var(--color-surface)"),
            ("--color-header", "var(--color-surface)"),
            ("--color-input", "var(--color-surface)"),
        ],
        "rose-pine" => vec![
            ("--color-background", "#191724"),
            ("--color-background-alt", "#1f1d2e"),
            ("--color-surface", "#26233a"),
            ("--color-card", "#2a2740"),
            ("--color-foreground", "#e0def4"),
            ("--color-primary", "#e0def4"),
            ("--color-border", "#403d52"),
            ("--color-brand", "#c4a7e7"),
            ("--color-accent", "#ebbcba"),
            ("--color-panel", "var(--color-surface)"),
            ("--color-sidebar", "var(--color-surface)"),
            ("--color-header", "var(--color-surface)"),
            ("--color-input", "var(--color-surface)"),
        ],
        _ => return None,
    };
    Some(v)
}

/// 可选预设 id 列表（供前端渲染选择）。
pub fn preset_list() -> Vec<(&'static str, &'static str)> {
    vec![
        ("midnight", "午夜蓝"),
        ("nord", "Nord 极地"),
        ("dracula", "Dracula 吸血鬼"),
        ("gruvbox", "Gruvbox 复古"),
        ("tokyo-night", "东京夜"),
        ("rose-pine", "Rose Pine 玫瑰松"),
    ]
}

fn font_stack(font: &str) -> String {
    format!(
        "\"{}\", ui-sans-serif, system-ui, -apple-system, \"Segoe UI\", sans-serif",
        font
    )
}

fn mono_stack(font: &str) -> String {
    format!(
        "\"{}\", ui-monospace, SFMono-Regular, Menlo, Consolas, monospace",
        font
    )
}

// ───────────────────────── CSS 生成 ─────────────────────────

/// 允许的背景图扩展名（复制进 asar 后由 CSS 引用）。
pub const BG_IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "webp", "gif"];

/// 背景图在 assets/ 内的文件名（沿用源扩展名）。非法扩展名返回 None。
pub fn bg_image_asset_name(path: &Path) -> Option<String> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    BG_IMAGE_EXTS
        .contains(&ext.as_str())
        .then(|| format!("zcode-bg.{ext}"))
}

/// 校验背景图：文件存在 + 扩展名受支持。成功返回 asar 内资源文件名。
pub fn validate_bg_image(path: &Path) -> Result<String> {
    if !path.exists() {
        anyhow::bail!("背景图不存在：{}", path.display());
    }
    bg_image_asset_name(path).ok_or_else(|| {
        anyhow::anyhow!("不支持的背景图格式（支持 png / jpg / jpeg / webp / gif）")
    })
}

/// 取预设主题的背景色（供毛玻璃混色用）。
fn preset_bg_color(theme: &str) -> Option<&'static str> {
    if theme == "none" {
        return None;
    }
    preset_vars(theme)?
        .iter()
        .find(|(k, _)| *k == "--color-background")
        .map(|(_, v)| *v)
}

/// 生成某一作用域（亮/暗）的表面半透明覆盖声明。
/// 基色优先级：自定义/主题背景色（统一染色全部表面 token）> ZCode 默认色板。
/// 不直接引用自身 token（`--color-background: ... var(--color-background)` 会构成循环），
/// 而是引用底层调色板 token 或已知的自定义色。
/// 覆盖面：background + panel/sidebar/header/input + card/popover/secondary——
/// 后三者（卡片/弹层/次级面）缺了会导致聊天气泡卡片、弹窗、输入栏仍为实心，
/// 把背景图 / acrylic 盖住（真机 styles-*.css 实测这些 token 均被 bg 工具类引用）。
/// 注意：**不能覆盖 `--color-surface`**——它是调色板混色的锚点且本身近透明，
/// 覆盖会构成循环引用导致整链失效。
fn frosted_block(light: bool, base: Option<&str>, alpha_pct: i32) -> String {
    let mix = |c: &str| format!("color-mix(in oklab, {} {}%, transparent)", c, alpha_pct);
    // ZCode 默认：亮 bg=neutral-50、panel/sidebar/header/card=neutral-100；
    //            暗 bg/card/header=neutral-900、sidebar=neutral-950。
    let (bg, side, head) = match (light, base) {
        (_, Some(c)) => (mix(c), mix(c), mix(c)),
        (true, None) => (
            mix("var(--color-neutral-50)"),
            mix("var(--color-neutral-100)"),
            mix("var(--color-neutral-100)"),
        ),
        (false, None) => (
            mix("var(--color-neutral-900)"),
            mix("var(--color-neutral-950)"),
            mix("var(--color-neutral-900)"),
        ),
    };
    format!(
        "  --color-background: {bg};\n  --color-card: {side};\n  --color-popover: {side};\n  --color-secondary: {head};\n  --color-panel: {side};\n  --color-sidebar: {side};\n  --color-header: {head};\n  --color-input: {head};\n"
    )
}

/// 根据配置生成 zcode-custom.css 内容。
pub fn generate_css(cfg: &BeautifyConfig) -> String {
    let mut vars: Vec<(String, String)> = Vec::new();

    if let Some(t) = &cfg.theme {
        if t == "none" {
            // 显式不应用主题
        } else if let Some(preset) = preset_vars(t) {
            for (k, v) in preset {
                vars.push((k.to_string(), v.to_string()));
            }
        }
    }
    if let Some(f) = &cfg.ui_font {
        vars.push(("--font-sans".to_string(), font_stack(f)));
    }
    if let Some(f) = &cfg.mono_font {
        vars.push(("--font-mono".to_string(), mono_stack(f)));
    }
    if let Some(c) = &cfg.bg_color {
        vars.push(("--color-background".to_string(), c.clone()));
    }
    if let Some(c) = &cfg.primary_color {
        vars.push(("--color-primary".to_string(), c.clone()));
    }

    // 毛玻璃或背景图任一启用时，都需要让表面半透明（否则亚克力/背景图被不透明底色盖住）
    let translucent = cfg.acrylic || cfg.bg_image.is_some();
    let bg_asset = cfg
        .bg_image
        .as_deref()
        .map(Path::new)
        .and_then(bg_image_asset_name);

    if vars.is_empty() && !translucent && bg_asset.is_none() {
        return "/* zcode-custom.css：当前无生效配置 */\n".to_string();
    }

    let mut s = String::new();
    s.push_str("/* zcode-custom.css — 由 zcode-assistant 生成，请勿手动编辑。*/\n");
    s.push_str("/* 同时覆盖 :root,:host 与 .dark，保证亮/暗模式都生效。*/\n");

    if !vars.is_empty() {
        s.push_str(":root, :host {\n");
        for (k, v) in &vars {
            s.push_str(&format!("  {}: {};\n", k, v));
        }
        s.push_str("}\n");
        s.push_str(".dark {\n");
        for (k, v) in &vars {
            s.push_str(&format!("  {}: {};\n", k, v));
        }
        s.push_str("}\n");
    }

    if translucent {
        // 毛玻璃：ZCode 在 Windows 上默认启用 acrylic 窗口材质，但被不透明的
        // --color-background 等表面 token 盖住；这里按 surface_opacity 混入透明。
        // 注意：必须排在上方主题/自定义色块之后，才能覆盖它们设置的 --color-background。
        let alpha = (cfg.surface_opacity.clamp(0.2, 1.0) * 100.0).round() as i32;
        let base = cfg.bg_color.as_deref().or_else(|| {
            cfg.theme
                .as_deref()
                .and_then(preset_bg_color)
        });
        s.push_str("/* 毛玻璃：主要表面半透明，透出 Windows acrylic / 背景图。*/\n");
        s.push_str(":root, :host {\n");
        s.push_str(&frosted_block(true, base, alpha));
        s.push_str("}\n");
        s.push_str(".dark {\n");
        s.push_str(&frosted_block(false, base, alpha));
        s.push_str("}\n");

        // 运行时 JS 补丁开关（见 patcher_js）：主样式表里大量工具类被 Tailwind
        // 编译为字面量色值，CSS 层够不着，只能由注入的 zcode-custom.js 在运行时
        // 改写。JS 检测到 --zq-alpha 存在且 <1 才启动；关闭透明特性后变量缺失，
        // JS 自动空转，不碰任何元素。
        s.push_str("/* 运行时补丁开关：zcode-custom.js 据此启动。*/\n");
        s.push_str(":root, :host {\n");
        s.push_str(&format!(
            "  --zq-alpha: {:.2};\n  --zq-blur: 22px;\n}}\n",
            cfg.surface_opacity.clamp(0.2, 1.0)
        ));

        // ZCode 主样式表的 .bg-* 工具类全部引用 var(--color-neutral-*) 等
        // 调色板 token（Tailwind v4 模式），直接覆盖这些 token 为半透明主题色，
        // 即可让所有直写容器背景统一透出 acrylic / 背景图（比逐类名覆写更全面）。
        // **按模式分档覆盖**，避免误伤文字色：亮色模式下浅档（50–300）是背景、
        // 文字用深档（700 等）；暗色模式相反。--color-white 同理只在亮色覆盖
        // （暗色的 text-white 不动）。
        // 混色源：优先主题/自定义背景色（整页统一色调）；否则退回 ZCode 的
        // --color-surface——但真机实测该 token 本身是近透明色（oklab … / 0.03），
        // 再混一次 transparent 结果几乎全透明，surface_opacity 形同虚设，
        // 因此无 base 时才用它兜底。
        let mix_src = base.unwrap_or("var(--color-surface)");
        let mix_sur = format!("color-mix(in oklab, {mix_src} {alpha}%, transparent)");
        s.push_str("/* 调色板 token 半透明覆写（.bg-* 工具类均引用；按亮/暗分档避免误伤文字色）。*/\n");
        s.push_str(":root:not(.dark), :host:not(.dark) {\n");
        for tok in [
            "--color-neutral-50",
            "--color-neutral-100",
            "--color-neutral-200",
            "--color-neutral-300",
            "--color-zinc-50",
            "--color-zinc-100",
            "--color-zinc-200",
            "--color-zinc-300",
            "--color-slate-50",
            "--color-slate-100",
            "--color-slate-200",
            "--color-gray-50",
            "--color-gray-100",
            "--color-gray-200",
            "--color-white",
        ] {
            s.push_str(&format!("  {tok}: {mix_sur};\n"));
        }
        s.push_str("}\n.dark {\n");
        for tok in [
            "--color-neutral-800",
            "--color-neutral-900",
            "--color-neutral-950",
            "--color-zinc-800",
            "--color-zinc-900",
            "--color-zinc-950",
            "--color-slate-800",
            "--color-slate-900",
            "--color-slate-950",
            "--color-gray-800",
            "--color-gray-900",
            "--color-gray-950",
        ] {
            s.push_str(&format!("  {tok}: {mix_sur};\n"));
        }
        s.push_str("}\n");

        // 类名级覆写作双保险（防御个别工具类未走 token 引用的版本差异）
        s.push_str(
            "/* 直写 tailwind 容器背景：覆写为半透明主题色，让 acrylic / 背景图透出。*/\n",
        );
        s.push_str(
            "html, body, #root { background: transparent !important; }\n",
        );
        s.push_str(&format!(
            ".bg-neutral-50, .bg-neutral-100, .bg-neutral-200, .bg-zinc-50, .bg-zinc-100, .bg-zinc-200, .bg-slate-50, .bg-slate-100, .bg-slate-200, .bg-gray-50, .bg-gray-100, .bg-white {{ background-color: {mix_sur} !important; }}\n",
        ));
        s.push_str(&format!(
            ".dark .bg-neutral-900, .dark .bg-neutral-950, .dark .bg-neutral-800, .dark .bg-zinc-900, .dark .bg-zinc-950, .dark .bg-slate-900, .dark .bg-slate-950, .dark .bg-gray-900, .dark .bg-gray-950 {{ background-color: {mix_sur} !important; }}\n",
        ));
    }

    if let Some(name) = &bg_asset {
        // 背景图层：固定在内容之下（z-index:-1），透过上方半透明表面显现。
        // 直写 tailwind 字面量色的容器由运行时 zcode-custom.js 改透明（见上），
        // CSS 层只负责变量引用类的表面。
        let op = cfg.bg_image_opacity.clamp(0.1, 1.0);
        s.push_str("/* 背景图层：固定在内容之下，透过上方半透明表面显现。*/\n");
        s.push_str(&format!(
            "html::before {{ content: \"\"; position: fixed; inset: 0; z-index: -1; background: url(\"./{name}\") center / cover no-repeat; opacity: {op:.2}; pointer-events: none; }}\n"
        ));
        // 确保 html/body 透明不挡住图层（ZCode 已把这两层 bg 设 transparent，但
        // 防御性重写，避免 ZCode 版本变更时出现 opaque html 把图层盖死）
        s.push_str("html, body { background: transparent !important; }\n");
    }
    s
}

/// 一段非常明显的测试 CSS（红色背景），用于真机验证注入链路是否生效。
#[allow(dead_code)]
pub fn test_css() -> &'static str {
    "\
/* === zcode-assistant 注入测试：成功则界面应明显偏红 === */
:root, :host, .dark {
  --color-background: #b71c1c;
  --color-background-alt: #c62828;
  --color-surface: #d32f2f;
  --color-card: #e53935;
  --color-surface-hover: #ef5350;
  --color-primary: #ffebee;
  --color-foreground: #ffebee;
  --color-border: #ef9a9a;
  --color-brand: #ffcdd2;
}
"
}

/// 运行时补丁脚本（注入为 assets/zcode-custom.js）。
///
/// 解决 CSS 层够不着的问题：ZCode 主样式表把大量工具类编译为字面量色值
/// （`.bg-background/95 → #fafafae6`、`.dark:bg-[#484A58]`），CSS 变量覆写与
/// 类名枚举都无法触达。本脚本在 React 挂载前后持续工作：
///
/// 1. **开关**：读取 CSS 变量 `--zq-alpha`，缺失或 ≥1（未开透明特性）立即退出，
///    不碰任何元素——因此脚本可以无条件注入。
/// 2. **透明化**：MutationObserver 监听 DOM 变化（rAF 去抖，扫描期间断开观察
///    避免自触发循环），把计算样式为不透明（α≥0.99）的 backgroundColor 改写为
///    同色 + 目标透明度。`data-zq-bg` 标记已处理元素防重复；React 重渲染只会
///    改 class，内联样式与 dataset 保留，不会被冲掉。
/// 3. **真磨砂**：在已透明化的大面块（≥12% 视口、≥40px、视口内）里挑最大的
///    ≤6 块加 `backdrop-filter: blur()`，跳过已有模糊祖先（避免叠加卡顿）。
/// 4. **排除**：#loading 启动屏（保持启动观感）、pre/code（代码可读性）、
///    svg/img/video/canvas/iframe/picture（媒体元素）。
///
/// 注意保持 ES5 兼容写法（var / function），Electron 旧内核也能跑。
fn patcher_js() -> &'static str {
    r#"/*! zcode-custom.js — 由 zcode-assistant 生成，请勿手动编辑。
 * 运行时表面透明 + backdrop-filter 磨砂补丁。开关：CSS 变量 --zq-alpha（0<x<1 启用）。
 */
(function () {
  "use strict";
  if (window.__ZQ_BEAUTIFY__) return;
  window.__ZQ_BEAUTIFY__ = true;

  var cs0 = getComputedStyle(document.documentElement);
  var ALPHA = parseFloat(cs0.getPropertyValue("--zq-alpha"));
  if (!isFinite(ALPHA) || ALPHA <= 0 || ALPHA >= 1) return; // 未开启透明特性：空转
  var BLUR = parseFloat(cs0.getPropertyValue("--zq-blur"));
  if (!isFinite(BLUR) || BLUR <= 0) BLUR = 22;

  var MAX_BLUR = 6;
  var MIN_BLUR_SIZE = 40;
  var MIN_BLUR_AREA = 0.12; // 占视口面积比例
  var blurred = [];

  function parseColor(s) {
    var m = /^rgba?\(([^)]+)\)$/i.exec(s);
    if (m) {
      var p = m[1].split(/[,\s/]+/).filter(Boolean).map(Number);
      if (p.length >= 3 && p.every(isFinite)) {
        return { r: Math.round(p[0]), g: Math.round(p[1]), b: Math.round(p[2]), a: p.length > 3 ? p[3] : 1 };
      }
      return null;
    }
    m = /^color\(\s*srgb\s+([0-9.]+)\s+([0-9.]+)\s+([0-9.]+)(?:\s*\/\s*([0-9.]+))?\s*\)$/i.exec(s);
    if (m) {
      return { r: Math.round(+m[1] * 255), g: Math.round(+m[2] * 255), b: Math.round(+m[3] * 255), a: m[4] === undefined ? 1 : +m[4] };
    }
    return null;
  }

  function excluded(el) {
    if (el.id === "loading") return true;
    var t = el.tagName;
    if (t === "PRE" || t === "CODE" || t === "SVG" || t === "IMG" || t === "VIDEO" || t === "CANVAS" || t === "IFRAME" || t === "PICTURE") return true;
    if (el.closest && el.closest("pre,code,#loading")) return true;
    return false;
  }

  function patch(el) {
    if (el.dataset && el.dataset.zqBg) return; // 已处理
    if (excluded(el)) return;
    var bg;
    try { bg = getComputedStyle(el).backgroundColor; } catch (e) { return; }
    var c = bg ? parseColor(bg) : null;
    if (!c || c.a < 0.99) return; // 已透明 / 渐变无底色 / 无法解析：跳过
    el.style.backgroundColor = "rgba(" + c.r + "," + c.g + "," + c.b + "," + ALPHA + ")";
    if (el.dataset) el.dataset.zqBg = "1";
  }

  function scan(root) {
    if (!root || root.nodeType !== 1 || !root.querySelectorAll) return;
    patch(root);
    var list = root.querySelectorAll("*");
    for (var i = 0; i < list.length; i++) patch(list[i]);
    blurPass();
  }

  function hasBlurredAncestor(el) {
    for (var p = el.parentElement; p; p = p.parentElement) {
      if (p.dataset && p.dataset.zqBlur) return true;
    }
    return false;
  }

  function blurPass() {
    for (var i = 0; i < blurred.length; i++) {
      var b = blurred[i];
      b.style.backdropFilter = "";
      b.style.webkitBackdropFilter = "";
      if (b.dataset) delete b.dataset.zqBlur;
    }
    blurred = [];
    var vw = window.innerWidth, vh = window.innerHeight;
    if (!vw || !vh) return;
    var marked = document.querySelectorAll('[data-zq-bg="1"]');
    var cand = [];
    for (var i = 0; i < marked.length; i++) {
      var el = marked[i];
      var r = el.getBoundingClientRect();
      if (r.width < MIN_BLUR_SIZE || r.height < MIN_BLUR_SIZE) continue;
      if (r.width * r.height < vw * vh * MIN_BLUR_AREA) continue;
      if (r.bottom <= 0 || r.top >= vh || r.right <= 0 || r.left >= vw) continue;
      cand.push({ el: el, area: r.width * r.height });
    }
    cand.sort(function (a, b) { return b.area - a.area; });
    for (var j = 0; j < cand.length && blurred.length < MAX_BLUR; j++) {
      var el = cand[j].el;
      if (hasBlurredAncestor(el)) continue;
      var v = "blur(" + BLUR + "px)";
      el.style.backdropFilter = v;
      el.style.webkitBackdropFilter = v;
      if (el.dataset) el.dataset.zqBlur = "1";
      blurred.push(el);
    }
  }

  var obs = null;
  var scheduled = false;
  function scheduleScan() {
    if (scheduled) return;
    scheduled = true;
    requestAnimationFrame(function () {
      scheduled = false;
      if (obs) obs.disconnect(); // 扫描期间断开，避免自身样式写入触发死循环
      try { scan(document.body); } finally { if (obs) obs.observe(document.documentElement, OBS_OPTS); }
    });
  }
  var OBS_OPTS = { childList: true, subtree: true, attributes: true, attributeFilter: ["class", "style"] };
  function start() {
    obs = new MutationObserver(scheduleScan);
    scheduleScan();
    obs.observe(document.documentElement, OBS_OPTS);
  }
  if (document.body) start();
  else document.addEventListener("DOMContentLoaded", start, { once: true });
})();
"#
}



// ───────────────────────── 注入 ─────────────────────────

/// 判断解包目录的 index.html 是否已注入自定义 CSS link。
#[allow(dead_code)]
pub fn is_injected(extracted_dir: &Path) -> bool {
    fs::read_to_string(extracted_dir.join(INDEX_HTML))
        .map(|s| s.contains(INJECT_MARK))
        .unwrap_or(false)
}

/// 把 css_content 写入 assets/zcode-custom.css、运行时补丁写入
/// assets/zcode-custom.js，并在 index.html 的 </head> 前注入 link + script。
/// 若提供 bg_image_src，则把图片复制为 assets/zcode-bg.<ext>（先清掉旧背景图）。
/// 幂等：两个标签分别判断是否已存在，只补缺失的那个——因此对「已注入过 CSS
/// 的旧版 asar」也能原地升级补上 script 标签（文件内容每次都重写）。
pub fn inject(extracted_dir: &Path, css_content: &str, bg_image_src: Option<&Path>) -> Result<()> {
    let assets_dir = extracted_dir.join("out/renderer/assets");
    fs::create_dir_all(&assets_dir)?;
    fs::write(assets_dir.join(CUSTOM_CSS_NAME), css_content)?;
    // JS 内容为常量且自带开关（无 --zq-alpha 时空转），无条件写入无害
    fs::write(assets_dir.join(CUSTOM_JS_NAME), patcher_js())?;

    // 清理上一次注入的背景图，避免配置移除后残留
    if let Ok(entries) = fs::read_dir(&assets_dir) {
        for e in entries.flatten() {
            if e.file_name().to_string_lossy().starts_with("zcode-bg.") {
                let _ = fs::remove_file(e.path());
            }
        }
    }
    if let Some(src) = bg_image_src {
        let asset = validate_bg_image(src)?;
        fs::copy(src, assets_dir.join(&asset))
            .with_context(|| format!("复制背景图失败：{}", src.display()))?;
    }

    let css_link = format!(
        "    <link rel=\"stylesheet\" href=\"./assets/{CUSTOM_CSS_NAME}\">\n"
    );
    let js_tag = format!(
        "    <script defer src=\"./assets/{CUSTOM_JS_NAME}\"></script>\n"
    );
    let html_path = extracted_dir.join(INDEX_HTML);
    let html = fs::read_to_string(&html_path)
        .with_context(|| format!("读取 {} 失败", INDEX_HTML))?;
    if html.contains(&css_link) && html.contains(&js_tag) {
        return Ok(()); // 两个标签都在，仅更新了文件内容
    }
    let Some(idx) = html.find("</head>") else {
        return Err(anyhow::anyhow!("index.html 未找到 </head> 注入锚点"));
    };
    let (before, after) = html.split_at(idx);
    let mut new_html = String::with_capacity(html.len() + css_link.len() + js_tag.len());
    new_html.push_str(before);
    if !html.contains(&css_link) {
        new_html.push_str(&css_link);
    }
    if !html.contains(&js_tag) {
        new_html.push_str(&js_tag);
    }
    new_html.push_str(after);
    fs::write(&html_path, new_html)?;
    Ok(())
}

// ───────────────────────── 配置持久化 ─────────────────────────

/// 配置文件路径（zcode-assistant app data / beautify/config.json）。
pub fn config_path() -> Result<std::path::PathBuf> {
    Ok(asar::backup_dir()?.join("config.json"))
}

/// 读取美化配置；不存在则返回默认空配置。
pub fn read_config() -> Result<BeautifyConfig> {
    let p = config_path()?;
    if !p.exists() {
        return Ok(BeautifyConfig::default());
    }
    let txt = fs::read_to_string(&p)?;
    let cfg: BeautifyConfig = serde_json::from_str(&txt).context("美化配置解析失败")?;
    Ok(cfg)
}

/// 写入美化配置。
pub fn write_config(cfg: &BeautifyConfig) -> Result<()> {
    let p = config_path()?;
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent)?;
    }
    let txt = serde_json::to_string_pretty(cfg)?;
    fs::write(&p, format!("{txt}\n"))?;
    Ok(())
}

// ───────────────────────── 模板（命名配置快照）─────────────────────────

/// 美化模板：一份带名称的配置快照。
#[derive(Serialize, Deserialize, Clone)]
pub struct BeautifyTemplate {
    pub name: String,
    pub config: BeautifyConfig,
}

/// 模板文件路径（beautify/templates.json）。
pub fn templates_path() -> Result<std::path::PathBuf> {
    Ok(asar::backup_dir()?.join("templates.json"))
}

/// 读取模板列表；文件不存在或损坏时返回空列表。
pub fn read_templates() -> Vec<BeautifyTemplate> {
    templates_path()
        .ok()
        .filter(|p| p.exists())
        .and_then(|p| fs::read_to_string(&p).ok())
        .and_then(|txt| serde_json::from_str(&txt).ok())
        .unwrap_or_default()
}

/// 写入模板列表。
pub fn write_templates(list: &[BeautifyTemplate]) -> Result<()> {
    let p = templates_path()?;
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent)?;
    }
    let txt = serde_json::to_string_pretty(list)?;
    fs::write(&p, format!("{txt}\n"))?;
    Ok(())
}

// ───────────────────────── 高层 apply / restore ─────────────────────────

/// 完整应用美化：
/// 备份（首次）→ kill ZCode 解锁 → extract → 注入 → pack → 原子替换 app.asar。
/// 不负责重启 ZCode（由命令层 emit restart 事件，前端走全局 RestartDialog）。
pub fn apply(cfg: &BeautifyConfig) -> Result<()> {
    // 0. 背景图预校验（在关闭 ZCode 之前尽早失败）
    if let Some(p) = cfg.bg_image.as_deref() {
        validate_bg_image(Path::new(p))?;
    }

    let asar_path = asar::asar_path()?;
    let resources_dir = asar_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("app.asar 无父目录"))?;

    // 1. 首次备份原始 app.asar
    asar::ensure_backup()?;

    // 2. 关闭 ZCode，释放 app.asar 文件锁
    let _ = process::kill_zcode();

    // 3. 解包到临时目录
    let work = std::env::temp_dir().join("zcode_beautify_apply");
    if work.exists() {
        let _ = fs::remove_dir_all(&work);
    }
    let extracted = work.join("extracted");
    let unpacked = asar::extract(&asar_path, &extracted)?;

    // 4. 注入 CSS + 背景图 + link
    let bg_src = cfg.bg_image.as_deref().map(Path::new);
    inject(&extracted, &generate_css(cfg), bg_src)?;

    // 5. 重打包到同目录的临时文件（同卷，便于原子 rename）
    let new_asar = resources_dir.join("app.asar.new");
    asar::pack(&extracted, &new_asar, &unpacked)?;

    // 6. 原子替换：rename 会覆盖目标（Windows MoveFileEx REPLACE_EXISTING）
    fs::rename(&new_asar, &asar_path)
        .with_context(|| "替换 app.asar 失败（可能 ZCode 仍在运行占用文件）")?;

    // 7. 清理临时目录
    let _ = fs::remove_dir_all(&work);
    Ok(())
}

/// 用测试 CSS 应用到真实 ZCode（仅用于真机验证）。
#[allow(dead_code)]
pub fn apply_test_css() -> Result<()> {
    let asar_path = asar::asar_path()?;
    let resources_dir = asar_path.parent().unwrap();
    asar::ensure_backup()?;
    let _ = process::kill_zcode();
    let work = std::env::temp_dir().join("zcode_beautify_apply");
    if work.exists() {
        let _ = fs::remove_dir_all(&work);
    }
    let extracted = work.join("extracted");
    let unpacked = asar::extract(&asar_path, &extracted)?;
    inject(&extracted, test_css(), None)?;
    let new_asar = resources_dir.join("app.asar.new");
    asar::pack(&extracted, &new_asar, &unpacked)?;
    fs::rename(&new_asar, &asar_path)?;
    let _ = fs::remove_dir_all(&work);
    Ok(())
}

/// 还原：kill ZCode → 用备份覆盖 app.asar。
pub fn restore() -> Result<()> {
    let _ = process::kill_zcode();
    asar::restore_from_backup()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 离线验证 inject() 全链路（不触碰真实 app.asar）：
    /// CSS/JS 写入、背景图复制、旧背景图清理、link + script 注入、幂等与
    /// 「已注入过 CSS 的旧版 asar」升级补 script。
    #[test]
    fn inject_offline_with_bg_image() {
        let work = std::env::temp_dir().join("zcode_beautify_inject_test");
        let _ = fs::remove_dir_all(&work);
        let extracted = work.join("extracted");
        let assets = extracted.join("out/renderer/assets");
        fs::create_dir_all(&assets).unwrap();
        fs::write(
            extracted.join("out/renderer/index.html"),
            "<!doctype html><html><head><title>ZCode</title></head><body><div id=\"root\"></div></body></html>",
        )
        .unwrap();

        // 模拟背景图源文件与上一次注入遗留的旧背景图
        let img_src = work.join("wallpaper.png");
        fs::write(&img_src, b"\x89PNG fake bytes").unwrap();
        fs::write(assets.join("zcode-bg.jpg"), b"old").unwrap();

        inject(&extracted, "/* test css */", Some(img_src.as_path())).unwrap();

        assert_eq!(
            fs::read_to_string(assets.join(CUSTOM_CSS_NAME)).unwrap(),
            "/* test css */"
        );
        assert!(
            assets.join(CUSTOM_JS_NAME).exists(),
            "运行时补丁脚本未写入"
        );
        assert!(assets.join("zcode-bg.png").exists(), "背景图未按源扩展名复制");
        assert!(!assets.join("zcode-bg.jpg").exists(), "旧背景图未清理");
        let html = fs::read_to_string(extracted.join(INDEX_HTML)).unwrap();
        assert!(html.contains(r#"<link rel="stylesheet" href="./assets/zcode-custom.css">"#));
        assert!(
            html.contains(r#"<script defer src="./assets/zcode-custom.js"></script>"#),
            "script 标签未注入"
        );
        assert!(
            html.find("zcode-custom.css").unwrap() < html.find("</head>").unwrap(),
            "link 未注入到 </head> 之前"
        );

        // 幂等 + 移除背景图场景：link/script 均不重复插入，旧背景图被清掉
        inject(&extracted, "/* test css 2 */", None).unwrap();
        let html = fs::read_to_string(extracted.join(INDEX_HTML)).unwrap();
        assert_eq!(html.matches("zcode-custom.css").count(), 1, "link 重复注入");
        assert_eq!(html.matches("zcode-custom.js").count(), 1, "script 重复注入");
        assert!(!assets.join("zcode-bg.png").exists(), "移除背景图后未清理");

        // 升级路径：手工构造「只有 CSS link 的旧版 asar」，inject 应补上 script
        let old_html = html.replace(r#"<script defer src="./assets/zcode-custom.js"></script>"#, "");
        fs::write(extracted.join(INDEX_HTML), old_html).unwrap();
        inject(&extracted, "/* test css 3 */", None).unwrap();
        let html = fs::read_to_string(extracted.join(INDEX_HTML)).unwrap();
        assert_eq!(html.matches("zcode-custom.css").count(), 1);
        assert_eq!(html.matches("zcode-custom.js").count(), 1, "旧版 asar 未补上 script 标签");

        let _ = fs::remove_dir_all(&work);
    }

    /// 真机验证①：注入测试 CSS（红色背景）到真实 ZCode 并启动。
    /// 手动跑：`cargo test --lib apply_test_to_real -- --ignored --nocapture`
    /// 跑完后请人工确认 ZCode 界面变红，然后跑 restore_real 还原。
    #[test]
    #[ignore]
    fn apply_test_to_real() {
        println!("应用测试 CSS（红色背景）到真实 ZCode...");
        apply_test_css().expect("应用测试 CSS 失败");
        println!("✅ 已注入测试 CSS。启动 ZCode 以确认界面变红...");
        // 启动 ZCode 供人工确认
        let _ = process::launch_zcode();
        println!("ZCode 已启动。请观察界面是否明显偏红。");
        println!("确认后运行 restore_real 测试还原：cargo test --lib restore_real -- --ignored --nocapture");
    }

    /// 真机验证②：还原真实 ZCode 到官方 app.asar 并启动。
    #[test]
    #[ignore]
    fn restore_real() {
        println!("还原真实 ZCode 到官方 app.asar...");
        restore().expect("还原失败");
        println!("✅ 已还原。启动 ZCode 确认恢复官方外观...");
        let _ = process::launch_zcode();
        println!("ZCode 已启动。请确认界面恢复原状。");
    }

    /// 生成测试：打印各预设与自定义组合的 CSS（不改动任何文件）。
    #[test]
    fn preview_css() {
        for (id, name) in preset_list() {
            let cfg = BeautifyConfig {
                enabled: true,
                theme: Some(id.to_string()),
                ..Default::default()
            };
            let css = generate_css(&cfg);
            println!("===== {} ({}) =====\n{}", name, id, css);
        }
        // 毛玻璃（无主题，走 ZCode 默认色板）
        let cfg = BeautifyConfig {
            enabled: true,
            acrylic: true,
            ..Default::default()
        };
        println!("===== 毛玻璃（默认色板）=====\n{}", generate_css(&cfg));
        // 毛玻璃 + 主题 + 背景图组合
        let cfg = BeautifyConfig {
            enabled: true,
            theme: Some("tokyo-night".to_string()),
            acrylic: true,
            surface_opacity: 0.65,
            bg_image: Some("C:/Pictures/demo.jpg".to_string()),
            bg_image_opacity: 0.8,
            ..Default::default()
        };
        println!("===== 毛玻璃+主题+背景图 =====\n{}", generate_css(&cfg));
    }

    /// 背景图资源名与校验逻辑。
    #[test]
    fn bg_image_asset_name_rules() {
        assert_eq!(
            bg_image_asset_name(Path::new("C:/a/photo.JPG")).as_deref(),
            Some("zcode-bg.jpg")
        );
        assert_eq!(bg_image_asset_name(Path::new("C:/a/photo.bmp")), None);
        assert!(validate_bg_image(Path::new("C:/no-such-file.png")).is_err());
    }

    /// 只读校验：真实 app.asar 内 index.html 存在 `</head>` 注入锚点。
    /// 若 ZCode 未安装则跳过（不视为失败）。防止未来 ZCode 版本变更 index.html
    /// 结构导致注入锚点失效而未被察觉。
    #[test]
    fn real_index_html_has_inject_anchor() {
        let Ok(asar) = asar::asar_path() else {
            println!("未找到 ZCode 安装，跳过锚点校验");
            return;
        };
        if !asar.exists() {
            println!("app.asar 不存在，跳过锚点校验: {}", asar.display());
            return;
        }
        let bytes = asar::read_file(&asar, INDEX_HTML).expect("读取 index.html 失败");
        let html = String::from_utf8_lossy(&bytes);
        assert!(
            html.contains("</head>"),
            "index.html 未找到 </head> 注入锚点，注入策略需要调整"
        );
        assert!(
            html.contains("styles-") && html.contains(".css"),
            "index.html 未见主样式表引用，结构可能已变化"
        );
        println!("✓ index.html 含 </head> 锚点（{} 字节）", bytes.len());
    }
}
