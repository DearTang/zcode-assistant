<script setup lang="ts">
import { MyIcon } from 'myui'
import { prefs } from '@/api'
import { persistUi, setThemeChoice, ui } from '@/store/ui'

defineOptions({ name: 'AppTopBar' })

defineProps<{ title: string; subtitle?: string }>()

function toggleCollapse(): void {
  ui.isCollapse = !ui.isCollapse
  persistUi()
}

function toggleTheme(): void {
  setThemeChoice(ui.isDark ? 'light' : 'dark')
}
</script>

<template>
  <header class="ui-topbar">
    <div class="ui-topbar__left">
      <button class="ui-icon-btn" type="button" :aria-expanded="!ui.isCollapse" aria-label="切换侧栏" @click="toggleCollapse">
        <MyIcon name="Menu" :size="17" />
      </button>
      <div class="ui-brand">
        <span class="ui-brand__mark">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"
            stroke-linecap="round" stroke-linejoin="round">
            <path d="M7 7 L17 7 L7 17 L17 17" />
          </svg>
        </span>
        <span class="ui-brand__copy">
          <span class="ui-brand__name">zcode-assistant</span>
          <span class="ui-brand__subtitle">增强工具</span>
        </span>
      </div>
      <span class="ui-topbar__divider" />
      <div class="ui-topbar__titles">
        <span class="ui-topbar__title">{{ title }}</span>
        <span v-if="subtitle" class="ui-topbar__sub">{{ subtitle }}</span>
      </div>
    </div>

    <div class="ui-topbar__right">
      <button class="ui-search-trigger" type="button" @click="ui.paletteOpen = true">
        <MyIcon name="Search" :size="14" />
        <span>搜索功能或命令</span>
        <kbd>Ctrl K</kbd>
      </button>

      <button class="ui-icon-btn" type="button" title="显示悬浮球" @click="prefs.setFloatBallVisible(true).catch(() => {})">
        <MyIcon name="Activity" :size="16" />
      </button>

      <button class="ui-icon-btn" type="button" :aria-label="ui.isDark ? '切换浅色' : '切换深色'" @click="toggleTheme">
        <MyIcon :name="ui.isDark ? 'Sun' : 'Moon'" :size="16" />
      </button>
    </div>
  </header>
</template>

<style scoped>
.ui-topbar__divider {
  width: 1px;
  height: 20px;
  background: var(--border-base);
}
</style>
