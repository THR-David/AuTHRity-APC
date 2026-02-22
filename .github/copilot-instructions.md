# Copilot Instructions - AuTHRity (code-derived)

## Architecture & Data Flow
- Multi-process system around OPC UA: `opcua_server` (UA server + hot-reload API), `virtual_plant` (simulator), `apc_engine` (MPC worker), `controller_host` (supervisor API), `hmi` (Rust backend + React frontend).
- `controller_host` deploys model JSONs and spawns `apc_engine` with CLI args (`--model`, `--opc`, `--pki`) and tracks process state; see [controller_host/src/main.rs](controller_host/src/main.rs).
- `hmi` backend bridges OPC UA to WebSocket: `opc_worker` reads/subscribes tags and broadcasts `OpcUpdate` via `broadcast::Sender`, frontend sends writes over `/ws` and backend forwards to OPC UA via `mpsc` commands; see [hmi/src/opc_worker.rs](hmi/src/opc_worker.rs) and [hmi/src/web_routes.rs](hmi/src/web_routes.rs).
- `hmi` backend also acts as orchestration proxy: deployment bundle route (`/api/deploy`) forwards JSON to supervisor and YAML to OPC hot-reload API, and controller lifecycle proxy routes (`/api/prox/controllers/*`) forward start/stop/list with supervisor API key.
- Infrastructure endpoints exist in HMI (`/api/infrastructure`) backed by `config/hosts.json`; model list/node map are resolved from configured remote OPC server first.

## Control Loop (APC Engine)
- `apc_engine` loads a `UnifiedModel` JSON, generates step response from FOPDT when `model_type == "parametric"`, or uses explicit coefficients for `"step_response"`; see [apc_engine/src/config.rs](apc_engine/src/config.rs) and [apc_engine/src/modelloader.rs](apc_engine/src/modelloader.rs).
- Each scan reads PV/target/MV/DV/mode/limits from OPC UA, validates ranges, runs `DmcController::next_move`, and writes predictions + setpoints only when `OperatingMode == 2`; see [apc_engine/src/main.rs](apc_engine/src/main.rs).
- MV writes are gated by PID mode: actual `Mode` must be 3 (Remote Cascade). If `ModeTarget` is 3, the engine sends a bumpless SP; otherwise it skips writes; see [apc_engine/src/main.rs](apc_engine/src/main.rs).
- Live tuning and limits are read from OPC UA each scan (CV weights/alphas, MV weights, limit nodes), so model JSON is treated as a baseline, not the live truth.
- Runtime auth to OPC UA is controlled by CLI (`--auth-mode username|x509` with optional `--username`/`--password`) and is injected by `controller_host` start payload.

## OPC UA Naming & Integration
- Node IDs are defined in model JSON: CVs include `pv`, `target`, `prediction`; MVs include `sp`, `op`, `mode`, `mode_target`, `future_plan`; DVs include `pv` and `limits`; see [apc_engine/src/config.rs](apc_engine/src/config.rs).
- System nodes are prefixed by engine ID (model name or file stem), e.g. `{engine}:OperatingMode`, `{engine}:Heartbeat`, `{engine}:SolverStatus` in [apc_engine/src/main.rs](apc_engine/src/main.rs).

## HMI Frontend Conventions
- WebSocket connects to `/ws` on the backend host; messages are JSON `{type:"WRITE", nodeId, value}` and `{type:"REFRESH"}`; see [hmi/src/web_routes.rs](hmi/src/web_routes.rs) and [hmi/frontend/src/App.tsx](hmi/frontend/src/App.tsx).
- Frontend uses Zustand `useTagStore` for tag updates and pending write acknowledgments; see [hmi/frontend/src/store/tagStore.ts](hmi/frontend/src/store/tagStore.ts).
- Model YAML is fetched via `/api/model?file=...` and tags are classified as CV/MV/DV by presence of `:Prediction` or `:FuturePlan`; see [hmi/frontend/src/App.tsx](hmi/frontend/src/App.tsx).
- Model Generator supports `parametric` and `step_response` workflows, plus Step Response Tool that computes coefficients from historian data via `/api/stepresponse/calculate`.
- In current UI, step-response coefficients are imported/attached and summarized, but there is no dedicated coefficient curve editor view yet.

## Model Schema Notes (Important)
- `UnifiedModel` now includes richer optimization/tuning fields in CV/MV structs (e.g. `optimization_mode`, `slack_weight`, `is_integrating`, optional MV `target` + `target_weight`). Preserve backward compatibility when adding parser/export changes.
- Physics supports both MV and DV parametric matrices (`gain/tau/dead_time` and `gain_dv/tau_dv/dead_time_dv`) and explicit response matrices (`step_coefficients`, `dv_coefficients`).
- Loader behavior remains mode-gated today: `parametric` generates coefficients from FOPDT; `step_response` consumes provided coefficients.

## Historian & Step Response
- HMI historian can query QuestDB trends (`/api/trends`) and provide available tags (`/api/stepresponse/tags`).
- Step response calculation endpoint normalizes CV deltas by detected MV step size, resamples to controller sample time, and returns raw + fitted response arrays for visualization and export.

## Simulator & OPC UA Server
- `virtual_plant` simulates Debutanizer + CSTR and writes OPC UA tags, initializing PID modes to 3 (Remote Cascade) for control; see [virtual_plant/src/main.rs](virtual_plant/src/main.rs).
- `opcua_server` builds a UA server and exposes a hot-reload API to load YAML model nodes at runtime; see [opcua_server/src/main.rs](opcua_server/src/main.rs).

## Collaboration Preferences (User-Specific)
- The user runs all terminal commands manually in fish.
- Do **not** execute terminal commands on the user's behalf.
- When command-line validation/build/test steps are needed, provide the exact commands for the user to run and wait for their output.
