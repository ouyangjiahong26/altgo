/** 首次安装引导标记：与主题/字号/窗口尺寸偏好一样存 localStorage，不进 Tauri 配置。 */
export const ONBOARDING_KEY = "altgo-onboarded";

export function isOnboarded(): boolean {
  try {
    return localStorage.getItem(ONBOARDING_KEY) === "1";
  } catch {
    return false;
  }
}

export function completeOnboarding(): void {
  try {
    localStorage.setItem(ONBOARDING_KEY, "1");
  } catch {
    /* 忽略 */
  }
}

/** 触发键自检提示标记：首次进主页把检测卡展开一次，之后只留入口按钮。 */
export const HOTKEY_HINT_KEY = "altgo-hotkey-hint-seen";

export function hasSeenHotkeyHint(): boolean {
  try {
    return localStorage.getItem(HOTKEY_HINT_KEY) === "1";
  } catch {
    return false;
  }
}

export function markHotkeyHintSeen(): void {
  try {
    localStorage.setItem(HOTKEY_HINT_KEY, "1");
  } catch {
    /* 忽略 */
  }
}
