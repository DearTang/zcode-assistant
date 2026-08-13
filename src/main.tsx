import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import App from "./App";
import FloatBall from "./windows/FloatBall";
import FloatPanel from "./windows/FloatPanel";
import { ThemeProvider } from "./hooks/useTheme";
import "./styles/tokens.css";
import "./styles/global.css";
import "./styles/components.css";
import "./styles/layout.css";
import "./styles/float.css";

const label = getCurrentWindow().label;

// 移除启动 loader
const loader = document.getElementById("boot-loader");
if (loader) loader.remove();

const root = ReactDOM.createRoot(
  document.getElementById("root") as HTMLElement
);

if (label === "float-ball") {
  document.documentElement.classList.add("za-floatball");
  root.render(
    <ThemeProvider>
      <FloatBall />
    </ThemeProvider>
  );
} else if (label === "float-panel") {
  document.documentElement.classList.add("za-floatpanel");
  root.render(
    <ThemeProvider>
      <FloatPanel />
    </ThemeProvider>
  );
} else {
  root.render(
    <React.StrictMode>
      <ThemeProvider>
        <App />
      </ThemeProvider>
    </React.StrictMode>
  );
}
