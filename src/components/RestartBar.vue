<script setup lang="ts">
import { ref } from 'vue'
import { MyButton, MyIcon, MyPanel } from 'myui'
import { zcode } from '@/api'
import { toast } from '@/composables/toast'

/**
 * 重启 zcode 提示条：说明文字 + 手动重启按钮。
 * 用于模型管理 / 账号切换等需要配置生效的页面顶部。
 */
defineProps<{ hint: string }>()
defineOptions({ name: 'RestartBar' })

const busy = ref(false)

async function handleRestart(): Promise<void> {
  busy.value = true
  try {
    await zcode.restartZcode()
    toast.success('zcode 已重启')
  } catch (e: unknown) {
    toast.error(typeof e === 'string' ? e : '重启失败')
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <MyPanel>
    <div class="rb">
      <span class="hint">{{ hint }}</span>
      <MyButton size="small" :loading="busy" @click="handleRestart">
        <MyIcon name="Power" :size="13" />
        {{ busy ? '重启中…' : '重启 zcode' }}
      </MyButton>
    </div>
  </MyPanel>
</template>

<style scoped>
.rb {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}
.hint {
  color: var(--text-secondary);
  font-size: 12px;
}
</style>
