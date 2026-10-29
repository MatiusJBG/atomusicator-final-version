#ifndef AppVersion
  #define AppVersion "0.7.0"
#endif
#ifndef ReleaseDir
  #define ReleaseDir "..\build\windows\x64\runner\Release"
#endif
#ifndef OutputDir
  #define OutputDir ".\dist"
#endif

#define AppName "Atomusicator"
#define AppExeName "atomusicator.exe"
#ifndef InstallerAppId
  #define InstallerAppId "{{4D328E01-9A7D-49E9-B5D8-9EAF45BE7078}"
#endif
#ifndef InstallerCompression
  #define InstallerCompression "lzma2"
#endif

[Setup]
AppId={#InstallerAppId}
AppName={#AppName}
AppVersion={#AppVersion}
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
UninstallDisplayIcon={app}\{#AppExeName}
OutputDir={#OutputDir}
OutputBaseFilename=Atomusicator-Setup-{#AppVersion}
ArchitecturesAllowed=x64os
ArchitecturesInstallIn64BitMode=x64os
PrivilegesRequired=admin
PrivilegesRequiredOverridesAllowed=commandline
Compression={#InstallerCompression}
SolidCompression=yes
WizardStyle=modern

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Additional shortcuts:"; Flags: unchecked

[Files]
Source: "{#ReleaseDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\{#AppExeName}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExeName}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#AppExeName}"; Description: "Launch {#AppName}"; Flags: nowait postinstall skipifsilent
