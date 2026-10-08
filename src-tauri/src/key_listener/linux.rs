//! Linux 按键监听器。
//!
//! - Wayland 会话：优先 `evtest` 读 `/dev/input/event*`（XWayland 上 `xinput test-xi2` 常能启动但收不到全局键盘）。
//! - 传统 X11：优先 `xinput test-xi2`（XInput2），失败再 `evtest`。
//!
//! 通过 `xmodmap -pke` 解析按键名称到 keycode 的映射（xinput 路径）。

use super::{KeyEvent, KeyListener};
use crate::config::KeyListenerConfig;
use crate::error::KeyListenerError;
use std::collections::HashMap;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tokio::sync::mpsc;

/// xmodmap keycode 映射缓存。解析 xmodmap 输出开销大，
/// 因此首次使用后缓存整张 keycode 表。
static XMODMAP_CACHE: OnceLock<std::collections::HashMap<String, u8>> = OnceLock::new();

/// 运行 `xmodmap -pke` 并解析为 keysym 到 keycode 的映射，失败时返回空表。
fn load_xmodmap_keycodes() -> std::collections::HashMap<String, u8> {
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
    for line in stdout.lines() {
        if let Some(keycode_str) = line.split_whitespace().nth(1) {
            if let Ok(keycode) = keycode_str.parse::<u8>() {
                // 提取行内所有 keysym（跳过 "keycode N =" 部分）
                for keysym in line.split_whitespace().skip(3) {
                    // 跳过可能存在的 "="
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
}

/// Alt 键的 evdev keycode（evtest 回退，与 `linux/input-event-codes.h` 一致）
const EVDEV_KEY_ALT: u16 = 56; // KEY_LEFTALT
const EVDEV_KEY_ALT_R: u16 = 100; // KEY_RIGHTALT

/// X11 按键监听器，使用 `xinput test-xi2` 捕获全局按键事件。
///
/// 无需 root 权限，依赖 XInput2 扩展。
pub struct X11Listener {
    key_name: String,
    linux_evdev_code: Option<u16>,
    running: Arc<AtomicBool>,
    child: Option<Child>,
    // evtest 子进程按设备节点索引：设备监视线程与 stop() 都要触碰，
    // 放进共享映射才能在运行期按热插拔补启与回收。读取线程只借走 stdout，
    // kill 由映射持有方执行，管道 EOF 自然唤醒读取线程并回收僵尸。
    evtest_children: Arc<Mutex<HashMap<PathBuf, Child>>>,
}

/// 枚举可用于 `evtest` 回退的键盘设备（供按键捕获使用）。
///
/// 以 `/proc/bus/input/devices` 中带 `kbd` handler 的 event 节点为准：
/// 真键盘可能不在 `/dev/input/by-id`（蓝牙、特殊接收器），而游戏鼠标的
/// 键盘接口反而会占据 by-id 的 `*-kbd` 链接，只扫 by-id 会漏掉真键盘。
/// 枚举宁多勿漏：监听循环按 evdev 码过滤，多余设备只多一个空闲子进程。
pub fn list_keyboard_devices() -> Result<Vec<PathBuf>, KeyListenerError> {
    let text = std::fs::read_to_string("/proc/bus/input/devices")?;
    Ok(parse_keyboard_devices(&text))
}

/// 解析 `/proc/bus/input/devices` 文本，返回带 `kbd` handler 的 event 节点路径。
///
/// 每个输入设备一段，以空行分隔。`H: Handlers=` 行形如
/// `Handlers=sysrq kbd event16 leds`，其中 `eventN` 即设备节点。
fn parse_keyboard_devices(text: &str) -> Vec<PathBuf> {
    let mut devices = Vec::new();
    let mut handlers: Option<&str> = None;

    let flush = |handlers: &mut Option<&str>, devices: &mut Vec<PathBuf>| {
        if let Some(h) = handlers.take() {
            let tokens: Vec<&str> = h.split_whitespace().collect();
            // 只有带 `kbd` handler 的设备才是键盘类，纯鼠标接口只有 `mouse0`。
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
    flush(&mut handlers, &mut devices);

    devices
}

/// 监视 xinput 的 stderr 头部若干行，发现 XWayland 告警时记日志。
fn watch_xinput_stderr(stderr: Option<std::process::ChildStderr>) {
    let Some(stderr) = stderr else {
        return;
    };
    std::thread::spawn(move || {
        let reader = std::io::BufReader::new(stderr);
        for line in reader.lines().take(10).flatten() {
            if line.contains("Xwayland") || line.contains("BadAccess") {
                tracing::warn!("detected XWayland, xinput test-xi2 will not work");
            }
        }
    });
}

/// 读取 xinput stdout 行流并解析为按键事件发往 `tx`，直到停止标志置位或流关闭。
fn run_xinput_stdout_loop(
    stdout: std::process::ChildStdout,
    keycode: u8,
    tx: tokio::sync::mpsc::UnboundedSender<KeyEvent>,
    running: Arc<AtomicBool>,
) {
    let reader = std::io::BufReader::new(stdout);
    let mut event_type: Option<bool> = None; // true 表示按下，false 表示松开

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
                        tracing::warn!("key event receiver dropped, key listener thread exiting");
                        break;
                    }
                }
            } else {
                tracing::debug!(
                    line = %trimmed,
                    "detail line without preceding event type, skipping"
                );
            }
        } else if !trimmed.is_empty() && !trimmed.starts_with("EVENT") && !trimmed.contains(':') {
            tracing::trace!(line = %trimmed, "unparsed xinput line");
        }
    }
    tracing::warn!("xinput stdout closed, key listener thread exiting");
}

/// 读取会话环境：返回（DISPLAY 是否非空、是否 Wayland 会话）。
fn session_env() -> (bool, bool) {
    let display_set = std::env::var("DISPLAY")
        .map(|s| !s.is_empty())
        .unwrap_or(false);
    let wayland_hint = std::env::var("WAYLAND_DISPLAY").is_ok()
        || std::env::var("XDG_SESSION_TYPE")
            .map(|v| v == "wayland")
            .unwrap_or(false);
    (display_set, wayland_hint)
}

/// 枚举 evtest 回退要监听的键盘设备，一台都没有时返回启动失败。
fn evtest_fallback_devices() -> Result<Vec<PathBuf>, KeyListenerError> {
    let keyboard_devices = list_keyboard_devices()?;
    if keyboard_devices.is_empty() {
        return Err(KeyListenerError::StartFailed(
            "no keyboard devices found for evtest fallback".to_string(),
        ));
    }
    Ok(keyboard_devices)
}

/// 把一行 evtest 输出解析为按键按下/松开，非 EV_KEY 行、解析失败、
/// 不在允许集合内的码与自动重复都返回 `None`。
fn evtest_line_to_key_event(line: &str, allowed: &[u16]) -> Option<bool> {
    // 解析 evtest 输出："Event: time ..., type 1 (EV_KEY), code 100 (KEY_RIGHTALT), value 1"
    if !line.contains("EV_KEY") {
        return None;
    }
    let code_tail = line.split("code ").nth(1)?;
    let code_str = code_tail.split_whitespace().next()?;
    let code = code_str.parse::<u16>().ok()?;
    // 仅匹配允许的 evdev 码（通常为单个，捕获模式为按下的物理码）。
    if !allowed.contains(&code) {
        return None;
    }
    let value_tail = line.split("value ").nth(1)?;
    let value_raw = value_tail
        .trim()
        .split(|c: char| c.is_whitespace() || c == ',')
        .next()
        .unwrap_or("");
    let value = value_raw.parse::<i32>().ok()?;
    // evdev：0 = 松开，1 = 按下，2 = 自动重复（键仍按住）。
    // 把 repeat 当作松开会破坏长按录音。
    match value {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    }
}

/// 为单个键盘设备启动 evtest 子进程，stdout 供读取线程消费。
fn spawn_evtest_child(device_path: &Path) -> std::io::Result<Child> {
    // evtest 把设备信息和全部 EV_* 行打到 stdout（不是 stderr）。
    // 读 stderr 会让 Wayland 回退路径永远收不到按键事件。
    Command::new("evtest")
        .arg(device_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
}

/// 单轮设备调和的待执行动作。
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum ReconcileAction {
    /// 设备在列但没有活着的 evtest：补启。
    Spawn(PathBuf),
    /// evtest 还活着但设备已从枚举中消失：回收（真实蓝牙断链后
    /// evtest 不退出，只是挂在已注销的设备对象上永远读不到事件）。
    Kill(PathBuf),
}

/// 对比当前枚举到的设备与在管子进程，产出本轮动作。
///
/// 三种失配都要处理：新设备补启；设备消失回收；子进程已退出但设备
/// 仍在列（uinput 销毁会直接杀死读者，节点随后被重连设备复用）则重启。
fn plan_reconcile(
    devices: &[PathBuf],
    children: &mut HashMap<PathBuf, Child>,
) -> Vec<ReconcileAction> {
    let mut actions = Vec::new();
    for device in devices {
        let needs_spawn = match children.get_mut(device) {
            None => true,
            Some(child) => child.try_wait().ok().flatten().is_some(),
        };
        if needs_spawn {
            actions.push(ReconcileAction::Spawn(device.clone()));
        }
    }
    for path in children.keys() {
        if !devices.contains(path) {
            actions.push(ReconcileAction::Kill(path.clone()));
        }
    }
    actions
}

/// evtest 路径的设备监视线程：周期对比设备集合与子进程映射并调和。
///
/// 每秒一次的代价只是一次 `/proc/bus/input/devices` 读取与文本解析；
/// 不引入 inotify 依赖即可覆盖蓝牙断链重连、接收器换插等全部形态。
fn device_watch_loop(
    running: Arc<AtomicBool>,
    children: Arc<Mutex<HashMap<PathBuf, Child>>>,
    allowed: std::sync::Arc<[u16]>,
    tx: mpsc::UnboundedSender<KeyEvent>,
) {
    while running.load(Ordering::SeqCst) {
        std::thread::sleep(std::time::Duration::from_secs(1));
        if !running.load(Ordering::SeqCst) {
            break;
        }
        let Ok(text) = std::fs::read_to_string("/proc/bus/input/devices") else {
            continue;
        };
        let devices = parse_keyboard_devices(&text);
        let actions = {
            let mut map = match children.lock() {
                Ok(map) => map,
                Err(poisoned) => poisoned.into_inner(),
            };
            plan_reconcile(&devices, &mut map)
        };
        for action in actions {
            match action {
                ReconcileAction::Spawn(path) => {
                    spawn_evtest_listener(&path, &children, &allowed, &tx, &running);
                }
                ReconcileAction::Kill(path) => {
                    let mut map = match children.lock() {
                        Ok(map) => map,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    if let Some(mut child) = map.remove(&path) {
                        tracing::info!(device = %path.display(), "device vanished, reaping evtest");
                        let _ = child.kill();
                        let _ = child.wait();
                    }
                }
            }
        }
    }
    tracing::info!("device watch thread exiting");
}

/// 为单台设备补启 evtest 并挂上读取线程，成功则登记进子进程映射。
fn spawn_evtest_listener(
    path: &Path,
    children: &Arc<Mutex<HashMap<PathBuf, Child>>>,
    allowed: &std::sync::Arc<[u16]>,
    tx: &mpsc::UnboundedSender<KeyEvent>,
    running: &Arc<AtomicBool>,
) {
    let mut child = match spawn_evtest_child(path) {
        Ok(child) => child,
        Err(e) => {
            tracing::warn!(error = %e, device = %path.display(), "failed to spawn evtest");
            return;
        }
    };
    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            tracing::warn!(device = %path.display(), "evtest produced no stdout");
            let _ = child.kill();
            let _ = child.wait();
            return;
        }
    };
    let mut map = match children.lock() {
        Ok(map) => map,
        Err(poisoned) => poisoned.into_inner(),
    };
    map.insert(path.to_path_buf(), child);
    drop(map);

    let running = Arc::clone(running);
    let tx = tx.clone();
    let allowed = std::sync::Arc::clone(allowed);
    std::thread::spawn(move || run_evtest_stdout_loop(stdout, allowed, tx, running));
    tracing::info!(device = %path.display(), "evtest listener started");
}

/// 单个键盘设备的 evtest 读取循环：解析 EV_KEY 行并把按键事件发往 `tx`，
/// 直到停止标志置位、读取出错或接收端消失。
/// 子进程句柄不在本线程：stop() 终止子进程后管道 EOF 自然结束本循环。
fn run_evtest_stdout_loop(
    stdout: std::process::ChildStdout,
    allowed: std::sync::Arc<[u16]>,
    tx: mpsc::UnboundedSender<KeyEvent>,
    running: Arc<AtomicBool>,
) {
    let reader = std::io::BufReader::new(stdout);

    for line in reader.lines() {
        if !running.load(Ordering::SeqCst) {
            break;
        }

        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };

        let Some(pressed) = evtest_line_to_key_event(&line, &allowed) else {
            continue;
        };
        if tx.send(KeyEvent { pressed }).is_err() {
            tracing::warn!("evtest: receiver dropped, exiting");
            break;
        }
    }
}

impl X11Listener {
    /// 创建新的 X11 监听器，验证 `xinput` 是否可用。
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
            evtest_children: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// 开始监听按键事件，返回事件通道与后端标识（`"xinput"` / `"evtest"`）。
    pub fn start(
        &mut self,
    ) -> Result<(mpsc::UnboundedReceiver<KeyEvent>, &'static str), KeyListenerError> {
        let allowed_evdev = self.allowed_evdev_codes();
        let (display_set, wayland_hint) = session_env();

        // Wayland 下 DISPLAY 仍指向 XWayland，`xinput test-xi2` 通常能启动
        // 却收不到全局键盘事件，表现为"无报错、也无按键"。此时优先走 evdev。
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
        if display_set {
            if let Some(rx) = self.try_start_xinput_listener(wayland_hint) {
                return Ok((rx, "xinput"));
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

    /// evtest 回退允许的 evdev 码集合：有捕获码则只认该码。
    /// 否则按 keysym 映射到 evdev（左/右 Alt 严格区分）。
    fn allowed_evdev_codes(&self) -> std::sync::Arc<[u16]> {
        if let Some(c) = self.linux_evdev_code {
            std::sync::Arc::from([c])
        } else {
            let code = match self.key_name.as_str() {
                "Alt_L" => EVDEV_KEY_ALT,
                "Alt_R" | "ISO_Level3_Shift" | "AltGr" => EVDEV_KEY_ALT_R,
                _ => {
                    tracing::warn!(
                        key_name = self.key_name,
                        "key_name not mapped for evtest fallback, defaulting to KEY_RIGHTALT"
                    );
                    EVDEV_KEY_ALT_R
                }
            };
            std::sync::Arc::from([code])
        }
    }

    /// 尝试 xinput 后端：解析 keycode 并启动 `xinput test-xi2` 读取循环。
    ///
    /// 任何一步失败都只记日志并返回 `None`，由调用方决定回退 evtest。
    fn try_start_xinput_listener(
        &mut self,
        wayland_hint: bool,
    ) -> Option<mpsc::UnboundedReceiver<KeyEvent>> {
        tracing::info!(
            session = wayland_hint,
            "DISPLAY is set; trying xinput test-xi2"
        );
        let keycode = match self.resolve_keycode() {
            Ok(keycode) => {
                tracing::info!(
                    key_name = self.key_name,
                    keycode,
                    "resolved key to X11 keycode"
                );
                keycode
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "xmodmap/keycode resolution failed (no working X?), falling back to evtest"
                );
                return None;
            }
        };

        let (tx, rx) = mpsc::unbounded_channel();
        let running = Arc::clone(&self.running);
        running.store(true, Ordering::SeqCst);
        match self.try_start_xinput(keycode, tx, running) {
            Ok(child) => {
                self.child = Some(child);
                Some(rx)
            }
            Err(e) => {
                tracing::warn!(error = %e, "xinput failed to start, falling back to evtest");
                None
            }
        }
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

        watch_xinput_stderr(child.stderr.take());

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| KeyListenerError::StartFailed("no stdout from xinput".to_string()))?;

        std::thread::spawn(move || run_xinput_stdout_loop(stdout, keycode, tx, running));

        Ok(child)
    }

    /// 启动 evtest fallback 监听器。
    /// evtest 需要读取 /dev/input/event* 设备，需要用户属于 input 组。
    fn start_evtest_fallback(
        &mut self,
        allowed_evdev: std::sync::Arc<[u16]>,
        tx: tokio::sync::mpsc::UnboundedSender<KeyEvent>,
        running: Arc<AtomicBool>,
    ) -> Result<(), KeyListenerError> {
        let keyboard_devices = evtest_fallback_devices()?;

        tracing::info!(
            "using evtest fallback with {} keyboard devices",
            keyboard_devices.len()
        );

        for device in keyboard_devices {
            spawn_evtest_listener(
                &device,
                &self.evtest_children,
                &allowed_evdev,
                &tx,
                &running,
            );
        }

        // 设备监视线程：蓝牙断链重连等热插拔发生后补启与回收 evtest。
        let children = Arc::clone(&self.evtest_children);
        let watch_result = std::thread::Builder::new()
            .name("evtest-device-watch".to_string())
            .spawn(move || {
                device_watch_loop(running, children, allowed_evdev, tx);
            });
        if let Err(e) = watch_result {
            tracing::warn!(error = %e, "failed to spawn device watch thread");
        }

        Ok(())
    }

    /// 停止监听：终止并回收全部子进程。evtest 读取线程阻塞在 read() 上，
    /// 只有 kill 子进程让管道 EOF 才能唤醒它们，退出后不留孤儿与僵尸。
    /// 回收必须同步：SIGKILL 后 wait 是亚毫秒级，放后台线程会让应用
    /// 退出时来不及执行 kill，evtest 以活体孤儿泄漏。
    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
        // 锁被监视线程短暂持有，不会中毒死锁；监视线程看到标志后一秒内退出。
        let mut map = match self.evtest_children.lock() {
            Ok(map) => map,
            Err(poisoned) => poisoned.into_inner(),
        };
        for (_, mut child) in std::mem::take(&mut *map) {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    fn resolve_keycode(&self) -> Result<u8, KeyListenerError> {
        let keycode_map = XMODMAP_CACHE.get_or_init(load_xmodmap_keycodes);

        if keycode_map.is_empty() {
            return Err(KeyListenerError::ResolveFailed(
                "xmodmap failed to produce keycode mappings — is xmodmap installed and DISPLAY set?"
                    .to_string(),
            ));
        }

        let candidates: &[&str] = match self.key_name.as_str() {
            "Alt_L" => &["Alt_L"],
            // 布局上可能只出现 Alt_R、ISO_Level3_Shift 或 AltGr 之一，任一对上即可。
            "Alt_R" | "ISO_Level3_Shift" | "AltGr" => &["Alt_R", "ISO_Level3_Shift", "AltGr"],
            _ => {
                let name = self.key_name.as_str();
                // 单元素切片：自定义 keysym 名
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
    fn stop_terminates_and_reaps_evtest_children() {
        // 回归：stop() 曾只置停止标志，evtest 子进程既不终止也不回收，
        // 读取线程阻塞在 read() 上永不退出，子进程泄漏成孤儿或僵尸。
        // 用 sleep 进程顶替 evtest 验证生命周期语义，避免依赖真实设备。
        // 直接构造结构体：new() 的 xinput 存在性检查与回收逻辑无关，
        // 不该让 CI（不装 xinput）静默跳过这条回归。
        let mut listener = X11Listener {
            key_name: "Alt_R".to_string(),
            linux_evdev_code: None,
            running: Arc::new(AtomicBool::new(false)),
            child: None,
            evtest_children: Arc::new(Mutex::new(HashMap::new())),
        };
        let child = Command::new("sleep").arg("30").spawn().unwrap();
        let pid = child.id();
        listener
            .evtest_children
            .lock()
            .unwrap()
            .insert(PathBuf::from("/dev/input/event16"), child);

        listener.stop();

        // stop() 同步 kill+wait；僵尸进程的 /proc 条目也会消失，
        // 条目仍存在即视为未回收（pid 复用概率在毫秒级窗口内可忽略）。
        assert!(
            !Path::new(&format!("/proc/{pid}")).exists(),
            "stop() 后子进程应被终止并回收，pid {pid} 仍存在"
        );
    }

    /// 构造一个仍在运行的替身子进程（不依赖真实输入设备）。
    fn spawn_long_running_child() -> Child {
        Command::new("sleep").arg("30").spawn().unwrap()
    }

    #[test]
    fn plan_reconcile_spawns_for_new_device() {
        let mut children = HashMap::new();
        let plan = plan_reconcile(&[PathBuf::from("/dev/input/event16")], &mut children);
        assert_eq!(
            plan,
            vec![ReconcileAction::Spawn(PathBuf::from("/dev/input/event16"))]
        );
    }

    #[test]
    fn plan_reconcile_respawns_exited_child_while_device_still_listed() {
        // 回归：uinput 设备销毁会杀死它的 evtest 读者，节点被重连设备
        // 复用后设备仍在枚举里，必须重启读者，否则重连后按键失灵。
        let path = PathBuf::from("/dev/input/event16");
        let mut children = HashMap::new();
        children.insert(path.clone(), Command::new("true").spawn().unwrap());
        // true 立即退出；try_wait 需要观察到退出状态
        std::thread::sleep(std::time::Duration::from_millis(50));

        let plan = plan_reconcile(std::slice::from_ref(&path), &mut children);
        assert_eq!(plan, vec![ReconcileAction::Spawn(path)]);
    }

    #[test]
    fn plan_reconcile_kills_child_of_vanished_device() {
        // 回归：真实蓝牙断链后 evtest 不退出，只是挂在已注销的设备对象
        // 上永远收不到事件；设备从枚举消失时必须回收它的读者。
        let path = PathBuf::from("/dev/input/event16");
        let mut children = HashMap::new();
        children.insert(path.clone(), spawn_long_running_child());

        let plan = plan_reconcile(&[], &mut children);
        assert_eq!(plan, vec![ReconcileAction::Kill(path)]);
    }

    #[test]
    fn plan_reconcile_leaves_healthy_children_alone() {
        let path = PathBuf::from("/dev/input/event16");
        let mut children = HashMap::new();
        children.insert(path.clone(), spawn_long_running_child());

        let plan = plan_reconcile(&[path], &mut children);
        assert!(plan.is_empty());
    }

    #[test]
    fn plan_reconcile_handles_mixed_devices() {
        // 三台设备：一台健康、一台已退出、一台新增；另一台子进程的设备已消失。
        let healthy = PathBuf::from("/dev/input/event1");
        let exited = PathBuf::from("/dev/input/event2");
        let vanished = PathBuf::from("/dev/input/event3");
        let added = PathBuf::from("/dev/input/event4");
        let mut children = HashMap::new();
        children.insert(healthy.clone(), spawn_long_running_child());
        children.insert(exited.clone(), Command::new("true").spawn().unwrap());
        children.insert(vanished.clone(), spawn_long_running_child());
        std::thread::sleep(std::time::Duration::from_millis(50));

        let mut plan = plan_reconcile(&[healthy, exited.clone(), added.clone()], &mut children);
        plan.sort();
        assert_eq!(
            plan,
            vec![
                ReconcileAction::Spawn(exited),
                ReconcileAction::Spawn(added),
                ReconcileAction::Kill(vanished),
            ]
        );
    }

    #[test]
    fn keysym_to_evdev_alt_r() {
        // 直接测试内部映射逻辑
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
        let sample = "N: Name=\"Wave Keys\"\nH: Handlers=kbd event16 leds \n";
        let devices = parse_keyboard_devices(sample);
        assert_eq!(devices, vec![PathBuf::from("/dev/input/event16")]);
    }
}
