import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { message as showMessageDialog } from "@tauri-apps/plugin-dialog";

export interface AppConfig {
  keyName: string;
  linuxEvdevCode: number | null;
  windowsVkCode: number | null;
  language: string;
  model: string;
  polishLevel: string;
  polishModel: string;
  polishApiBaseUrl: string;
  polishProtocol: string;
  polishThinkingLevel: string;
  guiLanguage: string;
  overlayPosition: string;
  autoCheckUpdate: boolean;
  injectText: boolean;
  polisherApiKey: string;
  hasPolisherApiKey: boolean;
}

export function saveRequestBody(c: AppConfig) {
  return {
    keyName: c.keyName,
    linuxEvdevCode: c.linuxEvdevCode,
    windowsVkCode: c.windowsVkCode,
    language: c.language,
    model: c.model,
    polishLevel: c.polishLevel,
    polishModel: c.polishModel,
    ...(c.polisherApiKey ? { polishApiKey: c.polisherApiKey } : {}),
    polishApiBaseUrl: c.polishApiBaseUrl,
    polishProtocol: c.polishProtocol,
    polishThinkingLevel: c.polishThinkingLevel,
    guiLanguage: c.guiLanguage,
    overlayPosition: c.overlayPosition,
    autoCheckUpdate: c.autoCheckUpdate,
    injectText: c.injectText,
  };
}

export function normalizeConfig(c: AppConfig): AppConfig {
  return {
    ...c,
    linuxEvdevCode: c.linuxEvdevCode ?? null,
    windowsVkCode: c.windowsVkCode ?? null,
    autoCheckUpdate: c.autoCheckUpdate ?? true,
    polishThinkingLevel: c.polishThinkingLevel ?? "off",
    injectText: c.injectText ?? false,
    polisherApiKey: "",
    hasPolisherApiKey: c.hasPolisherApiKey ?? false,
  };
}

export interface UseConfigFormOptions {
  t: (key: string) => string;
  setLang: (lang: string) => void;
  onAfterSave?: (saved: AppConfig) => void;
}

export interface UseConfigFormResult {
  config: AppConfig | null;
  setConfig: React.Dispatch<React.SetStateAction<AppConfig | null>>;
  saving: boolean;
  message: string;
  setMessage: (msg: string) => void;
  update: <K extends keyof AppConfig>(key: K, value: AppConfig[K]) => void;
  save: () => Promise<void>;
  saveWith: (next: AppConfig) => Promise<void>;
  keyCapturing: boolean;
  captureActivationKey: () => Promise<void>;
}

/**
 * Owns the configuration form: state, normalization, persistence, key capture.
 */
export function useConfigForm({
  t,
  setLang,
  onAfterSave,
}: UseConfigFormOptions): UseConfigFormResult {
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState("");
  const [keyCapturing, setKeyCapturing] = useState(false);

  useEffect(() => {
    invoke<AppConfig>("get_config")
      .then((c) => setConfig(normalizeConfig(c)))
      .catch((e) => setMessage(String(e)));
  }, []);

  const update = useCallback(
    <K extends keyof AppConfig>(key: K, value: AppConfig[K]) => {
      setConfig((prev) => (prev ? { ...prev, [key]: value } : prev));
    },
    [],
  );

  const saveWith = useCallback(
    async (next: AppConfig) => {
      setSaving(true);
      setMessage("");
      try {
        await invoke("save_config", { patch: saveRequestBody(next) });
        setConfig(next);
        setLang(next.guiLanguage);
        setMessage("saved");
        onAfterSave?.(next);
      } catch (e) {
        setMessage(String(e));
      } finally {
        setSaving(false);
      }
    },
    [setLang, onAfterSave],
  );

  const save = useCallback(async () => {
    if (!config) return;
    await saveWith(config);
  }, [config, saveWith]);

  const captureActivationKey = useCallback(async () => {
    if (!config) return;
    setKeyCapturing(true);
    setMessage("");
    try {
      const r = await invoke<{
        keyName: string;
        linuxEvdevCode?: number | null;
        windowsVkCode?: number | null;
      }>("capture_activation_key");
      const next: AppConfig = {
        ...config,
        keyName: r.keyName,
        linuxEvdevCode: r.linuxEvdevCode ?? null,
        windowsVkCode: r.windowsVkCode ?? null,
      };
      await saveWith(next);
    } catch (e) {
      setMessage(String(e));
      await invoke("start_pipeline").catch(() => {});
      await showMessageDialog(String(e), {
        title: t("settings.capture_error_title"),
        kind: "error",
      }).then(() => undefined);
    } finally {
      setKeyCapturing(false);
    }
  }, [config, saveWith, t]);

  return {
    config,
    setConfig,
    saving,
    message,
    setMessage,
    update,
    save,
    saveWith,
    keyCapturing,
    captureActivationKey,
  };
}
