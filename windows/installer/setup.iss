; 抖音视频解析 Windows 安装包（CI 中由 ISCC 编译）
; 用法：ISCC /DMyArch=x64|arm64 /DMyVersion=1.0.3 /DMySourceDir=<便携目录> /DMyOutputDir=<输出目录> setup.iss
; 注意：本文件含中文，CI 编译前会重写为 UTF-8 BOM，否则 Inno Setup 会按 ANSI 解析导致乱码。

#ifndef MyArch
  #define MyArch "x64"
#endif
#ifndef MyVersion
  #define MyVersion "1.0.0"
#endif
#ifndef MySourceDir
  #define MySourceDir "."
#endif
#ifndef MyOutputDir
  #define MyOutputDir "."
#endif

[Setup]
AppId={{B6B5A7E4-4C1B-4E80-9C6C-3D2F1A9E7B12}
AppName=抖音视频解析
AppVersion={#MyVersion}
AppVerName=抖音视频解析 {#MyVersion}
AppPublisher=抖音视频解析
DefaultDirName={autopf}\抖音视频解析
DefaultGroupName=抖音视频解析
DisableProgramGroupPage=yes
OutputDir={#MyOutputDir}
OutputBaseFilename=douyin_parser-win-{#MyArch}-setup
SetupIconFile=..\runner\resources\app.ico
UninstallDisplayIcon={app}\抖音视频解析.exe
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
PrivilegesRequired=admin
CloseApplications=yes
RestartApplications=no

; x64 安装包拒绝 Arm64 系统（Arm64 Win11 可通过兼容层运行 x64 程序，
; 但应使用专用的 arm64 安装包）；arm64 安装包仅允许 Arm64 系统。
#if MyArch == "arm64"
ArchitecturesAllowed=arm64
ArchitecturesInstallIn64BitMode=arm64
#else
ArchitecturesAllowed=x64compatible and not arm64
ArchitecturesInstallIn64BitMode=x64compatible
#endif

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#MySourceDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\抖音视频解析"; Filename: "{app}\抖音视频解析.exe"
Name: "{autodesktop}\抖音视频解析"; Filename: "{app}\抖音视频解析.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\抖音视频解析.exe"; Description: "{cm:LaunchProgram,抖音视频解析}"; Flags: nowait postinstall skipifsilent
