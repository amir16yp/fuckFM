; fuckFM Windows installer (Inno Setup 6)
; Built by CI from packaging/staging/. Optional tasks mirror scripts/register-fm.cmd
; and scripts/install-shell.cmd (HKCU only; no admin required).

#ifndef MyAppVersion
  #define MyAppVersion "0.1.0"
#endif

#ifndef StagingDir
  #define StagingDir "..\..\packaging\staging"
#endif

#define MyAppName "fuckFM"
#define MyAppPublisher "fuckFM"
#define MyAppExeName "fuckfm.exe"
#define MyWmExeName "fuckwm.exe"
#define WinlogonKey "Software\Microsoft\Windows NT\CurrentVersion\Winlogon"
#define ShellBackup "Shell.fuckfm.bak"
#define InstallFlagsKey "Software\fuckFM"

[Setup]
AppId={{A7F3C2E1-9B4D-4E8A-9F21-6C5D8E0A1B2C}}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppVerName={#MyAppName} {#MyAppVersion}
AppPublisher={#MyAppPublisher}
DefaultDirName={localappdata}\{#MyAppName}
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=..\..\dist
OutputBaseFilename=fuckFM-Setup-{#MyAppVersion}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
UninstallDisplayIcon={app}\{#MyAppExeName}
SetupLogging=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a &desktop shortcut"; GroupDescription: "Additional icons:"; Flags: unchecked
Name: "replaceshell"; Description: "Replace &shell — set fuckFM as the default Explorer folder opener"; GroupDescription: "Shell / window manager:"; Flags: unchecked
Name: "replacewm"; Description: "Replace &shell host — set fuckwm.exe as the Winlogon Shell (Explorer session replacement; not Microsoft DWM)"; GroupDescription: "Shell / window manager:"; Flags: unchecked

[Files]
Source: "{#StagingDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"
Name: "{group}\fuckWM"; Filename: "{app}\{#MyWmExeName}"
Name: "{group}\Uninstall {#MyAppName}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "Launch {#MyAppName}"; Flags: nowait postinstall skipifsilent unchecked

[Code]
function ShellBackupExists: Boolean;
begin
  Result := RegValueExists(HKCU, '{#WinlogonKey}', '{#ShellBackup}');
end;

procedure BackupAndSetShell(const NewShell: String);
var
  Prev: String;
begin
  if not ShellBackupExists then
  begin
    if RegQueryStringValue(HKCU, '{#WinlogonKey}', 'Shell', Prev) and (Prev <> '') then
      RegWriteStringValue(HKCU, '{#WinlogonKey}', '{#ShellBackup}', Prev)
    else
      RegWriteStringValue(HKCU, '{#WinlogonKey}', '{#ShellBackup}', 'explorer.exe');
  end;
  RegWriteStringValue(HKCU, '{#WinlogonKey}', 'Shell', NewShell);
end;

procedure RestoreShell;
var
  Prev: String;
begin
  if ShellBackupExists then
  begin
    if not RegQueryStringValue(HKCU, '{#WinlogonKey}', '{#ShellBackup}', Prev) or (Prev = '') then
      Prev := 'explorer.exe';
    RegWriteStringValue(HKCU, '{#WinlogonKey}', 'Shell', Prev);
    RegDeleteValue(HKCU, '{#WinlogonKey}', '{#ShellBackup}');
  end
  else if RegValueExists(HKCU, '{#WinlogonKey}', 'Shell') then
  begin
    { Best-effort: if Shell still points at our install, put Explorer back. }
    if RegQueryStringValue(HKCU, '{#WinlogonKey}', 'Shell', Prev) then
    begin
      if Pos(LowerCase(ExpandConstant('{app}\{#MyWmExeName}')), LowerCase(Prev)) > 0 then
        RegWriteStringValue(HKCU, '{#WinlogonKey}', 'Shell', 'explorer.exe');
    end;
  end;
end;

function RunFm(const Args: String): Boolean;
var
  ResultCode: Integer;
begin
  Result := Exec(
    ExpandConstant('{app}\{#MyAppExeName}'),
    Args,
    ExpandConstant('{app}'),
    SW_HIDE,
    ewWaitUntilTerminated,
    ResultCode
  ) and (ResultCode = 0);
end;

procedure MarkTask(const Name: String; Selected: Boolean);
begin
  if Selected then
    RegWriteStringValue(HKCU, '{#InstallFlagsKey}', Name, '1')
  else
    RegDeleteValue(HKCU, '{#InstallFlagsKey}', Name);
end;

function WasTaskMarked(const Name: String): Boolean;
var
  V: String;
begin
  Result := RegQueryStringValue(HKCU, '{#InstallFlagsKey}', Name, V) and (V = '1');
end;

procedure CurStepChanged(CurStep: TSetupStep);
var
  WmPath: String;
begin
  if CurStep <> ssPostInstall then
    Exit;

  MarkTask('ReplaceShell', WizardIsTaskSelected('replaceshell'));
  MarkTask('ReplaceWm', WizardIsTaskSelected('replacewm'));

  if WizardIsTaskSelected('replaceshell') then
  begin
    if not RunFm('--register-folder-handler') then
      MsgBox('Failed to register fuckFM as the default folder opener.', mbError, MB_OK);
  end;

  if WizardIsTaskSelected('replacewm') then
  begin
    WmPath := ExpandConstant('{app}\{#MyWmExeName}');
    BackupAndSetShell(WmPath);
    if not WizardSilent then
      MsgBox(
        'fuckwm.exe is now the Winlogon Shell for this user (Explorer replacement).' + #13#10 +
        'This does not replace Microsoft''s Desktop Window Manager compositor.' + #13#10 +
        'Log off and back on (or reboot) for the change to take effect.' + #13#10 +
        'If the desktop fails to start, run the uninstaller or restore Explorer from Safe Mode.',
        mbInformation,
        MB_OK
      );
  end;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep <> usUninstall then
    Exit;

  { Always attempt undo; helpers are idempotent when we never installed the option. }
  if WasTaskMarked('ReplaceShell') or FileExists(ExpandConstant('{app}\{#MyAppExeName}')) then
    RunFm('--unregister-folder-handler');

  if WasTaskMarked('ReplaceWm') or ShellBackupExists then
    RestoreShell;

  RegDeleteKeyIncludingSubkeys(HKCU, '{#InstallFlagsKey}');
end;
