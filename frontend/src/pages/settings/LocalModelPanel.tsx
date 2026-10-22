import { ChevronDown, ChevronRight, Download, Trash2 } from "lucide-react";
import { useId, type Dispatch, type SetStateAction } from "react";
import type { AppConfig } from "../../hooks/useConfigForm";
import type { ModelEntry } from "../../hooks/useModelManager";
import { formatSize } from "../../utils/format";

export interface LocalModelPanelProps {
  t: (key: string) => string;
  config: AppConfig;
  update: <K extends keyof AppConfig>(key: K, value: AppConfig[K]) => void;
  saving: boolean;
  models: ModelEntry[];
  downloading: string | null;
  getDownloadProgress: (name: string) => { percent: number; connecting: boolean };
  onUseLocalModel: (name: string) => Promise<void>;
  onDownloadAndUse: (name: string) => Promise<void>;
  onDeleteModel: (name: string) => Promise<void>;
  advancedPath: boolean;
  setAdvancedPath: Dispatch<SetStateAction<boolean>>;
}

/**
 * 本地识别面板：模型行与自定义模型路径。
 * 路径开合状态由父层持有，切换在线/本地后端再切回时保持原开合。
 */
export function LocalModelPanel({
  t, config, update, saving, models, downloading, getDownloadProgress,
  onUseLocalModel, onDownloadAndUse, onDeleteModel, advancedPath, setAdvancedPath,
}: LocalModelPanelProps) {
  return (
    <>
      <div className="settings-model-grid">
        {models.map((m) => (
          <ModelCard
            key={m.name}
            t={t}
            model={m}
            isActive={config.model === m.name}
            saving={saving}
            downloading={downloading}
            getDownloadProgress={getDownloadProgress}
            onUse={onUseLocalModel}
            onDownloadAndUse={onDownloadAndUse}
            onDelete={onDeleteModel}
          />
        ))}
      </div>
      <AdvancedModelPath
        t={t}
        config={config}
        update={update}
        advancedPath={advancedPath}
        setAdvancedPath={setAdvancedPath}
      />
    </>
  );
}

interface ModelCardProps {
  t: (key: string) => string;
  model: ModelEntry;
  isActive: boolean;
  saving: boolean;
  downloading: string | null;
  getDownloadProgress: (name: string) => { percent: number; connecting: boolean };
  onUse: (name: string) => Promise<void>;
  onDownloadAndUse: (name: string) => Promise<void>;
  onDelete: (name: string) => Promise<void>;
}

function ModelCard({
  t, model: m, isActive, saving, downloading, getDownloadProgress,
  onUse, onDownloadAndUse, onDelete,
}: ModelCardProps) {
  const { percent, connecting } = getDownloadProgress(m.name);

  return (
    <div className={`settings-model-card ${isActive ? "is-active" : ""}`}>
      <div className="settings-model-card-head">
        <span className="settings-model-card-name">{m.name}</span>
        {isActive && (
          <span className="badge badge-accent">{t("settings.in_use")}</span>
        )}
      </div>
      <p className="settings-model-card-desc">{m.description}</p>
      <p className="settings-model-card-meta">
        {formatSize(m.sizeBytes)} · {m.filename}
      </p>
      <div className="settings-model-card-actions">
        <ModelCardActions
          t={t}
          model={m}
          isActive={isActive}
          saving={saving}
          downloading={downloading}
          percent={percent}
          connecting={connecting}
          onUse={onUse}
          onDownloadAndUse={onDownloadAndUse}
          onDelete={onDelete}
        />
      </div>
    </div>
  );
}

interface ModelCardActionsProps {
  t: (key: string) => string;
  model: ModelEntry;
  isActive: boolean;
  saving: boolean;
  downloading: string | null;
  percent: number;
  connecting: boolean;
  onUse: (name: string) => Promise<void>;
  onDownloadAndUse: (name: string) => Promise<void>;
  onDelete: (name: string) => Promise<void>;
}

function ModelCardActions({
  t, model: m, isActive, saving, downloading, percent, connecting,
  onUse, onDownloadAndUse, onDelete,
}: ModelCardActionsProps) {
  return m.downloaded ? (
    <>
      <button
        type="button"
        className="btn btn-sm btn-secondary"
        onClick={() => void onUse(m.name)}
        disabled={isActive || saving}
      >
        {isActive ? t("settings.current") : t("settings.use_model")}
      </button>
      <button
        type="button"
        className="btn btn-sm btn-danger"
        onClick={() => void onDelete(m.name)}
      >
        <Trash2 size={12} aria-hidden="true" />
        {t("settings.delete_model")}
      </button>
    </>
  ) : downloading === m.name ? (
    <div className="model-progress">
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
      className="btn btn-sm btn-secondary"
      onClick={() => void onDownloadAndUse(m.name)}
      disabled={downloading !== null}
    >
      <Download size={12} aria-hidden="true" />
      {t("settings.download_and_use")}
    </button>
  );
}

function AdvancedModelPath({
  t,
  config,
  update,
  advancedPath,
  setAdvancedPath,
}: Pick<
  LocalModelPanelProps,
  "t" | "config" | "update" | "advancedPath" | "setAdvancedPath"
>) {
  const bodyId = useId();

  return (
    <>
      <button
        type="button"
        className="settings-advanced"
        aria-expanded={advancedPath}
        aria-controls={advancedPath ? bodyId : undefined}
        onClick={() => setAdvancedPath(!advancedPath)}
      >
        {advancedPath ? (
          <ChevronDown size={14} aria-hidden="true" />
        ) : (
          <ChevronRight size={14} aria-hidden="true" />
        )}
        {t("settings.advanced_model_path")}
      </button>
      {advancedPath && (
        <div className="settings-advanced-body" id={bodyId}>
          <div className="settings-field">
            <span className="settings-field-label-text">{t("settings.custom_path")}</span>
            <div className="settings-field-control">
              <input
                type="text"
                className="field"
                value={config.model}
                onChange={(e) => update("model", e.target.value)}
                placeholder={t("settings.custom_path_placeholder")}
              />
            </div>
          </div>
        </div>
      )}
    </>
  );
}
