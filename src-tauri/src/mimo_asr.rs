//! 在线 ASR（小米 MiMo）转写器。
//!
//! 通过网关的 chat/completions 接口上传 WAV（base64）完成在线转写，
//! 面向低配机器绕过本地 SenseVoice 推理。失败策略：直接报错，不回退本地。
//!
//! Online ASR (Xiaomi MiMo) transcriber.
//!
//! Uploads WAV audio (base64) through the gateway's chat/completions endpoint for online
//! transcription, letting low-spec machines skip local SenseVoice inference. Failure policy:
//! error out directly, no local fallback.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use base64::Engine as _;

use crate::error::TranscriberError;
use crate::polisher::protocol::{ApiProtocol, ChatResponse};
use crate::transcriber::{TranscribeResult, Transcriber};

/// MiMo 在线转写器。
/// MiMo online transcriber.
pub struct MimoAsr {
    client: reqwest::Client,
    endpoint: String,
    api_key: String,
    model: String,
    language: String,
}

impl MimoAsr {
    /// 从在线 ASR 配置构造；不发任何网络请求。
    /// Build from online ASR settings; performs no network requests.
    pub fn new(
        api_key: &str,
        api_base_url: &str,
        model: &str,
        language: &str,
        timeout: Duration,
    ) -> Result<Self, TranscriberError> {
        // 端点推导与润色器同源：base 无路径补 /v1/chat/completions，已含路径只补 /chat/completions。
        // Endpoint derivation shares the polisher logic: bare base gets /v1/chat/completions,
        // a base with a path only gets /chat/completions.
        let endpoint = crate::polisher::build_endpoint(api_base_url, ApiProtocol::OpenAi)
            .map_err(|_| TranscriberError::InvalidBaseUrl(api_base_url.to_string()))?;
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|e| TranscriberError::HttpError(e.to_string()))?;
        Ok(Self {
            client,
            endpoint,
            api_key: api_key.to_string(),
            model: model.to_string(),
            language: language.to_string(),
        })
    }

    /// 组装请求体。
    ///
    /// 网关硬约束：`content` 只允许一个 `input_audio` 块，带文本块会被 400 拒绝
    /// （text prompt 由网关注入）；语言为空时传 "auto"。
    ///
    /// Builds the request body.
    ///
    /// Gateway hard constraint: `content` must contain exactly one `input_audio` block; text
    /// blocks are rejected with 400 (the text prompt is injected by the gateway). Empty
    /// language is sent as "auto".
    fn request_body(&self, audio_b64: String) -> serde_json::Value {
        let language = {
            let t = self.language.trim();
            if t.is_empty() {
                "auto".to_string()
            } else {
                t.to_string()
            }
        };
        serde_json::json!({
            "model": self.model,
            "messages": [{
                "role": "user",
                "content": [{
                    "type": "input_audio",
                    "input_audio": { "data": audio_b64, "format": "wav" }
                }]
            }],
            "asr_options": { "language": language }
        })
    }
}

impl Transcriber for MimoAsr {
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
            // Interactive dictation skips retries: fail fast beats stall; the client owns timeouts.
            let audio_b64 = base64::engine::general_purpose::STANDARD.encode(audio);
            let body = self.request_body(audio_b64);

            let resp = self
                .client
                .post(&self.endpoint)
                .bearer_auth(&self.api_key)
                .json(&body)
                .send()
                .await
                .map_err(|e| TranscriberError::HttpError(e.to_string()))?;

            let status = resp.status().as_u16();
            let resp_text = resp
                .text()
                .await
                .map_err(|e| TranscriberError::HttpError(e.to_string()))?;

            if !(200..300).contains(&status) {
                // 错误 body 截断，防止超大响应刷爆悬浮窗。
                // Truncate error bodies so oversized responses don't flood the overlay.
                let mut body_text = resp_text;
                if body_text.chars().count() > 500 {
                    body_text = body_text.chars().take(500).collect();
                }
                return Err(TranscriberError::ApiError {
                    status,
                    body: body_text,
                });
            }

            // 标准 chat.completion 响应，文本在 choices[0].message.content。
            // Standard chat.completion response; the text sits in choices[0].message.content.
            let parsed: ChatResponse = serde_json::from_str(&resp_text)
                .map_err(|e| TranscriberError::JsonError(e.to_string()))?;
            let text = parsed
                .choices
                .first()
                .map(|c| c.message.content.trim().to_string())
                .ok_or_else(|| TranscriberError::JsonError("empty choices".to_string()))?;

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

    #[tokio::test]
    async fn test_transcribe_success_posts_audio_and_parses_text() {
        let mut server = mockito::Server::new_async().await;
        // server.url() 无路径：顺带验证 build_endpoint 补全 /v1/chat/completions。
        // server.url() has no path: also verifies build_endpoint appends /v1/chat/completions.
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .match_header("authorization", "Bearer test-key")
            .match_body(Matcher::PartialJsonString(
                r#"{"model": "mimo-v2.5-asr", "messages": [{"content": [{"type": "input_audio"}]}]}"#
                    .to_string(),
            ))
            .with_status(200)
            .with_body(r#"{"choices":[{"message":{"role":"assistant","content":"你好世界"}}]}"#)
            .create_async()
            .await;

        let asr = MimoAsr::new(
            "test-key",
            &server.url(),
            "mimo-v2.5-asr",
            "zh",
            Duration::from_secs(5),
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
    async fn test_api_error_maps_status_and_body() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(500)
            .with_body("boom")
            .create_async()
            .await;

        let asr = MimoAsr::new(
            "k",
            &server.url(),
            "mimo-v2.5-asr",
            "zh",
            Duration::from_secs(5),
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
    async fn test_non_json_response_maps_to_json_error() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(200)
            .with_body("not json at all")
            .create_async()
            .await;

        let asr = MimoAsr::new(
            "k",
            &server.url(),
            "mimo-v2.5-asr",
            "zh",
            Duration::from_secs(5),
        )
        .unwrap();
        assert!(matches!(
            asr.transcribe(&[1], Arc::new(|_| {})).await,
            Err(TranscriberError::JsonError(_))
        ));
    }

    #[tokio::test]
    async fn test_empty_audio_errors_without_network() {
        let asr = MimoAsr::new(
            "k",
            "https://token-plan-cn.xiaomimimo.com/v1",
            "mimo-v2.5-asr",
            "zh",
            Duration::from_secs(5),
        )
        .unwrap();
        assert!(matches!(
            asr.transcribe(&[], Arc::new(|_| {})).await,
            Err(TranscriberError::EmptyAudio)
        ));
    }
}
