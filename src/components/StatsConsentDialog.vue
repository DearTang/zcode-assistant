<script setup lang="ts">
import { MyButton, MyDialog } from 'myui'

/**
 * 一次性匿名使用统计同意弹窗。每个新版本首次启动时显示（若用户此前未同意）。
 * 说明采集什么（仅版本 + 系统 + 随机设备 ID）、不采集什么，以及同意模型。
 * 遮罩点击 / Esc = 拒绝（dismissable + cancel 事件映射 onDecline）。
 */
defineProps<{ version: string }>()
defineEmits<{ agree: []; decline: [] }>()
defineOptions({ name: 'StatsConsentDialog' })
</script>

<template>
  <MyDialog
    :model-value="true"
    title="帮助 zcode-assistant 变得更好"
    :width="460"
    dismissable
    hide-footer
    @cancel="$emit('decline')"
    @update:model-value="$emit('decline')"
  >
    <div class="sc">
      <p class="sc-lead">
        检测到你升级到了 v{{ version }}。是否允许发送一次
        <strong>完全匿名</strong> 的统计数据，帮助我们了解有多少用户在使用？
      </p>

      <div class="sc-box">
        <div class="sc-box-title">收集的内容（仅此而已）：</div>
        <div>✓ 应用版本号（v{{ version }}）</div>
        <div>✓ 操作系统（如 Windows）</div>
        <div>✓ 一个随机设备 ID（用于去重计数，不绑定任何个人信息）</div>
        <div class="sc-box-title sc-mt">绝不收集：</div>
        <div>✗ 服务器地址 / 用户名 / 密码 / API Key / 连接内容</div>
      </div>

      <p class="sc-note">
        每次升级到新版本都会询问一次。同意将发送本次匿名统计；选择暂不则本次不发送。
      </p>

      <div class="sc-actions">
        <MyButton variant="secondary" @click="$emit('decline')">暂不</MyButton>
        <MyButton variant="primary" autofocus @click="$emit('agree')">允许匿名统计</MyButton>
      </div>
    </div>
  </MyDialog>
</template>

<style scoped>
.sc {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.sc-lead {
  margin: 0;
  color: var(--text-secondary);
  font-size: 12px;
  line-height: 1.7;
}
.sc-lead strong {
  color: var(--text-primary);
}
.sc-box {
  margin-top: 12px;
  padding: 12px 14px;
  border: 1px solid var(--border-base);
  border-radius: 10px;
  background: var(--bg-overlay);
  color: var(--text-tertiary);
  font-size: 12px;
  line-height: 1.8;
}
.sc-box-title {
  color: var(--text-secondary);
  font-weight: 600;
}
.sc-mt {
  margin-top: 8px;
}
.sc-note {
  margin: 6px 0 0;
  color: var(--text-tertiary);
  font-size: 11px;
  line-height: 1.6;
}
.sc-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 14px;
}
</style>
