<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { MyButton, MyIcon, MyPanel, MySegmented, MySlider, MyToggle } from 'myui'
import UpdateNotification from '@/components/UpdateNotification.vue'
import { zcode, app, updater, prefs as prefsApi, events, beautify } from '@/api'
import { toast } from '@/composables/toast'
import { setThemeChoice, ui, type ThemeChoice } from '@/store/ui'
import { appearance, setAppearance, resetAppearance, bgState } from '@/composables/useAppearance'
import type { AppPrefs, UpdateInfo, UsageDisplayMode } from '@/types'

defineOptions({ name: 'SettingsView' })

const probe = ref<{ exePath: string | null; running: boolean; configDir: string } | null>(null)
const version = ref('')
const updateInfo = ref<UpdateInfo | null>(null)
const checking = ref(false)
const updateDismissed = ref(false)
const prefs = ref<AppPrefs | null>(null)
let un: (() => void) | undefined

const themeOptions = [
  { label: '深色', value: 'dark' },
  { label: '浅色', value: 'light' },
  { label: '跟随系统', value: 'system' },
]

const usageOptions = [
  { label: '展示已用量', value: 'used' },
  { label: '展示剩余用量', value: 'remaining' },
]

onMounted(() => {
  zcode.probe().then((v) => (probe.value = v)).catch(() => (probe.value = null))
  app.getVersion().then((v) => (version.value = v)).catch(() => {})
  // 偏好：初始读取 + 监听变更（托盘菜单切悬浮球时这里同步刷新）
  prefsApi.get().then((v) => (prefs.value = v)).catch(() => {})
  events.onPrefsUpdated((v) => (prefs.value = v)).then((fn) => (un = fn))
})
onBeforeUnmount(() => un?.())

async function patchPrefs(patch: Partial<AppPrefs>, apply: (p: Partial<AppPrefs>) => Promise<unknown>): Promise<void> {
  if (prefs.value) prefs.value = { ...prefs.value, ...patch }
  try {
    await apply(patch)
  } catch (e: unknown) {
    toast.error(String(e))
    prefsApi.get().then((v) => (prefs.value = v)).catch(() => {})
  }
}

const setFloatBallVisible = (visible: boolean) =>
  patchPrefs({ floatBallVisible: visible }, () => prefsApi.setFloatBallVisible(visible))
const setUsageDisplay = (mode: UsageDisplayMode) =>
  patchPrefs({ usageDisplay: mode }, () => prefsApi.setUsageDisplay(mode))
const setSwitchRestart = (enabled: boolean) =>
  patchPrefs({ switchRestartZcode: enabled }, () => prefsApi.setSwitchRestart(enabled))
const setAutostart = (enabled: boolean) =>
  patchPrefs({ autostart: enabled }, () => prefsApi.setAutostart(enabled))

// 外观定制：选背景图（复用美化页的文件选择器）；还原默认
async function pickBgImage(): Promise<void> {
  try {
    const p = await beautify.pickImage()
    if (p) setAppearance({ bgImage: p })
  } catch (e: unknown) {
    toast.error(String(e))
  }
}
function resetAppearanceAll(): void {
  resetAppearance()
  toast.success('已还原默认主题')
}

async function checkUpdates(): Promise<void> {
  checking.value = true
  try {
    const info = await updater.checkForUpdates()
    updateInfo.value = info
    if (info.hasUpdate) updateDismissed.value = false
  } catch {
    /* 保留原 info */
  } finally {
    checking.value = false
  }
}

const updateStatus = computed(() => {
  if (checking.value) return '检查中…'
  if (!updateInfo.value) return null
  if (updateInfo.value.error) return `检查失败：${updateInfo.value.error}`
  if (updateInfo.value.hasUpdate) return `发现新版本 v${updateInfo.value.latestVersion.replace(/^v/, '')}`
  return '已是最新版本'
})

// 供 UpdateNotification 使用的非空视图
const updateBanner = computed(() => (updateInfo.value?.hasUpdate ? updateInfo.value : null))

/** 主题色预设（OKLCH hue；null = 默认青绿，不覆盖任何 token） */
const ACCENT_PRESETS: { label: string; hue: number | null }[] = [
  { label: '青绿（默认）', hue: null },
  { label: '蔚蓝', hue: 250 },
  { label: '紫罗兰', hue: 300 },
  { label: '品红', hue: 350 },
  { label: '珊瑚', hue: 25 },
  { label: '琥珀', hue: 75 },
]
</script>

<template>
  <div class="st">
    <MyPanel title="通用">
      <div class="st-row">
        <div class="st-copy">
          <strong>开机自启动</strong>
          <small>登录系统后自动启动并驻留托盘</small>
        </div>
        <MyToggle :model-value="prefs?.autostart ?? false" title="开机自启动" @update:model-value="setAutostart($event === true)" />
      </div>
    </MyPanel>

    <MyPanel title="外观">
      <MySegmented
        :model-value="ui.themeChoice"
        :options="themeOptions"
        @update:model-value="setThemeChoice($event as ThemeChoice)"
      />
    </MyPanel>

    <!-- 外观定制：主题色 / 透明度 / 背景图，即时生效 -->
    <MyPanel title="外观定制">
      <template #actions>
        <MyButton size="small" @click="resetAppearanceAll">还原默认主题</MyButton>
      </template>
      <p class="st-desc">
        参考「ZCode 美化」的思路给本应用换肤：主题色更换强调色系，透明度让光晕 /
        背景图更透出，背景图铺满窗口底层。全部即时生效，仅主窗口（悬浮球 / 托盘面板不套用）。
      </p>

      <div class="st-sub">主题色</div>
      <div class="st-chips">
        <button
          v-for="p in ACCENT_PRESETS"
          :key="p.label"
          type="button"
          class="st-chip"
          :class="{ active: appearance.accentHue === p.hue }"
          :title="p.label"
          @click="setAppearance({ accentHue: p.hue })"
        >
          <span class="st-chip-dot" :style="{ background: `oklch(0.68 0.15 ${p.hue ?? 180})` }" />
          <span class="st-chip-label">{{ p.label }}</span>
        </button>
      </div>
      <div class="st-hue">
        <MySlider
          :model-value="appearance.accentHue ?? 180"
          :min="0"
          :max="360"
          unit="°"
          title="自定义色相（OKLCH hue）"
          @update:model-value="setAppearance({ accentHue: $event as number })"
        />
      </div>

      <div class="st-sub st-mt">透明度</div>
      <MySlider
        :model-value="Math.round(appearance.surfaceOpacity * 100)"
        :min="40"
        :max="100"
        unit="%"
        @update:model-value="setAppearance({ surfaceOpacity: ($event as number) / 100 })"
      />
      <div class="st-hint">面板与表面不透明度；调低让光晕 / 背景图更透出（100% = 默认实底）</div>

      <div class="st-sub st-mt">背景图</div>
      <div class="st-bg-actions">
        <MyButton size="small" @click="pickBgImage">选择图片…</MyButton>
        <MyButton v-if="appearance.bgImage" size="small" @click="setAppearance({ bgImage: null })">移除</MyButton>
        <span v-if="appearance.bgImage && !bgState.dataUrl" class="st-hint">
          图片不可用（超过 8MB 或格式不支持）
        </span>
      </div>
      <div v-if="bgState.dataUrl" class="st-bg-preview">
        <div
          class="st-bg-img"
          :style="{ backgroundImage: `url(${bgState.dataUrl})`, opacity: appearance.bgOpacity }"
        />
        <MySlider
          :model-value="Math.round(appearance.bgOpacity * 100)"
          :min="10"
          :max="100"
          unit="%"
          @update:model-value="setAppearance({ bgOpacity: ($event as number) / 100 })"
        />
      </div>
    </MyPanel>

    <MyPanel title="悬浮球与托盘">
      <div class="st-row">
        <div class="st-copy">
          <strong>显示悬浮球</strong>
          <small>常驻桌面的配额监控小球，托盘菜单里也可以切换</small>
        </div>
        <MyToggle
          :model-value="prefs?.floatBallVisible ?? true"
          title="显示/隐藏悬浮球"
          @update:model-value="setFloatBallVisible($event === true)"
        />
      </div>
    </MyPanel>

    <MyPanel title="模型用量展示方案">
      <MySegmented
        :model-value="prefs?.usageDisplay ?? 'used'"
        :options="usageOptions"
        @update:model-value="setUsageDisplay($event as UsageDisplayMode)"
      />
      <p class="st-hint st-mt-s">
        统一控制总览、供应商管理额度行、悬浮球、悬浮面板与托盘菜单 / tooltip
        的百分比口径；颜色始终按已用度分级。
      </p>
    </MyPanel>

    <MyPanel title="切换行为">
      <div class="st-row">
        <div class="st-copy">
          <strong>切换后重启 ZCode</strong>
          <small>自动切换写入配置与各会话模型选择并自动重启；账号切换完成后弹窗确认</small>
        </div>
        <MyToggle
          :model-value="prefs?.switchRestartZcode ?? true"
          title="切换后重启 ZCode"
          @update:model-value="setSwitchRestart($event === true)"
        />
      </div>
      <p class="st-hint st-mt-s">
        开启时自动切换写入配置与全部符合条件的会话（模型选择 + 供应商配置）并自动重启
        ZCode，全部对话统一生效；关闭时不重启，各对话在恢复 / 新开时使用新模型，
        账号切换会直接重启 ZCode。
      </p>
    </MyPanel>

    <MyPanel title="zcode 连接">
      <div v-if="probe" class="st-probe">
        <div class="st-probe-ok">
          <MyIcon name="Check" :size="16" style="color: var(--success)" />
          <span>已检测到 zcode</span>
          <span v-if="probe.running" class="st-badge">运行中</span>
        </div>
        <div class="st-mono">{{ probe.exePath ?? '未找到 ZCode.exe' }}</div>
        <div class="st-mono st-faint">配置目录：{{ probe.configDir }}</div>
      </div>
      <p v-else class="st-desc">未检测到 zcode。</p>
    </MyPanel>

    <MyPanel title="关于">
      <div class="st-rows">
        <div class="st-kv"><span>应用</span><span class="st-mono">{{ version ? `zcode-assistant v${version}` : 'zcode-assistant' }}</span></div>
        <div class="st-kv"><span>技术栈</span><span class="st-mono">Tauri 2 · Vue 3 · myui · TypeScript</span></div>
        <div class="st-kv"><span>设计</span><span class="st-mono">统一界面（unified-ui / myui 令牌）</span></div>
        <div class="st-kv"><span>数据</span><span class="st-mono">本地读写 ~/.zcode/v2，不外传</span></div>
      </div>
      <div class="st-about-actions">
        <MyButton size="small" :loading="checking" @click="checkUpdates">
          <MyIcon name="Refresh" :size="15" />
          {{ checking ? '检查中…' : '检查更新' }}
        </MyButton>
        <span v-if="updateStatus" class="st-update-status" :class="{ hot: updateInfo?.hasUpdate }">
          {{ updateStatus }}
        </span>
      </div>
    </MyPanel>

    <UpdateNotification
      v-if="updateBanner && !updateDismissed"
      :update-info="updateBanner"
      @ignored="updateDismissed = true"
    />
  </div>
</template>

<style scoped>
.st {
  display: grid;
  gap: var(--ui-gap);
}
.st-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
}
.st-copy {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.st-copy strong {
  font-size: 13px;
  font-weight: 500;
}
.st-copy small {
  color: var(--text-tertiary);
  font-size: 11px;
}
.st-desc {
  margin: 0 0 12px;
  color: var(--text-secondary);
  font-size: 12px;
  line-height: 1.7;
}
.st-sub {
  font-size: 13px;
  color: var(--text-secondary);
  font-weight: 500;
  margin-bottom: 8px;
}
.st-mt {
  margin-top: 14px;
}
.st-mt-s {
  margin: 8px 0 0;
}
.st-hint {
  color: var(--text-tertiary);
  font-size: 11px;
  margin-top: 4px;
}
.st-chips {
  display: flex;
  gap: 14px;
  flex-wrap: wrap;
  margin-bottom: 8px;
}
.st-chip {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 4px;
  background: transparent;
  border: none;
  padding: 0;
  cursor: pointer;
}
.st-chip-dot {
  width: 24px;
  height: 24px;
  border-radius: 50%;
  display: block;
}
.st-chip.active .st-chip-dot {
  box-shadow: 0 0 0 2px var(--bg-base), 0 0 0 4px currentColor;
}
.st-chip-label {
  font-size: 11px;
  color: var(--text-tertiary);
  white-space: nowrap;
}
.st-chip.active .st-chip-label {
  color: var(--text-primary);
}
.st-bg-actions {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}
.st-bg-img {
  height: 84px;
  border-radius: 10px;
  border: 1px solid var(--border-subtle);
  background-position: center;
  background-size: cover;
  background-repeat: no-repeat;
  margin-bottom: 8px;
}
.st-probe {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.st-probe-ok {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
}
.st-badge {
  font-size: 11px;
  color: var(--accent);
  border: 1px solid var(--accent);
  border-radius: 999px;
  padding: 0 8px;
  line-height: 18px;
}
.st-mono {
  font-family: var(--font-mono);
  font-size: 12px;
  color: var(--text-secondary);
  word-break: break-all;
}
.st-faint {
  color: var(--text-tertiary);
  font-size: 11px;
}
.st-rows {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.st-kv {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  font-size: 13px;
}
.st-kv > span:first-child {
  color: var(--text-secondary);
}
.st-kv .st-mono {
  font-size: 11px;
}
.st-about-actions {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  flex-wrap: wrap;
  margin-top: 12px;
}
.st-update-status {
  font-size: 11px;
  color: var(--text-tertiary);
}
.st-update-status.hot {
  color: var(--accent);
}
</style>
