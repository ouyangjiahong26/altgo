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
