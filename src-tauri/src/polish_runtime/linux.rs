//! Linux 平台的运行时进程实现。
//!
//! 两处平台特有：解包与手动放置的文件都要补执行位，物理核数要从 `/proc/cpuinfo` 的
//! `physical id` 与 `core id` 组合里数（`available_parallelism` 给的是逻辑线程）。

use super::process::{drain_stderr, LogTail, RuntimeChild, RuntimeProcess, TokioChild};
use crate::error::PolishRuntimeError;
use std::collections::HashSet;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use tokio::process::Command;

#[derive(Default)]
pub struct LinuxProcess;

impl LinuxProcess {
    pub fn new() -> Self {
        Self
    }
}

impl RuntimeProcess for LinuxProcess {
    fn spawn(
        &self,
        exe: &Path,
        args: &[String],
        log_tail: LogTail,
    ) -> Result<Box<dyn RuntimeChild>, PolishRuntimeError> {
        let mut command = Command::new(exe);
        command
            .args(args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        let child = command.spawn().map_err(|e| {
            PolishRuntimeError::StartupFailed(format!(
                "无法启动润色运行时 {}: {}",
                exe.display(),
                e
            ))
        })?;

        let mut inner = TokioChild::new(child);
        if let Some(stderr) = inner.take_stderr() {
            drain_stderr(stderr, log_tail);
        }
        Ok(Box::new(inner))
    }

    fn ensure_executable(&self, path: &Path) -> Result<(), PolishRuntimeError> {
        let mut permissions = std::fs::metadata(path)?.permissions();
        let mode = permissions.mode();
        if mode & 0o111 == 0o111 {
            return Ok(());
        }
        permissions.set_mode(mode | 0o755);
        std::fs::set_permissions(path, permissions)?;
        Ok(())
    }

    fn physical_core_count(&self) -> usize {
        match std::fs::read_to_string("/proc/cpuinfo") {
            Ok(text) => match count_physical_cores(&text) {
                0 => fallback_core_count(),
                cores => cores,
            },
            Err(_) => fallback_core_count(),
        }
    }
}

/// 数 `/proc/cpuinfo` 里 `(physical id, core id)` 的唯一组合。
fn count_physical_cores(cpuinfo: &str) -> usize {
    let mut pairs = HashSet::new();
    let mut physical_id = String::from("0");
    for line in cpuinfo.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        match key.trim() {
            "physical id" => physical_id = value.trim().to_string(),
            "core id" => {
                pairs.insert((physical_id.clone(), value.trim().to_string()));
            }
            _ => {}
        }
    }
    pairs.len()
}

fn fallback_core_count() -> usize {
    std::thread::available_parallelism().map_or(1, |count| count.get())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_physical_cores_reads_unique_pairs() {
        let cpuinfo = "\
processor\t: 0
physical id\t: 0
core id\t\t: 0

processor\t: 1
physical id\t: 0
core id\t\t: 1

processor\t: 2
physical id\t: 0
core id\t\t: 0

processor\t: 3
physical id\t: 1
core id\t\t: 0
";
        assert_eq!(count_physical_cores(cpuinfo), 3);
        assert_eq!(count_physical_cores(""), 0);
    }

    #[test]
    fn test_ensure_executable_adds_execute_bits() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("llama-server");
        std::fs::write(&path, b"binary").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();

        LinuxProcess::new().ensure_executable(&path).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o111, 0o111);
    }

    #[test]
    fn test_ensure_executable_reports_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("absent");
        assert!(LinuxProcess::new().ensure_executable(&path).is_err());
    }
}
