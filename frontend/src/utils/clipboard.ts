import { invoke } from "@tauri-apps/api/core";

/**
 * 将文本写入系统剪贴板。
 *
 * 优先走 Tauri 后端（Linux 下为 xclip/xsel/wl-copy），失败后回退到 WebView API。
 * 两者都失败时返回 false。
 */
export async function copyToClipboard(text: string): Promise<boolean> {
  try {
    await invoke("copy_text", { text });
    return true;
  } catch {
    // 后端剪贴板可能不可用；回退到 WebView API
  }
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}
