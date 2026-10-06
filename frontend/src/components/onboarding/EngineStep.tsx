/** 转写引擎步：本地/在线二选一，本地列模型卡片，在线填 API 密钥。 */
import { Download } from "lucide-react";
import type { AppConfig } from "../../hooks/useConfigForm";
import type { ModelEntry, UseModelManagerResult } from "../../hooks/useModelManager";
import { formatSize } from "../../utils/format";

interface EngineStepProps {
  t: (key: string) => string;
  stepLabel: (n: number) => string;
  config: AppConfig;
  update: <K extends keyof AppConfig>(key: K, value: AppConfig[K]) => void;
  saving: boolean;
  models: ModelEntry[];
  downloading: string | null;
  getDownloadProgress: UseModelManagerResult["getDownloadProgress"];
  applyLocalModel: (name: string) => Promise<void>;
  downloadAndUse: (name: string) => Promise<void>;
}

export function EngineStep({
  t, stepLabel, config, update, saving, models, downloading,
  getDownloadProgress, applyLocalModel, downloadAndUse,
}: EngineStepProps) {
  const onlineActive = config.transcriberBackend === "online";

  return (
    <div className="onboarding-step">
      <div className="onboarding-step-label">{stepLabel(2)}</div>
      <h2 className="onboarding-title">{t("onboarding.engine_title")}</h2>
      <EngineChoiceCard t={t} onlineActive={onlineActive} update={update} />
      {onlineActive ? (
        <OnlineKeyCard t={t} config={config} update={update} />
      ) : (
        <LocalModelGrid
          t={t} config={config} saving={saving} models={models}
          downloading={downloading} getDownloadProgress={getDownloadProgress}
          applyLocalModel={applyLocalModel} downloadAndUse={downloadAndUse}
        />
      )}
    </div>
  );
}

/** 引擎选择卡片：本地与在线两个单选项。 */
function EngineChoiceCard({
  t, onlineActive, update,
}: Pick<EngineStepProps, "t" | "update"> & { onlineActive: boolean }) {
  return (
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
  );
}

/** 在线引擎的密钥卡片。 */
function OnlineKeyCard({
  t, config, update,
}: Pick<EngineStepProps, "t" | "config" | "update">) {
  return (
    <div className="onboarding-card">
      <div className="settings-field">
        <span className="settings-field-label-text">{t("settings.api_key")}</span>
        <div className="settings-field-control">
          <input
            type="password"
            className="field"
            value={config.asrApiKey}
            onChange={(e) => update("asrApiKey", e.target.value)}
            placeholder={config.hasAsrApiKey ? "sk-***" : "sk-..."}
          />
        </div>
      </div>
    </div>
  );
}

/** 本地引擎的模型卡片网格。 */
function LocalModelGrid({
  t, config, saving, models, downloading,
  getDownloadProgress, applyLocalModel, downloadAndUse,
}: Pick<
  EngineStepProps,
  "t" | "config" | "saving" | "models" | "downloading"
  | "getDownloadProgress" | "applyLocalModel" | "downloadAndUse"
>) {
  return (
    <div className="settings-model-grid">
      {models.map((m) => (
        <EngineModelCard
          key={m.name}
          t={t}
          model={m}
          isActive={config.model === m.name}
          saving={saving}
          downloading={downloading}
          getDownloadProgress={getDownloadProgress}
          onUse={applyLocalModel}
          onDownloadAndUse={downloadAndUse}
        />
      ))}
    </div>
  );
}

interface EngineModelCardProps {
  t: (key: string) => string;
  model: ModelEntry;
  isActive: boolean;
  saving: boolean;
  downloading: string | null;
  getDownloadProgress: UseModelManagerResult["getDownloadProgress"];
  onUse: (name: string) => Promise<void>;
  onDownloadAndUse: (name: string) => Promise<void>;
}

function EngineModelCard({
  t, model: m, isActive, saving, downloading, getDownloadProgress,
  onUse, onDownloadAndUse,
}: EngineModelCardProps) {
  const { percent, connecting } = getDownloadProgress(m.name);

  return (
    <div className={`settings-model-card ${isActive ? "is-active" : ""}`}>
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
        <EngineModelCardActions
          t={t} model={m} isActive={isActive} saving={saving}
          downloading={downloading} percent={percent} connecting={connecting}
          onUse={onUse} onDownloadAndUse={onDownloadAndUse}
        />
      </div>
    </div>
  );
}

interface EngineModelCardActionsProps {
  t: (key: string) => string;
  model: ModelEntry;
  isActive: boolean;
  saving: boolean;
  downloading: string | null;
  percent: number;
  connecting: boolean;
  onUse: (name: string) => Promise<void>;
  onDownloadAndUse: (name: string) => Promise<void>;
}

function EngineModelCardActions({
  t, model: m, isActive, saving, downloading, percent, connecting,
  onUse, onDownloadAndUse,
}: EngineModelCardActionsProps) {
  return m.downloaded ? (
    <button
      type="button"
      className="btn btn-sm btn-secondary"
      onClick={() => void onUse(m.name)}
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
      className="btn btn-sm btn-primary"
      onClick={() => void onDownloadAndUse(m.name)}
      disabled={downloading !== null}
    >
      <Download size={11} />
      {t("settings.download_and_use")}
    </button>
  );
}
