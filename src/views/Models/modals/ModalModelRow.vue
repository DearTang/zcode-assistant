<script setup lang="ts">
import { ref } from 'vue'
import { MyButton, MyIcon, MyInput, MyToggle } from 'myui'
import { DEFAULT_CONTEXT } from '../shared'
import type { ZcModel } from '@/types'

/** 弹窗内的模型行（context 默认 200000） */
const props = defineProps<{
  name: string
  model: ZcModel
  busy: boolean
}>()
const emit = defineEmits<{
  save: [ctx?: number, out?: number]
  'toggle-enabled': [enabled: boolean]
  delete: []
}>()
defineOptions({ name: 'ModalModelRow' })

// 没有上下文大小时默认 200000
const ctx = ref(props.model.limit?.context?.toString() ?? String(DEFAULT_CONTEXT))
const out = ref(props.model.limit?.output?.toString() ?? '')
const enabled = props.model.enabled !== false

function onSave(): void {
  emit('save', ctx.value ? Number(ctx.value) : DEFAULT_CONTEXT, out.value ? Number(out.value) : undefined)
}
</script>

<template>
  <div class="mr" :class="{ dimmed: !enabled }">
    <span class="mr-name">{{ name }}</span>
    <div class="mr-ops">
      <label class="mr-field">
        ctx
        <MyInput v-model="ctx" class="mr-input" :placeholder="String(DEFAULT_CONTEXT)" title="上下文 token（留空默认 200000）" />
      </label>
      <label class="mr-field">
        out
        <MyInput v-model="out" class="mr-input out" placeholder="output" title="最大输出 token" />
      </label>
      <MyToggle
        :model-value="enabled"
        :title="enabled ? '已启用' : '已禁用'"
        @click.stop
        @update:model-value="emit('toggle-enabled', $event === true)"
      />
      <MyButton size="small" :disabled="busy" @click="onSave">保存</MyButton>
      <button class="mr-del" type="button" :disabled="busy" title="删除模型" @click="emit('delete')">
        <MyIcon name="Trash" :size="13" />
      </button>
    </div>
  </div>
</template>

<style scoped>
.mr {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  padding: 6px 0;
  border-bottom: 1px solid var(--border-subtle);
}
.mr.dimmed {
  opacity: 0.5;
}
.mr-name {
  font-family: var(--font-mono);
  font-size: 13px;
}
.mr-ops {
  display: flex;
  align-items: center;
  gap: 6px;
}
.mr-field {
  display: flex;
  align-items: center;
  gap: 4px;
  font-size: 11px;
  color: var(--text-tertiary);
}
.mr-input {
  width: 110px;
}
.mr-input.out {
  width: 100px;
}
.mr-input :deep(input) {
  height: 28px;
  font-family: var(--font-mono);
}
.mr-del {
  width: 28px;
  height: 28px;
  display: inline-grid;
  place-items: center;
  border: 1px solid transparent;
  border-radius: 7px;
  background: transparent;
  color: var(--text-secondary);
  cursor: pointer;
}
.mr-del:hover {
  color: var(--text-primary);
  background: var(--surface-translucent-hover);
}
</style>
