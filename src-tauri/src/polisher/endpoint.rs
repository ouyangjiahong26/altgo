//! 请求 endpoint 推导与 base URL 的 host 解析。

use super::protocol;
use crate::error::PolisherError;

/// 由 base URL 与协议推导请求 endpoint。
///
/// 兼容两种写法：带版本路径的 base（如 `https://api.moonshot.cn/v1`、
/// `https://open.bigmodel.cn/api/paas/v4`）与不带路径的 origin
/// （如 `https://api.deepseek.com`）。
///
/// 规则：
/// 1. 剥首尾空白与尾部 `/`。
/// 2. 路径已以该协议的请求路径（openai `/chat/completions`、anthropic
///    `/messages`）结尾时，视为完整 endpoint，直接使用。
/// 3. 路径为空：openai 补 `/v1/chat/completions`，anthropic 补 `/v1/messages`
///    （两家官方 SDK 的默认约定）。
/// 4. 路径非空：仅补请求路径，版本段（`/v1`、`/api/paas/v4` 等）由 base 自带。
pub fn build_endpoint(
    base_url: &str,
    protocol: protocol::ApiProtocol,
) -> Result<String, PolisherError> {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err(PolisherError::InvalidBaseUrl(base_url.to_string()));
    }

    // 定位 origin 之后的首个 '/'，其起即为路径，无 scheme 时按首个 '/' 兜底。
    let path_start = match trimmed.find("://") {
        Some(scheme_end) => trimmed[scheme_end + 3..]
            .find('/')
            .map(|p| scheme_end + 3 + p),
        None => trimmed.find('/'),
    };
    let path = path_start.map(|i| &trimmed[i..]).unwrap_or("");

    match protocol {
        protocol::ApiProtocol::OpenAi => {
            if path.ends_with("/chat/completions") {
                Ok(trimmed.to_string())
            } else if path.is_empty() {
                Ok(format!("{trimmed}/v1/chat/completions"))
            } else {
                Ok(format!("{trimmed}/chat/completions"))
            }
        }
        protocol::ApiProtocol::Anthropic => {
            if path.ends_with("/messages") {
                Ok(trimmed.to_string())
            } else if path.is_empty() {
                Ok(format!("{trimmed}/v1/messages"))
            } else {
                Ok(format!("{trimmed}/messages"))
            }
        }
    }
}

/// 从 base URL 提取 host（小写比较由调用方处理，这里保留原文、不含端口）。
pub(super) fn host_of(url: &str) -> &str {
    let trimmed = url.trim();
    let after_scheme = match trimmed.find("://") {
        Some(i) => &trimmed[i + 3..],
        None => trimmed,
    };
    let authority = after_scheme.split('/').next().unwrap_or("");
    authority.split(':').next().unwrap_or("")
}

/// 判断 host 是否属于某域名（等于该域，或为其子域）。
/// 边界检查防短域名误命中（如 `notz.ai` 不应命中 `z.ai`）。
pub(super) fn host_matches(host: &str, domain: &str) -> bool {
    host == domain || host.ends_with(&format!(".{domain}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_endpoint_openai_preset_matrix() {
        use protocol::ApiProtocol;
        let p = ApiProtocol::OpenAi;
        // 各预设 base（与 frontend/src/config/modelPresets.ts 对齐）
        assert_eq!(
            build_endpoint("https://api.deepseek.com", p).unwrap(),
            "https://api.deepseek.com/v1/chat/completions"
        );
        assert_eq!(
            build_endpoint("https://api.moonshot.cn/v1", p).unwrap(),
            "https://api.moonshot.cn/v1/chat/completions"
        );
        assert_eq!(
            build_endpoint("https://open.bigmodel.cn/api/paas/v4", p).unwrap(),
            "https://open.bigmodel.cn/api/paas/v4/chat/completions"
        );
        assert_eq!(
            build_endpoint("https://dashscope.aliyuncs.com/compatible-mode/v1", p).unwrap(),
            "https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions"
        );
        assert_eq!(
            build_endpoint("https://api.openai.com/v1", p).unwrap(),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            build_endpoint("https://api.siliconflow.cn/v1", p).unwrap(),
            "https://api.siliconflow.cn/v1/chat/completions"
        );
    }

    #[test]
    fn test_build_endpoint_anthropic_preset_matrix() {
        use protocol::ApiProtocol;
        let p = ApiProtocol::Anthropic;
        assert_eq!(
            build_endpoint("https://api.anthropic.com", p).unwrap(),
            "https://api.anthropic.com/v1/messages"
        );
        assert_eq!(
            build_endpoint("https://api.anthropic.com/v1", p).unwrap(),
            "https://api.anthropic.com/v1/messages"
        );
    }

    #[test]
    fn test_host_of_extracts_host() {
        assert_eq!(host_of("https://api.deepseek.com"), "api.deepseek.com");
        assert_eq!(host_of("http://localhost:11434/v1"), "localhost");
        assert_eq!(
            host_of("https://open.bigmodel.cn/api/paas/v4"),
            "open.bigmodel.cn"
        );
        // 大小写与空白容错
        assert_eq!(host_of("  HTTPS://API.OPENAI.com/"), "API.OPENAI.com");
        // 无 scheme 时按整体 authority 处理
        assert_eq!(host_of("api.deepseek.com"), "api.deepseek.com");
        assert_eq!(host_of(""), "");
    }

    #[test]
    fn test_host_matches_domain_boundary() {
        assert!(host_matches("api.z.ai", "z.ai"));
        assert!(host_matches("z.ai", "z.ai"));
        assert!(!host_matches("notz.ai", "z.ai"));
        assert!(!host_matches("z.ai.evil.com", "z.ai"));
        assert!(host_matches("open.bigmodel.cn", "bigmodel.cn"));
        assert!(!host_matches("bigmodel.cn.evil.com", "bigmodel.cn"));
    }

    #[test]
    fn test_build_endpoint_tolerates_common_variants() {
        use protocol::ApiProtocol;
        // 本地 Ollama（origin 无路径）
        assert_eq!(
            build_endpoint("http://localhost:11434", ApiProtocol::OpenAi).unwrap(),
            "http://localhost:11434/v1/chat/completions"
        );
        // 尾部斜杠与空白
        assert_eq!(
            build_endpoint("  http://localhost:11434/ ", ApiProtocol::OpenAi).unwrap(),
            "http://localhost:11434/v1/chat/completions"
        );
        assert_eq!(
            build_endpoint("https://api.moonshot.cn/v1/", ApiProtocol::OpenAi).unwrap(),
            "https://api.moonshot.cn/v1/chat/completions"
        );
        // 已是完整 endpoint 时直接使用
        assert_eq!(
            build_endpoint("https://x.example/v1/chat/completions", ApiProtocol::OpenAi).unwrap(),
            "https://x.example/v1/chat/completions"
        );
        assert_eq!(
            build_endpoint("https://x.example/v1/messages", ApiProtocol::Anthropic).unwrap(),
            "https://x.example/v1/messages"
        );
        // 空值报错
        assert!(matches!(
            build_endpoint("", ApiProtocol::OpenAi),
            Err(PolisherError::InvalidBaseUrl(_))
        ));
        assert!(build_endpoint("   ", ApiProtocol::Anthropic).is_err());
    }
}
