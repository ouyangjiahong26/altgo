import { ChevronDown, ChevronRight, Sparkles } from "lucide-react";
import { useId, type Dispatch, type SetStateAction } from "react";
import { ProviderPresetSelector } from "../../components/ProviderPresetSelector";
import type { ModelCatalogEntry, ProviderPreset } from "../../config/modelPresets";
import type { AppConfig } from "../../hooks/useConfigForm";
import { LocalPolishPanel, type LocalPolishPanelProps } from "./LocalPolishPanel";
import { CollapsibleSectionHead, sectionNote } from "./SectionHead";

interface PolishingSectionProps {
  "data-settings-order"?: number;
  t: (key: string) => string;
  lang: string;
  config: AppConfig;
  update: <K extends keyof AppConfig>(key: K, value: AppConfig[K]) => void;
  /** 切换润色引擎并立即持久化（窄补丁，只写润色后端字段）。 */
  setPolishBackend: (backend: string) => void;
  /** 本地引擎面板的状态与回调，与 t/config/update 合并后传给 LocalPolishPanel。 */
  localPolish: Omit<LocalPolishPanelProps, "t" | "config" | "update">;
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
  setPolishBackend, localPolish,
  catalogPresets, catalogLoading, catalogError, onRetryCatalog,
  testing, testResult, onTestConnection, clearingKey, onClearApiKey,
  polishOpen, setPolishOpen, polishAdvancedOpen, setPolishAdvancedOpen,
}: PolishingSectionProps) {
  const localActive = config.polishBackend === "local";
  const bodyId = useId();

  return (
    <section
      data-settings-order={order}
      className="settings-section settings-section--polishing"
    >
      <CollapsibleSectionHead
        leading={<Sparkles size={14} aria-hidden="true" />}
        title={t("settings.polishing")}
        note={sectionNote(t, "settings.polishing_note")}
        expanded={polishOpen}
        controls={bodyId}
        onToggle={() => setPolishOpen(!polishOpen)}
      />
      {polishOpen && (
        <div className="card settings-rows" id={bodyId}>
          <PolishEngineField
            t={t} config={config} setPolishBackend={setPolishBackend}
          />
          {localActive ? (
            <LocalPolishPanel t={t} config={config} update={update} {...localPolish} />
          ) : (
            <>
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
            </>
          )}
        </div>
      )}
    </section>
  );
}

/** 润色引擎二选一：云端保留供应商预设与密钥，本地走模型与运行时。 */
function PolishEngineField({
  t,
  config,
  setPolishBackend,
}: Pick<PolishingSectionProps, "t" | "config" | "setPolishBackend">) {
  const labelId = useId();

  return (
    <div className="settings-field">
      <span className="settings-field-label-text" id={labelId}>
        {t("settings.polish_backend")}
      </span>
      <div className="settings-field-control">
        <select
          className="select"
          aria-labelledby={labelId}
          value={config.polishBackend === "local" ? "local" : "remote"}
          onChange={(e) => setPolishBackend(e.target.value)}
        >
          <option value="remote">{t("settings.polish_backend_remote")}</option>
          <option value="local">{t("settings.polish_backend_local")}</option>
        </select>
      </div>
    </div>
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
        <p className="settings-card-note">{t("settings.catalog_loading")}</p>
      )}
      {catalogError && (
        <>
          <p className="settings-card-note settings-test-err">{catalogError}</p>
          <div className="settings-row-actions">
            <button
              type="button"
              className="btn btn-sm btn-secondary"
              onClick={() => void onRetryCatalog()}
              disabled={catalogLoading}
            >
              {t("settings.catalog_retry")}
            </button>
          </div>
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
  const labelId = useId();

  return (
    <>
      <div className="settings-field">
        <span className="settings-field-label-text" id={labelId}>
          {t("settings.polish_level")}
        </span>
        <div className="settings-field-control">
          <select
            className="select"
            aria-labelledby={labelId}
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
        <p className="settings-card-note">{t("settings.polish_disabled_hint")}</p>
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
  const labelId = useId();

  return (
    <>
      <div className="settings-field">
        <span className="settings-field-label-text" id={labelId}>
          {t("settings.api_key")}
        </span>
        <div className="settings-field-control">
          <input
            type="password"
            className="field"
            aria-labelledby={labelId}
            value={config.polisherApiKey}
            onChange={(e) => update("polisherApiKey", e.target.value)}
            placeholder={config.hasPolisherApiKey ? "sk-***" : "sk-..."}
          />
        </div>
      </div>
      <div className="settings-row-actions">
        {testResult?.ok && (
          <span className="badge badge-success">{t("settings.test_ok")}</span>
        )}
        <button
          type="button"
          className="btn btn-sm btn-secondary"
          onClick={() => void onTestConnection()}
          disabled={testing}
        >
          {testing ? t("settings.testing") : t("settings.test_connection")}
        </button>
        {config.hasPolisherApiKey && (
          <button
            type="button"
            className="btn btn-sm btn-ghost"
            onClick={() => void onClearApiKey()}
            disabled={clearingKey}
          >
            {t("settings.clear_api_key")}
          </button>
        )}
      </div>
      {testResult && !testResult.ok && (
        <p className="settings-card-note settings-test-err">{testResult.error}</p>
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
  const bodyId = useId();

  return (
    <>
      <button
        type="button"
        className="settings-advanced"
        aria-expanded={polishAdvancedOpen}
        aria-controls={polishAdvancedOpen ? bodyId : undefined}
        onClick={() => setPolishAdvancedOpen(!polishAdvancedOpen)}
      >
        {polishAdvancedOpen ? (
          <ChevronDown size={14} aria-hidden="true" />
        ) : (
          <ChevronRight size={14} aria-hidden="true" />
        )}
        {t("settings.advanced_polish")}
      </button>
      {polishAdvancedOpen && (
        <div className="settings-advanced-body" id={bodyId}>
          <PolishProtocolAndModelFields t={t} config={config} update={update} />
          <PolishUrlAndThinkingFields t={t} config={config} update={update} />
        </div>
      )}
    </>
  );
}

function PolishProtocolAndModelFields({
  t,
  config,
  update,
}: Pick<PolishingSectionProps, "t" | "config" | "update">) {
  const protocolId = useId();
  const modelId = useId();

  return (
    <>
      <div className="settings-field">
        <span className="settings-field-label-text" id={protocolId}>
          {t("settings.api_protocol")}
        </span>
        <div className="settings-field-control">
          <select
            className="select"
            aria-labelledby={protocolId}
            value={config.polishProtocol === "anthropic" ? "anthropic" : "openai"}
            onChange={(e) => update("polishProtocol", e.target.value)}
          >
            <option value="openai">{t("settings.api_protocol_openai")}</option>
            <option value="anthropic">{t("settings.api_protocol_anthropic")}</option>
          </select>
        </div>
      </div>
      <div className="settings-field">
        <span className="settings-field-label-text" id={modelId}>
          {t("settings.model")}
        </span>
        <div className="settings-field-control">
          <input
            type="text"
            className="field"
            aria-labelledby={modelId}
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
  const urlId = useId();
  const thinkingId = useId();

  return (
    <>
      <div className="settings-field">
        <span className="settings-field-label-text" id={urlId}>
          {t("settings.api_url")}
        </span>
        <div className="settings-field-control">
          <input
            type="text"
            className="field"
            aria-labelledby={urlId}
            value={config.polishApiBaseUrl}
            onChange={(e) => update("polishApiBaseUrl", e.target.value)}
            placeholder="https://api.openai.com"
          />
        </div>
      </div>
      <div className="settings-field">
        <span className="settings-field-label-text" id={thinkingId}>
          {t("settings.thinking_level")}
        </span>
        <div className="settings-field-control">
          <select
            className="select"
            aria-labelledby={thinkingId}
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
