import type { ReactNode } from "react";

/** 占位视图：图标 + 标题 + 徽标 + 说明 + 计划要点 */
export function Placeholder({
  icon,
  title,
  badge,
  desc,
  plan,
}: {
  icon: ReactNode;
  title: string;
  badge: string;
  desc: string;
  plan: string[];
}) {
  return (
    <div className="za-panel za-card-pad">
      <div className="za-section-title">
        <div className="za-row" style={{ gap: "var(--space-3)" }}>
          <span style={{ color: "var(--accent)" }}>{icon}</span>
          <h3>{title}</h3>
        </div>
        <span className="za-badge za-badge-neutral">{badge}</span>
      </div>
      <p className="za-muted" style={{ marginTop: 0 }}>
        {desc}
      </p>
      <ul
        style={{
          margin: 0,
          paddingLeft: "var(--space-5)",
          color: "var(--text-secondary)",
          fontSize: "var(--fs-sm)",
          lineHeight: 1.8,
        }}
      >
        {plan.map((t) => (
          <li key={t}>{t}</li>
        ))}
      </ul>
    </div>
  );
}
