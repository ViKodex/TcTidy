; TcTidy for TwinCAT: installer for discovered TwinCAT XAE Shell instances.
;
; The bundled tctidy.exe resolves Beckhoff's COM registrations for TcXaeShell
; 15.0 (x86 registry view) and 17.0 (x64 registry view). Setup installs into the
; selected shell's all-users Extensions directory and forces a menu re-merge. It
; preselects the shell that holds an older TcTidy, because a new version does
; nothing in a shell it did not reach.
;
; Built by CI for each release (see README.md). Version and shell-specific payload
; directories are passed in by the build:
;   ISCC /DAppVersion=1.2.3-rc.4 /DNumericVersion=1.2.3.0 /DPayload15Dir=..\xae15 /DPayload17Dir=..\xae17 TcTidy.iss

#ifndef AppVersion
  #define AppVersion "0.0.0-dev"
#endif
#ifndef NumericVersion
  #define NumericVersion "0.0.0.0"
#endif
#ifndef Payload15Dir
  #ifdef PayloadDir
    #define Payload15Dir PayloadDir
  #else
    #define Payload15Dir "payload-xae15"
  #endif
#endif
#ifndef Payload17Dir
  #ifdef PayloadDir
    #define Payload17Dir PayloadDir
  #else
    #define Payload17Dir "payload-xae17"
  #endif
#endif
#ifndef OutputSuffix
  #define OutputSuffix ""
#endif

#ifdef PayloadFixture1517
  ; Identities of their own, so this fixture has one uninstall record per shell.
  #define AppId15 "{1B6C9E1C-7C2F-4B7B-9E0A-5D5B7C3A1F15}"
  #define AppId17 "{2C7DAF2D-8D30-4C8C-AF1B-6E6C8D4B2F17}"
#else
  #define AppId15 "{6F2B4A7E-1C3D-4E5F-8A9B-0C1D2E3F4A5B}"
  #define AppId17 "{63FCDC0D-8D5F-4E75-9A60-8DA4D4C34CEC}"
#endif
#define PayloadFixtureAppId "{E36237D7-2F05-4E8E-99A5-AC006E028B04}"
; Not a version a release can have, so it never equals this setup's version.
#define UnknownInstalledVersion "unknown"

[Setup]
; A generation-specific AppId keeps 15.0 and 17.0 installs independently
; upgradeable and uninstallable. The 15.0 ID is the one installers up to 0.3.2
; used, so those installations upgrade in place.
AppId={code:GetSelectedAppId}
UsePreviousLanguage=no
AppName=TcTidy for TwinCAT
AppVersion={#AppVersion}
; PE binary versions are numeric; keep the full SemVer (including a preview suffix) in text.
VersionInfoVersion={#NumericVersion}
VersionInfoProductVersion={#NumericVersion}
VersionInfoProductTextVersion={#AppVersion}
AppPublisher=ViKodex
AppPublisherURL=https://github.com/ViKodex/TcTidy
AppSupportURL=https://github.com/ViKodex/TcTidy/issues
AppUpdatesURL=https://github.com/ViKodex/TcTidy/releases
AppCopyright=Copyright 2026 ViKodex
UninstallDisplayName=TcTidy for TwinCAT XAE Shell {code:GetSelectedShellLabel}
DefaultDirName={code:GetSelectedExtensionDir}
DisableDirPage=yes
; The target comes from discovery, never from the path of an earlier install.
UsePreviousAppDir=no
DisableProgramGroupPage=yes
Uninstallable=yes
; Installers up to 0.3.2 appended a recursive {app} deletion to this
; application's uninstall log. Overwrite that log so an upgrade from one of
; them drops the destructive entry; current [Files] entries rebuild exact
; ownership after [InstallDelete] removes exact obsolete package files
; (TcTidy has no sharedfile entries).
UninstallLogMode=overwrite
; All-users shell extension directories and merge markers require elevation.
#ifdef PayloadFixture
PrivilegesRequired=lowest
#else
PrivilegesRequired=admin
#endif
OutputDir=dist
OutputBaseFilename=TcTidySetup-{#AppVersion}{#OutputSuffix}
WizardStyle=modern
SolidCompression=yes
CloseApplications=yes
RestartApplications=no

[Files]
; Keep the discovery helper first for fast ExtractTemporaryFile with solid compression.
Source: "{#Payload17Dir}\tctidy.exe"; DestName: "tctidy-discovery.exe"; Flags: dontcopy noencryption
Source: "{#Payload15Dir}\tctidy.exe"; DestDir: "{app}"; Flags: ignoreversion noencryption; Check: IsSelectedVersion15
Source: "{#Payload15Dir}\*"; Excludes: "tctidy.exe"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs; Check: IsSelectedVersion15
Source: "{#Payload17Dir}\tctidy.exe"; DestDir: "{app}"; Flags: ignoreversion noencryption; Check: IsSelectedVersion17
Source: "{#Payload17Dir}\*"; Excludes: "tctidy.exe"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs; Check: IsSelectedVersion17

[InstallDelete]
; The 0.3.2 installer also installed this Visual Studio host assembly. Remove
; only that obsolete file before rebuilding current payload ownership.
Type: files; Name: "{app}\Microsoft.VisualStudio.DpiAwareness.dll"

#ifndef PayloadFixture
[Registry]
; The AppId-specific, administrator-owned value lets uninstall recover only
; the selected generation. Foreign values in the private key are preserved.
Root: HKLM; Subkey: "Software\ViKodex\TcTidy\XaeShell\{{#AppId15}"; ValueType: string; ValueName: "InstallDir"; ValueData: "{app}"; Flags: uninsdeletevalue uninsdeletekeyifempty; Check: IsSelectedVersion15
Root: HKLM; Subkey: "Software\ViKodex\TcTidy\XaeShell\{{#AppId17}"; ValueType: string; ValueName: "InstallDir"; ValueData: "{app}"; Flags: uninsdeletevalue uninsdeletekeyifempty; Check: IsSelectedVersion17
#endif

[Code]
const
  CopyNone = 0;
  CopyOlder = 1;
  CopyCurrent = 2;
  CopyNewer = 3;

var
  ShellVersions: TArrayOfString;
  ShellArchitectures: TArrayOfString;
  ShellExecutables: TArrayOfString;
  ShellExtensionDirs: TArrayOfString;
  ShellMergeMarkers: TArrayOfString;
  ShellInstalledVersions: TArrayOfString;
  ShellSelectionPage: TInputOptionWizardPage;
  HasShellSelectionPage: Boolean;
  SelectedShellIndex: Integer;
  UninstallMergeMarker: String;
  UninstallShellVersion: String;

function AppIdOfGeneration(const Version: String): String;
begin
  if Version = '15.0' then
    Result := '{#AppId15}'
  else
    Result := '{#AppId17}';
end;

{ What a shell holds: '' without TcTidy, otherwise the version its last setup
  recorded. The package DLL decides, because it is what the shell loads; a copy
  that no setup recorded has an unknown version. }
function ReadInstalledVersion(const Version, ExtensionDir: String): String;
begin
  Result := '';
  if not FileExists(AddBackslash(ExtensionDir) + 'TcTidy.VsExtension.dll') then
    Exit;
  if not RegQueryStringValue(HKA,
       'Software\Microsoft\Windows\CurrentVersion\Uninstall\' +
       AppIdOfGeneration(Version) + '_is1', 'DisplayVersion', Result) or
     (Trim(Result) = '') then
    Result := '{#UnknownInstalledVersion}';
end;

procedure AddShellInstance(const Version, Architecture, Executable,
  ExtensionDir, MergeMarker: String);
var
  Index: Integer;
begin
  Index := GetArrayLength(ShellVersions);
  SetArrayLength(ShellVersions, Index + 1);
  SetArrayLength(ShellArchitectures, Index + 1);
  SetArrayLength(ShellExecutables, Index + 1);
  SetArrayLength(ShellExtensionDirs, Index + 1);
  SetArrayLength(ShellMergeMarkers, Index + 1);
  SetArrayLength(ShellInstalledVersions, Index + 1);
  ShellVersions[Index] := Version;
  ShellArchitectures[Index] := Architecture;
  ShellExecutables[Index] := Executable;
  ShellExtensionDirs[Index] := ExtensionDir;
  ShellMergeMarkers[Index] := MergeMarker;
  ShellInstalledVersions[Index] := ReadInstalledVersion(Version, ExtensionDir);
end;

function TakeField(var Remainder: String; var Field: String): Boolean;
var
  Separator: Integer;
begin
  Separator := Pos('|', Remainder);
  Result := Separator > 0;
  if not Result then
    Exit;

  Field := Trim(Copy(Remainder, 1, Separator - 1));
  Delete(Remainder, 1, Separator);
  Result := Field <> '';
end;

function IsAbsoluteDrivePath(const Value: String): Boolean;
begin
  Result := (Length(Value) >= 3) and
    (((Value[1] >= 'A') and (Value[1] <= 'Z')) or
     ((Value[1] >= 'a') and (Value[1] <= 'z'))) and
    (Value[2] = ':') and (Value[3] = '\');
end;

function ExpectedCacheRelative(const Version: String): String;
begin
  if Version = '15.0' then
    Result := 'Beckhoff\TcXaeShell\15.0_IsoShell\ComponentModelCache'
  else if Version = '17.0' then
    Result := 'Beckhoff\TcXaeShell\17.0_IsoShell\ComponentModelCache'
  else
    Result := '';
end;

function TryBuildComponentCachePath(const ProfilePath, Version: String;
  var CachePath: String): Boolean;
var
  NormalizedProfile, CacheRelative: String;
begin
  Result := False;
  CachePath := '';
  CacheRelative := ExpectedCacheRelative(Version);
  if CacheRelative = '' then
    Exit;
  if not IsAbsoluteDrivePath(ProfilePath) then
    Exit;
  try
    NormalizedProfile :=
      RemoveBackslashUnlessRoot(ExpandFileName(ProfilePath));
  except
    Exit;
  end;
  if not IsAbsoluteDrivePath(NormalizedProfile) then
    Exit;

  CachePath := AddBackslash(NormalizedProfile) +
    'AppData\Local\' + CacheRelative;
  Result := True;
end;

function TryDeriveMergeMarker(const ExtensionDir: String;
  var MergeMarker: String): Boolean;
var
  Normalized, ExtensionsDir, IdeDir, Common7Dir: String;
begin
  Result := False;
  MergeMarker := '';
  try
    Normalized := RemoveBackslashUnlessRoot(ExpandFileName(ExtensionDir));
  except
    Exit;
  end;
  if not IsAbsoluteDrivePath(Normalized) or
     (CompareText(ExtractFileName(Normalized), 'TcTidy') <> 0) then
    Exit;

  ExtensionsDir := ExtractFileDir(Normalized);
  if CompareText(ExtractFileName(ExtensionsDir), 'Extensions') <> 0 then
    Exit;
  IdeDir := ExtractFileDir(ExtensionsDir);
  if CompareText(ExtractFileName(IdeDir), 'IDE') <> 0 then
    Exit;
  Common7Dir := ExtractFileDir(IdeDir);
  if CompareText(ExtractFileName(Common7Dir), 'Common7') <> 0 then
    Exit;

  MergeMarker := AddBackslash(IdeDir) + 'extension.configurationchanged';
  Result := True;
end;

function TryAddShellRecord(const RecordLine: String): Boolean;
var
  Remainder, Version, Architecture, Executable, ExtensionDir,
    MergeMarker, CacheRelative, ExpectedMergeMarker: String;
begin
  Result := False;
  Remainder := Trim(RecordLine);
  if not TakeField(Remainder, Version) or
     not TakeField(Remainder, Architecture) or
     not TakeField(Remainder, Executable) or
     not TakeField(Remainder, ExtensionDir) or
     not TakeField(Remainder, MergeMarker) then
    Exit;

  CacheRelative := Trim(Remainder);
  if (CacheRelative = '') or (Pos('|', CacheRelative) > 0) then
    Exit;
  if not (((Version = '15.0') and (Architecture = 'x86')) or
          ((Version = '17.0') and (Architecture = 'x64'))) then
    Exit;
  if not IsAbsoluteDrivePath(Executable) or
     not IsAbsoluteDrivePath(ExtensionDir) or
     not IsAbsoluteDrivePath(MergeMarker) or
     not FileExists(Executable) then
    Exit;
  if (CacheRelative <> ExpectedCacheRelative(Version)) or
     not TryDeriveMergeMarker(ExtensionDir, ExpectedMergeMarker) or
     (CompareText(MergeMarker, ExpectedMergeMarker) <> 0) then
    Exit;

  AddShellInstance(Version, Architecture, Executable, ExtensionDir, MergeMarker);
  Result := True;
end;

function DiscoverShellInstances(var ErrorText: String): Boolean;
var
  Started: Boolean;
  ResultCode, Index: Integer;
  Output: TExecOutput;
  Helper: String;
begin
  Result := False;
  ErrorText := '';
  Helper := ExpandConstant('{tmp}\tctidy-discovery.exe');

  try
    ExtractTemporaryFile('tctidy-discovery.exe');
    Started := ExecAndCaptureOutput(Helper, '--discover-xae-shells', '',
      SW_SHOWNORMAL, ewWaitUntilTerminated, ResultCode, Output);
  except
    ErrorText := GetExceptionMessage;
    Log('XAE Shell discovery exception: ' + ErrorText);
    Exit;
  end;

  if not Started then
  begin
    ErrorText := 'The bundled discovery helper could not be started.';
    Exit;
  end;

  for Index := 0 to GetArrayLength(Output.StdErr) - 1 do
    Log('XAE Shell discovery: ' + Output.StdErr[Index]);

  if Output.Error then
  begin
    ErrorText := 'The discovery helper output could not be read completely.';
    Exit;
  end;
  if ResultCode <> 0 then
  begin
    ErrorText := Format('The discovery helper exited with code %d.', [ResultCode]);
    Exit;
  end;

  for Index := 0 to GetArrayLength(Output.StdOut) - 1 do
  begin
    if (Trim(Output.StdOut[Index]) <> '') and
       not TryAddShellRecord(Output.StdOut[Index]) then
    begin
      ErrorText := 'The discovery helper returned an invalid shell record.';
      Log(ErrorText + ' Record: ' + Output.StdOut[Index]);
      Exit;
    end;
  end;

  Result := True;
end;

function LoadShellInstances(var ErrorText: String): Boolean;
#ifdef PayloadFixture
var
  RootDir, IdeDir: String;
#endif
begin
#ifdef PayloadFixture
  RootDir := RemoveBackslashUnlessRoot(
    ExpandConstant('{param:PayloadFixtureRoot|}'));
  if not IsAbsoluteDrivePath(RootDir) then
  begin
    ErrorText := 'The payload fixture root must be an absolute drive path.';
    Result := False;
    Exit;
  end;
#ifdef PayloadFixture1517
  IdeDir := AddBackslash(RootDir) + 'xae17\Common7\IDE';
  AddShellInstance('17.0', 'x64',
    AddBackslash(IdeDir) + 'TcXaeShell.exe',
    AddBackslash(IdeDir) + 'Extensions\TcTidy',
    AddBackslash(IdeDir) + 'extension.configurationchanged');
  IdeDir := AddBackslash(RootDir) + 'xae15\Common7\IDE';
  AddShellInstance('15.0', 'x86',
    AddBackslash(IdeDir) + 'TcXaeShell.exe',
    AddBackslash(IdeDir) + 'Extensions\TcTidy',
    AddBackslash(IdeDir) + 'extension.configurationchanged');
#else
  IdeDir := AddBackslash(RootDir) + 'Common7\IDE';
#ifdef PayloadFixture17
  AddShellInstance('17.0', 'x64',
    AddBackslash(IdeDir) + 'TcXaeShell.exe',
    AddBackslash(IdeDir) + 'Extensions\TcTidy',
    AddBackslash(IdeDir) + 'extension.configurationchanged');
#else
  AddShellInstance('15.0', 'x86',
    AddBackslash(IdeDir) + 'TcXaeShell.exe',
    AddBackslash(IdeDir) + 'Extensions\TcTidy',
    AddBackslash(IdeDir) + 'extension.configurationchanged');
#endif
#endif
  ErrorText := '';
  Result := True;
#else
#ifdef DiscoveryFixture15
  { Compile-only CI fixtures exercise every target shape without a Beckhoff install. }
  AddShellInstance('15.0', 'x86',
    'C:\Fixture XAE 15\Common7\IDE\TcXaeShell.exe',
    'C:\Fixture XAE 15\Common7\IDE\Extensions\TcTidy',
    'C:\Fixture XAE 15\Common7\IDE\extension.configurationchanged');
  ErrorText := '';
  Result := True;
#else
  #ifdef DiscoveryFixture17
  { Compile-only CI fixtures exercise every target shape without a Beckhoff install. }
  AddShellInstance('17.0', 'x64',
    'D:\Fixture XAE 17\Common7\IDE\TcXaeShell.exe',
    'D:\Fixture XAE 17\Common7\IDE\Extensions\TcTidy',
    'D:\Fixture XAE 17\Common7\IDE\extension.configurationchanged');
  ErrorText := '';
  Result := True;
  #else
    #ifdef DiscoveryFixture1517
  { Compile-only CI fixtures exercise every target shape without a Beckhoff install. }
  AddShellInstance('17.0', 'x64',
    'D:\Fixture XAE 17\Common7\IDE\TcXaeShell.exe',
    'D:\Fixture XAE 17\Common7\IDE\Extensions\TcTidy',
    'D:\Fixture XAE 17\Common7\IDE\extension.configurationchanged');
  AddShellInstance('15.0', 'x86',
    'C:\Fixture XAE 15\Common7\IDE\TcXaeShell.exe',
    'C:\Fixture XAE 15\Common7\IDE\Extensions\TcTidy',
    'C:\Fixture XAE 15\Common7\IDE\extension.configurationchanged');
  ErrorText := '';
  Result := True;
    #else
  Result := DiscoverShellInstances(ErrorText);
    #endif
  #endif
#endif
#endif
end;

function SelectedShellIsValid(): Boolean;
begin
  Result := (SelectedShellIndex >= 0) and
    (SelectedShellIndex < GetArrayLength(ShellVersions));
end;

function GetSelectedAppId(Param: String): String;
begin
#if Defined(PayloadFixture) && !Defined(PayloadFixture1517)
  Result := '{#PayloadFixtureAppId}';
  Exit;
#endif
  Result := '';
  if not SelectedShellIsValid() then
    Exit;

  Result := AppIdOfGeneration(ShellVersions[SelectedShellIndex]);
end;

function GetSelectedExtensionDir(Param: String): String;
begin
  if SelectedShellIsValid() then
    Result := ShellExtensionDirs[SelectedShellIndex]
  else
    Result := ExpandConstant('{tmp}\TcTidy');
end;

function ShellLabel(Index: Integer): String;
begin
  Result := ShellVersions[Index] + ' (' + ShellArchitectures[Index] + ')';
end;

function GetSelectedShellLabel(Param: String): String;
begin
  if SelectedShellIsValid() then
    Result := ShellLabel(SelectedShellIndex)
  else
    Result := '';
end;

{ Reads the next number of a dotted version and drops it from Remainder. }
function TakeVersionNumber(var Remainder: String): Integer;
var
  Dot: Integer;
begin
  Dot := Pos('.', Remainder);
  if Dot = 0 then
  begin
    Result := StrToIntDef(Remainder, 0);
    Remainder := '';
  end
  else
  begin
    Result := StrToIntDef(Copy(Remainder, 1, Dot - 1), 0);
    Delete(Remainder, 1, Dot);
  end;
end;

function WithoutVersionSuffix(const Version: String): String;
var
  Suffix: Integer;
begin
  Result := Version;
  Suffix := Pos('+', Result);
  if Suffix > 0 then
    Result := Copy(Result, 1, Suffix - 1);
  Suffix := Pos('-', Result);
  if Suffix > 0 then
    Result := Copy(Result, 1, Suffix - 1);
end;

{ Compares the X.Y.Z of two versions: below, equal to or above zero. }
function CompareVersionNumbers(const Left, Right: String): Integer;
var
  LeftRest, RightRest: String;
  Part: Integer;
begin
  LeftRest := WithoutVersionSuffix(Left);
  RightRest := WithoutVersionSuffix(Right);
  Result := 0;
  for Part := 1 to 3 do
  begin
    Result := TakeVersionNumber(LeftRest) - TakeVersionNumber(RightRest);
    if Result <> 0 then
      Exit;
  end;
end;

{ How a shell's copy relates to this setup. The same number under another label
  is older, so that a test build replaces its release; an unrecorded copy is not
  assumed current. }
function ClassifyCopy(const InstalledVersion, SetupVersion: String): Integer;
begin
  if InstalledVersion = '' then
    Result := CopyNone
  else if InstalledVersion = SetupVersion then
    Result := CopyCurrent
  else if (InstalledVersion <> '{#UnknownInstalledVersion}') and
          (CompareVersionNumbers(InstalledVersion, SetupVersion) > 0) then
    Result := CopyNewer
  else
    Result := CopyOlder;
end;

function FirstShellWithCopy(const InstalledVersions: TArrayOfString;
  const SetupVersion: String; Kind: Integer): Integer;
var
  Index: Integer;
begin
  Result := -1;
  for Index := 0 to GetArrayLength(InstalledVersions) - 1 do
    if ClassifyCopy(InstalledVersions[Index], SetupVersion) = Kind then
    begin
      Result := Index;
      Exit;
    end;
end;

{ The shell to preselect, given what each one holds, newest generation first. An
  older copy comes first, since upgrading is what a returning user came for. Then
  the wizard offers a shell without TcTidy, which is how to reach the second one
  and which the user can see and change. An unattended run stays where this
  version already is instead, because nobody chose to spread TcTidy to another
  shell. A newer copy is only chosen when there is nothing else. }
function PreferredShellIndex(const InstalledVersions: TArrayOfString;
  const SetupVersion: String; Unattended: Boolean): Integer;
var
  Second, Third: Integer;
begin
  Result := FirstShellWithCopy(InstalledVersions, SetupVersion, CopyOlder);
  if Result >= 0 then
    Exit;
  if Unattended then
  begin
    Second := CopyCurrent;
    Third := CopyNone;
  end
  else
  begin
    Second := CopyNone;
    Third := CopyCurrent;
  end;
  Result := FirstShellWithCopy(InstalledVersions, SetupVersion, Second);
  if Result >= 0 then
    Exit;
  Result := FirstShellWithCopy(InstalledVersions, SetupVersion, Third);
  if Result < 0 then
    Result := 0;
end;

function IndexOfShell(const Version: String): Integer;
var
  Index: Integer;
begin
  Result := -1;
  for Index := 0 to GetArrayLength(ShellVersions) - 1 do
    if ShellVersions[Index] = Version then
    begin
      Result := Index;
      Exit;
    end;
end;

function DescribeInstalled(const InstalledVersion: String): String;
begin
  if InstalledVersion = '' then
    Result := 'TcTidy not installed'
  else if InstalledVersion = '{#UnknownInstalledVersion}' then
    Result := 'TcTidy installed, version unknown'
  else
    Result := 'TcTidy ' + InstalledVersion + ' installed';
end;

function DescribeChange(const InstalledVersion, SetupVersion: String): String;
begin
  case ClassifyCopy(InstalledVersion, SetupVersion) of
    CopyNone:
      Result := 'Installs ' + SetupVersion;
    CopyCurrent:
      Result := 'Reinstalls ' + SetupVersion;
    CopyNewer:
      Result := 'Downgrades ' + InstalledVersion + ' to ' + SetupVersion;
  else
    if InstalledVersion = '{#UnknownInstalledVersion}' then
      Result := 'Replaces the installed version with ' + SetupVersion
    else
      Result := 'Replaces ' + InstalledVersion + ' with ' + SetupVersion;
  end;
end;

{ What the last page says: where the new version went, and which other shell
  still has an older TcTidy, since setup updates one shell per run. }
function FinishedText(): String;
var
  Index: Integer;
begin
  Result := 'TcTidy {#AppVersion} is installed in TwinCAT XAE Shell ' +
    ShellLabel(SelectedShellIndex) + '.' + #13#10#13#10 +
    'If this shell is open, restart it to load this version.';
  for Index := 0 to GetArrayLength(ShellVersions) - 1 do
    if (Index <> SelectedShellIndex) and
       (ClassifyCopy(ShellInstalledVersions[Index], '{#AppVersion}') = CopyOlder) then
      Result := Result + #13#10#13#10 + 'TwinCAT XAE Shell ' +
        ShellLabel(Index) + ' was not updated (' +
        DescribeInstalled(ShellInstalledVersions[Index]) +
        '). To update it, run this setup again and select that shell.';
end;

function IsSelectedVersion15(): Boolean;
begin
  Result := SelectedShellIsValid() and
    (ShellVersions[SelectedShellIndex] = '15.0');
end;

function IsSelectedVersion17(): Boolean;
begin
  Result := SelectedShellIsValid() and
    (ShellVersions[SelectedShellIndex] = '17.0');
end;

function WqlEscape(const Value: String): String;
begin
  Result := Value;
  StringChangeEx(Result, #92, #92#92, True);
  StringChangeEx(Result, #39, #92#39, True);
end;

function IsSidString(const Value: String): Boolean;
var
  Index: Integer;
begin
  Result := (Length(Value) >= 3) and
    (Value[1] = 'S') and (Value[2] = '-');
  if not Result then
    Exit;
  for Index := 3 to Length(Value) do
    if not (((Value[Index] >= '0') and (Value[Index] <= '9')) or
            (Value[Index] = '-')) then
    begin
      Result := False;
      Exit;
    end;
end;

function TryGetTrimmedWmiString(const Value: Variant;
  var Text: String): Boolean;
begin
  Result := False;
  Text := '';
  if VarIsEmpty(Value) or VarIsNull(Value) then
    Exit;
  Text := Value;
  Text := Trim(Text);
  Result := Text <> '';
end;

function TryGetConsoleProfile(var ProfilePath: String): Boolean;
var
  Locator, Services, Systems, Computer, Accounts, Account,
    Profiles, Profile: Variant;
  ConsoleAccount, Domain, UserName, Sid, Query: String;
  Separator: Integer;
begin
  Result := False;
  ProfilePath := '';
  try
    Locator := CreateOleObject('WbemScripting.SWbemLocator');
    Services := Locator.ConnectServer('.', 'root\CIMV2');

    { Win32_ComputerSystem.UserName reports the console user even when Setup
      itself was started from a Terminal Services or SSH session. }
    Systems := Services.ExecQuery(
      'SELECT UserName FROM Win32_ComputerSystem');
    if Systems.Count <> 1 then
      Exit;
    Computer := Systems.ItemIndex(0);
    if not TryGetTrimmedWmiString(Computer.UserName, ConsoleAccount) then
      Exit;
    Separator := Pos('\', ConsoleAccount);
    if (Separator <= 1) or (Separator >= Length(ConsoleAccount)) then
      Exit;
    Domain := Copy(ConsoleAccount, 1, Separator - 1);
    UserName := Copy(ConsoleAccount, Separator + 1,
      Length(ConsoleAccount) - Separator);

    Query := 'SELECT SID FROM Win32_UserAccount WHERE Domain = ''' +
      WqlEscape(Domain) + ''' AND Name = ''' + WqlEscape(UserName) + '''';
    Accounts := Services.ExecQuery(Query);
    if Accounts.Count <> 1 then
      Exit;
    Account := Accounts.ItemIndex(0);
    if not TryGetTrimmedWmiString(Account.SID, Sid) then
      Exit;
    if not IsSidString(Sid) then
      Exit;

    Query := 'SELECT LocalPath FROM Win32_UserProfile WHERE SID = ''' +
      WqlEscape(Sid) + '''';
    Profiles := Services.ExecQuery(Query);
    if Profiles.Count <> 1 then
      Exit;
    Profile := Profiles.ItemIndex(0);
    if not TryGetTrimmedWmiString(Profile.LocalPath, ProfilePath) then
      Exit;
    Result := IsAbsoluteDrivePath(ProfilePath);
  except
    Log('Could not resolve the active console profile: ' +
      GetExceptionMessage);
  end;
end;

procedure ClearConsoleComponentCache(const Version: String);
var
  ProfilePath, CachePath: String;
begin
  if ExpectedCacheRelative(Version) = '' then
  begin
    Log('Refusing XAE Shell cache cleanup for an unsupported generation.');
    Exit;
  end;
  if not TryGetConsoleProfile(ProfilePath) then
  begin
    Log('Could not resolve the active console profile for XAE Shell cache cleanup.');
    Exit;
  end;
  if not TryBuildComponentCachePath(ProfilePath, Version, CachePath) then
  begin
    Log('The active console profile did not produce a safe XAE Shell cache path.');
    Exit;
  end;
  if DirExists(CachePath) then
  begin
    if DelTree(CachePath, True, True, True) then
      Log('Cleared the active console user XAE Shell ' + Version +
        ' component cache.')
    else
      Log('Could not clear the active console user XAE Shell component cache: ' +
        CachePath);
  end;
end;

function StoredInstallMatches(const Subkey, AppDir: String): Boolean;
var
  StoredDir, NormalizedStored, NormalizedApp: String;
begin
  Result := False;
  if not RegQueryStringValue(HKLM, Subkey, 'InstallDir', StoredDir) then
    Exit;
  try
    NormalizedStored :=
      RemoveBackslashUnlessRoot(ExpandFileName(StoredDir));
    NormalizedApp :=
      RemoveBackslashUnlessRoot(ExpandFileName(AppDir));
  except
    Exit;
  end;
  Result := IsAbsoluteDrivePath(NormalizedStored) and
    IsAbsoluteDrivePath(NormalizedApp) and
    (CompareText(NormalizedStored, NormalizedApp) = 0);
end;

function TryReadUninstallVersion(const AppDir: String;
  var Version: String): Boolean;
var
  Match15, Match17: Boolean;
begin
  Version := '';
  Match15 := StoredInstallMatches(
    'Software\ViKodex\TcTidy\XaeShell\{#AppId15}', AppDir);
  Match17 := StoredInstallMatches(
    'Software\ViKodex\TcTidy\XaeShell\{#AppId17}', AppDir);
  Result := Match15 <> Match17;
  if Match15 and not Match17 then
    Version := '15.0'
  else if Match17 and not Match15 then
    Version := '17.0';
end;

#ifdef SecurityFixture
function RunSecurityFixture(): Boolean;
var
  RootDir, AppDir, Marker, UnexpectedMarker, ExpectedMarker, MetadataPath,
    CachePath: String;
begin
  Result := False;
  RootDir := ExpandConstant('{tmp}\tctidy-installer-security');
  AppDir := RootDir + '\Common7\IDE\Extensions\TcTidy';
  MetadataPath := AppDir + '\.tctidy-install-target';
  ExpectedMarker := RootDir + '\Common7\IDE\extension.configurationchanged';
  if not ForceDirectories(AppDir) then
    Exit;
  if not SaveStringToFile(MetadataPath,
    'C:\Windows\System32\drivers\etc\hosts' + #13#10 +
    '..\..\Victim', False) then
    Exit;

  if not TryDeriveMergeMarker(AppDir, Marker) or
     (CompareText(Marker, ExpectedMarker) <> 0) or
     TryDeriveMergeMarker(AppDir + '\..\..\Victim', UnexpectedMarker) or
     (ExpectedCacheRelative('15.0') <>
       'Beckhoff\TcXaeShell\15.0_IsoShell\ComponentModelCache') or
     (ExpectedCacheRelative('17.0') <>
       'Beckhoff\TcXaeShell\17.0_IsoShell\ComponentModelCache') or
     (ExpectedCacheRelative('16.0') <> '') or
     (ExpectedCacheRelative('..\..\Victim') <> '') or
     not IsSidString('S-1-5-21-1234') or
     IsSidString('S-1-5-21-1234 OR Name') or
     (WqlEscape('DOMAIN\console') <> 'DOMAIN\\console') or
     not TryBuildComponentCachePath('C:\Users\console', '15.0',
       CachePath) or
     (CachePath <>
       'C:\Users\console\AppData\Local\Beckhoff\TcXaeShell\' +
       '15.0_IsoShell\ComponentModelCache') or
     TryBuildComponentCachePath('..\Victim', '15.0', CachePath) or
     TryBuildComponentCachePath('C:\Users\console', '..\Victim',
       CachePath) then
    Exit;
  Result := FileExists(MetadataPath);
end;

{ What a setup preselects in the wizard and unattended, given what the 17.0 and
  15.0 shells hold. }
function Preselects(const Holds17, Holds15, SetupVersion: String;
  Wizard, Unattended: Integer): Boolean;
var
  Holds: TArrayOfString;
begin
  SetArrayLength(Holds, 2);
  Holds[0] := Holds17;
  Holds[1] := Holds15;
  Result := (PreferredShellIndex(Holds, SetupVersion, False) = Wizard) and
    (PreferredShellIndex(Holds, SetupVersion, True) = Unattended);
end;

function ShellSelectionRuleHolds(): Boolean;
begin
  Result :=
    { No TcTidy anywhere: the newest shell. }
    Preselects('', '', '2.0.0', 0, 0) and
    { An upgrade goes where TcTidy is, whichever generation that is. }
    Preselects('', '1.0.0', '2.0.0', 1, 1) and
    Preselects('1.0.0', '', '2.0.0', 0, 0) and
    Preselects('1.0.0', '1.0.0', '2.0.0', 0, 0) and
    { A current copy is not the one to upgrade. }
    Preselects('2.0.0', '1.0.0', '2.0.0', 1, 1) and
    Preselects('1.0.0', '2.0.0', '2.0.0', 0, 0) and
    { A test build replaces its release, and a release its prerelease. }
    Preselects('', '2.0.0', '2.0.0-test', 1, 1) and
    Preselects('', '2.0.0-rc.1', '2.0.0', 1, 1) and
    { An unrecorded copy is not assumed current. }
    Preselects('', '{#UnknownInstalledVersion}', '2.0.0', 1, 1) and
    { Run again: the wizard offers the other shell, an unattended run stays. }
    Preselects('2.0.0', '', '2.0.0', 1, 0) and
    Preselects('', '2.0.0', '2.0.0', 0, 1) and
    Preselects('2.0.0', '2.0.0', '2.0.0', 0, 0) and
    { A newer copy is left alone while there is anything else to do. }
    Preselects('', '3.0.0', '2.0.0', 0, 0) and
    Preselects('3.0.0', '', '2.0.0', 1, 1) and
    Preselects('3.0.0', '1.0.0', '2.0.0', 1, 1) and
    Preselects('3.0.0', '2.0.0', '2.0.0', 1, 1) and
    Preselects('2.1.0-rc.1', '', '2.0.0', 1, 1) and
    Preselects('3.0.0', '3.0.0', '2.0.0', 0, 0) and
    { Versions compare as numbers. }
    Preselects('10.0.0', '', '9.0.0', 1, 1);
end;
#endif

function InitializeSetup(): Boolean;
var
  ErrorText, RequestedShell: String;
#ifdef SecurityFixture
  FixtureOutput, FixtureResult: String;
#endif
begin
  Result := False;
#ifdef SecurityFixture
  FixtureOutput := ExpandConstant('{param:SecurityFixtureOutput|}');
  if RunSecurityFixture() and ShellSelectionRuleHolds() then
    FixtureResult := 'PASS'
  else
    FixtureResult := 'FAIL';
  if FixtureOutput <> '' then
    SaveStringToFile(FixtureOutput, FixtureResult, False);
  Exit;
#endif

  SelectedShellIndex := -1;
  if not LoadShellInstances(ErrorText) then
  begin
    SuppressibleMsgBox('TwinCAT XAE Shell discovery failed.' + #13#10#13#10 + ErrorText,
      mbCriticalError, MB_OK, IDOK);
    Exit;
  end;

  if GetArrayLength(ShellVersions) = 0 then
  begin
    SuppressibleMsgBox('No supported TwinCAT XAE Shell was found.' + #13#10#13#10 +
      'Install TcXaeShell 15.0 (x86) or 17.0 (x64), then run this setup again.',
      mbCriticalError, MB_OK, IDOK);
    Exit;
  end;

  RequestedShell := ExpandConstant('{param:SHELL|}');
  if RequestedShell = '' then
    SelectedShellIndex := PreferredShellIndex(ShellInstalledVersions,
      '{#AppVersion}', WizardSilent())
  else
  begin
    SelectedShellIndex := IndexOfShell(RequestedShell);
    if SelectedShellIndex < 0 then
    begin
      Log('The requested XAE Shell ' + RequestedShell + ' was not found.');
      SuppressibleMsgBox('TwinCAT XAE Shell ' + RequestedShell +
        ' was not found on this machine.', mbCriticalError, MB_OK, IDOK);
      Exit;
    end;
  end;
  Result := True;
end;

procedure InitializeWizard();
var
  Index: Integer;
begin
  { Right away, so that the in-use check looks at the shell's folder even when
    /DIR= names another one. }
  WizardForm.DirEdit.Text := ShellExtensionDirs[SelectedShellIndex];
  HasShellSelectionPage := GetArrayLength(ShellVersions) > 1;
  if not HasShellSelectionPage then
    Exit;

  ShellSelectionPage := CreateInputOptionPage(wpWelcome,
    'Select TwinCAT XAE Shell',
    'Choose the shell instance that will receive TcTidy.',
    'Each shell instance is installed, upgraded, and uninstalled independently: ' +
    'setup updates one per run.',
    True, False);
  for Index := 0 to GetArrayLength(ShellVersions) - 1 do
    ShellSelectionPage.Add(Format('%s: %s - %s', [ShellLabel(Index),
      DescribeInstalled(ShellInstalledVersions[Index]), ShellExecutables[Index]]));
  ShellSelectionPage.SelectedValueIndex := SelectedShellIndex;
end;

procedure ApplyShellSelection();
begin
  if HasShellSelectionPage then
    SelectedShellIndex := ShellSelectionPage.SelectedValueIndex;
  if SelectedShellIsValid() then
    WizardForm.DirEdit.Text := ShellExtensionDirs[SelectedShellIndex];
end;

function NextButtonClick(CurPageID: Integer): Boolean;
begin
  Result := True;
  if HasShellSelectionPage and (CurPageID = ShellSelectionPage.ID) then
  begin
    ApplyShellSelection();
    Result := SelectedShellIsValid();
  end;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  ApplyShellSelection();
  if not SelectedShellIsValid() then
  begin
    Result := 'No TwinCAT XAE Shell instance is selected.';
    Exit;
  end;
  if not FileExists(ShellExecutables[SelectedShellIndex]) then
  begin
    Result := 'The selected TwinCAT XAE Shell no longer exists at:' + #13#10 +
      ShellExecutables[SelectedShellIndex];
    Exit;
  end;
  Result := '';
end;

function UpdateReadyMemo(Space, NewLine, MemoUserInfoInfo, MemoDirInfo,
  MemoTypeInfo, MemoComponentsInfo, MemoGroupInfo, MemoTasksInfo: String): String;
begin
  Result := 'TwinCAT XAE Shell ' + ShellLabel(SelectedShellIndex) + ':' + NewLine +
    Space + ShellExecutables[SelectedShellIndex] + NewLine + NewLine +
    'TcTidy:' + NewLine +
    Space + DescribeChange(ShellInstalledVersions[SelectedShellIndex], '{#AppVersion}');
end;

procedure CurPageChanged(CurPageID: Integer);
begin
  if CurPageID = wpFinished then
  begin
    WizardForm.FinishedLabel.Caption := FinishedText();
    { The label keeps the height of its default text and would cut this one. }
    WizardForm.FinishedLabel.AdjustHeight();
  end;
end;

{ Touch both shell markers, next to the validated extension path: the IDE-level
  one, and Extensions\extensions.configurationchanged, which makes the shell
  re-merge every .pkgdef at its next start so a changed package registration
  (such as an autoload rule) takes effect after an upgrade. }
procedure ForceShellRefresh(const MergeMarker: String);
begin
  if MergeMarker <> '' then
  begin
    SaveStringToFile(MergeMarker, '', False);
    SaveStringToFile(AddBackslash(ExtractFileDir(MergeMarker)) +
      'Extensions\extensions.configurationchanged', '', False);
  end;
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then
  begin
    ForceShellRefresh(ShellMergeMarkers[SelectedShellIndex]);
#ifndef PayloadFixture
    ClearConsoleComponentCache(ShellVersions[SelectedShellIndex]);
#endif
    Log(DescribeChange(ShellInstalledVersions[SelectedShellIndex], '{#AppVersion}'));
    Log(FinishedText());
  end;
end;

function InitializeUninstall(): Boolean;
begin
  UninstallShellVersion := '';
  if not TryDeriveMergeMarker(ExpandConstant('{app}'), UninstallMergeMarker) then
  begin
    UninstallMergeMarker := '';
    Log('The uninstall directory is not a supported XAE Shell extension path; ' +
      'the merge marker will not be touched.');
  end;
#ifndef PayloadFixture
  if not TryReadUninstallVersion(ExpandConstant('{app}'),
    UninstallShellVersion) then
    Log('The uninstall generation could not be verified; no XAE Shell cache ' +
      'will be touched.');
#endif
  Result := True;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usUninstall then
  begin
#ifndef PayloadFixture
    if UninstallShellVersion <> '' then
      ClearConsoleComponentCache(UninstallShellVersion);
#endif
  end
  else if CurUninstallStep = usPostUninstall then
    ForceShellRefresh(UninstallMergeMarker);
end;
