/** 转写引擎步：本地/在线二选一，本地列模型条目，在线填 API 密钥。 */
import { Download } from "lucide-react";
import type { AppConfig } from "../../hooks/useConfigForm";
import type { ModelEntry, UseModelManagerResult } from "../../hooks/useModelManager";
import { formatSize } from "../../utils/format";
import { DownloadMeter } from "./DownloadMeter";

interface EngineStepProps {
  t: (key: string) => string;
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
  t, config, update, saving, models, downloading,
  getDownloadProgress, applyLocalModel, downloadAndUse,
}: EngineStepProps) {
  const onlineActive = config.transcriberBackend === "online";

  return (
    <div className="onboarding-step">
      <h2 className="onboarding-title">{t("onboarding.engine_title")}</h2>
      <EngineChoiceCard t={t} onlineActive={onlineActive} update={update} />
      {onlineActive ? (
        <OnlineKeyCard t={t} config={config} update={update} />
      ) : (
        <LocalModelList
          t={t} config={config} saving={saving} models={models}
          downloading={downloading} getDownloadProgress={getDownloadProgress}
          applyLocalModel={applyLocalModel} downloadAndUse={downloadAndUse}
        />
      )}
    </div>
  );
}

/** 引擎选择：本地与在线两个单选卡，直接铺在步骤里，不套外层卡片。 */
function EngineChoiceCard({
  t, onlineActive, update,
}: Pick<EngineStepProps, "t" | "update"> & { onlineActive: boolean }) {
  return (
    <div className="onboarding-choices">
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
    <div className="card onboarding-card">
      <div className="onboarding-field">
        <span className="onboarding-field-label">{t("settings.api_key")}</span>
        <div className="onboarding-field-control">
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

/** 本地引擎的模型列表。 */
function LocalModelList({
  t, config, saving, models, downloading,
  getDownloadProgress, applyLocalModel, downloadAndUse,
}: Pick<
  EngineStepProps,
  "t" | "config" | "saving" | "models" | "downloading"
  | "getDownloadProgress" | "applyLocalModel" | "downloadAndUse"
>) {
  return (
    <div className="card onboarding-card onboarding-list">
      {models.map((m) => (
        <EngineModelRow
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

interface EngineModelRowProps {
  t: (key: string) => string;
  model: ModelEntry;
  isActive: boolean;
  saving: boolean;
  downloading: string | null;
  getDownloadProgress: UseModelManagerResult["getDownloadProgress"];
  onUse: (name: string) => Promise<void>;
  onDownloadAndUse: (name: string) => Promise<void>;
}

function EngineModelRow({
  t, model: m, isActive, saving, downloading, getDownloadProgress,
  onUse, onDownloadAndUse,
}: EngineModelRowProps) {
  const { percent, connecting } = getDownloadProgress(m.name);

  return (
    <div className="onboarding-item">
      <div className="onboarding-model-info">
        <span className="onboarding-model-name">{m.name}</span>
        <span className="onboarding-model-meta">
          {formatSize(m.sizeBytes)} · {m.filename}
        </span>
      </div>
      <EngineModelAction
        t={t} model={m} isActive={isActive} saving={saving}
        downloading={downloading} percent={percent} connecting={connecting}
        onUse={onUse} onDownloadAndUse={onDownloadAndUse}
      />
    </div>
  );
}

interface EngineModelActionProps {
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

/** 行尾动作：使用中是徽标，已下载给“使用”，未下载给“下载并启用”，下载中改进度。
 * 本视图的主操作在底部（下一步），行内动作因此用中性或描边按钮，不抢反色实心。 */
function EngineModelAction({
  t, model: m, isActive, saving, downloading, percent, connecting,
  onUse, onDownloadAndUse,
}: EngineModelActionProps) {
  // 当前模型用徽标而不是禁用按钮：灰色禁用态读不出“已经在用”。
  if (isActive) return <span className="badge">{t("settings.current")}</span>;
  if (m.downloaded) {
    return (
      <button
        type="button"
        className="btn btn-sm btn-secondary"
        onClick={() => void onUse(m.name)}
        disabled={saving}
      >
        {t("settings.use_model")}
      </button>
    );
  }
  if (downloading === m.name) {
    return <DownloadMeter percent={percent} connecting={connecting} t={t} />;
  }
  return (
    <button
      type="button"
      className="btn btn-sm btn-outline"
      onClick={() => void onDownloadAndUse(m.name)}
      disabled={downloading !== null}
    >
      <Download size={13} />
      {t("settings.download_and_use")}
    </button>
  );
}
