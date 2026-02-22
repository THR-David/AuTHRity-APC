# AuTHRity System Overview (code-aligned)

This document reflects the current implementation across `hmi`, `controller_host`, `apc_engine`, `opcua_server`, and `virtual_plant`.

---

## Architecture Overview

```
┌─────────────────┐      OPC UA       ┌──────────────────┐
│     DCS/PLC     │◄────────────────► │  OPC UA Server   │
└─────────────────┘                   └───────────────┬──┘
                                         ▲            │
                                  OPC UA │            │ OPC UA
                                         ▼            │
┌─────────────────┐           ┌──────────────────┐    │
│ Controller Host │──────────►│    APC Engine    │    │
│  (Supervisor)   │   spawns  │  (Child Process) │    │
└─────────────────┘           └──────────────────┘    │
         ▲                                            │
         │ HTTP (Control API)                         │
         │                                            │
┌────────┴───────────────────────────────┐            │
│         HMI Backend (Rust)             │◄───────────┘
│  • WebSocket Server                    │
│  • OPC UA Bridge                       │◄────►┌───────────────────────┐
│  • Auth/RBAC + Admin APIs              │      │       QuestDB         │
└────────────────────────────────────────┘      │  • Process Historian  │
         ▲                                      │  • Trends             │
         │ WebSocket/HTTP                       │  • Step response      │
         ▼                                      └───────────────────────┘
┌────────────────────────────────────────┐
│       HMI Frontend (React/TS)          │
│  • Login + RBAC UI                     │
│  • Model builder / tuning / deploy     │
│  • Real-time visualization             │
└────────────────────────────────────────┘
```

---

## Service Responsibilities

## `controller_host` (Supervisor)
- Hosts controller lifecycle API (`/api/controllers`, `/start`, `/stop`, `/config`, `/models/...`, `/api/deploy`).
- Stores deployed model files by controller folder/version.
- Spawns `apc_engine` as child process with runtime CLI args.
- Enforces API-key middleware on control routes.
- Exposes health endpoint (`/api/health`).

## `apc_engine` (Controller runtime)
- Loads `UnifiedModel` JSON.
- Runs periodic DMC cycle: reads OPC UA values, solves constrained moves, writes outputs in engage mode.
- Honors operating mode and per-MV mode gates (remote cascade logic).
- Supports model save trigger path (`save_configuration`) for runtime snapshots.
- Reads live tuning/limits from OPC UA each cycle (JSON is baseline, OPC is live truth).

## `hmi` backend (Rust/Axum)
- Serves frontend assets and API routes.
- Bridges OPC UA updates to browser via WebSocket broadcast.
- Accepts browser write commands and forwards to OPC worker command channel.
- Proxies deployment and controller lifecycle calls.
- Hosts auth/session/RBAC + admin user/audit APIs.
- Runs historian task for QuestDB logging and trend/step-response data endpoints.

## `hmi` frontend (React)
- Login/session bootstrap (`/api/auth/me`) and role-gated experience.
- Real-time process views over `/ws`.
- Model Generator with parametric + step-response workflows.
- Step Response Tool and trend tooling through backend APIs.
- Settings view for infrastructure, user management, and audit events.

## `opcua_server`
- Hosts/serves node maps and OPC UA process namespace.
- Supports model node hot-reload workflow used by deployment bundle path.

## `virtual_plant`
- Simulates process behavior and writes tags to OPC UA for testing/training.

---

## HMI API Surface (Current)

### Authentication / Session
- `POST /api/auth/login`
- `POST /api/auth/logout`
- `GET /api/auth/me`
- `POST /api/auth/change-password`

### Admin / RBAC
- `GET /api/admin/audit`
- `GET|POST /api/admin/users`
- `POST /api/admin/users/:id/role`
- `POST /api/admin/users/:id/disable`
- `POST /api/admin/users/:id/password`

### Runtime / Integration
- `GET /ws`
- `GET /api/models`
- `GET /api/model?file=...`
- `GET /api/controller/model`
- `GET /api/controller/:controller_id/models/:model_filename`
- `GET /api/trends`
- `GET /api/stepresponse/data`
- `GET /api/stepresponse/tags`
- `POST /api/stepresponse/calculate`
- `POST /api/deploy`
- `GET|POST /api/infrastructure`
- `GET /api/physics/:id`
- `GET /api/prox/controllers`
- `POST /api/prox/controllers/:id/start`
- `POST /api/prox/controllers/:id/stop`

---

## Auth & RBAC Model (Current)

### Roles
- Viewer
- Operator
- Engineer
- Admin

### Permission Map
- `read:view`
- `runtime:write_basic`
- `runtime:write_limits_operational`
- `runtime:write_limits_safety`
- `model:deploy`
- `controller:lifecycle`
- `infra:write`
- `user:manage`
- `audit:read`

### Enforcement
- Backend is the security boundary.
- HTTP routes enforce role/permission checks server-side.
- WebSocket requires authenticated session; WRITE is checked per-message.
- CSRF is required for state-changing HTTP endpoints.
- Session invalidation uses `auth_version` (role/disable/password/reset effects).

### Current runtime-write policy
- Viewer: read-only.
- Operator: basic writes + operational limits (`LOW/HIGH`).
- Engineer/Admin: includes safety limit writes (`LOWLOW/HIGHHIGH`) and advanced operations.

---

## Data Flow (Operational)

## 1) Deployment flow
1. Frontend sends deploy bundle to HMI (`/api/deploy`).
2. HMI forwards:
   - YAML node map to OPC server hot-reload API.
   - JSON model to Supervisor deploy API.
3. Supervisor stores model file under controller model repo.
4. Frontend starts controller via HMI proxy (`/api/prox/controllers/:id/start`).
5. Supervisor spawns `apc_engine` with selected model + OPC/PKI args.

## 2) Control cycle flow
1. Engine reads PV/Targets/MV/DV/Modes/Limits from OPC UA.
2. If mode > 0, engine computes next move.
3. In engage mode, writes are gated by MV mode checks.
4. Engine writes predictions/future plan/status and approved outputs.

## 3) HMI real-time flow
1. OPC worker reads/subscribes nodes.
2. Worker publishes `OpcUpdate` to broadcast channel.
3. WS clients receive updates and refresh UI.
4. UI write actions send WS `WRITE` messages.
5. Backend checks permission and forwards approved writes to OPC worker command channel.

## 4) Historian flow
1. HMI historian consumes OPC updates from broadcast stream.
2. Writes process history to QuestDB.
3. Trend/step-response endpoints query historian data for frontend tools.

---

## Model Sources and Loading

## HMI model/node source
- `GET /api/models`: uses configured remote OPC server (`config/hosts.json`) as model source.
- `GET /api/model?file=...`: fetches node map from remote OPC server (`/api/nodes/{model}`).
- No local fallback model source is used in this flow.

## Physics/model retrieval
- HMI can fetch active controller model and specific model file content via supervisor-proxy routes.

## Engine model handling
- Supports `parametric` and `step_response` model types.
- Runtime save path exists to persist current tuned configuration snapshots.

---

## HMI Frontend State Notes

- OPC tag values and pending writes are managed with Zustand store.
- Pending write entries are cleared on observed node update acknowledgment.
- Write UX includes role-based denial messaging in modal paths.
- Login/account UX includes self-service password change and logout menu.

---

## Security and Operations Notes

- Current HMI runtime is HTTP for local/dev; production should use HTTPS/WSS at edge/reverse proxy.
- Auth cookies are configured secure/samesite in session layer.
- First-run seed behavior creates default admin user only when user store is empty.
- Forced password change is enforced for seeded/reset credentials.

---

## File Anchors

- HMI app/bootstrap/routes: `hmi/src/main.rs`
- HMI auth/RBAC: `hmi/src/auth.rs`
- HMI route handlers + WS authorization: `hmi/src/web_routes.rs`
- HMI OPC worker: `hmi/src/opc_worker.rs`
- HMI shared state: `hmi/src/state.rs`
- Supervisor: `controller_host/src/main.rs`
- APC engine loop/save path: `apc_engine/src/main.rs`

---

## Related Docs

- `README.md`
- `CONFIGURATION_GUIDE.md`
- `hmi/QUESTDB_SETUP.md`
- `roadmap/hmi-rbac-implementation-plan.md`
