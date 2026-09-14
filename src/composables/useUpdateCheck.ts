import { ref, type Ref } from 'vue'
import { updater } from '@/api'
import type { UpdateInfo } from '@/types'

/**
 * 后台更新检查 composable。
 * - enabled 翻 true 后每会话只跑一次自动检查；checkNow 供「关于」页手动触发。
 * - 永不抛错：失败时 info 保留原值（或 null），UI 把 null 视为「无更新，保持沉默」。
 */
export function useUpdateCheck(enabled: boolean): {
  info: Ref<UpdateInfo | null>
  loading: Ref<boolean>
  checkNow: () => void
} {
  const info = ref<UpdateInfo | null>(null)
  const loading = ref(false)
  let inFlight = false
  let autoRan = false

  async function runCheck(): Promise<void> {
    if (inFlight) return
    inFlight = true
    loading.value = true
    try {
      info.value = await updater.checkForUpdates()
    } catch {
      // 防御性：保留原 info，不向用户暴露错误
    } finally {
      inFlight = false
      loading.value = false
    }
  }

  if (enabled && !autoRan) {
    autoRan = true
    void runCheck()
  }

  return { info, loading, checkNow: () => void runCheck() }
}
