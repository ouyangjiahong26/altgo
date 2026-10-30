import { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { applyThemeToDocument, installThemeListeners } from "./theme";
import UpdateNotesView from "./components/UpdateNotesView";
import {
  UPDATE_INSTALL_PROGRESS_EVENT,
  UPDATE_NOTES_EVENT,
  type UpdateInfo,
  type UpdateInstallProgress,
  type UpdateInstallState,
} from "./updateNotes";
import "./styles/global.css";
// 底部操作按钮直接用跨页复用的 .btn 基底，避免本窗再维护一套按钮皮肤。
import "./styles/components/ui-primitives.css";
import "./styles/pages/update-notes.css";

/**
 * 更新说明窗口（label `update-notes`）的入口：副作用都在这里，标记在
 * `components/UpdateNotesView.tsx`。
 *
 * 与悬浮窗同构：页面随应用启动常驻加载、不可见，主窗发 `update-notes-data`
 * 事件后由设置页调用 show()。关闭走 hide()，保住 webview 与事件监听，重开
 * 零成本。数据全部由事件驱动，本窗口不主动查询。
 */

/** hide 而非 close：关闭后 webview 与监听器保留，再次打开无需重新加载。 */
async function hideWindow(): Promise<void> {
  try {
    await getCurrentWindow().hide();
  } catch {
    // 窗口已销毁（应用退出中）时忽略。
  }
}

function UpdateNotes() {
  const [info, setInfo] = useState<UpdateInfo | null>(null);
  const [installState, setInstallState] = useState<UpdateInstallState>("idle");
  const [progress, setProgress] = useState<UpdateInstallProgress | null>(null);
  const [installError, setInstallError] = useState<string | null>(null);

  // 主题跟随主窗：storage 事件跨同源窗口同步（与 overlay 同款）。
  useEffect(() => {
    applyThemeToDocument();
    return installThemeListeners(() => applyThemeToDocument());
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | null = null;
    listen<UpdateInfo>(UPDATE_NOTES_EVENT, (event) => {
      setInfo(event.payload);
      setInstallState("idle");
      setProgress(null);
      setInstallError(null);
    })
      .then((fn) => {
        unlisten = fn;
      })
      .catch(() => {
        // 非 Tauri 环境（浏览器直开页面）下没有事件通道，走空态。
      });
    return () => {
      if (unlisten) unlisten();
    };
  }, []);

  // deb 直装的下载与授权进度由主进程推送；就地更新档不发这个事件。
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    listen<UpdateInstallProgress>(UPDATE_INSTALL_PROGRESS_EVENT, (event) => {
      setProgress(event.payload);
      if (event.payload.phase === "installing") setInstallState("installing");
    })
      .then((fn) => {
        unlisten = fn;
      })
      .catch(() => {
        // 非 Tauri 环境没有事件通道。
      });
    return () => {
      if (unlisten) unlisten();
    };
  }, []);

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") void hideWindow();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  // 系统关闭（Alt+F4、窗口管理器的关闭）同样只隐藏：Tauri 仅在窗口存在
  // close-requested 监听器时才阻止关闭，否则 webview 会被销毁，此后“检查更新”
  // 与“查看更新说明”都会静默失效。
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    getCurrentWindow()
      .onCloseRequested((event) => {
        event.preventDefault();
        void hideWindow();
      })
      .then((fn) => {
        unlisten = fn;
      })
      .catch(() => {
        // 非 Tauri 环境（浏览器直开页面）下没有窗口事件通道。
      });
    return () => {
      if (unlisten) unlisten();
    };
  }, []);

  const handleInstall = async () => {
    setInstallState("downloading");
    setInstallError(null);
    try {
      await invoke("install_update");
      // 就地更新档安装成功后应用会自行重启，正常走不到这里。
      setInstallState("installed");
    } catch (err) {
      setInstallError(String(err));
      setInstallState("idle");
    }
  };

  const handleRestart = async () => {
    try {
      await invoke("restart_app");
    } catch (err) {
      setInstallError(String(err));
    }
  };

  return (
    <UpdateNotesView
      info={info}
      installState={installState}
      progress={progress}
      installError={installError}
      onInstall={handleInstall}
      onRestart={handleRestart}
      onClose={() => void hideWindow()}
    />
  );
}

createRoot(document.getElementById("root")!).render(<UpdateNotes />);
