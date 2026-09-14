<script lang="ts">
import { usageColor } from '@/api'

/** 用 canvas 绘制 32x32 双环托盘图标：外圈=每5小时、内圈=每周；
 *  弧长随展示方案（已用 / 剩余），颜色始终按已用占比分级（绿/黄/红），返回 RGBA 像素 */
function drawTrayIcon(
  used5h: number | null,
  usedWeekly: number | null,
  showRemaining: boolean,
): { rgba: number[]; width: number; height: number } {
  const size = 32
  const canvas = document.createElement('canvas')
  canvas.width = size
  canvas.height = size
  const ctx = canvas.getContext('2d')
  if (!ctx) return { rgba: [], width: size, height: size }
  ctx.clearRect(0, 0, size, size)
  drawTrayRing(ctx, 16, 16, 12, 3, used5h, showRemaining) // 外圈 = 每5小时
  drawTrayRing(ctx, 16, 16, 6.5, 2.5, usedWeekly, showRemaining) // 内圈 = 每周
  return {
    rgba: Array.from(ctx.getImageData(0, 0, size, size).data),
    width: size,
    height: size,
  }
}

/** 画一道配额环：背景底环 + 数值弧（showRemaining 时弧长=剩余占比；颜色始终按已用占比分级） */
function drawTrayRing(
  ctx: CanvasRenderingContext2D,
  cx: number,
  cy: number,
  r: number,
  lw: number,
  usedPct: number | null,
  showRemaining: boolean,
): void {
  ctx.beginPath()
  ctx.arc(cx, cy, r, 0, Math.PI * 2)
  ctx.strokeStyle = 'rgba(255,255,255,0.15)'
  ctx.lineWidth = lw
  ctx.stroke()
  if (usedPct == null) return
  const used = Math.max(0, Math.min(100, usedPct))
  const frac = showRemaining ? (100 - used) / 100 : used / 100
  ctx.beginPath()
  ctx.arc(cx, cy, r, -Math.PI / 2, -Math.PI / 2 + Math.PI * 2 * frac)
  ctx.strokeStyle = usageColor(usedPct)
  ctx.lineWidth = lw
  ctx.lineCap = 'round'
  ctx.stroke()
}

export default { name: 'AppShell' }
</script>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { MyCommandPalette, type CommandItem } from 'myui'
import AppTopBar from '@/components/AppTopBar.vue'
import AppSidebar from '@/components/AppSidebar.vue'
import RestartDialog from '@/components/RestartDialog.vue'
import SecondInstanceDialog from '@/components/SecondInstanceDialog.vue'
import UpdateNotification from '@/components/UpdateNotification.vue'
import AboutDialog from '@/components/AboutDialog.vue'
import StatsConsentDialog from '@/components/StatsConsentDialog.vue'
import Dashboard from '@/views/Dashboard/index.vue'
import Models from '@/views/Models/index.vue'
import AutoSwitch from '@/views/AutoSwitch/index.vue'
import Usage from '@/views/Usage/index.vue'
import Projects from '@/views/Projects/index.vue'
import Accounts from '@/views/Accounts/index.vue'
import Proxy from '@/views/Proxy/index.vue'
import Beautify from '@/views/Beautify/index.vue'
import ZcodeSettings from '@/views/ZcodeSettings/index.vue'
import Settings from '@/views/Settings/index.vue'
import { toast } from '@/composables/toast'
import { useUpdateCheck } from '@/composables/useUpdateCheck'
import { events, win, quota, health, prefs as prefsApi, pickRingBuckets, models, app } from '@/api'
import { checkReportNeeded, markVersionHandled, reportVersion, setStatsConsent } from '@/lib/usageStats'
import { persistUi, setThemeChoice, ui } from '@/store/ui'
import type { AppPrefs, QuotaOverview, ViewId } from '@/types'

const META: Record<ViewId, { title: string; sub: string }> = {
  dashboard: { title: '总览', sub: '配额 · 当前模型 · 账号状态' },
  models: { title: '模型管理', sub: '获取可用模型 · 配置上下文 · 写入 zcode' },
  autoswitch: { title: '自动切换', sub: '定时切换 · 配额耗尽自动切换' },
  usage: { title: '用量查询', sub: '按供应商 / 模型 / 日期统计 token 用量与速度' },
  projects: { title: '项目管理', sub: '管理项目与会话 · 查看消耗 · 改名 · 批量删除' },
  accounts: { title: '智谱账号', sub: '多账号捕获与切换' },
  proxy: { title: '网络代理', sub: 'HTTP / SOCKS5 代理' },
  beautify: { title: 'ZCode 美化', sub: '换肤 · 换字体 · 自定义配色（可还原）' },
  'zcode-settings': { title: 'ZCode 设置', sub: '调参 ZCode 本体行为 · 模型调用重试' },
  settings: { title: '设置', sub: '主题 · 关于' },
}

const meta = computed(() => META[ui.activeView])

// ============ 后台更新检查 + 关于弹窗 ============
const { info: updateInfo, loading: updateChecking, checkNow } = useUpdateCheck(true)
const updateDismissed = ref(false)
const appVersion = ref('')
const statsPrompt = ref<{ version: string } | null>(null)

const ignoredVersion = computed(
  () =>
    updateInfo.value?.latestVersion &&
    localStorage.getItem('za.ignoredUpdateVersion') === updateInfo.value.latestVersion,
)

// 供 UpdateNotification 使用的非空视图（v-if 与 props 绑定的类型窄化）
const updateBanner = computed(() => (updateInfo.value?.hasUpdate ? updateInfo.value : null))

function onStatsAgree(): void {
  const v = statsPrompt.value
  if (!v) return
  setStatsConsent(true)
  // reportVersion 只在发送成功后才盖戳，发送失败时下次启动静默重试（不再弹窗）
  void reportVersion(v.version, navigator.platform)
  statsPrompt.value = null
}

function onStatsDecline(): void {
  const v = statsPrompt.value
  if (!v) return
  setStatsConsent(false)
  markVersionHandled(v.version)
  statsPrompt.value = null
}

// ============ 配额轮询（全局唯一查询源）============
// Dashboard / 悬浮球 / 悬浮面板 / 托盘图标共用：每 5s 查询并广播 quota://updated。
// 必须挂在常驻的 AppShell 而非 Dashboard 视图——否则切到其他视图后轮询停止，悬浮窗断更。
const quotaData = ref<QuotaOverview | null>(null)
const quotaLoading = ref(true)
const quotaError = ref<string | null>(null)
const quotaRef = ref<QuotaOverview | null>(null)
const lastQuotaErr = ref<string | null>(null)
const quotaInFlight = ref(false)

async function refreshQuota(silent = false): Promise<void> {
  if (quotaInFlight.value) return
  quotaInFlight.value = true
  if (!silent) quotaLoading.value = true
  quotaError.value = null
  try {
    // 总览配额：主供应商优先（总览 / 悬浮窗 / 托盘共用），未设置则自动识别智谱 Coding Plan
    const q = await quota.getOverview()
    quotaData.value = q
    quotaRef.value = q
    lastQuotaErr.value = null
    // 作为唯一数据源，把结果广播给悬浮窗 / 悬浮球 / 托盘
    events.emitQuotaUpdated(q)
  } catch (e: unknown) {
    const m = typeof e === 'string' ? e : (e as Error)?.message ?? '配额查询失败'
    quotaError.value = m
    if (lastQuotaErr.value !== m) {
      toast.error(`配额查询失败：${m}`)
      lastQuotaErr.value = m
    }
  } finally {
    quotaInFlight.value = false
    quotaLoading.value = false
  }
}

// ============ 应用偏好（悬浮球显隐 / 用量展示方案）============
const prefs = ref<AppPrefs>({
  floatBallVisible: true,
  usageDisplay: 'used',
  switchRestartZcode: true,
  autostart: false,
})
const prefsRef = ref(prefs.value)
watch(prefs, (v) => (prefsRef.value = v), { deep: true })

// 监听配额更新 → 重绘任务栏托盘图标（进度环，颜色随用量变化；弧长随展示方案）
async function applyTrayIcon(q: QuotaOverview | null): Promise<void> {
  if (!q) return
  const { b5, bW } = pickRingBuckets(q)
  const u5 = b5 && b5.total > 0 ? (b5.used / b5.total) * 100 : null
  const uW = bW && bW.total > 0 ? (bW.used / bW.total) * 100 : null
  const icon = drawTrayIcon(u5, uW, prefsRef.value.usageDisplay === 'remaining')
  if (icon.rgba.length > 0) {
    try {
      await win.setTrayIcon(icon.rgba, icon.width, icon.height)
    } catch {
      /* ignore */
    }
  }
}

const unlistenFns: (() => void)[] = []
let quotaTimer = 0
let healthTimer = 0

onMounted(() => {
  // 应用启动：取版本号 → 决定是否上报统计 / 弹同意框
  app
    .getVersion()
    .then((v) => {
      appVersion.value = v
      const { shouldReport, hasConsent } = checkReportNeeded(v)
      if (shouldReport) {
        if (hasConsent) void reportVersion(v, navigator.platform)
        else statsPrompt.value = { version: v }
      }
    })
    .catch(() => {
      /* 取版本号失败静默 */
    })

  // 启动引导主供应商：best-effort，让总览/悬浮窗一启动就有数据源
  models.bootstrapPrimary().catch(() => {})
  void refreshQuota()
  quotaTimer = window.setInterval(() => void refreshQuota(true), 5000)

  // 监听刷新请求（托盘菜单 / 账号切换后转发而来）
  events.onRefreshRequested(() => void refreshQuota()).then((fn) => unlistenFns.push(fn))

  // 当前模型可用性检测：每 30s 冷却检查（命令内部做冷却/退避）；模型切换后立即检测
  const check = () => void health.check(false).catch(() => {})
  check()
  healthTimer = window.setInterval(check, 30_000)
  events.onModelSwitched(check).then((fn) => unlistenFns.push(fn))

  // 偏好加载 + 订阅
  prefsApi
    .get()
    .then((v) => (prefs.value = v))
    .catch(() => {})
  events.onPrefsUpdated((v) => (prefs.value = v)).then((fn) => unlistenFns.push(fn))

  // 托盘图标：仅消费上方广播的配额更新（单一数据源）
  events.onQuotaUpdated((q) => void applyTrayIcon(q)).then((fn) => unlistenFns.push(fn))
  void applyTrayIcon(quotaRef.value)
})

onBeforeUnmount(() => {
  clearInterval(quotaTimer)
  clearInterval(healthTimer)
  unlistenFns.forEach((fn) => fn())
})

// 展示方案变更 → 用最近一次配额立即重绘，不等下一轮轮询
watch(
  () => prefs.value.usageDisplay,
  () => void applyTrayIcon(quotaRef.value),
)

// ============ 命令面板（Ctrl+K）：页面导航 ============
const paletteItems = computed<CommandItem[]>(() =>
  (Object.keys(META) as ViewId[]).map((id) => ({
    id: `page:${id}`,
    label: META[id].title,
    description: META[id].sub,
    group: '页面导航',
    keywords: id,
  })),
)

function onPaletteSelect(item: CommandItem): void {
  ui.paletteOpen = false
  if (item.id.startsWith('page:')) {
    ui.activeView = item.id.slice(5) as ViewId
    return
  }
  if (item.id === 'action:theme') {
    setThemeChoice(ui.isDark ? 'light' : 'dark')
    persistUi()
  }
  if (item.id === 'action:about') ui.aboutOpen = true
}
</script>

<template>
  <div class="ui-shell" :class="{ 'is-collapsed': ui.isCollapse }">
    <AppTopBar :title="meta.title" :subtitle="meta.sub" />
    <AppSidebar :update-available="!!updateInfo?.hasUpdate" @open-about="ui.aboutOpen = true" />

    <main :key="ui.activeView" class="ui-main">
      <div class="ui-content ui-fade-in">
        <Dashboard
          v-if="ui.activeView === 'dashboard'"
          :data="quotaData"
          :loading="quotaLoading"
          :error="quotaError"
          :usage-display="prefs.usageDisplay"
          @refresh="refreshQuota()"
        />
        <Models v-else-if="ui.activeView === 'models'" :usage-display="prefs.usageDisplay" />
        <AutoSwitch v-else-if="ui.activeView === 'autoswitch'" />
        <Usage v-else-if="ui.activeView === 'usage'" />
        <Projects v-else-if="ui.activeView === 'projects'" />
        <Accounts v-else-if="ui.activeView === 'accounts'" />
        <Proxy v-else-if="ui.activeView === 'proxy'" />
        <Beautify v-else-if="ui.activeView === 'beautify'" />
        <ZcodeSettings v-else-if="ui.activeView === 'zcode-settings'" />
        <Settings v-else-if="ui.activeView === 'settings'" />
      </div>
    </main>
  </div>

  <RestartDialog />
  <SecondInstanceDialog />
  <UpdateNotification
    v-if="updateBanner && !ignoredVersion && !updateDismissed"
    :update-info="updateBanner"
    @ignored="updateDismissed = true"
  />
  <AboutDialog
    v-if="ui.aboutOpen"
    :version="appVersion"
    :update-info="updateInfo"
    :checking="updateChecking"
    @close="ui.aboutOpen = false"
    @check-updates="checkNow"
  />
  <StatsConsentDialog v-if="statsPrompt" :version="statsPrompt.version" @agree="onStatsAgree" @decline="onStatsDecline" />

  <MyCommandPalette v-model="ui.paletteOpen" :items="paletteItems" placeholder="搜索功能或命令…" @select="onPaletteSelect" />
</template>
