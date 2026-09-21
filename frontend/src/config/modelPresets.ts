/**
 * 模型预设配置。
 *
 * 当前只保留文本润色供应商预设；本地转写使用 SenseVoice，无云端供应商。
 */

export type ModelType = "polisher";

export type ProviderCategory =
  | "official"      // 官方（OpenAI、Anthropic）
  | "cn_official"   // 国产官方（DeepSeek、Kimi、智谱等）
  | "aggregator"    // 聚合服务（OpenRouter、SiliconFlow 等）
  | "third_party"   // 第三方中转
  | "custom";       // 自定义

export interface ModelCatalogEntry {
  model: string;
  displayName: string;
  description?: string;
  contextWindow?: number;
  inputModalities?: ("text" | "audio" | "image")[];
  recommended?: boolean;
}

export interface ProviderPreset {
  /** 供应商名称 */
  name: string;
  /** i18n key */
  nameKey?: string;
  /** 官网链接 */
  websiteUrl: string;
  /** 获取 API Key 的链接 */
  apiKeyUrl?: string;
  /** API Base URL */
  apiBaseUrl: string;
  /** 分类 */
  category: ProviderCategory;
  /** 支持的模型类型 */
  modelTypes: ModelType[];
  /** 推荐模型目录 */
  models: ModelCatalogEntry[];
  /** 默认模型 */
  defaultModel: string;
  /** API 协议格式 */
  apiFormat: "openai" | "anthropic";
  /** 图标名称（用于 UI 展示） */
  icon?: string;
  /** 图标颜色 */
  iconColor?: string;
  /** 是否为合作伙伴 */
  isPartner?: boolean;
  /** 置顶合作伙伴 */
  primePartner?: boolean;
  /** 说明文本 i18n key */
  descriptionKey?: string;
}


export const categoryOrder: ProviderCategory[] = [
  "official",
  "cn_official",
  "aggregator",
  "third_party",
  "custom",
];

export const categoryLabels: Record<ProviderCategory, string> = {
  official: "官方",
  cn_official: "国产",
  aggregator: "聚合服务",
  third_party: "第三方中转",
  custom: "本地/自定义",
};

export const categoryLabelsEn: Record<ProviderCategory, string> = {
  official: "Official",
  cn_official: "Chinese",
  aggregator: "Aggregator",
  third_party: "Third-party",
  custom: "Local/Custom",
};
