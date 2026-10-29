# Choose osu! stable or osu!lazer

osu!flow is one desktop app with a **Game client** choice in Settings. The normal Windows installer remains available; Linux has a separate AppImage and Debian package. Both modes use the same search, previews, filters, shortcut, Skip videos option and downloads. Stable remains the default for existing settings. Changing clients preserves each client's separate paths and launch settings.

Version 3.4.0 is a preview. Linux/macOS integration and the new lazer path still need confirmation with real games. Installers are available from the [3.4.0 experimental release](https://github.com/tempted14/osuflow/releases/tag/v3.4.0). Choose the Windows setup, Linux AppImage/deb, or the matching Mac architecture. These builds are unsigned.

## osu! stable on Windows

Use the normal x64 setup installer and keep **Game client → osu! stable**. Existing game detection, file associations, Songs-folder indexing, shortcut and optional beatmap link handler continue to work as before.

## osu! stable on Linux with osu-winello

Install [osu-winello](https://github.com/NelloKudo/osu-winello) and confirm the game runs through it first. In osu!flow choose **osu! stable**. The app reads osu-winello's selected game directory, including a custom location chosen with `osu-wine --changedir`, and reads stable's configured Songs path.

The launcher is found on `PATH`, in an exported `BINDIR`, or under `~/.local/bin`, so launching osu!flow from a desktop menu does not require a terminal's updated `PATH`. Imports use `osu-wine --osuhandler` with the archive as one argument. The wrapper selects its own patched Wine, runtime and configured prefix. osu!flow does not replace that with a prefix inferred from the game's folder, and does not retry a failed wrapper using a different Wine installation.

Leave **Wine prefix** and **Custom launch command** blank for this automatic osu-winello path. If discovery fails, expand **Wine and install settings** and use **Browse** to choose the host's `osu!.exe` or Songs folder. An explicit Wine prefix selects generic Wine; an explicit custom command selects that command. Use those only when you intend to override automatic routing.

If a stable import cannot be completed, the map is saved to Songs and the UI says **press F5 in song select**. See the [Wine/platform guide](experimental-wine.md) for X11, Wayland, macOS and login details.

## osu!lazer

1. Choose **Game client → osu!lazer** in Settings.
2. A running native client, a normal Windows/macOS install, or the installed Linux Flatpak can be detected automatically. For an AppImage or portable install, use **Browse** to choose your existing application and **Save lazer settings**. Make the Linux AppImage executable first. This selects a launcher; osu!flow does not install or replace the game.
3. Leave the data folder on automatic unless you moved lazer's storage. Under **Library and launch options**, choose the folder containing `client.realm` and `files` if needed. The game's own **Open osu! folder** option helps locate it. The `FullPath` redirect in `storage.ini` is respected.
4. Search, preview and download maps normally. The archive is passed to the selected native application, which uses lazer's supported file-import path. For Flatpak, only the archive is shared through its document portal (`--file-forwarding`); broad access to the home directory is not added.

**Sent to osu!lazer** means the launch/import request was sent. It is not proof that the game completed the import. If a launcher cannot be used, **Downloaded — open in osu!lazer** stays visible; choose **Show downloads**, then open or drag the archive into lazer. Settings also has **Open downloads folder**. Recovery archives stay in osu!flow's downloads area until its normal cleanup of files older than 24 hours at startup.

Lazer never uses the Songs/F5 fallback. osu!flow does not copy archives into lazer's hashed storage, open its Realm database, or modify game files. Existing-map detection reads beatmap headers from the available hashed files and watches for changes. Because lazer retains some deleted files for its recycle bin, those maps may remain marked installed until the files are cleaned up. Offline maps without an online set ID cannot be matched to mirror results.

Advanced custom launch commands run directly as an executable and argument list, with `{file}` as one archive argument. They do not expand shell variables or pipes. A lazer command is separate from the stable/Wine command. A successful command still confirms a request only.

## Validation still required

Automated tests use fake launchers and temporary libraries. Headless UI checks mock native commands and file pickers. They never start the installed game or download real maps into a player's library.

Before treating a configuration as confirmed, test it with its actual installer: installation/uninstallation, client switching, shortcut or Wayland CLI, search/preview, one intentionally chosen map import, recovery after a missing launcher, and library updates. Check fullscreen visibility and focus separately. No Linux/macOS tester is available yet.

Implementation references checked on October 29, 2026:

- [osu-winello launcher](https://github.com/NelloKudo/osu-winello/blob/main/osu-wine) and [handler implementation](https://github.com/NelloKudo/osu-winello/blob/main/osu-winello.sh).
- [Lazer desktop import arguments](https://github.com/ppy/osu/blob/master/osu.Desktop/Program.cs) and [archive IPC](https://github.com/ppy/osu/blob/master/osu.Game/IPC/ArchiveImportIPCChannel.cs).
- [Lazer user file storage](https://github.com/ppy/osu/wiki/User-file-storage) and [storage redirect](https://github.com/ppy/osu/blob/master/osu.Game/IO/OsuStorage.cs).
- [Flathub's lazer package](https://github.com/flathub/sh.ppy.osu/blob/master/sh.ppy.osu.yaml) and [Flatpak file forwarding](https://docs.flatpak.org/en/latest/flatpak-command-reference.html#flatpak-run).
