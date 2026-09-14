<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { MyDualRing } from 'myui'
import { win, events, prefs as prefsApi, pickRingBuckets, feLog } from '@/api'
import type { AppPrefs, QuotaOverview } from '@/types'

/**
 * 悬浮球：悬停 → 展开信息面板（float-panel 独立窗口），单击 → 固定/取消固定
 * 面板，双击 → 打开主界面，拖拽 → 移动。
 * 鼠标离开时通过 float://ball-leave 通知面板延迟隐藏（球与面板是两个 OS 窗口，
 * 跨窗口 hover 需事件总线协调，否则移到面板途中会触发隐藏）。
 */
defineOptions({ name: 'FloatBall' })

const clamp = (n: number) => Math.max(0, Math.min(100, n))

/** 单击与双击的判定窗口：单击延迟固定面板，双击取消挂起的固定并打开主界面 */
const CLICK_DELAY = 260

const data = ref<QuotaOverview | null>(null)
const pinned = ref(false)
const prefs = ref<AppPrefs>({
  floatBallVisible: true,
  usageDisplay: 'used',
  switchRestartZcode: true,
  autostart: false,
})
let clickTimer: ReturnType<typeof setTimeout> | null = null
let wasDragged = false

const unlistenFns: (() => void)[] = []

onMounted(() => {
  feLog('mounted, label=' + getCurrentWindow().label)
  getCurrentWindow()
    .setIgnoreCursorEvents(false)
    .then(() => feLog('setIgnoreCursorEvents(false) ok'))
    .catch((e) => feLog('setIgnoreCursorEvents failed: ' + String(e), 'error'))
  events.onQuotaUpdated((q) => (data.value = q)).then((fn) => unlistenFns.push(fn))
  // 展示方案（已用 / 剩余）随设置联动
  prefsApi.get().then((v) => (prefs.value = v)).catch(() => {})
  events.onPrefsUpdated((v) => (prefs.value = v)).then((fn) => unlistenFns.push(fn))
  // 面板固定态（单击球切换；面板 ✕ / 再次单击收起时复位）→ 显示指示点
  events.onPanelPinned((v) => (pinned.value = v)).then((fn) => unlistenFns.push(fn))
})

onBeforeUnmount(() => {
  unlistenFns.forEach((fn) => fn())
  if (clickTimer) clearTimeout(clickTimer)
})

// 双环 bucket：智谱取「每5小时 / 每周」，其余供应商（用量模板）回退前两个 bucket
const buckets = computed(() => pickRingBuckets(data.value))
const used5 = computed(() =>
  buckets.value.b5 && buckets.value.b5.total > 0 ? clamp((buckets.value.b5.used / buckets.value.b5.total) * 100) : null,
)
const usedW = computed(() =>
  buckets.value.bW && buckets.value.bW.total > 0 ? clamp((buckets.value.bW.used / buckets.value.bW.total) * 100) : null,
)
// 中心数字按展示方案：已用占比 或 剩余占比（环弧长同步；颜色始终按已用度分档）
const showRemaining = computed(() => prefs.value.usageDisplay === 'remaining')
const shown5 = computed(() => (used5.value != null ? (showRemaining.value ? 100 - used5.value : used5.value) : null))
const shownW = computed(() => (usedW.value != null ? (showRemaining.value ? 100 - usedW.value : usedW.value) : null))
// 弧长随展示方案；分档依据始终为已用占比
const arc5 = computed(() => (used5.value != null ? (showRemaining.value ? 100 - used5.value : used5.value) : null))
const arcW = computed(() => (usedW.value != null ? (showRemaining.value ? 100 - usedW.value : usedW.value) : null))

function onMouseDown(e: MouseEvent): void {
  if (e.button !== 0) return
  // 新一次按下即取消挂起的单击固定（快速点一下又按住拖拽的场景不应触发固定）
  if (clickTimer) {
    clearTimeout(clickTimer)
    clickTimer = null
  }
  const startX = e.screenX
  const startY = e.screenY
  let dragged = false
  const onMove = (ev: MouseEvent) => {
    if (!dragged && (Math.abs(ev.screenX - startX) > 4 || Math.abs(ev.screenY - startY) > 4)) {
      dragged = true
      getCurrentWindow()
        .startDragging()
        .catch((err) => feLog('startDragging failed: ' + String(err), 'error'))
    }
  }
  const onUp = () => {
    window.removeEventListener('mousemove', onMove)
    window.removeEventListener('mouseup', onUp)
    wasDragged = dragged
  }
  window.addEventListener('mousemove', onMove)
  window.addEventListener('mouseup', onUp)
}

// 悬停 → 显示面板；离开 → 通知面板延迟隐藏（面板自身决定是否真的隐藏）
function onMouseEnter(): void {
  win.showFloatPanel().catch((e) => feLog('showFloatPanel ERR: ' + String(e), 'error'))
}
function onMouseLeave(): void {
  events.emitBallLeave().catch(() => {})
}

// 单击 → 固定/取消固定面板。双击会先触发两次 click：第二次命中挂起定时器
// 直接忽略，随后 dblclick 取消定时器并打开主界面，避免固定态被误切换。
function onClick(): void {
  if (wasDragged) {
    wasDragged = false
    return
  }
  if (clickTimer) return
  clickTimer = setTimeout(() => {
    clickTimer = null
    win.toggleFloatPanelPin().catch((e) => feLog('toggleFloatPanelPin ERR: ' + String(e), 'error'))
  }, CLICK_DELAY)
}

// 双击 → 打开主界面（取消挂起的单击固定）
function onDoubleClick(): void {
  if (clickTimer) {
    clearTimeout(clickTimer)
    clickTimer = null
  }
  win.showMain().catch((e) => feLog('showMain ERR: ' + String(e), 'error'))
}
</script>

<template>
  <div class="fb-stage">
    <div
      class="fb-ball"
      :class="{ 'fb-ball-pinned': pinned }"
      :title="pinned ? '面板已固定：单击球收起，双击打开主界面' : '单击固定面板，双击打开主界面'"
      @mousedown="onMouseDown"
      @mouseenter="onMouseEnter"
      @mouseleave="onMouseLeave"
      @click="onClick"
      @dblclick="onDoubleClick"
    >
      <span v-if="pinned" class="fb-pin-dot" />
      <div class="fb-inner">
        <div class="fb-ring-layer">
          <MyDualRing
            :size="52"
            :value="arc5"
            :color-value="used5 ?? undefined"
            :inner-value="arcW"
            :inner-color-value="usedW ?? undefined"
            :warn-at="70"
            :danger-at="90"
          />
        </div>
        <div class="fb-center">
          <template v-if="shown5 != null || shownW != null">
            <!-- 上行：每 5 小时（对应外环，按展示方案为已用或剩余） -->
            <div class="fb-line">
              <span class="fb-pct">{{ shown5 != null ? Math.round(shown5) : '—' }}</span>
              <span class="fb-unit">%</span>
            </div>
            <!-- 下行：每周（对应内环） -->
            <div class="fb-line fb-line-sub">
              <span class="fb-pct-sm">{{ shownW != null ? Math.round(shownW) : '—' }}</span>
              <span class="fb-unit-sm">%</span>
            </div>
          </template>
          <span v-else class="fb-dot" />
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.fb-ring-layer {
  position: absolute;
  inset: 0;
  display: grid;
  place-items: center;
}
</style>
