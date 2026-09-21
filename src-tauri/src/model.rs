//! SenseVoice 模型管理模块。
//!
//! 提供 SenseVoice（sherpa-onnx）模型的注册、下载、切换功能。
//! 模型存储在 altgo 配置目录的 `models/<name>/` 子目录下，每个模型
//! 一个目录，内含 `model.int8.onnx` 与 `tokens.txt` 两个文件。
//!
//! SenseVoice model management.
//!
//! Registers, downloads, and switches SenseVoice (sherpa-onnx) models. Models live under the
//! altgo config directory at `models/<name>/`, one directory per model containing
//! `model.int8.onnx` and `tokens.txt`.

use crate::error::ModelError;
use futures_util::StreamExt;
use reqwest::Client;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

/// HF 官方域名与国内镜像域名；各模型的仓库路径记录在 `ModelInfo::repo_path`，
/// 下载 URL = `<域名>/<repo_path>/resolve/main/<文件名>`。
///
/// HF official domain and mainland-China mirror; each model's repo path lives in
/// `ModelInfo::repo_path`, and a download URL = `<domain>/<repo_path>/resolve/main/<filename>`.
const HF_DOMAINS: &[&str] = &["https://huggingface.co", "https://hf-mirror.com"];

/// 可通过环境变量覆盖下载基址（勿以 `/` 结尾），便于国内等网络环境使用镜像，例如：
/// `ALTGO_MODEL_BASE_URL=https://hf-mirror.com/csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2025-09-09/resolve/main`
const ENV_MODEL_BASE_URL: &str = "ALTGO_MODEL_BASE_URL";

const DOWNLOAD_ATTEMPTS: u32 = 3;

/// 主模型文件最小可接受大小（字节）。小于此值视为下载损坏。
/// Minimum acceptable size of the main model file in bytes; smaller means a corrupted download.
const MIN_MODEL_FILE_BYTES: u64 = 10 * 1024 * 1024;

/// 主模型文件名（其余文件为配套资源）。
/// Main model file name (remaining files are supporting resources).
const MAIN_MODEL_FILENAME: &str = "model.int8.onnx";
const TOKENS_FILENAME: &str = "tokens.txt";
const MAIN_MODEL_SHA256: &str = "c71f0ce00bec95b07744e116345e33d8cbbe08cef896382cf907bf4b51a2cd51";
const TOKENS_SHA256: &str = "f449eb28dc567533d7fa59be34e2abca8784f771850c78a47fb731a31429a1dc";

/// 粤语增强版（int8-2025-09-09）主模型 SHA-256。
/// SHA-256 of the Cantonese-enhanced (int8-2025-09-09) main model file.
const SENSE_VOICE_YUE_SHA256: &str =
    "12ca1a2ae7ecf3e0019ef2822307ee0b5cadc9196569e379b4c4026f8205276d";

fn model_download_bases(repo_path: &str) -> Vec<String> {
    if let Ok(s) = std::env::var(ENV_MODEL_BASE_URL) {
        let t = s.trim();
        if !t.is_empty() {
            return vec![t.trim_end_matches('/').to_string()];
        }
    }
    HF_DOMAINS
        .iter()
        .map(|d| format!("{d}/{repo_path}/resolve/main"))
        .collect()
}

fn model_download_client() -> &'static Client {
    static CLIENT: OnceLock<Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        Client::builder()
            .user_agent(concat!(
                "altgo/",
                env!("CARGO_PKG_VERSION"),
                " (sherpa-onnx sense-voice model download)"
            ))
            .connect_timeout(Duration::from_secs(120))
            .pool_idle_timeout(Duration::from_secs(600))
            .build()
            .unwrap_or_else(|e| {
                tracing::error!(error = %e, "failed to build model download client");
                // 回退到默认 client——下载仍可能成功，只是设置不够优。
                // Fallback to default client — download may still work with less optimal settings.
                Client::new()
            })
    })
}

/// 模型内单个文件。
/// A single file within a model.
pub struct ModelFile {
    pub filename: &'static str,
    /// 近似大小（用于进度条；与 Content-Length 接近即可）。
    /// Approximate size (for progress display; close to Content-Length is fine).
    pub size_bytes: u64,
    /// 官方发布文件的 SHA-256，用于识别中断下载和损坏缓存。
    /// SHA-256 of the official release file, used to spot interrupted downloads and corrupt caches.
    pub sha256: &'static str,
}

/// 已知模型信息。
/// Known-model metadata.
pub struct ModelInfo {
    pub name: &'static str,
    /// HF 仓库路径（`<owner>/<repo>`），下载 URL 由 `model_download_bases` 拼接。
    /// HF repo path (`<owner>/<repo>`); download URLs are assembled by `model_download_bases`.
    pub repo_path: &'static str,
    pub files: &'static [ModelFile],
    pub description: &'static str,
}

/// SenseVoice int8（2024-07-17）：中/英/日/韩/粤自动检测，CPU 实时率远高于 whisper。
/// SenseVoice int8 (2024-07-17): auto-detects Chinese/English/Japanese/Korean/Cantonese with far
/// better CPU real-time performance than whisper.
const SENSE_VOICE_FILES: &[ModelFile] = &[
    ModelFile {
        filename: MAIN_MODEL_FILENAME,
        size_bytes: 230 * 1024 * 1024,
        sha256: MAIN_MODEL_SHA256,
    },
    ModelFile {
        filename: TOKENS_FILENAME,
        size_bytes: 8 * 1024,
        sha256: TOKENS_SHA256,
    },
];

/// SenseVoice 粤语增强 int8（2025-09-09）：在 WenetSpeech-Yue 大规模粤语语料上继续训练，
/// 语种与词表不变（tokens.txt 与 2024-07-17 相同），粤语识别更准。
/// Cantonese-enhanced SenseVoice int8 (2025-09-09): continued training on the large-scale
/// WenetSpeech-Yue Cantonese corpus; same languages and vocab (tokens.txt identical to
/// 2024-07-17), noticeably better Cantonese accuracy.
const SENSE_VOICE_YUE_FILES: &[ModelFile] = &[
    ModelFile {
        filename: MAIN_MODEL_FILENAME,
        size_bytes: 237_115_547,
        sha256: SENSE_VOICE_YUE_SHA256,
    },
    ModelFile {
        filename: TOKENS_FILENAME,
        size_bytes: 315_894,
        sha256: TOKENS_SHA256,
    },
];

const MODELS: &[ModelInfo] = &[
    ModelInfo {
        name: "sense-voice",
        repo_path: "csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17",
        files: SENSE_VOICE_FILES,
        description: "SenseVoice（中英日韩粤自动检测，速度快）",
    },
    ModelInfo {
        name: "sense-voice-yue",
        repo_path: "csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2025-09-09",
        files: SENSE_VOICE_YUE_FILES,
        description: "SenseVoice 粤语增强（2025 新版，粤语更准）",
    },
];

pub fn models_info() -> &'static [ModelInfo] {
    MODELS
}

/// 返回模型存储根目录（`~/.config/altgo/models/` 或 `%APPDATA%/altgo/models/`）。
/// Returns the model storage root (`~/.config/altgo/models/` or `%APPDATA%/altgo/models/`).
pub fn models_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("altgo")
        .join("models")
}

/// 返回指定模型的目录。
/// Returns the directory of the given model.
pub fn model_dir(name: &str) -> PathBuf {
    models_dir().join(name)
}

fn file_sha256(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 64 * 1024];

    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

fn model_file_structurally_ready(file: &ModelFile, path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    metadata.is_file()
        && ((file.filename == MAIN_MODEL_FILENAME && metadata.len() >= MIN_MODEL_FILE_BYTES)
            || (file.filename != MAIN_MODEL_FILENAME && metadata.len() > 0))
}

fn model_file_ready(file: &ModelFile, path: &Path) -> bool {
    model_file_structurally_ready(file, path)
        && file_sha256(path).is_ok_and(|sha256| sha256 == file.sha256)
}

fn model_files_ready_with<F>(dir: &Path, files: &[ModelFile], is_ready: F) -> bool
where
    F: Fn(&ModelFile, &Path) -> bool,
{
    files
        .iter()
        .all(|file| is_ready(file, &dir.join(file.filename)))
}

/// 指定模型的文件是否齐全且校验和匹配官方发布版本。
/// Whether the given model's files are complete and checksums match the official release.
fn model_files_ready(dir: &Path, files: &[ModelFile]) -> bool {
    model_files_ready_with(dir, files, model_file_ready)
}

/// 自定义模型目录是否包含可供 SenseVoice 加载的文件。
/// Whether a custom model directory holds files loadable by SenseVoice.
fn custom_model_files_ready(dir: &Path) -> bool {
    model_files_ready_with(dir, SENSE_VOICE_FILES, model_file_structurally_ready)
}

/// 扫描已下载的模型，返回存在的模型名称列表。
/// Scans downloaded models, returning the list of names present on disk.
pub fn list_downloaded() -> Vec<String> {
    let dir = models_dir();
    if !dir.exists() {
        return Vec::new();
    }

    let mut downloaded = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Some(name) = path.file_name().map(|n| n.to_string_lossy().to_string()) else {
                continue;
            };
            if let Some(m) = MODELS.iter().find(|m| m.name == name) {
                if model_files_ready(&path, m.files) {
                    downloaded.push(name);
                }
            }
        }
    }
    downloaded
}

/// 检查指定模型是否已下载。
/// Checks whether the given model has been downloaded.
pub fn is_downloaded(name: &str) -> bool {
    MODELS
        .iter()
        .find(|m| m.name == name)
        .is_some_and(|m| model_files_ready(&model_dir(name), m.files))
}

/// 模型列表项（含下载状态），供 IPC 返回给前端。
/// Model list entry (with download status) returned to the frontend over IPC.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelEntry {
    pub name: String,
    pub filename: String,
    pub size_bytes: u64,
    pub description: String,
    pub downloaded: bool,
}

/// 返回所有已知模型及下载状态。
/// Returns every known model plus its download status.
pub fn list_all_with_status() -> Vec<ModelEntry> {
    models_info()
        .iter()
        .map(|m| ModelEntry {
            name: m.name.to_string(),
            filename: m.files[0].filename.to_string(),
            size_bytes: m.files.iter().map(|f| f.size_bytes).sum(),
            description: m.description.to_string(),
            downloaded: is_downloaded(m.name),
        })
        .collect()
}

/// 校验模型名是否在已知模型列表中。
/// Validates that a model name is among the known models.
pub fn validate_name(name: &str) -> Result<(), ModelError> {
    if models_info().iter().any(|m| m.name == name) {
        Ok(())
    } else {
        Err(ModelError::UnknownModel(name.to_string()))
    }
}

/// 删除指定根目录下的模型目录。
/// Removes a model directory under the given root.
fn delete_from_root(name: &str, root: &Path) -> Result<(), ModelError> {
    validate_name(name)?;
    let path = root.join(name);
    if path.exists() {
        std::fs::remove_dir_all(path)?;
    }
    Ok(())
}

/// 删除指定模型的本地目录。
/// Removes the local directory of the given model.
pub fn delete(name: &str) -> Result<(), ModelError> {
    delete_from_root(name, &models_dir())
}

/// 解析配置中的模型值，返回模型目录（含 `model.int8.onnx` 与 `tokens.txt`）。
///
/// 如果 `config_model` 是模型名称（如 "sense-voice"），返回已下载的模型目录。
/// 如果是目录路径，直接返回；如果是 `.onnx` 文件路径，返回其父目录。
/// 如果为空或目录不完整，返回 None。
///
/// Resolves the configured model value into a model directory (containing `model.int8.onnx`
/// and `tokens.txt`).
///
/// If `config_model` is a model name (e.g. "sense-voice"), returns its downloaded directory.
/// Directory paths pass through as-is; `.onnx` file paths resolve to their parent. Empty values
/// or incomplete directories yield `None`.
pub fn resolve_model_dir(config_model: &str) -> Option<PathBuf> {
    if config_model.is_empty() {
        return None;
    }

    // 是模型名吗？
    // Check if it's a model name.
    if let Some(m) = MODELS.iter().find(|m| m.name == config_model) {
        let dir = model_dir(config_model);
        if model_files_ready(&dir, m.files) {
            return Some(dir);
        }
        return None;
    }

    // 是目录路径吗？
    // Check if it's a directory path.
    let path = Path::new(config_model);
    if path.is_dir() && custom_model_files_ready(path) {
        return Some(path.to_path_buf());
    }

    // 是直接的 .onnx 文件路径吗？
    // Check if it's a direct .onnx file path.
    if path.is_file() && path.file_name().is_some_and(|n| n == MAIN_MODEL_FILENAME) {
        if let Some(parent) = path.parent() {
            if custom_model_files_ready(parent) {
                return Some(parent.to_path_buf());
            }
        }
    }

    None
}

/// 下载指定模型（全部文件），通过回调报告进度。
///
/// `on_progress` 参数为 `(downloaded_bytes, total_bytes)`，跨文件累计。
///
/// Downloads all files of the given model, reporting progress via callback.
///
/// The `on_progress` arguments are `(downloaded_bytes, total_bytes)`, accumulated across files.
pub async fn download_with_progress<F>(name: &str, on_progress: F) -> Result<PathBuf, ModelError>
where
    F: FnMut(u64, u64),
{
    let repo_path = MODELS
        .iter()
        .find(|m| m.name == name)
        .map(|m| m.repo_path)
        .ok_or_else(|| ModelError::UnknownModel(name.to_string()))?;
    download_with_progress_to(
        name,
        model_download_bases(repo_path),
        models_dir(),
        on_progress,
    )
    .await
}

async fn download_with_progress_to<F>(
    name: &str,
    bases: Vec<String>,
    root_dir: PathBuf,
    on_progress: F,
) -> Result<PathBuf, ModelError>
where
    F: FnMut(u64, u64),
{
    let info = MODELS
        .iter()
        .find(|m| m.name == name)
        .ok_or_else(|| ModelError::UnknownModel(name.to_string()))?;
    download_model_with_progress_to(info, bases, root_dir, on_progress).await
}

async fn download_model_with_progress_to<F>(
    info: &ModelInfo,
    bases: Vec<String>,
    root_dir: PathBuf,
    mut on_progress: F,
) -> Result<PathBuf, ModelError>
where
    F: FnMut(u64, u64),
{
    let dir = root_dir.join(info.name);
    std::fs::create_dir_all(&dir)?;

    let total_bytes = info.files.iter().map(|file| file.size_bytes).sum();
    download_model_files(info.files, &bases, &dir, total_bytes, &mut on_progress).await?;
    Ok(dir)
}

async fn download_model_files<F>(
    files: &[ModelFile],
    bases: &[String],
    dir: &Path,
    total_bytes: u64,
    on_progress: &mut F,
) -> Result<(), ModelError>
where
    F: FnMut(u64, u64),
{
    let mut done_bytes = 0;
    for file in files {
        done_bytes +=
            download_model_file(file, bases, dir, done_bytes, total_bytes, on_progress).await?;
    }
    Ok(())
}

async fn download_model_file<F>(
    file: &ModelFile,
    bases: &[String],
    dir: &Path,
    done_bytes: u64,
    total_bytes: u64,
    on_progress: &mut F,
) -> Result<u64, ModelError>
where
    F: FnMut(u64, u64),
{
    let dest = dir.join(file.filename);
    if model_file_ready(file, &dest) {
        return Ok(file.size_bytes);
    }
    remove_invalid_model_file(&dest)?;

    let tmp_path = dir.join(format!("{}.tmp", file.filename));
    download_model_file_with_retries(
        file,
        bases,
        &dest,
        &tmp_path,
        done_bytes,
        total_bytes,
        on_progress,
    )
    .await?;
    Ok(file.size_bytes)
}

fn remove_invalid_model_file(path: &Path) -> Result<(), ModelError> {
    if !path.exists() {
        return Ok(());
    }
    if !path.is_file() {
        return Err(ModelError::DownloadFailed(format!(
            "模型资源路径不是文件: {}",
            path.display()
        )));
    }
    std::fs::remove_file(path)?;
    Ok(())
}

async fn download_model_file_with_retries<F>(
    file: &ModelFile,
    bases: &[String],
    dest: &Path,
    tmp_path: &Path,
    done_bytes: u64,
    total_bytes: u64,
    on_progress: &mut F,
) -> Result<(), ModelError>
where
    F: FnMut(u64, u64),
{
    let mut last_err = None;
    for attempt in 0..DOWNLOAD_ATTEMPTS {
        if attempt > 0 {
            let _ = std::fs::remove_file(tmp_path);
            tokio::time::sleep(Duration::from_secs(2 * u64::from(attempt))).await;
        }
        for base in bases {
            let url = format!("{}/{}", base, file.filename);
            let result = download_once_to_tmp(&url, done_bytes, total_bytes, tmp_path, on_progress)
                .await
                .and_then(|()| finalize_model_download(file, tmp_path, dest));
            match result {
                Ok(()) => return Ok(()),
                Err(error) => {
                    last_err = Some(error);
                    let _ = std::fs::remove_file(tmp_path);
                }
            }
        }
    }
    Err(last_err.expect("download attempts must produce an error"))
}

fn finalize_model_download(
    file: &ModelFile,
    tmp_path: &Path,
    dest: &Path,
) -> Result<(), ModelError> {
    if !model_file_ready(file, tmp_path) {
        return Err(ModelError::DownloadFailed(format!(
            "下载的模型文件校验失败: {}",
            file.filename
        )));
    }
    std::fs::rename(tmp_path, dest)?;
    Ok(())
}

async fn download_once_to_tmp<F>(
    url: &str,
    base_done: u64,
    total: u64,
    tmp_path: &Path,
    on_progress: &mut F,
) -> Result<(), ModelError>
where
    F: FnMut(u64, u64),
{
    let response = model_download_client().get(url).send().await.map_err(|e| {
        ModelError::HttpError(format!("无法从 {} 下载（网络或 TLS 错误）: {}", url, e))
    })?;

    if !response.status().is_success() {
        return Err(ModelError::DownloadFailed(format!(
            "下载失败: HTTP {} — {}\n可尝试设置环境变量 {} 使用镜像基址。",
            response.status(),
            url,
            ENV_MODEL_BASE_URL
        )));
    }

    on_progress(base_done, total);
    let mut file_handle = std::fs::File::create(tmp_path)?;

    let mut downloaded: u64 = 0;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| ModelError::HttpError(format!("读取下载数据失败: {e}")))?;
        std::io::Write::write_all(&mut file_handle, &chunk)?;
        downloaded += chunk.len() as u64;
        on_progress(base_done + downloaded, total);
    }

    Ok(())
}

/// 把字节数格式化为人类可读的大小。
/// Format bytes as human-readable size.
#[cfg(test)]
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_model_info(model_payload: &[u8], tokens_payload: &[u8]) -> ModelInfo {
        let model_sha256: &'static str =
            Box::leak(format!("{:x}", Sha256::digest(model_payload)).into_boxed_str());
        let tokens_sha256: &'static str =
            Box::leak(format!("{:x}", Sha256::digest(tokens_payload)).into_boxed_str());
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
            files: Box::leak(files.into_boxed_slice()),
            description: "test model",
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
        // 只有 tokens.txt 没有主模型 → 视为未下载
        // tokens.txt without the main model → treat as not downloaded
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
        let sha256: &'static str =
            Box::leak(format!("{:x}", Sha256::digest(b"valid model")).into_boxed_str());
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
    fn test_list_all_with_status_count() {
        let entries = list_all_with_status();
        assert_eq!(entries.len(), models_info().len());
        assert!(entries.iter().any(|e| e.name == "sense-voice"));
        // 主模型文件名应暴露给前端展示
        // The main model filename should be exposed for frontend display
        assert!(entries.iter().all(|e| e.filename == MAIN_MODEL_FILENAME));
    }

    #[test]
    fn test_model_registry_entries() {
        let names: Vec<_> = models_info().iter().map(|m| m.name).collect();
        assert!(names.contains(&"sense-voice"));
        assert!(names.contains(&"sense-voice-yue"));
        // 模型名必须唯一，否则 models/ 下目录会互相覆盖
        // Model names must be unique, or models/ directories would collide
        let unique: std::collections::HashSet<_> = names.iter().collect();
        assert_eq!(unique.len(), names.len());

        // 每个模型都要有自己的仓库路径，下载基址按它拼接
        // Each model needs its own repo path; download bases are derived from it
        let repos: Vec<_> = models_info().iter().map(|m| m.repo_path).collect();
        assert!(repos.iter().all(|r| !r.is_empty()));
        let unique_repos: std::collections::HashSet<_> = repos.iter().collect();
        assert_eq!(unique_repos.len(), repos.len());

        // 两个模型的主模型校验和不同（tokens 词表相同）；若被"统一"成同一 SHA，
        // 其中一个模型的 is_downloaded 会永远判 false
        // The two models' main-file checksums differ (tokens vocab is shared); unifying them
        // would make one model's is_downloaded forever false
        let sha_of =
            |name: &str| models_info().iter().find(|m| m.name == name).unwrap().files[0].sha256;
        assert_ne!(sha_of("sense-voice"), sha_of("sense-voice-yue"));
    }

    #[test]
    fn test_model_download_bases_per_repo() {
        // 清掉外部覆盖，验证默认的官方 + 镜像双源拼接
        // Drop any external override and verify the default official + mirror pair
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

    #[tokio::test]
    async fn test_download_success_writes_dest_and_reports_progress() {
        let mut server = mockito::Server::new_async().await;
        let model_payload = vec![0u8; MIN_MODEL_FILE_BYTES as usize + 1];
        let tokens_payload = b"token list";
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
            .with_body(tokens_payload)
            .create_async()
            .await;

        let tmp_dir = tempfile::tempdir().unwrap();
        let dest_dir = tmp_dir.path().join("sense-voice");

        let info = test_model_info(&model_payload, tokens_payload);
        let progress_calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let calls = progress_calls.clone();
        let result = download_model_with_progress_to(
            &info,
            vec![server.url()],
            tmp_dir.path().to_path_buf(),
            move |d, t| calls.lock().unwrap().push((d, t)),
        )
        .await;

        let path = result.unwrap();
        assert_eq!(path, dest_dir);
        assert!(dest_dir.join(MAIN_MODEL_FILENAME).exists());
        assert_eq!(
            std::fs::metadata(dest_dir.join(MAIN_MODEL_FILENAME))
                .unwrap()
                .len(),
            model_payload.len() as u64
        );
        assert_eq!(
            std::fs::read(dest_dir.join("tokens.txt")).unwrap(),
            tokens_payload
        );
        assert!(!dest_dir.join("model.int8.onnx.tmp").exists());
        {
            let calls = progress_calls.lock().unwrap();
            assert!(!calls.is_empty());
            // total 恒为声明总大小；进度按实际下载字节累计
            // total stays the declared total; progress accumulates actual downloaded bytes
            let total = calls.last().unwrap().1;
            assert_eq!(
                total,
                info.files.iter().map(|file| file.size_bytes).sum::<u64>()
            );
            assert!(calls.iter().any(|(d, _)| *d == model_payload.len() as u64));
        }
        model_mock.assert_async().await;
        tokens_mock.assert_async().await;
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
        // mockito matches same-named mocks in creation order; eventual success is enough to
        // verify retry semantics.
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
        // All files already exist: must succeed even when the download source is unreachable
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
}
