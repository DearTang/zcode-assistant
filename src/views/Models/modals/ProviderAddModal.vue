<script setup lang="ts">
import { computed, ref } from 'vue'
import { MyButton, MyDialog, MyFieldShell, MyInput, MySelect } from 'myui'
import { models, openUrl } from '@/api'
import { PRESET_CATEGORY_LABELS, PRESET_PROVIDERS, type ProviderPreset } from '@/presets/providerPresets'
import type { ProviderKind } from '@/types'

/** 添加供应商弹窗：预设一键填充 + 测试连接 + MyDialog error 模幅 */
const emit = defineEmits<{ close: []; added: [msg: string] }>()
defineOptions({ name: 'ProviderAddModal' })

const fId = ref('')
const fName = ref('')
const fKind = ref<ProviderKind>('openai-compatible')
const fUrl = ref('')
const fKey = ref('')
// 选中的预设供应商：自动填充 名称/协议/Base URL/建议标识
const preset = ref<ProviderPreset | null>(null)
// 预设自动填入的标识（标识为空或仍是上次自动值时可随预设切换覆盖，手改过则保留）
let presetId = ''

const kindOptions = [
  { label: 'OpenAI 兼容', value: 'openai-compatible' },
  { label: 'OpenAI', value: 'openai' },
  { label: 'Anthropic', value: 'anthropic' },
]

/** 预设分组（myui v0.9.0 MySelect groups） */
const presetGroups = computed(() =>
  PRESET_CATEGORY_LABELS.map((g) => ({
    label: g.label,
    options: PRESET_PROVIDERS.filter((x) => x.category === g.key).map((p) => ({ label: p.name, value: p.id })),
  })),
)

/** 应用预设：一键填充供应商名称、接口格式、Base URL 与建议标识 */
function applyPreset(p: ProviderPreset): void {
  preset.value = p
  fName.value = p.name
  fKind.value = p.kind
  fUrl.value = p.baseUrl
  if (!fId.value.trim() || fId.value.trim() === presetId) {
    fId.value = p.id
    presetId = p.id
  }
}

const busy = ref(false)
const testing = ref(false)
const testResult = ref<{ ok: boolean; message: string } | null>(null)
// 弹窗内顶部错误提示（校验 / 测试 / 添加失败均在此显示）——走 MyDialog error 模幅
const error = ref<string | null>(null)

async function handleTest(): Promise<void> {
  error.value = null
  if (!fUrl.value.trim()) {
    error.value = '请先填写 Base URL'
    return
  }
  testing.value = true
  testResult.value = null
  try {
    const r = await models.testConnection(fUrl.value, fKey.value, fKind.value)
    testResult.value = { ok: r.ok, message: r.message }
  } catch (e: unknown) {
    testResult.value = { ok: false, message: typeof e === 'string' ? e : '测试请求失败' }
  } finally {
    testing.value = false
  }
}

async function handleAdd(): Promise<void> {
  error.value = null
  if (!fId.value.trim() || !fName.value.trim() || !fUrl.value.trim()) {
    error.value = '请填写供应商标识、名称和 Base URL'
    return
  }
  busy.value = true
  try {
    await models.addProvider(fName.value, fKind.value, fUrl.value, fKey.value, fId.value)
    emit('added', `已添加供应商「${fName.value}」`)
  } catch (e: unknown) {
    error.value = typeof e === 'string' ? e : '添加失败'
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <MyDialog
    :model-value="true"
    title="添加供应商"
    :width="640"
    :error="error ?? undefined"
    hide-footer
    @cancel="emit('close')"
    @update:model-value="emit('close')"
  >
    <div class="pa">
      <div class="pa-grid">
        <!-- 预设供应商：分组下拉（myui v0.9.0 MySelect groups） -->
        <MyFieldShell label="预设供应商" class="span2">
          <MySelect
            :model-value="preset?.id ?? ''"
            :groups="presetGroups"
            placeholder="选择预设自动填充…"
            :clearable="false"
            title="预设数据来自 cc-switch（github.com/farion1231/cc-switch，MIT），可在此基础上修改"
            @update:model-value="(v) => { const p = PRESET_PROVIDERS.find((x) => x.id === v); if (p) applyPreset(p) }"
          />
        </MyFieldShell>

        <!-- 选中预设后展示官网 / 获取 API Key 链接 -->
        <div v-if="preset" class="pa-links span2">
          <span class="pa-faint">官网</span>
          <span class="pa-link" :title="preset.websiteUrl" @click="openUrl(preset.websiteUrl)">{{ preset.websiteUrl }}</span>
          <template v-if="preset.apiKeyUrl && preset.apiKeyUrl !== preset.websiteUrl">
            <span class="pa-faint">获取 API Key</span>
            <span class="pa-link" :title="preset.apiKeyUrl" @click="openUrl(preset.apiKeyUrl ?? '')">{{ preset.apiKeyUrl }}</span>
          </template>
        </div>

        <MyFieldShell label="供应商标识" class="span2">
          <MyInput v-model="fId" class="mono" placeholder="如 my-glm（字母/数字/-/_，创建后不可修改）" />
        </MyFieldShell>
        <MyFieldShell label="名称">
          <MyInput v-model="fName" placeholder="My Provider" />
        </MyFieldShell>
        <MyFieldShell label="协议">
          <MySelect v-model="fKind" :options="kindOptions" :filterable="false" />
        </MyFieldShell>
        <MyFieldShell label="Base URL" class="span2">
          <MyInput v-model="fUrl" class="mono" placeholder="https://api.example.com/v1" />
        </MyFieldShell>
        <MyFieldShell label="API Key" class="span2">
          <MyInput v-model="fKey" class="mono" placeholder="sk-..." />
        </MyFieldShell>
      </div>

      <!-- 测试连接 -->
      <div class="pa-test">
        <MyButton size="small" :disabled="testing" :loading="testing" @click="handleTest">
          {{ testing ? '测试中…' : '测试连接' }}
        </MyButton>
        <span v-if="testResult" class="pa-test-result" :class="testResult.ok ? 'ok' : 'bad'">
          {{ testResult.ok ? '✓ ' : '✕ ' }}{{ testResult.message }}
        </span>
      </div>

      <div class="pa-footer">
        <MyButton size="small" @click="emit('close')">取消</MyButton>
        <MyButton variant="primary" size="small" :disabled="busy" :loading="busy" @click="handleAdd">
          {{ busy ? '添加中…' : '添加' }}
        </MyButton>
      </div>
    </div>
  </MyDialog>
</template>

<style scoped>
.pa {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.pa-error {
  padding: 8px 12px;
  border-left: 3px solid var(--danger);
  border-radius: 6px;
  background: oklch(0.62 0.24 27 / 0.08);
  color: var(--danger);
  font-size: 12px;
}
.pa-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 12px;
}
.span2 {
  grid-column: 1 / -1;
}
.mono :deep(input) {
  font-family: var(--font-mono);
}
.pa-links {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
  font-size: 11px;
}
.pa-faint {
  color: var(--text-tertiary);
}
.pa-link {
  font-family: var(--font-mono);
  color: var(--accent);
  cursor: pointer;
  word-break: break-all;
}
.pa-test {
  display: flex;
  gap: 8px;
  align-items: center;
  flex-wrap: wrap;
}
.pa-test-result {
  font-family: var(--font-mono);
  font-size: 11px;
}
.pa-test-result.ok {
  color: var(--accent);
}
.pa-test-result.bad {
  color: var(--danger);
}
.pa-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>
