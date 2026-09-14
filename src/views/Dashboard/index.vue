<script setup lang="ts">
import { computed } from 'vue'
import { MyButton, MyIcon, MyPanel, MyProgress, MyResultState, MyStatCard, MySegmented } from 'myui'
import { formatUnits } from '@/api'
import type { QuotaOverview, UsageDisplayMode } from '@/types'

/**
 * 总览视图：配额数据由 AppShell 的全局轮询（唯一查询源）下发，本组件纯展示。
 * 刷新按钮触发 AppShell 的查询（non-silent，会广播给悬浮窗 / 托盘）。
 */
const props = defineProps<{
  data: QuotaOverview | null
  loading: boolean
  error: string | null
  /** 模型用量展示方案（与悬浮球/托盘同源）：已用 / 剩余 */
  usageDisplay?: UsageDisplayMode
}>()
const emit = defineEmits<{ refresh: [] }>()
defineOptions({ name: 'DashboardView' })

// 展示方案：数字与进度条宽度随方案（已用 / 剩余），颜色始终按已用度分级
const showRemaining = computed(() => props.usageDisplay === 'remaining')

const metaLine = computed(() => {
  if (!props.data) return ''
  const t = new Date(props.data.fetchedAt).toLocaleTimeString()
  return `更新于 ${t} · 数据源 ${props.data.source}`
})
</script>

<template>
  <div class="dash">
    <div class="dash-stats">
      <MyStatCard
        icon="User"
        label="当前账号"
        :value="data?.accountLabel ?? (loading ? '加载中…' : '-')"
      />
      <MyStatCard
        icon="Cpu"
        label="套餐"
        :value="data?.planName ?? '-'"
        :loading="loading && !data"
      />
      <MyStatCard
        icon="Zap"
        label="供应商"
        :value="data?.providerName ?? (loading ? '加载中…' : '自动 · 智谱 Coding Plan')"
      />
    </div>

    <MyPanel title="主供应商配额">
      <template #actions>
        <span v-if="metaLine" class="dash-meta">{{ metaLine }}</span>
        <MyButton size="small" :loading="loading" @click="emit('refresh')">
          <MyIcon name="Refresh" :size="14" />
          {{ loading ? '刷新中' : '刷新' }}
        </MyButton>
      </template>

      <div class="dash-buckets">
        <div v-for="b in data?.buckets ?? []" :key="b.name" class="dash-bucket">
          <div class="dash-bucket-head">
            <span class="dash-bucket-name">{{ b.name }}</span>
            <span class="dash-bucket-nums">
              {{ formatUnits(b.used, b.unit) }} / {{ formatUnits(b.total, b.unit) }}
              <span class="dash-remain">剩 {{ formatUnits(b.remaining, b.unit) }}</span>
            </span>
          </div>
          <!-- myui v0.9.0 配额语义：宽度按 percentage、颜色档按 colorValue（展示剩余时两者解耦） -->
          <MyProgress
            :percentage="b.total > 0 ? (showRemaining ? 100 - (b.used / b.total) * 100 : (b.used / b.total) * 100) : 0"
            :color-value="b.total > 0 ? (b.used / b.total) * 100 : 0"
            :warn-at="70"
            :danger-at="90"
            :stroke-width="6"
            :show-text="false"
          />
          <div class="dash-bucket-foot">
            <span>
              {{ showRemaining ? '剩余' : '已用' }}
              {{ Math.round(b.total > 0 ? (showRemaining ? 100 - (b.used / b.total) * 100 : (b.used / b.total) * 100) : 0) }}%
            </span>
            <span v-if="b.periodEnd">重置时间 {{ new Date(b.periodEnd).toLocaleString() }}</span>
          </div>
        </div>

        <MyResultState
          v-if="data && data.buckets.length === 0"
          type="empty"
          title="暂无配额数据"
          :description="data.source === 'none' ? '未配置主供应商且未登录智谱 Coding Plan——设置主供应商或登录智谱账号后可查看用量' : ''"
        />
        <MyResultState v-if="error" type="error" title="配额查询失败" :description="error" />
        <div v-if="!data && !error" class="ui-empty">{{ loading ? '加载中…' : '暂无数据' }}</div>
      </div>
    </MyPanel>
  </div>
</template>

<style scoped>
.dash {
  display: grid;
  gap: var(--ui-gap);
}
.dash-stats {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
  gap: var(--ui-gap);
}
.dash-meta {
  margin-right: 8px;
  color: var(--text-tertiary);
  font-size: 11px;
}
.dash-buckets {
  display: flex;
  flex-direction: column;
  gap: 20px;
}
.dash-bucket-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  margin-bottom: 8px;
}
.dash-bucket-name {
  font-weight: 500;
}
.dash-bucket-nums {
  font-family: var(--font-mono);
  font-size: 12px;
  color: var(--text-secondary);
  font-variant-numeric: tabular-nums;
}
.dash-remain {
  margin-left: 8px;
  color: var(--accent);
}
.dash-bucket-foot {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  margin-top: 4px;
  color: var(--text-tertiary);
  font-size: 11px;
}
</style>
