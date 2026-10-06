//! altgo 核心库。
//!
//! 包含所有平台的语音转文字管道逻辑：
//! 按键监听 → 状态机 → 录音 → 语音识别 → 文本润色 → 输出

pub mod audio;
pub mod cmd;
pub mod config;
pub mod config_store;
pub mod display_backend;
pub mod error;
pub mod history;
pub mod key_capture;
pub mod key_listener;
pub mod mimo_asr;
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
pub(crate) fn spawn_pipeline_thread(
    app: &tauri::AppHandle,
    cfg: Arc<config::Config>,
    pipeline_status: Arc<std::sync::RwLock<crate::pipeline_controller::PipelineStatus>>,
) -> PipelineHandle {
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
    // 重试请求通道：sender 交给 managed state，receiver 进流水线主循环。
    let (retry_tx, retry_rx) = tokio::sync::mpsc::unbounded_channel::<()>();
    app.state::<voice_pipeline::RetryRequestHandle>()
        .set(retry_tx);
    let pending_store = app
        .state::<voice_pipeline::PendingRecordingStore>()
        .inner()
        .clone();
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
        rt.block_on(voice_pipeline::run(
            cfg,
            stop_rx,
            sink,
            pending_store,
            retry_rx,
        ));
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
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // Wayland 下客户端窗口定位不可用，需在 GUI 初始化前切到 X11 后端
    // （XWayland）。完整因由见 display_backend 模块文档。
    #[cfg(target_os = "linux")]
    if let Some(backend) = display_backend::resolve_display_backend(
        std::env::var_os("WAYLAND_DISPLAY").is_some(),
        std::env::var("GDK_BACKEND").ok().as_deref(),
    ) {
        std::env::set_var("GDK_BACKEND", backend);
    }

    // 共享状态一律经 `Builder::manage` 注册：Tauri 在调用 `setup` 钩子之前就已创建
    // `tauri.conf.json` 里的窗口并开始加载前端，前端首批 IPC（如 `get_config`）可能先于
    // `setup` 到达。状态若只在 `setup` 里注册，这些命令会以 state not managed 失败，
    // 首次引导会永久停在“加载中”。
    let config_path = config::Config::default_config_path();
    let history_path = config_path
        .parent()
        .map(|p| p.join("history.json"))
        .unwrap_or_else(|| config_path.with_extension("history.json"));

    tauri::Builder::default()
        .manage(config_store::ConfigStore::load(config_path))
        .manage(history::HistoryStore::new(history_path))
        .manage(pipeline_controller::PipelineController::new())
        .manage(voice_pipeline::PendingRecordingStore::default())
        .manage(voice_pipeline::RetryRequestHandle::default())
        .manage(Arc::new(output::PlatformOutput::new()) as Arc<dyn output::Output>)
        // 单实例保护：第二个实例启动时唤起已有实例的窗口并自行退出。
        // 避免两个进程各装一个键盘钩子，导致同一次录音被转写、注入两次。
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.show();
                let _ = win.unminimize();
                let _ = win.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let cfg = Arc::new(app.state::<config_store::ConfigStore>().snapshot_blocking());
            cfg.validate().map_err(|e| e.to_string())?;

            tray::create_tray(app)?;

            // 拦截主窗口的关闭请求，让应用驻留托盘。
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
            // 前端可能在 `setup` 之前就调用 save_config（窗口加载早于 setup 钩子），那条路径
            // 已经起过流水线，此时保留既有实例，既不重复启动、也不当作致命错误（返回 Err 会让
            // Tauri 直接 panic 退出）。
            controller.ensure_started_with_blocking(|| {
                spawn_pipeline_thread(app.handle(), cfg, status_arc)
            });

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
            cmd::get_pending_recording,
            cmd::retry_pending_transcription,
            cmd::discard_pending_recording,
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
