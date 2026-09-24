; The Windows installer, built by Inno Setup 6 in the nightly:
;
;   ISCC /DVersion=0.1.4 bundle\windows\cydonia.iss
;
; Run from the repository root after `cargo build --release`. Per-user, into
; the directory the zip-based install.ps1 used to unpack to, so it upgrades
; those installs in place. install.ps1 now runs this installer silently.

#ifndef Version
  #define Version "0.0.0"
#endif

[Setup]
; Never change: Windows matches upgrades and the uninstall entry on it.
AppId={{F316EBB9-B364-4B91-AB89-EB50D3A433D3}
AppName=Cydonia
AppVersion={#Version}
AppPublisher=crabtalk
AppPublisherURL=https://cydonia.sh
DefaultDirName={localappdata}\Programs\cydonia
DisableDirPage=yes
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
UninstallDisplayIcon={app}\cydonia.exe
CloseApplications=yes
OutputDir=..\..\target\bundle
OutputBaseFilename=cydonia-windows-x86_64-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern

[Files]
Source: "..\..\target\release\cydonia.exe"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{userprograms}\Cydonia"; Filename: "{app}\cydonia.exe"; WorkingDir: "{app}"
Name: "{userdesktop}\Cydonia"; Filename: "{app}\cydonia.exe"; WorkingDir: "{app}"; Tasks: desktopicon

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; Flags: unchecked

[Run]
Filename: "{app}\cydonia.exe"; Description: "{cm:LaunchProgram,Cydonia}"; Flags: nowait postinstall skipifsilent
