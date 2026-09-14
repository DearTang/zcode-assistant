<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { MyBadge, MyButton, MyIcon, MyInput, MyPanel, MySelect, MyToggle, MyResultState } from 'myui'
import RestartBar from '@/components/RestartBar.vue'
import ProviderQuotaRow from './quotaRow.vue'
import ExportPreviewModal from './modals/ExportPreviewModal.vue'
import ImportPreviewModal from './modals/ImportPreviewModal.vue'
import ContextConfirmModal from './modals/ContextConfirmModal.vue'
import ProviderAddModal from './modals/ProviderAddModal.vue'
import ProviderEditModal from './modals/ProviderEditModal.vue'
import { RESTART_HINT } from './shared'
import {
  importer,
  exporter,
  ompSync,
  models,
  zcode,
  quota,
  events,
  health,
} from '@/api'
import type { ExportOutcome, ExportPreview, ImportResult, ProviderPreview } from '@/api'
import { toast } from '@/composables/toast'
import type { UsageDisplayMode, ZcProvider, ZcodeConfig, ZcodeSetting, QuotaOverview } from '@/types'

defineOptions({ name: 'ModelsView' })

const props = withDefaults(
  defineProps<{
    /** 模型用量展示方案（与总览/悬浮球同源）：已用 / 剩余 */
    usageDisplay?: UsageDisplayMode
  }>(),
  { usageDisplay: 'used' },
)

const config = ref<ZcodeConfig | null>(null)
const setting = ref<ZcodeSetting | null>(null)
const selected = ref<string | null>(null)
const busy = ref(false)
const showAddModal = ref(false)
// 导入配置
const impSource = ref('opencode')
const impPath = ref('')
const impResults = ref<ImportResult[] | null>(null)
// 预览弹窗：解析出的待导入 provider 列表（null=弹窗关闭）
const impPreview = ref<ProviderPreview[] | null>(null)
// 「重新获取上下文」开关：仅当次导入生效
const impRefetchCtx = ref(false)
// 未命中模型的上下文确认弹窗
const ctxConfirm = ref<{ ids: string[]; unmatched: string[] } | null>(null)
// 反向同步（zcode → cc-switch）
const expPreview = ref<ExportPreview[] | null>(null)
const expOutcome = ref<ExportOutcome | null>(null)
// 反向同步（zcode → Oh My Pi）
const ompExpPreview = ref<ExportPreview[] | null>(null)
const ompExpOutcome = ref<ExportOutcome | null>(null)
// 本次会话新导入的 provider key（仅内存，用于 NEW 标记）
const newKeys = ref(new Set<string>())
// 双击打开的供应商编辑弹窗 key
const editKey = ref<string | null>(null)
// 主供应商 key
const primary = ref<string | null>(null)
// 每个供应商的最新配额（key -> QuotaOverview | null）
const quotaMap = ref<Record<string, QuotaOverview | null>>({})
// 拖拽排序：gripDown=手柄按下标记（同步变量，避免响应式延迟导致 draggable 时序错位）
const dragIdx = ref<number | null>(null)
let gripDown = false

const impSourceOptions = [
  { label: 'opencode (opencode.json)', value: 'opencode' },
  { label: 'Claude Code (settings.json)', value: 'claude' },
  { label: 'Codex (config.toml)', value: 'codex' },
  { label: 'ZCode (config.json)', value: 'zcode' },
  { label: 'Oh My Pi (models.json)', value: 'omp' },
]

async function reload(): Promise<void> {
  try {
    const [c, s, p] = await Promise.all([zcode.getConfig(), zcode.getSetting(), models.getPrimary()])
    config.value = c
    setting.value = s
    primary.value = p ?? null
    selected.value =
      selected.value ?? Object.keys(c.provider).find((k) => !k.startsWith('builtin:')) ?? null
  } catch (e: unknown) {
    toast.error(typeof e === 'string' ? e : '读取 zcode 配置失败')
  }
}

const unlistenFns: (() => void)[] = []
let quotaTimer = 0

onMounted(() => {
  void reload()
  // 后端 select_provider 广播「当前模型已切换」→ 多窗口同步刷新当前选中态
  events.onModelSwitched(() => void reload()).then((fn) => unlistenFns.push(fn))
})
onBeforeUnmount(() => {
  unlistenFns.forEach((fn) => fn())
  clearInterval(quotaTimer)
})

const family = computed(() => setting.value?.providerFamilyDomain)
const currentKey = computed(() =>
  family.value ? setting.value?.modelProviderFamilySelectedKeys?.[family.value] : undefined,
)
// setting.json 中选中值可能带 "coding-plan:" 前缀
function isCurrentKey(key: string): boolean {
  return currentKey.value === key || currentKey.value === `coding-plan:${key}`
}

// 智谱账号对应的 builtin 供应商：优先取当前 family 选中的 builtin，
// 兜底取启用的、模型最多的 builtin。固定展示在供应商列表首位。
const builtinKey = computed(() => {
  const cfg = config.value
  if (!cfg) return undefined
  if (currentKey.value) {
    const k = currentKey.value.startsWith('coding-plan:') ? currentKey.value.slice('coding-plan:'.length) : currentKey.value
    if (k.startsWith('builtin:') && cfg.provider[k]) return k
  }
  const fb = Object.entries(cfg.provider)
    .filter(([k, p]) => k.startsWith('builtin:') && !p.systemDisabledReason && p.enabled !== false)
    .sort((a, b) => Object.keys(b[1].models).length - Object.keys(a[1].models).length)[0]
  return fb?.[0]
})
const builtinEntry = computed<[string, ZcProvider] | undefined>(() => {
  const k = builtinKey.value
  return k && config.value ? [k, config.value.provider[k]] : undefined
})

// 每 60s 轮询供应商配额：智谱账号（builtin）走内置 Coding Plan 接口；
// Token Plan 供应商按 baseURL 自动识别查询，其余走用量模板
watch(
  [config, builtinKey],
  async () => {
    const cfg = config.value
    if (!cfg) return
    const customKeys = Object.keys(cfg.provider).filter((k) => !k.startsWith('builtin:'))
    let cancelled = false
    const poll = async () => {
      const entries = await Promise.all([
        ...customKeys.map(async (k): Promise<[string, QuotaOverview | null]> => {
          try {
            return [k, await quota.getProviderQuota(k)]
          } catch {
            return [k, null]
          }
        }),
        ...(builtinKey.value
          ? [
              (async (): Promise<[string, QuotaOverview | null]> => {
                try {
                  return [builtinKey.value!, await quota.getCodingPlan()]
                } catch {
                  return [builtinKey.value!, null]
                }
              })(),
            ]
          : []),
      ])
      if (!cancelled) quotaMap.value = Object.fromEntries(entries)
    }
    void poll()
    clearInterval(quotaTimer)
    quotaTimer = window.setInterval(() => void poll(), 60_000)
    return () => {
      cancelled = true
    }
  },
  { immediate: true },
)
onBeforeUnmount(() => {
  // 二次清理（watch 内的闭包定时器由上方 onBeforeUnmount 清理）
})

/** 智谱内置供应商为「智谱CodingPlan」可配额语义，保留原视觉徽标文案 */
const BIGMODEL_GREEN = 'oklch(0.7 0.17 160 / 0.15)'

// ⚡ 验证指定供应商配置（GET /models 免费探测，绕过冷却）
async function checkProvider(key: string, name: string): Promise<void> {
  toast.success(`正在验证「${name}」配置…`)
  try {
    const r = await health.checkProvider(key)
    if (r.ok) toast.success(`「${name}」配置有效：${r.message}`)
    else toast.warning(`「${name}」配置验证失败：${r.message}`)
  } catch (e: unknown) {
    toast.error(typeof e === 'string' ? e : '验证失败')
  }
}

// ⭐ 设为/取消主供应商（全局唯一）：立即触发 App 全局配额刷新
function togglePrimary(key: string, name: string): void {
  const v = primary.value !== key
  models
    .setPrimary(key, v)
    .then(() => {
      events.emitRefreshRequested()
      primary.value = v ? key : null
      toast.success(
        v ? `已设「${name}」为主供应商，总览 / 悬浮窗 / 托盘将展示其配额` : '已取消主供应商，展示回退自动识别',
      )
    })
    .catch(() => toast.error('设置主供应商失败'))
}

// 隐藏系统未授权的 provider；builtin 由 builtinEntry 单独置顶展示
const providers = computed(() =>
  config.value
    ? Object.entries(config.value.provider).filter(([key, p]) => !p.systemDisabledReason && !key.startsWith('builtin:'))
    : [],
)

async function run(fn: () => Promise<void>): Promise<void> {
  busy.value = true
  try {
    await fn()
    await reload()
  } catch (e: unknown) {
    toast.error(typeof e === 'string' ? e : '操作失败')
  } finally {
    busy.value = false
  }
}

// 点「导入」→ 先预览解析（只读不写），弹窗勾选后再执行导入
async function handleImport(): Promise<void> {
  busy.value = true
  try {
    const items =
      impSource.value === 'omp'
        ? await ompSync.importPreview(impPath.value.trim() || undefined)
        : await importer.preview(impSource.value, impPath.value.trim() || undefined)
    impPreview.value = items
  } catch (e: unknown) {
    toast.error(typeof e === 'string' ? e : '解析配置失败')
  } finally {
    busy.value = false
  }
}

// 实际执行导入并处理结果（refetch=「重新获取上下文」；overrides=未命中模型的确认值）
async function runImport(ids: string[], refetch: boolean, overrides?: Record<string, number>): Promise<void> {
  busy.value = true
  try {
    const results =
      impSource.value === 'omp'
        ? await ompSync.importProviders(impPath.value.trim() || undefined, ids)
        : await importer.from(impSource.value, impPath.value.trim() || undefined, ids, refetch, overrides)
    impPreview.value = null
    impResults.value = results
    // 记录新导入的 provider key（仅 success），用于本次会话 NEW 标记
    const added = results.filter((r) => r.status === 'success' && r.providerKey).map((r) => r.providerKey)
    const updated = results.filter((r) => r.status === 'updated')
    if (added.length > 0) {
      const next = new Set(newKeys.value)
      added.forEach((k) => next.add(k))
      newKeys.value = next
    }
    await reload()
    if (added.length > 0) {
      toast.success(`成功导入 ${added.length} 个供应商`)
      toast.warning(RESTART_HINT)
    } else if (updated.length > 0) {
      toast.success(`已覆盖更新 ${updated.length} 个供应商`)
      toast.warning(RESTART_HINT)
    } else {
      toast.error('未成功导入任何供应商')
    }
  } catch (e: unknown) {
    toast.error(typeof e === 'string' ? e : '导入失败')
  } finally {
    busy.value = false
    // 「重新获取上下文」仅当次生效：导入完成后自动还原为关
    impRefetchCtx.value = false
  }
}

// 预览弹窗确认：开关关闭直接导入；开启先目录匹配，未命中的弹窗逐个确认后再导入
async function handleImportConfirm(ids: string[]): Promise<void> {
  if (impSource.value === 'omp' || !impRefetchCtx.value) {
    await runImport(ids, false)
    return
  }
  busy.value = true
  try {
    const res = await importer.resolveContexts(impSource.value, impPath.value.trim() || undefined, ids)
    busy.value = false
    if (res.unmatched.length === 0) {
      await runImport(ids, true)
    } else {
      ctxConfirm.value = { ids, unmatched: res.unmatched }
    }
  } catch (e: unknown) {
    toast.error(typeof e === 'string' ? e : '解析模型上下文失败')
  } finally {
    busy.value = false
  }
}

async function handlePickFile(): Promise<void> {
  try {
    const picked = await importer.pickFile(impSource.value, impPath.value || undefined)
    if (picked) impPath.value = picked
  } catch {
    /* 用户取消或不可用，忽略 */
  }
}

// 反向同步：预览 zcode 可导出的供应商（标记 cc-switch 侧覆盖关系），不写入
async function handleExport(): Promise<void> {
  busy.value = true
  expOutcome.value = null
  try {
    expPreview.value = await exporter.preview()
  } catch (e: unknown) {
    toast.error(typeof e === 'string' ? e : '解析 zcode 配置失败')
  } finally {
    busy.value = false
  }
}

// 同步确认：只导出勾选的供应商（统一写 cc-switch 的 opencode 供应商组）
async function handleExportConfirm(ids: string[]): Promise<void> {
  busy.value = true
  try {
    const outcome = await exporter.to(ids)
    expPreview.value = null
    expOutcome.value = outcome
    const added = outcome.results.filter((r) => r.status === 'success')
    const updated = outcome.results.filter((r) => r.status === 'updated')
    if (added.length > 0) toast.success(`已新增 ${added.length} 个供应商到 cc-switch`)
    if (updated.length > 0) toast.success(`已覆盖更新 ${updated.length} 个供应商`)
    if (outcome.warning) toast.warning(outcome.warning)
    if (added.length === 0 && updated.length === 0) toast.error('未成功导出任何供应商')
  } catch (e: unknown) {
    toast.error(typeof e === 'string' ? e : '同步失败')
  } finally {
    busy.value = false
  }
}

async function handleOmpExport(): Promise<void> {
  busy.value = true
  ompExpOutcome.value = null
  try {
    ompExpPreview.value = await ompSync.exportPreview()
  } catch (e: unknown) {
    toast.error(typeof e === 'string' ? e : '解析 Oh My Pi 配置失败')
  } finally {
    busy.value = false
  }
}

async function handleOmpExportConfirm(ids: string[]): Promise<void> {
  busy.value = true
  try {
    const outcome = await ompSync.exportProviders(ids)
    ompExpPreview.value = null
    ompExpOutcome.value = outcome
    const added = outcome.results.filter((r) => r.status === 'success')
    const updated = outcome.results.filter((r) => r.status === 'updated')
    if (added.length > 0) toast.success(`已新增 ${added.length} 个供应商到 Oh My Pi`)
    if (updated.length > 0) toast.success(`已覆盖更新 ${updated.length} 个供应商`)
    if (outcome.warning) toast.warning(outcome.warning)
    if (added.length === 0 && updated.length === 0) toast.error('未成功同步任何供应商')
  } catch (e: unknown) {
    toast.error(typeof e === 'string' ? e : '同步 Oh My Pi 失败')
  } finally {
    busy.value = false
  }
}

function handleRemove(key: string): void {
  void run(async () => {
    await models.removeProvider(key)
    if (selected.value === key) selected.value = null
    if (editKey.value === key) editKey.value = null
    toast.success('已删除供应商')
    toast.warning(RESTART_HINT)
  })
}

// 拖拽排序：把 dragIdx 项移到 targetIdx 位置，写回 config.json
async function handleReorder(targetIdx: number): Promise<void> {
  const src = dragIdx.value
  dragIdx.value = null
  gripDown = false
  if (src === null || src === targetIdx) return
  const keys = providers.value.map(([k]) => k)
  const [moved] = keys.splice(src, 1)
  keys.splice(targetIdx, 0, moved)
  busy.value = true
  try {
    await models.reorderProviders(keys)
    await reload()
    toast.success('供应商顺序已更新')
  } catch (e: unknown) {
    toast.error(typeof e === 'string' ? e : '排序失败')
  } finally {
    busy.value = false
  }
}

// 弹窗中操作的 provider 数据
const editProvider = computed(() => (editKey.value ? config.value?.provider[editKey.value] : undefined))

interface ResultLine {
  status: string
  name: string
  message: string
}
function toLines(outcome: ExportOutcome | ImportResult[]): ResultLine[] {
  const results = Array.isArray(outcome) ? outcome : outcome.results
  return results.map((r) => ({ status: r.status, name: r.name, message: r.message }))
}
const impLines = computed(() => (impResults.value ? toLines(impResults.value) : []))
const impSummary = computed(() => {
  if (!impResults.value) return null
  const c = (s: string) => impResults.value!.filter((r) => r.status === s).length
  return { succ: c('success'), upd: c('updated'), dup: c('duplicate'), fail: c('failed') }
})
const expLines = computed(() => (expOutcome.value ? toLines(expOutcome.value) : []))
const ompExpLines = computed(() => (ompExpOutcome.value ? toLines(ompExpOutcome.value) : []))

function lineMark(status: string): string {
  return status === 'failed' ? '✕' : status === 'updated' ? '↻' : status === 'duplicate' ? '↻' : '✓'
}
function lineClass(status: string): string {
  return status === 'failed' ? 'bad' : status === 'updated' ? 'hot' : 'dim'
}
</script>

<template>
  <div class="md">
    <RestartBar hint="供应商 / 模型变更后需重启 zcode 生效" />

    <!-- 从其他工具导入配置 -->
    <MyPanel title="导入配置">
      <p class="md-desc">
        从 opencode / Claude Code / Codex / Oh My Pi 配置文件导入 provider。留空路径用各工具默认位置；也可手动指定。
      </p>
      <div class="md-toolbar">
        <MySelect v-model="impSource" :options="impSourceOptions" :filterable="false" class="md-source" />
        <div class="md-filepick" title="点击选择配置文件" @click="handlePickFile">
          <MyIcon name="Folder" :size="14" class="md-filepick-icon" />
          <span class="md-filepick-text" :class="{ filled: impPath }">
            {{ impPath || '点击选择配置文件，或留空使用默认路径' }}
          </span>
        </div>
        <button v-if="impPath" class="md-icon-btn" type="button" title="清除路径" @click="impPath = ''">
          <MyIcon name="Close" :size="13" />
        </button>
        <label
          v-if="impSource !== 'omp'"
          class="md-refetch"
          title="开启后本次导入按 OpenRouter 目录 / 内置规格表匹配真实上下文并覆盖旧值；未命中的模型弹窗逐个确认（默认 200k，可修改）。仅当次生效，导入完成后自动关闭"
        >
          <MyToggle v-model="impRefetchCtx" />
          重新获取上下文
        </label>
        <MyButton variant="primary" size="small" :disabled="busy" :loading="busy" @click="handleImport">导入</MyButton>
      </div>

      <!-- 导入结果摘要 -->
      <div v-if="impSummary" class="md-summary">
        <div class="md-summary-head">
          <span class="ok">✓ 成功 {{ impSummary.succ }}</span>
          <span class="ok">↻ 覆盖 {{ impSummary.upd }}</span>
          <span v-if="impSummary.dup > 0" class="dim">↻ 重复 {{ impSummary.dup }}</span>
          <span class="bad">✕ 失败 {{ impSummary.fail }}</span>
        </div>
        <div v-if="impLines.length > 0" class="md-lines">
          <div v-for="(r, i) in impLines" :key="`${r.name}-${i}`" class="md-line">
            <span :class="lineClass(r.status)">{{ lineMark(r.status) }}</span>
            <span class="md-line-name">{{ r.name }}</span>
            <span class="dim">— {{ r.message }}</span>
          </div>
        </div>
      </div>
    </MyPanel>

    <!-- 反向同步：zcode 配置导出到 cc-switch -->
    <MyPanel title="同步到 cc-switch">
      <p class="md-desc">
        把 zcode 的自定义供应商（含模型与上下文限制）同步到 cc-switch（其 opencode
        供应商组，app_type='opencode'，切换供应商时由 cc-switch 落到 opencode.json）。
        baseURL + apiKey 一致的条目覆盖更新，其余新增；写入前自动备份数据库（.bak）。
      </p>
      <MyButton variant="primary" size="small" :disabled="busy" :loading="busy" @click="handleExport">同步到 cc-switch</MyButton>

      <div v-if="expOutcome && expOutcome.results.length > 0" class="md-summary">
        <div class="md-summary-head">
          <span class="ok">✓ 新增 {{ expOutcome.results.filter((r) => r.status === 'success').length }}</span>
          <span class="ok">↻ 覆盖 {{ expOutcome.results.filter((r) => r.status === 'updated').length }}</span>
          <span class="bad">✕ 失败 {{ expOutcome.results.filter((r) => r.status === 'failed').length }}</span>
        </div>
        <div v-if="expOutcome.warning" class="md-warn">⚠ {{ expOutcome.warning }}</div>
        <div v-if="expLines.length > 0" class="md-lines">
          <div v-for="(r, i) in expLines" :key="`${r.name}-${i}`" class="md-line">
            <span :class="lineClass(r.status)">{{ lineMark(r.status) }}</span>
            <span class="md-line-name">{{ r.name }}</span>
            <span class="dim">— {{ r.message }}</span>
          </div>
        </div>
      </div>
    </MyPanel>

    <!-- 同步到 Oh My Pi -->
    <MyPanel title="同步到 Oh My Pi">
      <p class="md-desc">
        把 zcode 的自定义供应商和模型同步到 Oh My Pi 原生
        <code>~/.pi/agent/models.json</code>。baseUrl + apiKey 一致的条目覆盖更新，其余新增；写入前自动创建
        <code>models.json.bak</code>。不会修改 <code>settings.json</code> 的默认供应商或模型。
      </p>
      <MyButton variant="primary" size="small" :disabled="busy" :loading="busy" @click="handleOmpExport">同步到 Oh My Pi</MyButton>

      <div v-if="ompExpOutcome && ompExpOutcome.results.length > 0" class="md-summary">
        <div class="md-summary-head">
          <span class="ok">✓ 新增 {{ ompExpOutcome.results.filter((r) => r.status === 'success').length }}</span>
          <span class="ok">↻ 覆盖 {{ ompExpOutcome.results.filter((r) => r.status === 'updated').length }}</span>
          <span class="bad">✕ 失败 {{ ompExpOutcome.results.filter((r) => r.status === 'failed').length }}</span>
        </div>
        <div v-if="ompExpOutcome.warning" class="md-warn">⚠ {{ ompExpOutcome.warning }}</div>
        <div v-if="ompExpLines.length > 0" class="md-lines">
          <div v-for="(r, i) in ompExpLines" :key="`${r.name}-${i}`" class="md-line">
            <span :class="lineClass(r.status)">{{ lineMark(r.status) }}</span>
            <span class="md-line-name">{{ r.name }}</span>
            <span class="dim">— {{ r.message }}</span>
          </div>
        </div>
      </div>
    </MyPanel>

    <!-- 供应商列表 -->
    <MyPanel title="供应商">
      <template #actions>
        <MyButton variant="primary" size="small" @click="showAddModal = true">
          <MyIcon name="Plus" :size="14" /> 添加供应商
        </MyButton>
      </template>
      <p class="md-desc faint">单击选中，双击编辑，拖动 ⠿ 手柄调整顺序（智谱账号固定首位）</p>

      <div class="md-list">
        <!-- 智谱账号（builtin）：固定第一位，不可拖拽/禁用/删除/改信息，仅可管理其模型 -->
        <div
          v-if="builtinEntry"
          class="md-provider builtin"
          :class="{ selected: selected === builtinEntry[0] }"
          :style="{ opacity: builtinEntry[1].enabled !== false ? 1 : 0.5 }"
          title="双击管理套餐模型（供应商信息不可修改）"
          @click="selected = builtinEntry[0]"
          @dblclick="editKey = builtinEntry[0]"
        >
          <div class="md-provider-head">
            <div class="md-provider-main">
              <span class="md-pin" title="固定第一位">📌</span>
              <MyIcon v-if="isCurrentKey(builtinEntry[0])" name="Check" :size="14" class="md-check" />
              <div class="md-provider-copy">
                <span class="md-provider-name">{{ builtinEntry[1].name }}</span>
                <span class="md-provider-key">标识 {{ builtinEntry[0] }}</span>
              </div>
              <MyBadge value="智谱CodingPlan" type="success" title="智谱 Coding Plan 订阅（登录态托管）" />
              <MyBadge v-if="primary === builtinEntry[0]" value="主供应商" type="success" title="主供应商：总览 / 悬浮窗 / 托盘展示此供应商的配额" />
              <MyBadge :value="builtinEntry[1].kind" type="info" />
            </div>
            <div class="md-provider-ops">
              <span class="md-faint">模型数：{{ Object.keys(builtinEntry[1].models).length }}</span>
              <button
                class="md-icon-btn"
                :class="{ star: primary === builtinEntry[0] }"
                type="button"
                :title="primary === builtinEntry[0] ? '取消主供应商' : '设为主供应商：总览 / 悬浮窗 / 托盘展示此供应商的配额'"
                @click.stop="togglePrimary(builtinEntry[0], builtinEntry[1].name)"
              >
                <svg width="13" height="13" viewBox="0 0 24 24" :fill="primary === builtinEntry[0] ? 'currentColor' : 'none'"
                  stroke="currentColor" stroke-width="2" stroke-linejoin="round">
                  <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" />
                </svg>
              </button>
              <button
                class="md-icon-btn"
                type="button"
                title="验证此供应商配置（GET /models，只验证 baseURL/apiKey 连通性，不消耗 token）"
                @click.stop="checkProvider(builtinEntry[0], builtinEntry[1].name)"
              >
                <MyIcon name="Zap" :size="13" />
              </button>
            </div>
          </div>
          <ProviderQuotaRow
            :quota="quotaMap[builtinEntry[0]]"
            :is-bigmodel="(builtinEntry[1].options?.baseURL ?? '').toLowerCase().includes('bigmodel')"
            :usage-display="usageDisplay"
          />
        </div>

        <MyResultState v-if="providers.length === 0 && !builtinEntry" type="empty" title="未读取到 provider（请确认 zcode 已配置）" />

        <!-- 用户供应商 -->
        <div
          v-for="([key, p], idx) in providers"
          :key="key"
          class="md-provider"
          :class="{ selected: selected === key, dragging: dragIdx === idx }"
          :style="{ opacity: dragIdx === idx ? 0.5 : p.enabled !== false ? 1 : 0.5 }"
          title="双击编辑"
          draggable="true"
          @click="selected = key"
          @dblclick="editKey = key"
          @dragstart="
            (e) => {
              // 只有手柄按下时才允许拖拽（避免误触单击/双击触发的拖拽）
              if (!gripDown) {
                e.preventDefault();
                return;
              }
              dragIdx = idx;
            }
          "
          @dragover.prevent
          @drop="handleReorder(idx)"
          @dragend="
            () => {
              gripDown = false;
              dragIdx = null;
            }
          "
        >
          <div class="md-provider-head">
            <div class="md-provider-main">
              <span
                class="md-grip"
                title="拖动调整顺序"
                @mousedown="gripDown = true"
                @mouseup="gripDown = false"
              >⠿</span>
              <MyIcon v-if="isCurrentKey(key)" name="Check" :size="14" class="md-check" />
              <div class="md-provider-copy">
                <span class="md-provider-name">{{ p.name }}</span>
                <span class="md-provider-key">标识 {{ key }}</span>
              </div>
              <MyBadge v-if="newKeys.has(key)" value="NEW" type="primary" />
              <MyBadge v-if="primary === key" value="主供应商" type="success" title="主供应商：总览 / 悬浮窗 / 托盘展示此供应商的配额" />
              <MyBadge :value="p.kind" type="info" />
              <MyBadge v-if="p.source === 'custom'" value="自定义" type="primary" />
            </div>
            <div class="md-provider-ops">
              <span class="md-faint" title="该供应商当前配置的模型数">模型数：{{ Object.keys(p.models).length }}</span>
              <button
                class="md-icon-btn"
                :class="{ star: primary === key }"
                type="button"
                :title="primary === key ? '取消主供应商' : '设为主供应商：总览 / 悬浮窗 / 托盘展示此供应商的配额'"
                @click.stop="togglePrimary(key, p.name)"
              >
                <svg width="13" height="13" viewBox="0 0 24 24" :fill="primary === key ? 'currentColor' : 'none'"
                  stroke="currentColor" stroke-width="2" stroke-linejoin="round">
                  <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" />
                </svg>
              </button>
              <button
                class="md-icon-btn"
                type="button"
                title="验证此供应商配置（GET /models，只验证 baseURL/apiKey 连通性，不消耗 token）"
                @click.stop="checkProvider(key, p.name)"
              >
                <MyIcon name="Zap" :size="13" />
              </button>
              <MyToggle
                :model-value="p.enabled !== false"
                :title="p.enabled !== false ? '已启用' : '已禁用'"
                @click.stop
                @update:model-value="(v) => run(async () => { await models.setProviderEnabled(key, v === true) })"
              />
              <button class="md-icon-btn" type="button" title="删除供应商" @click.stop="handleRemove(key)">
                <MyIcon name="Trash" :size="13" />
              </button>
            </div>
          </div>
          <ProviderQuotaRow
            :quota="quotaMap[key]"
            :is-bigmodel="(p.options?.baseURL ?? '').toLowerCase().includes('bigmodel')"
            :usage-display="usageDisplay"
          />
        </div>
      </div>
    </MyPanel>

    <!-- 双击供应商 → 编辑弹窗 -->
    <ProviderEditModal
      v-if="editKey && editProvider"
      :provider-key="editKey"
      :provider="editProvider"
      :is-current="isCurrentKey(editKey)"
      :is-builtin="editKey === builtinKey"
      :busy="busy"
      @close="editKey = null"
      @run="run"
    />

    <ProviderAddModal
      v-if="showAddModal"
      @close="showAddModal = false"
      @added="
        (m) => {
          showAddModal = false;
          toast.success(m);
          toast.warning(RESTART_HINT);
          reload();
        }
      "
    />

    <!-- 导入预览：勾选要导入的供应商后再执行写入 -->
    <ImportPreviewModal
      v-if="impPreview"
      :items="impPreview"
      :busy="busy"
      @close="impPreview = null"
      @confirm="handleImportConfirm"
    />

    <!-- 未命中模型的上下文确认：逐个填写（默认 200k）后继续导入 -->
    <ContextConfirmModal
      v-if="ctxConfirm"
      :models="ctxConfirm.unmatched"
      :busy="busy"
      @close="ctxConfirm = null"
      @confirm="
        (overrides) => {
          const ids = ctxConfirm!.ids;
          ctxConfirm = null;
          runImport(ids, true, overrides);
        }
      "
    />

    <!-- 同步预览：勾选要导出的供应商后再执行写入 -->
    <ExportPreviewModal v-if="expPreview" :items="expPreview" :busy="busy" @close="expPreview = null" @confirm="handleExportConfirm" />
    <ExportPreviewModal
      v-if="ompExpPreview"
      title="同步到 Oh My Pi"
      :items="ompExpPreview"
      :busy="busy"
      @close="ompExpPreview = null"
      @confirm="handleOmpExportConfirm"
    />
  </div>
</template>

<style scoped>
.md {
  display: grid;
  gap: var(--ui-gap);
}
.md-desc {
  margin: 0 0 10px;
  color: var(--text-secondary);
  font-size: 12px;
  line-height: 1.7;
}
.md-desc.faint {
  color: var(--text-tertiary);
  font-size: 11px;
}
.md-desc code {
  font-family: var(--font-mono);
  font-size: 11px;
  background: var(--bg-elevated);
  padding: 1px 5px;
  border-radius: 4px;
}
.md-toolbar {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
  align-items: center;
}
.md-source {
  width: 250px;
}
.md-filepick {
  height: 30px;
  width: 320px;
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 0 8px;
  border: 1px solid var(--border-base);
  border-radius: 8px;
  background: var(--bg-base);
  cursor: pointer;
}
.md-filepick:hover {
  border-color: var(--border-strong);
}
.md-filepick-icon {
  color: var(--accent);
  flex-shrink: 0;
}
.md-filepick-text {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--text-tertiary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  flex: 1;
}
.md-filepick-text.filled {
  color: var(--text-primary);
}
.md-refetch {
  display: flex;
  gap: 6px;
  align-items: center;
  font-size: 12px;
  cursor: pointer;
  color: var(--text-secondary);
}
.md-summary {
  margin-top: 10px;
}
.md-summary-head {
  display: flex;
  gap: 14px;
  font-size: 12px;
}
.md-warn {
  margin-top: 6px;
  color: var(--text-secondary);
  font-size: 11px;
}
.md-lines {
  margin-top: 6px;
  display: flex;
  flex-direction: column;
  gap: 3px;
}
.md-line {
  display: flex;
  gap: 6px;
  font-family: var(--font-mono);
  font-size: 11px;
}
.md-line-name {
  color: var(--text-primary);
}
.ok {
  color: var(--accent);
}
.bad {
  color: var(--danger);
}
.hot {
  color: var(--accent);
}
.dim {
  color: var(--text-tertiary);
}
.md-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.md-provider {
  padding: 8px 12px;
  border-radius: 8px;
  cursor: pointer;
  display: flex;
  flex-direction: column;
  gap: 6px;
  border: 1px solid var(--border-subtle);
}
.md-provider.selected {
  background: var(--accent-subtle);
  border-color: var(--accent);
}
.md-provider.builtin {
  border-color: var(--accent);
}
.md-provider.dragging {
  border-style: dashed;
}
.md-provider-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  flex-wrap: wrap;
}
.md-provider-main {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
  flex-wrap: wrap;
}
.md-provider-copy {
  display: flex;
  flex-direction: column;
  gap: 1px;
  min-width: 0;
}
.md-provider-name {
  font-weight: 500;
}
.md-provider-key {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--text-tertiary);
  word-break: break-all;
}
.md-provider-ops {
  display: flex;
  align-items: center;
  gap: 8px;
}
.md-pin {
  color: var(--accent);
  user-select: none;
  line-height: 1;
  flex-shrink: 0;
}
.md-check {
  color: var(--accent);
}
.md-grip {
  cursor: grab;
  color: var(--text-tertiary);
  font-size: 14px;
  line-height: 1;
  user-select: none;
  flex-shrink: 0;
}
.md-icon-btn {
  width: 26px;
  height: 26px;
  display: inline-grid;
  place-items: center;
  border: 1px solid transparent;
  border-radius: 7px;
  background: transparent;
  color: var(--text-secondary);
  cursor: pointer;
}
.md-icon-btn:hover {
  color: var(--text-primary);
  background: var(--surface-translucent-hover);
}
.md-icon-btn.star {
  color: #f5b301;
}
.md-faint {
  color: var(--text-tertiary);
  font-size: 11px;
  flex-shrink: 0;
}
</style>
