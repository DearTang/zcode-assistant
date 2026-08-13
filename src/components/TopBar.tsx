import type { ReactNode } from "react";

interface TopBarProps {
  title: string;
  subtitle?: string;
  actions?: ReactNode;
}

export function TopBar({ title, subtitle, actions }: TopBarProps) {
  return (
    <header className="za-topbar">
      <div className="za-topbar-titles">
        <h1>{title}</h1>
        {subtitle && <span className="za-muted">{subtitle}</span>}
      </div>
      {actions && <div className="za-topbar-actions">{actions}</div>}
    </header>
  );
}
