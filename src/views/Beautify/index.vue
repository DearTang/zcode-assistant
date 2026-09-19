<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { confirmDialog, MyButton, MyColorField, MyInput, MyPanel, MySelect, MySlider, MyTag, MyToggle } from 'myui'
import { beautify as bf } from '@/api'
import { toast } from '@/composables/toast'
import type { BeautifyConfig, BeautifyPreset, BeautifyTemplate } from '@/types'

defineOptions({ name: 'BeautifyView' })

/** 常用 UI（无衬线）字体候选，值为 CSS font-family 首项 */
const UI_FONTS = [
  { v: '', label: '跟随 ZCode 默认' },
  { v: 'Microsoft YaHei UI', label: '微软雅黑' },
  { v: 'Source Han Sans SC', label: '思源黑体' },
  { v: 'Noto Sans SC', label: 'Noto Sans SC' },
  { v: 'PingFang SC', label: '苹方（macOS）' },
  { v: 'HarmonyOS Sans SC', label: '鸿蒙黑体' },
  { v: 'Segoe UI', label: 'Segoe UI' },
]

/** 常用等宽（代码/终端）字体候选 */
const MONO_FONTS = [
  { v: '', label: '跟随 ZCode 默认' },
  { v: 'Cascadia Code', label: 'Cascadia Code' },
  { v: 'JetBrains Mono', label: 'JetBrains Mono' },
  { v: 'Fira Code', label: 'Fira Code' },
  { v: 'Source Code Pro', label: 'Source Code Pro' },
  { v: 'Maple Mono', label: 'Maple Mono' },
  { v: 'Consolas', label: 'Consolas' },
]

/** 预设主题色卡预览（背景色 + 品牌色），与后端 preset_vars 对应 */
const PREVIEW: Record<string, { bg: string; brand: string }> = {
  midnight: { bg: '#0b1020', brand: '#7c8cff' },
  nord: { bg: '#2e3440', brand: '#88c0d0' },
  dracula: { bg: '#282a36', brand: '#bd93f9' },
  gruvbox: { bg: '#282828', brand: '#fabd2f' },
  'tokyo-night': { bg: '#1a1b26', brand: '#7aa2f7' },
  'rose-pine': { bg: '#191724', brand: '#c4a7e7' },
}

/** 模拟桌面壁纸渐变（预览用，代表窗口外的桌面内容） */
const DESKTOP_GRADIENT = 'linear-gradient(135deg, #2b5876 0%, #4e4376 45%, #b06ab3 100%)'

const defaultCfg = (): BeautifyConfig => ({
  enabled: true,
  theme: 'tokyo-night',
})

/** 壁纸是否为视频（mp4/webm/mov 无法用 <img> 预览） */
const isVideoPath = (p?: string) => /\.(mp4|webm|mov)$/i.test(p ?? '')

const presets = ref<BeautifyPreset[]>([])
const cfg = ref<BeautifyConfig>(defaultCfg())
const installed = ref(false)
const hasBackup = ref(false)
const zcodeVersion = ref('')
const backupVersion = ref('')
const asarPath = ref('')
const busy = ref<'apply' | 'restore' | null>(null)
const loading = ref(true)
const preview = ref<string | null>(null)
const templates = ref<BeautifyTemplate[]>([])
const templateName = ref('')
const activeTemplate = ref('')
const syncedAt = ref<number | null>(null)
const syncing = ref(false)
/** 最近一次热保存的配置快照（跳过载入回显触发的保存） */
const lastSaved = ref('')

function patchCfg(patch: Partial<BeautifyConfig>): void {
  cfg.value = { ...cfg.value, ...patch }
}

async function reload(): Promise<void> {
  try {
    const st = await bf.getStatus()
    installed.value = st.installed
    hasBackup.value = st.has_backup
    zcodeVersion.value = st.zcode_version ?? ''
    backupVersion.value = st.backup_version ?? ''
    asarPath.value = st.asar_path ?? ''
    // 用已存配置覆盖默认值；无配置则保持默认（编辑态友好起点）
    const c = st.config
    const hasAny =
      c &&
      (c.enabled ||
        c.theme ||
        c.ui_font ||
        c.mono_font ||
        c.bg_color ||
        c.primary_color ||
        c.acrylic ||
        c.wallpaper ||
        c.bg_image)
    const merged = { ...defaultCfg(), ...(hasAny ? c : {}) }
    cfg.value = merged
    lastSaved.value = JSON.stringify({ ...merged, enabled: true })
  } catch (e: unknown) {
    toast.error(String(e))
  } finally {
    loading.value = false
  }
}

onMounted(() => {
  void reload()
  bf.getPresets().then((v) => (presets.value = v)).catch(() => {})
  bf.listTemplates().then((v) => (templates.value = v)).catch(() => {})
})

// 已注入状态下参数变更 → 热保存（防抖 600ms）：只写 zq-vars.css，约 1 秒生效
let saveTimer = 0
watch(
  cfg,
  () => {
    if (!installed.value || loading.value) return
    const payload: BeautifyConfig = { ...cfg.value, enabled: true }
    const json = JSON.stringify(payload)
    if (json === lastSaved.value) return
    clearTimeout(saveTimer)
    saveTimer = window.setTimeout(() => {
      syncing.value = true
      bf.saveParams(payload)
        .then(() => {
          lastSaved.value = json
          syncedAt.value = Date.now()
        })
        .catch((e: unknown) => toast.error(String(e)))
        .finally(() => (syncing.value = false))
    }, 600)
  },
  { deep: true },
)
onBeforeUnmount(() => clearTimeout(saveTimer))

// 壁纸变化时加载预览（base64 data URL；视频或 >8MB 返回 null 则不显示）
const wallpaperPath = computed(() => cfg.value.wallpaper ?? cfg.value.bg_image)
watch(
  wallpaperPath,
  (p) => {
    preview.value = null
    if (p && !isVideoPath(p)) {
      bf.readImagePreview(p)
        .then((d) => (preview.value = d))
        .catch(() => {})
    }
  },
  { immediate: true },
)

async function apply(): Promise<void> {
  busy.value = 'apply'
  try {
    await bf.apply({ ...cfg.value, enabled: true })
    toast.success('美化已应用，重启 ZCode 后生效')
    await reload()
  } catch (e: unknown) {
    toast.error(String(e))
  } finally {
    busy.value = null
  }
}

async function restore(): Promise<void> {
  const ok = await confirmDialog({
    message: '确定还原为 ZCode 官方外观？当前美化将被移除。',
    type: 'warning',
    confirmButtonText: '还原',
  })
  if (!ok) return
  busy.value = 'restore'
  try {
    await bf.restore()
    toast.success('已还原官方外观，重启 ZCode 后生效')
    await reload()
  } catch (e: unknown) {
    toast.error(String(e))
  } finally {
    busy.value = null
  }
}

async function pickWallpaper(): Promise<void> {
  try {
    const p = await bf.pickImage()
    if (p) patchCfg({ wallpaper: p, bg_image: undefined })
  } catch (e: unknown) {
    toast.error(String(e))
  }
}

// 默认模板名：方案N（取最小可用序号）
function nextTemplateName(): string {
  for (let n = 1; ; n++) {
    const name = `方案${n}`
    if (!templates.value.some((t) => t.name === name)) return name
  }
}

async function saveTemplate(): Promise<void> {
  const name = templateName.value.trim() || nextTemplateName()
  try {
    const list = await bf.saveTemplate(name, { ...cfg.value, enabled: true })
    templates.value = list
    templateName.value = ''
    activeTemplate.value = name
    toast.success(`模板「${name}」已保存`)
  } catch (e: unknown) {
    toast.error(String(e))
  }
}

function loadTemplate(t: BeautifyTemplate): void {
  cfg.value = { ...defaultCfg(), ...t.config }
  activeTemplate.value = t.name
  toast.success(
    installed.value ? `已载入模板「${t.name}」，参数实时生效中` : `已载入模板「${t.name}」，点「应用美化」后生效`,
  )
}

async function removeTemplate(name: string): Promise<void> {
  const ok = await confirmDialog({ message: `删除模板「${name}」？`, type: 'warning', confirmButtonText: '删除' })
  if (!ok) return
  try {
    const list = await bf.deleteTemplate(name)
    templates.value = list
    if (activeTemplate.value === name) activeTemplate.value = ''
  } catch (e: unknown) {
    toast.error(String(e))
  }
}

const busyNow = computed(() => busy.value !== null)
// 备份版本与当前 ZCode 不一致 = 备份已过期（升级替换了 app.asar，备份没跟上）
const backupStale = computed(
  () => hasBackup.value && !!backupVersion.value && !!zcodeVersion.value && backupVersion.value !== zcodeVersion.value,
)
// 透出模式：毛玻璃或壁纸任一启用
const translucencyActive = computed(() => !!cfg.value.acrylic || !!cfg.value.wallpaper || !!cfg.value.bg_image)
// 预览用表面色：自定义背景色 > 主题背景色 > ZCode 暗色默认
const surfaceColor = computed(() => cfg.value.bg_color || PREVIEW[cfg.value.theme ?? '']?.bg || '#171717')
// 三分区滑块值：未单独设置时跟随「桌面不透明度」
const surf = computed(() => cfg.value.surface_opacity ?? 0.72)
const sideOp = computed(() => Math.round((cfg.value.sidebar_opacity ?? surf.value) * 100))
const panelOp = computed(() => Math.round((cfg.value.panel_opacity ?? surf.value) * 100))
const rightOp = computed(() => Math.round((cfg.value.sidebar_right_opacity ?? surf.value) * 100))

/** 模板摘要：主题 / 字体 / 毛玻璃 / 壁纸 等要点 */
function describeTemplate(c: BeautifyConfig): string {
  const parts: string[] = []
  if (c.theme && c.theme !== 'none') {
    parts.push(presets.value.find((p) => p.id === c.theme)?.name ?? c.theme)
  }
  if (c.ui_font) parts.push(c.ui_font)
  if (c.bg_color) parts.push(`背景色 ${c.bg_color}`)
  if (c.acrylic) parts.push('毛玻璃')
  const wp = c.wallpaper ?? c.bg_image
  if (wp) parts.push(/\.(mp4|webm|mov)$/i.test(wp) ? '视频壁纸' : '壁纸')
  return parts.length > 0 ? parts.join(' · ') : '默认外观'
}

const uiFontOptions = UI_FONTS.map((f) => ({ label: f.label, value: f.v }))
const monoFontOptions = MONO_FONTS.map((f) => ({ label: f.label, value: f.v }))
</script>

<template>
  <div class="bf">
    <!-- 状态与还原 -->
    <MyPanel title="状态">
      <template #actions>
        <MyTag :type="installed ? 'success' : 'info'" size="small" round>
          {{ loading ? '检测中…' : installed ? '已注入美化' : '官方外观' }}
        </MyTag>
      </template>
      <p class="bf-desc">
        通过向 ZCode 的 <code>app.asar</code> 注入三个 <code>file://</code> 外链（变量 / 主题 / 运行时脚本）实现换肤、
        换字体、毛玻璃、视频壁纸。首次应用会自动备份原始包；此后在已注入状态下
        <b>所有参数改动约 1 秒实时生效</b>，无需重复应用。
        ZCode 自动更新后美化会失效，需重新应用。
      </p>
      <div class="bf-meta">
        <span>ZCode 版本：<span class="bf-mono">{{ zcodeVersion || '—' }}</span></span>
        <span>
          原始备份：
          {{ hasBackup ? (backupVersion ? `已有（v${backupVersion}）` : '已有（版本未知）') : '无（首次应用时创建）' }}
        </span>
      </div>
      <p v-if="backupStale" class="bf-warn">
        备份属于 ZCode v{{ backupVersion }}，与当前 v{{ zcodeVersion }} 不一致：
        {{
          installed
            ? '当前已注入且没有该版本官方包，无法安全还原；如需还原请先重装/修复 ZCode。'
            : '下次「应用美化」会用当前官方包自动重建备份。'
        }}
      </p>
      <div v-if="asarPath" class="bf-asar" :title="asarPath">{{ asarPath }}</div>
      <div class="bf-actions">
        <MyButton
          size="small"
          :disabled="busyNow || !hasBackup || backupStale"
          :title="backupStale ? '备份版本与当前 ZCode 不一致，还原会把旧文件盖到新版本上，已禁止' : hasBackup ? '用备份恢复官方 app.asar' : '尚无备份'"
          @click="restore"
        >
          {{ busy === 'restore' ? '还原中…' : '还原官方外观' }}
        </MyButton>
      </div>
    </MyPanel>

    <!-- 预设主题 -->
    <MyPanel title="配色主题">
      <p class="bf-desc">选一个预设覆盖核心配色 token；选「跟随默认」则不改颜色。</p>
      <div class="bf-themes">
        <button
          type="button"
          class="bf-theme"
          :class="{ selected: !cfg.theme || cfg.theme === 'none' }"
          @click="patchCfg({ theme: 'none' })"
        >
          <span class="bf-theme-swatch" style="--swatch-bg: transparent; --swatch-brand: var(--text-secondary)">
            <span class="bf-theme-bar" />
          </span>
          <span class="bf-theme-name">跟随默认</span>
        </button>
        <button
          v-for="p in presets"
          :key="p.id"
          type="button"
          class="bf-theme"
          :class="{ selected: cfg.theme === p.id }"
          :style="{
            '--swatch-bg': (PREVIEW[p.id] ?? { bg: '#1e1e1e' }).bg,
            '--swatch-brand': (PREVIEW[p.id] ?? { brand: '#888' }).brand,
          }"
          @click="patchCfg({ theme: p.id })"
        >
          <span class="bf-theme-swatch"><span class="bf-theme-bar" /></span>
          <span class="bf-theme-name">{{ p.name }}</span>
        </button>
      </div>
    </MyPanel>

    <!-- 字体 -->
    <MyPanel title="字体">
      <p class="bf-desc">
        UI 字体影响界面文字；等宽字体影响代码 / 终端显示。留空则沿用 ZCode 默认。（字号请在 ZCode 设置内调整。）
      </p>
      <div class="bf-grid2">
        <MyFieldShell label="UI 字体（--font-sans）">
          <MySelect
            :model-value="cfg.ui_font ?? ''"
            :options="uiFontOptions"
            :filterable="false"
            @update:model-value="patchCfg({ ui_font: ($event as string) || undefined })"
          />
        </MyFieldShell>
        <MyFieldShell label="等宽字体（--font-mono）">
          <MySelect
            :model-value="cfg.mono_font ?? ''"
            :options="monoFontOptions"
            :filterable="false"
            @update:model-value="patchCfg({ mono_font: ($event as string) || undefined })"
          />
        </MyFieldShell>
      </div>
    </MyPanel>

    <!-- 自定义颜色 -->
    <MyPanel title="自定义颜色（可选）">
      <p class="bf-desc">在预设之上进一步覆盖。留空表示不覆盖。自定义优先级高于预设。</p>
      <div class="bf-grid2">
        <MyColorField label="背景色（--color-background）" :model-value="cfg.bg_color ?? null" @update:model-value="patchCfg({ bg_color: $event ?? undefined })" />
        <MyColorField label="主色调（--color-primary）" :model-value="cfg.primary_color ?? null" @update:model-value="patchCfg({ primary_color: $event ?? undefined })" />
      </div>
    </MyPanel>

    <!-- 毛玻璃 -->
    <MyPanel title="毛玻璃">
      <p class="bf-desc">
        ZCode 在 Windows 上默认启用了 acrylic 材质，但被不透明的界面底色盖住。
        开启后界面表面变半透明，透出原生毛玻璃模糊。
      </p>
      <div class="bf-toggle-row">
        <MyToggle :model-value="!!cfg.acrylic" title="毛玻璃（透出 Windows acrylic）" @update:model-value="patchCfg({ acrylic: $event === true })" />
        <span>毛玻璃：{{ cfg.acrylic ? '开' : '关' }}</span>
      </div>
      <div class="bf-range">
        <span class="bf-range-label">桌面不透明度（越小越透）</span>
        <MySlider
          :model-value="Math.round(surf * 100)"
          :min="20"
          :max="100"
          unit="%"
          :disabled="!translucencyActive"
          @update:model-value="patchCfg({ surface_opacity: ($event as number) / 100 })"
        />
      </div>

      <!-- 毛玻璃实时预览：模拟桌面 + 背景图层 + 半透明界面表面 -->
      <div class="bf-preview-cap">效果预览（模拟桌面 + 半透明界面，随上方滑块实时变化）</div>
      <div class="bf-preview">
        <div class="bf-preview-desktop" />
        <img
          v-if="wallpaperPath && !isVideoPath(wallpaperPath) && preview"
          class="bf-preview-img"
          :src="preview"
          alt=""
          :style="{ opacity: cfg.bg_image_opacity ?? 1 }"
        />
        <div class="bf-preview-surface" :style="{ background: surfaceColor, opacity: translucencyActive ? surf : 1 }" />
        <span class="bf-preview-bar w110" />
        <span class="bf-preview-bar dim w180" style="top: 32px" />
        <span class="bf-preview-bar dim w150" style="top: 46px" />
        <span class="bf-preview-box" />
      </div>
    </MyPanel>

    <!-- 壁纸 -->
    <MyPanel title="壁纸">
      <p class="bf-desc">
        选择本地图片或视频（mp4 / webm / mov）作为窗口背景。应用后以独立图层
        置于界面之下，透过半透明表面显现；换图 / 调滤镜实时生效。
      </p>
      <div class="bf-wp-row">
        <MyButton size="small" @click="pickWallpaper">选择壁纸…</MyButton>
        <template v-if="wallpaperPath">
          <span class="bf-wp-name" :title="wallpaperPath">
            {{ isVideoPath(wallpaperPath) ? '🎬 ' : '' }}{{ wallpaperPath.split(/[\\/]/).pop() }}
          </span>
          <MyButton size="small" @click="patchCfg({ wallpaper: undefined, bg_image: undefined })">移除</MyButton>
        </template>
        <span v-else class="bf-faint">未设置（png / jpg / webp / gif / mp4 / webm / mov）</span>
      </div>

      <div class="bf-range">
        <span class="bf-range-label">壁纸不透明度</span>
        <MySlider
          :model-value="Math.round((cfg.bg_image_opacity ?? 1) * 100)"
          :min="10"
          :max="100"
          unit="%"
          :disabled="!wallpaperPath"
          @update:model-value="patchCfg({ bg_image_opacity: ($event as number) / 100 })"
        />
      </div>

      <!-- 壁纸滤镜 -->
      <template v-if="wallpaperPath">
        <div class="bf-range">
          <span class="bf-range-label">亮度</span>
          <MySlider
            :model-value="Math.round((cfg.wp_brightness ?? 1.1) * 100)"
            :min="20"
            :max="200"
            unit="%"
            @update:model-value="patchCfg({ wp_brightness: ($event as number) / 100 })"
          />
        </div>
        <div class="bf-range">
          <span class="bf-range-label">饱和度</span>
          <MySlider
            :model-value="Math.round((cfg.wp_saturate ?? 1.4) * 100)"
            :min="0"
            :max="200"
            unit="%"
            @update:model-value="patchCfg({ wp_saturate: ($event as number) / 100 })"
          />
        </div>
        <div class="bf-range">
          <span class="bf-range-label">模糊</span>
          <MySlider
            :model-value="cfg.wp_blur ?? 0"
            :min="0"
            :max="30"
            unit="px"
            @update:model-value="patchCfg({ wp_blur: $event as number })"
          />
        </div>
        <div class="bf-range">
          <span class="bf-range-label">压暗遮罩（壁纸过亮时压暗保证文字可读）</span>
          <MySlider
            :model-value="Math.round((cfg.mask_strength ?? 0) * 100)"
            :min="0"
            :max="90"
            unit="%"
            @update:model-value="patchCfg({ mask_strength: ($event as number) / 100 })"
          />
        </div>
        <div v-if="isVideoPath(wallpaperPath)" class="bf-range">
          <span class="bf-range-label">播放速率（视频壁纸）</span>
          <MySlider
            :model-value="Math.round((cfg.playback_rate ?? 1) * 100)"
            :min="25"
            :max="400"
            unit="%"
            @update:model-value="patchCfg({ playback_rate: ($event as number) / 100 })"
          />
        </div>
      </template>

      <!-- 图片壁纸预览 -->
      <template v-if="wallpaperPath && preview">
        <div class="bf-preview-cap">效果预览</div>
        <div class="bf-preview" style="--preview-bg: none; background: var(--preview-bg, none)">
          <div class="bf-preview-desktop" />
          <img class="bf-preview-img" :src="preview" alt="壁纸预览" :style="{ opacity: cfg.bg_image_opacity ?? 1 }" />
        </div>
      </template>
    </MyPanel>

    <!-- 分区透明度 -->
    <MyPanel title="分区透明度与文字可读性">
      <p class="bf-desc">
        左栏 / 对话区 / 右栏三块主区域各自独立控制透明度，互不牵连；
        不单独调整时跟随「桌面不透明度」。壁纸过亮 / 过暗时可用文字描边
        把前景文字从背景里托出来。
      </p>
      <div class="bf-range">
        <span class="bf-range-label">左栏透明度</span>
        <MySlider
          :model-value="sideOp"
          :min="0"
          :max="100"
          unit="%"
          :disabled="!translucencyActive"
          @update:model-value="patchCfg({ sidebar_opacity: ($event as number) / 100 })"
        />
      </div>
      <div class="bf-range">
        <span class="bf-range-label">对话区透明度</span>
        <MySlider
          :model-value="panelOp"
          :min="0"
          :max="100"
          unit="%"
          :disabled="!translucencyActive"
          @update:model-value="patchCfg({ panel_opacity: ($event as number) / 100 })"
        />
      </div>
      <div class="bf-range">
        <span class="bf-range-label">右栏透明度</span>
        <MySlider
          :model-value="rightOp"
          :min="0"
          :max="100"
          unit="%"
          :disabled="!translucencyActive"
          @update:model-value="patchCfg({ sidebar_right_opacity: ($event as number) / 100 })"
        />
      </div>
      <div class="bf-range">
        <span class="bf-range-label">文字描边</span>
        <MySlider
          :model-value="Math.round((cfg.text_shadow ?? 0) * 100)"
          :min="0"
          :max="100"
          unit="%"
          :disabled="!translucencyActive"
          @update:model-value="patchCfg({ text_shadow: ($event as number) / 100 })"
        />
      </div>
    </MyPanel>

    <!-- 应用 + 模板 -->
    <MyPanel>
      <div class="bf-apply-row">
        <MyButton variant="primary" :loading="busyNow" @click="apply">
          {{ busy === 'apply' ? '正在写入 app.asar…' : installed ? '重新应用美化' : '应用美化' }}
        </MyButton>
        <span v-if="busyNow" class="bf-faint">正在补丁 app.asar（约 300MB），需数秒~十几秒，请稍候…</span>
        <span v-if="installed && !busyNow" class="bf-faint">
          {{ syncing ? '正在同步参数…' : syncedAt ? `参数已实时生效（${new Date(syncedAt).toLocaleTimeString()}）` : '参数改动自动实时生效（约 1 秒）' }}
        </span>
      </div>
      <p class="bf-desc">
        「应用」只在首次注入、ZCode 升级后或需要修复注入时使用：
        会先关闭 ZCode 以释放文件锁，完成后询问是否重启。
        已注入状态下日常调整参数无需点「应用」。
      </p>

      <!-- 我的模板 -->
      <div class="bf-templates">
        <h4 class="bf-tpl-title">我的模板</h4>
        <p class="bf-faint">把当前全部设置保存为模板，下次一键载入。不填名称默认「方案1」，序号依次递增。</p>
        <div class="bf-tpl-save">
          <MyInput v-model="templateName" class="bf-tpl-name" :placeholder="nextTemplateName()" />
          <MyButton size="small" @click="saveTemplate">保存当前设置为模板</MyButton>
        </div>
        <div v-if="templates.length > 0" class="bf-tpl-list">
          <div
            v-for="t in templates"
            :key="t.name"
            class="bf-tpl-row"
            :class="{ active: activeTemplate === t.name }"
          >
            <div class="bf-tpl-copy">
              <div class="bf-tpl-name-text" :class="{ active: activeTemplate === t.name }">{{ t.name }}</div>
              <div class="bf-tpl-desc">{{ describeTemplate(t.config) }}</div>
            </div>
            <MyButton size="small" @click="loadTemplate(t)">载入</MyButton>
            <MyButton size="small" @click="removeTemplate(t.name)">删除</MyButton>
          </div>
        </div>
        <p v-else class="bf-faint" style="margin-top: 10px">暂无模板</p>
      </div>
    </MyPanel>
  </div>
</template>

<style scoped>
.bf {
  display: grid;
  gap: var(--ui-gap);
}
.bf-desc {
  margin: 0 0 12px;
  color: var(--text-secondary);
  font-size: 12px;
  line-height: 1.7;
}
.bf-desc code {
  font-family: var(--font-mono);
  font-size: 11px;
  background: var(--bg-elevated);
  padding: 1px 5px;
  border-radius: 4px;
}
.bf-meta {
  display: flex;
  gap: 16px;
  flex-wrap: wrap;
  font-size: 12px;
  color: var(--text-tertiary);
}
.bf-mono {
  font-family: var(--font-mono);
}
.bf-warn {
  margin: 10px 0 0;
  font-size: 12px;
  color: var(--warning);
  line-height: 1.6;
}
.bf-asar {
  color: var(--text-tertiary);
  font-family: var(--font-mono);
  font-size: 11px;
  margin-top: 6px;
  word-break: break-all;
}
.bf-actions {
  display: flex;
  gap: 8px;
  margin-top: 14px;
}
.bf-themes {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
  gap: 10px;
}
.bf-theme {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 10px;
  border-radius: 10px;
  border: 1px solid var(--border-subtle);
  background: var(--bg-surface);
  cursor: pointer;
  text-align: left;
}
.bf-theme.selected {
  border: 1.5px solid var(--accent);
  background: var(--accent-subtle);
}
.bf-theme-swatch {
  position: relative;
  height: 40px;
  border-radius: 6px;
  border: 1px solid var(--border-subtle);
  background: var(--swatch-bg);
  overflow: hidden;
  display: block;
}
.bf-theme-bar {
  position: absolute;
  left: 8px;
  bottom: 6px;
  width: 26px;
  height: 5px;
  border-radius: 3px;
  background: var(--swatch-brand);
  display: block;
}
.bf-theme-name {
  font-size: 13px;
  color: var(--text-primary);
}
.bf-theme.selected .bf-theme-name {
  font-weight: 600;
}
.bf-grid2 {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
  gap: 10px;
}
.bf-toggle-row {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 12px;
  font-size: 13px;
  color: var(--text-secondary);
}
.bf-range {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin-bottom: 10px;
}
.bf-range-label {
  font-size: 13px;
  color: var(--text-secondary);
}
.bf-range:has(.us-slider.is-disabled),
.bf-range:has([disabled]) {
  opacity: 0.55;
}
.bf-preview-cap {
  font-size: 11px;
  color: var(--text-tertiary);
  margin: 6px 0 4px;
}
.bf-preview {
  position: relative;
  height: 96px;
  border-radius: 8px;
  overflow: hidden;
  border: 1px solid var(--border-subtle);
}
.bf-preview-desktop {
  position: absolute;
  inset: 0;
  background: v-bind('DESKTOP_GRADIENT');
}
.bf-preview-img {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  object-fit: cover;
}
.bf-preview-surface {
  position: absolute;
  inset: 0;
}
.bf-preview-bar {
  position: absolute;
  left: 12px;
  top: 14px;
  height: 9px;
  border-radius: 4px;
  background: rgba(255, 255, 255, 0.38);
}
.bf-preview-bar.dim {
  height: 7px;
  background: rgba(255, 255, 255, 0.22);
}
.w110 {
  width: 110px;
}
.w180 {
  width: 180px;
}
.w150 {
  width: 150px;
}
.bf-preview-box {
  position: absolute;
  right: 12px;
  top: 14px;
  width: 64px;
  height: 39px;
  border-radius: 6px;
  background: rgba(255, 255, 255, 0.14);
  border: 1px solid rgba(255, 255, 255, 0.18);
}
.bf-wp-row {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}
.bf-wp-name {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--text-secondary);
  max-width: 320px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.bf-faint {
  color: var(--text-tertiary);
  font-size: 11px;
}
.bf-apply-row {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
}
.bf-templates {
  margin-top: 14px;
  padding-top: 12px;
  border-top: 1px solid var(--border-subtle);
}
.bf-tpl-title {
  margin: 0 0 4px;
  font-size: 14px;
}
.bf-tpl-save {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 10px;
}
.bf-tpl-name {
  flex: 1;
  max-width: 200px;
}
.bf-tpl-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-top: 10px;
}
.bf-tpl-row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 10px;
  border-radius: 8px;
  border: 1px solid var(--border-subtle);
}
.bf-tpl-row.active {
  border-color: var(--accent);
  background: var(--accent-subtle);
}
.bf-tpl-copy {
  flex: 1;
  min-width: 0;
}
.bf-tpl-name-text {
  font-size: 13px;
}
.bf-tpl-name-text.active {
  font-weight: 600;
}
.bf-tpl-desc {
  color: var(--text-tertiary);
  font-size: 11px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
