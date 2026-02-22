# Build all AuTHRity components for deployment
# Run from project root: .\build_all.ps1

$ErrorActionPreference = "Stop"

Write-Host "`n=== Building AuTHRity System ===" -ForegroundColor Cyan

# Check if we're in the right directory
if (-not (Test-Path "apc_engine") -or -not (Test-Path "opcua_server")) {
    Write-Host "ERROR: Must run from project root (c:\RUST\authrity)" -ForegroundColor Red
    exit 1
}

# Function to build a Rust project
function Build-RustProject {
    param($Name, $Path)
    
    Write-Host "`n--- Building $Name ---" -ForegroundColor Yellow
    Push-Location $Path
    try {
        cargo build --release
        if ($LASTEXITCODE -eq 0) {
            Write-Host "✅ $Name built successfully" -ForegroundColor Green
        } else {
            Write-Host "❌ $Name build failed" -ForegroundColor Red
            Pop-Location
            exit 1
        }
    } finally {
        Pop-Location
    }
}

# Build Rust projects
Build-RustProject "OPC UA Server" "opcua_server"
Build-RustProject "DMC Engine" "apc_engine"
Build-RustProject "HMI Backend" "hmi"
Build-RustProject "Virtual Plant" "virtual_plant"
Build-RustProject "Controller Host" "controller_host"
Build-RustProject "AuTHRity Bridge" "authrity_bridge"

# Build HMI Frontend
Write-Host "`n--- Building HMI Frontend ---" -ForegroundColor Yellow
Push-Location "hmi\frontend"
try {
    if (-not (Test-Path "node_modules")) {
        Write-Host "Running npm install..." -ForegroundColor Cyan
        npm install
    }
    npm run build
    if ($LASTEXITCODE -eq 0) {
        Write-Host "✅ HMI Frontend built successfully" -ForegroundColor Green
    } else {
        Write-Host "❌ HMI Frontend build failed" -ForegroundColor Red
        Pop-Location
        exit 1
    }
} finally {
    Pop-Location
}

Write-Host "`n=== Build Summary ===" -ForegroundColor Cyan
Write-Host "✅ opcua_server\target\release\authrity-opcua-server.exe" -ForegroundColor Green
Write-Host "✅ apc_engine\target\release\authrity-apc-engine.exe" -ForegroundColor Green
Write-Host "✅ hmi\target\release\authrity-hmi.exe" -ForegroundColor Green
Write-Host "✅ virtual_plant\target\release\authrity-virtual-plant.exe" -ForegroundColor Green
Write-Host "✅ controller_host\target\release\controller_host.exe" -ForegroundColor Green
Write-Host "✅ authrity_bridge\target\release\authrity-bridge.exe" -ForegroundColor Green
Write-Host "✅ hmi\frontend\dist\" -ForegroundColor Green

Write-Host "`nNext steps:" -ForegroundColor Cyan
Write-Host "1. Update IP addresses in all config files"
Write-Host "2. See README.md -> 'Deployment to Another Windows Machine'"
Write-Host "3. Copy deployment package to target machine"
