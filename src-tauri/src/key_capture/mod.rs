//! 短时捕获用户按下的物理键，用于设置激活录音键。

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
pub use linux::LinuxKeyCapture;
#[cfg(target_os = "windows")]
pub use windows::WindowsKeyCapture;

#[cfg(target_os = "linux")]
pub type PlatformKeyCapture = LinuxKeyCapture;
#[cfg(target_os = "windows")]
pub type PlatformKeyCapture = WindowsKeyCapture;

use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureActivationResponse {
    pub key_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linux_evdev_code: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub windows_vk_code: Option<u16>,
}

/// 平台激活键捕获的 trait seam。
///
/// 由 Linux adapter 实现；`capture()` 为同步阻塞方法，调用方负责在独立线程执行。
pub trait KeyCapture: Send {
    fn capture(&mut self) -> Result<CaptureActivationResponse, String>;
}

/// 将常见 evdev 码映射为 `xmodmap -pke` 中可能出现的 keysym 名称；未知则 `evdev_<code>`。
pub fn evdev_code_to_keysym_name(code: u16) -> String {
    match code {
        1 => "Escape".to_string(),
        2 => "1".to_string(),
        3 => "2".to_string(),
        4 => "3".to_string(),
        5 => "4".to_string(),
        6 => "5".to_string(),
        7 => "6".to_string(),
        8 => "7".to_string(),
        9 => "8".to_string(),
        10 => "9".to_string(),
        11 => "0".to_string(),
        12 => "minus".to_string(),
        13 => "equal".to_string(),
        14 => "BackSpace".to_string(),
        15 => "Tab".to_string(),
        16 => "q".to_string(),
        17 => "w".to_string(),
        18 => "e".to_string(),
        19 => "r".to_string(),
        20 => "t".to_string(),
        21 => "y".to_string(),
        22 => "u".to_string(),
        23 => "i".to_string(),
        24 => "o".to_string(),
        25 => "p".to_string(),
        26 => "bracketleft".to_string(),
        27 => "bracketright".to_string(),
        28 => "Return".to_string(),
        29 => "Control_L".to_string(),
        30 => "a".to_string(),
        31 => "s".to_string(),
        32 => "d".to_string(),
        33 => "f".to_string(),
        34 => "g".to_string(),
        35 => "h".to_string(),
        36 => "j".to_string(),
        37 => "k".to_string(),
        38 => "l".to_string(),
        39 => "semicolon".to_string(),
        40 => "apostrophe".to_string(),
        41 => "grave".to_string(),
        42 => "Shift_L".to_string(),
        43 => "backslash".to_string(),
        44 => "z".to_string(),
        45 => "x".to_string(),
        46 => "c".to_string(),
        47 => "v".to_string(),
        48 => "b".to_string(),
        49 => "n".to_string(),
        50 => "m".to_string(),
        51 => "comma".to_string(),
        52 => "period".to_string(),
        53 => "slash".to_string(),
        54 => "Shift_R".to_string(),
        55 => "KP_Multiply".to_string(),
        56 => "Alt_L".to_string(),
        57 => "space".to_string(),
        58 => "Caps_Lock".to_string(),
        59 => "F1".to_string(),
        60 => "F2".to_string(),
        61 => "F3".to_string(),
        62 => "F4".to_string(),
        63 => "F5".to_string(),
        64 => "F6".to_string(),
        65 => "F7".to_string(),
        66 => "F8".to_string(),
        67 => "F9".to_string(),
        68 => "F10".to_string(),
        69 => "Num_Lock".to_string(),
        70 => "Scroll_Lock".to_string(),
        87 => "F11".to_string(),
        88 => "F12".to_string(),
        96 => "KP_Enter".to_string(),
        97 => "Control_R".to_string(),
        98 => "KP_Divide".to_string(),
        99 => "Print".to_string(),
        100 => "ISO_Level3_Shift".to_string(),
        102 => "Home".to_string(),
        103 => "Up".to_string(),
        104 => "Prior".to_string(),
        105 => "Left".to_string(),
        106 => "Right".to_string(),
        107 => "End".to_string(),
        108 => "Down".to_string(),
        109 => "Next".to_string(),
        110 => "Insert".to_string(),
        111 => "Delete".to_string(),
        113 => "Mute".to_string(),
        114 => "VolumeDown".to_string(),
        115 => "VolumeUp".to_string(),
        125 => "Super_L".to_string(),
        126 => "Super_R".to_string(),
        _ => format!("evdev_{code}"),
    }
}

/// Windows 虚拟键码（Win32 `VK_*` 常量，与 `winuser.h` 一致）。
pub mod windows_vk {
    pub const VK_LMENU: u16 = 0x12;
    pub const VK_RMENU: u16 = 0xA5;
    pub const VK_LSHIFT: u16 = 0xA0;
    pub const VK_RSHIFT: u16 = 0xA1;
    pub const VK_LCONTROL: u16 = 0xA2;
    pub const VK_RCONTROL: u16 = 0xA3;
    pub const VK_LWIN: u16 = 0x5B;
    pub const VK_RWIN: u16 = 0x5C;
}

/// 将 Windows VK 码映射为与 Linux keysym 风格一致的按键名称；未知则 `vk_0xXX`。
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

/// 将按键名称解析为 Windows VK 码；与 [`windows_vk_code_to_name`] 互逆，`AltGr`/`ISO_Level3_Shift` 视作右 Alt。
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
