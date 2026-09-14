import { reactive } from 'vue'
import type { ViewId } from '@/types'

export type ThemeChoice = 'dark' | 'light' | 'system'

interface UiState {
  themeChoice: ThemeChoice
  /** 由 themeChoice 解析出的当前生效主题（system 时跟随系统） */
  isDark: boolean
  isCollapse: boolean
  activeView: ViewId
  paletteOpen: boolean
  aboutOpen: boolean
}

const PERSIST_KEY = 'ui.prefs'
const LEGACY_THEME_KEY = 'za-theme'
const PERSISTED_KEYS = ['themeChoice', 'isCollapse'] as const

function readPersisted(): Partial<UiState> {
  try {
    const raw = localStorage.getItem(PERSIST_KEY)
    if (raw) return JSON.parse(raw) as Partial<UiState>
    // 迁移兼容：旧版 useTheme 的 za-theme（dark/light 二态）→ themeChoice
    const legacy = localStorage.getItem(LEGACY_THEME_KEY)
    if (legacy === 'light' || legacy === 'dark') return { themeChoice: legacy }
  } catch {
    /* ignore */
  }
  return {}
}

function resolveDark(choice: ThemeChoice): boolean {
  if (choice === 'system') return window.matchMedia('(prefers-color-scheme: light)').matches === false
  return choice === 'dark'
}

const persisted = readPersisted()

export const ui = reactive<UiState>({
  themeChoice: 'dark',
  isDark: true,
  isCollapse: false,
  activeView: 'dashboard',
  paletteOpen: false,
  aboutOpen: false,
  ...persisted,
})

export function persistUi(): void {
  const snapshot: Record<string, unknown> = {}
  for (const key of PERSISTED_KEYS) snapshot[key] = ui[key]
  localStorage.setItem(PERSIST_KEY, JSON.stringify(snapshot))
}

/** 把主题应用到文档根节点（myui 约定：html.dark / html.light 双 class）。 */
export function applyTheme(): void {
  ui.isDark = resolveDark(ui.themeChoice)
  const root = document.documentElement
  root.classList.toggle('dark', ui.isDark)
  root.classList.toggle('light', !ui.isDark)
}

/** themeChoice = system 时跟随系统主题变化。 */
export function registerSystemThemeWatcher(): void {
  const media = window.matchMedia('(prefers-color-scheme: light)')
  media.addEventListener('change', () => {
    if (ui.themeChoice === 'system') applyTheme()
  })
}

export function setThemeChoice(choice: ThemeChoice): void {
  ui.themeChoice = choice
  applyTheme()
  persistUi()
}
