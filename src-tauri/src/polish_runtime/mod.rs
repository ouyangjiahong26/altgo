//! 本地润色运行时。
//!
//! 职责拆成三块：`manifest` 记录固定构建号与文件校验和，`install` 负责取回并解包，
//! `process` 负责把可执行文件拉起来并确认它能服务。`LocalRuntime` 把后两块串成
//! 惰性启动与空闲回收的生命周期，供本地润色引擎使用。

pub mod install;
pub mod manifest;
pub mod process;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "windows")]
mod windows;

use crate::error::PolishRuntimeError;
use process::{IdleClock, RuntimeChild};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

pub use manifest::{current_spec, runtime_dir, RuntimeSpec, RUNTIME_BUILD};
pub use process::{LaunchConfig, LogTail, RuntimeEndpoint, RuntimeProcess};

/// 设置页用的运行时状态快照。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PolishRuntimeStatus {
    /// 当前平台是否有清单条目，没有就不支持本地润色。
    pub supported: bool,
    /// 运行时文件是否就位。
    pub installed: bool,
    /// 运行时进程是否在跑。
    pub running: bool,
    /// 固定的构建号。
    pub build: String,
    /// 压缩包字节数，用于进度条总量。
    pub archive_size: u64,
}

/// 进程内共享的运行时句柄。
///
/// 流水线与设置页命令必须是同一个实例：否则两边各自拉起一个进程，既翻倍内存，
/// 又让界面上的“正在运行”与“下载运行时”判断失真。
pub fn active() -> Arc<LocalRuntime> {
    static ACTIVE: LazyLock<Mutex<Option<Arc<LocalRuntime>>>> = LazyLock::new(|| Mutex::new(None));
    let mut slot = ACTIVE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    match slot.as_ref() {
        Some(runtime) => Arc::clone(runtime),
        None => {
            let runtime = Arc::new(LocalRuntime::new(LaunchConfig::default()));
            *slot = Some(Arc::clone(&runtime));
            runtime
        }
    }
}

/// 本地润色模型在运行时里的别名，润色请求的 `model` 字段用它。
pub const MODEL_ALIAS: &str = "altgo-local";

/// 覆盖运行时下载基址的环境变量，便于网络不畅时指向镜像。
pub const ENV_RUNTIME_BASE_URL: &str = "ALTGO_RUNTIME_BASE_URL";

/// llama.cpp 官方发布产物的下载基址，完整地址还要接上构建号与文件名。
const OFFICIAL_BASE_URL: &str = "https://github.com/ggml-org/llama.cpp/releases/download";

#[cfg(target_os = "windows")]
pub type PlatformRuntimeProcess = windows::WindowsProcess;
#[cfg(target_os = "linux")]
pub type PlatformRuntimeProcess = linux::LinuxProcess;

/// 物理核数，推理线程数的依据（实测逻辑线程反而更慢）。
pub fn physical_core_count() -> usize {
    PlatformRuntimeProcess::new().physical_core_count()
}

/// 运行时下载基址：环境变量优先，否则用官方发布地址。
pub fn download_bases() -> Vec<String> {
    if let Ok(base) = std::env::var(ENV_RUNTIME_BASE_URL) {
        let trimmed = base.trim();
        if !trimmed.is_empty() {
            return vec![trimmed.trim_end_matches('/').to_string()];
        }
    }
    vec![format!("{}/{}", OFFICIAL_BASE_URL, RUNTIME_BUILD)]
}

struct Running {
    child: Box<dyn RuntimeChild>,
    endpoint: RuntimeEndpoint,
    clock: IdleClock,
}

/// 本地润色运行时的生命周期。
///
/// 惰性启动：只有第一次真正要用润色时才拉进程。空闲超过 `LaunchConfig::idle_timeout`
/// 回收，被外部杀掉后下次使用时重新拉起。
pub struct LocalRuntime {
    dir: PathBuf,
    spec: Option<&'static RuntimeSpec>,
    cfg: LaunchConfig,
    process: PlatformRuntimeProcess,
    state: Mutex<Option<Running>>,
}

impl LocalRuntime {
    /// 用清单里当前平台的条目构造。
    pub fn new(cfg: LaunchConfig) -> Self {
        Self::with_dir(runtime_dir(), cfg, PlatformRuntimeProcess::new())
    }

    /// 指定目录与平台实现构造，供测试与手动放置场景使用。
    pub fn with_dir(dir: PathBuf, cfg: LaunchConfig, process: PlatformRuntimeProcess) -> Self {
        Self {
            dir,
            spec: current_spec(),
            cfg,
            process,
            state: Mutex::new(None),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// 运行时文件是否已就位，手动放置的文件同样算数。
    pub fn is_installed(&self) -> bool {
        self.spec
            .is_some_and(|spec| install::is_installed(spec, &self.dir))
    }

    /// 运行时进程是否在跑。
    pub fn is_running(&self) -> bool {
        self.state.lock().is_ok_and(|state| state.is_some())
    }

    /// 设置页用的状态快照。
    pub fn status(&self) -> PolishRuntimeStatus {
        PolishRuntimeStatus {
            supported: self.spec.is_some(),
            installed: self.is_installed(),
            running: self.is_running(),
            build: RUNTIME_BUILD.to_string(),
            archive_size: self.spec.map_or(0, |spec| spec.archive_size),
        }
    }

    /// 下载并解包运行时，进度回调语义与模型下载一致（累计字节对总量）。
    pub async fn install<F>(&self, on_progress: F) -> Result<(), PolishRuntimeError>
    where
        F: FnMut(u64, u64),
    {
        let spec = self.require_spec()?;
        install::install(spec, &download_bases(), &self.dir, on_progress).await
    }

    /// 删除已下载的运行时文件，供“重新下载运行时”使用。
    pub fn remove(&self) -> Result<(), PolishRuntimeError> {
        install::remove(&self.dir)
    }

    /// 确保运行时已启动并返回端点，已启动时只刷新使用时刻。
    pub async fn endpoint(&self, model: &Path) -> Result<RuntimeEndpoint, PolishRuntimeError> {
        if let Some(endpoint) = self.live_endpoint()? {
            return Ok(endpoint);
        }

        let spec = self.require_spec()?;
        if !install::is_installed(spec, &self.dir) {
            return Err(PolishRuntimeError::Incomplete(format!(
                "润色运行时文件不完整：{}",
                self.dir.display()
            )));
        }

        let exe = self.dir.join(spec.executable);
        self.process.ensure_executable(&exe)?;
        let port = process::choose_port(self.cfg.preferred_port)?;
        let api_key = process::generate_api_key();
        let args = process::build_args(model, &self.cfg, port, &api_key);
        let log_tail = LogTail::default();
        tracing::info!(
            port,
            threads = self.cfg.threads,
            context = self.cfg.context_size,
            "starting polish runtime"
        );

        let mut child = self.process.spawn(&exe, &args, log_tail.clone())?;
        if let Err(error) =
            process::wait_ready(port, self.cfg.startup_timeout, child.as_mut(), &log_tail).await
        {
            let _ = child.kill().await;
            return Err(error);
        }

        let endpoint = RuntimeEndpoint {
            base_url: format!("http://127.0.0.1:{}/v1", port),
            api_key,
        };

        // 并发调用可能各自拉起了一个进程，保留先到的，回收后到的。这里是唯一读状态的地方，
        // 因此不会把守卫带过 await 点。
        let raced = {
            let state = self.lock_state()?;
            state.as_ref().map(|running| running.endpoint.clone())
        };
        if let Some(existing) = raced {
            let _ = child.kill().await;
            return Ok(existing);
        }

        // 若两次加锁之间又有调用写入实例，覆盖它即可：被顶掉的子进程句柄释放时会被回收。
        let mut state = self.lock_state()?;
        *state = Some(Running {
            child,
            endpoint: endpoint.clone(),
            clock: IdleClock::default(),
        });
        Ok(endpoint)
    }

    /// 空闲超过阈值则回收进程，返回本次是否回收了。
    pub async fn release_if_idle(&self) -> Result<bool, PolishRuntimeError> {
        let idle_running = {
            let mut state = self.lock_state()?;
            let idle = state
                .as_ref()
                .is_some_and(|running| running.clock.is_idle(self.cfg.idle_timeout));
            if idle {
                state.take()
            } else {
                None
            }
        };
        match idle_running {
            Some(mut running) => {
                running.child.kill().await?;
                tracing::info!("polish runtime stopped after idle");
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// 主动结束运行时：应用退出或切回云端档时调用。
    pub async fn shutdown(&self) -> Result<(), PolishRuntimeError> {
        let running = self.lock_state()?.take();
        if let Some(mut running) = running {
            running.child.kill().await?;
        }
        Ok(())
    }

    /// 返回仍在运行的端点并刷新空闲时钟。进程已被外部结束（崩溃、被杀、手动关掉）时
    /// 清掉状态并返回 `None`，让调用方重新拉起，而不是把失效端点给出去。
    fn live_endpoint(&self) -> Result<Option<RuntimeEndpoint>, PolishRuntimeError> {
        let mut state = self.lock_state()?;
        if state.is_none() {
            return Ok(None);
        }
        let alive = match state.as_mut() {
            Some(running) => running.child.try_wait()?.is_none(),
            None => false,
        };
        if !alive {
            state.take();
            tracing::warn!("polish runtime exited unexpectedly, will restart on next use");
            return Ok(None);
        }

        let mut endpoint = None;
        if let Some(running) = state.as_mut() {
            running.clock.touch();
            endpoint = Some(running.endpoint.clone());
        }
        Ok(endpoint)
    }

    fn lock_state(&self) -> Result<std::sync::MutexGuard<'_, Option<Running>>, PolishRuntimeError> {
        self.state
            .lock()
            .map_err(|_| PolishRuntimeError::ProcessFailed("润色运行时状态锁已损坏".to_string()))
    }

    fn require_spec(&self) -> Result<&'static RuntimeSpec, PolishRuntimeError> {
        self.spec.ok_or_else(|| {
            PolishRuntimeError::UnsupportedPlatform(format!(
                "{}-{}",
                std::env::consts::OS,
                std::env::consts::ARCH
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runtime_in(dir: PathBuf) -> LocalRuntime {
        LocalRuntime::with_dir(dir, LaunchConfig::default(), PlatformRuntimeProcess::new())
    }

    /// 测试用：塞入运行中状态，用来验证存活检查与回收，不必真起进程。
    impl LocalRuntime {
        fn seed_running(&self, child: Box<dyn RuntimeChild>, endpoint: RuntimeEndpoint) {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            *state = Some(Running {
                child,
                endpoint,
                clock: IdleClock::default(),
            });
        }
    }

    /// 存活与否由标志位控制的假进程。
    struct FakeChild {
        exited: bool,
    }

    impl RuntimeChild for FakeChild {
        fn id(&self) -> u32 {
            42
        }

        fn try_wait(&mut self) -> Result<Option<i32>, PolishRuntimeError> {
            Ok(if self.exited { Some(0) } else { None })
        }

        fn kill<'a>(
            &'a mut self,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<(), PolishRuntimeError>> + Send + 'a>,
        > {
            self.exited = true;
            Box::pin(async { Ok(()) })
        }
    }

    fn cached_endpoint() -> RuntimeEndpoint {
        RuntimeEndpoint {
            base_url: "http://127.0.0.1:1/v1".to_string(),
            api_key: "cached".to_string(),
        }
    }

    /// 进程还在时应直接复用缓存端点：运行时目录里没有文件，走启动路径会报不完整。
    #[tokio::test]
    async fn test_endpoint_reuses_live_process_without_spawning() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = runtime_in(dir.path().to_path_buf());
        let endpoint = cached_endpoint();
        runtime.seed_running(Box::new(FakeChild { exited: false }), endpoint.clone());

        let model = dir.path().join("model.gguf");
        assert_eq!(runtime.endpoint(&model).await.unwrap(), endpoint);
        assert!(runtime.is_running());
    }

    /// 进程被外部结束后，状态要清掉并走重新启动的路径，不能把失效端点给出去。
    #[tokio::test]
    async fn test_endpoint_restarts_after_process_exit() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = runtime_in(dir.path().to_path_buf());
        runtime.seed_running(Box::new(FakeChild { exited: true }), cached_endpoint());

        let model = dir.path().join("model.gguf");
        let error = runtime.endpoint(&model).await.unwrap_err();
        assert!(
            matches!(error, PolishRuntimeError::Incomplete(_)),
            "应当走重新启动的路径，实际：{error:?}"
        );
        assert!(!runtime.is_running(), "失效状态应被清掉");
    }

    #[test]
    fn test_not_installed_on_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = runtime_in(dir.path().to_path_buf());
        assert!(!runtime.is_installed());
        assert!(!runtime.is_running());
    }

    #[test]
    fn test_status_reflects_missing_runtime_files() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = runtime_in(dir.path().to_path_buf());
        let status = runtime.status();
        assert_eq!(status.supported, current_spec().is_some());
        assert!(!status.installed);
        assert!(!status.running);
        assert_eq!(status.build, RUNTIME_BUILD);
        assert_eq!(
            status.archive_size,
            current_spec().map_or(0, |spec| spec.archive_size)
        );
    }

    #[test]
    fn test_active_returns_the_same_instance() {
        assert!(Arc::ptr_eq(&active(), &active()));
    }

    #[tokio::test]
    async fn test_release_and_shutdown_are_noops_without_process() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = runtime_in(dir.path().to_path_buf());
        assert!(!runtime.release_if_idle().await.unwrap());
        assert!(runtime.shutdown().await.is_ok());
    }

    #[tokio::test]
    async fn test_endpoint_reports_incomplete_runtime_instead_of_spawning() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = runtime_in(dir.path().to_path_buf());
        let model = dir.path().join("model.gguf");
        match runtime.endpoint(&model).await {
            Err(PolishRuntimeError::Incomplete(message)) => {
                assert!(message.contains("不完整"), "{message}");
            }
            other => panic!("应当报文件不完整，实际：{other:?}"),
        }
        assert!(!runtime.is_running());
    }

    /// 文件就位但可执行文件不可用时，要报启动失败而不是挂起。
    #[tokio::test]
    async fn test_endpoint_reports_spawn_failure_for_bogus_executable() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = runtime_in(dir.path().to_path_buf());
        let spec = match current_spec() {
            Some(spec) => spec,
            None => return,
        };
        let cpu_library = format!("{}-baseline{}", spec.cpu_lib_prefix, spec.shared_lib_suffix);
        for name in [spec.executable, cpu_library.as_str()]
            .into_iter()
            .chain(spec.required_libraries.iter().copied())
        {
            std::fs::write(dir.path().join(name), b"not really a binary").unwrap();
        }
        assert!(runtime.is_installed(), "文件齐备应通过结构检查");
        let model = dir.path().join("model.gguf");
        std::fs::write(&model, b"not really a model").unwrap();

        let result = runtime.endpoint(&model).await;
        assert!(result.is_err(), "伪造的运行时不应当启动成功");
        assert!(!runtime.is_running());
    }
}
