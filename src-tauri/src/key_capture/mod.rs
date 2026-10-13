//! 短时捕获用户按下的物理键，用于“按下以设置”触发键与触发键自检。
//!
//! 捕获结果带平台原始键码（Windows 为 VK，Linux 为 evdev），与监听器
//! 实际用于匹配的码一致，因此可以直接判断按下的键能不能触发录音。

use std::time::Duration;

mod evdev_names;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "windows")]
mod windows;

pub use evdev_names::evdev_code_to_keysym_name;

#[cfg(target_os = "linux")]
pub use linux::LinuxKeyCapture;
#[cfg(target_os = "windows")]
pub use windows::WindowsKeyCapture;

#[cfg(target_os = "linux")]
pub type PlatformKeyCapture = LinuxKeyCapture;
#[cfg(target_os = "windows")]
pub type PlatformKeyCapture = WindowsKeyCapture;

#[cfg(target_os = "linux")]
pub use linux::{
    capture_fallback_timeout_message, capture_timeout_message, configured_match_code,
    configured_match_code_from_config,
};
#[cfg(target_os = "windows")]
pub use windows::{
    capture_fallback_timeout_message, capture_timeout_message, configured_match_code,
    configured_match_code_from_config,
};

use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureActivationResponse {
    pub key_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linux_evdev_code: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub windows_vk_code: Option<u16>,
    /// 该按键是否由其他程序注入。接受注入的捕获里可能为 true，其余情形恒为 false；
    /// Linux 的 evdev 路径区分不出注入，恒为 `None`。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub injected: Option<bool>,
}

impl CaptureActivationResponse {
    /// 本次捕获的原始键码：Windows 为 VK，Linux 为 evdev。
    pub fn raw_code(&self) -> Option<u16> {
        self.windows_vk_code.or(self.linux_evdev_code)
    }
}

/// “按下以设置”等待按键的时长。
pub const CAPTURE_TIMEOUT: Duration = Duration::from_secs(12);

/// 一次捕获的结局。
///
/// 超时是与“抓到按键”并列的独立结局：设置页把它当失败展示，
/// 触发键自检要把它和“抓到了但键不对”讲成两件事。
#[derive(Debug)]
pub enum CaptureOutcome {
    Captured(CaptureActivationResponse),
    TimedOut,
}

/// 触发键自检结果：按下触发键后能不能被监听器认出来。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivationKeyProbe {
    /// 实际抓到的按键，超时为 `None`。
    pub observed: Option<CaptureActivationResponse>,
    /// 当前配置里的触发键名。
    pub configured_key_name: String,
    /// 监听器实际用于匹配的键码（Windows 为 VK，Linux 为 evdev），解析不出为 `None`。
    pub configured_code: Option<u16>,
    /// 这次抓到的按键是否由其他程序注入。
    pub injected: bool,
    /// 这次按键能不能触发起录：键码与配置一致，且不是其他程序注入的。
    pub matched: bool,
}

/// 键名的等价比较：`AltGr` 与 `ISO_Level3_Shift` 是右 Alt 的别名。
fn key_names_equivalent(left: &str, right: &str) -> bool {
    fn canonical(name: &str) -> &str {
        match name {
            "AltGr" | "ISO_Level3_Shift" => "Alt_R",
            other => other,
        }
    }
    canonical(left) == canonical(right)
}

/// 由观测值与配置解析出的匹配码判定自检结果。
///
/// 比对以平台原始码为准，因为名字表可能把两个码映到同一个名字，而监听器匹配用的
/// 就是原始码；原始码缺失时（X11 的 xinput 路径只能给出键名）退回按名字判。
/// 注入的按键即使键码对得上也触发起不了录，因为监听器一律忽略注入事件，
/// 所以这里直接判为不命中。
pub fn evaluate_probe(
    observed: Option<CaptureActivationResponse>,
    configured_key_name: String,
    configured_code: Option<u16>,
) -> ActivationKeyProbe {
    let injected = observed
        .as_ref()
        .and_then(|response| response.injected)
        .unwrap_or(false);
    let observed_code = observed
        .as_ref()
        .and_then(CaptureActivationResponse::raw_code);
    let code_matched = match (observed_code, configured_code) {
        (Some(observed_code), Some(configured_code)) => observed_code == configured_code,
        _ => false,
    };
    let name_matched = observed
        .as_ref()
        .map(|response| key_names_equivalent(&response.key_name, &configured_key_name))
        .unwrap_or(false);
    ActivationKeyProbe {
        observed,
        configured_key_name,
        configured_code,
        injected,
        matched: (code_matched || name_matched) && !injected,
    }
}

/// 平台触发键捕获的 trait seam。
///
/// 由平台 adapter 实现。`capture()` 为同步阻塞方法，调用方负责在独立线程执行。
pub trait KeyCapture: Send {
    /// 阻塞等待一次物理按键，`timeout` 内没有按键时返回 [`CaptureOutcome::TimedOut`]。
    fn capture(&mut self, timeout: Duration) -> Result<CaptureOutcome, String>;
}

/// Windows 虚拟键码（Win32 `VK_*` 常量，与 `winuser.h` 一致）。
pub mod windows_vk {
    pub const VK_LMENU: u16 = 0xA4;
    pub const VK_RMENU: u16 = 0xA5;
    pub const VK_LSHIFT: u16 = 0xA0;
    pub const VK_RSHIFT: u16 = 0xA1;
    pub const VK_LCONTROL: u16 = 0xA2;
    pub const VK_RCONTROL: u16 = 0xA3;
    pub const VK_LWIN: u16 = 0x5B;
    pub const VK_RWIN: u16 = 0x5C;
}

/// 将 Windows VK 码映射为与 Linux keysym 风格一致的按键名称，未知则 `vk_0xXX`。
pub fn windows_vk_code_to_name(vk: u16) -> String {
    match vk {
        0x08 => "BackSpace".to_string(),
        0x09 => "Tab".to_string(),
        0x0D => "Return".to_string(),
        0x1B => "Escape".to_string(),
        0x20 => "space".to_string(),
        0x21 => "Prior".to_string(),
        0x22 => "Next".to_string(),
        0x23 => "End".to_string(),
        0x24 => "Home".to_string(),
        0x25 => "Left".to_string(),
        0x26 => "Up".to_string(),
        0x27 => "Right".to_string(),
        0x28 => "Down".to_string(),
        0x2D => "Insert".to_string(),
        0x2E => "Delete".to_string(),
        0x30..=0x39 => char::from(b'0' + (vk - 0x30) as u8).to_string(),
        0x41..=0x5A => char::from(b'a' + (vk - 0x41) as u8).to_string(),
        0x70..=0x7B => format!("F{}", vk - 0x6F),
        0xA0 => "Shift_L".to_string(),
        0xA1 => "Shift_R".to_string(),
        windows_vk::VK_LMENU => "Alt_L".to_string(),
        windows_vk::VK_RMENU => "Alt_R".to_string(),
        0xA2 => "Control_L".to_string(),
        0xA3 => "Control_R".to_string(),
        0x5B => "Super_L".to_string(),
        0x5C => "Super_R".to_string(),
        _ => format!("vk_0x{:02X}", vk),
    }
}

/// 将按键名称解析为 Windows VK 码，与 [`windows_vk_code_to_name`] 互逆，`AltGr`/`ISO_Level3_Shift` 视作右 Alt。
pub fn key_name_to_windows_vk(name: &str) -> Option<u16> {
    Some(match name {
        "BackSpace" => 0x08,
        "Tab" => 0x09,
        "Return" => 0x0D,
        "Escape" => 0x1B,
        "space" => 0x20,
        "Prior" => 0x21,
        "Next" => 0x22,
        "End" => 0x23,
        "Home" => 0x24,
        "Left" => 0x25,
        "Up" => 0x26,
        "Right" => 0x27,
        "Down" => 0x28,
        "Insert" => 0x2D,
        "Delete" => 0x2E,
        "Shift_L" => 0xA0,
        "Shift_R" => 0xA1,
        "Alt_L" => windows_vk::VK_LMENU,
        "Alt_R" | "AltGr" | "ISO_Level3_Shift" => windows_vk::VK_RMENU,
        "Control_L" => 0xA2,
        "Control_R" => 0xA3,
        "Super_L" => 0x5B,
        "Super_R" => 0x5C,
        _ => {
            let mut chars = name.chars();
            let first = chars.next()?;
            if first.is_ascii_alphabetic() && chars.next().is_none() {
                (first as u8 & !0x20) as u16 // 小写字母 → 'A'..'Z'
            } else if first.is_ascii_digit() && chars.next().is_none() {
                first as u16
            } else {
                let f = name.strip_prefix("F")?;
                let n: u16 = f.parse().ok()?;
                if (1..=12).contains(&n) {
                    0x6F + n
                } else {
                    return None;
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evdev_code_to_keysym_name_maps_right_alt() {
        assert_eq!(evdev_code_to_keysym_name(100), "ISO_Level3_Shift");
    }

    /// 常量表必须与 winuser.h 一致。用字面量而不是另一张表断言，
    /// 否则表与测试一起改就锁不住偏差。
    #[test]
    fn windows_vk_constants_match_winuser_h() {
        assert_eq!(windows_vk::VK_LSHIFT, 0xA0);
        assert_eq!(windows_vk::VK_RSHIFT, 0xA1);
        assert_eq!(windows_vk::VK_LCONTROL, 0xA2);
        assert_eq!(windows_vk::VK_RCONTROL, 0xA3);
        assert_eq!(windows_vk::VK_LMENU, 0xA4);
        assert_eq!(windows_vk::VK_RMENU, 0xA5);
        assert_eq!(windows_vk::VK_LWIN, 0x5B);
        assert_eq!(windows_vk::VK_RWIN, 0x5C);
    }

    /// 左 Alt 必须解析成 0xA4 并解码回可读名字。0x12 是通用 VK_MENU，
    /// 钩子按左右区分上报时它永远匹配不上。
    #[test]
    fn windows_vk_left_alt_uses_dedicated_code() {
        assert_eq!(key_name_to_windows_vk("Alt_L"), Some(0xA4));
        assert_eq!(windows_vk_code_to_name(0xA4), "Alt_L");
        assert_ne!(key_name_to_windows_vk("Alt_L"), Some(0x12));
    }

    #[test]
    fn probe_matched_when_observed_code_equals_configured() {
        let observed = CaptureActivationResponse {
            key_name: "Alt_R".to_string(),
            linux_evdev_code: None,
            windows_vk_code: Some(0xA5),
            injected: None,
        };
        let probe = evaluate_probe(Some(observed), "Alt_R".to_string(), Some(0xA5));
        assert!(probe.matched);
    }

    #[test]
    fn probe_reports_mismatch_between_observed_and_configured() {
        let observed = CaptureActivationResponse {
            key_name: "vk_0xA4".to_string(),
            linux_evdev_code: None,
            windows_vk_code: Some(0xA4),
            injected: None,
        };
        let probe = evaluate_probe(Some(observed), "Alt_R".to_string(), Some(0xA5));
        assert!(!probe.matched);
        assert_eq!(probe.observed.as_ref().unwrap().raw_code(), Some(0xA4));
        assert_eq!(probe.configured_code, Some(0xA5));
    }

    /// 注入的按键即使键码与配置一致也触发起不了录：监听器一律忽略注入事件。
    #[test]
    fn probe_treats_injected_key_as_not_matched() {
        let observed = CaptureActivationResponse {
            key_name: "Alt_R".to_string(),
            linux_evdev_code: None,
            windows_vk_code: Some(0xA5),
            injected: Some(true),
        };
        let probe = evaluate_probe(Some(observed), "Alt_R".to_string(), Some(0xA5));
        assert!(probe.injected);
        assert!(!probe.matched);
    }

    /// 物理按键照常判为命中，注入标记为假。
    #[test]
    fn probe_reports_physical_key_as_not_injected() {
        let observed = CaptureActivationResponse {
            key_name: "Alt_R".to_string(),
            linux_evdev_code: None,
            windows_vk_code: Some(0xA5),
            injected: Some(false),
        };
        let probe = evaluate_probe(Some(observed), "Alt_R".to_string(), Some(0xA5));
        assert!(!probe.injected);
        assert!(probe.matched);
    }

    #[test]
    fn probe_without_observed_key_is_not_matched() {
        let probe = evaluate_probe(None, "Alt_R".to_string(), Some(0xA5));
        assert!(!probe.matched);
        assert!(probe.observed.is_none());
    }

    /// 配置的键名解析不出码时不能算命中，否则会把“配置本身有问题”报成正常。
    #[test]
    fn probe_without_configured_code_is_not_matched() {
        let observed = CaptureActivationResponse {
            key_name: "ISO_Level3_Shift".to_string(),
            linux_evdev_code: Some(100),
            windows_vk_code: None,
            injected: None,
        };
        let probe = evaluate_probe(Some(observed), "不存在的键".to_string(), None);
        assert!(!probe.matched);
    }

    #[test]
    fn raw_code_prefers_platform_specific_field() {
        let windows = CaptureActivationResponse {
            key_name: "Alt_R".to_string(),
            linux_evdev_code: None,
            windows_vk_code: Some(0xA5),
            injected: None,
        };
        assert_eq!(windows.raw_code(), Some(0xA5));

        let linux = CaptureActivationResponse {
            key_name: "ISO_Level3_Shift".to_string(),
            linux_evdev_code: Some(100),
            windows_vk_code: None,
            injected: None,
        };
        assert_eq!(linux.raw_code(), Some(100));
    }

    #[test]
    fn windows_vk_roundtrip_common_keys() {
        for name in [
            "Alt_L",
            "Alt_R",
            "Shift_L",
            "Shift_R",
            "Control_L",
            "Control_R",
            "Super_L",
            "Super_R",
            "space",
            "Return",
            "Tab",
            "Escape",
            "BackSpace",
            "Delete",
            "Insert",
            "Home",
            "End",
            "Left",
            "Right",
            "Up",
            "Down",
            "q",
            "7",
            "F1",
            "F12",
        ] {
            let vk = key_name_to_windows_vk(name).unwrap_or_else(|| panic!("no VK for {name}"));
            assert_eq!(
                windows_vk_code_to_name(vk),
                name,
                "roundtrip failed for {name}"
            );
        }
    }

    #[test]
    fn windows_vk_altgr_aliases_normalize_to_right_alt() {
        for name in ["AltGr", "ISO_Level3_Shift"] {
            assert_eq!(
                key_name_to_windows_vk(name),
                key_name_to_windows_vk("Alt_R")
            );
        }
    }

    #[test]
    fn windows_vk_unknown_name_returns_none() {
        assert!(key_name_to_windows_vk("not-a-key").is_none());
        assert!(key_name_to_windows_vk("F13").is_none());
    }
}
