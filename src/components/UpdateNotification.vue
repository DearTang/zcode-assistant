<script setup lang="ts">
import { computed } from 'vue'
import { MyButton, MyDialog, MyIcon, MyTag } from 'myui'
import { updater } from '@/api'
import type { UpdateInfo } from '@/types'
import { useUpdateDownload } from '@/composables/useUpdateDownload'

/**
 * 居中模态更新弹窗（4 阶段：prompt/downloading/ready/failed）。
 * 启动后后台检查发现新版本时显示一次：
 *   - 「更新」→ 自动下载安装器 → 进度条 → 「安装并重启」
 *   - 「忽略」→ 把该版本记到 localStorage，同版本不再提示
 * 非 Windows 跳过下载/安装阶段，只给「打开下载页」。
 */
const props = defineProps<{ updateInfo: UpdateInfo }>()
const emit = defineEmits<{ ignored: [] }>()
defineOptions({ name: 'UpdateNotification' })

const IGNORE_KEY = 'za.ignoredUpdateVersion'

const downloadUrl = computed(() => props.updateInfo.downloadUrl || props.updateInfo.releaseUrl || '')
const latestDisplay = computed(() => {
  const v = props.updateInfo.latestVersion
  return v.startsWith('v') ? v : `v${v}`
})
const isBrowserMode = computed(() => props.updateInfo.updateStrategy === 'browser')

const { phase, error, start, install, pct } = useUpdateDownload()
// 弹窗自身用 prompt 语义（idle 即初始提示态）

function handleIgnore(): void {
  try {
    localStorage.setItem(IGNORE_KEY, props.updateInfo.latestVersion)
  } catch {
    /* best-effort */
  }
  emit('ignored')
}

function openDownloadPage(): void {
  if (downloadUrl.value) void updater.openReleasePage(downloadUrl.value)
}
</script>

<template>
  <MyDialog
    :model-value="true"
    title="zcode-assistant 有新版本可用"
    :width="400"
    hide-footer
    class="update-dialog"
  >
    <div class="un">
      <div class="un-head">
        <div class="un-icon"><MyIcon name="Sparkle" :size="22" /></div>
        <strong>zcode-assistant 有新版本可用</strong>
        <MyTag type="primary" size="small" round>{{ latestDisplay }}</MyTag>
      </div>

      <p class="un-sub">
        {{ phase === 'idle' && (isBrowserMode ? '检测到新版本。当前系统暂不支持应用内自动更新，请前往下载页手动下载安装。' : '新版本已就绪，是否立即更新？') }}
        {{ phase === 'downloading' && (pct() !== null ? `正在下载… ${pct()}%` : '正在下载…') }}
        {{ phase === 'ready' && '下载完成，点击安装并重启应用' }}
        {{ phase === 'failed' && (error || '下载出现问题') }}
      </p>

      <div v-if="phase === 'downloading'" class="un-progress">
        <div class="un-progress-fill" :style="{ width: `${pct() ?? 0}%` }" />
      </div>

      <div class="un-actions">
        <template v-if="phase === 'idle'">
          <MyButton variant="secondary" size="small" @click="handleIgnore">忽略</MyButton>
          <MyButton v-if="isBrowserMode" variant="primary" size="small" @click="openDownloadPage">
            <MyIcon name="External" :size="15" /> 打开下载页
          </MyButton>
          <MyButton v-else variant="primary" size="small" @click="start(downloadUrl)">
            <MyIcon name="Download" :size="15" /> 更新
          </MyButton>
        </template>
        <span v-if="phase === 'downloading'" class="un-faint">请稍候，下载完成后将自动提示…</span>
        <MyButton v-if="phase === 'ready'" variant="primary" size="small" class="un-grow" @click="install">
          安装并重启
        </MyButton>
        <template v-if="phase === 'failed'">
          <MyButton variant="secondary" size="small" @click="openDownloadPage">浏览器下载</MyButton>
          <MyButton variant="primary" size="small" @click="start(downloadUrl)">重试</MyButton>
        </template>
      </div>
    </div>
  </MyDialog>
</template>

<style scoped>
.un {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.un-head {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
}
.un-head strong {
  font-size: 14px;
}
.un-icon {
  width: 44px;
  height: 44px;
  border-radius: 999px;
  background: var(--accent-subtle);
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--accent);
}
.un-sub {
  margin: 0;
  text-align: center;
  color: var(--text-secondary);
  font-size: 12px;
  line-height: 1.6;
}
.un-progress {
  height: 6px;
  border-radius: 999px;
  background: var(--border-subtle);
  overflow: hidden;
}
.un-progress-fill {
  height: 100%;
  border-radius: 999px;
  background: var(--accent);
  transition: width 0.3s ease;
}
.un-actions {
  display: flex;
  justify-content: flex-end;
  align-items: center;
  gap: 8px;
}
.un-grow {
  flex: 1;
}
.un-faint {
  color: var(--text-tertiary);
  font-size: 11px;
}
</style>
