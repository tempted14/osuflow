//! Login-entry generation and IO. Tests use temporary files, never the real login directories.
use std::{
    io,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug)]
pub enum Kind {
    #[cfg(any(not(target_os = "macos"), test))]
    Desktop,
    #[cfg(any(target_os = "macos", test))]
    LaunchAgent,
}

pub struct LoginEntry {
    path: PathBuf,
    kind: Kind,
}

#[cfg(any(not(target_os = "macos"), test))]
fn desktop_arg(path: &Path) -> io::Result<String> {
    let text = path.to_str().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "The app path must be valid UTF-8",
        )
    })?;
    if text.contains(['\n', '\r', '\0']) {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    // Desktop Entry Exec quoting is not shell quoting. Percent signs are field codes even in quotes.
    let mut result = String::from("\"");
    for ch in text.chars() {
        match ch {
            '%' => result.push_str("%%"),
            '\\' => result.push_str("\\\\\\\\"),
            '"' | '$' | '`' => {
                result.push_str("\\\\");
                result.push(ch);
            }
            _ => result.push(ch),
        }
    }
    result.push('"');
    Ok(result)
}

#[cfg(any(target_os = "macos", test))]
fn xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub fn contents(exe: &Path, kind: Kind) -> io::Result<String> {
    match kind {
        #[cfg(any(not(target_os = "macos"), test))]
        Kind::Desktop => Ok(format!(
            "[Desktop Entry]\nType=Application\nName=osu!flow\nExec={} --background\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
            desktop_arg(exe)?
        )),
        #[cfg(any(target_os = "macos", test))]
        Kind::LaunchAgent => {
            let text = exe.to_str().ok_or(io::ErrorKind::InvalidInput)?;
            if text
                .chars()
                .any(|ch| ch < ' ' && !matches!(ch, '\t' | '\n' | '\r'))
            {
                return Err(io::ErrorKind::InvalidInput.into());
            }
            Ok(format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict>\n<key>Label</key><string>com.osuflow.app</string>\n<key>ProgramArguments</key><array><string>{}</string><string>--background</string></array>\n<key>RunAtLoad</key><true/>\n<key>LimitLoadToSessionType</key><string>Aqua</string>\n</dict></plist>\n",
                xml(text)
            ))
        }
    }
}

impl LoginEntry {
    pub fn new(path: PathBuf, kind: Kind) -> Self {
        Self { path, kind }
    }
    pub fn is_enabled(&self, exe: &Path) -> bool {
        let Ok(text) = std::fs::read_to_string(&self.path) else {
            return false;
        };
        let Ok(expected) = contents(exe, self.kind) else {
            return false;
        };
        match self.kind {
            #[cfg(any(not(target_os = "macos"), test))]
            Kind::Desktop => {
                let value = |key: &str| {
                    text.lines()
                        .filter_map(|line| line.trim().split_once('='))
                        .filter(|(name, _)| *name == key)
                        .map(|(_, value)| value.trim())
                        .next_back()
                };
                let expected_exec = expected.lines().find_map(|line| line.strip_prefix("Exec="));
                value("Exec") == expected_exec
                    && value("Hidden") != Some("true")
                    && value("X-GNOME-Autostart-enabled") != Some("false")
            }
            #[cfg(any(target_os = "macos", test))]
            Kind::LaunchAgent => text == expected,
        }
    }
    pub fn enable(&self, exe: &Path) -> io::Result<()> {
        let content = contents(exe, self.kind)?;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let temporary = self.path.with_extension("tmp");
        std::fs::write(&temporary, content)?;
        std::fs::rename(temporary, &self.path)
    }
    pub fn disable(&self) -> io::Result<()> {
        match std::fs::remove_file(&self.path) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn desktop_entry_quotes_reserved_characters_and_never_invokes_a_shell() {
        let text = contents(
            Path::new("/opt/Apps $USER/100% \"osu!flow\""),
            Kind::Desktop,
        )
        .unwrap();
        assert!(text.contains("100%%"));
        assert!(text.contains("\\\\$USER"));
        assert!(text.contains("\\\\\"osu!flow\\\\\""));
        assert!(text.contains(" --background\n"));
        assert!(!text.contains("sh -c"));
        assert!(contents(Path::new("/bad\nExec=other"), Kind::Desktop).is_err());
    }
    #[test]
    fn plist_arguments_escape_xml_and_start_quietly() {
        let text = contents(
            Path::new("/Apps/osu!flow & <test>.app/Contents/MacOS/osuflow"),
            Kind::LaunchAgent,
        )
        .unwrap();
        assert!(text.contains("&amp; &lt;test&gt;"));
        assert!(text.contains("<string>--background</string>"));
        assert!(text.contains("<key>RunAtLoad</key><true/>"));
    }
    #[test]
    fn login_files_can_be_enabled_repaired_and_disabled_in_a_sandbox() {
        for (name, kind) in [
            ("osuflow.desktop", Kind::Desktop),
            ("com.osuflow.app.plist", Kind::LaunchAgent),
        ] {
            let root = tempfile::tempdir().unwrap();
            let path = root.path().join(name);
            let entry = LoginEntry::new(path.clone(), kind);
            let exe = Path::new("/opt/Apps/osuflow");
            assert!(!entry.is_enabled(exe));
            entry.enable(exe).unwrap();
            assert!(entry.is_enabled(exe));
            assert!(!entry.is_enabled(Path::new("/some/other/osuflow")));
            if matches!(kind, Kind::Desktop) {
                let mut text = std::fs::read_to_string(&path).unwrap();
                text.push_str("Hidden=true\n");
                std::fs::write(&path, text).unwrap();
                assert!(!entry.is_enabled(exe));
                entry.enable(exe).unwrap();
                assert!(entry.is_enabled(exe));
            }
            entry.disable().unwrap();
            entry.disable().unwrap();
            assert!(!entry.is_enabled(exe));
        }
    }
}
