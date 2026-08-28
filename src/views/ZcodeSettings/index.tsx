import { useEffect, useState } from "react";
import { zcode } from "../../api";
import { RestartBar } from "../../components/RestartBar";
import { toast } from "../../components/Toast";
import type { ModelRetryConfig } from "../../types";

/** ZCode 内置默认值（zcode.cjs resolveAiSdkModelRetryOptions），输入框留空 = 跟随默认 */
const ZC_DEFAULTS = {
  maxRetries: 10,
  baseDelayMs: 2000,
  backoffFactor: 2,
  maxDelayMs: 60000,
};

/** 可编辑的字段定义：key ↔ ModelRetryConfig 字段 */
const FIELDS = [
  {
    key: "maxRetries" as const,
    label: "最大重试次数",
    def: ZC_DEFAULTS.maxRetries,
    unit: "次",
    step: 1,
    hint: "接口失败后的重试上限（0 = 失败不重试，直接报错）",
  },
  {
    key: "baseDelayMs" as const,
    label: "起步延迟",
    def: ZC_DEFAULTS.baseDelayMs,
    unit: "ms",
    step: 100,
    hint: "第一次重试前等待的时间",
  },
  {
    key: "backoffFactor" as const,
    label: "退避倍数",
    def: ZC_DEFAULTS.backoffFactor,
    unit: "×",
    step: 0.5,
    hint: "每次重试后延迟翻倍的倍率",
  },
  {
    key: "maxDelayMs" as const,
    label: "延迟上限",
    def: ZC_DEFAULTS.maxDelayMs,
    unit: "ms",
    step: 1000,
    hint: "单次重试等待的封顶时间",
  },
];

/** 由当前编辑值生成延迟序列预览（与 zcode.cjs 同公式：base × factor^n，封顶 max） */
function delayPreview(v: {
  maxRetries: string;
  baseDelayMs: string;
  backoffFactor: string;
  maxDelayMs: string;
}): string {
  const retries = Math.max(
    1,
    Math.min(10, Math.round(Number(v.maxRetries) || ZC_DEFAULTS.maxRetries))
  );
  const base = Number(v.baseDelayMs) || ZC_DEFAULTS.baseDelayMs;
  const factor = Number(v.backoffFactor) || ZC_DEFAULTS.backoffFactor;
  const cap = Number(v.maxDelayMs) || ZC_DEFAULTS.maxDelayMs;
  const seq: string[] = [];
  let d = base;
  for (let i = 0; i < Math.min(retries, 4); i++) {
    seq.push(d >= 1000 ? `${(d / 1000).toFixed(1)}s` : `${Math.round(d)}ms`);
    d = Math.min(d * factor, cap);
  }
  if (retries > 4) seq.push("…");
  return seq.join(" → ");
}

/**
 * ZCode 设置：调参 ZCode 本体行为的入口（区别于本应用自身偏好的「设置」页）。
 * 当前能力：模型调用重试 —— ZCode 官方 ZCODE_MODEL_RETRY_* 用户环境变量，
 * 全局生效于所有模型；写入后广播环境变更，ZCode 运行中会弹全局重启确认。
 */
export default function ZcodeSettings() {
  const [saved, setSaved] = useState<ModelRetryConfig | null>(null);
  // 编辑态用字符串承载（允许留空 = 跟随 ZCode 默认）
  const [edit, setEdit] = useState({
    maxRetries: "",
    baseDelayMs: "",
    backoffFactor: "",
    maxDelayMs: "",
  });
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [dirty, setDirty] = useState(false);

  useEffect(() => {
    zcode
      .getRetryConfig()
      .then((cfg) => {
        setSaved(cfg);
        setEdit({
          maxRetries: cfg.maxRetries?.toString() ?? "",
          baseDelayMs: cfg.baseDelayMs?.toString() ?? "",
          backoffFactor: cfg.backoffFactor?.toString() ?? "",
          maxDelayMs: cfg.maxDelayMs?.toString() ?? "",
        });
      })
      .catch((e) => toast.error(`读取重试配置失败：${String(e)}`))
      .finally(() => setLoading(false));
  }, []);

  const setField = (key: keyof typeof edit, v: string) => {
    setEdit((e) => ({ ...e, [key]: v }));
    setDirty(true);
  };

  /** 编辑态 → 配置对象；非法输入弹错返回 null */
  const buildConfig = (): ModelRetryConfig | null => {
    const parse = (key: keyof typeof edit, label: string): number | null => {
      const raw = edit[key].trim();
      if (!raw) return null;
      const n = Number(raw);
      if (!Number.isFinite(n) || n < 0) {
        toast.error(`「${label}」不是有效的非负数字`);
        throw new Error(label);
      }
      return n;
    };
    try {
      return {
        maxRetries: parse("maxRetries", "最大重试次数"),
        baseDelayMs: parse("baseDelayMs", "起步延迟"),
        backoffFactor: parse("backoffFactor", "退避倍数"),
        maxDelayMs: parse("maxDelayMs", "延迟上限"),
      };
    } catch {
      return null;
    }
  };

  const save = async () => {
    const cfg = buildConfig();
    if (!cfg) return;
    setSaving(true);
    try {
      const applied = await zcode.setRetryConfig(cfg);
      setSaved(applied);
      setDirty(false);
      toast.success("重试配置已写入用户环境变量");
    } catch (e: unknown) {
      toast.error(typeof e === "string" ? e : `保存失败：${String(e)}`);
    } finally {
      setSaving(false);
    }
  };

  const resetDefaults = async () => {
    setEdit({ maxRetries: "", baseDelayMs: "", backoffFactor: "", maxDelayMs: "" });
    setSaving(true);
    try {
      const applied = await zcode.setRetryConfig({
        maxRetries: null,
        baseDelayMs: null,
        backoffFactor: null,
        maxDelayMs: null,
      });
      setSaved(applied);
      setDirty(false);
      toast.success("已恢复 ZCode 默认重试策略（清除全部覆盖值）");
    } catch (e: unknown) {
      toast.error(typeof e === "string" ? e : `恢复失败：${String(e)}`);
    } finally {
      setSaving(false);
    }
  };

  return (
    <>
      <div className="za-panel za-card-pad">
        <div className="za-section-title">
          <h3>模型调用重试</h3>
          <span className="za-faint" style={{ fontSize: "var(--fs-xs)" }}>
            ZCODE_MODEL_RETRY_* · 全局生效
          </span>
        </div>
        <p className="za-muted" style={{ margin: "0 0 14px", fontSize: "var(--fs-sm)" }}>
          ZCode 调用模型接口失败时默认最多重试 {ZC_DEFAULTS.maxRetries} 次（起步{" "}
          {ZC_DEFAULTS.baseDelayMs / 1000}s、×{ZC_DEFAULTS.backoffFactor} 指数退避、
          单次封顶 {ZC_DEFAULTS.maxDelayMs / 1000}s）。此处通过官方环境变量调整，
          对所有模型 / 供应商统一生效；可重试的错误类型（限流、服务端错误、网络中断、
          流超时）由 ZCode 内部判定，4xx 等不可重试错误不会消耗次数。
        </p>

        {loading ? (
          <div className="za-empty">加载中…</div>
        ) : (
          <>
            <div
              style={{
                display: "grid",
                gridTemplateColumns: "repeat(auto-fit, minmax(200px, 1fr))",
                gap: "var(--space-4)",
              }}
            >
              {FIELDS.map((f) => (
                <div key={f.key} style={{ display: "flex", flexDirection: "column", gap: 4 }}>
                  <label
                    style={{
                      fontSize: "var(--fs-sm)",
                      color: "var(--text-secondary)",
                    }}
                  >
                    {f.label}
                    <span className="za-faint" style={{ marginLeft: 6, fontSize: "var(--fs-xs)" }}>
                      默认 {f.def}
                      {f.unit}
                    </span>
                  </label>
                  <div className="za-row" style={{ gap: 6, alignItems: "center" }}>
                    <input
                      className="za-input za-mono"
                      type="number"
                      min={0}
                      step={f.step}
                      value={edit[f.key]}
                      placeholder={`默认 ${f.def}`}
                      onChange={(e) => setField(f.key, e.target.value)}
                      style={{ width: 110 }}
                    />
                    <span className="za-faint" style={{ fontSize: "var(--fs-xs)" }}>
                      {f.unit}
                    </span>
                  </div>
                  <span className="za-faint" style={{ fontSize: "var(--fs-xs)" }}>
                    {f.hint}
                  </span>
                </div>
              ))}
            </div>

            <div
              className="za-row-between"
              style={{ marginTop: 14, gap: 10, flexWrap: "wrap" }}
            >
              <div className="za-faint" style={{ fontSize: "var(--fs-xs)" }}>
                重试延迟示例：{delayPreview(edit)}
                <span style={{ marginLeft: 6 }}>
                  （留空 = 跟随 ZCode 默认，未设置的项不写入环境变量）
                </span>
              </div>
              <div className="za-row" style={{ gap: 8 }}>
                <button className="za-btn za-btn-sm" onClick={resetDefaults} disabled={saving}>
                  恢复 ZCode 默认
                </button>
                <button
                  className="za-btn za-btn-sm za-btn-primary"
                  onClick={save}
                  disabled={saving || !dirty}
                >
                  {saving ? "写入中…" : dirty ? "保存并生效" : "已与当前配置一致"}
                </button>
              </div>
            </div>
          </>
        )}
      </div>

      {saved && !dirty && (
        <div style={{ marginTop: 12 }}>
          <RestartBar hint="重试配置写入用户环境变量，重启 ZCode 后对新会话生效" />
        </div>
      )}
    </>
  );
}
