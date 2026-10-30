import { emit } from "@tauri-apps/api/event";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";

/**
 * 更新说明窗口的跨窗口契约：label、事件名与载荷类型。
 *
 * 主窗（设置页）发事件并显示窗口，更新说明窗口监听同一事件。放在这里是为了
 * 让事件名与载荷结构只有一个定义处：两侧各自写一遍字符串会悄悄漂移。
 */

/** 与 `src-tauri/tauri.conf.json` 中静态窗口的 label 一致。 */
export const UPDATE_NOTES_WINDOW_LABEL = "update-notes";

/** 主窗 → 更新说明窗口的数据事件（payload 为 `UpdateInfo`）。 */
export const UPDATE_NOTES_EVENT = "update-notes-data";

/** 外部引导级别（deb/rpm/AUR）的下载页。 */
export const RELEASES_URL = "https://github.com/ouyangjiahong26/altgo/releases/latest";

/** `check_update` 命令的返回结构（IPC 面向结构体，字段为 camelCase）。 */
export interface UpdateInfo {
  hasUpdate: boolean;
  currentVersion: string;
  latestVersion: string;
  /** latest.json 的 `notes`，即 CHANGELOG.md 当前版本小节（受限 Markdown）。 */
  body?: string;
  date?: string;
  supportTier: "in_place" | "deb_package" | "external";
}

/**
 * 更新安装在更新说明窗里的四个状态：
 * `idle` 未开始，`downloading` 正在下载安装包，`installing` 等待系统授权并安装，
 * `installed` 已装好、等用户重启。就地更新档只用得到前后两个状态。
 */
export type UpdateInstallState = "idle" | "downloading" | "installing" | "installed";

/** 主进程 → 更新说明窗的下载进度事件（仅 deb 直装档发送）。 */
export const UPDATE_INSTALL_PROGRESS_EVENT = "update-install-progress";

export interface UpdateInstallProgress {
  phase: "downloading" | "installing";
  downloaded: number;
  total: number;
}

/**
 * 把更新结果推给更新说明窗口并前置显示。
 * 窗口不存在（配置被改动）时静默放弃，调用方无需分支。
 */
export async function openUpdateNotes(info: UpdateInfo): Promise<void> {
  // 先发数据再显示：窗口渲染首帧即带内容，不会闪一下空态。
  await emit(UPDATE_NOTES_EVENT, info);

  const win = await WebviewWindow.getByLabel(UPDATE_NOTES_WINDOW_LABEL);
  if (!win) return;

  await win.show();
  await win.setFocus();
}
