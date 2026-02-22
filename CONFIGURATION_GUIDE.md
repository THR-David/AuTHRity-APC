# AuTHRity Configuration Guide (code-derived)

This guide lists the runtime configuration surfaces used by the codebase.

## OPC UA Server

- [opcua_server/config/server.conf](opcua_server/config/server.conf)
  - `application_name`, `application_uri`
  - `tcp_config.host`, `tcp_config.port`
  - `pki_dir`, `certificate_path`, `private_key_path`

## HMI Backend

- [hmi/config/settings.toml](hmi/config/settings.toml)
  - `[opcua]` `endpoint_url`, `namespace_index`
  - `[identity]` `app_name`, `app_uri`
  - `[services]` `supervisor_url`, `supervisor_api_key`, `opc_hot_reload_url`
  - `[historian]` QuestDB settings (ILP + REST)

## Virtual Plant

- [virtual_plant/config/settings.toml](virtual_plant/config/settings.toml)
  - `[opcua]` `endpoint_url`, `namespace_index`
  - `[identity]` `app_name`, `app_uri`
  - `[runtime]` `speed_multiplier`, `cycle_time_ms`

## APC Engine

- CLI args (used by `controller_host`):
  - `--model <path>`
  - `--opc opc.tcp://host:port`
  - `--pki <dir>`

## HMI Infrastructure Registry

- [hmi/config/hosts.json](hmi/config/hosts.json)
  - OPC server endpoints for discovery
  - Supervisor URL

## Portable Configuration

If you build a portable bundle with `copy_portable.ps1`, run `portable/CONFIGURE.ps1` to update IPs/hosts in the packaged configs.
