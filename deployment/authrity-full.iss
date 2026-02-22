; AuTHRity All-in-One Windows Installer
; Package 1: Full Bundle
; Contains: All components + QuestDB + JRE

#define MyAppName "AuTHRity"
#define MyAppVersion "0.2.0"
#define MyAppPublisher "Industrial Control Systems"
#define MyAppURL "https://github.com/yourusername/authrity"

[Setup]
; Basic info
AppId={{E8F9A1B2-C3D4-E5F6-A7B8-C9D0E1F2A3B4}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
AppUpdatesURL={#MyAppURL}

; Installation paths
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes

; Output
OutputDir=deployment\output
OutputBaseFilename=authrity-full-setup-{#MyAppVersion}
Compression=lzma2/max
SolidCompression=yes

; Windows version requirements
MinVersion=10.0.17763
ArchitecturesAllowed=x64
ArchitecturesInstallIn64BitMode=x64

; Privileges
PrivilegesRequired=admin
PrivilegesRequiredOverridesAllowed=dialog

; UI
WizardStyle=modern
SetupIconFile=deployment\icon.ico
WizardImageFile=deployment\wizard.bmp
WizardSmallImageFile=deployment\wizard-small.bmp

; License
LicenseFile=LICENSE
InfoBeforeFile=deployment\readme_install.txt

; Uninstall
UninstallDisplayIcon={app}\tools\nssm.exe

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Types]
Name: "full"; Description: "Full Installation (All components + Virtual Plant)"
Name: "standard"; Description: "Standard Installation (No Virtual Plant)"
Name: "custom"; Description: "Custom Installation"; Flags: iscustom

[Components]
Name: "core"; Description: "Core Components"; Types: full standard custom; Flags: fixed
Name: "core\controller"; Description: "APC Engine + Controller Host"; Types: full standard custom; Flags: fixed
Name: "core\hmi"; Description: "HMI Web Interface"; Types: full standard custom; Flags: fixed
Name: "core\opcua"; Description: "OPC UA Server"; Types: full standard custom; Flags: fixed
Name: "core\database"; Description: "QuestDB + JRE"; Types: full standard custom; Flags: fixed
Name: "virtualplant"; Description: "Virtual Plant Simulator (for testing)"; Types: full custom

[Tasks]
Name: "autostart"; Description: "Start services automatically on Windows startup"; GroupDescription: "Service Configuration:"; Flags: checkedonce
Name: "startservices"; Description: "Start services after installation"; GroupDescription: "Service Configuration:"; Flags: checkedonce
Name: "desktopicon"; Description: "Create a &desktop shortcut to HMI"; GroupDescription: "Additional icons:"

[Files]
; Binaries
Source: "deployment\artifacts\bin\authrity-apc-engine.exe"; DestDir: "{app}\bin"; Components: core\controller; Flags: ignoreversion
Source: "deployment\artifacts\bin\authrity-controller-host.exe"; DestDir: "{app}\bin"; Components: core\controller; Flags: ignoreversion
Source: "deployment\artifacts\bin\authrity-hmi.exe"; DestDir: "{app}\bin"; Components: core\hmi; Flags: ignoreversion
Source: "deployment\artifacts\bin\authrity-opcua-server.exe"; DestDir: "{app}\bin"; Components: core\opcua; Flags: ignoreversion
Source: "deployment\artifacts\bin\authrity-virtual-plant.exe"; DestDir: "{app}\bin"; Components: virtualplant; Flags: ignoreversion

; Configuration files
Source: "deployment\artifacts\config\controller_settings.toml"; DestDir: "{app}\config"; DestName: "controller_settings.toml"; Components: core\controller; Flags: ignoreversion onlyifdoesntexist
Source: "deployment\artifacts\config\hmi_settings.toml"; DestDir: "{app}\config"; DestName: "hmi_settings.toml"; Components: core\hmi; Flags: ignoreversion onlyifdoesntexist
Source: "deployment\artifacts\config\opcua_server.conf"; DestDir: "{app}\config"; DestName: "opcua_server.conf"; Components: core\opcua; Flags: ignoreversion onlyifdoesntexist
Source: "deployment\artifacts\config\virtualplant_settings.toml"; DestDir: "{app}\config"; DestName: "virtualplant_settings.toml"; Components: virtualplant; Flags: ignoreversion onlyifdoesntexist

; HMI Frontend
Source: "deployment\artifacts\frontend\*"; DestDir: "{app}\frontend"; Components: core\hmi; Flags: ignoreversion recursesubdirs createallsubdirs

; Models
Source: "deployment\artifacts\models\*.json"; DestDir: "{app}\models"; Components: core\controller; Flags: ignoreversion

; OPC UA Node definitions
Source: "hmi\models\*.yaml"; DestDir: "{app}\opcua_models"; Components: core\opcua; Flags: ignoreversion

; Tools
Source: "deployment\artifacts\tools\nssm.exe"; DestDir: "{app}\tools"; Components: core; Flags: ignoreversion

; QuestDB
Source: "deployment\questdb\*"; DestDir: "{app}\questdb"; Components: core\database; Flags: ignoreversion recursesubdirs createallsubdirs

; JRE (bundled with QuestDB)
Source: "deployment\jre\*"; DestDir: "{app}\jre"; Components: core\database; Flags: ignoreversion recursesubdirs createallsubdirs

; Documentation
Source: "README.md"; DestDir: "{app}\docs"; Flags: ignoreversion
Source: "CONFIGURATION_GUIDE.md"; DestDir: "{app}\docs"; Flags: ignoreversion
Source: "CONFIG_ANALYSIS.md"; DestDir: "{app}\docs"; Flags: ignoreversion

; Create PKI directories
[Dirs]
Name: "{app}\pki\own"; Components: core
Name: "{app}\pki\trusted"; Components: core
Name: "{app}\pki\rejected"; Components: core
Name: "{app}\logs"; Components: core
Name: "{app}\data"; Components: core\database

[Icons]
Name: "{group}\{#MyAppName} HMI"; Filename: "http://localhost:3000"; IconFilename: "{app}\tools\nssm.exe"
Name: "{group}\QuestDB Console"; Filename: "http://localhost:9000"; IconFilename: "{app}\tools\nssm.exe"
Name: "{group}\Configuration Folder"; Filename: "{app}\config"
Name: "{group}\Logs Folder"; Filename: "{app}\logs"
Name: "{group}\Uninstall {#MyAppName}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#MyAppName} HMI"; Filename: "http://localhost:3000"; IconFilename: "{app}\tools\nssm.exe"; Tasks: desktopicon

[Run]
; Install and start services if requested
Filename: "{app}\tools\nssm.exe"; Parameters: "install AuTHRity-QuestDB ""{app}\jre\bin\java.exe"" ""-Xmx2g -jar {app}\questdb\questdb.jar"""; StatusMsg: "Installing QuestDB service..."; Components: core\database; Flags: runhidden
Filename: "{app}\tools\nssm.exe"; Parameters: "set AuTHRity-QuestDB AppDirectory ""{app}\data"""; Components: core\database; Flags: runhidden
Filename: "{app}\tools\nssm.exe"; Parameters: "set AuTHRity-QuestDB DisplayName ""AuTHRity QuestDB"""; Components: core\database; Flags: runhidden
Filename: "{app}\tools\nssm.exe"; Parameters: "set AuTHRity-QuestDB Description ""AuTHRity Time-Series Database"""; Components: core\database; Flags: runhidden
Filename: "{app}\tools\nssm.exe"; Parameters: "set AuTHRity-QuestDB Start SERVICE_AUTO_START"; Components: core\database; Tasks: autostart; Flags: runhidden
Filename: "{app}\tools\nssm.exe"; Parameters: "set AuTHRity-QuestDB AppStdout ""{app}\logs\questdb.log"""; Components: core\database; Flags: runhidden
Filename: "{app}\tools\nssm.exe"; Parameters: "set AuTHRity-QuestDB AppStderr ""{app}\logs\questdb-error.log"""; Components: core\database; Flags: runhidden

Filename: "{app}\tools\nssm.exe"; Parameters: "install AuTHRity-OPCServer ""{app}\bin\authrity-opcua-server.exe"""; StatusMsg: "Installing OPC UA Server service..."; Components: core\opcua; Flags: runhidden
Filename: "{app}\tools\nssm.exe"; Parameters: "set AuTHRity-OPCServer AppDirectory ""{app}"""; Components: core\opcua; Flags: runhidden
Filename: "{app}\tools\nssm.exe"; Parameters: "set AuTHRity-OPCServer DisplayName ""AuTHRity OPC UA Server"""; Components: core\opcua; Flags: runhidden
Filename: "{app}\tools\nssm.exe"; Parameters: "set AuTHRity-OPCServer Description ""AuTHRity OPC UA Communication Server"""; Components: core\opcua; Flags: runhidden
Filename: "{app}\tools\nssm.exe"; Parameters: "set AuTHRity-OPCServer Start SERVICE_AUTO_START"; Components: core\opcua; Tasks: autostart; Flags: runhidden
Filename: "{app}\tools\nssm.exe"; Parameters: "set AuTHRity-OPCServer AppStdout ""{app}\logs\opcua-server.log"""; Components: core\opcua; Flags: runhidden
Filename: "{app}\tools\nssm.exe"; Parameters: "set AuTHRity-OPCServer AppStderr ""{app}\logs\opcua-server-error.log"""; Components: core\opcua; Flags: runhidden
Filename: "{app}\tools\nssm.exe"; Parameters: "set AuTHRity-OPCServer AppParameters ""--config config\opcua_server.conf"""; Components: core\opcua; Flags: runhidden

Filename: "{app}\tools\nssm.exe"; Parameters: "install AuTHRity-ControllerHost ""{app}\bin\authrity-controller-host.exe"""; StatusMsg: "Installing Controller Host service..."; Components: core\controller; Flags: runhidden
Filename: "{app}\tools\nssm.exe"; Parameters: "set AuTHRity-ControllerHost AppDirectory ""{app}"""; Components: core\controller; Flags: runhidden
Filename: "{app}\tools\nssm.exe"; Parameters: "set AuTHRity-ControllerHost DisplayName ""AuTHRity Controller Host"""; Components: core\controller; Flags: runhidden
Filename: "{app}\tools\nssm.exe"; Parameters: "set AuTHRity-ControllerHost Description ""AuTHRity APC Controller Management"""; Components: core\controller; Flags: runhidden
Filename: "{app}\tools\nssm.exe"; Parameters: "set AuTHRity-ControllerHost Start SERVICE_AUTO_START"; Components: core\controller; Tasks: autostart; Flags: runhidden
Filename: "{app}\tools\nssm.exe"; Parameters: "set AuTHRity-ControllerHost AppStdout ""{app}\logs\controller.log"""; Components: core\controller; Flags: runhidden
Filename: "{app}\tools\nssm.exe"; Parameters: "set AuTHRity-ControllerHost AppStderr ""{app}\logs\controller-error.log"""; Components: core\controller; Flags: runhidden
Filename: "{app}\tools\nssm.exe"; Parameters: "set AuTHRity-ControllerHost AppParameters ""--config config\controller_settings.toml"""; Components: core\controller; Flags: runhidden

Filename: "{app}\tools\nssm.exe"; Parameters: "install AuTHRity-HMI ""{app}\bin\authrity-hmi.exe"""; StatusMsg: "Installing HMI service..."; Components: core\hmi; Flags: runhidden
Filename: "{app}\tools\nssm.exe"; Parameters: "set AuTHRity-HMI AppDirectory ""{app}"""; Components: core\hmi; Flags: runhidden
Filename: "{app}\tools\nssm.exe"; Parameters: "set AuTHRity-HMI DisplayName ""AuTHRity HMI"""; Components: core\hmi; Flags: runhidden
Filename: "{app}\tools\nssm.exe"; Parameters: "set AuTHRity-HMI Description ""AuTHRity Human Machine Interface"""; Components: core\hmi; Flags: runhidden
Filename: "{app}\tools\nssm.exe"; Parameters: "set AuTHRity-HMI Start SERVICE_AUTO_START"; Components: core\hmi; Tasks: autostart; Flags: runhidden
Filename: "{app}\tools\nssm.exe"; Parameters: "set AuTHRity-HMI AppStdout ""{app}\logs\hmi.log"""; Components: core\hmi; Flags: runhidden
Filename: "{app}\tools\nssm.exe"; Parameters: "set AuTHRity-HMI AppStderr ""{app}\logs\hmi-error.log"""; Components: core\hmi; Flags: runhidden
Filename: "{app}\tools\nssm.exe"; Parameters: "set AuTHRity-HMI AppParameters ""--config config\hmi_settings.toml"""; Components: core\hmi; Flags: runhidden
Filename: "{app}\tools\nssm.exe"; Parameters: "set AuTHRity-HMI AppEnvironmentExtra ""FRONTEND_PATH={app}\frontend"""; Components: core\hmi; Flags: runhidden

Filename: "{app}\tools\nssm.exe"; Parameters: "install AuTHRity-VirtualPlant ""{app}\bin\authrity-virtual-plant.exe"""; StatusMsg: "Installing Virtual Plant service..."; Components: virtualplant; Flags: runhidden
Filename: "{app}\tools\nssm.exe"; Parameters: "set AuTHRity-VirtualPlant AppDirectory ""{app}"""; Components: virtualplant; Flags: runhidden
Filename: "{app}\tools\nssm.exe"; Parameters: "set AuTHRity-VirtualPlant DisplayName ""AuTHRity Virtual Plant"""; Components: virtualplant; Flags: runhidden
Filename: "{app}\tools\nssm.exe"; Parameters: "set AuTHRity-VirtualPlant Description ""AuTHRity Process Simulator"""; Components: virtualplant; Flags: runhidden
Filename: "{app}\tools\nssm.exe"; Parameters: "set AuTHRity-VirtualPlant Start SERVICE_AUTO_START"; Components: virtualplant; Tasks: autostart; Flags: runhidden
Filename: "{app}\tools\nssm.exe"; Parameters: "set AuTHRity-VirtualPlant AppStdout ""{app}\logs\virtualplant.log"""; Components: virtualplant; Flags: runhidden
Filename: "{app}\tools\nssm.exe"; Parameters: "set AuTHRity-VirtualPlant AppStderr ""{app}\logs\virtualplant-error.log"""; Components: virtualplant; Flags: runhidden
Filename: "{app}\tools\nssm.exe"; Parameters: "set AuTHRity-VirtualPlant AppParameters ""--config config\virtualplant_settings.toml"""; Components: virtualplant; Flags: runhidden

; Start services (in dependency order: DB → OPC UA → Controller → HMI → Plant)
Filename: "{app}\tools\nssm.exe"; Parameters: "start AuTHRity-QuestDB"; StatusMsg: "Starting QuestDB..."; Components: core\database; Tasks: startservices; Flags: runhidden waituntilterminated
Filename: "{sys}\timeout.exe"; Parameters: "/t 5 /nobreak"; Tasks: startservices; Flags: runhidden waituntilterminated
Filename: "{app}\tools\nssm.exe"; Parameters: "start AuTHRity-OPCServer"; StatusMsg: "Starting OPC UA Server..."; Components: core\opcua; Tasks: startservices; Flags: runhidden waituntilterminated
Filename: "{sys}\timeout.exe"; Parameters: "/t 3 /nobreak"; Tasks: startservices; Flags: runhidden waituntilterminated
Filename: "{app}\tools\nssm.exe"; Parameters: "start AuTHRity-ControllerHost"; StatusMsg: "Starting Controller Host..."; Components: core\controller; Tasks: startservices; Flags: runhidden waituntilterminated
Filename: "{sys}\timeout.exe"; Parameters: "/t 3 /nobreak"; Tasks: startservices; Flags: runhidden waituntilterminated
Filename: "{app}\tools\nssm.exe"; Parameters: "start AuTHRity-HMI"; StatusMsg: "Starting HMI..."; Components: core\hmi; Tasks: startservices; Flags: runhidden waituntilterminated
Filename: "{sys}\timeout.exe"; Parameters: "/t 2 /nobreak"; Tasks: startservices; Flags: runhidden waituntilterminated
Filename: "{app}\tools\nssm.exe"; Parameters: "start AuTHRity-VirtualPlant"; StatusMsg: "Starting Virtual Plant..."; Components: virtualplant; Tasks: startservices; Flags: runhidden waituntilterminated

; Open HMI in browser after installation
Filename: "http://localhost:3000"; Description: "Open AuTHRity HMI"; Flags: postinstall shellexec skipifsilent

[UninstallRun]
; Stop and remove services
Filename: "{app}\tools\nssm.exe"; Parameters: "stop AuTHRity-VirtualPlant"; RunOnceId: "StopVPlant"; Flags: runhidden
Filename: "{app}\tools\nssm.exe"; Parameters: "stop AuTHRity-HMI"; RunOnceId: "StopHMI"; Flags: runhidden
Filename: "{app}\tools\nssm.exe"; Parameters: "stop AuTHRity-ControllerHost"; RunOnceId: "StopController"; Flags: runhidden
Filename: "{app}\tools\nssm.exe"; Parameters: "stop AuTHRity-OPCServer"; RunOnceId: "StopOPCUA"; Flags: runhidden
Filename: "{app}\tools\nssm.exe"; Parameters: "stop AuTHRity-QuestDB"; RunOnceId: "StopDB"; Flags: runhidden
Filename: "{sys}\timeout.exe"; Parameters: "/t 5 /nobreak"; Flags: runhidden

Filename: "{app}\tools\nssm.exe"; Parameters: "remove AuTHRity-VirtualPlant confirm"; RunOnceId: "RemoveVPlant"; Flags: runhidden
Filename: "{app}\tools\nssm.exe"; Parameters: "remove AuTHRity-HMI confirm"; RunOnceId: "RemoveHMI"; Flags: runhidden
Filename: "{app}\tools\nssm.exe"; Parameters: "remove AuTHRity-ControllerHost confirm"; RunOnceId: "RemoveController"; Flags: runhidden
Filename: "{app}\tools\nssm.exe"; Parameters: "remove AuTHRity-OPCServer confirm"; RunOnceId: "RemoveOPCUA"; Flags: runhidden
Filename: "{app}\tools\nssm.exe"; Parameters: "remove AuTHRity-QuestDB confirm"; RunOnceId: "RemoveDB"; Flags: runhidden

[Code]
function InitializeSetup(): Boolean;
var
  ResultCode: Integer;
begin
  Result := True;
  
  // Check if .NET Framework or required runtimes exist
  // (QuestDB needs Java, but we bundle JRE)
  
  // Check for existing installation
  if RegKeyExists(HKEY_LOCAL_MACHINE, 'SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\{E8F9A1B2-C3D4-E5F6-A7B8-C9D0E1F2A3B4}_is1') then
  begin
    if MsgBox('AuTHRity is already installed. Do you want to upgrade?', mbConfirmation, MB_YESNO) = IDNO then
      Result := False;
  end;
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then
  begin
    // Any post-install tasks
    Log('Installation completed successfully');
  end;
end;

function ServiceExists(ServiceName: String): Boolean;
var
  ResultCode: Integer;
begin
  Exec('sc.exe', 'query "' + ServiceName + '"', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Result := (ResultCode = 0);
end;
