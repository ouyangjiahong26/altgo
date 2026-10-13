//! xinput 路径的测试。
//!
//! 分两层。解析、过滤与旁路发布用受控输出喂真实的解析循环，不需要 X 服务器，任何环境
//! 都能跑；后端选择在 Xvfb 里验一次，缺 DISPLAY 或外部工具时跳过。`linux.rs` 已接近
//! 千行上限，所以这两条放在单独的文件里。

use std::process::{Command, Stdio};

use crate::config::KeyListenerConfig;
use crate::key_listener::{subscribe_activation_keys, X11Listener};

/// 一次按键在 `xinput test-xi2 --root` 下的真实输出形态：RawKeyPress 与 KeyPress 两套
/// 掩码各打一条 `detail:`，所以同一个物理键会出现两条观察。108 是右 Alt（触发键），
/// 64 是左 Alt（不在触发键上，只应出现在旁路里）。
const CANNED_XINPUT_OUTPUT: &str = "\
EVENT type 13 (RawKeyPress)
    device: 3 (10)
    time: 1001
    detail: 108
EVENT type 2 (KeyPress)
    device: 3 (6)
    time: 1001
    detail: 108
EVENT type 2 (KeyRelease)
    device: 3 (6)
    time: 1002
    detail: 108
EVENT type 2 (KeyPress)
    device: 3 (6)
    time: 1003
    detail: 64
EVENT type 2 (KeyRelease)
    device: 3 (6)
    time: 1004
    detail: 64
";

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

/// xmodmap 能否产出键表。缺包或 DISPLAY 指向不可用的 X 服务器时为 false，此时监听器会
/// 回退 evtest，后端选择那条用例的前提不成立。
fn xmodmap_table_available() -> bool {
    match Command::new("xmodmap").arg("-pke").output() {
        Ok(out) => out.status.success() && out.stdout.iter().any(|b| !b.is_ascii_whitespace()),
        Err(_) => false,
    }
}

/// 把右 Alt 钉在 keycode 108 上。键表由服务器持有，钉死之后键名与键码的对应关系不再
/// 取决于发行版默认键表怎么命名。
fn pin_trigger_keymap() -> bool {
    Command::new("xmodmap")
        .args(["-e", "keycode 108 = Alt_R Meta_R"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// 把监听器内部的 tracing 输出接进测试日志。
fn install_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_writer(std::io::stderr)
        .try_init();
}

/// 受控输出喂真实的解析循环：触发键的按下与松开进通道，非触发键只出现在旁路。
///
/// X11 上键名要查 xmodmap 表，没有 X 时退化成 `keycode_<N>`，两种形态都接受。
#[tokio::test]
async fn xinput_stdout_loop_publishes_bypass_and_filters_by_keycode() {
    install_tracing();
    let _guard = crate::key_listener::lock_bypass_test().await;
    let mut sub = subscribe_activation_keys();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));

    crate::key_listener::linux::run_xinput_stdout_loop(
        CANNED_XINPUT_OUTPUT.as_bytes(),
        108,
        tx,
        running,
    );

    let mut observed = Vec::new();
    while let Some(observation) = sub.next(std::time::Duration::from_millis(50)).await {
        observed.push(observation.key_name);
    }
    assert!(
        observed
            .iter()
            .any(|name| name == "Alt_R" || name == "keycode_108"),
        "旁路应看到右 Alt，实际 {observed:?}"
    );
    assert!(
        observed
            .iter()
            .any(|name| name == "Alt_L" || name == "keycode_64"),
        "旁路应看到左 Alt，实际 {observed:?}"
    );

    let (mut pressed, mut released) = (0, 0);
    while let Ok(event) = rx.try_recv() {
        if event.pressed {
            pressed += 1;
        } else {
            released += 1;
        }
    }
    // 右 Alt 的 RawKeyPress 与 KeyPress 各转发一次按下，左 Alt 一条都不转发。
    assert_eq!(pressed, 2, "触发键应转发两次按下");
    assert_eq!(released, 1, "触发键应转发一次松开");
}

/// Xvfb 里的后端选择：xmodmap 能解析出键码时应报 xinput，而不是静默回退 evtest。
///
/// 环境不满足（无 DISPLAY、缺 xinput 或 xmodmap）时直接跳过，不用 `#[ignore]`：那是
/// “本机没装工具”，与待验行为无关。真跳过时打一行输出，日志里看得见。
#[tokio::test]
async fn x11_backend_is_xinput_when_xmodmap_resolves() {
    let display_set = matches!(std::env::var("DISPLAY"), Ok(d) if !d.is_empty());
    if !display_set
        || !tool_available("xinput")
        || !xmodmap_table_available()
        || !pin_trigger_keymap()
    {
        eprintln!("skipping x11 backend test: 无 DISPLAY 或缺 xinput/xmodmap");
        return;
    }
    install_tracing();

    let mut listener = X11Listener::new(&test_config()).expect("xinput 存在时 new() 应成功");
    let (_rx, backend) = listener.start().expect("start() 应成功");
    assert_eq!(backend, "xinput", "xmodmap 可用时应走 xinput 后端");
    listener.stop();
}
