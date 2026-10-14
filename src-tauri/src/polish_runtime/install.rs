//! 润色运行时的获取：下载官方产物、校验、按保留规则解包。
//!
//! 下载内核与模型共用（`crate::download`）。压缩包留在运行时目录里直到解包成功，因此
//! 解包失败重试时不必重新下载；校验和固定，手动放置的文件也能被同一套结构检查接受。

use super::manifest::RuntimeSpec;
use crate::download::{self, RemoteFile};
use crate::error::{DownloadError, PolishRuntimeError};
use std::fs::File;
#[cfg(unix)]
use std::io::BufReader;
use std::path::{Component, Path, PathBuf};

/// 运行时文件是否齐备，手动放置的目录同样算数。
pub fn is_installed(spec: &RuntimeSpec, dir: &Path) -> bool {
    if !download::structurally_ready(&dir.join(spec.executable), 1) {
        return false;
    }
    let libraries_ready = spec
        .required_libraries
        .iter()
        .all(|library| download::structurally_ready(&dir.join(library), 1));
    libraries_ready && has_cpu_library(spec, dir)
}

/// 至少要有一个非空的 CPU 后端库，否则服务启动时会缺后端。
fn has_cpu_library(spec: &RuntimeSpec, dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let name = entry.file_name().to_string_lossy().into_owned();
        name.starts_with(spec.cpu_lib_prefix)
            && name.ends_with(spec.shared_lib_suffix)
            && download::structurally_ready(&entry.path(), 1)
    })
}

/// 下载并解包运行时，进度按压缩包字节报告。
pub async fn install<F>(
    spec: &RuntimeSpec,
    bases: &[String],
    dir: &Path,
    mut on_progress: F,
) -> Result<(), PolishRuntimeError>
where
    F: FnMut(u64, u64),
{
    if is_installed(spec, dir) {
        on_progress(spec.archive_size, spec.archive_size);
        return Ok(());
    }
    std::fs::create_dir_all(dir)?;

    let archive = dir.join(spec.archive_name);
    if !archive_matches(spec, &archive).await {
        download::fetch_file(
            bases,
            &download::FetchRequest {
                file: RemoteFile {
                    filename: spec.archive_name,
                    size_bytes: spec.archive_size,
                    sha256: &spec.archive_sha256,
                },
                dest: archive.clone(),
                min_bytes: spec.archive_size,
                progress_base: 0,
                progress_total: spec.archive_size,
                attempts: download::DOWNLOAD_ATTEMPTS,
            },
            &mut on_progress,
        )
        .await
        .map_err(to_runtime_error)?;
    }

    extract(spec, &archive, dir)?;
    let _ = std::fs::remove_file(&archive);
    tracing::info!(dir = %dir.display(), "polish runtime installed");
    Ok(())
}

/// 删除整个运行时目录，供“重新下载运行时”使用。
pub fn remove(dir: &Path) -> Result<(), PolishRuntimeError> {
    if dir.exists() {
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

/// 已落盘的压缩包是否就是清单里那一份。
async fn archive_matches(spec: &RuntimeSpec, archive: &Path) -> bool {
    if !download::structurally_ready(archive, spec.archive_size) {
        return false;
    }
    download::file_sha256_blocking(archive.to_path_buf())
        .await
        .is_ok_and(|sha256| sha256 == spec.archive_sha256)
}

/// 压缩包条目在运行时目录里的相对路径。越界条目、目录条目与不保留的文件都返回 `None`。
///
/// 越界判定不看压缩包工具的说法，自己拒绝绝对路径、`..` 与盘符前缀，避免解包写穿目录。
fn kept_relative_path(spec: &RuntimeSpec, entry_name: &str) -> Option<PathBuf> {
    let normalized = entry_name.replace('\\', "/");
    let without_root = match spec.archive_root {
        "" => normalized.as_str(),
        root => normalized.strip_prefix(root)?.strip_prefix('/')?,
    };
    if without_root.is_empty() {
        return None;
    }

    let path = Path::new(without_root);
    let safe = path
        .components()
        .all(|component| matches!(component, Component::Normal(_)));
    if !safe {
        return None;
    }

    let name = path.file_name()?.to_string_lossy().into_owned();
    spec.keeps(&name).then(|| path.to_path_buf())
}

#[cfg(windows)]
fn extract(spec: &RuntimeSpec, archive: &Path, dir: &Path) -> Result<(), PolishRuntimeError> {
    let file = File::open(archive)?;
    let mut zip =
        zip::ZipArchive::new(file).map_err(|e| PolishRuntimeError::ExtractFailed(e.to_string()))?;
    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|e| PolishRuntimeError::ExtractFailed(e.to_string()))?;
        if entry.is_dir() {
            continue;
        }
        let Some(relative) = kept_relative_path(spec, entry.name()) else {
            continue;
        };
        let dest = dir.join(&relative);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out = File::create(&dest)?;
        std::io::copy(&mut entry, &mut out)?;
    }
    Ok(())
}

#[cfg(unix)]
fn extract(spec: &RuntimeSpec, archive: &Path, dir: &Path) -> Result<(), PolishRuntimeError> {
    let file = BufReader::new(File::open(archive)?);
    let decoder = flate2::read::GzDecoder::new(file);
    let mut tar = tar::Archive::new(decoder);
    let entries = tar
        .entries()
        .map_err(|e| PolishRuntimeError::ExtractFailed(e.to_string()))?;
    for entry in entries {
        let mut entry = entry.map_err(|e| PolishRuntimeError::ExtractFailed(e.to_string()))?;
        let path = entry
            .path()
            .map_err(|e| PolishRuntimeError::ExtractFailed(e.to_string()))?
            .into_owned();
        let Some(relative) = kept_relative_path(spec, &path.to_string_lossy()) else {
            continue;
        };
        let dest = dir.join(&relative);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // unpack 会按 tar 头里的模式位落地，符号链接也在这里被还原。
        entry
            .unpack(&dest)
            .map_err(|e| PolishRuntimeError::ExtractFailed(e.to_string()))?;
    }
    Ok(())
}

#[cfg(not(any(windows, unix)))]
fn extract(_spec: &RuntimeSpec, _archive: &Path, _dir: &Path) -> Result<(), PolishRuntimeError> {
    Err(PolishRuntimeError::UnsupportedPlatform(
        std::env::consts::OS.to_string(),
    ))
}

/// 下载内核的错误映射成运行时域的用户可见文案。
fn to_runtime_error(error: DownloadError) -> PolishRuntimeError {
    match error {
        DownloadError::Io(e) => PolishRuntimeError::Io(e),
        DownloadError::Request { url, detail } => PolishRuntimeError::Download(format!(
            "无法从 {} 下载（网络或 TLS 错误）: {}",
            url, detail
        )),
        DownloadError::Status { status, url } => PolishRuntimeError::Download(format!(
            "下载失败（HTTP {}）：{}\n可尝试设置环境变量 {} 使用镜像基址。",
            status,
            url,
            super::ENV_RUNTIME_BASE_URL
        )),
        DownloadError::Stream(detail) => {
            PolishRuntimeError::Download(format!("读取下载数据失败: {}", detail))
        }
        DownloadError::Corrupt(filename) => {
            PolishRuntimeError::Download(format!("下载的运行时文件校验失败：{}", filename))
        }
        DownloadError::NotAFile(path) => {
            PolishRuntimeError::Download(format!("运行时资源路径不是文件: {}", path))
        }
        DownloadError::TaskFailed(detail) => PolishRuntimeError::Download(detail),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;
    use std::io::Write;

    #[cfg(windows)]
    const EXE: &str = "llama-server.exe";
    #[cfg(unix)]
    const EXE: &str = "llama-server";
    #[cfg(windows)]
    const CORE: &str = "llama-server-impl.dll";
    #[cfg(unix)]
    const CORE: &str = "libllama-server-impl.so";
    #[cfg(windows)]
    const CPU: &str = "ggml-cpu-x64.dll";
    #[cfg(unix)]
    const CPU: &str = "libggml-cpu-x64.so";
    #[cfg(windows)]
    const SUFFIX: &str = ".dll";
    #[cfg(unix)]
    const SUFFIX: &str = ".so";
    #[cfg(windows)]
    const CPU_PREFIX: &str = "ggml-cpu";
    #[cfg(unix)]
    const CPU_PREFIX: &str = "libggml-cpu";
    #[cfg(windows)]
    const REQUIRED: &[&str] = &["llama-server-impl.dll"];
    #[cfg(unix)]
    const REQUIRED: &[&str] = &["libllama-server-impl.so"];

    const ARCHIVE_NAME: &str = "runtime.zip";
    const UNKEPT_TOOL: &str = "llama-cli";

    fn spec_with(sha256: &str, size: u64, root: &'static str) -> RuntimeSpec {
        RuntimeSpec {
            target: "test",
            archive_name: ARCHIVE_NAME,
            archive_size: size,
            archive_sha256: Cow::Owned(sha256.to_string()),
            archive_root: root,
            executable: EXE,
            shared_lib_suffix: SUFFIX,
            required_libraries: REQUIRED,
            cpu_lib_prefix: CPU_PREFIX,
        }
    }

    /// 造一个与真实产物同构的最小压缩包。
    #[cfg(windows)]
    fn build_archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut cursor = std::io::Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut cursor);
            for (name, data) in entries {
                writer
                    .start_file(*name, zip::write::SimpleFileOptions::default())
                    .unwrap();
                writer.write_all(data).unwrap();
            }
            writer.finish().unwrap();
        }
        cursor.into_inner()
    }

    #[cfg(unix)]
    fn build_archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let encoder = flate2::write::GzEncoder::new(&mut out, flate2::Compression::default());
            let mut builder = tar::Builder::new(encoder);
            for (name, data) in entries {
                let mut header = tar::Header::new_gnu();
                header.set_size(data.len() as u64);
                header.set_mode(0o755);
                header.set_cksum();
                builder.append_data(&mut header, *name, *data).unwrap();
            }
            builder.into_inner().unwrap().finish().unwrap();
        }
        out
    }

    fn sha256_of(bytes: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        crate::download::bytes_to_hex(&Sha256::digest(bytes))
    }

    fn archive_entries() -> Vec<(&'static str, &'static [u8])> {
        vec![
            (EXE, b"server binary".as_slice()),
            (CORE, b"server implementation".as_slice()),
            (CPU, b"cpu backend".as_slice()),
            (UNKEPT_TOOL, b"unwanted tool".as_slice()),
            ("LICENSE", b"mit".as_slice()),
        ]
    }

    #[tokio::test]
    async fn test_install_extracts_kept_files_and_drops_tools() {
        let payload = build_archive(&archive_entries());
        let sha = sha256_of(&payload);
        let size = payload.len() as u64;
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", format!("/{ARCHIVE_NAME}").as_str())
            .with_body(payload.clone())
            .create_async()
            .await;

        let dir = tempfile::tempdir().unwrap();
        let spec = spec_with(&sha, size, "");
        let mut progress = Vec::new();
        install(&spec, &[server.url()], dir.path(), |done, total| {
            progress.push((done, total))
        })
        .await
        .unwrap();

        mock.assert_async().await;
        assert!(dir.path().join(EXE).exists());
        assert!(dir.path().join(CORE).exists());
        assert!(dir.path().join(CPU).exists());
        assert!(dir.path().join("LICENSE").exists());
        assert!(!dir.path().join(UNKEPT_TOOL).exists());
        assert!(!dir.path().join(ARCHIVE_NAME).exists(), "压缩包应被清掉");
        assert!(is_installed(&spec, dir.path()));
        assert_eq!(progress.last(), Some(&(size, size)));
    }

    #[tokio::test]
    async fn test_install_rejects_wrong_checksum() {
        let payload = build_archive(&archive_entries());
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", format!("/{ARCHIVE_NAME}").as_str())
            .with_body(payload.clone())
            .create_async()
            .await;

        let dir = tempfile::tempdir().unwrap();
        let wrong = "0".repeat(64);
        let spec = spec_with(&wrong, payload.len() as u64, "");
        let error = install(&spec, &[server.url()], dir.path(), |_, _| {})
            .await
            .unwrap_err();

        assert!(error.to_string().contains("校验失败"), "{error}");
        assert!(!is_installed(&spec, dir.path()));
        assert!(!dir.path().join(EXE).exists());
    }

    #[tokio::test]
    async fn test_install_skips_download_when_already_installed() {
        let payload = build_archive(&archive_entries());
        let sha = sha256_of(&payload);
        let dir = tempfile::tempdir().unwrap();
        for name in [EXE, CORE, CPU] {
            std::fs::write(dir.path().join(name), b"already here").unwrap();
        }
        let spec = spec_with(&sha, payload.len() as u64, "");

        // 基址指向不可达端口：已经装好就不应该发起任何请求。
        install(
            &spec,
            &["http://127.0.0.1:1".to_string()],
            dir.path(),
            |_, _| {},
        )
        .await
        .unwrap();
        assert!(is_installed(&spec, dir.path()));
    }

    #[tokio::test]
    async fn test_install_reuses_cached_archive_without_downloading() {
        let payload = build_archive(&archive_entries());
        let sha = sha256_of(&payload);
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(ARCHIVE_NAME), &payload).unwrap();
        let spec = spec_with(&sha, payload.len() as u64, "");

        install(
            &spec,
            &["http://127.0.0.1:1".to_string()],
            dir.path(),
            |_, _| {},
        )
        .await
        .unwrap();
        assert!(is_installed(&spec, dir.path()));
    }

    #[test]
    fn test_kept_relative_path_rejects_traversal_and_foreign_roots() {
        let flat = spec_with("00", 1, "");
        assert_eq!(
            kept_relative_path(&flat, "llama-server.exe").map(|p| p.to_string_lossy().into_owned()),
            Some(EXE.to_string())
        );
        assert_eq!(kept_relative_path(&flat, "../evil.dll"), None);
        assert_eq!(kept_relative_path(&flat, "/etc/passwd"), None);
        assert_eq!(
            kept_relative_path(&flat, "nested/ggml-cpu-x64.dll"),
            Some(PathBuf::from(format!("nested/{CPU}")))
        );
        assert_eq!(kept_relative_path(&flat, UNKEPT_TOOL), None);
        assert_eq!(kept_relative_path(&flat, ""), None);

        let rooted = spec_with("00", 1, "llama-b11612");
        assert!(kept_relative_path(&rooted, &format!("llama-b11612/{EXE}")).is_some());
        assert_eq!(kept_relative_path(&rooted, &format!("other/{EXE}")), None);
        assert_eq!(kept_relative_path(&rooted, "llama-b11612X/lib.so"), None);
        assert_eq!(kept_relative_path(&rooted, "llama-b11612"), None);
    }

    #[tokio::test]
    async fn test_extract_restores_archive_root_prefix() {
        let exe_path = format!("llama-b11612/{EXE}");
        let core_path = format!("llama-b11612/{CORE}");
        let cpu_path = format!("llama-b11612/{CPU}");
        let inner: Vec<(&str, &[u8])> = vec![
            (exe_path.as_str(), b"server".as_slice()),
            (core_path.as_str(), b"core".as_slice()),
            (cpu_path.as_str(), b"cpu".as_slice()),
        ];
        let payload = build_archive(&inner);
        let sha = sha256_of(&payload);
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", format!("/{ARCHIVE_NAME}").as_str())
            .with_body(payload.clone())
            .create_async()
            .await;

        let dir = tempfile::tempdir().unwrap();
        let spec = spec_with(&sha, payload.len() as u64, "llama-b11612");
        install(&spec, &[server.url()], dir.path(), |_, _| {})
            .await
            .unwrap();

        assert!(dir.path().join(EXE).exists(), "顶层目录要剥掉");
        assert!(is_installed(&spec, dir.path()));
    }

    #[test]
    fn test_is_installed_requires_cpu_library() {
        let dir = tempfile::tempdir().unwrap();
        let spec = spec_with("00", 1, "");
        std::fs::write(dir.path().join(EXE), b"x").unwrap();
        std::fs::write(dir.path().join(CORE), b"x").unwrap();
        assert!(!is_installed(&spec, dir.path()), "缺 CPU 后端库不算完整");
        std::fs::write(dir.path().join(CPU), b"x").unwrap();
        assert!(is_installed(&spec, dir.path()));
        std::fs::write(dir.path().join(CPU), b"").unwrap();
        assert!(!is_installed(&spec, dir.path()), "空文件不算就位");
    }

    #[test]
    fn test_remove_clears_runtime_directory() {
        let dir = tempfile::tempdir().unwrap();
        let runtime_dir = dir.path().join("llama-server");
        std::fs::create_dir_all(&runtime_dir).unwrap();
        std::fs::write(runtime_dir.join("llama-server.exe"), b"x").unwrap();

        remove(&runtime_dir).unwrap();
        assert!(!runtime_dir.exists());
        remove(&runtime_dir).unwrap();
    }
}
