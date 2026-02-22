# Build AuTHRity All-in-One Installer
# Compiles all components and prepares for Inno Setup packaging

param(
    [switch]$SkipTests,
    [switch]$SkipFrontend,
    [switch]$Verbose
)

$ErrorActionPreference = "Stop"

# Colors
function Write-Step { param($Message) Write-Host "`n$Message" -ForegroundColor Cyan }
function Write-Info { param($Message) Write-Host "   $Message" -ForegroundColor Gray }
function Write-Success { param($Message) Write-Host "✅ $Message" -ForegroundColor Green }
function Write-Warning { param($Message) Write-Host "⚠️  $Message" -ForegroundColor Yellow }
function Write-Error2 { param($Message) Write-Host "❌ $Message" -ForegroundColor Red }

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "🚀 AuTHRity All-in-One Installer Build" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan

# Determine script location
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RootDir = Split-Path -Parent $ScriptDir
Push-Location $RootDir

# Create deployment directories
Write-Step "📁 Creating deployment structure..."
$DeploymentDir = "$RootDir\deployment"
$ArtifactsDir = "$DeploymentDir\artifacts"
$BinDir = "$ArtifactsDir\bin"
$ConfigDir = "$ArtifactsDir\config"
$FrontendDir = "$ArtifactsDir\frontend"
$ModelsDir = "$ArtifactsDir\models"
$ToolsDir = "$ArtifactsDir\tools"

@($DeploymentDir, $ArtifactsDir, $BinDir, $ConfigDir, $FrontendDir, $ModelsDir, $ToolsDir) | ForEach-Object {
    if (-not (Test-Path $_)) {
        New-Item -Path $_ -ItemType Directory -Force | Out-Null
        Write-Info "Created: $_"
    }
}

# Step 1: Run tests (unless skipped)
if (-not $SkipTests) {
    Write-Step "📋 Running tests..."
    Write-Info "Running apc_engine tests..."
    Push-Location apc_engine
    cargo test --release --quiet
    if ($LASTEXITCODE -ne 0) {
        Write-Error2 "Tests failed!"
        Pop-Location
        Pop-Location
        exit $LASTEXITCODE
    }
    Pop-Location
    Write-Success "All tests passed"
}

# Step 2: Build Rust binaries
Write-Step "🦀 Building Rust binaries (release mode)..."

$Components = @(
    @{Name="apc_engine"; Path="apc_engine"; Binary="authrity-apc-engine.exe"},
    @{Name="controller_host"; Path="controller_host"; Binary="authrity-controller-host.exe"},
    @{Name="opcua_server"; Path="opcua_server"; Binary="authrity-opcua-server.exe"},
    @{Name="virtual_plant"; Path="virtual_plant"; Binary="authrity-virtual-plant.exe"},
    @{Name="hmi"; Path="hmi"; Binary="authrity-hmi.exe"}
)

foreach ($Component in $Components) {
    Write-Info "Building $($Component.Name)..."
    
    Push-Location $Component.Path
    if ($Verbose) {
        cargo build --release
    } else {
        cargo build --release --quiet
    }
    
    if ($LASTEXITCODE -ne 0) {
        Write-Error2 "Failed to build $($Component.Name)"
        Pop-Location
        Pop-Location
        exit $LASTEXITCODE
    }
    
    # Copy binary to artifacts
    $SourceBin = "target\release\$($Component.Binary)"
    if (-not (Test-Path $SourceBin)) {
        # Try without .exe extension pattern
        $SourceBin = "target\release\$($Component.Name).exe"
    }
    
    if (Test-Path $SourceBin) {
        Copy-Item $SourceBin -Destination $BinDir -Force
        Write-Success "$($Component.Binary) → artifacts\bin\"
    } else {
        Write-Warning "Binary not found: $SourceBin"
    }
    
    Pop-Location
}

# Step 3: Build HMI frontend
if (-not $SkipFrontend) {
    Write-Step "⚛️  Building HMI frontend..."
    Push-Location hmi\frontend
    
    Write-Info "Installing dependencies..."
    if ($Verbose) {
        npm ci
    } else {
        npm ci --silent
    }
    
    if ($LASTEXITCODE -ne 0) {
        Write-Error2 "npm ci failed"
        Pop-Location
        Pop-Location
        exit $LASTEXITCODE
    }
    
    Write-Info "Building production bundle..."
    if ($Verbose) {
        npm run build
    } else {
        npm run build --silent
    }
    
    if ($LASTEXITCODE -ne 0) {
        Write-Error2 "npm build failed"
        Pop-Location
        Pop-Location
        exit $LASTEXITCODE
    }
    
    # Copy dist to artifacts
    if (Test-Path "dist") {
        Copy-Item "dist\*" -Destination $FrontendDir -Recurse -Force
        Write-Success "Frontend built → artifacts\frontend\"
    }
    
    Pop-Location
} else {
    Write-Warning "Skipping frontend build"
}

# Step 4: Copy configuration files
Write-Step "📝 Preparing configuration files..."

# HMI config (localhost all-in-one)
$HmiConfig = @"
# AuTHRity HMI Configuration (All-in-One)
# Generated: $(Get-Date -Format "yyyy-MM-dd HH:mm:ss")

[server]
host = "0.0.0.0"
port = 3000

[controller]
api_url = "http://localhost:8080"
api_key = "default-allinone-key"

[opcua]
endpoint_url = "opc.tcp://localhost:4840"
namespace_index = 2
security_policy = "None"
message_mode = "None"

[identity]
app_name = "AuTHRity HMI"
app_uri = "urn:authrity:hmi"
auto_create_keys = true
trust_server_certs = true

[historian]
enabled = true
host = "localhost"
ilp_port = 9009
http_port = 9000
table_name = "process_data"
batch_size = 100
flush_interval_ms = 1000

[paths]
pki_dir = "pki"

[runtime]
reconnect_delay_sec = 5
"@

$HmiConfig | Out-File -FilePath "$ConfigDir\hmi_settings.toml" -Encoding UTF8
Write-Info "Created hmi_settings.toml"

# Controller Host config
$ControllerConfig = @"
# AuTHRity Controller Host Configuration (All-in-One)
# Generated: $(Get-Date -Format "yyyy-MM-dd HH:mm:ss")

[api]
host = "0.0.0.0"
port = 8080
api_key = "default-allinone-key"

[paths]
model_dir = "./models"
engine_bin = "./authrity-apc-engine.exe"
pki_dir = "./pki"

[runtime]
max_instances = 10
"@

$ControllerConfig | Out-File -FilePath "$ConfigDir\controller_settings.toml" -Encoding UTF8
Write-Info "Created controller_settings.toml"

# OPC UA Server config
$OpcUaConfig = @"
# AuTHRity OPC UA Server Configuration (All-in-One)
# Generated: $(Get-Date -Format "yyyy-MM-dd HH:mm:ss")

application_name: ModelPredictiveControlServer
application_uri: urn:authrity:opcua:server
create_sample_keypair: true

tcp_config:
  host: 0.0.0.0
  port: 4840

pki_dir: "./pki"
certificate_path: own/cert.der
private_key_path: private/private.pem

certificate_validation:
  trust_client_certs: true
  check_time: false

limits:
  max_subscriptions: 100
  max_monitored_items_per_sub: 1000

user_tokens:
  anonymous:
    user: anonymous
    pass: ""
  admin:
    user: admin
    pass: admin123

# Node models loaded separately
"@

$OpcUaConfig | Out-File -FilePath "$ConfigDir\opcua_server.conf" -Encoding UTF8
Write-Info "Created opcua_server.conf"

# Virtual Plant config
$PlantConfig = @"
# AuTHRity Virtual Plant Configuration (All-in-One)
# Generated: $(Get-Date -Format "yyyy-MM-dd HH:mm:ss")

[opcua]
endpoint_url = "opc.tcp://localhost:4840"
namespace_index = 2
security_policy = "None"
message_mode = "None"

[identity]
app_name = "AuTHRity Virtual Plant"
app_uri = "urn:authrity:virtualplant"
auto_create_keys = true
trust_server_certs = true

[runtime]
speed_multiplier = 1
cycle_time_ms = 1000
reconnect_delay_sec = 5

[plant]
model = "debutanizer"

[debutanizer]
num_stages = 12
feed_stage = 6
relative_volatility = 2.2
hold_up_molar = 15000.0
dt_seconds = 1.0

[paths]
pki_dir = "pki"
"@

$PlantConfig | Out-File -FilePath "$ConfigDir\virtualplant_settings.toml" -Encoding UTF8
Write-Info "Created virtualplant_settings.toml"

Write-Success "All configuration files created"

# Step 5: Copy model files
Write-Step "📊 Copying model files..."
if (Test-Path "controller_host\models") {
    Copy-Item "controller_host\models\*.json" -Destination $ModelsDir -Force -ErrorAction SilentlyContinue
    $ModelCount = (Get-ChildItem "$ModelsDir\*.json" -ErrorAction SilentlyContinue).Count
    Write-Info "Copied $ModelCount model files"
}

# Step 6: Check for NSSM
Write-Step "🔧 Checking for NSSM..."
$NssmPath = "$ToolsDir\nssm.exe"
if (-not (Test-Path $NssmPath)) {
    Write-Warning "NSSM not found. Downloading..."
    try {
        $NssmUrl = "https://nssm.cc/release/nssm-2.24.zip"
        $NssmZip = "$DeploymentDir\nssm.zip"
        
        Invoke-WebRequest -Uri $NssmUrl -OutFile $NssmZip -UseBasicParsing
        Expand-Archive $NssmZip -DestinationPath "$DeploymentDir\temp" -Force
        Copy-Item "$DeploymentDir\temp\nssm-2.24\win64\nssm.exe" -Destination $NssmPath -Force
        Remove-Item "$DeploymentDir\temp" -Recurse -Force
        Remove-Item $NssmZip -Force
        
        Write-Success "NSSM downloaded"
    } catch {
        Write-Error2 "Failed to download NSSM: $_"
        Write-Info "Please download manually from https://nssm.cc/release/nssm-2.24.zip"
        Write-Info "Extract nssm.exe to: $ToolsDir"
    }
} else {
    Write-Success "NSSM found"
}

# Step 7: Check for QuestDB
Write-Step "💾 Checking for QuestDB..."
$QuestDbDir = "$DeploymentDir\questdb"
if (-not (Test-Path $QuestDbDir)) {
    Write-Warning "QuestDB not found"
    Write-Info "Please download QuestDB manually:"
    Write-Info "1. Go to: https://questdb.io/download/"
    Write-Info "2. Download: QuestDB (no-jre-bin)"
    Write-Info "3. Extract to: $QuestDbDir"
    Write-Info ""
    Write-Info "Also download JRE 11:"
    Write-Info "1. Go to: https://adoptium.net/temurin/releases/"
    Write-Info "2. Download: JRE 11 Windows x64 .zip"
    Write-Info "3. Extract to: $DeploymentDir\jre"
} else {
    Write-Success "QuestDB found"
    $QuestDbJar = Get-ChildItem "$QuestDbDir\questdb.jar" -ErrorAction SilentlyContinue
    if ($QuestDbJar) {
        Write-Info "QuestDB JAR: $($QuestDbJar.Name)"
    }
}

# Step 8: Summary
Write-Step "📋 Build Summary"
Write-Host ""
Write-Info "Artifacts prepared in: $ArtifactsDir"
Write-Info "Binaries: $((Get-ChildItem $BinDir -ErrorAction SilentlyContinue).Count) files"
Write-Info "Config files: $((Get-ChildItem $ConfigDir -ErrorAction SilentlyContinue).Count) files"
Write-Info "Frontend: $(if (Test-Path $FrontendDir\index.html) { 'Ready' } else { 'Missing' })"
Write-Info "Models: $((Get-ChildItem $ModelsDir -ErrorAction SilentlyContinue).Count) files"
Write-Info "NSSM: $(if (Test-Path $NssmPath) { 'Ready' } else { 'Missing' })"
Write-Info "QuestDB: $(if (Test-Path $QuestDbDir) { 'Ready' } else { 'Missing' })"

Write-Host ""
if ((Test-Path $NssmPath) -and (Test-Path $QuestDbDir)) {
    Write-Success "All components ready!"
    Write-Info "Next step: Run Inno Setup compiler on authrity-full.iss"
} else {
    Write-Warning "Some components missing - see warnings above"
}

Pop-Location
Write-Host ""
