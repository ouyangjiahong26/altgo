/** 主题偏好存本地（不进 Tauri 配置）。 */

export const THEME_PREF_KEY = "altgo-theme-pref";

export type ThemePref = "system" | "light" | "dark";

export function getThemePref(): ThemePref {
  try {
    const v = localStorage.getItem(THEME_PREF_KEY);
    if (v === "light" || v === "dark" || v === "system") return v;
  } catch {
    /* 忽略 */
  }
  return "system";
}

export function resolveEffectiveTheme(pref: string): "light" | "dark" {
  if (pref === "light") return "light";
  if (pref === "dark") return "dark";
  if (typeof window === "undefined" || !window.matchMedia) return "dark";
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

export function applyThemeToDocument(pref?: string): void {
  const p = pref ?? getThemePref();
  document.documentElement.dataset.theme = resolveEffectiveTheme(p);
}

export function setThemePref(pref: ThemePref): void {
  try {
    localStorage.setItem(THEME_PREF_KEY, pref);
  } catch {
    /* 忽略 */
  }
  applyThemeToDocument(pref);
  window.dispatchEvent(new CustomEvent("altgo-theme-changed"));
}

export function installThemeListeners(onApply: () => void): () => void {
  const mq = window.matchMedia("(prefers-color-scheme: dark)");
  const onMq = () => {
    if (getThemePref() === "system") onApply();
  };
  const onCustom = () => onApply();
  const onStorage = (e: StorageEvent) => {
    if (e.key === THEME_PREF_KEY) onApply();
  };
  mq.addEventListener("change", onMq);
  window.addEventListener("altgo-theme-changed", onCustom);
  window.addEventListener("storage", onStorage);
  return () => {
    mq.removeEventListener("change", onMq);
    window.removeEventListener("altgo-theme-changed", onCustom);
    window.removeEventListener("storage", onStorage);
  };
}
