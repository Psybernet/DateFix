; Extra steps for the DateFix installer.
; DateFix keeps its undo history and browser data in "DateFix data" next to
; DateFix.exe. Remove that folder on uninstall so nothing is left behind.
!macro NSIS_HOOK_POSTUNINSTALL
  RMDir /r "$INSTDIR\DateFix data"
  RMDir "$INSTDIR"
!macroend
