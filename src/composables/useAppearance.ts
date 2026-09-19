import { reactive, watchEffect } from 'vue'
import { beautify } from '@/api'

/**
 * 外观定制（仅主窗口；悬浮球 / 悬浮面板小窗不套用）：
 * - 主题色：改写强调色系 token（--accent 家族 + --glow-accent），保留原有 L/C 只换色相；
 * - 透明度：把表面 / 玻璃面板 token 按 color-mix 透明化；
 * - 背景图：fixed 图层铺在窗口底层，侧边栏 / 顶栏降为轻纱玻璃。
 * 全部通过运行时注入 <style>（#za-appearance-style）实现，追加在 myui/styles 之后、
 * 同名 html.dark / html.light 选择器靠源顺序覆盖；还原默认 = 清空配置即恢复。
 */
export interface Appearance {
  /** 强调色色相（OKLCH hue 0–360）；null = 默认青绿（不覆盖任何 token） */
  accentHue: number | null
  /** 玻璃面板与表面不透明度 0.4–1；1 = 不覆盖（默认实底） */
  surfaceOpacity: number
  /** 背景图本地绝对路径；null = 无背景图 */
  bgImage: string | null
  /** 背景图不透明度 0.1–1 */
  bgOpacity: number
}

const DEFAULTS: Appearance = {
  accentHue: null,
  surfaceOpacity: 1,
  bgImage: null,
  bgOpacity: 1,
}
const STORAGE_KEY = 'za-appearance'

const round3 = (n: number) => Math.round(n * 1000) / 1000

/** 生成覆盖 myui token 的注入样式；空配置返回空串（还原默认） */
function buildCss(ap: Appearance): string {
  const parts: string[] = []
  const h = ap.accentHue
  if (h != null) {
    // 强调色系：沿用 myui tokens 的 L/C 数值，只替换色相
    parts.push(
      `html.dark{
  --accent: oklch(0.72 0.15 ${h});
  --accent-hover: oklch(0.77 0.14 ${h});
  --accent-active: oklch(0.67 0.15 ${h});
  --accent-subtle: oklch(0.72 0.15 ${h} / 0.12);
  --accent-fg: oklch(0.15 0.012 250);
  --glow-accent: 0 0 40px oklch(0.72 0.15 ${h} / 0.15);
  --glow-accent-strong: 0 0 60px oklch(0.72 0.15 ${h} / 0.28);
}`,
      `html.light{
  --accent: oklch(0.55 0.13 ${h});
  --accent-hover: oklch(0.5 0.14 ${h});
  --accent-active: oklch(0.46 0.14 ${h});
  --accent-subtle: oklch(0.55 0.13 ${h} / 0.1);
  --accent-fg: oklch(0.99 0 0);
  --glow-accent: 0 0 40px oklch(0.55 0.13 ${h} / 0.12);
  --glow-accent-strong: 0 0 60px oklch(0.55 0.13 ${h} / 0.2);
}`,
    )
  }
  if (ap.surfaceOpacity < 1) {
    // 表面 / 玻璃透明化（--bg-base 窗口底色保持不透明，否则整体不可读）；
    // 原不透明度数值来自 myui tokens.scss
    const k = ap.surfaceOpacity
    const mix = (c: string) => `color-mix(in oklab, ${c} ${Math.round(k * 100)}%, transparent)`
    parts.push(
      `html.dark{
  --bg-surface: ${mix('oklch(0.19 0.014 250)')};
  --bg-elevated: ${mix('oklch(0.22 0.016 250)')};
  --bg-overlay: ${mix('oklch(0.26 0.018 250)')};
  --glass-bg: oklch(0.17 0.014 250 / ${round3(0.48 * k)});
  --glass-bg-strong: oklch(0.17 0.014 250 / ${round3(Math.min(1, 0.7 * k))});
}`,
      `html.light{
  --bg-surface: ${mix('oklch(1 0 0)')};
  --bg-elevated: ${mix('oklch(0.97 0.003 250)')};
  --bg-overlay: ${mix('oklch(1 0 0)')};
  --glass-bg: oklch(1 0 0 / ${round3(0.72 * k)});
  --glass-bg-strong: oklch(1 0 0 / ${round3(Math.min(1, 0.88 * k))});
}`,
    )
  }
  if (ap.bgImage != null) {
    // 背景图生效：侧边栏 / 顶栏从厚玻璃降为轻纱，背景图连续透过整窗
    parts.push(
      `.ui-sidebar, .ui-topbar {
  backdrop-filter: blur(12px) saturate(150%);
  -webkit-backdrop-filter: blur(12px) saturate(150%);
}`,
      `html.dark .ui-sidebar,
html.dark .ui-topbar {
  background: oklch(0.16 0.012 250 / 0.3);
}`,
      `html.light .ui-sidebar,
html.light .ui-topbar {
  background: oklch(1 0 0 / 0.38);
}`,
    )
  }
  return parts.join('\n')
}

function load(): Appearance {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return DEFAULTS
    const v = JSON.parse(raw) as Partial<Appearance>
    return {
      accentHue:
        typeof v.accentHue === 'number' && v.accentHue >= 0 && v.accentHue <= 360 ? v.accentHue : null,
      surfaceOpacity:
        typeof v.surfaceOpacity === 'number' ? Math.min(1, Math.max(0.4, v.surfaceOpacity)) : 1,
      bgImage: typeof v.bgImage === 'string' && v.bgImage ? v.bgImage : null,
      bgOpacity: typeof v.bgOpacity === 'number' ? Math.min(1, Math.max(0.1, v.bgOpacity)) : 1,
    }
  } catch {
    return DEFAULTS
  }
}

/** 模块级单例状态（initAppearance 在 main.ts 调用，先于任何组件） */
export const appearance = reactive<Appearance>(load())

export function setAppearance(patch: Partial<Appearance>): void {
  Object.assign(appearance, DEFAULTS, { ...appearance, ...patch })
}

export function resetAppearance(): void {
  Object.assign(appearance, DEFAULTS)
}

/** 背景图 data URL（>8MB 或读取失败为 null） */
export const bgState = reactive<{ dataUrl: string | null }>({ dataUrl: null })

let inited = false

/** 初始化外观定制：注入覆盖样式 + 加载背景图（仅主窗口调用一次）。 */
export function initAppearance(): void {
  if (inited) return
  inited = true

  // 1) 注入 / 更新覆盖样式
  watchEffect(() => {
    let el = document.getElementById('za-appearance-style') as HTMLStyleElement | null
    if (!el) {
      el = document.createElement('style')
      el.id = 'za-appearance-style'
      document.head.appendChild(el)
    }
    el.textContent = buildCss(appearance)
    localStorage.setItem(STORAGE_KEY, JSON.stringify(appearance))
  })

  // 2) 背景图 data URL
  watchEffect((onCleanup) => {
    let cancelled = false
    onCleanup(() => {
      cancelled = true
    })
    bgState.dataUrl = null
    if (!appearance.bgImage) return
    beautify
      .readImagePreview(appearance.bgImage)
      .then((d) => {
        if (!cancelled) bgState.dataUrl = d
      })
      .catch(() => {})
  })

  // 3) 背景图层：fixed 铺满窗口底层，随不透明度透出
  watchEffect(() => {
    let el = document.getElementById('za-appearance-bg')
    if (!bgState.dataUrl) {
      el?.remove()
      return
    }
    if (!el) {
      el = document.createElement('div')
      el.id = 'za-appearance-bg'
      document.body.appendChild(el)
    }
    // z-index 必须为负：壳层无显式层叠上下文，0 会让本层（body 末尾、fixed）
    // 在绘制顺序上盖住全部 static 内容（WebView2 实测整窗只剩壁纸）
    el.style.cssText = `position:fixed;inset:0;z-index:-1;pointer-events:none;background:url("${bgState.dataUrl}") center / cover no-repeat;opacity:${appearance.bgOpacity};`
  })
}
