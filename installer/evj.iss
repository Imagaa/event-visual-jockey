; EVJ installer (Inno Setup 6). Built by tools/build-installer.ps1, which stages the files first.
#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif

[Setup]
AppId={{6B7E2C8A-4F1D-4E0B-9C57-2D8E5A1F3B90}
AppName=EVJ
AppVersion={#AppVersion}
AppVerName=EVJ {#AppVersion}
AppPublisher=EVJ
DefaultDirName={autopf}\EVJ
DefaultGroupName=EVJ
DisableProgramGroupPage=yes
OutputDir=..\dist
OutputBaseFilename=EVJ-Setup-{#AppVersion}
SetupIconFile=..\assets\evj.ico
UninstallDisplayIcon={app}\evj.exe
Compression=lzma2/max
SolidCompression=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequiredOverridesAllowed=dialog
MinVersion=10.0
WizardStyle=modern
ChangesAssociations=yes
CloseApplications=yes

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"
Name: "assoc"; Description: "Open .vjproj show files with EVJ"

[Files]
Source: "..\target\installer\stage\*"; DestDir: "{app}"; Flags: recursesubdirs ignoreversion

[Dirs]
Name: "{app}\effects"

[Icons]
Name: "{autoprograms}\EVJ"; Filename: "{app}\evj.exe"
Name: "{autoprograms}\EVJ User Guide"; Filename: "{app}\docs\USER-GUIDE.md"
Name: "{autodesktop}\EVJ"; Filename: "{app}\evj.exe"; Tasks: desktopicon

[Registry]
Root: HKA; Subkey: "Software\Classes\.vjproj"; ValueType: string; ValueData: "EVJ.Show"; Flags: uninsdeletevalue; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\EVJ.Show"; ValueType: string; ValueData: "EVJ show"; Flags: uninsdeletekey; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\EVJ.Show\DefaultIcon"; ValueType: string; ValueData: "{app}\evj.exe,0"; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\EVJ.Show\shell\open\command"; ValueType: string; ValueData: """{app}\evj.exe"" ""%1"""; Tasks: assoc

[Run]
Filename: "{app}\evj.exe"; Description: "Start EVJ"; Flags: nowait postinstall skipifsilent
