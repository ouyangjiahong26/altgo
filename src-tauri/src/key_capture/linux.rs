//! Linux 激活键捕获（evtest）。
//!
//! 通过 `evtest` 监听 `/dev/input/event*`，阻塞等待一次按键。

use std::io::{BufRead, Read};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use super::{evdev_code_to_keysym_name, CaptureActivationResponse, CaptureOutcome, KeyCapture};

/// Alt 键的 evdev keycode（与 `linux/input-event-codes.h` 一致）。
const EVDEV_KEY_ALT: u16 = 56; // KEY_LEFTALT
const EVDEV_KEY_ALT_R: u16 = 100; // KEY_RIGHTALT

/// evtest 路径实际用于匹配的 evdev 码：优先“按下以设置”捕获到的值，
/// 否则按按键名映射，名字表认不出的键名回退到右 Alt 并记一条告警。
///
/// 监听器过滤与自检比对共用这一个函数，两处各解析一遍就会漂移，
/// 也会让自检报出一个与监听器实际行为不符的结论。
pub fn configured_match_code(key_name: &str, captured_code: Option<u16>) -> u16 {
    if let Some(code) = captured_code {
        return code;
    }
    match key_name {
        "Alt_L" => EVDEV_KEY_ALT,
        "Alt_R" | "ISO_Level3_Shift" | "AltGr" => EVDEV_KEY_ALT_R,
        _ => {
            tracing::warn!(
                key_name,
                "key_name not mapped for evtest fallback, defaulting to KEY_RIGHTALT"
            );
            EVDEV_KEY_ALT_R
        }
    }
}

/// 超时未检测到按键时给用户的提示。Linux 上最常见的原因是读不到输入设备节点。
pub fn capture_timeout_message() -> &'static str {
    "超时：未检测到按键（请确认对 /dev/input 有读权限，如在 input 组）"
}

/// 当前配置下监听器在 evtest 路径上实际用于匹配的键码。
pub fn configured_match_code_from_config(cfg: &crate::config::KeyListenerConfig) -> Option<u16> {
    Some(configured_match_code(&cfg.key_name, cfg.linux_evdev_code))
}

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
    fn capture(&mut self, timeout: Duration) -> Result<CaptureOutcome, String> {
        capture_activation_key_blocking(timeout)
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

fn capture_evdev_press(timeout: Duration) -> Result<Option<u16>, String> {
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

/// 为每个键盘设备启动 evtest，收集子进程与 stdout 读取线程。
///
/// spawn 失败或取不到 stdout 的设备跳过，取不到 stdout 的子进程当场回收。
fn spawn_evtest_readers<F>(
    devices: Vec<PathBuf>,
    tx: &mpsc::SyncSender<u16>,
    spawn_evtest: &mut F,
) -> (Vec<Box<dyn EvtestChild>>, Vec<thread::JoinHandle<()>>)
where
    F: FnMut(&std::path::Path) -> std::io::Result<Box<dyn EvtestChild>>,
{
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

    (children, readers)
}

/// 回收全部 evtest 子进程并等待读取线程退出，保证捕获结束后不留残留进程。
fn reap_evtest_children(children: Vec<Box<dyn EvtestChild>>, readers: Vec<thread::JoinHandle<()>>) {
    for mut child in children {
        let _ = child.kill();
        let _ = child.wait();
    }
    for reader in readers {
        let _ = reader.join();
    }
}

fn capture_evdev_press_with_devices<F>(
    devices: Vec<PathBuf>,
    timeout: Duration,
    mut spawn_evtest: F,
) -> Result<Option<u16>, String>
where
    F: FnMut(&std::path::Path) -> std::io::Result<Box<dyn EvtestChild>>,
{
    if devices.is_empty() {
        return Err("未找到键盘设备（/dev/input）".into());
    }

    let (tx, rx) = mpsc::sync_channel::<u16>(1);
    let deadline = Instant::now() + timeout;
    let (children, readers) = spawn_evtest_readers(devices, &tx, &mut spawn_evtest);

    drop(tx);
    let remaining = deadline.saturating_duration_since(Instant::now());
    // 超时与“设备都不出键”（接收端断开）都是没抓到按键，对调用方是同一件事。
    let pressed = rx.recv_timeout(remaining).ok();

    reap_evtest_children(children, readers);

    Ok(pressed)
}

fn capture_activation_key_blocking(timeout: Duration) -> Result<CaptureOutcome, String> {
    let Some(code) = capture_evdev_press(timeout)? else {
        return Ok(CaptureOutcome::TimedOut);
    };
    let key_name = evdev_code_to_keysym_name(code);
    Ok(CaptureOutcome::Captured(CaptureActivationResponse {
        key_name,
        linux_evdev_code: Some(code),
        windows_vk_code: None,
    }))
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

    /// 字面量断言：键名映射改错时这两条要能直接指出是左边还是右边错了。
    #[test]
    fn configured_match_code_maps_alt_names() {
        assert_eq!(configured_match_code("Alt_L", None), 56);
        assert_eq!(configured_match_code("Alt_R", None), 100);
        assert_eq!(configured_match_code("ISO_Level3_Shift", None), 100);
        // 认不出的键名回退右 Alt，与监听器过滤行为一致。
        assert_eq!(configured_match_code("F9", None), 100);
    }

    /// “按下以设置”捕获到的码优先于键名映射，否则改键后自检会拿错码比对。
    #[test]
    fn configured_match_code_prefers_captured_code() {
        assert_eq!(configured_match_code("Alt_R", Some(0x29)), 0x29);
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

        assert_eq!(result.unwrap(), Some(56));
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

        assert!(result.unwrap().is_none());
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

        // 设备一个键都不出：对调用方与超时同义，仍要回收子进程。
        assert!(result.unwrap().is_none());
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
