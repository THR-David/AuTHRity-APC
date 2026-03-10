#!/bin/bash
set -euo pipefail

# Canonical Linux build script.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$REPO_ROOT"

CYAN='\033[0;36m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

echo -e "${CYAN}\n=== Building AuTHRity System (Linux) ===${NC}"

if [ ! -d "apc_engine" ] || [ ! -d "opcua_server" ]; then
	echo -e "${RED}ERROR: Must run from project root${NC}"
	exit 1
fi

build_rust() {
	NAME=$1
	PATH_DIR=$2
	echo -e "${YELLOW}\n--- Building $NAME ---${NC}"
	(
		cd "$PATH_DIR" || exit
		if cargo build --release; then
			echo -e "${GREEN}$NAME built successfully${NC}"
		else
			echo -e "${RED}$NAME build failed${NC}"
			exit 1
		fi
	)
}

build_rust "OPC UA Server" "opcua_server"
build_rust "DMC Engine" "apc_engine"
build_rust "HMI Backend" "hmi"
build_rust "Virtual Plant" "virtual_plant"
build_rust "Controller Host" "controller_host"

echo -e "${YELLOW}\n--- Building HMI Frontend ---${NC}"
(
	cd "hmi/frontend" || exit
	if [ ! -d "node_modules" ]; then
		echo -e "${CYAN}Running npm install...${NC}"
		npm install
	fi
	if npm run build; then
		echo -e "${GREEN}HMI Frontend built successfully${NC}"
	else
		echo -e "${RED}HMI Frontend build failed${NC}"
		exit 1
	fi
)

echo -e "${CYAN}\n=== Build Summary ===${NC}"
echo -e "${GREEN}opcua_server/target/release/authrity-opcua-server${NC}"
echo -e "${GREEN}apc_engine/target/release/authrity-apc-engine${NC}"
echo -e "${GREEN}hmi/target/release/authrity-hmi${NC}"
echo -e "${GREEN}virtual_plant/target/release/authrity-virtual-plant${NC}"
echo -e "${GREEN}controller_host/target/release/controller_host${NC}"
echo -e "${GREEN}hmi/frontend/dist/${NC}"

echo -e "${CYAN}\nNext steps:${NC}"
echo "1. Run ./Build_scripts/linux/copy_portable.sh portable-linux to package everything"
echo "2. Run portable-linux/start_all.sh to launch"
