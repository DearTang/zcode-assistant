<script setup lang="ts">
import { computed } from 'vue'
import MarkdownIt from 'markdown-it'
import { MyButton, MyDialog, MyIcon } from 'myui'
import { updater } from '@/api'
import type { UpdateInfo } from '@/types'
import { useUpdateDownload } from '@/composables/useUpdateDownload'
// `?raw` 在构建期把文件内容作为字符串打包（见 vite-env.d.ts），离线展示更新日志
import changelog from '../../CHANGELOG.md?raw'

/**
 * 「关于」弹窗：点击侧边栏底部版本号打开。
 * 顶部品牌与版本；中部检查更新区（发现新版本时变为下载/安装横幅）；
 * 下方滚动展示内置 CHANGELOG.md。外链点击经事件委托交给系统浏览器。
 */

const props = defineProps<{
  version: string
  updateInfo: UpdateInfo | null
  checking: boolean
}>()
const emit = defineEmits<{ close: []; 'check-updates': [] }>()
defineOptions({ name: 'AboutDialog' })

const hasUpdate = computed(() => !!props.updateInfo?.hasUpdate)
const downloadUrl = computed(() => props.updateInfo?.downloadUrl || props.updateInfo?.releaseUrl || '')
// latestVersion 来自 release tag，已带 "v"，原样展示避免双 v
const latestDisplay = computed(() => {
  const v = props.updateInfo?.latestVersion
  if (!v) return ''
  return v.startsWith('v') ? v : `v${v}`
})
const isBrowserMode = computed(() => props.updateInfo?.updateStrategy === 'browser')

const { phase, error, start, install, pct } = useUpdateDownload()

// 剥离维护者注释并压缩空行
const cleanChangelog = changelog
  .replace(/<!--[\s\S]*?-->/g, '')
  .replace(/\n{3,}/g, '\n\n')
  .trim()

const md = new MarkdownIt({ linkify: false, breaks: false })
const changelogHtml = md.render(cleanChangelog)

/** Tauri webview 内不直接跳转外链，事件委托交给系统浏览器 */
function onChangelogClick(e: MouseEvent): void {
  const a = (e.target as HTMLElement).closest('a')
  if (a) {
    e.preventDefault()
    const href = a.getAttribute('href')
    if (href) void updater.openReleasePage(href)
  }
}

function openDownloadPage(): void {
  if (downloadUrl.value) void updater.openReleasePage(downloadUrl.value)
}
</script>

<template>
  <MyDialog
    :model-value="true"
    title="关于 zcode-assistant"
    :width="560"
    dismissable
    hide-footer
    class="about-dialog"
    @cancel="emit('close')"
    @update:model-value="emit('close')"
  >
    <div class="ab">
      <!-- 头部：品牌 + 版本 -->
      <div class="ab-head">
        <div class="ab-mark">
          <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"
            stroke-linecap="round" stroke-linejoin="round">
            <path d="M7 7 L17 7 L7 17 L17 17" />
          </svg>
        </div>
        <div class="ab-copy">
          <strong>zcode-assistant</strong>
          <small>{{ version ? `版本 v${version}` : '版本获取中…' }}</small>
        </div>
        <MyButton variant="ghost" size="small" @click="emit('close')">关闭</MyButton>
      </div>

      <!-- 检查更新区 -->
      <div class="ab-update">
        <div v-if="hasUpdate" class="ab-banner">
          <div class="ab-banner-head">
            <span class="ab-banner-icon"><MyIcon name="Sparkle" :size="18" /></span>
            <div class="ab-banner-copy">
              <strong>发现新版本 {{ latestDisplay }}</strong>
              <small>
                {{ phase === 'idle' && (isBrowserMode ? '当前系统暂不支持应用内自动更新，请前往下载页手动下载安装' : '可自动下载安装，也可前往网页下载') }}
                {{ phase === 'downloading' && (pct() !== null ? `正在下载… ${pct()}%` : '正在下载…') }}
                {{ phase === 'ready' && '下载完成，可安装' }}
                {{ phase === 'failed' && (error || '下载失败') }}
              </small>
            </div>
          </div>
          <div v-if="phase === 'downloading'" class="ab-progress">
            <div class="ab-progress-fill" :style="{ width: `${pct() ?? 0}%` }" />
          </div>
          <div class="ab-banner-actions">
            <template v-if="phase === 'idle'">
              <MyButton v-if="isBrowserMode" variant="primary" size="small" @click="openDownloadPage">
                <MyIcon name="External" :size="15" /> 打开下载页
              </MyButton>
              <template v-else>
                <MyButton variant="primary" size="small" @click="start(downloadUrl)">
                  <MyIcon name="Download" :size="15" /> 更新
                </MyButton>
                <MyButton variant="secondary" size="small" @click="openDownloadPage">网页下载</MyButton>
              </template>
            </template>
            <span v-if="phase === 'downloading'" class="ab-faint">请稍候，下载完成后将自动提示…</span>
            <MyButton v-if="phase === 'ready'" variant="primary" size="small" class="ab-grow" @click="install">
              安装并重启
            </MyButton>
            <template v-if="phase === 'failed'">
              <MyButton variant="secondary" size="small" @click="openDownloadPage">浏览器下载</MyButton>
              <MyButton variant="primary" size="small" @click="start(downloadUrl)">重试</MyButton>
            </template>
          </div>
        </div>
        <div v-else class="ab-check">
          <MyButton variant="secondary" size="small" :loading="checking" @click="emit('check-updates')">
            <MyIcon name="Refresh" :size="15" />
            {{ checking ? '检查中…' : '检查更新' }}
          </MyButton>
          <span class="ab-faint">
            {{ updateInfo?.error ? '上次检查失败，可重试' : updateInfo && !hasUpdate ? '当前已是最新版本' : '' }}
          </span>
        </div>
      </div>

      <!-- 更新日志（滚动区） -->
      <div class="ab-changelog" @click="onChangelogClick" v-html="changelogHtml" />
    </div>
  </MyDialog>
</template>

<style scoped>
.ab {
  display: flex;
  flex-direction: column;
  max-height: 74vh;
  overflow: hidden;
}
.ab-head {
  display: flex;
  align-items: center;
  gap: 12px;
  padding-bottom: 12px;
}
.ab-mark {
  width: 40px;
  height: 40px;
  display: grid;
  place-items: center;
  border: 1px solid var(--accent-subtle);
  border-radius: 11px;
  background: var(--accent-subtle);
  color: var(--accent);
  flex: none;
}
.ab-copy {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.ab-copy strong {
  font-size: 15px;
  letter-spacing: -0.02em;
}
.ab-copy small {
  color: var(--text-tertiary);
  font-size: 11px;
}

.ab-update {
  padding-bottom: 12px;
}
.ab-check {
  display: flex;
  align-items: center;
  gap: 10px;
}
.ab-banner {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 12px 14px;
  border: 1px solid var(--accent-subtle);
  border-radius: 12px;
  background: var(--accent-subtle);
}
.ab-banner-head {
  display: flex;
  align-items: center;
  gap: 10px;
}
.ab-banner-icon {
  color: var(--accent);
  display: inline-flex;
}
.ab-banner-copy {
  flex: 1;
  min-width: 0;
}
.ab-banner-copy strong {
  font-size: 14px;
}
.ab-banner-copy small {
  display: block;
  margin-top: 2px;
  color: var(--text-tertiary);
  font-size: 11px;
}
.ab-progress {
  height: 6px;
  border-radius: 999px;
  background: var(--border-subtle);
  overflow: hidden;
}
.ab-progress-fill {
  height: 100%;
  border-radius: 999px;
  background: var(--accent);
  transition: width 0.3s ease;
}
.ab-banner-actions {
  display: flex;
  justify-content: flex-end;
  align-items: center;
  gap: 8px;
}
.ab-grow {
  flex: 1;
}
.ab-faint {
  color: var(--text-tertiary);
  font-size: 11px;
}

.ab-changelog {
  flex: 1;
  min-height: 200px;
  overflow-y: auto;
  padding: 12px 4px 4px;
  border-top: 1px solid var(--border-subtle);
  font-size: 13px;
  color: var(--text-secondary);
  line-height: 1.6;
  scrollbar-width: thin;
}
.ab-changelog :deep(h1) {
  margin: 14px 0 8px;
  font-size: 16px;
  color: var(--text-primary);
}
.ab-changelog :deep(h2) {
  margin: 14px 0 8px;
  font-size: 15px;
  color: var(--text-primary);
}
.ab-changelog :deep(h3) {
  margin: 10px 0 6px;
  font-size: 13px;
  color: var(--text-primary);
}
.ab-changelog :deep(p) {
  margin: 6px 0;
}
.ab-changelog :deep(ul),
.ab-changelog :deep(ol) {
  margin: 6px 0;
  padding-left: 20px;
}
.ab-changelog :deep(li) {
  margin: 3px 0;
}
.ab-changelog :deep(a) {
  color: var(--accent);
  text-decoration: none;
}
.ab-changelog :deep(hr) {
  border: none;
  border-top: 1px solid var(--border-subtle);
  margin: 14px 0;
}
.ab-changelog :deep(code) {
  font-family: var(--font-mono);
  font-size: 12px;
  background: var(--bg-elevated);
  padding: 1px 5px;
  border-radius: 4px;
}
.ab-changelog :deep(blockquote) {
  margin: 8px 0;
  padding: 8px 12px;
  border-left: 3px solid var(--border-strong);
  background: var(--bg-elevated);
  border-radius: 6px;
  color: var(--text-tertiary);
  font-size: 12px;
}
</style>
