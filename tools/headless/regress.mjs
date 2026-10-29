// Regression suite for the redesigned overlay, run against the built dist/ in headless Edge with the mocked
// Tauri bridge (nothing real is downloaded, hidden or sent). node regress.mjs [group...]   (TAG=name for PNGs)
import { start, render, stop, sleep } from './harness.mjs';

const groups = process.argv.slice(2);
const validGroups = ['topbar', 'panel', 'hover', 'small', 'features'];
if (groups.some((group) => !validGroups.includes(group))) throw new Error(`Unknown group. Choose: ${validGroups.join(', ')}`);
const want = (name) => !groups.length || groups.includes(name);
const tag = process.env.TAG ?? 'reg';
let pass = 0;
let fail = 0;
const check = (name, ok, detail = '') => {
  ok ? pass++ : fail++;
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${detail ? '   -- ' + detail : ''}`);
};

const size = { width: 1100, height: 780 };
const open = async (p) => {
  await p.emit('overlay-shown', { osuRunning: true, exclusiveFullscreen: false });
  for (let i = 0; i < 60 && (await p.eval("document.querySelectorAll('.card:not(.ghost)').length")) < 6; i++) await sleep(100);
  await sleep(700);
};
const ev = (p, expression) => p.eval(expression);
const json = async (p, expression) => JSON.parse(await p.eval(`JSON.stringify(${expression})`));
const rect = (p, selector, index = 0) =>
  json(p, `(() => { const e = document.querySelectorAll(${JSON.stringify(selector)})[${index}]; if (!e) return null; const r = e.getBoundingClientRect(); return { x: r.x, y: r.y, w: r.width, h: r.height }; })()`);
const count = (p, selector) => ev(p, `document.querySelectorAll(${JSON.stringify(selector)}).length`);
const text = (p, selector, index = 0) => ev(p, `document.querySelectorAll(${JSON.stringify(selector)})[${index}]?.textContent?.trim() ?? null`);
const cards = (p) => count(p, '.card:not(.ghost)');
const searches = (p) => ev(p, 'window.__searches.length');
const lastQuery = (p) => json(p, 'window.__searches.at(-1)');
const calls = (p, name) => json(p, `(window.__calls ?? []).filter((c) => c.cmd === ${JSON.stringify(name)})`);
const clickEl = (p, selector, index = 0) => ev(p, `document.querySelectorAll(${JSON.stringify(selector)})[${index}].click()`);
const clickByText = (p, selector, label) =>
  ev(p, `(() => { const e = [...document.querySelectorAll(${JSON.stringify(selector)})].find((x) => x.textContent.trim().startsWith(${JSON.stringify(label)})); if (!e) throw new Error('no ' + ${JSON.stringify(label)}); e.click(); })()`);
// Inspect a real CSS transition at a fixed progress, independent of a busy CI runner's clock.
const transitionSample = (p) => ev(p, `(async () => {
  const wrap = document.querySelector('.fwrap');
  wrap.style.transitionDuration = '10s'; void wrap.offsetHeight;
  document.querySelector('.filters-btn').click();
  await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
  const animation = wrap.getAnimations().find((animation) => animation.transitionProperty === 'grid-template-rows');
  if (!animation) throw new Error('Filter transition is missing');
  animation.pause(); animation.currentTime = 4000;
  const height = document.querySelector('.fclip').getBoundingClientRect().height;
  animation.finish(); wrap.style.transitionDuration = '';
  return height;
})()`);
const move = (p, x, y) => p.cdp('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y, button: 'none', buttons: 0 });
const mouse = (p, type, x, y) => p.cdp('Input.dispatchMouseEvent', { type, x, y, button: 'left', buttons: type === 'mouseReleased' ? 0 : 1, clickCount: 1 });
const KEYS = { Enter: [13, '\r'], ArrowDown: [40], ArrowUp: [38], ArrowLeft: [37], ArrowRight: [39], Escape: [27], ' ': [32, ' '], Home: [36], End: [35] };
const key = async (p, name) => {
  const [vk, chars] = KEYS[name] ?? [name.toUpperCase().charCodeAt(0), name];
  const code = name.length === 1 && name !== ' ' ? `Key${name.toUpperCase()}` : name === ' ' ? 'Space' : name;
  await p.cdp('Input.dispatchKeyEvent', { type: 'keyDown', key: name, code, windowsVirtualKeyCode: vk, text: chars });
  await p.cdp('Input.dispatchKeyEvent', { type: 'keyUp', key: name, code, windowsVirtualKeyCode: vk });
  await sleep(120);
};
const typeText = (p, value) =>
  ev(p, `(() => { const i = document.querySelector('.search'); Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(i, ${JSON.stringify(value)}); i.dispatchEvent(new Event('input', { bubbles: true })); })()`);
const center = (r) => ({ x: r.x + r.w / 2, y: r.y + r.h / 2 });
const near = (a, b, tolerance = 1.5) => Math.abs(a - b) <= tolerance;
const sliderIndex = { stars: 0, ar: 1, cs: 2, od: 3, hp: 4, bpm: 5, length: 6 };

await start();
try {
  // ------------------------------------------------------------------ the top bar
  if (want('topbar')) {
    console.log(`\n== top bar -> ${await render({
      name: `${tag}-topbar`, label: 'overlay', ...size,
      steps: async (p) => {
        await open(p);
        // The segmented control: the glide-in highlight sits exactly on the chosen button.
        for (const [label, mode] of [['osu!', 0], ['taiko', 1], ['catch', 2], ['mania', 3], ['All', null]]) {
          await clickByText(p, '.seg button', label);
          await sleep(450);
          const pill = await rect(p, '.seg-pill');
          const buttonIndex = ['All', 'osu!', 'taiko', 'catch', 'mania'].indexOf(label);
          const button = await rect(p, '.seg button', buttonIndex);
          check(`mode ${label}: highlight sits on the button`, near(pill.x, button.x) && near(pill.w, button.w), `pill ${pill.x.toFixed(1)}/${pill.w.toFixed(1)} button ${button.x.toFixed(1)}/${button.w.toFixed(1)}`);
          // "All" was searched at the start, so going back to it is answered from the cache with no new search.
          check(`mode ${label}: pressed and searched`, (await text(p, '.seg button.on')) === label && (mode === null || (await lastQuery(p)).mode === mode), JSON.stringify((await lastQuery(p)).mode));
        }

        // The Show menu ticks several categories at once.
        const statusesOf = async () => (await lastQuery(p)).statuses;
        const item = (i) => ev(p, `document.querySelectorAll('.flyout-item')[${i}].click()`);
        await clickEl(p, '.menu-trigger', 0);
        await sleep(250);
        check('Show menu opens with six choices', (await count(p, '.flyout')) === 1 && (await count(p, '.flyout-item')) === 6);
        check('Show menu ticks the current choice', (await text(p, '.flyout-item.ticked')) === 'Ranked' && (await count(p, '.flyout-item .tick')) === 6);
        check('it is a multiple-choice list', (await ev(p, "document.querySelector('.flyout').getAttribute('aria-multiselectable')")) === 'true');
        await item(1);
        await sleep(400);
        check('ticking Loved keeps the menu open and shows both on the button', (await count(p, '.flyout')) === 1 && (await text(p, '.menu-value', 0)) === 'Ranked, Loved', await text(p, '.menu-value', 0));
        check('...and searches ranked and loved together', JSON.stringify(await statusesOf()) === '[1,4]', JSON.stringify(await statusesOf()));
        await item(0);
        await sleep(400);
        check('unticking Ranked leaves Loved alone', (await text(p, '.menu-value', 0)) === 'Loved' && JSON.stringify(await statusesOf()) === '[4]');
        await item(1);
        await sleep(300);
        check('the last ticked category cannot be unticked', (await text(p, '.menu-value', 0)) === 'Loved' && JSON.stringify(await statusesOf()) === '[4]' && (await count(p, '.flyout-item.ticked')) === 1);
        await item(0); await item(2);
        await sleep(450);
        check('a third category: the button gives a count and the search lists all three', (await text(p, '.menu-value', 0)) === '3 selected' && JSON.stringify(await statusesOf()) === '[1,4,3]', JSON.stringify(await statusesOf()));
        check('saved with the other choices', JSON.stringify(JSON.parse(await ev(p, "localStorage.getItem('osuflow.browse.v1')")).statuses) === '[1,4,3]' || JSON.stringify(JSON.parse(await ev(p, "localStorage.getItem('osuflow.browse.v1')")).statuses) === '[1,3,4]');
        await move(p, 40, 700); await mouse(p, 'mousePressed', 40, 700); await mouse(p, 'mouseReleased', 40, 700);
        await sleep(250);
        check('clicking away closes it', (await count(p, '.flyout')) === 0);

        // By keyboard: arrows open it and move, Space and Enter tick, and none of it reaches the results behind.
        await ev(p, "document.querySelectorAll('.menu-trigger')[0].focus()");
        await key(p, 'ArrowDown');
        check('ArrowDown on the button opens the menu', (await count(p, '.flyout')) === 1);
        check('the opened menu has the keyboard', await ev(p, "document.activeElement?.classList.contains('flyout')"));
        const selectedBefore = await ev(p, "document.querySelector('.card.selected')?.dataset.index ?? null");
        await key(p, 'ArrowDown'); await key(p, 'ArrowDown');
        check('arrows move the highlight, not the card selection', (await ev(p, "document.querySelector('.card.selected')?.dataset.index ?? null")) === selectedBefore && (await text(p, '.flyout-item.hot')) === 'Qualified', await text(p, '.flyout-item.hot'));
        await key(p, ' ');
        await sleep(300);
        check('Space unticks the highlighted entry and the menu stays open', (await text(p, '.menu-value', 0)) === 'Ranked, Loved' && (await count(p, '.flyout')) === 1);
        await key(p, 'ArrowDown');
        await key(p, 'Enter');
        await sleep(300);
        check('Enter ticks the next one (Pending) and the menu stays open', (await text(p, '.menu-value', 0)) === '3 selected' && (await count(p, '.flyout')) === 1);
        check('...and nothing was downloaded', (await calls(p, 'download_beatmapset')).length === 0);
        await key(p, 'Escape');
        check('Escape closes the menu and focus returns to the button', (await count(p, '.flyout')) === 0 && (await ev(p, "document.activeElement?.classList.contains('menu-trigger')")));

        // Escape closes the menu first, and only the next Escape closes the overlay.
        await key(p, 'ArrowDown');
        await key(p, 'Escape');
        check('Escape closes the menu but not the overlay', (await count(p, '.flyout')) === 0 && (await calls(p, 'hide_overlay')).length === 0);
        await key(p, 'Escape');
        check('a second Escape closes the overlay', (await calls(p, 'hide_overlay')).length === 1);

        // Clicking away; Home/End; type-ahead.
        await clickEl(p, '.menu-trigger', 1);
        await sleep(200);
        check('Sort menu lists the sorts', (await count(p, '.flyout-item')) >= 10, String(await count(p, '.flyout-item')));
        await key(p, 'End');
        check('End goes to the last entry', (await text(p, '.flyout-item.hot')) === (await text(p, '.flyout-item', (await count(p, '.flyout-item')) - 1)));
        await key(p, 'Home');
        check('Home goes to the first entry', (await text(p, '.flyout-item.hot')) === (await text(p, '.flyout-item', 0)));
        await key(p, 't');
        check('typing a letter jumps to a matching entry', (await text(p, '.flyout-item.hot')).toLowerCase().startsWith('t'), await text(p, '.flyout-item.hot'));
        await move(p, 40, 700); await mouse(p, 'mousePressed', 40, 700); await mouse(p, 'mouseReleased', 40, 700);
        await sleep(200);
        check('clicking elsewhere closes the menu', (await count(p, '.flyout')) === 0);

        // Choosing a sort shows the direction button and reversing it searches the other way round.
        await clickEl(p, '.menu-trigger', 1);
        await sleep(200);
        await clickByText(p, '.flyout-item', 'Stars');
        await sleep(300);
        const first = await lastQuery(p);
        check('Stars sort searched', first.sort === 'difficulty', JSON.stringify(first.sort));
        check('direction button appears', (await count(p, '.dir')) === 1, await text(p, '.dir'));
        await clickEl(p, '.dir');
        await sleep(300);
        check('reversing flips the direction', (await lastQuery(p)).direction !== first.direction, `${first.direction} -> ${(await lastQuery(p)).direction}`);
        await clickEl(p, '.menu-trigger', 1);
        await sleep(200);
        await clickByText(p, '.flyout-item', 'Best match');
        await sleep(300);
        check('Best match hides the direction button', (await count(p, '.dir')) === 0);

        // Layout switch.
        await clickEl(p, '.layout button', 1);
        await sleep(300);
        check('card layout', await ev(p, "document.querySelector('.grid:not([aria-hidden])').classList.contains('cards')"));
        await clickEl(p, '.layout button', 0);
        await sleep(300);
        check('list layout', await ev(p, "document.querySelector('.grid:not([aria-hidden])').classList.contains('list')"));
        // Keep the screenshot for looking at: menu open.
        await clickEl(p, '.menu-trigger', 1);
        await sleep(350);
      },
    })}`);
  }

  // ------------------------------------------------------------------ the filter panel and its sliders
  if (want('panel')) {
    console.log(`\n== filter panel -> ${await render({
      name: `${tag}-panel`, label: 'overlay', ...size,
      steps: async (p) => {
        await open(p);
        const total = await cards(p);
        check('panel starts closed and inert', (await ev(p, "document.querySelector('.fwrap').dataset.open")) === 'false' && (await ev(p, "document.querySelector('.fwrap').inert")) === true);
        check('closed panel takes no room', (await rect(p, '.fclip')).h < 1, String((await rect(p, '.fclip')).h));
        const resultsClosed = await rect(p, '.results');
        const midway = await transitionSample(p);
        await sleep(500);
        const opened = (await rect(p, '.fclip')).h;
        check('opening animates (partway, then full height)', midway > 0 && midway < opened, `${midway.toFixed(0)} -> ${opened.toFixed(0)}`);
        check('open panel is no longer inert', (await ev(p, "document.querySelector('.fwrap').inert")) === false);
        check('results area keeps room', (await rect(p, '.results')).h > 200, String((await rect(p, '.results')).h));

        // The sliders are always there, in a grid that never changes: using them moves nothing.
        const snapshot = () => json(p, `(() => { const r = (s) => { const b = document.querySelector(s).getBoundingClientRect(); return [b.x, b.y, b.width, b.height].map((n) => Math.round(n * 10) / 10); }; return { rows: r('.frows'), bottom: r('.fbottom'), grid: r('.sgrid'), foot: r('.ffoot'), results: r('.results'), panel: r('.fpanel'), cells: [...document.querySelectorAll('.slider')].map((e) => { const b = e.getBoundingClientRect(); return [Math.round(b.x), Math.round(b.y)]; }) }; })()`);
        const before = await snapshot();
        check('all seven sliders are always shown', before.cells.length === 7 && (await count(p, '.slider-track')) === 7);
        check('there are no expand buttons any more', (await count(p, '.chip.range')) === 0 && (await count(p, '.chip-value')) === 0 && (await count(p, '[aria-expanded]:not(.filters-btn):not(.menu-trigger)')) === 0);
        check('every slider starts at "Any"', await ev(p, "[...document.querySelectorAll('.slider-value')].every((v) => v.textContent.trim() === 'Any')"));
        const order = await json(p, "[...document.querySelectorAll('.slider-label')].map((e) => e.textContent)");
        check('they are Stars, AR, CS, OD, HP, BPM, Length in that order', JSON.stringify(order) === JSON.stringify(['Stars', 'AR', 'CS', 'OD', 'HP', 'BPM', 'Length']), JSON.stringify(order));
        const rowsBox = await rect(p, '.frows'); const gridBox = await rect(p, '.sgrid');
        check('they sit in the bottom block, under the options', gridBox.y >= rowsBox.y + rowsBox.h - 1, `options end ${(rowsBox.y + rowsBox.h).toFixed(0)}, sliders start ${gridBox.y.toFixed(0)}`);
        check('four across, then three, on two rows', new Set(before.cells.map((c) => c[1])).size === 2 && before.cells.filter((c) => c[1] === before.cells[0][1]).length === 4, JSON.stringify(before.cells));
        check('the "Ranges" label is there, like the other rows', await ev(p, "[...document.querySelectorAll('.flabel')].map((e) => e.textContent).includes('Ranges')"));

        // Setting sliders: real drags.
        const baseSearches = await searches(p);
        const track = (index, value, min, max) => rect(p, '.slider-track', index).then((r) => ({ x: r.x + 8 + ((r.w - 16) * (value - min)) / (max - min), y: r.y + r.h / 2 }));
        const drag = async (index, from, to, min, max) => {
          const a = await track(index, from, min, max); const b = await track(index, to, min, max);
          await move(p, a.x, a.y); await mouse(p, 'mousePressed', a.x, a.y);
          for (let i = 1; i <= 8; i++) { await mouse(p, 'mouseMoved', a.x + ((b.x - a.x) * i) / 8, a.y); await sleep(20); }
          await mouse(p, 'mouseReleased', b.x, b.y); await sleep(200);
        };
        await drag(0, 10, 6, 0, 10);
        await drag(0, 0, 4, 0, 10);
        await drag(1, 0, 9, 0, 10);
        check('Stars shows its setting', (await text(p, '.slider-value', 0)) === '4.0★ – 6.0★', await text(p, '.slider-value', 0));
        check('AR shows its setting', (await text(p, '.slider-value', 1)) === '9.0 and up', await text(p, '.slider-value', 1));
        check('sliders narrow the results with no new search', (await cards(p)) < total && (await searches(p)) === baseSearches, `${total} -> ${await cards(p)}`);
        check('Filters badge counts both', (await text(p, '.filters-btn .count')) === '2');
        check('using sliders moves nothing in the panel', JSON.stringify(await snapshot()) === JSON.stringify(before));
        const labelColours = await json(p, "[...document.querySelectorAll('.slider-label')].map((e) => getComputedStyle(e).color)");
        check('a slider that is narrowing the results is marked in blue, the others are not', labelColours[0] === 'rgb(143, 216, 255)' && labelColours[1] === 'rgb(143, 216, 255)' && labelColours[2] !== 'rgb(143, 216, 255)', JSON.stringify(labelColours.slice(0, 3)));
        const narrowed = await cards(p);
        // Click the groove: the nearer handle jumps there.
        const cs = await track(2, 3, 0, 10);
        await mouse(p, 'mousePressed', cs.x, cs.y); await mouse(p, 'mouseReleased', cs.x, cs.y); await sleep(150);
        check('clicking a slider track moves the nearer handle', (await text(p, '.slider-value', 2)) === '3.0 and up', await text(p, '.slider-value', 2));
        // The value button clears one slider.
        await clickEl(p, '.slider.on button.slider-value', 2);
        await sleep(250);
        check('clearing a slider returns it to Any and leaves the others alone', (await text(p, '.slider-value', 2)) === 'Any' && (await text(p, '.slider-value', 1)) === '9.0 and up' && (await text(p, '.slider-value', 0)) === '4.0★ – 6.0★');
        // Other chips.
        await clickByText(p, '.chip', 'Pop');
        await sleep(250);
        check('genre chip filters', (await cards(p)) <= narrowed);
        check('a chosen option has the pink ring', (await ev(p, "getComputedStyle([...document.querySelectorAll('.chip.on')].find((c) => c.textContent.trim() === 'Pop')).boxShadow")).includes('232, 104, 155'));
        check('Reset filters is available', (await ev(p, "document.querySelector('.ffoot .reset').disabled")) === false);
        await clickEl(p, '.ffoot .reset');
        await sleep(300);
        check('Reset filters clears every filter and slider', (await cards(p)) === total && (await count(p, '.slider.on')) === 0 && (await count(p, '.filters-btn .count')) === 0);
        check('...and the panel looks exactly as it did at the start', JSON.stringify(await snapshot()) === JSON.stringify(before));
        check('Reset button goes quiet when there is nothing to reset', (await ev(p, "document.querySelector('.ffoot .reset').disabled")) === true);
        // Saved choices.
        await drag(0, 10, 7, 0, 10); await sleep(200);
        const saved = JSON.parse(await ev(p, "localStorage.getItem('osuflow.browse.v1')"));
        check('slider choices are saved', saved.filters.ranges.stars.max === 7, JSON.stringify(saved.filters.ranges.stars));
        // Closing the panel animates and removes it from the tab order.
        const closing = await transitionSample(p);
        await sleep(500);
        check('closing animates down to nothing', closing > 0 && (await rect(p, '.fclip')).h < 1, `${closing.toFixed(0)} -> ${(await rect(p, '.fclip')).h}`);
        check('closed panel is inert and its options are not tabbable', (await ev(p, "document.querySelector('.fwrap').inert")) === true);
        check('results area gets its room back', near((await rect(p, '.results')).y, resultsClosed.y, 2), `${resultsClosed.y} vs ${(await rect(p, '.results')).y}`);
        await clickEl(p, '.filters-btn');
        await sleep(600);
        check('the slider setting is still there after reopening', (await text(p, '.slider-value', 0)).includes('7.0★'));
        await drag(2, 0, 4, 0, 10);
        await drag(5, 60, 150, 60, 300);
      },
    })}`);
  }

  // ------------------------------------------------------------------ hover and Enter
  if (want('hover')) {
    console.log(`\n== hover + Enter -> ${await render({
      name: `${tag}-hover`, label: 'overlay', ...size,
      steps: async (p) => {
        await open(p);
        const downloads = () => calls(p, 'download_beatmapset');
        const titleOf = (index) => text(p, `.card[data-index="${index}"] .title`);
        const free = await ev(p, "[...document.querySelectorAll('.card:not(.ghost)')].filter((c) => !c.classList.contains('installed')).map((c) => Number(c.dataset.index)).slice(0, 8).join(',')");
        const ids = free.split(',').map(Number);
        const [a, b, c] = ids;
        const at = async (index) => center(await rect(p, `.card[data-index="${index}"]`));

        // 1. Point at a card and press Enter: it downloads, with nothing clicked and nothing selected.
        const pa = await at(a);
        await move(p, pa.x - 40, pa.y); await move(p, pa.x, pa.y);
        await sleep(200);
        check('the key hint appears when a card is pointed at', await ev(p, "document.querySelector('.keyhint').classList.contains('on')"));
        check('nothing is selected by keyboard', (await count(p, '.card.selected')) === 0);
        await key(p, 'Enter');
        let d = await downloads();
        check('Enter downloads the card under the pointer', d.length === 1 && d[0].args.title === (await titleOf(a)), JSON.stringify(d.map((x) => x.args.title)) + ' vs ' + (await titleOf(a)));

        // 2. Pressing Enter again without moving does nothing more (the pointer has not moved since a key).
        await key(p, 'Enter');
        check('a second Enter without moving does not download again', (await downloads()).length === 1);
        // 3. Moving to another card and pressing Enter downloads that one.
        const pb = await at(b);
        await move(p, pb.x - 30, pb.y); await move(p, pb.x, pb.y);
        await sleep(150);
        await key(p, 'Enter');
        d = await downloads();
        check('moving to another card and pressing Enter downloads that one', d.length === 2 && d[1].args.title === (await titleOf(b)), JSON.stringify(d.map((x) => x.args.title)));
        // 4. Held key: auto-repeat never repeats the download.
        const pc = await at(c);
        await move(p, pc.x - 20, pc.y); await move(p, pc.x, pc.y);
        await sleep(150);
        await p.cdp('Input.dispatchKeyEvent', { type: 'keyDown', key: 'Enter', code: 'Enter', windowsVirtualKeyCode: 13, text: '\r' });
        await p.cdp('Input.dispatchKeyEvent', { type: 'keyDown', key: 'Enter', code: 'Enter', windowsVirtualKeyCode: 13, text: '\r', autoRepeat: true });
        await p.cdp('Input.dispatchKeyEvent', { type: 'keyDown', key: 'Enter', code: 'Enter', windowsVirtualKeyCode: 13, text: '\r', autoRepeat: true });
        await p.cdp('Input.dispatchKeyEvent', { type: 'keyUp', key: 'Enter', code: 'Enter', windowsVirtualKeyCode: 13 });
        await sleep(150);
        check('holding Enter downloads once', (await downloads()).length === 3, String((await downloads()).length));

        // 5. Typing then Enter with the pointer resting on a card must NOT download it.
        const before = (await downloads()).length;
        await ev(p, "document.querySelector('.search').focus()");
        await key(p, 'x');
        await key(p, 'Enter');
        check('typing a search then Enter, pointer resting on a card: no download', (await downloads()).length === before);
        // ...but moving the pointer afterwards and pressing Enter does.
        await typeText(p, '');
        await sleep(500);
        const pd = await at(a === ids[0] ? ids[3] : ids[0]);
        await move(p, pd.x - 25, pd.y); await move(p, pd.x, pd.y);
        await sleep(150);
        await key(p, 'Enter');
        check('after typing, pointing again and pressing Enter downloads', (await downloads()).length === before + 1, String((await downloads()).length));

        // 6. Enter with focus on a button: pointing at a card wins; pointing at nothing leaves the button alone.
        await ev(p, "document.querySelector('.layout button:last-child').focus()");
        const pe = await at(ids[4]);
        await move(p, pe.x - 25, pe.y); await move(p, pe.x, pe.y);
        await sleep(150);
        const layoutBefore = await ev(p, "document.querySelector('.grid:not([aria-hidden])').className");
        const n = (await downloads()).length;
        await key(p, 'Enter');
        check('Enter with a button focused downloads the pointed card', (await downloads()).length === n + 1);
        check('...and does not press the focused button', (await ev(p, "document.querySelector('.grid:not([aria-hidden])').className")) === layoutBefore);

        // 7. The keyboard path is unchanged: arrows select, Enter downloads the selection.
        await move(p, 5, 5);
        await ev(p, "document.querySelector('.search').focus()");
        await key(p, 'ArrowDown');
        check('ArrowDown selects the first card', (await count(p, '.card.selected')) === 1 && (await ev(p, "document.querySelector('.card.selected').dataset.index")) === '0');
        await key(p, 'ArrowRight');
        check('ArrowRight moves the selection', (await ev(p, "document.querySelector('.card.selected')?.dataset.index")) === '1');
        // Walk to a card that has not been downloaded yet in this run (asking again for one in progress is ignored).
        const fresh = ids[7];
        for (let step = 1; step < fresh; step++) await key(p, 'ArrowRight');
        check('the arrows reach a fresh card', (await ev(p, "document.querySelector('.card.selected')?.dataset.index")) === String(fresh), String(fresh));
        const m = (await downloads()).length;
        await key(p, 'Enter');
        check('Enter downloads the selected card', (await downloads()).length === m + 1 && (await downloads()).at(-1).args.title === (await titleOf(fresh)), (await downloads()).at(-1)?.args.title);
        // 8. Pointing overrides the earlier arrow selection once the pointer moves.
        const pf = await at(ids[5]);
        await move(p, pf.x - 25, pf.y); await move(p, pf.x, pf.y);
        await sleep(150);
        const k = (await downloads()).length;
        await key(p, 'Enter');
        const last = (await downloads()).at(-1);
        check('pointing after using the arrows: Enter goes to the pointed card', (await downloads()).length === k + 1 && last.args.title === (await titleOf(ids[5])), last?.args.title);
        // 9. An installed card is not downloaded.
        const installedIndex = await ev(p, "document.querySelector('.card.installed')?.dataset.index ?? null");
        if (installedIndex !== null) {
          const pi = await at(Number(installedIndex));
          await move(p, pi.x - 25, pi.y); await move(p, pi.x, pi.y);
          await sleep(150);
          const q = (await downloads()).length;
          await key(p, 'Enter');
          check('Enter on an installed card downloads nothing', (await downloads()).length === q);
        }
        // 10. Escape still closes the overlay with a card pointed at.
        await key(p, 'Escape');
        check('Escape closes the overlay', (await calls(p, 'hide_overlay')).length === 1);
        await sleep(200);
      },
    })}`);
  }

  // ------------------------------------------------------------------ a small window
  if (want('small')) {
    for (const [width, height] of [[820, 600], [620, 520]]) {
      console.log(`\n== small window ${width}x${height} -> ${await render({
        name: `${tag}-small-${width}`, label: 'overlay', width, height,
        steps: async (p) => {
          await open(p);
          await clickEl(p, '.filters-btn');
          await sleep(500);
          await sleep(300);
          const metrics = await json(p, "(() => { const panel = document.querySelector('.panel'); const r = document.querySelector('.results').getBoundingClientRect(); const f = document.querySelector('.fpanel'); const rows = document.querySelector('.frows'); return { overflowX: panel.scrollWidth > panel.clientWidth, resultsHeight: Math.round(r.height), panelHeight: Math.round(f.getBoundingClientRect().height), rowsScroll: rows.scrollHeight > rows.clientHeight, windowHeight: innerHeight }; })()");
          check(`${width}x${height}: nothing overflows sideways`, !metrics.overflowX);
          check(`${width}x${height}: the results keep room`, metrics.resultsHeight >= 80, JSON.stringify(metrics));
          check(`${width}x${height}: every slider is fully visible`, await ev(p, "(() => { const w = document.querySelector('.panel').getBoundingClientRect(); return [...document.querySelectorAll('.slider')].every((s) => { const a = s.getBoundingClientRect(); return a.bottom <= w.bottom && a.top >= w.top && a.right <= w.right; }); })()"));
          await clickEl(p, '.menu-trigger', 1);
          await sleep(300);
          const menuBox = await json(p, "(() => { const m = document.querySelector('.flyout').getBoundingClientRect(); const t = document.querySelectorAll('.menu-trigger')[1].getBoundingClientRect(); return { top: Math.round(m.top), bottom: Math.round(m.bottom), right: Math.round(m.right), triggerTop: Math.round(t.top), innerHeight, innerWidth }; })()");
          check(`${width}x${height}: a menu stays inside the window`, menuBox.bottom <= menuBox.innerHeight && menuBox.right <= menuBox.innerWidth, JSON.stringify(menuBox));
        },
      })}`);
    }
  }

  // ------------------------------------------------------------------ earlier features
  if (want('features')) {
    console.log(`\n== earlier features -> ${await render({
      name: `${tag}-features`, label: 'overlay', ...size,
      steps: async (p) => {
        await open(p);
        const total = await cards(p);
        const base = await searches(p);
        await typeText(p, 'stars>5 ar>9');
        await sleep(600);
        check('typed conditions still become chips', (await count(p, '.cond')) === 2, JSON.stringify(await ev(p, "[...document.querySelectorAll('.cond')].map((c) => c.textContent)")));
        check('...and narrow the results without a new search', (await cards(p)) < total && (await searches(p)) === base);
        await clickEl(p, '.cond', 0);
        await sleep(300);
        check('clicking a chip removes the condition', (await ev(p, "document.querySelector('.search').value")) === 'ar>9');
        await typeText(p, '');
        await sleep(300);
        const id = await ev(p, "(async () => (await window.__TAURI_INTERNALS__.invoke('search', { query: { offset: 0 } })).results[3].id)()");
        await typeText(p, `https://osu.ppy.sh/beatmapsets/${id}#osu/1`);
        await sleep(1000);
        check('a pasted link finds that map', (await cards(p)) === 1);
        await typeText(p, 'a'); await sleep(700); await typeText(p, 'ab'); await sleep(700);
        const s = await searches(p);
        await typeText(p, 'a'); await sleep(120);
        check('going back to an earlier search is instant (cached)', (await searches(p)) === s && (await cards(p)) > 1);
        await typeText(p, '');
        await sleep(300);
        // Prefs seeded by a previous run come back.
      },
    })}`);
    console.log(`\n== saved choices -> ${await render({
      name: `${tag}-seeded`, label: 'overlay', ...size,
      extra: { prefs: { layout: 'cards', panelOpen: true, sort: 'bpm', dir: 'asc', statuses: [1, 4], mode: 1, filters: { genre: 5, tier: 'hard', ranges: { ar: { min: 9, max: null }, length: { min: null, max: 240 } } } } },
      steps: async (p) => {
        await open(p);
        check('saved game mode is selected', (await text(p, '.seg button.on')) === 'taiko');
        check('saved Show choices are shown', (await text(p, '.menu-value', 0)) === 'Ranked, Loved', await text(p, '.menu-value', 0));
        check('saved sort is shown', (await text(p, '.menu-value', 1)) === 'BPM', await text(p, '.menu-value', 1));
        check('saved layout and open panel', (await ev(p, "document.querySelector('.grid:not([aria-hidden])').classList.contains('cards')")) && (await ev(p, "document.querySelector('.fwrap').dataset.open")) === 'true');
        const value = (i) => text(p, '.slider-value', i);
        check('saved sliders come back set, the others at Any', (await value(1)) === '9.0 and up' && (await value(6)) === 'up to 4:00' && (await value(0)) === 'Any' && (await count(p, '.slider.on')) === 2, JSON.stringify([await value(0), await value(1), await value(6)]));
        const q = await json(p, "window.__searches[0]");
        check('the first search used the saved choices', q.mode === 1 && JSON.stringify(q.statuses) === '[1,4]' && q.sort === 'bpm' && q.direction === 'asc', JSON.stringify(q));
        check('Filters badge counts genre, difficulty and two sliders', (await text(p, '.filters-btn .count')) === '4', await text(p, '.filters-btn .count'));
      },
    })}`);
  }
} finally {
  console.log(`\n${pass} passed, ${fail} failed`);
  await stop();
  if (fail) process.exitCode = 1;
}
