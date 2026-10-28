// Screenshots of the real built UI with fixture data and a mocked backend.
// SETS=real-sets.json SHOT_FORMAT=webp SCALE=2 node tools/headless/shots.mjs
import { start, render, stop, sleep } from './harness.mjs';

const ev = (p, expression) => p.eval(expression);
const json = async (p, expression) => JSON.parse(await p.eval(`JSON.stringify(${expression})`));
const rect = (p, selector, index = 0) =>
  json(p, `(() => { const e = document.querySelectorAll(${JSON.stringify(selector)})[${index}]; if (!e) return null; const r = e.getBoundingClientRect(); return { x: r.x, y: r.y, w: r.width, h: r.height }; })()`);
const move = (p, x, y) => p.cdp('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y, button: 'none', buttons: 0 });
const mouse = (p, type, x, y) => p.cdp('Input.dispatchMouseEvent', { type, x, y, button: 'left', buttons: type === 'mouseReleased' ? 0 : 1, clickCount: 1 });
const open = async (p) => {
  await p.emit('overlay-shown', { osuRunning: true, exclusiveFullscreen: false });
  for (let i = 0; i < 60 && (await ev(p, "document.querySelectorAll('.card:not(.ghost)').length")) < 6; i++) await sleep(100);
  await sleep(900);
};

await start();
try {
  const size = { width: 1180, height: 760, verifyImages: true, extra: { prefs: { mode: 0 } } };

  console.log(await render({
    name: 'shots/app-overlay', label: 'overlay', ...size,
    steps: async (p) => {
      await open(p);
      // Rest the pointer on a map: it lifts, and the Enter and Space hint appears.
      const card = await rect(p, '.card', 3);
      await move(p, card.x + card.w * 0.4, card.y + card.h * 0.55 - 30);
      await move(p, card.x + card.w * 0.4, card.y + card.h * 0.55);
      await sleep(700);
    },
  }));

  console.log(await render({
    name: 'shots/app-filters', label: 'overlay', ...size,
    steps: async (p) => {
      await open(p);
      await ev(p, "document.querySelector('.filters-btn').click()");
      await sleep(700);
      const track = async (index, value, min, max) => {
        const r = await rect(p, '.slider-track', index);
        return { x: r.x + 8 + ((r.w - 16) * (value - min)) / (max - min), y: r.y + r.h / 2 };
      };
      const drag = async (index, from, to, min, max) => {
        const a = await track(index, from, min, max);
        const b = await track(index, to, min, max);
        await move(p, a.x, a.y);
        await mouse(p, 'mousePressed', a.x, a.y);
        for (let i = 1; i <= 8; i++) { await mouse(p, 'mouseMoved', a.x + ((b.x - a.x) * i) / 8, a.y); await sleep(20); }
        await mouse(p, 'mouseReleased', b.x, b.y);
        await sleep(150);
      };
      await drag(0, 0, 3.5, 0, 10);
      await drag(0, 10, 6.5, 0, 10);
      await drag(1, 0, 9, 0, 10);
      await drag(5, 60, 150, 60, 300);
      await move(p, 5, 5);
      await sleep(600);
    },
  }));

  console.log(await render({
    name: 'shots/app-show', label: 'overlay', ...size, height: 560,
    steps: async (p) => {
      await open(p);
      await ev(p, "document.querySelectorAll('.menu-trigger')[0].click()");
      await sleep(250);
      await ev(p, "document.querySelectorAll('.flyout-item')[1].click()");
      await sleep(150);
      await ev(p, "document.querySelectorAll('.flyout-item')[2].click()");
      await sleep(250);
      await ev(p, "document.querySelectorAll('.flyout-item')[3].dispatchEvent(new PointerEvent('pointermove', { bubbles: true }))");
      await sleep(400);
    },
  }));

  console.log(await render({
    name: 'shots/app-settings', label: 'main', width: 560, height: 420, verifyImages: true,
    steps: async () => { await sleep(1200); },
  }));
} finally {
  await stop();

}
