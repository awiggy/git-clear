#ifndef AppName
#define AppName "Git Agent"
#endif
#ifndef AppVersion
#define AppVersion "dev"
#endif
#ifndef SourceDir
#define SourceDir "..\\..\\target\\release"
#endif
#ifndef OutputDir
#define OutputDir "..\\..\\dist"
#endif
// Clear-edition builds pass a distinct GUID, install directory, group, and output name so
// the two editions coexist and uninstall independently.
#ifndef AppGuid
#define AppGuid "7F2D2B68-AB4B-4B9A-9E91-F3574BA53C5D"
#endif
#ifndef InstallDirName
#define InstallDirName "GitAgent"
#endif
#ifndef OutputName
#define OutputName "GitAgentSetup-"
#endif

[Setup]
AppId={{{#AppGuid}}}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher=adoin
DefaultDirName={localappdata}\Programs\{#InstallDirName}
DisableDirPage=no
DisableProgramGroupPage=no
DefaultGroupName={#AppName}
PrivilegesRequired=lowest
OutputDir={#OutputDir}
OutputBaseFilename={#OutputName}{#AppVersion}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
UninstallDisplayIcon={app}\git-agent.exe
SetupIconFile=..\..\assets\icons\git-agent.ico

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Dirs]
Name: "{app}\data"

[Files]
Source: "{#SourceDir}\git-agent.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\git-agent-merge.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\git-agent-diff.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\theme.json"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\git-agent.exe"; WorkingDir: "{app}"; IconFilename: "{app}\git-agent.exe"
Name: "{group}\Uninstall {#AppName}"; Filename: "{uninstallexe}"

[Run]
Filename: "{app}\git-agent.exe"; Description: "Launch Git Agent"; Flags: nowait postinstall skipifsilent
