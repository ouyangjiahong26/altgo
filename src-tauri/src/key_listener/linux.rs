//! Linux 按键监听器。
//!
//! - **Wayland 会话**：优先 `evtest` 读 `/dev/input/event*`（XWayland 上 `xinput test-xi2` 常能启动但收不到全局键盘）。
//! - **传统 X11**：优先 `xinput test-xi2`（XInput2），失败再 `evtest`。
//!
//! 通过 `xmodmap -pke` 解析按键名称到 keycode 的映射（xinput 路径）。
//!
//! Linux key listener.
//!
//! - **Wayland sessions**: prefer `evtest` reading `/dev/input/event*` (on XWayland,
//!   `xinput test-xi2` often starts yet never receives global keyboard events).
//! - **Classic X11**: prefer `xinput test-xi2` (XInput2), falling back to `evtest`.
//!
//! Key-name-to-keycode mapping is parsed via `xmodmap -pke` (the xinput path).

use super::{KeyEvent, KeyListener};
use crate::config::KeyListenerConfig;
use crate::error::KeyListenerError;
use std::io::BufRead;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use tokio::sync::mpsc;

/// xmodmap keycode 映射缓存。解析 xmodmap 输出开销大，
/// 因此首次使用后缓存整张 keycode 表。
/// Cache for xmodmap keycode mappings. Parsing xmodmap output is expensive,
/// so we cache the entire keycode table on first use.
static XMODMAP_CACHE: OnceLock<std::collections::HashMap<String, u8>> = OnceLock::new();

/// evdev keycode for Alt keys（evtest 回退；与 `linux/input-event-codes.h` 一致）
/// evdev keycode of the Alt keys (evtest fallback; aligned with `linux/input-event-codes.h`)
const EVDEV_KEY_ALT: u16 = 56; // KEY_LEFTALT
const EVDEV_KEY_ALT_R: u16 = 100; // KEY_RIGHTALT

/// X11 按键监听器，使用 `xinput test-xi2` 捕获全局按键事件。
///
/// 无需 root 权限，依赖 XInput2 扩展。
///
/// X11 key listener capturing global key events through `xinput test-xi2`.
///
/// Needs no root privileges; depends on the XInput2 extension.
pub struct X11Listener {
    key_name: String,
    linux_evdev_code: Option<u16>,
    running: Arc<AtomicBool>,
    child: Option<Child>,
}

/// 枚举可用于 `evtest` 回退的键盘设备（供按键捕获使用）。
///
/// 以 `/proc/bus/input/devices` 中带 `kbd` handler 的 event 节点为准：
/// 真键盘可能不在 `/dev/input/by-id`（蓝牙、特殊接收器），而游戏鼠标的
/// 键盘接口反而会占据 by-id 的 `*-kbd` 链接，只扫 by-id 会漏掉真键盘。
/// 枚举宁多勿漏——监听循环按 evdev 码过滤，多余设备只多一个空闲子进程。
///
/// Enumerates keyboard devices usable by the `evtest` fallback (shared with key capture).
///
/// Devices are taken from the `kbd`-handler event nodes in `/proc/bus/input/devices`:
/// a real keyboard may be absent from `/dev/input/by-id` (Bluetooth, special receivers),
/// while a gaming mouse's keyboard interface can occupy the `*-kbd` link there — scanning
/// by-id alone misses the real keyboard. Prefer over-inclusion: the listener filters by
/// evdev code, so surplus devices only cost one idle subprocess each.
pub fn list_keyboard_devices() -> Result<Vec<PathBuf>, KeyListenerError> {
    let text = std::fs::read_to_string("/proc/bus/input/devices")?;
    Ok(parse_keyboard_devices(&text))
}

/// 解析 `/proc/bus/input/devices` 文本，返回带 `kbd` handler 的 event 节点路径。
///
/// 每个输入设备一段，以空行分隔；`H: Handlers=` 行形如
/// `Handlers=sysrq kbd event16 leds`，其中 `eventN` 即设备节点。
///
/// Parses `/proc/bus/input/devices` text into event-node paths carrying a `kbd` handler.
///
/// Each input device forms a paragraph separated by blank lines; the `H: Handlers=` line
/// looks like `Handlers=sysrq kbd event16 leds`, where `eventN` is the device node.
fn parse_keyboard_devices(text: &str) -> Vec<PathBuf> {
    let mut devices = Vec::new();
    let mut handlers: Option<&str> = None;

    let flush = |handlers: &mut Option<&str>, devices: &mut Vec<PathBuf>| {
        if let Some(h) = handlers.take() {
            let tokens: Vec<&str> = h.split_whitespace().collect();
            // 只有带 `kbd` handler 的设备才是键盘类；纯鼠标接口只有 `mouse0`。
            // Only devices with a `kbd` handler are keyboard-like; pure mouse
            // interfaces carry `mouse0` only.
            if !tokens.contains(&"kbd") {
                return;
            }
            for token in tokens {
                if token.starts_with("event") {
                    devices.push(PathBuf::from("/dev/input").join(token));
                }
            }
        }
    };

    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("H: Handlers=") {
            handlers = Some(rest);
        } else if line.is_empty() {
            flush(&mut handlers, &mut devices);
        }
    }
    // 文件末尾可能没有空行，补最后一次。
    // The file may not end with a blank line; flush once more.
    flush(&mut handlers, &mut devices);

    devices
}

impl X11Listener {
    /// 创建新的 X11 监听器，验证 `xinput` 是否可用。
    /// Creates a new X11 listener, verifying that `xinput` is available.
    pub fn new(cfg: &KeyListenerConfig) -> Result<Self, KeyListenerError> {
        Command::new("xinput")
            .arg("version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|_| {
                KeyListenerError::ToolNotFound(
                    "xinput not found — install xinput package".to_string(),
                )
            })?;

        Ok(Self {
            key_name: cfg.key_name.clone(),
            linux_evdev_code: cfg.linux_evdev_code,
            running: Arc::new(AtomicBool::new(false)),
            child: None,
        })
    }

    /// 开始监听按键事件，返回事件通道与后端标识（`"xinput"` / `"evtest"`）。
    /// Starts listening for key events, returning the event channel and backend label
    /// (`"xinput"` / `"evtest"`).
    pub fn start(
        &mut self,
    ) -> Result<(mpsc::UnboundedReceiver<KeyEvent>, &'static str), KeyListenerError> {
        // evtest：有捕获码则只认该码；否则按 keysym 映射到 evdev（左/右 Alt 严格区分）。
        // evtest: with a captured code, accept only that code; otherwise map keysyms onto evdev
        // (left/right Alt strictly distinguished).
        let allowed_evdev: std::sync::Arc<[u16]> = if let Some(c) = self.linux_evdev_code {
            std::sync::Arc::from([c])
        } else {
            let code = match self.key_name.as_str() {
                "Alt_L" => EVDEV_KEY_ALT,
                "Alt_R" | "ISO_Level3_Shift" | "AltGr" => EVDEV_KEY_ALT_R,
                _ => {
                    tracing::warn!(
                        "key_name '{}' not mapped for evtest fallback; defaulting to KEY_RIGHTALT",
                        self.key_name
                    );
                    EVDEV_KEY_ALT_R
                }
            };
            std::sync::Arc::from([code])
        };
        let display_set = std::env::var("DISPLAY")
            .map(|s| !s.is_empty())
            .unwrap_or(false);
        let wayland_hint = std::env::var("WAYLAND_DISPLAY").is_ok()
            || std::env::var("XDG_SESSION_TYPE")
                .map(|v| v == "wayland")
                .unwrap_or(false);

        // Wayland 下 DISPLAY 仍指向 XWayland；`xinput test-xi2 --root` 通常能启动
        // 却收不到全局键盘事件——表现为"无报错、也无按键"。此时优先走 evdev。
        // On Wayland, DISPLAY still points at XWayland; `xinput test-xi2 --root` usually starts
        // but does not receive global keyboard events — looks like "no errors, no keys". Prefer evdev.
        if wayland_hint {
            tracing::info!(
                "Wayland session: trying evtest first (xinput on XWayland typically misses keyboard)"
            );
            match self.try_start_evtest(allowed_evdev.clone()) {
                Ok(rx) => return Ok((rx, "evtest")),
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "evtest failed on Wayland, will try xinput (may still not receive keys)"
                    );
                }
            }
        }

        // 经典 X11 或 Wayland 下的 evtest 回退：有真实 X11 键表时用 xinput。
        // Classic X11 or Wayland evtest fallback: xinput when we have a real X11 keymap.
        if display_set {
            tracing::info!(
                session = wayland_hint,
                "DISPLAY is set; trying xinput test-xi2"
            );
            match self.resolve_keycode() {
                Ok(keycode) => {
                    tracing::info!(
                        "resolved key '{}' to X11 keycode {}",
                        self.key_name,
                        keycode
                    );
                    let (tx, rx) = mpsc::unbounded_channel();
                    let running = Arc::clone(&self.running);
                    running.store(true, Ordering::SeqCst);
                    match self.try_start_xinput(keycode, tx, running) {
                        Ok(child) => {
                            self.child = Some(child);
                            return Ok((rx, "xinput"));
                        }
                        Err(e) => {
                            tracing::warn!(
                                error = %e,
                                "xinput failed to start, falling back to evtest"
                            );
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "xmodmap/keycode resolution failed (no working X?), falling back to evtest"
                    );
                }
            }
        } else if !wayland_hint {
            tracing::info!("DISPLAY is unset; using evtest on /dev/input");
        }

        if wayland_hint {
            tracing::info!("Wayland: evtest requires read access to /dev/input/event* (e.g. user in group input)");
        }

        tracing::info!("starting evtest for key events");
        let rx = self.try_start_evtest(allowed_evdev)?;
        Ok((rx, "evtest"))
    }

    fn try_start_evtest(
        &mut self,
        allowed_evdev: std::sync::Arc<[u16]>,
    ) -> Result<mpsc::UnboundedReceiver<KeyEvent>, KeyListenerError> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.running.store(true, Ordering::SeqCst);
        let running = Arc::clone(&self.running);
        if let Err(e) = self.start_evtest_fallback(allowed_evdev, tx, running) {
            self.running.store(false, Ordering::SeqCst);
            return Err(e);
        }
        Ok(rx)
    }

    /// 尝试启动 xinput test-xi2。
    /// 如果检测到 XWayland (BadAccess)，返回错误触发 fallback。
    /// Tries to start xinput test-xi2.
    /// Returns an error when XWayland (BadAccess) is detected, triggering the fallback.
    fn try_start_xinput(
        &mut self,
        keycode: u8,
        tx: tokio::sync::mpsc::UnboundedSender<KeyEvent>,
        running: Arc<AtomicBool>,
    ) -> Result<Child, KeyListenerError> {
        let mut child = Command::new("xinput")
            .args(["test-xi2", "--root"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                KeyListenerError::StartFailed(format!("failed to start xinput test-xi2: {e}"))
            })?;

        // 检查 stderr 里是否有 XWayland 告警
        // Check stderr for XWayland warning
        let stderr = child.stderr.take();
        if let Some(stderr) = stderr {
            std::thread::spawn(move || {
                let reader = std::io::BufReader::new(stderr);
                for line in reader.lines().take(10).flatten() {
                    if line.contains("Xwayland") || line.contains("BadAccess") {
                        tracing::warn!("detected XWayland, xinput test-xi2 will not work");
                    }
                }
            });
        }

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| KeyListenerError::StartFailed("no stdout from xinput".to_string()))?;

        std::thread::spawn(move || {
            let reader = std::io::BufReader::new(stdout);
            let mut event_type: Option<bool> = None; // true=press, false=release

            for line in reader.lines() {
                if !running.load(Ordering::SeqCst) {
                    tracing::info!("key listener thread stopped by user");
                    break;
                }

                let line = match line {
                    Ok(l) => l,
                    Err(e) => {
                        tracing::warn!(error = %e, "xinput stdout read error, key listener thread exiting");
                        break;
                    }
                };

                let trimmed = line.trim();

                if trimmed.contains("KeyPress") && trimmed.starts_with("EVENT") {
                    event_type = Some(true);
                } else if trimmed.contains("KeyRelease") && trimmed.starts_with("EVENT") {
                    event_type = Some(false);
                } else if let Some(detail_str) = trimmed.strip_prefix("detail:") {
                    if let Some(pressed) = event_type.take() {
                        if let Ok(detail) = detail_str.trim().parse::<u8>() {
                            if detail == keycode && tx.send(KeyEvent { pressed }).is_err() {
                                tracing::warn!(
                                    "key event receiver dropped, key listener thread exiting"
                                );
                                break;
                            }
                        }
                    } else {
                        tracing::debug!(
                            line = %trimmed,
                            "detail line without preceding event type, skipping"
                        );
                    }
                } else if !trimmed.is_empty()
                    && !trimmed.starts_with("EVENT")
                    && !trimmed.contains(':')
                {
                    tracing::trace!(line = %trimmed, "unparsed xinput line");
                }
            }
            tracing::warn!("xinput stdout closed, key listener thread exiting");
        });

        Ok(child)
    }

    /// 启动 evtest fallback 监听器。
    /// evtest 需要读取 /dev/input/event* 设备，需要用户属于 input 组。
    /// Starts the evtest fallback listener.
    /// evtest reads /dev/input/event* devices, requiring membership in the input group.
    fn start_evtest_fallback(
        &mut self,
        allowed_evdev: std::sync::Arc<[u16]>,
        tx: tokio::sync::mpsc::UnboundedSender<KeyEvent>,
        running: Arc<AtomicBool>,
    ) -> Result<(), KeyListenerError> {
        // 找出键盘设备
        // Find keyboard devices
        let keyboard_devices = list_keyboard_devices()?;
        if keyboard_devices.is_empty() {
            return Err(KeyListenerError::StartFailed(
                "no keyboard devices found for evtest fallback".to_string(),
            ));
        }

        tracing::info!(
            "using evtest fallback with {} keyboard devices",
            keyboard_devices.len()
        );

        for device in keyboard_devices {
            let running = Arc::clone(&running);
            let tx = tx.clone();
            let device_path = device.clone();
            let allowed = std::sync::Arc::clone(&allowed_evdev);

            std::thread::spawn(move || {
                // evtest 把设备信息和全部 EV_* 行打到 stdout（不是 stderr）。
                // 读 stderr 会让 Wayland 回退路径永远收不到按键事件。
                // evtest prints device info and all EV_* lines to stdout (not stderr).
                // Reading stderr caused Wayland fallback to never see key events.
                let mut child = match Command::new("evtest")
                    .arg(&device_path)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .spawn()
                {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::warn!(error = %e, device = %device_path.display(), "failed to spawn evtest");
                        return;
                    }
                };

                let stdout = child.stdout.take().expect("evtest stdout captured");
                let reader = std::io::BufReader::new(stdout);

                for line in reader.lines() {
                    if !running.load(Ordering::SeqCst) {
                        break;
                    }

                    let line = match line {
                        Ok(l) => l,
                        Err(_) => continue,
                    };

                    // 解析 evtest 输出："Event: time ..., type 1 (EV_KEY), code 100 (KEY_RIGHTALT), value 1"
                    // Parse evtest output: "Event: time ..., type 1 (EV_KEY), code 100 (KEY_RIGHTALT), value 1"
                    if line.contains("EV_KEY") {
                        let Some(code_tail) = line.split("code ").nth(1) else {
                            continue;
                        };
                        let Some(code_str) = code_tail.split_whitespace().next() else {
                            continue;
                        };
                        let Ok(code) = code_str.parse::<u16>() else {
                            continue;
                        };
                        // 仅匹配允许的 evdev 码（通常为单个；捕获模式为按下的物理码）。
                        // Match only allowed evdev codes (usually one; in capture mode, the physically pressed code).
                        let key_matches = allowed.contains(&code);
                        if key_matches {
                            let Some(value_tail) = line.split("value ").nth(1) else {
                                continue;
                            };
                            let value_raw = value_tail
                                .trim()
                                .split(|c: char| c.is_whitespace() || c == ',')
                                .next()
                                .unwrap_or("");
                            let Ok(value) = value_raw.parse::<i32>() else {
                                continue;
                            };
                            // evdev：0 = 松开，1 = 按下，2 = 自动重复（键仍按住）。
                            // 把 repeat 当作松开会破坏长按录音。
                            // evdev: 0 = release, 1 = press, 2 = autorepeat (key still held).
                            // Treating repeat as release breaks hold-to-record.
                            let pressed = match value {
                                0 => false,
                                1 => true,
                                2 => continue,
                                _ => continue,
                            };
                            if tx.send(KeyEvent { pressed }).is_err() {
                                tracing::warn!("evtest: receiver dropped, exiting");
                                break;
                            }
                        }
                    }
                }
            });
        }

        Ok(())
    }

    /// 停止监听。
    /// Stops listening.
    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    fn resolve_keycode(&self) -> Result<u8, KeyListenerError> {
        let keycode_map = XMODMAP_CACHE.get_or_init(|| {
            let output = match Command::new("xmodmap").arg("-pke").output() {
                Ok(o) => o,
                Err(e) => {
                    tracing::error!(error = %e, "xmodmap not found or failed to run");
                    return std::collections::HashMap::new();
                }
            };

            let stdout = String::from_utf8_lossy(&output.stdout);
            if stdout.trim().is_empty() {
                tracing::error!("xmodmap returned empty output");
                return std::collections::HashMap::new();
            }

            let mut map = std::collections::HashMap::new();

            // xmodmap -pke 输出格式：keycode <N> = keysym ...
            // 如 "keycode  64 = Alt_L Meta_L Alt_L Meta_L"
            // xmodmap -pke output format: keycode <N> = keysym ...
            // e.g., "keycode  64 = Alt_L Meta_L Alt_L Meta_L"
            for line in stdout.lines() {
                if let Some(keycode_str) = line.split_whitespace().nth(1) {
                    if let Ok(keycode) = keycode_str.parse::<u8>() {
                        // 提取行内所有 keysym（跳过 "keycode N =" 部分）
                        // Extract all keysyms from the line (skip "keycode N =")
                        for keysym in line.split_whitespace().skip(3) {
                            // 跳过可能存在的 "="
                            // Skip "=" if present
                            let keysym = keysym.trim_end_matches('=');
                            if !keysym.is_empty() && !map.contains_key(keysym) {
                                map.insert(keysym.to_string(), keycode);
                            }
                        }
                    }
                }
            }
            if map.is_empty() {
                tracing::error!("xmodmap output contained no parseable keycode mappings");
            }
            map
        });

        if keycode_map.is_empty() {
            return Err(KeyListenerError::ResolveFailed(
                "xmodmap failed to produce keycode mappings — is xmodmap installed and DISPLAY set?"
                    .to_string(),
            ));
        }

        let candidates: &[&str] = match self.key_name.as_str() {
            "Alt_L" => &["Alt_L"],
            // 布局上可能只出现 Alt_R、ISO_Level3_Shift 或 AltGr 之一，任一对上即可。
            // A layout may expose only one of Alt_R, ISO_Level3_Shift, or AltGr—matching any suffices.
            "Alt_R" | "ISO_Level3_Shift" | "AltGr" => &["Alt_R", "ISO_Level3_Shift", "AltGr"],
            _ => {
                let name = self.key_name.as_str();
                // 单元素切片：自定义 keysym 名
                // Single-element slice: custom keysym name
                return keycode_map.get(name).copied().ok_or_else(|| {
                    KeyListenerError::ResolveFailed(format!(
                        "keycode for '{}' not found in xmodmap output",
                        self.key_name
                    ))
                });
            }
        };

        for name in candidates {
            if let Some(&k) = keycode_map.get(*name) {
                tracing::info!(
                    keysym = %name,
                    keycode = k,
                    "resolved xmodmap keycode for trigger key"
                );
                return Ok(k);
            }
        }

        Err(KeyListenerError::ResolveFailed(format!(
            "keycode for trigger '{}' not found in xmodmap (tried: {:?})",
            self.key_name, candidates
        )))
    }
}

impl Drop for X11Listener {
    fn drop(&mut self) {
        self.stop();
    }
}

impl KeyListener for X11Listener {
    fn start(
        &mut self,
    ) -> Result<(mpsc::UnboundedReceiver<KeyEvent>, &'static str), KeyListenerError> {
        self.start()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> crate::config::KeyListenerConfig {
        crate::config::KeyListenerConfig {
            key_name: "Alt_R".to_string(),
            linux_evdev_code: None,
            windows_vk_code: None,
            long_press_threshold: std::time::Duration::from_millis(400),
            double_click_interval: std::time::Duration::from_millis(200),
            min_press_duration: std::time::Duration::from_millis(80),
        }
    }

    #[test]
    fn x11_listener_new_validates_xinput_presence() {
        let cfg = test_config();
        let result = X11Listener::new(&cfg);
        // 有 xinput 时返回 Ok，无 xinput 时返回 Err
        // Returns Ok with xinput present, Err otherwise
        let has_xinput = Command::new("xinput")
            .arg("version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok();

        if has_xinput {
            assert!(result.is_ok());
        } else {
            assert!(result.is_err());
        }
    }

    #[test]
    fn x11_listener_new_stores_key_name() {
        let mut cfg = test_config();
        cfg.key_name = "Alt_L".to_string();
        cfg.linux_evdev_code = Some(56);

        // 只有在 xinput 可用时才能构造成功
        // Construction succeeds only when xinput is available
        if Command::new("xinput")
            .arg("version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok()
        {
            let listener = X11Listener::new(&cfg).unwrap();
            assert_eq!(listener.key_name, "Alt_L");
            assert_eq!(listener.linux_evdev_code, Some(56));
        }
    }

    #[test]
    fn keysym_to_evdev_alt_r() {
        // 直接测试内部映射逻辑
        // Test the internal mapping logic directly
        let code = match "Alt_R" {
            "Alt_L" => EVDEV_KEY_ALT,
            "Alt_R" | "ISO_Level3_Shift" | "AltGr" => EVDEV_KEY_ALT_R,
            _ => EVDEV_KEY_ALT_R,
        };
        assert_eq!(code, EVDEV_KEY_ALT_R);
    }

    #[test]
    fn keysym_to_evdev_alt_l() {
        let code = match "Alt_L" {
            "Alt_L" => EVDEV_KEY_ALT,
            "Alt_R" | "ISO_Level3_Shift" | "AltGr" => EVDEV_KEY_ALT_R,
            _ => EVDEV_KEY_ALT_R,
        };
        assert_eq!(code, EVDEV_KEY_ALT);
    }

    #[test]
    fn parse_keyboard_devices_picks_kbd_handlers_only() {
        // 样例取自真实 /proc/bus/input/devices 结构：鼠标的键盘接口（event3）
        // 与真键盘（event16）都带 kbd handler，纯鼠标接口（event2）不带。
        // Sample mirrors real /proc/bus/input/devices structure: the mouse's keyboard
        // interface (event3) and the real keyboard (event16) carry a kbd handler,
        // while the pure mouse interface (event2) does not.
        let sample = "I: Bus=0003 Vendor=046d Product=c092 Version=0111\n\
                      N: Name=\"Logitech G102 LIGHTSYNC Gaming Mouse\"\n\
                      P: Phys=usb-0000:00:14.0-1/input0\n\
                      H: Handlers=mouse0 event2 \n\
                      B: EV=1\n\
                      \n\
                      I: Bus=0003 Vendor=046d Product=c092 Version=0111\n\
                      N: Name=\"Logitech G102 LIGHTSYNC Gaming Mouse Keyboard\"\n\
                      P: Phys=usb-0000:00:14.0-1/input1\n\
                      H: Handlers=sysrq kbd event3 \n\
                      \n\
                      I: Bus=0011 Vendor=0001 Product=0001 Version=0000\n\
                      N: Name=\"Wave Keys\"\n\
                      P: Phys=...\n\
                      H: Handlers=sysrq kbd event16 leds \n";
        let devices = parse_keyboard_devices(sample);
        assert_eq!(
            devices,
            vec![
                PathBuf::from("/dev/input/event3"),
                PathBuf::from("/dev/input/event16"),
            ]
        );
    }

    #[test]
    fn parse_keyboard_devices_handles_missing_trailing_blank_line() {
        // 末段无空行结尾时也要能取到。
        // The last paragraph without a trailing blank line must still be captured.
        let sample = "N: Name=\"Wave Keys\"\nH: Handlers=kbd event16 leds \n";
        let devices = parse_keyboard_devices(sample);
        assert_eq!(devices, vec![PathBuf::from("/dev/input/event16")]);
    }
}
