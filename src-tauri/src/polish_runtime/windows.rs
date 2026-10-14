//! Windows 平台的运行时进程实现。
//!
//! 两处平台特有：拉起子进程时不要控制台窗口，以及把子进程放进一个句柄关闭即整组结束的
//! Job 对象，避免应用异常退出后留下一条占着端口与内存的孤儿进程。物理核数用
//! `GetLogicalProcessorInformationEx` 数，逻辑线程在混合架构上反而更慢。

use super::process::{drain_stderr, LogTail, RuntimeChild, RuntimeProcess, TokioChild};
use crate::error::PolishRuntimeError;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use tokio::process::Command;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use windows::Win32::System::SystemInformation::{
    GetLogicalProcessorInformationEx, RelationProcessorCore,
};

/// 不弹出控制台窗口。
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// `SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX` 每个条目以关系类型开头，随后是条目长度。
const SIZE_OFFSET: usize = 4;
const FIELD_WIDTH: usize = 4;

#[derive(Default)]
pub struct WindowsProcess;

impl WindowsProcess {
    pub fn new() -> Self {
        Self
    }

    /// 把子进程挂到 Job 对象上，句柄关闭时整组进程一起结束。
    fn attach_job(child: &tokio::process::Child) -> Option<HANDLE> {
        unsafe {
            let job = CreateJobObjectW(None, PCWSTR::null()).ok()?;
            let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let configured = SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                std::ptr::from_ref(&info).cast(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            if configured.is_err() {
                let _ = CloseHandle(job);
                return None;
            }
            let handle = child.raw_handle()?;
            if AssignProcessToJobObject(job, HANDLE(handle)).is_err() {
                let _ = CloseHandle(job);
                return None;
            }
            Some(job)
        }
    }
}

impl RuntimeProcess for WindowsProcess {
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
            .kill_on_drop(true)
            .creation_flags(CREATE_NO_WINDOW);
        let child = command.spawn().map_err(|e| {
            PolishRuntimeError::StartupFailed(format!(
                "无法启动润色运行时 {}: {}",
                exe.display(),
                e
            ))
        })?;

        let job = Self::attach_job(&child).map(JobHandle);
        if job.is_none() {
            tracing::warn!("failed to attach polish runtime to a job object");
        }
        let mut inner = TokioChild::new(child);
        if let Some(stderr) = inner.take_stderr() {
            drain_stderr(stderr, log_tail);
        }
        Ok(Box::new(WindowsChild { inner, job }))
    }

    fn ensure_executable(&self, _path: &Path) -> Result<(), PolishRuntimeError> {
        Ok(())
    }

    fn physical_core_count(&self) -> usize {
        let cores = processor_core_count();
        if cores == 0 {
            fallback_core_count()
        } else {
            cores
        }
    }
}

struct WindowsChild {
    inner: TokioChild,
    /// 关闭即结束整组进程，因此随子进程一起释放。
    job: Option<JobHandle>,
}

/// Job 句柄只在 Drop 里关闭，句柄值本身不参与跨线程共享。
struct JobHandle(HANDLE);

// 安全：句柄是内核对象的所有权凭证，只由持有者关闭一次，没有共享可变状态。
unsafe impl Send for JobHandle {}

impl Drop for WindowsChild {
    fn drop(&mut self) {
        if let Some(job) = self.job.take() {
            unsafe {
                let _ = CloseHandle(job.0);
            }
        }
    }
}

impl RuntimeChild for WindowsChild {
    fn id(&self) -> u32 {
        self.inner.id()
    }

    fn try_wait(&mut self) -> Result<Option<i32>, PolishRuntimeError> {
        self.inner.try_wait()
    }

    fn kill<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), PolishRuntimeError>> + Send + 'a>> {
        self.inner.kill()
    }
}

/// 数物理核。缓冲区里每个条目以「关系类型（4 字节）加条目长度（4 字节）」开头，按长度步进。
fn processor_core_count() -> usize {
    unsafe {
        let mut length = 0u32;
        let _ = GetLogicalProcessorInformationEx(RelationProcessorCore, None, &mut length);
        if length == 0 {
            return 0;
        }
        let mut buffer = vec![0u8; length as usize];
        let mut written = length;
        let read = GetLogicalProcessorInformationEx(
            RelationProcessorCore,
            Some(buffer.as_mut_ptr().cast()),
            &mut written,
        );
        if read.is_err() {
            return 0;
        }

        let mut offset = 0usize;
        let mut cores = 0usize;
        while offset + SIZE_OFFSET + FIELD_WIDTH <= buffer.len() {
            let relationship = i32::from_ne_bytes(
                buffer[offset..offset + FIELD_WIDTH]
                    .try_into()
                    .unwrap_or([0; 4]),
            );
            let size = u32::from_ne_bytes(
                buffer[offset + SIZE_OFFSET..offset + SIZE_OFFSET + FIELD_WIDTH]
                    .try_into()
                    .unwrap_or([0; 4]),
            ) as usize;
            if size < SIZE_OFFSET + FIELD_WIDTH {
                break;
            }
            if relationship == RelationProcessorCore.0 {
                cores += 1;
            }
            offset += size;
        }
        cores
    }
}

fn fallback_core_count() -> usize {
    std::thread::available_parallelism().map_or(1, |count| count.get())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_processor_core_count_is_within_logical_count() {
        let logical = std::thread::available_parallelism().map_or(1, |count| count.get());
        let physical = WindowsProcess::new().physical_core_count();
        assert!(physical >= 1);
        assert!(
            physical <= logical,
            "物理核 {physical} 不应超过逻辑核 {logical}"
        );
    }

    #[test]
    fn test_ensure_executable_is_a_noop() {
        assert!(WindowsProcess::new()
            .ensure_executable(Path::new("C:/whatever.exe"))
            .is_ok());
    }
}
