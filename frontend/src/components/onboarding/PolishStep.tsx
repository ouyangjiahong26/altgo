/** 润色步：云端填供应商预设与连接信息，本地只提示可选下载本地润色模型。 */
import { Download } from "lucide-react";
import type { AppConfig } from "../../hooks/useConfigForm";
import type { PolishModelEntry } from "../../hooks/usePolishModelManager";
import { ProviderPresetSelector } from "../ProviderPresetSelector";
import { type ProviderPreset, type ModelCatalogEntry } from "../../config/modelPresets";

interface PolishStepProps {
  t: (key: string) => string;
  lang: string;
  stepLabel: (n: number) => string;
  config: AppConfig;
  update: <K extends keyof AppConfig>(key: K, value: AppConfig[K]) => void;
  catalogPresets: ProviderPreset[];
  catalogError: string | null;
  polishModels: PolishModelEntry[];
  polishDownloading: string | null;
  getPolishDownloadProgress: (name: string) => { percent: number; connecting: boolean };
  onDownloadLocalPolish: (name: string) => Promise<void>;
}

export function PolishStep({
  t, lang, stepLabel, config, update, catalogPresets, catalogError,
  polishModels, polishDownloading, getPolishDownloadProgress, onDownloadLocalPolish,
}: PolishStepProps) {
  const localActive = config.polishBackend === "local";

  return (
    <div className="onboarding-step">
      <div className="onboarding-step-label">{stepLabel(3)}</div>
      <h2 className="onboarding-title">{t("onboarding.polish_title")}</h2>
      <PolishEngineChoiceCard t={t} localActive={localActive} update={update} />
      {localActive ? (
        <LocalPolishCard
          t={t}
          config={config}
          polishModels={polishModels}
          polishDownloading={polishDownloading}
          getPolishDownloadProgress={getPolishDownloadProgress}
          onDownloadLocalPolish={onDownloadLocalPolish}
        />
      ) : (
        <div className="onboarding-card">
          <PresetAndLevelFields
            t={t} lang={lang} config={config} update={update}
            catalogPresets={catalogPresets}
          />
          <ConnectionFields t={t} config={config} update={update} />
        </div>
      )}
      {!localActive && catalogError && (
        <p className="settings-hint settings-test-err">{catalogError}</p>
      )}
    </div>
  );
}

/** 润色引擎二选一：云端与本地两个单选项。 */
function PolishEngineChoiceCard({
  t, localActive, update,
}: Pick<PolishStepProps, "t" | "update"> & { localActive: boolean }) {
  return (
    <div className="onboarding-card">
      <label className={`onboarding-choice ${localActive ? "" : "is-active"}`}>
        <input
          type="radio"
          name="onboarding-polish-engine"
          checked={!localActive}
          onChange={() => update("polishBackend", "remote")}
        />
        <span className="onboarding-choice-body">
          <span className="onboarding-choice-title">{t("onboarding.polish_engine_remote")}</span>
          <span className="onboarding-choice-desc">{t("onboarding.polish_engine_remote_desc")}</span>
        </span>
      </label>
      <label className={`onboarding-choice ${localActive ? "is-active" : ""}`}>
        <input
          type="radio"
          name="onboarding-polish-engine"
          checked={localActive}
          onChange={() => update("polishBackend", "local")}
        />
        <span className="onboarding-choice-body">
          <span className="onboarding-choice-title">{t("onboarding.polish_engine_local")}</span>
          <span className="onboarding-choice-desc">{t("onboarding.polish_engine_local_desc")}</span>
        </span>
      </label>
    </div>
  );
}

/** 本地润色只给说明与一个可选下载按钮，不自动下载。 */
function LocalPolishCard({
  t, config, polishModels, polishDownloading,
  getPolishDownloadProgress, onDownloadLocalPolish,
}: Pick<
  PolishStepProps,
  "t" | "config" | "polishModels" | "polishDownloading"
  | "getPolishDownloadProgress" | "onDownloadLocalPolish"
>) {
  const name = config.polishLocalModel;
  const entry = polishModels.find((m) => m.name === name);
  const { percent, connecting } = getPolishDownloadProgress(name);

  return (
    <div className="onboarding-card">
      <p className="settings-hint settings-hint--polish">
        {t("onboarding.polish_local_desc")}
      </p>
      <p className="settings-hint settings-hint--polish">
        {t("onboarding.polish_local_hint")}
      </p>
      <div className="settings-polish-actions">
        {polishDownloading === name ? (
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
        ) : entry?.downloaded ? (
          <span className="badge">{t("onboarding.polish_local_downloaded")}</span>
        ) : (
          <button
            type="button"
            className="btn btn-sm btn-primary"
            onClick={() => void onDownloadLocalPolish(name)}
            disabled={polishDownloading !== null}
          >
            <Download size={11} />
            {t("onboarding.polish_local_download")}
          </button>
        )}
      </div>
    </div>
  );
}

/** 供应商预设与润色级别。 */
function PresetAndLevelFields({
  t, lang, config, update, catalogPresets,
}: Pick<
  PolishStepProps,
  "t" | "lang" | "config" | "update" | "catalogPresets"
>) {
  return (
    <>
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
            className="select"
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
    </>
  );
}

/** API 连接信息：密钥、地址与模型。 */
function ConnectionFields({
  t, config, update,
}: Pick<PolishStepProps, "t" | "config" | "update">) {
  return (
    <>
      <div className="settings-field">
        <span className="settings-field-label-text">{t("settings.api_key")}</span>
        <div className="settings-field-control">
          <input
            type="password"
            className="field"
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
            className="field"
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
            className="field"
            value={config.polishModel}
            onChange={(e) => update("polishModel", e.target.value)}
            placeholder="gpt-4o-mini"
          />
        </div>
      </div>
    </>
  );
}
