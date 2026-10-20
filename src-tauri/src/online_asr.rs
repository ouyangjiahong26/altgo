//! 在线 ASR 转写器：按协议家族分派。
//!
//! 三个协议共享同一实现（ADR-0013）：
//! - `mimo_chat`：MiMo 网关方言的 chat/completions + input_audio，请求携带
//!   私有 `asr_options` 扩展，语言经 `asr_options.language` 传递；
//! - `chat_input_audio`：纯 OpenAI 形状的 chat/completions + input_audio，
//!   不带 `asr_options`（百炼 qwen3-asr-flash、OpenAI audio-preview 等）；
//! - `transcriptions`：OpenAI `/audio/transcriptions` multipart（whisper、
//!   智谱 glm-asr、SiliconFlow SenseVoiceSmall 云端版等）。
//!
//! 失败策略：直接报错，不回退本地。

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use base64::Engine as _;

use crate::error::TranscriberError;
use crate::polisher::protocol::{ApiProtocol, ChatResponse};
use crate::transcriber::{TranscribeResult, Transcriber};

/// 在线转写协议家族。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnlineProtocol {
    /// MiMo 网关方言：chat + input_audio + 私有 asr_options。
    MimoChat,
    /// 纯 OpenAI 形状：chat + input_audio，不带 asr_options。
    ChatInputAudio,
    /// OpenAI transcriptions：multipart file + model，响应取 text 字段。
    Transcriptions,
}

impl OnlineProtocol {
    /// 从配置字符串解析；非法值返回错误。
    pub fn parse(value: &str) -> Result<Self, TranscriberError> {
        match value.trim().to_lowercase().as_str() {
            "mimo_chat" => Ok(Self::MimoChat),
            "chat_input_audio" => Ok(Self::ChatInputAudio),
            "transcriptions" => Ok(Self::Transcriptions),
            _ => Err(TranscriberError::InvalidProtocol(value.to_string())),
        }
    }
}

/// 由 base URL 推导 transcriptions endpoint，规则与润色器 `build_endpoint` 同源：
/// 路径已以 `/audio/transcriptions` 结尾时视为完整 endpoint；路径为空补
/// `/v1/audio/transcriptions`（OpenAI SDK 默认约定）；否则仅补请求路径，
/// 版本段（`/v1`、`/api/paas/v4` 等）由 base 自带。
fn build_transcriptions_endpoint(base_url: &str) -> Result<String, TranscriberError> {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err(TranscriberError::InvalidBaseUrl(base_url.to_string()));
    }
    let path_start = match trimmed.find("://") {
        Some(scheme_end) => trimmed[scheme_end + 3..]
            .find('/')
            .map(|p| scheme_end + 3 + p),
        None => trimmed.find('/'),
    };
    let path = path_start.map(|i| &trimmed[i..]).unwrap_or("");
    if path.ends_with("/audio/transcriptions") {
        Ok(trimmed.to_string())
    } else if path.is_empty() {
        Ok(format!("{trimmed}/v1/audio/transcriptions"))
    } else {
        Ok(format!("{trimmed}/audio/transcriptions"))
    }
}

/// 在线转写器：构造时确定协议与 endpoint，转写时按协议分派请求形状。
pub struct OnlineAsr {
    client: reqwest::Client,
    protocol: OnlineProtocol,
    endpoint: String,
    api_key: String,
    model: String,
    language: String,
}

impl OnlineAsr {
    /// 从在线 ASR 配置构造；不发任何网络请求。
    ///
    /// `protocol` 取配置字符串（见 [`OnlineProtocol::parse`]）；
    /// chat 族端点推导复用润色器 `build_endpoint`，transcriptions 族按
    /// base 补 `/audio/transcriptions` 路径。
    pub fn new(
        protocol: &str,
        api_key: &str,
        api_base_url: &str,
        model: &str,
        language: &str,
        timeout: Duration,
    ) -> Result<Self, TranscriberError> {
        let protocol = OnlineProtocol::parse(protocol)?;
        let endpoint = match protocol {
            OnlineProtocol::Transcriptions => build_transcriptions_endpoint(api_base_url)?,
            _ => crate::polisher::build_endpoint(api_base_url, ApiProtocol::OpenAi)
                .map_err(|_| TranscriberError::InvalidBaseUrl(api_base_url.to_string()))?,
        };
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|e| TranscriberError::HttpError(e.to_string()))?;
        Ok(Self {
            client,
            protocol,
            endpoint,
            api_key: api_key.to_string(),
            model: model.to_string(),
            language: language.to_string(),
        })
    }

    /// 组装 chat 族请求体。
    ///
    /// `mimo_chat`：网关硬约束 `content` 只允许一个 `input_audio` 块，带文本块
    /// 会被 400 拒绝（text prompt 由网关注入）；语言为空时传 "auto"。
    /// `chat_input_audio`：不带 `asr_options` 的纯 OpenAI 形状，语言非空时作为
    /// 顶层字段携带、空值不传（由服务端自动检测）。
    fn chat_request_body(&self, audio_b64: String) -> serde_json::Value {
        let language = self.language.trim();
        let mut body = serde_json::json!({
            "model": self.model,
            "messages": [{
                "role": "user",
                "content": [{
                    "type": "input_audio",
                    "input_audio": { "data": audio_b64, "format": "wav" }
                }]
            }]
        });
        match self.protocol {
            OnlineProtocol::MimoChat => {
                body["asr_options"] = serde_json::json!({
                    "language": if language.is_empty() { "auto" } else { language }
                });
            }
            OnlineProtocol::ChatInputAudio => {
                if !language.is_empty() {
                    body["language"] = serde_json::json!(language);
                }
            }
            OnlineProtocol::Transcriptions => unreachable!("chat_request_body 仅服务 chat 族协议"),
        }
        body
    }
}

impl Transcriber for OnlineAsr {
    fn backend(&self) -> &'static str {
        "online"
    }

    fn transcribe<'life0, 'life1>(
        &'life0 self,
        audio: &'life1 [u8],
        on_progress: Arc<dyn Fn(f32) + Send + Sync>,
    ) -> Pin<Box<dyn Future<Output = Result<TranscribeResult, TranscriberError>> + Send + 'life0>>
    where
        'life1: 'life0,
    {
        Box::pin(async move {
            if audio.is_empty() {
                return Err(TranscriberError::EmptyAudio);
            }
            // 交互式听写不做重试：快速失败反馈更好，超时由 client 承担。
            let resp = match self.protocol {
                OnlineProtocol::Transcriptions => {
                    let mut form = reqwest::multipart::Form::new()
                        .text("model", self.model.clone())
                        .part(
                            "file",
                            reqwest::multipart::Part::bytes(audio.to_vec())
                                .file_name("audio.wav")
                                .mime_str("audio/wav")
                                .map_err(|e| TranscriberError::HttpError(e.to_string()))?,
                        );
                    let language = self.language.trim();
                    if !language.is_empty() {
                        form = form.text("language", language.to_string());
                    }
                    self.client
                        .post(&self.endpoint)
                        .bearer_auth(&self.api_key)
                        .multipart(form)
                        .send()
                        .await
                }
                _ => {
                    let audio_b64 = base64::engine::general_purpose::STANDARD.encode(audio);
                    let body = self.chat_request_body(audio_b64);
                    self.client
                        .post(&self.endpoint)
                        .bearer_auth(&self.api_key)
                        .json(&body)
                        .send()
                        .await
                }
            }
            .map_err(|e| TranscriberError::HttpError(e.to_string()))?;

            let status = resp.status().as_u16();
            let resp_text = resp
                .text()
                .await
                .map_err(|e| TranscriberError::HttpError(e.to_string()))?;

            if !(200..300).contains(&status) {
                // 错误 body 截断，防止超大响应刷爆悬浮窗。
                let mut body_text = resp_text;
                if body_text.chars().count() > 500 {
                    body_text = body_text.chars().take(500).collect();
                }
                return Err(TranscriberError::ApiError {
                    status,
                    body: body_text,
                });
            }

            // chat 族：标准 chat.completion 响应，文本在 choices[0].message.content；
            // transcriptions 族：文本在顶层 text 字段。
            // 响应合法但没有可用内容（空 choices 或空 text）时与润色器的
            // EmptyResponse 同口径，交由上层统一按“空结果”处理。
            let text = match self.protocol {
                OnlineProtocol::Transcriptions => {
                    #[derive(serde::Deserialize)]
                    struct TranscriptionsResponse {
                        text: String,
                    }
                    let parsed: TranscriptionsResponse = serde_json::from_str(&resp_text)
                        .map_err(|e| TranscriberError::JsonError(e.to_string()))?;
                    parsed.text.trim().to_string()
                }
                _ => {
                    let parsed: ChatResponse = serde_json::from_str(&resp_text)
                        .map_err(|e| TranscriberError::JsonError(e.to_string()))?;
                    parsed
                        .choices
                        .first()
                        .map(|c| c.message.content.trim().to_string())
                        .unwrap_or_default()
                }
            };
            if text.is_empty() {
                return Err(TranscriberError::EmptyResponse);
            }

            (on_progress)(1.0);
            Ok(TranscribeResult {
                text,
                language: self.language.clone(),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::Matcher;

    const TIMEOUT: Duration = Duration::from_secs(5);

    #[test]
    fn test_protocol_parse_accepts_known_values_and_rejects_unknown() {
        assert_eq!(
            OnlineProtocol::parse("mimo_chat").unwrap(),
            OnlineProtocol::MimoChat
        );
        assert_eq!(
            OnlineProtocol::parse(" Chat_Input_Audio ").unwrap(),
            OnlineProtocol::ChatInputAudio
        );
        assert_eq!(
            OnlineProtocol::parse("transcriptions").unwrap(),
            OnlineProtocol::Transcriptions
        );
        assert!(matches!(
            OnlineProtocol::parse("mimo"),
            Err(TranscriberError::InvalidProtocol(_))
        ));
    }

    #[tokio::test]
    async fn test_mimo_chat_posts_audio_with_asr_options() {
        let mut server = mockito::Server::new_async().await;
        // server.url() 无路径：顺带验证 build_endpoint 补全 /v1/chat/completions。
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .match_header("authorization", "Bearer test-key")
            .match_body(Matcher::PartialJsonString(
                r#"{"model": "mimo-v2.5-asr", "asr_options": {"language": "zh"}, "messages": [{"content": [{"type": "input_audio"}]}]}"#
                    .to_string(),
            ))
            .with_status(200)
            .with_body(r#"{"choices":[{"message":{"role":"assistant","content":"你好世界"}}]}"#)
            .create_async()
            .await;

        let asr = OnlineAsr::new(
            "mimo_chat",
            "test-key",
            &server.url(),
            "mimo-v2.5-asr",
            "zh",
            TIMEOUT,
        )
        .unwrap();
        let result = asr
            .transcribe(&[0u8, 1, 2, 3], Arc::new(|_| {}))
            .await
            .unwrap();
        assert_eq!(result.text, "你好世界");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_mimo_chat_empty_language_sends_auto() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .match_body(Matcher::PartialJsonString(
                r#"{"asr_options": {"language": "auto"}}"#.to_string(),
            ))
            .with_status(200)
            .with_body(r#"{"choices":[{"message":{"role":"assistant","content":"ok"}}]}"#)
            .create_async()
            .await;

        let asr = OnlineAsr::new(
            "mimo_chat",
            "k",
            &server.url(),
            "mimo-v2.5-asr",
            "",
            TIMEOUT,
        )
        .unwrap();
        asr.transcribe(&[1], Arc::new(|_| {})).await.unwrap();
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_chat_input_audio_omits_asr_options_and_empty_language() {
        let mut server = mockito::Server::new_async().await;
        // 正常路径的兜底 mock：命中即返回合法响应。
        let succeed = server
            .mock("POST", "/v1/chat/completions")
            .match_body(Matcher::PartialJsonString(
                r#"{"model": "qwen3-asr-flash", "messages": [{"content": [{"type": "input_audio"}]}]}"#
                    .to_string(),
            ))
            .with_status(200)
            .with_body(r#"{"choices":[{"message":{"role":"assistant","content":"你好"}}]}"#)
            .create_async()
            .await;
        // 反向断言：asr_options 与 language 键都不应出现在请求体里。
        let absent = server
            .mock("POST", "/v1/chat/completions")
            .match_body(Matcher::Regex(r#""asr_options"|"language""#.to_string()))
            .expect(0)
            .create_async()
            .await;

        let asr = OnlineAsr::new(
            "chat_input_audio",
            "k",
            &server.url(),
            "qwen3-asr-flash",
            "",
            TIMEOUT,
        )
        .unwrap();
        let result = asr.transcribe(&[1], Arc::new(|_| {})).await.unwrap();
        assert_eq!(result.text, "你好");
        succeed.assert_async().await;
        absent.assert_async().await;
    }

    #[tokio::test]
    async fn test_chat_input_audio_nonempty_language_sends_top_level_field() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .match_body(Matcher::PartialJsonString(
                r#"{"model": "gpt-4o-audio-preview", "language": "zh"}"#.to_string(),
            ))
            .with_status(200)
            .with_body(r#"{"choices":[{"message":{"role":"assistant","content":"你好"}}]}"#)
            .create_async()
            .await;

        let asr = OnlineAsr::new(
            "chat_input_audio",
            "k",
            &server.url(),
            "gpt-4o-audio-preview",
            "zh",
            TIMEOUT,
        )
        .unwrap();
        asr.transcribe(&[1], Arc::new(|_| {})).await.unwrap();
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_transcriptions_posts_multipart_and_parses_text() {
        let mut server = mockito::Server::new_async().await;
        // base 无路径：验证补全 /v1/audio/transcriptions。
        let mock = server
            .mock("POST", "/v1/audio/transcriptions")
            .match_header("authorization", "Bearer test-key")
            .match_header(
                "content-type",
                Matcher::Regex(r#"^multipart/form-data"#.to_string()),
            )
            .match_body(Matcher::AllOf(vec![
                Matcher::Regex(r#"name="model""#.to_string()),
                Matcher::Regex(r#"whisper-1"#.to_string()),
                Matcher::Regex(r#"name="language""#.to_string()),
                Matcher::Regex(r#"zh"#.to_string()),
                Matcher::Regex(r#"name="file"; filename="audio.wav""#.to_string()),
            ]))
            .with_status(200)
            .with_body(r#"{"text":"你好世界"}"#)
            .create_async()
            .await;

        let asr = OnlineAsr::new(
            "transcriptions",
            "test-key",
            &server.url(),
            "whisper-1",
            "zh",
            TIMEOUT,
        )
        .unwrap();
        let result = asr
            .transcribe(&[0u8, 1, 2, 3], Arc::new(|_| {}))
            .await
            .unwrap();
        assert_eq!(result.text, "你好世界");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_transcriptions_base_with_version_path_appends_only_request_path() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/api/paas/v4/audio/transcriptions")
            .with_status(200)
            .with_body(r#"{"text":"你好"}"#)
            .create_async()
            .await;
        // base 已带版本段 /api/paas/v4：只补 /audio/transcriptions。
        let asr = OnlineAsr::new(
            "transcriptions",
            "k",
            &format!("{}/api/paas/v4", server.url()),
            "glm-asr-2512",
            "",
            TIMEOUT,
        )
        .unwrap();
        let result = asr.transcribe(&[1], Arc::new(|_| {})).await.unwrap();
        assert_eq!(result.text, "你好");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_transcriptions_empty_language_omits_field() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/audio/transcriptions")
            .match_body(Matcher::Regex(r#"name="language""#.to_string()))
            .expect(0)
            .create_async()
            .await;
        // 主体走正常路径；若 language 字段被携带，expect(0) 的 mock 会在断言时失败。
        let _succeed = server
            .mock("POST", "/v1/audio/transcriptions")
            .with_status(200)
            .with_body(r#"{"text":"ok"}"#)
            .create_async()
            .await;

        let asr = OnlineAsr::new(
            "transcriptions",
            "k",
            &server.url(),
            "whisper-1",
            "",
            TIMEOUT,
        )
        .unwrap();
        asr.transcribe(&[1], Arc::new(|_| {})).await.unwrap();
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_api_error_maps_status_and_body() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(500)
            .with_body("boom")
            .create_async()
            .await;

        let asr = OnlineAsr::new(
            "mimo_chat",
            "k",
            &server.url(),
            "mimo-v2.5-asr",
            "zh",
            TIMEOUT,
        )
        .unwrap();
        let err = asr
            .transcribe(&[1, 2, 3], Arc::new(|_| {}))
            .await
            .unwrap_err();
        match err {
            TranscriberError::ApiError { status, body } => {
                assert_eq!(status, 500);
                assert_eq!(body, "boom");
            }
            other => panic!("expected ApiError, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_api_error_body_is_truncated_at_500_chars() {
        let mut server = mockito::Server::new_async().await;
        let long_body = "x".repeat(1000);
        let _mock = server
            .mock("POST", "/v1/audio/transcriptions")
            .with_status(401)
            .with_body(long_body)
            .create_async()
            .await;

        let asr = OnlineAsr::new(
            "transcriptions",
            "k",
            &server.url(),
            "whisper-1",
            "",
            TIMEOUT,
        )
        .unwrap();
        let err = asr.transcribe(&[1], Arc::new(|_| {})).await.unwrap_err();
        match err {
            TranscriberError::ApiError { status, body } => {
                assert_eq!(status, 401);
                assert_eq!(body.chars().count(), 500);
            }
            other => panic!("expected ApiError, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_non_json_response_maps_to_json_error() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(200)
            .with_body("not json at all")
            .create_async()
            .await;

        let asr = OnlineAsr::new(
            "mimo_chat",
            "k",
            &server.url(),
            "mimo-v2.5-asr",
            "zh",
            TIMEOUT,
        )
        .unwrap();
        assert!(matches!(
            asr.transcribe(&[1], Arc::new(|_| {})).await,
            Err(TranscriberError::JsonError(_))
        ));
    }

    #[tokio::test]
    async fn test_empty_audio_errors_without_network() {
        let asr = OnlineAsr::new(
            "mimo_chat",
            "k",
            "https://token-plan-cn.xiaomimimo.com/v1",
            "mimo-v2.5-asr",
            "zh",
            TIMEOUT,
        )
        .unwrap();
        assert!(matches!(
            asr.transcribe(&[], Arc::new(|_| {})).await,
            Err(TranscriberError::EmptyAudio)
        ));
    }

    #[test]
    fn test_invalid_protocol_rejected_at_construction() {
        assert!(matches!(
            OnlineAsr::new("mimo", "k", "https://x.example/v1", "m", "zh", TIMEOUT),
            Err(TranscriberError::InvalidProtocol(_))
        ));
    }

    #[test]
    fn test_invalid_base_url_rejected_at_construction() {
        assert!(matches!(
            OnlineAsr::new("transcriptions", "k", "   ", "m", "zh", TIMEOUT),
            Err(TranscriberError::InvalidBaseUrl(_))
        ));
    }
}
