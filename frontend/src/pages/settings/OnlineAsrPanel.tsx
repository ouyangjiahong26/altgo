import { ChevronDown, ChevronRight } from "lucide-react";
import type { Dispatch, SetStateAction } from "react";
import type { AppConfig } from "../../hooks/useConfigForm";

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
 * 在线识别面板：密钥输入、清除已存密钥与高级模型/地址项。
 * 开合状态由父层持有，切换本地/在线后端再切回时保持原开合。
 */
export function OnlineAsrPanel({
  t, config, update, advancedOpen, setAdvancedOpen, clearingKey, clearError, onClearApiKey,
}: OnlineAsrPanelProps) {
  return (
    <>
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
      {clearError && <p className="settings-hint settings-test-err">{clearError}</p>}
      {config.hasAsrApiKey && (
        <div className="settings-polish-actions">
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
        className="settings-advanced-toggle"
        onClick={() => setAdvancedOpen(!advancedOpen)}
      >
        {advancedOpen ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
        {t("settings.advanced_asr")}
      </button>
      {advancedOpen && <AsrAdvancedFields t={t} config={config} update={update} />}
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
    </>
  );
}
