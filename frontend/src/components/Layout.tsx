import { useTranslation } from "../i18n";
import { NavLink } from "react-router-dom";
import { House, Settings, Minus, Maximize2, Minimize2, X } from "lucide-react";
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
    // 启动时静默检查更新（受配置控制，若网络失败静默忽略）
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
    if (target.closest("a") || target.closest("button") || target.closest(".window-controls")) {
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
              width={18}
              height={18}
              className="layout-logo-mark"
            />
            <span className="layout-logo">altgo</span>
          </div>
          <span className="layout-subtitle">{t("title.subtitle")}</span>
        </div>
        <div className="layout-header-right" data-tauri-drag-region="false">
          <div className="window-controls">
            <button
              className="window-control-btn"
              onClick={handleMinimize}
              aria-label={t("window.minimize")}
            >
              <Minus size={14} />
            </button>
            <button
              className="window-control-btn"
              onClick={handleToggleMaximize}
              aria-label={t(isMaximized ? "window.restore" : "window.maximize")}
            >
              {isMaximized ? <Minimize2 size={14} /> : <Maximize2 size={14} />}
            </button>
            <button
              className="window-control-btn window-control-close"
              onClick={handleClose}
              aria-label={t("window.close")}
            >
              <X size={14} />
            </button>
          </div>
        </div>
      </header>
      <main className="layout-main">{children}</main>
      <div className="layout-dock-wrap">
        <nav className="layout-dock" aria-label={t("nav.label")}>
          <NavLink
            to="/"
            end
            className={({ isActive }) => `dock-item${isActive ? " active" : ""}`}
          >
            <House />
            <span>{t("nav.home")}</span>
          </NavLink>
          <NavLink
            to="/settings"
            className={({ isActive }) => `dock-item${isActive ? " active" : ""}`}
          >
            <Settings />
            <span>{t("nav.settings")}</span>
            {hasNewUpdate && (
              <span
                className="dock-item-dot"
                title={t("settings.update_available")}
                aria-hidden="true"
              />
            )}
          </NavLink>
        </nav>
      </div>
    </div>
  );
}
