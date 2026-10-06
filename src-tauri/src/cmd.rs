//! Tauri commands：前端通过 IPC 调用的函数。

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
    pub transcriber_backend: String,
    pub asr_model: String,
    pub asr_api_base_url: String,
    pub has_asr_api_key: bool,
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
        transcriber_backend: cfg.transcriber.backend.clone(),
        asr_model: cfg.transcriber.online.model.clone(),
        asr_api_base_url: cfg.transcriber.online.api_base_url.clone(),
        // 明文 key 不回读，只回“是否已配置”。
        has_asr_api_key: !cfg.transcriber.online.api_key.trim().is_empty(),
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
) -> Result<crate::updater::UpdateCheckResponse, String> {
    let provider = crate::updater::TauriUpdateProvider { app };
    let support_tier = crate::updater::detect_support_tier();
    crate::updater::check_update_core(
        &provider,
        mode,
        std::time::Duration::from_secs(10),
        support_tier,
    )
    .await
    .map_err(|e| e.message)
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

/// 查看待重试录音元信息（无音频本体）。主窗横幅的初始状态来源。
#[tauri::command]
pub async fn get_pending_recording(
    store: State<'_, voice_pipeline::PendingRecordingStore>,
) -> Result<Option<voice_pipeline::PendingRecordingInfo>, String> {
    Ok(store.peek_info())
}

/// 请求重新识别待重试录音。转写在流水线主循环内串行执行（ADR-0003）。
#[tauri::command]
pub async fn retry_pending_transcription(
    store: State<'_, voice_pipeline::PendingRecordingStore>,
    retry: State<'_, voice_pipeline::RetryRequestHandle>,
) -> Result<(), String> {
    retry_pending_transcription_core(&store, &retry)
}

/// 可测试核心：槽位为空报错、流水线未运行报错，否则投递重试请求。
pub(crate) fn retry_pending_transcription_core(
    store: &voice_pipeline::PendingRecordingStore,
    retry: &voice_pipeline::RetryRequestHandle,
) -> Result<(), String> {
    if store.peek_info().is_none() {
        return Err("no pending recording".to_string());
    }
    if !retry.send() {
        return Err("pipeline not running".to_string());
    }
    Ok(())
}

/// 放弃待重试录音：清空槽位并告知前端撤下横幅。
#[tauri::command]
pub async fn discard_pending_recording(
    app: AppHandle,
    store: State<'_, voice_pipeline::PendingRecordingStore>,
) -> Result<(), String> {
    if store.clear() {
        app.emit(
            voice_pipeline::pending::PENDING_RECORDING_EVENT,
            serde_json::Value::Null,
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn list_models() -> Result<Vec<crate::model::ModelEntry>, String> {
    Ok(crate::model::list_all_with_status())
}

pub(crate) type EventEmitter = Arc<dyn Fn(&str, serde_json::Value) + Send + Sync>;

/// 模型下载的核心逻辑，可注入 `emit` 与 `download` 以便测试。
///
/// 真实路径中 `download` 传 `crate::model::download_with_progress`。
/// 测试中可替换为 fake downloader，仅验证事件序列。
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

    emit_initial_download_progress(name, &emit);

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

    emit_download_finished(name, &emit, &result);

    Ok(())
}

/// 下载开始前先发一次 0 字节进度事件，让前端尽早拿到总大小，
/// 目录中查不到该模型时跳过，避免发出缺少 total 的进度事件。
fn emit_initial_download_progress(name: &str, emit: &EventEmitter) {
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
}

/// 下载结束后发一次 finished 事件：成功携带本地路径，失败携带错误文案，
/// 前端按 `success` 字段区分两种终态。
fn emit_download_finished(
    name: &str,
    emit: &EventEmitter,
    result: &Result<std::path::PathBuf, crate::error::ModelError>,
) {
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
/// 手动润色固定 medium 档：这是用户显式触发的动作，绕过全局 none
/// （全局 none 只约束实时流水线）。
pub(crate) async fn polish_history_entry_core(
    config_store: &ConfigStore,
    history_store: &HistoryStore,
    id: &str,
    extra_instruction: Option<&str>,
    emit: impl Fn(&str),
) -> Result<history::HistoryEntry, String> {
    let cfg = config_store.snapshot().await;
    let formatter =
        polisher::LLMFormatter::from_config_with_sources(&cfg).map_err(|e| e.to_string())?;

    let updated = voice_pipeline::dispatch_history_polish(
        history_store,
        id,
        &formatter,
        polisher::PolishLevel::Medium,
        extra_instruction,
    )
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
    extra_instruction: Option<String>,
) -> Result<history::HistoryEntry, String> {
    polish_history_entry_core(
        &config_store,
        &history_store,
        &id,
        extra_instruction.as_deref(),
        |event| {
            let _ = app.emit(event, ());
        },
    )
    .await
}

/// 测试润色 API 连接：基于表单当前值发一次最小请求，不落盘、不重启流水线。
///
/// 密钥为空时回落到已保存的密钥，方便“填好后未保存”与“已保存”两种状态都能测。
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
const PROVIDER_CATALOG_URL: &str = "https://catalog.stencil.so/models.json";

/// 从给定 URL 拉取供应商目录 JSON，返回原始值交给前端解析（`parseCatalog`）。
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
#[tauri::command]
pub async fn fetch_provider_catalog() -> Result<serde_json::Value, String> {
    fetch_provider_catalog_from(PROVIDER_CATALOG_URL).await
}

#[cfg(test)]
mod tests;
