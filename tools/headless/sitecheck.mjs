// Website layout, images, links, keyboard demo and fallback checks in isolated headless Edge.
import fs from 'node:fs';
import path from 'node:path';
import { ROOT, OUT, output, serve, sleep, startBrowser } from './browser.mjs';
const SITE = process.env.SITE ? path.resolve(process.env.SITE) : path.resolve(ROOT, '../osuflow.github.io');
let pass = 0, fail = 0;
const check = (name, ok, detail = '') => {
  ok ? pass++ : fail++;
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${detail ? '   -- ' + detail : ''}`);
};
const server = process.env.BASE ? null : await serve(SITE, {}, true);
let browser;
try { browser = await startBrowser(); } catch (error) { await server?.close(); throw error; }
const { send, events } = browser;
async function page({ width, height, mobile = false, reduced = false, noScript = false }, steps) {
  const { targetId } = await send('Target.createTarget', { url: 'about:blank' });
  const { sessionId } = await send('Target.attachToTarget', { targetId, flatten: true });
  const call = (m, p) => send(m, p, sessionId);
  const p = {
    call,
    eval: async (expression) => {
      const r = await call('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
      if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception?.description ?? 'page error');
      return r.result.value;
    },
    json: async (expression) => JSON.parse(await p.eval(`JSON.stringify(${expression})`)),
  };
  await call('Page.enable'); await call('Runtime.enable'); await call('Log.enable'); await call('Network.enable');
  await call('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: 1, mobile });
  if (reduced) await call('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-reduced-motion', value: 'reduce' }] });
  if (noScript) await call('Emulation.setScriptExecutionDisabled', { value: true });
  events.length = 0;
  await call('Page.navigate', { url: process.env.BASE ?? server.url });
  await sleep(process.env.BASE ? 3500 : 1800);
  await steps(p);
  const mine = events.filter((e) => e.sessionId === sessionId);
  await send('Target.closeTarget', { targetId });
  return mine;
}

async function scrollThrough(p) {
  const height = await p.eval('document.documentElement.scrollHeight');
  for (let y = 0; y < height; y += 500) { await p.eval(`window.scrollTo(0, ${y})`); await sleep(90); }
  await p.eval('window.scrollTo(0, 0)');
  await sleep(600);
}

async function fullShot(p, name) {
  const { contentSize } = await p.call('Page.getLayoutMetrics');
  const shot = await p.call('Page.captureScreenshot', { format: 'png', captureBeyondViewport: true, clip: { x: 0, y: 0, width: Math.ceil(contentSize.width), height: Math.min(Math.ceil(contentSize.height), 9000), scale: 1 } });
  fs.mkdirSync(path.join(OUT, 'site-shots'), { recursive: true });
  fs.writeFileSync(path.join(OUT, 'site-shots', name + '.png'), Buffer.from(shot.data, 'base64'));
}

try {
  for (const [name, width, height, mobile] of [['desktop', 1440, 900, false], ['tablet', 820, 1100, false], ['phone', 390, 844, true]]) {
    console.log(`\n== ${name} ${width}px`);
    const log = await page({ width, height, mobile }, async (p) => {
      await scrollThrough(p);
      const m = await p.json("(() => ({ scrollW: document.documentElement.scrollWidth, innerW: innerWidth, title: document.title, h1: document.querySelectorAll('h1').length, revealed: document.querySelectorAll('.reveal.in').length, reveals: document.querySelectorAll('.reveal').length, jsClass: document.documentElement.classList.contains('js') }))()");
      check(`${name}: the page is as wide as the screen and no wider`, m.scrollW <= width && m.innerW <= width, `content ${m.scrollW}, layout ${m.innerW}, screen ${width}`);
      check(`${name}: one h1 and a title`, m.h1 === 1 && m.title.includes('osu!flow'));
      const images = await p.json("[...document.images].map((i) => ({ src: i.getAttribute('src'), ok: i.complete && i.naturalWidth > 0, alt: i.hasAttribute('alt') }))");
      check(`${name}: every image loaded`, images.every((i) => i.ok), JSON.stringify(images.filter((i) => !i.ok)));
      check(`${name}: every image has an alt attribute`, images.every((i) => i.alt));
      const tooWide = await p.json(`[...document.querySelectorAll('body *')].filter((e) => { const r = e.getBoundingClientRect(); return r.width > 0 && r.right > ${width} + 1 && getComputedStyle(e).position !== 'fixed' && !e.closest('.glow'); }).slice(0, 6).map((e) => e.tagName + '.' + e.className + ' right=' + Math.round(e.getBoundingClientRect().right))`);
      check(`${name}: nothing pokes out of the window`, tooWide.length === 0, JSON.stringify(tooWide));
      const hidden = await p.json("(() => { const nav = getComputedStyle(document.querySelector('.nav')).display; const btn = document.querySelector('.top .btn').getBoundingClientRect(); return { nav, btnVisible: btn.width > 0 && btn.right <= innerWidth }; })()");
      check(`${name}: the Download button is always reachable`, hidden.btnVisible, JSON.stringify(hidden));
      await fullShot(p, name);
      if (process.env.SECTIONS && name !== 'tablet') {
        for (const id of ['top', 'how', 'features', 'try', 'look', 'syntax', 'faq', 'download']) {
          await p.eval(`document.getElementById(${JSON.stringify(id)}).scrollIntoView({ behavior: 'instant' })`);
          await sleep(500);
          const s = await p.call('Page.captureScreenshot', { format: 'png' });
          fs.writeFileSync(path.join(OUT, 'site-shots', `${name}-${id}.png`), Buffer.from(s.data, 'base64'));
        }
      }
    });
    const errors = log.filter((e) => (e.method === 'Runtime.exceptionThrown') || (e.method === 'Log.entryAdded' && e.params.entry.level === 'error' && !/api\.github\.com/.test(e.params.entry.url ?? '')));
    check(`${name}: no console errors`, errors.length === 0, JSON.stringify(errors.map((e) => e.params.exceptionDetails?.text ?? e.params.entry?.text)));
    const failed = log.filter((e) => e.method === 'Network.loadingFailed' && !(e.params.errorText ?? '').includes('ABORTED'));
    check(`${name}: no failed requests`, failed.length === 0, JSON.stringify(failed.map((e) => e.params.errorText)));
  }

  console.log('\n== links, headings and the head');
  await page({ width: 1280, height: 900 }, async (p) => {
    check('screenshots describe real ranked maps', await p.eval(`/real.*ranked/i.test(document.querySelector('#look .note').textContent)`));
    check('screenshots credit the artwork owners and no old invented-map claims remain', await p.eval(`document.querySelector('.legal').textContent.includes('Cover art, titles and artists in screenshots belong to their respective owners (ppy, mappers, artists); osu!flow includes none of them.') && !/invented maps|no one.s cover art is shown/i.test(document.body.textContent)`));
    const anchors = await p.json("[...document.querySelectorAll('a[href^=\"#\"]')].map((a) => ({ href: a.getAttribute('href'), ok: a.getAttribute('href') === '#' || !!document.querySelector(a.getAttribute('href')) }))");
    check('every in-page link has a target', anchors.every((a) => a.ok), JSON.stringify(anchors.filter((a) => !a.ok)));
    const external = await p.json("[...document.querySelectorAll('a[href^=\"http\"]')].map((a) => a.href)");
    check('external links are github.com only', external.every((h) => new URL(h).hostname === 'github.com'), JSON.stringify([...new Set(external)].filter((h) => new URL(h).hostname !== 'github.com')));
    const ids = await p.json("[...document.querySelectorAll('[id]')].map((e) => e.id)");
    check('ids are unique', new Set(ids).size === ids.length);
    const head = await p.json("({ desc: document.querySelector('meta[name=description]')?.content.length, og: document.querySelector('meta[property=\"og:image\"]')?.content, canon: document.querySelector('link[rel=canonical]')?.href, lang: document.documentElement.lang, ld: (() => { try { return JSON.parse(document.querySelector('script[type=\"application/ld+json\"]').textContent)['@type']; } catch { return null; } })() })");
    check('description, canonical, language and structured data are present', head.desc > 80 && head.canon === 'https://osuflow.github.io/' && head.lang === 'en' && head.ld === 'SoftwareApplication', JSON.stringify(head));
    const noTrackers = await p.json("[...document.scripts].map((s) => s.src).filter(Boolean)");
    check("the only script is the site's own", noTrackers.length === 1 && noTrackers[0].endsWith('/assets/app.js'), JSON.stringify(noTrackers));
    const latest = await p.json("({ version: document.querySelector('.js-version').textContent, href: document.querySelector('a.js-download').href, size: document.querySelector('.js-size').textContent })");
    console.log('   latest-release lookup gave', JSON.stringify(latest));
    check('the download button points at GitHub', new URL(latest.href).hostname === 'github.com');
  });

  console.log('\n== the demo');
  await page({ width: 1280, height: 900 }, async (p) => {
    await p.eval("document.getElementById('try').scrollIntoView()");
    await sleep(900);
    const rowRect = (i) => p.json(`(() => { const r = document.querySelectorAll('.demo-row')[${i}].getBoundingClientRect(); return { x: r.x + r.width * 0.4, y: r.y + r.height / 2 }; })()`);
    const states = () => p.json("[...document.querySelectorAll('.demo-row')].map((r) => r.dataset.state)");
    const key = async (name) => { for (const type of ['keyDown', 'keyUp']) await p.call('Input.dispatchKeyEvent', { type, key: name, code: name, windowsVirtualKeyCode: name === 'Enter' ? 13 : 0, text: name === 'Enter' ? '\r' : undefined }); await sleep(100); };
    const move = (x, y) => p.call('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y, button: 'none', buttons: 0 });
    check('all three maps start idle', JSON.stringify(await states()) === '["idle","idle","idle"]');
    check('"Start over" is really hidden until something is downloaded', (await p.eval("getComputedStyle(document.querySelector('.demo-reset')).display")) === 'none');
    await key('Enter');
    check('Enter with the pointer on nothing does nothing', JSON.stringify(await states()) === '["idle","idle","idle"]');
    const a = await rowRect(1);
    await move(a.x - 30, a.y); await move(a.x, a.y);
    await sleep(300);
    check('pointing at a map shows the hint', (await p.eval("document.querySelector('.demo-hint-text').textContent")).includes('Enter'));
    await key('Enter');
    check('Enter downloads the map under the pointer, and only that one', JSON.stringify(await states()) === '["idle","busy","idle"]', JSON.stringify(await states()));
    await sleep(1500);
    check('it finishes as "in song select"', JSON.stringify(await states()) === '["idle","done","idle"]' && (await p.eval("document.querySelector('.demo-hint-text').textContent")).includes('song select'));
    check('a "Start over" button appears', (await p.eval("document.querySelector('.demo-reset').hidden")) === false);
    await key('Enter');
    check('Enter again on a finished map does nothing', JSON.stringify(await states()) === '["idle","done","idle"]');
    await p.eval("document.querySelectorAll('.demo-get')[2].click()");
    await sleep(1500);
    check('the button works too (touch and mouse)', JSON.stringify(await states()) === '["idle","done","done"]');
    await p.eval("document.querySelector('.demo-reset').click()");
    check('Start over resets the demo', JSON.stringify(await states()) === '["idle","idle","idle"]' && (await p.eval("document.querySelector('.demo-reset').hidden")) === true);
    await p.eval("document.querySelectorAll('.demo-row')[0].focus()");
    await key('Enter');
    check('a map focused with the keyboard downloads on Enter', (await states())[0] === 'busy');
    await sleep(1400);
    await p.call('Page.captureScreenshot', { format: 'png' }).then((s) => { fs.mkdirSync(path.join(OUT, 'site-shots'), { recursive: true }); fs.writeFileSync(path.join(OUT, 'site-shots', 'demo.png'), Buffer.from(s.data, 'base64')); });
  });

  console.log('\n== fallbacks');
  await page({ width: 1280, height: 900, noScript: true }, async (p) => {
    const opacity = await p.json("[...document.querySelectorAll('.reveal')].map((e) => getComputedStyle(e).opacity)");
    check('without JavaScript every section is visible', opacity.every((o) => o === '1'), JSON.stringify([...new Set(opacity)]));
    check('...and the download button still points somewhere real', (await p.eval("document.querySelector('a.js-download').href")).startsWith('https://github.com/tempted14/osuflow/releases'));
  });
  await page({ width: 1280, height: 900, reduced: true }, async (p) => {
    const opacity = await p.json("[...document.querySelectorAll('.reveal')].map((e) => getComputedStyle(e).opacity)");
    check('with reduced motion every section is visible at once', opacity.every((o) => o === '1'), JSON.stringify([...new Set(opacity)]));
  });
  console.log('\n== 404 page');
  await page({ width: 800, height: 600 }, async (p) => {
    await p.call('Page.navigate', { url: `${process.env.BASE ?? server.url}nope/missing` });
    await sleep(900);
    check('a missing page shows the 404 page with a way home', (await p.eval("document.title")).toLowerCase().includes('not found') && (await p.eval("!!document.querySelector('a[href]')")));
  });

} finally {
  console.log(`\n${pass} passed, ${fail} failed`);
  try { await browser.close(); } finally { await server?.close(); }
  if (fail) process.exitCode = 1;
}
