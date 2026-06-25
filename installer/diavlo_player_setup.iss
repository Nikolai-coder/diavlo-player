; --- DIAVLO PLAYER Inno Setup Installer ---------------------------------
; Build: download Inno Setup 6 from https://jrsoftware.org/isinfo.php
; Then run: ISCC.exe diavlo_player_setup.iss

#define MyAppName "DIAVLO PLAYER"
#define MyAppVersion "0.1.0"
#define MyAppPublisher "DIAVLO"
#define MyAppURL "https://github.com/Nikolai-coder/diavlo-player"
#define MyAppExeName "diavlo-player.exe"

[Setup]
AppId={{B4F2A8E1-7D3C-4F1A-9E6B-2C5D8A3F1E07}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
AppUpdatesURL={#MyAppURL}
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
AllowNoIcons=yes
LicenseFile=..\LICENSE
OutputDir=.
OutputBaseFilename=DIAVLO_PLAYER_Setup_v{#MyAppVersion}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequiredOverridesAllowed=dialog
; Dark style hint
WizardSmallImageFile=
WizardImageFile=

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "..\target\release\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion
; Any bundled DLLs from the Rust build
Source: "..\target\release\*.dll"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"
Name: "{group}\{cm:UninstallProgram,{#MyAppName}}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#StringChange(MyAppName, '&', '&&')}}"; Flags: nowait postinstall skipifsilent

; ── Registry: file associations (HKCU — no admin required) ──

#define Extensions "'wav' 'mp3' 'flac' 'ogg' 'aiff' 'aac' 'opus' 'm4a'"
#define ProgID "diavlo-player.Audio"

[Registry]
; ProgID: friendly name
Root: HKCU; Subkey: "Software\Classes\{#ProgID}"; ValueType: string; ValueData: "{#MyAppName} Audio File"; Flags: uninsdeletekey
; Default icon
Root: HKCU; Subkey: "Software\Classes\{#ProgID}\DefaultIcon"; ValueType: string; ValueData: "{app}\{#MyAppExeName},0"; Flags: uninsdeletekey
; Open command
Root: HKCU; Subkey: "Software\Classes\{#ProgID}\shell\open\command"; ValueType: string; ValueData: """{app}\{#MyAppExeName}"" ""%1"""; Flags: uninsdeletekey

; Extensions (OpenWithProgids — polite, doesn't hijack existing defaults)
Root: HKCU; Subkey: "Software\Classes\.wav\OpenWithProgids"; ValueType: string; ValueName: "{#ProgID}"; ValueData: ""; Flags: uninsdeletevalue
Root: HKCU; Subkey: "Software\Classes\.mp3\OpenWithProgids"; ValueType: string; ValueName: "{#ProgID}"; ValueData: ""; Flags: uninsdeletevalue
Root: HKCU; Subkey: "Software\Classes\.flac\OpenWithProgids"; ValueType: string; ValueName: "{#ProgID}"; ValueData: ""; Flags: uninsdeletevalue
Root: HKCU; Subkey: "Software\Classes\.ogg\OpenWithProgids"; ValueType: string; ValueName: "{#ProgID}"; ValueData: ""; Flags: uninsdeletevalue
Root: HKCU; Subkey: "Software\Classes\.aiff\OpenWithProgids"; ValueType: string; ValueName: "{#ProgID}"; ValueData: ""; Flags: uninsdeletevalue
Root: HKCU; Subkey: "Software\Classes\.aac\OpenWithProgids"; ValueType: string; ValueName: "{#ProgID}"; ValueData: ""; Flags: uninsdeletevalue
Root: HKCU; Subkey: "Software\Classes\.opus\OpenWithProgids"; ValueType: string; ValueName: "{#ProgID}"; ValueData: ""; Flags: uninsdeletevalue
Root: HKCU; Subkey: "Software\Classes\.m4a\OpenWithProgids"; ValueType: string; ValueName: "{#ProgID}"; ValueData: ""; Flags: uninsdeletevalue

; RegisteredApplications
Root: HKCU; Subkey: "Software\RegisteredApplications"; ValueType: string; ValueName: "diavlo-player"; ValueData: "Software\Classes\{#ProgID}\Capabilities"; Flags: uninsdeletevalue
; Capabilities
Root: HKCU; Subkey: "Software\Classes\{#ProgID}\Capabilities"; ValueType: string; ValueName: "ApplicationName"; ValueData: "{#MyAppName}"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\{#ProgID}\Capabilities"; ValueType: string; ValueName: "ApplicationDescription"; ValueData: "A modern glass-style music player (WAV, FLAC, MP3, AAC, OGG, AIFF)"; Flags: uninsdeletekey
; FileAssociations
Root: HKCU; Subkey: "Software\Classes\{#ProgID}\Capabilities\FileAssociations"; ValueType: string; ValueName: ".wav"; ValueData: "{#ProgID}"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\{#ProgID}\Capabilities\FileAssociations"; ValueType: string; ValueName: ".mp3"; ValueData: "{#ProgID}"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\{#ProgID}\Capabilities\FileAssociations"; ValueType: string; ValueName: ".flac"; ValueData: "{#ProgID}"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\{#ProgID}\Capabilities\FileAssociations"; ValueType: string; ValueName: ".ogg"; ValueData: "{#ProgID}"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\{#ProgID}\Capabilities\FileAssociations"; ValueType: string; ValueName: ".aiff"; ValueData: "{#ProgID}"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\{#ProgID}\Capabilities\FileAssociations"; ValueType: string; ValueName: ".aac"; ValueData: "{#ProgID}"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\{#ProgID}\Capabilities\FileAssociations"; ValueType: string; ValueName: ".opus"; ValueData: "{#ProgID}"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\{#ProgID}\Capabilities\FileAssociations"; ValueType: string; ValueName: ".m4a"; ValueData: "{#ProgID}"; Flags: uninsdeletekey
