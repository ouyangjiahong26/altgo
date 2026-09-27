import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { ThemeProvider } from "./ThemeContext";
import { applyFontSize, applyWindowSize } from "./ui-size";

applyFontSize();
void applyWindowSize();
import "./styles/global.css";
import "./styles/layout.css";
import "./styles/components/ui-primitives.css";
import "./styles/components/status-indicator.css";
import "./styles/pages/home.css";
import "./styles/pages/settings.css";
import "./styles/pages/history.css";
import "./styles/pages/onboarding.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ThemeProvider>
      <App />
    </ThemeProvider>
  </React.StrictMode>,
);
