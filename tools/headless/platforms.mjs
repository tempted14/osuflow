// Platform UI checks use the built frontend and mocked native commands only.
import { start, render, stop, sleep } from './harness.mjs';
let passed = 0;
const check = (name, condition) => { if (!condition) throw new Error(name); passed++; console.log(`PASS ${name}`); };
const info = (os, wayland = false) => ({ os, wayland, experimental: os !== 'windows', defaultHotkey: os === 'macos' ? 'CommandOrControl+Shift+D' : 'Ctrl+Shift+D' });
const clickText = (p, selector, text) => p.eval(`(() => { const element = [...document.querySelectorAll(${JSON.stringify(selector)})].find((item) => item.textContent.trim() === ${JSON.stringify(text)}); if (!element) throw new Error('Missing control'); element.click(); })()`);

await start();
try {
  for (const [os, wayland] of [['windows', false], ['linux', false], ['linux', true], ['macos', false]]) {
    const name = os + (wayland ? '-wayland' : '');
    console.log(await render({ name: `platform/${name}-settings`, label: 'main', width: 560, height: os === 'windows' ? 440 : 900,
      extra: { platform: info(os, wayland), install: os === 'windows' ? undefined : null,
        hotkeyStatus: { hotkey: info(os).defaultHotkey, registered: !wayland, error: wayland ? 'Global shortcuts are unavailable on Wayland' : null } },
      steps: async (p) => {
        const text = await p.eval('document.body.textContent');
        check(`${name}: correct login wording`, text.includes(os === 'windows' ? 'Start with Windows' : 'Start at login'));
        check(`${name}: platform shortcut`, (await p.eval("document.querySelector('.hotkey').value")) === (os === 'macos' ? 'Command+Shift+D' : 'Ctrl+Shift+D'));
        if (os === 'macos') check('macOS: default shortcut fits', await p.eval("(() => { const el = document.querySelector('.hotkey'); return el.scrollWidth <= el.clientWidth; })()"));
        const calls = await p.eval('(window.__calls ?? []).map((call) => call.cmd)');
        check(`${name}: Windows link handler gated`, calls.includes('get_link_handler_status') === (os === 'windows'));
        check(`${name}: experimental status is explicit`, text.includes('Experimental support') === (os !== 'windows'));
        if (wayland) {
          check('Wayland: CLI alternative is visible', text.includes('osuflow --toggle'));
          check('Wayland: does not ask for another unsupported shortcut', !text.includes('Pick another one'));
        }
        if (os !== 'windows') {
          await p.eval("document.querySelector('.wine-settings summary').click()"); await sleep(100);
          check(`${name}: all four install controls exist`, (await p.eval("document.querySelectorAll('.wine-settings input').length")) === 4);
          if (os === 'linux' && !wayland) {
            const values = ['/home/player/Games/osu!/osu!.exe', '/mnt/Maps', '/home/player/.wine', 'wine start /unix "{file}"'];
            await p.eval(`(() => { const fields = [...document.querySelectorAll('.wine-settings input')]; const values = ${JSON.stringify(values)}; fields.forEach((field, index) => { Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(field, values[index]); field.dispatchEvent(new Event('input', {bubbles:true})); }); })()`);
            await clickText(p, '.wine-settings button', 'Save install settings'); await sleep(200);
            const saved = await p.eval("window.__calls.filter((call) => call.cmd === 'set_settings').at(-1)?.args.settings");
            check('Linux: host paths and quoted command are saved intact', saved?.osuExePath === values[0] && saved?.songsDir === values[1] && saved?.winePrefix === values[2] && saved?.osuLaunchCommand === values[3]);
          }
          const overflow = await p.eval('document.documentElement.scrollWidth > innerWidth');
          check(`${name}: settings do not overflow sideways`, !overflow);
        }
      },
    }));
  }
  console.log(await render({ name: 'platform/songs-fallback', label: 'overlay', width: 1180, height: 760,
    extra: { platform: info('linux'), importMethod: 'movedToSongs' }, steps: async (p) => {
      await p.emit('overlay-shown', { osuRunning: true, exclusiveFullscreen: false }); await sleep(700);
      // Synthetic keyboard input reaches only this mocked page, never a real Download button/backend.
      await p.cdp('Input.dispatchKeyEvent', { type: 'keyDown', key: 'ArrowDown', code: 'ArrowDown', windowsVirtualKeyCode: 40 });
      await p.cdp('Input.dispatchKeyEvent', { type: 'keyUp', key: 'ArrowDown', code: 'ArrowDown', windowsVirtualKeyCode: 40 });
      await p.cdp('Input.dispatchKeyEvent', { type: 'keyDown', key: 'Enter', code: 'Enter', windowsVirtualKeyCode: 13, text: '\r' });
      await p.cdp('Input.dispatchKeyEvent', { type: 'keyUp', key: 'Enter', code: 'Enter', windowsVirtualKeyCode: 13 });
      await sleep(100);
      const id = await p.eval("window.__calls.find((call) => call.cmd === 'download_beatmapset')?.args.setId");
      check('Songs fallback: download uses the mock', Number.isInteger(id));
      await p.emit('download-state', { setId: id, state: 'done', error: null }); await sleep(100);
      check('Songs fallback: F5 instruction is shown', (await p.eval("document.querySelector('.dl-pill')?.textContent ?? ''")).includes('F5'));
      await sleep(6300);
      check('Songs fallback: instruction stays until dismissed', (await p.eval("document.querySelector('.dl-pill')?.textContent ?? ''")).includes('F5'));
    },
  }));
  console.log(`${passed} platform checks passed`);
} finally { await stop(); }
