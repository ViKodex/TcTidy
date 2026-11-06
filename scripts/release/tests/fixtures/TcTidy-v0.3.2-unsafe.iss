; Frozen uninstall-log compatibility fixture derived from the v0.3.2 installer.
; The AppId and destination are isolated for CI, but the recursive ownership
; entry matches the historical installer behavior relevant to upgrades.

#ifndef PayloadDir
  #define PayloadDir "payload"
#endif

[Setup]
AppId={{E36237D7-2F05-4E8E-99A5-AC006E028B04}
AppName=TcTidy v0.3.2 uninstall fixture
AppVersion=0.3.2
DefaultDirName={autopf}\TcTidy-v0.3.2-fixture
DisableDirPage=yes
DisableProgramGroupPage=yes
Uninstallable=yes
PrivilegesRequired=lowest
OutputBaseFilename=TcTidySetup-0.3.2-unsafe-fixture
WizardStyle=modern
SolidCompression=yes

[Files]
Source: "{#PayloadDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[UninstallDelete]
Type: filesandordirs; Name: "{app}"
