import { useTranslation } from "../i18n";
import { NavLink } from "react-router-dom";
import { Mic, Settings, Minus, Maximize2, Minimize2, X } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { useState, useEffect } from "react";

interface LayoutProps {
  children: React.ReactNode;
}

export default function Layout({ children }: LayoutProps) {
  const { t } = useTranslation();
  const [isMaximized, setIsMaximized] = useState(false);
  const [hasNewUpdate, setHasNewUpdate] = useState(false);

  useEffect(() => {
    // 启动时静默检查更新（受配置控制；若网络失败静默忽略）
    const performSilentUpdateCheck = async () => {
      try {
        const cfg = await invoke<{ autoCheckUpdate: boolean }>("get_config");
        if (cfg.autoCheckUpdate !== false) {
          const res = await invoke<{ hasUpdate: boolean }>("check_update", { mode: "silent" });
          if (res && res.hasUpdate) {
            setHasNewUpdate(true);
          }
        }
      } catch {
        // 静默检查失败不打扰用户
      }
    };

    performSilentUpdateCheck();
  }, []);

  useEffect(() => {
    const win = getCurrentWindow();
    let unlisten: (() => void) | null = null;

    const checkMaximized = async () => {
      try {
        const maximized = await win.isMaximized();
        setIsMaximized(maximized);
      } catch {
        // 忽略
      }
    };

    checkMaximized();

    win.onResized(() => {
      checkMaximized();
    }).then((fn) => {
      unlisten = fn;
    });

    return () => {
      if (unlisten) unlisten();
    };
  }, []);

  useEffect(() => {
    const root = document.getElementById("root");
    if (!root) return;
    root.classList.toggle("maximized", isMaximized);
  }, [isMaximized]);

  const handleMinimize = async () => {
    try {
      await getCurrentWindow().minimize();
    } catch {
      // 忽略
    }
  };

  const handleToggleMaximize = async () => {
    try {
      await getCurrentWindow().toggleMaximize();
    } catch {
      // 忽略
    }
  };

  const handleClose = async () => {
    try {
      await getCurrentWindow().hide();
    } catch {
      // 忽略
    }
  };

  const handleHeaderMouseDown = (e: React.MouseEvent) => {
    const target = e.target as HTMLElement;
    if (
      target.closest("a") ||
      target.closest("button") ||
      target.closest("nav") ||
      target.closest(".window-controls")
    ) {
      return;
    }
    try {
      getCurrentWindow().startDragging();
    } catch {
      // 忽略
    }
  };

  return (
    <div className="layout">
      <header className="layout-header" data-tauri-drag-region onMouseDown={handleHeaderMouseDown}>
        <div className="layout-header-left">
          <div className="layout-logo-wrapper">
            <img
              src="/altgo-logo.svg"
              alt=""
              width={28}
              height={28}
              className="layout-logo-mark"
            />
            <span className="layout-logo">altgo</span>
          </div>
          <span className="layout-subtitle">{t("title.subtitle")}</span>
        </div>
        <div className="layout-header-right" data-tauri-drag-region="false">
          <nav className="layout-nav">
            <NavLink
              to="/"
              end
              className={({ isActive }) =>
                `layout-nav-link ${isActive ? "active" : ""}`
              }
            >
              <Mic size={16} />
              {t("nav.home")}
            </NavLink>
            <NavLink
              to="/settings"
              className={({ isActive }) =>
                `layout-nav-link ${isActive ? "active" : ""}`
              }
              style={{ position: "relative" }}
            >
              <Settings size={16} />
              {t("nav.settings")}
              {hasNewUpdate && (
                <span
                  style={{
                    position: "absolute",
                    top: "4px",
                    right: "4px",
                    width: "7px",
                    height: "7px",
                    borderRadius: "50%",
                    backgroundColor: "#ef4444",
                  }}
                  title={t("settings.update_available")}
                />
              )}
            </NavLink>
          </nav>
          <div className="window-controls">
            <button
              className="window-control-btn"
              onClick={handleMinimize}
              aria-label="Minimize"
            >
              <Minus size={14} />
            </button>
            <button
              className="window-control-btn"
              onClick={handleToggleMaximize}
              aria-label={isMaximized ? "Restore" : "Maximize"}
            >
              {isMaximized ? <Minimize2 size={14} /> : <Maximize2 size={14} />}
            </button>
            <button
              className="window-control-btn window-control-close"
              onClick={handleClose}
              aria-label="Close"
            >
              <X size={14} />
            </button>
          </div>
        </div>
      </header>
      <main className="layout-main">{children}</main>
    </div>
  );
}
