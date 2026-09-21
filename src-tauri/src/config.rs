//! 配置加载模块。
//!
//! 从 TOML 文件加载 altgo 配置，所有字段均通过 `serde(default)` 提供默认值，
//! 因此部分配置文件也可以正常工作。
//!
//! 文本润色 API 密钥支持通过环境变量覆盖：
//! - `ALTGO_POLISHER_API_KEY` — 覆盖文本润色 API 密钥
//!
//! 默认配置路径为 `~/.config/altgo/altgo.toml`。
//!
//! Configuration loading.
//!
//! Loads altgo config from a TOML file; every field carries a `serde(default)`, so partial config
//! files work fine.
//!
//! The text-polishing API key can be overridden via an environment variable:
//! - `ALTGO_POLISHER_API_KEY` — overrides the polisher API key
//!
//! Default config path: `~/.config/altgo/altgo.toml`.

use crate::error::ConfigError;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// serde 辅助模块：TOML 中为 `u64` 毫秒，Rust 侧为 `Duration`。
/// Serde helper: `u64` milliseconds in TOML, `Duration` on the Rust side.
mod duration_ms {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S: Serializer>(dur: &Duration, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u64(dur.as_millis() as u64)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Duration, D::Error> {
        let ms = u64::deserialize(d)?;
        Ok(Duration::from_millis(ms))
    }
}

/// serde 辅助模块：TOML 中为 `u64` 秒，Rust 侧为 `Duration`。
/// Serde helper: `u64` seconds in TOML, `Duration` on the Rust side.
mod duration_secs {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S: Serializer>(dur: &Duration, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u64(dur.as_secs())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Duration, D::Error> {
        let secs = u64::deserialize(d)?;
        Ok(Duration::from_secs(secs))
    }
}

/// altgo 主配置结构体，包含所有子系统的配置。
/// The root altgo config struct, covering every subsystem.
#[derive(Debug, Default, Deserialize, Clone, serde::Serialize)]
#[serde(default)]
pub struct Config {
    /// 按键监听配置
    /// Key listener settings
    pub key_listener: KeyListenerConfig,
    /// 录音配置
    /// Recording settings
    pub recorder: RecorderConfig,
    /// 语音识别配置
    /// Speech recognition settings
    pub transcriber: TranscriberConfig,
    /// 文本润色配置
    /// Text polishing settings
    pub polisher: PolisherConfig,
    /// 输出（剪切板/通知）配置
    /// Output (clipboard/injection) settings
    pub output: OutputConfig,
    /// GUI 配置
    /// GUI settings
    pub gui: GuiConfig,
}

/// 按键监听配置。
/// Key listener settings.
#[derive(Debug, Deserialize, Clone, serde::Serialize)]
#[serde(default)]
pub struct KeyListenerConfig {
    /// 监听的按键名称（如 `Alt_L`、`Alt_R`），与 xmodmap keysym 一致
    /// Key name to listen for (e.g. `Alt_L`, `Alt_R`), matching an xmodmap keysym
    pub key_name: String,
    /// Linux evtest 回退路径使用的 evdev 键码（由「按下以设置」捕获）；`None` 时沿用 Alt 预设的启发式映射
    /// evdev keycode for the Linux evtest fallback path (captured via "press to set"); when
    /// `None`, the heuristic mapping of the Alt presets applies
    pub linux_evdev_code: Option<u16>,
    /// Windows 使用的虚拟键码（由「按下以设置」捕获）；`None` 时由 `key_name` 解析
    /// Virtual-key code used on Windows (captured via "press to set"); when `None`, resolved
    /// from `key_name`
    pub windows_vk_code: Option<u16>,
    /// 长按阈值（毫秒），超过此时间视为长按录音
    /// Long-press threshold (ms); holding beyond it starts recording
    #[serde(with = "duration_ms", alias = "long_press_threshold_ms")]
    pub long_press_threshold: Duration,
    /// 双击间隔（毫秒），两次点击在此时间窗口内视为双击
    /// Double-click interval (ms); two presses within this window count as a double click
    #[serde(with = "duration_ms", alias = "double_click_interval_ms")]
    pub double_click_interval: Duration,
    /// 最短按下时长（毫秒），过滤 IME 导致的瞬时分合
    /// Minimum press duration (ms), filtering out spurious IME press/release flickers
    #[serde(with = "duration_ms", alias = "min_press_duration_ms")]
    pub min_press_duration: Duration,
}

impl Default for KeyListenerConfig {
    fn default() -> Self {
        Self {
            key_name: "Alt_R".to_string(),
            linux_evdev_code: None,
            windows_vk_code: None,
            long_press_threshold: Duration::from_millis(200),
            double_click_interval: Duration::from_millis(300),
            min_press_duration: Duration::from_millis(100),
        }
    }
}

/// 录音配置。
/// Recording settings.
#[derive(Debug, Deserialize, Clone, serde::Serialize)]
#[serde(default)]
pub struct RecorderConfig {
    /// 采样率（Hz），默认 16000
    /// Sample rate (Hz), default 16000
    pub sample_rate: u32,
}

impl Default for RecorderConfig {
    fn default() -> Self {
        Self {
            sample_rate: crate::recorder::SAMPLE_RATE,
        }
    }
}

/// 本地语音识别配置。
///
/// 遗留的旧版 TOML 字段会被 `serde(default)` 静默忽略
/// （见测试 `test_load_ignores_legacy_cloud_transcriber_fields`）。
///
/// Local speech recognition settings.
///
/// Legacy cloud-transcriber TOML fields are silently ignored through `serde(default)`
/// (see test `test_load_ignores_legacy_cloud_transcriber_fields`).
#[derive(Debug, Deserialize, Clone, serde::Serialize)]
#[serde(default)]
pub struct TranscriberConfig {
    /// 模型名称（如 "sense-voice"）或模型目录路径
    /// Model name (e.g. "sense-voice") or model directory path
    pub model: String,
    /// 语言代码（如 `"zh"`、`"en"`；空字符串表示自动检测）
    /// Language code (e.g. `"zh"`, `"en"`; empty string = auto-detect)
    pub language: String,
    /// 本地引擎线程数；`0` 表示按 CPU 并行度自动取满
    /// Local engine thread count; `0` fills all CPU cores automatically
    pub threads: u32,
}

impl Default for TranscriberConfig {
    fn default() -> Self {
        Self {
            model: String::new(),
            language: "zh".to_string(),
            threads: 0,
        }
    }
}

/// 文本润色配置。
/// Text polishing settings.
#[derive(Debug, Deserialize, Clone, serde::Serialize)]
#[serde(default)]
pub struct PolisherConfig {
    /// API 协议：`"openai"`（OpenAI/DeepSeek 等）或 `"anthropic"`
    /// API protocol: `"openai"` (OpenAI/DeepSeek etc.) or `"anthropic"`
    pub protocol: String,
    /// API 密钥（可通过 `ALTGO_POLISHER_API_KEY` 环境变量覆盖）
    /// API key (overridable via the `ALTGO_POLISHER_API_KEY` env var)
    pub api_key: String,
    /// API 基础 URL（如 `https://api.openai.com`、`https://api.anthropic.com`）
    /// API base URL (e.g. `https://api.openai.com`, `https://api.anthropic.com`)
    pub api_base_url: String,
    /// 模型名称（如 `"gpt-3.5-turbo"`、`"claude-sonnet-4-20250514"`）
    /// Model name (e.g. `"gpt-3.5-turbo"`, `"claude-sonnet-4-20250514"`)
    pub model: String,
    /// 润色级别：`"none"`、`"light"`、`"medium"`、`"heavy"`
    /// Polish level: `"none"`, `"light"`, `"medium"`, `"heavy"`
    pub level: String,
    /// 请求超时时间（秒）
    /// Request timeout (seconds)
    #[serde(with = "duration_secs", alias = "timeout_seconds")]
    pub timeout: Duration,
    /// 最大生成 token 数
    /// Max generated tokens
    pub max_tokens: u32,
    /// LLM temperature（0.0 - 2.0），默认 0.3
    /// LLM temperature (0.0 - 2.0), default 0.3
    pub temperature: f32,
    /// 思考层级：`"off"`（默认，自动关闭思考）、`"low"`、`"medium"`、`"high"`
    /// Thinking level: `"off"` (default, thinking auto-off), `"low"`, `"medium"`, `"high"`
    pub thinking_level: String,
    /// 自定义 system prompt，为空时使用内置 prompt
    /// Custom system prompt; the built-in prompt applies when empty
    pub system_prompt: String,
}

impl Default for PolisherConfig {
    fn default() -> Self {
        Self {
            protocol: "openai".to_string(),
            api_key: String::new(),
            api_base_url: String::new(),
            model: String::new(),
            level: "none".to_string(),
            timeout: Duration::from_secs(60),
            max_tokens: 1024,
            temperature: 0.3,
            thinking_level: "off".to_string(),
            system_prompt: String::new(),
        }
    }
}

/// 输出配置。
/// Output settings.
#[derive(Debug, Deserialize, Clone, serde::Serialize)]
#[serde(default)]
pub struct OutputConfig {
    /// 注入/复制时是否优先使用润色后的文本
    /// Whether injection/copy prefers polished text over raw transcription
    pub prefer_polished: bool,
    /// 转写完成后是否把文本注入到当前焦点窗口（仅 Windows 实现注入；默认关闭，仅写剪贴板）
    /// Whether to inject text into the focused window after transcription (implemented on Windows
    /// only; off by default—clipboard only)
    pub inject_text: bool,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            prefer_polished: true,
            inject_text: false,
        }
    }
}

/// GUI 配置。
/// GUI settings.
#[derive(Debug, Deserialize, Clone, serde::Serialize)]
#[serde(default)]
pub struct GuiConfig {
    /// 界面语言：`"zh"` 或 `"en"`
    /// UI language: `"zh"` or `"en"`
    pub language: String,
    /// 悬浮窗位置：`"bottom_center"`（默认）或 `"top_center"`
    /// Overlay position: `"bottom_center"` (default) or `"top_center"`
    pub overlay_position: String,
    /// 启动时是否自动检查更新（默认开启）
    /// Whether to check updates automatically at startup (on by default)
    pub auto_check_update: bool,
}

impl Default for GuiConfig {
    fn default() -> Self {
        Self {
            language: "zh".to_string(),
            overlay_position: "bottom_center".to_string(),
            auto_check_update: true,
        }
    }
}

impl Config {
    /// 从指定路径加载配置文件。如果文件不存在，返回默认配置。
    /// 环境变量 `ALTGO_POLISHER_API_KEY`
    /// 会覆盖配置文件中的对应字段。
    /// Loads the config from the given path. A missing file yields the default config.
    /// The `ALTGO_POLISHER_API_KEY` env var overrides the corresponding file field.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        Self::load_with_env(path, |name| std::env::var(name))
    }

    fn load_with_env<E>(path: &Path, env_var: E) -> Result<Self, ConfigError>
    where
        E: Fn(&str) -> Result<String, std::env::VarError>,
    {
        let mut cfg = if path.exists() {
            let content = std::fs::read_to_string(path)?;
            toml::from_str(&content)
                .map_err(|e| ConfigError::ParseError(format!("parse {}: {}", path.display(), e)))?
        } else {
            Config::default()
        };

        apply_api_key_overrides(&mut cfg, env_var);

        Ok(cfg)
    }

    /// 校验已加载的配置。
    /// 在 `load()` 之后调用；润色开启时检查 [polisher] 的 API key。
    /// Validate the loaded configuration.
    /// Call this after `load()` to check the polisher API key when polishing is enabled.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.recorder.sample_rate != crate::recorder::SAMPLE_RATE {
            return Err(ConfigError::ValidationFailed(format!(
                "SenseVoice 只支持 {}Hz 单声道输入，请将 [recorder] sample_rate 设为 {}。",
                crate::recorder::SAMPLE_RATE,
                crate::recorder::SAMPLE_RATE
            )));
        }

        // 润色开启时逐项校验 [polisher] 必填字段，错误信息指明缺失项。
        // When polishing is enabled, validate each required [polisher] field and point at missing ones.
        if self.polisher.level != "none" {
            let protocol = self.polisher.protocol.trim().to_lowercase();
            if protocol != "openai" && protocol != "anthropic" {
                return Err(ConfigError::ValidationFailed(format!(
                    "润色协议（protocol）无效：'{}'，应为 \"openai\" 或 \"anthropic\"。",
                    self.polisher.protocol
                )));
            }

            let mut missing: Vec<&str> = Vec::new();
            if self.polisher.api_key.trim().is_empty() {
                missing.push("API 密钥（api_key）");
            }
            if self.polisher.api_base_url.trim().is_empty() {
                missing.push("API 地址（api_base_url）");
            }
            if self.polisher.model.trim().is_empty() {
                missing.push("模型名称（model）");
            }
            if !missing.is_empty() {
                return Err(ConfigError::ValidationFailed(format!(
                    "润色功能已开启（level = \"{}\"），但缺少：{}。请在设置的「润色」分区补全，或编辑配置文件 {} 的 [polisher] 段；不需要润色时把级别设为「关闭」。密钥也可通过环境变量 ALTGO_POLISHER_API_KEY 设置。",
                    self.polisher.level,
                    missing.join("、"),
                    Self::default_config_path().display()
                )));
            }
        }
        Ok(())
    }

    /// 将配置保存到指定路径。
    /// Saves the config to the given path.
    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        let content = toml::to_string_pretty(self).map_err(|e| {
            ConfigError::SerializeError(format!("failed to serialize config to TOML: {e}"))
        })?;

        // 确保父目录存在。
        // Ensure parent directory exists.
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        std::fs::write(path, content)?;

        // 文件权限收紧为仅属主可读（保护落盘的 API key）。
        // Restrict file permissions to owner-only (protect API keys at rest).
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        }

        tracing::info!(path = %path.display(), "config saved");
        Ok(())
    }

    /// 返回默认配置文件路径（`~/.config/altgo/altgo.toml`）。
    /// Returns the default config file path (`~/.config/altgo/altgo.toml`).
    pub fn default_config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("altgo")
            .join("altgo.toml")
    }
}

fn apply_api_key_overrides<E>(cfg: &mut Config, env_var: E)
where
    E: Fn(&str) -> Result<String, std::env::VarError>,
{
    if let Ok(key) = env_var("ALTGO_POLISHER_API_KEY") {
        cfg.polisher.api_key = key;
    }
}

// ---------------------------------------------------------------------------
// ConfigPatch — partial update for Config
// ConfigPatch —— Config 的部分更新
// ---------------------------------------------------------------------------

/// 三态反序列化：JSON 字段缺失 = 不修改；`null` = 清除；值 = 设置。
///
/// 泛型实现，适用于任意可从 JSON 值反序列化的类型。
///
/// Three-state deserialization: JSON field absent = no change; `null` = clear; value = set.
///
/// Generic, works with any type deserializable from a JSON value.
fn deserialize_opt_patch<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: DeserializeOwned,
{
    let v = serde_json::Value::deserialize(deserializer)?;
    match v {
        serde_json::Value::Null => Ok(Some(None)),
        other => {
            let val: T = serde_json::from_value(other)
                .map_err(|e| serde::de::Error::custom(e.to_string()))?;
            Ok(Some(Some(val)))
        }
    }
}

fn apply_nested_opt<T>(target: &mut Option<T>, patch: Option<Option<T>>) {
    match patch {
        None => {}
        Some(None) => *target = None,
        Some(Some(v)) => *target = Some(v),
    }
}

/// serde 辅助模块：将 `deserialize_opt_patch<u16>` 暴露为模块形式供 `deserialize_with` 使用。
/// Serde helper: re-exports `deserialize_opt_patch<u16>` as a module form for `deserialize_with`.
mod opt_patch_u16 {
    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Option<u16>>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        super::deserialize_opt_patch::<D, u16>(deserializer)
    }
}

/// 应用于内存中配置的部分更新。全部字段可选；
/// 缺省字段保持不变。
/// Partial update applied to the in-memory config. All fields are optional;
/// absent fields are left unchanged.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigPatch {
    /// 按键名称；缺省不修改。
    /// Key name; absent means no change.
    pub key_name: Option<String>,
    /// 三态更新：`None` = 字段缺省（不修改）；`Some(None)` = JSON `null`（清除）；
    /// `Some(Some(v))` = 设为 v。
    ///
    /// Three-state update: `None` = field absent (no change); `Some(None)` = JSON `null`
    /// (clear); `Some(Some(v))` = set to v.
    #[serde(default, deserialize_with = "opt_patch_u16::deserialize")]
    pub linux_evdev_code: Option<Option<u16>>,
    /// 三态更新：`None` = 字段缺省（不修改）；`Some(None)` = JSON `null`（清除）；
    /// `Some(Some(v))` = 设为 v。
    ///
    /// Three-state update: `None` = field absent (no change); `Some(None)` = JSON `null`
    /// (clear); `Some(Some(v))` = set to v.
    #[serde(default, deserialize_with = "opt_patch_u16::deserialize")]
    pub windows_vk_code: Option<Option<u16>>,
    /// 三态更新：`None` = 字段缺省（不修改）；`Some(None)` = JSON `null`（清除）；
    /// `Some(Some(v))` = 设为 v。
    ///
    /// Three-state update: `None` = field absent (no change); `Some(None)` = JSON `null`
    /// (clear); `Some(Some(v))` = set to v.
    pub language: Option<String>,
    pub model: Option<String>,
    pub polish_level: Option<String>,
    pub polish_model: Option<String>,
    pub polish_protocol: Option<String>,
    pub polish_thinking_level: Option<String>,
    pub polish_api_key: Option<String>,
    pub polish_api_base_url: Option<String>,
    pub gui_language: Option<String>,
    pub overlay_position: Option<String>,
    pub auto_check_update: Option<bool>,
    pub inject_text: Option<bool>,
}

impl ConfigPatch {
    /// 将 patch 中的 `Some` 字段写入 `cfg`。
    /// Writes every `Some` field of the patch into `cfg`.
    pub fn apply_to_config(&self, cfg: &mut Config) {
        if let Some(ref v) = self.key_name {
            cfg.key_listener.key_name = v.clone();
        }
        apply_nested_opt(
            &mut cfg.key_listener.linux_evdev_code,
            self.linux_evdev_code,
        );
        apply_nested_opt(&mut cfg.key_listener.windows_vk_code, self.windows_vk_code);
        if let Some(ref v) = self.language {
            cfg.transcriber.language = v.clone();
        }
        if let Some(ref v) = self.model {
            cfg.transcriber.model = v.clone();
        }
        if let Some(ref v) = self.polish_level {
            cfg.polisher.level = v.clone();
        }
        if let Some(ref v) = self.polish_model {
            cfg.polisher.model = v.clone();
        }
        if let Some(ref v) = self.polish_protocol {
            cfg.polisher.protocol = v.clone();
        }
        if let Some(v) = &self.polish_thinking_level {
            cfg.polisher.thinking_level = v.clone();
        }
        if let Some(ref v) = self.polish_api_key {
            cfg.polisher.api_key = v.clone();
        }
        if let Some(ref v) = self.polish_api_base_url {
            cfg.polisher.api_base_url = v.clone();
        }
        if let Some(ref v) = self.gui_language {
            cfg.gui.language = v.clone();
        }
        if let Some(ref v) = self.overlay_position {
            cfg.gui.overlay_position = v.clone();
        }
        if let Some(v) = self.auto_check_update {
            cfg.gui.auto_check_update = v;
        }
        if let Some(v) = self.inject_text {
            cfg.output.inject_text = v;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_default_config() {
        let cfg = Config::default();
        assert_eq!(cfg.key_listener.key_name, "Alt_R");
        assert!(cfg.key_listener.linux_evdev_code.is_none());
        assert_eq!(
            cfg.key_listener.long_press_threshold,
            Duration::from_millis(200)
        );
        assert_eq!(
            cfg.key_listener.min_press_duration,
            Duration::from_millis(100)
        );
        assert_eq!(cfg.recorder.sample_rate, 16000);
        assert_eq!(cfg.transcriber.model, "");
        assert_eq!(cfg.transcriber.language, "zh");
        assert_eq!(cfg.polisher.level, "none");
        assert_eq!(cfg.polisher.temperature, 0.3);
        assert_eq!(cfg.polisher.thinking_level, "off");
        assert!(cfg.output.prefer_polished);
        assert!(
            !cfg.output.inject_text,
            "inject_text 应默认关闭（仅写剪贴板）"
        );
        assert!(cfg.gui.auto_check_update);
    }

    #[test]
    fn test_load_nonexistent_file() {
        let cfg = Config::load(Path::new("/nonexistent/altgo.toml")).unwrap();
        assert_eq!(cfg.recorder.sample_rate, 16000);
    }

    #[test]
    fn test_load_valid_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("altgo.toml");
        let mut f = std::fs::File::create(&path).unwrap();
        write!(
            f,
            r#"
[key_listener]
key_name = "Alt_R"

[recorder]
sample_rate = 48000

[transcriber]
model = "sense-voice"
language = "en"

[polisher]
level = "heavy"
thinking_level = "low"

[output]
prefer_polished = false
inject_text = true
"#
        )
        .unwrap();

        let cfg = Config::load(&path).unwrap();
        assert_eq!(cfg.key_listener.key_name, "Alt_R");
        assert_eq!(cfg.recorder.sample_rate, 48000);
        assert_eq!(cfg.transcriber.model, "sense-voice");
        assert_eq!(cfg.transcriber.language, "en");
        assert_eq!(cfg.polisher.level, "heavy");
        assert_eq!(cfg.polisher.thinking_level, "low");
        assert!(!cfg.output.prefer_polished);
        assert!(cfg.output.inject_text);
    }

    #[test]
    fn test_load_ignores_legacy_cloud_transcriber_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("altgo.toml");
        std::fs::write(
            &path,
            r#"
[transcriber]
engine = "api"
api_key = "legacy-key"
api_base_url = "https://api.openai.com"
temperature = 0.2
prompt = "legacy prompt"
model = "sense-voice"
language = "zh"
"#,
        )
        .unwrap();

        let cfg = Config::load(&path).unwrap();
        assert_eq!(cfg.transcriber.model, "sense-voice");
        assert_eq!(cfg.transcriber.language, "zh");
    }

    #[test]
    fn test_load_invalid_toml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("altgo.toml");
        std::fs::write(&path, "this is not valid [[[").unwrap();
        assert!(Config::load(&path).is_err());
    }

    #[test]
    fn test_env_override() {
        let cfg = Config::load_with_env(Path::new("/nonexistent.toml"), |name| match name {
            "ALTGO_POLISHER_API_KEY" => Ok("test-polish-key".to_string()),
            _ => Err(std::env::VarError::NotPresent),
        })
        .unwrap();

        assert_eq!(cfg.polisher.api_key, "test-polish-key");
    }

    #[test]
    fn test_duration_fields() {
        let cfg = Config::default();
        assert_eq!(cfg.polisher.timeout, Duration::from_secs(60));
        assert_eq!(
            cfg.key_listener.long_press_threshold,
            Duration::from_millis(200)
        );
        assert_eq!(
            cfg.key_listener.double_click_interval,
            Duration::from_millis(300)
        );
        assert_eq!(
            cfg.key_listener.min_press_duration,
            Duration::from_millis(100)
        );
    }

    #[test]
    fn test_default_config_path() {
        let path = Config::default_config_path();
        assert!(path.to_string_lossy().contains("altgo"));
        assert!(path.to_string_lossy().ends_with("altgo.toml"));
    }

    #[test]
    fn test_validate_local_mode_no_polisher_key() {
        // 本地转写且润色关闭时不应要求 API key。
        // Local transcription with polishing disabled should not require an API key.
        let mut cfg = Config::default();
        cfg.polisher.level = "none".to_string();
        cfg.polisher.api_key = String::new();
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn test_validate_rejects_non_sensevoice_sample_rate() {
        let mut cfg = Config::default();
        cfg.recorder.sample_rate = 48_000;
        let err = cfg.validate().unwrap_err();
        assert!(err.to_string().contains("16000"));
    }

    #[test]
    fn test_validate_polisher_requires_key_when_enabled() {
        // 润色级别 != "none" 时必须有 API key。
        // When polisher level != "none", API key is required.
        let mut cfg = Config::default();
        cfg.polisher.level = "medium".to_string();
        cfg.polisher.api_key = String::new();
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_validate_polisher_lists_missing_fields() {
        let mut cfg = Config::default();
        cfg.polisher.level = "light".to_string();
        cfg.polisher.api_key = "k".to_string();
        let err = cfg.validate().unwrap_err().to_string();
        assert!(err.contains("API 地址"));
        assert!(err.contains("模型名称"));
        assert!(!err.contains("API 密钥"));

        cfg.polisher.api_base_url = "https://api.deepseek.com".to_string();
        cfg.polisher.model = "deepseek-chat".to_string();
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn test_validate_polisher_rejects_unknown_protocol() {
        let mut cfg = Config::default();
        cfg.polisher.level = "light".to_string();
        cfg.polisher.api_key = "k".to_string();
        cfg.polisher.api_base_url = "https://x.example".to_string();
        cfg.polisher.model = "m".to_string();
        cfg.polisher.protocol = "graphql".to_string();
        let err = cfg.validate().unwrap_err().to_string();
        assert!(err.contains("protocol"));
        cfg.polisher.protocol = "Anthropic".to_string();
        assert!(cfg.validate().is_ok());
    }

    // -- ConfigPatch 测试 -----------------------------------------------------
    // -- ConfigPatch tests ---------------------------------------------------

    #[test]
    fn evdev_json_null_clears() {
        let j = r#"{"linuxEvdevCode":null}"#;
        let p: ConfigPatch = serde_json::from_str(j).unwrap();
        assert_eq!(p.linux_evdev_code, Some(None));
    }

    #[test]
    fn evdev_missing_field_means_no_patch() {
        let j = r#"{}"#;
        let p: ConfigPatch = serde_json::from_str(j).unwrap();
        assert!(p.linux_evdev_code.is_none());
    }

    #[test]
    fn evdev_number_sets() {
        let j = r#"{"linuxEvdevCode":100}"#;
        let p: ConfigPatch = serde_json::from_str(j).unwrap();
        assert_eq!(p.linux_evdev_code, Some(Some(100)));
    }

    #[test]
    fn patch_apply_to_config_updates_selected_fields() {
        let mut cfg = Config::default();
        let patch: ConfigPatch =
            serde_json::from_str(r#"{"keyName":"space","language":"en","polishLevel":"heavy"}"#)
                .unwrap();
        patch.apply_to_config(&mut cfg);
        assert_eq!(cfg.key_listener.key_name, "space");
        assert_eq!(cfg.transcriber.language, "en");
        assert_eq!(cfg.polisher.level, "heavy");
        // 未修改的字段保留默认值
        // Unchanged fields keep defaults
        assert_eq!(cfg.transcriber.model, "");
    }

    #[test]
    fn patch_apply_polish_protocol_updates_field() {
        let mut cfg = Config::default();
        let patch: ConfigPatch = serde_json::from_str(r#"{"polishProtocol":"anthropic"}"#).unwrap();
        patch.apply_to_config(&mut cfg);
        assert_eq!(cfg.polisher.protocol, "anthropic");
    }

    #[test]
    fn patch_apply_polish_thinking_level_updates_field() {
        let mut cfg = Config::default();
        assert_eq!(cfg.polisher.thinking_level, "off");
        let patch: ConfigPatch = serde_json::from_str(r#"{"polishThinkingLevel":"high"}"#).unwrap();
        patch.apply_to_config(&mut cfg);
        assert_eq!(cfg.polisher.thinking_level, "high");
    }

    #[test]
    fn patch_apply_evdev_null_clears() {
        let mut cfg = Config::default();
        cfg.key_listener.linux_evdev_code = Some(56);
        let patch: ConfigPatch = serde_json::from_str(r#"{"linuxEvdevCode":null}"#).unwrap();
        patch.apply_to_config(&mut cfg);
        assert!(cfg.key_listener.linux_evdev_code.is_none());
    }

    #[test]
    fn patch_apply_inject_text_updates_field() {
        let mut cfg = Config::default();
        let patch: ConfigPatch = serde_json::from_str(r#"{"injectText":true}"#).unwrap();
        patch.apply_to_config(&mut cfg);
        assert!(cfg.output.inject_text);

        // 缺省字段不修改
        // Absent fields leave values unchanged
        let patch: ConfigPatch = serde_json::from_str(r#"{}"#).unwrap();
        patch.apply_to_config(&mut cfg);
        assert!(cfg.output.inject_text);
    }

    #[test]
    #[cfg(unix)]
    fn test_save_file_permissions_unix() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("altgo.toml");

        let cfg = Config::default();
        cfg.save(&path).unwrap();

        let metadata = std::fs::metadata(&path).unwrap();
        let permissions = metadata.permissions();
        assert_eq!(permissions.mode() & 0o777, 0o600);
    }
}
