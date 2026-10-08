#ifndef AppVersion
  #define AppVersion "0.1.2"
#endif
#ifndef AppBinary
  #error AppBinary must point to the verified Release EXE.
#endif
#ifndef ReleaseOutput
  #error ReleaseOutput must point to the output directory.
#endif
[Setup]
AppId={{80A931DE-1B97-4FC8-B0BE-DC2A5AFB9417}
AppName=南枫 Codex 额度
AppVersion={#AppVersion}
AppPublisher=席瑞
AppPublisherURL=https://github.com/nanzhufeng/NanfengCodexQuota-Windows
AppSupportURL=https://github.com/nanzhufeng/NanfengCodexQuota-Windows/issues
AppUpdatesURL=https://github.com/nanzhufeng/NanfengCodexQuota-Windows/releases
DefaultDirName={localappdata}\Programs\NanfengCodexQuota
DefaultGroupName=南枫 Codex 额度
PrivilegesRequired=lowest
SetupArchitecture=x64
ArchitecturesAllowed=x64compatible and not arm64
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.17763
OutputDir={#ReleaseOutput}
OutputBaseFilename=Nanfeng-Codex-Quota-Windows-v{#AppVersion}-Setup
SetupIconFile=..\assets\app-icon.ico
UninstallDisplayIcon={app}\南枫Codex额度.exe
LicenseFile=..\LICENSE
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
DisableProgramGroupPage=yes
AppMutex=Local\NanfengCodexQuota.Standalone.v1
CloseApplications=no
RestartApplications=no
[Languages]
Name: "chinesesimp"; MessagesFile: "compiler:Languages\ChineseSimplified.isl"
[Tasks]
Name: "desktopicon"; Description: "创建桌面快捷方式"; GroupDescription: "快捷方式："
[Files]
Source: "{#AppBinary}"; DestDir: "{app}"; DestName: "南枫Codex额度.exe"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\NOTICE"; DestDir: "{app}"; Flags: ignoreversion
[Icons]
Name: "{autoprograms}\南枫 Codex 额度"; Filename: "{app}\南枫Codex额度.exe"; WorkingDir: "{app}"
Name: "{autodesktop}\南枫 Codex 额度"; Filename: "{app}\南枫Codex额度.exe"; WorkingDir: "{app}"; Tasks: desktopicon
[Run]
Filename: "{app}\南枫Codex额度.exe"; Description: "启动南枫 Codex 额度"; Flags: nowait postinstall skipifsilent
[Code]
const RunKey = 'Software\Microsoft\Windows\CurrentVersion\Run';
const RunName = 'NanfengCodexQuota';
procedure CurStepChanged(CurStep: TSetupStep);
var Existing: String;
begin
  if CurStep = ssPostInstall then begin
    { Preserve opt-in startup across install-directory changes. Do not opt new users in. }
    if RegQueryStringValue(HKCU, RunKey, RunName, Existing) then
      RegWriteStringValue(HKCU, RunKey, RunName, '"' + ExpandConstant('{app}\南枫Codex额度.exe') + '"');
  end;
end;
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var Existing, Expected: String;
begin
  if CurUninstallStep = usUninstall then begin
    Expected := '"' + ExpandConstant('{app}\南枫Codex额度.exe') + '"';
    if RegQueryStringValue(HKCU, RunKey, RunName, Existing) and (CompareText(Existing, Expected) = 0) then
      RegDeleteValue(HKCU, RunKey, RunName);
  end;
  { Preserve user settings and never touch Codex authentication data. }
end;
