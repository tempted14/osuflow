# Headless UI checks and screenshots

These scripts render the **built React frontend**, with a mocked Tauri bridge, in an isolated headless
Edge/Chromium browser. They never start osu!flow, call its Rust backend, register shortcuts, or import
maps. Download-key checks record mock calls only. Browser audio is muted. This does not test the native
windows or actual imports.

Use Node **22 or newer** (native `fetch` and `WebSocket`), install the app's dependencies, and run
`npm run build` first. No additional npm packages are needed. Commands below run from the app repo root.

```powershell
node tools/headless/regress.mjs                 # all 108 recovered checks
node tools/headless/platforms.mjs               # platform settings + Songs/F5 fallback (mocked)
node tools/headless/clients.mjs                 # client switching, Browse, lazer recovery (mocked)
node tools/headless/regress.mjs topbar hover    # optional groups
node tools/headless/real-fixture.mjs           # refresh 16 ranked sets; check 64 cover URLs
$env:SETS = 'real-sets.json'
$env:SHOT_FORMAT = 'webp'
$env:SCALE = '2'
node tools/headless/shots.mjs                  # four product shots
Remove-Item Env:SETS, Env:SHOT_FORMAT, Env:SCALE

$env:SITE = 'C:\path\to\osuflow.github.io'
node tools/headless/sitecheck.mjs              # local website
node tools/headless/og.mjs                     # website social card (1200x630)
$env:OG_WIDTH = '1280'; $env:OG_HEIGHT = '640'
$env:OG_OUTPUT = 'C:\path\to\social-preview.png'
node tools/headless/og.mjs                     # GitHub social preview
node tools/headless/icon.mjs                   # original icon and size preview

$env:BASE = 'https://osuflow.github.io/'
node tools/headless/sitecheck.mjs              # deployed site, read only
```

Regression groups: `topbar`, `panel`, `hover`, `small`, `features`. Leave `SETS` unset for regression
checks: the original `sets.json` fixture has the mixed modes/difficulties those checks exercise.
`fake-sets.json` preserves the old synthetic fixture. `real-sets.json` is for product screenshots.

Configuration:

| Variable | Default / purpose |
| --- | --- |
| `EDGE_PATH` | Auto-detect Edge on Windows; common Edge/Chrome/Chromium paths on macOS/Linux. Set the executable's absolute path to override. |
| `DIST` | App repo `dist/`; accepts an absolute override. |
| `SETS` | `sets.json`; fixture path relative to this folder, or absolute. |
| `OUT_DIR` | `tools/headless/.artifacts/` (ignored); screenshot and report images. |
| `SHOT_FORMAT`, `SCALE` | `png`, `1`; screenshots support `png`, `webp`, `jpeg`. |
| `TAG` | `reg`; regression screenshot filename prefix. |
| `SITE` | Sibling checkout `../osuflow.github.io`; local site folder for `sitecheck` and `og`. |
| `BASE` | Unset: serve `SITE` locally. Set a trailing-slash URL to check a deployed site. |
| `SECTIONS` | Set to capture individual desktop/phone sections during `sitecheck`. |
| `OG_WIDTH`, `OG_HEIGHT` | `1200`, `630`; use `1280`, `640` for GitHub. |
| `OG_OUTPUT` | `.artifacts/og.png`; explicit output filename. |

Servers bind only to localhost on available ports. Every run creates a fresh temporary browser
profile and closes/removes only that profile. Generated files are not published automatically.
Failures set a nonzero exit code, including browser startup errors and exceptions inside checks.

The 16 screenshot sets are selected by ID from the most-played ranked osu!standard search. Refreshing
keeps the mirror's original objects and writes provenance/cover checks to `real-sets.meta.json`. No
NSFW sets are accepted. Review cover artwork visually before publishing refreshed screenshots.

For the app README, copy the four screenshots to `docs/images/{overlay,filters,show,settings}.webp`.
The 1280x640 social preview belongs at `docs/images/social-preview.png`. After publishing the changes,
the maintainer can upload that image in GitHub **Settings → General → Social preview**. Check the README
anonymously and run `sitecheck` with `BASE` after the website deployment finishes.
