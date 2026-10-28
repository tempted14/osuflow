// Render a website (1200x630) or GitHub (1280x640) social card using the app screenshot.
import fs from 'node:fs';
import path from 'node:path';
import { ROOT, output, serve, sleep, startBrowser } from './browser.mjs';
const SITE = path.resolve(process.env.SITE ?? path.resolve(ROOT, '../osuflow.github.io'));
const width = Number(process.env.OG_WIDTH ?? 1200), height = Number(process.env.OG_HEIGHT ?? 630);
const destination = process.env.OG_OUTPUT ? path.resolve(process.env.OG_OUTPUT) : output('og.png');
const PAGE = `<!doctype html><meta charset="utf-8"><title>og</title>
<style>
  * { box-sizing: border-box; }
  html, body { margin: 0; width: ${width}px; height: ${height}px; overflow: hidden; }
  body { position: relative; background: #17171a; color: #f5f4fb; font: 400 28px/1.4 "Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif; }
  .copy { position: absolute; left: 72px; top: 64px; width: 560px; }
  .brand { display: flex; align-items: center; gap: 16px; font: 700 34px "Segoe UI Variable Display", "Segoe UI", sans-serif; letter-spacing: -.01em; }
  .brand img { width: 64px; height: 64px; border-radius: 16px; }
  h1 { margin: 46px 0 0; font: 700 58px/1.1 "Segoe UI Variable Display", "Segoe UI", sans-serif; letter-spacing: -.035em; }
  h1 span { color: #e8689b; }
  p { margin: 26px 0 0; color: rgba(245, 244, 251, .76); font-size: 27px; }
  .shot { position: absolute; left: 640px; top: 96px; width: 780px; border-radius: 18px; border: 1px solid rgba(255,255,255,.2); overflow: hidden;
            }
  .shot img { display: block; width: 100%; }
</style>
<div class="copy">
  <div class="brand"><img src="/assets/icon.png" alt=""> osu!flow</div>
  <h1>Search and download<br>osu! beatmaps<br><span>without leaving the game.</span></h1>
  <p>A free Windows app for osu! (stable).</p>
</div>
<div class="shot"><img src="/assets/img/app-overlay.webp" alt=""></div>`;


const server = await serve(SITE, { '/__og': PAGE });
let browser;
try { browser = await startBrowser(); } catch (error) { await server.close(); throw error; }
const { send } = browser;
try {
  const { targetId } = await send('Target.createTarget', { url: 'about:blank' });
  const { sessionId } = await send('Target.attachToTarget', { targetId, flatten: true });
  const call = (m, p) => send(m, p, sessionId);
  await call('Page.enable');
  await call('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: 1, mobile: false });
  await call('Page.navigate', { url: server.url + '__og' });
  await sleep(1500);
  const ready = await call('Runtime.evaluate', { expression: `Promise.all([...document.images].map((image) => image.decode())).then(() => true)`, awaitPromise: true, returnByValue: true });
  if (ready.exceptionDetails || ready.result.value !== true) throw new Error('Social card images did not load');
  const shot = await call('Page.captureScreenshot', { format: 'png' });
  fs.writeFileSync(destination, Buffer.from(shot.data, 'base64'));
  console.log('wrote', destination, Math.round(fs.statSync(destination).size / 1024), 'KB');

} finally { try { await browser.close(); } finally { await server.close(); } }
