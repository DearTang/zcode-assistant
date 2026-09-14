<script setup lang="ts">
import { computed } from 'vue'
import { MyDualRing } from 'myui'
import { formatUnits, pickBucketByName } from '@/api'
import type { QuotaBucket, QuotaOverview, UsageDisplayMode } from '@/types'

/**
 * 供应商卡片额度行（智谱 vs 模板 统一展示）。
 * quota: undefined=查询中 / null=查询失败或未配置 / QuotaOverview=正常。
 */
const props = withDefaults(
  defineProps<{
    quota: QuotaOverview | null | undefined
    isBigmodel: boolean
    usageDisplay?: UsageDisplayMode
  }>(),
  { usageDisplay: 'used' },
)
defineOptions({ name: 'ProviderQuotaRow' })

const showRemaining = computed(() => props.usageDisplay === 'remaining')

/** 弧长随展示方案（已用/剩余占比）；分档依据始终为已用占比（colorValue） */
const arc = (used: number | null): number | null => (used != null && showRemaining.value ? 100 - used : used)

/** 按方案格式化单桶占比（% 单位取整）或绝对量 */
function fmtBucket(b: { unit?: string | null; used: number; total: number; remaining: number }): string {
  if (b.unit === '%') {
    return showRemaining.value ? `${Math.round(b.remaining)}%` : `${b.total > 0 ? Math.round((b.used / b.total) * 100) : 0}%`
  }
  return showRemaining.value ? formatUnits(b.remaining) : formatUnits(b.used)
}

/** bucket 是否已耗尽：% 桶按取整判定（与 fmtBucket 显示口径一致） */
function bucketExhausted(b: QuotaBucket): boolean {
  if (b.unit === '%') return Math.round(b.remaining) <= 0
  return b.remaining <= 0
}

function formatReset(iso: string): string {
  const d = new Date(iso)
  if (isNaN(d.getTime())) return ''
  const p = (n: number) => String(n).padStart(2, '0')
  return `${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`
}

function formatTime(iso: string): string {
  const d = new Date(iso)
  if (isNaN(d.getTime())) return ''
  const p = (n: number) => String(n).padStart(2, '0')
  return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`
}

const quota = computed(() => props.quota)
const placeholder = computed(() => {
  if (quota.value === undefined) return '额度查询中…'
  if (quota.value === null) {
    return props.isBigmodel ? '智谱配额查询失败' : '未配置用量查询（双击详情配置）'
  }
  return ''
})

const b5 = computed(() => (quota.value ? pickBucketByName(quota.value, '5小时') : undefined))
const bW = computed(() => (quota.value ? pickBucketByName(quota.value, '每周') : undefined))
// 月限额桶（可选）：排除智谱的「MCP每月额度」（那是工具调用次数，不是模型月限额）
const bM = computed(() => quota.value?.buckets.find((b) => b.name.includes('每月') && !b.name.includes('MCP')))
// 余额桶（可选）：不属于 5小时/每周/每月 的第一个桶（模板主桶）
const bB = computed(() =>
  quota.value?.buckets.find((b) => !b.name.includes('5小时') && !b.name.includes('每周') && !b.name.includes('每月')),
)

const monthlyPart = computed(() => (bM.value ? ` · 每月 ${showRemaining.value ? '剩' : '已用'}${fmtBucket(bM.value)}` : ''))
const balancePart = computed(() => (bB.value ? ` · ${bB.value.name} ${showRemaining.value ? '剩' : '已用'}${fmtBucket(bB.value)}` : ''))
const monthlyOk = computed(() => !bM.value || !bucketExhausted(bM.value))
const balanceOk = computed(() => !bB.value || !bucketExhausted(bB.value))

// 双环分支（每5小时 + 每周）
const dual = computed(() => {
  if (!quota.value || !(b5.value || bW.value)) return null
  const u5 = b5.value && b5.value.total > 0 ? (b5.value.used / b5.value.total) * 100 : null
  const uW = bW.value && bW.value.total > 0 ? (bW.value.used / bW.value.total) * 100 : null
  const word = showRemaining.value ? '剩' : '已用'
  const summary = b5.value && bW.value
    ? `每5小时 ${word}${fmtBucket(b5.value)} · 每周 ${word}${fmtBucket(bW.value)}`
    : b5.value
      ? `每5小时 ${word}${fmtBucket(b5.value)}`
      : bW.value
        ? `每周 ${word}${fmtBucket(bW.value)}`
        : '无额度数据'
  // 可用性只判定实际存在的 bucket：部分供应商没有周限额（只有 5 小时窗口）
  const ok = (b5.value ? !bucketExhausted(b5.value) : true) && (bW.value ? !bucketExhausted(bW.value) : true) && monthlyOk.value && balanceOk.value
  // 重置时间优先级：已耗尽的桶优先（月→周→5小时），均未耗尽时回退 5 小时
  const reset =
    (bM.value && bucketExhausted(bM.value) ? bM.value.periodEnd : undefined) ||
    (bW.value && bucketExhausted(bW.value) ? bW.value.periodEnd : undefined) ||
    (b5.value && bucketExhausted(b5.value) ? b5.value.periodEnd : undefined) ||
    b5.value?.periodEnd ||
    bW.value?.periodEnd
  return { u5, uW, summary, ok, reset }
})

// 单桶分支（模板）
const single = computed(() => {
  if (!quota.value || b5.value || bW.value) return null
  const b = quota.value.buckets[0]
  if (!b) return null
  const usedPct = b.total > 0 ? (b.used / b.total) * 100 : null
  const summary = `${showRemaining.value ? '剩' : '已用'} ${fmtBucket(b)}` + monthlyPart.value
  const ok = !bucketExhausted(b) && monthlyOk.value
  // 重置时间：月限额耗尽时优先展示月重置，否则用主桶
  const reset = (bM.value && bucketExhausted(bM.value) ? bM.value.periodEnd : undefined) || b.periodEnd
  return { usedPct, summary, ok, reset }
})
</script>

<template>
  <!-- 查询中 / 失败占位 -->
  <div v-if="placeholder" class="qr">
    <span class="qr-empty-ring" />
    <span class="qr-text">{{ placeholder }}</span>
  </div>

  <!-- 智谱 BigModel / Token Plan：每5小时 + 每周双环（部分再追加每月、余额） -->
  <div v-else-if="dual" class="qr">
    <MyDualRing
      :size="28"
      :value="arc(dual.u5)"
      :color-value="dual.u5 ?? undefined"
      :inner-value="arc(dual.uW)"
      :inner-color-value="dual.uW ?? undefined"
      :warn-at="70"
      :danger-at="90"
    />
    <span class="qr-text">{{ dual.summary }}{{ monthlyPart }}{{ balancePart }}</span>
    <span class="qr-badge" :class="dual.ok ? 'ok' : 'bad'">{{ dual.ok ? '可用' : '不可用' }}</span>
    <span v-if="dual.reset" class="qr-reset">重置 {{ formatReset(dual.reset) }}</span>
  </div>

  <!-- 模板：单 bucket -->
  <div v-else-if="single" class="qr">
    <MyDualRing
      :size="28"
      :value="arc(single.usedPct)"
      :color-value="single.usedPct ?? undefined"
      :warn-at="70"
      :danger-at="90"
    />
    <span class="qr-text">{{ single.summary }}</span>
    <span class="qr-badge" :class="single.ok ? 'ok' : 'bad'">{{ single.ok ? '可用' : '不可用' }}</span>
    <span class="qr-reset">
      {{ single.reset ? `重置 ${formatReset(single.reset)}` : `更新 ${quota ? formatTime(quota.fetchedAt) : ''}` }}
    </span>
  </div>

  <div v-else class="qr">
    <span class="qr-empty-ring" />
    <span class="qr-text">无额度数据</span>
  </div>
</template>

<style scoped>
.qr {
  display: flex;
  gap: 8px;
  align-items: center;
  padding-left: 22px;
}
.qr-empty-ring {
  width: 28px;
  height: 28px;
  border-radius: 50%;
  border: 2px solid var(--border-subtle);
  flex-shrink: 0;
}
.qr-text {
  font-family: var(--font-mono);
  font-size: 11px;
}
.qr-badge {
  font-size: 11px;
  padding: 0 8px;
  line-height: 18px;
  border-radius: 999px;
}
.qr-badge.ok {
  background: oklch(0.7 0.17 160 / 0.15);
  color: var(--success);
}
.qr-badge.bad {
  background: oklch(0.62 0.24 27 / 0.15);
  color: var(--danger);
}
.qr-reset {
  color: var(--text-tertiary);
  font-family: var(--font-mono);
  font-size: 11px;
}
</style>
