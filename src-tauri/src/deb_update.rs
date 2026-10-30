//! deb 安装包的应用内更新。
//!
//! 仅用于 dpkg 安装的 Linux 构建：更新说明窗在支持级别为 `deb_package` 时走
//! 这里，从 GitHub Release 取 checksums.txt 取得 SHA-256，下载当前架构的 deb，
//! 再经 `pkexec apt-get install` 安装。授权由系统 polkit 对话框完成，应用进程
//! 不接触用户密码。

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter};

use crate::download::{self, FetchRequest, RemoteFile};
use crate::updater::{check_with_app, UpdateInfoRaw, UpdateProvider};

/// GitHub Release 资产下载根地址（不含版本段）。
pub const RELEASE_DOWNLOAD_BASE: &str =
    "https://github.com/ouyangjiahong26/altgo/releases/download";

/// 下载进度事件名，与前端 `updateNotes.ts` 共用。
pub const UPDATE_INSTALL_PROGRESS_EVENT: &str = "update-install-progress";

/// 发布产物校验和文件名。
const CHECKSUMS_FILE: &str = "checksums.txt";

/// deb 体积下限：防止把错误页或截断文件当成安装包。
const MIN_DEB_BYTES: u64 = 1 << 20;

/// 下载进度的最小发送间隔，避免每收一块就跨进程发一次事件。
const PROGRESS_INTERVAL: Duration = Duration::from_millis(120);

/// 下载进度阶段：先下载安装包，再等系统授权并安装。
#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallPhase {
    Downloading,
    Installing,
}

/// 下载进度事件载荷。
#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInstallProgress {
    pub phase: InstallPhase,
    pub downloaded: u64,
    pub total: u64,
}

/// 当前架构对应的 Debian 架构名。
pub fn deb_arch() -> Option<&'static str> {
    match std::env::consts::ARCH {
        "x86_64" => Some("amd64"),
        "aarch64" => Some("arm64"),
        _ => None,
    }
}

/// 由远端版本号构造 deb 资产文件名。
pub fn deb_asset_name(version: &str) -> Result<String, String> {
    let version = normalize_version(version)?;
    let arch = deb_arch().ok_or_else(|| {
        format!(
            "当前架构（{}）没有对应的 deb 安装包",
            std::env::consts::ARCH
        )
    })?;
    Ok(format!("altgo_{version}_{arch}.deb"))
}

/// 去掉可选的 v 前缀并校验版本号字符。
///
/// 版本号来自远端清单，拼进 URL 前只允许字母、数字与 `.`、`-`、`+`，
/// 避免路径穿越或查询串注入。
fn normalize_version(version: &str) -> Result<&str, String> {
    let trimmed = version.trim();
    let trimmed = trimmed.strip_prefix('v').unwrap_or(trimmed);
    let valid = !trimmed.is_empty()
        && trimmed
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+'));
    if valid {
        Ok(trimmed)
    } else {
        Err(format!("安装包版本号格式不受支持：{version}"))
    }
}

/// 当前进程是否由官方 deb 包装，且具备应用内安装条件。
#[cfg(target_os = "linux")]
pub fn can_install_deb() -> bool {
    deb_arch().is_some()
        && command_path("dpkg-query").is_some()
        && command_path("apt-get").is_some()
        && command_path("pkexec").is_some()
        && dpkg_owns_altgo()
}

#[cfg(not(target_os = "linux"))]
pub fn can_install_deb() -> bool {
    false
}

/// 在 PATH（及常见系统目录）中查找可执行文件。
fn command_path(name: &str) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default();
    dirs.extend(
        ["/usr/bin", "/bin", "/usr/sbin", "/sbin"]
            .iter()
            .map(PathBuf::from),
    );
    dirs.into_iter()
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

/// 当前可执行文件是否由名为 altgo 的 dpkg 包安装。
///
/// 用包名确认而不是只看 dpkg 是否存在：AUR、rpm 与手动安装的二进制改用 deb
/// 就地覆盖会破坏原包管理器的记录。
#[cfg(target_os = "linux")]
fn dpkg_owns_altgo() -> bool {
    let Some(dpkg_query) = command_path("dpkg-query") else {
        return false;
    };
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let Ok(exe) = exe.canonicalize() else {
        return false;
    };
    let Ok(output) = std::process::Command::new(dpkg_query)
        .arg("-S")
        .arg(&exe)
        .output()
    else {
        return false;
    };
    output.status.success()
        && String::from_utf8_lossy(&output.stdout)
            .lines()
            .any(|line| line.split(':').next() == Some("altgo"))
}

/// 从校验和清单里找出目标文件的 SHA-256。
///
/// 清单每行形如 `<sha256>  ./deb/altgo_2.8.2_amd64.deb`，路径带发布目录前缀。
pub fn checksum_for(checksums: &str, filename: &str) -> Option<String> {
    checksums.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let sha256 = parts.next()?;
        let path = parts.next()?;
        let name = path.rsplit('/').next()?;
        let valid = sha256.len() == 64 && sha256.bytes().all(|byte| byte.is_ascii_hexdigit());
        (name == filename && valid).then(|| sha256.to_ascii_lowercase())
    })
}

/// 下载指定版本、当前架构的 deb，返回落盘路径。
pub async fn download_deb<F>(version: &str, on_progress: F) -> Result<PathBuf, String>
where
    F: FnMut(u64, u64),
{
    let filename = deb_asset_name(version)?;
    let version = normalize_version(version)?;
    let bases = vec![format!("{RELEASE_DOWNLOAD_BASE}/v{version}")];
    download_from(&bases, &filename, &update_dir(), MIN_DEB_BYTES, on_progress).await
}

/// 下载内核的可注入版本：基址、目标目录与体积下限由调用方给出，供测试使用。
pub(crate) async fn download_from<F>(
    bases: &[String],
    filename: &str,
    dir: &Path,
    min_bytes: u64,
    mut on_progress: F,
) -> Result<PathBuf, String>
where
    F: FnMut(u64, u64),
{
    let base = bases
        .first()
        .ok_or_else(|| "缺少安装包下载基址".to_string())?;
    let checksums = fetch_checksums(base).await?;
    let sha256 = checksum_for(&checksums, filename)
        .ok_or_else(|| format!("校验和清单里没有 {filename}，已停止安装"))?;

    std::fs::create_dir_all(dir).map_err(|e| format!("无法创建下载目录：{e}"))?;
    let dest = dir.join(filename);

    let url = format!("{base}/{filename}");
    let total = probe_content_length(&url).await;
    if reuse_verified(&dest, &sha256, min_bytes, total, &mut on_progress).await? {
        return Ok(dest);
    }
    download::remove_invalid(&dest).map_err(|e| format!("无法清理旧安装包：{e}"))?;

    download::fetch_file(
        bases,
        &FetchRequest {
            file: RemoteFile {
                filename,
                size_bytes: total,
                sha256: &sha256,
            },
            dest: dest.clone(),
            min_bytes,
            progress_base: 0,
            progress_total: total,
            attempts: download::DOWNLOAD_ATTEMPTS,
        },
        &mut on_progress,
    )
    .await
    .map_err(|e| format!("下载安装包失败：{e}"))?;
    Ok(dest)
}

/// 已落盘的安装包校验一致时直接复用，授权取消后重试不必再传几十兆。
async fn reuse_verified<F>(
    path: &Path,
    sha256: &str,
    min_bytes: u64,
    total: u64,
    on_progress: &mut F,
) -> Result<bool, String>
where
    F: FnMut(u64, u64),
{
    if !download::structurally_ready(path, min_bytes) {
        return Ok(false);
    }
    let existing = download::file_sha256_blocking(path.to_path_buf())
        .await
        .map_err(|e| format!("读取旧安装包失败：{e}"))?;
    if existing != sha256 {
        return Ok(false);
    }
    let size = std::fs::metadata(path)
        .map(|meta| meta.len())
        .unwrap_or(total);
    on_progress(size, size);
    Ok(true)
}

/// 取发布产物校验和清单。
async fn fetch_checksums(base: &str) -> Result<String, String> {
    let url = format!("{base}/{CHECKSUMS_FILE}");
    let response = download::client()
        .get(&url)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| format!("获取校验和清单失败：{e}"))?;
    if !response.status().is_success() {
        return Err(format!("获取校验和清单失败：HTTP {}", response.status()));
    }
    response
        .text()
        .await
        .map_err(|e| format!("读取校验和清单失败：{e}"))
}

/// 探测安装包总字节数，只用于进度条；拿不到时返回 0，前端按不确定进度展示。
async fn probe_content_length(url: &str) -> u64 {
    download::client()
        .head(url)
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .ok()
        .and_then(|response| response.content_length())
        .unwrap_or(0)
}

/// 安装包下载目录：`<缓存目录>/altgo/updates/`。
fn update_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("altgo")
        .join("updates")
}

/// 用系统授权对话框运行 apt-get 安装已下载的 deb。
pub async fn install_deb(deb: &Path) -> Result<(), String> {
    let (program, args) = privilege_command(deb)?;
    let output = tokio::process::Command::new(&program)
        .args(&args)
        .output()
        .await
        .map_err(|e| format!("无法启动管理员授权程序：{e}"))?;
    install_result(
        output.status.code(),
        &String::from_utf8_lossy(&output.stderr),
        deb,
    )
}

/// pkexec 调用 apt-get 的完整命令，程序路径用绝对路径，避免 polkit 精简环境里
/// PATH 不含 sbin 目录。
fn privilege_command(deb: &Path) -> Result<(PathBuf, Vec<String>), String> {
    let pkexec =
        command_path("pkexec").ok_or_else(|| "未找到 pkexec，无法申请管理员权限".to_string())?;
    let apt_get =
        command_path("apt-get").ok_or_else(|| "未找到 apt-get，无法安装 deb 安装包".to_string())?;
    Ok((
        pkexec,
        vec![
            apt_get.display().to_string(),
            "install".to_string(),
            "-y".to_string(),
            "--allow-downgrades".to_string(),
            deb.display().to_string(),
        ],
    ))
}

/// 把 pkexec 与 apt-get 的退出状态翻译成用户文案。
fn install_result(code: Option<i32>, stderr: &str, deb: &Path) -> Result<(), String> {
    if code == Some(0) {
        return Ok(());
    }
    let manual = format!("可手动执行：sudo apt-get install {}", deb.display());
    match code {
        Some(126) => Err(format!("已取消管理员授权，更新未安装。{manual}")),
        Some(127) => Err(format!(
            "无法请求管理员授权，请确认桌面会话有 polkit 认证代理。{manual}"
        )),
        Some(code) => {
            let detail = stderr_tail(stderr);
            if detail.is_empty() {
                Err(format!("安装失败（退出码 {code}）。{manual}"))
            } else {
                Err(format!("安装失败（退出码 {code}）：{detail}。{manual}"))
            }
        }
        None => Err(format!("安装进程没有正常退出。{manual}")),
    }
}

/// 截取错误输出末尾 400 个字符，单行展示在更新说明窗底部的错误区。
fn stderr_tail(stderr: &str) -> String {
    let trimmed = stderr.trim();
    let start = trimmed
        .char_indices()
        .rev()
        .nth(400)
        .map(|(index, _)| index)
        .unwrap_or(0);
    trimmed[start..].replace('\n', " ")
}

/// deb 档的更新提供者：复用更新检查结果，下载 deb 后经 polkit 安装。
pub struct DebUpdateProvider {
    pub app: AppHandle,
}

impl UpdateProvider for DebUpdateProvider {
    fn check_update_raw<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<UpdateInfoRaw>, String>> + Send + 'a>> {
        let app = self.app.clone();
        Box::pin(async move { check_with_app(&app).await })
    }

    fn download_and_install_raw<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>> {
        let app = self.app.clone();
        Box::pin(async move {
            let info = check_with_app(&app)
                .await?
                .ok_or_else(|| "没有检测到可用更新".to_string())?;
            let path = download_deb(&info.version, progress_emitter(&app)).await?;
            let size = std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
            emit_progress(&app, InstallPhase::Installing, size, size);
            install_deb(&path).await?;
            let _ = tokio::fs::remove_file(&path).await;
            Ok(())
        })
    }
}

/// 构造带节流的下载进度回调。
fn progress_emitter(app: &AppHandle) -> impl FnMut(u64, u64) {
    let app = app.clone();
    let mut last_emit: Option<Instant> = None;
    move |downloaded: u64, total: u64| {
        let finished = total > 0 && downloaded >= total;
        let due = last_emit.is_none_or(|at| at.elapsed() >= PROGRESS_INTERVAL);
        if due || finished {
            last_emit = Some(Instant::now());
            emit_progress(&app, InstallPhase::Downloading, downloaded, total);
        }
    }
}

fn emit_progress(app: &AppHandle, phase: InstallPhase, downloaded: u64, total: u64) {
    let _ = app.emit(
        UPDATE_INSTALL_PROGRESS_EVENT,
        UpdateInstallProgress {
            phase,
            downloaded,
            total,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    #[test]
    fn test_deb_asset_name_uses_current_arch_and_strips_v() {
        let arch = deb_arch().expect("测试架构应支持 deb");
        assert_eq!(
            deb_asset_name("2.8.2").unwrap(),
            format!("altgo_2.8.2_{arch}.deb")
        );
        assert_eq!(
            deb_asset_name("v2.8.2").unwrap(),
            format!("altgo_2.8.2_{arch}.deb")
        );
    }

    #[test]
    fn test_deb_asset_name_rejects_unsafe_versions() {
        assert!(deb_asset_name("").is_err());
        assert!(deb_asset_name("../2.8.2").is_err());
        assert!(deb_asset_name("2.8.2?x=1").is_err());
        assert!(deb_asset_name("2.8.2/arm64").is_err());
    }

    #[test]
    fn test_checksum_for_matches_basename_in_release_path() {
        let arch = deb_arch().unwrap();
        let filename = format!("altgo_2.8.2_{arch}.deb");
        let sha = "a".repeat(64);
        let manifest = format!(
            "{}  ./appimage/altgo_2.8.2_x86_64.AppImage\n{sha}  ./deb/{filename}\n",
            "b".repeat(64)
        );
        assert_eq!(checksum_for(&manifest, &filename), Some(sha));
        assert_eq!(checksum_for(&manifest, "altgo_2.8.2_riscv64.deb"), None);
    }

    #[test]
    fn test_checksum_for_skips_malformed_lines() {
        let filename = "altgo_2.8.2_amd64.deb";
        let sha = "C".repeat(64);
        let manifest = format!("not-a-hash  ./deb/{filename}\n{sha}  ./deb/{filename}\n");
        assert_eq!(checksum_for(&manifest, filename), Some(sha.to_lowercase()));
    }

    #[test]
    fn test_install_result_maps_polkit_exit_codes() {
        let deb = Path::new("/tmp/altgo.deb");
        assert!(install_result(Some(0), "", deb).is_ok());

        let cancelled = install_result(Some(126), "", deb).unwrap_err();
        assert!(cancelled.contains("取消"), "{cancelled}");

        let no_agent = install_result(Some(127), "", deb).unwrap_err();
        assert!(no_agent.contains("polkit"), "{no_agent}");

        let failed =
            install_result(Some(100), "E: Sub-process /usr/bin/dpkg\nreturned", deb).unwrap_err();
        assert!(failed.contains("退出码 100"), "{failed}");
        assert!(
            failed.contains("sudo apt-get install /tmp/altgo.deb"),
            "{failed}"
        );
        assert!(!failed.contains('\n'), "{failed}");
    }

    #[test]
    fn test_non_deb_binary_is_not_offered_as_deb_install() {
        // 测试二进制位于 target 目录，不属于任何 dpkg 包。
        assert!(!can_install_deb());
    }

    #[tokio::test]
    async fn test_download_from_verifies_checksum_and_reuses_file() {
        let payload = vec![7u8; 64];
        let sha256 = download::bytes_to_hex(&Sha256::digest(&payload));
        let filename = "altgo_9.9.9_amd64.deb";

        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/checksums.txt")
            .with_body(format!("{sha256}  ./deb/{filename}\n"))
            .create_async()
            .await;
        let mock = server
            .mock("GET", format!("/{filename}").as_str())
            .with_body(payload.clone())
            .expect(1)
            .create_async()
            .await;

        let dir = tempfile::tempdir().unwrap();
        let mut progress = Vec::new();
        let path = download_from(&[server.url()], filename, dir.path(), 16, |done, total| {
            progress.push((done, total))
        })
        .await
        .unwrap();

        assert_eq!(std::fs::read(&path).unwrap(), payload);
        assert_eq!(progress.last(), Some(&(payload.len() as u64, 0)));

        // 第二次调用命中已校验文件，不再发起下载请求。
        let again = download_from(&[server.url()], filename, dir.path(), 16, |_, _| {})
            .await
            .unwrap();
        assert_eq!(again, path);
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_download_from_rejects_checksum_mismatch() {
        let payload = vec![3u8; 64];
        let filename = "altgo_9.9.9_amd64.deb";

        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/checksums.txt")
            .with_body(format!("{}  ./deb/{filename}\n", "0".repeat(64)))
            .create_async()
            .await;
        server
            .mock("GET", format!("/{filename}").as_str())
            .with_body(payload)
            .create_async()
            .await;

        let dir = tempfile::tempdir().unwrap();
        let error = download_from(&[server.url()], filename, dir.path(), 16, |_, _| {})
            .await
            .unwrap_err();

        assert!(error.contains("下载安装包失败"), "{error}");
        assert!(!dir.path().join(filename).exists());
    }

    #[tokio::test]
    async fn test_download_from_fails_when_checksums_lack_asset() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/checksums.txt")
            .with_body("")
            .create_async()
            .await;

        let dir = tempfile::tempdir().unwrap();
        let error = download_from(
            &[server.url()],
            "altgo_9.9.9_amd64.deb",
            dir.path(),
            16,
            |_, _| {},
        )
        .await
        .unwrap_err();

        assert!(error.contains("校验和清单里没有"), "{error}");
    }
}
