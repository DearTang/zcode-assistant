import { createI18n } from 'vue-i18n'

/**
 * 应用文案保持硬编码中文，不走 i18n；此实例仅承载 myui 库词条
 * （main.ts 里 setupMyUII18n 会把 myui.* 命名空间合并进来）。
 */
export const i18n = createI18n({
  legacy: false,
  locale: 'zh-CN',
  fallbackLocale: 'zh-CN',
  messages: { 'zh-CN': {} },
})
