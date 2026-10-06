//! 润色级别与思考层级的枚举及字符串解析。

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PolishLevel {
    /// 不润色
    None,
    /// 轻度润色：修复标点和错别字
    Light,
    /// 中度润色：修复标点、错别字和语病
    Medium,
    /// 重度润色：重写为结构清晰的文字
    Heavy,
}

impl PolishLevel {
    #[cfg(test)]
    fn as_str(self) -> &'static str {
        match self {
            PolishLevel::None => "none",
            PolishLevel::Light => "light",
            PolishLevel::Medium => "medium",
            PolishLevel::Heavy => "heavy",
        }
    }

    /// 解析润色级别字符串，无效值回退到 `Medium`。
    pub fn effective(level_str: &str) -> Self {
        <Self as std::str::FromStr>::from_str(level_str).unwrap_or_else(|_| {
            tracing::warn!(level = level_str, "invalid polish level, using medium");
            PolishLevel::Medium
        })
    }
}

impl std::str::FromStr for PolishLevel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            s if s.eq_ignore_ascii_case("none") => Ok(PolishLevel::None),
            s if s.eq_ignore_ascii_case("light") => Ok(PolishLevel::Light),
            s if s.eq_ignore_ascii_case("medium") => Ok(PolishLevel::Medium),
            s if s.eq_ignore_ascii_case("heavy") => Ok(PolishLevel::Heavy),
            other => Err(format!("unknown polish level: {other}")),
        }
    }
}

/// 思考层级：控制润色请求是否让模型思考（深度推理）及投入多少。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThinkingLevel {
    /// 关闭思考（默认）
    Off,
    Low,
    Medium,
    High,
}

impl ThinkingLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            ThinkingLevel::Off => "off",
            ThinkingLevel::Low => "low",
            ThinkingLevel::Medium => "medium",
            ThinkingLevel::High => "high",
        }
    }

    /// 解析思考层级字符串，无效值回退 `Off`（保持默认关闭）。
    pub fn effective(level_str: &str) -> Self {
        <Self as std::str::FromStr>::from_str(level_str).unwrap_or_else(|_| {
            tracing::warn!(level = level_str, "invalid thinking level, using off");
            ThinkingLevel::Off
        })
    }
}

impl std::str::FromStr for ThinkingLevel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            s if s.eq_ignore_ascii_case("off") => Ok(ThinkingLevel::Off),
            s if s.eq_ignore_ascii_case("low") => Ok(ThinkingLevel::Low),
            s if s.eq_ignore_ascii_case("medium") => Ok(ThinkingLevel::Medium),
            s if s.eq_ignore_ascii_case("high") => Ok(ThinkingLevel::High),
            other => Err(format!("unknown thinking level: {other}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_polish_level_from_str() {
        assert!(matches!(
            PolishLevel::from_str("none").unwrap(),
            PolishLevel::None
        ));
        assert!(matches!(
            PolishLevel::from_str("light").unwrap(),
            PolishLevel::Light
        ));
        assert!(matches!(
            PolishLevel::from_str("medium").unwrap(),
            PolishLevel::Medium
        ));
        assert!(matches!(
            PolishLevel::from_str("heavy").unwrap(),
            PolishLevel::Heavy
        ));
        assert!(PolishLevel::from_str("unknown").is_err());
    }

    #[test]
    fn test_polish_level_as_str() {
        assert_eq!(PolishLevel::None.as_str(), "none");
        assert_eq!(PolishLevel::Light.as_str(), "light");
        assert_eq!(PolishLevel::Medium.as_str(), "medium");
        assert_eq!(PolishLevel::Heavy.as_str(), "heavy");
    }

    #[test]
    fn test_thinking_level_from_str() {
        use std::str::FromStr;
        assert_eq!(ThinkingLevel::from_str("off").unwrap(), ThinkingLevel::Off);
        assert_eq!(ThinkingLevel::from_str("LOW").unwrap(), ThinkingLevel::Low);
        assert_eq!(
            ThinkingLevel::from_str("medium").unwrap(),
            ThinkingLevel::Medium
        );
        assert_eq!(
            ThinkingLevel::from_str("high").unwrap(),
            ThinkingLevel::High
        );
        assert!(ThinkingLevel::from_str("unknown").is_err());
        assert_eq!(ThinkingLevel::effective("bogus"), ThinkingLevel::Off);
    }
}
