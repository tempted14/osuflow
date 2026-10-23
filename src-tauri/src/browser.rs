//! Browser plumbing for the link handler: registering QuickDirect as a browser Windows can
//! offer in Default apps, finding the browser it replaced, and passing ordinary links on to it.
//!
//! Windows doesn't let a program make itself the default browser - the user has to choose it
//! in Default apps - so registering only makes QuickDirect *selectable*. Registration is
//! written under HKEY_CURRENT_USER (no administrator rights), and the registry root is a
//! parameter so tests use a throwaway key and never touch a real browser registration.
//!
//! Who currently handles web links is asked of the shell (`AssocQueryString`) rather than read
//! from registry keys: where Windows stores that choice differs between versions, and on some
//! machines no explicit choice exists at all.

use reqwest::Url;
use std::{
    io,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

pub const APP_NAME: &str = "QuickDirect";
/// The class Windows uses for http(s) links while QuickDirect is the default browser.
pub const PROG_ID: &str = "QuickDirectURL";

const CLIENT_KEY: &str = r"Software\Clients\StartMenuInternet\QuickDirect";
const CAPABILITIES_KEY: &str = r"Software\Clients\StartMenuInternet\QuickDirect\Capabilities";
const REGISTERED_APPS_KEY: &str = r"Software\RegisteredApplications";

#[cfg(windows)]
pub use windows_registry::Registry;
#[cfg(all(test, windows))]
pub use windows_registry::{sandbox, shell_default_browser};
#[cfg(not(windows))]
pub use stub_registry::Registry;

#[cfg(windows)]
mod windows_registry {
    use super::*;
    use winreg::{
        RegKey,
        enums::{HKEY_CURRENT_USER, KEY_WRITE},
    };

    /// The executable Windows currently opens https (or, failing that, http) links with.
    pub fn shell_default_browser() -> Option<PathBuf> {
        use windows::{
            Win32::UI::Shell::{ASSOCF_IS_PROTOCOL, ASSOCSTR_EXECUTABLE, AssocQueryStringW},
            core::{PWSTR, w},
        };
        [w!("https"), w!("http")].into_iter().find_map(|scheme| {
            let mut buffer = [0u16; 1024];
            let mut length = buffer.len() as u32;
            // SAFETY: `buffer` outlives the call and `length` is its size in UTF-16 units.
            let result = unsafe {
                AssocQueryStringW(
                    ASSOCF_IS_PROTOCOL,
                    ASSOCSTR_EXECUTABLE,
                    scheme,
                    w!("open"),
                    Some(PWSTR(buffer.as_mut_ptr())),
                    &mut length,
                )
            };
            if result.is_err() {
                return None;
            }
            let text = String::from_utf16_lossy(&buffer[..(length as usize).min(buffer.len())]);
            let text = text.trim_end_matches('\0');
            (!text.is_empty()).then(|| PathBuf::from(text))
        })
    }

    /// Where the "who handles web links" answer comes from.
    enum DefaultSource {
        Shell,
        #[cfg(test)]
        Pretend(std::sync::Arc<std::sync::Mutex<Option<PathBuf>>>),
    }

    impl DefaultSource {
        fn executable(&self) -> Option<PathBuf> {
            match self {
                DefaultSource::Shell => shell_default_browser(),
                #[cfg(test)]
                DefaultSource::Pretend(cell) => cell.lock().unwrap().clone(),
            }
        }
    }

    pub struct Registry {
        /// Where QuickDirect registers itself: HKEY_CURRENT_USER in the app, a throwaway key
        /// in tests.
        user: RegKey,
        default_source: DefaultSource,
    }

    fn ignore_not_found(result: io::Result<()>) -> io::Result<()> {
        match result {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            other => other,
        }
    }

    impl Registry {
        pub fn system() -> Self {
            Self { user: RegKey::predef(HKEY_CURRENT_USER), default_source: DefaultSource::Shell }
        }

        /// Makes QuickDirect appear in Windows' Default apps as a browser. Safe to repeat; a
        /// changed `exe` path overwrites the old one.
        pub fn register(&self, exe: &Path) -> io::Result<()> {
            let exe = exe.display().to_string();

            let (class, _) = self.user.create_subkey(format!(r"Software\Classes\{PROG_ID}"))?;
            class.set_value("", &"QuickDirect URL")?;
            class.create_subkey("DefaultIcon")?.0.set_value("", &format!("{exe},0"))?;
            class
                .create_subkey(r"shell\open\command")?
                .0
                .set_value("", &format!("\"{exe}\" \"%1\""))?;

            let (client, _) = self.user.create_subkey(CLIENT_KEY)?;
            client.set_value("", &APP_NAME)?;
            client.create_subkey("DefaultIcon")?.0.set_value("", &format!("{exe},0"))?;
            client
                .create_subkey(r"shell\open\command")?
                .0
                .set_value("", &format!("\"{exe}\""))?;

            let (capabilities, _) = self.user.create_subkey(CAPABILITIES_KEY)?;
            capabilities.set_value("ApplicationName", &APP_NAME)?;
            capabilities.set_value(
                "ApplicationDescription",
                &"Downloads osu! beatmaps from links and passes every other link to your usual browser.",
            )?;
            let (urls, _) = capabilities.create_subkey("URLAssociations")?;
            urls.set_value("http", &PROG_ID)?;
            urls.set_value("https", &PROG_ID)?;

            let (apps, _) = self.user.create_subkey(REGISTERED_APPS_KEY)?;
            apps.set_value(APP_NAME, &CAPABILITIES_KEY)?;
            Ok(())
        }

        /// Removes everything `register` wrote. Missing pieces are fine.
        pub fn unregister(&self) -> io::Result<()> {
            ignore_not_found(self.user.delete_subkey_all(format!(r"Software\Classes\{PROG_ID}")))?;
            ignore_not_found(self.user.delete_subkey_all(CLIENT_KEY))?;
            if let Ok(apps) = self.user.open_subkey_with_flags(REGISTERED_APPS_KEY, KEY_WRITE) {
                ignore_not_found(apps.delete_value(APP_NAME))?;
            }
            Ok(())
        }

        pub fn is_registered(&self) -> bool {
            self.user.open_subkey(format!(r"Software\Classes\{PROG_ID}")).is_ok()
                && self.user.open_subkey(CAPABILITIES_KEY).is_ok()
                && self
                    .user
                    .open_subkey(REGISTERED_APPS_KEY)
                    .and_then(|apps| apps.get_value::<String, _>(APP_NAME))
                    .is_ok()
        }

        /// True when Windows hands web links to `me` (this program).
        pub fn is_default(&self, me: &Path) -> bool {
            self.default_source.executable().is_some_and(|exe| same_exe(&exe, me))
        }

        /// The browser currently opening links, unless that is `me` or it can't be determined.
        pub fn default_browser(&self, me: &Path) -> Option<PathBuf> {
            self.default_source.executable().filter(|exe| !same_exe(exe, me))
        }

        /// Test helper: pretend Windows hands web links to `exe` (`None` for no handler).
        #[cfg(test)]
        pub fn pretend_default(&self, exe: Option<&str>) {
            match &self.default_source {
                DefaultSource::Pretend(cell) => *cell.lock().unwrap() = exe.map(PathBuf::from),
                DefaultSource::Shell => panic!("pretend_default is only for sandboxed registries"),
            }
        }
    }

    #[cfg(test)]
    pub struct SandboxGuard(String);

    #[cfg(test)]
    impl Drop for SandboxGuard {
        fn drop(&mut self) {
            let hkcu = RegKey::predef(HKEY_CURRENT_USER);
            let _ = hkcu.delete_subkey_all(&self.0);
            // Removes the shared parent too once nothing else is using it.
            let _ = hkcu.delete_subkey(r"Software\QuickDirectTests");
        }
    }

    /// A registry that lives under a throwaway key in HKEY_CURRENT_USER, with a pretend
    /// default browser, so tests never touch real registrations or the user's choices.
    #[cfg(test)]
    pub fn sandbox() -> (Registry, SandboxGuard) {
        use std::sync::{
            Arc, Mutex,
            atomic::{AtomicU32, Ordering},
        };
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let name = format!(
            r"Software\QuickDirectTests\{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let (root, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(&name).unwrap();
        let registry = Registry { user: root, default_source: DefaultSource::Pretend(Arc::new(Mutex::new(None))) };
        (registry, SandboxGuard(name))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        const EXE: &str = r"C:\Apps\QuickDirect\quickdirect.exe";
        const CHROME: &str = r"C:\Program Files\Google\Chrome\Application\chrome.exe";

        #[test]
        fn registering_writes_what_windows_needs_to_offer_it_as_a_browser() {
            let (registry, _guard) = sandbox();
            assert!(!registry.is_registered());
            registry.register(Path::new(EXE)).unwrap();
            assert!(registry.is_registered());

            let command: String = registry
                .user
                .open_subkey(format!(r"Software\Classes\{PROG_ID}\shell\open\command"))
                .unwrap()
                .get_value("")
                .unwrap();
            assert_eq!(command, format!("\"{EXE}\" \"%1\""));

            let urls = registry.user.open_subkey(format!(r"{CAPABILITIES_KEY}\URLAssociations")).unwrap();
            assert_eq!(urls.get_value::<String, _>("http").unwrap(), PROG_ID);
            assert_eq!(urls.get_value::<String, _>("https").unwrap(), PROG_ID);

            let apps = registry.user.open_subkey(REGISTERED_APPS_KEY).unwrap();
            assert_eq!(apps.get_value::<String, _>(APP_NAME).unwrap(), CAPABILITIES_KEY);

            let client: String = registry
                .user
                .open_subkey(format!(r"{CLIENT_KEY}\shell\open\command"))
                .unwrap()
                .get_value("")
                .unwrap();
            assert_eq!(client, format!("\"{EXE}\""));
        }

        #[test]
        fn registering_again_updates_the_path_instead_of_failing() {
            let (registry, _guard) = sandbox();
            registry.register(Path::new(EXE)).unwrap();
            registry.register(Path::new(r"D:\Moved\quickdirect.exe")).unwrap();
            let command: String = registry
                .user
                .open_subkey(format!(r"Software\Classes\{PROG_ID}\shell\open\command"))
                .unwrap()
                .get_value("")
                .unwrap();
            assert!(command.contains(r"D:\Moved\quickdirect.exe"), "{command}");
        }

        #[test]
        fn unregistering_removes_everything_and_is_safe_to_repeat() {
            let (registry, _guard) = sandbox();
            registry.unregister().unwrap(); // nothing there yet
            registry.register(Path::new(EXE)).unwrap();
            registry.unregister().unwrap();
            assert!(!registry.is_registered());
            assert!(registry.user.open_subkey(format!(r"Software\Classes\{PROG_ID}")).is_err());
            assert!(registry.user.open_subkey(CLIENT_KEY).is_err());
            let apps = registry.user.open_subkey(REGISTERED_APPS_KEY).unwrap();
            assert!(apps.get_value::<String, _>(APP_NAME).is_err());
            registry.unregister().unwrap();
        }

        #[test]
        fn reports_the_browser_that_currently_opens_links() {
            let (registry, _guard) = sandbox();
            registry.pretend_default(Some(CHROME));
            assert_eq!(registry.default_browser(Path::new(EXE)), Some(PathBuf::from(CHROME)));
            assert!(!registry.is_default(Path::new(EXE)));
        }

        #[test]
        fn no_handler_means_no_browser_is_found() {
            let (registry, _guard) = sandbox();
            assert_eq!(registry.default_browser(Path::new(EXE)), None);
            assert!(!registry.is_default(Path::new(EXE)));
        }

        #[test]
        fn quickdirect_being_the_default_is_noticed_and_never_reported_as_the_old_browser() {
            let (registry, _guard) = sandbox();
            // Same program, different spelling: Windows paths are case-insensitive.
            registry.pretend_default(Some(r"c:\apps\quickdirect\QuickDirect.EXE"));
            assert!(registry.is_default(Path::new(EXE)));
            assert_eq!(registry.default_browser(Path::new(EXE)), None);
        }
    }
}

#[cfg(not(windows))]
mod stub_registry {
    use super::*;

    pub struct Registry;

    impl Registry {
        pub fn system() -> Self {
            Self
        }

        pub fn register(&self, _exe: &Path) -> io::Result<()> {
            Err(io::ErrorKind::Unsupported.into())
        }

        pub fn unregister(&self) -> io::Result<()> {
            Err(io::ErrorKind::Unsupported.into())
        }

        pub fn is_registered(&self) -> bool {
            false
        }

        pub fn is_default(&self, _me: &Path) -> bool {
            false
        }

        pub fn default_browser(&self, _me: &Path) -> Option<PathBuf> {
            None
        }
    }
}

/// Windows paths compare case-insensitively and may or may not carry the `\\?\` prefix.
pub fn same_exe(a: &Path, b: &Path) -> bool {
    let normal = |p: &Path| {
        std::fs::canonicalize(p)
            .unwrap_or_else(|_| p.to_path_buf())
            .to_string_lossy()
            .trim_start_matches(r"\\?\")
            .to_lowercase()
    };
    normal(a) == normal(b)
}

/// The browser to pass links to: the saved one if it still exists, else the first usable
/// fallback. QuickDirect itself is never chosen - opening a link with it would loop forever.
pub fn choose_browser(
    saved: Option<&Path>,
    fallbacks: &[PathBuf],
    me: &Path,
    exists: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    saved
        .map(Path::to_path_buf)
        .into_iter()
        .chain(fallbacks.iter().cloned())
        .find(|candidate| !same_exe(candidate, me) && exists(candidate))
}

/// Well-known browsers, used when the replaced browser couldn't be detected or has gone.
pub fn fallback_browsers() -> Vec<PathBuf> {
    let roots: Vec<PathBuf> = ["ProgramFiles(x86)", "ProgramFiles", "LOCALAPPDATA"]
        .iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
        .collect();
    [
        r"Microsoft\Edge\Application\msedge.exe",
        r"Google\Chrome\Application\chrome.exe",
        r"Mozilla Firefox\firefox.exe",
    ]
    .iter()
    .flat_map(|relative| roots.iter().map(move |root| root.join(relative)))
    .collect()
}

/// Opens `url` in `browser`. Only http(s) URLs are ever passed on, so a link can't smuggle a
/// command-line option or another kind of target to the browser.
pub fn open_in_browser(browser: &Path, url: &Url) -> io::Result<()> {
    if !crate::link_url::is_web_url(url) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "only http and https links can be opened"));
    }
    Command::new(browser)
        .arg(url.as_str())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn exe_paths_compare_like_windows_does() {
        assert!(same_exe(Path::new(r"C:\Apps\QD\quickdirect.exe"), Path::new(r"c:\apps\qd\QuickDirect.EXE")));
        assert!(same_exe(Path::new(r"\\?\C:\Apps\quickdirect.exe"), Path::new(r"C:\Apps\quickdirect.exe")));
        assert!(!same_exe(Path::new(r"C:\Apps\a.exe"), Path::new(r"C:\Apps\b.exe")));
    }

    #[test]
    fn chooses_the_saved_browser_then_a_fallback_but_never_itself() {
        let me = Path::new(r"C:\QD\quickdirect.exe");
        let saved = PathBuf::from(r"C:\Browsers\chrome.exe");
        let fallbacks = [PathBuf::from(r"C:\Browsers\edge.exe"), PathBuf::from(r"C:\Browsers\firefox.exe")];

        let everything_exists = |_: &Path| true;
        assert_eq!(choose_browser(Some(&saved), &fallbacks, me, everything_exists), Some(saved.clone()));

        let saved_is_gone = |p: &Path| p != saved;
        assert_eq!(choose_browser(Some(&saved), &fallbacks, me, saved_is_gone), Some(fallbacks[0].clone()));

        assert_eq!(choose_browser(None, &fallbacks, me, |p: &Path| p == fallbacks[1]), Some(fallbacks[1].clone()));
        assert_eq!(choose_browser(None, &fallbacks, me, |_: &Path| false), None);

        // If the "saved" browser turns out to be QuickDirect, skip it - otherwise every link loops.
        assert_eq!(choose_browser(Some(me), &fallbacks, me, everything_exists), Some(fallbacks[0].clone()));
        let only_me = [me.to_path_buf()];
        assert_eq!(choose_browser(Some(me), &only_me, me, everything_exists), None);
    }

    #[test]
    fn the_fallback_list_covers_the_common_browsers() {
        let list = fallback_browsers();
        for name in ["msedge.exe", "chrome.exe", "firefox.exe"] {
            assert!(list.iter().any(|p| p.ends_with(name)), "{name} missing from {list:?}");
        }
    }

    #[test]
    fn refuses_to_open_anything_but_web_links() {
        for url in ["file:///C:/Windows/notepad.exe", "javascript:alert(1)", "osu://b/1", "ftp://example.com/x"] {
            let error = open_in_browser(Path::new("browser-that-must-not-start.exe"), &Url::parse(url).unwrap()).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::InvalidInput, "{url}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn passes_the_link_to_the_browser_as_its_only_argument() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("opened.txt");
        // A stand-in "browser": writes the argument it received to a file.
        let fake = dir.path().join("fake-browser.cmd");
        std::fs::write(&fake, "@echo off\r\n>\"%~dp0opened.txt\" echo %1\r\n").unwrap();

        open_in_browser(&fake, &Url::parse("https://example.com/some/page").unwrap()).unwrap();

        let started = Instant::now();
        while !out.exists() && started.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(50));
        }
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(std::fs::read_to_string(&out).unwrap().trim(), "https://example.com/some/page");
    }

    #[test]
    fn a_missing_browser_is_an_error_not_a_hang() {
        let error = open_in_browser(Path::new(r"C:\no\such\browser.exe"), &Url::parse("https://example.com/").unwrap());
        assert!(error.is_err());
    }
}
