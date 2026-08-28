import { useEffect, useState } from "react";
import type { ComponentType, SVGProps } from "react";
import type { ViewId } from "../types";
import { useTheme } from "../hooks/useTheme";
import {
  IconDashboard,
  IconCpu,
  IconSwap,
  IconChart,
  IconFolder,
  IconUser,
  IconGlobe,
  IconSparkle,
  IconSliders,
  IconSettings,
  IconSun,
  IconMoon,
  IconPower,
} from "./icons";
import { win, app } from "../api";

interface NavEntry {
  id: ViewId;
  label: string;
  icon: ComponentType<SVGProps<SVGSVGElement>>;
}

const NAV: NavEntry[] = [
  { id: "dashboard", label: "总览", icon: IconDashboard },
  { id: "models", label: "模型管理", icon: IconCpu },
  { id: "autoswitch", label: "自动切换", icon: IconSwap },
  { id: "usage", label: "用量查询", icon: IconChart },
  { id: "projects", label: "项目管理", icon: IconFolder },
  { id: "accounts", label: "智谱账号", icon: IconUser },
  { id: "proxy", label: "网络代理", icon: IconGlobe },
  { id: "beautify", label: "ZCode 美化", icon: IconSparkle },
  { id: "zcode-settings", label: "ZCode 设置", icon: IconSliders },
  { id: "settings", label: "设置", icon: IconSettings },
];

interface SidebarProps {
  current: ViewId;
  onSelect: (id: ViewId) => void;
  /** 检测到新版本时版本号旁显示小圆点 */
  updateAvailable?: boolean;
  /** 点击版本号打开「关于」弹窗（检查更新 / 更新日志） */
  onOpenAbout?: () => void;
}

export function Sidebar({
  current,
  onSelect,
  updateAvailable,
  onOpenAbout,
}: SidebarProps) {
  const { theme, toggle } = useTheme();
  const [version, setVersion] = useState("");

  useEffect(() => {
    app.getVersion().then(setVersion).catch(() => {});
  }, []);

  return (
    <aside className="za-sidebar">
      <div className="za-logo">
        <div className="za-logo-mark">
          <svg
            width="18"
            height="18"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2.5"
            strokeLinecap="round"
            strokeLinejoin="round"
          >
            <path d="M7 7 L17 7 L7 17 L17 17" />
          </svg>
        </div>
        <div>
          <div className="za-logo-text">zcode-assistant</div>
          <div className="za-logo-sub">增强工具</div>
        </div>
      </div>

      <nav className="za-nav">
        {NAV.map((e) => {
          const Icon = e.icon;
          return (
            <button
              key={e.id}
              className="za-nav-item"
              data-active={current === e.id}
              onClick={() => onSelect(e.id)}
            >
              <Icon width={18} height={18} />
              {e.label}
            </button>
          );
        })}
      </nav>

      <div className="za-sidebar-foot">
        <button className="za-nav-item" onClick={toggle}>
          {theme === "dark" ? (
            <IconSun width={18} height={18} />
          ) : (
            <IconMoon width={18} height={18} />
          )}
          {theme === "dark" ? "浅色模式" : "深色模式"}
        </button>
        <button className="za-nav-item" onClick={() => win.quitApp()}>
          <IconPower width={18} height={18} />
          退出
        </button>
        {version && (
          <button
            className="za-version"
            title={updateAvailable ? "发现新版本，点击查看" : "关于 zcode-assistant"}
            onClick={() => onOpenAbout?.()}
          >
            <span className="za-mono">v{version}</span>
            {updateAvailable && <span className="za-version-dot" />}
          </button>
        )}
      </div>
    </aside>
  );
}
