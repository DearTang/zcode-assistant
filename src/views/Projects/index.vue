<script setup lang="ts">
/**
 * 项目管理 —— 数据源为 zcode cli db 的 session 等表（读写）。
 * - 项目列表：会话数（活跃 / 归档）/ 对话次数 / token 消耗 / 创建与最近活跃时间；
 * - 展开项目查看会话明细（消耗含子代理后代），支持行内改名（title_source=custom）；
 * - 归档会话默认隐藏，「查看历史」两级控制（全局 / 项目内），归档可一键恢复；
 * - 项目与会话均支持勾选批量归档（不删除，可恢复）与批量删除（级联清数据）。
 */
import { computed, onMounted, ref, watch } from 'vue'
import { confirmDialog, MyButton, MyCheckbox, MyDialog, MyIcon, MyInlineEdit, MyPanel, MyRadioGroup, MyResultState, MyTag } from 'myui'
import RestartBar from '@/components/RestartBar.vue'
import { projects as projectsApi, formatUnits } from '@/api'
import { toast } from '@/composables/toast'
import type { ZcCacheStats, ZcProject, ZcSession } from '@/types'

defineOptions({ name: 'ProjectsView' })

/** 会话名称/归档恢复相关操作需重启 zcode 才可生效的提示文案 */
const SESSION_RESTART_HINT = '会话名称修改及恢复，需要重启zcode生效'

function fmtTime(ms?: number | null): string {
  if (ms == null || !Number.isFinite(ms)) return '—'
  const d = new Date(ms)
  if (Number.isNaN(d.getTime())) return '—'
  const p = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`
}

function baseName(dir: string): string {
  if (!dir) return '(未知)'
  const parts = dir.replace(/[\\/]+$/, '').split(/[\\/]/)
  return parts[parts.length - 1] || dir
}

function fmtBytes(n: number): string {
  if (!Number.isFinite(n) || n <= 0) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  let i = 0
  let v = n
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024
    i++
  }
  const s = i === 0 ? String(Math.round(v)) : v >= 100 ? v.toFixed(0) : v.toFixed(1)
  return `${s} ${units[i]}`
}

/** 清理缓存的时间范围选项（删除最后活跃早于该时间点的会话及其全部数据） */
const CACHE_OPTIONS = [
  { days: 3, label: '3 天前的会话' },
  { days: 5, label: '5 天前的会话' },
  { days: 7, label: '7 天前的会话' },
  { days: 15, label: '半个月前的会话' },
  { days: 30, label: '1 个月前的会话' },
]

const loading = ref(true)
const list = ref<ZcProject[]>([])
const expandedId = ref<string | null>(null)
// 项目 → 会话列表缓存（删除 / 改名后按需失效）
const sessionsMap = ref<Record<string, ZcSession[]>>({})
const sessionsLoading = ref<Record<string, boolean>>({})
const selectedProjects = ref(new Set<string>())
const selectedSessions = ref(new Set<string>())
const deleting = ref(false)
const archiving = ref(false)
const showHistory = ref(false)
const projectHistory = ref<Record<string, boolean>>({})
// 清理缓存弹窗
const cacheOpen = ref(false)
const cacheDays = ref(7)
const cacheStats = ref<ZcCacheStats | null>(null)
const cacheStatsLoading = ref(false)
const cleaning = ref(false)

async function reload(): Promise<void> {
  loading.value = true
  try {
    list.value = await projectsApi.list()
  } catch (e: unknown) {
    toast.error(`加载失败：${e instanceof Error ? e.message : String(e)}`)
  } finally {
    loading.value = false
  }
}

onMounted(reload)

// 打开清理弹窗或切换时间范围时拉取预览统计
watch([cacheOpen, cacheDays], ([open]) => {
  if (!open) return
  cacheStats.value = null
  cacheStatsLoading.value = true
  projectsApi
    .cacheStats(cacheDays.value)
    .then((s) => (cacheStats.value = s))
    .catch((e: unknown) => toast.error(`统计失败：${e instanceof Error ? e.message : String(e)}`))
    .finally(() => (cacheStatsLoading.value = false))
})

async function loadSessions(projectId: string, force = false): Promise<void> {
  if (!force && sessionsMap.value[projectId]) return
  sessionsLoading.value[projectId] = true
  try {
    const rows = await projectsApi.sessions(projectId)
    sessionsMap.value[projectId] = rows
  } catch (e: unknown) {
    toast.error(`加载会话失败：${e instanceof Error ? e.message : String(e)}`)
  } finally {
    sessionsLoading.value[projectId] = false
  }
}

function toggleExpand(p: ZcProject): void {
  if (expandedId.value === p.id) {
    expandedId.value = null
    return
  }
  expandedId.value = p.id
  void loadSessions(p.id)
}

// ============ 勾选 ============
function toggleProject(id: string): void {
  const n = new Set(selectedProjects.value)
  if (n.has(id)) n.delete(id)
  else n.add(id)
  selectedProjects.value = n
}
function toggleSession(id: string): void {
  const n = new Set(selectedSessions.value)
  if (n.has(id)) n.delete(id)
  else n.add(id)
  selectedSessions.value = n
}
function clearSelection(): void {
  selectedProjects.value = new Set()
  selectedSessions.value = new Set()
}

// ============ 改名 ============
async function commitEdit(id: string, v: string): Promise<void> {
  if (!v) return
  try {
    await projectsApi.renameSession(id, v)
    const pid = expandedId.value
    if (pid) {
      sessionsMap.value[pid] = (sessionsMap.value[pid] ?? []).map((s) =>
        s.id === id ? { ...s, title: v, titleSource: 'custom' } : s,
      )
    }
    toast.success('已更新会话名称')
    toast.warning(SESSION_RESTART_HINT)
  } catch (e: unknown) {
    toast.error(`改名失败：${e instanceof Error ? e.message : String(e)}`)
  }
}

// ============ 归档 / 恢复 ============
async function doArchiveSession(s: ZcSession): Promise<void> {
  try {
    await projectsApi.archiveSession(s.id)
    sessionsMap.value[s.projectId] = (sessionsMap.value[s.projectId] ?? []).map((x) =>
      x.id === s.id ? { ...x, archived: true, timeArchivedMs: Date.now() } : x,
    )
    await reload()
    toast.success('已归档会话，可在「查看历史会话」中恢复')
  } catch (e: unknown) {
    toast.error(`归档失败：${e instanceof Error ? e.message : String(e)}`)
  }
}

async function doArchiveProject(p: ZcProject): Promise<void> {
  try {
    const n = await projectsApi.archiveProject(p.id)
    await Promise.all([loadSessions(p.id, true), reload()])
    toast.success(`已归档该项目 ${n} 个会话，可在「查看历史会话」恢复`)
  } catch (e: unknown) {
    toast.error(`归档失败：${e instanceof Error ? e.message : String(e)}`)
  }
}

async function doArchiveSelected(): Promise<void> {
  const sids = [...selectedSessions.value]
  if (sids.length === 0) return
  const ok = await confirmDialog({
    message: `将归档所选 ${sids.length} 个会话（已归档的自动跳过），在 zcode 会话列表隐藏，可随时在「查看历史会话」中恢复。继续？`,
    type: 'warning',
    confirmButtonText: '归档',
  })
  if (!ok) return
  archiving.value = true
  try {
    const n = await projectsApi.archiveSessions(sids)
    clearSelection()
    if (expandedId.value) await loadSessions(expandedId.value, true)
    await reload()
    toast.success(`已归档 ${n} 个会话，可在「查看历史会话」中恢复`)
  } catch (e: unknown) {
    toast.error(`归档失败：${e instanceof Error ? e.message : String(e)}`)
  } finally {
    archiving.value = false
  }
}

async function doArchiveSelectedProjects(): Promise<void> {
  const pids = [...selectedProjects.value]
  if (pids.length === 0) return
  const ok = await confirmDialog({
    message: `将归档所选 ${pids.length} 个项目的全部活跃会话（已归档的自动跳过），在 zcode 会话列表隐藏，可随时恢复。继续？`,
    type: 'warning',
    confirmButtonText: '归档',
  })
  if (!ok) return
  archiving.value = true
  try {
    let n = 0
    for (const pid of pids) {
      n += await projectsApi.archiveProject(pid)
    }
    clearSelection()
    if (expandedId.value) await loadSessions(expandedId.value, true)
    await reload()
    toast.success(`已归档 ${pids.length} 个项目共 ${n} 个会话，可在「查看历史会话」中恢复`)
  } catch (e: unknown) {
    toast.error(`归档失败：${e instanceof Error ? e.message : String(e)}`)
  } finally {
    archiving.value = false
  }
}

async function doRestoreProject(p: ZcProject): Promise<void> {
  if (p.archivedSessions === 0) return
  try {
    const n = await projectsApi.restoreProject(p.id)
    await Promise.all([loadSessions(p.id, true), reload()])
    toast.success(`已恢复该项目 ${n} 个会话，可在 zcode 中继续对话`)
    toast.warning(SESSION_RESTART_HINT)
  } catch (e: unknown) {
    toast.error(`恢复失败：${e instanceof Error ? e.message : String(e)}`)
  }
}

async function doRestore(s: ZcSession): Promise<void> {
  try {
    await projectsApi.restoreSession(s.id)
    sessionsMap.value[s.projectId] = (sessionsMap.value[s.projectId] ?? []).map((x) =>
      x.id === s.id ? { ...x, archived: false, timeArchivedMs: null } : x,
    )
    await reload()
    toast.success('已恢复归档，可在 zcode 该项目的会话列表中继续对话')
    toast.warning(SESSION_RESTART_HINT)
  } catch (e: unknown) {
    toast.error(`恢复失败：${e instanceof Error ? e.message : String(e)}`)
  }
}

// ============ 批量删除（双重确认） ============
async function doDelete(targetSids?: string[], targetPids?: string[]): Promise<void> {
  const pids = targetPids ?? [...selectedProjects.value]
  const sids = targetSids ?? [...selectedSessions.value]
  if (pids.length === 0 && sids.length === 0) return
  const nameOf = (id: string) => {
    const p = list.value.find((x) => x.id === id)
    return p ? `「${baseName(p.directory)}」` : ''
  }
  const msg =
    pids.length > 0
      ? `将删除 ${pids.length} 个项目（${pids.slice(0, 3).map(nameOf).join('、')}${pids.length > 3 ? ' 等' : ''}）及其全部会话、消息与用量记录`
      : `将删除 ${sids.length} 个会话（含子代理）及其消息与用量记录`
  const first = await confirmDialog({
    message: `${msg}，操作不可恢复。若相关会话正在 zcode 中打开，建议先关闭对应窗口。继续？`,
    type: 'warning',
    confirmButtonText: '继续',
  })
  if (!first) return
  const second = await confirmDialog({
    message: '二次确认：删除后数据不可恢复、无法找回。确定删除吗？',
    type: 'error',
    confirmButtonText: '确定删除',
  })
  if (!second) return
  deleting.value = true
  try {
    const res = await projectsApi.delete(sids, pids)
    clearSelection()
    // 失效受影响缓存
    const next: Record<string, ZcSession[]> = {}
    for (const k of Object.keys(sessionsMap.value)) {
      if (!pids.includes(k)) next[k] = sessionsMap.value[k].filter((s) => !sids.includes(s.id))
    }
    sessionsMap.value = next
    if (expandedId.value && pids.includes(expandedId.value)) expandedId.value = null
    await reload()
    toast.success(
      `已删除 ${res.deletedSessions} 个会话${res.deletedProjects > 0 ? `（含 ${res.deletedProjects} 个项目）` : ''}`,
    )
  } catch (e: unknown) {
    toast.error(`删除失败：${e instanceof Error ? e.message : String(e)}`)
  } finally {
    deleting.value = false
  }
}

// ============ 清理缓存 ============
async function doCacheCleanup(): Promise<void> {
  if (!cacheStats.value || cacheStats.value.sessions === 0) return
  const opt = CACHE_OPTIONS.find((o) => o.days === cacheDays.value)
  const label = opt ? opt.label : `${cacheDays.value} 天前`
  const sizeText = cacheStats.value.freedBytes > 0 ? `，可释放约 ${fmtBytes(cacheStats.value.freedBytes)} 的缓存文件` : ''
  const first = await confirmDialog({
    message: `将删除${label}至今未活跃的 ${cacheStats.value.sessions} 个会话（含子代理共 ${cacheStats.value.totalSessions} 个）及其消息、子代理记录、工具缓存与用量记录${sizeText}。若相关会话正在 zcode 中打开，建议先关闭对应窗口。继续？`,
    type: 'warning',
    confirmButtonText: '继续',
  })
  if (!first) return
  const second = await confirmDialog({
    message: '二次确认：清理后数据不可恢复、无法找回。确定清理吗？',
    type: 'error',
    confirmButtonText: '确定清理',
  })
  if (!second) return
  cleaning.value = true
  try {
    const res = await projectsApi.cacheCleanup(cacheDays.value)
    cacheOpen.value = false
    sessionsMap.value = {}
    clearSelection()
    expandedId.value = null
    await reload()
    toast.success(
      `已清理 ${res.deletedSessions} 个会话${res.freedBytes > 0 ? `，释放约 ${fmtBytes(res.freedBytes)}` : ''}${res.dbVacuumed ? '，会话库已压缩' : ''}`,
    )
  } catch (e: unknown) {
    toast.error(`清理失败：${e instanceof Error ? e.message : String(e)}`)
  } finally {
    cleaning.value = false
  }
}

const totalSelected = computed(() => selectedProjects.value.size + selectedSessions.value.size)
// 全局「查看历史」关闭时隐藏全部会话均已归档的项目
const visibleProjects = computed(() =>
  showHistory.value ? list.value : list.value.filter((p) => p.archivedSessions < p.sessions),
)
const expanded = computed(() => visibleProjects.value.find((p) => p.id === expandedId.value) ?? null)
const sessions = computed(() => (expanded.value ? sessionsMap.value[expanded.value.id] ?? [] : []))
const showProjHistory = computed(() => (expanded.value ? projectHistory.value[expanded.value.id] === true : false))
const visibleSessions = computed(() =>
  showProjHistory.value ? sessions.value : sessions.value.filter((s) => !s.archived),
)
const archivedCount = computed(() => sessions.value.filter((s) => s.archived).length)
// 会话表头全选：作用于当前展开项目的可见会话（半选 = 只选中了其中一部分）
const allSessionsSelected = computed(
  () => visibleSessions.value.length > 0 && visibleSessions.value.every((s) => selectedSessions.value.has(s.id)),
)
const someSessionsSelected = computed(() => visibleSessions.value.some((s) => selectedSessions.value.has(s.id)))
function toggleSelectAllSessions(): void {
  const n = new Set(selectedSessions.value)
  if (allSessionsSelected.value) {
    visibleSessions.value.forEach((s) => n.delete(s.id))
  } else {
    visibleSessions.value.forEach((s) => n.add(s.id))
  }
  selectedSessions.value = n
}

const cacheDayOptions = CACHE_OPTIONS.map((o) => ({ label: o.label, value: o.days }))
</script>

<template>
  <div class="pj">
    <RestartBar :hint="SESSION_RESTART_HINT" />

    <!-- 工具条 -->
    <MyPanel title="项目管理">
      <template #actions>
        <div class="pj-tools">
          <MyButton size="small" title="按时间批量清理久未活跃的会话及其缓存数据，释放磁盘空间" @click="cacheOpen = true">
            <MyIcon name="Trash" :size="14" /> 清理缓存
          </MyButton>
          <MyButton
            size="small"
            :class="{ 'is-active': showHistory }"
            :title="showHistory ? '当前显示历史项目' : '开启后显示全部会话均已归档的项目'"
            @click="showHistory = !showHistory"
          >
            查看历史项目
          </MyButton>
          <MyButton variant="ghost" size="small" :loading="loading" @click="reload">
            <MyIcon name="Refresh" :size="14" /> 刷新
          </MyButton>
        </div>
      </template>

      <p class="pj-desc">
        管理 zcode 的项目与会话：查看各项目 / 会话的 token 消耗、对话次数与创建时间，支持会话改名、
        归档项目 / 归档会话 / 恢复项目 / 恢复会话与批量归档（只归档不删除，可在「查看历史」恢复）、
        批量删除。默认只显示活跃会话，开启「查看历史项目 / 查看历史会话」查看归档。
        「清理缓存」可按时间批量删除久未活跃的会话及其全部数据；删除 / 清理会同时清理消息、用量记录、
        子代理记录与缓存文件，不可恢复。
      </p>

      <!-- 批量操作条 -->
      <div v-if="totalSelected > 0" class="pj-bulk">
        <span class="pj-bulk-count">已选 {{ selectedProjects.size }} 个项目、{{ selectedSessions.size }} 个会话</span>
        <div class="pj-bulk-ops">
          <MyButton size="small" :disabled="deleting" @click="clearSelection">取消选择</MyButton>
          <MyButton
            v-if="selectedSessions.size > 0"
            size="small"
            :disabled="archiving || deleting"
            title="批量归档所选会话（不删除：zcode 会话列表隐藏，可随时恢复）"
            @click="doArchiveSelected"
          >
            {{ archiving ? '归档中…' : '归档所选会话' }}
          </MyButton>
          <MyButton
            v-if="selectedProjects.size > 0"
            size="small"
            :disabled="archiving || deleting"
            title="批量归档所选项目的全部活跃会话（不删除：可随时恢复）"
            @click="doArchiveSelectedProjects"
          >
            {{ archiving ? '归档中…' : '归档所选项目' }}
          </MyButton>
          <MyButton variant="danger" size="small" :loading="deleting" @click="doDelete()">
            <MyIcon name="Trash" :size="13" /> 删除所选
          </MyButton>
        </div>
      </div>

      <div v-if="loading" class="ui-empty">加载中…</div>
      <MyResultState
        v-else-if="visibleProjects.length === 0"
        type="empty"
        :title="list.length === 0 ? '暂无项目（未找到 zcode 会话数据）' : '活跃项目为空，开启「查看历史」查看已归档项目'"
      />
      <div v-else class="pj-list">
        <div
          v-for="p in visibleProjects"
          :key="p.id"
          class="pj-project"
          :class="{ open: expandedId === p.id, dimmed: p.archivedSessions >= p.sessions }"
        >
          <!-- 项目行 -->
          <div class="pj-row" @click="toggleExpand(p)">
            <div class="pj-row-main">
              <MyCheckbox
                :model-value="selectedProjects.has(p.id)"
                class="pj-check"
                @click.stop
                @update:model-value="toggleProject(p.id)"
              />
              <MyIcon name="Folder" :size="16" class="pj-folder" />
              <div class="pj-copy">
                <div class="pj-name">
                  {{ baseName(p.directory) }}
                  <MyTag
                    :type="p.archivedSessions >= p.sessions ? 'info' : 'success'"
                    size="small"
                    round
                  >
                    {{ p.archivedSessions >= p.sessions ? '归档' : '活跃' }}
                  </MyTag>
                  <MyTag type="info" size="small" round>
                    {{ `${p.sessions} 会话 · ${p.sessions - p.archivedSessions} 活跃 · ${p.archivedSessions} 归档` }}
                  </MyTag>
                </div>
                <div class="pj-dir" :title="p.directory">{{ p.directory }}</div>
              </div>
            </div>
            <div class="pj-row-meta">
              <MyButton
                v-if="p.sessions - p.archivedSessions > 0"
                size="small"
                class="pj-mini-btn"
                title="归档项目：批量归档该项目全部活跃会话（对称于 zcode 的「归档项目」），可随时恢复"
                @click.stop="doArchiveProject(p)"
              >
                归档项目
              </MyButton>
              <MyButton
                v-if="p.archivedSessions > 0"
                size="small"
                class="pj-mini-btn"
                title="恢复项目：恢复该项目全部已归档会话，可在 zcode 中继续对话"
                @click.stop="doRestoreProject(p)"
              >
                恢复项目
              </MyButton>
              <span title="对话次数（含子代理）">对话 {{ p.turns.toLocaleString() }}</span>
              <span title="token 总消耗（与用量查询同口径）">{{ formatUnits(p.totalTokens) }} tok</span>
              <span title="最近活跃 / 创建时间">{{ fmtTime(p.timeUpdatedMs) }} · {{ fmtTime(p.timeCreatedMs) }}</span>
              <span>{{ expandedId === p.id ? '▲' : '▼' }}</span>
            </div>
          </div>

          <!-- 会话明细 -->
          <div v-if="expandedId === p.id" class="pj-sessions">
            <div class="pj-sessions-head">
              <span class="pj-sessions-meta">
                共 {{ sessions.length }} 会话 · {{ sessions.length - archivedCount }} 活跃 · {{ archivedCount }} 归档（按最后活跃排序）
              </span>
              <MyButton
                size="small"
                :class="{ 'is-active': showProjHistory }"
                title="开启后显示该项目已归档的会话"
                @click="projectHistory[p.id] = !(projectHistory[p.id] === true)"
              >
                查看历史会话
              </MyButton>
            </div>

            <div v-if="sessionsLoading[p.id]" class="ui-empty">加载会话…</div>
            <MyResultState
              v-else-if="visibleSessions.length === 0"
              type="empty"
              :title="archivedCount > 0 ? '无活跃会话，开启「查看历史」查看归档会话' : '该项目暂无会话'"
            />
            <div v-else class="pj-table-wrap">
              <table class="pj-table">
                <thead>
                  <tr>
                    <th class="col-check">
                      <!-- myui v0.9.0 MyCheckbox 内建 indeterminate（表头全选半选） -->
                      <MyCheckbox
                        :model-value="allSessionsSelected"
                        :indeterminate="someSessionsSelected && !allSessionsSelected"
                        title="全选 / 取消全选当前显示的会话"
                        @update:model-value="toggleSelectAllSessions()"
                      />
                    </th>
                    <th class="ta-l">会话</th>
                    <th>对话</th>
                    <th>调用</th>
                    <th>输入</th>
                    <th>输出</th>
                    <th>总量</th>
                    <th>创建时间</th>
                    <th>最近活跃</th>
                    <th class="col-ops"></th>
                  </tr>
                </thead>
                <tbody>
                  <tr v-for="s in visibleSessions" :key="s.id" :class="{ archived: s.archived }">
                    <td>
                      <MyCheckbox
                        :model-value="selectedSessions.has(s.id)"
                        @update:model-value="toggleSession(s.id)"
                      />
                    </td>
                    <td class="ta-l" :title="s.title">
                      <span class="pj-session-title">
                        <!-- myui v0.9.0 MyInlineEdit：点击名称即进入编辑，Enter 提交 / Esc 取消 -->
                        <MyInlineEdit
                          :model-value="s.title || ''"
                          placeholder="输入会话名称"
                          @confirm="(v) => commitEdit(s.id, v)"
                        />
                        <MyTag v-if="s.titleSource === 'custom'" type="info" size="small" round>自定义</MyTag>
                        <MyTag
                          :type="s.archived ? 'info' : 'success'"
                          size="small"
                          round
                          :title="s.archived ? (s.timeArchivedMs ? `归档于 ${fmtTime(s.timeArchivedMs)}` : '已归档（zcode 任务索引）') : '活跃会话'"
                        >
                          {{ s.archived ? '归档' : '活跃' }}
                        </MyTag>
                      </span>
                    </td>
                    <td class="mono">{{ s.turns.toLocaleString() }}</td>
                    <td class="mono faint">{{ s.calls.toLocaleString() }}</td>
                    <td class="mono faint">{{ formatUnits(s.inputTokens) }}</td>
                    <td class="mono">{{ formatUnits(s.outputTokens) }}</td>
                    <td class="mono strong">{{ formatUnits(s.totalTokens) }}</td>
                    <td class="mono faint">{{ fmtTime(s.timeCreatedMs) }}</td>
                    <td class="mono faint">{{ fmtTime(s.timeUpdatedMs) }}</td>
                    <td>
                      <div class="pj-session-ops">
                        <MyButton
                          v-if="s.archived"
                          size="small"
                          class="pj-mini-btn"
                          title="恢复会话：清掉归档标记，回 zcode 会话列表继续对话"
                          @click="doRestore(s)"
                        >
                          恢复会话
                        </MyButton>
                        <MyButton
                          v-else
                          size="small"
                          class="pj-mini-btn"
                          title="归档会话：zcode 会话列表隐藏，可随时恢复"
                          @click="doArchiveSession(s)"
                        >
                          归档会话
                        </MyButton>
                        <button class="pj-icon-btn" type="button" title="删除会话" @click="doDelete([s.id], [])">
                          <MyIcon name="Trash" :size="13" />
                        </button>
                      </div>
                    </td>
                  </tr>
                </tbody>
              </table>
            </div>
          </div>
        </div>
      </div>
    </MyPanel>

    <!-- 清理缓存弹窗（myui v0.10.0：confirmDisabled 禁用确认按钮 + danger + manualClose） -->
    <MyDialog
      v-model="cacheOpen"
      title="清理缓存数据"
      :width="440"
      :dismissable="!cleaning"
      cancel-text="取消"
      :confirm-text="cleaning ? '清理中…' : '确认清理'"
      :danger="true"
      :confirm-loading="cleaning"
      :confirm-disabled="!cacheStats || cacheStats.sessions === 0"
      :manual-close="true"
      @confirm="doCacheCleanup"
    >
      <div class="pj-cache">
        <p class="pj-cache-desc">
          删除最后活跃时间早于所选时间点的会话及其全部数据：消息、子代理执行记录、工具结果缓存、
          执行日志与用量记录，并尝试压缩会话数据库进一步释放空间。
        </p>
        <MyRadioGroup v-model="cacheDays" :options="cacheDayOptions" />
        <div class="pj-cache-stats">
          {{
            cacheStatsLoading
              ? '统计中…'
              : cacheStats
                ? cacheStats.sessions === 0
                  ? '该时间范围内没有可清理的会话'
                  : `将删除 ${cacheStats.sessions} 个会话（含子代理共 ${cacheStats.totalSessions} 个），预计释放约 ${fmtBytes(cacheStats.freedBytes)} 的缓存文件。`
                : '—'
          }}
        </div>
      </div>
    </MyDialog>
  </div>
</template>

<style scoped>
.pj {
  display: grid;
  gap: var(--ui-gap);
}
.pj-tools {
  display: flex;
  gap: 8px;
}
.pj-tools :deep(.is-active) {
  border-color: var(--accent);
  color: var(--accent);
  background: var(--accent-subtle);
}
.pj-desc {
  margin: 0 0 12px;
  color: var(--text-secondary);
  font-size: 12px;
  line-height: 1.7;
}
.pj-bulk {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  padding: 8px 12px;
  margin-bottom: 12px;
  border-radius: 8px;
  border: 1px solid oklch(0.62 0.24 27 / 0.33);
  background: oklch(0.62 0.24 27 / 0.08);
  flex-wrap: wrap;
}
.pj-bulk-count {
  color: var(--text-tertiary);
  font-size: 12px;
}
.pj-bulk-ops {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
}
.pj-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.pj-project {
  border-radius: 8px;
  border: 1px solid var(--border-subtle);
  overflow: hidden;
}
.pj-project.open {
  border-color: var(--accent);
}
.pj-project.dimmed {
  opacity: 0.72;
}
.pj-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  padding: 8px 12px;
  cursor: pointer;
}
.pj-row-main {
  display: flex;
  align-items: center;
  gap: 10px;
  min-width: 0;
  flex: 1;
}
.pj-check {
  flex-shrink: 0;
}
.pj-folder {
  color: var(--accent);
  flex-shrink: 0;
}
.pj-copy {
  min-width: 0;
}
.pj-name {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
  font-weight: 500;
}
.pj-dir {
  color: var(--text-tertiary);
  font-family: var(--font-mono);
  font-size: 11px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.pj-row-meta {
  display: flex;
  align-items: center;
  gap: 14px;
  flex-shrink: 0;
  color: var(--text-tertiary);
  font-family: var(--font-mono);
  font-size: 11px;
}
.pj-mini-btn {
  height: 24px !important;
  padding: 0 10px !important;
}
.pj-sessions {
  border-top: 1px solid var(--border-subtle);
  padding: 8px 12px 12px;
}
.pj-sessions-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  margin-bottom: 8px;
}
.pj-sessions-head :deep(.is-active) {
  border-color: var(--accent);
  color: var(--accent);
  background: var(--accent-subtle);
}
.pj-sessions-meta {
  color: var(--text-tertiary);
  font-family: var(--font-mono);
  font-size: 11px;
}
.pj-table-wrap {
  overflow-x: auto;
}
.pj-table {
  width: 100%;
  border-collapse: collapse;
  font-size: 12px;
}
.pj-table th {
  text-align: right;
  padding: 6px 8px;
  color: var(--text-secondary);
  font-weight: 500;
  border-bottom: 1px solid var(--border-base);
  white-space: nowrap;
  cursor: default;
}
.pj-table td {
  text-align: right;
  padding: 6px 8px;
  border-bottom: 1px solid var(--border-subtle);
  white-space: nowrap;
}
.pj-table tr:hover td {
  background: var(--surface-translucent-hover);
}
.pj-table tr.archived {
  opacity: 0.62;
}
.ta-l {
  text-align: left !important;
}
.col-check {
  width: 30px;
}
.col-ops {
  width: 170px;
}
.mono {
  font-family: var(--font-mono);
  font-variant-numeric: tabular-nums;
}
.faint {
  color: var(--text-tertiary);
}
.strong {
  font-weight: 600;
}
.pj-session-title {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
}
.pj-session-name {
  flex: 1;
  min-width: 120px;
  max-width: 320px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.pj-session-ops {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 4px;
}
.pj-icon-btn {
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
.pj-icon-btn:hover {
  color: var(--text-primary);
  background: var(--surface-translucent-hover);
}
.pj-cache-desc {
  margin: 0 0 12px;
  color: var(--text-secondary);
  font-size: 12px;
  line-height: 1.7;
}
.pj-cache {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.pj-cache-stats {
  color: var(--text-tertiary);
  font-size: 11px;
  padding: 8px 10px;
  border-radius: 8px;
  border: 1px solid var(--border-subtle);
  line-height: 1.6;
}
.pj-cache-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>
