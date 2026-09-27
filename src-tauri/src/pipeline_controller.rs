//! 流水线生命周期管理与状态跟踪。

use std::sync::Arc;
use tokio::sync::Mutex;

use crate::PipelineHandle;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PipelineStatus {
    #[default]
    Idle,
    Recording,
    Processing,
    Done,
    Stopped,
}

impl PipelineStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Recording => "recording",
            Self::Processing => "processing",
            Self::Done => "done",
            Self::Stopped => "stopped",
        }
    }
}

/// 管理流水线生命周期。持有运行句柄与共享的状态 Arc。
///
/// 调用方在外部 spawn 流水线线程并经 `start_with` / `ensure_started_with_blocking`
/// 注入句柄，本模块因此不依赖 Tauri 与 sink。
#[derive(Default)]
pub struct PipelineController {
    handle: Mutex<Option<PipelineHandle>>,
    status: Arc<std::sync::RwLock<PipelineStatus>>,
}

impl PipelineController {
    pub fn new() -> Self {
        Self::default()
    }

    /// 共享状态 Arc 的克隆 —— spawn 时传给 sink。
    pub fn status_arc(&self) -> Arc<std::sync::RwLock<PipelineStatus>> {
        self.status.clone()
    }

    pub fn current_status(&self) -> PipelineStatus {
        *self.status.read().unwrap_or_else(|e| e.into_inner())
    }

    /// 用给定的 spawn 闭包启动流水线。
    /// 已有流水线在跑时返回错误。
    pub async fn start_with<F: FnOnce() -> PipelineHandle>(&self, spawn: F) -> Result<(), String> {
        let mut guard = self.handle.lock().await;
        if guard.is_some() {
            return Err("pipeline already running".into());
        }
        *guard = Some(spawn());
        Ok(())
    }

    /// 阻塞地确保流水线在跑：已有实例时保留它并返回 `false`，本次启动则返回 `true`。
    ///
    /// 供同步初始化场景使用。窗口加载可能早于 `setup` 钩子，前端极早的 `save_config`
    /// 会先起流水线；`setup` 不应把它当作致命错误（返回 Err 会让 Tauri 直接 panic），
    /// 也不该重复启动。检查与启动在同一次持锁内完成，不留竞态窗口。
    pub fn ensure_started_with_blocking<F: FnOnce() -> PipelineHandle>(&self, spawn: F) -> bool {
        let mut guard = self.handle.blocking_lock();
        if guard.is_some() {
            return false;
        }
        *guard = Some(spawn());
        true
    }

    pub async fn stop(&self) {
        let handle = {
            let mut guard = self.handle.lock().await;
            (*guard).take()
        };
        if let Some(h) = handle {
            let _ = h.stop_tx.send(());
            // 等待旧 pipeline 线程完全退出，释放 OS 资源（子进程、设备节点、hook）。
            // 不阻塞 tokio 运行时——把阻塞的 join 移到专用线程池。
            let _ = tokio::task::spawn_blocking(move || {
                let _ = h.thread_handle.join();
            })
            .await;
        }
        if let Ok(mut s) = self.status.write() {
            *s = PipelineStatus::Idle;
        }
    }

    /// ExitRequested 处理器使用的阻塞版本。
    /// 等待旧 pipeline 线程完全退出后再返回。
    pub fn stop_blocking(&self) {
        let handle = {
            let mut guard = self.handle.blocking_lock();
            (*guard).take()
        };
        if let Some(h) = handle {
            let _ = h.stop_tx.send(());
            let _ = h.thread_handle.join();
        }
        if let Ok(mut s) = self.status.write() {
            *s = PipelineStatus::Idle;
        }
    }
}

#[cfg(test)]
mod tests {
    //! 行为测试：覆盖 PipelineController 的生命周期，不依赖 Tauri / sink。
    //!
    //! `PipelineHandle` 是公开结构体（`oneshot::Sender` + `JoinHandle`），
    //! 所以这里用一个 "spawn 一个等 stop 信号的线程" 的 fake 闭包来构造它，
    /// 完全不触碰真实管道。
    use super::*;
    use crate::PipelineHandle;

    /// 构造一个合法的 fake handle：spawn 一个线程，它阻塞直到收到 stop 信号。
    /// 返回 handle 和一个共享标志，可在线程退出后检查它确实收到信号。
    fn fake_spawn() -> (PipelineHandle, Arc<std::sync::atomic::AtomicBool>) {
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
        let stopped = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stopped_clone = Arc::clone(&stopped);
        let thread_handle = std::thread::spawn(move || {
            // 阻塞等待 stop 信号；永不主动退出（模拟长生命周期管道线程）。
            let _ = stop_rx.blocking_recv();
            stopped_clone.store(true, std::sync::atomic::Ordering::SeqCst);
        });
        (
            PipelineHandle {
                stop_tx,
                thread_handle,
            },
            stopped,
        )
    }

    /// 用一个 recv_timeout 把"是否死锁"变成可断言的布尔结果。
    /// 返回 true 表示 `f` 在超时窗口内完成。
    /// 注意：`f` 在独立 OS 线程里跑，若死锁，主线程靠 recv_timeout 仍能超时返回 false。
    fn completes_within<F: FnOnce() + Send + 'static>(timeout: std::time::Duration, f: F) -> bool {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            f();
            let _ = tx.send(());
        });
        rx.recv_timeout(timeout).is_ok()
    }

    #[tokio::test]
    async fn start_then_stop_does_not_deadlock() {
        let ctrl = Arc::new(PipelineController::new());
        let (handle, stopped) = fake_spawn();
        ctrl.start_with(move || handle).await.unwrap();

        // start 后立刻 stop：必须在合理时间内返回（不死锁）。
        let ctrl_clone = Arc::clone(&ctrl);
        assert!(completes_within(
            std::time::Duration::from_secs(3),
            move || {
                ctrl_clone.stop_blocking();
            }
        ));
        assert!(stopped.load(std::sync::atomic::Ordering::SeqCst));
        // stop 后状态复位。
        assert_eq!(ctrl.current_status(), PipelineStatus::Idle);
    }

    #[tokio::test]
    async fn repeated_start_stop_reentrant() {
        // start → stop → start → stop：第二次循环应正常工作，
        // 不留悬挂 handle（stop_blocking 会 join 掉旧线程）。
        let ctrl = Arc::new(PipelineController::new());

        for i in 0..3 {
            let (handle, stopped) = fake_spawn();
            ctrl.start_with(move || handle).await.unwrap();

            let ctrl_clone = Arc::clone(&ctrl);
            assert!(
                completes_within(std::time::Duration::from_secs(3), move || {
                    ctrl_clone.stop_blocking();
                }),
                "iteration {i}: stop_blocking deadlocked"
            );
            assert!(
                stopped.load(std::sync::atomic::Ordering::SeqCst),
                "iteration {i}: worker thread never stopped"
            );
        }

        // 全部循环后状态干净。
        assert_eq!(ctrl.current_status(), PipelineStatus::Idle);
    }

    #[tokio::test]
    async fn double_start_is_rejected() {
        // 已经在跑时再次 start 应返回错误，且不能覆盖已有 handle。
        let ctrl = Arc::new(PipelineController::new());
        let (handle, _stopped) = fake_spawn();
        ctrl.start_with(move || handle).await.unwrap();

        let (second, _stopped2) = fake_spawn();
        let err = ctrl.start_with(move || second).await;
        assert!(err.is_err(), "second start while running should error");

        // 清理：必须能干净停掉（证明第一个 handle 仍在）。
        let ctrl_clone = Arc::clone(&ctrl);
        assert!(completes_within(
            std::time::Duration::from_secs(3),
            move || {
                ctrl_clone.stop_blocking();
            }
        ));
    }

    #[test]
    fn ensure_started_keeps_existing_instance() {
        // 已在跑时 ensure 不应重复启动、也不覆盖既有 handle：第二次的 spawn 闭包一旦被调用
        // 就 panic 让测试失败。
        let ctrl = PipelineController::new();
        let (first, first_stopped) = fake_spawn();

        assert!(
            ctrl.ensure_started_with_blocking(move || first),
            "空状态下应启动流水线"
        );
        assert!(
            !ctrl.ensure_started_with_blocking(|| panic!("已在跑时不应再 spawn")),
            "已在跑时应返回 false"
        );

        ctrl.stop_blocking();
        assert!(
            first_stopped.load(std::sync::atomic::Ordering::SeqCst),
            "既有实例应被停掉（证明 handle 未被第二次调用覆盖）"
        );
    }

    #[tokio::test]
    async fn stop_when_idle_is_noop() {
        // 没启动就 stop_blocking：不应 panic、不应死锁。
        let ctrl = Arc::new(PipelineController::new());
        let ctrl_clone = Arc::clone(&ctrl);
        assert!(completes_within(
            std::time::Duration::from_secs(3),
            move || {
                ctrl_clone.stop_blocking();
            }
        ));
        assert_eq!(ctrl.current_status(), PipelineStatus::Idle);
    }
}
