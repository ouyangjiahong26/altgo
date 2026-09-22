//! altgo 核心库。
//!
//! 包含所有平台的语音转文字管道逻辑：
//! 按键监听 → 状态机 → 录音 → 语音识别 → 文本润色 → 输出
//!
//! Core library of altgo.
//!
//! Voice-to-text pipeline logic for all platforms:
//! key listener → state machine → recorder → transcription → text polishing → output

pub mod audio;
pub mod cmd;
pub mod config;
pub mod config_store;
pub mod display_backend;
pub mod error;
pub mod history;
pub mod key_capture;
pub mod key_listener;
pub mod model;
pub mod output;
pub mod overlay;
pub mod pipeline_controller;
pub mod polisher;
pub mod prompt_store;
pub mod recorder;
pub mod resource;
pub mod sherpa;
pub mod state_machine;
pub mod tauri_sink;
pub mod transcriber;
pub mod tray;
pub mod updater;
pub mod voice_pipeline;

use std::sync::Arc;
use tauri::Manager;

pub struct PipelineHandle {
    pub stop_tx: tokio::sync::oneshot::Sender<()>,
    pub thread_handle: std::thread::JoinHandle<()>,
}

/// 在专用 OS 线程上构造 Tauri pipeline sink 并拉起语音流水线。
/// 返回一个停止句柄，调用方可用它终止事件循环。
///
/// 集中在此处理，让 `cmd.rs` 只暴露 `#[tauri::command]` 函数，
/// 不必把 sink 与生命周期构造细节穿过 IPC 层传递。
///
/// Build the Tauri pipeline sink and spawn the voice pipeline on a dedicated
/// OS thread. Returns a stop handle the caller can use to terminate the loop.
///
/// Centralised here so `cmd.rs` only exposes `#[tauri::command]` functions
/// and never has to thread the sink/lifecycle construction through IPC.
pub(crate) fn spawn_pipeline_thread(
    app: &tauri::AppHandle,
    cfg: Arc<config::Config>,
    pipeline_status: Arc<std::sync::RwLock<crate::pipeline_controller::PipelineStatus>>,
) -> PipelineHandle {
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
    let app_handle = app.clone();
    let cfg_clone = cfg.clone();

    let overlay: Arc<dyn overlay::seam::OverlaySink> = Arc::new(
        overlay::manager::OverlayManager::new(
            overlay::tauri::TauriOverlayWindow::new(app_handle.clone()),
            overlay::seam::OverlayPosition::effective(&cfg.gui.overlay_position),
        )
        .with_auto_fade(overlay::manager::platform_auto_fade_policy()),
    );

    let thread_handle = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to build tokio runtime");
        let output: Arc<dyn output::Output> = app_handle
            .state::<Arc<dyn output::Output>>()
            .inner()
            .clone();
        let inject_text = cfg_clone.output.inject_text;
        let dispatch: Arc<dyn voice_pipeline::TranscriptionDispatch> =
            Arc::new(voice_pipeline::TranscriptionDispatcherImpl {
                output,
                history_store: app_handle
                    .state::<crate::history::HistoryStore>()
                    .inner()
                    .clone(),
                inject_text,
            });
        let sink = tauri_sink::TauriPipelineSink::new(
            Arc::new(tauri_sink::TauriEventEmitter::new(app_handle.clone())),
            pipeline_status,
            cfg_clone,
            dispatch,
            overlay,
        );
        rt.block_on(voice_pipeline::run(cfg, stop_rx, sink));
    });
    PipelineHandle {
        stop_tx,
        thread_handle,
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 初始化日志订阅器：tracing 宏在没有任何 subscriber 时静默丢弃所有日志，
    // 级别取 RUST_LOG（如 `RUST_LOG=debug altgo`），未设置时回退 info。
    // Install the log subscriber: tracing macros are silently dropped without one;
    // the level comes from RUST_LOG (e.g. `RUST_LOG=debug altgo`), defaulting to info.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // Wayland 下客户端窗口定位不可用，需在 GUI 初始化前切到 X11 后端
    // （XWayland）；完整因由见 display_backend 模块文档。
    // Client window positioning is unavailable under Wayland; switch to the X11 backend (XWayland)
    // before GUI init. See the display_backend module docs for the full rationale.
    #[cfg(target_os = "linux")]
    if let Some(backend) = display_backend::resolve_display_backend(
        std::env::var_os("WAYLAND_DISPLAY").is_some(),
        std::env::var("GDK_BACKEND").ok().as_deref(),
    ) {
        std::env::set_var("GDK_BACKEND", backend);
    }

    tauri::Builder::default()
        // 单实例保护：第二个实例启动时唤起已有实例的窗口并自行退出。
        // 避免两个进程各装一个键盘钩子，导致同一次录音被转写、注入两次。
        // Single-instance guard: a second instance wakes the existing instance's window and exits,
        // preventing two processes from each holding a keyboard hook and transcribing/injecting
        // the same recording twice.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.show();
                let _ = win.unminimize();
                let _ = win.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let config_path = config::Config::default_config_path();
            let history_path = config_path
                .parent()
                .map(|p| p.join("history.json"))
                .unwrap_or_else(|| config_path.with_extension("history.json"));

            let config_store = config_store::ConfigStore::load(config_path);
            let history_store = history::HistoryStore::new(history_path);
            let pipeline_controller = pipeline_controller::PipelineController::new();

            let cfg = Arc::new(config_store.snapshot_blocking());
            cfg.validate().map_err(|e| e.to_string())?;

            app.manage(config_store);
            app.manage(history_store);
            app.manage(pipeline_controller);
            app.manage(Arc::new(output::PlatformOutput::new()) as Arc<dyn output::Output>);

            tray::create_tray(app)?;

            // 拦截主窗口的关闭请求，让应用驻留托盘。
            // Intercept close requests on the main window so the app stays in the tray.
            if let Some(window) = app.get_webview_window("main") {
                let app_handle = app.handle().clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        if let Some(win) = app_handle.get_webview_window("main") {
                            let _ = win.hide();
                        }
                    }
                });
            }

            let controller = app.state::<pipeline_controller::PipelineController>();
            let status_arc = controller.status_arc();
            controller
                .start_with_blocking(|| spawn_pipeline_thread(app.handle(), cfg, status_arc))?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            cmd::get_config,
            cmd::save_config,
            cmd::start_pipeline,
            cmd::copy_text,
            cmd::hide_overlay,
            cmd::list_models,
            cmd::download_model,
            cmd::delete_model,
            cmd::resolve_model,
            cmd::capture_activation_key,
            cmd::test_polisher_connection,
            cmd::fetch_provider_catalog,
            cmd::list_history,
            cmd::delete_history_entries,
            cmd::clear_history,
            cmd::polish_history_entry,
            cmd::check_update,
            cmd::install_update,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                app_handle
                    .state::<pipeline_controller::PipelineController>()
                    .stop_blocking();
            }
        });
}
