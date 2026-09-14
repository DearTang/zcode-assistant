import { onBeforeUnmount, ref, watch } from 'vue'
import { onUpdateDownloadProgress, updater } from '@/api'

export type DownloadPhase = 'idle' | 'downloading' | 'ready' | 'failed'

/**
 * 更新包下载 + 安装状态机（「关于」弹窗与启动更新通知共用）：
 * idle → downloading → ready（可安装） / failed（可重试或浏览器下载）。
 */
export function useUpdateDownload() {
  const phase = ref<DownloadPhase>('idle')
  const progress = ref<{ downloaded: number; total: number }>({ downloaded: 0, total: 0 })
  const error = ref('')
  const downloadedPath = ref('')

  let unlisten: (() => void) | null = null
  let cancelled = false

  watch(phase, (p) => {
    if (p === 'downloading' && !unlisten) {
      cancelled = false
      onUpdateDownloadProgress((d) => {
        progress.value = { downloaded: d.downloaded, total: d.total }
      }).then((fn) => {
        if (cancelled) fn()
        else unlisten = fn
      })
    }
  })

  onBeforeUnmount(() => {
    cancelled = true
    unlisten?.()
  })

  async function start(url: string): Promise<void> {
    phase.value = 'downloading'
    progress.value = { downloaded: 0, total: 0 }
    error.value = ''
    try {
      downloadedPath.value = await updater.downloadUpdate(url)
      phase.value = 'ready'
    } catch (e) {
      error.value = String(e)
      phase.value = 'failed'
    }
  }

  async function install(): Promise<void> {
    if (!downloadedPath.value) {
      error.value = '安装包路径丢失'
      phase.value = 'failed'
      return
    }
    try {
      await updater.installUpdate(downloadedPath.value)
    } catch (e) {
      error.value = String(e)
      phase.value = 'failed'
    }
  }

  const pct = () => {
    const { downloaded, total } = progress.value
    return total > 0 ? Math.min(100, Math.round((downloaded / total) * 100)) : null
  }

  return { phase, error, start, install, pct }
}
