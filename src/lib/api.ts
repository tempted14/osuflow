// Thin typed wrappers over the Tauri commands in src-tauri/src/commands.rs.
import { invoke } from "@tauri-apps/api/core";
import type {
  Beatmapset,
  HotkeyStatus,
  ImportMethod,
  LinkHandlerStatus,
  OsuInstall,
  SearchQuery,
  SearchResponse,
  Settings,
} from "./types";

export const search = (query: SearchQuery) => invoke<SearchResponse>("search", { query });

/** Resolves once the map has been handed to osu!; progress arrives as `download-*` events. */
export const downloadBeatmapset = (setId: number) =>
  invoke<ImportMethod>("download_beatmapset", { setId });

export const cancelDownload = (setId: number) => invoke<boolean>("cancel_download", { setId });

export const resolveSetId = (beatmapId: number) => invoke<number>("resolve_set_id", { beatmapId });

export const lookupBeatmapset = (setId: number) =>
  invoke<Beatmapset>("lookup_beatmapset", { setId });

export const detectOsu = () => invoke<OsuInstall | null>("detect_osu");

export const getSettings = () => invoke<Settings>("get_settings");

export const setSettings = (settings: Settings) => invoke<Settings>("set_settings", { settings });

/** Ids of every beatmapset already in the Songs folder. */
export const getInstalled = () => invoke<number[]>("get_installed");

export const showOverlay = () => invoke<void>("show_overlay");
export const hideOverlay = () => invoke<void>("hide_overlay");
export const toggleOverlay = () => invoke<void>("toggle_overlay");

export const getHotkeyStatus = () => invoke<HotkeyStatus>("get_hotkey_status");

/** Rejects with a readable message when the shortcut is invalid or taken by another program. */
export const setHotkey = (hotkey: string) => invoke<HotkeyStatus>("set_hotkey", { hotkey });

export const openBeatmapsetPage = (setId: number) =>
  invoke<void>("open_beatmapset_page", { setId });

export const getLinkHandlerStatus = () => invoke<LinkHandlerStatus>("get_link_handler_status");

/** Registers QuickDirect as a selectable browser (or switches handling off). */
export const setLinkHandler = (enabled: boolean) =>
  invoke<LinkHandlerStatus>("set_link_handler", { enabled });

/** Rejects while QuickDirect is still the default browser. */
export const removeLinkRegistration = () => invoke<LinkHandlerStatus>("remove_link_registration");

export const openDefaultApps = () => invoke<void>("open_default_apps");

/** The toast's buttons: open the clicked link in the user's browser, retry it, or hide the toast. */
export const toastOpenPage = (url: string) => invoke<void>("toast_open_page", { url });
export const toastRetry = (url: string) => invoke<void>("toast_retry", { url });
export const dismissToast = () => invoke<void>("dismiss_toast");
