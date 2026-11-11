; Installer of UltraRushFTP (Inno Setup 6). Built by scripts/package.ps1:
;
;   ISCC.exe /DAppVersion=0.9.0 /DNumericVersion=0.9.0 /DSourceDir=<staged files> /DOutputDir=<dist> installer\UltraRushFTP.iss
;
; Installs for the current user by default (no administrator rights); the first page offers
; "all users" (Program Files) instead. Settings, sites and queues stay in
; %APPDATA%\UltraRushFTP: the uninstaller leaves them, like the portable zip does.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef NumericVersion
  #define NumericVersion AppVersion
#endif
#ifndef SourceDir
  #error SourceDir must point to the staged files (scripts/package.ps1)
#endif
#ifndef OutputDir
  #define OutputDir "..\dist"
#endif

[Setup]
AppId={{56023ECD-9C9F-43EB-819C-DE014FF7B871}
AppName=UltraRushFTP
AppVersion={#AppVersion}
AppVerName=UltraRushFTP {#AppVersion}
AppPublisher=werewolf004
AppPublisherURL=https://github.com/werewolf004/UltraRushFTP
AppSupportURL=https://github.com/werewolf004/UltraRushFTP/issues
VersionInfoVersion={#NumericVersion}
VersionInfoProductName=UltraRushFTP
VersionInfoDescription=UltraRushFTP setup
DefaultDirName={autopf}\UltraRushFTP
DefaultGroupName=UltraRushFTP
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
OutputDir={#OutputDir}
OutputBaseFilename=UltraRushFTP-{#AppVersion}-windows-x64-setup
SetupIconFile=..\crates\urftp-gui\assets\UltraRushFTP.ico
UninstallDisplayIcon={app}\UltraRushFTP.exe
UninstallDisplayName=UltraRushFTP {#AppVersion}
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
; A running UltraRushFTP is closed (and offered again at the end) through the Restart Manager.
CloseApplications=yes
RestartApplications=no
ShowLanguageDialog=auto

[Languages]
Name: "en"; MessagesFile: "compiler:Default.isl"; LicenseFile: "{#SourceDir}\LICENSE"
Name: "fr"; MessagesFile: "compiler:Languages\French.isl"; LicenseFile: "{#SourceDir}\LICENSE.fr.md"

[CustomMessages]
en.MircFiles=Link for mIRC (rushmirc.dll, 32 and 64-bit)
fr.MircFiles=Liaison avec mIRC (rushmirc.dll, 32 et 64 bits)

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#SourceDir}\UltraRushFTP.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\urftp.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\validate-server.ps1"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\mirc\*"; DestDir: "{app}\mirc"; Flags: ignoreversion recursesubdirs
Source: "{#SourceDir}\*.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\THIRD-PARTY-LICENSES.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\docs\*"; DestDir: "{app}\docs"; Flags: ignoreversion recursesubdirs

[Icons]
Name: "{autoprograms}\UltraRushFTP"; Filename: "{app}\UltraRushFTP.exe"
Name: "{autodesktop}\UltraRushFTP"; Filename: "{app}\UltraRushFTP.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\UltraRushFTP.exe"; Description: "{cm:LaunchProgram,UltraRushFTP}"; Flags: nowait postinstall skipifsilent
