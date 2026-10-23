// Mirrors the Rust types in src-tauri/src (models.rs, mirrors.rs, downloader.rs, ...).
// Beatmapset/Beatmap keep the osu! API's snake_case names; the wrappers around them are camelCase.

export type Mirror = "catboy" | "osuDirect";

export type Sort =
  | "relevance"
  | "rankedDate"
  | "plays"
  | "favourites"
  | "updated"
  | "title"
  | "artist"
  | "difficulty";

export interface Covers {
  cover: string;
  "cover@2x": string;
  card: string;
  "card@2x": string;
  list: string;
  "list@2x": string;
  slimcover: string;
  "slimcover@2x": string;
}

export interface Beatmap {
  id: number;
  beatmapset_id: number;
  version: string;
  mode: string;
  mode_int: number;
  difficulty_rating: number;
  total_length: number;
  hit_length: number;
  bpm: number;
  cs: number;
  ar: number;
  accuracy: number;
  drain: number;
  count_circles: number;
  count_sliders: number;
  count_spinners: number;
  max_combo: number | null;
  convert: boolean;
}

export interface Beatmapset {
  id: number;
  title: string;
  title_unicode: string;
  artist: string;
  artist_unicode: string;
  creator: string;
  source: string;
  status: string;
  ranked: number;
  covers: Covers;
  play_count: number;
  favourite_count: number;
  video: boolean;
  storyboard: boolean;
  nsfw: boolean;
  bpm: number;
  preview_url: string | null;
  last_updated: string | null;
  ranked_date: string | null;
  submitted_date: string | null;
  beatmaps: Beatmap[];
}

export interface SearchQuery {
  query: string;
  /** 0 osu!, 1 taiko, 2 catch, 3 mania; null for all modes. */
  mode: number | null;
  /** -2 graveyard, -1 wip, 0 pending, 1 ranked, 2 approved, 3 qualified, 4 loved. */
  status: number;
  sort: Sort;
  offset: number;
  limit: number;
}

export interface SearchResponse {
  results: Beatmapset[];
  mirror: Mirror;
  /** False when the chosen sort couldn't be honoured because only catboy.best answered. */
  sortApplied: boolean;
  hasMore: boolean;
}

export interface OsuInstall {
  exe: string;
  dir: string;
  songsDir: string;
  exclusiveFullscreen: boolean;
  source: "settings" | "runningProcess" | "registry" | "defaultPath";
}

export interface Settings {
  osuExePath: string | null;
  songsDir: string | null;
  noVideo: boolean;
  mirrorOrder: Mirror[];
}

export type DownloadState =
  | "queued"
  | "downloading"
  | "importing"
  | "done"
  | "failed"
  | "cancelled";

export interface DownloadStateEvent {
  setId: number;
  state: DownloadState;
  error: string | null;
}

export interface DownloadProgressEvent {
  setId: number;
  received: number;
  total: number | null;
  mirror: string;
}

export type ImportMethod = "launchedOsu" | "movedToSongs";
