@echo off
REM Build Windows release binaries and register dwm.exe as the Winlogon Shell
REM (Explorer.exe replacement for this user). Run from an elevated or normal
REM CMD in the repo root, or double-click after editing paths.
REM
REM Usage:
REM   scripts\install-shell.cmd              build + install shell
REM   scripts\install-shell.cmd /build-only  build only
REM   scripts\install-shell.cmd /restore     restore previous Shell value
REM   scripts\install-shell.cmd /status      show current Shell
REM
REM Log off / reboot after install or restore for the change to take effect.

setlocal EnableExtensions EnableDelayedExpansion

set "ROOT=%~dp0.."
pushd "%ROOT%" >nul 2>&1
if errorlevel 1 (
  echo error: cannot cd to repo root
  exit /b 1
)

set "ACTION=install"
if /I "%~1"=="/restore" set "ACTION=restore"
if /I "%~1"=="/build-only" set "ACTION=build"
if /I "%~1"=="/status" set "ACTION=status"
if /I "%~1"=="/help" goto :help
if /I "%~1"=="-h" goto :help
if /I "%~1"=="--help" goto :help

set "WINLOGON=HKCU\Software\Microsoft\Windows NT\CurrentVersion\Winlogon"
set "BACKUP_VAL=Shell.fuckfm.bak"
set "PROFILE=release"
set "OUT=%ROOT%\target\%PROFILE%"

where cargo >nul 2>&1
if errorlevel 1 (
  echo error: cargo not found in PATH
  exit /b 1
)

if /I "%ACTION%"=="status" goto :status
if /I "%ACTION%"=="restore" goto :restore

echo ==^> Building Windows release bins
cargo build --release --bins
if errorlevel 1 (
  echo error: cargo build failed
  exit /b 1
)

if not exist "%OUT%\dwm.exe" (
  echo error: missing %OUT%\dwm.exe
  exit /b 1
)
if not exist "%OUT%\fuckfm.exe" (
  echo error: missing %OUT%\fuckfm.exe
  exit /b 1
)

REM Stage icon packs next to the shell binary
if exist "%ROOT%\assets\icons" (
  if not exist "%OUT%\assets\icons" mkdir "%OUT%\assets\icons"
  xcopy /E /I /Y /Q "%ROOT%\assets\icons\*" "%OUT%\assets\icons\" >nul
)

echo ==^> Built:
dir /b "%OUT%\fuckfm.exe" "%OUT%\fuckwm.exe" "%OUT%\dwm.exe"

if /I "%ACTION%"=="build" (
  echo ==^> build-only done
  popd >nul
  exit /b 0
)

echo.
echo ==^> Registering shell replacement for THIS USER
echo     Winlogon Shell -^> "%OUT%\dwm.exe"
echo     Previous value stored as %BACKUP_VAL%
echo.

REM Backup current Shell if not already backed up by us
reg query "%WINLOGON%" /v %BACKUP_VAL% >nul 2>&1
if errorlevel 1 (
  for /f "tokens=2*" %%A in ('reg query "%WINLOGON%" /v Shell 2^>nul ^| findstr /I "Shell"') do (
    set "OLD_SHELL=%%B"
  )
  if defined OLD_SHELL (
    reg add "%WINLOGON%" /v %BACKUP_VAL% /t REG_SZ /d "!OLD_SHELL!" /f >nul
    echo     backed up: !OLD_SHELL!
  ) else (
    REM No Shell value usually means explorer.exe default
    reg add "%WINLOGON%" /v %BACKUP_VAL% /t REG_SZ /d "explorer.exe" /f >nul
    echo     backed up: explorer.exe ^(implicit default^)
  )
) else (
  echo     backup already present — leaving %BACKUP_VAL% unchanged
)

reg add "%WINLOGON%" /v Shell /t REG_SZ /d "%OUT%\dwm.exe" /f >nul
if errorlevel 1 (
  echo error: failed to set Shell registry value
  popd >nul
  exit /b 1
)

echo.
echo ==^> Shell set. Log off and back on ^(or reboot^) to start dwm.exe instead of Explorer.
echo     Restore later with:  scripts\restore-shell.cmd
echo                          ^(or scripts\install-shell.cmd /restore^)
echo     Status:              scripts\install-shell.cmd /status
popd >nul
exit /b 0

:restore
echo ==^> Restoring previous Winlogon Shell
reg query "%WINLOGON%" /v %BACKUP_VAL% >nul 2>&1
if errorlevel 1 (
  echo     no %BACKUP_VAL% found — setting Shell=explorer.exe
  reg add "%WINLOGON%" /v Shell /t REG_SZ /d "explorer.exe" /f >nul
) else (
  for /f "tokens=2*" %%A in ('reg query "%WINLOGON%" /v %BACKUP_VAL% ^| findstr /I "%BACKUP_VAL%"') do (
    set "PREV=%%B"
  )
  if not defined PREV set "PREV=explorer.exe"
  reg add "%WINLOGON%" /v Shell /t REG_SZ /d "!PREV!" /f >nul
  echo     restored Shell=!PREV!
  reg delete "%WINLOGON%" /v %BACKUP_VAL% /f >nul 2>&1
)
echo     Log off / reboot for Explorer ^(or previous shell^) to return.
popd >nul
exit /b 0

:status
echo ==^> Winlogon Shell status ^(HKCU^)
reg query "%WINLOGON%" /v Shell 2>nul
reg query "%WINLOGON%" /v %BACKUP_VAL% 2>nul
if exist "%OUT%\dwm.exe" (
  echo     dwm.exe: %OUT%\dwm.exe
) else (
  echo     dwm.exe: not built yet ^(%OUT%\dwm.exe^)
)
popd >nul
exit /b 0

:help
echo install-shell.cmd — build dwm.exe and set it as the user shell ^(Explorer replacement^)
echo.
echo   scripts\install-shell.cmd              build release + set Shell=dwm.exe
echo   scripts\install-shell.cmd /build-only  cargo build --release only
echo   scripts\install-shell.cmd /restore     restore previous Shell
echo   scripts\install-shell.cmd /status      show Shell registry values
echo.
echo Writes HKCU\...\Winlogon\Shell for the current user only.
echo Log off after install/restore. Use /restore if the desktop fails to start.
popd >nul
exit /b 0
