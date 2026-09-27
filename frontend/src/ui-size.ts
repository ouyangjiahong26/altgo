/** UI 尺寸偏好：字体大小与主窗口大小。与 theme 一样存 localStorage，不进 Tauri config。 */
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";

export const FONT_SIZE_KEY = "altgo-font-size";
export const WINDOW_SIZE_KEY = "altgo-window-size";

export type FontSizePref = "small" | "medium" | "large";
export type WindowSizePref = "compact" | "standard" | "large";

/* 整套样式以 rem 为基准，调 root font-size 即可整体缩放字体与间距 */
const FONT_SIZE_PX: Record<FontSizePref, string> = {
  small: "15px",
  medium: "17px",
  large: "19px",
};

const WINDOW_SIZE: Record<WindowSizePref, { width: number; height: number }> = {
  compact: { width: 400, height: 600 },
  standard: { width: 440, height: 680 },
  large: { width: 480, height: 760 },
};

export function getFontSizePref(): FontSizePref {
  try {
    const v = localStorage.getItem(FONT_SIZE_KEY);
    if (v === "small" || v === "medium" || v === "large") return v;
  } catch {
    /* 忽略 */
  }
  return "medium";
}

export function getWindowSizePref(): WindowSizePref {
  try {
    const v = localStorage.getItem(WINDOW_SIZE_KEY);
    if (v === "compact" || v === "standard" || v === "large") return v;
  } catch {
    /* 忽略 */
  }
  return "standard";
}

export function applyFontSize(pref?: FontSizePref): void {
  document.documentElement.style.fontSize = FONT_SIZE_PX[pref ?? getFontSizePref()];
}

export async function applyWindowSize(pref?: WindowSizePref): Promise<void> {
  try {
    const win = getCurrentWindow();
    if (await win.isMaximized()) return;
    const size = WINDOW_SIZE[pref ?? getWindowSizePref()];
    await win.setSize(new LogicalSize(size.width, size.height));
  } catch {
    /* 非 Tauri 环境或窗口状态不允许调整时忽略 */
  }
}

export function setFontSizePref(pref: FontSizePref): void {
  try {
    localStorage.setItem(FONT_SIZE_KEY, pref);
  } catch {
    /* 忽略 */
  }
  applyFontSize(pref);
}

export function setWindowSizePref(pref: WindowSizePref): void {
  try {
    localStorage.setItem(WINDOW_SIZE_KEY, pref);
  } catch {
    /* 忽略 */
  }
  void applyWindowSize(pref);
}
