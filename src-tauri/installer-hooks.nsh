; osu!flow's installer hooks (see bundle.windows.nsis.installerHooks in tauri.conf.json).
;
; Uninstalling should leave nothing behind outside the app's own folder. These are the only
; entries osu!flow can create elsewhere, and all of them are per-user (HKCU):
;   - "Start with Windows" adds a value to the Run key.
;   - "Beatmap links" registers osu!flow as a browser that can be chosen in Default apps.
; If osu!flow was the default browser, Windows simply asks for a new one the next time a link is
; opened, as it does after uninstalling any browser.

!macro NSIS_HOOK_PREUNINSTALL
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "osu!flow"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "osu!flow"
  DeleteRegValue HKCU "Software\RegisteredApplications" "osu!flow"
  DeleteRegKey HKCU "Software\Clients\StartMenuInternet\osuflow"
  DeleteRegKey HKCU "Software\Classes\OsuFlowURL"
!macroend
