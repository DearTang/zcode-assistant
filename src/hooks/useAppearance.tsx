import {
  createContext,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import { beautify } from "../api";

/**
 * 外观定制（仅主窗口；悬浮球 / 悬浮面板小窗不套用）：
 * 参考「ZCode 美化」的思路给 zcode-assistant 自己做换肤——
 * - 主题色：改写强调色系 token（--accent 家族 + 光晕 orb-1），保留原有 L/C 只换色相；
 * - 透明度：把表面 / 玻璃面板 token 按 color-mix 透明化，让光晕与背景图透出；
 * - 背景图：fixed 图层铺在窗口底层（光晕之上、内容之下），随不透明度透出；
 *   生效时侧边栏 / 顶栏同步降为轻纱玻璃（低罩色 + 轻模糊），整窗连续透出背景。
 * 全部通过运行时注入 <style>（#za-appearance-style）实现，追加在 tokens.css 之后、
 * 同名 [data-theme] 选择器靠源顺序覆盖；还原默认 = 清空配置，注入样式即恢复原样。
 */
export interface Appearance {
  /** 强调色色相（OKLCH hue 0–360）；null = 默认青绿（不覆盖任何 token） */
  accentHue: number | null;
  /** 玻璃面板与表面不透明度 0.4–1；1 = 不覆盖（默认实底） */
  surfaceOpacity: number;
  /** 背景图本地绝对路径；null = 无背景图 */
  bgImage: string | null;
  /** 背景图不透明度 0.1–1 */
  bgOpacity: number;
}

const DEFAULTS: Appearance = {
  accentHue: null,
  surfaceOpacity: 1,
  bgImage: null,
  bgOpacity: 1,
};
const STORAGE_KEY = "za-appearance";

interface AppearanceCtx {
  appearance: Appearance;
  /** 增量更新（与 DEFAULTS 合并校验），即时生效 */
  setAppearance: (patch: Partial<Appearance>) => void;
  /** 还原默认主题（清空全部定制并移除注入样式 / 背景层） */
  reset: () => void;
  /** 背景图 data URL（异步加载；>8MB 或读取失败为 null） */
  bgDataUrl: string | null;
}

const Ctx = createContext<AppearanceCtx | null>(null);

const round3 = (n: number) => Math.round(n * 1000) / 1000;

/** 生成覆盖 tokens.css 的注入样式；空配置返回空串（还原默认） */
function buildCss(ap: Appearance): string {
  const parts: string[] = [];
  const h = ap.accentHue;
  if (h != null) {
    // 强调色系：沿用 tokens.css 的 L/C 数值，只替换色相；orb-1 跟随主题色
    parts.push(
      `[data-theme="dark"]{
  --accent: oklch(0.72 0.15 ${h});
  --accent-hover: oklch(0.77 0.14 ${h});
  --accent-pressed: oklch(0.67 0.16 ${h});
  --accent-fg: oklch(0.16 0.02 ${h});
  --accent-subtle: oklch(0.72 0.15 ${h} / 0.12);
  --glow-accent: 0 0 40px oklch(0.72 0.15 ${h} / 0.15);
  --orb-1: oklch(0.72 0.15 ${h} / 0.4);
}`,
      `[data-theme="light"]{
  --accent: oklch(0.6 0.13 ${h});
  --accent-hover: oklch(0.55 0.14 ${h});
  --accent-pressed: oklch(0.5 0.15 ${h});
  --accent-fg: oklch(0.98 0.004 ${h});
  --accent-subtle: oklch(0.6 0.13 ${h} / 0.1);
  --glow-accent: 0 0 40px oklch(0.6 0.13 ${h} / 0.18);
  --orb-1: oklch(0.72 0.15 ${h} / 0.22);
}`
    );
  }
  if (ap.surfaceOpacity < 1) {
    // 表面 / 玻璃透明化（--bg-base 窗口底色保持不透明，否则整体不可读）；
    // 混色比例 = 原不透明度 × 用户系数，浅/深主题各自的原值均来自 tokens.css
    const k = ap.surfaceOpacity;
    const mix = (c: string) =>
      `color-mix(in oklab, ${c} ${Math.round(k * 100)}%, transparent)`;
    parts.push(
      `[data-theme="dark"]{
  --bg-surface: ${mix("oklch(0.19 0.014 250)")};
  --bg-elevated: ${mix("oklch(0.22 0.016 250)")};
  --bg-overlay: ${mix("oklch(0.26 0.018 250)")};
  --glass-bg: oklch(0.17 0.014 250 / ${round3(0.48 * k)});
  --glass-bg-strong: oklch(0.17 0.014 250 / ${round3(Math.min(1, 0.7 * k))});
}`,
      `[data-theme="light"]{
  --bg-surface: ${mix("oklch(0.96 0.006 250)")};
  --bg-elevated: ${mix("oklch(1 0 0)")};
  --bg-overlay: ${mix("oklch(1 0 0)")};
  --glass-bg: oklch(1 0 0 / ${round3(0.6 * k)});
  --glass-bg-strong: oklch(1 0 0 / ${round3(Math.min(1, 0.85 * k))});
}`
    );
  }
  if (ap.bgImage != null) {
    // 背景图生效：侧边栏 / 顶栏从厚玻璃降为轻纱（低罩色 + 轻模糊），背景图
    // 连续透过整窗（内容区本就透明），避免两侧 chrome 看起来没被背景覆盖；
    // 漂移光晕同步压低，避免彩色光斑糊在照片上
    parts.push(
      `.za-sidebar, .za-topbar {
  backdrop-filter: blur(12px) saturate(150%);
  -webkit-backdrop-filter: blur(12px) saturate(150%);
}`,
      `[data-theme="dark"] .za-sidebar,
[data-theme="dark"] .za-topbar {
  background: oklch(0.16 0.012 250 / 0.3);
}`,
      `[data-theme="light"] .za-sidebar,
[data-theme="light"] .za-topbar {
  background: oklch(1 0 0 / 0.38);
}`,
      `body::before, body::after, #root::before {
  opacity: 0.45;
}`
    );
  }
  return parts.join("\n");
}

function load(): Appearance {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return DEFAULTS;
    const v = JSON.parse(raw) as Partial<Appearance>;
    return {
      accentHue:
        typeof v.accentHue === "number" && v.accentHue >= 0 && v.accentHue <= 360
          ? v.accentHue
          : null,
      surfaceOpacity:
        typeof v.surfaceOpacity === "number"
          ? Math.min(1, Math.max(0.4, v.surfaceOpacity))
          : 1,
      bgImage: typeof v.bgImage === "string" && v.bgImage ? v.bgImage : null,
      bgOpacity:
        typeof v.bgOpacity === "number"
          ? Math.min(1, Math.max(0.1, v.bgOpacity))
          : 1,
    };
  } catch {
    return DEFAULTS;
  }
}

export function AppearanceProvider({ children }: { children: ReactNode }) {
  const [appearance, setAppearanceState] = useState<Appearance>(load);
  const [bgDataUrl, setBgDataUrl] = useState<string | null>(null);

  // 1) 注入 / 更新覆盖样式（追加在 tokens.css 之后，同选择器靠源顺序覆盖）
  useEffect(() => {
    let el = document.getElementById("za-appearance-style") as HTMLStyleElement | null;
    if (!el) {
      el = document.createElement("style");
      el.id = "za-appearance-style";
      document.head.appendChild(el);
    }
    el.textContent = buildCss(appearance);
    localStorage.setItem(STORAGE_KEY, JSON.stringify(appearance));
  }, [appearance]);

  // 2) 背景图 data URL（复用美化页的图片读取命令，>8MB / 格式不支持返回 null）
  useEffect(() => {
    let cancelled = false;
    setBgDataUrl(null);
    if (!appearance.bgImage) return;
    beautify
      .readImagePreview(appearance.bgImage)
      .then((d) => {
        if (!cancelled) setBgDataUrl(d);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [appearance.bgImage]);

  // 3) 背景图层：fixed 铺满窗口底层（光晕之上、内容 #root 之下），随不透明度透出
  useEffect(() => {
    let el = document.getElementById("za-appearance-bg");
    if (!bgDataUrl) {
      el?.remove();
      return;
    }
    if (!el) {
      el = document.createElement("div");
      el.id = "za-appearance-bg";
      document.body.appendChild(el);
    }
    el.style.cssText = `position:fixed;inset:0;z-index:0;pointer-events:none;background:url("${bgDataUrl}") center / cover no-repeat;opacity:${appearance.bgOpacity};`;
  }, [bgDataUrl, appearance.bgOpacity]);

  const value = useMemo<AppearanceCtx>(
    () => ({
      appearance,
      setAppearance: (patch) =>
        setAppearanceState((a) => ({ ...DEFAULTS, ...a, ...patch })),
      reset: () => setAppearanceState(DEFAULTS),
      bgDataUrl,
    }),
    [appearance, bgDataUrl]
  );

  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function useAppearance(): AppearanceCtx {
  const ctx = useContext(Ctx);
  if (!ctx) throw new Error("useAppearance 必须在 AppearanceProvider 内使用");
  return ctx;
}
