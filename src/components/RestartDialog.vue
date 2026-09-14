<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from 'vue'
import { MyDialog } from 'myui'
import { events, zcode } from '@/api'
import { toast } from '@/composables/toast'

/** 全局重启确认弹窗：监听后端 zcode://restart-requested，确认后重启 zcode */
defineOptions({ name: 'RestartDialog' })

const reason = ref<string | null>(null)
const busy = ref(false)
let un: (() => void) | undefined

onMounted(() => {
  events.onRestartRequested((p) => (reason.value = p.reason)).then((fn) => (un = fn))
})
onBeforeUnmount(() => un?.())

async function confirm(): Promise<void> {
  busy.value = true
  try {
    await zcode.restartZcode()
    reason.value = null
  } catch (e: unknown) {
    toast.error(typeof e === 'string' ? e : '重启失败')
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <MyDialog
    :model-value="reason != null"
    title="需要重启 zcode 生效"
    :width="380"
    cancel-text="稍后"
    :confirm-text="busy ? '重启中…' : '立即重启'"
    :confirm-loading="busy"
    manual-close
    @cancel="reason = null"
    @confirm="confirm"
  >
    <p class="rd-reason">{{ reason }}</p>
  </MyDialog>
</template>

<style scoped>
.rd-reason {
  margin: 0;
  color: var(--text-secondary);
  font-size: 12px;
  line-height: 1.7;
}
</style>
