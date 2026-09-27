//! 应用自动更新模块。
//!
//! 负责检查应用新版本、下载更新及安装引导。
//! 支持后台静默模式与手动模式，处理 10 秒超时与不同打包类型的分级支持。

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use crate::pipeline_controller::{PipelineController, PipelineStatus};
use serde::{Deserialize, Serialize};

/// 检查模式：静默模式（启动时触发）或手动模式（用户主动触发）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckMode {
    Silent,
    Manual,
}

/// 更新支持级别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateSupportTier {
    /// 就地更新：Windows 与 Linux AppImage
    InPlace,
    /// 外部引导：Linux deb/rpm/AUR
    External,
}

/// 检查更新返回的结果。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheckResponse {
    pub has_update: bool,
    pub current_version: String,
    pub latest_version: String,
    pub body: Option<String>,
    pub date: Option<String>,
    pub support_tier: UpdateSupportTier,
}

/// 错误类别。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateErrorKind {
    Timeout,
    Network,
    Signature,
    RateLimited,
    Unknown,
}

/// 检查更新失败的详细错误。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateErrorResponse {
    pub kind: UpdateErrorKind,
    pub message: String,
}

/// 解析当前运行环境的更新支持级别。
pub fn detect_support_tier() -> UpdateSupportTier {
    #[cfg(windows)]
    {
        UpdateSupportTier::InPlace
    }
    #[cfg(target_os = "linux")]
    {
        if std::env::var_os("APPIMAGE").is_some() {
            UpdateSupportTier::InPlace
        } else {
            UpdateSupportTier::External
        }
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        UpdateSupportTier::External
    }
}

/// 原始更新信息。
#[derive(Debug, Clone)]
pub struct UpdateInfoRaw {
    pub version: String,
    pub current_version: String,
    pub body: Option<String>,
    pub date: Option<String>,
}

/// 更新提供者 trait（抽象 seam，用于测试和生产）。
pub trait UpdateProvider: Send + Sync {
    fn check_update_raw<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<UpdateInfoRaw>, String>> + Send + 'a>>;

    fn download_and_install_raw<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>>;
}

/// 核心编排函数：在指定超时时间内执行更新检查，并映射为结构化响应。
pub async fn check_update_core<P: UpdateProvider + ?Sized>(
    provider: &P,
    _mode: CheckMode,
    timeout_duration: Duration,
    support_tier: UpdateSupportTier,
) -> Result<UpdateCheckResponse, UpdateErrorResponse> {
    let check_future = provider.check_update_raw();
    let result = match tokio::time::timeout(timeout_duration, check_future).await {
        Ok(res) => res,
        Err(_) => {
            return Err(UpdateErrorResponse {
                kind: UpdateErrorKind::Timeout,
                message: "检查更新超时（10秒），请检查网络连接后重试".to_string(),
            });
        }
    };

    match result {
        Ok(Some(info)) => Ok(UpdateCheckResponse {
            has_update: true,
            current_version: info.current_version,
            latest_version: info.version,
            body: info.body,
            date: info.date,
            support_tier,
        }),
        Ok(None) => Ok(UpdateCheckResponse {
            has_update: false,
            current_version: env!("CARGO_PKG_VERSION").to_string(),
            latest_version: env!("CARGO_PKG_VERSION").to_string(),
            body: None,
            date: None,
            support_tier,
        }),
        Err(err_msg) => {
            let lower = err_msg.to_lowercase();
            let kind = if lower.contains("timeout") || lower.contains("timed out") {
                UpdateErrorKind::Timeout
            } else if lower.contains("signature")
                || lower.contains("verification failed")
                || lower.contains("minisign")
            {
                UpdateErrorKind::Signature
            } else if lower.contains("429") || lower.contains("rate limit") {
                UpdateErrorKind::RateLimited
            } else if lower.contains("connect")
                || lower.contains("dns")
                || lower.contains("network")
                || lower.contains("http")
                || lower.contains("reqwest")
                || lower.contains("could not fetch")
                || lower.contains("failed to fetch")
                || lower.contains("release json")
            {
                UpdateErrorKind::Network
            } else {
                UpdateErrorKind::Unknown
            };

            let user_msg = match kind {
                UpdateErrorKind::Timeout => "检查更新超时，请检查网络连接后重试".to_string(),
                UpdateErrorKind::Signature => {
                    format!("更新包签名验证失败，防止安全篡改：{err_msg}")
                }
                UpdateErrorKind::RateLimited => "更新接口请求过于频繁，请稍后再试".to_string(),
                UpdateErrorKind::Network => format!("无法连接到更新服务器：{err_msg}"),
                UpdateErrorKind::Unknown => format!("检查更新失败：{err_msg}"),
            };

            Err(UpdateErrorResponse {
                kind,
                message: user_msg,
            })
        }
    }
}

/// 生产环境更新提供者，直接调用 `tauri_plugin_updater`。
pub struct TauriUpdateProvider {
    pub app: tauri::AppHandle,
}

impl UpdateProvider for TauriUpdateProvider {
    fn check_update_raw<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<UpdateInfoRaw>, String>> + Send + 'a>> {
        let app = self.app.clone();
        Box::pin(async move {
            use tauri_plugin_updater::UpdaterExt;
            let updater = app.updater().map_err(|e| e.to_string())?;
            let update = updater.check().await.map_err(|e| e.to_string())?;
            Ok(update.map(|u| UpdateInfoRaw {
                version: u.version,
                current_version: u.current_version,
                body: u.body,
                date: u.date.map(|d| d.to_string()),
            }))
        })
    }

    fn download_and_install_raw<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>> {
        let app = self.app.clone();
        Box::pin(async move {
            use tauri_plugin_updater::UpdaterExt;
            let updater = app.updater().map_err(|e| e.to_string())?;
            if let Some(update) = updater.check().await.map_err(|e| e.to_string())? {
                let mut downloaded = 0;
                update
                    .download_and_install(
                        |chunk_length, content_length| {
                            downloaded += chunk_length;
                            tracing::debug!(downloaded, content_length, "downloading update");
                        },
                        || {
                            tracing::info!("download finished");
                        },
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                app.restart();
            } else {
                return Err("没有检测到可用更新".to_string());
            }
            #[allow(unreachable_code)]
            Ok(())
        })
    }
}

/// 核心编排函数：在检查流水线状态后执行更新安装。
pub async fn install_update_core<P: UpdateProvider + ?Sized>(
    provider: &P,
    controller: &PipelineController,
) -> Result<(), String> {
    let status = *controller.status_arc().read().unwrap();
    match status {
        PipelineStatus::Recording | PipelineStatus::Processing => {
            return Err("正在录音或转写中，请稍候再执行更新".to_string());
        }
        PipelineStatus::Idle | PipelineStatus::Done | PipelineStatus::Stopped => {}
    }

    provider.download_and_install_raw().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    struct MockUpdateProvider {
        result: Result<Option<UpdateInfoRaw>, String>,
        delay: Option<Duration>,
        install_called: AtomicBool,
        install_result: Result<(), String>,
    }

    impl MockUpdateProvider {
        fn new(result: Result<Option<UpdateInfoRaw>, String>) -> Self {
            Self {
                result,
                delay: None,
                install_called: AtomicBool::new(false),
                install_result: Ok(()),
            }
        }

        fn with_delay(mut self, delay: Duration) -> Self {
            self.delay = Some(delay);
            self
        }
    }

    impl UpdateProvider for MockUpdateProvider {
        fn check_update_raw<'a>(
            &'a self,
        ) -> Pin<Box<dyn Future<Output = Result<Option<UpdateInfoRaw>, String>> + Send + 'a>>
        {
            let res = self.result.clone();
            let delay = self.delay;
            Box::pin(async move {
                if let Some(d) = delay {
                    tokio::time::sleep(d).await;
                }
                res
            })
        }

        fn download_and_install_raw<'a>(
            &'a self,
        ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>> {
            self.install_called.store(true, Ordering::SeqCst);
            let res = self.install_result.clone();
            Box::pin(async move { res })
        }
    }

    #[tokio::test]
    async fn test_check_update_has_new_version() {
        let provider = MockUpdateProvider::new(Ok(Some(UpdateInfoRaw {
            version: "2.7.0".to_string(),
            current_version: "2.6.3".to_string(),
            body: Some("修复了一些 bug".to_string()),
            date: Some("2025-02-24".to_string()),
        })));

        let res = check_update_core(
            &provider,
            CheckMode::Manual,
            Duration::from_secs(10),
            UpdateSupportTier::InPlace,
        )
        .await
        .unwrap();

        assert!(res.has_update);
        assert_eq!(res.latest_version, "2.7.0");
        assert_eq!(res.current_version, "2.6.3");
        assert_eq!(res.support_tier, UpdateSupportTier::InPlace);
        assert_eq!(res.body.as_deref(), Some("修复了一些 bug"));
    }

    #[tokio::test]
    async fn test_check_update_already_latest() {
        let provider = MockUpdateProvider::new(Ok(None));

        let res = check_update_core(
            &provider,
            CheckMode::Manual,
            Duration::from_secs(10),
            UpdateSupportTier::External,
        )
        .await
        .unwrap();

        assert!(!res.has_update);
        assert_eq!(res.latest_version, env!("CARGO_PKG_VERSION"));
        assert_eq!(res.support_tier, UpdateSupportTier::External);
    }

    #[tokio::test]
    async fn test_check_update_timeout_error() {
        let provider = MockUpdateProvider::new(Ok(None)).with_delay(Duration::from_millis(50));

        let err = check_update_core(
            &provider,
            CheckMode::Manual,
            Duration::from_millis(10),
            UpdateSupportTier::InPlace,
        )
        .await
        .unwrap_err();

        assert_eq!(err.kind, UpdateErrorKind::Timeout);
        assert!(err.message.contains("超时"));
    }

    #[tokio::test]
    async fn test_check_update_network_error_mapping() {
        let provider = MockUpdateProvider::new(Err(
            "failed to connect to github: network is unreachable".to_string(),
        ));

        let err = check_update_core(
            &provider,
            CheckMode::Manual,
            Duration::from_secs(10),
            UpdateSupportTier::InPlace,
        )
        .await
        .unwrap_err();

        assert_eq!(err.kind, UpdateErrorKind::Network);
        assert!(err.message.contains("无法连接到更新服务器"));

        // 测试 Could not fetch a valid release JSON 的映射
        let provider2 = MockUpdateProvider::new(Err(
            "Could not fetch a valid release JSON from the remote".to_string(),
        ));
        let err2 = check_update_core(
            &provider2,
            CheckMode::Manual,
            Duration::from_secs(10),
            UpdateSupportTier::InPlace,
        )
        .await
        .unwrap_err();
        assert_eq!(err2.kind, UpdateErrorKind::Network);
        assert!(err2.message.contains("无法连接到更新服务器"));
    }

    #[tokio::test]
    async fn test_check_update_rate_limit_error_mapping() {
        let provider = MockUpdateProvider::new(Err(
            "HTTP error 429 Too Many Requests: rate limit exceeded".to_string(),
        ));

        let err = check_update_core(
            &provider,
            CheckMode::Manual,
            Duration::from_secs(10),
            UpdateSupportTier::InPlace,
        )
        .await
        .unwrap_err();

        assert_eq!(err.kind, UpdateErrorKind::RateLimited);
        assert!(err.message.contains("请求过于频繁"));
    }

    #[tokio::test]
    async fn test_install_update_rejected_when_pipeline_recording() {
        let provider = MockUpdateProvider::new(Ok(None));
        let controller = PipelineController::new();
        *controller.status_arc().write().unwrap() = PipelineStatus::Recording;

        let err = install_update_core(&provider, &controller)
            .await
            .unwrap_err();
        assert!(err.contains("正在录音或转写中"));
        assert!(!provider.install_called.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_install_update_succeeds_when_pipeline_idle() {
        let provider = MockUpdateProvider::new(Ok(None));
        let controller = PipelineController::new();
        *controller.status_arc().write().unwrap() = PipelineStatus::Idle;

        let res = install_update_core(&provider, &controller).await;
        assert!(res.is_ok());
        assert!(provider.install_called.load(Ordering::SeqCst));
    }
}
