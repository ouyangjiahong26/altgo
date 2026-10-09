import { ChevronDown, ChevronRight, Sparkles } from "lucide-react";
import type { Dispatch, SetStateAction } from "react";
import { ProviderPresetSelector } from "../../components/ProviderPresetSelector";
import type { ModelCatalogEntry, ProviderPreset } from "../../config/modelPresets";
import type { AppConfig } from "../../hooks/useConfigForm";

interface PolishingSectionProps {
  "data-settings-order"?: number;
  t: (key: string) => string;
  lang: string;
  config: AppConfig;
  update: <K extends keyof AppConfig>(key: K, value: AppConfig[K]) => void;
  catalogPresets: ProviderPreset[];
  catalogLoading: boolean;
  catalogError: string | null;
  onRetryCatalog: () => void;
  testing: boolean;
  testResult: { ok: boolean; error?: string } | null;
  onTestConnection: () => Promise<void>;
  clearingKey: boolean;
  onClearApiKey: () => Promise<void>;
  polishOpen: boolean;
  setPolishOpen: Dispatch<SetStateAction<boolean>>;
  polishAdvancedOpen: boolean;
  setPolishAdvancedOpen: Dispatch<SetStateAction<boolean>>;
}

/** 润色区块：供应商目录、润色档位、密钥与连通性测试、高级参数。 */
export function PolishingSection({
  "data-settings-order": order,
  t, lang, config, update,
  catalogPresets, catalogLoading, catalogError, onRetryCatalog,
  testing, testResult, onTestConnection, clearingKey, onClearApiKey,
  polishOpen, setPolishOpen, polishAdvancedOpen, setPolishAdvancedOpen,
}: PolishingSectionProps) {
  return (
    <section
      data-settings-order={order}
      className="settings-section settings-section--polishing"
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
          <PresetCatalogArea
            t={t} lang={lang} config={config} update={update}
            catalogPresets={catalogPresets} catalogLoading={catalogLoading}
            catalogError={catalogError} onRetryCatalog={onRetryCatalog}
          />
          <PolishLevelArea t={t} config={config} update={update} />
          <PolishKeyAndTestArea
            t={t} config={config} update={update}
            testing={testing} testResult={testResult}
            onTestConnection={onTestConnection} clearingKey={clearingKey}
            onClearApiKey={onClearApiKey}
          />
          <PolishAdvancedBlock
            t={t} config={config} update={update}
            polishAdvancedOpen={polishAdvancedOpen} setPolishAdvancedOpen={setPolishAdvancedOpen}
          />
        </div>
      )}
    </section>
  );
}

type PresetCatalogAreaProps = Pick<
  PolishingSectionProps,
  | "t" | "lang" | "config" | "update"
  | "catalogPresets" | "catalogLoading" | "catalogError" | "onRetryCatalog"
>;

function PresetCatalogArea({
  t, lang, config, update, catalogPresets, catalogLoading, catalogError, onRetryCatalog,
}: PresetCatalogAreaProps) {
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
            className="btn btn-sm btn-secondary"
            onClick={() => void onRetryCatalog()}
            disabled={catalogLoading}
          >
            {t("settings.catalog_retry")}
          </button>
        </>
      )}
    </>
  );
}

function PolishLevelArea({
  t,
  config,
  update,
}: Pick<PolishingSectionProps, "t" | "config" | "update">) {
  return (
    <>
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
      {(config.hasPolisherApiKey || config.polisherApiKey) && config.polishLevel === "none" && (
        <p className="settings-hint settings-hint--polish">
          {t("settings.polish_disabled_hint")}
        </p>
      )}
    </>
  );
}

type PolishKeyAndTestAreaProps = Pick<
  PolishingSectionProps,
  | "t" | "config" | "update" | "testing" | "testResult"
  | "onTestConnection" | "clearingKey" | "onClearApiKey"
>;

function PolishKeyAndTestArea({
  t, config, update, testing, testResult, onTestConnection, clearingKey, onClearApiKey,
}: PolishKeyAndTestAreaProps) {
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
      <div className="settings-polish-actions">
        <button
          type="button"
          className="btn btn-sm btn-secondary"
          onClick={onTestConnection}
          disabled={testing}
        >
          {testing ? t("settings.testing") : t("settings.test_connection")}
        </button>
        {config.hasPolisherApiKey && (
          <button
            type="button"
            className="btn btn-sm btn-secondary"
            onClick={onClearApiKey}
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
    </>
  );
}

function PolishAdvancedBlock({
  t,
  config,
  update,
  polishAdvancedOpen,
  setPolishAdvancedOpen,
}: Pick<
  PolishingSectionProps,
  "t" | "config" | "update" | "polishAdvancedOpen" | "setPolishAdvancedOpen"
>) {
  return (
    <>
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
          <PolishProtocolAndModelFields t={t} config={config} update={update} />
          <PolishUrlAndThinkingFields t={t} config={config} update={update} />
        </>
      )}
    </>
  );
}

function PolishProtocolAndModelFields({
  t,
  config,
  update,
}: Pick<PolishingSectionProps, "t" | "config" | "update">) {
  return (
    <>
      <div className="settings-field">
        <span className="settings-field-label-text">{t("settings.api_protocol")}</span>
        <div className="settings-field-control">
          <select
            className="select"
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

function PolishUrlAndThinkingFields({
  t,
  config,
  update,
}: Pick<PolishingSectionProps, "t" | "config" | "update">) {
  return (
    <>
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
        <span className="settings-field-label-text">{t("settings.thinking_level")}</span>
        <div className="settings-field-control">
          <select
            className="select"
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
  );
}
