/** 首次安装引导：五步向导（欢迎 → 触发键 → 转写引擎 → 润色 → 完成）。
 * First-run wizard: welcome → trigger key → engine → polishing → done.
 * 复用设置页的表单钩子、模型管理与供应商选择器，不另造一套配置逻辑。
 * Reuses the settings form hook, model manager and provider picker instead of a
 * second configuration path. */
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Check, Download, Keyboard, Sparkles } from "lucide-react";
import { useTranslation } from "../i18n";
import { useConfigForm } from "../hooks/useConfigForm";
import { useModelManager } from "../hooks/useModelManager";
import { ProviderPresetSelector } from "./ProviderPresetSelector";
import { loadCatalog } from "../config/catalog";
import { type ProviderPreset, type ModelCatalogEntry } from "../config/modelPresets";
import { KEY_PRESETS, isPresetKeyName, presetSelectValue } from "../config/keyPresets";
import { formatSize } from "../utils/format";
import { completeOnboarding } from "../onboarding";

type Step = "welcome" | "key" | "engine" | "polish" | "done";

const STEPS: Step[] = ["welcome", "key", "engine", "polish", "done"];

/** 润色级别 → i18n key：完成页摘要显示级别名用。 */
/* Polish level → i18n key, used by the done-step summary. */
const POLISH_LEVEL_KEYS: Record<string, string> = {
  none: "settings.polish_none",
  light: "settings.polish_light",
  medium: "settings.polish_medium",
  heavy: "settings.polish_heavy",
};

export default function Onboarding({ onDone }: { onDone: () => void }) {
  const { t, lang, setLang } = useTranslation();
  const [stepIndex, setStepIndex] = useState(0);
  const [finishing, setFinishing] = useState(false);
  const [catalogPresets, setCatalogPresets] = useState<ProviderPreset[]>([]);
  const [catalogError, setCatalogError] = useState<string | null>(null);

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
  const { models, downloading, getDownloadProgress } = modelMgr;

  const step = STEPS[stepIndex];
  const onlineActive = config?.transcriberBackend === "online";

  useEffect(() => {
    loadCatalog()
      .then(setCatalogPresets)
      .catch((e) => setCatalogError(String(e)));
  }, []);

  // 保存成功后收尾；保存失败则留在完成步并显示表单错误。
  // Finish only after a successful save; on failure stay on the step and show the error.
  useEffect(() => {
    if (!finishing || saving) return;
    if (message === "saved") {
      completeOnboarding();
      onDone();
    } else if (message !== "") {
      setFinishing(false);
    }
  }, [finishing, saving, message, onDone]);

  const goto = (index: number) => {
    setStepIndex(Math.max(0, Math.min(STEPS.length - 1, index)));
  };

  // 只写模型字段：下载在 model-download-finished 之后才结束，可能晚于向导关闭；整份回写会把
  // 期间（含向导结束后在设置页）保存的配置覆盖回旧值。逐字段窄补丁与清除密钥走同一条路径。
  // Write only the model field: the download finishes after model-download-finished, possibly after
  // the wizard closed; a whole-config write would revert whatever was saved meanwhile.
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

  const finish = async () => {
    setFinishing(true);
    await save();
  };

  const startDragging = () => {
    try {
      getCurrentWindow().startDragging();
    } catch {
      // ignore
    }
  };

  /** 步号只数配置步（触发键 / 转写引擎 / 润色，共 3 步）；欢迎页与完成页不编号。 */
  /* Only configuration steps are numbered (key / engine / polishing = 3); welcome and done are not. */
  const stepLabel = (n: number) =>
    t("onboarding.step_label").replace("{n}", String(n));

  const nextLabel =
    step === "done"
      ? t("onboarding.start")
      : step === "polish" && config?.polishLevel === "none"
        ? t("onboarding.polish_skip")
        : t("onboarding.next");

  const handleNext = () => {
    if (step === "done") {
      void finish();
      return;
    }
    goto(stepIndex + 1);
  };

  if (!config) {
    return (
      <div className="onboarding">
        <div className="onboarding-body">{t("settings.loading")}</div>
      </div>
    );
  }

  return (
    <div className="onboarding">
      <div className="onboarding-drag" data-tauri-drag-region onMouseDown={startDragging} />
      <div className="onboarding-body">
        {step === "welcome" && (
          <div className="onboarding-step">
            <img src="/altgo-logo.svg" alt="" width={52} height={52} className="onboarding-logo" />
            <h2 className="onboarding-title">{t("onboarding.welcome_title")}</h2>
            <p className="onboarding-text">{t("onboarding.welcome_text")}</p>
          </div>
        )}

        {step === "key" && (
          <div className="onboarding-step">
            <div className="onboarding-step-label">{stepLabel(1)}</div>
            <h2 className="onboarding-title">{t("onboarding.key_title")}</h2>
            <div className="onboarding-card">
              <div className="settings-field">
                <span className="settings-field-label-text">{t("settings.key_name")}</span>
                <div className="settings-field-control">
                  <select
                    className="settings-select"
                    value={presetSelectValue(config.keyName)}
                    onChange={(e) => {
                      if (e.target.value === "__custom__") return;
                      setConfig((prev) =>
                        prev
                          ? { ...prev, keyName: e.target.value, linuxEvdevCode: null }
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
                            ? { ...prev, keyName: e.target.value, linuxEvdevCode: null }
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
                    {keyCapturing
                      ? t("settings.capture_waiting")
                      : t("settings.capture_activation_short")}
                  </button>
                </div>
              </div>
            </div>
          </div>
        )}

        {step === "engine" && (
          <div className="onboarding-step">
            <div className="onboarding-step-label">{stepLabel(2)}</div>
            <h2 className="onboarding-title">{t("onboarding.engine_title")}</h2>
            <div className="onboarding-card">
              <label className={`onboarding-choice ${onlineActive ? "" : "is-active"}`}>
                <input
                  type="radio"
                  name="onboarding-engine"
                  checked={!onlineActive}
                  onChange={() => update("transcriberBackend", "local")}
                />
                <span className="onboarding-choice-body">
                  <span className="onboarding-choice-title">{t("onboarding.engine_local")}</span>
                  <span className="onboarding-choice-desc">{t("onboarding.engine_local_desc")}</span>
                </span>
              </label>
              <label className={`onboarding-choice ${onlineActive ? "is-active" : ""}`}>
                <input
                  type="radio"
                  name="onboarding-engine"
                  checked={onlineActive}
                  onChange={() => update("transcriberBackend", "online")}
                />
                <span className="onboarding-choice-body">
                  <span className="onboarding-choice-title">{t("onboarding.engine_online")}</span>
                  <span className="onboarding-choice-desc">{t("onboarding.engine_online_desc")}</span>
                </span>
              </label>
            </div>

            {onlineActive ? (
              <div className="onboarding-card">
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
              </div>
            ) : (
              <div className="settings-model-grid">
                {models.map((m) => {
                  const isActive = config.model === m.name;
                  const { percent, connecting } = getDownloadProgress(m.name);
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
                      <p className="settings-model-card-meta">
                        {formatSize(m.sizeBytes)} · {m.filename}
                      </p>
                      <div className="settings-model-card-actions">
                        {m.downloaded ? (
                          <button
                            type="button"
                            className="settings-btn settings-btn-sm settings-btn-secondary"
                            onClick={() => void applyLocalModel(m.name)}
                            disabled={isActive || saving}
                          >
                            {isActive ? t("settings.current") : t("settings.use_model")}
                          </button>
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
                            onClick={() => void downloadAndUse(m.name)}
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
            )}
          </div>
        )}

        {step === "polish" && (
          <div className="onboarding-step">
            <div className="onboarding-step-label">{stepLabel(3)}</div>
            <h2 className="onboarding-title">{t("onboarding.polish_title")}</h2>
            <div className="onboarding-card">
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
            </div>
            {catalogError && (
              <p className="settings-hint settings-test-err">{catalogError}</p>
            )}
          </div>
        )}

        {step === "done" && (
          <div className="onboarding-step">
            <div className="onboarding-done-icon">
              <Check size={26} />
            </div>
            <h2 className="onboarding-title">{t("onboarding.done_title")}</h2>
            <div className="onboarding-summary">
              <div className="settings-field">
                <span className="settings-field-label-text">{t("settings.key_name")}</span>
                <div className="settings-field-control">
                  <span className="onboarding-summary-value">{config.keyName}</span>
                </div>
              </div>
              <div className="settings-field">
                <span className="settings-field-label-text">{t("settings.transcriber_backend")}</span>
                <div className="settings-field-control">
                  <span className="onboarding-summary-value">
                    {onlineActive
                      ? t("settings.transcriber_backend_online")
                      : t("settings.transcriber_backend_local")}
                  </span>
                </div>
              </div>
              <div className="settings-field">
                <span className="settings-field-label-text">{t("settings.polishing")}</span>
                <div className="settings-field-control">
                  <span className="onboarding-summary-value">
                    {config.polishLevel === "none"
                      ? t("settings.polish_none")
                      : [config.polishModel, t(POLISH_LEVEL_KEYS[config.polishLevel] ?? "settings.polish_none")]
                          .filter(Boolean)
                          .join(" · ")}
                  </span>
                </div>
              </div>
            </div>
            {message && message !== "saved" && (
              <p className="settings-hint settings-test-err">{message}</p>
            )}
          </div>
        )}
      </div>

      <div className="onboarding-foot">
        <div className="onboarding-dots">
          {STEPS.map((s, i) => (
            <span key={s} className={`onboarding-dot ${i === stepIndex ? "on" : ""}`} />
          ))}
        </div>
        <button
          type="button"
          className="settings-btn settings-btn-secondary"
          onClick={() => goto(stepIndex - 1)}
          disabled={saving}
          style={{ visibility: stepIndex === 0 ? "hidden" : "visible" }}
        >
          {t("onboarding.prev")}
        </button>
        <button
          type="button"
          className="settings-btn settings-btn-primary"
          onClick={handleNext}
          disabled={saving}
        >
          <Sparkles size={13} />
          {saving ? t("settings.saving") : nextLabel}
        </button>
      </div>
    </div>
  );
}
