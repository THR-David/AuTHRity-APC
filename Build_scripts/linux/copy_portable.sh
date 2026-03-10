#!/bin/bash
set -euo pipefail

# Canonical Linux portable packager.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$REPO_ROOT"

CYAN='\033[0;36m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

OUT="${1:-portable-linux}"

if [ ! -d "apc_engine" ] || [ ! -d "controller_host" ]; then
	echo -e "${RED}ERROR: Must be run from the project root (authrity/).${NC}"
	exit 1
fi

echo -e "${CYAN}\nAssembling portable folder: $OUT${NC}"

rm -rf "$OUT"
mkdir -p "$OUT/logs"
mkdir -p "$OUT/controller_host"
mkdir -p "$OUT/hmi"
mkdir -p "$OUT/opcua_server"
mkdir -p "$OUT/virtual_plant"

copy_file() {
	local SRC="$1"
	local DST="$2"
	if [ -f "$SRC" ]; then
		cp "$SRC" "$DST"
	else
		echo -e "${YELLOW}  WARN: $SRC not found - skipped${NC}"
	fi
}

copy_dir() {
	local SRC="$1"
	local DST="$2"
	if [ -d "$SRC" ]; then
		cp -r "$SRC" "$DST"
	else
		echo -e "${YELLOW}  WARN: $SRC not found - skipped${NC}"
	fi
}

echo -e "${CYAN}Copying Controller Host...${NC}"
copy_file "controller_host/target/release/controller_host"             "$OUT/controller_host/controller_host"
copy_file "apc_engine/target/release/authrity-apc-engine"              "$OUT/controller_host/authrity-apc-engine"
copy_dir  "apc_engine/pki"                                             "$OUT/controller_host/pki"
mkdir -p "$OUT/controller_host/config"
copy_file "controller_host/config/opc_client.env"                      "$OUT/controller_host/config/opc_client.env"
chmod +x "$OUT/controller_host/controller_host" 2>/dev/null || true
chmod +x "$OUT/controller_host/authrity-apc-engine" 2>/dev/null || true
echo -e "${GREEN}  Done${NC}"

echo -e "${CYAN}Copying HMI...${NC}"
copy_file "hmi/target/release/authrity-hmi"                            "$OUT/hmi/authrity-hmi"
copy_dir  "hmi/config"                                                 "$OUT/hmi/config"
copy_dir  "hmi/pki"                                                    "$OUT/hmi/pki"
if [ -d "hmi/frontend/dist" ]; then
	mkdir -p "$OUT/hmi/frontend"
	cp -r "hmi/frontend/dist" "$OUT/hmi/frontend/dist"
	echo -e "${GREEN}  Frontend dist copied${NC}"
else
	echo -e "${YELLOW}  WARN: hmi/frontend/dist not found - run 'npm run build' first${NC}"
fi
chmod +x "$OUT/hmi/authrity-hmi" 2>/dev/null || true
echo -e "${GREEN}  Done${NC}"

echo -e "${CYAN}Copying OPC UA Server...${NC}"
copy_file "opcua_server/target/release/authrity-opcua-server"          "$OUT/opcua_server/authrity-opcua-server"
copy_dir  "opcua_server/config"                                        "$OUT/opcua_server/config"
copy_dir  "opcua_server/models"                                        "$OUT/opcua_server/models"
copy_dir  "opcua_server/users"                                         "$OUT/opcua_server/users"
copy_dir  "opcua_server/pki"                                           "$OUT/opcua_server/pki"
chmod +x "$OUT/opcua_server/authrity-opcua-server" 2>/dev/null || true
echo -e "${GREEN}  Done${NC}"

echo -e "${CYAN}Copying Virtual Plant...${NC}"
copy_file "virtual_plant/target/release/authrity-virtual-plant"        "$OUT/virtual_plant/authrity-virtual-plant"
copy_dir  "virtual_plant/config"                                       "$OUT/virtual_plant/config"
copy_dir  "virtual_plant/pki"                                          "$OUT/virtual_plant/pki"
chmod +x "$OUT/virtual_plant/authrity-virtual-plant" 2>/dev/null || true
echo -e "${GREEN}  Done${NC}"

echo -e "${CYAN}Copying controller models...${NC}"
mkdir -p "$OUT/controller_host/models"
if [ -d "controller_host/models" ]; then
	cp -r controller_host/models/. "$OUT/controller_host/models/"
	echo -e "${GREEN}  Models copied${NC}"
else
	echo -e "${YELLOW}  WARN: controller_host/models not found - skipped${NC}"
fi

echo -e "${CYAN}Fetching fresh QuestDB...${NC}"
QUESTDB_VERSION="8.2.3"
QUESTDB_TAR="questdb-${QUESTDB_VERSION}-no-jre-bin.tar.gz"
QUESTDB_URL="https://github.com/questdb/questdb/releases/download/${QUESTDB_VERSION}/${QUESTDB_TAR}"
QUESTDB_DIR="$OUT/questdb"

mkdir -p "$QUESTDB_DIR"

if command -v curl >/dev/null 2>&1; then
	curl -L --progress-bar "$QUESTDB_URL" -o "/tmp/$QUESTDB_TAR"
elif command -v wget >/dev/null 2>&1; then
	wget -q --show-progress "$QUESTDB_URL" -O "/tmp/$QUESTDB_TAR"
else
	echo -e "${RED}ERROR: Neither curl nor wget found. Cannot download QuestDB.${NC}"
	exit 1
fi

tar -xzf "/tmp/$QUESTDB_TAR" -C "$QUESTDB_DIR" --strip-components=1
rm "/tmp/$QUESTDB_TAR"
mkdir -p "$QUESTDB_DIR/data"
echo -e "${GREEN}  QuestDB ${QUESTDB_VERSION} ready (fresh - no existing data)${NC}"

echo -e "${CYAN}Writing start_all.sh...${NC}"
cat > "$OUT/start_all.sh" << 'EOF'
#!/bin/bash
DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
cd "$DIR"
mkdir -p logs

launch_term() {
	local TITLE="$1"
	local WORKDIR="$2"
	local CMD="$3"
	local SCRIPT
	printf -v SCRIPT 'cd %q && %s; echo; echo "[%s exited - terminal kept open]"; exec bash' \
		"$WORKDIR" "$CMD" "$TITLE"

	if command -v konsole &>/dev/null; then
		konsole --title "$TITLE" -e bash -c "$SCRIPT" &
	elif command -v gnome-terminal &>/dev/null; then
		gnome-terminal --title="$TITLE" -- bash -c "$SCRIPT" &
	elif command -v xfce4-terminal &>/dev/null; then
		xfce4-terminal --title="$TITLE" -x bash -c "$SCRIPT" &
	elif command -v alacritty &>/dev/null; then
		alacritty --title "$TITLE" -e bash -c "$SCRIPT" &
	elif command -v kitty &>/dev/null; then
		kitty --title "$TITLE" bash -c "$SCRIPT" &
	elif command -v wezterm &>/dev/null; then
		wezterm start --title "$TITLE" -- bash -c "$SCRIPT" &
	elif command -v xterm &>/dev/null; then
		xterm -title "$TITLE" -e bash -c "$SCRIPT" &
	else
		echo "  No GUI terminal found - $TITLE starting in background (logs/$TITLE.log)"
		(cd "$WORKDIR" && eval "$CMD") >> "$DIR/logs/$TITLE.log" 2>&1 &
	fi
}

if [ -f "questdb/questdb.jar" ]; then
	echo "Starting QuestDB..."
	launch_term "QuestDB" "$DIR/questdb" "java -jar questdb.jar -d ./data"
	sleep 5
fi

echo "Starting OPC UA Server..."
launch_term "opcua_server"    "$DIR/opcua_server"    "./authrity-opcua-server"
sleep 2

echo "Starting Virtual Plant..."
launch_term "virtual_plant"   "$DIR/virtual_plant"   "./authrity-virtual-plant"
sleep 2

echo "Starting Controller Host..."
launch_term "controller_host" "$DIR/controller_host" "./controller_host"
sleep 2

echo "Starting HMI..."
launch_term "hmi"             "$DIR/hmi"             "./authrity-hmi"
sleep 1

echo ""
echo "AuTHRity launched. Each service is in its own terminal window."
echo "HMI: http://localhost:3000"
xdg-open http://localhost:3000 2>/dev/null || true
EOF
chmod +x "$OUT/start_all.sh"

echo -e "${CYAN}Writing stop_all.sh...${NC}"
cat > "$OUT/stop_all.sh" << 'EOF'
#!/bin/bash
echo "Stopping AuTHRity..."
pkill -f authrity-hmi          2>/dev/null || true
pkill -f controller_host       2>/dev/null || true
pkill -f authrity-apc-engine   2>/dev/null || true
pkill -f authrity-virtual-plant 2>/dev/null || true
pkill -f authrity-opcua-server 2>/dev/null || true
pkill -f questdb.jar           2>/dev/null || true
echo "Done."
EOF
chmod +x "$OUT/stop_all.sh"

echo -e "${GREEN}\nPortable folder ready: $OUT${NC}"
echo -e "   Start:  ${CYAN}cd $OUT && ./start_all.sh${NC}"
echo -e "   Stop:   ${CYAN}cd $OUT && ./stop_all.sh${NC}"
