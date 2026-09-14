import { createApp } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { setupMyUII18n } from 'myui'

// 样式分层：EP 基础样式 → EP 暗色变量 → MyUI 令牌/组件观感 → EP 变量桥 → 应用壳层 → 悬浮窗
import 'element-plus/dist/index.css'
import 'element-plus/theme-chalk/dark/css-vars.css'
import 'myui/styles'
import 'myui/element-theme'
import './styles/app.css'
import './styles/float.css'

import { i18n } from './i18n'
import { applyTheme, registerSystemThemeWatcher } from './store/ui'
import { initAppearance } from './composables/useAppearance'
import App from './App.vue'
import FloatBall from './windows/FloatBall.vue'
import FloatPanel from './windows/FloatPanel.vue'

// 浏览器直开 dev server 时无 Tauri 环境，降级按主窗口渲染（便于布局验收）
let label = 'main'
try {
  label = getCurrentWindow().label
} catch {
  /* non-tauri */
}

// 移除启动 loader
const loader = document.getElementById('boot-loader')
if (loader) loader.remove()

setupMyUII18n(i18n as unknown as Parameters<typeof setupMyUII18n>[0])
applyTheme()
registerSystemThemeWatcher()

const app = createApp(label === 'float-ball' ? FloatBall : label === 'float-panel' ? FloatPanel : App)
app.use(i18n)

if (label === 'float-ball') {
  document.documentElement.classList.add('za-floatball')
} else if (label === 'float-panel') {
  document.documentElement.classList.add('za-floatpanel')
} else {
  // 外观定制仅主窗口；悬浮球 / 悬浮面板小窗不套用
  initAppearance()
}

app.mount('#app')
