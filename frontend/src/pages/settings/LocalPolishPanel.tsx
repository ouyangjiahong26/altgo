import { Download, Trash2 } from "lucide-react";
import type { AppConfig } from "../../hooks/useConfigForm";
import type { PolishModelEntry } from "../../hooks/usePolishModelManager";
import type { PolishRuntimeStatus } from "../../hooks/usePolishRuntime";
import { formatSize } from "../../utils/format";

export interface LocalPolishPanelProps {
  t: (key: string) => string;
  config: AppConfig;
  update: <K extends keyof AppConfig>(key: K, value: AppConfig[K]) => void;
  saving: boolean;
  models: PolishModelEntry[];
  downloading: string | null;
  getDownloadProgress: (name: string) => { percent: number; connecting: boolean };
  onUseLocalModel: (name: string) => Promise<void>;
  onDownloadAndUse: (name: string) => Promise<void>;
  onDeleteModel: (name: string) => Promise<void>;
  runtime: {
    status: PolishRuntimeStatus | null;
    downloading: boolean;
    progress: { downloaded: number; total: number };
    onDownload: () => Promise<void>;
    onRedownload: () => Promise<void>;
  };
}

/**
 * 本地润色面板：单档润色级别、本地模型卡网格与运行时安装行。
 * 与本地识别面板共用模型卡与进度条标记，本地分支下隐藏云端专属字段。
 */
export function LocalPolishPanel({
  t, config, update, saving, models, downloading, getDownloadProgress,
  onUseLocalModel, onDownloadAndUse, onDeleteModel, runtime,
}: LocalPolishPanelProps) {
  return (
    <>
      <LocalPolishLevelArea t={t} config={config} update={update} />
      <div className="settings-model-grid">
        {models.map((m) => (
          <PolishModelCard
            key={m.name}
            t={t}
            model={m}
            isActive={config.polishLocalModel === m.name}
            saving={saving}
            downloading={downloading}
            getDownloadProgress={getDownloadProgress}
            onUse={onUseLocalModel}
            onDownloadAndUse={onDownloadAndUse}
            onDelete={onDeleteModel}
          />
        ))}
      </div>
      <PolishRuntimeRow t={t} runtime={runtime} />
    </>
  );
}

/** 本地润色只提供关闭与开启两档，参数已在后端按实测调好。 */
function LocalPolishLevelArea({
  t,
  config,
  update,
}: Pick<LocalPolishPanelProps, "t" | "config" | "update">) {
  return (
    <>
      <div className="settings-field">
        <span className="settings-field-label-text">{t("settings.polish_level")}</span>
        <div className="settings-field-control">
          <select
            className="select"
            value={config.polishLevel === "none" ? "none" : "medium"}
            onChange={(e) => update("polishLevel", e.target.value)}
          >
            <option value="none">{t("settings.polish_local_level_none")}</option>
            <option value="medium">{t("settings.polish_local_level_on")}</option>
          </select>
        </div>
      </div>
      <p className="settings-hint settings-hint--polish">
        {t("settings.polish_local_level_hint")}
      </p>
    </>
  );
}

interface PolishModelCardProps {
  t: (key: string) => string;
  model: PolishModelEntry;
  isActive: boolean;
  saving: boolean;
  downloading: string | null;
  getDownloadProgress: (name: string) => { percent: number; connecting: boolean };
  onUse: (name: string) => Promise<void>;
  onDownloadAndUse: (name: string) => Promise<void>;
  onDelete: (name: string) => Promise<void>;
}

function PolishModelCard({
  t, model: m, isActive, saving, downloading, getDownloadProgress,
  onUse, onDownloadAndUse, onDelete,
}: PolishModelCardProps) {
  const { percent, connecting } = getDownloadProgress(m.name);

  return (
    <div className={`settings-model-card ${isActive ? "is-active" : ""}`}>
      <div className="settings-model-card-head">
        <span className="settings-model-card-name">{m.name}</span>
        {isActive && (
          <span className="settings-model-card-badge">{t("settings.in_use")}</span>
        )}
      </div>
      <p className="settings-model-card-desc">{m.description}</p>
      <p className="settings-model-card-meta">{formatSize(m.sizeBytes)}</p>
      <div className="settings-model-card-actions">
        {m.downloaded ? (
          <>
            <button
              type="button"
              className="btn btn-sm btn-secondary"
              onClick={() => onUse(m.name)}
              disabled={isActive || saving}
            >
              {isActive ? t("settings.current") : t("settings.use_model")}
            </button>
            <button
              type="button"
              className="btn btn-sm btn-danger"
              onClick={() => onDelete(m.name)}
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
            className="btn btn-sm btn-primary"
            onClick={() => onDownloadAndUse(m.name)}
            disabled={downloading !== null}
          >
            <Download size={11} />
            {t("settings.download_and_use")}
          </button>
        )}
      </div>
    </div>
  );
}

interface PolishRuntimeRowProps {
  t: (key: string) => string;
  runtime: LocalPolishPanelProps["runtime"];
}

/** 运行时一行：状态徽标加下载按钮，下载中显示进度条。 */
function PolishRuntimeRow({ t, runtime }: PolishRuntimeRowProps) {
  const { status, downloading, progress, onDownload, onRedownload } = runtime;
  const percent = progress.total > 0
    ? Math.round((progress.downloaded / progress.total) * 100)
    : 0;
  const connecting = progress.downloaded === 0;

  return (
    <>
      <div className="settings-field">
        <span className="settings-field-label-text">{t("settings.polish_runtime")}</span>
        <div className="settings-field-control">
          <span className="badge">{runtimeStatusText(t, status)}</span>
          {status?.supported && (
            <button
              type="button"
              className="btn btn-sm btn-secondary"
              onClick={() => void (status.installed ? onRedownload() : onDownload())}
              disabled={downloading}
            >
              {status.installed
                ? t("settings.polish_runtime_redownload")
                : t("settings.polish_runtime_download")}
            </button>
          )}
        </div>
      </div>
      {downloading && (
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
      )}
    </>
  );
}

function runtimeStatusText(
  t: (key: string) => string,
  status: PolishRuntimeStatus | null,
): string {
  if (!status) return t("settings.polish_runtime_unknown");
  if (!status.supported) return t("settings.polish_runtime_unsupported");
  if (status.running) return t("settings.polish_runtime_running");
  if (status.installed) return t("settings.polish_runtime_installed");
  return t("settings.polish_runtime_missing");
}
