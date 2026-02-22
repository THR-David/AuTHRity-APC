# AuTHRity Configuration Wizard
# Automatically updates all config files for deployment on a new machine

param(
    [switch]$Silent,
    [string]$OpcIp,
    [string]$OpcPort,
    [string]$ControllerIp,
    [string]$ControllerPort,
    [string]$HmiIp,
    [string]$HmiPort,
    [string]$QuestDbIp
)

$ErrorActionPreference = "Stop"

Write-Host "`n========================================" -ForegroundColor Cyan
Write-Host "  AuTHRity Configuration Wizard v1.0" -ForegroundColor Cyan
Write-Host "  Updates all config files for deployment" -ForegroundColor Cyan
Write-Host "========================================`n" -ForegroundColor Cyan

# Get current machine IP
$LocalIp = (Get-NetIPAddress -AddressFamily IPv4 | Where-Object { $_.IPAddress -notlike "127.*" -and $_.IPAddress -notlike "169.*" } | Select-Object -First 1).IPAddress
if (-not $LocalIp) { $LocalIp = "127.0.0.1" }

Write-Host "Detected local IP: $LocalIp`n" -ForegroundColor Gray

# === Configuration Files to Update ===
$ConfigFiles = @{
    "OPC UA Server" = "opcua_server\server.conf"
    "Virtual Plant" = "virtual_plant\config\settings.toml"
    "HMI Backend"   = "hmi\config\settings.toml"
    "HMI Hosts"     = "hmi\config\hosts.json"
}

# Check if files exist
Write-Host "Checking config files..." -ForegroundColor Yellow
$AllExist = $true
foreach ($file in $ConfigFiles.Values) {
    if (Test-Path $file) {
        Write-Host "  [OK] $file" -ForegroundColor Green
    } else {
        Write-Host "  [MISSING] $file" -ForegroundColor Red
        $AllExist = $false
    }
}

if (-not $AllExist) {
    Write-Host "`nSome config files are missing. Run from portable\ folder." -ForegroundColor Red
    exit 1
}

Write-Host ""

# === Read Current Values ===
function Get-CurrentValue {
    param($File, $Pattern)
    if (Test-Path $File) {
        $content = Get-Content $File -Raw
        if ($content -match $Pattern) {
            return $matches[1]
        }
    }
    return "NOT FOUND"
}

$CurrentOpcIp = Get-CurrentValue "opcua_server\server.conf" 'host:\s*([^\s\r\n]+)'
$CurrentOpcPort = Get-CurrentValue "opcua_server\server.conf" 'port:\s*(\d+)'
$CurrentControllerPort = Get-CurrentValue "hmi\config\settings.toml" 'supervisor_url\s*=\s*"http://[^:]+:(\d+)"'
$CurrentHmiPort = Get-CurrentValue "hmi\config\settings.toml" 'http_port\s*=\s*(\d+)'
$CurrentQuestIp = Get-CurrentValue "hmi\config\settings.toml" 'host\s*=\s*"([^"]+)"'

# === Show Current Configuration ===
Write-Host "Current Configuration:" -ForegroundColor Cyan
Write-Host "  - OPC UA Server:    $CurrentOpcIp`:$CurrentOpcPort" -ForegroundColor Gray
Write-Host "  - Controller Host:  localhost:$CurrentControllerPort" -ForegroundColor Gray
Write-Host "  - HMI Backend:      localhost:$CurrentHmiPort" -ForegroundColor Gray
Write-Host "  - QuestDB:          $CurrentQuestIp`:9009" -ForegroundColor Gray
Write-Host ""

# === Prompt for New Values ===
if (-not $Silent) {
    Write-Host "Deployment Scenarios:" -ForegroundColor Yellow
    Write-Host "  [1] All-in-One (localhost) - Everything on this machine" -ForegroundColor White
    Write-Host "  [2] Same Network - All services on detected IP: $LocalIp" -ForegroundColor White
    Write-Host "  [3] Custom - Manually specify each IP/port" -ForegroundColor White
    Write-Host "  [Q] Quit" -ForegroundColor DarkGray
    Write-Host ""
    
    $choice = Read-Host "Select deployment type [1-3, Q]"
    
    switch ($choice.ToUpper()) {
        "Q" {
            Write-Host "Cancelled." -ForegroundColor Yellow
            exit 0
        }
        "1" {
            Write-Host "`nSelected: All-in-One (localhost)`n" -ForegroundColor Green
            $OpcIp = "127.0.0.1"
            $OpcPort = "4855"
            $ControllerIp = "127.0.0.1"
            $ControllerPort = "8080"
            $HmiIp = "127.0.0.1"
            $HmiPort = "3000"
            $QuestDbIp = "localhost"
        }
        "2" {
            Write-Host "`nSelected: Same Network ($LocalIp)`n" -ForegroundColor Green
            $OpcIp = $LocalIp
            $OpcPort = "4855"
            $ControllerIp = $LocalIp
            $ControllerPort = "8080"
            $HmiIp = $LocalIp
            $HmiPort = "3000"
            $QuestDbIp = $LocalIp
        }
        "3" {
            Write-Host "`nSelected: Custom Configuration`n" -ForegroundColor Green
            
            Write-Host "OPC UA Server:" -ForegroundColor Cyan
            $OpcIp = Read-Host "  IP Address [$CurrentOpcIp]"
            if ([string]::IsNullOrWhiteSpace($OpcIp)) { $OpcIp = $CurrentOpcIp }
            
            $OpcPort = Read-Host "  Port [$CurrentOpcPort]"
            if ([string]::IsNullOrWhiteSpace($OpcPort)) { $OpcPort = $CurrentOpcPort }
            
            Write-Host "`nController Host:" -ForegroundColor Cyan
            $ControllerIp = Read-Host "  IP Address [$LocalIp]"
            if ([string]::IsNullOrWhiteSpace($ControllerIp)) { $ControllerIp = $LocalIp }
            
            $ControllerPort = Read-Host "  Port [$CurrentControllerPort]"
            if ([string]::IsNullOrWhiteSpace($ControllerPort)) { $ControllerPort = $CurrentControllerPort }
            
            Write-Host "`nHMI Backend:" -ForegroundColor Cyan
            $HmiIp = Read-Host "  IP Address [$LocalIp]"
            if ([string]::IsNullOrWhiteSpace($HmiIp)) { $HmiIp = $LocalIp }
            
            $HmiPort = Read-Host "  Port [$CurrentHmiPort]"
            if ([string]::IsNullOrWhiteSpace($HmiPort)) { $HmiPort = $CurrentHmiPort }
            
            Write-Host "`nQuestDB:" -ForegroundColor Cyan
            $QuestDbIp = Read-Host "  IP Address [$CurrentQuestIp]"
            if ([string]::IsNullOrWhiteSpace($QuestDbIp)) { $QuestDbIp = $CurrentQuestIp }
        }
        default {
            Write-Host "❌ Invalid choice" -ForegroundColor Red
            exit 1
        }
    }
}

# === Summary ===
Write-Host "NEW CONFIGURATION SUMMARY" -ForegroundColor Yellow
Write-Host "======================================" -ForegroundColor Yellow
Write-Host "  OPC UA Server:    opc.tcp://$OpcIp`:$OpcPort" -ForegroundColor White
Write-Host "  Controller Host:  http://$ControllerIp`:$ControllerPort" -ForegroundColor White
Write-Host "  HMI Backend:      http://$HmiIp`:$HmiPort" -ForegroundColor White
Write-Host "  QuestDB:          $QuestDbIp`:9009" -ForegroundColor White
Write-Host ""

if (-not $Silent) {
    $confirm = Read-Host "Apply these changes? [Y/N]"
    if ($confirm -ne "Y" -and $confirm -ne "y") {
        Write-Host "Cancelled." -ForegroundColor Yellow
        exit 0
    }
}

# === Backup Original Files ===
Write-Host "`nCreating backups..." -ForegroundColor Yellow
$BackupDir = "config_backup_$(Get-Date -Format 'yyyyMMdd_HHmmss')"
New-Item -ItemType Directory -Path $BackupDir -Force | Out-Null

foreach ($file in $ConfigFiles.Values) {
    if (Test-Path $file) {
        $backupPath = Join-Path $BackupDir (Split-Path $file -Leaf)
        Copy-Item $file $backupPath
        Write-Host "  [OK] Backed up: $file" -ForegroundColor Gray
    }
}

# === Update Configuration Files ===
Write-Host "`nUpdating configuration files..." -ForegroundColor Yellow

# 1. OPC UA Server (YAML format)
Write-Host "  [1/4] opcua_server\server.conf..." -ForegroundColor Cyan
$opcContent = Get-Content "opcua_server\server.conf" -Raw
$opcContent = $opcContent -replace '(host:\s*)([^\s\r\n]+)', "`${1}$OpcIp"
$opcContent = $opcContent -replace '(port:\s*)(\d+)', "`${1}$OpcPort"
$opcContent = $opcContent -replace '(opc\.tcp://)[^:]+:(\d+)', "`${1}$OpcIp`:$OpcPort"
$opcContent | Set-Content "opcua_server\server.conf" -NoNewline
Write-Host "    [OK] Updated OPC UA Server config" -ForegroundColor Green

# 2. Virtual Plant (TOML format)
Write-Host "  [2/4] virtual_plant\config\settings.toml..." -ForegroundColor Cyan
$vpContent = Get-Content "virtual_plant\config\settings.toml" -Raw
$vpContent = $vpContent -replace '(endpoint_url\s*=\s*"opc\.tcp://)[^:]+:(\d+)', "`${1}$OpcIp`:$OpcPort"
$vpContent | Set-Content "virtual_plant\config\settings.toml" -NoNewline
Write-Host "    [OK] Updated Virtual Plant config" -ForegroundColor Green

# 3. HMI Backend (TOML format)
Write-Host "  [3/4] hmi\config\settings.toml..." -ForegroundColor Cyan
$hmiContent = Get-Content "hmi\config\settings.toml" -Raw
$hmiContent = $hmiContent -replace '(endpoint_url\s*=\s*"opc\.tcp://)[^:]+:(\d+)', "`${1}$OpcIp`:$OpcPort"
$hmiContent = $hmiContent -replace '(supervisor_url\s*=\s*"http://)[^:]+:(\d+)', "`${1}$ControllerIp`:$ControllerPort"
$hmiContent = $hmiContent -replace '(host\s*=\s*")[^"]+(")', "`${1}$QuestDbIp`${2}"
$hmiContent | Set-Content "hmi\config\settings.toml" -NoNewline
Write-Host "    [OK] Updated HMI Backend config" -ForegroundColor Green

# 4. HMI Hosts (JSON format)
Write-Host "  [4/4] hmi\config\hosts.json..." -ForegroundColor Cyan
if (Test-Path "hmi\config\hosts.json") {
    $hostsJson = Get-Content "hmi\config\hosts.json" -Raw | ConvertFrom-Json
    
    # Update supervisor URL
    if ($hostsJson.supervisors -and $hostsJson.supervisors.Count -gt 0) {
        $hostsJson.supervisors[0].url = "http://$ControllerIp`:$ControllerPort"
    }
    
    # Update OPC servers
    foreach ($opc in $hostsJson.opc_servers) {
        if ($opc.endpoint -match 'opc\.tcp://[^:]+:\d+') {
            $opc.endpoint = "opc.tcp://$OpcIp`:$OpcPort"
        }
        if ($opc.reload_url -match 'http://[^:]+:\d+') {
            $opc.reload_url = "http://$OpcIp`:9090"
        }
    }
    
    $hostsJson | ConvertTo-Json -Depth 10 | Set-Content "hmi\config\hosts.json"
    Write-Host "    [OK] Updated HMI Hosts config" -ForegroundColor Green
} else {
    Write-Host "    [WARN] hosts.json not found, skipping" -ForegroundColor Yellow
}

# === Validation ===
Write-Host "`nConfiguration Update Complete!" -ForegroundColor Green
Write-Host "`nNext Steps:" -ForegroundColor Cyan
Write-Host "  1. Review the updated configs in each service folder" -ForegroundColor White
Write-Host "  2. If needed, restore from: $BackupDir" -ForegroundColor White
Write-Host "  3. Run START_ALL.bat to launch with new configuration" -ForegroundColor White
Write-Host "`nAccess URLs:" -ForegroundColor Cyan
Write-Host "  HMI:       http://$HmiIp`:$HmiPort" -ForegroundColor White
Write-Host "  QuestDB:   http://$QuestDbIp`:9000" -ForegroundColor White
Write-Host "  OPC UA:    opc.tcp://$OpcIp`:$OpcPort" -ForegroundColor White
Write-Host ""
