# Experimental osu! stable support on Linux and macOS

Version 3.4.0 adds an experimental Wine path. **It still needs testing on real Linux and macOS machines.** CI builds and unit tests do not confirm that an overlay works above a game, that a Wine wrapper accepts an archive, or that an installer works on a particular desktop. osu!lazer is outside this feature's scope.

Until a release is published, test installers are attached to successful runs of the repository's **Release** workflow on the `codex/wine-platform-support` branch. The website continues to offer the released Windows version.

## Install and choose the game

Linux builds include an x86_64 AppImage and a Debian package. Make the AppImage executable, keep it in a permanent location, and run it; moving it later requires turning **Start at login** off and on again. Desktop tray support varies: GNOME may need its AppIndicator extension. Use the tray menu's **Search maps** entry to open search; Linux tray clicks are not used.

macOS builds include separate Apple Silicon (`aarch64`) and Intel (`x64`) disk images. Move osu!flow to Applications before enabling **Start at login**. These builds are unsigned and unnotarized. After trying to open a downloaded build that you trust, use **System Settings → Privacy & Security → Open Anyway**, then confirm **Open**, following [Apple's instructions](https://support.apple.com/en-us/102445). Do not disable Gatekeeper globally.

Install and run osu! stable through your existing Wine setup first. [osu-winello](https://github.com/NelloKudo/osu-winello) is a Linux option; its current configuration can point to a custom game directory and Songs folder. macOS Wine wrappers, including Wineskin and CrossOver, remain best-effort and may need a custom launch command.

Open osu!flow Settings. If automatic detection fails, expand **Wine and install settings**:

| Setting | What to enter |
| --- | --- |
| osu!.exe | The executable's absolute **host** path, such as `/home/player/Games/osu!/osu!.exe`; not a `C:\` path or a Wine loader. |
| Songs folder | The actual host folder used by the game, including a custom Songs location. |
| Wine prefix | An existing absolute prefix folder containing `drive_c`. Leave blank for automatic detection. |
| Custom launch command | An optional executable and arguments. `{file}` becomes one archive argument. Quote paths containing spaces; this runs directly, with no shell expansion. |

For example, `osu-wine --osuhandler "{file}"` uses an available osu-winello wrapper. A wrapper outside `PATH` needs its full quoted path. For a particular generic Wine install, set its prefix and use `wine start /unix "{file}"`. A macOS wrapper may need its own executable and bottle arguments; use the wrapper's documented file-opening command. Do not include shell assignments, pipes, `~`, or `$HOME` in the command; use the Wine prefix field and absolute paths instead. Clear a field to return it to automatic detection.

Detection checks a configured prefix first, running osu! Wine processes, osu-winello's game-directory pointer, and common prefix locations. Windows `C:` and `Z:` paths in the game's configuration are translated to host paths; additional drive mappings are resolved from `dosdevices` when available. A configured Songs folder takes precedence.

## Open search and import a map

On Linux X11 the default shortcut is **Ctrl+Shift+D**. On macOS it is **Command+Shift+D**. The overlay is implemented as an NSPanel on macOS so it can join fullscreen Spaces; real-game behavior still needs confirmation.

On Wayland, global shortcut registration is disabled. Bind the installed executable with `--toggle` in the desktop/compositor's shortcut settings, for example `/home/player/Applications/osuflow.AppImage --toggle`. An existing instance receives the command; a first launch opens search too. Tray → **Search maps** is another option. Wayland compositors may ignore always-on-top requests, so borderless/windowed osu! may work better than fullscreen.

The importer tries `osu-wine --osuhandler` when available, then generic `wine start /unix` with the detected prefix. An explicit prefix bypasses automatic osu-winello routing to avoid opening a different installation. A custom command replaces these automatic launch choices.

The hand-off checks whether osu! consumes the archive. If launching fails or the archive remains unconsumed, it falls back to copying it into the selected Songs folder. **When the UI says “Saved to Songs — press F5 in song select”, return to song select and press F5.** Copying a map to Songs is not confirmation that the game imported it.

## Limits and testing record

- Unix readiness uses identified osu! processes. It cannot inspect osu!'s dialogs, detect a hung game window, return focus to the game, or choose its monitor as Windows can. Multiple detected game processes block automatic hand-off rather than guessing.
- Fullscreen visibility, game focus, tray integration and Wine file associations differ across desktops and wrappers. No Linux/macOS configuration is confirmed yet.
- Beatmap link-handler registration is hidden and disabled outside Windows in this version.
- **Start at login** creates a user `.desktop` autostart entry on Linux or a user LaunchAgent plist on macOS. Changes take effect at the next login; the app must stay at the saved path.
- CI runs frontend tests, backend tests and Rust checks on Windows, Ubuntu and macOS, then builds Windows NSIS, Linux AppImage/deb, and both macOS disk images. Native UI and game imports are not automated.

Before publishing, a tester on each offered platform should record the installer/architecture, OS and desktop, Wine wrapper/version and whether these work:

- [ ] Install, launch and quit without starting a second copy.
- [ ] Open search using the hotkey or Wayland `--toggle`, both with and without osu! running.
- [ ] Verify overlay placement, fullscreen visibility, keyboard input, dismissal and game focus.
- [ ] Search and preview a map; import one explicitly chosen test map through the same Wine install.
- [ ] Confirm the Songs fallback and its F5 instruction without a working launch helper.
- [ ] Confirm login enable/disable after a new login and uninstall/cleanup behavior.

Keep the release experimental and record only configurations actually confirmed by testers. Add website Linux/macOS download buttons after the release and corresponding assets exist.
