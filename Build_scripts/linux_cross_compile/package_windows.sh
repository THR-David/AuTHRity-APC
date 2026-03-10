#!/bin/bash
set -euo pipefail

# Package previously cross-compiled Windows binaries (.exe) into a portable folder.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
OUT="${1:-portable-windows-cross}"
TARGET="x86_64-pc-windows-gnu"
QUESTDB_VERSION="${QUESTDB_VERSION:-8.2.3}"

copy_file() {
    local src="$1"
    local dst="$2"
    if [ -f "$src" ]; then
        mkdir -p "$(dirname "$dst")"
        cp "$src" "$dst"
    else
        echo "WARN: Missing $src"
    fi
}

copy_dir() {
    local src="$1"
    local dst="$2"
    if [ -d "$src" ]; then
        mkdir -p "$(dirname "$dst")"
        cp -r "$src" "$dst"
    else
        echo "WARN: Missing $src"
    fi
}

download_questdb_windows() {
    local questdb_dir="$OUT/questdb"
    local tmp_pkg=""
    local base_url="https://github.com/questdb/questdb/releases/download/${QUESTDB_VERSION}"
    local downloaded=0
    local selected_asset=""

    # Candidate asset names vary slightly by release.
    local candidates=(
        "questdb-${QUESTDB_VERSION}-rt-windows-x86-64.tar.gz"
        "questdb-${QUESTDB_VERSION}-rt-windows-amd64.zip"
        "questdb-${QUESTDB_VERSION}-windows-amd64.zip"
    )

    echo "Attempting to bundle QuestDB ${QUESTDB_VERSION} for Windows..."
    mkdir -p "$questdb_dir"

    for asset in "${candidates[@]}"; do
        local url="${base_url}/${asset}"
        tmp_pkg="/tmp/${asset}"
        if command -v curl >/dev/null 2>&1; then
            if curl -fL --silent --show-error "$url" -o "$tmp_pkg"; then
                downloaded=1
                selected_asset="$asset"
                break
            fi
        elif command -v wget >/dev/null 2>&1; then
            if wget -q "$url" -O "$tmp_pkg"; then
                downloaded=1
                selected_asset="$asset"
                break
            fi
        else
            echo "WARN: Neither curl nor wget is available. Skipping QuestDB download."
            return 0
        fi
    done

    if [ "$downloaded" -eq 0 ]; then
        echo "WARN: Could not download QuestDB Windows bundle."
        echo "      You can copy QuestDB manually into $questdb_dir"
        return 0
    fi

    case "$selected_asset" in
        *.tar.gz)
            if command -v tar >/dev/null 2>&1; then
                tar -xzf "$tmp_pkg" -C "$questdb_dir"
            elif command -v bsdtar >/dev/null 2>&1; then
                bsdtar -xf "$tmp_pkg" -C "$questdb_dir"
            else
                echo "WARN: No tar extractor found (need tar or bsdtar)."
                echo "      Left archive at $tmp_pkg"
                return 0
            fi
            ;;
        *.zip)
            if command -v unzip >/dev/null 2>&1; then
                unzip -q "$tmp_pkg" -d "$questdb_dir"
            elif command -v bsdtar >/dev/null 2>&1; then
                bsdtar -xf "$tmp_pkg" -C "$questdb_dir"
            else
                echo "WARN: No zip extractor found (need unzip or bsdtar)."
                echo "      Left archive at $tmp_pkg"
                return 0
            fi
            ;;
        *)
            echo "WARN: Unknown QuestDB archive format: $selected_asset"
            echo "      Left archive at $tmp_pkg"
            return 0
            ;;
    esac

    rm -f "$tmp_pkg"

    # Normalize extracted layout so START_ALL.bat can find questdb\\bin\\questdb.exe.
    # Common case: archive expands into questdb-<version>-*/
    if [ ! -f "$questdb_dir/bin/questdb.exe" ]; then
        local top_dir
        top_dir=$(find "$questdb_dir" -mindepth 1 -maxdepth 1 -type d -name "questdb-*" | head -n 1 || true)
        if [ -n "$top_dir" ]; then
            mv "$top_dir"/* "$questdb_dir"/ 2>/dev/null || true
            rm -rf "$top_dir"
        fi
    fi

    # Fallback: locate questdb.exe in deeper nested path and flatten from there.
    if [ ! -f "$questdb_dir/bin/questdb.exe" ]; then
        local nested
        nested=$(find "$questdb_dir" -type f -path "*/bin/questdb.exe" | head -n 1 || true)
        if [ -n "$nested" ]; then
            local root
            root="${nested%/bin/questdb.exe}"
            if [ "$root" != "$questdb_dir" ]; then
                mv "$root"/* "$questdb_dir"/ 2>/dev/null || true
                rm -rf "$root"
            fi
        fi
    fi

    if [ -f "$questdb_dir/bin/questdb.exe" ]; then
        echo "QuestDB bundled at $questdb_dir"
    else
        echo "WARN: QuestDB download extracted, but questdb.exe was not found."
        echo "      Please verify Windows QuestDB files in $questdb_dir"
    fi
}

cd "$REPO_ROOT"
rm -rf "$OUT"
mkdir -p "$OUT"/logs
mkdir -p "$OUT"/controller_host
mkdir -p "$OUT"/hmi
mkdir -p "$OUT"/opcua_server
mkdir -p "$OUT"/virtual_plant
mkdir -p "$OUT"/models

# Try to include QuestDB Windows runtime for one-step portable startup.
download_questdb_windows

# Windows binaries
copy_file "controller_host/target/$TARGET/release/controller_host.exe" "$OUT/controller_host/controller_host.exe"
copy_file "apc_engine/target/$TARGET/release/authrity-apc-engine.exe" "$OUT/controller_host/authrity-apc-engine.exe"
copy_file "hmi/target/$TARGET/release/authrity-hmi.exe" "$OUT/hmi/authrity-hmi.exe"
copy_file "opcua_server/target/$TARGET/release/authrity-opcua-server.exe" "$OUT/opcua_server/authrity-opcua-server.exe"
copy_file "virtual_plant/target/$TARGET/release/authrity-virtual-plant.exe" "$OUT/virtual_plant/authrity-virtual-plant.exe"

# Config + data folders
copy_dir "apc_engine/pki" "$OUT/controller_host/pki"
mkdir -p "$OUT/controller_host/config"
copy_file "controller_host/config/opc_client.env" "$OUT/controller_host/config/opc_client.env"
copy_dir "controller_host/models" "$OUT/controller_host/models"
copy_dir "hmi/config" "$OUT/hmi/config"
copy_dir "hmi/pki" "$OUT/hmi/pki"
copy_dir "hmi/frontend/dist" "$OUT/hmi/frontend/dist"
copy_dir "opcua_server/config" "$OUT/opcua_server/config"
copy_dir "opcua_server/models" "$OUT/opcua_server/models"
copy_dir "opcua_server/users" "$OUT/opcua_server/users"
copy_dir "opcua_server/pki" "$OUT/opcua_server/pki"
copy_dir "virtual_plant/config" "$OUT/virtual_plant/config"
copy_dir "virtual_plant/pki" "$OUT/virtual_plant/pki"

# Shared model JSON mirror for compatibility with older portable layouts.
if [ -d "controller_host/models" ]; then
    cp -r controller_host/models/. "$OUT/models/"
fi

# Bundle Windows config wizard.
copy_file "Build_scripts/shared/CONFIGURE.ps1" "$OUT/CONFIGURE.ps1"

# Startup scripts for Windows portable package.
cat > "$OUT/START_ALL.bat" << 'EOF'
@echo off
echo Starting AuTHRity...
cd /d "%~dp0"

if exist "questdb\bin\questdb.exe" (
  start "QuestDB" /MIN cmd /c "cd questdb\bin && .\questdb.exe"
  timeout /t 5 /nobreak >nul
) else (
  echo QuestDB not bundled. Install or copy QuestDB into .\questdb\bin before starting historian.
)

start "OPCServer" /MIN cmd /c "cd opcua_server && .\authrity-opcua-server.exe"
timeout /t 2 /nobreak >nul

start "VirtualPlant" /MIN cmd /c "cd virtual_plant && .\authrity-virtual-plant.exe"
timeout /t 2 /nobreak >nul

start "Controller" /MIN cmd /c "cd controller_host && .\controller_host.exe"
timeout /t 2 /nobreak >nul

start "HMI" /MIN cmd /c "cd hmi && .\authrity-hmi.exe"
timeout /t 2 /nobreak >nul

echo.
echo AuTHRity started.
echo HMI: http://localhost:3000
start http://localhost:3000
pause
EOF

cat > "$OUT/STOP_ALL.bat" << 'EOF'
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
EOF

echo "Portable Windows package created: $OUT"
echo "Optional next step: zip it with 'zip -r ${OUT}.zip $OUT'"
