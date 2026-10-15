//! 润色运行时的获取：下载官方产物、校验、按保留规则解包。
//!
//! 下载内核与模型共用（`crate::download`）。压缩包留在运行时目录里直到解包成功，因此
//! 解包失败重试时不必重新下载。校验和固定，手动放置的文件也能被同一套结构检查接受。

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
    // 解包后复核：保留规则若与产物布局不符，这里直接报错，而不是删掉压缩包留下一个用不了的目录。
    if !is_installed(spec, dir) {
        tracing::warn!(
            dir = %dir.display(),
            "runtime archive extracted but the files are still incomplete"
        );
        return Err(PolishRuntimeError::Incomplete(dir.display().to_string()));
    }
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
    // 只有 Windows 的 zip 构造器要写字节，Unix 的 tar 构造器不用。
    #[cfg(windows)]
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
        build_archive_with_links(entries, &[])
    }

    /// 支持符号链接的归档构造器，用来复现 Linux 产物的布局。
    #[cfg(unix)]
    fn build_archive_with_links(files: &[(&str, &[u8])], links: &[(&str, &str)]) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let encoder = flate2::write::GzEncoder::new(&mut out, flate2::Compression::default());
            let mut builder = tar::Builder::new(encoder);
            for (name, data) in files {
                let mut header = tar::Header::new_gnu();
                header.set_size(data.len() as u64);
                header.set_mode(0o755);
                header.set_cksum();
                builder.append_data(&mut header, *name, *data).unwrap();
            }
            for (name, target) in links {
                let mut header = tar::Header::new_gnu();
                header.set_entry_type(tar::EntryType::Symlink);
                header.set_size(0);
                header.set_mode(0o777);
                builder.append_link(&mut header, *name, *target).unwrap();
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

    /// 两种平台的归档命名各造一个 spec，在任一平台上都跑同一套断言。
    /// 先前这里把 Windows 的文件名写死在断言里，Linux 上直接挂掉。
    fn named_spec(
        exe: &'static str,
        suffix: &'static str,
        cpu_prefix: &'static str,
        required: &'static [&'static str],
        root: &'static str,
    ) -> RuntimeSpec {
        RuntimeSpec {
            target: "test",
            archive_name: ARCHIVE_NAME,
            archive_size: 1,
            archive_sha256: Cow::Owned("00".to_string()),
            archive_root: root,
            executable: exe,
            shared_lib_suffix: suffix,
            required_libraries: required,
            cpu_lib_prefix: cpu_prefix,
        }
    }

    fn assert_common_path_rules(spec: &RuntimeSpec, exe: &str, cpu: &str, tool: &str) {
        assert_eq!(
            kept_relative_path(spec, exe).map(|p| p.to_string_lossy().into_owned()),
            Some(exe.to_string())
        );
        assert_eq!(kept_relative_path(spec, "../evil.dll"), None);
        assert_eq!(kept_relative_path(spec, "/etc/passwd"), None);
        assert_eq!(kept_relative_path(spec, tool), None);
        assert_eq!(kept_relative_path(spec, ""), None);
        let nested = format!("nested/{cpu}");
        assert_eq!(
            kept_relative_path(spec, &nested).map(|p| p.to_string_lossy().into_owned()),
            Some(nested)
        );
    }

    #[test]
    fn test_kept_relative_path_rejects_traversal_and_foreign_roots() {
        let windows_like = named_spec(
            "llama-server.exe",
            ".dll",
            "ggml-cpu",
            &["llama-server-impl.dll"],
            "",
        );
        assert_common_path_rules(
            &windows_like,
            "llama-server.exe",
            "ggml-cpu-x64.dll",
            "llama-cli.exe",
        );
        assert!(windows_like.keeps("llama-server-impl.dll"));
        assert!(!windows_like.keeps("ggml.dll.bak"));

        let linux_like = named_spec(
            "llama-server",
            ".so",
            "libggml-cpu",
            &["libllama-server-impl.so"],
            "",
        );
        assert_common_path_rules(
            &linux_like,
            "llama-server",
            "libggml-cpu-zen4.so",
            "llama-cli",
        );
        assert!(linux_like.keeps("libllama.so.0.6.0"));
        assert!(linux_like.keeps("libllama-common.so.0"));
        assert!(!linux_like.keeps("libfoo.socket"));
    }

    #[test]
    fn test_kept_relative_path_strips_archive_root() {
        let rooted = named_spec(
            "llama-server",
            ".so",
            "libggml-cpu",
            &["libllama-server-impl.so"],
            "llama-b11612",
        );
        assert!(kept_relative_path(&rooted, "llama-b11612/llama-server").is_some());
        assert_eq!(kept_relative_path(&rooted, "other/llama-server"), None);
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

    /// 复现 Linux 产物的布局：真身带版本后缀，清单要求的名字是指向它的符号链接。
    /// 保留规则漏掉带版本的真身时，这条会失败。
    #[cfg(unix)]
    #[tokio::test]
    async fn test_install_keeps_versioned_libraries_reachable_through_symlinks() {
        const LINUX_REQUIRED: &[&str] = &["libllama-server-impl.so", "libllama-common.so"];

        let files: Vec<(&str, &[u8])> = vec![
            ("llama-server", b"server".as_slice()),
            ("libllama-server-impl.so", b"impl".as_slice()),
            ("libllama-common.so.0.6.0", b"common".as_slice()),
            ("libggml-cpu-zen4.so", b"cpu".as_slice()),
            ("llama-cli", b"tool".as_slice()),
        ];
        let links: Vec<(&str, &str)> = vec![("libllama-common.so", "libllama-common.so.0.6.0")];
        let payload = build_archive_with_links(&files, &links);
        let sha = sha256_of(&payload);
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", format!("/{ARCHIVE_NAME}").as_str())
            .with_body(payload.clone())
            .create_async()
            .await;

        let dir = tempfile::tempdir().unwrap();
        let spec = RuntimeSpec {
            target: "test-linux",
            archive_name: ARCHIVE_NAME,
            archive_size: payload.len() as u64,
            archive_sha256: Cow::Owned(sha),
            archive_root: "",
            executable: EXE,
            shared_lib_suffix: SUFFIX,
            required_libraries: LINUX_REQUIRED,
            cpu_lib_prefix: CPU_PREFIX,
        };
        install(&spec, &[server.url()], dir.path(), |_, _| {})
            .await
            .unwrap();

        assert!(
            dir.path().join("libllama-common.so.0.6.0").exists(),
            "带版本后缀的真身要保留"
        );
        assert!(
            dir.path().join("libllama-common.so").exists(),
            "符号链接要保留且能解析到真身"
        );
        assert!(!dir.path().join("llama-cli").exists());
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
