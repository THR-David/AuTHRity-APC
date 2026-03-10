# Linux -> Windows Cross-Compile

This folder contains scripts for building Windows `.exe` artifacts on Linux and packaging them into a Windows-portable folder.

## Prerequisites

- Rust toolchain with `rustup`
- MinGW cross compiler (`x86_64-w64-mingw32-gcc`)
- Node.js + npm

Example install commands (Ubuntu/Debian):

```bash
sudo apt-get update
sudo apt-get install -y mingw-w64
```

## Usage

Run from repo root:

```bash
chmod +x Build_scripts/linux_cross_compile/*.sh
./Build_scripts/linux_cross_compile/build_windows.sh
./Build_scripts/linux_cross_compile/package_windows.sh portable-windows-cross
```

## Notes

- This package includes `CONFIGURE.ps1` and Windows start/stop batch scripts.
- The packager auto-attempts to download a Windows QuestDB bundle. If download/extraction fails, copy QuestDB manually into `portable-windows-cross/questdb`.
- OPC UA config is bundled from `opcua_server/config` (including `serversettings.env` when present).
