<script setup lang="ts">
import { computed, ref } from 'vue'
import { MyCheckbox, MyPopover } from 'myui'

/**
 * 星期多选下拉：按钮 + 弹出 checkbox 面板（myui v0.9.0 MyPopover 承载浮层，
 * 外点收起 / Esc 交给 EP；此前 backdrop-filter 下 fixed 遮罩失效的坑一并消除）。
 */
const props = defineProps<{ modelValue: string }>()
const emit = defineEmits<{ 'update:modelValue': [value: string] }>()
defineOptions({ name: 'WeekdayPicker' })

const WEEKDAYS = [
  { v: 1, label: '周一' },
  { v: 2, label: '周二' },
  { v: 3, label: '周三' },
  { v: 4, label: '周四' },
  { v: 5, label: '周五' },
  { v: 6, label: '周六' },
  { v: 7, label: '周日' },
]

const open = ref(false)

const selected = computed(
  () =>
    new Set(
      props.modelValue
        .split(',')
        .map((s) => s.trim())
        .filter(Boolean)
        .map(Number),
    ),
)

function toggle(v: number): void {
  const next = new Set(selected.value)
  if (next.has(v)) next.delete(v)
  else next.add(v)
  emit(
    'update:modelValue',
    WEEKDAYS.filter((w) => next.has(w.v))
      .map((w) => w.v)
      .sort((a, b) => a - b)
      .join(','),
  )
}

const summary = computed(() => {
  const labels = WEEKDAYS.filter((w) => selected.value.has(w.v)).map((w) => w.label)
  if (labels.length === 0) return '未选择'
  if (labels.length === 7) return '每天'
  return labels.join('、')
})
</script>

<template>
  <MyPopover v-model="open" trigger="click" placement="bottom-start" :width="160">
    <button type="button" class="wp-trigger">
      <span>{{ summary }}</span>
      <span class="wp-caret">▾</span>
    </button>
    <template #content>
      <label v-for="w in WEEKDAYS" :key="w.v" class="wp-item">
        <MyCheckbox :model-value="selected.has(w.v)" @update:model-value="toggle(w.v)" />
        {{ w.label }}
      </label>
    </template>
  </MyPopover>
</template>

<style scoped>
.wp-trigger {
  width: 100%;
  height: 32px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 6px;
  padding: 0 10px;
  border: 1px solid var(--border-base);
  border-radius: 8px;
  background: var(--bg-base);
  color: var(--text-primary);
  font-size: 13px;
  cursor: pointer;
}
.wp-trigger:hover {
  border-color: var(--border-strong);
}
.wp-caret {
  font-size: 10px;
  opacity: 0.6;
}
.wp-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 5px 8px;
  cursor: pointer;
  font-size: 13px;
  border-radius: 6px;
}
.wp-item:hover {
  background: var(--surface-translucent-hover);
}
</style>
