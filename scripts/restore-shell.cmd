@echo off
REM Restore the previous Winlogon Shell (undo Explorer / dwm replacement).
REM Same as: scripts\install-shell.cmd /restore
REM
REM Log off or reboot after running for Explorer to come back.

setlocal EnableExtensions EnableDelayedExpansion

set "ROOT=%~dp0.."
pushd "%ROOT%" >nul 2>&1
if errorlevel 1 (
  echo error: cannot cd to repo root
  exit /b 1
)

set "WINLOGON=HKCU\Software\Microsoft\Windows NT\CurrentVersion\Winlogon"
set "BACKUP_VAL=Shell.fuckfm.bak"

echo ==^> Restoring previous Winlogon Shell

reg query "%WINLOGON%" /v %BACKUP_VAL% >nul 2>&1
if errorlevel 1 (
  echo     no %BACKUP_VAL% found — setting Shell=explorer.exe
  reg add "%WINLOGON%" /v Shell /t REG_SZ /d "explorer.exe" /f >nul
  if errorlevel 1 (
    echo error: failed to set Shell=explorer.exe
    popd >nul
    exit /b 1
  )
) else (
  for /f "tokens=2*" %%A in ('reg query "%WINLOGON%" /v %BACKUP_VAL% ^| findstr /I "%BACKUP_VAL%"') do (
    set "PREV=%%B"
  )
  if not defined PREV set "PREV=explorer.exe"
  reg add "%WINLOGON%" /v Shell /t REG_SZ /d "!PREV!" /f >nul
  if errorlevel 1 (
    echo error: failed to restore Shell=!PREV!
    popd >nul
    exit /b 1
  )
  echo     restored Shell=!PREV!
  reg delete "%WINLOGON%" /v %BACKUP_VAL% /f >nul 2>&1
)

echo     Log off / reboot for the previous shell to start.
popd >nul
exit /b 0
