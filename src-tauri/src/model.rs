//! 本地识别模型管理模块。
//!
//! 提供本地识别模型（sherpa-onnx）的注册、下载、切换功能。
//! 模型存储在 altgo 配置目录的 `models/<name>/` 子目录下，每个模型
//! 一个目录，内含 `model.int8.onnx` 与 `tokens.txt` 两个文件。

use crate::error::ModelError;
use futures_util::StreamExt;
use reqwest::Client;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::borrow::Cow;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;
use tokio::io::AsyncWriteExt;

/// HF 官方域名与国内镜像域名。各模型的仓库路径记录在 `ModelInfo::repo_path`，
/// 下载 URL = `<域名>/<repo_path>/resolve/main/<文件名>`。
const HF_DOMAINS: &[&str] = &["https://huggingface.co", "https://hf-mirror.com"];

/// 可通过环境变量覆盖下载基址（勿以 `/` 结尾），便于国内等网络环境使用镜像，例如：
/// `ALTGO_MODEL_BASE_URL=https://hf-mirror.com/csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2025-09-09/resolve/main`
const ENV_MODEL_BASE_URL: &str = "ALTGO_MODEL_BASE_URL";

const DOWNLOAD_ATTEMPTS: u32 = 3;

/// 主模型文件最小可接受大小（字节）。小于此值视为下载损坏。
const MIN_MODEL_FILE_BYTES: u64 = 10 * 1024 * 1024;

/// 主模型文件名（其余文件为配套资源）。
const MAIN_MODEL_FILENAME: &str = "model.int8.onnx";
const TOKENS_FILENAME: &str = "tokens.txt";
const MAIN_MODEL_SHA256: &str = "c71f0ce00bec95b07744e116345e33d8cbbe08cef896382cf907bf4b51a2cd51";
const TOKENS_SHA256: &str = "f449eb28dc567533d7fa59be34e2abca8784f771850c78a47fb731a31429a1dc";
const SENSE_VOICE_YUE_SHA256: &str =
    "12ca1a2ae7ecf3e0019ef2822307ee0b5cadc9196569e379b4c4026f8205276d";

/// FireRedASR2 CTC int8（2026-02-25）主模型与词表的 SHA-256。
const FIRE_RED_ASR2_CTC_MODEL_SHA256: &str =
    "ca3dbabd82170110cc0b343c2890866d449984bc9cd92b9a18371ff80a81bb99";
const FIRE_RED_ASR2_CTC_TOKENS_SHA256: &str =
    "1bc613de2112d257e61a349c3e72d1b1a9cf19c33d3ca954197ad2171e5ea07b";

/// 本地推理引擎类型：决定 `SherpaTranscriber` 用哪种识别器配置加载模型目录。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineKind {
    /// SenseVoice：中英日韩粤自动检测，速度快，但句内中英混说偏弱。
    SenseVoice,
    /// FireRedASR2 CTC：中文与英文（含句内混说）识别，普通话加二十多种方言。
    FireRedAsrCtc,
}

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
                // 回退到默认 client，下载仍可能成功，只是设置不够优。
                Client::new()
            })
    })
}

/// 模型内单个文件。
#[derive(Clone)]
pub struct ModelFile {
    pub filename: &'static str,
    /// 近似大小（用于进度条，与 Content-Length 接近即可）。
    pub size_bytes: u64,
    /// 官方发布文件的 SHA-256，用于识别中断下载和损坏缓存。
    pub sha256: Cow<'static, str>,
}

/// 已知模型信息。
pub struct ModelInfo {
    pub name: &'static str,
    /// HF 仓库路径（`<owner>/<repo>`），下载 URL 由 `model_download_bases` 拼接。
    pub repo_path: &'static str,
    pub files: Cow<'static, [ModelFile]>,
    pub description: &'static str,
    /// 该模型使用的本地推理引擎，决定识别器配置的构建方式。
    pub engine: EngineKind,
}

/// SenseVoice int8（2024-07-17）：中/英/日/韩/粤自动检测，CPU 实时率远高于 whisper。
const SENSE_VOICE_FILES: &[ModelFile] = &[
    ModelFile {
        filename: MAIN_MODEL_FILENAME,
        size_bytes: 230 * 1024 * 1024,
        sha256: Cow::Borrowed(MAIN_MODEL_SHA256),
    },
    ModelFile {
        filename: TOKENS_FILENAME,
        size_bytes: 8 * 1024,
        sha256: Cow::Borrowed(TOKENS_SHA256),
    },
];

/// SenseVoice 粤语增强 int8（2025-09-09）：在 WenetSpeech-Yue 大规模粤语语料上继续训练，
/// 语种与词表不变（tokens.txt 与 2024-07-17 相同），粤语识别更准。
const SENSE_VOICE_YUE_FILES: &[ModelFile] = &[
    ModelFile {
        filename: MAIN_MODEL_FILENAME,
        size_bytes: 237_115_547,
        sha256: Cow::Borrowed(SENSE_VOICE_YUE_SHA256),
    },
    ModelFile {
        filename: TOKENS_FILENAME,
        size_bytes: 315_894,
        sha256: Cow::Borrowed(TOKENS_SHA256),
    },
];

/// FireRedASR2 CTC int8（2026-02-25）：中英混说（code-switching）更强，
/// 文件布局与 SenseVoice 相同（model.int8.onnx + tokens.txt），代价是约 740 MB 下载。
const FIRE_RED_ASR2_CTC_FILES: &[ModelFile] = &[
    ModelFile {
        filename: MAIN_MODEL_FILENAME,
        size_bytes: 775_861_420,
        sha256: Cow::Borrowed(FIRE_RED_ASR2_CTC_MODEL_SHA256),
    },
    ModelFile {
        filename: TOKENS_FILENAME,
        size_bytes: 79_172,
        sha256: Cow::Borrowed(FIRE_RED_ASR2_CTC_TOKENS_SHA256),
    },
];

const MODELS: &[ModelInfo] = &[
    ModelInfo {
        name: "sense-voice",
        repo_path: "csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17",
        files: Cow::Borrowed(SENSE_VOICE_FILES),
        description: "SenseVoice（中英日韩粤自动检测，速度快）",
        engine: EngineKind::SenseVoice,
    },
    ModelInfo {
        name: "sense-voice-yue",
        repo_path: "csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2025-09-09",
        files: Cow::Borrowed(SENSE_VOICE_YUE_FILES),
        description: "SenseVoice 粤语增强（2025 新版，粤语更准）",
        engine: EngineKind::SenseVoice,
    },
    ModelInfo {
        name: "fire-red-asr2-ctc",
        repo_path: "csukuangfj2/sherpa-onnx-fire-red-asr2-ctc-zh_en-int8-2026-02-25",
        files: Cow::Borrowed(FIRE_RED_ASR2_CTC_FILES),
        description: "FireRedASR2 CTC（中英混说更准，下载约 740 MB）",
        engine: EngineKind::FireRedAsrCtc,
    },
];

/// 解析配置模型值对应的本地引擎。注册表命中的模型返回注册引擎；
/// 自定义路径（目录或 .onnx 文件）无法从注册表判断，按 SenseVoice 处理
/// （两者的文件布局相同，引擎不匹配只会在加载时报模型错误）。
pub fn engine_for(config_model: &str) -> EngineKind {
    MODELS
        .iter()
        .find(|m| m.name == config_model.trim())
        .map(|m| m.engine)
        .unwrap_or(EngineKind::SenseVoice)
}

pub fn models_info() -> &'static [ModelInfo] {
    MODELS
}

/// 返回模型存储根目录（`~/.config/altgo/models/` 或 `%APPDATA%/altgo/models/`）。
pub fn models_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("altgo")
        .join("models")
}

/// 返回指定模型的目录。
pub fn model_dir(name: &str) -> PathBuf {
    models_dir().join(name)
}

// sha2 0.11 的 digest 输出不再实现 LowerHex，改用显式小写 hex 编码。
fn bytes_to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
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

    Ok(bytes_to_hex(&hasher.finalize()))
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

/// 校验放到阻塞线程池：主模型文件可达 237 MB，同步哈希会占住 current_thread 运行时。
async fn model_file_ready_blocking(file: &ModelFile, path: &Path) -> Result<bool, ModelError> {
    let file = file.clone();
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || model_file_ready(&file, &path))
        .await
        .map_err(|e| ModelError::DownloadFailed(e.to_string()))
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
fn model_files_ready(dir: &Path, files: &[ModelFile]) -> bool {
    model_files_ready_with(dir, files, model_file_ready)
}

/// 自定义模型目录是否包含可供 SenseVoice 加载的文件。
fn custom_model_files_ready(dir: &Path) -> bool {
    model_files_ready_with(dir, SENSE_VOICE_FILES, model_file_structurally_ready)
}

/// 扫描已下载的模型，返回存在的模型名称列表。
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
                if model_files_ready(&path, &m.files) {
                    downloaded.push(name);
                }
            }
        }
    }
    downloaded
}

/// 检查指定模型是否已下载。
pub fn is_downloaded(name: &str) -> bool {
    MODELS
        .iter()
        .find(|m| m.name == name)
        .is_some_and(|m| model_files_ready(&model_dir(name), &m.files))
}

/// 模型列表项（含下载状态），供 IPC 返回给前端。
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
pub fn validate_name(name: &str) -> Result<(), ModelError> {
    if models_info().iter().any(|m| m.name == name) {
        Ok(())
    } else {
        Err(ModelError::UnknownModel(name.to_string()))
    }
}

/// 删除指定根目录下的模型目录。
fn delete_from_root(name: &str, root: &Path) -> Result<(), ModelError> {
    validate_name(name)?;
    let path = root.join(name);
    if path.exists() {
        std::fs::remove_dir_all(path)?;
    }
    Ok(())
}

/// 删除指定模型的本地目录。
pub fn delete(name: &str) -> Result<(), ModelError> {
    delete_from_root(name, &models_dir())
}

/// 解析配置中的模型值，返回模型目录（含 `model.int8.onnx` 与 `tokens.txt`）。
///
/// 如果 `config_model` 是模型名称（如 "sense-voice"），返回已下载的模型目录。
/// 如果是目录路径，直接返回。如果是 `.onnx` 文件路径，返回其父目录。
/// 如果为空或目录不完整，返回 None。
pub fn resolve_model_dir(config_model: &str) -> Option<PathBuf> {
    if config_model.is_empty() {
        return None;
    }

    // 是模型名吗？
    if let Some(m) = MODELS.iter().find(|m| m.name == config_model) {
        let dir = model_dir(config_model);
        if model_files_ready(&dir, &m.files) {
            return Some(dir);
        }
        return None;
    }

    // 是目录路径吗？
    let path = Path::new(config_model);
    if path.is_dir() && custom_model_files_ready(path) {
        return Some(path.to_path_buf());
    }

    // 是直接的 .onnx 文件路径吗？
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
    download_model_files(&info.files, &bases, &dir, total_bytes, &mut on_progress).await?;
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
    if model_file_ready_blocking(file, &dest).await? {
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
            let result =
                match download_once_to_tmp(&url, done_bytes, total_bytes, tmp_path, on_progress)
                    .await
                {
                    Ok(()) => finalize_model_download_blocking(file, tmp_path, dest).await,
                    Err(error) => Err(error),
                };
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
            "下载的模型文件校验失败：{}",
            file.filename
        )));
    }
    std::fs::rename(tmp_path, dest)?;
    Ok(())
}

/// 校验与落盘放阻塞线程池，理由同 `model_file_ready_blocking`。
async fn finalize_model_download_blocking(
    file: &ModelFile,
    tmp_path: &Path,
    dest: &Path,
) -> Result<(), ModelError> {
    let file = file.clone();
    let tmp_path = tmp_path.to_path_buf();
    let dest = dest.to_path_buf();
    tokio::task::spawn_blocking(move || finalize_model_download(&file, &tmp_path, &dest))
        .await
        .map_err(|e| ModelError::DownloadFailed(e.to_string()))?
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
            "下载失败（HTTP {}）：{}\n可尝试设置环境变量 {} 使用镜像基址。",
            response.status(),
            url,
            ENV_MODEL_BASE_URL
        )));
    }

    on_progress(base_done, total);
    let mut file = tokio::fs::File::create(tmp_path).await?;

    let mut downloaded: u64 = 0;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| ModelError::HttpError(format!("读取下载数据失败: {e}")))?;
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;
        on_progress(base_done + downloaded, total);
    }
    // tokio 的 File 无用户态缓冲，flush 不引入新的错误路径，忽略返回值。
    let _ = file.flush().await;

    Ok(())
}

#[cfg(test)]
mod tests;
