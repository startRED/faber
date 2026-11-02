; The move from the Tauri Faber (R459). Run without `/D=`, the installer goes where the Tauri one installed (its uninstall
; key's InstallLocation, written with quotes); every install removes the Tauri uninstaller and its uninstall key, so Apps
; and features shows one Faber. The data folder is never touched: settings and conversations stay where they are.
; FABER_INSTALLER_SMOKE=1 (scripts/electron-smoke.mjs) leaves the real registry alone.
!macro customInit
  ReadEnvStr $R1 "FABER_INSTALLER_SMOKE"
  StrCmp $R1 "1" done
  !insertmacro GetDParameter $R0
  StrCmp $R0 "" 0 done
  ReadRegStr $R0 HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Faber" "InstallLocation"
  StrCmp $R0 "" done

  StrCpy $R1 $R0 1
  StrCmp $R1 '$\"' 0 useLocation
  StrCpy $R0 $R0 -1 1
  useLocation:
  StrCpy $INSTDIR $R0
  done:
!macroend

!macro customInstall
  Delete "$INSTDIR\uninstall.exe"
  Delete "$INSTDIR\faber-local.exe"
  ReadEnvStr $R0 "FABER_INSTALLER_SMOKE"
  StrCmp $R0 "1" done
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Faber"
  done:
!macroend
