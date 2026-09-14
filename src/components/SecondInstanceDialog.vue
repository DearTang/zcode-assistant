<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from 'vue'
import { MyDialog } from 'myui'
import { events, win } from '@/api'

/**
 * 单实例弹窗：第二个应用实例尝试启动时，已有实例收到 app://second-instance
 * 广播（后端已唤出主窗口、新实例已自动退出），由用户选择：
 * - 覆盖启动：重启当前进程（restart_app），等效用新实例替换旧实例
 * - 退出：保持现有实例，仅关闭弹窗
 */
defineOptions({ name: 'SecondInstanceDialog' })

const open = ref(false)
let un: (() => void) | undefined

onMounted(() => {
  events.onSecondInstance(() => (open.value = true)).then((fn) => (un = fn))
})
onBeforeUnmount(() => un?.())
</script>

<template>
  <MyDialog
    v-model="open"
    title="应用已在运行"
    :width="420"
    cancel-text="退出"
    confirm-text="覆盖启动"
    @confirm="win.restartApp().catch(() => {})"
  >
    <p class="si-desc">
      检测到另一次启动。覆盖启动会结束当前实例并重新启动应用；
      退出则保持当前实例继续运行（本次启动已自动结束）。
    </p>
  </MyDialog>
</template>

<style scoped>
.si-desc {
  margin: 0;
  color: var(--text-secondary);
  font-size: 12px;
  line-height: 1.7;
}
</style>
