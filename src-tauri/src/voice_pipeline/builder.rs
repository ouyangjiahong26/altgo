//! PipelineBuilder — 组件构造。

use std::sync::{Arc, Mutex};

use crate::error::{FatalError, PipelineError};
use crate::key_listener::KeyListener;
use crate::polisher::{LLMFormatter, PolishLevel};
use crate::recorder::Recorder;
use crate::transcriber::Transcriber;

use super::context::PipelineContext;

/// 从配置构建流水线各组件。
pub struct PipelineBuilder {
    cfg: Arc<crate::config::Config>,
}

impl PipelineBuilder {
    pub fn new(cfg: Arc<crate::config::Config>) -> Self {
        Self { cfg }
    }

    /// 从配置构建录音器。
    pub fn build_recorder(&self) -> Box<dyn Recorder> {
        Box::new(crate::recorder::PlatformRecorder::new(
            self.cfg.recorder.sample_rate,
        ))
    }

    /// 从配置构建转写引擎。
    ///
    /// `backend = "online"` 走小米 MiMo 在线识别（纯网络调用，不触碰本地模型解析）；
    /// 其余（默认 `"local"`）走 sherpa-onnx SenseVoice，本地模型缺失或加载失败时返回错误。
    pub fn build_transcriber(&self) -> Result<Box<dyn Transcriber>, PipelineError> {
        let cfg = &self.cfg.transcriber;

        if cfg.backend.trim().eq_ignore_ascii_case("online") {
            let o = &cfg.online;
            let asr = crate::mimo_asr::MimoAsr::new(
                &o.api_key,
                &o.api_base_url,
                &o.model,
                &cfg.language,
                o.timeout,
            )
            .map_err(PipelineError::fatal_transcriber)?;
            return Ok(Box::new(asr));
        }

        let model_dir = match crate::model::resolve_model_dir(&cfg.model) {
            Some(d) => d,
            None => {
                return Err(PipelineError::Fatal(FatalError::ModelNotFound {
                    model: cfg.model.clone(),
                    searched: vec![dirs::config_dir().unwrap_or_default().join("altgo/models")],
                }));
            }
        };

        let transcriber =
            crate::sherpa::SherpaTranscriber::new(model_dir, cfg.language.clone(), cfg.threads)
                .map_err(PipelineError::fatal_transcriber)?;

        Ok(Box::new(transcriber))
    }

    /// 按配置构建润色器。
    ///
    /// 协议未知或 HTTP 客户端初始化失败时返回错误。
    /// 通过 `LLMFormatter::from_config_with_sources` 共享工厂构造，确保与
    /// IPC handler（`cmd::polish_history_entry`）走同一条 prompt source chain。
    pub fn build_polisher(&self) -> Result<LLMFormatter, PipelineError> {
        LLMFormatter::from_config_with_sources(&self.cfg).map_err(PipelineError::fatal_polisher)
    }

    /// 从配置构建按键监听器。
    ///
    /// 返回 boxed trait object，供流水线跨平台使用。
    pub fn build_key_listener(&self) -> Result<Box<dyn KeyListener>, PipelineError> {
        let listener =
            crate::key_listener::PlatformListener::new(&self.cfg.key_listener).map_err(|e| {
                PipelineError::Fatal(FatalError::KeyListenerFailed {
                    backend: "platform".to_string(),
                    reason: e.to_string(),
                })
            })?;
        Ok(Box::new(listener))
    }

    /// 从配置读取润色级别。
    pub fn polish_level(&self) -> PolishLevel {
        PolishLevel::effective(&self.cfg.polisher.level)
    }

    /// 从配置构建完整的流水线上下文。
    pub fn build_context(&self) -> Result<PipelineContext, PipelineError> {
        let recorder = self.build_recorder();
        let transcriber = self.build_transcriber()?;
        let formatter = self.build_polisher()?;
        let polish_level = self.polish_level();
        let listener = self.build_key_listener()?;

        Ok(PipelineContext {
            recorder,
            transcriber,
            formatter,
            polish_level,
            listener: Mutex::new(Some(listener)),
            long_press_threshold: self.cfg.key_listener.long_press_threshold,
            double_click_interval: self.cfg.key_listener.double_click_interval,
            min_press_duration: self.cfg.key_listener.min_press_duration,
        })
    }
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::polisher::PolishLevel;
    use std::sync::Arc;

    fn test_config() -> crate::config::Config {
        crate::config::Config::default()
    }

    #[test]
    fn test_build_recorder() {
        let cfg = Arc::new(test_config());
        let builder = PipelineBuilder::new(cfg);
        let _recorder = builder.build_recorder();
    }

    #[test]
    fn test_build_transcriber_local_model_not_found() {
        use crate::error::{FatalError, PipelineError};

        let mut cfg = test_config();
        cfg.transcriber.model = "nonexistent-model".to_string();

        let builder = PipelineBuilder::new(Arc::new(cfg));
        let err = match builder.build_transcriber() {
            Ok(_) => panic!("expected error"),
            Err(e) => e,
        };
        assert!(err.is_fatal());
        assert!(matches!(
            err,
            PipelineError::Fatal(FatalError::ModelNotFound { .. })
        ));
    }

    #[test]
    fn test_build_transcriber_online_skips_local_model() {
        // 在线后端不解析本地模型：无模型目录也应构造成功。
        let mut cfg = test_config();
        cfg.transcriber.backend = "online".to_string();
        cfg.transcriber.online.api_key = "k".to_string();

        let builder = PipelineBuilder::new(Arc::new(cfg));
        assert!(builder.build_transcriber().is_ok());
    }

    #[test]
    fn test_build_polisher_unknown_protocol() {
        use crate::error::{FatalError, PipelineError, PolisherError};

        let mut cfg = test_config();
        cfg.polisher.protocol = "unknown-protocol".to_string();

        let builder = PipelineBuilder::new(Arc::new(cfg));
        let result = builder.build_polisher();

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.is_fatal());
        assert!(matches!(
            err,
            PipelineError::Fatal(FatalError::PolisherInitFailed(
                PolisherError::UnknownProtocol { .. }
            ))
        ));
    }

    #[test]
    fn test_polish_level() {
        let mut cfg = test_config();
        cfg.polisher.level = "heavy".to_string();

        let builder = PipelineBuilder::new(Arc::new(cfg));
        let level = builder.polish_level();

        assert_eq!(level, PolishLevel::Heavy);
    }

    // 端到端入口测试：`run()` 必须在 build_context 失败时把错误上报到 sink。
    // 故障点属于 builder（构建上下文），因此下沉到本模块。
    #[tokio::test]
    async fn run_reports_error_when_context_build_fails() {
        use crate::voice_pipeline::sink::TranscriptionResult;
        use std::sync::Mutex;

        struct ErrorSink {
            errors: Arc<Mutex<Vec<String>>>,
        }
        impl crate::voice_pipeline::sink::PipelineSink for ErrorSink {
            fn on_status_change(&self, _: crate::pipeline_controller::PipelineStatus) {}
            fn on_error(&self, msg: &str) {
                self.errors.lock().unwrap().push(msg.to_string());
            }
            fn on_transcription_result(&self, _: &TranscriptionResult) {}
            fn on_progress(&self, _: &str, _: Option<f32>) {}
            fn on_key_listener_backend(&self, _: &str) {}
        }

        // 用未知的 polisher protocol 强制 build_context 失败。
        let mut cfg = test_config();
        cfg.polisher.protocol = "unknown".to_string();
        let errors = Arc::new(Mutex::new(Vec::new()));
        let sink = ErrorSink {
            errors: Arc::clone(&errors),
        };
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
        drop(stop_tx);
        super::super::run(Arc::new(cfg), stop_rx, sink).await;
        assert!(!errors.lock().unwrap().is_empty());
    }
}
