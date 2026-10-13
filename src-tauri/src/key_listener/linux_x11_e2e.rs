//! xinput 路径的端到端测试。
//!
//! 需要真实 X 服务器与 `xinput`/`xdotool`/`xmodmap`，CI 里由 `xvfb-run` 提供
//! （见 `.github/workflows/ci.yml`），本机不满足条件时直接跳过。单列一个文件是因为
//! `linux.rs` 已接近千行上限，而这条用例依赖外部工具、不属于模块级的纯逻辑单测。

use std::process::{Child, Command, Stdio};

use crate::config::KeyListenerConfig;
use crate::key_listener::{subscribe_activation_keys, ActivationKeySubscription, X11Listener};

/// 监听器的按键通道，断言从这里取事件。
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

/// 把触发键钉在固定的 keycode 上，返回是否成功。
///
/// Xvfb 默认键表里 keycode 108 的主名可能是 `ISO_Level3_Shift`，`xmodmap -pke`
/// 打印出来的名字与 xdotool 按 keysym 名反查的 keycode 都可能与预期不同。端到端
/// 用例要验的是监听器的解析、过滤与转发，所以先把键表钉死，让两边从同一张表出发。
fn pin_trigger_keymap() -> bool {
    Command::new("xmodmap")
        .args(["-e", "keycode 108 = Alt_R Meta_R"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// 环境是否满足：有 DISPLAY，三个外部工具都在，且键表钉定成功。
fn environment_ready() -> bool {
    matches!(std::env::var("DISPLAY"), Ok(d) if !d.is_empty())
        && tool_available("xinput")
        && tool_available("xdotool")
        && xmodmap_table_available()
        && pin_trigger_keymap()
}

/// 独立的 `xinput test-xi2 --root` 观察进程，用来判定环境层（Xvfb 加 XTest）
/// 到底喂不喂得到 XI2 事件。句柄留在结构体里，Drop 时终止回收，避免残留进程。
struct Xi2Probe {
    child: Child,
    rx: std::sync::mpsc::Receiver<u8>,
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
            // 与监听器同一套判据：EVENT 行置状态，随后第一条 detail 即一次按下，
            // 把该行的 keycode 报回去，断言失败时能看出环境喂来的到底是哪个键。
            let reader = std::io::BufReader::new(stdout);
            let mut pending = false;
            for line in reader.lines().map_while(Result::ok) {
                let trimmed = line.trim();
                if trimmed.starts_with("EVENT") && trimmed.contains("KeyPress") {
                    pending = true;
                } else if let Some(detail) = trimmed.strip_prefix("detail:") {
                    if !pending {
                        continue;
                    }
                    pending = false;
                    if let Ok(code) = detail.trim().parse::<u8>() {
                        if tx.send(code).is_err() {
                            break;
                        }
                    }
                }
            }
        });
        Some(Self { child, rx })
    }

    /// 窗口内是否观察到一次按下，是则返回该事件的 keycode。
    fn next_press(&self, window: std::time::Duration) -> Option<u8> {
        self.rx.recv_timeout(window).ok()
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

/// 取走旁路里已入队的全部观察（每次物理按键会因 RawKey 与 Key 两套掩码重复上报）。
async fn collect_observations(
    sub: &mut ActivationKeySubscription,
) -> Vec<crate::key_capture::CaptureActivationResponse> {
    let mut into = Vec::new();
    while let Some(observation) = sub.next(std::time::Duration::from_millis(100)).await {
        into.push(observation);
    }
    into
}

/// 注入右 Alt 直到环境层确认按键进了 XI2，返回
/// （环境探针看到的 keycode 列表，通道收到按下，通道收到松开）。
///
/// xinput 子进程要先完成事件选择才收得到事件，注入因此要留重试窗口。断言按
/// “至少一次”收集：RawKey 与 Key 两套掩码会让同一物理键重复上报。
fn inject_until_seen(probe: &Xi2Probe, rx: &mut KeyEventReceiver) -> (Vec<u8>, bool, bool) {
    let (mut press_seen, mut release_seen) = (false, false);
    let mut probe_details = Vec::new();
    for _ in 0..5 {
        press_key("Alt_R");
        // 注入到 xinput 打印、再到转发线程入队有几十毫秒延迟，稍等再收割。
        std::thread::sleep(std::time::Duration::from_millis(400));
        if let Some(code) = probe.next_press(std::time::Duration::from_millis(50)) {
            probe_details.push(code);
        }
        let (press, release) = drain_key_events(rx);
        press_seen |= press;
        release_seen |= release;
        if !probe_details.is_empty() && press_seen && release_seen {
            break;
        }
    }
    (probe_details, press_seen, release_seen)
}

/// 注入左 Alt：旁路应看到它，通道按 keycode 过滤掉。
async fn assert_left_alt_is_filtered(
    sub: &mut ActivationKeySubscription,
    rx: &mut KeyEventReceiver,
) {
    // 前一阶段的收尾事件可能仍在路上，先静置再清空，避免误判。
    std::thread::sleep(std::time::Duration::from_millis(300));
    clear_key_events(rx);
    press_key("Alt_L");
    std::thread::sleep(std::time::Duration::from_millis(1000));
    let filtered = rx.try_recv().is_err();
    let observations = collect_observations(sub).await;
    assert!(filtered, "非触发键不应在通道产生事件");
    assert!(
        observations.iter().any(|o| o.key_name == "Alt_L"),
        "旁路应看到左 Alt，实际 {observations:?}"
    );
}

/// stop() 之后通道不再产出。
fn assert_stopped(listener: &mut X11Listener, rx: &mut KeyEventReceiver) {
    listener.stop();
    clear_key_events(rx);
    press_key("Alt_R");
    std::thread::sleep(std::time::Duration::from_millis(500));
    assert!(rx.try_recv().is_err(), "stop() 后通道不应再产出事件");
}

/// 进程是否还活着（`/proc` 下还有这个 pid）。
fn process_alive(pid: u32) -> bool {
    std::path::Path::new(&format!("/proc/{pid}")).exists()
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
        // 环境不满足按仓库惯例直接跳过。CI 里工具是我们自己装的，真跳过说明环境出了问题，
        // 留一行输出让日志里看得见，不依赖“零条用例通过”这种看不出来的信号。
        eprintln!("skipping x11 e2e: DISPLAY, xinput, xdotool, xmodmap 或键表钉定不可用");
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

    let (probe_details, press_seen, release_seen) = inject_until_seen(&probe, &mut rx);
    // 环境自检先于被测断言：Xvfb 喂不到 XI2 时后续失败不该记到被测代码头上。
    assert!(
        !probe_details.is_empty(),
        "xdotool 注入的按键未进入 XI2，该 Xvfb 环境不支持这条端到端路径"
    );
    // 监听器自己的子进程先看还活着没有：它中途死掉会让后面每条断言都指向错的方向。
    let pid = listener.child_pid().expect("监听器应持有 xinput 子进程");
    assert!(
        process_alive(pid),
        "监听器的 xinput 子进程已退出（pid {pid}），环境探针看到的 keycode: {probe_details:?}"
    );
    // 旁路带着监听器自己解析出的键名，断言失败时能看出它到底看到了什么。
    let observations = collect_observations(&mut sub).await;
    assert!(
        !observations.is_empty(),
        "监听器没从 xinput 收到任何按键，环境探针看到的 keycode: {probe_details:?}，子进程存活 {}",
        process_alive(pid)
    );
    assert!(
        press_seen,
        "通道应至少收到一次按下，监听器旁路看到 {observations:?}"
    );
    assert!(
        release_seen,
        "通道应至少收到一次松开，监听器旁路看到 {observations:?}"
    );
    assert!(
        observations.iter().any(|o| {
            matches!(o.key_name.as_str(), "Alt_R" | "ISO_Level3_Shift" | "AltGr")
                && o.linux_evdev_code.is_none()
                && o.injected.is_none()
        }),
        "旁路应看到右 Alt 且无 evdev 码与合成标记，实际 {observations:?}"
    );

    assert_left_alt_is_filtered(&mut sub, &mut rx).await;
    assert_stopped(&mut listener, &mut rx);
}
