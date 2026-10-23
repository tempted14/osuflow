//! "Start with Windows": one value under HKCU's `Run` key that starts osu!flow quietly in the
//! tray. Windows lets the user turn such an entry off in Task Manager (a flag under
//! `StartupApproved`), which is honoured here, so the switch never claims what Windows won't do.

use std::{io, path::Path};

/// The flag the Run entry passes so a start at login shows no window.
pub const BACKGROUND_FLAG: &str = "--background";
const VALUE_NAME: &str = "osu!flow";
const WINDOWS_ROOT: &str = r"Software\Microsoft\Windows\CurrentVersion";

/// `"C:\path\osuflow.exe" --background`
fn command_for(exe: &Path) -> String {
    format!("\"{}\" {BACKGROUND_FLAG}", exe.display())
}

/// The program a Run command starts, whether or not its path is quoted.
fn command_exe(command: &str) -> Option<&str> {
    let command = command.trim();
    match command.strip_prefix('"') {
        Some(rest) => rest.split('"').next().filter(|path| !path.is_empty()),
        None => command.split(' ').next().filter(|path| !path.is_empty()),
    }
}

/// StartupApproved values start with a byte that is even while the entry is allowed to run.
fn flagged_disabled(approved: &[u8]) -> bool {
    approved.first().is_some_and(|flag| flag % 2 == 1)
}

pub struct Autostart {
    run_key: String,
    approved_key: String,
}

impl Autostart {
    pub fn system() -> Self {
        Self::under(WINDOWS_ROOT)
    }

    /// Lets tests use a throwaway key instead of the real one.
    fn under(root: &str) -> Self {
        Self {
            run_key: format!(r"{root}\Run"),
            approved_key: format!(r"{root}\Explorer\StartupApproved\Run"),
        }
    }
}

#[cfg(windows)]
impl Autostart {
    /// True when the entry exists, starts `exe`, and Windows hasn't been told to skip it.
    pub fn is_enabled(&self, exe: &Path) -> bool {
        use winreg::{RegKey, enums::HKEY_CURRENT_USER};
        let user = RegKey::predef(HKEY_CURRENT_USER);
        let Some(command) = user
            .open_subkey(&self.run_key)
            .ok()
            .and_then(|key| key.get_value::<String, _>(VALUE_NAME).ok())
        else {
            return false;
        };
        let starts_us = command_exe(&command).is_some_and(|started| crate::browser::same_exe(Path::new(started), exe));
        let disabled = user
            .open_subkey(&self.approved_key)
            .ok()
            .and_then(|key| key.get_raw_value(VALUE_NAME).ok())
            .is_some_and(|value| flagged_disabled(&value.bytes));
        starts_us && !disabled
    }

    /// Adds (or repairs) the entry and clears a "disabled in Task Manager" flag.
    pub fn enable(&self, exe: &Path) -> io::Result<()> {
        use winreg::{RegKey, enums::{HKEY_CURRENT_USER, KEY_WRITE}};
        let user = RegKey::predef(HKEY_CURRENT_USER);
        let (run, _) = user.create_subkey(&self.run_key)?;
        run.set_value(VALUE_NAME, &command_for(exe))?;
        if let Ok(approved) = user.open_subkey_with_flags(&self.approved_key, KEY_WRITE) {
            let _ = approved.delete_value(VALUE_NAME);
        }
        Ok(())
    }

    pub fn disable(&self) -> io::Result<()> {
        use winreg::{RegKey, enums::{HKEY_CURRENT_USER, KEY_WRITE}};
        let user = RegKey::predef(HKEY_CURRENT_USER);
        let Ok(run) = user.open_subkey_with_flags(&self.run_key, KEY_WRITE) else { return Ok(()) };
        match run.delete_value(VALUE_NAME) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        }
    }
}

#[cfg(not(windows))]
impl Autostart {
    pub fn is_enabled(&self, _exe: &Path) -> bool {
        false
    }

    pub fn enable(&self, _exe: &Path) -> io::Result<()> {
        Err(io::ErrorKind::Unsupported.into())
    }

    pub fn disable(&self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use winreg::{RegKey, enums::HKEY_CURRENT_USER};

    /// A throwaway key under HKCU that is deleted again when the test ends.
    struct Sandbox {
        root: String,
        autostart: Autostart,
    }

    impl Sandbox {
        fn new() -> Self {
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let root = format!(r"Software\OsuFlowTests\autostart-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::SeqCst));
            Self { autostart: Autostart::under(&root), root }
        }
    }

    impl Drop for Sandbox {
        fn drop(&mut self) {
            let user = RegKey::predef(HKEY_CURRENT_USER);
            let _ = user.delete_subkey_all(&self.root);
            let _ = user.delete_subkey(r"Software\OsuFlowTests");
        }
    }

    const EXE: &str = r"C:\Apps\osu!flow\osuflow.exe";

    #[test]
    fn the_command_starts_the_app_quietly_and_can_be_read_back() {
        assert_eq!(command_for(Path::new(EXE)), r#""C:\Apps\osu!flow\osuflow.exe" --background"#);
        assert_eq!(command_exe(&command_for(Path::new(EXE))), Some(EXE));
        assert_eq!(command_exe(r"C:\plain\app.exe --x"), Some(r"C:\plain\app.exe"));
        assert_eq!(command_exe(""), None);
        assert_eq!(command_exe("\"\""), None);
    }

    #[test]
    fn startup_approved_flags_follow_windows_convention() {
        assert!(!flagged_disabled(&[2, 0, 0, 0]));
        assert!(!flagged_disabled(&[6, 0, 0, 0]));
        assert!(flagged_disabled(&[3, 0, 0, 0]));
        assert!(!flagged_disabled(&[]));
    }

    #[test]
    fn enabling_and_disabling_round_trip() {
        let sandbox = Sandbox::new();
        let exe = Path::new(EXE);
        assert!(!sandbox.autostart.is_enabled(exe));
        sandbox.autostart.enable(exe).unwrap();
        assert!(sandbox.autostart.is_enabled(exe));
        sandbox.autostart.disable().unwrap();
        assert!(!sandbox.autostart.is_enabled(exe));
        sandbox.autostart.disable().unwrap(); // already gone: not an error
    }

    #[test]
    fn an_entry_that_starts_a_different_copy_is_not_ours() {
        let sandbox = Sandbox::new();
        sandbox.autostart.enable(Path::new(r"D:\Old\osuflow.exe")).unwrap();
        assert!(!sandbox.autostart.is_enabled(Path::new(EXE)));
        sandbox.autostart.enable(Path::new(EXE)).unwrap();
        assert!(sandbox.autostart.is_enabled(Path::new(EXE)), "enabling repairs a stale entry");
    }

    #[test]
    fn turning_it_off_in_task_manager_is_respected_and_enabling_clears_it() {
        let sandbox = Sandbox::new();
        let exe = Path::new(EXE);
        sandbox.autostart.enable(exe).unwrap();
        let user = RegKey::predef(HKEY_CURRENT_USER);
        let (approved, _) = user.create_subkey(&sandbox.autostart.approved_key).unwrap();
        approved
            .set_raw_value(VALUE_NAME, &winreg::RegValue { bytes: vec![3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0].into(), vtype: winreg::enums::REG_BINARY })
            .unwrap();
        assert!(!sandbox.autostart.is_enabled(exe), "Windows will not start it, so it must not read as on");
        sandbox.autostart.enable(exe).unwrap();
        assert!(sandbox.autostart.is_enabled(exe));
    }
}
