import { Sparkles } from "lucide-react";
import type { AppConfig } from "../../hooks/useConfigForm";
import { LocalModelPanel, type LocalModelPanelProps } from "./LocalModelPanel";
import { OnlineAsrPanel, type OnlineAsrPanelProps } from "./OnlineAsrPanel";

interface TranscriptionSectionProps {
  "data-settings-order"?: number;
  t: (key: string) => string;
  config: AppConfig;
  update: <K extends keyof AppConfig>(key: K, value: AppConfig[K]) => void;
  /** 在线识别面板的状态与回调，与 t/config/update 合并后传给 OnlineAsrPanel。 */
  onlineAsr: Omit<OnlineAsrPanelProps, "t" | "config" | "update">;
  /** 本地模型面板的状态与回调，与 t/config/update 合并后传给 LocalModelPanel。 */
  localModel: Omit<LocalModelPanelProps, "t" | "config" | "update">;
}

/** 转写区块：识别语言、后端切换、后端专属面板与可选文本注入。 */
export function TranscriptionSection({
  "data-settings-order": order,
  t,
  config,
  update,
  onlineAsr,
  localModel,
}: TranscriptionSectionProps) {
  // 在线后端只需配置密钥即可用。本地面板由模型卡与错误事件自行表达就绪状态。
  const onlineActive = config.transcriberBackend === "online";

  return (
    <section
      data-settings-order={order}
      className="settings-section settings-section--primary settings-section--transcription"
    >
      <h3 className="settings-section-title">
        <Sparkles size={14} />
        {t("settings.transcription")}
      </h3>

      <>
        <LanguageBackendFields t={t} config={config} update={update} />
        {onlineActive ? (
          <OnlineAsrPanel t={t} config={config} update={update} {...onlineAsr} />
        ) : (
          <LocalModelPanel t={t} config={config} update={update} {...localModel} />
        )}
        <InjectTextField t={t} config={config} update={update} />
      </>
    </section>
  );
}

function LanguageBackendFields({
  t,
  config,
  update,
}: Pick<TranscriptionSectionProps, "t" | "config" | "update">) {
  return (
    <>
      <div className="settings-field">
        <span className="settings-field-label-text">{t("settings.language")}</span>
        <div className="settings-field-control settings-field-control--narrow">
          <select
            className="select"
            value={config.language}
            onChange={(e) => update("language", e.target.value)}
          >
            <option value="zh">{t("settings.language_zh")}</option>
            <option value="en">{t("settings.language_en")}</option>
            <option value="">{t("settings.language_auto")}</option>
            {/* 旧配置或手写 TOML 里的其他取值（如 yue/ja/ko/auto）原样保留，
                否则受控下拉会显示成“中文”，与后端实际使用的语言不符。 */}
            {!["zh", "en", ""].includes(config.language) && (
              <option value={config.language}>{config.language}</option>
            )}
          </select>
        </div>
      </div>

      <div className="settings-field">
        <span className="settings-field-label-text">
          {t("settings.transcriber_backend")}
        </span>
        <div className="settings-field-control">
          <select
            className="select"
            value={config.transcriberBackend === "online" ? "online" : "local"}
            onChange={(e) => update("transcriberBackend", e.target.value)}
          >
            <option value="local">{t("settings.transcriber_backend_local")}</option>
            <option value="online">{t("settings.transcriber_backend_online")}</option>
          </select>
        </div>
      </div>
    </>
  );
}

function InjectTextField({
  t,
  config,
  update,
}: Pick<TranscriptionSectionProps, "t" | "config" | "update">) {
  return (
    <div className="settings-field">
      <span className="settings-field-label-text">{t("settings.inject_text")}</span>
      <div className="settings-field-control">
        <label style={{ display: "flex", alignItems: "center", cursor: "pointer", gap: "var(--space-1-5)" }}>
          <input
            type="checkbox"
            checked={config.injectText}
            onChange={(e) => update("injectText", e.target.checked)}
          />
        </label>
      </div>
    </div>
  );
}
