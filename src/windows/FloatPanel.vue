<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { win, events, prefs as prefsApi, pickRingBuckets, usageColor, feLog } from '@/api'
import type { AppPrefs, QuotaOverview } from '@/types'

/**
 * 悬浮球触发的信息面板（独立 float-panel 窗口）：供应商 / 套餐 / 配额进度。
 * 纯展示（无按钮）。hover 联动隐藏由 float://ball-leave 事件协调：
 *   球离开 → 发 ball-leave → 本面板 250ms 后隐藏；
 *   鼠标进入本面板 → 取消隐藏；离开本面板 → 立即隐藏。
 * 固定态（float://panel-pinned，单击悬浮球切换）：面板常驻——忽略上述所有
 * 自动收起联动，点击面板窗口外也不关闭；再次单击球或点本面板 ✕ 才收起。
 */
defineOptions({ name: 'FloatPanel' })

const HIDE_DELAY = 250

const q = ref<QuotaOverview | null>(null)
const pinned = ref(false)
const prefs = ref<AppPrefs>({
  floatBallVisible: true,
  usageDisplay: 'used',
  switchRestartZcode: true,
  autostart: false,
})
let hideTimer: ReturnType<typeof setTimeout> | null = null

// 事件回调里读最新固定态（闭包旧值问题）
const pinnedRef = ref(false)
watch(pinned, (v) => (pinnedRef.value = v), { immediate: true })

function cancelHide(): void {
  if (hideTimer) {
    clearTimeout(hideTimer)
    hideTimer = null
  }
}
function scheduleHide(): void {
  if (pinnedRef.value) return
  cancelHide()
  hideTimer = setTimeout(() => {
    feLog('panel: hide by scheduled timer')
    void win.hideFloatPanel()
  }, HIDE_DELAY)
}
function hideNow(): void {
  if (pinnedRef.value) return
  cancelHide()
  void win.hideFloatPanel()
}

const unlistenFns: (() => void)[] = []

onMounted(() => {
  feLog('mounted, label=' + getCurrentWindow().label)
  // 配额（主窗口 AppShell 每 5s 全局轮询广播，唯一数据源）
  events.onQuotaUpdated((v) => (q.value = v)).then((fn) => unlistenFns.push(fn))
  prefsApi.get().then((v) => (prefs.value = v)).catch(() => {})
  events.onPrefsUpdated((v) => (prefs.value = v)).then((fn) => unlistenFns.push(fn))
  // 球离开 → 延迟隐藏
  events.onBallLeave(() => scheduleHide()).then((fn) => unlistenFns.push(fn))
  // 固定态联动（单击球固定/收起时后端广播）
  events.onPanelPinned((v) => (pinned.value = v)).then((fn) => unlistenFns.push(fn))
})

onBeforeUnmount(() => {
  unlistenFns.forEach((fn) => fn())
  cancelHide()
})

// 双环 bucket：智谱取「每5小时 / 每周」，其余供应商（用量模板）回退前两个 bucket
const buckets = computed(() => pickRingBuckets(q.value))
const showRemaining = computed(() => prefs.value.usageDisplay === 'remaining')

/** 展示名压缩：每5小时使用额度 → 每5小时、每周使用额度 → 每周；其余名称原样返回 */
function shortBucketName(name?: string): string | undefined {
  if (!name) return undefined
  if (name.includes('5小时')) return '每5小时'
  if (name.includes('每周')) return '每周'
  return name
}

/** 重置时间紧凑显示：今天 → HH:mm；跨天 → MM-dd HH:mm */
function fmtReset(iso?: string): string {
  if (!iso) return '—'
  const d = new Date(iso)
  if (Number.isNaN(d.getTime())) return '—'
  const now = new Date()
  const hh = String(d.getHours()).padStart(2, '0')
  const mi = String(d.getMinutes()).padStart(2, '0')
  if (d.toDateString() === now.toDateString()) return `${hh}:${mi}`
  const mm = String(d.getMonth() + 1).padStart(2, '0')
  const dd = String(d.getDate()).padStart(2, '0')
  return `${mm}-${dd} ${hh}:${mi}`
}

interface Block {
  key: string
  label: string
  usedPct: number | null
  periodEnd?: string
  /** 单环几何：cx/cy、半径、周长、弧偏移、颜色 */
  c: number
  r: number
  C: number
  offset: number
  color: string
  valueText: string
}

function buildBlock(key: string, fallbackLabel: string, b?: { name: string; used: number; total: number; periodEnd?: string } | null): Block | null {
  if (key !== 'b5' && !b) return null
  const usedPct = b && b.total > 0 ? (b.used / b.total) * 100 : null
  const size = 32
  const c = size / 2
  const r = c - 3
  const C = 2 * Math.PI * r
  const used = usedPct != null ? Math.max(0, Math.min(100, usedPct)) : null
  const frac = used != null ? (showRemaining.value ? 100 - used : used) / 100 : 0
  const offset = used != null ? C * (1 - frac) : C
  const color = usedPct != null ? usageColor(usedPct) : 'rgba(255,255,255,0.25)'
  const shown = used != null ? Math.round(showRemaining.value ? 100 - used : used) : null
  const word = showRemaining.value ? '剩余' : '已用'
  return {
    key,
    label: shortBucketName(b?.name) ?? fallbackLabel,
    usedPct,
    periodEnd: b?.periodEnd,
    c,
    r,
    C,
    offset,
    color,
    valueText: shown != null ? `${word} ${shown}%` : '—',
  }
}

const blocks = computed<Block[]>(() => {
  const b5 = buildBlock('b5', '每5小时', buckets.value.b5)
  const bW = buildBlock('bW', '每周', buckets.value.bW)
  return [b5, bW].filter((x): x is Block => x !== null)
})
</script>

<template>
  <div class="fp-root" @mouseenter="cancelHide" @mouseleave="hideNow">
    <div class="fp-card">
      <div class="fp-header">
        <span class="fp-title">
          zcode-assistant<span v-if="pinned" class="fp-pin-badge">已固定</span>
        </span>
        <button
          class="fp-close"
          type="button"
          :title="pinned ? '收起面板（取消固定）' : '收起面板'"
          @click="win.hideFloatPanel()"
        >
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
            stroke-linecap="round" stroke-linejoin="round">
            <path d="M18 6 6 18" />
            <path d="m6 6 12 12" />
          </svg>
        </button>
      </div>

      <div class="fp-row">
        <span class="fp-row-label">供应商</span>
        <span class="fp-row-value">{{ q?.providerName ?? '自动 · 智谱' }}</span>
      </div>
      <div class="fp-row">
        <span class="fp-row-label">套餐</span>
        <span class="fp-row-value">{{ q?.planName ?? '—' }}</span>
      </div>

      <div class="fp-quota">
        <div class="fp-quota-list">
          <div v-for="b in blocks" :key="b.key" class="fp-block">
            <svg :width="32" :height="32" :viewBox="`0 0 32 32`" style="transform: rotate(-90deg); display: block">
              <circle :cx="b.c" :cy="b.c" :r="b.r" fill="none" stroke="rgba(255,255,255,0.1)" stroke-width="3" />
              <circle
                :cx="b.c" :cy="b.c" :r="b.r" fill="none" :stroke="b.color" stroke-width="3"
                stroke-linecap="round" :stroke-dasharray="b.C" :stroke-dashoffset="b.offset"
                style="transition: stroke-dashoffset 0.5s ease, stroke 0.3s ease"
              />
            </svg>
            <div class="fp-block-copy">
              <div class="fp-block-head">
                <span class="fp-block-label">{{ b.label }}</span>
                <span v-if="b.periodEnd" class="fp-block-reset">重置 {{ fmtReset(b.periodEnd) }}</span>
              </div>
              <span class="fp-block-value" :style="{ color: b.usedPct != null ? b.color : 'var(--text-tertiary)' }">
                {{ b.valueText }}
              </span>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.fp-quota-list {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.fp-block {
  display: flex;
  align-items: center;
  gap: 10px;
}
.fp-block-copy {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
  flex: 1;
}
.fp-block-head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 8px;
}
.fp-block-label {
  font-size: 11px;
  color: var(--text-secondary);
}
.fp-block-reset {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--text-tertiary);
}
.fp-block-value {
  font-family: var(--font-mono);
  font-size: 13px;
  font-weight: 600;
}
</style>
