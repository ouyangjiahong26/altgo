//! Polisher API 协议类型定义。
//!
//! Polisher API protocol type definitions.

use crate::error::PolisherError;
use serde::{Deserialize, Serialize};

/// API 协议类型。
/// API protocol kind.
#[derive(Debug, Clone, Copy)]
pub enum ApiProtocol {
    /// OpenAI 兼容接口（/v1/chat/completions）
    /// OpenAI-compatible chat interface (/v1/chat/completions)
    OpenAi,
    /// Anthropic Messages 接口（/v1/messages）
    /// Anthropic Messages interface (/v1/messages)
    Anthropic,
}

impl std::str::FromStr for ApiProtocol {
    type Err = PolisherError;

    fn from_str(s: &str) -> Result<Self, PolisherError> {
        match s.to_lowercase().as_str() {
            "openai" => Ok(ApiProtocol::OpenAi),
            "anthropic" => Ok(ApiProtocol::Anthropic),
            _other => Err(PolisherError::UnknownProtocol {
                protocol: s.to_string(),
            }),
        }
    }
}

// --- OpenAI 兼容协议 ---
// --- OpenAI-compatible protocol ---

#[derive(Debug, Serialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub temperature: f32,
    pub max_tokens: u32,
    /// 按服务商附加的顶层字段（如关闭思考的参数），序列化时平铺进请求体。
    /// Vendor-specific top-level fields (e.g. parameters that disable thinking), flattened into
    /// the request body on serialization.
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub extra: Option<serde_json::Map<String, serde_json::Value>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct ChatResponse {
    pub choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
pub struct ChatChoice {
    pub message: ChatMessage,
}

// --- Anthropic 协议 ---
// --- Anthropic protocol ---

/// Anthropic Messages API 请求体。
/// Anthropic Messages API request body.
#[derive(Debug, Serialize)]
pub struct AnthropicRequest {
    pub model: String,
    pub max_tokens: u32,
    pub system: String,
    pub messages: Vec<AnthropicMessage>,
    /// 扩展思考与 `temperature` 不兼容（须为 1 或缺省）：思考开启时不发送。
    /// Extended thinking is incompatible with `temperature` (must be 1 or absent):
    /// omitted when thinking is on.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// 扩展思考参数；`None` 时不发送（Anthropic 思考默认关闭）。
    /// Extended-thinking parameter; `None` sends nothing (Anthropic thinking is opt-in).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<AnthropicThinking>,
}

/// Anthropic 扩展思考参数。
///
/// `type` 为 `enabled` 时必带 `budget_tokens`（最低 1024）；显式关闭思考时 `type` 为
/// `disabled`，此时不发送 `budget_tokens`。
///
/// Anthropic extended-thinking parameter.
///
/// `budget_tokens` (minimum 1024) accompanies `type = "enabled"`; when thinking is explicitly
/// turned off (`type = "disabled"`) no budget is sent.
#[derive(Debug, Serialize)]
pub struct AnthropicThinking {
    #[serde(rename = "type")]
    pub thinking_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub budget_tokens: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AnthropicMessage {
    pub role: String,
    pub content: String,
}

/// Anthropic Messages API 响应。
/// Anthropic Messages API response.
#[derive(Debug, Deserialize)]
pub struct AnthropicResponse {
    pub content: Vec<AnthropicContent>,
}

#[derive(Debug, Deserialize)]
pub struct AnthropicContent {
    /// 块类型（"text" / "thinking" 等）；缺省按文本块处理，兼容不填 type 的端点。
    /// Block type ("text" / "thinking" etc.); absent defaults to text, staying compatible with
    /// endpoints that omit it.
    #[serde(rename = "type", default)]
    pub block_type: String,
    /// 仅文本块有此字段；thinking 块无。
    /// Present on text blocks only; thinking blocks have none.
    #[serde(default)]
    pub text: Option<String>,
}
