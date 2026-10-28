// Refresh the curated screenshot fixture from the public mirror; never downloads .osz archives.
import fs from 'node:fs';
import path from 'node:path';
import { HERE } from './browser.mjs';

const source = 'https://osu.direct/api/v2/search?status=1&mode=0&amount=50&sort=play_count:desc';
// Recognisable songs with a range of difficulties; exclude explicit/NSFW sets.
const ids = [320118, 765778, 596704, 873811, 781509, 399358, 163112, 332532,
  158023, 593620, 527431, 513590, 387700, 292301, 128931, 962088];
const response = await fetch(source, { signal: AbortSignal.timeout(30000) });
if (!response.ok) throw new Error(`Mirror search failed: ${response.status}`);
const results = await response.json();
if (!Array.isArray(results)) throw new Error('Unexpected mirror response: expected an array');
const sets = [];
const checks = [];
for (const id of ids) {
  let set = results.find((candidate) => candidate.id === id);
  if (!set) {
    const lookup = await fetch(`https://osu.direct/api/v2/s/${id}`, { signal: AbortSignal.timeout(30000) });
    if (!lookup.ok) throw new Error(`Set ${id}: lookup failed (${lookup.status})`);
    set = await lookup.json();
  }
  if (set.id !== id || set.status !== 'ranked' || set.ranked !== 1 || set.nsfw !== false || !set.beatmaps?.some((map) => map.mode_int === 0)) {
    throw new Error(`Set ${id} is not a non-NSFW ranked osu!standard set`);
  }
  const covers = {};
  for (const key of ['list', 'list@2x', 'card', 'card@2x']) {
    const url = new URL(set.covers[key]);
    if (url.hostname !== 'assets.ppy.sh') throw new Error(`Unexpected cover host for ${id}`);
    const cover = await fetch(url, { signal: AbortSignal.timeout(30000) });
    if (!cover.ok || !cover.headers.get('content-type')?.startsWith('image/')) throw new Error(`Broken ${key} cover: ${id} (${cover.status})`);
    const bytes = (await cover.arrayBuffer()).byteLength;
    if (!bytes) throw new Error(`Empty ${key} cover: ${id}`);
    covers[key] = { status: cover.status, bytes };
  }
  sets.push(set); // Keep the exact mirror object shape and metadata.
  checks.push({ id, title: set.title, artist: set.artist, playCount: set.play_count, covers });
  console.log(`${id}: ${set.artist} - ${set.title} (4 covers OK)`);
}
fs.writeFileSync(path.join(HERE, 'real-sets.json'), JSON.stringify(sets, null, 2) + '\n');
fs.writeFileSync(path.join(HERE, 'real-sets.meta.json'), JSON.stringify({ source, retrievedAt: new Date().toISOString(), checks }, null, 2) + '\n');
console.log(`Wrote ${sets.length} ranked, non-NSFW sets with validated covers.`);
