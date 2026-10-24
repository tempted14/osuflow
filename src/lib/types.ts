// Mirrors the Rust types in src-tauri/src (models.rs, mirrors.rs, downloader.rs, ...).
// Beatmapset/Beatmap keep the osu! API's snake_case names; the wrappers around them are camelCase.

export type Mirror = "catboy" | "osuDirect";

export type Sort =
  | "relevance"
  | "rankedDate"
  | "updated"
  | "submitted"
  | "plays"
  | "favourites"
  | "difficulty"
  | "bpm"
  | "length"
  | "title"
  | "artist";

export type SortDirection = "asc" | "desc";

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
  /** Which way to sort; null for relevance, which has no direction. */
  direction: SortDirection | null;
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
  hotkey: string;
  hideInstalled: boolean;
  linkHandler: boolean;
  previousBrowser: string | null;
}

export interface HotkeyStatus {
  hotkey: string;
  /** False when Windows refused the shortcut, usually because another program owns it. */
  registered: boolean;
  error: string | null;
}

/** Sent with the `overlay-shown` event. */
export interface OverlayContext {
  osuRunning: boolean;
  exclusiveFullscreen: boolean;
}

/** Payload of the `library-changed` event. */
export interface LibraryDelta {
  added: number[];
  removed: number[];
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

/** What the archive is named after, when the caller already has the map's details. */
export interface NameHint {
  artist: string;
  title: string;
}

export type ToastState = "resolving" | "installed" | "working" | "done" | "failed" | "hidden";

/** Payload of the `link-toast` event: what the corner toast shows for a clicked beatmap link. */
export interface LinkToast {
  state: ToastState;
  setId: number | null;
  label: string;
  message: string | null;
  /** The link that was clicked, for "Open page" and "Try again". */
  url: string;
}

export interface LinkHandlerStatus {
  /** The user's switch. */
  enabled: boolean;
  /** osu!flow is listed as a browser in Windows' Default apps. */
  registered: boolean;
  /** The user has chosen osu!flow there - the one step Windows won't let an app do. */
  isDefault: boolean;
  previousBrowser: string | null;
}

/** One download as the overlay tracks it, built from the `download-*` events. */
export interface DownloadJob {
  setId: number;
  label: string;
  state: DownloadState;
  received: number;
  total: number | null;
  error: string | null;
  mirror: string | null;
}
