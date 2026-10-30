//! 运行时资源工具模块。

use std::path::{Path, PathBuf};

/// VAD 模型相对资源目录的路径。
const VAD_MODEL_RESOURCE: &str = "vad/silero_vad.onnx";

/// 默认线程数下限的回退值（取不到 CPU 并行度时使用）。
const DEFAULT_THREADS_FALLBACK: u32 = 4;

/// 解析有效线程数：配置 `> 0` 时用配置值，否则用 CPU 并行度。
pub fn effective_threads(configured: u32) -> u32 {
    if configured > 0 {
        configured
    } else {
        std::thread::available_parallelism()
            .map(|n| n.get() as u32)
            .unwrap_or(DEFAULT_THREADS_FALLBACK)
    }
}

/// 将以 `~` 开头的路径展开为用户主目录路径。
pub fn expand_tilde(path: &str) -> PathBuf {
    if path.starts_with("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(path.trim_start_matches("~/"));
        }
    }
    PathBuf::from(path)
}

/// 解析随安装包分发的 silero-vad 模型路径。
///
/// 安装后从打包资源目录取，从源码目录运行时取仓库的资源目录。两处都没有时
/// 返回 `None`，调用方退回 RMS 静音切分。
pub fn vad_model_path(resource_dir: Option<&Path>) -> Option<PathBuf> {
    find_vad_model(resource_dir, Some(Path::new("resources")))
}

/// 依次在资源目录与源码资源目录里查找 VAD 模型。
fn find_vad_model(resource_dir: Option<&Path>, source_root: Option<&Path>) -> Option<PathBuf> {
    resource_dir
        .map(|dir| dir.join(VAD_MODEL_RESOURCE))
        .into_iter()
        .chain(source_root.map(|root| root.join(VAD_MODEL_RESOURCE)))
        .find(|path| path.is_file())
}

/// 在系统 PATH 上查找命令，返回其绝对路径，找不到返回 `None`。
pub fn which_binary(name: &str) -> Option<PathBuf> {
    let output = std::process::Command::new("which")
        .arg(name)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
    if path.exists() {
        Some(path)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_vad_model(root: &Path) -> PathBuf {
        let path = root.join(VAD_MODEL_RESOURCE);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"fake").unwrap();
        path
    }

    #[test]
    fn test_find_vad_model_prefers_resource_dir() {
        let packaged = tempfile::tempdir().unwrap();
        let source = tempfile::tempdir().unwrap();
        let packaged_path = write_vad_model(packaged.path());
        write_vad_model(source.path());

        assert_eq!(
            find_vad_model(Some(packaged.path()), Some(source.path())),
            Some(packaged_path)
        );
    }

    #[test]
    fn test_find_vad_model_falls_back_to_source_root() {
        let packaged = tempfile::tempdir().unwrap();
        let source = tempfile::tempdir().unwrap();
        let source_path = write_vad_model(source.path());

        assert_eq!(
            find_vad_model(Some(packaged.path()), Some(source.path())),
            Some(source_path)
        );
    }

    #[test]
    fn test_find_vad_model_none_when_missing() {
        let packaged = tempfile::tempdir().unwrap();
        let source = tempfile::tempdir().unwrap();

        assert_eq!(
            find_vad_model(Some(packaged.path()), Some(source.path())),
            None
        );
        assert_eq!(find_vad_model(None, None), None);
    }
}
