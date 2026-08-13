import { useEffect } from "react";
import type { CSSProperties } from "react";

interface Props {
  version: string;
  onAgree: () => void;
  onDecline: () => void;
}

/**
 * 一次性匿名使用统计同意弹窗（参考 myshell）。每个新版本首次启动时显示（若用户
 * 此前未同意）。说明采集什么（仅版本 + 系统 + 随机设备 ID）、不采集什么，以及
 * 同意模型（同意一次 = 后续版本自动；拒绝 = 下版本再问）。
 */
export function StatsConsentDialog({ version, onAgree, onDecline }: Props) {
  // ESC = 拒绝（即不追踪的选项）。
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onDecline();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onDecline]);

  return (
    <div className="rd-overlay" onClick={onDecline}>
      <div
        className="za-glass-strong"
        style={panel}
        onClick={(e) => e.stopPropagation()}
      >
        <div style={{ fontSize: 26, marginBottom: 6 }}>📊</div>
        <strong style={{ fontSize: "var(--fs-md)" }}>
          帮助 zcode-assistant 变得更好
        </strong>
        <div
          className="za-muted"
          style={{ fontSize: "var(--fs-sm)", lineHeight: 1.7, marginTop: 8 }}
        >
          检测到你升级到了 v{version}。是否允许发送一次
          <strong style={{ color: "var(--text-primary)" }}> 完全匿名</strong>
          的统计数据，帮助我们了解有多少用户在使用？
        </div>

        <div className="za-glass-strong" style={box}>
          <div style={{ fontWeight: 600, color: "var(--text-secondary)", marginBottom: 6 }}>
            收集的内容（仅此而已）：
          </div>
          ✓ 应用版本号（v{version}）
          <br />
          ✓ 操作系统（如 Windows）
          <br />
          ✓ 一个随机设备 ID（用于去重计数，不绑定任何个人信息）
          <div
            style={{
              fontWeight: 600,
              color: "var(--text-secondary)",
              marginTop: 10,
              marginBottom: 4,
            }}
          >
            绝不收集：
          </div>
          ✗ 服务器地址 / 用户名 / 密码 / API Key / 连接内容
        </div>

        <div
          className="za-faint"
          style={{ fontSize: "var(--fs-xs)", lineHeight: 1.5, marginTop: 4 }}
        >
          每次升级到新版本都会询问一次。同意将发送本次匿名统计；选择暂不则本次不发送。
        </div>

        <div className="za-row" style={{ justifyContent: "flex-end", gap: 8, marginTop: 8 }}>
          <button className="za-btn za-btn-sm" onClick={onDecline}>
            暂不
          </button>
          <button
            className="za-btn za-btn-sm za-btn-primary"
            onClick={onAgree}
            autoFocus
          >
            允许匿名统计
          </button>
        </div>
      </div>
    </div>
  );
}

const panel: CSSProperties = {
  width: 440,
  maxWidth: "92vw",
  padding: 22,
  display: "flex",
  flexDirection: "column",
  gap: 4,
};

const box: CSSProperties = {
  background: "var(--bg-overlay)",
  border: "1px solid var(--border-base)",
  borderRadius: "var(--radius-md)",
  padding: "12px 14px",
  marginTop: 14,
  fontSize: "var(--fs-xs)",
  color: "var(--text-tertiary)",
  lineHeight: 1.7,
};
