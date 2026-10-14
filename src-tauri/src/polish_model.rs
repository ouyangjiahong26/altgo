//! 本地润色模型（GGUF）的注册、下载与路径解析。
//!
//! 与转写模型分开：清单结构、目录与下载基址都是独立的一套，只复用下载内核
//! （`crate::download`）。模型落在配置目录的 `polish-models/<name>/` 下，不随安装包分发。

use crate::download::{self, RemoteFile};
use crate::error::{DownloadError, ModelError};
use std::borrow::Cow;
use std::path::{Path, PathBuf};

/// HF 官方域名与国内镜像，下载 URL 为
/// `<域名>/<仓库路径>/resolve/main/<文件名>`。
const HF_DOMAINS: &[&str] = &["https://huggingface.co", "https://hf-mirror.com"];

/// 覆盖下载基址的环境变量（勿以 `/` 结尾），便于网络不畅时指向镜像或自建托管。
const ENV_POLISH_MODEL_BASE_URL: &str = "ALTGO_POLISH_MODEL_BASE_URL";

/// 模型文件最小可接受大小，小于此值视为下载损坏。
const MIN_MODEL_BYTES: u64 = 10 * 1024 * 1024;

/// 已知的本地润色模型。
pub struct PolishModelInfo {
    /// 配置里 `[polisher] local_model` 用的名字。
    pub name: &'static str,
    /// HF 仓库路径（`<owner>/<repo>`）。
    pub repo_path: &'static str,
    /// 设置页展示用的说明。
    pub description: &'static str,
    /// 权重文件名。
    pub filename: &'static str,
    pub size_bytes: u64,
    /// 官方发布文件的 SHA-256。
    pub sha256: Cow<'static, str>,
}

/// 默认档：CPU 上几十字的句子约 0.3 秒，常驻内存约 1 GB。
static QWEN3_06B: PolishModelInfo = PolishModelInfo {
    name: "qwen3-0.6b",
    repo_path: "unsloth/Qwen3-0.6B-GGUF",
    description: "默认档，速度最快，约 1 GB 内存。",
    filename: "Qwen3-0.6B-Q4_K_M.gguf",
    size_bytes: 396_705_472,
    sha256: Cow::Borrowed("ac2d97712095a558e31573f62f466a3f9d93990898b0ec79d7c974c1780d524a"),
};

/// 质量档：输出更稳，速度约为默认档的一半，常驻内存约 2.5 GB。
static QWEN3_17B: PolishModelInfo = PolishModelInfo {
    name: "qwen3-1.7b",
    repo_path: "unsloth/Qwen3-1.7B-GGUF",
    description: "质量更高，速度约为默认档的一半，约 2.5 GB 内存。",
    filename: "Qwen3-1.7B-Q4_K_M.gguf",
    size_bytes: 1_107_409_472,
    sha256: Cow::Borrowed("b139949c5bd74937ad8ed8c8cf3d9ffb1e99c866c823204dc42c0d91fa181897"),
};

/// 全部可选模型，第一项是默认档。
pub static MODELS: [&PolishModelInfo; 2] = [&QWEN3_06B, &QWEN3_17B];

/// 设置页与 IPC 用的模型摘要。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PolishModelSummary {
    pub name: String,
    pub description: String,
    pub size_bytes: u64,
    pub downloaded: bool,
}

/// 全部模型及其下载状态。
pub fn models_info() -> Vec<PolishModelSummary> {
    MODELS
        .iter()
        .map(|info| PolishModelSummary {
            name: info.name.to_string(),
            description: info.description.to_string(),
            size_bytes: info.size_bytes,
            downloaded: is_downloaded(info.name),
        })
        .collect()
}

/// 删除已下载的模型文件。
///
/// 只接受内置清单里的名字。这个函数会递归删除目录，而名字可能来自 IPC 参数，
/// 未经校验的绝对路径或 `..` 会删到模型目录之外。
pub fn delete(name: &str) -> Result<(), ModelError> {
    if !is_known(name) {
        return Err(ModelError::UnknownModel(name.to_string()));
    }
    remove_dir_if_exists(&model_dir(name))
}

/// 目录存在则整棵删除。
fn remove_dir_if_exists(dir: &Path) -> Result<(), ModelError> {
    if dir.exists() {
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

/// 返回指定名字的清单条目。
pub fn find(name: &str) -> Option<&'static PolishModelInfo> {
    MODELS.iter().copied().find(|m| m.name == name)
}

/// 名字是否是清单里的已知模型。
pub fn is_known(name: &str) -> bool {
    find(name.trim()).is_some()
}

/// 存储根目录：`<配置目录>/altgo/polish-models/`。
pub fn models_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("altgo")
        .join("polish-models")
}

/// 指定模型的目录。
pub fn model_dir(name: &str) -> PathBuf {
    models_dir().join(name)
}

/// 指定模型是否已下载（只按文件就位与大小下限判断，不算校验和）。
pub fn is_downloaded(name: &str) -> bool {
    find(name).is_some_and(|info| download::structurally_ready(&model_path(info), MIN_MODEL_BYTES))
}

/// 解析 `[polisher] local_model`：已知名字走模型目录，否则当作自定义 GGUF 文件路径。
pub fn resolve_model_path(name: &str) -> Option<PathBuf> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(info) = find(trimmed) {
        let path = model_path(info);
        return download::structurally_ready(&path, MIN_MODEL_BYTES).then_some(path);
    }
    let path = PathBuf::from(trimmed);
    path.is_file().then_some(path)
}

/// 下载指定模型，进度为 `(已下载字节, 总字节)`。
pub async fn download_with_progress<F>(name: &str, on_progress: F) -> Result<PathBuf, ModelError>
where
    F: FnMut(u64, u64),
{
    let info = find(name).ok_or_else(|| ModelError::UnknownModel(name.to_string()))?;
    download_with_progress_to(
        info,
        download_bases(info.repo_path),
        models_dir(),
        on_progress,
    )
    .await
}

/// 下载内核的可注入版本：基址与根目录由调用方给出，供测试与镜像场景使用。
pub(crate) async fn download_with_progress_to<F>(
    info: &PolishModelInfo,
    bases: Vec<String>,
    root_dir: PathBuf,
    mut on_progress: F,
) -> Result<PathBuf, ModelError>
where
    F: FnMut(u64, u64),
{
    let dir = root_dir.join(info.name);
    std::fs::create_dir_all(&dir)?;
    let dest = dir.join(info.filename);
    if download::structurally_ready(&dest, MIN_MODEL_BYTES) {
        on_progress(info.size_bytes, info.size_bytes);
        return Ok(dest);
    }
    download::remove_invalid(&dest).map_err(to_model_error)?;

    download::fetch_file(
        &bases,
        &download::FetchRequest {
            file: RemoteFile {
                filename: info.filename,
                size_bytes: info.size_bytes,
                sha256: &info.sha256,
            },
            dest: dest.clone(),
            min_bytes: MIN_MODEL_BYTES,
            progress_base: 0,
            progress_total: info.size_bytes,
            attempts: download::DOWNLOAD_ATTEMPTS,
        },
        &mut on_progress,
    )
    .await
    .map_err(to_model_error)?;
    Ok(dest)
}

fn model_path(info: &PolishModelInfo) -> PathBuf {
    model_dir(info.name).join(info.filename)
}

fn download_bases(repo_path: &str) -> Vec<String> {
    if let Ok(base) = std::env::var(ENV_POLISH_MODEL_BASE_URL) {
        let trimmed = base.trim();
        if !trimmed.is_empty() {
            return vec![trimmed.trim_end_matches('/').to_string()];
        }
    }
    HF_DOMAINS
        .iter()
        .map(|domain| format!("{domain}/{repo_path}/resolve/main"))
        .collect()
}

/// 下载内核的错误映射成模型域的用户可见文案。
fn to_model_error(error: DownloadError) -> ModelError {
    match error {
        DownloadError::Io(e) => ModelError::Io(e),
        DownloadError::Request { url, detail } => ModelError::HttpError(format!(
            "无法从 {} 下载（网络或 TLS 错误）: {}",
            url, detail
        )),
        DownloadError::Status { status, url } => ModelError::DownloadFailed(format!(
            "下载失败（HTTP {}）：{}\n可尝试设置环境变量 {} 使用镜像基址。",
            status, url, ENV_POLISH_MODEL_BASE_URL
        )),
        DownloadError::Stream(detail) => {
            ModelError::HttpError(format!("读取下载数据失败: {}", detail))
        }
        DownloadError::Corrupt(filename) => {
            ModelError::DownloadFailed(format!("下载的润色模型校验失败：{}", filename))
        }
        DownloadError::NotAFile(path) => {
            ModelError::DownloadFailed(format!("润色模型路径不是文件: {}", path))
        }
        DownloadError::TaskFailed(detail) => ModelError::DownloadFailed(detail),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    #[test]
    fn test_registry_entries_are_pinned() {
        assert!(MODELS.len() >= 2);
        for info in MODELS {
            assert!(info.size_bytes > MIN_MODEL_BYTES, "{}", info.name);
            assert_eq!(info.sha256.len(), 64, "{}", info.name);
            assert!(info.filename.ends_with(".gguf"), "{}", info.name);
            assert!(info.repo_path.contains('/'), "{}", info.name);
        }
        assert_eq!(MODELS[0].name, "qwen3-0.6b", "第一项应当是默认档");
    }

    #[test]
    fn test_default_bases_prefer_official_then_mirror() {
        std::env::remove_var(ENV_POLISH_MODEL_BASE_URL);
        let bases = download_bases("unsloth/Qwen3-0.6B-GGUF");
        assert_eq!(bases.len(), 2);
        assert!(bases[0].starts_with("https://huggingface.co/"));
        assert!(bases[1].starts_with("https://hf-mirror.com/"));
    }

    #[test]
    fn test_resolve_model_path_handles_known_missing_custom_and_empty() {
        let dir = tempfile::tempdir().unwrap();
        let custom = dir.path().join("my-polish.gguf");
        std::fs::write(&custom, vec![0u8; 8]).unwrap();

        assert_eq!(
            resolve_model_path(custom.to_str().unwrap()),
            Some(custom.clone()),
            "自定义路径只要文件在就接受"
        );
        assert!(
            resolve_model_path("qwen3-0.6b").is_none(),
            "未下载时解析不到"
        );
        assert!(resolve_model_path("").is_none());
        assert!(resolve_model_path("不存在.gguf").is_none());
    }

    #[test]
    fn test_is_known_rejects_unknown_names() {
        assert!(is_known("qwen3-0.6b"));
        assert!(is_known(" qwen3-1.7b "));
        assert!(!is_known("ernie-0.3b"));
    }

    #[test]
    fn test_models_info_mirrors_registry() {
        let info = models_info();
        assert_eq!(info.len(), MODELS.len());
        for (entry, spec) in info.iter().zip(MODELS.iter()) {
            assert_eq!(entry.name, spec.name);
            assert_eq!(entry.size_bytes, spec.size_bytes);
            assert!(!entry.description.is_empty());
        }
    }

    #[test]
    fn test_delete_clears_model_directory() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("qwen3-0.6b");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("weights.gguf"), b"x").unwrap();

        remove_dir_if_exists(&target).unwrap();
        assert!(!target.exists());
        remove_dir_if_exists(&target).unwrap();
    }

    /// 递归删除只认清单里的名字，路径越界与未知名字都要被拒且不动文件系统。
    #[test]
    fn test_delete_rejects_names_outside_the_catalog() {
        let dir = tempfile::tempdir().unwrap();
        let victim = dir.path().join("victim");
        std::fs::create_dir_all(&victim).unwrap();
        std::fs::write(victim.join("keep.txt"), b"x").unwrap();

        for name in [
            "..",
            "../..",
            "qwen3-0.6b/../../victim",
            victim.to_str().unwrap(),
            "ernie-0.3b",
        ] {
            let error = delete(name).unwrap_err();
            assert!(
                matches!(error, ModelError::UnknownModel(_)),
                "「{name}」应被拒，实际：{error:?}"
            );
            assert!(victim.exists(), "被拒的名字不该动文件系统");
        }
    }

    /// 清单里的哈希与大小要与真实产物一致。本机存在该文件时当场复算，没有就跳过
    /// （CI 不下 400 MB 权重），断言语义与仓库里其它依赖外部工具的平台接缝测试一致。
    #[test]
    fn test_pinned_hashes_match_local_artifacts_when_present() {
        let Ok(dir) = std::env::var("ALTGO_POLISH_MODEL_DIR") else {
            return;
        };
        let mut checked = 0;
        for info in MODELS {
            let path = PathBuf::from(&dir).join(info.filename);
            if !path.is_file() {
                continue;
            }
            let size = std::fs::metadata(&path).unwrap().len();
            assert_eq!(
                size, info.size_bytes,
                "{} 的清单大小与真实文件不符",
                info.name
            );
            let sha256 = download::file_sha256(&path).unwrap();
            assert_eq!(
                sha256, info.sha256,
                "{} 的清单哈希与真实文件不符",
                info.name
            );
            checked += 1;
        }
        println!("复算了 {checked} 个本地产物");
    }

    #[tokio::test]
    async fn test_download_lands_verified_file_and_skips_next_time() {
        let payload = vec![9u8; MIN_MODEL_BYTES as usize + 1];
        let sha256 = download::bytes_to_hex(&Sha256::digest(&payload));
        let info = PolishModelInfo {
            name: "test-model",
            repo_path: "owner/repo",
            description: "test",
            filename: "test.gguf",
            size_bytes: payload.len() as u64,
            sha256: Cow::Owned(sha256),
        };

        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", format!("/{}", info.filename).as_str())
            .with_body(payload.clone())
            .expect(1)
            .create_async()
            .await;

        let root = tempfile::tempdir().unwrap();
        let mut progress = Vec::new();
        let path = download_with_progress_to(
            &info,
            vec![server.url()],
            root.path().to_path_buf(),
            |done, total| progress.push((done, total)),
        )
        .await
        .unwrap();

        assert_eq!(std::fs::read(&path).unwrap(), payload);
        assert_eq!(
            progress.last(),
            Some(&(payload.len() as u64, payload.len() as u64))
        );

        // 第二次调用不再下载：mock 只允许一次请求。
        let again = download_with_progress_to(
            &info,
            vec![server.url()],
            root.path().to_path_buf(),
            |_, _| {},
        )
        .await
        .unwrap();
        assert_eq!(again, path);
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_download_rejects_content_that_does_not_match_checksum() {
        let payload = vec![3u8; MIN_MODEL_BYTES as usize + 1];
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", format!("/{}", QWEN3_06B.filename).as_str())
            .with_body(payload)
            .create_async()
            .await;

        let root = tempfile::tempdir().unwrap();
        let error = download_with_progress_to(
            &QWEN3_06B,
            vec![server.url()],
            root.path().to_path_buf(),
            |_, _| {},
        )
        .await
        .unwrap_err();

        assert!(error.to_string().contains("校验失败"), "{error}");
        assert!(!root
            .path()
            .join(QWEN3_06B.name)
            .join(QWEN3_06B.filename)
            .exists());
    }

    #[tokio::test]
    async fn test_download_reports_unknown_model() {
        let error = download_with_progress("no-such-model", |_, _| {})
            .await
            .unwrap_err();
        assert!(matches!(error, ModelError::UnknownModel(_)));
    }
}
