<script setup lang="ts">
import { computed, ref } from 'vue'
import { MyButton, MyCheckbox, MyDialog } from 'myui'
import type { ExportPreview } from '@/api'

/** 同步预览弹窗（cc-switch / Oh My Pi 共用）：默认勾选 zcode 中启用的供应商 */
const props = defineProps<{
  title?: string
  items: ExportPreview[]
  busy: boolean
}>()
const emit = defineEmits<{ close: []; confirm: [ids: string[]] }>()
defineOptions({ name: 'ExportPreviewModal' })

const sel = ref(new Set(props.items.filter((i) => i.enabled).map((i) => i.id)))
const all = computed(() => sel.value.size === props.items.length)
const title = computed(() => props.title ?? '同步到 cc-switch')

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
    :title="title"
    :width="640"
    hide-footer
    @cancel="emit('close')"
    @update:model-value="emit('close')"
  >
    <div class="ep">
      <span class="ep-sub">共 {{ items.length }} 个可导出供应商</span>

      <div class="ep-toolbar">
        <MyButton size="small" @click="toggleAll">{{ all ? '取消全选' : '全选' }}</MyButton>
        <span class="ep-sub">已选 {{ sel.size }} / {{ items.length }}（默认仅勾选 zcode 中启用的）</span>
      </div>

      <div class="ep-list">
        <label v-for="it in items" :key="it.id" class="ep-item" :class="{ checked: sel.has(it.id) }">
          <MyCheckbox
            :model-value="sel.has(it.id)"
            @update:model-value="toggle(it.id, $event === true)"
          />
          <div class="ep-copy">
            <div class="ep-badges">
              <span class="ep-name">{{ it.name }}</span>
              <span
                v-if="it.duplicateOf"
                class="ep-tag warn"
                :title="`baseURL + apiKey 与目标供应商一致，同步将覆盖更新「${it.duplicateOf}」`"
              >
                覆盖 {{ it.duplicateOf }}
              </span>
              <span v-else class="ep-tag new">新增</span>
              <span v-if="!it.enabled" class="ep-tag dim" title="该供应商在 zcode 中处于禁用状态">已禁用</span>
              <span v-if="!it.hasApiKey" class="ep-tag dim" title="zcode 配置中无 apiKey，目标侧同样不含 key">无 apiKey</span>
            </div>
            <span class="ep-url">{{ it.baseUrl }}</span>
            <span class="ep-faint">{{ it.modelCount > 0 ? `${it.modelCount} 个模型` : '无模型' }}</span>
          </div>
        </label>
      </div>

      <div class="ep-footer">
        <MyButton size="small" :disabled="busy" @click="emit('close')">取消</MyButton>
        <MyButton variant="primary" size="small" :disabled="busy || sel.size === 0" :loading="busy" @click="onConfirm">
          {{ busy ? '同步中…' : `同步 ${sel.size} 项` }}
        </MyButton>
      </div>
    </div>
  </MyDialog>
</template>

<style scoped>
.ep {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.ep-sub {
  color: var(--text-secondary);
  font-size: 12px;
}
.ep-toolbar {
  display: flex;
  align-items: center;
  gap: 8px;
}
.ep-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
  max-height: 46vh;
  overflow-y: auto;
}
.ep-item {
  display: flex;
  gap: 8px;
  align-items: flex-start;
  padding: 6px 8px;
  border-radius: 6px;
  cursor: pointer;
  border: 1px solid var(--border-subtle);
}
.ep-item.checked {
  background: var(--accent-subtle);
}
.ep-copy {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
  flex: 1;
}
.ep-badges {
  display: flex;
  gap: 6px;
  flex-wrap: wrap;
  align-items: center;
}
.ep-name {
  font-weight: 500;
}
.ep-tag {
  font-size: 11px;
  padding: 0 8px;
  line-height: 18px;
  border-radius: 999px;
}
.ep-tag.new {
  background: var(--accent-subtle);
  color: var(--accent);
}
.ep-tag.warn {
  background: oklch(0.75 0.16 75 / 0.15);
  color: var(--warning);
}
.ep-tag.dim {
  background: var(--border-subtle);
  color: var(--text-tertiary);
}
.ep-url {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--text-tertiary);
  word-break: break-all;
}
.ep-faint {
  color: var(--text-tertiary);
  font-size: 11px;
}
.ep-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>
