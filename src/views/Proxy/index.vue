<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { MyButton, MyFieldShell, MyInput, MyPanel, MySelect, MyToggle } from 'myui'
import { proxy as proxyApi } from '@/api'
import type { ProxyConfig } from '@/types'
import { toast } from '@/composables/toast'

defineOptions({ name: 'ProxyView' })

const cfg = ref<ProxyConfig>({
  enabled: false,
  type: 'http',
  host: '',
  port: 7890,
  username: '',
  hasPassword: false,
})
const pw = ref('')
const busy = ref(false)
const test = ref<{ ok?: boolean; latencyMs?: number; status?: number; error?: string } | null>(null)

const typeOptions = [
  { label: 'HTTP', value: 'http' },
  { label: 'SOCKS5', value: 'socks5' },
]

onMounted(() => {
  proxyApi
    .get()
    .then((v) => (cfg.value = v))
    .catch(() => {
      /* 未配置 */
    })
})

async function save(): Promise<void> {
  busy.value = true
  try {
    await proxyApi.set({ ...cfg.value, hasPassword: pw.value.length > 0 }, pw.value || undefined)
    toast.success('已保存')
    pw.value = ''
  } catch (e: unknown) {
    toast.error(typeof e === 'string' ? e : '保存失败')
  } finally {
    busy.value = false
  }
}

async function doTest(): Promise<void> {
  busy.value = true
  try {
    test.value = (await proxyApi.test()) as { ok?: boolean; latencyMs?: number; status?: number }
  } catch (e: unknown) {
    test.value = { ok: false, error: String(e) }
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <MyPanel title="网络代理">
    <template #actions>
      <div class="px-toggle">
        <MyToggle v-model="cfg.enabled" />
        <span>{{ cfg.enabled ? '已启用' : '已禁用' }}</span>
      </div>
    </template>

    <p class="px-desc">所有对外请求（配额查询、模型列表拉取）均走此代理。密码存 OS keyring。</p>

    <div class="px-grid">
      <MyFieldShell label="协议">
        <MySelect v-model="cfg.type" :options="typeOptions" :disabled="!cfg.enabled" />
      </MyFieldShell>
      <MyFieldShell label="端口">
        <MyInput v-model.number="cfg.port" type="number" placeholder="7890" :disabled="!cfg.enabled" />
      </MyFieldShell>
      <MyFieldShell label="主机">
        <MyInput v-model="cfg.host" placeholder="127.0.0.1" :disabled="!cfg.enabled" />
      </MyFieldShell>
      <MyFieldShell label="用户名（可选）">
        <MyInput v-model="cfg.username" :disabled="!cfg.enabled" />
      </MyFieldShell>
    </div>

    <div class="px-pw">
      <MyFieldShell label="密码（可选，存 keyring；留空不修改）">
        <MyInput
          v-model="pw"
          type="password"
          show-password
          :placeholder="cfg.hasPassword ? '••••（已设置，留空保留）' : ''"
          :disabled="!cfg.enabled"
        />
      </MyFieldShell>
    </div>

    <div class="px-actions">
      <MyButton variant="primary" :loading="busy" @click="save">保存</MyButton>
      <MyButton variant="secondary" :loading="busy" @click="doTest">测试连通性</MyButton>
    </div>

    <div v-if="test" class="px-result" :class="test.ok ? 'ok' : 'bad'">
      {{ test.ok ? `✓ 连通（${test.latencyMs}ms，status ${test.status}）` : `✗ ${test.error || '失败'}` }}
    </div>
  </MyPanel>
</template>

<style scoped>
.px-toggle {
  display: flex;
  align-items: center;
  gap: 8px;
  color: var(--text-secondary);
  font-size: 12px;
}
.px-desc {
  margin: 0 0 14px;
  color: var(--text-secondary);
  font-size: 12px;
}
.px-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
  gap: 14px;
  margin-bottom: 14px;
}
.px-actions {
  display: flex;
  gap: 8px;
}
.px-result {
  margin-top: 10px;
  font-family: var(--font-mono);
  font-size: 12px;
}
.px-result.ok {
  color: var(--success);
}
.px-result.bad {
  color: var(--danger);
}
</style>
