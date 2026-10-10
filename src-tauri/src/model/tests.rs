//! model 模块的单元测试：下载、校验、模型目录解析。

use super::*;

/// 把字节数格式化为人类可读的大小。
fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.0} MB", bytes as f64 / MB as f64)
    } else {
        format!("{:.0} KB", bytes as f64 / KB as f64)
    }
}

fn test_model_info(model_payload: &[u8], tokens_payload: &[u8]) -> ModelInfo {
    let model_sha256 = Cow::Owned(bytes_to_hex(&Sha256::digest(model_payload)));
    let tokens_sha256 = Cow::Owned(bytes_to_hex(&Sha256::digest(tokens_payload)));
    let files = vec![
        ModelFile {
            filename: MAIN_MODEL_FILENAME,
            size_bytes: model_payload.len() as u64,
            sha256: model_sha256,
        },
        ModelFile {
            filename: TOKENS_FILENAME,
            size_bytes: tokens_payload.len() as u64,
            sha256: tokens_sha256,
        },
    ];

    ModelInfo {
        name: "sense-voice",
        repo_path: "test/repo",
        files: Cow::Owned(files),
        description: "test model",
        engine: EngineKind::SenseVoice,
    }
}

#[test]
fn test_format_size() {
    assert_eq!(format_size(75 * 1024 * 1024), "75 MB");
    assert_eq!(format_size(230 * 1024 * 1024), "230 MB");
    assert_eq!(format_size(2900 * 1024 * 1024), "2.8 GB");
    assert_eq!(format_size(500 * 1024), "500 KB");
}

#[test]
fn test_resolve_model_dir_empty() {
    assert!(resolve_model_dir("").is_none());
}

#[test]
fn test_resolve_model_dir_nonexistent() {
    assert!(resolve_model_dir("/nonexistent/model").is_none());
}

#[test]
fn test_resolve_model_dir_unknown_name() {
    assert!(resolve_model_dir("nonexistent-name").is_none());
}

#[test]
fn test_resolve_model_dir_incomplete_dir() {
    let dir = tempfile::tempdir().unwrap();
    // 只有 tokens.txt 没有主模型时视为未下载
    std::fs::write(dir.path().join("tokens.txt"), b"tok").unwrap();
    assert!(resolve_model_dir(dir.path().to_str().unwrap()).is_none());
}

#[test]
fn test_resolve_model_dir_missing_tokens() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(MAIN_MODEL_FILENAME), b"model").unwrap();
    assert!(resolve_model_dir(dir.path().to_str().unwrap()).is_none());
}

#[test]
fn test_resolve_model_dir_rejects_small_model() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(MAIN_MODEL_FILENAME), b"model").unwrap();
    std::fs::write(dir.path().join(TOKENS_FILENAME), b"tok").unwrap();
    assert!(resolve_model_dir(dir.path().to_str().unwrap()).is_none());
}

#[test]
fn test_resolve_model_dir_rejects_empty_tokens() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join(MAIN_MODEL_FILENAME),
        vec![0; MIN_MODEL_FILE_BYTES as usize],
    )
    .unwrap();
    std::fs::write(dir.path().join(TOKENS_FILENAME), b"").unwrap();
    assert!(resolve_model_dir(dir.path().to_str().unwrap()).is_none());
}

#[test]
fn test_resolve_custom_model_dir_uses_structural_checks() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join(MAIN_MODEL_FILENAME),
        vec![0; MIN_MODEL_FILE_BYTES as usize],
    )
    .unwrap();
    std::fs::write(dir.path().join(TOKENS_FILENAME), b"custom tokens").unwrap();
    assert_eq!(
        resolve_model_dir(dir.path().to_str().unwrap()),
        Some(dir.path().to_path_buf())
    );
}

#[test]
fn test_model_file_ready_requires_matching_checksum() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test-model");
    std::fs::write(&path, b"valid model").unwrap();
    let sha256 = Cow::Owned(bytes_to_hex(&Sha256::digest(b"valid model")));
    let file = ModelFile {
        filename: "test-model",
        size_bytes: 11,
        sha256,
    };

    assert!(model_file_ready(&file, &path));
    std::fs::write(&path, b"corrupted model").unwrap();
    assert!(!model_file_ready(&file, &path));
}

#[test]
fn test_models_dir_contains_altgo() {
    let dir = models_dir();
    assert!(dir.to_string_lossy().contains("altgo"));
    assert!(dir.to_string_lossy().contains("models"));
}

#[test]
fn test_validate_name_known() {
    assert!(validate_name("sense-voice").is_ok());
}

#[test]
fn test_validate_name_unknown() {
    assert!(validate_name("nonexistent").is_err());
    assert!(validate_name("").is_err());
}

#[test]
fn test_engine_for_registered_models() {
    assert_eq!(engine_for("sense-voice"), EngineKind::SenseVoice);
    assert_eq!(engine_for("sense-voice-yue"), EngineKind::SenseVoice);
    assert_eq!(engine_for("fire-red-asr2-ctc"), EngineKind::FireRedAsrCtc);
}

#[test]
fn test_engine_for_custom_path_defaults_to_sense_voice() {
    // 自定义路径（目录或 .onnx 文件）不在注册表里，按 SenseVoice 处理。
    assert_eq!(
        engine_for("/home/user/models/my-model"),
        EngineKind::SenseVoice
    );
    assert_eq!(engine_for(""), EngineKind::SenseVoice);
    assert_eq!(engine_for("unknown-name"), EngineKind::SenseVoice);
}

#[test]
fn test_list_all_with_status_count() {
    let entries = list_all_with_status();
    assert_eq!(entries.len(), models_info().len());
    assert!(entries.iter().any(|e| e.name == "sense-voice"));
    // 主模型文件名应暴露给前端展示
    assert!(entries.iter().all(|e| e.filename == MAIN_MODEL_FILENAME));
}

#[test]
fn test_model_registry_entries() {
    let names: Vec<_> = models_info().iter().map(|m| m.name).collect();
    assert!(names.contains(&"sense-voice"));
    assert!(names.contains(&"sense-voice-yue"));
    // 模型名必须唯一，否则 models/ 下目录会互相覆盖
    let unique: std::collections::HashSet<_> = names.iter().collect();
    assert_eq!(unique.len(), names.len());

    // 每个模型都要有自己的仓库路径，下载基址按它拼接
    let repos: Vec<_> = models_info().iter().map(|m| m.repo_path).collect();
    assert!(repos.iter().all(|r| !r.is_empty()));
    let unique_repos: std::collections::HashSet<_> = repos.iter().collect();
    assert_eq!(unique_repos.len(), repos.len());

    // 两个模型的主模型校验和不同（tokens 词表相同）。若被误改成同一 SHA，
    // 其中一个模型的 is_downloaded 会永远判 false
    let sha_of =
        |name: &str| &models_info().iter().find(|m| m.name == name).unwrap().files[0].sha256;
    assert_ne!(sha_of("sense-voice"), sha_of("sense-voice-yue"));
}

#[test]
fn test_model_download_bases_per_repo() {
    // 清掉外部覆盖，验证默认的官方 + 镜像双源拼接
    std::env::remove_var(ENV_MODEL_BASE_URL);
    assert_eq!(
        model_download_bases("owner/repo"),
        vec![
            "https://huggingface.co/owner/repo/resolve/main".to_string(),
            "https://hf-mirror.com/owner/repo/resolve/main".to_string(),
        ]
    );
}

#[test]
fn test_delete_unknown_model_errors() {
    assert!(delete("nonexistent_model").is_err());
}

#[test]
fn test_delete_missing_dir_ok() {
    let dir = tempfile::tempdir().unwrap();
    delete_from_root("sense-voice", dir.path()).unwrap();
    assert!(!dir.path().join("sense-voice").exists());
}

/// 一次成功下载的产物，供按断言组拆分的测试分别检查。
struct SuccessfulDownloadRun {
    /// 持有临时目录，drop 时才清理，文件断言在此之前始终有效。
    _tmp_dir: tempfile::TempDir,
    /// Server 被 drop 时会清空 mock 状态，需保活到 mock 断言结束。
    _server: mockito::ServerGuard,
    dest_dir: PathBuf,
    result: PathBuf,
    model_payload: Vec<u8>,
    tokens_payload: Vec<u8>,
    /// 两个文件声明 size_bytes 之和，进度回调的 total 应恒等于它。
    declared_total_bytes: u64,
    progress_calls: std::sync::Arc<std::sync::Mutex<Vec<(u64, u64)>>>,
    model_mock: mockito::Mock,
    tokens_mock: mockito::Mock,
}

/// 起 mockito 服务端按成功路径完整下载两个模型文件。
async fn run_successful_download() -> SuccessfulDownloadRun {
    let mut server = mockito::Server::new_async().await;
    let model_payload = vec![0u8; MIN_MODEL_FILE_BYTES as usize + 1];
    let tokens_payload = b"token list".to_vec();
    let model_mock = server
        .mock("GET", "/model.int8.onnx")
        .with_status(200)
        .with_header("content-type", "application/octet-stream")
        .with_body(&model_payload)
        .create_async()
        .await;
    let tokens_mock = server
        .mock("GET", "/tokens.txt")
        .with_status(200)
        .with_body(&tokens_payload)
        .create_async()
        .await;

    let tmp_dir = tempfile::tempdir().unwrap();
    let dest_dir = tmp_dir.path().join("sense-voice");
    let info = test_model_info(&model_payload, &tokens_payload);
    let declared_total_bytes = info.files.iter().map(|file| file.size_bytes).sum();

    let progress_calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let calls = progress_calls.clone();
    let result = download_model_with_progress_to(
        &info,
        vec![server.url()],
        tmp_dir.path().to_path_buf(),
        move |d, t| calls.lock().unwrap().push((d, t)),
    )
    .await
    .unwrap();

    SuccessfulDownloadRun {
        _tmp_dir: tmp_dir,
        _server: server,
        dest_dir,
        result,
        model_payload,
        tokens_payload,
        declared_total_bytes,
        progress_calls,
        model_mock,
        tokens_mock,
    }
}

#[tokio::test]
async fn test_download_success_writes_dest_files() {
    let run = run_successful_download().await;

    assert_eq!(run.result, run.dest_dir);
    assert!(run.dest_dir.join(MAIN_MODEL_FILENAME).exists());
    assert_eq!(
        std::fs::metadata(run.dest_dir.join(MAIN_MODEL_FILENAME))
            .unwrap()
            .len(),
        run.model_payload.len() as u64
    );
    assert_eq!(
        std::fs::read(run.dest_dir.join("tokens.txt")).unwrap(),
        run.tokens_payload
    );
    run.model_mock.assert_async().await;
    run.tokens_mock.assert_async().await;
}

#[tokio::test]
async fn test_download_success_clears_tmp_file() {
    let run = run_successful_download().await;

    assert!(!run.dest_dir.join("model.int8.onnx.tmp").exists());
}

#[tokio::test]
async fn test_download_success_reports_progress() {
    let run = run_successful_download().await;

    let calls = run.progress_calls.lock().unwrap();
    assert!(!calls.is_empty());
    // total 恒为声明总大小，进度按实际下载字节累计
    let total = calls.last().unwrap().1;
    assert_eq!(total, run.declared_total_bytes);
    assert!(calls
        .iter()
        .any(|(d, _)| *d == run.model_payload.len() as u64));
}

#[tokio::test]
async fn test_download_replaces_incomplete_existing_model() {
    let mut server = mockito::Server::new_async().await;
    let model_payload = vec![0u8; MIN_MODEL_FILE_BYTES as usize + 1];
    let tokens_payload = b"tok";
    let model_mock = server
        .mock("GET", "/model.int8.onnx")
        .with_status(200)
        .with_body(&model_payload)
        .create_async()
        .await;

    let info = test_model_info(&model_payload, tokens_payload);
    let tmp_dir = tempfile::tempdir().unwrap();
    let dest_dir = tmp_dir.path().join("sense-voice");
    std::fs::create_dir_all(&dest_dir).unwrap();
    std::fs::write(dest_dir.join(MAIN_MODEL_FILENAME), b"incomplete").unwrap();
    std::fs::write(dest_dir.join(TOKENS_FILENAME), tokens_payload).unwrap();

    download_model_with_progress_to(
        &info,
        vec![server.url()],
        tmp_dir.path().to_path_buf(),
        |_d, _t| {},
    )
    .await
    .unwrap();

    assert_eq!(
        std::fs::metadata(dest_dir.join(MAIN_MODEL_FILENAME))
            .unwrap()
            .len(),
        model_payload.len() as u64
    );
    model_mock.assert_async().await;
}

#[tokio::test]
async fn test_download_http_error_clears_tmp() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/model.int8.onnx")
        .with_status(500)
        .expect_at_least(1)
        .create_async()
        .await;

    let model_payload = vec![0u8; MIN_MODEL_FILE_BYTES as usize + 1];
    let info = test_model_info(&model_payload, b"tok");
    let tmp_dir = tempfile::tempdir().unwrap();
    let dest_dir = tmp_dir.path().join("sense-voice");

    let result = download_model_with_progress_to(
        &info,
        vec![server.url()],
        tmp_dir.path().to_path_buf(),
        |_d, _t| {},
    )
    .await;

    assert!(result.is_err());
    assert!(!dest_dir.join(MAIN_MODEL_FILENAME).exists());
    assert!(!dest_dir.join("model.int8.onnx.tmp").exists());
    mock.assert_async().await;
}

#[tokio::test]
async fn test_download_too_small_detected_as_corrupt() {
    let mut server = mockito::Server::new_async().await;
    let model_payload = vec![0u8; 1024];
    let mock = server
        .mock("GET", "/model.int8.onnx")
        .with_status(200)
        .with_body(&model_payload)
        .expect_at_least(1)
        .create_async()
        .await;

    let info = test_model_info(&model_payload, b"tok");
    let tmp_dir = tempfile::tempdir().unwrap();
    let dest_dir = tmp_dir.path().join("sense-voice");

    let result = download_model_with_progress_to(
        &info,
        vec![server.url()],
        tmp_dir.path().to_path_buf(),
        |_d, _t| {},
    )
    .await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("校验失败"));
    assert!(!dest_dir.join(MAIN_MODEL_FILENAME).exists());
    assert!(!dest_dir.join("model.int8.onnx.tmp").exists());
    mock.assert_async().await;
}

#[tokio::test]
async fn test_download_retries_then_succeeds() {
    let mut server = mockito::Server::new_async().await;
    let model_payload = vec![0u8; MIN_MODEL_FILE_BYTES as usize + 1];
    let tokens_payload = b"tok";
    let fail_mock = server
        .mock("GET", "/model.int8.onnx")
        .with_status(500)
        .expect_at_least(1)
        .create_async()
        .await;
    let success_mock = server
        .mock("GET", "/model.int8.onnx")
        .with_status(200)
        .with_body(&model_payload)
        .create_async()
        .await;
    let tokens_mock = server
        .mock("GET", "/tokens.txt")
        .with_status(200)
        .with_body(tokens_payload)
        .create_async()
        .await;

    let info = test_model_info(&model_payload, tokens_payload);
    let tmp_dir = tempfile::tempdir().unwrap();

    let result = download_model_with_progress_to(
        &info,
        vec![server.url()],
        tmp_dir.path().to_path_buf(),
        |_d, _t| {},
    )
    .await;

    // mockito 同名 mock 按创建顺序匹配，只要最终成功即可验证重试语义。
    assert!(result.is_ok());
    assert_eq!(
        std::fs::metadata(result.unwrap().join(MAIN_MODEL_FILENAME))
            .unwrap()
            .len(),
        model_payload.len() as u64
    );
    fail_mock.assert_async().await;
    success_mock.assert_async().await;
    tokens_mock.assert_async().await;
}

#[tokio::test]
async fn test_download_skips_when_dest_exists() {
    let model_payload = vec![0; MIN_MODEL_FILE_BYTES as usize];
    let tokens_payload = b"tok";
    let info = test_model_info(&model_payload, tokens_payload);
    let tmp_dir = tempfile::tempdir().unwrap();
    let dest_dir = tmp_dir.path().join("sense-voice");
    std::fs::create_dir_all(&dest_dir).unwrap();
    std::fs::write(dest_dir.join(MAIN_MODEL_FILENAME), &model_payload).unwrap();
    std::fs::write(dest_dir.join(TOKENS_FILENAME), tokens_payload).unwrap();

    // 全部文件已存在：即使下载源不可达也应直接成功
    let result = download_model_with_progress_to(
        &info,
        vec!["http://127.0.0.1:1".to_string()],
        tmp_dir.path().to_path_buf(),
        |_d, _t| {},
    )
    .await;

    assert_eq!(result.unwrap(), dest_dir);
    assert_eq!(
        std::fs::metadata(dest_dir.join(MAIN_MODEL_FILENAME))
            .unwrap()
            .len(),
        MIN_MODEL_FILE_BYTES
    );
}
