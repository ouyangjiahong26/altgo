//! 设置页“测试连接”的连通性验证与错误提示。

use super::formatter::LLMFormatter;
use super::level::PolishLevel;
use super::protocol;
use crate::error::PolisherError;
use std::time::Duration;

/// 用给定参数构造临时润色器并发一次最小请求，验证地址、密钥与模型可用。
///
/// 供设置页“测试连接”使用：不落盘、不影响流水线，
/// 请求超时 20 秒、max_tokens 限制在 16 以控制花费。
pub async fn test_connection(
    api_key: &str,
    api_base_url: &str,
    model: &str,
    protocol: protocol::ApiProtocol,
) -> Result<(), PolisherError> {
    let formatter = LLMFormatter::with_config(
        api_key.to_string(),
        api_base_url.to_string(),
        model.to_string(),
        Duration::from_secs(20),
        16,
        protocol,
        0.0,
        "zh".to_string(),
    )?;
    formatter
        .polish("你好", PolishLevel::Light)
        .await
        .map(|_| ())
}

/// 把润色错误转成“测试连接”结果的可读提示，指明最可能的原因。
pub fn describe_test_error(e: &PolisherError) -> String {
    match e {
        PolisherError::ApiError { status: 401, .. }
        | PolisherError::ApiError { status: 403, .. } => {
            "密钥无效或没有权限（HTTP 401/403）。请检查 API Key 是否正确、账户是否有余额。"
                .to_string()
        }
        PolisherError::ApiError { status: 404, .. } => {
            "接口地址不对（HTTP 404）。请检查 API URL 是否与供应商文档一致。".to_string()
        }
        PolisherError::ApiError { status: 400, body } => {
            let body = truncate_body(body);
            format!("请求被拒绝（HTTP 400），模型名可能不对。服务商返回：{body}")
        }
        PolisherError::ApiError { status: 429, .. } => {
            "请求频率或额度受限（HTTP 429）。请稍后重试。".to_string()
        }
        PolisherError::HttpError(msg) => {
            if msg.contains("timed out") {
                "连接超时。请检查网络，以及 API 地址是否可达。".to_string()
            } else if msg.contains("relative URL without a base")
                || msg.contains("builder error")
                || msg.contains("invalid URL")
            {
                format!("API 地址无效：{msg}")
            } else {
                format!("网络请求失败：{msg}")
            }
        }
        PolisherError::InvalidBaseUrl(url) => {
            format!("API 地址无效：'{url}'。请填写完整 URL（如 https://api.deepseek.com）。")
        }
        PolisherError::UnknownProtocol { protocol } => {
            format!("协议无效：'{protocol}'，应为 \"openai\" 或 \"anthropic\"。")
        }
        other => other.message(),
    }
}

fn truncate_body(body: &str) -> String {
    const MAX_LEN: usize = 200;
    if body.len() <= MAX_LEN {
        body.to_string()
    } else {
        let mut end = MAX_LEN;
        while !body.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}…", &body[..end])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_connection_success() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(200)
            .with_body(mock_success_response("你好"))
            .create_async()
            .await;

        let result = test_connection(
            "key",
            &server.url(),
            "deepseek-chat",
            protocol::ApiProtocol::OpenAi,
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_connection_auth_error_is_described() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(401)
            .create_async()
            .await;

        let err = test_connection(
            "bad-key",
            &server.url(),
            "deepseek-chat",
            protocol::ApiProtocol::OpenAi,
        )
        .await
        .unwrap_err();
        let msg = describe_test_error(&err);
        assert!(msg.contains("密钥"), "unexpected: {msg}");
    }

    #[tokio::test]
    async fn test_connection_404_is_described() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("POST", mockito::Matcher::Any)
            .with_status(404)
            .create_async()
            .await;

        let err = test_connection("key", &server.url(), "m", protocol::ApiProtocol::OpenAi)
            .await
            .unwrap_err();
        let msg = describe_test_error(&err);
        assert!(msg.contains("地址"), "unexpected: {msg}");
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
}
