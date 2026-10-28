// Shared, isolated headless Chromium session. Never attaches to a user's browser.
import fs from 'node:fs';
import http from 'node:http';
import os from 'node:os';
import path from 'node:path';
import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { once } from 'node:events';

export const HERE = path.dirname(fileURLToPath(import.meta.url));
export const ROOT = path.resolve(HERE, '../..');
export const OUT = path.resolve(process.env.OUT_DIR ?? path.join(HERE, '.artifacts'));
export const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
export function output(name) {
  const file = path.resolve(OUT, name);
  fs.mkdirSync(path.dirname(file), { recursive: true });
  return file;
}

const TYPES = { '.html': 'text/html; charset=utf-8', '.js': 'text/javascript', '.css': 'text/css', '.svg': 'image/svg+xml', '.png': 'image/png', '.webp': 'image/webp', '.ico': 'image/x-icon', '.txt': 'text/plain', '.xml': 'application/xml', '.json': 'application/json' };
export async function serve(root, routes = {}, fallback = false) {
  root = path.resolve(root);
  const server = http.createServer((req, res) => {
    let pathname;
    try { pathname = decodeURIComponent(new URL(req.url, 'http://localhost').pathname); }
    catch { res.writeHead(400).end(); return; }
    if (routes[pathname]) {
      res.writeHead(200, { 'content-type': 'text/html; charset=utf-8' }).end(routes[pathname]);
      return;
    }
    let file = path.resolve(root, '.' + (pathname === '/' ? '/index.html' : pathname));
    if (!file.startsWith(root + path.sep)) { res.writeHead(403).end(); return; }
    let status = 200;
    if (!fs.existsSync(file) || !fs.statSync(file).isFile()) {
      if (!fallback) { res.writeHead(404).end(); return; }
      file = path.join(root, '404.html'); status = 404;
    }
    fs.readFile(file, (error, data) => {
      if (error) { res.writeHead(404).end(); return; }
      res.writeHead(status, { 'content-type': TYPES[path.extname(file)] ?? 'application/octet-stream', 'cache-control': 'no-store' }).end(data);
    });
  });
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  return { url: `http://127.0.0.1:${server.address().port}/`, close: () => new Promise((resolve) => { server.close(resolve); server.closeAllConnections(); }) };
}

function executable() {
  if (process.env.EDGE_PATH) return process.env.EDGE_PATH;
  const candidates = process.platform === 'win32'
    ? [process.env['ProgramFiles(x86)'], process.env.ProgramFiles, process.env.LOCALAPPDATA].filter(Boolean).map((base) => path.join(base, 'Microsoft/Edge/Application/msedge.exe'))
    : process.platform === 'darwin'
      ? ['/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge', '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome']
      : ['/usr/bin/microsoft-edge', '/usr/bin/chromium', '/usr/bin/chromium-browser', '/usr/bin/google-chrome'];
  const found = candidates.find((file) => fs.existsSync(file));
  if (!found) throw new Error('No Chromium browser found. Set EDGE_PATH to the Edge/Chrome/Chromium executable.');
  return found;
}

export async function startBrowser() {
  const binary = executable();
  const profile = fs.mkdtempSync(path.join(os.tmpdir(), 'osuflow-headless-'));
  const child = spawn(binary, ['--headless=new', '--disable-gpu', '--mute-audio', '--no-first-run', '--no-default-browser-check', '--disable-extensions', '--remote-debugging-address=127.0.0.1', '--remote-debugging-port=0', `--user-data-dir=${profile}`, 'about:blank'], { stdio: 'ignore', windowsHide: true });
  let launchError;
  child.on('error', (error) => { launchError = error; });
  let ws;
  let nextId = 1;
  const pending = new Map();
  const events = [];
  const close = async () => {
    try {
      if (ws?.readyState === WebSocket.OPEN) {
        const id = nextId++;
        ws.send(JSON.stringify({ id, method: 'Browser.close' }));
        ws.close();
      }
    } catch { /* browser already closed */ }
    for (let i = 0; i < 40 && child.exitCode === null && child.signalCode === null; i++) await sleep(100);
    if (child.exitCode === null && child.signalCode === null) { child.kill(); await sleep(300); }
    for (const { reject, timeout } of pending.values()) { clearTimeout(timeout); reject(new Error('Browser closed')); }
    pending.clear();
    if (path.dirname(profile) !== path.resolve(os.tmpdir()) || !path.basename(profile).startsWith('osuflow-headless-')) throw new Error('Refusing to remove an unexpected browser profile path');
    try { fs.rmSync(profile, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 }); }
    catch (error) { console.warn(`Temporary browser profile could not be removed: ${error.code}`); }
  };
  try {
    const portFile = path.join(profile, 'DevToolsActivePort');
    for (let i = 0; i < 120 && !fs.existsSync(portFile); i++) {
      if (launchError) throw launchError;
      if (child.exitCode !== null) throw new Error(`Headless browser exited (${child.exitCode})`);
      await sleep(100);
    }
    if (!fs.existsSync(portFile)) throw new Error('Timed out starting the headless browser');
    const [port, endpoint] = fs.readFileSync(portFile, 'utf8').trim().split(/\r?\n/);
    ws = new WebSocket(`ws://127.0.0.1:${port}${endpoint}`);
    await new Promise((resolve, reject) => { ws.onopen = resolve; ws.onerror = reject; });
    ws.onmessage = (message) => {
      const data = JSON.parse(message.data);
      const request = pending.get(data.id);
      if (request) {
        pending.delete(data.id); clearTimeout(request.timeout);
        data.error ? request.reject(new Error(data.error.message)) : request.resolve(data.result);
      } else if (data.method) events.push(data);
    };
    const send = (method, params = {}, sessionId) => new Promise((resolve, reject) => {
      const id = nextId++;
      const timeout = setTimeout(() => { pending.delete(id); reject(new Error(`CDP timed out: ${method}`)); }, 30000);
      pending.set(id, { resolve, reject, timeout });
      ws.send(JSON.stringify({ id, method, params, sessionId }));
    });
    return { send, events, close };
  } catch (error) { await close(); throw error; }
}
