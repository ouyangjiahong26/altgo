//! 运行时资源工具模块。

use std::path::PathBuf;

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

/// 在系统 PATH 上查找命令，返回其绝对路径；找不到返回 `None`。
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
