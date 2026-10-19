/**
 * 在线识别服务商静态预设（前端表，不进配置结构）。
 *
 * 预设只做表单初始填充：选中后自动填 base URL、模型与协议三项，字段仍可手改；
 * 当前表单值与任一预设完全一致时下拉回显该预设，否则显示“自定义”。
 */

/** 在线转写协议家族，取值与后端 `[transcriber.online] protocol` 一致。 */
export type AsrProtocol = "mimo_chat" | "chat_input_audio" | "transcriptions";

export interface AsrProviderPreset {
  id: string;
  /** 服务商显示名的 i18n key */
  nameKey: string;
  apiBaseUrl: string;
  model: string;
  protocol: AsrProtocol;
}

export const ASR_PROVIDER_PRESETS: AsrProviderPreset[] = [
  {
    id: "mimo",
    nameKey: "settings.asr_provider_mimo",
    apiBaseUrl: "https://token-plan-cn.xiaomimimo.com/v1",
    model: "mimo-v2.5-asr",
    protocol: "mimo_chat",
  },
  {
    id: "zhipu",
    nameKey: "settings.asr_provider_zhipu",
    apiBaseUrl: "https://open.bigmodel.cn/api/paas/v4",
    model: "glm-asr-2512",
    protocol: "transcriptions",
  },
  {
    id: "openai",
    nameKey: "settings.asr_provider_openai",
    apiBaseUrl: "https://api.openai.com/v1",
    model: "whisper-1",
    protocol: "transcriptions",
  },
  {
    id: "groq",
    nameKey: "settings.asr_provider_groq",
    apiBaseUrl: "https://api.groq.com/openai/v1",
    model: "whisper-large-v3",
    protocol: "transcriptions",
  },
  {
    id: "siliconflow",
    nameKey: "settings.asr_provider_siliconflow",
    apiBaseUrl: "https://api.siliconflow.cn/v1",
    model: "FunAudioLLM/SenseVoiceSmall",
    protocol: "transcriptions",
  },
];

/** “自定义”项在下拉里的值：不对应任何预设，选中后不改任何字段。 */
export const ASR_PROVIDER_CUSTOM = "custom";

/** 协议三选一的展示项：value 即后端取值，label 走 i18n。 */
export const ASR_PROTOCOL_OPTIONS: { value: AsrProtocol; labelKey: string }[] = [
  { value: "mimo_chat", labelKey: "settings.asr_protocol_mimo_chat" },
  { value: "chat_input_audio", labelKey: "settings.asr_protocol_chat_input_audio" },
  { value: "transcriptions", labelKey: "settings.asr_protocol_transcriptions" },
];

/** 表单值与某预设三项全等时回显该预设 id，否则回显“自定义”。 */
export function asrProviderValue(c: {
  asrApiBaseUrl: string;
  asrModel: string;
  asrProtocol: string;
}): string {
  const hit = ASR_PROVIDER_PRESETS.find(
    (p) =>
      p.apiBaseUrl === c.asrApiBaseUrl && p.model === c.asrModel && p.protocol === c.asrProtocol,
  );
  return hit?.id ?? ASR_PROVIDER_CUSTOM;
}
