//! 本地润色引擎：把润色请求发到本机的润色运行时。
//!
//! 与远端引擎共用请求发送与响应解析（`LLMFormatter`），差别只在三处：端点由运行期决定
//! （惰性启动的子进程与随机密钥），生成参数固定（温度 0、输出上限、关闭思考），
//! system prompt 由本地档自己解析。运行时进程随本对象存活，空闲与停止时回收。

use super::fidelity;
use super::formatter::LLMFormatter;
use super::level::PolishLevel;
use super::prompt::{build_prompt_source_chain, get_local_system_prompt, SystemPromptSource};
use super::Polisher;
use crate::error::{PolishRuntimeError, PolisherError};
use crate::polish_runtime::{LocalRuntime, MODEL_ALIAS};
use reqwest::Client;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
#[cfg(test)]
use std::time::Duration;

/// 本地档输出上限。实测三十条真实语料的输出中位 20 token、p90 落后 67 token，
/// 192 留足长句余量，同时把最坏情况下的等待时间钉住。
const LOCAL_MAX_TOKENS: u32 = 192;

/// 本地档温度固定为 0：整理任务要可复现，采样只会带来改写风险。
const LOCAL_TEMPERATURE: f32 = 0.0;

pub struct LocalPolisher {
    runtime: Arc<LocalRuntime>,
    /// 配置里的 `[polisher] local_model`：已知模型名或自定义 GGUF 路径。
    model_setting: String,
    client: Client,
    language: String,
    prompt_source: Option<Box<dyn SystemPromptSource>>,
}

impl LocalPolisher {
    /// 按配置构造，运行时取进程内共享的那一份（与设置页命令看到同一份状态）。
    pub fn new(cfg: &crate::config::Config) -> Result<Self, PolisherError> {
        Self::with_runtime(crate::polish_runtime::active(), cfg)
    }

    /// 注入运行时管理器，供测试与手动放置场景使用。
    pub fn with_runtime(
        runtime: Arc<LocalRuntime>,
        cfg: &crate::config::Config,
    ) -> Result<Self, PolisherError> {
        let client = Client::builder()
            .timeout(cfg.polisher.timeout)
            .build()
            .map_err(|e| PolisherError::HttpError(format!("failed to build HTTP client: {}", e)))?;
        Ok(Self {
            runtime,
            model_setting: cfg.polisher.local_model.clone(),
            client,
            language: cfg.transcriber.language.clone(),
            prompt_source: build_prompt_source_chain(cfg),
        })
    }

    /// 解析本地模型文件路径。
    fn model_path(&self) -> Result<PathBuf, PolisherError> {
        crate::polish_model::resolve_model_path(&self.model_setting).ok_or_else(|| {
            PolisherError::LocalRuntime(format!(
                "找不到本地润色模型“{}”。请在设置页下载，或把 [polisher] local_model 指向一个存在的 GGUF 文件。",
                self.model_setting
            ))
        })
    }

    /// 解析本地档的 system prompt：来源链失败或缺位时用本地档的短提示词兜底
    /// （内置的长提示词是给云端强模型写的，小模型遵循度差且预填更贵）。
    fn resolve_system_prompt(&self, level: PolishLevel) -> String {
        self.prompt_source
            .as_ref()
            .and_then(|source| source.get_prompt(level, &self.language).ok())
            .unwrap_or_else(|| get_local_system_prompt(&self.language))
    }

    /// 确保运行时已就绪，并构造一次调用用的请求客户端。
    async fn formatter(&self) -> Result<LLMFormatter, PolisherError> {
        let model = self.model_path()?;
        let endpoint = self
            .runtime
            .endpoint(&model)
            .await
            .map_err(to_polisher_error)?;
        Ok(LLMFormatter::from_endpoint(
            self.client.clone(),
            endpoint.api_key,
            endpoint.base_url,
            MODEL_ALIAS.to_string(),
            LOCAL_MAX_TOKENS,
            LOCAL_TEMPERATURE,
            self.language.clone(),
        ))
    }
}

impl Polisher for LocalPolisher {
    fn polish<'a>(
        &'a self,
        text: &'a str,
        level: PolishLevel,
    ) -> Pin<Box<dyn Future<Output = Result<String, PolisherError>> + Send + 'a>> {
        Box::pin(async move {
            if matches!(level, PolishLevel::None) || text.is_empty() {
                return Ok(text.to_string());
            }
            let formatter = self.formatter().await?;
            let polished = formatter
                .polish_with_system_prompt(self.resolve_system_prompt(level), text)
                .await?;
            fidelity::apply_guard(text, &polished)
        })
    }

    fn polish_with_instruction<'a>(
        &'a self,
        text: &'a str,
        level: PolishLevel,
        instruction: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<String, PolisherError>> + Send + 'a>> {
        Box::pin(async move {
            if matches!(level, PolishLevel::None) || text.is_empty() {
                return Ok(text.to_string());
            }
            let instruction = instruction.trim();
            if instruction.is_empty() {
                return self.polish(text, level).await;
            }
            let formatter = self.formatter().await?;
            let system_prompt = format!(
                "{}\n\n补充要求：{}",
                self.resolve_system_prompt(level),
                instruction
            );
            let polished = formatter
                .polish_with_system_prompt(system_prompt, text)
                .await?;
            fidelity::apply_guard(text, &polished)
        })
    }

    fn release_idle<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<bool, PolisherError>> + Send + 'a>> {
        Box::pin(async move {
            self.runtime
                .release_if_idle()
                .await
                .map_err(to_polisher_error)
        })
    }

    fn shutdown<'a>(&'a self) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            if let Err(error) = self.runtime.shutdown().await {
                tracing::warn!(error = %error, "failed to stop polish runtime");
            }
        })
    }
}

/// 运行时错误映射成润色域的错误，用户文案由 `PolisherError::LocalRuntime` 给出。
fn to_polisher_error(error: PolishRuntimeError) -> PolisherError {
    PolisherError::LocalRuntime(error.message())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::polish_runtime::{LaunchConfig, PlatformRuntimeProcess};
    use tempfile::TempDir;

    fn local_config(local_model: &str) -> crate::config::Config {
        let mut cfg = crate::config::Config::default();
        cfg.polisher.backend = "local".to_string();
        cfg.polisher.local_model = local_model.to_string();
        cfg.polisher.level = "medium".to_string();
        cfg.polisher.timeout = Duration::from_secs(5);
        cfg
    }

    fn polisher_with_temp_runtime(dir: &TempDir, cfg: &crate::config::Config) -> LocalPolisher {
        let runtime = Arc::new(LocalRuntime::with_dir(
            dir.path().to_path_buf(),
            LaunchConfig::default(),
            PlatformRuntimeProcess::new(),
        ));
        LocalPolisher::with_runtime(runtime, cfg).unwrap()
    }

    #[tokio::test]
    async fn test_none_level_returns_text_without_touching_runtime() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = local_config("qwen3-0.6b");
        let polisher = polisher_with_temp_runtime(&dir, &cfg);
        let text = polisher
            .polish("嗯，明天上午十点开会。", PolishLevel::None)
            .await
            .unwrap();
        assert_eq!(text, "嗯，明天上午十点开会。");
        assert!(!polisher.runtime.is_running());
    }

    #[tokio::test]
    async fn test_missing_model_reports_actionable_error() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = local_config("不存在的模型.gguf");
        let polisher = polisher_with_temp_runtime(&dir, &cfg);

        let error = polisher
            .polish("随便一句话", PolishLevel::Medium)
            .await
            .unwrap_err();
        let message = error.to_string();
        assert!(message.contains("不存在的模型.gguf"), "{message}");
        match &error {
            PolisherError::LocalRuntime(detail) => assert!(detail.contains("找不到本地润色模型")),
            other => panic!("应当是运行时错误，实际：{other:?}"),
        }
        assert!(!polisher.runtime.is_running(), "缺模型时不应启动运行时");
    }

    #[tokio::test]
    async fn test_model_present_but_runtime_missing_reports_incomplete() {
        let dir = tempfile::tempdir().unwrap();
        let model = dir.path().join("custom.gguf");
        std::fs::write(&model, vec![0u8; 1024]).unwrap();
        let cfg = local_config(model.to_str().unwrap());
        let polisher = polisher_with_temp_runtime(&dir, &cfg);

        let error = polisher
            .polish("随便一句话", PolishLevel::Medium)
            .await
            .unwrap_err();
        let message = error.to_string();
        assert!(message.contains("不完整"), "{message}");
        assert!(!polisher.runtime.is_running());
    }

    #[tokio::test]
    async fn test_release_idle_is_noop_when_nothing_running() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = local_config("qwen3-0.6b");
        let polisher = polisher_with_temp_runtime(&dir, &cfg);
        assert!(!polisher.release_idle().await.unwrap());
        polisher.shutdown().await;
    }
}
