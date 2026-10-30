; PDF.dk: the installer's own steps (Tauri's installerHooks), used by nsis/installer.nsi.
; This app used to be installed under another name and program file (until 0.5: product name
; "PDF.dk Desktop"; a 0.6.0 test build: id dk.pdf.desktop; both with pdfdk-desktop.exe). Such an install
; is uninstalled first (settings and login are kept – the app moves them over itself), its
; leftover start-at-login entry goes, and its shortcuts come back under the new name: an update
; makes none of its own.
!define LEGACYBINARY "pdfdk-desktop.exe"

Var LegacyFound
Var LegacyDesktopLnk

; One old install, by its Uninstall key name and its default folder.
!macro PDFDK_REMOVE_LEGACY KEYNAME DEFAULTDIR
  ReadRegStr $R9 HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${KEYNAME}" "InstallLocation"
  ; Written in quotes.
  StrCpy $R8 $R9 1
  ${If} $R8 == '"'
    StrCpy $R9 $R9 "" 1
    StrCpy $R9 $R9 -1
  ${EndIf}
  ; Older installers wrote no InstallLocation: their default folder.
  ${IfThen} $R9 == "" ${|} StrCpy $R9 "${DEFAULTDIR}" ${|}
  ; Only if it is this app (the other app may have had this name).
  ${If} ${FileExists} "$R9\${LEGACYBINARY}"
    StrCpy $LegacyFound 1
    ${If} ${FileExists} "$DESKTOP\${KEYNAME}.lnk"
    ${OrIf} ${FileExists} "$DESKTOP\${PRODUCTNAME}.lnk"
      StrCpy $LegacyDesktopLnk 1
    ${EndIf}
    !insertmacro CheckIfAppIsRunning "${LEGACYBINARY}" "${PRODUCTNAME}"
    DetailPrint "${PRODUCTNAME}: removing the previous version"
    ExecWait '"$R9\uninstall.exe" /S _?=$R9'
    Delete "$R9\uninstall.exe"
    Delete "$R9\${LEGACYBINARY}"
    RMDir "$R9"
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREINSTALL
  StrCpy $LegacyFound 0
  StrCpy $LegacyDesktopLnk 0
  !insertmacro PDFDK_REMOVE_LEGACY "PDF.dk Desktop" "$LOCALAPPDATA\PDF.dk Desktop"
  !insertmacro PDFDK_REMOVE_LEGACY "dk.pdf.desktop" "$LOCALAPPDATA\Programs\${PRODUCTNAME}"
  ; The old start-at-login entry (named after the old product name) – only when it starts this app.
  ReadRegStr $R9 HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "PDF.dk Desktop"
  ${StrLoc} $R8 $R9 "${LEGACYBINARY}" ">"
  ${If} $R8 != ""
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "PDF.dk Desktop"
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "PDF.dk Desktop"
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ${If} $LegacyFound = 1
    CreateShortcut "$SMPROGRAMS\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    !insertmacro SetLnkAppUserModelId "$SMPROGRAMS\${PRODUCTNAME}.lnk"
    ${If} $LegacyDesktopLnk = 1
      CreateShortcut "$DESKTOP\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
      !insertmacro SetLnkAppUserModelId "$DESKTOP\${PRODUCTNAME}.lnk"
    ${EndIf}
  ${EndIf}
!macroend
