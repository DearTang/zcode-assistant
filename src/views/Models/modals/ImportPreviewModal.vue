<script setup lang="ts">
import { computed, ref } from 'vue'
import { MyButton, MyCheckbox, MyDialog } from 'myui'
import type { ProviderPreview } from '@/api'

/** 导入预览弹窗：列出解析出的全部供应商，默认全选，确认后导入选中项 */
const props = defineProps<{
  items: ProviderPreview[]
  busy: boolean
}>()
const emit = defineEmits<{ close: []; confirm: [ids: string[]] }>()
defineOptions({ name: 'ImportPreviewModal' })

// 默认全选
const sel = ref(new Set(props.items.map((i) => i.id)))
const all = computed(() => sel.value.size === props.items.length)

function toggle(id: string, on: boolean): void {
  const next = new Set(sel.value)
  if (on) next.add(id)
  else next.delete(id)
  sel.value = next
}
function toggleAll(): void {
  sel.value = all.value ? new Set() : new Set(props.items.map((i) => i.id))
}
function onConfirm(): void {
  emit('confirm', [...sel.value])
}
</script>

<template>
  <MyDialog
    :model-value="true"
    title="导入预览"
    :width="640"
    hide-footer
    @cancel="emit('close')"
    @update:model-value="emit('close')"
  >
    <div class="ip">
      <span class="ip-sub">共解析到 {{ items.length }} 个供应商</span>

      <div class="ip-toolbar">
        <MyButton size="small" @click="toggleAll">{{ all ? '取消全选' : '全选' }}</MyButton>
        <span class="ip-sub">已选 {{ sel.size }} / {{ items.length }}</span>
      </div>

      <div class="ip-list">
        <label v-for="it in items" :key="it.id" class="ip-item" :class="{ checked: sel.has(it.id) }">
          <MyCheckbox
            :model-value="sel.has(it.id)"
            @update:model-value="toggle(it.id, $event === true)"
          />
          <div class="ip-copy">
            <div class="ip-badges">
              <span class="ip-name">{{ it.name }}</span>
              <span class="ip-tag dim">{{ it.kind }}</span>
              <span
                v-if="it.duplicateOf"
                class="ip-tag warn"
                :title="`baseURL + apiKey 与已有供应商一致，导入将覆盖更新「${it.duplicateOf}」`"
              >
                覆盖 {{ it.duplicateOf }}
              </span>
              <span v-else class="ip-tag new">新增</span>
              <span v-if="!it.hasApiKey" class="ip-tag dim" title="源配置中无 apiKey（如 Codex OAuth），仅导入 baseURL">
                无 apiKey
              </span>
            </div>
            <span class="ip-url">{{ it.baseUrl }}</span>
            <span class="ip-faint">{{ it.models.length > 0 ? `${it.models.length} 个模型` : '无模型' }}</span>
          </div>
        </label>
      </div>

      <div class="ip-footer">
        <MyButton size="small" :disabled="busy" @click="emit('close')">取消</MyButton>
        <MyButton variant="primary" size="small" :disabled="busy || sel.size === 0" :loading="busy" @click="onConfirm">
          {{ busy ? '导入中…' : `导入选中（${sel.size}）` }}
        </MyButton>
      </div>
    </div>
  </MyDialog>
</template>

<style scoped>
.ip {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.ip-sub {
  color: var(--text-secondary);
  font-size: 12px;
}
.ip-toolbar {
  display: flex;
  align-items: center;
  gap: 8px;
}
.ip-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
  max-height: 46vh;
  overflow-y: auto;
}
.ip-item {
  display: flex;
  gap: 8px;
  align-items: flex-start;
  padding: 6px 8px;
  border-radius: 6px;
  cursor: pointer;
  border: 1px solid var(--border-subtle);
}
.ip-item.checked {
  background: var(--accent-subtle);
}
.ip-copy {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
  flex: 1;
}
.ip-badges {
  display: flex;
  gap: 6px;
  flex-wrap: wrap;
  align-items: center;
}
.ip-name {
  font-weight: 500;
}
.ip-tag {
  font-size: 11px;
  padding: 0 8px;
  line-height: 18px;
  border-radius: 999px;
}
.ip-tag.new {
  background: var(--accent-subtle);
  color: var(--accent);
}
.ip-tag.warn {
  background: oklch(0.75 0.16 75 / 0.15);
  color: var(--warning);
}
.ip-tag.dim {
  background: var(--border-subtle);
  color: var(--text-tertiary);
}
.ip-url {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--text-tertiary);
  word-break: break-all;
}
.ip-faint {
  color: var(--text-tertiary);
  font-size: 11px;
}
.ip-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>
