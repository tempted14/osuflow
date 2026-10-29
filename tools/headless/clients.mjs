// Both clients share the real built UI; all native calls, file pickers and downloads are mocked.
import { start, render, stop, sleep } from './harness.mjs';
let passed = 0;
const check = (name, ok) => { if (!ok) throw new Error(name); console.log(`PASS ${name}`); passed++; };
const platform = (os) => ({ os, wayland: false, experimental: os !== 'windows', defaultHotkey: os === 'macos' ? 'CommandOrControl+Shift+D' : 'Ctrl+Shift+D' });
const selectClient = async (p, value) => {
  await p.eval(`(() => { const field = document.querySelector('[aria-label="Game client"]'); Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value').set.call(field, ${JSON.stringify(value)}); field.dispatchEvent(new Event('change', {bubbles:true})); })()`); await sleep(200);
};
await start();
try {
  for (const os of ['windows', 'linux', 'macos']) {
    console.log(await render({ name: `clients/${os}-lazer`, label: 'main', width: 560, height: 900,
      extra: { platform: platform(os), dialogPaths: [os === 'windows' ? 'C:\\Apps\\lazer\\osu!.exe' : os === 'macos' ? '/Applications/osu!.app' : '/home/player/Applications/osu.AppImage', os === 'windows' ? 'C:\\Maps\\lazer' : '/home/player/lazer-data'] },
      steps: async (p) => {
        check(`${os}: stable remains the default`, (await p.eval("document.querySelector('[aria-label=\"Game client\"]').value")) === 'stable');
        await selectClient(p, 'lazer');
        check(`${os}: lazer choice is saved`, (await p.eval("window.__calls.filter((call) => call.cmd === 'set_settings').at(-1)?.args.settings.gameClient")) === 'lazer');
        check(`${os}: native setup replaces Wine controls`, await p.eval("!!document.querySelector('.lazer-settings') && !document.querySelector('.wine-settings:not(.lazer-settings)')"));
        await p.eval("document.querySelector('.lazer-settings summary').click()");
        for (const label of ['Lazer application', 'Lazer data folder']) {
          await p.eval(`[...document.querySelectorAll('button')].find((button) => button.getAttribute('aria-label') === ${JSON.stringify('Browse for ' + label)}).click()`); await sleep(100);
        }
        const picked = await p.eval("[...document.querySelectorAll('.lazer-settings .path-picker input')].map((input) => input.value)");
        check(`${os}: Browse fills application and data paths`, picked.length === 2 && picked.every((value) => value.length > 0));
        await p.eval("[...document.querySelectorAll('.lazer-settings button')].find((button) => button.textContent === 'Save lazer settings').click()"); await sleep(200);
        const saved = await p.eval("window.__calls.filter((call) => call.cmd === 'set_settings').at(-1)?.args.settings");
        check(`${os}: independent lazer settings preserve stable settings`, saved?.gameClient === 'lazer' && saved?.lazerExePath === picked[0] && saved?.lazerDataDir === picked[1] && saved?.osuExePath === null && saved?.winePrefix === null);
        check(`${os}: setup fits the window width`, await p.eval("document.querySelector('#root').scrollWidth <= innerWidth"));
        check(`${os}: library limitation is visible`, (await p.eval('document.body.textContent')).includes('recycle bin'));
        // A second selection proves the stable settings were retained instead of migrated away.
        await selectClient(p, 'stable'); check(`${os}: switching back restores stable mode`, !await p.eval("!!document.querySelector('.lazer-settings')"));
        await selectClient(p, 'lazer');
      },
    }));
  }
  for (const method of ['openedInLazer', 'savedForLazer']) {
    console.log(await render({ name: `clients/${method}`, label: 'overlay', width: 1180, height: 760,
      extra: { platform: platform('linux'), settings: { gameClient: 'lazer' }, importMethod: method },
      steps: async (p) => {
        await p.emit('overlay-shown', { osuRunning: true, exclusiveFullscreen: false }); await sleep(700);
        for (const [key, vk] of [['ArrowDown', 40], ['Enter', 13]]) {
          await p.cdp('Input.dispatchKeyEvent', { type: 'keyDown', key, code: key, windowsVirtualKeyCode: vk, ...(key === 'Enter' ? { text: '\r' } : {}) });
          await p.cdp('Input.dispatchKeyEvent', { type: 'keyUp', key, code: key, windowsVirtualKeyCode: vk });
        }
        await sleep(150); const id = await p.eval("window.__calls.find((call) => call.cmd === 'download_beatmapset')?.args.setId");
        check(`${method}: download is mocked`, Number.isInteger(id));
        await p.emit('download-state', { setId: id, state: 'done', error: null }); await sleep(150);
        const text = await p.eval("document.querySelector('.dl-pill')?.textContent ?? ''");
        check(`${method}: uses lazer feedback without an F5 instruction`, text.includes('osu!lazer') && !text.includes('F5'));
        if (method === 'savedForLazer') {
          await sleep(6300); check('lazer fallback: recovery stays visible', await p.eval("!!document.querySelector('.dl-pill')"));
          await p.eval("document.querySelector('.dl-pill').click()"); await sleep(100);
          check('lazer fallback: offers the downloads folder', (await p.eval("document.querySelector('.dl-panel').textContent")).includes('Show downloads'));
          await p.eval("[...document.querySelectorAll('.dl-actions button')].find((button) => button.textContent === 'Show downloads').click()");
          check('lazer fallback: folder action stays mocked', await p.eval("window.__calls.some((call) => call.cmd === 'open_downloads_folder')"));
        }
      },
    }));
  }
  console.log(`${passed} client checks passed`);
} finally { await stop(); }
