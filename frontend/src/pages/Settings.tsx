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
import { openUpdateNotes, type UpdateInfo } from "../updateNotes";
import { loadCatalog } from "../config/catalog";
import { type ProviderPreset, type ModelCatalogEntry } from "../config/modelPresets";
import { KEY_PRESETS, isPresetKeyName, presetSelectValue } from "../config/keyPresets";
import { formatSize } from "../utils/format";

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
  const [asrAdvancedOpen, setAsrAdvancedOpen] = useState(false);
  const [polishAdvancedOpen, setPolishAdvancedOpen] = useState(false);
  const [appVersion, setAppVersion] = useState<string>("");
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<{ ok: boolean; error?: string } | null>(null);
  const [clearingKey, setClearingKey] = useState(false);
  const [clearingAsrKey, setClearingAsrKey] = useState(false);
  const [asrClearError, setAsrClearError] = useState<string | null>(null);
  const [catalogPresets, setCatalogPresets] = useState<ProviderPreset[]>([]);
  const [catalogLoading, setCatalogLoading] = useState(false);
  const [catalogError, setCatalogError] = useState<string | null>(null);


  const [checkingUpdate, setCheckingUpdate] = useState(false);
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);
  const [updateError, setUpdateError] = useState<string | null>(null);

  const modelMgr = useModelManager({ t });
  const form = useConfigForm({
    t,
    setLang,
    onAfterSave: () => {
      modelMgr.refreshModels();
    },
  });
  const {
    config,
    setConfig,
    saving,
    message,
    setMessage,
    update,
    save,
    keyCapturing,
    captureActivationKey,
  } = form;
  const { models, downloading } = modelMgr;

  useEffect(() => {
    getVersion().then(setAppVersion).catch(() => {});
  }, []);

  // 只写模型字段：下载在 model-download-finished 之后才结束，可能晚于用户离开本页；整份回写
  // 会把期间（或在别处）保存的配置覆盖回旧值。窄补丁与清除密钥走同一条路径。
  // Write only the model field: the download finishes after model-download-finished, possibly after
  // the user left this page; a whole-config write would revert whatever was saved meanwhile.
  const applyLocalModel = async (name: string) => {
    update("model", name);
    try {
      await invoke("save_config", { patch: { model: name } });
    } catch (e) {
      setMessage(String(e));
    }
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

  // 清除已保存的在线识别密钥（后端存明文，清除后 hasAsrApiKey 变 false）。
  // Clears the saved online ASR key (stored server-side; hasAsrApiKey flips false after clearing).
  const clearAsrApiKey = async () => {
    setClearingAsrKey(true);
    setAsrClearError(null);
    try {
      await invoke("save_config", { patch: { asrApiKey: "" } });
      const c = await invoke<AppConfig>("get_config");
      setConfig(normalizeConfig(c));
    } catch (e) {
      setAsrClearError(String(e));
    } finally {
      setClearingAsrKey(false);
    }
  };

  const handleCheckUpdate = async () => {
    setCheckingUpdate(true);
    setUpdateError(null);
    setUpdateInfo(null);
    try {
      const res = await invoke<UpdateInfo>("check_update", { mode: "manual" });
      setUpdateInfo(res);
      // 手动检查发现新版本即弹更新说明窗口；无更新或失败保持页内提示，不弹窗。
      // Pop the release-notes window only when a manual check finds a new version.
      if (res.hasUpdate) await openUpdateNotes(res);
    } catch (err) {
      // Tauri 命令错误：字符串或带 message 的对象。
      // Tauri command errors are either a string or an object with `message`.
      setUpdateError(
        err && typeof err === "object" && "message" in err && typeof err.message === "string"
          ? err.message
          : String(err)
      );
    } finally {
      setCheckingUpdate(false);
    }
  };

  if (!config) {
    return <div className="loading-container">{t("settings.loading")}</div>;
  }

  // 在线后端只需配置密钥即可用；本地面板由模型卡与错误事件自行表达就绪状态。
  // The online backend only needs a key; the local panel conveys its own readiness
  // through the model cards and error events.
  const onlineActive = config.transcriberBackend === "online";

  return (
    <div className="settings-page">
      <div className="settings-form">
        <SettingsSectionOrder>
        <section
          data-settings-order={2}
          className="settings-section settings-section--primary settings-section--transcription"
        >
          <h3 className="settings-section-title">
            <Sparkles size={14} />
            {t("settings.transcription")}
          </h3>

          <>
              <div className="settings-field">
                <span className="settings-field-label-text">{t("settings.language")}</span>
                <div className="settings-field-control settings-field-control--narrow">
                  <select
                    className="settings-select"
                    value={config.language}
                    onChange={(e) => update("language", e.target.value)}
                  >
                    <option value="zh">{t("settings.language_zh")}</option>
                    <option value="en">{t("settings.language_en")}</option>
                    <option value="">{t("settings.language_auto")}</option>
                    {/* 旧配置或手写 TOML 里的其他取值（如 yue/ja/ko/auto）原样保留，
                        否则受控下拉会显示成“中文”，与后端实际使用的语言不符。 */}
                    {/* Other values from older configs or hand-written TOML (yue/ja/ko/auto) are
                        kept as-is; otherwise the controlled select would show Chinese while the
                        backend keeps using the original language. */}
                    {!["zh", "en", ""].includes(config.language) && (
                      <option value={config.language}>{config.language}</option>
                    )}
                  </select>
                </div>
              </div>

              <div className="settings-field">
                <span className="settings-field-label-text">
                  {t("settings.transcriber_backend")}
                </span>
                <div className="settings-field-control">
                  <select
                    className="settings-select"
                    value={config.transcriberBackend === "online" ? "online" : "local"}
                    onChange={(e) => update("transcriberBackend", e.target.value)}
                  >
                    <option value="local">{t("settings.transcriber_backend_local")}</option>
                    <option value="online">{t("settings.transcriber_backend_online")}</option>
                  </select>
                </div>
              </div>

              {onlineActive ? (
                <>
                  <div className="settings-field">
                    <span className="settings-field-label-text">{t("settings.api_key")}</span>
                    <div className="settings-field-control">
                      <input
                        type="password"
                        className="settings-input"
                        value={config.asrApiKey}
                        onChange={(e) => update("asrApiKey", e.target.value)}
                        placeholder={config.hasAsrApiKey ? "sk-***" : "sk-..."}
                      />
                    </div>
                  </div>
                  {asrClearError && (
                    <p className="settings-hint settings-test-err">{asrClearError}</p>
                  )}
                  {config.hasAsrApiKey && (
                    <div className="settings-polish-actions">
                      <button
                        type="button"
                        className="settings-btn settings-btn-sm settings-btn-secondary"
                        onClick={clearAsrApiKey}
                        disabled={clearingAsrKey}
                      >
                        {t("settings.clear_api_key")}
                      </button>
                    </div>
                  )}
                  <button
                    type="button"
                    className="settings-advanced-toggle"
                    onClick={() => setAsrAdvancedOpen(!asrAdvancedOpen)}
                  >
                    {asrAdvancedOpen ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
                    {t("settings.advanced_asr")}
                  </button>
                  {asrAdvancedOpen && (
                    <>
                      <div className="settings-field">
                        <span className="settings-field-label-text">{t("settings.model")}</span>
                        <div className="settings-field-control">
                          <input
                            type="text"
                            className="settings-input"
                            value={config.asrModel}
                            onChange={(e) => update("asrModel", e.target.value)}
                            placeholder="mimo-v2.5-asr"
                          />
                        </div>
                      </div>
                      <div className="settings-field">
                        <span className="settings-field-label-text">{t("settings.api_url")}</span>
                        <div className="settings-field-control">
                          <input
                            type="text"
                            className="settings-input"
                            value={config.asrApiBaseUrl}
                            onChange={(e) => update("asrApiBaseUrl", e.target.value)}
                            placeholder="https://token-plan-cn.xiaomimimo.com/v1"
                          />
                        </div>
                      </div>
                    </>
                  )}
                </>
              ) : (
                <>
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
                  </div>
                )}
                </>
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
            </>
        </section>

        <section data-settings-order={1} className="settings-section settings-section--recording">
          <h3 className="settings-section-title">
            <Mic size={14} />
            {t("settings.recording")}
          </h3>
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
        </section>

        <section
          data-settings-order={3}
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
              {(config.hasPolisherApiKey || config.polisherApiKey) && config.polishLevel === "none" && (
                <p className="settings-hint settings-hint--polish">
                  {t("settings.polish_disabled_hint")}
                </p>
              )}
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
              <button
                type="button"
                className="settings-advanced-toggle"
                onClick={() => setPolishAdvancedOpen(!polishAdvancedOpen)}
              >
                {polishAdvancedOpen ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
                {t("settings.advanced_polish")}
              </button>
              {polishAdvancedOpen && (
                <>
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
                    <span className="settings-field-label-text">{t("settings.thinking_level")}</span>
                    <div className="settings-field-control">
                      <select
                        className="settings-select"
                        value={config.polishThinkingLevel}
                        onChange={(e) => update("polishThinkingLevel", e.target.value)}
                      >
                        <option value="off">{t("settings.thinking_off")}</option>
                        <option value="low">{t("settings.thinking_low")}</option>
                        <option value="medium">{t("settings.thinking_medium")}</option>
                        <option value="high">{t("settings.thinking_high")}</option>
                      </select>
                    </div>
                  </div>
                </>
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
                disabled={checkingUpdate}
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
            <div style={{ marginTop: "8px", display: "flex", alignItems: "center", gap: "8px" }}>
              <span
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: "6px",
                  fontSize: "var(--text-sm)",
                  fontWeight: 600,
                  color: "var(--color-accent)",
                }}
              >
                <Sparkles size={14} />
                {t("settings.update_available")} v{updateInfo.latestVersion}
              </span>
              <button
                type="button"
                className="settings-btn settings-btn-secondary settings-btn-sm"
                onClick={() => openUpdateNotes(updateInfo)}
              >
                {t("settings.view_update_notes")}
              </button>
            </div>
          )}
        </section>

        <div className="settings-save-row">
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
