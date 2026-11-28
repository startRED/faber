; The move from the Tauri Faber (R459). Run without `/D=`, the installer goes where the Tauri one installed (its uninstall
; key's InstallLocation, written with quotes); every install removes the Tauri uninstaller and its uninstall key, so Apps
; and features shows one Faber. The data folder is never touched: settings and conversations stay where they are.
; FABER_INSTALLER_SMOKE=1 (scripts/electron-smoke.mjs) leaves the real registry alone.
!macro customHeader
  ; The uninstaller build includes this header too and never uses it (warning 6001 is an error).
  !ifndef BUILD_UNINSTALLER
    Var /GLOBAL WaybackTemp
  !endif
!macroend

!macro customInit
  ReadEnvStr $R1 "FABER_INSTALLER_SMOKE"
  StrCmp $R1 "1" backup
  !insertmacro GetDParameter $R0
  StrCmp $R0 "" 0 backup
  ReadRegStr $R0 HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Faber" "InstallLocation"
  StrCmp $R0 "" backup

  ; The key is only a hint a process of the same account could have rewritten: it is used when it is an absolute folder
  ; (drive letter), its quotes balance, and a Faber program is really there. Anything else installs where the installer
  ; would by default.
  StrCpy $R1 $R0 1
  StrCmp $R1 '$\"' 0 absolute
  StrCpy $R2 $R0 1 -1
  StrCmp $R2 '$\"' 0 backup
  StrCpy $R0 $R0 -1 1
  absolute:
  StrCpy $R1 $R0 1 1
  StrCmp $R1 ":" 0 backup
  IfFileExists "$R0\faber.exe" 0 backup
  StrCpy $INSTDIR $R0
  backup:
  ; Any Electron file means the current faber.exe is Electron (a half-finished install keeps them too): never save it as the Tauri copy.
  ; A saved Tauri copy is never replaced: an interrupted migration can leave Electron with no marker file.
  IfFileExists "$INSTDIR\previous\faber-tauri.exe" preserve
  IfFileExists "$INSTDIR\resources\app.asar" preserve
  IfFileExists "$INSTDIR\chrome_100_percent.pak" preserve
  IfFileExists "$INSTDIR\icudtl.dat" preserve
  IfFileExists "$INSTDIR\v8_context_snapshot.bin" preserve
  IfFileExists "$INSTDIR\ffmpeg.dll" preserve
  IfFileExists "$INSTDIR\libEGL.dll" preserve
  IfFileExists "$INSTDIR\faber.exe" 0 done
  CreateDirectory "$INSTDIR\previous"
  ClearErrors
  CopyFiles /SILENT "$INSTDIR\faber.exe" "$INSTDIR\previous\faber-tauri.exe.new"
  IfErrors backup_failed
  Delete "$INSTDIR\previous\faber-tauri.exe.old"
  Rename "$INSTDIR\previous\faber-tauri.exe" "$INSTDIR\previous\faber-tauri.exe.old"
  ClearErrors
  Rename "$INSTDIR\previous\faber-tauri.exe.new" "$INSTDIR\previous\faber-tauri.exe"
  IfErrors backup_failed
  Delete "$INSTDIR\previous\faber-tauri.exe.old"
  FileOpen $R2 "$INSTDIR\previous\README.txt" w
  FileWrite $R2 "To go back, close Faber, disconnect the internet, and run faber-tauri.exe or reinstall your old version. Restore your data backup first if needed. Reconnecting may update Faber again.$\r$\n"
  FileClose $R2
  Goto done
  preserve:
  ; The old Electron uninstaller removes its installation folder during updates.
  IfFileExists "$INSTDIR\previous\faber-tauri.exe" 0 done
  ClearErrors
  GetTempFileName $WaybackTemp
  IfErrors backup_failed
  CopyFiles /SILENT "$INSTDIR\previous\faber-tauri.exe" "$WaybackTemp"
  IfErrors backup_failed
  Goto done
  backup_failed:
  Abort "Could not save the previous Faber. The installation was stopped."
  done:
!macroend

!macro customInstall
  StrCmp $WaybackTemp "" restored
  CreateDirectory "$INSTDIR\previous"
  ClearErrors
  CopyFiles /SILENT "$WaybackTemp" "$INSTDIR\previous\faber-tauri.exe"
  IfErrors restore_failed
  Delete "$WaybackTemp"
  FileOpen $R2 "$INSTDIR\previous\README.txt" w
  FileWrite $R2 "To go back, close Faber, disconnect the internet, and run faber-tauri.exe or reinstall your old version. Restore your data backup first if needed. Reconnecting may update Faber again.$\r$\n"
  FileClose $R2
  Goto restored
  restore_failed:
  Abort "Could not restore the previous Faber. Its copy is at $WaybackTemp."
  restored:
  Delete "$INSTDIR\uninstall.exe"
  Delete "$INSTDIR\faber-local.exe"
  ReadEnvStr $R0 "FABER_INSTALLER_SMOKE"
  StrCmp $R0 "1" done
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Faber"
  done:
!macroend
