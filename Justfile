# Justfile for AuTHRity System
# Cross-platform build and deployment automation

# --- Variables ---
# Detect OS to handle .exe extensions
is_windows := if os() == "windows" { "true" } else { "false" }
exe_ext := if is_windows == "true" { ".exe" } else { "" }

# Build Profiles
profile := "release"
target_dir := if profile == "release" { "target/release" } else { "target/debug" }

# --- Main Commands ---

# Default: List available commands
default:
    @just --list

# Build everything
build-all: build-server build-engine build-hmi build-virtual build-controller
    @echo "✅ All components built successfully."

# Create the Portable deployment folder (Builds everything first)
portable: build-all
    @echo "📦 Creating portable package..."
    # Clean up old folder
    rm -rf portable
    mkdir -p portable/logs portable/models
    
    # Create Component Folders
    mkdir -p portable/controller_host/pki
    mkdir -p portable/hmi/config portable/hmi/pki portable/hmi/frontend/dist
    mkdir -p portable/opcua_server/models portable/opcua_server/users portable/opcua_server/pki portable/opcua_server/config
    mkdir -p portable/virtual_plant/config portable/virtual_plant/pki
    
    # --- Controller Host & Engine ---
    cp controller_host/{{target_dir}}/controller_host{{exe_ext}} portable/controller_host/
    cp apc_engine/{{target_dir}}/authrity-apc-engine{{exe_ext}} portable/controller_host/
    cp -r apc_engine/pki/* portable/controller_host/pki/ || true
    cp controller_host/models/*.json portable/models/ || true

    # --- HMI ---
    cp hmi/{{target_dir}}/authrity-hmi{{exe_ext}} portable/hmi/
    cp -r hmi/config/* portable/hmi/config/ || true
    cp -r hmi/frontend/dist/* portable/hmi/frontend/dist/ || true
    cp -r hmi/pki/* portable/hmi/pki/ || true

    # --- OPC UA Server ---
    cp opcua_server/{{target_dir}}/authrity-opcua-server{{exe_ext}} portable/opcua_server/
    cp opcua_server/config/server.conf portable/opcua_server/config/ || true
    cp -r opcua_server/models/* portable/opcua_server/models/ || true
    cp -r opcua_server/users/* portable/opcua_server/users/ || true
    cp -r opcua_server/pki/* portable/opcua_server/pki/ || true

    # --- Virtual Plant ---
    cp virtual_plant/{{target_dir}}/authrity-virtual-plant{{exe_ext}} portable/virtual_plant/
    cp -r virtual_plant/config/* portable/virtual_plant/config/ || true
    cp -r virtual_plant/pki/* portable/virtual_plant/pki/ || true

    # --- Generate Startup Scripts ---
    @echo "📜 Generating startup scripts..."
    
    # Generate Windows Batch Script
    @echo "@echo off" > portable/START_ALL.bat
    @echo "echo Starting AuTHRity..." >> portable/START_ALL.bat
    @echo "cd /d \"%~dp0\"" >> portable/START_ALL.bat
    @echo "start \"OPCServer\" /MIN cmd /c \"cd opcua_server && .\\authrity-opcua-server.exe\"" >> portable/START_ALL.bat
    @echo "timeout /t 2 >nul" >> portable/START_ALL.bat
    @echo "start \"VirtualPlant\" /MIN cmd /c \"cd virtual_plant && .\\authrity-virtual-plant.exe\"" >> portable/START_ALL.bat
    @echo "timeout /t 2 >nul" >> portable/START_ALL.bat
    @echo "start \"Controller\" /MIN cmd /c \"cd controller_host && .\\controller_host.exe\"" >> portable/START_ALL.bat
    @echo "timeout /t 2 >nul" >> portable/START_ALL.bat
    @echo "start \"HMI\" /MIN cmd /c \"cd hmi && .\\authrity-hmi.exe\"" >> portable/START_ALL.bat
    @echo "echo AuTHRity started! HMI: http://localhost:3000" >> portable/START_ALL.bat
    @echo "start http://localhost:3000" >> portable/START_ALL.bat
    @echo "pause" >> portable/START_ALL.bat

    # Generate Linux Shell Script
    @echo "#!/bin/bash" > portable/start_all.sh
    @echo "# Start AuTHRity" >> portable/start_all.sh
    @echo "DIR=\"\$( cd \"\$( dirname \"\${BASH_SOURCE[0]}\" )\" && pwd )\"" >> portable/start_all.sh
    @echo "cd \"\$DIR\"" >> portable/start_all.sh
    @echo "start_comp() { echo \"Starting \$1...\"; (cd \$3 && ./\$2) > \"logs/\$1.log\" 2>&1 & }" >> portable/start_all.sh
    @echo "start_comp \"OPC Server\" \"authrity-opcua-server\" \"opcua_server\"" >> portable/start_all.sh
    @echo "sleep 2" >> portable/start_all.sh
    @echo "start_comp \"Virtual Plant\" \"authrity-virtual-plant\" \"virtual_plant\"" >> portable/start_all.sh
    @echo "sleep 2" >> portable/start_all.sh
    @echo "start_comp \"Controller Host\" \"controller_host\" \"controller_host\"" >> portable/start_all.sh
    @echo "sleep 2" >> portable/start_all.sh
    @echo "start_comp \"HMI\" \"authrity-hmi\" \"hmi\"" >> portable/start_all.sh
    @echo "echo \"AuTHRity launched. HMI: http://localhost:3000\"" >> portable/start_all.sh
    @echo "xdg-open http://localhost:3000 2>/dev/null || true" >> portable/start_all.sh
    chmod +x portable/start_all.sh

    @echo "✅ Portable package ready in ./portable"

# --- Component Build Recipes ---

build-server:
    @echo "🔨 Building OPC UA Server..."
    cd opcua_server && cargo build --{{profile}}

build-engine:
    @echo "🔨 Building APC Engine..."
    cd apc_engine && cargo build --{{profile}}

build-hmi: build-hmi-backend build-hmi-frontend

build-hmi-backend:
    @echo "🔨 Building HMI Backend..."
    cd hmi && cargo build --{{profile}}

build-hmi-frontend:
    @echo "🎨 Building HMI Frontend..."
    cd hmi/frontend && npm install && npm run build

build-virtual:
    @echo "🔨 Building Virtual Plant..."
    cd virtual_plant && cargo build --{{profile}}

build-controller:
    @echo "🔨 Building Controller Host..."
    cd controller_host && cargo build --{{profile}}

# --- Utility ---

clean:
    cargo clean
    rm -rf portable
    @echo "🧹 Workspace cleaned."

# Run localized tests
test:
    cargo test --workspace

