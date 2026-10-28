// Recover the original icon generator; outputs only to OUT_DIR/.artifacts.
import fs from 'node:fs';
import { output, sleep, startBrowser } from './browser.mjs';
export const SVG = `<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024">
    <rect width="1024" height="1024" rx="228" fill="#1d1d21"/>
  <circle cx="512" cy="512" r="274" fill="none" stroke="#e8689b" stroke-width="100"/>
  <circle cx="512" cy="512" r="108" fill="#e8689b"/>
</svg>`;


const browser = await startBrowser();
const { send } = browser;
try {
async function shot(html, width, height, file, transparent) {
  const { targetId } = await send('Target.createTarget', { url: 'about:blank' });
  const { sessionId } = await send('Target.attachToTarget', { targetId, flatten: true });
  const call = (m, p) => send(m, p, sessionId);
  await call('Page.enable');
  await call('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: 1, mobile: false });
  if (transparent) await call('Emulation.setDefaultBackgroundColorOverride', { color: { r: 0, g: 0, b: 0, a: 0 } });
  await call('Page.navigate', { url: 'data:text/html;charset=utf-8,' + encodeURIComponent(html) });
  await sleep(400);
  const r = await call('Page.captureScreenshot', { format: 'png', fromSurface: true });
  fs.writeFileSync(output(file), Buffer.from(r.data, 'base64'));
  await send('Target.closeTarget', { targetId });
}

const page = (inner, bg = 'transparent') => `<!doctype html><meta charset="utf-8"><body style="margin:0;background:${bg}">${inner}</body>`;
await shot(page(SVG), 1024, 1024, 'icon-1024.png', true);

// Previews: the real sizes it will be shown at, on dark and light.
const sizes = [16, 24, 32, 48, 64, 128, 256];
const cell = (size, bg) => `<div style="display:inline-flex;flex-direction:column;align-items:center;gap:6px;padding:14px;background:${bg};color:${bg === '#ffffff' ? '#333' : '#ddd'};font:11px sans-serif"><img src="data:image/svg+xml;utf8,${encodeURIComponent(SVG)}" width="${size}" height="${size}">${size}</div>`;
const row = (bg) => `<div style="display:flex;align-items:flex-end;background:${bg}">${sizes.map((s) => cell(s, bg)).join('')}</div>`;
await shot(page(row('#202124') + row('#ffffff')), 880, 700, 'icon-preview.png', false);


} finally { await browser.close(); }
