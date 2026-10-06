/** 润色步：供应商预设、润色级别与 API 连接信息。 */
import type { AppConfig } from "../../hooks/useConfigForm";
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
}

export function PolishStep({
  t,
  lang,
  stepLabel,
  config,
  update,
  catalogPresets,
  catalogError,
}: PolishStepProps) {
  return (
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
      </div>
      {catalogError && (
        <p className="settings-hint settings-test-err">{catalogError}</p>
      )}
    </div>
  );
}
