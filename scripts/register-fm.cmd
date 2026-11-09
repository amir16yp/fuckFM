@echo off
REM Register fuckFM.exe as the default folder / drive handler for this user (HKCU).
REM
REM Usage:
REM   scripts\register-fm.cmd
REM   scripts\register-fm.cmd /unregister
REM   scripts\register-fm.cmd /status
REM
REM Builds release bins if fuckfm.exe is missing.

setlocal EnableExtensions EnableDelayedExpansion

set "ROOT=%~dp0.."
pushd "%ROOT%" >nul 2>&1
if errorlevel 1 (
  echo error: cannot cd to repo root
  exit /b 1
)

set "ACTION=register"
if /I "%~1"=="/unregister" set "ACTION=unregister"
if /I "%~1"=="/restore" set "ACTION=unregister"
if /I "%~1"=="/status" set "ACTION=status"
if /I "%~1"=="/help" goto :help

set "OUT=%ROOT%\target\release"
set "FM=%OUT%\fuckfm.exe"

if /I "%ACTION%"=="status" goto :status

if not exist "%FM%" (
  where cargo >nul 2>&1
  if errorlevel 1 (
    echo error: cargo not found and %FM% missing
    exit /b 1
  )
  echo ==^> Building release fuckfm.exe
  cargo build --release --bin fuckfm
  if errorlevel 1 exit /b 1
)

if not exist "%FM%" (
  echo error: missing %FM%
  exit /b 1
)

if /I "%ACTION%"=="unregister" goto :unregister

echo ==^> Registering folder handler
echo     %FM%
"%FM%" --register-folder-handler
if errorlevel 1 (
  echo error: registration failed
  popd >nul
  exit /b 1
)
echo     Done. Double-click a folder to open fuckFM.
echo     Undo with: scripts\register-fm.cmd /unregister
popd >nul
exit /b 0

:unregister
echo ==^> Unregistering folder handler
"%FM%" --unregister-folder-handler
if errorlevel 1 (
  echo error: unregister failed
  popd >nul
  exit /b 1
)
echo     Restored previous Directory/Drive open commands.
popd >nul
exit /b 0

:status
reg query "HKCU\Software\Classes\Directory\shell\open\command" 2>nul
reg query "HKCU\Software\Classes\Drive\shell\open\command" 2>nul
if exist "%FM%" (echo     fuckfm.exe: %FM%) else (echo     fuckfm.exe: not built)
popd >nul
exit /b 0

:help
echo register-fm.cmd — set fuckFM as default folder opener ^(HKCU^)
echo   scripts\register-fm.cmd
echo   scripts\register-fm.cmd /unregister
echo   scripts\register-fm.cmd /status
popd >nul
exit /b 0
