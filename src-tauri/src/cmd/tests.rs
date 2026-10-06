//! cmd 模块的单元测试：命令的 core 函数与编排逻辑。
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
    let (old_handle, old_stopped) = fake_spawn();
    controller.start_with(move || old_handle).await.unwrap();

    // restart_pipeline 应停止旧流水线并启动新流水线。
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

    let emit: EventEmitter =
        Arc::new(move |event, payload| events2.lock().unwrap().push((event.to_string(), payload)));

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

    let emit: EventEmitter =
        Arc::new(move |event, payload| events2.lock().unwrap().push((event.to_string(), payload)));

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

    let emit: EventEmitter =
        Arc::new(move |event, payload| events2.lock().unwrap().push((event.to_string(), payload)));

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
    let updated = polish_history_entry_core(&config_store, &store, &entry.id, None, |event| {
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
    let result = polish_history_entry_core(&config_store, &store, "missing-id", None, |event| {
        emitted.lock().unwrap().push(event.to_string())
    })
    .await;

    assert!(result.is_err());
    assert!(result.unwrap_err().contains("not found"));
    assert!(emitted.lock().unwrap().is_empty());
}

#[tokio::test]
async fn polish_history_entry_core_global_none_level_still_polishes() {
    // 手动润色固定 medium：即使全局档位为 none 也应发起润色请求并写回 text。
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/v1/chat/completions")
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
    cfg.polisher.level = "none".to_string();
    cfg.polisher.api_key = "polish-key".to_string();
    cfg.polisher.api_base_url = server.url();
    cfg.polisher.model = "model".to_string();
    cfg.save(&cfg_path).unwrap();
    let config_store = ConfigStore::load(cfg_path);

    let updated = polish_history_entry_core(&config_store, &store, &entry.id, None, |_| {})
        .await
        .unwrap();

    assert_eq!(updated.text, "润色后的文本");
    assert_eq!(updated.raw_text, "原始文本");
    mock.assert_async().await;
}

#[tokio::test]
async fn polish_history_entry_core_extra_instruction_reaches_request_body() {
    // 补充指令须透传到请求体：system prompt 末尾带“补充要求：”前缀与指令内容。
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/v1/chat/completions")
        .match_body(mockito::Matcher::Regex("补充要求：语气更客气".to_string()))
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
    cfg.polisher.level = "medium".to_string();
    cfg.polisher.api_key = "polish-key".to_string();
    cfg.polisher.api_base_url = server.url();
    cfg.polisher.model = "model".to_string();
    cfg.save(&cfg_path).unwrap();
    let config_store = ConfigStore::load(cfg_path);

    let updated =
        polish_history_entry_core(&config_store, &store, &entry.id, Some("语气更客气"), |_| {})
            .await
            .unwrap();

    assert_eq!(updated.text, "润色后的文本");
    mock.assert_async().await;
}

#[test]
fn retry_pending_transcription_core_validates_slot_and_pipeline() {
    use crate::error::UserFacingError;
    use crate::voice_pipeline::{PendingRecordingStore, RetryRequestHandle};

    let store = PendingRecordingStore::default();
    let retry = RetryRequestHandle::default();

    // 槽位为空：直接报错，不触碰流水线。
    assert_eq!(
        retry_pending_transcription_core(&store, &retry),
        Err("no pending recording".to_string())
    );

    store.retain(
        vec![0u8; 100],
        UserFacingError {
            code: "transcriber.http_error".to_string(),
            params: None,
        },
    );
    // 流水线未运行：请求投递失败。
    assert_eq!(
        retry_pending_transcription_core(&store, &retry),
        Err("pipeline not running".to_string())
    );

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    retry.set(tx);
    assert!(retry_pending_transcription_core(&store, &retry).is_ok());
    assert_eq!(rx.try_recv(), Ok(()));
}
