; Inno Setup script for InkPDF. Build it with installer\build.ps1 (it passes the version and the
; path of the release exe). Installs for the current user by default, with no administrator prompt.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef ExeSource
  #error Pass /DExeSource=path\to\inkpdf.exe (installer\build.ps1 does)
#endif
#ifndef ThumbSource
  #error Pass /DThumbSource=path\to\inkpdf_thumb.dll (installer\build.ps1 does)
#endif

#define AppName "InkPDF"
#define AppExe "inkpdf.exe"
#define ProgId "InkPDF.Document"
; The class id of the thumbnail handler (apps\inkpdf-thumb), written with an escaped brace for Inno.
#define ThumbClsid "{{7A4C2E19-5B3D-4F80-A6D1-92C0E8B54F37}"

[Setup]
AppId={{6B3A1C52-7D0E-4C86-9F4B-1E2A5D7C9B31}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher=InkPDF
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
SetupIconFile=app.ico
UninstallDisplayIcon={app}\{#AppExe}
UninstallDisplayName={#AppName}
OutputDir=Output
OutputBaseFilename=InkPDF-Setup-{#AppVersion}
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
ChangesAssociations=yes
CloseApplications=yes
VersionInfoVersion={#AppVersion}
VersionInfoProductName={#AppName}
VersionInfoDescription={#AppName} setup

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; Flags: unchecked
Name: "openwith"; Description: "Show {#AppName} in the ""Open with"" list for PDF files, and show their first page as the file icon"; GroupDescription: "File types:"

[Files]
Source: "{#ExeSource}"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#ThumbSource}"; DestDir: "{app}"; Flags: ignoreversion restartreplace uninsrestartdelete
Source: "readme.txt"; DestDir: "{app}"; Flags: ignoreversion isreadme
Source: "..\..\..\LICENSE-MIT"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\..\LICENSE-APACHE"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExe}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; Tasks: desktopicon

[Registry]
; "Open with" for PDFs. Windows does not let a program make itself the default; the person picks
; InkPDF once in "Open with > Choose another app", or in Settings > Default apps.
Root: HKA; Subkey: "Software\Classes\{#ProgId}"; ValueType: string; ValueName: ""; ValueData: "PDF document (InkPDF)"; Flags: uninsdeletekey; Tasks: openwith
Root: HKA; Subkey: "Software\Classes\{#ProgId}\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"",0"; Tasks: openwith
Root: HKA; Subkey: "Software\Classes\{#ProgId}\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""; Tasks: openwith
Root: HKA; Subkey: "Software\Classes\.pdf\OpenWithProgids"; ValueType: string; ValueName: "{#ProgId}"; ValueData: ""; Flags: uninsdeletevalue; Tasks: openwith
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}"; ValueType: string; ValueName: "FriendlyAppName"; ValueData: "{#AppName}"; Flags: uninsdeletekey; Tasks: openwith
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\SupportedTypes"; ValueType: string; ValueName: ".pdf"; ValueData: ""; Tasks: openwith
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""; Tasks: openwith

; The first page of a PDF as its icon in Explorer (the way Acrobat does it), for PDFs that open in
; InkPDF: a small thumbnail-handler DLL. Windows runs a handler registered for one user inside the
; program showing the thumbnails (DisableProcessIsolation), so the DLL contains no PDF code: it
; only starts inkpdf.exe --thumbnail, a separate short-lived process that does the drawing.
Root: HKA; Subkey: "Software\Classes\CLSID\{#ThumbClsid}"; ValueType: string; ValueName: ""; ValueData: "InkPDF Thumbnail Provider"; Flags: uninsdeletekey; Tasks: openwith
Root: HKA; Subkey: "Software\Classes\CLSID\{#ThumbClsid}"; ValueType: dword; ValueName: "DisableProcessIsolation"; ValueData: 1; Tasks: openwith
Root: HKA; Subkey: "Software\Classes\CLSID\{#ThumbClsid}\InprocServer32"; ValueType: string; ValueName: ""; ValueData: "{app}\inkpdf_thumb.dll"; Tasks: openwith
Root: HKA; Subkey: "Software\Classes\CLSID\{#ThumbClsid}\InprocServer32"; ValueType: string; ValueName: "ThreadingModel"; ValueData: "Both"; Tasks: openwith
Root: HKA; Subkey: "Software\Classes\{#ProgId}\ShellEx\{{e357fccd-a995-4576-b01f-234630154e96}"; ValueType: string; ValueName: ""; ValueData: "{#ThumbClsid}"; Tasks: openwith
[Run]
Filename: "{app}\{#AppExe}"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent
