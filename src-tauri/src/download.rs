//! 带校验的文件下载内核。
//!
//! 本地识别模型与润色运行时共用这套取文件的规则：基址列表逐个尝试，失败按次数重试
//! 并退避，内容流式写入临时文件，SHA-256 与大小下限都通过后才改名落盘。校验不过或
//! 中断留下的临时文件在下次尝试前删除，损坏的旧文件在下载前删除。

use crate::error::DownloadError;
use futures_util::StreamExt;
use reqwest::Client;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::Duration;
use tokio::io::AsyncWriteExt;

/// 单次下载允许的尝试次数，每次尝试会遍历全部基址。
pub const DOWNLOAD_ATTEMPTS: u32 = 3;

/// 重试退避基数（秒），第 n 次重试前等待该值的 n 倍。
const RETRY_BACKOFF_SECS: u64 = 2;

/// 待下载文件：远端文件名、期望字节数、官方 SHA-256。
#[derive(Debug, Clone, Copy)]
pub struct RemoteFile<'a> {
    pub filename: &'a str,
    /// 期望字节数，用于进度条总量与调用方的结构检查。
    pub size_bytes: u64,
    /// 官方发布文件的 SHA-256，用于识别中断下载与损坏缓存。
    pub sha256: &'a str,
}

/// 共享 HTTP 客户端（连接超时与连接池对多文件下载影响明显）。
pub fn client() -> &'static Client {
    static CLIENT: LazyLock<Client> = LazyLock::new(|| {
        Client::builder()
            .user_agent(concat!(
                "altgo/",
                env!("CARGO_PKG_VERSION"),
                " (offline model download)"
            ))
            .connect_timeout(Duration::from_secs(120))
            .pool_idle_timeout(Duration::from_secs(600))
            .build()
            .unwrap_or_else(|e| {
                tracing::error!(error = %e, "failed to build download client");
                // 回退到默认 client，下载仍可能成功，只是设置不够优。
                Client::new()
            })
    });
    &CLIENT
}

/// 字节串转小写十六进制。
pub fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// 计算文件 SHA-256，返回小写十六进制。
pub fn file_sha256(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        use std::io::Read;
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(bytes_to_hex(&hasher.finalize()))
}

/// 计算 SHA-256 放到阻塞线程池：文件可达数百 MB，同步哈希会占住 current_thread 运行时。
pub async fn file_sha256_blocking(path: PathBuf) -> Result<String, DownloadError> {
    tokio::task::spawn_blocking(move || file_sha256(&path))
        .await
        .map_err(|e| DownloadError::TaskFailed(e.to_string()))?
        .map_err(DownloadError::Io)
}

/// 文件是否就位且不小于 `min_bytes`。
pub fn structurally_ready(path: &Path, min_bytes: u64) -> bool {
    match std::fs::metadata(path) {
        Ok(metadata) => metadata.is_file() && metadata.len() >= min_bytes,
        Err(_) => false,
    }
}

/// 删除已存在但无效的目标文件，供下载前清理。
pub fn remove_invalid(path: &Path) -> Result<(), DownloadError> {
    if !path.exists() {
        return Ok(());
    }
    if !path.is_file() {
        return Err(DownloadError::NotAFile(path.display().to_string()));
    }
    std::fs::remove_file(path)?;
    Ok(())
}

/// 一次取文件的参数。
pub struct FetchRequest<'a> {
    pub file: RemoteFile<'a>,
    /// 落盘路径，临时文件与它同目录。
    pub dest: PathBuf,
    /// 落盘前要求的最小字节数。
    pub min_bytes: u64,
    /// 进度起点（多文件下载时前面文件已完成的字节数）。
    pub progress_base: u64,
    /// 进度总量。
    pub progress_total: u64,
    /// 允许的尝试次数，每次尝试会遍历全部基址。
    pub attempts: u32,
}

/// 下载并校验一个文件。
///
/// `bases` 是基址列表（按优先级），最终 URL 为 `<基址>/<文件名>`。进度以
/// `progress_base + 本次已写入字节` 对 `progress_total` 回调，便于多文件共用一根进度条。
pub async fn fetch_file<F>(
    bases: &[String],
    request: &FetchRequest<'_>,
    on_progress: &mut F,
) -> Result<(), DownloadError>
where
    F: FnMut(u64, u64),
{
    let file = request.file;
    let dest = request.dest.as_path();
    let tmp = temp_path_for(dest);
    let mut last_err = None;
    for attempt in 0..request.attempts {
        if attempt > 0 {
            let _ = std::fs::remove_file(&tmp);
            tokio::time::sleep(Duration::from_secs(RETRY_BACKOFF_SECS * u64::from(attempt))).await;
        }
        for base in bases {
            let url = format!("{}/{}", base, file.filename);
            let result = match download_to_tmp(
                &url,
                request.progress_base,
                request.progress_total,
                &tmp,
                on_progress,
            )
            .await
            {
                Ok(()) => verify_and_place(file, request.min_bytes, &tmp, dest).await,
                Err(error) => Err(error),
            };
            match result {
                Ok(()) => return Ok(()),
                Err(error) => {
                    last_err = Some(error);
                    let _ = std::fs::remove_file(&tmp);
                }
            }
        }
    }
    Err(last_err.expect("download attempts must produce an error"))
}

/// 目标文件对应的临时文件路径，与目标同目录以保证改名是同一文件系统内的原子操作。
pub fn temp_path_for(dest: &Path) -> PathBuf {
    match dest.file_name() {
        Some(name) => dest.with_file_name(format!("{}.tmp", name.to_string_lossy())),
        None => dest.with_extension("tmp"),
    }
}

async fn download_to_tmp<F>(
    url: &str,
    progress_base: u64,
    progress_total: u64,
    tmp_path: &Path,
    on_progress: &mut F,
) -> Result<(), DownloadError>
where
    F: FnMut(u64, u64),
{
    let response = client()
        .get(url)
        .send()
        .await
        .map_err(|e| DownloadError::Request {
            url: url.to_string(),
            detail: e.to_string(),
        })?;

    if !response.status().is_success() {
        return Err(DownloadError::Status {
            status: response.status().as_u16(),
            url: url.to_string(),
        });
    }

    on_progress(progress_base, progress_total);
    let mut file = tokio::fs::File::create(tmp_path).await?;

    let mut downloaded: u64 = 0;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| DownloadError::Stream(e.to_string()))?;
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;
        on_progress(progress_base + downloaded, progress_total);
    }
    // tokio 的 File 无用户态缓冲，flush 不引入新的错误路径，忽略返回值。
    let _ = file.flush().await;

    Ok(())
}

async fn verify_and_place(
    file: RemoteFile<'_>,
    min_bytes: u64,
    tmp_path: &Path,
    dest: &Path,
) -> Result<(), DownloadError> {
    if !structurally_ready(tmp_path, min_bytes) {
        return Err(DownloadError::Corrupt(file.filename.to_string()));
    }
    let sha256 = file_sha256_blocking(tmp_path.to_path_buf()).await?;
    if sha256 != file.sha256 {
        return Err(DownloadError::Corrupt(file.filename.to_string()));
    }
    std::fs::rename(tmp_path, dest)?;
    Ok(())
}
