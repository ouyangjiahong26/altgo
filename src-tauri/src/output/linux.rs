//! Linux 输出模块。
//!
//! 剪切板支持三种后端：`xclip`、`xsel`、`wl-copy`（Wayland）。
//! 根据 `XDG_SESSION_TYPE` 自动检测可用工具。

use std::process::Command;
use std::sync::Arc;

use crate::error::OutputError;

/// Linux 上可用的剪切板管理工具。
#[derive(Debug, Clone, Copy)]
pub enum ClipboardTool {
    XClip,
    XSel,
    WlCopy,
}

impl std::fmt::Display for ClipboardTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClipboardTool::XClip => write!(f, "xclip"),
            ClipboardTool::XSel => write!(f, "xsel"),
            ClipboardTool::WlCopy => write!(f, "wl-copy"),
        }
    }
}

/// 检测系统上可用的剪切板工具。
pub fn detect_clipboard_tool() -> Option<ClipboardTool> {
    let session_type = std::env::var("XDG_SESSION_TYPE").unwrap_or_default();

    if session_type == "wayland" {
        if which("wl-copy") {
            return Some(ClipboardTool::WlCopy);
        }
        // Wayland 下没有 wl-copy 时，回退到 xclip/xsel（可能通过 XWayland 工作）
        if which("xclip") {
            return Some(ClipboardTool::XClip);
        }
        if which("xsel") {
            return Some(ClipboardTool::XSel);
        }
        return None;
    }

    // X11 或未指定：优先 xclip，其次 xsel。
    if which("xclip") {
        return Some(ClipboardTool::XClip);
    }
    if which("xsel") {
        return Some(ClipboardTool::XSel);
    }

    // 兜底：检查 wl-copy，防止用户在 Wayland 上但未设置 XDG_SESSION_TYPE。
    if which("wl-copy") {
        return Some(ClipboardTool::WlCopy);
    }

    None
}

/// 使用指定的剪切板工具写入文本。
pub fn write_clipboard_with_tool(tool: ClipboardTool, text: &str) -> Result<(), OutputError> {
    let args: Vec<&str> = match tool {
        ClipboardTool::XClip => vec!["-selection", "clipboard"],
        ClipboardTool::XSel => vec!["--clipboard", "--input"],
        ClipboardTool::WlCopy => vec![],
    };

    let output = Command::new(tool.to_string())
        .args(&args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(text.as_bytes())?;
            }
            child.wait_with_output()
        });

    match output {
        Ok(out) if out.status.success() => Ok(()),
        Ok(out) => Err(OutputError::ClipboardFailed(format!(
            "clipboard command {} failed: {}",
            tool,
            String::from_utf8_lossy(&out.stderr)
        ))),
        Err(e) => Err(OutputError::ClipboardFailed(format!(
            "failed to run {}: {}",
            tool, e
        ))),
    }
}

/// 检查命令是否存在于 PATH 中。
fn which(cmd: &str) -> bool {
    crate::resource::which_binary(cmd).is_some()
}

/// Linux `Output` 适配器 —— 封装各类剪切板工具。
pub struct LinuxOutput {
    tool: Option<ClipboardTool>,
}

impl LinuxOutput {
    pub fn new() -> Self {
        Self {
            tool: detect_clipboard_tool(),
        }
    }
}

impl super::Output for LinuxOutput {
    fn write_clipboard(&self, text: &str) -> Result<(), OutputError> {
        let tool = self.tool.ok_or(OutputError::NoClipboardTool)?;
        write_clipboard_with_tool(tool, text)
    }

    fn clone_box(&self) -> Arc<dyn super::Output> {
        Arc::new(LinuxOutput { tool: self.tool })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_clipboard_tool() {
        let tool = detect_clipboard_tool();
        if let Some(t) = tool {
            let s = t.to_string();
            assert!(s == "xclip" || s == "xsel" || s == "wl-copy");
        }
    }
}
