/**
 * Token Plan 供应商识别（与后端 coding_plan::detect 口径一致，对齐 cc-switch
 * codingPlanProviders：按 baseURL 子串匹配；命中即自动用该供应商 API Key + Base URL 查额度）
 */
export function detectCodingPlan(base: string): {
  id: 'kimi' | 'zhipu' | 'minimax' | 'zenmux' | 'volcengine'
  label: string
} | null {
  const u = (base || '').toLowerCase()
  if (u.includes('api.kimi.com/coding')) return { id: 'kimi', label: 'Kimi For Coding' }
  if (u.includes('bigmodel.cn') || u.includes('api.z.ai')) return { id: 'zhipu', label: 'Zhipu GLM(智谱)' }
  if (u.includes('api.minimaxi.com') || u.includes('api.minimax.io')) return { id: 'minimax', label: 'MiniMax' }
  if (u.includes('zenmux')) return { id: 'zenmux', label: 'ZenMux' }
  if (u.includes('volces.com/api/coding')) return { id: 'volcengine', label: '火山方舟(Volcengine)' }
  return null
}

/** 模型上下文缺省值 */
export const DEFAULT_CONTEXT = 200_000

/** 供应商/模型变更后的统一黄色提醒 */
export const RESTART_HINT = '供应商/模型修改需要重启zcode才可以生效哟!'
