//! 润色运行时的进程管理：端口选择、启动、就绪探测、日志留存、空闲回收。
//!
//! 这里不关心运行时文件从哪来（见 `super::install`），只关心一件事：把一个可执行文件
//! 拉起来，确认它真的能服务，然后在不需要时干净地关掉。平台差异（创建参数、进程树
//! 回收、物理核数）收在 `super::windows` 与 `super::linux` 里。

use crate::error::PolishRuntimeError;
use std::collections::VecDeque;
use std::future::Future;
use std::net::TcpListener;
use std::path::Path;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// 就绪探测的间隔。
const HEALTH_POLL_INTERVAL: Duration = Duration::from_millis(200);

/// 保留的运行时日志行数，用于失败时给出可诊断的尾部输出。
const LOG_TAIL_LINES: usize = 40;

/// 结束子进程后等待回收的上限与轮询间隔。
const REAP_TIMEOUT: Duration = Duration::from_secs(5);
const REAP_POLL_INTERVAL: Duration = Duration::from_millis(20);

/// 本地润色的固定启动参数。
///
/// 取值来自离线润色的实测：线程数取物理核数（同一台机器 16 物理核生成 100.8 tok/s，
/// 22 逻辑线程掉到 60.5 tok/s），上下文 4096 足够容纳短提示词加几十字的输入输出。
#[derive(Debug, Clone)]
pub struct LaunchConfig {
    /// 推理线程数。
    pub threads: usize,
    /// 上下文窗口。
    pub context_size: u32,
    /// 空闲多久后回收进程。
    pub idle_timeout: Duration,
    /// 启动到就绪的最长等待。
    pub startup_timeout: Duration,
    /// 优先使用的端口，被占用时改用系统分配的空闲端口。
    pub preferred_port: u16,
}

impl Default for LaunchConfig {
    fn default() -> Self {
        Self {
            threads: crate::polish_runtime::physical_core_count().min(16),
            context_size: 4096,
            idle_timeout: Duration::from_secs(600),
            startup_timeout: Duration::from_secs(60),
            preferred_port: 11711,
        }
    }
}

/// 本地润色引擎要用的端点。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeEndpoint {
    /// 形如 `http://127.0.0.1:11711/v1`。
    pub base_url: String,
    /// 每次拉起运行时新生成的随机密钥，只在本进程与运行时之间使用。
    pub api_key: String,
}

/// 运行时的日志尾部，环形保留最近若干行。
#[derive(Clone, Default)]
pub struct LogTail(Arc<Mutex<VecDeque<String>>>);

impl LogTail {
    pub fn push(&self, line: String) {
        let Ok(mut lines) = self.0.lock() else {
            return;
        };
        if lines.len() == LOG_TAIL_LINES {
            lines.pop_front();
        }
        lines.push_back(line);
    }

    /// 最近日志的合并文本，供错误消息使用。
    pub fn tail(&self) -> String {
        self.0
            .lock()
            .map(|lines| lines.iter().cloned().collect::<Vec<_>>().join("\n"))
            .unwrap_or_default()
    }
}

/// 拉起运行时的平台接缝。
pub trait RuntimeProcess: Send + Sync {
    /// 启动子进程，标准错误由实现负责交给 `log_tail`。
    fn spawn(
        &self,
        exe: &Path,
        args: &[String],
        log_tail: LogTail,
    ) -> Result<Box<dyn RuntimeChild>, PolishRuntimeError>;

    /// 给可执行文件补上执行位（Windows 为空实现）。
    fn ensure_executable(&self, path: &Path) -> Result<(), PolishRuntimeError>;

    /// 逻辑核数之外的物理核数，用于决定推理线程数。
    fn physical_core_count(&self) -> usize;
}

/// 已启动的运行时进程。
pub trait RuntimeChild: Send {
    fn id(&self) -> u32;
    /// 已退出则返回退出码。
    fn try_wait(&mut self) -> Result<Option<i32>, PolishRuntimeError>;
    /// 结束进程并回收。
    fn kill<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), PolishRuntimeError>> + Send + 'a>>;
}

/// 通用的 tokio 子进程包装：创建参数与额外清理由平台模块决定。
pub struct TokioChild {
    child: tokio::process::Child,
}

impl TokioChild {
    pub fn new(child: tokio::process::Child) -> Self {
        Self { child }
    }

    /// 取出标准错误，交给日志读取任务。
    pub fn take_stderr(&mut self) -> Option<tokio::process::ChildStderr> {
        self.child.stderr.take()
    }
}

impl RuntimeChild for TokioChild {
    fn id(&self) -> u32 {
        self.child.id().unwrap_or(0)
    }

    fn try_wait(&mut self) -> Result<Option<i32>, PolishRuntimeError> {
        match self.child.try_wait() {
            Ok(Some(status)) => Ok(status.code()),
            Ok(None) => Ok(None),
            Err(e) => Err(PolishRuntimeError::ProcessFailed(e.to_string())),
        }
    }

    fn kill<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), PolishRuntimeError>> + Send + 'a>> {
        Box::pin(async move {
            // 不走 tokio 的 `kill()`：它内部会把退出状态 wait 掉，之后 `try_wait` 在 Linux 上
            // 一直返回 `None`，调用方就再也确认不了进程是否真的没了。这里自己发信号再回收。
            if let Err(error) = self.child.start_kill() {
                // 已经退出过的子进程会在这里报错，按已结束处理。
                tracing::debug!(error = %error, "child exited before kill");
            }
            let deadline = Instant::now() + REAP_TIMEOUT;
            loop {
                match self.child.try_wait() {
                    Ok(Some(_)) => return Ok(()),
                    Ok(None) => {
                        if Instant::now() >= deadline {
                            return Err(PolishRuntimeError::ProcessFailed(
                                "结束润色运行时超时".to_string(),
                            ));
                        }
                        tokio::time::sleep(REAP_POLL_INTERVAL).await;
                    }
                    Err(error) => return Err(PolishRuntimeError::ProcessFailed(error.to_string())),
                }
            }
        })
    }
}

/// 读取子进程标准错误并环形保留，失败时用于给出可诊断的尾部输出。
pub fn drain_stderr(stderr: tokio::process::ChildStderr, log_tail: LogTail) {
    tokio::spawn(async move {
        use tokio::io::AsyncBufReadExt;
        let mut lines = tokio::io::BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            log_tail.push(line);
        }
    });
}

/// 选一个可用的回环端口：优先 `preferred`，被占用则交给系统分配。
pub fn choose_port(preferred: u16) -> Result<u16, PolishRuntimeError> {
    if let Ok(listener) = TcpListener::bind(("127.0.0.1", preferred)) {
        drop(listener);
        return Ok(preferred);
    }
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

/// 组装启动参数。模型路径由调用方决定（本地润色模型的 GGUF 文件）。
pub fn build_args(model: &Path, cfg: &LaunchConfig, port: u16, api_key: &str) -> Vec<String> {
    vec![
        "--host".to_string(),
        "127.0.0.1".to_string(),
        "--port".to_string(),
        port.to_string(),
        "--model".to_string(),
        model.display().to_string(),
        "--alias".to_string(),
        crate::polish_runtime::MODEL_ALIAS.to_string(),
        "--ctx-size".to_string(),
        cfg.context_size.to_string(),
        "--threads".to_string(),
        cfg.threads.to_string(),
        "--parallel".to_string(),
        "1".to_string(),
        "--api-key".to_string(),
        api_key.to_string(),
        // 独立于客户端关闭思考：本地档的提示词不走各家服务商的方言参数，
        // 关掉思考既省时间也避免思维链残渣进入结果。
        "--chat-template-kwargs".to_string(),
        r#"{"enable_thinking":false}"#.to_string(),
        "--no-webui".to_string(),
    ]
}

/// 等待运行时就绪。
///
/// 子进程在就绪前退出会立刻返回错误并带上日志尾部，避免白等到超时。
pub async fn wait_ready(
    port: u16,
    timeout: Duration,
    child: &mut dyn RuntimeChild,
    log_tail: &LogTail,
) -> Result<(), PolishRuntimeError> {
    let url = format!("http://127.0.0.1:{}/health", port);
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(code) = child.try_wait()? {
            return Err(PolishRuntimeError::StartupFailed(format!(
                "润色运行时在就绪前退出（退出码 {}）。\n{}",
                code,
                log_tail.tail()
            )));
        }
        if health_ok(&url).await {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(PolishRuntimeError::StartupFailed(format!(
                "润色运行时未在 {} 秒内就绪。\n{}",
                timeout.as_secs(),
                log_tail.tail()
            )));
        }
        tokio::time::sleep(HEALTH_POLL_INTERVAL).await;
    }
}

/// 就绪探测：只有返回 200 且状态为 ok 才算就绪，加载中的 503 继续等。
async fn health_ok(url: &str) -> bool {
    let Ok(response) = crate::download::client().get(url).send().await else {
        return false;
    };
    if !response.status().is_success() {
        return false;
    }
    response
        .json::<serde_json::Value>()
        .await
        .is_ok_and(|body| body.get("status").and_then(|s| s.as_str()) == Some("ok"))
}

/// 生成每次启动用的随机密钥。
///
/// 端点只在回环地址上监听，密钥用于挡住同机其他进程的误用，不需要密码学强度之外的
/// 额外来源，因此用随机 UUID 加进程号拼成。
pub fn generate_api_key() -> String {
    format!("{}{}", uuid::Uuid::new_v4().simple(), std::process::id())
}

/// 记录最近使用时刻，用于空闲回收判断。
pub struct IdleClock {
    last_used: Instant,
}

impl Default for IdleClock {
    fn default() -> Self {
        Self {
            last_used: Instant::now(),
        }
    }
}

impl IdleClock {
    /// 每次真正使用运行时（发过请求）时调用。
    pub fn touch(&mut self) {
        self.last_used = Instant::now();
    }

    /// 空闲是否已超过阈值。
    pub fn is_idle(&self, timeout: Duration) -> bool {
        self.last_used.elapsed() >= timeout
    }
}

/// 把参数渲染成日志用的一行文本，供测试断言启动参数。
#[cfg(test)]
pub fn describe_args(args: &[String]) -> String {
    args.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_choose_port_prefers_free_preferred_port() {
        let free = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = free.local_addr().unwrap().port();
        drop(free);
        assert_eq!(choose_port(port).unwrap(), port);
    }

    #[test]
    fn test_choose_port_falls_back_when_preferred_is_taken() {
        let held = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = held.local_addr().unwrap().port();
        let chosen = choose_port(port).unwrap();
        assert_ne!(chosen, port);
        assert_ne!(chosen, 0);
    }

    #[test]
    fn test_build_args_pins_loopback_and_alias() {
        let cfg = LaunchConfig {
            threads: 8,
            ..LaunchConfig::default()
        };
        let args = build_args(
            Path::new("C:/models/qwen3-0.6b.gguf"),
            &cfg,
            11711,
            "secret",
        );
        let joined = describe_args(&args);
        assert!(joined.contains("--host 127.0.0.1"));
        assert!(joined.contains("--port 11711"));
        assert!(joined.contains("--threads 8"));
        assert!(joined.contains("--ctx-size 4096"));
        assert!(joined.contains("--api-key secret"));
        assert!(joined.contains("--chat-template-kwargs"));
        assert!(joined.contains(r#"{"enable_thinking":false}"#));
        assert!(joined.contains("--no-webui"));
        assert!(joined.contains(&format!("--alias {}", crate::polish_runtime::MODEL_ALIAS)));
    }

    #[test]
    fn test_default_threads_never_exceed_sixteen_and_are_at_least_one() {
        let threads = LaunchConfig::default().threads;
        assert!(threads >= 1);
        assert!(threads <= 16);
    }

    #[test]
    fn test_log_tail_keeps_only_recent_lines() {
        let tail = LogTail::default();
        for i in 0..(LOG_TAIL_LINES + 10) {
            tail.push(format!("line {i}"));
        }
        let text = tail.tail();
        assert!(!text.contains("line 0"));
        assert!(text.contains(&format!("line {}", LOG_TAIL_LINES + 9)));
        assert_eq!(text.lines().count(), LOG_TAIL_LINES);
    }

    #[test]
    fn test_generate_api_key_is_unique_and_nontrivial() {
        let a = generate_api_key();
        let b = generate_api_key();
        assert_ne!(a, b);
        assert!(a.len() >= 32);
    }

    /// 就绪探测要区分“加载中的 503”与“就绪的 200”。
    #[tokio::test]
    async fn test_health_ok_requires_success_status_and_ok_body() {
        let mut server = mockito::Server::new_async().await;
        let loading = server
            .mock("GET", "/loading/health")
            .with_status(503)
            .with_body(r#"{"error":"loading"}"#)
            .create_async()
            .await;
        let ready = server
            .mock("GET", "/ready/health")
            .with_status(200)
            .with_body(r#"{"status":"ok"}"#)
            .create_async()
            .await;

        assert!(!health_ok(&format!("{}/loading/health", server.url())).await);
        assert!(health_ok(&format!("{}/ready/health", server.url())).await);
        assert!(!health_ok(&format!("{}/missing/health", server.url())).await);

        loading.assert_async().await;
        ready.assert_async().await;
    }

    /// 子进程在就绪前退出时要立刻报错，并带上日志尾部。
    struct ExitedChild;

    impl RuntimeChild for ExitedChild {
        fn id(&self) -> u32 {
            0
        }

        fn try_wait(&mut self) -> Result<Option<i32>, PolishRuntimeError> {
            Ok(Some(1))
        }

        fn kill<'a>(
            &'a mut self,
        ) -> Pin<Box<dyn Future<Output = Result<(), PolishRuntimeError>> + Send + 'a>> {
            Box::pin(async { Ok(()) })
        }
    }

    #[tokio::test]
    async fn test_wait_ready_reports_early_exit_with_log_tail() {
        let tail = LogTail::default();
        tail.push("error: failed to load model".to_string());
        let mut child = ExitedChild;
        let err = wait_ready(9, Duration::from_secs(5), &mut child, &tail)
            .await
            .unwrap_err();
        let text = err.to_string();
        assert!(text.contains("就绪前退出"), "{text}");
        assert!(text.contains("failed to load model"), "{text}");
    }

    /// 探针一直不成功时要走到超时分支，而不是无限等待。
    #[tokio::test]
    async fn test_wait_ready_times_out_when_never_ready() {
        struct AliveChild;

        impl RuntimeChild for AliveChild {
            fn id(&self) -> u32 {
                0
            }

            fn try_wait(&mut self) -> Result<Option<i32>, PolishRuntimeError> {
                Ok(None)
            }

            fn kill<'a>(
                &'a mut self,
            ) -> Pin<Box<dyn Future<Output = Result<(), PolishRuntimeError>> + Send + 'a>>
            {
                Box::pin(async { Ok(()) })
            }
        }

        let mut child = AliveChild;
        let err = wait_ready(1, Duration::ZERO, &mut child, &LogTail::default())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("未在"), "{err}");
    }

    /// 平台实现要能在真实进程上完成启动、探活与回收。
    #[tokio::test]
    async fn test_platform_process_spawns_and_kills_real_child() {
        use crate::polish_runtime::PlatformRuntimeProcess;
        let process = PlatformRuntimeProcess::new();
        let log_tail = LogTail::default();
        let (exe, args) = long_running_command();
        let mut child = process
            .spawn(Path::new(&exe), &args, log_tail.clone())
            .unwrap();
        assert!(child.id() > 0);
        assert_eq!(child.try_wait().unwrap(), None);
        child.kill().await.unwrap();
        let mut exited = None;
        for _ in 0..100 {
            if let Some(code) = child.try_wait().unwrap() {
                exited = Some(code);
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(exited.is_some(), "被 kill 的子进程应当很快退出");
    }

    /// 不存在的可执行文件要给出明确错误，而不是静默成功。
    #[tokio::test]
    async fn test_platform_process_reports_missing_executable() {
        use crate::polish_runtime::PlatformRuntimeProcess;
        let process = PlatformRuntimeProcess::new();
        let result = process.spawn(
            Path::new("definitely-missing-altgo-runtime"),
            &[],
            LogTail::default(),
        );
        assert!(result.is_err());
    }

    /// 平台无关的长命令：Windows 用 ping，Unix 用 sleep。
    #[cfg(windows)]
    fn long_running_command() -> (String, Vec<String>) {
        (
            "ping".to_string(),
            vec!["-n".to_string(), "30".to_string(), "127.0.0.1".to_string()],
        )
    }

    #[cfg(unix)]
    fn long_running_command() -> (String, Vec<String>) {
        ("sleep".to_string(), vec!["30".to_string()])
    }

    #[test]
    fn test_physical_core_count_is_plausible() {
        use crate::polish_runtime::PlatformRuntimeProcess;
        let process = PlatformRuntimeProcess::new();
        let physical = process.physical_core_count();
        let logical = std::thread::available_parallelism().map_or(1, |n| n.get());
        assert!(physical >= 1);
        assert!(
            physical <= logical,
            "物理核 {physical} 不应超过逻辑核 {logical}"
        );
    }
}
