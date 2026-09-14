<script setup lang="ts">
import { onMounted, ref, watch } from 'vue'
import { ElMenu, ElMenuItem, ElScrollbar, ElTooltip } from 'element-plus'
import { MyIcon, MyStatusDot } from 'myui'
import { app, win, zcode } from '@/api'
import { toast } from '@/composables/toast'
import { persistUi, setThemeChoice, ui } from '@/store/ui'
import type { ViewId } from '@/types'

defineOptions({ name: 'AppSidebar' })

interface NavItem {
  id: ViewId
  icon: string
  label: string
}

const GROUPS: { label: string; items: NavItem[] }[] = [
  {
    label: '监控',
    items: [
      { id: 'dashboard', icon: 'Dashboard', label: '总览' },
      { id: 'usage', icon: 'Chart', label: '用量查询' },
    ],
  },
  {
    label: '管理',
    items: [
      { id: 'models', icon: 'Cpu', label: '模型管理' },
      { id: 'autoswitch', icon: 'Swap', label: '自动切换' },
      { id: 'projects', icon: 'Folder', label: '项目管理' },
      { id: 'accounts', icon: 'User', label: '智谱账号' },
    ],
  },
  {
    label: '工具',
    items: [
      { id: 'proxy', icon: 'Globe', label: '网络代理' },
      { id: 'beautify', icon: 'Sparkle', label: 'ZCode 美化' },
      { id: 'zcode-settings', icon: 'Sliders', label: 'ZCode 设置' },
      { id: 'settings', icon: 'Setting', label: '设置' },
    ],
  },
]

defineProps<{ updateAvailable?: boolean }>()
const emit = defineEmits<{ 'open-about': [] }>()

const version = ref('')
const restarting = ref(false)
const collapsed = ref(false)

// ui.isCollapse 由顶栏/侧栏共同控制，侧栏跟随显示
watch(
  () => ui.isCollapse,
  (v) => (collapsed.value = v),
  { immediate: true },
)

function onSelect(id: string): void {
  ui.activeView = id as ViewId
}

function toggleTheme(): void {
  setThemeChoice(ui.isDark ? 'light' : 'dark')
}

function toggleCollapse(): void {
  ui.isCollapse = !ui.isCollapse
  persistUi()
}

async function restartZcode(): Promise<void> {
  restarting.value = true
  try {
    await zcode.restartZcode()
    toast.success('ZCode 已重启')
  } catch (e: unknown) {
    toast.error(typeof e === 'string' ? e : '重启失败')
  } finally {
    restarting.value = false
  }
}

onMounted(() => {
  app.getVersion().then((v) => (version.value = v)).catch(() => {})
})
</script>

<template>
  <aside class="ui-sidebar" :class="{ 'is-collapsed': collapsed }">
    <ElScrollbar class="ui-nav-scroll">
      <template v-for="group in GROUPS" :key="group.label">
        <p class="ui-nav-group-label">{{ group.label }}</p>
        <ElMenu :default-active="ui.activeView" :collapse="collapsed" :collapse-transition="false" @select="onSelect">
          <template v-for="item in group.items" :key="item.id">
            <ElTooltip :content="item.label" placement="right" :disabled="!collapsed" :show-after="150">
              <ElMenuItem :index="item.id">
                <MyIcon :name="item.icon" :size="16" />
                <template #title>{{ item.label }}</template>
              </ElMenuItem>
            </ElTooltip>
          </template>
        </ElMenu>
      </template>
    </ElScrollbar>

    <div class="ui-sidebar__bottom">
      <div class="ui-sidebar__row">
        <button class="ui-side-btn" type="button" @click="toggleTheme">
          <MyIcon :name="ui.isDark ? 'Sun' : 'Moon'" :size="16" />
          <span class="grow">{{ ui.isDark ? '浅色模式' : '深色模式' }}</span>
        </button>
      </div>
      <div class="ui-sidebar__row">
        <button class="ui-side-btn" type="button" :disabled="restarting" @click="restartZcode">
          <MyIcon name="Power" :size="16" />
          <span class="grow">{{ restarting ? '重启中…' : '重启 ZCode' }}</span>
        </button>
      </div>
      <div class="ui-sidebar__row">
        <button class="ui-side-btn" type="button" @click="win.quitApp()">
          <MyIcon name="Close" :size="16" />
          <span class="grow">退出</span>
        </button>
        <button
          v-if="version"
          class="ui-version-btn"
          type="button"
          :title="updateAvailable ? '发现新版本，点击查看' : '关于 zcode-assistant'"
          @click="emit('open-about')"
        >
          <span class="ui-version-text">v{{ version }}</span>
          <MyStatusDot v-if="updateAvailable" tone="success" pulse />
        </button>
      </div>
    </div>
  </aside>
</template>

<style scoped>
.ui-nav-scroll {
  flex: 1;
  min-height: 0;
}

.ui-nav-scroll :deep(.el-scrollbar__view) {
  padding-bottom: 10px;
}

.ui-nav-scroll :deep(.el-menu) {
  width: 100%;
  border-right: 0;
  background: transparent;
}

.ui-nav-scroll :deep(.el-menu-item) {
  height: 38px;
  margin: 2px 10px;
  border-radius: 8px;
  font-size: 13px;
}

.ui-nav-scroll :deep(.el-menu-item:hover) {
  background: var(--surface-translucent-hover);
}

.ui-nav-scroll :deep(.el-menu-item.is-active) {
  background: var(--accent-subtle);
  color: var(--accent);
}

.is-collapsed .ui-nav-scroll :deep(.el-menu--collapse .el-menu-item) {
  margin: 2px 12px;
  padding: 0 !important;
}
</style>
