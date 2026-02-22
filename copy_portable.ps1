# Simple Copy Script - Assumes everything is already built
# Just copies files to portable folder

param(
    [string]$OutputDir = "portable"
)

$ErrorActionPreference = "Stop"

# Detect OS
$IsWindows = $env:OS -like "*Windows*"
if ($IsWindows) {
    $ExeExt = ".exe"
    Write-Host "`nEnvironment: Windows detected" -ForegroundColor Cyan
} else {
    $ExeExt = ""
    Write-Host "`nEnvironment: Linux/Unix detected" -ForegroundColor Cyan
}

Write-Host "`nCreating portable folder..." -ForegroundColor Cyan

$RootDir = Get-Location
$PortableDir = Join-Path $RootDir $OutputDir

# Clean and create
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

# Copy Controller Host
Write-Host "Copying Controller Host..." -ForegroundColor Gray
Copy-Item "controller_host/target/release/controller_host$ExeExt" -Destination "$PortableDir/controller_host/" -Force -ErrorAction SilentlyContinue
Copy-Item "apc_engine/target/release/authrity-apc-engine$ExeExt" -Destination "$PortableDir/controller_host/" -Force -ErrorAction SilentlyContinue
if (Test-Path "apc_engine/pki") {
    Copy-Item "apc_engine/pki" -Destination "$PortableDir/controller_host/pki" -Recurse -Force
    Write-Host "  PKI folder copied" -ForegroundColor Green
}

# Copy HMI
Write-Host "Copying HMI..." -ForegroundColor Gray
Copy-Item "hmi/target/release/authrity-hmi$ExeExt" -Destination "$PortableDir/hmi/" -Force -ErrorAction SilentlyContinue
if (Test-Path "hmi/config") {
    Copy-Item "hmi/config" -Destination "$PortableDir/hmi/config" -Recurse -Force
}
if (Test-Path "hmi/frontend/dist") {
    Copy-Item "hmi/frontend/dist" -Destination "$PortableDir/hmi/frontend/dist" -Recurse -Force
    Write-Host "  Frontend dist copied" -ForegroundColor Green
}
if (Test-Path "hmi/pki") {
    Copy-Item "hmi/pki" -Destination "$PortableDir/hmi/pki" -Recurse -Force
    Write-Host "  PKI folder copied" -ForegroundColor Green
}

# Copy OPC UA Server
Write-Host "Copying OPC UA Server..." -ForegroundColor Gray
Copy-Item "opcua_server/target/release/authrity-opcua-server$ExeExt" -Destination "$PortableDir/opcua_server/" -Force -ErrorAction SilentlyContinue
if (Test-Path "opcua_server/server.conf") {
    Copy-Item "opcua_server/server.conf" -Destination "$PortableDir/opcua_server/" -Force
}
if (Test-Path "opcua_server/models") {
    Copy-Item "opcua_server/models" -Destination "$PortableDir/opcua_server/models" -Recurse -Force
}
if (Test-Path "opcua_server/users") {
    Copy-Item "opcua_server/users" -Destination "$PortableDir/opcua_server/users" -Recurse -Force
}
if (Test-Path "opcua_server/pki") {
    Copy-Item "opcua_server/pki" -Destination "$PortableDir/opcua_server/pki" -Recurse -Force
    Write-Host "  PKI folder copied" -ForegroundColor Green
}

# Copy Virtual Plant
Write-Host "Copying Virtual Plant..." -ForegroundColor Gray
Copy-Item "virtual_plant/target/release/authrity-virtual-plant$ExeExt" -Destination "$PortableDir/virtual_plant/" -Force -ErrorAction SilentlyContinue
if (Test-Path "virtual_plant/config") {
    Copy-Item "virtual_plant/config" -Destination "$PortableDir/virtual_plant/config" -Recurse -Force
}
if (Test-Path "virtual_plant/pki") {
    Copy-Item "virtual_plant/pki" -Destination "$PortableDir/virtual_plant/pki" -Recurse -Force
    Write-Host "  PKI folder copied" -ForegroundColor Green
}

# Copy models to shared models folder
Write-Host "Copying models..." -ForegroundColor Gray
if (Test-Path "controller_host/models") {
    Copy-Item "controller_host/models/*.json" -Destination "$PortableDir/models/" -Force -ErrorAction SilentlyContinue
}

# Copy QuestDB
Write-Host "Copying QuestDB..." -ForegroundColor Gray
$QuestDbSource = Join-Path (Split-Path $RootDir -Parent) "questdb"
if (Test-Path $QuestDbSource) {
    New-Item -Path "$PortableDir/questdb" -ItemType Directory -Force | Out-Null
    Copy-Item "$QuestDbSource/*" -Destination "$PortableDir/questdb/" -Recurse -Force
    Write-Host "  QuestDB copied" -ForegroundColor Green
} else {
    Write-Host "  WARNING: QuestDB not found at $QuestDbSource" -ForegroundColor Yellow
}

# Create START_ALL.bat (main entry point - this is what users click)
Write-Host "Creating START_ALL.bat..." -ForegroundColor Gray
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
echo.
echo IMPORTANT: To prevent accidental pausing, right-click each console
echo            window title bar ^> Properties ^> Uncheck "QuickEdit Mode"
echo            This is a one-time setup step.
echo.
start http://localhost:3000
pause
'@
$StartBat | Out-File -FilePath "$PortableDir\START_ALL.bat" -Encoding ASCII

# Create STOP_ALL.bat
Write-Host "Creating STOP_ALL.bat..." -ForegroundColor Gray
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

# Copy configuration wizard
Write-Host "Copying configuration wizard..." -ForegroundColor Gray
$ConfigWizardSource = Join-Path $RootDir "deployment\CONFIGURE.ps1"
if (Test-Path $ConfigWizardSource) {
    Copy-Item $ConfigWizardSource -Destination "$PortableDir\CONFIGURE.ps1" -Force
    Write-Host "  CONFIGURE.ps1 copied" -ForegroundColor Green
} else {
    Write-Host "  WARNING: deployment\CONFIGURE.ps1 not found" -ForegroundColor Yellow
}

Write-Host "`nDONE!" -ForegroundColor Green
Write-Host "Folder: $PortableDir" -ForegroundColor White
Write-Host ""
Write-Host "⚙️  IMPORTANT: Run CONFIGURE.ps1 to update IPs for your network!" -ForegroundColor Yellow
Write-Host ""
if ($IsWindows) {
    Write-Host "Run START_ALL.bat to launch." -ForegroundColor Cyan
} else {
    # Generate Linux Scripts if on Linux
    Write-Host "Creating Linux scripts..." -ForegroundColor Gray
    
    $StartSh = @'
#!/bin/bash
# Start AuTHRity System on Linux
DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
cd "$DIR"
echo "Starting AuTHRity..."
if [ -d "questdb/bin" ]; then
    echo "Starting QuestDB..."
    (cd questdb/bin && ./questdb.sh start) &
    sleep 5
fi

# Helper to start component
start_comp() {
    TITLE=$1
    CMD=$2
    DIR=$3
    echo "Starting $TITLE..."
    mkdir -p logs
    (cd $DIR && $CMD) > "logs/$TITLE.log" 2>&1 &
}

start_comp "OPC Server" "./authrity-opcua-server" "opcua_server"
sleep 2
start_comp "Virtual Plant" "./authrity-virtual-plant" "virtual_plant"
sleep 2
start_comp "Controller Host" "./controller_host" "controller_host"
sleep 2
start_comp "HMI" "./authrity-hmi" "hmi"
sleep 2

echo "AuTHRity launched in background. Check logs/*.log"
echo "HMI: http://localhost:3000"
'@
    $StartSh | Out-File -FilePath "$PortableDir/start_all.sh" -Encoding utf8
    chmod +x "$PortableDir/start_all.sh"

    $StopSh = @'
#!/bin/bash
echo "Stopping all AuTHRity processes..."
pkill -f authrity-hmi
pkill -f authrity-apc-engine
pkill -f authrity-opcua-server
pkill -f authrity-virtual-plant
pkill -f controller_host
if [ -d "questdb/bin" ]; then
    (cd questdb/bin && ./questdb.sh stop)
fi
echo "Done."
'@
    $StopSh | Out-File -FilePath "$PortableDir/stop_all.sh" -Encoding utf8
    chmod +x "$PortableDir/stop_all.sh"

    Write-Host "Run ./start_all.sh to launch." -ForegroundColor Cyan
}
