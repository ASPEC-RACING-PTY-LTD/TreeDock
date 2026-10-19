; SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
; TreeDock installer (Inno Setup 6). Built by tools/build_release.ps1, which passes:
;   /DAppVersion=0.1.0  /DSourceDir=<published app folder>  /DOutputDir=<dist>  /DArch=x64
; Per-user install by default (no admin); the setup dialog also offers an all-users install.

#ifndef AppVersion
  #error AppVersion is required (/DAppVersion=X.Y.Z)
#endif
#ifndef SourceDir
  #error SourceDir is required (/DSourceDir=...)
#endif
#ifndef OutputDir
  #define OutputDir "..\dist"
#endif
#ifndef Arch
  #define Arch "x64"
#endif

[Setup]
AppId={{81CD5C56-AD79-4CC6-B321-0AC3C4333436}
AppName=TreeDock
AppVersion={#AppVersion}
AppVerName=TreeDock {#AppVersion}
AppPublisher=ASPEC RACING PTY LTD
AppCopyright=Copyright (c) 2026 ASPEC RACING PTY LTD
AppPublisherURL=https://github.com/ASPEC-RACING-PTY-LTD/TreeDock
AppSupportURL=https://github.com/ASPEC-RACING-PTY-LTD/TreeDock/issues
AppUpdatesURL=https://github.com/ASPEC-RACING-PTY-LTD/TreeDock/releases
VersionInfoVersion={#AppVersion}
VersionInfoProductVersion={#AppVersion}
DefaultDirName={autopf}\TreeDock
DefaultGroupName=TreeDock
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog commandline
ArchitecturesAllowed={#Arch}compatible
ArchitecturesInstallIn64BitMode={#Arch}compatible
MinVersion=10.0.17763
LicenseFile=..\LICENSE.md
SetupIconFile=..\app\TreeDock.App\Assets\TreeDock.ico
UninstallDisplayIcon={app}\TreeDock.exe
UninstallDisplayName=TreeDock
OutputDir={#OutputDir}
OutputBaseFilename=TreeDock-{#AppVersion}-{#Arch}-setup
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#SourceDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\TreeDock"; Filename: "{app}\TreeDock.exe"
Name: "{autodesktop}\TreeDock"; Filename: "{app}\TreeDock.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\TreeDock.exe"; Description: "{cm:LaunchProgram,TreeDock}"; Flags: nowait postinstall skipifsilent
