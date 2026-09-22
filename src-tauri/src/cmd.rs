//! Tauri commands — 前端通过 IPC 调用的函数。
//!
//! Tauri commands — functions exposed to the frontend over IPC.

use std::{sync::Arc, time::Duration};

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::{
    config::ConfigPatch,
    config_store::ConfigStore,
    history,
    history::HistoryStore,
    key_capture::KeyCapture,
    output,
    overlay::manager::{OverlayManager, OverlayPosition, OverlayState},
    overlay::tauri::TauriOverlayWindow,
    pipeline_controller::PipelineController,
    polisher, voice_pipeline,
};

/// 重启语音流水线的核心编排：
/// 校验配置快照 → 停止旧流水线 → 用 `spawn` 启动新流水线。
///
/// `spawn` 由调用方注入，使本函数不依赖 `AppHandle`，从而可在测试中
/// 用 fake `PipelineHandle` 验证编排，而无需构造 Tauri app。
///
/// Core orchestration for restarting the voice pipeline:
/// validate config snapshot → stop old pipeline → start new pipeline with `spawn`.
///
/// `spawn` is injected by the caller so this function never depends on `AppHandle`; tests can
/// verify orchestration with a fake `PipelineHandle` instead of building a Tauri app.
async fn restart_pipeline<S>(
    controller: &PipelineController,
    cfg: Arc<crate::config::Config>,
    spawn: S,
) -> Result<(), String>
where
    S: FnOnce(Arc<crate::config::Config>) -> crate::PipelineHandle,
{
    cfg.validate().map_err(|e| e.to_string())?;
    controller.stop().await;
    controller.start_with(|| spawn(cfg)).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigResponse {
    pub key_name: String,
    pub linux_evdev_code: Option<u16>,
    pub windows_vk_code: Option<u16>,
    pub language: String,
    pub model: String,
    pub polish_level: String,
    pub polish_model: String,
    pub polish_protocol: String,
    pub polish_thinking_level: String,
    pub polish_api_base_url: String,
    pub gui_language: String,
    pub overlay_position: String,
    pub auto_check_update: bool,
    pub has_polisher_api_key: bool,
    pub inject_text: bool,
}

fn build_config_response(cfg: &crate::config::Config) -> ConfigResponse {
    ConfigResponse {
        key_name: cfg.key_listener.key_name.clone(),
        linux_evdev_code: cfg.key_listener.linux_evdev_code,
        windows_vk_code: cfg.key_listener.windows_vk_code,
        language: cfg.transcriber.language.clone(),
        model: cfg.transcriber.model.clone(),
        polish_level: cfg.polisher.level.clone(),
        polish_model: cfg.polisher.model.clone(),
        polish_protocol: cfg.polisher.protocol.clone(),
        polish_thinking_level: cfg.polisher.thinking_level.clone(),
        polish_api_base_url: cfg.polisher.api_base_url.clone(),
        gui_language: cfg.gui.language.clone(),
        overlay_position: cfg.gui.overlay_position.clone(),
        auto_check_update: cfg.gui.auto_check_update,
        has_polisher_api_key: !cfg.polisher.api_key.trim().is_empty(),
        inject_text: cfg.output.inject_text,
    }
}

#[tauri::command]
pub async fn get_config(config_store: State<'_, ConfigStore>) -> Result<ConfigResponse, String> {
    let cfg = config_store.snapshot().await;
    Ok(build_config_response(&cfg))
}

#[tauri::command]
pub async fn check_update(
    app: AppHandle,
    mode: crate::updater::CheckMode,
) -> Result<crate::updater::UpdateCheckResponse, crate::updater::UpdateErrorResponse> {
    let provider = crate::updater::TauriUpdateProvider { app };
    let support_tier = crate::updater::detect_support_tier();
    crate::updater::check_update_core(
        &provider,
        mode,
        std::time::Duration::from_secs(10),
        support_tier,
    )
    .await
}

#[tauri::command]
pub async fn install_update(
    app: AppHandle,
    controller: State<'_, PipelineController>,
) -> Result<(), String> {
    let provider = crate::updater::TauriUpdateProvider { app };
    crate::updater::install_update_core(&provider, &controller).await
}

#[tauri::command]
pub async fn save_config(
    app: AppHandle,
    config_store: State<'_, ConfigStore>,
    controller: State<'_, PipelineController>,
    patch: ConfigPatch,
) -> Result<(), String> {
    let update = config_store.apply_patch_for_update(patch).await?;
    let cfg = Arc::new(update.config().clone());
    let status_arc = controller.status_arc();
    let result = restart_pipeline(&controller, cfg, move |cfg| {
        crate::spawn_pipeline_thread(&app, cfg, status_arc)
    })
    .await;
    drop(update);
    result
}

#[tauri::command]
pub async fn capture_activation_key(
    controller: State<'_, PipelineController>,
) -> Result<crate::key_capture::CaptureActivationResponse, String> {
    controller.stop().await;
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;

    match tokio::task::spawn_blocking(|| crate::key_capture::PlatformKeyCapture::new().capture())
        .await
    {
        Ok(Ok(r)) => Ok(r),
        Ok(Err(e)) => Err(e),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub async fn start_pipeline(
    app: tauri::AppHandle,
    config_store: State<'_, ConfigStore>,
    controller: State<'_, PipelineController>,
) -> Result<(), String> {
    let cfg = Arc::new(config_store.snapshot().await);
    let status_arc = controller.status_arc();
    controller
        .start_with(|| crate::spawn_pipeline_thread(&app, cfg, status_arc))
        .await
}

async fn copy_text_core(output: Arc<dyn output::Output>, text: String) -> Result<(), String> {
    let out = output.clone_box();
    tokio::task::spawn_blocking(move || out.write_clipboard(&text))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn copy_text(
    output_state: State<'_, Arc<dyn output::Output>>,
    text: String,
) -> Result<(), String> {
    copy_text_core(output_state.inner().clone(), text).await
}

#[tauri::command]
pub async fn hide_overlay(app: tauri::AppHandle) -> Result<(), String> {
    OverlayManager::new(TauriOverlayWindow::new(app), OverlayPosition::BottomCenter)
        .set_state(OverlayState::hidden());
    Ok(())
}

#[tauri::command]
pub async fn list_models() -> Result<Vec<crate::model::ModelEntry>, String> {
    Ok(crate::model::list_all_with_status())
}

pub(crate) type EventEmitter = Arc<dyn Fn(&str, serde_json::Value) + Send + Sync>;

/// 模型下载的核心逻辑，可注入 `emit` 与 `download` 以便测试。
///
/// 真实路径中 `download` 传 `crate::model::download_with_progress`；
/// 测试中可替换为 fake downloader，仅验证事件序列。
///
/// Core model-download logic; `emit` and `download` can be injected for testing.
///
/// On the real path `download` is `crate::model::download_with_progress`; tests may substitute
/// a fake downloader and verify only the event sequence.
pub(crate) async fn download_model_with_emitter<D, F>(
    name: &str,
    emit: EventEmitter,
    download: D,
) -> Result<(), String>
where
    D: FnOnce(String, Box<dyn FnMut(u64, u64) + Send>) -> F,
    F: std::future::Future<Output = Result<std::path::PathBuf, crate::error::ModelError>> + Send,
{
    crate::model::validate_name(name).map_err(|e| e.to_string())?;

    if let Some(info) = crate::model::models_info().iter().find(|m| m.name == name) {
        emit(
            "model-download-progress",
            serde_json::json!({
                "name": name,
                "downloaded": 0_u64,
                "total": info.files.iter().map(|f| f.size_bytes).sum::<u64>(),
            }),
        );
    }

    let name_for_callback = name.to_string();
    let emit_for_progress = Arc::clone(&emit);
    let result = download(
        name.to_string(),
        Box::new(move |downloaded, total| {
            emit_for_progress(
                "model-download-progress",
                serde_json::json!({
                    "name": name_for_callback,
                    "downloaded": downloaded,
                    "total": total,
                }),
            );
        }),
    )
    .await;

    match result {
        Ok(path) => {
            emit(
                "model-download-finished",
                serde_json::json!({
                    "name": name,
                    "success": true,
                    "path": path.to_string_lossy(),
                }),
            );
        }
        Err(e) => {
            emit(
                "model-download-finished",
                serde_json::json!({
                    "name": name,
                    "success": false,
                    "error": e.to_string(),
                }),
            );
        }
    }

    Ok(())
}

#[tauri::command]
pub async fn download_model(app: AppHandle, name: String) -> Result<(), String> {
    let app_task = app.clone();
    let name_task = name.clone();
    tauri::async_runtime::spawn(async move {
        let emit: EventEmitter = Arc::new(move |event: &str, payload: serde_json::Value| {
            let _ = app_task.emit(event, payload);
        });
        let _ = download_model_with_emitter(&name_task, emit, |name, on_progress| async move {
            crate::model::download_with_progress(&name, on_progress).await
        })
        .await;
    });

    Ok(())
}

#[tauri::command]
pub async fn delete_model(name: String) -> Result<(), String> {
    crate::model::delete(&name).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn resolve_model(model: String) -> Result<Option<String>, String> {
    Ok(crate::model::resolve_model_dir(&model).map(|p| p.to_string_lossy().to_string()))
}

#[tauri::command]
pub async fn list_history(
    history_store: State<'_, HistoryStore>,
) -> Result<Vec<history::HistoryEntry>, String> {
    let store = history_store.inner().clone();
    tokio::task::spawn_blocking(move || store.list())
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_history_entries(
    history_store: State<'_, HistoryStore>,
    ids: Vec<String>,
) -> Result<usize, String> {
    let store = history_store.inner().clone();
    tokio::task::spawn_blocking(move || store.delete(&ids))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn clear_history(history_store: State<'_, HistoryStore>) -> Result<(), String> {
    let store = history_store.inner().clone();
    tokio::task::spawn_blocking(move || store.clear())
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// 对历史条目重新润色的可测试核心。
///
/// 把 `AppHandle.emit` 抽象为 `emit` 回调，避免测试中构造 Tauri app。
///
/// Testable core of re-polishing a history entry.
///
/// Abstracts `AppHandle.emit` behind an `emit` callback so tests don't have to build a Tauri app.
pub(crate) async fn polish_history_entry_core(
    config_store: &ConfigStore,
    history_store: &HistoryStore,
    id: &str,
    emit: impl Fn(&str),
) -> Result<history::HistoryEntry, String> {
    let cfg = config_store.snapshot().await;
    let formatter =
        polisher::LLMFormatter::from_config_with_sources(&cfg).map_err(|e| e.to_string())?;
    let polish_level = polisher::PolishLevel::effective(&cfg.polisher.level);

    let updated =
        voice_pipeline::dispatch_history_polish(history_store, id, &formatter, polish_level)
            .await?;

    emit("history-updated");
    Ok(updated)
}

#[tauri::command]
pub async fn polish_history_entry(
    app: AppHandle,
    config_store: State<'_, ConfigStore>,
    history_store: State<'_, HistoryStore>,
    id: String,
) -> Result<history::HistoryEntry, String> {
    polish_history_entry_core(&config_store, &history_store, &id, |event| {
        let _ = app.emit(event, ());
    })
    .await
}

/// 测试润色 API 连接：基于表单当前值发一次最小请求，不落盘、不重启流水线。
///
/// 密钥为空时回落到已保存的密钥，方便「填好后未保存」与「已保存」两种状态都能测。
///
/// Tests the polish API connection: sends one minimal request from current form values, without
/// persisting or restarting the pipeline.
///
/// When the key field is empty it falls back to the saved key, covering both the
/// "filled but not saved" and "saved" states in tests.
#[tauri::command]
pub async fn test_polisher_connection(
    config_store: State<'_, ConfigStore>,
    protocol: String,
    api_base_url: String,
    api_key: String,
    model: String,
) -> Result<(), String> {
    let stored = config_store.snapshot().await;
    let api_key = if api_key.trim().is_empty() {
        stored.polisher.api_key.clone()
    } else {
        api_key
    };

    if api_base_url.trim().is_empty() {
        return Err("API 地址为空，请先填写。".to_string());
    }
    if model.trim().is_empty() {
        return Err("模型名称为空，请先填写。".to_string());
    }
    if api_key.trim().is_empty() {
        return Err("API 密钥为空，请先填写（或此前已保存过密钥）。".to_string());
    }

    let proto = protocol
        .parse::<polisher::protocol::ApiProtocol>()
        .map_err(|e| polisher::describe_test_error(&e))?;

    polisher::test_connection(api_key.trim(), api_base_url.trim(), model.trim(), proto)
        .await
        .map_err(|e| polisher::describe_test_error(&e))
}

/// 供应商目录地址（与 `omp models` 同源）。拉取在 Rust 侧完成：WebView 受 CSP
/// `connect-src 'self'` 限制，跨域请求统一走 reqwest（与润色测试连接、模型下载同范式）。
///
/// Provider catalog URL (same source as `omp models`). Fetched on the Rust side: the WebView
/// runs under CSP `connect-src 'self'`, so cross-origin requests go through reqwest like
/// every other network call (polisher test connection, model download).
const PROVIDER_CATALOG_URL: &str = "https://catalog.stencil.so/models.json";

/// 从给定 URL 拉取供应商目录 JSON，返回原始值交给前端解析（`parseCatalog`）。
///
/// Fetches the provider catalog from `url` and returns the raw JSON value; the frontend
/// parses it via `parseCatalog`.
async fn fetch_provider_catalog_from(url: &str) -> Result<serde_json::Value, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| format!("目录拉取失败：{e}"))?;
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("目录拉取失败：{e}"))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("目录服务返回 HTTP {status}"));
    }
    resp.json::<serde_json::Value>()
        .await
        .map_err(|e| format!("目录解析失败：{e}"))
}

/// 拉取 omp 供应商目录（设置页供应商清单的唯一来源）。
///
/// Fetches the omp provider catalog (the sole source of the Settings provider list).
#[tauri::command]
pub async fn fetch_provider_catalog() -> Result<serde_json::Value, String> {
    fetch_provider_catalog_from(PROVIDER_CATALOG_URL).await
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    use crate::config::Config;
    use crate::config_store::ConfigStore;
    use crate::error::{ModelError, OutputError};
    use crate::history::HistoryStore;
    use crate::output::Output;
    use crate::pipeline_controller::{PipelineController, PipelineStatus};

    use super::*;

    /// 目录拉取成功：返回原始 JSON 值，交前端 `parseCatalog` 解析。
    /// Catalog fetch success: returns the raw JSON value for the frontend to parse.
    #[tokio::test]
    async fn fetch_provider_catalog_from_parses_json() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/models.json")
            .with_body(r#"{"openai":{"api":"https://api.openai.com/v1"}}"#)
            .create_async()
            .await;

        let value = fetch_provider_catalog_from(&format!("{}/models.json", server.url()))
            .await
            .expect("catalog fetch should succeed");

        assert_eq!(
            value["openai"]["api"], "https://api.openai.com/v1",
            "raw JSON must pass through untouched"
        );
        mock.assert();
    }

    /// 目录服务返回非 2xx：报可读的 HTTP 状态错误（设置页据此展示并可重试）。
    /// Catalog service returns non-2xx: a readable HTTP status error the Settings page shows.
    #[tokio::test]
    async fn fetch_provider_catalog_from_reports_http_error() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/models.json")
            .with_status(503)
            .create_async()
            .await;

        let err = fetch_provider_catalog_from(&format!("{}/models.json", server.url()))
            .await
            .expect_err("HTTP 503 must be an error");

        assert!(
            err.contains("503"),
            "error should name the status, got: {err}"
        );
    }

    /// 构造一个等待 stop 信号后才退出的 fake pipeline handle，
    /// 用于验证 `restart_pipeline` 的停止/启动编排。
    /// Builds a fake pipeline handle that exits only after receiving the stop signal,
    /// used to verify the stop/start orchestration of `restart_pipeline`.
    fn fake_spawn() -> (crate::PipelineHandle, Arc<AtomicBool>) {
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
        let stopped = Arc::new(AtomicBool::new(false));
        let stopped_clone = Arc::clone(&stopped);
        let thread_handle = std::thread::spawn(move || {
            let _ = stop_rx.blocking_recv();
            stopped_clone.store(true, Ordering::SeqCst);
        });
        (
            crate::PipelineHandle {
                stop_tx,
                thread_handle,
            },
            stopped,
        )
    }

    fn gated_stop_spawn(
        stopping: tokio::sync::oneshot::Sender<()>,
        release: tokio::sync::oneshot::Receiver<()>,
    ) -> crate::PipelineHandle {
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
        let thread_handle = std::thread::spawn(move || {
            let _ = stop_rx.blocking_recv();
            let _ = stopping.send(());
            let _ = release.blocking_recv();
        });
        crate::PipelineHandle {
            stop_tx,
            thread_handle,
        }
    }

    #[tokio::test]
    async fn restart_pipeline_stops_old_and_starts_new() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = ConfigStore::load(temp_dir.path().join("altgo.toml"));
        let controller = PipelineController::new();

        // 先启动一个旧流水线。
        // Start an old pipeline first.
        let (old_handle, old_stopped) = fake_spawn();
        controller.start_with(move || old_handle).await.unwrap();

        // restart_pipeline 应停止旧流水线并启动新流水线。
        // restart_pipeline should stop the old pipeline and start the new one.
        let (new_handle, _new_stopped) = fake_spawn();
        let result = restart_pipeline(&controller, Arc::new(store.snapshot().await), move |_cfg| {
            new_handle
        })
        .await;

        assert!(result.is_ok());
        assert!(
            old_stopped.load(Ordering::SeqCst),
            "old pipeline should be stopped"
        );
        assert_eq!(controller.current_status(), PipelineStatus::Idle);
    }

    #[tokio::test]
    async fn restart_pipeline_passes_supplied_config_to_spawn() {
        let controller = PipelineController::new();
        let mut config = Config::default();
        config.transcriber.language = "en".to_string();
        let (handle, _stopped) = fake_spawn();
        let received_language = Arc::new(Mutex::new(None));
        let received_language_clone = Arc::clone(&received_language);

        restart_pipeline(&controller, Arc::new(config), move |cfg| {
            *received_language_clone.lock().unwrap() = Some(cfg.transcriber.language.clone());
            handle
        })
        .await
        .unwrap();

        assert_eq!(
            received_language.lock().unwrap().as_deref(),
            Some("en"),
            "spawn should receive the validated configuration snapshot"
        );
    }

    #[tokio::test]
    async fn restart_pipeline_keeps_its_save_config_current() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = Arc::new(ConfigStore::load(temp_dir.path().join("altgo.toml")));
        let controller = PipelineController::new();
        let (stopping_tx, mut stopping_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        controller
            .start_with(move || gated_stop_spawn(stopping_tx, release_rx))
            .await
            .unwrap();

        let update = store
            .apply_patch_for_update(serde_json::from_str(r#"{"language":"en"}"#).unwrap())
            .await
            .unwrap();
        let cfg = Arc::new(update.config().clone());
        let (spawned_tx, spawned_rx) = tokio::sync::oneshot::channel();
        let restart = restart_pipeline(&controller, cfg, move |cfg| {
            let _ = spawned_tx.send(cfg.transcriber.language.clone());
            fake_spawn().0
        });
        tokio::pin!(restart);

        tokio::select! {
            result = &mut restart => panic!("restart completed before old pipeline stopped: {result:?}"),
            _ = &mut stopping_rx => {}
        }

        let second_store = Arc::clone(&store);
        let second_save = tokio::spawn(async move {
            second_store
                .apply_patch(serde_json::from_str(r#"{"language":"ja"}"#).unwrap())
                .await
        });

        assert!(
            !second_save.is_finished(),
            "another save must wait until the current pipeline restart completes"
        );
        release_tx.send(()).unwrap();

        restart.await.unwrap();
        drop(update);

        assert_eq!(spawned_rx.await.unwrap(), "en");
        second_save.await.unwrap().unwrap();
        assert_eq!(store.snapshot().await.transcriber.language, "ja");
    }

    #[tokio::test]
    async fn restart_pipeline_fails_when_config_invalid() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("altgo.toml");
        let mut cfg = Config::default();
        cfg.polisher.level = "medium".to_string();
        cfg.polisher.api_key = String::new();
        cfg.save(&path).unwrap();

        let store = ConfigStore::load(path);
        let controller = PipelineController::new();

        let result = restart_pipeline(
            &controller,
            Arc::new(store.snapshot().await),
            |_cfg| unreachable!(),
        )
        .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("api_key") || err.contains("API"),
            "expected polisher API key validation error, got: {err}"
        );
    }

    #[test]
    fn build_config_response_maps_fields() {
        let mut cfg = Config::default();
        cfg.key_listener.key_name = "space".to_string();
        cfg.key_listener.linux_evdev_code = Some(56);
        cfg.transcriber.language = "en".to_string();
        cfg.transcriber.model = "sense-voice".to_string();
        cfg.polisher.level = "light".to_string();
        cfg.polisher.thinking_level = "high".to_string();
        cfg.polisher.api_key = "polish-key".to_string();
        cfg.output.inject_text = true;

        let resp = build_config_response(&cfg);
        assert_eq!(resp.key_name, "space");
        assert_eq!(resp.linux_evdev_code, Some(56));
        assert_eq!(resp.language, "en");
        assert_eq!(resp.model, "sense-voice");
        assert!(resp.has_polisher_api_key);
        assert_eq!(resp.polish_level, "light");
        assert_eq!(resp.polish_thinking_level, "high");
        assert_eq!(resp.overlay_position, "bottom_center");
        assert!(resp.auto_check_update);
        assert!(resp.inject_text);
    }

    struct FakeOutput {
        writes: Arc<Mutex<Vec<String>>>,
    }

    impl Output for FakeOutput {
        fn write_clipboard(&self, text: &str) -> Result<(), OutputError> {
            self.writes.lock().unwrap().push(text.to_string());
            Ok(())
        }

        fn clone_box(&self) -> Arc<dyn Output> {
            Arc::new(FakeOutput {
                writes: Arc::clone(&self.writes),
            })
        }
    }

    #[tokio::test]
    async fn copy_text_core_writes_to_clipboard() {
        let writes = Arc::new(Mutex::new(Vec::new()));
        let output = Arc::new(FakeOutput {
            writes: Arc::clone(&writes),
        });

        copy_text_core(output, "hello".to_string()).await.unwrap();

        assert_eq!(writes.lock().unwrap().as_slice(), &["hello".to_string()]);
    }

    #[tokio::test]
    async fn download_model_with_emitter_success_emits_event_sequence() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let events2 = Arc::clone(&events);

        let emit: EventEmitter = Arc::new(move |event, payload| {
            events2.lock().unwrap().push((event.to_string(), payload))
        });

        download_model_with_emitter("sense-voice", emit, |_name, mut on_progress| async move {
            on_progress(0, 100);
            on_progress(50, 100);
            on_progress(100, 100);
            Ok(PathBuf::from("/models/sense-voice"))
        })
        .await
        .unwrap();

        let events = events.lock().unwrap();
        assert_eq!(events.len(), 5);

        assert_eq!(events[0].0, "model-download-progress");
        assert_eq!(events[0].1["name"], "sense-voice");
        assert_eq!(events[0].1["downloaded"], 0);
        let total_size: u64 = crate::model::models_info()
            .iter()
            .find(|m| m.name == "sense-voice")
            .unwrap()
            .files
            .iter()
            .map(|f| f.size_bytes)
            .sum();
        assert_eq!(events[0].1["total"], total_size);

        assert_eq!(events[1].0, "model-download-progress");
        assert_eq!(events[1].1["downloaded"], 0);
        assert_eq!(events[2].1["downloaded"], 50);
        assert_eq!(events[3].1["downloaded"], 100);

        assert_eq!(events[4].0, "model-download-finished");
        assert_eq!(events[4].1["success"], true);
        assert_eq!(events[4].1["path"], "/models/sense-voice");
    }

    #[tokio::test]
    async fn download_model_with_emitter_failure_emits_finished_with_error() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let events2 = Arc::clone(&events);

        let emit: EventEmitter = Arc::new(move |event, payload| {
            events2.lock().unwrap().push((event.to_string(), payload))
        });

        download_model_with_emitter("sense-voice", emit, |_name, _on_progress| async move {
            Err(ModelError::DownloadFailed("network down".to_string()))
        })
        .await
        .unwrap();

        let events = events.lock().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].0, "model-download-progress");
        assert_eq!(events[1].0, "model-download-finished");
        assert_eq!(events[1].1["success"], false);
        assert!(events[1].1["error"]
            .as_str()
            .unwrap()
            .contains("network down"));
    }

    #[tokio::test]
    async fn download_model_with_emitter_unknown_model_returns_error_without_events() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let events2 = Arc::clone(&events);

        let emit: EventEmitter = Arc::new(move |event, payload| {
            events2.lock().unwrap().push((event.to_string(), payload))
        });

        let result =
            download_model_with_emitter("unknown-model", emit, |_name, _on_progress| async move {
                Ok(PathBuf::from("/tmp/x.bin"))
            })
            .await;

        assert!(result.is_err());
        assert!(events.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn polish_history_entry_core_success_updates_history_and_emits() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .match_header("Authorization", "Bearer polish-key")
            .with_status(200)
            .with_body(r#"{"choices":[{"message":{"role":"assistant","content":"润色后的文本"}}]}"#)
            .create_async()
            .await;

        let history_dir = tempfile::tempdir().unwrap();
        let store = HistoryStore::new(history_dir.path().join("history.json"));
        let entry = store
            .append("原始文本".to_string(), "原始文本".to_string())
            .unwrap();

        let cfg_dir = tempfile::tempdir().unwrap();
        let cfg_path = cfg_dir.path().join("altgo.toml");
        let mut cfg = Config::default();
        cfg.transcriber.language = "zh".to_string();
        cfg.polisher.level = "light".to_string();
        cfg.polisher.api_key = "polish-key".to_string();
        cfg.polisher.api_base_url = server.url();
        cfg.polisher.model = "model".to_string();
        cfg.save(&cfg_path).unwrap();
        let config_store = ConfigStore::load(cfg_path);

        let emitted = Arc::new(Mutex::new(Vec::new()));
        let emitted2 = Arc::clone(&emitted);
        let updated = polish_history_entry_core(&config_store, &store, &entry.id, |event| {
            emitted2.lock().unwrap().push(event.to_string())
        })
        .await
        .unwrap();

        assert_eq!(updated.raw_text, "原始文本");
        assert_eq!(updated.text, "润色后的文本");
        let fetched = store.get(&entry.id).unwrap().unwrap();
        assert_eq!(fetched.text, "润色后的文本");
        assert_eq!(
            emitted.lock().unwrap().as_slice(),
            &["history-updated".to_string()]
        );

        mock.assert_async().await;
    }

    #[tokio::test]
    async fn polish_history_entry_core_missing_entry_returns_error() {
        let history_dir = tempfile::tempdir().unwrap();
        let store = HistoryStore::new(history_dir.path().join("history.json"));

        let cfg_dir = tempfile::tempdir().unwrap();
        let cfg_path = cfg_dir.path().join("altgo.toml");
        Config::default().save(&cfg_path).unwrap();
        let config_store = ConfigStore::load(cfg_path);

        let emitted = Arc::new(Mutex::new(Vec::new()));
        let result = polish_history_entry_core(&config_store, &store, "missing-id", |event| {
            emitted.lock().unwrap().push(event.to_string())
        })
        .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
        assert!(emitted.lock().unwrap().is_empty());
    }
}
