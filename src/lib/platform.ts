export type Platform = "windows" | "linux" | "macos";

export interface PlatformInfo {
  os: Platform;
  wayland: boolean;
  defaultHotkey: string;
  experimental: boolean;
}

export function displayHotkey(hotkey: string, platform: Platform): string {
  return hotkey
    .replace(/CommandOrControl/gi, platform === "macos" ? "Command" : "Ctrl")
    .replace(/\b(Super|Meta)\b/g, platform === "macos" ? "Command" : "Super");
}
