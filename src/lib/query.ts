import type { Beatmap, Beatmapset } from "./types";

/**
 * The osu! website's and osu!lazer's search syntax, for what the mirrors can't filter themselves:
 * `stars>5 ar<=9.5 bpm=180..200 length<3m keys=4 creator=name`. Words that aren't one of these
 * stay ordinary search words and go to the mirror; the rest narrow the results inside the app.
 */

export type NumericKey = "stars" | "ar" | "cs" | "od" | "hp" | "bpm" | "length" | "keys";
export type FieldKey = "artist" | "title" | "creator";

/** A band of allowed values for one difficulty property; either end may be open or unbounded. */
export interface NumericConstraint {
  key: NumericKey;
  min: number;
  max: number;
  minOpen: boolean;
  maxOpen: boolean;
}

export interface FieldConstraint {
  key: FieldKey;
  value: string;
}

/** A piece of the text that was understood, with where it sits so it can be taken out again. */
export interface QueryChip {
  label: string;
  start: number;
  end: number;
}

export interface ParsedQuery {
  /** What the mirrors are asked for: the ordinary words, plus the names given to artist, title and creator. */
  text: string;
  numeric: NumericConstraint[];
  fields: FieldConstraint[];
  chips: QueryChip[];
  /** Words that look like a filter but have a value that can't be read (the one still being typed is left out). */
  ignored: string[];
}

export const NO_QUERY: ParsedQuery = { text: "", numeric: [], fields: [], chips: [], ignored: [] };

const NUMERIC_KEYS: Record<string, NumericKey> = {
  star: "stars",
  stars: "stars",
  sr: "stars",
  ar: "ar",
  cs: "cs",
  od: "od",
  hp: "hp",
  dr: "hp",
  drain: "hp",
  bpm: "bpm",
  length: "length",
  len: "length",
  key: "keys",
  keys: "keys",
};

const FIELD_KEYS: Record<string, FieldKey> = { artist: "artist", title: "title", creator: "creator", mapper: "creator" };

const NAMES: Record<NumericKey | FieldKey, string> = {
  stars: "Stars",
  ar: "AR",
  cs: "CS",
  od: "OD",
  hp: "HP",
  bpm: "BPM",
  length: "Length",
  keys: "Keys",
  artist: "Artist",
  title: "Title",
  creator: "Mapper",
};

const SYMBOLS: Record<string, string> = { ">=": "≥", "<=": "≤", ">": ">", "<": "<", "=": "=", "==": "=", ":": "=" };

/** `a..b` keeps its two ends. */
const RANGE = /^(.+?)\.\.(.+)$/;

interface Token {
  raw: string;
  start: number;
  end: number;
}

/** Splits on spaces, except inside double quotes, remembering where each piece sits. */
function tokenize(text: string): Token[] {
  const tokens: Token[] = [];
  let start = -1;
  let quoted = false;
  for (let index = 0; index <= text.length; index++) {
    const char = text[index];
    const boundary = index === text.length || (!quoted && /\s/.test(char));
    if (boundary) {
      if (start >= 0) tokens.push({ raw: text.slice(start, index), start, end: index });
      start = -1;
      quoted = false;
      continue;
    }
    if (start < 0) start = index;
    if (char === '"') quoted = !quoted;
  }
  return tokens;
}

interface Reading {
  value: number;
  /** How far from `value` still counts as equal to it. */
  tolerance: number;
}

const NUMBER = /^(?:\d+\.?\d*|\.\d+)$/;

function readNumber(text: string, tolerance: number): Reading | null {
  return NUMBER.test(text) ? { value: Number(text), tolerance } : null;
}

/** `90`, `90s`, `3m`, `1.5h`, or `3:30`. A bare number is seconds, as in the game. */
function readLength(text: string): Reading | null {
  const clock = /^(\d+):([0-5]?\d)$/.exec(text);
  if (clock) return { value: Number(clock[1]) * 60 + Number(clock[2]), tolerance: 0.5 };
  const unit = /^(\d+\.?\d*|\.\d+)([smh])?$/i.exec(text);
  if (!unit) return null;
  const seconds = { s: 1, m: 60, h: 3600 }[(unit[2] ?? "s").toLowerCase() as "s" | "m" | "h"];
  return { value: Number(unit[1]) * seconds, tolerance: seconds / 2 };
}

function read(key: NumericKey, text: string): Reading | null {
  switch (key) {
    case "stars":
      return readNumber(text.replace(/[★*]$/, ""), 0.005);
    case "bpm":
      return readNumber(text, 0.5);
    case "length":
      return readLength(text);
    case "keys":
      return /^\d+$/.test(text) ? { value: Number(text), tolerance: 0.01 } : null;
    default:
      return readNumber(text, 0.05);
  }
}

function constraintFor(key: NumericKey, op: string, valueText: string): NumericConstraint | null {
  const equals = op === "=" || op === ":" || op === "==";
  const range = equals ? RANGE.exec(valueText) : null;
  if (range) {
    const [low, high] = [read(key, range[1]), read(key, range[2])];
    if (!low || !high) return null;
    const [min, max] = low.value <= high.value ? [low.value, high.value] : [high.value, low.value];
    return { key, min, max, minOpen: false, maxOpen: false };
  }
  const reading = read(key, valueText);
  if (!reading) return null;
  const { value, tolerance } = reading;
  if (equals) return { key, min: value - tolerance, max: value + tolerance, minOpen: false, maxOpen: false };
  if (op.startsWith(">")) return { key, min: value, max: Infinity, minOpen: op === ">", maxOpen: false };
  return { key, min: -Infinity, max: value, minOpen: false, maxOpen: op === "<" };
}

/**
 * Picks the filters out of what was typed. A filter word with nothing after it yet (`stars>`) or a
 * value that can't be read is left out of the search rather than sent to the mirrors as a stray word.
 */
export function parseQuery(text: string): ParsedQuery {
  const typing = text.length > 0 && !/\s$/.test(text);
  const tokens = tokenize(text);
  const words: string[] = [];
  const query: ParsedQuery = { text, numeric: [], fields: [], chips: [], ignored: [] };
  let understood = false;

  tokens.forEach((token, position) => {
    const match = /^([a-z]+)(>=|<=|==|=|:|>|<)(.*)$/i.exec(token.raw);
    const name = match?.[1].toLowerCase() ?? "";
    const op = match?.[2] ?? "";
    const value = match?.[3] ?? "";

    const numericKey = NUMERIC_KEYS[name];
    if (match && numericKey) {
      understood = true;
      const constraint = value ? constraintFor(numericKey, op, value) : null;
      if (constraint) {
        query.numeric.push(constraint);
        const shown = RANGE.exec(value) && SYMBOLS[op] === "=" ? value.replace("..", "–") : `${SYMBOLS[op]} ${value}`;
        query.chips.push({ label: `${NAMES[numericKey]} ${shown}`, start: token.start, end: token.end });
      } else if (value && !(typing && position === tokens.length - 1)) {
        query.ignored.push(token.raw);
      }
      return;
    }

    const fieldKey = FIELD_KEYS[name];
    if (match && fieldKey && (op === "=" || op === ":" || op === "==")) {
      understood = true;
      const given = value.replace(/^"/, "").replace(/"$/, "").trim();
      if (given) {
        query.fields.push({ key: fieldKey, value: given });
        words.push(given);
        query.chips.push({ label: `${NAMES[fieldKey]}: ${given}`, start: token.start, end: token.end });
      }
      return;
    }

    words.push(token.raw);
  });

  if (understood) query.text = words.join(" ");
  return query;
}

/** True when the words narrow the results by themselves: something the app must check on what the mirrors sent. */
export function queryNarrows(query: ParsedQuery): boolean {
  return query.numeric.length > 0 || query.fields.length > 0;
}

/** The text without one of its filters, with the spaces around it tidied. */
export function withoutChip(text: string, chip: QueryChip): string {
  const before = text.slice(0, chip.start).replace(/\s+$/, "");
  const after = text.slice(chip.end).replace(/^\s+/, "");
  return before && after ? `${before} ${after}` : before || after;
}

const EPSILON = 1e-9;

function valueOf(map: Beatmap, key: NumericKey): number | null {
  switch (key) {
    case "stars":
      return map.difficulty_rating;
    case "ar":
      return map.ar;
    case "cs":
      return map.cs;
    case "od":
      return map.accuracy;
    case "hp":
      return map.drain;
    case "bpm":
      return map.bpm;
    case "length":
      return map.total_length;
    case "keys":
      // Only mania maps have keys, and the game stores the key count as the circle size.
      return map.mode_int === 3 ? map.cs : null;
  }
}

/** True when this one difficulty satisfies every numeric filter, as it does in the game. */
export function mapMatches(map: Beatmap, constraints: readonly NumericConstraint[]): boolean {
  return constraints.every((constraint) => {
    const value = valueOf(map, constraint.key);
    if (value === null || !Number.isFinite(value)) return false;
    const aboveMin = constraint.minOpen ? value > constraint.min + EPSILON : value >= constraint.min - EPSILON;
    const belowMax = constraint.maxOpen ? value < constraint.max - EPSILON : value <= constraint.max + EPSILON;
    return aboveMin && belowMax;
  });
}

/** Whether the artist, title and creator filters all hold (a substring, either spelling, any case). */
export function fieldsMatch(set: Beatmapset, fields: readonly FieldConstraint[]): boolean {
  return fields.every(({ key, value }) => {
    const wanted = value.toLowerCase();
    const candidates =
      key === "artist" ? [set.artist, set.artist_unicode] : key === "title" ? [set.title, set.title_unicode] : [set.creator];
    return candidates.some((candidate) => candidate?.toLowerCase().includes(wanted));
  });
}
