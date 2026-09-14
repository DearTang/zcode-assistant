<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { MyButton, MyInput, MyPanel } from 'myui'
import RestartBar from '@/components/RestartBar.vue'
import { zcode } from '@/api'
import { toast } from '@/composables/toast'
import type { ModelRetryConfig } from '@/types'

defineOptions({ name: 'ZcodeSettingsView' })

/** ZCode 内置默认值（zcode.cjs resolveAiSdkModelRetryOptions），输入框留空 = 跟随默认 */
const ZC_DEFAULTS = {
  maxRetries: 10,
  baseDelayMs: 2000,
  backoffFactor: 2,
  maxDelayMs: 60000,
}

const FIELDS = [
  { key: 'maxRetries', label: '最大重试次数', def: ZC_DEFAULTS.maxRetries, unit: '次', step: 1, hint: '接口失败后的重试上限（0 = 失败不重试，直接报错）' },
  { key: 'baseDelayMs', label: '起步延迟', def: ZC_DEFAULTS.baseDelayMs, unit: 'ms', step: 100, hint: '第一次重试前等待的时间' },
  { key: 'backoffFactor', label: '退避倍数', def: ZC_DEFAULTS.backoffFactor, unit: '×', step: 0.5, hint: '每次重试后延迟翻倍的倍率' },
  { key: 'maxDelayMs', label: '延迟上限', def: ZC_DEFAULTS.maxDelayMs, unit: 'ms', step: 1000, hint: '单次重试等待的封顶时间' },
] as const

type EditKey = (typeof FIELDS)[number]['key']
type EditState = Record<EditKey, string>

const saved = ref<ModelRetryConfig | null>(null)
// 编辑态用字符串承载（允许留空 = 跟随 ZCode 默认）
const edit = ref<EditState>({ maxRetries: '', baseDelayMs: '', backoffFactor: '', maxDelayMs: '' })
const loading = ref(true)
const saving = ref(false)
const dirty = ref(false)

onMounted(() => {
  zcode
    .getRetryConfig()
    .then((cfg) => {
      saved.value = cfg
      edit.value = {
        maxRetries: cfg.maxRetries?.toString() ?? '',
        baseDelayMs: cfg.baseDelayMs?.toString() ?? '',
        backoffFactor: cfg.backoffFactor?.toString() ?? '',
        maxDelayMs: cfg.maxDelayMs?.toString() ?? '',
      }
    })
    .catch((e) => toast.error(`读取重试配置失败：${String(e)}`))
    .finally(() => (loading.value = false))
})

function setField(key: EditKey, v: string): void {
  edit.value[key] = v
  dirty.value = true
}

/** 编辑态 → 配置对象；非法输入弹错返回 null */
function buildConfig(): ModelRetryConfig | null {
  const parse = (key: EditKey, label: string): number | null => {
    const raw = edit.value[key].trim()
    if (!raw) return null
    const n = Number(raw)
    if (!Number.isFinite(n) || n < 0) {
      toast.error(`「${label}」不是有效的非负数字`)
      return null
    }
    return n
  }
  const maxRetries = parse('maxRetries', '最大重试次数')
  if (maxRetries === null && edit.value.maxRetries.trim()) return null
  const baseDelayMs = parse('baseDelayMs', '起步延迟')
  if (baseDelayMs === null && edit.value.baseDelayMs.trim()) return null
  const backoffFactor = parse('backoffFactor', '退避倍数')
  if (backoffFactor === null && edit.value.backoffFactor.trim()) return null
  const maxDelayMs = parse('maxDelayMs', '延迟上限')
  if (maxDelayMs === null && edit.value.maxDelayMs.trim()) return null
  return { maxRetries, baseDelayMs, backoffFactor, maxDelayMs }
}

async function save(): Promise<void> {
  const cfg = buildConfig()
  if (!cfg) return
  saving.value = true
  try {
    saved.value = await zcode.setRetryConfig(cfg)
    dirty.value = false
    toast.success('重试配置已写入用户环境变量')
  } catch (e: unknown) {
    toast.error(typeof e === 'string' ? e : `保存失败：${String(e)}`)
  } finally {
    saving.value = false
  }
}

async function resetDefaults(): Promise<void> {
  edit.value = { maxRetries: '', baseDelayMs: '', backoffFactor: '', maxDelayMs: '' }
  saving.value = true
  try {
    saved.value = await zcode.setRetryConfig({
      maxRetries: null,
      baseDelayMs: null,
      backoffFactor: null,
      maxDelayMs: null,
    })
    dirty.value = false
    toast.success('已恢复 ZCode 默认重试策略（清除全部覆盖值）')
  } catch (e: unknown) {
    toast.error(typeof e === 'string' ? e : `恢复失败：${String(e)}`)
  } finally {
    saving.value = false
  }
}

/** 由当前编辑值生成延迟序列预览（与 zcode.cjs 同公式：base × factor^n，封顶 max） */
const preview = computed(() => {
  const v = edit.value
  const retries = Math.max(1, Math.min(10, Math.round(Number(v.maxRetries) || ZC_DEFAULTS.maxRetries)))
  const base = Number(v.baseDelayMs) || ZC_DEFAULTS.baseDelayMs
  const factor = Number(v.backoffFactor) || ZC_DEFAULTS.backoffFactor
  const cap = Number(v.maxDelayMs) || ZC_DEFAULTS.maxDelayMs
  const seq: string[] = []
  let d = base
  for (let i = 0; i < Math.min(retries, 4); i++) {
    seq.push(d >= 1000 ? `${(d / 1000).toFixed(1)}s` : `${Math.round(d)}ms`)
    d = Math.min(d * factor, cap)
  }
  if (retries > 4) seq.push('…')
  return seq.join(' → ')
})
</script>

<template>
  <div class="zs">
    <MyPanel title="模型调用重试">
      <template #actions>
        <span class="zs-tag">ZCODE_MODEL_RETRY_* · 全局生效</span>
      </template>

      <p class="zs-desc">
        ZCode 调用模型接口失败时默认最多重试 {{ ZC_DEFAULTS.maxRetries }} 次（起步
        {{ ZC_DEFAULTS.baseDelayMs / 1000 }}s、×{{ ZC_DEFAULTS.backoffFactor }} 指数退避、
        单次封顶 {{ ZC_DEFAULTS.maxDelayMs / 1000 }}s）。此处通过官方环境变量调整，
        对所有模型 / 供应商统一生效；可重试的错误类型（限流、服务端错误、网络中断、
        流超时）由 ZCode 内部判定，4xx 等不可重试错误不会消耗次数。
      </p>

      <div v-if="loading" class="ui-empty">加载中…</div>
      <template v-else>
        <div class="zs-grid">
          <div v-for="f in FIELDS" :key="f.key" class="zs-field">
            <label class="zs-label">
              {{ f.label }}
              <span class="zs-def">默认 {{ f.def }}{{ f.unit }}</span>
            </label>
            <div class="zs-input-row">
              <!-- myui MyNumberField 无 unit 后缀（缺口已记反馈清单），此处用 MyInput + 后缀 -->
              <MyInput
                :model-value="edit[f.key]"
                type="number"
                :min="0"
                :step="f.step"
                class="zs-mono-input"
                :placeholder="`默认 ${f.def}`"
                @update:model-value="setField(f.key, String($event))"
              />
              <span class="zs-unit">{{ f.unit }}</span>
            </div>
            <span class="zs-hint">{{ f.hint }}</span>
          </div>
        </div>

        <div class="zs-foot">
          <div class="zs-preview">
            重试延迟示例：{{ preview }}
            <span>（留空 = 跟随 ZCode 默认，未设置的项不写入环境变量）</span>
          </div>
          <div class="zs-actions">
            <MyButton size="small" :disabled="saving" @click="resetDefaults">恢复 ZCode 默认</MyButton>
            <MyButton variant="primary" size="small" :disabled="saving || !dirty" @click="save">
              {{ saving ? '写入中…' : dirty ? '保存并生效' : '已与当前配置一致' }}
            </MyButton>
          </div>
        </div>
      </template>
    </MyPanel>

    <RestartBar v-if="saved && !dirty" hint="重试配置写入用户环境变量，重启 ZCode 后对新会话生效" />
  </div>
</template>

<style scoped>
.zs {
  display: grid;
  gap: 12px;
}
.zs-tag {
  color: var(--text-tertiary);
  font-size: 11px;
}
.zs-desc {
  margin: 0 0 14px;
  color: var(--text-secondary);
  font-size: 12px;
  line-height: 1.7;
}
.zs-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
  gap: 16px;
}
.zs-field {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.zs-label {
  font-size: 13px;
  color: var(--text-secondary);
}
.zs-def {
  margin-left: 6px;
  color: var(--text-tertiary);
  font-size: 11px;
}
.zs-input-row {
  display: flex;
  align-items: center;
  gap: 6px;
}
.zs-mono-input {
  width: 130px;
}
.zs-mono-input :deep(input) {
  font-family: var(--font-mono);
}
.zs-unit {
  color: var(--text-tertiary);
  font-size: 11px;
}
.zs-hint {
  color: var(--text-tertiary);
  font-size: 11px;
}
.zs-foot {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  flex-wrap: wrap;
  margin-top: 14px;
}
.zs-preview {
  color: var(--text-tertiary);
  font-size: 11px;
}
.zs-actions {
  display: flex;
  gap: 8px;
}
</style>
