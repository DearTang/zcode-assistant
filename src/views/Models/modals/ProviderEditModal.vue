<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { MyBadge, MyButton, MyCheckbox, MyDialog, MyFieldShell, MyIcon, MyInput, MySelect, MyToggle, notify } from 'myui'
import ModalModelRow from './ModalModelRow.vue'
import { DEFAULT_CONTEXT, RESTART_HINT, detectCodingPlan } from '../shared'
import { events, maskApiKey, models, openUrl, quotaToken, templates } from '@/api'
import { toast } from '@/composables/toast'
import type { ProviderKind, ZcModel, ZcProvider, QuotaTemplate, QuotaTokenStatus, ModelSpec } from '@/types'

/** 供应商编辑弹窗（巨型表单）：供应商信息 / 模型列表（拖拽排序+拉取）/ 用量查询模板 / Token 获取 */
const props = defineProps<{
  providerKey: string
  provider: ZcProvider
  isCurrent: boolean
  isBuiltin: boolean
  busy: boolean
}>()
const emit = defineEmits<{ close: []; run: [fn: () => Promise<void>] }>()
defineOptions({ name: 'ProviderEditModal' })

function onRun(fn: () => Promise<void>): void {
  emit('run', fn)
}

// ===== 供应商信息（编辑态）=====
const eName = ref(props.provider.name)
const eKind = ref<ProviderKind>(props.provider.kind)
const eUrl = ref(props.provider.options.baseURL ?? '')
// apiKey 来自后端时是脱敏的 <REDACTED>，输入框显示占位提示
const eKey = ref('')
const keyTouched = ref(false)
// 「查看 key」：临时展示明文（不影响保存；保存仍以 keyTouched 为准）
const revealedKey = ref<string | null>(null)
const copied = ref(false)

// ===== 拉取可用模型的候选（非 null = 选择弹窗打开）=====
const fetchCands = ref<ModelSpec[] | null>(null)
const fetchSel = ref(new Set<string>())
const fetchQuery = ref('')
// 模型拖拽排序
const dragModelIdx = ref<number | null>(null)
let modelGrip = false

// ===== 用量查询模板 =====
const tTmpl = ref<QuotaTemplate>({ providerKey: props.providerKey, method: 'GET' })
const tHasTmpl = ref(false)
const allTmpls = ref<QuotaTemplate[]>([])
const builtinTmpls = ref<QuotaTemplate[]>([])
const tToken = ref<QuotaTokenStatus | null>(null)
const revealedToken = ref<string | null>(null)
const tokenCopied = ref(false)
const showManualToken = ref(false)
const manualTokenDraft = ref('')
const isTokenMode = computed(() => (tTmpl.value.authMode ?? 'appkey') === 'token')
const codingPlan = computed(() => detectCodingPlan(props.provider.options.baseURL ?? ''))
const tExtra = computed<Record<string, string>>(() => {
  try {
    return JSON.parse(tTmpl.value.extraJson || '{}')
  } catch {
    return {}
  }
})
function setExtra(k: string, v: string): void {
  tTmpl.value = { ...tTmpl.value, extraJson: JSON.stringify({ ...tExtra.value, [k]: v }) }
}

// 供应商信息锁定：当前供应商 / 智谱账号 builtin 均不可改
const infoLocked = computed(() => props.isCurrent || props.isBuiltin)

const selModels = computed(() => Object.entries(props.provider.models))
const apiKeyDisplay = computed(() => revealedKey.value ?? (keyTouched.value ? eKey.value : maskApiKey(props.provider.options.apiKey)))

// 模板插值内不能出现字面 "{{...}}"（会被编译器截断），文本引用统一走常量
const TOKEN_REF = '{{token}}'
const TOKEN_HINT = `用 ${TOKEN_REF} 引用登录获取的会话 Token`
const APIKEY_HINT = '用 {{apiKey}}/{{baseURL}} 引用供应商 API Key'
const URL_PLACEHOLDER = '{{baseURL}}/dashboard/billing/credit_grants'
const HEADERS_PLACEHOLDER = '{"Authorization":"Bearer {{token}}"}'

const kindOptions = [
  { label: 'OpenAI 兼容', value: 'openai-compatible' },
  { label: 'OpenAI', value: 'openai' },
  { label: 'Anthropic', value: 'anthropic' },
]
const methodOptions = [
  { label: 'GET', value: 'GET' },
  { label: 'POST', value: 'POST' },
]
const unitOptions = [
  { label: '绝对值（默认）', value: '' },
  { label: '百分比 %（0-1 自动 ×100）', value: '%' },
]

// ===== 行为 =====
async function handleReveal(): Promise<void> {
  if (revealedKey.value != null) {
    revealedKey.value = null
    return
  }
  revealedKey.value = await models.getApiKey(props.providerKey)
}

async function handleCopyId(): Promise<void> {
  try {
    await navigator.clipboard.writeText(props.providerKey)
    copied.value = true
    setTimeout(() => (copied.value = false), 1200)
  } catch {
    /* 剪贴板不可用，忽略 */
  }
}

function handleSaveProvider(): void {
  onRun(async () => {
    await models.updateProvider(props.providerKey, {
      name: eName.value,
      kind: eKind.value,
      baseURL: eUrl.value,
      // 只有用户改过 apiKey 才提交
      apiKey: keyTouched.value ? eKey.value : undefined,
    })
    toast.success('供应商信息已保存')
    toast.warning(RESTART_HINT)
  })
}

function handleFetch(): void {
  onRun(async () => {
    const specs = await models.fetchAvailable(props.providerKey)
    const builtin = await models.builtinSpecs()
    const merged = specs.map((s) => {
      const b = builtin.find((x) => x.id === s.id)
      return {
        ...s,
        contextLength: s.contextLength ?? b?.contextLength,
        maxOutput: s.maxOutput ?? b?.maxOutput,
      }
    })
    const existing = new Set(Object.keys(props.provider.models))
    fetchSel.value = new Set(merged.filter((s) => !existing.has(s.id)).map((s) => s.id))
    fetchCands.value = merged
    fetchQuery.value = ''
  })
}

// 拖拽排序：把 dragModelIdx 项移到 targetIdx 位置
function handleModelReorder(targetIdx: number): void {
  const src = dragModelIdx.value
  dragModelIdx.value = null
  modelGrip = false
  if (src === null || src === targetIdx) return
  const names = selModels.value.map(([n]) => n)
  const [moved] = names.splice(src, 1)
  names.splice(targetIdx, 0, moved)
  onRun(async () => {
    await models.reorderModels(props.providerKey, names)
    toast.success('模型顺序已更新')
    toast.warning(RESTART_HINT)
  })
}

function handleApplySelected(): void {
  onRun(async () => {
    if (!fetchCands.value) return
    const picked = fetchCands.value.filter((s) => fetchSel.value.has(s.id))
    closeFetchModal()
    if (picked.length === 0) return
    const n = await models.applyModels(props.providerKey, picked)
    toast.success(`已写入 ${n} 个模型到 config.json`)
    toast.warning(RESTART_HINT)
  })
}

function closeFetchModal(): void {
  fetchCands.value = null
  fetchSel.value = new Set()
  fetchQuery.value = ''
}

/** 模糊匹配：查询词逐字符按顺序出现在模型 id 中即命中 */
function fuzzyMatch(text: string, q: string): boolean {
  if (!q.trim()) return true
  const t = text.toLowerCase()
  let i = 0
  for (const ch of q.toLowerCase()) {
    i = t.indexOf(ch, i)
    if (i < 0) return false
    i += 1
  }
  return true
}
const filteredCands = computed(() => (fetchCands.value ?? []).filter((s) => fuzzyMatch(s.id, fetchQuery.value)))

function toggleFetchSel(id: string, on: boolean): void {
  const next = new Set(fetchSel.value)
  if (on) next.add(id)
  else next.delete(id)
  fetchSel.value = next
}
function selectAllFiltered(): void {
  const next = new Set(fetchSel.value)
  for (const s of filteredCands.value) next.add(s.id)
  fetchSel.value = next
}

// ===== 模板 / Token 生命周期 =====
onMounted(() => {
  templates
    .get(props.providerKey)
    .then((t) => {
      if (t) {
        tTmpl.value = t
        tHasTmpl.value = true
      }
    })
    .catch(() => {})
  templates.list().then((v) => (allTmpls.value = v)).catch(() => {})
  templates.builtin().then((v) => (builtinTmpls.value = v)).catch(() => {})
  quotaToken
    .status(props.providerKey)
    .then((v) => (tToken.value = v))
    .catch(() => (tToken.value = null))
})

const unlistenFns: (() => void)[] = []
// Token 状态：登录窗获取成功后广播刷新
events
  .onTokenUpdated((p) => {
    if (p.providerKey === props.providerKey) {
      quotaToken.status(props.providerKey).then((v) => (tToken.value = v)).catch(() => {})
    }
  })
  .then((fn) => unlistenFns.push(fn))
onBeforeUnmount(() => unlistenFns.forEach((fn) => fn()))

// 新 token 写入（fetchedAt 变化）后收回已显示的明文
watch(
  () => tToken.value?.fetchedAt,
  () => {
    revealedToken.value = null
    tokenCopied.value = false
  },
)

async function handleRevealToken(): Promise<void> {
  if (revealedToken.value != null) {
    revealedToken.value = null
    return
  }
  revealedToken.value = (await quotaToken.value(props.providerKey)) ?? ''
}

async function handleCopyToken(): Promise<void> {
  if (!revealedToken.value) return
  try {
    await navigator.clipboard.writeText(revealedToken.value)
    tokenCopied.value = true
    setTimeout(() => (tokenCopied.value = false), 1200)
  } catch {
    /* 剪贴板不可用 */
  }
}

/** 使用模板：把模板内容字段一键复制进当前表单（providerKey 保留当前供应商）；
 *  附加凭据（extraJson）不属于「模板内容」：所选模板没带就保留用户已填的 */
function handleUseTmpl(key: string): void {
  const t = allTmpls.value.find((x) => x.providerKey === key) ?? builtinTmpls.value.find((x) => x.providerKey === key)
  if (!t) return
  tTmpl.value = {
    providerKey: props.providerKey,
    name: t.name,
    method: t.method,
    url: t.url,
    headersJson: t.headersJson,
    body: t.body,
    totalPath: t.totalPath,
    usedPath: t.usedPath,
    remainingPath: t.remainingPath,
    monthlyTotalPath: t.monthlyTotalPath,
    monthlyUsedPath: t.monthlyUsedPath,
    monthlyRemainingPath: t.monthlyRemainingPath,
    fiveHourTotalPath: t.fiveHourTotalPath,
    fiveHourUsedPath: t.fiveHourUsedPath,
    fiveHourRemainingPath: t.fiveHourRemainingPath,
    weeklyTotalPath: t.weeklyTotalPath,
    weeklyUsedPath: t.weeklyUsedPath,
    weeklyRemainingPath: t.weeklyRemainingPath,
    loginUrl: t.loginUrl,
    tokenSource: t.tokenSource,
    authMode: t.authMode,
    loginUsername: t.loginUsername,
    unit: t.unit,
    resetTimePath: t.resetTimePath,
    fiveHourResetTimePath: t.fiveHourResetTimePath,
    weeklyResetTimePath: t.weeklyResetTimePath,
    monthlyResetTimePath: t.monthlyResetTimePath,
    extraJson: t.extraJson ?? tTmpl.value.extraJson,
  }
  toast.success('模板内容已复制，可修改后点「保存模板」')
}

function handleSaveTmpl(): void {
  onRun(async () => {
    await templates.upsert(tTmpl.value)
    tHasTmpl.value = true
    toast.success('用量查询模板已保存')
  })
}

function handleRemoveTmpl(): void {
  onRun(async () => {
    await templates.remove(props.providerKey)
    tTmpl.value = { providerKey: props.providerKey, method: 'GET' }
    tHasTmpl.value = false
    toast.success('用量查询模板已清除')
  })
}

/** 登录获取 Token：先保存模板（后端读库里的 loginUrl/提取规则）再弹登录窗 */
function handleLoginToken(): void {
  onRun(async () => {
    if (!tTmpl.value.loginUrl?.trim() || !tTmpl.value.tokenSource?.trim()) {
      toast.error('请先填写「登录页 URL」和「Token 提取方式」并保存模板')
      return
    }
    await templates.upsert(tTmpl.value)
    tHasTmpl.value = true
    await quotaToken.startLogin(props.providerKey)
    toast.success('登录窗口已打开，完成登录（含两步验证）后自动获取 Token')
  })
}

function handleClearToken(): void {
  onRun(async () => {
    await quotaToken.clear(props.providerKey)
    tToken.value = tToken.value ? { ...tToken.value, hasToken: false, fetchedAt: undefined } : null
    toast.success('Token 已清除')
  })
}

/** 手动粘贴 Token 保存：写 keyring；状态更新由 onTokenUpdated 事件刷新 */
function handleSaveManualToken(): void {
  onRun(async () => {
    const v = manualTokenDraft.value.trim()
    if (v.length < 8) {
      toast.error('Token 太短（至少 8 字符），请核对后重新粘贴')
      return
    }
    await quotaToken.set(props.providerKey, v)
    manualTokenDraft.value = ''
    showManualToken.value = false
    toast.success('Token 已手动写入系统凭证库')
  })
}

// 模板选择数据源（使用模板下拉）：myui v0.9.0 MySelect groups 三组分组
const builtinCpTmpls = computed(() => builtinTmpls.value.filter((t) => t.providerKey.startsWith('preset:cp-')))
const builtinBalTmpls = computed(() => builtinTmpls.value.filter((t) => !t.providerKey.startsWith('preset:cp-')))
const myTmpls = computed(() => allTmpls.value.filter((t) => t.providerKey !== props.providerKey))
function tmplLabel(t: QuotaTemplate): string {
  return t.name?.trim() || t.providerKey
}
const tmplGroups = computed(() =>
  [
    { label: 'Token Plan 额度（内置，自动查询）', options: builtinCpTmpls.value.map((t) => ({ label: tmplLabel(t), value: t.providerKey })) },
    { label: '余额查询（内置）', options: builtinBalTmpls.value.map((t) => ({ label: tmplLabel(t), value: t.providerKey })) },
    { label: '我的模板', options: myTmpls.value.map((t) => ({ label: tmplLabel(t), value: t.providerKey })) },
  ].filter((g) => g.options.length > 0),
)
</script>

<template>
  <MyDialog
    :model-value="true"
    :title="provider.name"
    :width="720"
    hide-footer
    class="edit-dialog"
    @cancel="emit('close')"
    @update:model-value="emit('close')"
  >
    <div class="ed">
      <!-- 头部徽标 -->
      <div class="ed-head">
        <MyBadge :value="provider.kind" type="info" />
        <MyBadge v-if="isBuiltin" value="智谱CodingPlan" type="success" />
        <MyBadge v-if="isCurrent" value="当前" type="primary" />
      </div>

      <!-- 供应商信息 -->
      <div class="ed-section-head">
        <span class="ed-section-title">供应商信息</span>
        <span v-if="infoLocked" class="ed-faint">
          {{ isBuiltin ? '智谱账号供应商不可修改，仅可管理其模型' : '当前供应商信息不可修改' }}
        </span>
      </div>
      <div class="ed-grid">
        <MyFieldShell label="供应商标识" class="span2">
          <div class="ed-key-row">
            <MyInput :model-value="providerKey" class="mono ed-grow" title="供应商标识不可修改" />
            <MyButton size="small" :title="'复制标识'" @click="handleCopyId">{{ copied ? '已复制' : '复制' }}</MyButton>
          </div>
        </MyFieldShell>
        <MyFieldShell label="名称">
          <MyInput v-model="eName" :disabled="infoLocked" placeholder="Provider Name" />
        </MyFieldShell>
        <MyFieldShell label="协议">
          <MySelect v-model="eKind" :options="kindOptions" :filterable="false" :disabled="infoLocked" />
        </MyFieldShell>
        <MyFieldShell label="Base URL" class="span2">
          <MyInput v-model="eUrl" class="mono" :disabled="infoLocked" placeholder="https://api.example.com/v1" />
        </MyFieldShell>
        <MyFieldShell label="API Key" class="span2">
          <div class="ed-key-row">
            <MyInput
              v-model="eKey"
              class="mono ed-grow"
              :disabled="infoLocked"
              :placeholder="provider.options.apiKey ? '••••（如需修改请输入新 Key）' : 'sk-...'"
              @update:model-value="revealedKey = null"
            />
            <MyButton size="small" :disabled="infoLocked" :title="revealedKey != null ? '隐藏 Key' : '查看明文 Key'" @click="handleReveal">
              {{ revealedKey != null ? '隐藏' : '显示' }}
            </MyButton>
          </div>
        </MyFieldShell>
      </div>

      <!-- 模型列表 -->
      <div class="ed-section-head">
        <div class="ed-models-title">
          <span class="ed-section-title">模型（{{ selModels.length }}）</span>
          <span v-if="selModels.length > 0" class="ed-faint">拖动 ⠿ 手柄调整顺序</span>
        </div>
        <MyButton variant="primary" size="small" :disabled="busy" :loading="busy" @click="handleFetch">
          <MyIcon name="Refresh" :size="13" />
          {{ busy ? '处理中…' : '拉取可用模型' }}
        </MyButton>
      </div>

      <!-- 拉取可用模型：嵌套二级弹窗（模糊搜索 + 勾选添加） -->
      <MyDialog
        :model-value="fetchCands != null"
        title="选择要添加的模型"
        :width="540"
        hide-footer
        class="fetch-dialog"
        append-to-body
        @update:model-value="closeFetchModal"
      >
        <div class="ed-fetch">
          <span class="ed-faint">已选 {{ fetchSel.size }} / 匹配 {{ filteredCands.length }}（共 {{ fetchCands?.length ?? 0 }}）</span>
          <div class="ed-fetch-toolbar">
            <MyInput v-model="fetchQuery" class="mono ed-grow" placeholder="输入关键词模糊过滤，如 glm-4.7" />
            <MyButton size="small" :disabled="filteredCands.length === 0" @click="selectAllFiltered">全选匹配</MyButton>
            <MyButton size="small" @click="fetchSel = new Set()">清空</MyButton>
          </div>
          <div class="ed-fetch-list">
            <label
              v-for="s in filteredCands"
              :key="s.id"
              class="ed-fetch-item"
              :class="{ checked: fetchSel.has(s.id), exists: provider.models[s.id] }"
            >
              <MyCheckbox
                :model-value="fetchSel.has(s.id)"
                @update:model-value="toggleFetchSel(s.id, $event === true)"
              />
              <span class="mono ed-fetch-id">{{ s.id }}</span>
              <span v-if="s.contextLength" class="ed-faint">ctx {{ s.contextLength.toLocaleString() }}</span>
              <span v-if="provider.models[s.id]" class="ed-tag dim">已存在</span>
            </label>
            <div v-if="filteredCands.length === 0" class="ui-empty">无匹配模型，换个关键词试试</div>
          </div>
          <div class="ed-fetch-footer">
            <MyButton size="small" @click="closeFetchModal">取消</MyButton>
            <MyButton variant="primary" size="small" :disabled="busy || fetchSel.size === 0" @click="handleApplySelected">
              添加选中（{{ fetchSel.size }}）
            </MyButton>
          </div>
        </div>
      </MyDialog>

      <div v-if="selModels.length === 0" class="ui-empty">无模型，点击「拉取可用模型」自动填充上下文长度</div>
      <div v-else class="ed-model-list">
        <div
          v-for="([name, m], idx) in selModels"
          :key="name"
          class="ed-model-row"
          :class="{ dragging: dragModelIdx === idx }"
          draggable="true"
          @dragstart="
            (e) => {
              // 只有手柄按下时才允许拖拽（避免误触输入框选中等操作）
              if (!modelGrip) {
                e.preventDefault();
                return;
              }
              dragModelIdx = idx;
            }
          "
          @dragover.prevent
          @drop="handleModelReorder(idx)"
          @dragend="
            () => {
              modelGrip = false;
              dragModelIdx = null;
            }
          "
        >
          <span
            class="ed-grip"
            title="拖动调整模型顺序"
            @mousedown="modelGrip = true"
            @mouseup="modelGrip = false"
          >⠿</span>
          <div class="ed-grow">
            <ModalModelRow
              :name="name"
              :model="m as ZcModel"
              :busy="busy"
              @save="
                (c, o) =>
                  onRun(async () => {
                    await models.updateModelLimit(providerKey, name, c, o);
                    toast.success(`模型「${name}」已保存`);
                    toast.warning(RESTART_HINT);
                  })
              "
              @toggle-enabled="
                (v) =>
                  onRun(async () => {
                    await models.setModelEnabled(providerKey, name, v);
                  })
              "
              @delete="
                onRun(async () => {
                  await models.removeModel(providerKey, name);
                })
              "
            />
          </div>
        </div>
      </div>

      <!-- 用量查询模板 -->
      <div class="ed-tmpl">
        <div class="ed-section-head">
          <div class="ed-models-title">
            <span class="ed-section-title">用量查询模板</span>
            <span
              v-if="codingPlan ? !!(tExtra.organizationId || tExtra.accessKeyId) : tHasTmpl"
              class="ed-tag new"
            >
              {{ codingPlan ? '附加凭据已配置' : '已配置' }}
            </span>
          </div>
          <div class="ed-tmpl-ops">
            <!-- 使用模板：一次性动作下拉（选中即复制模板内容，弹回占位） -->
            <MySelect
              :model-value="''"
              :groups="tmplGroups"
              placeholder="使用模板…"
              :clearable="false"
              class="ed-tmpl-select"
              title="选择模板（含内置预设），一键复制内容到当前表单"
              @update:model-value="(v) => handleUseTmpl(v as string)"
            />
            <MyButton v-if="tHasTmpl" size="small" @click="handleRemoveTmpl">清除</MyButton>
            <MyButton variant="primary" size="small" @click="handleSaveTmpl">保存模板</MyButton>
          </div>
        </div>

        <!-- Token Plan 供应商：自动查询，无需配置模板 -->
        <template v-if="codingPlan">
          <div class="ed-ok-box">
            <span class="ed-ok-mark">✓</span>
            <p class="ed-ok-text">
              已内置 <b>{{ codingPlan.label }}</b> 的 Token Plan 额度查询：
              自动使用该供应商的 API Key 与 Base URL，无需配置模板（与 cc-switch 一致）。
            </p>
          </div>

          <!-- 智谱团队版附加凭据 -->
          <div v-if="codingPlan.id === 'zhipu'" class="ed-cred-box">
            <div class="ed-cred-head">
              <div class="ed-models-title">
                <span class="ed-cred-title">智谱团队版（可选）</span>
                <span v-if="tExtra.organizationId" class="ed-tag new">团队版已配置</span>
              </div>
              <MyButton variant="primary" size="small" :disabled="busy" title="保存组织 / 项目 ID 到本机（写入模板附加凭据）" @click="handleSaveTmpl">
                保存
              </MyButton>
            </div>
            <div class="ed-grid">
              <MyFieldShell label="组织 ID（organizationId）">
                <MyInput :model-value="tExtra.organizationId ?? ''" class="mono" placeholder="bigmodel 团队组织 ID" @update:model-value="setExtra('organizationId', $event)" />
              </MyFieldShell>
              <MyFieldShell label="项目 ID（projectId）">
                <MyInput :model-value="tExtra.projectId ?? ''" class="mono" placeholder="bigmodel 团队项目 ID" @update:model-value="setExtra('projectId', $event)" />
              </MyFieldShell>
            </div>
            <p class="ed-faint">填写并点本框「保存」后按团队接口（type=2 + bigmodel-organization/project 头）查询；留空走个人版接口。</p>
          </div>

          <!-- 火山方舟 AK/SK -->
          <div v-if="codingPlan.id === 'volcengine'" class="ed-cred-box">
            <div class="ed-cred-head">
              <div class="ed-models-title">
                <span class="ed-cred-title">火山方舟 AccessKey（必需）</span>
                <span v-if="tExtra.accessKeyId" class="ed-tag new">AK/SK 已配置</span>
              </div>
              <MyButton variant="primary" size="small" :disabled="busy" title="保存 AK/SK 到本机（写入模板附加凭据）" @click="handleSaveTmpl">
                保存
              </MyButton>
            </div>
            <div class="ed-grid">
              <MyFieldShell label="AccessKeyId">
                <MyInput :model-value="tExtra.accessKeyId ?? ''" class="mono" placeholder="火山控制台 IAM AccessKey ID" @update:model-value="setExtra('accessKeyId', $event)" />
              </MyFieldShell>
              <MyFieldShell label="SecretAccessKey">
                <MyInput
                  :model-value="tExtra.secretAccessKey ?? ''"
                  class="mono"
                  type="password"
                  autocomplete="new-password"
                  placeholder="火山控制台 IAM Secret Access Key"
                  @update:model-value="setExtra('secretAccessKey', $event)"
                />
              </MyFieldShell>
            </div>
            <p class="ed-faint">
              火山用量查询需账号级 AccessKey ID / Secret（与推理 API Key 不同），填写后点本框「保存」保存到本机。请在火山引擎控制台右上角账号菜单
              →「API访问密钥」中创建。
              <br />
              密钥创建地址：
              <a class="ed-link" @click="openUrl('https://console.volcengine.com/iam/keymanage')">https://console.volcengine.com/iam/keymanage</a>
            </p>
          </div>
        </template>

        <!-- 通用模板表单 -->
        <template v-else>
          <div class="ed-auth-row">
            <span class="ed-auth-label">用量查询方式</span>
            <MyButton
              v-for="o in [
                { v: 'appkey', label: 'API Key' },
                { v: 'token', label: '登录 Token' },
              ]"
              :key="o.v"
              size="small"
              :class="{ 'is-active': (tTmpl.authMode ?? 'appkey') === o.v }"
              @click="tTmpl = { ...tTmpl, authMode: o.v as 'appkey' | 'token' }"
            >
              {{ o.label }}
            </MyButton>
            <span class="ed-faint">
              {{ isTokenMode ? TOKEN_HINT : APIKEY_HINT }}
            </span>
          </div>
          <p class="ed-faint ed-mb">按 dot path（如 <span class="mono">data.balance</span>）提取总额/已用/剩余。</p>

          <div class="ed-grid">
            <MyFieldShell label="名称">
              <MyInput v-model="tTmpl.name" />
            </MyFieldShell>
            <MyFieldShell label="方法">
              <MySelect v-model="tTmpl.method" :options="methodOptions" :filterable="false" />
            </MyFieldShell>
            <MyFieldShell label="URL" class="span2">
              <MyInput v-model="tTmpl.url" class="mono" :placeholder="URL_PLACEHOLDER" />
            </MyFieldShell>
            <MyFieldShell label="Headers (JSON，值支持 {{apiKey}}/{{token}})" class="span2">
              <MyInput v-model="tTmpl.headersJson" type="textarea" :rows="2" class="mono" :placeholder="HEADERS_PLACEHOLDER" />
            </MyFieldShell>
            <MyFieldShell label="Body（POST 时使用，JSON 字符串）" class="span2">
              <MyInput v-model="tTmpl.body" type="textarea" :rows="2" class="mono" />
            </MyFieldShell>
            <MyFieldShell label="总额 path">
              <MyInput v-model="tTmpl.totalPath" class="mono" placeholder="data.total" />
            </MyFieldShell>
            <MyFieldShell label="已用 path">
              <MyInput v-model="tTmpl.usedPath" class="mono" placeholder="data.used" />
            </MyFieldShell>
            <MyFieldShell label="剩余 path" class="span2">
              <MyInput v-model="tTmpl.remainingPath" class="mono" placeholder="data.remaining" />
            </MyFieldShell>
            <MyFieldShell label="配额单位">
              <MySelect :model-value="tTmpl.unit ?? ''" :options="unitOptions" :filterable="false" @update:model-value="tTmpl.unit = ($event as string) || undefined" />
            </MyFieldShell>
            <MyFieldShell label="重置时间 path">
              <MyInput v-model="tTmpl.resetTimePath" class="mono" placeholder="data.resetTime" />
            </MyFieldShell>
          </div>

          <p class="ed-faint ed-gap">每5小时窗口（可选，Token Plan 供应商）：从同一响应里再提取一组「每5小时使用额度」，与每周桶组成双环展示；未配置或提取不到则不展示。</p>
          <div class="ed-grid">
            <MyFieldShell label="5小时总额 path">
              <MyInput v-model="tTmpl.fiveHourTotalPath" class="mono" placeholder="data.interval_total" />
            </MyFieldShell>
            <MyFieldShell label="5小时已用 path">
              <MyInput v-model="tTmpl.fiveHourUsedPath" class="mono" placeholder="data.interval_used" />
            </MyFieldShell>
            <MyFieldShell label="5小时剩余 path">
              <MyInput v-model="tTmpl.fiveHourRemainingPath" class="mono" placeholder="data.interval_remaining" />
            </MyFieldShell>
            <MyFieldShell label="5小时重置时间 path">
              <MyInput v-model="tTmpl.fiveHourResetTimePath" class="mono" placeholder="data.interval_reset" />
            </MyFieldShell>
          </div>

          <p class="ed-faint ed-gap">每周窗口（可选）：从同一响应里再提取一组「每周使用额度」；未配置或提取不到则不展示。</p>
          <div class="ed-grid">
            <MyFieldShell label="每周总额 path">
              <MyInput v-model="tTmpl.weeklyTotalPath" class="mono" placeholder="data.weekly_total" />
            </MyFieldShell>
            <MyFieldShell label="每周已用 path">
              <MyInput v-model="tTmpl.weeklyUsedPath" class="mono" placeholder="data.weekly_used" />
            </MyFieldShell>
            <MyFieldShell label="每周剩余 path">
              <MyInput v-model="tTmpl.weeklyRemainingPath" class="mono" placeholder="data.weekly_remaining" />
            </MyFieldShell>
            <MyFieldShell label="每周重置时间 path">
              <MyInput v-model="tTmpl.weeklyResetTimePath" class="mono" placeholder="data.weekly_reset" />
            </MyFieldShell>
          </div>

          <p class="ed-faint ed-gap">月限额（可选，仅部分供应商有）：从同一响应里再提取一组「每月使用额度」，额度行会追加「每月 剩xx%」；未配置或提取不到则不展示。</p>
          <div class="ed-grid">
            <MyFieldShell label="每月总额 path">
              <MyInput v-model="tTmpl.monthlyTotalPath" class="mono" placeholder="data.monthly_total" />
            </MyFieldShell>
            <MyFieldShell label="每月已用 path">
              <MyInput v-model="tTmpl.monthlyUsedPath" class="mono" placeholder="data.monthly_used" />
            </MyFieldShell>
            <MyFieldShell label="每月剩余 path">
              <MyInput v-model="tTmpl.monthlyRemainingPath" class="mono" placeholder="data.monthly_remaining" />
            </MyFieldShell>
            <MyFieldShell label="每月重置时间 path">
              <MyInput v-model="tTmpl.monthlyResetTimePath" class="mono" placeholder="data.monthly_reset" />
            </MyFieldShell>
          </div>

          <!-- Token 获取（仅「登录 Token」方式显示） -->
          <div v-if="isTokenMode" class="ed-token-box">
            <div class="ed-token-head">
              <span class="ed-cred-title">Token 获取</span>
              <span class="ed-auth-label">
                Token：
                <span v-if="tToken?.hasToken" class="ed-ok-text">
                  已获取{{ tToken.fetchedAt ? `（${new Date(tToken.fetchedAt).toLocaleString()}）` : '' }}
                </span>
                <span v-else class="ed-faint">未获取</span>
              </span>
            </div>

            <div v-if="tToken?.hasToken" class="ed-key-row ed-mb">
              <div
                class="ed-token-mask mono"
                :title="'最终获取到的 Token'"
                :style="{ color: revealedToken != null ? 'var(--text-primary)' : 'var(--text-tertiary)' }"
              >
                {{ revealedToken != null ? revealedToken || '（空）' : '••••••••••••' }}
              </div>
              <MyButton size="small" :title="revealedToken != null ? '隐藏 Token' : '显示明文 Token'" @click="handleRevealToken">
                {{ revealedToken != null ? '隐藏' : '显示' }}
              </MyButton>
              <MyButton v-if="revealedToken != null && revealedToken" size="small" :title="'复制 Token'" @click="handleCopyToken">
                {{ tokenCopied ? '已复制' : '复制' }}
              </MyButton>
            </div>

            <div class="ed-grid">
              <MyFieldShell label="登录页 URL" class="span2">
                <MyInput v-model="tTmpl.loginUrl" placeholder="https://platform.example.com/login" />
              </MyFieldShell>
              <MyFieldShell label="Token 提取方式" class="span2">
                <MyInput v-model="tTmpl.tokenSource" class="mono" placeholder="cookie:session_id 或 cookie:（留空取全部）或 localstorage:user#token" />
              </MyFieldShell>
            </div>

            <div class="ed-token-actions">
              <MyButton v-if="tToken?.hasToken" size="small" @click="handleClearToken">清除 Token</MyButton>
              <MyButton size="small" :title="'跳过登录弹窗，直接粘贴 Token（从浏览器 DevTools / 其他工具复制）'" @click="showManualToken = !showManualToken">
                {{ showManualToken ? '收起' : '手动输入 Token' }}
              </MyButton>
              <MyButton size="small" @click="handleLoginToken">登录获取 Token</MyButton>
            </div>

            <div v-if="showManualToken" class="ed-manual">
              <p class="ed-faint">
                直接粘贴 Token（无需走登录窗）：登录平台后从浏览器 DevTools
                的 Application → Cookies / LocalStorage 取出，或从其他工具复制。保存后写入系统凭证库，模板里用
                <span class="mono">{{ TOKEN_REF }}</span> 引用。
              </p>
              <MyInput v-model="manualTokenDraft" type="textarea" :rows="2" class="mono" placeholder="粘贴完整 Token / Cookie 串（≥8 字符）" />
              <div class="ed-token-actions">
                <MyButton size="small" @click="manualTokenDraft = ''; showManualToken = false">取消</MyButton>
                <MyButton variant="primary" size="small" @click="handleSaveManualToken">保存 Token</MyButton>
              </div>
            </div>

            <p class="ed-faint ed-mt">
              两种获取方式：「登录获取 Token」弹出该平台登录页，在登录窗中完成登录（含验证码 / 两步验证）即可；「手动输入
              Token」跳过登录窗，直接粘贴从浏览器 DevTools / 其他工具复制的 Token。两者都存系统凭证库，模板中用
              <span class="mono">{{ TOKEN_REF }}</span> 引用。提取方式：
              <span class="mono">cookie:名称</span>（Windows 下支持 HttpOnly cookie；留空或写
              <span class="mono">*</span> 取完整 cookie 串，适合 Cookie 头认证的供应商）或
              <span class="mono">localstorage:key</span>（值为 JSON 加 <span class="mono">#字段.路径</span>）。
            </p>
          </div>
        </template>
      </div>

      <!-- 底部操作栏 -->
      <div class="ed-footer">
        <MyButton size="small" @click="emit('close')">关闭</MyButton>
        <MyButton v-if="!isBuiltin" variant="primary" size="small" :disabled="busy || isCurrent" @click="handleSaveProvider">
          保存供应商
        </MyButton>
      </div>
    </div>
  </MyDialog>
</template>

<style scoped>
.ed {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.ed-head {
  display: flex;
  gap: 8px;
}
.ed-section-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
}
.ed-section-title {
  font-weight: 600;
  font-size: 14px;
}
.ed-models-title {
  display: flex;
  align-items: baseline;
  gap: 8px;
}
.ed-faint {
  color: var(--text-tertiary);
  font-size: 11px;
  line-height: 1.6;
}
.ed-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 10px;
}
.span2 {
  grid-column: 1 / -1;
}
.mono :deep(input),
.mono :deep(textarea) {
  font-family: var(--font-mono);
}
.ed-key-row {
  display: flex;
  gap: 6px;
  align-items: center;
}
.ed-grow {
  flex: 1;
  min-width: 0;
}
.ed-model-list {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.ed-model-row {
  display: flex;
  gap: 6px;
  align-items: center;
}
.ed-model-row.dragging {
  opacity: 0.5;
}
.ed-grip {
  cursor: grab;
  color: var(--text-tertiary);
  font-size: 14px;
  line-height: 1;
  user-select: none;
  flex-shrink: 0;
}
.ed-tmpl {
  padding: 12px;
  border: 1px solid var(--border-subtle);
  border-radius: 10px;
  background: var(--bg-surface);
}
.ed-tmpl-ops {
  display: flex;
  gap: 6px;
  align-items: center;
}
.ed-tmpl-select {
  max-width: 190px;
}
.ed-tag {
  font-size: 11px;
  padding: 0 8px;
  line-height: 18px;
  border-radius: 999px;
}
.ed-tag.new {
  background: var(--accent-subtle);
  color: var(--accent);
}
.ed-tag.dim {
  background: var(--border-subtle);
  color: var(--text-tertiary);
}
.ed-ok-box {
  display: flex;
  gap: 8px;
  align-items: flex-start;
  padding: 8px 10px;
  border-radius: 8px;
  background: oklch(0.7 0.17 160 / 0.08);
  border: 1px solid oklch(0.7 0.17 160 / 0.25);
  margin-bottom: 10px;
}
.ed-ok-mark {
  color: var(--success);
  line-height: 20px;
}
.ed-ok-text {
  margin: 0;
  color: var(--success);
  font-size: 12px;
  line-height: 1.6;
}
.ed-cred-box {
  margin-top: 10px;
  padding: 10px;
  border-radius: 8px;
  border: 1px solid var(--border-subtle);
}
.ed-cred-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  margin-bottom: 8px;
}
.ed-cred-title {
  font-weight: 600;
  font-size: 13px;
}
.ed-auth-row {
  display: flex;
  gap: 8px;
  align-items: center;
  margin-bottom: 10px;
  flex-wrap: wrap;
}
.ed-auth-row :deep(.is-active) {
  border-color: var(--accent);
  color: var(--accent);
}
.ed-auth-label {
  font-size: 12px;
  color: var(--text-secondary);
}
.ed-mb {
  margin-bottom: 8px;
}
.ed-mt {
  margin-top: 8px;
}
.ed-gap {
  margin: 10px 0 6px;
}
.ed-fetch {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.ed-fetch-toolbar {
  display: flex;
  gap: 6px;
  align-items: center;
  flex-wrap: wrap;
}
.ed-fetch-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
  max-height: 320px;
  overflow-y: auto;
}
.ed-fetch-item {
  display: flex;
  gap: 8px;
  align-items: center;
  padding: 4px 8px;
  border-radius: 6px;
  cursor: pointer;
}
.ed-fetch-item.checked {
  background: var(--accent-subtle);
}
.ed-fetch-item.exists {
  opacity: 0.6;
}
.ed-fetch-id {
  font-family: var(--font-mono);
  font-size: 12px;
}
.ed-fetch-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
.ed-token-box {
  margin-top: 10px;
  padding: 10px;
  border-radius: 8px;
  border: 1px solid var(--border-subtle);
}
.ed-token-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  flex-wrap: wrap;
  margin-bottom: 8px;
}
.ed-token-mask {
  flex: 1;
  min-width: 0;
  height: 28px;
  display: flex;
  align-items: center;
  padding: 0 8px;
  border-radius: 6px;
  border: 1px solid var(--border-subtle);
  font-size: 11px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.ed-token-actions {
  display: flex;
  gap: 8px;
  margin-top: 8px;
  justify-content: flex-end;
  flex-wrap: wrap;
}
.ed-manual {
  margin-top: 8px;
  padding: 8px;
  border-radius: 6px;
  border: 1px dashed var(--border-base);
  background: var(--bg-surface);
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.ed-link {
  color: var(--accent);
  cursor: pointer;
  text-decoration: underline;
}
.ed-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>
