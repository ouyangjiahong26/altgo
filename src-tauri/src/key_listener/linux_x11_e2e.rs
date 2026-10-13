//! xinput 路径的端到端测试。
//!
//! 需要真实 X 服务器与 `xinput`/`xdotool`/`xmodmap`，CI 里由 `xvfb-run` 提供
//! （见 `.github/workflows/ci.yml`），本机不满足条件时直接跳过。单列一个文件是因为
//! `linux.rs` 已接近千行上限，而这条用例依赖外部工具、不属于模块级的纯逻辑单测。

use std::process::{Child, Command, Stdio};

use crate::config::KeyListenerConfig;
use crate::key_listener::{subscribe_activation_keys, ActivationKeySubscription, X11Listener};

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
/// 这两种情况下 resolve_keycode 会失败、监听器静默回退 evtest，端到端用例
/// 的前提（走 xinput 后端）不成立，必须前置排除，否则断言失败会归因错。
fn xmodmap_table_available() -> bool {
    match Command::new("xmodmap").arg("-pke").output() {
        Ok(out) => out.status.success() && out.stdout.iter().any(|b| !b.is_ascii_whitespace()),
        Err(_) => false,
    }
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
            for line in reader.lines().flatten() {
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

/// 取出当前已入队的全部旁路观察（每次物理按键会因两套掩码重复上报）。
async fn drain_observations(
    sub: &mut ActivationKeySubscription,
    into: &mut Vec<crate::key_capture::CaptureActivationResponse>,
) {
    while let Some(obs) = sub.next(std::time::Duration::from_millis(100)).await {
        into.push(obs);
    }
}

/// xinput 路径的端到端测试：在 Xvfb 里用 xdotool 注入合成键，验证后端选择、
/// 旁路语义与 keycode 过滤。
///
/// 环境不满足（无 DISPLAY、缺 xinput/xdotool/xmodmap）时直接返回，不用
/// #[ignore]：那是“本机没装工具”，与待修功能无关。Xvfb 会用 XTest 注入，
/// 而 X11 协议层没有“合成”标记，所以注入键与真实键在此路径无法区分。
#[tokio::test]
async fn x11_xinput_end_to_end_injects_and_filters_activation_key() {
    let display_set = matches!(std::env::var("DISPLAY"), Ok(d) if !d.is_empty());
    if !display_set
        || !tool_available("xinput")
        || !tool_available("xdotool")
        || !xmodmap_table_available()
    {
        return;
    }

    let mut listener = X11Listener::new(&test_config()).expect("xinput 存在时 new() 应成功");
    let (mut rx, backend) = listener.start().expect("start() 应成功");
    // 后端名是 xinput，即证明 xmodmap 表建起来了、keycode 也解析出来了。
    assert_eq!(backend, "xinput", "应走 xinput 后端，而非 evtest 回退");

    let mut sub = subscribe_activation_keys();
    let probe = match Xi2Probe::spawn() {
        Some(p) => p,
        None => return,
    };

    // xinput 子进程要先完成事件选择才收得到事件，注入因此要留重试窗口；
    // 断言按“至少一次”收集：RawKey* 与 Key* 两套掩码会让同一物理键重复上报。
    let mut probe_seen = false;
    let mut press_seen = false;
    let mut release_seen = false;
    for _ in 0..5 {
        let _ = Command::new("xdotool")
            .args(["key", "--clearmodifiers", "Alt_R"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        // 注入到 xinput 打印、再到转发线程入队有几十毫秒延迟，稍等再收割。
        std::thread::sleep(std::time::Duration::from_millis(400));
        probe_seen |= probe.wait_press(std::time::Duration::from_millis(50));
        while let Ok(ev) = rx.try_recv() {
            if ev.pressed {
                press_seen = true;
            } else {
                release_seen = true;
            }
        }
        if probe_seen && press_seen && release_seen {
            break;
        }
    }

    // 环境自检先于被测断言：Xvfb 喂不到 XI2 时后续失败不该记到被测代码头上。
    assert!(
        probe_seen,
        "xdotool 注入的按键未进入 XI2，该 Xvfb 环境不支持这条端到端路径"
    );
    assert!(press_seen, "通道应至少收到一次按下");
    assert!(release_seen, "通道应至少收到一次松开");

    // 旁路应看到右 Alt。X11 只有 keycode、没有 evdev 码，协议层也没有“合成”
    // 标记，所以 linux_evdev_code 与 injected 都必须为 None。
    let mut observations = Vec::new();
    drain_observations(&mut sub, &mut observations).await;
    assert!(
        observations.iter().any(|o| {
            matches!(o.key_name.as_str(), "Alt_R" | "ISO_Level3_Shift" | "AltGr")
                && o.linux_evdev_code.is_none()
                && o.injected.is_none()
        }),
        "旁路应看到右 Alt 且无 evdev 码/合成标记，实际: {observations:?}"
    );

    // 注入左 Alt：旁路能看到，但通道按 keycode 过滤掉。
    // 前一阶段的收尾事件可能仍在路上，先静置再清空，避免误判。
    std::thread::sleep(std::time::Duration::from_millis(300));
    while rx.try_recv().is_ok() {}
    let _ = Command::new("xdotool")
        .args(["key", "--clearmodifiers", "Alt_L"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    std::thread::sleep(std::time::Duration::from_millis(1000));
    assert!(rx.try_recv().is_err(), "非触发键不应在通道产生事件");

    drain_observations(&mut sub, &mut observations).await;
    assert!(
        observations.iter().any(|o| o.key_name == "Alt_L"),
        "旁路应看到左 Alt，实际: {observations:?}"
    );

    // stop() 之后通道不再产出。
    listener.stop();
    while rx.try_recv().is_ok() {}
    let _ = Command::new("xdotool")
        .args(["key", "--clearmodifiers", "Alt_R"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    std::thread::sleep(std::time::Duration::from_millis(500));
    assert!(rx.try_recv().is_err(), "stop() 后通道不应再产出事件");
}
