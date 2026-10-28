// Renders the built React UI with a completely mocked native bridge. No real downloads or app launch.
import fs from 'node:fs';
import path from 'node:path';
import { HERE, ROOT, output, serve, sleep, startBrowser } from './browser.mjs';
export { sleep } from './browser.mjs';
const DIST = path.resolve(process.env.DIST ?? path.join(ROOT, 'dist'));
const sets = JSON.parse(fs.readFileSync(path.resolve(HERE, process.env.SETS ?? 'sets.json'), 'utf8'));
const FORMAT = process.env.SHOT_FORMAT ?? 'png';
let server, browser, send;
export async function start() {
  if (!fs.existsSync(path.join(DIST, 'index.html'))) throw new Error('Build the frontend first: npm run build');
  server = await serve(DIST);
  try { browser = await startBrowser(); send = browser.send; }
  catch (error) { await server.close(); throw error; }
}
export function mockScript(label, extra = {}) {
  const settings = { osuExePath: null, songsDir: null, noVideo: true, mirrorOrder: ['catboy', 'osuDirect'], hotkey: 'Ctrl+Shift+D', hideInstalled: false, linkHandler: false, previousBrowser: null, ...extra.settings };
  const data = {
    sets,
    installed: extra.installed ?? [sets[2].id, sets[5].id],
    settings,
    label,
    link: extra.link ?? { enabled: false, registered: false, isDefault: false, previousBrowser: 'chrome.exe' },
    install: extra.install === undefined
      ? { exe: 'C:\\Users\\Player\\AppData\\Local\\osu!\\osu!.exe', dir: 'C:\\Users\\Player\\AppData\\Local\\osu!', songsDir: 'C:\\Users\\Player\\AppData\\Local\\osu!\\Songs', exclusiveFullscreen: false, source: 'runningProcess' }
      : extra.install,
    hasMore: extra.hasMore ?? false,
    prefs: extra.prefs ?? null,
    toast: extra.toast ?? null,
    autostart: extra.autostart ?? false,
    hotkeyStatus: extra.hotkeyStatus ?? { hotkey: 'Ctrl+Shift+D', registered: true, error: null },
  };
  return `(() => {
    const D = ${JSON.stringify(data)};
    try { localStorage.removeItem('osuflow.browse.v1'); if (D.prefs) localStorage.setItem('osuflow.browse.v1', JSON.stringify(D.prefs)); } catch {}
    const cbs = {}; let next = 1; const ls = {};
    window.__emit = (event, payload) => (ls[event] || []).forEach(([id, cbid]) => cbs[cbid] && cbs[cbid]({ event, id, payload }));
    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
    window.__TAURI_INTERNALS__ = {
      metadata: { currentWindow: { label: D.label }, currentWebview: { windowLabel: D.label, label: D.label } },
      transformCallback(cb) { const id = next++; cbs[id] = cb; return id; },
      unregisterCallback(id) { delete cbs[id]; },
      convertFileSrc: (p) => p,
      async invoke(cmd, args) {
        if (!cmd.startsWith('plugin:')) (window.__calls ??= []).push({ cmd, args });
        switch (cmd) {
          case 'plugin:event|listen': { const id = next++; (ls[args.event] ??= []).push([id, args.handler]); return id; }
          case 'plugin:event|unlisten': return;
          case 'plugin:event|emit': window.__emit(args.event, args.payload); return;
          case 'get_settings': return D.settings;
          case 'set_settings': Object.assign(D.settings, args.settings); return D.settings;
          case 'get_installed': return D.installed;
          case 'search': (window.__searches ??= []).push(args.query);
            if (args.query.offset > 0) { await new Promise((r) => setTimeout(r, 150)); return { results: [], mirror: 'osuDirect', sortApplied: true, hasMore: D.hasMore }; }
            return { results: D.sets, mirror: 'osuDirect', sortApplied: true, hasMore: D.hasMore };
          case 'lookup_beatmapset': { const s = D.sets.find((x) => x.id === args.setId); if (!s) throw 'not found'; return s; }
          case 'resolve_set_id': { const s = D.sets.find((x) => x.beatmaps.some((b) => b.id === args.beatmapId)); if (!s) throw 'not found'; return s.id; }
          case 'detect_osu': return D.install;
          case 'get_hotkey_status': return D.hotkeyStatus;
          case 'get_autostart': return D.autostart;
          case 'get_link_handler_status': return D.link;
          case 'get_current_toast': return D.toast;
          default: return undefined;
        }
      },
    };
    document.addEventListener('DOMContentLoaded', () => { const s = document.createElement('style'); s.textContent = 'html{background:#241f33}'; document.head.appendChild(s); });
  })();`;
}


/** Opens a tab sized w x h with the mock for `label`, runs `steps(page)` and saves a PNG. */
export async function render({ name, label, width, height, steps, extra, verifyImages = false }) {
  const { targetId } = await send('Target.createTarget', { url: 'about:blank' });
  const { sessionId } = await send('Target.attachToTarget', { targetId, flatten: true });
  const call = (m, p) => send(m, p, sessionId);
  const page = {
    eval: async (expression) => {
      const r = await call('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
      if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception?.description ?? 'page error');
      return r.result.value;
    },
    mouse: (type, x, y) => call('Input.dispatchMouseEvent', { type, x, y, button: type === 'mouseMoved' ? 'none' : 'left', buttons: type === 'mousePressed' ? 1 : 0, clickCount: 1 }),
    emit: (event, payload) => page.eval(`window.__emit(${JSON.stringify(event)}, ${JSON.stringify(payload)})`),
    cdp: call,
    sleep,
  };
  await call('Page.enable');
  await call('Runtime.enable');
  await call('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: Number(process.env.SCALE ?? 1), mobile: false });
  await call('Page.addScriptToEvaluateOnNewDocument', { source: mockScript(label, extra) });
  await call('Page.navigate', { url: server.url + '?' + Date.now() });
  await sleep(900);
  if (steps) await steps(page);
  await page.eval(`Promise.race([Promise.all([...document.images].filter((image) => image.getBoundingClientRect().width > 0).map((image) => { image.loading = 'eager'; return image.decode().catch(() => {}); })), new Promise((resolve) => setTimeout(resolve, 10000))])`);
  if (verifyImages) {
    const broken = await page.eval(`[...document.images].filter((image) => image.getBoundingClientRect().width > 0 && (!image.complete || !image.naturalWidth)).map((image) => image.src)`);
    if (broken.length) throw new Error(`Screenshot has broken images: ${broken.join(', ')}`);
    if (await page.eval(`(window.__calls ?? []).some((call) => call.cmd === 'download_beatmapset')`)) throw new Error('Screenshot unexpectedly invoked a mock download');
  }
  const shot = await call('Page.captureScreenshot', FORMAT === 'png' ? { format: 'png' } : { format: FORMAT, quality: 92 });
  const file = output(name + '.' + (FORMAT === 'jpeg' ? 'jpg' : FORMAT));
  fs.writeFileSync(file, Buffer.from(shot.data, 'base64'));
  await send('Target.closeTarget', { targetId });
  return file;
}


export async function stop() {
  try { await browser?.close(); } finally { await server?.close(); }
}
