<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { confirmDialog, MyButton, MyIcon, MyInlineEdit, MyInput, MyPanel, MyTag } from 'myui'
import RestartBar from '@/components/RestartBar.vue'
import { accounts as acc, events, scheduleZcodeReload } from '@/api'
import { toast } from '@/composables/toast'
import type { AccountMeta } from '@/types'

defineOptions({ name: 'AccountsView' })

const list = ref<AccountMeta[]>([])
const current = ref<AccountMeta | null>(null)
const label = ref('')
const busy = ref(false)
const deletingId = ref<string | null>(null)

async function reload(): Promise<void> {
  try {
    list.value = await acc.list()
    current.value = await acc.current()
  } catch (e: unknown) {
    toast.error(String(e))
  }
}

onMounted(reload)

async function capture(): Promise<void> {
  busy.value = true
  try {
    const cur = await acc.current()
    if (cur) {
      toast.success(`该账号已存在（${cur.label}），无需重复捕获`)
      return
    }
    await acc.capture(label.value || '账号')
    label.value = ''
    await reload()
    toast.success('已捕获当前账号快照')
  } catch (e: unknown) {
    toast.error(String(e))
  } finally {
    busy.value = false
  }
}

async function use(id: string): Promise<void> {
  const ok = await confirmDialog({
    message: '切换账号会关闭并重启 zcode，继续？',
    type: 'warning',
    confirmButtonText: '切换',
    cancelButtonText: '取消',
  })
  if (!ok) return
  busy.value = true
  try {
    await acc.use(id)
    await reload()
    // 通知 Dashboard 立即刷新「当前账号 / 套餐」（不必等下一次 5s 轮询）
    events.emitRefreshRequested()
    // 防抖触发 ZCode 重载窗口，让新账号登录态被 ZCode 重新读取
    scheduleZcodeReload()
    toast.success('已切换')
  } catch (e: unknown) {
    toast.error(String(e))
  } finally {
    busy.value = false
  }
}

async function doDelete(id: string): Promise<void> {
  try {
    await acc.remove(id)
    deletingId.value = null
    await reload()
    toast.success('已删除账号')
  } catch (e: unknown) {
    toast.error(String(e))
  }
}

async function commitEdit(id: string, v: string): Promise<void> {
  if (!v) return
  try {
    await acc.rename(id, v)
    await reload()
    toast.success('已更新别名')
  } catch (e: unknown) {
    toast.error(String(e))
  }
}
</script>

<template>
  <div class="ac">
    <RestartBar hint="账号 / 配置变更后需重启 zcode 生效" />

    <MyPanel title="智谱账号">
      <template #actions>
        <div class="ac-capture">
          <MyInput v-model="label" placeholder="账号备注名" class="ac-label-input" />
          <MyButton variant="primary" size="small" :loading="busy" @click="capture">
            <MyIcon name="Plus" :size="13" /> 捕获当前
          </MyButton>
        </div>
      </template>

      <p class="ac-desc">
        捕获当前 zcode 登录态为快照（机器绑定，不可跨机器），可在多个智谱账号间一键切换。切换会
        kill 并重启 zcode。
      </p>

      <div v-if="list.length === 0" class="ui-empty">暂无账号，点击「捕获当前」添加</div>
      <div v-else class="ac-list">
        <div v-for="a in list" :key="a.id" class="ac-row">
          <div class="ac-main">
            <MyIcon v-if="current?.id === a.id" name="Check" :size="14" class="ac-cur" />
            <div class="ac-copy">
              <div class="ac-label">
                <!-- myui v0.9.0 MyInlineEdit：点击名称即进入编辑，Enter 提交 / Esc 取消 -->
                <MyInlineEdit
                  :model-value="a.label"
                  placeholder="输入账号别名"
                  @confirm="(v) => commitEdit(a.id, v)"
                />
                <MyTag v-if="current?.id === a.id" type="primary" size="small" round>当前</MyTag>
              </div>
              <div class="ac-sub">{{ a.email || a.shortId || a.userId }} · {{ new Date(a.capturedAt).toLocaleString() }}</div>
            </div>
          </div>

          <div class="ac-ops">
            <MyButton
              v-if="current?.id !== a.id && deletingId !== a.id"
              size="small"
              :disabled="busy"
              @click="use(a.id)"
            >
              切换
            </MyButton>
            <template v-if="deletingId === a.id">
              <MyButton variant="danger" size="small" @click="doDelete(a.id)">确认删除</MyButton>
              <button class="ac-icon-btn" type="button" title="取消" @click="deletingId = null">
                <MyIcon name="Close" :size="13" />
              </button>
            </template>
            <button v-else class="ac-icon-btn" type="button" title="删除" @click="deletingId = a.id">
              <MyIcon name="Trash" :size="13" />
            </button>
          </div>
        </div>
      </div>
    </MyPanel>
  </div>
</template>

<style scoped>
.ac {
  display: grid;
  gap: var(--ui-gap);
}
.ac-capture {
  display: flex;
  align-items: center;
  gap: 8px;
}
.ac-label-input {
  width: 140px;
}
.ac-desc {
  margin: 0 0 12px;
  color: var(--text-secondary);
  font-size: 12px;
  line-height: 1.7;
}
.ac-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.ac-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  padding: 8px 12px;
  border-radius: 8px;
  border: 1px solid var(--border-subtle);
}
.ac-main {
  display: flex;
  align-items: center;
  gap: 8px;
  flex: 1;
  min-width: 0;
}
.ac-cur {
  color: var(--accent);
}
.ac-copy {
  min-width: 0;
}
.ac-label {
  display: flex;
  align-items: center;
  gap: 8px;
  font-weight: 500;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.ac-sub {
  color: var(--text-tertiary);
  font-family: var(--font-mono);
  font-size: 11px;
}
.ac-ops {
  display: flex;
  align-items: center;
  gap: 6px;
}
.ac-icon-btn {
  width: 26px;
  height: 26px;
  display: inline-grid;
  place-items: center;
  border: 1px solid transparent;
  border-radius: 7px;
  background: transparent;
  color: var(--text-secondary);
  cursor: pointer;
}
.ac-icon-btn:hover {
  color: var(--text-primary);
  background: var(--surface-translucent-hover);
}
</style>
