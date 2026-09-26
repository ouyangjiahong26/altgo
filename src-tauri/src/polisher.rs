//! 文本润色模块。
//!
//! 使用 LLM 对语音识别结果进行后期处理，支持 4 个润色级别：
//!
//! - `none`：不润色，直接返回原文
//! - `light`：修复标点和明显错别字
//! - `medium`：修复标点、错别字和语病，使语句更通顺
//! - `heavy`：重写为结构清晰、表达准确的文字
//!
//! 当语言为 `zh` 时，内置系统提示会约束输出为规范简体中文，并合并：材料概括类写作要求，以及本地安装的
//! **ljg-writes** / **ljg-plain**（lijigang/ljg-skills）中与“口语文本润色”相关的取向（非全文摘抄 skill 文件）。
//!
//! 使用兼容 OpenAI 的聊天 API，支持指数退避重试（最多 3 次）。
//!
//! Text polishing module.
//!
//! Post-processes speech recognition output with an LLM across four polish levels:
//!
//! - `none`: no polishing, return the original text
//! - `light`: fix punctuation and obvious typos
//! - `medium`: fix punctuation, typos, and grammar for smoother sentences
//! - `heavy`: rewrite into well-structured, precise prose
//!
//! When the language is `zh`, the built-in system prompt constrains output to standard Simplified
//! Chinese and merges: material-summarization writing requirements, plus the oral-text polishing
//! orientations of locally installed **ljg-writes** / **ljg-plain** (lijigang/ljg-skills)—not
//! quoting the skill files verbatim.
//!
//! Uses OpenAI-compatible chat APIs with exponential-backoff retries (up to 3).

pub mod protocol;

use crate::error::PolisherError;
use reqwest::Client;
use std::time::Duration;

/// 重试延迟基数（毫秒），用于指数退避计算。
/// Base retry delay (ms) for exponential-backoff computation.
const RETRY_BASE_DELAY_MS: u64 = 500;

/// 指数退避的通用重试助手。
///
/// 对给定的异步操作最多重试 `max_retries` 次。
/// 不可重试的错误（401、403）立即返回。
/// Generic retry helper with exponential backoff.
///
/// Retries the given async operation up to `max_retries` times.
/// Non-retryable errors (401, 403) are returned immediately.
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
                // Check for non-retryable auth errors
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PolishLevel {
    /// 不润色
    /// No polishing
    None,
    /// 轻度润色：修复标点和错别字
    /// Light polish: fix punctuation and typos
    Light,
    /// 中度润色：修复标点、错别字和语病
    /// Medium polish: fix punctuation, typos, and grammar issues
    Medium,
    /// 重度润色：重写为结构清晰的文字
    /// Heavy polish: rewrite into well-structured prose
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
    /// Parses a polish-level string; invalid values fall back to `Medium`.
    pub fn effective(level_str: &str) -> Self {
        <Self as std::str::FromStr>::from_str(level_str).unwrap_or_else(|_| {
            tracing::warn!("invalid polish level '{level_str}', using medium");
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
/// Thinking level: whether polish requests let the model think (deep reasoning), and how much.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThinkingLevel {
    /// 关闭思考（默认）
    /// Thinking off (default)
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
    /// Parses a thinking-level string; invalid values fall back to `Off` (off by default).
    pub fn effective(level_str: &str) -> Self {
        <Self as std::str::FromStr>::from_str(level_str).unwrap_or_else(|_| {
            tracing::warn!("invalid thinking level '{level_str}', using off");
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

/// 中文润色时附加的写作与材料概括要求（与简体约束一并传入模型）。
/// Writing and material-summarization requirements appended for Chinese polishing (sent to the
/// model together with the Simplified-Chinese constraint).
const ZH_WRITE_GUIDANCE: &str = r#"注意写作的要求：要善于总结材料，这种总结就是将丰富的感性材料科学地加以概括，进行去粗取精、去伪存真、由此及彼、由表及里地加工改造。具体地讲，就是把材料搞全了、弄准了，把问题掰开了、揉碎了，把内在联系理清了、摆正了，这样才可以得到反映事物本质的真知和理论，才可以发现事物运动的规律。“关于写文章，请注意不要用过于夸大的修饰词，反而减损了力量。必须注意各种词语的逻辑界限和整篇文章的条理（也是逻辑问题）。废话应当尽量除去。”“文章写得通俗、亲切，由小讲到大，由近讲到远，引人入胜，这就很好。”“要采取和读者完全平等的态度。我们应该老老实实地办事，对事物有分析，写文章有说服力，不要靠装腔作势来吓人。”“总是先讲死人、外国人，这不好，应当从当前形势讲起。今后写文章要通俗，使工农都能接受。”"#;

/// 与语音转写润色相关的 **ljg-writes** / **ljg-plain** 取向（摘自用户本机 skill 要义，不含 Org/文件输出等仅技能执行用条款）。
/// The **ljg-writes** / **ljg-plain** orientations relevant to speech-transcription polishing
/// (distilled from the user's local skills, excluding Org/file-output clauses that only matter to skill execution).
const ZH_LJG_GUIDANCE: &str = r#"

【ljg-writes / ljg-plain（语音后润色适用；内化即可，勿输出本段标题或标签）】
姿态与诚实：心里是对一个具体的人讲，不是对抽象的“读者们”；不确定就保留不确定感，“大概七成”比空泛的“可能”诚实；忌群体代言、忌编经历、忌元评论（如“接下来我们讨论”）；禁止用“再深入一层”“最深的一层是”等宣告深度——深度靠下一句内容让人感受到，不靠自报。
语言：简洁、直白、质朴；能短则短；动词用准；砍掉机械连词（此外、另外）、形容词堆叠与软化套话（某种程度上、值得注意的是）；翻译腔句式（像英译中硬套）改成自然汉语；避免同一句式套话重复出现。
白话（ljg-plain 红线精神，按短文本尽量满足）：口语检验——像跟聪明朋友当面说吗；短词优先；一句一事，长句拆开；名词能具体则具体，动词有力，能删的形容词就删；开头少空泛铺陈与“自古以来”式引子；删开场白、拐杖词、宣传腔与夸大象征（标志着、见证了、充满活力等）；信任读者，不凑字数式手把手；专业词非必要不出现，必须出现时先大白话落地再点术语。
磨与中文：弱化学术/ AI 腔与“谁写都一样”的模板句；从句拆开、嵌套展平，发挥汉语意合；同一意思选最顺口的地道说法。"#;

fn get_system_prompt(level: PolishLevel, language: &str) -> String {
    let lang_name = match language {
        "zh" => "Simplified Chinese (简体中文, Mainland standard)",
        "en" => "English",
        "ja" => "日本語",
        "ko" => "한국어",
        "fr" => "français",
        "de" => "Deutsch",
        "es" => "español",
        _ => language,
    };

    // 明确约束简体，避免模型按“繁体/港台书面”习惯输出。
    // Constrain output to Simplified Chinese explicitly; otherwise models may drift toward
    // Traditional or HK/TW written conventions.
    let zh_script_rule = if language == "zh" {
        " For Chinese: output in Simplified Chinese only (大陆通用规范简体). Never use Traditional Chinese. If the input is Traditional, convert to Simplified. "
    } else {
        ""
    };

    // 写作与表达要求：全文融入用户提供的规范；轻量润色时强调不改结构、仅作最小必要调整。
    // Writing/expressiveness requirements: fold the user-provided rubric throughout the text;
    // at light polish, stress keeping structure intact with only minimal necessary edits.
    let zh_combined: String = if language == "zh" && !matches!(level, PolishLevel::None) {
        let intro = match level {
            PolishLevel::None => "",
            PolishLevel::Light => {
                " For light polish: do not restructure; tiny edits only. When text is Chinese, also heed these norms in spirit: "
            }
            PolishLevel::Medium | PolishLevel::Heavy => {
                " When output is Chinese, follow these writing norms: "
            }
        };
        format!("{intro}{ZH_WRITE_GUIDANCE}{ZH_LJG_GUIDANCE}")
    } else {
        String::new()
    };

    match level {
        PolishLevel::None => String::new(),
        PolishLevel::Light => format!(
            "You are a post-processing assistant for speech-to-text in {lang_name}. The user gives you raw speech recognition text in {lang_name}. Fix punctuation and obvious typos without changing the original meaning or word choices. Output only the corrected text with no explanation.{zh_script_rule}{zh_combined}"
        ),
        PolishLevel::Medium => format!(
            "You are a post-processing assistant for speech-to-text in {lang_name}. The user gives you raw speech recognition text in {lang_name}. Fix punctuation, typos, and grammar issues to make the text more fluent and natural, without changing the original meaning. Output only the corrected text with no explanation.{zh_script_rule}{zh_combined}"
        ),
        PolishLevel::Heavy => format!(
            "You are a post-processing assistant for speech-to-text in {lang_name}. The user gives you raw speech recognition text in {lang_name}. Rewrite it into well-structured, clearly expressed text. You may adjust word order and phrasing, but preserve the core meaning. Output only the rewritten text with no explanation.{zh_script_rule}{zh_combined}"
        ),
    }
}

// ---------------------------------------------------------------------------
// SystemPromptSource trait and implementations
// SystemPromptSource trait 与实现
// ---------------------------------------------------------------------------

/// 抽象 system prompt 来源，消除 `polish()` 内部的 fallback 链。
///
/// 由 `LLMFormatter` 持有，使 prompt 选择逻辑可在测试中替换。
///
/// Abstracts where a system prompt comes from, dissolving the fallback chain inside `polish()`.
///
/// Held by `LLMFormatter`, so prompt-selection logic is swappable in tests.
pub trait SystemPromptSource: Send + Sync {
    /// 获取指定级别和语言的 system prompt。
    /// Returns the system prompt for the given level and language.
    fn get_prompt(
        &self,
        level: PolishLevel,
        language: &str,
    ) -> Result<String, crate::prompt_store::PromptError>;

    /// 支持 clone 为 trait object（用于 `LLMFormatter::Clone`）。
    /// Supports cloning into a trait object (for `LLMFormatter::Clone`).
    fn clone_box(&self) -> Box<dyn SystemPromptSource>;
}

/// 基于 `PromptStore` 的 prompt 来源。
/// Prompt source backed by a `PromptStore`.
pub struct PromptStoreSource {
    store: crate::prompt_store::PromptStore,
}

impl PromptStoreSource {
    pub fn new(store: crate::prompt_store::PromptStore) -> Self {
        Self { store }
    }
}

impl SystemPromptSource for PromptStoreSource {
    fn get_prompt(
        &self,
        level: PolishLevel,
        _language: &str,
    ) -> Result<String, crate::prompt_store::PromptError> {
        self.store.get_system_prompt(level)
    }

    fn clone_box(&self) -> Box<dyn SystemPromptSource> {
        Box::new(PromptStoreSource {
            store: self.store.clone(),
        })
    }
}

/// 用户自定义 prompt 来源（来自 `config.polisher.system_prompt`）。
/// User-defined prompt source (from `config.polisher.system_prompt`).
pub struct CustomSource {
    prompt: String,
}

impl CustomSource {
    pub fn new(prompt: String) -> Self {
        Self { prompt }
    }
}

impl SystemPromptSource for CustomSource {
    fn get_prompt(
        &self,
        _level: PolishLevel,
        _language: &str,
    ) -> Result<String, crate::prompt_store::PromptError> {
        Ok(self.prompt.clone())
    }

    fn clone_box(&self) -> Box<dyn SystemPromptSource> {
        Box::new(CustomSource {
            prompt: self.prompt.clone(),
        })
    }
}

// ---------------------------------------------------------------------------
// Endpoint 推导
// Endpoint derivation（endpoint 推导）
// ---------------------------------------------------------------------------

/// 由 base URL 与协议推导请求 endpoint。
///
/// 兼容两种写法：带版本路径的 base（如 `https://api.moonshot.cn/v1`、
/// `https://open.bigmodel.cn/api/paas/v4`）与不带路径的 origin
/// （如 `https://api.deepseek.com`）。
///
/// 规则：
/// 1. 剥首尾空白与尾部 `/`；
/// 2. 路径已以该协议的请求路径（openai `/chat/completions`、anthropic
///    `/messages`）结尾时，视为完整 endpoint，直接使用；
///
/// Derives the request endpoint from a base URL and protocol.
///
/// Handles both versioned bases (`https://api.moonshot.cn/v1`,
/// `https://open.bigmodel.cn/api/paas/v4`) and bare origins (`https://api.deepseek.com`).
///
/// Rules:
/// 1. Trim whitespace and trailing slashes;
/// 2. A path already ending with the protocol's request path (openai `/chat/completions`,
///    anthropic `/messages`) counts as a complete endpoint, used as-is;
/// 3. Empty path: openai appends `/v1/chat/completions`, anthropic `/v1/messages`
///    (the two official SDK defaults);
/// 4. Non-empty path: append only the request path—the version segment (`/v1`,
///    `/api/paas/v4`, ...) comes with the base.
/// 3. 路径为空：openai 补 `/v1/chat/completions`，anthropic 补 `/v1/messages`
///    （两家官方 SDK 的默认约定）；
/// 4. 路径非空：仅补请求路径，版本段（`/v1`、`/api/paas/v4` 等）由 base 自带。
pub fn build_endpoint(
    base_url: &str,
    protocol: protocol::ApiProtocol,
) -> Result<String, PolisherError> {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err(PolisherError::InvalidBaseUrl(base_url.to_string()));
    }

    // 定位 origin 之后的首个 '/'，其起即为路径；无 scheme 时按首个 '/' 兜底。
    // Locate the first '/' after the origin; from there on it's the path. Without a scheme,
    // fall back to the first '/'.
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

// ---------------------------------------------------------------------------
// 思考层级控制
// Thinking-level control（思考层级控制）
// ---------------------------------------------------------------------------

/// 从 base URL 提取 host（小写比较由调用方处理，这里保留原文、不含端口）。
/// Extracts the host from a base URL (callers handle lowercasing; raw text kept, port excluded).
fn host_of(url: &str) -> &str {
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
/// Whether the host belongs to a domain (equals it or is a subdomain). Boundary checks prevent
/// short-domain false hits (e.g. `notz.ai` must not match `z.ai`).
fn host_matches(host: &str, domain: &str) -> bool {
    host == domain || host.ends_with(&format!(".{domain}"))
}

/// 按 host 后缀匹配的“默认开启思考”服务商表，按各家参数方言分三组。
///
/// 语音润色是轻量文本任务：默认档（`Off`）对已知默认开启思考的服务商附带关闭参数，
/// 否则思考会吃掉 `max_tokens`（可见文本为空）或凭空多出数秒延迟；未命中表的服务商
/// 保持原 body，避免严格校验未知字段的服务商（OpenAI 等）拒绝请求。
///
/// Host-suffix tables of vendors known to think by default, grouped by parameter dialect.
///
/// Voice polishing is a lightweight text task: at the default (`Off`) level the app attaches the
/// vendor's disable parameter for these hosts—otherwise thinking eats `max_tokens` (empty visible
/// text) or adds seconds of latency. Unmatched hosts keep the original body, since strict
/// validators (OpenAI etc.) reject unknown fields.
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
/// The vendor's thinking-parameter dialect.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ThinkingDialect {
    /// `enable_thinking` 布尔开关（通义 / SiliconFlow）
    /// Boolean `enable_thinking` toggle (dashscope / SiliconFlow)
    EnableThinking,
    /// `thinking: {"type": ...}`（智谱 / Kimi / DeepSeek / MiMo 等）
    /// `thinking: {"type": ...}` (bigmodel / Kimi / DeepSeek / MiMo etc.)
    ThinkingType,
    /// OpenRouter 的 `reasoning` 对象
    /// OpenRouter's `reasoning` object
    Reasoning,
}

/// 按 host 判定思考参数方言；未命中服务商表返回 `None`。
/// Resolves the thinking-parameter dialect for a host; `None` when the vendor table misses.
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
/// 默认（`Off`）按方言关闭思考；用户选择层级（`Low`/`Medium`/`High`）时按各家方言开启：
/// - 通义（dashscope）/ SiliconFlow：`enable_thinking`（仅开关，不分档）
/// - 智谱 / z.ai / 火山方舟 / MiniMax / DeepSeek / Moonshot / Kimi / MiMo：
///   `thinking: {"type": "enabled"}`（仅开关，不分档）
/// - OpenRouter：`reasoning: {"effort": "<档位>"}`
/// - 未命中表的服务商（含 OpenAI 官方）：`reasoning_effort: "<档位>"`
///   （OpenAI 事实标准；用户自担模型不支持时的 400）
///
/// Returns the request fields controlling thinking for a host and level, OpenAI-compatible
/// protocol.
///
/// At the default (`Off`) the vendor dialect disables thinking. When the user picks a level
/// (`Low`/`Medium`/`High`), thinking turns on in each vendor's dialect: on/off-only vendors just
/// enable; OpenRouter gets `reasoning.effort`; unmatched hosts get `reasoning_effort` (the
/// OpenAI de-facto standard; a 400 from an unsupported model is the user's opt-in risk).
fn thinking_fields(host: &str, level: ThinkingLevel) -> Vec<(&'static str, serde_json::Value)> {
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
/// 默认思考，思考与回答共享 `max_tokens`——预算被思考吃光后可见文本为空（表现为“润色失败”），
/// 或白等数秒。
///
/// 开启档与协议无关：按档给思考预算，Anthropic 要求 `max_tokens` 大于预算、且思考与
/// `temperature` 不兼容（调用方据此抬 `max_tokens`、省 `temperature`）。
///
/// Returns the Anthropic-protocol thinking control for a host and level.
///
/// At `Off` an explicit `disabled` goes only to hosts known to think by default: Anthropic's own
/// thinking is opt-in and its newer models reject `disabled` with a 400, so official and unknown
/// endpoints keep sending nothing. Without the field those vendors think by default and share
/// `max_tokens` with the answer—once thinking eats the budget the visible text is empty (read as
/// "polish failed"), or the user waits seconds for nothing.
///
/// The enabled levels are protocol-independent: each level sets a thinking budget, and Anthropic
/// requires `max_tokens` above the budget while thinking is incompatible with `temperature`
/// (callers raise `max_tokens` and drop `temperature` accordingly).
fn anthropic_thinking(host: &str, level: ThinkingLevel) -> Option<protocol::AnthropicThinking> {
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

/// ASCII 大小写不敏感的字串查找（返回字节下标）。
/// ASCII case-insensitive substring search (returns a byte index).
fn find_ascii_ci(haystack: &str, needle: &str) -> Option<usize> {
    haystack
        .as_bytes()
        .windows(needle.len())
        .position(|w| w.eq_ignore_ascii_case(needle.as_bytes()))
}

/// 剥掉 `<think>…</think>` 块（含截断导致的未闭合块）。
///
/// 部分 OpenAI 兼容端点（尤其未命中抑制表的中转站）会把思维链以 think 块
/// 内联进正文；润色结果不应混入这些残渣。
/// Strips `<think>…</think>` blocks (including unclosed ones caused by truncation).
///
/// Some OpenAI-compatible endpoints—especially relays missing from the suppression table—inline
/// chains of thought as think blocks within the body; polished results must not carry that debris.
pub fn strip_thinking_tags(text: &str) -> String {
    let mut rest = text;
    let mut out = String::with_capacity(text.len());
    while let Some(start) = find_ascii_ci(rest, "<think>") {
        out.push_str(&rest[..start]);
        let after = &rest[start + "<think>".len()..];
        match find_ascii_ci(after, "</think>") {
            Some(end) => rest = &after[end + "</think>".len()..],
            // 未闭合块：剥到末尾
            // Unclosed block: strip through the end
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

/// 用给定参数构造临时润色器并发一次最小请求，验证地址、密钥与模型可用。
///
/// 供设置页“测试连接”使用：不落盘、不影响流水线，
/// 请求超时 20 秒、max_tokens 限制在 16 以控制花费。
///
/// Builds a throwaway polisher from the given parameters and fires one minimal request,
/// verifying that the address, key, and model all work.
///
/// Powers the Settings page "test connection": nothing is persisted, the pipeline is untouched,
/// and the request uses a 20s timeout with max_tokens capped at 16 to bound cost.
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
/// Turns polishing errors into readable "test connection" outcomes, naming the most likely cause.
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

/// LLM 文本润色器。
///
/// 支持 OpenAI 和 Anthropic 两种 API 协议，
/// 支持指数退避重试（最多 3 次）。
///
/// System prompt 通过 `SystemPromptSource` trait 注入；`prompt_source` 为 `None` 时
/// `polish()` 内部用内置 hardcoded prompt 兜底。
///
/// The LLM text polisher.
///
/// Speaks both the OpenAI and Anthropic API protocols, retrying with exponential backoff
/// (up to 3 times).
///
/// The system prompt is injected through the `SystemPromptSource` trait; when `prompt_source` is
/// `None`, `polish()` falls back internally to a built-in hardcoded prompt.
pub struct LLMFormatter {
    api_key: String,
    api_base_url: String,
    model: String,
    client: Client,
    max_retries: u32,
    max_tokens: u32,
    protocol: protocol::ApiProtocol,
    temperature: f32,
    language: String,
    thinking: ThinkingLevel,
    prompt_source: Option<Box<dyn SystemPromptSource>>,
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
    /// Create LLMFormatter from config sections.
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
    /// Shared factory: builds one LLMFormatter carrying every prompt source from the Config at once.
    ///
    /// Both the realtime pipeline (`voice_pipeline::builder::build_polisher`) and the IPC handler
    /// (`cmd::polish_history_entry`) construct through this so both paths resolve prompts alike:
    /// PromptStore → Custom → hardcoded fallback.
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

    /// 设置思考层级；默认 `Off`（自动关闭已知服务商的思考）。
    /// Sets the thinking level; defaults to `Off` (auto-off for known vendors).
    pub fn with_thinking_level(mut self, level: ThinkingLevel) -> Self {
        self.thinking = level;
        self
    }

    /// 设置 system prompt 的来源；`None` 表示使用内置 hardcoded prompt。
    /// Sets the prompt source for system prompt resolution; `None` uses the built-in
    /// hardcoded prompt.
    pub fn with_prompt_source(mut self, source: Option<Box<dyn SystemPromptSource>>) -> Self {
        self.prompt_source = source;
        self
    }

    /// 使用 LLM 润色文本。
    ///
    /// 如果级别为 `None` 或文本为空，直接返回原文。
    /// 润色失败时返回错误。
    /// Polishes text with the LLM.
    ///
    /// A `None` level or empty text returns the original text directly; polish failures return Err.
    pub async fn polish(&self, text: &str, level: PolishLevel) -> Result<String, PolisherError> {
        if matches!(level, PolishLevel::None) || text.is_empty() {
            return Ok(text.to_string());
        }

        let system_prompt = self
            .prompt_source
            .as_ref()
            .and_then(|s| s.get_prompt(level, &self.language).ok())
            .unwrap_or_else(|| get_system_prompt(level, &self.language));

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
        // Endpoints missing from the suppression table (e.g. relays) may inline chains of thought as
        // <think> blocks in the body; the polished output strips them plus surrounding whitespace
        // before touching the clipboard.
        Ok(strip_thinking_tags(&polished).trim().to_string())
    }

    /// 组装 Anthropic 协议请求体：Anthropic 扩展思考的关闭/开启、`max_tokens` 抬升与
    /// `temperature` 取舍都在这里。
    ///
    /// 关闭档对默认开启思考的服务商发 `disabled`（它们与回答共享 `max_tokens`，不关就会吃掉
    /// 全部预算）；对官方/未知端点不发字段（其思考是选择加入，新模型对 `disabled` 报 400）。
    /// 开启档按档给预算，此时 `max_tokens` 需大于预算并抬升配置值，且不发与思考不兼容的
    /// `temperature`。
    ///
    /// Builds the Anthropic-protocol request body: the off/on handling of Anthropic extended
    /// thinking, the `max_tokens` raise, and the `temperature` trade-off all live here.
    ///
    /// At `Off`, hosts known to think by default get `disabled` (their thinking shares
    /// `max_tokens` with the answer and otherwise swallows the whole budget); official/unknown
    /// endpoints send nothing (their thinking is opt-in and newer models reject `disabled` with a
    /// 400). At the on levels a budget is set—`max_tokens` must exceed it and rises above the
    /// configured value, and the thinking-incompatible `temperature` is dropped.
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

    async fn do_openai_request(
        &self,
        mut body: protocol::ChatRequest,
    ) -> Result<String, PolisherError> {
        let url = build_endpoint(&self.api_base_url, protocol::ApiProtocol::OpenAi)?;

        // 按服务商与思考层级附加控制思考的字段；层级关闭时不命中的服务商
        // 保持原始 body，避免严格校验未知字段的服务商（OpenAI 等）拒绝请求。
        // 经 `extra` 平铺序列化而非 to_value 中转，保持 f32 字段的紧凑表示。
        // Attach vendor- and thinking-level fields; with thinking off, unmatched vendors keep
        // the original body so strict validators (OpenAI etc.) don't reject unknown fields.
        // Flatten-serialize via `extra` instead of round-tripping to_value, keeping f32 fields compact.
        let fields = thinking_fields(host_of(&self.api_base_url), self.thinking);
        if !fields.is_empty() {
            let mut extra = serde_json::Map::new();
            for (key, value) in fields {
                extra.insert(key.to_string(), value);
            }
            body.extra = Some(extra);
        }

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body)
            .send()
            .await
            .map_err(|e| PolisherError::HttpError(e.to_string()))?;

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
            // Bearer token——各取所需，多余的头双方都会忽略。
            // Dual auth headers: official Anthropic wants x-api-key while most compatible endpoints/
            // relays want Bearer tokens—send both; each side ignores the surplus header.
            .header("x-api-key", &self.api_key)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("anthropic-version", "2023-06-01")
            .json(body)
            .send()
            .await
            .map_err(|e| PolisherError::HttpError(e.to_string()))?;

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

        let anthropic_resp: protocol::AnthropicResponse = resp
            .json()
            .await
            .map_err(|e| PolisherError::JsonError(e.to_string()))?;
        // 取第一个文本块：响应混入 thinking 块时它在最前（无 text 字段）；
        // 不填 type 的中转端点按文本块处理。
        // Take the first text block: when thinking blocks ride along they come first (no text field);
        // relays omitting type are treated as text blocks anyway.
        anthropic_resp
            .content
            .into_iter()
            .find(|b| b.block_type.is_empty() || b.block_type == "text")
            .and_then(|b| b.text)
            .ok_or(PolisherError::EmptyResponse)
    }
}

/// 抽出 builder.rs 里的 prompt source chain 构造逻辑。所有调用方共享一份。
///
/// 优先级：PromptStore（`resources/prompts` 加载成功）→ Custom（`system_prompt` 非空）→ `None`。
/// `None` 时调用方 `polish()` 用内置 hardcoded prompt 兜底。
/// Extracts the prompt-source-chain construction once living in builder.rs; all callers share it.
///
/// Priority: PromptStore (when `resources/prompts` loads) → Custom (non-empty `system_prompt`) →
/// `None`. On `None` the caller's `polish()` falls back to the built-in hardcoded prompt.
fn build_prompt_source_chain(cfg: &crate::config::Config) -> Option<Box<dyn SystemPromptSource>> {
    let store_source: Option<Box<dyn SystemPromptSource>> = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.join("resources/prompts")))
        .or_else(|| Some(std::path::PathBuf::from("resources/prompts")))
        .filter(|dir| dir.exists())
        .and_then(|dir| {
            let store = crate::prompt_store::PromptStore::new(dir);
            match store.ensure_loaded() {
                Ok(()) => {
                    tracing::info!("PromptStore loaded successfully");
                    Some(Box::new(PromptStoreSource::new(store)) as Box<dyn SystemPromptSource>)
                }
                Err(e) => {
                    tracing::warn!(error = %e, "failed to load prompts from PromptStore");
                    None
                }
            }
        });

    let custom_source: Option<Box<dyn SystemPromptSource>> =
        if !cfg.polisher.system_prompt.is_empty() {
            Some(
                Box::new(CustomSource::new(cfg.polisher.system_prompt.clone()))
                    as Box<dyn SystemPromptSource>,
            )
        } else {
            None
        };

    store_source.or(custom_source)
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
    fn test_build_endpoint_openai_preset_matrix() {
        use protocol::ApiProtocol;
        let p = ApiProtocol::OpenAi;
        // 各预设 base（与 frontend/src/config/modelPresets.ts 对齐）
        // Preset bases (aligned with frontend/src/config/modelPresets.ts)
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
        // Case and whitespace tolerance
        assert_eq!(host_of("  HTTPS://API.OPENAI.com/"), "API.OPENAI.com");
        // 无 scheme 时按整体 authority 处理
        // Without a scheme, treat everything as an authority
        assert_eq!(host_of("api.deepseek.com"), "api.deepseek.com");
        assert_eq!(host_of(""), "");
    }

    #[test]
    fn test_thinking_fields_off_matches_suppression_table() {
        // enable_thinking 系
        for host in [
            "dashscope.aliyuncs.com",
            "api.siliconflow.cn",
            "api.dashscope.aliyuncs.com",
            "api.siliconflow.com",
            "cloud.siliconflow.cn",
        ] {
            let fields = thinking_fields(host, ThinkingLevel::Off);
            assert_eq!(
                fields,
                vec![("enable_thinking", serde_json::json!(false))],
                "host: {host}"
            );
        }
        // thinking 系
        for host in [
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
        ] {
            let fields = thinking_fields(host, ThinkingLevel::Off);
            assert_eq!(
                fields,
                vec![("thinking", serde_json::json!({ "type": "disabled" }))],
                "host: {host}"
            );
        }
        // reasoning 系
        assert_eq!(
            thinking_fields("openrouter.ai", ThinkingLevel::Off),
            vec![("reasoning", serde_json::json!({ "enabled": false }))]
        );
        // 不命中的服务商：一个字段都不能发
        // Unmatched vendors: not a single extra field may be sent
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
        // Domain boundaries: look-alike spellings of short domains must not match
        assert!(thinking_fields("notz.ai", ThinkingLevel::Off).is_empty());
        assert!(thinking_fields("fake-kimi.com", ThinkingLevel::Off).is_empty());
        // host 大小写不敏感
        // Host matching is case-insensitive
        assert_eq!(
            thinking_fields("API.SILICONFLOW.CN", ThinkingLevel::Off),
            vec![("enable_thinking", serde_json::json!(false))]
        );
    }

    /// 用同一组参数构造 Anthropic 协议 formatter（host 取自 base URL，不发请求）。
    /// Builds an Anthropic-protocol formatter from one parameter set (host comes from the base URL;
    /// no request is sent).
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
        // 照发 temperature——不关思考时模型与回答共享 max_tokens，预算被吃光后可见文本为空。
        // Off + an Anthropic-compatible endpoint that thinks by default: the body carries an
        // explicit disabled, keeps max_tokens, and keeps temperature—without it the model shares
        // max_tokens with the answer and the visible text comes back empty.
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
        // Official Anthropic thinking is opt-in and its newer models reject disabled with a 400,
        // while unknown endpoints may validate strictly: the off level sends no thinking field.
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
        // The on levels are host-independent: the level sets the budget, max_tokens rises to
        // budget + configured, and temperature is dropped (incompatible with extended thinking).
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

    #[test]
    fn test_thinking_fields_levels_enable_per_dialect() {
        // 仅开关的服务商：任意层级都只是开启，不分级
        // On/off-only vendors: any level merely enables, no granularity
        assert_eq!(
            thinking_fields("api.siliconflow.cn", ThinkingLevel::Low),
            vec![("enable_thinking", serde_json::json!(true))]
        );
        assert_eq!(
            thinking_fields("api.deepseek.com", ThinkingLevel::High),
            vec![("thinking", serde_json::json!({ "type": "enabled" }))]
        );
        // OpenRouter：层级映射 reasoning.effort
        // OpenRouter: levels map onto reasoning.effort
        assert_eq!(
            thinking_fields("openrouter.ai", ThinkingLevel::Low),
            vec![("reasoning", serde_json::json!({ "effort": "low" }))]
        );
        assert_eq!(
            thinking_fields("openrouter.ai", ThinkingLevel::High),
            vec![("reasoning", serde_json::json!({ "effort": "high" }))]
        );
        // 未命中表的服务商（含 OpenAI 官方）：层级映射 reasoning_effort
        // Unmatched vendors (OpenAI official included): levels map onto reasoning_effort
        for host in ["api.openai.com", "localhost", ""] {
            assert_eq!(
                thinking_fields(host, ThinkingLevel::Medium),
                vec![("reasoning_effort", serde_json::json!("medium"))],
                "host: {host}"
            );
        }
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
    fn test_strip_thinking_tags() {
        // 闭合块
        // Closed block
        assert_eq!(strip_thinking_tags("<think>想一想</think>正文"), "正文");
        // 未闭合块（流式截断）：剥到末尾
        // Unclosed block (streaming truncation): strip to the end
        assert_eq!(strip_thinking_tags("正文<think>没写完"), "正文");
        // 多个块
        // Multiple blocks
        assert_eq!(
            strip_thinking_tags("<think>a</think>中<think>b</think>尾"),
            "中尾"
        );
        // 大小写不敏感
        // Case-insensitive
        assert_eq!(strip_thinking_tags("<THINK>x</THINK>ok"), "ok");
        // 无标签原样返回
        // No tags: returned unchanged
        assert_eq!(strip_thinking_tags("没有标签"), "没有标签");
        // 全是 think 块则清空
        // All-think input empties out
        assert_eq!(strip_thinking_tags("<think>只有思考</think>"), "");
        assert_eq!(strip_thinking_tags(""), "");
    }

    #[test]
    fn test_build_endpoint_tolerates_common_variants() {
        use protocol::ApiProtocol;
        // 本地 Ollama（origin 无路径）
        // Local Ollama (bare origin)
        assert_eq!(
            build_endpoint("http://localhost:11434", ApiProtocol::OpenAi).unwrap(),
            "http://localhost:11434/v1/chat/completions"
        );
        // 尾部斜杠与空白
        // Trailing slashes and whitespace
        assert_eq!(
            build_endpoint("  http://localhost:11434/ ", ApiProtocol::OpenAi).unwrap(),
            "http://localhost:11434/v1/chat/completions"
        );
        assert_eq!(
            build_endpoint("https://api.moonshot.cn/v1/", ApiProtocol::OpenAi).unwrap(),
            "https://api.moonshot.cn/v1/chat/completions"
        );
        // 已是完整 endpoint 时直接使用
        // Complete endpoints pass through as-is
        assert_eq!(
            build_endpoint("https://x.example/v1/chat/completions", ApiProtocol::OpenAi).unwrap(),
            "https://x.example/v1/chat/completions"
        );
        assert_eq!(
            build_endpoint("https://x.example/v1/messages", ApiProtocol::Anthropic).unwrap(),
            "https://x.example/v1/messages"
        );
        // 空值报错
        // Empty values must error
        assert!(matches!(
            build_endpoint("", ApiProtocol::OpenAi),
            Err(PolisherError::InvalidBaseUrl(_))
        ));
        assert!(build_endpoint("   ", ApiProtocol::Anthropic).is_err());
    }

    #[tokio::test]
    async fn test_polish_success_with_versioned_base_url() {
        // 预设式 base 自带 /v1：endpoint 应为 /v1/chat/completions 而非 /v1/v1/...
        // Preset-style base already carries /v1: endpoint should be /v1/chat/completions, not /v1/v1/...
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
        // mockito's host (127.0.0.1) misses the thinking-suppression table: the request body must match
        // ChatRequest exactly, with no extra fields (OpenAI et al. reject unknown parameters).
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
        // Unmatched host + a level on: the body carries reasoning_effort (OpenAI de-facto standard).
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
        // Anthropic + a level on: the body carries the thinking budget, max_tokens rises to
        // budget + configured, and temperature is absent (incompatible with thinking). Exact body
        // match so a leaked temperature fails the test.
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
        // Relay endpoints inline chains of thought as <think> blocks: the exit path must strip them.
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
        // First response block is a thinking block (no text field): take the first text block
        // instead of erroring.
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
