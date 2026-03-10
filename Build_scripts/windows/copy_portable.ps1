param(
    [string]$OutputDir = "portable-windows"
)

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = Resolve-Path (Join-Path $ScriptDir "..\..")
Set-Location $RepoRoot

Write-Host "`nCreating portable folder..." -ForegroundColor Cyan

$RootDir = Get-Location
$PortableDir = Join-Path $RootDir $OutputDir

if (Test-Path $PortableDir) {
    Remove-Item $PortableDir -Recurse -Force
}

Write-Host "Creating directories..." -ForegroundColor Gray
New-Item -Path "$PortableDir\logs" -ItemType Directory -Force | Out-Null
New-Item -Path "$PortableDir\models" -ItemType Directory -Force | Out-Null
New-Item -Path "$PortableDir\controller_host" -ItemType Directory -Force | Out-Null
New-Item -Path "$PortableDir\hmi" -ItemType Directory -Force | Out-Null
New-Item -Path "$PortableDir\opcua_server" -ItemType Directory -Force | Out-Null
New-Item -Path "$PortableDir\virtual_plant" -ItemType Directory -Force | Out-Null

Write-Host "Copying Controller Host..." -ForegroundColor Gray
Copy-Item "controller_host/target/release/controller_host.exe" -Destination "$PortableDir/controller_host/" -Force -ErrorAction SilentlyContinue
Copy-Item "apc_engine/target/release/authrity-apc-engine.exe" -Destination "$PortableDir/controller_host/" -Force -ErrorAction SilentlyContinue
if (Test-Path "apc_engine/pki") {
    Copy-Item "apc_engine/pki" -Destination "$PortableDir/controller_host/pki" -Recurse -Force
}

Write-Host "Copying HMI..." -ForegroundColor Gray
Copy-Item "hmi/target/release/authrity-hmi.exe" -Destination "$PortableDir/hmi/" -Force -ErrorAction SilentlyContinue
if (Test-Path "hmi/config") {
    Copy-Item "hmi/config" -Destination "$PortableDir/hmi/config" -Recurse -Force
}
if (Test-Path "hmi/frontend/dist") {
    Copy-Item "hmi/frontend/dist" -Destination "$PortableDir/hmi/frontend/dist" -Recurse -Force
}
if (Test-Path "hmi/pki") {
    Copy-Item "hmi/pki" -Destination "$PortableDir/hmi/pki" -Recurse -Force
}

Write-Host "Copying OPC UA Server..." -ForegroundColor Gray
Copy-Item "opcua_server/target/release/authrity-opcua-server.exe" -Destination "$PortableDir/opcua_server/" -Force -ErrorAction SilentlyContinue
if (Test-Path "opcua_server/config") {
    Copy-Item "opcua_server/config" -Destination "$PortableDir/opcua_server/config" -Recurse -Force
}
if (Test-Path "opcua_server/models") {
    Copy-Item "opcua_server/models" -Destination "$PortableDir/opcua_server/models" -Recurse -Force
}
if (Test-Path "opcua_server/users") {
    Copy-Item "opcua_server/users" -Destination "$PortableDir/opcua_server/users" -Recurse -Force
}
if (Test-Path "opcua_server/pki") {
    Copy-Item "opcua_server/pki" -Destination "$PortableDir/opcua_server/pki" -Recurse -Force
}

Write-Host "Copying Virtual Plant..." -ForegroundColor Gray
Copy-Item "virtual_plant/target/release/authrity-virtual-plant.exe" -Destination "$PortableDir/virtual_plant/" -Force -ErrorAction SilentlyContinue
if (Test-Path "virtual_plant/config") {
    Copy-Item "virtual_plant/config" -Destination "$PortableDir/virtual_plant/config" -Recurse -Force
}
if (Test-Path "virtual_plant/pki") {
    Copy-Item "virtual_plant/pki" -Destination "$PortableDir/virtual_plant/pki" -Recurse -Force
}

Write-Host "Copying models..." -ForegroundColor Gray
if (Test-Path "controller_host/models") {
    Copy-Item "controller_host/models/*.json" -Destination "$PortableDir/models/" -Force -ErrorAction SilentlyContinue
}

Write-Host "Copying configuration wizard..." -ForegroundColor Gray
$ConfigWizardSource = Join-Path $RootDir "Build_scripts\shared\CONFIGURE.ps1"
if (Test-Path $ConfigWizardSource) {
    Copy-Item $ConfigWizardSource -Destination "$PortableDir\CONFIGURE.ps1" -Force
}

$StartBat = @'
@echo off
echo Starting AuTHRity...
cd /d "%~dp0"

start "QuestDB" /MIN cmd /c "cd questdb\bin && .\questdb.exe"
timeout /t 5 /nobreak >nul

start "OPCServer" /MIN cmd /c "cd opcua_server && .\authrity-opcua-server.exe"
timeout /t 2 /nobreak >nul

start "VirtualPlant" /MIN cmd /c "cd virtual_plant && .\authrity-virtual-plant.exe"
timeout /t 2 /nobreak >nul

start "Controller" /MIN cmd /c "cd controller_host && .\controller_host.exe"
timeout /t 2 /nobreak >nul

start "HMI" /MIN cmd /c "cd hmi && .\authrity-hmi.exe"
timeout /t 2 /nobreak >nul

echo.
echo AuTHRity started!
echo HMI: http://localhost:3000
start http://localhost:3000
pause
'@
$StartBat | Out-File -FilePath "$PortableDir\START_ALL.bat" -Encoding ASCII

$StopBat = @'
@echo off
echo Stopping all AuTHRity processes...
taskkill /IM authrity-hmi.exe /F 2>nul
taskkill /IM authrity-apc-engine.exe /F 2>nul
taskkill /IM authrity-opcua-server.exe /F 2>nul
taskkill /IM authrity-virtual-plant.exe /F 2>nul
taskkill /IM controller_host.exe /F 2>nul
taskkill /IM questdb.exe /F 2>nul
echo Done.
pause
'@
$StopBat | Out-File -FilePath "$PortableDir\STOP_ALL.bat" -Encoding ASCII

Write-Host "`nDONE!" -ForegroundColor Green
Write-Host "Folder: $PortableDir" -ForegroundColor White
Write-Host "Run START_ALL.bat to launch." -ForegroundColor Cyan
