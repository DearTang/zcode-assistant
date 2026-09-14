<script setup lang="ts">
import { ref } from 'vue'
import { MyButton, MyDialog, MyInput } from 'myui'

/** 未命中模型的上下文确认弹窗（默认 200k，可修改） */
const props = defineProps<{
  models: string[]
  busy: boolean
}>()
const emit = defineEmits<{ close: []; confirm: [overrides: Record<string, number>] }>()
defineOptions({ name: 'ContextConfirmModal' })

const values = ref<Record<string, number>>(
  Object.fromEntries(props.models.map((m) => [m, 200000])),
)

function setValue(m: string, raw: string): void {
  values.value[m] = Math.max(1, Math.floor(Number(raw) || 0))
}
</script>

<template>
  <MyDialog
    :model-value="true"
    title="确认模型上下文"
    :width="560"
    hide-footer
    @cancel="emit('close')"
    @update:model-value="emit('close')"
  >
    <div class="cc">
      <span class="cc-sub">{{ models.length }} 个模型未匹配到目录</span>
      <p class="cc-desc">
        以下模型在 OpenRouter 目录与内置规格表中均未命中，请确认上下文长度（已预填
        200000，可修改；其余命中的模型将按目录真实值写入）。
      </p>
      <div class="cc-list">
        <div v-for="m in models" :key="m" class="cc-row">
          <span class="cc-name" :title="m">{{ m }}</span>
          <MyInput
            :model-value="values[m]"
            type="number"
            :min="1"
            :step="1000"
            class="cc-input"
            @update:model-value="setValue(m, String($event))"
          />
        </div>
      </div>
      <div class="cc-footer">
        <MyButton size="small" :disabled="busy" @click="emit('close')">取消</MyButton>
        <MyButton variant="primary" size="small" :disabled="busy" :loading="busy" @click="emit('confirm', values)">
          {{ busy ? '导入中…' : '确认并导入' }}
        </MyButton>
      </div>
    </div>
  </MyDialog>
</template>

<style scoped>
.cc {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.cc-sub {
  color: var(--text-secondary);
  font-size: 12px;
}
.cc-desc {
  margin: 0;
  color: var(--text-secondary);
  font-size: 12px;
  line-height: 1.7;
}
.cc-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
  max-height: 320px;
  overflow-y: auto;
}
.cc-row {
  display: flex;
  gap: 8px;
  align-items: center;
  padding: 6px 8px;
  border-radius: 6px;
  border: 1px solid var(--border-subtle);
}
.cc-name {
  font-family: var(--font-mono);
  font-size: 11px;
  flex: 1;
  min-width: 0;
  word-break: break-all;
}
.cc-input {
  width: 140px;
  flex: none;
}
.cc-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>
