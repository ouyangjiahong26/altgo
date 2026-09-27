/** 字节数转人类可读大小（设置页与首次引导的模型列表共用）。
 * Formats a byte count for the model lists in settings and onboarding. */
export function formatSize(bytes: number): string {
  const mb = bytes / (1024 * 1024);
  if (mb >= 1024) return `${(mb / 1024).toFixed(1)} GB`;
  return `${Math.round(mb)} MB`;
}
