import { Children, isValidElement, useEffect, useState, type ReactNode } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "../i18n";
import { useConfigForm, normalizeConfig, type AppConfig } from "../hooks/useConfigForm";
import { useModelManager } from "../hooks/useModelManager";
import {
  Save,
  Globe,
  Mic,
  Sparkles,
  Check,
  Download,
  Trash2,
  AlertCircle,
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  Palette,
  Keyboard,
} from "lucide-react";
import { useTheme, type ThemePref } from "../ThemeContext";
import {
  getFontSizePref,
  getWindowSizePref,
  setFontSizePref,
  setWindowSizePref,
  type FontSizePref,
  type WindowSizePref,
} from "../ui-size";
import { ProviderPresetSelector } from "../components/ProviderPresetSelector";
import { loadCatalog } from "../config/catalog";
import { type ProviderPreset, type ModelCatalogEntry } from "../config/modelPresets";

const KEY_PRESETS: { value: string; labelKey: string }[] = [
  { value: "Alt_R", labelKey: "settings.key_preset_right_alt" },
];

function isPresetKeyName(keyName: string): boolean {
  if (KEY_PRESETS.some((p) => p.value === keyName)) return true;
  return keyName === "ISO_Level3_Shift" || keyName === "AltGr";
}

function presetSelectValue(keyName: string): string {
  if (KEY_PRESETS.some((p) => p.value === keyName)) return keyName;
  if (keyName === "ISO_Level3_Shift" || keyName === "AltGr") return "Alt_R";
  return "__custom__";
}

function formatSize(bytes: number): string {
  const mb = bytes / (1024 * 1024);
  if (mb >= 1024) return `${(mb / 1024).toFixed(1)} GB`;
  return `${Math.round(mb)} MB`;
}

function SettingsSectionOrder({ children }: { children: ReactNode }) {
  const sections = Children.toArray(children).sort((a, b) => {
    const orderOf = (node: ReactNode) =>
      isValidElement<{ "data-settings-order"?: number }>(node)
        ? node.props["data-settings-order"] ?? 99
        : 99;
    return orderOf(a) - orderOf(b);
  });

  return <>{sections}</>;
}

export default function Settings() {
  const { t, lang, setLang } = useTranslation();
  const { themePref, setTheme } = useTheme();
  const [fontSize, setFontSize] = useState<FontSizePref>(() => getFontSizePref());
  const [windowSize, setWindowSize] = useState<WindowSizePref>(() => getWindowSizePref());
  const [polishOpen, setPolishOpen] = useState(true);
  const [advancedPath, setAdvancedPath] = useState(false);
  const [appVersion, setAppVersion] = useState<string>("");
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<{ ok: boolean; error?: string } | null>(null);
  const [clearingKey, setClearingKey] = useState(false);
  const [catalogPresets, setCatalogPresets] = useState<ProviderPreset[]>([]);
  const [catalogLoading, setCatalogLoading] = useState(false);
  const [catalogError, setCatalogError] = useState<string | null>(null);


  const [checkingUpdate, setCheckingUpdate] = useState(false);
  const [installingUpdate, setInstallingUpdate] = useState(false);
  const [updateInfo, setUpdateInfo] = useState<{
    hasUpdate: boolean;
    currentVersion: string;
    latestVersion: string;
    body?: string;
    date?: string;
    supportTier: "in_place" | "external";
  } | null>(null);
  const [updateError, setUpdateError] = useState<string | null>(null);

  const modelMgr = useModelManager({ t });
  const form = useConfigForm({
    t,
    setLang,
    onAfterSave: (saved) => {
      modelMgr.refreshResolved(saved.model);
      modelMgr.refreshModels();
    },
  });
  const {
    config,
    setConfig,
    saving,
    message,
    update,
    save,
    saveWith,
    keyCapturing,
    captureActivationKey,
  } = form;
  const { models, downloading, resolvedPath, refreshResolved } = modelMgr;

  useEffect(() => {
    getVersion().then(setAppVersion).catch(() => {});
  }, []);

  useEffect(() => {
    if (!config) return;
    refreshResolved(config.model);
  }, [config?.model, refreshResolved]);

  const applyLocalModel = async (name: string) => {
    if (!config) return;
    await saveWith({ ...config, model: name });
  };

  const downloadAndUse = async (name: string) => {
    await modelMgr.downloadAndUse(name, applyLocalModel);
  };

  const handleDelete = async (name: string) => {
    await modelMgr.deleteModel(name, (deleted) => {
      if (config?.model === deleted) {
        update("model", "");
      }
    });
  };

  // 拉取 omp 供应商目录（设置页挂载时自动调用；失败可重试，手填地址仍可用）。
  // Fetches the omp provider catalog (auto-invoked on mount; failures are retryable
  // and manual address entry keeps working).
  const loadOnlineCatalog = async () => {
    if (catalogLoading) return;
    setCatalogLoading(true);
    setCatalogError(null);
    try {
      setCatalogPresets(await loadCatalog());
    } catch (e) {
      setCatalogError(String(e));
    } finally {
      setCatalogLoading(false);
    }
  };

  useEffect(() => {
    void loadOnlineCatalog();
  }, []);

  // 用表单当前值直接测试（密钥留空时后端回落到已存密钥），不必先保存。
  // Test directly with current form values (an empty key falls back to the saved one server-side);
    // no need to save first.
  const runTestConnection = async () => {
    if (!config) return;
    setTesting(true);
    setTestResult(null);
    try {
      await invoke("test_polisher_connection", {
        protocol: config.polishProtocol,
        apiBaseUrl: config.polishApiBaseUrl,
        apiKey: config.polisherApiKey,
        model: config.polishModel,
      });
      setTestResult({ ok: true });
    } catch (e) {
      setTestResult({ ok: false, error: String(e) });
    } finally {
      setTesting(false);
    }
  };

  const clearApiKey = async () => {
    setClearingKey(true);
    setTestResult(null);
    try {
      await invoke("save_config", { patch: { polishApiKey: "" } });
      const c = await invoke<AppConfig>("get_config");
      setConfig(normalizeConfig(c));
    } catch (e) {
      setTestResult({ ok: false, error: String(e) });
    } finally {
      setClearingKey(false);
    }
  };

  const handleCheckUpdate = async () => {
    setCheckingUpdate(true);
    setUpdateError(null);
    setUpdateInfo(null);
    try {
      const res = await invoke<{
        hasUpdate: boolean;
        currentVersion: string;
        latestVersion: string;
        body?: string;
        date?: string;
        supportTier: "in_place" | "external";
      }>("check_update", { mode: "manual" });
      setUpdateInfo(res);
    } catch (err: any) {
      if (err && typeof err === "object" && err.message) {
        setUpdateError(err.message);
      } else {
        setUpdateError(String(err));
      }
    } finally {
      setCheckingUpdate(false);
    }
  };

  const handleInstallUpdate = async () => {
    setInstallingUpdate(true);
    setUpdateError(null);
    try {
      await invoke("install_update");
    } catch (err: any) {
      setUpdateError(String(err));
      setInstallingUpdate(false);
    }
  };

  const handleOpenReleasePage = () => {
    window.open("https://github.com/cislunarspace/altgo/releases/latest", "_blank");
  };

  if (!config) {
    return <div className="loading-container">{t("settings.loading")}</div>;
  }

  const localReady = resolvedPath != null && resolvedPath !== "";
  const localBlocked = config.model.trim() !== "" && resolvedPath === null;

  return (
    <div className="settings-page settings-page--v2">
      <div
        className={`settings-readiness ${
          localReady ? "settings-readiness--ok" : "settings-readiness--warn"
        }`}
      >
        <div className="settings-readiness-icon">
          {localReady ? <CheckCircle2 size={18} /> : <AlertCircle size={18} />}
        </div>
        <div className="settings-readiness-text">
          <strong className="settings-readiness-title">
            {localReady
              ? t("settings.readiness_local_ok")
              : t("settings.readiness_local_need")}
          </strong>
          <p className="settings-readiness-desc">
            {localBlocked
              ? t("settings.readiness_path_missing")
              : t("settings.readiness_local_desc")}
          </p>
          {resolvedPath && <code className="settings-readiness-path">{resolvedPath}</code>}
        </div>
      </div>

      <div className="settings-form">
        <SettingsSectionOrder>
        <section
          data-settings-order={3}
          className="settings-section settings-section--primary settings-section--transcription"
        >
          <h3 className="settings-section-title">
            <Sparkles size={14} />
            {t("settings.transcription")}
          </h3>
          <p className="settings-section-lead">{t("settings.transcription_lead")}</p>

          <>
              <div className="settings-field">
                <span className="settings-field-label-text">{t("settings.language")}</span>
                <div className="settings-field-control settings-field-control--narrow">
                  <input
                    type="text"
                    className="settings-input"
                    value={config.language}
                    onChange={(e) => update("language", e.target.value)}
                    placeholder="zh"
                  />
                </div>
              </div>

              <div className="settings-model-grid">
                {models.map((m) => {
                  const isActive = config.model === m.name;
                  const { percent, connecting } = modelMgr.getDownloadProgress(m.name);
                  return (
                    <div
                      key={m.name}
                      className={`settings-model-card ${isActive ? "is-active" : ""}`}
                    >
                      <div className="settings-model-card-head">
                        <span className="settings-model-card-name">{m.name}</span>
                        {isActive && (
                          <span className="settings-model-card-badge">{t("settings.in_use")}</span>
                        )}
                      </div>
                      <p className="settings-model-card-desc">{m.description}</p>
                      <p className="settings-model-card-meta">
                        {formatSize(m.sizeBytes)} · {m.filename}
                      </p>
                      <div className="settings-model-card-actions">
                        {m.downloaded ? (
                          <>
                            <button
                              type="button"
                              className="settings-btn settings-btn-sm settings-btn-secondary"
                              onClick={() => applyLocalModel(m.name)}
                              disabled={isActive || saving}
                            >
                              {isActive ? t("settings.current") : t("settings.use_model")}
                            </button>
                            <button
                              type="button"
                              className="settings-btn settings-btn-sm settings-btn-danger"
                              onClick={() => handleDelete(m.name)}
                            >
                              <Trash2 size={11} />
                              {t("settings.delete_model")}
                            </button>
                          </>
                        ) : downloading === m.name ? (
                          <div className="model-progress" style={{ width: "100%" }}>
                            <div className="progress-bar">
                              <div className="progress-fill" style={{ width: `${percent}%` }} />
                            </div>
                            <span className="progress-text">
                              {connecting
                                ? t("settings.model_download_connecting")
                                : `${percent}%`}
                            </span>
                          </div>
                        ) : (
                          <button
                            type="button"
                            className="settings-btn settings-btn-sm settings-btn-primary"
                            onClick={() => downloadAndUse(m.name)}
                            disabled={downloading !== null}
                          >
                            <Download size={11} />
                            {t("settings.download_and_use")}
                          </button>
                        )}
                      </div>
                    </div>
                  );
                })}
              </div>
              <button
                type="button"
                className="settings-advanced-toggle"
                onClick={() => setAdvancedPath(!advancedPath)}
              >
                {advancedPath ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
                {t("settings.advanced_model_path")}
              </button>
              {advancedPath && (
                <div className="settings-field settings-field--nested">
                  <span className="settings-field-label-text">{t("settings.custom_path")}</span>
                  <div className="settings-field-control">
                    <input
                      type="text"
                      className="settings-input"
                      value={config.model}
                      onChange={(e) => update("model", e.target.value)}
                      placeholder={t("settings.custom_path_placeholder")}
                    />
                  </div>
                  <p className="settings-hint">{t("settings.custom_path_hint")}</p>
                </div>
              )}
              <div className="settings-field">
                <span className="settings-field-label-text">{t("settings.inject_text")}</span>
                <div className="settings-field-control">
                  <label style={{ display: "flex", alignItems: "center", cursor: "pointer", gap: "6px" }}>
                    <input
                      type="checkbox"
                      checked={config.injectText}
                      onChange={(e) => update("injectText", e.target.checked)}
                    />
                  </label>
                </div>
              </div>
              <p className="settings-hint">{t("settings.inject_text_hint")}</p>
            </>
        </section>

        <section data-settings-order={1} className="settings-section settings-section--recording">
          <h3 className="settings-section-title">
            <Mic size={14} />
            {t("settings.recording")}
          </h3>
          <p className="settings-section-lead">{t("settings.recording_lead")}</p>
          <div className="settings-field">
            <span className="settings-field-label-text">{t("settings.key_name")}</span>
            <div className="settings-field-control settings-field-control--trigger-key">
              <select
                className="settings-select"
                value={presetSelectValue(config.keyName)}
                onChange={(e) => {
                  if (e.target.value === "__custom__") return;
                  setConfig((prev) =>
                    prev
                      ? {
                          ...prev,
                          keyName: e.target.value,
                          linuxEvdevCode: null,
                        }
                      : prev,
                  );
                }}
              >
                {KEY_PRESETS.map((p) => (
                  <option key={p.value} value={p.value}>
                    {t(p.labelKey)}
                  </option>
                ))}
                <option value="__custom__">{t("settings.key_custom")}</option>
              </select>
              {!isPresetKeyName(config.keyName) && (
                <div className="settings-key-binding-readout">
                  <span className="settings-muted">{t("settings.key_binding_active")}</span>
                  <code className="settings-key-binding-code">{config.keyName}</code>
                </div>
              )}
            </div>
          </div>
          {!isPresetKeyName(config.keyName) && config.linuxEvdevCode == null && (
            <div className="settings-field">
              <span className="settings-field-label-text">{t("settings.key_custom_value")}</span>
              <div className="settings-field-control">
                <input
                  type="text"
                  className="settings-input"
                  value={config.keyName}
                  onChange={(e) =>
                    setConfig((prev) =>
                      prev
                        ? {
                            ...prev,
                            keyName: e.target.value,
                            linuxEvdevCode: null,
                          }
                        : prev,
                    )
                  }
                />
              </div>
            </div>
          )}
          <div className="settings-field">
            <span className="settings-field-label-text">{t("settings.capture_activation")}</span>
            <div className="settings-field-control">
              <button
                type="button"
                className="settings-btn settings-btn-secondary"
                onClick={() => void captureActivationKey()}
                disabled={saving || keyCapturing}
              >
                <Keyboard size={14} />
                {keyCapturing ? t("settings.capture_waiting") : t("settings.capture_activation_short")}
              </button>
            </div>
          </div>
          <p className="settings-hint">{t("settings.capture_activation_lead")}</p>
        </section>

        <section
          data-settings-order={2}
          className="settings-section settings-section--polishing settings-section--primary"
        >
          <button
            type="button"
            className="settings-section-toggle"
            onClick={() => setPolishOpen(!polishOpen)}
          >
            <Sparkles size={14} />
            {t("settings.polishing")}
            {polishOpen ? <ChevronDown size={16} /> : <ChevronRight size={16} />}
          </button>
          {polishOpen && (
            <div className="settings-section-body">
              <ProviderPresetSelector
                presets={catalogPresets}
                modelType="polisher"
                currentApiBaseUrl={config.polishApiBaseUrl}
                currentModel={config.polishModel}
                lang={lang}
                t={t}
                onSelect={(preset: ProviderPreset, model?: ModelCatalogEntry) => {
                  update("polishApiBaseUrl", preset.apiBaseUrl);
                  update("polishModel", model?.model || preset.defaultModel);
                  update("polishProtocol", preset.apiFormat);
                }}
              />
              {catalogLoading && catalogPresets.length === 0 && (
                <p className="settings-hint settings-hint--polish">{t("settings.catalog_loading")}</p>
              )}
              {catalogError && (
                <>
                  <p className="settings-hint settings-hint--polish settings-test-err">
                    {catalogError}
                  </p>
                  <button
                    type="button"
                    className="settings-btn settings-btn-sm settings-btn-secondary"
                    onClick={() => void loadOnlineCatalog()}
                    disabled={catalogLoading}
                  >
                    {t("settings.catalog_retry")}
                  </button>
                </>
              )}
              <div className="settings-field">
                <span className="settings-field-label-text">{t("settings.polish_level")}</span>
                <div className="settings-field-control">
                  <select
                    className="settings-select"
                    value={config.polishLevel}
                    onChange={(e) => update("polishLevel", e.target.value)}
                  >
                    <option value="none">{t("settings.polish_none")}</option>
                    <option value="light">{t("settings.polish_light")}</option>
                    <option value="medium">{t("settings.polish_medium")}</option>
                    <option value="heavy">{t("settings.polish_heavy")}</option>
                  </select>
                </div>
              </div>
              <p className="settings-hint settings-hint--polish">{t("settings.polish_level_hint")}</p>
              {(config.hasPolisherApiKey || config.polisherApiKey) && config.polishLevel === "none" && (
                <p className="settings-hint settings-hint--polish">
                  {t("settings.polish_disabled_hint")}
                </p>
              )}
              <div className="settings-field">
                <span className="settings-field-label-text">{t("settings.api_protocol")}</span>
                <div className="settings-field-control">
                  <select
                    className="settings-select"
                    value={config.polishProtocol === "anthropic" ? "anthropic" : "openai"}
                    onChange={(e) => update("polishProtocol", e.target.value)}
                  >
                    <option value="openai">{t("settings.api_protocol_openai")}</option>
                    <option value="anthropic">{t("settings.api_protocol_anthropic")}</option>
                  </select>
                </div>
              </div>
              <div className="settings-field">
                <span className="settings-field-label-text">{t("settings.api_url")}</span>
                <div className="settings-field-control">
                  <input
                    type="text"
                    className="settings-input"
                    value={config.polishApiBaseUrl}
                    onChange={(e) => update("polishApiBaseUrl", e.target.value)}
                    placeholder="https://api.openai.com"
                  />
                </div>
              </div>
              <div className="settings-field">
                <span className="settings-field-label-text">{t("settings.model")}</span>
                <div className="settings-field-control">
                  <input
                    type="text"
                    className="settings-input"
                    value={config.polishModel}
                    onChange={(e) => update("polishModel", e.target.value)}
                    placeholder="gpt-4o-mini"
                  />
                </div>
              </div>
              <div className="settings-field">
                <span className="settings-field-label-text">{t("settings.api_key")}</span>
                <div className="settings-field-control">
                  <input
                    type="password"
                    className="settings-input"
                    value={config.polisherApiKey}
                    onChange={(e) => update("polisherApiKey", e.target.value)}
                    placeholder={config.hasPolisherApiKey ? "sk-***" : "sk-..."}
                  />
                </div>
              </div>
              <div className="settings-polish-actions">
                <button
                  type="button"
                  className="settings-btn settings-btn-sm settings-btn-secondary"
                  onClick={runTestConnection}
                  disabled={testing}
                >
                  {testing ? t("settings.testing") : t("settings.test_connection")}
                </button>
                {config.hasPolisherApiKey && (
                  <button
                    type="button"
                    className="settings-btn settings-btn-sm settings-btn-secondary"
                    onClick={clearApiKey}
                    disabled={clearingKey}
                  >
                    {t("settings.clear_api_key")}
                  </button>
                )}
              </div>
              {testResult && (
                <p className={`settings-hint ${testResult.ok ? "settings-test-ok" : "settings-test-err"}`}>
                  {testResult.ok
                    ? t("settings.test_ok")
                    : testResult.error}
                </p>
              )}
            </div>
          )}
        </section>

        </SettingsSectionOrder>

        <section className="settings-section settings-section--appearance">
          <h3 className="settings-section-title">
            <Palette size={14} />
            {t("settings.appearance")}
          </h3>
          <p className="settings-section-lead">{t("settings.appearance_lead")}</p>
          <div className="settings-field">
            <span className="settings-field-label-text">{t("settings.theme")}</span>
            <div className="settings-field-control">
              <select
                className="settings-select"
                value={themePref}
                onChange={(e) => setTheme(e.target.value as ThemePref)}
              >
                <option value="system">{t("settings.theme_system")}</option>
                <option value="light">{t("settings.theme_light")}</option>
                <option value="dark">{t("settings.theme_dark")}</option>
              </select>
            </div>
          </div>
          <div className="settings-field">
            <span className="settings-field-label-text">{t("settings.font_size")}</span>
            <div className="settings-field-control">
              <select
                className="settings-select"
                value={fontSize}
                onChange={(e) => {
                  const value = e.target.value as FontSizePref;
                  setFontSize(value);
                  setFontSizePref(value);
                }}
              >
                <option value="small">{t("settings.font_size_small")}</option>
                <option value="medium">{t("settings.font_size_medium")}</option>
                <option value="large">{t("settings.font_size_large")}</option>
              </select>
            </div>
          </div>
          <div className="settings-field">
            <span className="settings-field-label-text">{t("settings.window_size")}</span>
            <div className="settings-field-control">
              <select
                className="settings-select"
                value={windowSize}
                onChange={(e) => {
                  const value = e.target.value as WindowSizePref;
                  setWindowSize(value);
                  setWindowSizePref(value);
                }}
              >
                <option value="compact">{t("settings.window_size_compact")}</option>
                <option value="standard">{t("settings.window_size_standard")}</option>
                <option value="large">{t("settings.window_size_large")}</option>
              </select>
            </div>
          </div>
          <div className="settings-field">
            <span className="settings-field-label-text">{t("settings.overlay_position")}</span>
            <div className="settings-field-control">
              <select
                className="settings-select"
                value={config.overlayPosition}
                onChange={(e) => update("overlayPosition", e.target.value)}
              >
                <option value="bottom_center">{t("settings.overlay_position_bottom")}</option>
                <option value="top_center">{t("settings.overlay_position_top")}</option>
              </select>
            </div>
          </div>
        </section>

        <section className="settings-section settings-section--language">
          <h3 className="settings-section-title">
            <Globe size={14} />
            {t("settings.gui_language")}
          </h3>
          <div className="settings-field">
            <span className="settings-field-label-text">{t("settings.gui_language")}</span>
            <div className="settings-field-control">
              <select
                className="settings-select"
                value={config.guiLanguage}
                onChange={(e) => update("guiLanguage", e.target.value)}
              >
                <option value="zh">中文</option>
                <option value="en">English</option>
              </select>
            </div>
          </div>
        </section>

        <section className="settings-section settings-section--about">
          <h3 className="settings-section-title">
            <Sparkles size={14} />
            {t("settings.about")}
          </h3>
          <div className="settings-about-brand">
            <img src="/altgo-logo.svg" alt="" width={40} height={40} className="settings-about-logo" />
            <p className="settings-about-tagline">{t("settings.about_tagline")}</p>
          </div>
          <div className="settings-field">
            <span className="settings-field-label-text">{t("settings.version")}</span>
            <div className="settings-field-control" style={{ display: "flex", gap: "8px", alignItems: "center" }}>
              <span className="settings-muted">{appVersion || "…"}</span>
              <button
                type="button"
                className="settings-btn settings-btn-secondary"
                onClick={handleCheckUpdate}
                disabled={checkingUpdate || installingUpdate}
                style={{ padding: "3px 8px", fontSize: "12px" }}
              >
                {checkingUpdate ? t("settings.checking_update") : t("settings.check_update")}
              </button>
            </div>
          </div>

          <div className="settings-field">
            <span className="settings-field-label-text">{t("settings.auto_check_update")}</span>
            <div className="settings-field-control">
              <label style={{ display: "flex", alignItems: "center", cursor: "pointer", gap: "6px" }}>
                <input
                  type="checkbox"
                  checked={config.autoCheckUpdate}
                  onChange={(e) => update("autoCheckUpdate", e.target.checked)}
                />
              </label>
            </div>
          </div>

          {updateError && (
            <div className="settings-save-msg settings-save-msg--err" style={{ marginTop: "8px" }}>
              {updateError}
            </div>
          )}

          {updateInfo && !updateInfo.hasUpdate && !updateError && (
            <div className="settings-save-msg settings-save-msg--ok" style={{ marginTop: "8px" }}>
              <Check size={12} /> {t("settings.update_not_found")} ({updateInfo.latestVersion})
            </div>
          )}

          {updateInfo && updateInfo.hasUpdate && (
            <div style={{ marginTop: "12px", padding: "10px", background: "var(--bg-secondary, rgba(0,0,0,0.05))", borderRadius: "6px" }}>
              <div style={{ fontWeight: 600, display: "flex", alignItems: "center", gap: "6px", color: "var(--accent-color, #2563eb)" }}>
                <Sparkles size={14} />
                {t("settings.update_available")}: v{updateInfo.latestVersion}
              </div>
              {updateInfo.body && (
                <div style={{ marginTop: "6px", fontSize: "12px", whiteSpace: "pre-wrap", opacity: 0.85 }}>
                  <div style={{ fontWeight: 500 }}>{t("settings.update_notes")}</div>
                  {updateInfo.body}
                </div>
              )}
              <div style={{ marginTop: "10px", display: "flex", gap: "8px" }}>
                {updateInfo.supportTier === "in_place" ? (
                  <button
                    type="button"
                    className="settings-btn settings-btn-primary"
                    onClick={handleInstallUpdate}
                    disabled={installingUpdate}
                  >
                    <Download size={13} />
                    {installingUpdate ? t("settings.update_installing") : t("settings.update_install")}
                  </button>
                ) : (
                  <button
                    type="button"
                    className="settings-btn settings-btn-primary"
                    onClick={handleOpenReleasePage}
                  >
                    {t("settings.update_open_release")}
                  </button>
                )}
              </div>
            </div>
          )}
        </section>

        <div className="settings-save-row">
          <p className="settings-save-hint">{t("settings.restart_hint")}</p>
          {message === "saved" && (
            <span className="settings-save-msg settings-save-msg--ok">
              <Check size={12} /> {t("settings.saved")}
            </span>
          )}
          {message && message !== "saved" && (
            <span className="settings-save-msg settings-save-msg--err">{message}</span>
          )}
          <button
            type="button"
            className="settings-btn settings-btn-primary"
            onClick={save}
            disabled={saving}
          >
            <Save size={13} />
            {saving ? t("settings.saving") : t("settings.save")}
          </button>
        </div>
      </div>
    </div>
  );
}
