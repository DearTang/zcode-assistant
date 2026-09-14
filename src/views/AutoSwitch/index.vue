<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { MyBadge, MyButton, MyDialog, MyFieldShell, MyIcon, MyInput, MyPanel, MyResultState, MySelect, MyToggle } from 'myui'
import WeekdayPicker from '@/components/fields/WeekdayPicker.vue'
import { autoswitch as sw, zcode } from '@/api'
import { toast } from '@/composables/toast'
import type { AutoSwitchLog, AutoSwitchProject, AutoSwitchRule, ZcProvider, ZcodeConfig } from '@/types'

defineOptions({ name: 'AutoSwitchView' })

/** 取路径最后一段作为项目名（兼容 / 与 \） */
const baseName = (p: string) => p.split(/[\\/]/).filter(Boolean).pop() || p

/** 执行方式 → 中文标签 */
const TRIGGER_LABEL: Record<string, string> = {
  manual: '人工点击',
  cron: '定时切换',
  drain: '配额耗尽',
  appstart: '应用启动',
}

/** 规则类型徽标文案 */
const kindBadge = (kind: string) => (kind === 'cron' ? '定时' : kind === 'drain' ? '配额耗尽' : '应用启动')

const WEEKDAYS = [
  { v: 1, label: '周一' },
  { v: 2, label: '周二' },
  { v: 3, label: '周三' },
  { v: 4, label: '周四' },
  { v: 5, label: '周五' },
  { v: 6, label: '周六' },
  { v: 7, label: '周日' },
]
const weekdayLabel = (n: number) => WEEKDAYS.find((w) => w.v === n)?.label ?? String(n)

const blank = (): AutoSwitchRule => ({
  id: '',
  name: '',
  kind: 'cron',
  enabled: true,
  timeStart: '09:00',
  weekdays: '1,2,3,4,5',
  fromProvider: '',
  toProvider: '',
  toModel: '',
  createdAt: '',
  projectDir: '',
  switchPrimary: false,
})

const rules = ref<AutoSwitchRule[]>([])
const config = ref<ZcodeConfig | null>(null)
const editing = ref<AutoSwitchRule | null>(null)
const dragIdx = ref<number | null>(null)
const showLogs = ref(false)
const testing = ref<string | null>(null)
// 可限定项目列表（当前打开且有对话的项目），打开编辑器时刷新
const projects = ref<AutoSwitchProject[]>([])

async function loadProjects(): Promise<void> {
  try {
    projects.value = await sw.projects()
  } catch {
    // 列表加载失败不阻塞编辑，仅无可选项
  }
}

async function reload(): Promise<void> {
  try {
    const [rs, cfg] = await Promise.all([sw.listRules(), zcode.getConfig()])
    rules.value = rs
    config.value = cfg
  } catch (e: unknown) {
    toast.error(String(e))
  }
}

onMounted(reload)

// 启用供应商（含智谱内置），排除系统禁用项；与「模型管理」页口径一致
const providers = computed<[string, ZcProvider][]>(() => {
  if (!config.value) return []
  return Object.entries(config.value.provider).filter(([, p]) => !p.systemDisabledReason && p.enabled !== false)
})

function providerLabel(key?: string): string {
  if (!key) return '（任意）'
  return config.value?.provider?.[key]?.name || key
}
function modelsOf(providerKey?: string): string[] {
  if (!providerKey || !config.value) return []
  return Object.keys(config.value.provider?.[providerKey]?.models ?? {})
}

function newRule(): void {
  const b = blank()
  // 默认目标=第一个供应商 + 其第一个模型
  if (providers.value.length > 0) {
    const [firstKey] = providers.value[0]
    b.toProvider = firstKey
    b.toModel = modelsOf(firstKey)[0] ?? ''
  }
  editing.value = b
  void loadProjects()
}

function editRule(r: AutoSwitchRule): void {
  editing.value = { ...r }
  void loadProjects()
}

async function save(): Promise<void> {
  const e = editing.value
  if (!e) return
  const name = e.name.trim()
  if (!name) {
    toast.error('请填写规则名称')
    return
  }
  // 规则名称不允许重复（排除自身，支持改名保存）
  if (rules.value.some((x) => x.id !== e.id && x.name === name)) {
    toast.error(`规则名称「${name}」已存在，请换一个`)
    return
  }
  if (!e.toProvider || !e.toModel) {
    toast.error('请选择目标供应商与模型')
    return
  }
  if (e.kind === 'cron' && !e.timeStart) {
    toast.error('请填写执行时间')
    return
  }
  if (e.kind === 'appstart') {
    // 全天生效 = 起止均空；填了任意一端就必须填全
    if (!!(e.timeStart || e.timeEnd) && !(e.timeStart && e.timeEnd)) {
      toast.error('请填写完整的生效时间范围，或改回「全天生效」')
      return
    }
    if (e.timeStart && e.timeStart === e.timeEnd) {
      toast.error('生效时间范围的起止不能相同')
      return
    }
  }
  try {
    await sw.upsertRule({ ...e, name })
    editing.value = null
    await reload()
    toast.success('规则已保存')
  } catch (err: unknown) {
    toast.error(String(err))
  }
}

async function del(id: string): Promise<void> {
  try {
    await sw.deleteRule(id)
    await reload()
  } catch (e: unknown) {
    toast.error(String(e))
  }
}

// 手动测试：跳过触发条件立即执行切换（结果记入执行日志）
async function test(r: AutoSwitchRule): Promise<void> {
  testing.value = r.id
  try {
    toast.success(await sw.testRule(r.id))
  } catch (e: unknown) {
    toast.error(String(e))
  } finally {
    testing.value = null
  }
}

async function toggle(r: AutoSwitchRule): Promise<void> {
  try {
    await sw.upsertRule({ ...r, enabled: !r.enabled })
    await reload()
  } catch (e: unknown) {
    toast.error(String(e))
  }
}

async function onDrop(targetIdx: number): Promise<void> {
  if (dragIdx.value === null || dragIdx.value === targetIdx) {
    dragIdx.value = null
    return
  }
  const next = [...rules.value]
  const [moved] = next.splice(dragIdx.value, 1)
  next.splice(targetIdx, 0, moved)
  rules.value = next
  dragIdx.value = null
  try {
    await sw.reorder(next.map((r) => r.id))
  } catch (e: unknown) {
    toast.error(String(e))
    await reload()
  }
}

// 目标供应商切换 → 默认选中第一个模型
function pickToProvider(providerKey: string): void {
  if (!editing.value) return
  editing.value.toProvider = providerKey
  editing.value.toModel = modelsOf(providerKey)[0] ?? ''
}
function pickFromProvider(providerKey: string): void {
  if (!editing.value) return
  editing.value.fromProvider = providerKey
  editing.value.fromModel = ''
}

// 下拉选项：已加载的项目列表；规则里的项目已不在列表（已关闭）时补一项保留原值
const projectOptions = computed(() => {
  const list = [...projects.value]
  const cur = editing.value?.projectDir
  if (cur && !list.some((p) => p.dir === cur)) {
    list.push({ dir: cur, name: `${baseName(cur)}（已关闭）`, sessions: 0 })
  }
  return [{ label: '（全部项目）', value: '' }, ...list.map((p) => ({ label: p.name, value: p.dir }))]
})

const providerOptions = computed(() => [
  { label: '（任意供应商）', value: '' },
  ...providers.value.map(([key, p]) => ({ label: p.name || key, value: key })),
])
const toProviderOptions = computed(() => [
  { label: '请选择', value: '' },
  ...providers.value.map(([key, p]) => ({ label: p.name || key, value: key })),
])
const fromModelOptions = computed(() => [
  { label: '（任意模型）', value: '' },
  ...modelsOf(editing.value?.fromProvider).map((m) => ({ label: m, value: m })),
])
const toModelOptions = computed(() => [
  { label: '请选择', value: '' },
  ...modelsOf(editing.value?.toProvider).map((m) => ({ label: m, value: m })),
])
const kindOptions = [
  { label: '定时切换', value: 'cron' },
  { label: '配额耗尽', value: 'drain' },
  { label: '应用启动', value: 'appstart' },
]

function onKindChange(kind: string): void {
  if (!editing.value) return
  editing.value.kind = kind as AutoSwitchRule['kind']
  // 切到应用启动清空 cron 残留的执行时间：从「全天生效」起步
  if (kind === 'appstart') {
    editing.value.timeStart = ''
    editing.value.timeEnd = ''
  }
}

function weekdaySummary(wds: string): string {
  const arr = wds.split(',').filter(Boolean).map(Number)
  if (arr.length === 0) return '每天'
  return arr.map(weekdayLabel).join('、')
}

// ===== 日志弹窗 =====
const logs = ref<AutoSwitchLog[] | null>(null)

watch(showLogs, (open) => {
  if (!open) return
  logs.value = null
  sw
    .logs()
    .then((v) => (logs.value = v))
    .catch((e) => {
      toast.error(String(e))
      logs.value = []
    })
})

function fmtLogTime(iso: string): string {
  const d = new Date(iso)
  return Number.isNaN(d.getTime()) ? iso : d.toLocaleString('zh-CN', { hour12: false })
}
</script>

<template>
  <div class="as">
    <MyPanel title="自动切换规则">
      <template #actions>
        <div class="as-head-ops">
          <MyButton size="small" @click="showLogs = true">执行日志</MyButton>
          <MyButton variant="primary" size="small" @click="newRule">
            <MyIcon name="Plus" :size="13" /> 新建规则
          </MyButton>
        </div>
      </template>

      <p class="as-desc">
        ① 定时（指定执行时间 / 星期 → 切到目标）② 配额耗尽（剩余 ≤ 阈值 →
        切到目标）③ 应用启动（本应用启动后自动执行一次，可限定生效时间范围，默认全天）。列表顺序即优先级，拖动手柄可调整。
        可点「测试」立即手动执行一次切换。切换会写入配置与全部符合条件的会话（规则限定项目时仅该项目）：
        开启「设置 → 切换后重启 ZCode」（默认）时自动重启 ZCode，全部对话立即生效；
        关闭时不重启，各对话在恢复 / 新开时生效。
        规则可限定项目：仅当最近对话发生在该项目时才触发，不影响在其他项目的工作。
      </p>

      <MyResultState v-if="rules.length === 0" type="empty" title="暂无规则" />
      <div v-else class="as-list">
        <div
          v-for="(r, idx) in rules"
          :key="r.id"
          class="as-row"
          :class="{ dragging: dragIdx === idx }"
          draggable="true"
          @dragstart="dragIdx = idx"
          @dragover.prevent
          @drop="onDrop(idx)"
          @dragend="dragIdx = null"
        >
          <div class="as-row-main">
            <span class="as-grip" title="拖动调整优先级">⠿</span>
            <MyBadge :value="String(idx + 1)" type="info" />
            <div class="as-row-copy">
              <div class="as-row-title">
                {{ r.name }}
                <MyBadge :value="kindBadge(r.kind)" type="info" />
                <MyBadge v-if="r.projectDir" :value="baseName(r.projectDir)" type="info" />
                <MyBadge v-if="r.switchPrimary" value="同步主供应" type="info" />
              </div>
              <div class="as-row-sub">
                {{
                  r.kind === 'cron'
                    ? `${weekdaySummary(r.weekdays || '')} ${r.timeStart || ''}${
                        r.fromProvider ? `（源 ${providerLabel(r.fromProvider)}${r.fromModel ? '/' + r.fromModel : ''}）` : ''
                      } → ${providerLabel(r.toProvider)}${r.toModel ? '/' + r.toModel : ''}`
                    : r.kind === 'drain'
                      ? `剩余≤${r.threshold ?? 0} → ${providerLabel(r.toProvider)}${r.toModel ? '/' + r.toModel : ''}`
                      : `应用启动${r.timeStart && r.timeEnd ? `（${r.timeStart}–${r.timeEnd} 生效）` : ''} → ${providerLabel(r.toProvider)}${r.toModel ? '/' + r.toModel : ''}`
                }}
              </div>
            </div>
          </div>
          <div class="as-row-ops">
            <MyToggle :model-value="r.enabled" :title="r.enabled ? '已启用' : '已禁用'" @update:model-value="toggle(r)" />
            <MyButton size="small" :disabled="testing === r.id" title="跳过触发条件，立即在 ZCode 界面执行一次切换（免重启）" @click="test(r)">
              {{ testing === r.id ? '测试中…' : '测试' }}
            </MyButton>
            <MyButton size="small" @click="editRule(r)">编辑</MyButton>
            <button class="as-icon-btn" type="button" @click="del(r.id)">
              <MyIcon name="Trash" :size="13" />
            </button>
          </div>
        </div>
      </div>
    </MyPanel>

    <!-- 规则编辑器（内联展开，非弹窗） -->
    <MyPanel v-if="editing" :title="editing.id ? '编辑规则' : '新建规则'">
      <div class="as-edit-grid">
        <MyFieldShell label="规则名称">
          <MyInput v-model="editing.name" />
        </MyFieldShell>
        <MyFieldShell label="类型">
          <MySelect :model-value="editing.kind" :options="kindOptions" :filterable="false" @update:model-value="(v) => onKindChange(v as string)" />
        </MyFieldShell>

        <!-- 项目限定（可选，默认全部项目） -->
        <MyFieldShell label="项目（可选，留空=全部项目）">
          <MySelect v-model="editing.projectDir" :options="projectOptions" :filterable="false" />
        </MyFieldShell>

        <!-- 源：供应商 → 模型 -->
        <MyFieldShell label="源供应商（可选，留空=任意）">
          <MySelect :model-value="editing.fromProvider || ''" :options="providerOptions" :filterable="false" @update:model-value="(v) => pickFromProvider(v as string)" />
        </MyFieldShell>
        <MyFieldShell label="源模型（可选，留空=该供应商任意模型）">
          <MySelect v-model="editing.fromModel" :options="fromModelOptions" :disabled="!editing.fromProvider" :filterable="false" />
        </MyFieldShell>

        <!-- 目标：供应商 → 模型 -->
        <MyFieldShell label="目标供应商">
          <MySelect :model-value="editing.toProvider" :options="toProviderOptions" :filterable="false" @update:model-value="(v) => pickToProvider(v as string)" />
        </MyFieldShell>
        <MyFieldShell label="目标模型（必选）">
          <MySelect v-model="editing.toModel" :options="toModelOptions" :disabled="!editing.toProvider" />
        </MyFieldShell>

        <!-- 主供应商联动 -->
        <div class="as-primary-row">
          <MyToggle
            :model-value="!!editing.switchPrimary"
            title="开启后，规则切换时把模型管理里的主供应商也设为目标供应商"
            @update:model-value="editing.switchPrimary = $event === true"
          />
          <span>
            同时切换主供应商
            <span class="as-faint">
              （同步「模型管理」的 ⭐ 主供应商标记到目标供应商，总览 / 悬浮球 / 托盘的配额展示跟随切换）
            </span>
          </span>
        </div>

        <template v-if="editing.kind === 'cron'">
          <MyFieldShell label="执行时间">
            <MyInput v-model="editing.timeStart" type="time" />
          </MyFieldShell>
          <MyFieldShell label="星期（多选）">
            <WeekdayPicker :model-value="editing.weekdays || ''" @update:model-value="editing.weekdays = $event as string" />
          </MyFieldShell>
        </template>
        <MyFieldShell v-else-if="editing.kind === 'drain'" label="剩余阈值（token）">
          <MyInput v-model.number="editing.threshold" type="number" />
        </MyFieldShell>
        <template v-else>
          <MyFieldShell label="生效时间">
            <MySelect
              :model-value="editing.timeStart && editing.timeEnd ? 'range' : 'allday'"
              :options="[
                { label: '全天生效', value: 'allday' },
                { label: '指定时间范围', value: 'range' },
              ]"
              :filterable="false"
              @update:model-value="
                (v) => {
                  if (v === 'allday') {
                    editing!.timeStart = '';
                    editing!.timeEnd = '';
                  } else {
                    editing!.timeStart = editing!.timeStart || '09:00';
                    editing!.timeEnd = editing!.timeEnd || '18:00';
                  }
                }
              "
            />
          </MyFieldShell>
          <MyFieldShell v-if="editing.timeStart && editing.timeEnd" label="时间范围（跨零点请让结束时间小于开始时间）">
            <div class="as-time-range">
              <MyInput v-model="editing.timeStart" type="time" />
              <span class="as-faint">至</span>
              <MyInput v-model="editing.timeEnd" type="time" />
            </div>
          </MyFieldShell>
          <div v-else class="as-allday-hint">
            本应用每次启动后自动执行一次切换（免重启，下一轮对话生效）
          </div>
        </template>
      </div>

      <div class="as-edit-actions">
        <MyButton size="small" @click="editing = null">取消</MyButton>
        <MyButton variant="primary" size="small" @click="save">保存</MyButton>
      </div>
    </MyPanel>

    <!-- 执行日志 -->
    <MyDialog v-model="showLogs" title="自动切换执行日志" :width="680" dismissable>
      <div class="as-logs">
        <div v-if="logs === null" class="ui-empty">加载中…</div>
        <MyResultState v-else-if="logs.length === 0" type="empty" title="暂无执行日志" />
        <table v-else class="as-logs-table">
          <thead>
            <tr>
              <th>任务名</th>
              <th>操作方式</th>
              <th>操作时间</th>
              <th>结果</th>
              <th>错误日志</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="l in logs" :key="l.id">
              <td>{{ l.ruleName }}</td>
              <td class="nowrap">{{ TRIGGER_LABEL[l.triggerType] ?? l.triggerType }}</td>
              <td class="nowrap mono">{{ fmtLogTime(l.createdAt) }}</td>
              <td class="nowrap" :class="l.success ? 'ok' : 'bad'">{{ l.success ? '成功' : '失败' }}</td>
              <td class="msg" :title="l.message">{{ l.message || '—' }}</td>
            </tr>
          </tbody>
        </table>
      </div>
    </MyDialog>
  </div>
</template>

<style scoped>
.as {
  display: grid;
  gap: var(--ui-gap);
}
.as-head-ops {
  display: flex;
  gap: 8px;
}
.as-desc {
  margin: 0 0 12px;
  color: var(--text-secondary);
  font-size: 12px;
  line-height: 1.7;
}
.as-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.as-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  padding: 8px 12px;
  border-radius: 8px;
  border: 1px solid var(--border-subtle);
  cursor: grab;
}
.as-row.dragging {
  background: var(--surface-translucent-hover);
  opacity: 0.6;
}
.as-row-main {
  display: flex;
  align-items: center;
  gap: 10px;
  min-width: 0;
}
.as-grip {
  color: var(--text-tertiary);
  cursor: grab;
  font-size: 13px;
  line-height: 1;
  user-select: none;
}
.as-row-copy {
  min-width: 0;
}
.as-row-title {
  display: flex;
  align-items: center;
  gap: 6px;
  font-weight: 500;
  flex-wrap: wrap;
}
.as-row-sub {
  margin-top: 2px;
  color: var(--text-tertiary);
  font-family: var(--font-mono);
  font-size: 11px;
}
.as-row-ops {
  display: flex;
  align-items: center;
  gap: 8px;
  flex: none;
}
.as-icon-btn {
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
.as-icon-btn:hover {
  color: var(--text-primary);
  background: var(--surface-translucent-hover);
}
.as-edit-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
  gap: 12px;
}
.as-primary-row {
  grid-column: 1 / -1;
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  color: var(--text-secondary);
}
.as-faint {
  color: var(--text-tertiary);
  font-size: 11px;
}
.as-time-range {
  display: flex;
  align-items: center;
  gap: 6px;
}
.as-allday-hint {
  align-self: end;
  padding-bottom: 8px;
  color: var(--text-tertiary);
  font-size: 11px;
}
.as-edit-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 12px;
}
.as-logs {
  max-height: 60vh;
  overflow-y: auto;
}
.as-logs-table {
  width: 100%;
  border-collapse: collapse;
  font-size: 12px;
}
.as-logs-table th {
  text-align: left;
  padding: 6px 8px;
  color: var(--text-secondary);
  font-weight: 500;
  border-bottom: 1px solid var(--border-base);
  white-space: nowrap;
}
.as-logs-table td {
  padding: 6px 8px;
  border-bottom: 1px solid var(--border-subtle);
  vertical-align: top;
}
.nowrap {
  white-space: nowrap;
}
.mono {
  font-family: var(--font-mono);
}
.ok {
  color: var(--success);
}
.bad {
  color: var(--danger);
}
.msg {
  color: var(--text-secondary);
  max-width: 260px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
