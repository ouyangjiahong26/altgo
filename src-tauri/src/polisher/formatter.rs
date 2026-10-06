//! LLM 润色客户端：配置构造与润色入口，HTTP 发送与重试在 `transport`。

use super::level::{PolishLevel, ThinkingLevel};
use super::prompt::{build_prompt_source_chain, get_system_prompt, SystemPromptSource};
use super::protocol;
use crate::error::PolisherError;
use reqwest::Client;
use std::time::Duration;

/// LLM 文本润色器。
///
/// 支持 OpenAI 和 Anthropic 两种 API 协议，
/// 支持指数退避重试（最多 3 次）。
///
/// System prompt 通过 `SystemPromptSource` trait 注入，`prompt_source` 为 `None` 时
/// `polish()` 内部用内置 hardcoded prompt 兜底。
pub struct LLMFormatter {
    pub(super) api_key: String,
    pub(super) api_base_url: String,
    pub(super) model: String,
    pub(super) client: Client,
    pub(super) max_retries: u32,
    pub(super) max_tokens: u32,
    pub(super) protocol: protocol::ApiProtocol,
    pub(super) temperature: f32,
    pub(super) language: String,
    pub(super) thinking: ThinkingLevel,
    pub(super) prompt_source: Option<Box<dyn SystemPromptSource>>,
}

impl Clone for LLMFormatter {
    fn clone(&self) -> Self {
        Self {
            api_key: self.api_key.clone(),
            api_base_url: self.api_base_url.clone(),
            model: self.model.clone(),
            client: self.client.clone(),
            max_retries: self.max_retries,
            max_tokens: self.max_tokens,
            protocol: self.protocol,
            temperature: self.temperature,
            language: self.language.clone(),
            thinking: self.thinking,
            prompt_source: self.prompt_source.as_ref().map(|s| s.clone_box()),
        }
    }
}

impl std::fmt::Debug for LLMFormatter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LLMFormatter")
            .field("model", &self.model)
            .field("protocol", &self.protocol)
            .field("language", &self.language)
            .finish()
    }
}

impl TryFrom<&crate::config::Config> for LLMFormatter {
    type Error = PolisherError;

    fn try_from(cfg: &crate::config::Config) -> Result<Self, Self::Error> {
        Self::from_config(&cfg.polisher, &cfg.transcriber.language)
    }
}

impl LLMFormatter {
    /// 从配置的各小节构造 LLMFormatter。
    pub fn from_config(
        polisher: &crate::config::PolisherConfig,
        language: &str,
    ) -> Result<Self, PolisherError> {
        let protocol = polisher
            .protocol
            .parse::<protocol::ApiProtocol>()
            .map_err(|_| PolisherError::UnknownProtocol {
                protocol: polisher.protocol.clone(),
            })?;
        let formatter = Self::with_config(
            polisher.api_key.clone(),
            polisher.api_base_url.clone(),
            polisher.model.clone(),
            polisher.timeout,
            polisher.max_tokens,
            protocol,
            polisher.temperature,
            language.to_string(),
        )?
        .with_thinking_level(ThinkingLevel::effective(&polisher.thinking_level));
        Ok(formatter)
    }

    /// 共享工厂：从 Config 一次性构造带全部 prompt source 的 LLMFormatter。
    ///
    /// 实时管道（`voice_pipeline::builder::build_polisher`）与 IPC handler
    /// （`cmd::polish_history_entry`）都通过此构造，确保两条路径走相同的
    /// prompt 解析：PromptStore → Custom → hardcoded fallback。
    pub fn from_config_with_sources(cfg: &crate::config::Config) -> Result<Self, PolisherError> {
        let mut formatter = Self::from_config(&cfg.polisher, &cfg.transcriber.language)?;
        formatter.prompt_source = build_prompt_source_chain(cfg);
        Ok(formatter)
    }

    #[cfg(test)]
    pub fn new(
        api_key: String,
        api_base_url: String,
        model: String,
        timeout: Duration,
    ) -> Result<Self, PolisherError> {
        Self::with_config(
            api_key,
            api_base_url,
            model,
            timeout,
            1024,
            protocol::ApiProtocol::OpenAi,
            0.3,
            "zh".to_string(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn with_config(
        api_key: String,
        api_base_url: String,
        model: String,
        timeout: Duration,
        max_tokens: u32,
        protocol: protocol::ApiProtocol,
        temperature: f32,
        language: String,
    ) -> Result<Self, PolisherError> {
        let client = Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|e| PolisherError::HttpError(format!("failed to build HTTP client: {}", e)))?;
        Ok(Self {
            api_key,
            api_base_url,
            model,
            client,
            max_retries: 3,
            max_tokens,
            protocol,
            temperature,
            language,
            thinking: ThinkingLevel::Off,
            prompt_source: None,
        })
    }

    /// 设置思考层级，默认 `Off`（自动关闭已知服务商的思考）。
    pub fn with_thinking_level(mut self, level: ThinkingLevel) -> Self {
        self.thinking = level;
        self
    }

    /// 设置 system prompt 的来源，`None` 表示使用内置 hardcoded prompt。
    pub fn with_prompt_source(mut self, source: Option<Box<dyn SystemPromptSource>>) -> Self {
        self.prompt_source = source;
        self
    }

    /// 使用 LLM 润色文本。
    ///
    /// 如果级别为 `None` 或文本为空，直接返回原文。
    /// 润色失败时返回错误。
    pub async fn polish(&self, text: &str, level: PolishLevel) -> Result<String, PolisherError> {
        if matches!(level, PolishLevel::None) || text.is_empty() {
            return Ok(text.to_string());
        }

        let system_prompt = self.resolve_system_prompt(level);
        self.request_polish(system_prompt, text).await
    }

    /// 使用 LLM 润色文本，并在 system prompt 末尾追加用户补充要求。
    ///
    /// 补充指令经 trim 后为空时等价于 [`polish`]。级别为 `None` 或文本为空时
    /// 与 [`polish`] 一致，直接返回原文。
    pub async fn polish_with_instruction(
        &self,
        text: &str,
        level: PolishLevel,
        instruction: &str,
    ) -> Result<String, PolisherError> {
        if matches!(level, PolishLevel::None) || text.is_empty() {
            return Ok(text.to_string());
        }
        let instruction = instruction.trim();
        if instruction.is_empty() {
            return self.polish(text, level).await;
        }

        let system_prompt = format!(
            "{}\n\n补充要求：{}",
            self.resolve_system_prompt(level),
            instruction
        );
        self.request_polish(system_prompt, text).await
    }

    /// 解析当前生效的 system prompt：prompt source 链（PromptStore → Custom）
    /// 失败或缺位时回落到内置 hardcoded prompt。
    fn resolve_system_prompt(&self, level: PolishLevel) -> String {
        self.prompt_source
            .as_ref()
            .and_then(|s| s.get_prompt(level, &self.language).ok())
            .unwrap_or_else(|| get_system_prompt(level, &self.language))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_polish_success_with_versioned_base_url() {
        // 预设式 base 自带 /v1：endpoint 应为 /v1/chat/completions 而非 /v1/v1/...
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(200)
            .with_body(mock_success_response("ok"))
            .create_async()
            .await;

        let formatter = LLMFormatter::new(
            "test-key".to_string(),
            format!("{}/v1", server.url()),
            "moonshot-v1-8k".to_string(),
            Duration::from_secs(5),
        )
        .unwrap();
        let result = formatter
            .polish("原始文本", PolishLevel::Medium)
            .await
            .unwrap();
        assert_eq!(result, "ok");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_polish_none_skips_api() {
        let formatter = LLMFormatter::new(
            "key".to_string(),
            "http://localhost".to_string(),
            "model".to_string(),
            Duration::from_secs(5),
        )
        .unwrap();
        let result = formatter.polish("hello", PolishLevel::None).await.unwrap();
        assert_eq!(result, "hello");
    }

    #[tokio::test]
    async fn test_polish_empty_skips_api() {
        let formatter = LLMFormatter::new(
            "key".to_string(),
            "http://localhost".to_string(),
            "model".to_string(),
            Duration::from_secs(5),
        )
        .unwrap();
        let result = formatter.polish("", PolishLevel::Medium).await.unwrap();
        assert_eq!(result, "");
    }

    #[tokio::test]
    async fn test_polish_with_instruction_appends_instruction_to_system_prompt() {
        let mut server = mockito::Server::new_async().await;
        let expected_system = format!(
            "{}\n\n补充要求：{}",
            get_system_prompt(PolishLevel::Medium, "zh"),
            "语气更客气"
        );
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .match_body(mockito::Matcher::PartialJsonString(
                serde_json::json!({
                    "messages": [
                        {"role": "system", "content": expected_system},
                        {"role": "user", "content": "原始文本"}
                    ]
                })
                .to_string(),
            ))
            .with_status(200)
            .with_body(mock_success_response("润色后的文本"))
            .create_async()
            .await;

        let formatter = LLMFormatter::new(
            "key".to_string(),
            server.url(),
            "model".to_string(),
            Duration::from_secs(5),
        )
        .unwrap();
        let result = formatter
            .polish_with_instruction("原始文本", PolishLevel::Medium, "  语气更客气  ")
            .await
            .unwrap();
        assert_eq!(result, "润色后的文本");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_polish_with_instruction_blank_instruction_sends_plain_prompt() {
        // 空白指令 trim 后视同未提供：system prompt 与普通 polish 完全一致。
        // matcher 精确匹配 system content，若误加“补充要求”后缀则断言失败。
        let mut server = mockito::Server::new_async().await;
        let expected_system = get_system_prompt(PolishLevel::Medium, "zh");
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .match_body(mockito::Matcher::PartialJsonString(
                serde_json::json!({
                    "messages": [
                        {"role": "system", "content": expected_system},
                        {"role": "user", "content": "原始文本"}
                    ]
                })
                .to_string(),
            ))
            .with_status(200)
            .with_body(mock_success_response("润色后的文本"))
            .create_async()
            .await;

        let formatter = LLMFormatter::new(
            "key".to_string(),
            server.url(),
            "model".to_string(),
            Duration::from_secs(5),
        )
        .unwrap();
        let result = formatter
            .polish_with_instruction("原始文本", PolishLevel::Medium, "   ")
            .await
            .unwrap();
        assert_eq!(result, "润色后的文本");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_polish_with_instruction_none_level_returns_original() {
        let formatter = LLMFormatter::new(
            "key".to_string(),
            "http://localhost".to_string(),
            "model".to_string(),
            Duration::from_secs(5),
        )
        .unwrap();
        let result = formatter
            .polish_with_instruction("hello", PolishLevel::None, "更正式")
            .await
            .unwrap();
        assert_eq!(result, "hello");
    }

    fn mock_success_response(content: &str) -> String {
        serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": content
                }
            }]
        })
        .to_string()
    }

    #[tokio::test]
    async fn test_polish_success() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .match_header("Authorization", "Bearer test-key")
            .with_status(200)
            .with_body(mock_success_response("润色后的文本"))
            .create_async()
            .await;

        let formatter = LLMFormatter::new(
            "test-key".to_string(),
            server.url(),
            "deepseek-chat".to_string(),
            Duration::from_secs(5),
        )
        .unwrap();
        let result = formatter
            .polish("原始文本", PolishLevel::Medium)
            .await
            .unwrap();
        assert_eq!(result, "润色后的文本");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_polish_sends_correct_prompt_for_light() {
        let mut server = mockito::Server::new_async().await;
        let expected_system = get_system_prompt(PolishLevel::Light, "zh");
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .match_body(mockito::Matcher::PartialJsonString(
                serde_json::json!({
                    "messages": [
                        {"role": "system", "content": expected_system},
                        {"role": "user", "content": "test"}
                    ]
                })
                .to_string(),
            ))
            .with_status(200)
            .with_body(mock_success_response("ok"))
            .create_async()
            .await;

        let formatter = LLMFormatter::with_config(
            "key".to_string(),
            server.url(),
            "model".to_string(),
            Duration::from_secs(5),
            1024,
            protocol::ApiProtocol::OpenAi,
            0.3,
            "zh".to_string(),
        )
        .unwrap();
        let _ = formatter.polish("test", PolishLevel::Light).await;
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_polish_unknown_host_body_has_no_thinking_fields() {
        // mockito 的 host（127.0.0.1）不命中思考抑制表：请求体必须与
        // ChatRequest 完全一致，不携带任何额外字段（OpenAI 等会拒绝未知参数）。
        let mut server = mockito::Server::new_async().await;
        let expected_body = serde_json::json!({
            "model": "m",
            "messages": [
                {"role": "system", "content": get_system_prompt(PolishLevel::Light, "zh")},
                {"role": "user", "content": "test"}
            ],
            "temperature": 0.3,
            "max_tokens": 1024,
        });
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .match_body(mockito::Matcher::Json(expected_body))
            .with_status(200)
            .with_body(mock_success_response("ok"))
            .create_async()
            .await;

        let formatter = LLMFormatter::new(
            "key".to_string(),
            server.url(),
            "m".to_string(),
            Duration::from_secs(5),
        )
        .unwrap();
        let _ = formatter.polish("test", PolishLevel::Light).await;
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_polish_thinking_level_sends_reasoning_effort() {
        // 未命中 host 表的端点 + 层级开启：请求体应带 reasoning_effort（OpenAI 事实标准）。
        let mut server = mockito::Server::new_async().await;
        let expected_body = serde_json::json!({
            "model": "m",
            "messages": [
                {"role": "system", "content": get_system_prompt(PolishLevel::Light, "zh")},
                {"role": "user", "content": "test"}
            ],
            "temperature": 0.3,
            "max_tokens": 1024,
            "reasoning_effort": "low",
        });
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .match_body(mockito::Matcher::Json(expected_body))
            .with_status(200)
            .with_body(mock_success_response("ok"))
            .create_async()
            .await;

        let formatter = LLMFormatter::new(
            "key".to_string(),
            server.url(),
            "m".to_string(),
            Duration::from_secs(5),
        )
        .unwrap()
        .with_thinking_level(ThinkingLevel::Low);
        let _ = formatter.polish("test", PolishLevel::Light).await;
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_polish_anthropic_thinking_budgets_and_raises_max_tokens() {
        // Anthropic + 层级开启：body 带 thinking 预算，max_tokens 抬升“预算 + 配置值”，
        // 且不发 temperature（与扩展思考不兼容）。精确匹配整个 body，防止 temperature 漏发。
        let mut server = mockito::Server::new_async().await;
        let expected_body = serde_json::json!({
            "model": "m",
            "messages": [
                {"role": "user", "content": "test"}
            ],
            "max_tokens": 1024 + 4096,
            "system": get_system_prompt(PolishLevel::Medium, "zh"),
            "thinking": {"type": "enabled", "budget_tokens": 4096},
        });
        let mock = server
            .mock("POST", "/v1/messages")
            .match_body(mockito::Matcher::Json(expected_body))
            .with_status(200)
            .with_body(mock_anthropic_response("ok"))
            .create_async()
            .await;

        let formatter = LLMFormatter::with_config(
            "key".to_string(),
            server.url(),
            "m".to_string(),
            Duration::from_secs(5),
            1024,
            protocol::ApiProtocol::Anthropic,
            0.3,
            "zh".to_string(),
        )
        .unwrap()
        .with_thinking_level(ThinkingLevel::Medium);
        let _ = formatter.polish("test", PolishLevel::Medium).await;
        mock.assert_async().await;
    }

    fn mock_anthropic_response(content: &str) -> String {
        serde_json::json!({
            "content": [{"text": content}]
        })
        .to_string()
    }

    #[tokio::test]
    async fn test_polish_anthropic_success() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/messages")
            .match_header("x-api-key", "anthropic-key")
            .match_header("Authorization", "Bearer anthropic-key")
            .match_header("anthropic-version", "2023-06-01")
            .with_status(200)
            .with_body(mock_anthropic_response("润色后的文本"))
            .create_async()
            .await;

        let formatter = LLMFormatter::with_config(
            "anthropic-key".to_string(),
            server.url(),
            "claude-3-5-sonnet".to_string(),
            Duration::from_secs(5),
            1024,
            protocol::ApiProtocol::Anthropic,
            0.3,
            "zh".to_string(),
        )
        .unwrap();
        let result = formatter
            .polish("原始文本", PolishLevel::Medium)
            .await
            .unwrap();
        assert_eq!(result, "润色后的文本");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_polish_strips_inline_think_block() {
        // 中转端点把思维链以 <think> 块内联进正文：出口应剥掉。
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(200)
            .with_body(mock_success_response("<think>推理过程</think>\n\n净文本"))
            .create_async()
            .await;

        let formatter = LLMFormatter::new(
            "key".to_string(),
            server.url(),
            "model".to_string(),
            Duration::from_secs(5),
        )
        .unwrap();
        let result = formatter
            .polish("原始文本", PolishLevel::Medium)
            .await
            .unwrap();
        assert_eq!(result, "净文本");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_polish_anthropic_skips_thinking_block() {
        // 响应首块为 thinking 块（无 text 字段）：取第一个文本块而不是报错。
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/messages")
            .with_status(200)
            .with_body(
                serde_json::json!({
                    "content": [
                        {"type": "thinking", "thinking": "推理过程"},
                        {"type": "text", "text": "净文本"}
                    ]
                })
                .to_string(),
            )
            .create_async()
            .await;

        let formatter = LLMFormatter::with_config(
            "key".to_string(),
            server.url(),
            "claude-3-5-sonnet".to_string(),
            Duration::from_secs(5),
            1024,
            protocol::ApiProtocol::Anthropic,
            0.3,
            "zh".to_string(),
        )
        .unwrap();
        let result = formatter
            .polish("原始文本", PolishLevel::Medium)
            .await
            .unwrap();
        assert_eq!(result, "净文本");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_polish_retry_429_then_succeeds() {
        let mut server = mockito::Server::new_async().await;
        let error_mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(429)
            .expect(2)
            .create_async()
            .await;
        let success_mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(200)
            .with_body(mock_success_response("finally ok"))
            .create_async()
            .await;

        let formatter = LLMFormatter::new(
            "key".to_string(),
            server.url(),
            "model".to_string(),
            Duration::from_secs(5),
        )
        .unwrap();
        let result = formatter.polish("test", PolishLevel::Medium).await.unwrap();
        assert_eq!(result, "finally ok");
        error_mock.assert_async().await;
        success_mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_polish_retries_exhausted_returns_last_error() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(503)
            .with_body("unavailable")
            .expect(3)
            .create_async()
            .await;

        let formatter = LLMFormatter::new(
            "key".to_string(),
            server.url(),
            "model".to_string(),
            Duration::from_secs(5),
        )
        .unwrap();
        let result = formatter.polish("test", PolishLevel::Medium).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            PolisherError::ApiError { status: 503, .. }
        ));
    }

    #[tokio::test]
    async fn test_polish_transient_failure_then_succeeds() {
        let mut server = mockito::Server::new_async().await;
        let error_mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(503)
            .create_async()
            .await;
        let success_mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(200)
            .with_body(mock_success_response("recovered"))
            .create_async()
            .await;

        let formatter = LLMFormatter::new(
            "key".to_string(),
            server.url(),
            "model".to_string(),
            Duration::from_secs(5),
        )
        .unwrap();
        let result = formatter.polish("test", PolishLevel::Medium).await.unwrap();
        assert_eq!(result, "recovered");
        error_mock.assert_async().await;
        success_mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_polish_anthropic_429_returns_rate_limited() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("POST", "/v1/messages")
            .with_status(429)
            .expect(3)
            .create_async()
            .await;

        let formatter = LLMFormatter::with_config(
            "key".to_string(),
            server.url(),
            "model".to_string(),
            Duration::from_secs(5),
            1024,
            protocol::ApiProtocol::Anthropic,
            0.3,
            "zh".to_string(),
        )
        .unwrap();
        let result = formatter.polish("test", PolishLevel::Medium).await;
        assert!(matches!(result.unwrap_err(), PolisherError::RateLimited));
    }

    #[tokio::test]
    async fn test_polish_anthropic_empty_response() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("POST", "/v1/messages")
            .with_status(200)
            .with_body(serde_json::json!({"content": []}).to_string())
            .create_async()
            .await;

        let formatter = LLMFormatter::with_config(
            "key".to_string(),
            server.url(),
            "model".to_string(),
            Duration::from_secs(5),
            1024,
            protocol::ApiProtocol::Anthropic,
            0.3,
            "zh".to_string(),
        )
        .unwrap();
        let result = formatter.polish("test", PolishLevel::Medium).await;
        assert!(matches!(result.unwrap_err(), PolisherError::EmptyResponse));
    }
}
