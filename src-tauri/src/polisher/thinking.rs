//! 思考参数方言判定与两家协议的思考控制字段构造。

use super::endpoint::host_matches;
use super::level::ThinkingLevel;
use super::protocol;

/// 按 host 后缀匹配的“默认开启思考”服务商表，按各家参数方言分三组。
///
/// 语音润色是轻量文本任务：默认档（`Off`）对已知默认开启思考的服务商附带关闭参数，
/// 否则思考会吃掉 `max_tokens`（可见文本为空）或凭空多出数秒延迟。未命中表的服务商
/// 保持原 body，避免严格校验未知字段的服务商（OpenAI 等）拒绝请求。
const ENABLE_THINKING_HOSTS: &[&str] = &[
    "dashscope.aliyuncs.com",
    "siliconflow.cn",
    "siliconflow.com",
];
const THINKING_TYPE_HOSTS: &[&str] = &[
    "bigmodel.cn",
    "z.ai",
    "volces.com",
    "minimaxi.com",
    "minimax.io",
    "deepseek.com",
    "moonshot.cn",
    "kimi.com",
    "kimi.ai",
    "xiaomimimo.com",
];
const REASONING_HOSTS: &[&str] = &["openrouter.ai"];

/// 服务商的思考参数方言。
#[derive(Clone, Copy, PartialEq, Eq)]
enum ThinkingDialect {
    /// `enable_thinking` 布尔开关（通义 / SiliconFlow）
    EnableThinking,
    /// `thinking: {"type": ...}`（智谱 / Kimi / DeepSeek / MiMo 等）
    ThinkingType,
    /// OpenRouter 的 `reasoning` 对象
    Reasoning,
}

/// 按 host 判定思考参数方言，未命中服务商表返回 `None`。
fn thinking_dialect(host: &str) -> Option<ThinkingDialect> {
    let h = host.to_lowercase();
    if ENABLE_THINKING_HOSTS.iter().any(|d| host_matches(&h, d)) {
        Some(ThinkingDialect::EnableThinking)
    } else if THINKING_TYPE_HOSTS.iter().any(|d| host_matches(&h, d)) {
        Some(ThinkingDialect::ThinkingType)
    } else if REASONING_HOSTS.iter().any(|d| host_matches(&h, d)) {
        Some(ThinkingDialect::Reasoning)
    } else {
        None
    }
}

/// 按 host 与思考层级返回控制思考的请求字段（OpenAI 兼容协议）。
///
/// 默认（`Off`）按方言关闭思考，用户选择层级（`Low`/`Medium`/`High`）时按各家方言开启：
/// - 通义（dashscope）/ SiliconFlow：`enable_thinking`（仅开关，不分档）
/// - 智谱 / z.ai / 火山方舟 / MiniMax / DeepSeek / Moonshot / Kimi / MiMo：
///   `thinking: {"type": "enabled"}`（仅开关，不分档）
/// - OpenRouter：`reasoning: {"effort": "<档位>"}`
/// - 未命中表的服务商（含 OpenAI 官方）：`reasoning_effort: "<档位>"`
///   （OpenAI 事实标准，用户自担模型不支持时的 400）
pub(super) fn thinking_fields(
    host: &str,
    level: ThinkingLevel,
) -> Vec<(&'static str, serde_json::Value)> {
    match (thinking_dialect(host), level) {
        (Some(ThinkingDialect::EnableThinking), ThinkingLevel::Off) => {
            vec![("enable_thinking", serde_json::json!(false))]
        }
        (Some(ThinkingDialect::EnableThinking), _) => {
            vec![("enable_thinking", serde_json::json!(true))]
        }
        (Some(ThinkingDialect::ThinkingType), ThinkingLevel::Off) => {
            vec![("thinking", serde_json::json!({ "type": "disabled" }))]
        }
        (Some(ThinkingDialect::ThinkingType), _) => {
            vec![("thinking", serde_json::json!({ "type": "enabled" }))]
        }
        (Some(ThinkingDialect::Reasoning), ThinkingLevel::Off) => {
            vec![("reasoning", serde_json::json!({ "enabled": false }))]
        }
        (Some(ThinkingDialect::Reasoning), level) => {
            vec![("reasoning", serde_json::json!({ "effort": level.as_str() }))]
        }
        (None, ThinkingLevel::Off) => Vec::new(),
        (None, level) => vec![("reasoning_effort", serde_json::json!(level.as_str()))],
    }
}

/// 按 host 与思考层级返回 Anthropic 协议的思考控制字段。
///
/// 关闭档只对默认开启思考的服务商显式发 `disabled`：Anthropic 官方思考是选择加入，其新版
/// 模型对 `disabled` 直接报 400，因此官方与未知端点保持不发字段。不发字段时这些服务商会
/// 默认思考，思考与回答共享 `max_tokens`，预算被思考吃光后可见文本为空（表现为“润色失败”），
/// 或白等数秒。
///
/// 开启档与协议无关：按档给思考预算，Anthropic 要求 `max_tokens` 大于预算、且思考与
/// `temperature` 不兼容（调用方据此抬 `max_tokens`、省 `temperature`）。
pub(super) fn anthropic_thinking(
    host: &str,
    level: ThinkingLevel,
) -> Option<protocol::AnthropicThinking> {
    let budget_tokens = match level {
        ThinkingLevel::Off => {
            return thinking_dialect(host)
                .is_some()
                .then(|| protocol::AnthropicThinking {
                    thinking_type: "disabled".to_string(),
                    budget_tokens: None,
                })
        }
        ThinkingLevel::Low => 1024,
        ThinkingLevel::Medium => 4096,
        ThinkingLevel::High => 16384,
    };
    Some(protocol::AnthropicThinking {
        thinking_type: "enabled".to_string(),
        budget_tokens: Some(budget_tokens),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 断言一组 host 在关闭档产生的字段与预期完全一致（失败信息带 host）。
    fn assert_off_fields(hosts: &[&str], expected: Vec<(&'static str, serde_json::Value)>) {
        for host in hosts {
            assert_eq!(
                thinking_fields(host, ThinkingLevel::Off),
                expected,
                "host: {host}"
            );
        }
    }

    /// 未命中表的服务商关闭档一个字段都不能发，短域名相似拼写不得误命中，host 大小写不敏感。
    fn assert_off_keeps_unmatched_hosts_clean() {
        // 不命中的服务商：一个字段都不能发
        for host in [
            "api.openai.com",
            "api.anthropic.com",
            "qianfan.baidubce.com",
            "generativelanguage.googleapis.com",
            "localhost",
            "",
        ] {
            assert!(
                thinking_fields(host, ThinkingLevel::Off).is_empty(),
                "host 不应命中：{host}"
            );
        }
        // 域名边界：短域名的相似拼写不得误命中
        assert!(thinking_fields("notz.ai", ThinkingLevel::Off).is_empty());
        assert!(thinking_fields("fake-kimi.com", ThinkingLevel::Off).is_empty());
        // host 大小写不敏感
        assert_eq!(
            thinking_fields("API.SILICONFLOW.CN", ThinkingLevel::Off),
            vec![("enable_thinking", serde_json::json!(false))]
        );
    }

    #[test]
    fn test_thinking_fields_off_matches_suppression_table() {
        // enable_thinking 系
        assert_off_fields(
            &[
                "dashscope.aliyuncs.com",
                "api.siliconflow.cn",
                "api.dashscope.aliyuncs.com",
                "api.siliconflow.com",
                "cloud.siliconflow.cn",
            ],
            vec![("enable_thinking", serde_json::json!(false))],
        );
        // thinking 系
        assert_off_fields(
            &[
                "open.bigmodel.cn",
                "api.z.ai",
                "z.ai",
                "ark.cn-beijing.volces.com",
                "api.minimaxi.com",
                "api.minimax.io",
                "api.deepseek.com",
                "api.moonshot.cn",
                "api.kimi.com",
                "kimi.ai",
                "api.xiaomimimo.com",
                "token-plan-cn.xiaomimimo.com",
            ],
            vec![("thinking", serde_json::json!({ "type": "disabled" }))],
        );
        // reasoning 系
        assert_eq!(
            thinking_fields("openrouter.ai", ThinkingLevel::Off),
            vec![("reasoning", serde_json::json!({ "enabled": false }))]
        );
        assert_off_keeps_unmatched_hosts_clean();
    }

    #[test]
    fn test_thinking_fields_levels_enable_per_dialect() {
        // 仅开关的服务商：任意层级都只是开启，不分级
        assert_eq!(
            thinking_fields("api.siliconflow.cn", ThinkingLevel::Low),
            vec![("enable_thinking", serde_json::json!(true))]
        );
        assert_eq!(
            thinking_fields("api.deepseek.com", ThinkingLevel::High),
            vec![("thinking", serde_json::json!({ "type": "enabled" }))]
        );
        // OpenRouter：层级映射 reasoning.effort
        assert_eq!(
            thinking_fields("openrouter.ai", ThinkingLevel::Low),
            vec![("reasoning", serde_json::json!({ "effort": "low" }))]
        );
        assert_eq!(
            thinking_fields("openrouter.ai", ThinkingLevel::High),
            vec![("reasoning", serde_json::json!({ "effort": "high" }))]
        );
        // 未命中表的服务商（含 OpenAI 官方）：层级映射 reasoning_effort
        for host in ["api.openai.com", "localhost", ""] {
            assert_eq!(
                thinking_fields(host, ThinkingLevel::Medium),
                vec![("reasoning_effort", serde_json::json!("medium"))],
                "host: {host}"
            );
        }
    }
}
