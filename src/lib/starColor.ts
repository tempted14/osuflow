// osu!'s difficulty spectrum: the colour a map's star rating gets on the website and in game.
const STOPS: [number, [number, number, number]][] = [
  [0.1, [0x42, 0x90, 0xfb]],
  [1.25, [0x4f, 0xc0, 0xff]],
  [2, [0x4f, 0xff, 0xd5]],
  [2.5, [0x7c, 0xff, 0x4f]],
  [3.3, [0xf6, 0xf0, 0x5c]],
  [4.2, [0xff, 0x80, 0x68]],
  [4.9, [0xff, 0x4e, 0x6f]],
  [5.8, [0xc6, 0x45, 0xb8]],
  [6.7, [0x65, 0x63, 0xde]],
  [7.7, [0x18, 0x15, 0x8e]],
  [9, [0x00, 0x00, 0x00]],
];

const hex = (channel: number) => Math.round(channel).toString(16).padStart(2, "0");
const toHex = ([r, g, b]: [number, number, number]) => `#${hex(r)}${hex(g)}${hex(b)}`;

/** "#rrggbb" for a star rating: grey below 0.1 stars, black from 9, blended in between. */
export function starColor(stars: number): string {
  if (!(stars >= STOPS[0][0])) return "#aaaaaa";
  const last = STOPS[STOPS.length - 1];
  if (stars >= last[0]) return toHex(last[1]);
  const upper = STOPS.findIndex(([at]) => stars < at);
  const [lowAt, low] = STOPS[upper - 1];
  const [highAt, high] = STOPS[upper];
  const t = (stars - lowAt) / (highAt - lowAt);
  return toHex([0, 1, 2].map((i) => low[i] + (high[i] - low[i]) * t) as [number, number, number]);
}
