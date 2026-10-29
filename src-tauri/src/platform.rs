//! Small, explicit platform capabilities shared with the interface.
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformInfo {
    pub os: &'static str,
    pub wayland: bool,
    pub default_hotkey: &'static str,
    pub experimental: bool,
}

pub fn wayland() -> bool {
    cfg!(target_os = "linux")
        && (std::env::var_os("WAYLAND_DISPLAY").is_some_and(|value| !value.is_empty())
            || std::env::var("XDG_SESSION_TYPE")
                .is_ok_and(|value| value.eq_ignore_ascii_case("wayland")))
}

pub fn info() -> PlatformInfo {
    PlatformInfo {
        os: std::env::consts::OS,
        wayland: wayland(),
        default_hotkey: crate::hotkey::DEFAULT_HOTKEY,
        experimental: !cfg!(windows),
    }
}

pub fn autostart_label() -> &'static str {
    if cfg!(windows) {
        "Start with Windows"
    } else {
        "Start at login"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capabilities_do_not_claim_native_support_on_unix() {
        let info = info();
        assert_eq!(info.experimental, !cfg!(windows));
        assert_eq!(info.default_hotkey, crate::hotkey::DEFAULT_HOTKEY);
        if cfg!(windows) {
            assert!(!info.wayland);
            assert_eq!(autostart_label(), "Start with Windows");
        } else {
            assert_eq!(autostart_label(), "Start at login");
        }
    }
}
