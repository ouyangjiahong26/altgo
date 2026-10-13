//! xinput 路径的端到端测试。
//!
//! 需要真实 X 服务器与 `xinput`/`xdotool`/`xmodmap`，CI 里由 `xvfb-run` 提供
//! （见 `.github/workflows/ci.yml`），本机不满足条件时直接跳过。单列一个文件是因为
//! `linux.rs` 已接近千行上限，而这条用例依赖外部工具、不属于模块级的纯逻辑单测。

use std::process::{Child, Command, Stdio};

use crate::config::KeyListenerConfig;
use crate::key_listener::{subscribe_activation_keys, ActivationKeySubscription, X11Listener};

/// 监听器的按键通道，downstream 断言从这里取事件。
type KeyEventReceiver = tokio::sync::mpsc::UnboundedReceiver<crate::key_listener::KeyEvent>;

fn test_config() -> KeyListenerConfig {
    KeyListenerConfig {
        key_name: "Alt_R".to_string(),
        linux_evdev_code: None,
        windows_vk_code: None,
        long_press_threshold: std::time::Duration::from_millis(400),
        double_click_interval: std::time::Duration::from_millis(200),
        min_press_duration: std::time::Duration::from_millis(80),
    }
}

/// 二进制是否装好：只看能否启动，不看退出码（只判断“有没有这个工具”）。
fn tool_available(name: &str) -> bool {
    Command::new(name)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

/// xmodmap 能否产出键表。缺包，或 DISPLAY 指向不可用的 X 服务器时都为 false：
/// 这两种情况下 resolve_keycode 会失败、监听器静默回退 evtest，端到端用例的前提
/// （走 xinput 后端）不成立，必须前置排除，否则断言失败会归因错。
fn xmodmap_table_available() -> bool {
    match Command::new("xmodmap").arg("-pke").output() {
        Ok(out) => out.status.success() && out.stdout.iter().any(|b| !b.is_ascii_whitespace()),
        Err(_) => false,
    }
}

/// 环境是否满足：有 DISPLAY，且三个外部工具都在。
fn environment_ready() -> bool {
    matches!(std::env::var("DISPLAY"), Ok(d) if !d.is_empty())
        && tool_available("xinput")
        && tool_available("xdotool")
        && xmodmap_table_available()
}

/// 独立的 `xinput test-xi2 --root` 观察进程，用来判定环境层（Xvfb + XTest）
/// 到底喂不喂得到 XI2 事件。句柄留在结构体里，Drop 时终止回收，避免残留进程。
struct Xi2Probe {
    child: Child,
    rx: std::sync::mpsc::Receiver<()>,
}

impl Xi2Probe {
    fn spawn() -> Option<Self> {
        let mut child = Command::new("xinput")
            .args(["test-xi2", "--root"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let stdout = child.stdout.take()?;
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            use std::io::BufRead;
            // 与监听器同一套判据：EVENT 行置状态，随后第一条 detail 即一次按下。
            let reader = std::io::BufReader::new(stdout);
            let mut pending = false;
            for line in reader.lines().map_while(Result::ok) {
                let trimmed = line.trim();
                if trimmed.starts_with("EVENT") && trimmed.contains("KeyPress") {
                    pending = true;
                } else if trimmed.starts_with("detail:") && pending {
                    pending = false;
                    if tx.send(()).is_err() {
                        break;
                    }
                }
            }
        });
        Some(Self { child, rx })
    }

    /// 窗口内是否观察到一次按下。
    fn wait_press(&self, window: std::time::Duration) -> bool {
        self.rx.recv_timeout(window).is_ok()
    }
}

impl Drop for Xi2Probe {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// 用 xdotool 注入一次按键（按下加松开）。
fn press_key(name: &str) {
    let _ = Command::new("xdotool")
        .args(["key", "--clearmodifiers", name])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// 取走通道里已入队的事件，返回（见到按下，见到松开）。
fn drain_key_events(rx: &mut KeyEventReceiver) -> (bool, bool) {
    let (mut press, mut release) = (false, false);
    while let Ok(event) = rx.try_recv() {
        if event.pressed {
            press = true;
        } else {
            release = true;
        }
    }
    (press, release)
}

/// 清空通道里已入队的事件，用于阶段之间排除在途事件的干扰。
fn clear_key_events(rx: &mut KeyEventReceiver) {
    while rx.try_recv().is_ok() {}
}

/// 注入右 Alt 直到环境层确认按键进了 XI2，返回
/// （环境层见到，通道收到按下，通道收到松开）。
///
/// xinput 子进程要先完成事件选择才收得到事件，注入因此要留重试窗口。断言按
/// “至少一次”收集：RawKey* 与 Key* 两套掩码会让同一物理键重复上报。
fn inject_until_seen(probe: &Xi2Probe, rx: &mut KeyEventReceiver) -> (bool, bool, bool) {
    let (mut probe_seen, mut press_seen, mut release_seen) = (false, false, false);
    for _ in 0..5 {
        press_key("Alt_R");
        // 注入到 xinput 打印、再到转发线程入队有几十毫秒延迟，稍等再收割。
        std::thread::sleep(std::time::Duration::from_millis(400));
        probe_seen |= probe.wait_press(std::time::Duration::from_millis(50));
        let (press, release) = drain_key_events(rx);
        press_seen |= press;
        release_seen |= release;
        if probe_seen && press_seen && release_seen {
            break;
        }
    }
    (probe_seen, press_seen, release_seen)
}

/// 取出当前已入队的全部旁路观察（每次物理按键会因两套掩码重复上报）。
async fn drain_observations(
    sub: &mut ActivationKeySubscription,
    into: &mut Vec<crate::key_capture::CaptureActivationResponse>,
) {
    while let Some(obs) = sub.next(std::time::Duration::from_millis(100)).await {
        into.push(obs);
    }
}

/// 旁路里是否出现了名为 `names` 之一的物理按键。
///
/// X11 只有 keycode、没有 evdev 码，协议层也没有“合成”标记，所以 `linux_evdev_code`
/// 与 `injected` 都必须是 `None`。`names` 允许多个是因为 Xvfb 键表可能把 keycode 108
/// 的主名给成 `ISO_Level3_Shift` 或 `AltGr`。
async fn bypass_saw_physical_key(sub: &mut ActivationKeySubscription, names: &[&str]) -> bool {
    let mut observations = Vec::new();
    drain_observations(sub, &mut observations).await;
    observations.iter().any(|o| {
        names.contains(&o.key_name.as_str()) && o.linux_evdev_code.is_none() && o.injected.is_none()
    })
}

/// xinput 路径的端到端测试：在 Xvfb 里用 xdotool 注入合成键，验证后端选择、
/// 旁路语义与 keycode 过滤。
///
/// 环境不满足（无 DISPLAY、缺 xinput/xdotool/xmodmap）时直接返回，不用
/// #[ignore]：那是“本机没装工具”，与待修功能无关。Xvfb 会用 XTest 注入，
/// 而 X11 协议层没有“合成”标记，所以注入键与真实键在此路径无法区分。
#[tokio::test]
async fn x11_xinput_end_to_end_injects_and_filters_activation_key() {
    if !environment_ready() {
        return;
    }

    let mut listener = X11Listener::new(&test_config()).expect("xinput 存在时 new() 应成功");
    let (mut rx, backend) = listener.start().expect("start() 应成功");
    // 后端名是 xinput，即证明 xmodmap 表建起来了、keycode 也解析出来了。
    assert_eq!(backend, "xinput", "应走 xinput 后端，而非 evtest 回退");

    let mut sub = subscribe_activation_keys();
    let Some(probe) = Xi2Probe::spawn() else {
        return;
    };

    let (probe_seen, press_seen, release_seen) = inject_until_seen(&probe, &mut rx);
    // 环境自检先于被测断言：Xvfb 喂不到 XI2 时后续失败不该记到被测代码头上。
    assert!(
        probe_seen,
        "xdotool 注入的按键未进入 XI2，该 Xvfb 环境不支持这条端到端路径"
    );
    assert!(press_seen, "通道应至少收到一次按下");
    assert!(release_seen, "通道应至少收到一次松开");
    assert!(
        bypass_saw_physical_key(&mut sub, &["Alt_R", "ISO_Level3_Shift", "AltGr"]).await,
        "旁路应看到右 Alt 且无 evdev 码与合成标记"
    );

    // 注入左 Alt：旁路能看到，但通道按 keycode 过滤掉。
    // 前一阶段的收尾事件可能仍在路上，先静置再清空，避免误判。
    std::thread::sleep(std::time::Duration::from_millis(300));
    clear_key_events(&mut rx);
    press_key("Alt_L");
    std::thread::sleep(std::time::Duration::from_millis(1000));
    assert!(rx.try_recv().is_err(), "非触发键不应在通道产生事件");
    assert!(
        bypass_saw_physical_key(&mut sub, &["Alt_L"]).await,
        "旁路应看到左 Alt"
    );

    // stop() 之后通道不再产出。
    listener.stop();
    clear_key_events(&mut rx);
    press_key("Alt_R");
    std::thread::sleep(std::time::Duration::from_millis(500));
    assert!(rx.try_recv().is_err(), "stop() 后通道不应再产出事件");
}
