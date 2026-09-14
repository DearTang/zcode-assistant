import { toast as myToast } from 'myui'

/**
 * 全局轻提示适配层：沿用本项目 `toast.success/error/warning` 的调用习惯，
 * 底层走 myui toast()（ElMessage 玻璃观感）。
 * 旧版 React Toast 的「复制消息 / hover 暂停 / 队列上限」为 myui 缺口，已记入反馈清单。
 */
export const toast = {
  success: (msg: string) => myToast(msg, { type: 'success' }),
  error: (msg: string) => myToast(msg, { type: 'error' }),
  warning: (msg: string) => myToast(msg, { type: 'warning' }),
  info: (msg: string) => myToast(msg, { type: 'info' }),
}
