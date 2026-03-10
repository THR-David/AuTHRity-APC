# Build Scripts

Organized build/package entry points by platform and workflow.

## Folders

- `linux/`: Native Linux build and Linux portable packaging wrappers.
- `windows/`: Native Windows PowerShell wrappers.
- `linux_cross_compile/`: Build Windows `.exe` on Linux and package a Windows-portable folder.
- `shared/`: Shared helper scripts used by packagers (for example `CONFIGURE.ps1`).

## Quick Start

Linux native:

```bash
chmod +x Build_scripts/linux/*.sh
./Build_scripts/linux/build_all.sh
./Build_scripts/linux/copy_portable.sh portable-linux
```

Linux -> Windows cross-compile:

```bash
chmod +x Build_scripts/linux_cross_compile/*.sh
./Build_scripts/linux_cross_compile/build_windows.sh
./Build_scripts/linux_cross_compile/package_windows.sh portable-windows-cross
```

Windows native:

```powershell
powershell -ExecutionPolicy Bypass -File .\Build_scripts\windows\build_all.ps1
powershell -ExecutionPolicy Bypass -File .\Build_scripts\windows\copy_portable.ps1 -OutputDir portable
```
