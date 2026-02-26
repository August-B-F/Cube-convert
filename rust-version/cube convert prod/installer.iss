[Setup]
AppName=Cube Convert
AppVersion=0.1.0
DefaultDirName={autopf}\Cube-Convert
DefaultGroupName=Cube Convert
OutputDir=.
OutputBaseFilename=Cube-Convert-Setup
Compression=lzma2
SolidCompression=yes
SetupIconFile=C:\serverSync\Cube-convert\rust-version\test_win\assets\icon.ico
UninstallDisplayIcon={app}\cube_convert_rs.exe

[Files]
Source: "C:\serverSync\Cube-convert\rust-version\test_win\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs

[Icons]
Name: "{group}\Cube Convert"; Filename: "{app}\cube_convert_rs.exe"
Name: "{autodesktop}\Cube Convert"; Filename: "{app}\cube_convert_rs.exe"

[Run]
Filename: "{app}\cube_convert_rs.exe"; Description: "Launch Cube Convert"; Flags: nowait postinstall skipifsilent
