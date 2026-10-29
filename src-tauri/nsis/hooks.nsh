; PDF.dk: the installer's own steps (Tauri's installerHooks), used by nsis/installer.nsi.
; This app used to be installed as "PDF.dk Desktop". An install under that name with this app's program in it
; is uninstalled first (settings and login are kept – they live under the bundle identifier), and
; its shortcuts come back under the new name: an update makes none of its own.
!define LEGACYNAME "PDF.dk Desktop"

Var LegacyDir
Var LegacyDesktopLnk

!macro NSIS_HOOK_PREINSTALL
  StrCpy $LegacyDir ""
  StrCpy $LegacyDesktopLnk 0
  ReadRegStr $R9 HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${LEGACYNAME}" "InstallLocation"
  ; Written in quotes.
  StrCpy $R8 $R9 1
  ${If} $R8 == '"'
    StrCpy $R9 $R9 "" 1
    StrCpy $R9 $R9 -1
  ${EndIf}
  ; Older installers wrote no InstallLocation: their default folder.
  ${IfThen} $R9 == "" ${|} StrCpy $R9 "$LOCALAPPDATA\${LEGACYNAME}" ${|}
  ; Only if it is this app (the other app may have had this name).
  ${If} $R9 != ""
  ${AndIf} ${FileExists} "$R9\${MAINBINARYNAME}.exe"
    StrCpy $LegacyDir $R9
    ${If} ${FileExists} "$DESKTOP\${LEGACYNAME}.lnk"
      StrCpy $LegacyDesktopLnk 1
    ${EndIf}
    !insertmacro CheckIfAppIsRunning "${MAINBINARYNAME}.exe" "${LEGACYNAME}"
    DetailPrint "PDF.dk Flow: removing the install under its old name, ${LEGACYNAME}"
    ExecWait '"$LegacyDir\uninstall.exe" /S _?=$LegacyDir'
    Delete "$LegacyDir\uninstall.exe"
    RMDir "$LegacyDir"
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ${If} $LegacyDir != ""
    CreateShortcut "$SMPROGRAMS\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    !insertmacro SetLnkAppUserModelId "$SMPROGRAMS\${PRODUCTNAME}.lnk"
    ${If} $LegacyDesktopLnk = 1
      CreateShortcut "$DESKTOP\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
      !insertmacro SetLnkAppUserModelId "$DESKTOP\${PRODUCTNAME}.lnk"
    ${EndIf}
  ${EndIf}
!macroend
