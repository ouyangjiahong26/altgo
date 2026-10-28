import { ChevronDown, ChevronRight } from "lucide-react";
import { useId, type Dispatch, type SetStateAction } from "react";
import type { AppConfig } from "../../hooks/useConfigForm";
import {
  ASR_PROTOCOL_OPTIONS,
  ASR_PROVIDER_CUSTOM,
  ASR_PROVIDER_PRESETS,
  asrProviderValue,
} from "../../config/asrPresets";

export interface OnlineAsrPanelProps {
  t: (key: string) => string;
  config: AppConfig;
  update: <K extends keyof AppConfig>(key: K, value: AppConfig[K]) => void;
  advancedOpen: boolean;
  setAdvancedOpen: Dispatch<SetStateAction<boolean>>;
  clearingKey: boolean;
  clearError: string | null;
  onClearApiKey: () => Promise<void>;
}

/**
 * 在线识别面板：服务商预设、密钥输入、清除已存密钥与高级模型/地址/协议项。
 * 开合状态由父层持有，切换本地/在线后端再切回时保持原开合。
 */
export function OnlineAsrPanel({
  t, config, update, advancedOpen, setAdvancedOpen, clearingKey, clearError, onClearApiKey,
}: OnlineAsrPanelProps) {
  const advancedId = useId();

  const applyPreset = (id: string) => {
    const preset = ASR_PROVIDER_PRESETS.find((p) => p.id === id);
    if (!preset) return;
    update("asrApiBaseUrl", preset.apiBaseUrl);
    update("asrModel", preset.model);
    update("asrProtocol", preset.protocol);
  };

  return (
    <>
      <div className="settings-field">
        <span className="settings-field-label-text">{t("settings.asr_provider")}</span>
        <div className="settings-field-control">
          <select
            className="select"
            aria-label={t("settings.asr_provider")}
            value={asrProviderValue(config)}
            onChange={(e) => applyPreset(e.target.value)}
          >
            {ASR_PROVIDER_PRESETS.map((p) => (
              <option key={p.id} value={p.id}>
                {t(p.nameKey)}
              </option>
            ))}
            <option value={ASR_PROVIDER_CUSTOM}>
              {t("settings.asr_provider_custom")}
            </option>
          </select>
        </div>
      </div>
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
      {clearError && <p className="settings-card-note settings-test-err">{clearError}</p>}
      {config.hasAsrApiKey && (
        <div className="settings-row-actions">
          <button
            type="button"
            className="btn btn-sm btn-secondary"
            onClick={onClearApiKey}
            disabled={clearingKey}
          >
            {t("settings.clear_api_key")}
          </button>
        </div>
      )}
      <button
        type="button"
        className="settings-advanced"
        aria-expanded={advancedOpen}
        aria-controls={advancedOpen ? advancedId : undefined}
        onClick={() => setAdvancedOpen(!advancedOpen)}
      >
        {advancedOpen ? (
          <ChevronDown size={14} aria-hidden="true" />
        ) : (
          <ChevronRight size={14} aria-hidden="true" />
        )}
        {t("settings.advanced_asr")}
      </button>
      {advancedOpen && (
        <div className="settings-advanced-body" id={advancedId}>
          <AsrAdvancedFields t={t} config={config} update={update} />
        </div>
      )}
    </>
  );
}

function AsrAdvancedFields({
  t,
  config,
  update,
}: Pick<OnlineAsrPanelProps, "t" | "config" | "update">) {
  return (
    <>
      <div className="settings-field">
        <span className="settings-field-label-text">{t("settings.model")}</span>
        <div className="settings-field-control">
          <input
            type="text"
            className="field"
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
            className="field"
            value={config.asrApiBaseUrl}
            onChange={(e) => update("asrApiBaseUrl", e.target.value)}
            placeholder="https://token-plan-cn.xiaomimimo.com/v1"
          />
        </div>
      </div>
      <div className="settings-field">
        <span className="settings-field-label-text">{t("settings.asr_protocol")}</span>
        <div className="settings-field-control">
          <select
            className="select"
            aria-label={t("settings.asr_protocol")}
            value={config.asrProtocol}
            onChange={(e) => update("asrProtocol", e.target.value)}
          >
            {ASR_PROTOCOL_OPTIONS.map((o) => (
              <option key={o.value} value={o.value}>
                {t(o.labelKey)}
              </option>
            ))}
            {/* 手写 TOML 里的未知协议值原样保留，避免受控下拉错位回显。 */}
            {!ASR_PROTOCOL_OPTIONS.some((o) => o.value === config.asrProtocol) && (
              <option value={config.asrProtocol}>{config.asrProtocol}</option>
            )}
          </select>
        </div>
      </div>
    </>
  );
}
