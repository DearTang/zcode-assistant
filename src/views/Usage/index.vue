<script setup lang="ts">
/**
 * 用量查询 —— 解析 zcode 的模型调用记录，按供应商 / 模型 / 日期聚合 token 用量，
 * 并统计输出速度（最快 / 平均 / 最慢）。
 * 数据源：~/.zcode/cli/rollout/model-io-sess_*.jsonl（只读，由后端 usage_sync 解析）。
 */
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { ElTableColumn } from 'element-plus'
import { MyButton, MyIcon, MyInput, MyPanel, MyResultState, MySegmented, MySelect, MyDataTable } from 'myui'
import { usage, formatUnits, usageColor, type UsageQuery } from '@/api'
import type { UsageAggRow, UsageFilters, UsageGroupBy, UsageOverview, UsageRecord, UsageSyncResult } from '@/types'
import { toast } from '@/composables/toast'

defineOptions({ name: 'UsageView' })

type RangePreset = 'today' | '7d' | '30d' | 'all' | 'custom'
type SortKey =
  | 'label'
  | 'calls'
  | 'input'
  | 'output'
  | 'cache'
  | 'total'
  | 'avgTps'
  | 'maxTps'
  | 'minTps'
  | 'avgDuration'

const PRESETS = [
  { label: '今天', value: 'today' },
  { label: '近 7 天', value: '7d' },
  { label: '近 30 天', value: '30d' },
  { label: '全部', value: 'all' },
  { label: '自定义', value: 'custom' },
]

const GROUPS: { id: UsageGroupBy; label: string }[] = [
  { id: 'provider', label: '按供应商' },
  { id: 'model', label: '按模型' },
  { id: 'date', label: '按日期' },
]

/** YYYY-MM-DD（本地） */
function todayStr(): string {
  const d = new Date()
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`
}
function daysAgoStr(n: number): string {
  const d = new Date()
  d.setDate(d.getDate() - n)
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`
}

/**
 * 供应商/分组键的友好显示：
 * - `builtin:xxx` → 去前缀（bigmodel-coding-plan …）
 * - UUID → `自定义·前8位`
 * 注：raw id 仍用于筛选/分组，此处仅做展示美化。
 */
function prettyKey(k: string): string {
  if (!k) return k
  if (k.startsWith('builtin:')) return k.slice('builtin:'.length)
  if (/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-/i.test(k)) return `自定义·${k.slice(0, 8)}`
  return k
}

function fmtTps(n?: number | null): string {
  if (n == null || !Number.isFinite(n)) return '—'
  return n.toFixed(1)
}
function fmtMs(n?: number | null): string {
  if (n == null || !Number.isFinite(n)) return '—'
  if (n >= 1000) return `${(n / 1000).toFixed(1)}s`
  return `${Math.round(n)}ms`
}
function fmtTokens(n?: number | null): string {
  if (n == null) return '—'
  return formatUnits(n)
}
function fmtTime(s?: string): string {
  if (!s) return '—'
  const d = new Date(s)
  if (Number.isNaN(d.getTime())) return s.replace('T', ' ').replace('Z', '').slice(5, 16)
  const mm = String(d.getMonth() + 1).padStart(2, '0')
  const dd = String(d.getDate()).padStart(2, '0')
  const hh = String(d.getHours()).padStart(2, '0')
  const mi = String(d.getMinutes()).padStart(2, '0')
  return `${mm}-${dd} ${hh}:${mi}`
}
function dimLabel(g: UsageGroupBy): string {
  return g === 'provider' ? '供应商' : g === 'model' ? '模型' : '日期'
}

const loading = ref(true)
const syncing = ref(false)
const syncInfo = ref<UsageSyncResult | null>(null)

const filters = ref<UsageFilters | null>(null)
const overview = ref<UsageOverview | null>(null)
const rows = ref<UsageAggRow[]>([])

const preset = ref<RangePreset>('30d')
const from = ref('')
const to = ref('')
const selProvider = ref('')
const selModel = ref('')
const selRole = ref('')

// 供应商别名（UUID/builtin -> 可读名，由后端从 transcript 解析）
const labels = ref<Record<string, string>>({})

const groupBy = ref<UsageGroupBy>('provider')
const sortKey = ref<SortKey>('total')
const sortDir = ref<'asc' | 'desc'>('desc')

const detailOpen = ref(false)
const detailRows = ref<UsageRecord[]>([])
const detailLoading = ref(false)

// 预设 → 日期范围
watch(preset, (p) => {
  if (p === 'today') {
    from.value = todayStr()
    to.value = todayStr()
  } else if (p === '7d') {
    from.value = daysAgoStr(6)
    to.value = todayStr()
  } else if (p === '30d') {
    from.value = daysAgoStr(29)
    to.value = todayStr()
  } else if (p === 'all') {
    from.value = ''
    to.value = ''
  }
  // custom：沿用当前 from/to
})

const query = computed<UsageQuery>(() => ({
  from: from.value || undefined,
  to: to.value || undefined,
  provider: selProvider.value || undefined,
  model: selModel.value || undefined,
  role: selRole.value || undefined,
}))

/** 供应商显示名：transcript 别名 > builtin 去前缀 > UUID 短码 > 原值 */
function labelProvider(id: string): string {
  if (!id) return '—'
  if (labels.value[id]) return labels.value[id]
  if (id.startsWith('builtin:')) return id.slice('builtin:'.length)
  if (/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-/i.test(id)) return `自定义·${id.slice(0, 8)}`
  return id
}

async function loadAgg(): Promise<void> {
  loading.value = true
  try {
    const [ov, ag] = await Promise.all([usage.overview(query.value), usage.aggregate(groupBy.value, query.value)])
    overview.value = ov
    rows.value = ag
  } catch (e: unknown) {
    toast.error(`加载失败：${(e as Error)?.message ?? String(e)}`)
  } finally {
    loading.value = false
  }
}

async function loadFilters(): Promise<void> {
  try {
    filters.value = await usage.filters()
  } catch {
    /* ignore */
  }
}

async function loadLabels(): Promise<void> {
  try {
    labels.value = await usage.providerLabels()
  } catch {
    /* ignore */
  }
}

/** 同步：默认增量解析最近 30 天；full=true 回填全部历史 */
async function doSync(full = false): Promise<void> {
  syncing.value = true
  try {
    syncInfo.value = await usage.sync(full)
    await Promise.all([loadFilters(), loadAgg(), loadLabels()])
  } catch (e: unknown) {
    toast.error(`同步失败：${(e as Error)?.message ?? String(e)}`)
  } finally {
    syncing.value = false
  }
}

let firstRun = true
let labelTimer = 0

onMounted(() => {
  // 先同步一次（拿到最新数据）；transcript 别名扫描在后台进行，延迟再拉一次补全
  void doSync()
  labelTimer = window.setTimeout(() => void loadLabels(), 3000)
})
onBeforeUnmount(() => clearTimeout(labelTimer))

// 筛选条件或维度变化 → 重新查 overview + aggregate（首次由挂载 doSync 负责，跳过避免重复请求）
watch([query, groupBy], () => {
  if (firstRun) {
    firstRun = false
    return
  }
  void loadAgg()
})

// 明细展开时拉取
watch([detailOpen, query], ([open]) => {
  if (!open) return
  detailLoading.value = true
  usage
    .records(query.value, 200, 0)
    .then((r) => (detailRows.value = r))
    .catch(() => (detailRows.value = []))
    .finally(() => (detailLoading.value = false))
})

// 客户端排序
const sortedRows = computed(() => {
  const dir = sortDir.value === 'asc' ? 1 : -1
  const pick = (r: UsageAggRow): number | string => {
    switch (sortKey.value) {
      case 'label':
        return r.label
      case 'calls':
        return r.calls
      case 'input':
        return r.inputTokens
      case 'output':
        return r.outputTokens
      case 'cache':
        return r.cacheReadTokens
      case 'total':
        return r.totalTokens
      case 'avgTps':
        return r.avgTps ?? -1
      case 'maxTps':
        return r.maxTps ?? -1
      case 'minTps':
        return r.minTps ?? -1
      case 'avgDuration':
        return r.avgDurationMs ?? -1
    }
  }
  return [...rows.value].sort((a, b) => {
    const pa = pick(a)
    const pb = pick(b)
    if (typeof pa === 'string' || typeof pb === 'string') {
      return String(pa).localeCompare(String(pb)) * dir
    }
    return (pa - pb) * dir
  })
})

const sumTotal = computed(() => rows.value.reduce((s, r) => s + r.totalTokens, 0))
const maxTotal = computed(() => rows.value.reduce((m, r) => Math.max(m, r.totalTokens), 0))

/** el-table 排序事件（sortable="custom"）→ 同步本地排序状态 */
function onSortChange({ prop, order }: { prop: string; order: 'ascending' | 'descending' | null }): void {
  if (!order) return
  sortKey.value = prop as SortKey
  sortDir.value = order === 'ascending' ? 'asc' : 'desc'
}

const providerOptions = computed(() => [
  { label: '全部', value: '' },
  ...(filters.value?.providers ?? []).map((p) => ({ label: labelProvider(p), value: p })),
])
const modelOptions = computed(() => [
  { label: '全部', value: '' },
  ...(filters.value?.models ?? []).map((m) => ({ label: m, value: m })),
])
const roleOptions = [
  { label: '全部', value: '' },
  { label: '主模型', value: 'main' },
  { label: '轻量', value: 'lite' },
  { label: '子代理', value: 'subagent' },
]
</script>

<template>
  <div class="us">
    <!-- 筛选条 -->
    <MyPanel>
      <div class="us-bar">
        <MySegmented v-model="preset" :options="PRESETS" size="small" />
        <template v-if="preset === 'custom'">
          <MyInput v-model="from" type="date" class="us-date" />
          <span class="us-faint">至</span>
          <MyInput v-model="to" type="date" class="us-date" />
        </template>
        <div class="us-spacer" />
        <MyButton
          variant="ghost"
          size="small"
          :disabled="syncing"
          title="清空本地缓存并从 zcode 用量库全量重新导入"
          @click="doSync(true)"
        >
          重新同步
        </MyButton>
        <MyButton variant="primary" size="small" :disabled="syncing" @click="doSync(false)">
          <MyIcon name="Refresh" :size="14" />
          {{ syncing ? '同步中…' : '同步' }}
        </MyButton>
      </div>

      <div class="us-filters">
        <label class="us-filter">
          <span class="us-faint">供应商</span>
          <MySelect v-model="selProvider" :options="providerOptions" :filterable="false" class="us-filter-select" />
        </label>
        <label class="us-filter">
          <span class="us-faint">模型</span>
          <MySelect v-model="selModel" :options="modelOptions" class="us-filter-select" />
        </label>
        <label class="us-filter">
          <span class="us-faint">角色</span>
          <MySelect v-model="selRole" :options="roleOptions" :filterable="false" class="us-filter-select" />
        </label>
      </div>

      <div v-if="syncInfo" class="us-syncinfo">
        共 {{ syncInfo.totalCount.toLocaleString() }} 条记录
        {{ syncInfo.scannedFiles > 0 ? ` · 本次拉取 ${syncInfo.scannedFiles} 条 · 新增 ${syncInfo.newCount}` : '' }}
        {{ syncInfo.removedCount > 0 ? ` · 对账回收 ${syncInfo.removedCount} 条（zcode 侧已删除）` : '' }}
        {{ syncInfo.minDate && syncInfo.maxDate ? ` · 数据范围 ${syncInfo.minDate} ~ ${syncInfo.maxDate}` : '' }}
      </div>
    </MyPanel>

    <!-- 汇总卡片 -->
    <MyPanel title="用量汇总">
      <template #actions>
        <span class="us-faint us-mono">{{ loading ? '更新中…' : '随筛选条件实时统计' }}</span>
      </template>
      <div class="us-stats">
        <div class="us-stat"><span>总调用</span><strong class="us-mono">{{ overview ? overview.calls.toLocaleString() : '—' }}</strong></div>
        <div class="us-stat"><span>输入 tokens</span><strong class="us-mono">{{ fmtTokens(overview?.inputTokens) }}</strong></div>
        <div class="us-stat"><span>输出 tokens</span><strong class="us-mono">{{ fmtTokens(overview?.outputTokens) }}</strong></div>
        <div class="us-stat"><span>缓存命中</span><strong class="us-mono">{{ fmtTokens(overview?.cacheReadTokens) }}</strong></div>
        <div class="us-stat"><span>总量 tokens</span><strong class="us-mono us-accent">{{ fmtTokens(overview?.totalTokens) }}</strong></div>
        <div class="us-stat"><span>平均耗时</span><strong class="us-mono">{{ fmtMs(overview?.avgDurationMs) }}</strong></div>
      </div>
    </MyPanel>

    <!-- 速度卡片 -->
    <MyPanel title="输出速度">
      <template #actions>
        <span
          class="us-faint"
          title="输出 tokens ÷ 生成耗时。正常流式取「总耗时 − 首 token 等待 TTFB」；若首 token 到达过晚（TTFB ≥ 90% 总耗时，多为非流式或中转整块下发），改用 TTFB 估算，为保守值。仅统计输出 ≥10 tokens、生成耗时 ≥100ms 且 ≤500 tok/s 的请求，计时异常的样本不参与统计。"
        >
          token/s · 口径说明 ⓘ
        </span>
      </template>
      <div class="us-stats">
        <div class="us-stat"><span>最快</span><strong class="us-mono us-tps-good">{{ fmtTps(overview?.maxTps) }}<small> tok/s</small></strong></div>
        <div class="us-stat"><span>平均</span><strong class="us-mono">{{ fmtTps(overview?.avgTps) }}<small> tok/s</small></strong></div>
        <div class="us-stat"><span>最慢</span><strong class="us-mono us-tps-warn">{{ fmtTps(overview?.minTps) }}<small> tok/s</small></strong></div>
      </div>
    </MyPanel>

    <!-- 分组聚合表 -->
    <MyPanel title="分组统计">
      <template #actions>
        <MySegmented
          :model-value="groupBy"
          :options="GROUPS.map((g) => ({ label: g.label, value: g.id }))"
          size="small"
          @update:model-value="
            (v) => {
              groupBy = v as UsageGroupBy;
              // 切维度时重置默认排序：日期 → 最新在前，其余 → 总量降序
              sortKey = v === 'date' ? 'label' : 'total';
              sortDir = 'desc';
            }
          "
        />
      </template>

      <div v-if="loading" class="ui-empty">统计中…</div>
      <MyResultState v-else-if="sortedRows.length === 0" type="empty" title="所选范围内暂无用量数据" />
      <MyDataTable v-else :data="sortedRows" @sort-change="onSortChange">
        <el-table-column :label="dimLabel(groupBy)" prop="label" sortable="custom" min-width="160" show-overflow-tooltip>
          <template #default="{ row }">
            {{ groupBy === 'provider' ? labelProvider(row.label) : prettyKey(row.label) }}
          </template>
        </el-table-column>
        <el-table-column label="调用" prop="calls" sortable="custom" align="right" width="90">
          <template #default="{ row }">
            <span class="us-mono">{{ row.calls.toLocaleString() }}</span>
          </template>
        </el-table-column>
        <el-table-column label="输入" prop="input" sortable="custom" align="right" width="90">
          <template #default="{ row }"><span class="us-mono us-faint">{{ formatUnits(row.inputTokens) }}</span></template>
        </el-table-column>
        <el-table-column label="输出" prop="output" sortable="custom" align="right" width="90">
          <template #default="{ row }"><span class="us-mono">{{ formatUnits(row.outputTokens) }}</span></template>
        </el-table-column>
        <el-table-column label="缓存" prop="cache" sortable="custom" align="right" width="90">
          <template #default="{ row }"><span class="us-mono us-faint">{{ formatUnits(row.cacheReadTokens) }}</span></template>
        </el-table-column>
        <el-table-column label="总量" prop="total" sortable="custom" align="right" width="100">
          <template #default="{ row }"><span class="us-mono us-strong">{{ formatUnits(row.totalTokens) }}</span></template>
        </el-table-column>
        <el-table-column label="均速" prop="avgTps" sortable="custom" align="right" width="80">
          <template #default="{ row }"><span class="us-mono">{{ fmtTps(row.avgTps) }}</span></template>
        </el-table-column>
        <el-table-column label="最快" prop="maxTps" sortable="custom" align="right" width="80">
          <template #default="{ row }"><span class="us-mono us-tps-good">{{ fmtTps(row.maxTps) }}</span></template>
        </el-table-column>
        <el-table-column label="最慢" prop="minTps" sortable="custom" align="right" width="80">
          <template #default="{ row }"><span class="us-mono us-tps-warn">{{ fmtTps(row.minTps) }}</span></template>
        </el-table-column>
        <el-table-column label="均耗时" prop="avgDuration" sortable="custom" align="right" width="90">
          <template #default="{ row }"><span class="us-mono us-faint">{{ fmtMs(row.avgDurationMs) }}</span></template>
        </el-table-column>
        <el-table-column label="占比" align="center" width="140">
          <template #default="{ row }">
            <div
              class="us-bar-share"
              :style="{
                '--share': `${Math.max(2, sumTotal > 0 ? (row.totalTokens / sumTotal) * 100 : 0).toFixed(1)}%`,
                '--share-color': usageColor(maxTotal > 0 ? (row.totalTokens / maxTotal) * 100 : 0),
              }"
            >
              <div class="us-share-fill" />
              <span class="us-share-label">{{ (sumTotal > 0 ? (row.totalTokens / sumTotal) * 100 : 0).toFixed(1) }}%</span>
            </div>
          </template>
        </el-table-column>
      </MyDataTable>
    </MyPanel>

    <!-- 明细 -->
    <MyPanel title="调用明细">
      <template #actions>
        <MyButton variant="ghost" size="small" @click="detailOpen = !detailOpen">
          {{ detailOpen ? '收起' : '展开最近 200 条' }}
        </MyButton>
      </template>
      <div v-if="!detailOpen" class="ui-empty us-faint">点击右上角展开查看明细</div>
      <div v-else-if="detailLoading" class="ui-empty">加载中…</div>
      <MyResultState v-else-if="detailRows.length === 0" type="empty" title="暂无明细" />
      <MyDataTable v-else :data="detailRows">
        <el-table-column label="时间" min-width="100">
          <template #default="{ row }"><span class="us-mono us-faint">{{ fmtTime(row.startedAt) }}</span></template>
        </el-table-column>
        <el-table-column label="供应商" min-width="120" show-overflow-tooltip>
          <template #default="{ row }">
            <span :title="row.providerId">{{ labelProvider(row.providerId) }}</span>
          </template>
        </el-table-column>
        <el-table-column prop="modelId" label="模型" min-width="160" show-overflow-tooltip />
        <el-table-column label="角色" width="90">
          <template #default="{ row }">
            <span v-if="row.role" class="us-role">{{ row.role }}</span>
            <span v-else class="us-faint">—</span>
          </template>
        </el-table-column>
        <el-table-column label="输入" align="right" width="90">
          <template #default="{ row }"><span class="us-mono us-faint">{{ formatUnits(row.inputTokens) }}</span></template>
        </el-table-column>
        <el-table-column label="输出" align="right" width="90">
          <template #default="{ row }"><span class="us-mono">{{ formatUnits(row.outputTokens) }}</span></template>
        </el-table-column>
        <el-table-column label="缓存" align="right" width="90">
          <template #default="{ row }"><span class="us-mono us-faint">{{ formatUnits(row.cacheReadTokens) }}</span></template>
        </el-table-column>
        <el-table-column label="总量" align="right" width="100">
          <template #default="{ row }"><span class="us-mono us-strong">{{ formatUnits(row.totalTokens) }}</span></template>
        </el-table-column>
        <el-table-column label="耗时" align="right" width="90">
          <template #default="{ row }"><span class="us-mono us-faint">{{ fmtMs(row.durationMs) }}</span></template>
        </el-table-column>
        <el-table-column label="速度" align="right" width="80">
          <template #default="{ row }"><span class="us-mono">{{ fmtTps(row.tps) }}</span></template>
        </el-table-column>
        <el-table-column label="结束" min-width="80">
          <template #default="{ row }"><span class="us-faint">{{ row.finishReason ?? '—' }}</span></template>
        </el-table-column>
      </MyDataTable>
    </MyPanel>
  </div>
</template>

<style scoped>
.us {
  display: grid;
  gap: var(--ui-gap);
}
.us-bar {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}
.us-spacer {
  flex: 1;
}
.us-date {
  width: 150px;
}
.us-filters {
  display: flex;
  gap: 14px;
  flex-wrap: wrap;
  margin-top: 12px;
}
.us-filter {
  display: flex;
  flex-direction: column;
  gap: 4px;
  font-size: 12px;
}
.us-filter-select {
  width: 170px;
}
.us-syncinfo {
  margin-top: 12px;
  color: var(--text-tertiary);
  font-family: var(--font-mono);
  font-size: 11px;
}
.us-faint {
  color: var(--text-tertiary);
  font-size: 12px;
}
.us-mono {
  font-family: var(--font-mono);
  font-variant-numeric: tabular-nums;
}
.us-stats {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
  gap: 14px;
}
.us-stat {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.us-stat > span {
  color: var(--text-tertiary);
  font-size: 12px;
}
.us-stat > strong {
  font-family: var(--font-mono);
  font-size: 20px;
  font-weight: 600;
  font-variant-numeric: tabular-nums;
}
.us-stat > strong small {
  font-size: 12px;
  font-weight: 400;
  color: var(--text-tertiary);
}
.us-accent {
  color: var(--accent);
}
.us-tps-good {
  color: var(--success);
}
.us-tps-warn {
  color: var(--warning);
}
.us-strong {
  font-weight: 600;
}
.us-role {
  font-size: 11px;
  color: var(--text-secondary);
  border: 1px solid var(--border-base);
  border-radius: 999px;
  padding: 0 8px;
  line-height: 18px;
}
.us-bar-share {
  position: relative;
  width: 124px;
  height: 18px;
  border-radius: 999px;
  background: var(--border-subtle);
  overflow: hidden;
}
.us-share-fill {
  height: 100%;
  width: var(--share);
  border-radius: 999px;
  background: var(--share-color);
}
.us-share-label {
  position: absolute;
  inset: 0;
  display: grid;
  place-items: center;
  font-family: var(--font-mono);
  font-size: 10px;
  color: var(--text-primary);
  font-variant-numeric: tabular-nums;
}
</style>
