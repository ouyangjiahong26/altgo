//! HTTP 传输层：指数退避重试、按协议组装请求体、发送请求与解析响应。

use super::cleanup::strip_thinking_tags;
use super::endpoint::{build_endpoint, host_of};
use super::formatter::LLMFormatter;
use super::level::ThinkingLevel;
use super::protocol;
use super::thinking::{anthropic_thinking, thinking_fields};
use crate::error::PolisherError;
use std::time::Duration;

/// 重试延迟基数（毫秒），用于指数退避计算。
const RETRY_BASE_DELAY_MS: u64 = 500;

/// 指数退避的通用重试助手。
///
/// 对给定的异步操作最多重试 `max_retries` 次。
/// 不可重试的错误（401、403）立即返回。
async fn retry_with_backoff<F, Fut, T>(
    max_retries: u32,
    mut operation: F,
) -> Result<T, PolisherError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, PolisherError>>,
{
    let mut last_err = None;
    for attempt in 0..max_retries {
        if attempt > 0 {
            let delay = Duration::from_millis(RETRY_BASE_DELAY_MS * 2u64.pow(attempt - 1));
            tokio::time::sleep(delay).await;
        }

        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                // 检查不可重试的鉴权错误
                if matches!(
                    e,
                    PolisherError::ApiError { status: 401, .. }
                        | PolisherError::ApiError { status: 403, .. }
                ) {
                    return Err(e);
                }
                tracing::warn!(attempt, error = %e, "request failed");
                last_err = Some(e);
            }
        }
    }

    Err(last_err.unwrap_or(PolisherError::RetriesExhausted))
}

impl LLMFormatter {
    /// 按当前协议把 `system_prompt` + 待润色文本发给 LLM（含重试），
    /// 返回剥除思维链残渣后的润色结果。
    pub(super) async fn request_polish(
        &self,
        system_prompt: String,
        text: &str,
    ) -> Result<String, PolisherError> {
        let polished = retry_with_backoff(self.max_retries, || async {
            match self.protocol {
                protocol::ApiProtocol::OpenAi => {
                    let body = protocol::ChatRequest {
                        model: self.model.clone(),
                        messages: vec![
                            protocol::ChatMessage {
                                role: "system".to_string(),
                                content: system_prompt.clone(),
                            },
                            protocol::ChatMessage {
                                role: "user".to_string(),
                                content: text.to_string(),
                            },
                        ],
                        temperature: self.temperature,
                        max_tokens: self.max_tokens,
                        extra: None,
                    };
                    self.do_openai_request(body).await
                }
                protocol::ApiProtocol::Anthropic => {
                    let body = self.anthropic_body(system_prompt.clone(), text);
                    self.do_anthropic_request(&body).await
                }
            }
        })
        .await?;
        // 未命中抑制表的端点（如中转站）可能把思维链以 <think> 块混进正文，
        // 润色出口统一剥掉并去除首尾空白，避免污染剪贴板。
        Ok(strip_thinking_tags(&polished).trim().to_string())
    }

    /// 组装 Anthropic 协议请求体：Anthropic 扩展思考的关闭/开启、`max_tokens` 抬升与
    /// `temperature` 取舍都在这里。
    ///
    /// 关闭档对默认开启思考的服务商发 `disabled`（它们与回答共享 `max_tokens`，不关就会吃掉
    /// 全部预算）。对官方与未知端点不发字段（其思考是选择加入，新模型对 `disabled` 报 400）。
    /// 开启档按档给预算，此时 `max_tokens` 需大于预算并抬升配置值，且不发与思考不兼容的
    /// `temperature`。
    fn anthropic_body(&self, system_prompt: String, text: &str) -> protocol::AnthropicRequest {
        let thinking = anthropic_thinking(host_of(&self.api_base_url), self.thinking);
        let thinking_on = self.thinking != ThinkingLevel::Off;
        let max_tokens = if thinking_on {
            self.max_tokens.saturating_add(
                thinking
                    .as_ref()
                    .and_then(|t| t.budget_tokens)
                    .unwrap_or_default(),
            )
        } else {
            self.max_tokens
        };
        let temperature = if thinking_on {
            None
        } else {
            Some(self.temperature)
        };
        protocol::AnthropicRequest {
            model: self.model.clone(),
            max_tokens,
            system: system_prompt,
            messages: vec![protocol::AnthropicMessage {
                role: "user".to_string(),
                content: text.to_string(),
            }],
            temperature,
            thinking,
        }
    }

    /// 按服务商与思考层级附加控制思考的字段。层级关闭时不命中的服务商
    /// 保持原始 body，避免严格校验未知字段的服务商（OpenAI 等）拒绝请求。
    /// 经 `extra` 平铺序列化而非 to_value 中转，保持 f32 字段的紧凑表示。
    fn apply_thinking_fields(&self, body: &mut protocol::ChatRequest) {
        let fields = thinking_fields(host_of(&self.api_base_url), self.thinking);
        if fields.is_empty() {
            return;
        }
        let mut extra = serde_json::Map::new();
        for (key, value) in fields {
            extra.insert(key.to_string(), value);
        }
        body.extra = Some(extra);
    }

    async fn do_openai_request(
        &self,
        mut body: protocol::ChatRequest,
    ) -> Result<String, PolisherError> {
        let url = build_endpoint(&self.api_base_url, protocol::ApiProtocol::OpenAi)?;
        self.apply_thinking_fields(&mut body);

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body)
            .send()
            .await
            .map_err(|e| PolisherError::HttpError(e.to_string()))?;
        let resp = ensure_success(resp).await?;

        let chat_resp: protocol::ChatResponse = resp
            .json()
            .await
            .map_err(|e| PolisherError::JsonError(e.to_string()))?;
        chat_resp
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .ok_or(PolisherError::EmptyResponse)
    }

    async fn do_anthropic_request(
        &self,
        body: &protocol::AnthropicRequest,
    ) -> Result<String, PolisherError> {
        let url = build_endpoint(&self.api_base_url, protocol::ApiProtocol::Anthropic)?;
        let resp = self
            .client
            .post(&url)
            // 双鉴权头：官方 Anthropic 用 x-api-key，多数兼容端点/中转用
            // Bearer token 各取所需，多余的头双方都会忽略。
            .header("x-api-key", &self.api_key)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("anthropic-version", "2023-06-01")
            .json(body)
            .send()
            .await
            .map_err(|e| PolisherError::HttpError(e.to_string()))?;
        let resp = ensure_success(resp).await?;

        let anthropic_resp: protocol::AnthropicResponse = resp
            .json()
            .await
            .map_err(|e| PolisherError::JsonError(e.to_string()))?;
        // 取第一个文本块：响应混入 thinking 块时它在最前（无 text 字段），
        // 不填 type 的中转端点按文本块处理。
        anthropic_resp
            .content
            .into_iter()
            .find(|b| b.block_type.is_empty() || b.block_type == "text")
            .and_then(|b| b.text)
            .ok_or(PolisherError::EmptyResponse)
    }
}

/// 校验响应状态：429 映射为限流错误，其余非成功状态读出错误体原文。
async fn ensure_success(resp: reqwest::Response) -> Result<reqwest::Response, PolisherError> {
    let status = resp.status().as_u16();
    if status == 429 {
        return Err(PolisherError::RateLimited);
    }
    if !resp.status().is_success() {
        let resp_body = resp
            .text()
            .await
            .unwrap_or_else(|_| "failed to read error body".to_string());
        return Err(PolisherError::ApiError {
            status,
            body: resp_body,
        });
    }
    Ok(resp)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 用同一组参数构造 Anthropic 协议 formatter（host 取自 base URL，不发请求）。
    fn anthropic_formatter(base_url: &str) -> LLMFormatter {
        LLMFormatter::with_config(
            "key".to_string(),
            base_url.to_string(),
            "m".to_string(),
            Duration::from_secs(5),
            1024,
            protocol::ApiProtocol::Anthropic,
            0.3,
            "zh".to_string(),
        )
        .unwrap()
    }

    #[test]
    fn test_anthropic_body_disables_thinking_for_default_thinking_vendors() {
        // 关闭档 + 默认开启思考的 Anthropic 兼容端点：请求体显式 disabled，且不抬 max_tokens、
        // 照发 temperature：不关思考时模型与回答共享 max_tokens，预算被吃光后可见文本为空。
        for base_url in [
            "https://api.deepseek.com/anthropic",
            "https://api.xiaomimimo.com",
        ] {
            let formatter = anthropic_formatter(base_url);
            let body =
                serde_json::to_value(formatter.anthropic_body("sys".to_string(), "原文")).unwrap();
            assert_eq!(
                body["thinking"],
                serde_json::json!({ "type": "disabled" }),
                "base_url: {base_url}"
            );
            assert_eq!(body["max_tokens"], 1024, "base_url: {base_url}");
            assert!(
                body.get("temperature").is_some(),
                "关闭档应照发 temperature：{base_url}"
            );
        }
    }

    #[test]
    fn test_anthropic_body_keeps_official_and_unknown_hosts_untouched() {
        // Anthropic 官方思考是选择加入、新模型对 disabled 报 400，未知端点则可能严格校验字段：
        // 关闭档一个 thinking 字段都不发。
        for base_url in ["https://api.anthropic.com", "https://relay.example.com/v1"] {
            let formatter = anthropic_formatter(base_url);
            let body =
                serde_json::to_value(formatter.anthropic_body("sys".to_string(), "原文")).unwrap();
            assert!(body.get("thinking").is_none(), "base_url: {base_url}");
            assert_eq!(body["max_tokens"], 1024, "base_url: {base_url}");
            assert!(
                body.get("temperature").is_some(),
                "关闭档应照发 temperature：{base_url}"
            );
        }
    }

    #[test]
    fn test_anthropic_body_enables_thinking_with_budget() {
        // 开启档与 host 无关：按档给预算，max_tokens 抬到“预算 + 配置值”，不发 temperature
        // （与扩展思考不兼容）。
        let formatter = anthropic_formatter("https://api.deepseek.com/anthropic")
            .with_thinking_level(ThinkingLevel::Medium);
        let body =
            serde_json::to_value(formatter.anthropic_body("sys".to_string(), "原文")).unwrap();
        assert_eq!(
            body["thinking"],
            serde_json::json!({ "type": "enabled", "budget_tokens": 4096 })
        );
        assert_eq!(body["max_tokens"], 1024 + 4096);
        assert!(body.get("temperature").is_none());
    }
}
