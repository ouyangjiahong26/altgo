//! Linux 激活键捕获（evtest）。
//!
//! 通过 `evtest` 监听 `/dev/input/event*`，阻塞等待一次按键。

use std::io::{BufRead, Read};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use super::{evdev_code_to_keysym_name, CaptureActivationResponse, KeyCapture};

const CAPTURE_TIMEOUT: Duration = Duration::from_secs(12);

/// Linux 平台激活键捕获器。
pub struct LinuxKeyCapture;

impl LinuxKeyCapture {
    /// 创建新的捕获器。
    pub fn new() -> Self {
        Self
    }
}

impl Default for LinuxKeyCapture {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyCapture for LinuxKeyCapture {
    fn capture(&mut self) -> Result<CaptureActivationResponse, String> {
        capture_activation_key_blocking()
    }
}

fn parse_ev_key_line(line: &str) -> Option<(u16, i32)> {
    if !line.contains("EV_KEY") {
        return None;
    }
    let code_tail = line.split("code ").nth(1)?;
    let code_str = code_tail.split_whitespace().next()?;
    let code = code_str.parse::<u16>().ok()?;
    let value_tail = line.split("value ").nth(1)?;
    let value_raw = value_tail
        .trim()
        .split(|c: char| c.is_whitespace() || c == ',')
        .next()?;
    let value = value_raw.parse::<i32>().ok()?;
    Some((code, value))
}

fn capture_evdev_press(timeout: Duration) -> Result<u16, String> {
    let devices = crate::key_listener::list_keyboard_devices()
        .map_err(|e| format!("keyboard devices: {}", e))?;
    capture_evdev_press_with_devices(devices, timeout, |path| {
        let child = Command::new("evtest")
            .arg(path)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        Ok(Box::new(RealEvtestChild(child)) as Box<dyn EvtestChild>)
    })
}

trait EvtestChild: Send {
    fn take_stdout(&mut self) -> Option<Box<dyn Read + Send>>;
    fn kill(&mut self) -> std::io::Result<()>;
    fn wait(&mut self) -> std::io::Result<()>;
}

struct RealEvtestChild(Child);

impl EvtestChild for RealEvtestChild {
    fn take_stdout(&mut self) -> Option<Box<dyn Read + Send>> {
        self.0
            .stdout
            .take()
            .map(|stdout| Box::new(stdout) as Box<dyn Read + Send>)
    }

    fn kill(&mut self) -> std::io::Result<()> {
        self.0.kill()
    }

    fn wait(&mut self) -> std::io::Result<()> {
        self.0.wait().map(|_| ())
    }
}

fn capture_evdev_press_with_devices<F>(
    devices: Vec<PathBuf>,
    timeout: Duration,
    mut spawn_evtest: F,
) -> Result<u16, String>
where
    F: FnMut(&std::path::Path) -> std::io::Result<Box<dyn EvtestChild>>,
{
    if devices.is_empty() {
        return Err("未找到键盘设备（/dev/input）".into());
    }

    let (tx, rx) = mpsc::sync_channel::<u16>(1);
    let deadline = Instant::now() + timeout;
    let mut children = Vec::new();
    let mut readers = Vec::new();

    for device in devices {
        let tx = tx.clone();
        let mut child = match spawn_evtest(&device) {
            Ok(child) => child,
            Err(_) => continue,
        };
        let stdout = match child.take_stdout() {
            Some(stdout) => stdout,
            None => {
                let _ = child.kill();
                let _ = child.wait();
                continue;
            }
        };
        children.push(child);
        readers.push(thread::spawn(move || {
            let reader = std::io::BufReader::new(stdout);
            for line in reader.lines().map_while(Result::ok) {
                let Some((code, value)) = parse_ev_key_line(&line) else {
                    continue;
                };
                if value == 1 && tx.try_send(code).is_ok() {
                    return;
                }
            }
        }));
    }

    drop(tx);
    let remaining = deadline.saturating_duration_since(Instant::now());
    let result = rx.recv_timeout(remaining).map_err(|_| {
        "超时：未检测到按键（请确认对 /dev/input 有读权限，如在 input 组）".to_string()
    });

    for mut child in children {
        let _ = child.kill();
        let _ = child.wait();
    }
    for reader in readers {
        let _ = reader.join();
    }

    result
}

fn capture_activation_key_blocking() -> Result<CaptureActivationResponse, String> {
    let code = capture_evdev_press(CAPTURE_TIMEOUT)?;
    let key_name = evdev_code_to_keysym_name(code);
    Ok(CaptureActivationResponse {
        key_name,
        linux_evdev_code: Some(code),
        windows_vk_code: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ev_key_line_extracts_code_and_value() {
        let line = "Event: time 1234.567, type 1 (EV_KEY), code 56 (KEY_LEFTALT), value 1";
        let (code, value) = parse_ev_key_line(line).unwrap();
        assert_eq!(code, 56);
        assert_eq!(value, 1);
    }

    #[test]
    fn parse_ev_key_line_release() {
        let line = "Event: time 1234.568, type 1 (EV_KEY), code 56 (KEY_LEFTALT), value 0";
        let (code, value) = parse_ev_key_line(line).unwrap();
        assert_eq!(code, 56);
        assert_eq!(value, 0);
    }

    #[test]
    fn parse_ev_key_line_non_ev_key_returns_none() {
        let line = "Event: time 1234.567, type 3 (EV_ABS), code 0 (ABS_X), value 128";
        assert!(parse_ev_key_line(line).is_none());
    }

    #[test]
    fn capture_success_reaps_evtest_child() {
        let calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let spawned_calls = calls.clone();
        let result = capture_evdev_press_with_devices(
            vec![PathBuf::from("/controlled/event0")],
            Duration::from_secs(1),
            |_| {
                Ok(Box::new(TestEvtestChild::new(
                    b"Event: type 1 (EV_KEY), code 56 (KEY_LEFTALT), value 1\n".to_vec(),
                    spawned_calls.clone(),
                )))
            },
        );

        assert_eq!(result.unwrap(), 56);
        assert_eq!(
            *calls.lock().unwrap(),
            vec![TestChildCall::Kill, TestChildCall::Wait]
        );
    }

    #[test]
    fn capture_timeout_reaps_evtest_child() {
        let calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let spawned_calls = calls.clone();
        let result = capture_evdev_press_with_devices(
            vec![PathBuf::from("/controlled/event0")],
            Duration::from_millis(10),
            |_| {
                Ok(Box::new(TestEvtestChild::new(
                    Vec::new(),
                    spawned_calls.clone(),
                )))
            },
        );

        assert!(result.is_err());
        assert_eq!(
            *calls.lock().unwrap(),
            vec![TestChildCall::Kill, TestChildCall::Wait]
        );
    }

    #[test]
    fn capture_missing_stdout_reaps_evtest_child() {
        let calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let spawned_calls = calls.clone();
        let result = capture_evdev_press_with_devices(
            vec![PathBuf::from("/controlled/event0")],
            Duration::from_secs(1),
            |_| {
                Ok(Box::new(TestEvtestChild::without_stdout(
                    spawned_calls.clone(),
                )))
            },
        );

        assert!(result.is_err());
        assert_eq!(
            *calls.lock().unwrap(),
            vec![TestChildCall::Kill, TestChildCall::Wait]
        );
    }

    #[derive(Debug, PartialEq)]
    enum TestChildCall {
        Kill,
        Wait,
    }

    struct TestEvtestChild {
        stdout: Option<std::io::Cursor<Vec<u8>>>,
        calls: std::sync::Arc<std::sync::Mutex<Vec<TestChildCall>>>,
    }

    impl TestEvtestChild {
        fn new(
            stdout: Vec<u8>,
            calls: std::sync::Arc<std::sync::Mutex<Vec<TestChildCall>>>,
        ) -> Self {
            Self {
                stdout: Some(std::io::Cursor::new(stdout)),
                calls,
            }
        }

        fn without_stdout(calls: std::sync::Arc<std::sync::Mutex<Vec<TestChildCall>>>) -> Self {
            Self {
                stdout: None,
                calls,
            }
        }
    }

    impl EvtestChild for TestEvtestChild {
        fn take_stdout(&mut self) -> Option<Box<dyn Read + Send>> {
            self.stdout
                .take()
                .map(|stdout| Box::new(stdout) as Box<dyn Read + Send>)
        }

        fn kill(&mut self) -> std::io::Result<()> {
            self.calls.lock().unwrap().push(TestChildCall::Kill);
            Ok(())
        }

        fn wait(&mut self) -> std::io::Result<()> {
            self.calls.lock().unwrap().push(TestChildCall::Wait);
            Ok(())
        }
    }
}
