# osu!flow

Search and download osu! beatmaps without leaving the game. Press a hotkey in osu! (stable), search,
click a map, and a moment later it is in song select. A free take on osu!direct, built as a small
background app for Windows 11.

> osu!flow is not affiliated with or endorsed by ppy. It uses public beatmap mirrors, never touches
> the osu! client's memory or network, and hands maps over exactly like double-clicking an `.osz`.

## Install

1. Download **osu.flow_x.y.z_x64-setup.exe** from the [Releases](../../releases) page.
2. Run it. It installs for your user only (no administrator rights) and adds a Start menu entry.
   While osu!flow is new, Windows SmartScreen may say it protects your PC: choose **More info**,
   then **Run anyway**. See the [code signing policy](#code-signing-policy) below.
3. osu!flow opens its settings window and then waits in the tray. Press `Ctrl+Shift+D` in osu!.

To remove it, use Windows' *Installed apps*. That also removes the *Start with Windows* entry and
the browser registration, if you turned either on.

## Using it

- **Search:** press `Ctrl+Shift+D` in osu!. A frosted-glass card opens over the game with your
  search. Type to search (or paste an osu! beatmap link or ID to jump straight to that map), arrow
  keys to move, `Enter` to download, `Space` to preview, `Esc` to close.
- **Sort and filter:** pick a game mode and status, then one of ten sort orders (ranked date, last
  updated, submitted, plays, favourites, stars, BPM, length, title, artist). The button beside the
  menu says which way it runs ("Newest first", "Hardest first", "A to Z") and reverses it. The *Stars*
  menu keeps maps with a difficulty in an osu! band, Easy to Expert+. The beatmap mirrors can't filter by
  stars themselves, so that filter works on the results already fetched; for a rare band osu!flow fetches
  a few more pages and then offers *Keep searching*.
- **Background:** osu!flow lives in the system tray. Closing the settings window leaves it running;
  the tray menu has *Search maps*, *Settings*, *Start with Windows* and *Quit*. With *Start with
  Windows* on, it starts quietly at login and shows no window.
- **Beatmap links (optional):** clicking a beatmap link in osu! multiplayer, chat or Discord can
  download the map instead of opening a web page. Turn it on in Settings, then choose osu!flow in
  Windows' Default apps (Windows doesn't let an app do that last step itself). Every other link
  still goes to your usual browser.
- **Settings:** the shortcut, *Skip videos*, *Start with Windows* and the link handler.

## Code signing policy

Free code signing provided by [SignPath.io](https://about.signpath.io), certificate by
[SignPath Foundation](https://signpath.org).

- **Authors, reviewers and approvers:** [tempted14](https://github.com/tempted14), the only maintainer.
- Only installers built by this repository's [release workflow](.github/workflows/release.yml) from
  the source in this repository are signed, and every release is approved by hand before it is signed.
  Releases that are signed say so in their notes.

## Privacy

osu!flow collects nothing and has no telemetry. It contacts the beatmap mirrors (osu.direct and
catboy.best) to search for and download the maps you ask for, and osu!'s own servers (assets.ppy.sh and
b.ppy.sh) for cover images and previews. This program will not transfer any information to other
networked systems unless specifically requested by the user or the person installing or operating it.

## How it is built

Tauri 2 (Rust) with a React + TypeScript interface.

- **Frosted glass:** Windows 11's own acrylic backdrop, applied through DWM behind each window, with
  rounded corners and a dark frame. The page only paints translucent layers on top, so the blur costs
  the app nothing. Windows 10 gets solid colours instead.
- **Lean by design:** the search overlay is the only window that always exists (so the shortcut
  answers instantly). The settings window is built when opened and destroyed when closed, and the
  link toast only exists while link handling is on. The overlay lets go of its cover images shortly
  after it is hidden. All web views share one renderer process.
- **Fast hand-off:** downloads try catboy.best first (the same file about three times faster than
  osu.direct), open the connection while you are still searching, name the file from the search
  result instead of asking a mirror, and notice that osu! has taken the map within a few milliseconds.
  The import itself goes through Windows' `.osz` association, the same call as double-clicking the
  file, guarded so it never launches osu! while it has a dialog open.

## Performance

Measured on one PC (Windows 11, release builds, same method before and after), against the app's
previous version:

| | Before | After |
|---|---|---|
| Shortcut works after launch | 0.5-0.7 s | 0.3 s |
| Memory, settings window open | 139 MB, 9 processes | 104 MB, 7 processes |
| Memory, sitting in the tray | 137 MB | 100 MB (86 MB after a silent start at login) |
| Idle CPU | about 0 % | about 0 % |
| First time the search card opens: results on screen | 1.36 s | 0.12 s |
| A 4.5 MB map from "download" to osu! having it | 1.85 s | 0.75 s |

The first search runs at start-up, while the card is still hidden, so the first time you press the
shortcut the results are already there. Opening it again within a minute doesn't search again.

The transfer time is dominated by the mirror: catboy.best sent the same 3.9 MB file in 0.4 s where
osu.direct took 1.1-1.3 s. The rest was a name lookup that ran after the download (about 260 ms)
and a 400 ms poll for osu! picking the file up; both are gone.

## Building

```
npm install
npm run tauri build -- --no-bundle     # target/release/osuflow.exe
npm test                               # frontend tests
cd src-tauri && cargo test --lib       # backend tests
cd src-tauri && cargo test --lib live_ -- --ignored --nocapture --test-threads=1   # real mirrors, this PC's osu!
```

Settings live in `%APPDATA%\com.osuflow.app\settings.json`; a settings file from the app's earlier
name (QuickDirect) is copied across on first start.

## License

[MIT](LICENSE). osu! and its assets belong to ppy; none of them are included here.
