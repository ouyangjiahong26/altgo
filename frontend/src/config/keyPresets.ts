/** 触发键预设与取值归一：设置页与首次引导共用，避免两处各维护一份。
 * Trigger-key presets and value normalization shared by the settings page and the
 * first-run wizard, so both stay in sync. */
export const KEY_PRESETS: { value: string; labelKey: string }[] = [
  { value: "Alt_R", labelKey: "settings.key_preset_right_alt" },
];

export function isPresetKeyName(keyName: string): boolean {
  if (KEY_PRESETS.some((p) => p.value === keyName)) return true;
  return keyName === "ISO_Level3_Shift" || keyName === "AltGr";
}

export function presetSelectValue(keyName: string): string {
  if (KEY_PRESETS.some((p) => p.value === keyName)) return keyName;
  if (keyName === "ISO_Level3_Shift" || keyName === "AltGr") return "Alt_R";
  return "__custom__";
}
