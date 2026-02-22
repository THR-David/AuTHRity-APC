# APC Engine Overview (MPC/DMC)

This document explains how `apc_engine` implements DMC/MPC at runtime.

## Why this exists

- `DCS_ADAPTER_GUIDE.md` explains integration and mode mapping.
- This file explains the control concept and how the engine executes it each scan.

---

## MPC/DMC in one page

DMC (Dynamic Matrix Control) is a model-predictive control method.

At each control scan, the controller:
1. Predicts future CV behavior over a **prediction horizon** ($P$).
2. Optimizes a sequence of MV moves over a **control horizon** ($M$), with constraints and weights.
3. Applies **only the first move** to the plant.
4. Repeats next scan with fresh measurements.

This is called **receding horizon control**.

---

## Receding horizon: “optimize many, apply one”

- The optimizer computes a future MV plan (multiple steps).
- The engine sends only the first control action to the DCS/PLC.
- Next scan, it re-solves with updated PV/MV/DV/mode/limit values.

Why this is correct:
- New disturbances and model mismatch are handled every scan.
- Constraint handling remains consistent with latest plant state.
- The future plan is useful for visualization but is not fully pre-committed to actuation.

---

## What `apc_engine` does each scan

High-level runtime sequence:

1. Read live data from OPC UA:
   - CV PV/Target
   - MV feedback and mode nodes
   - DV values
   - Dynamic limits and tuning nodes
   - Operating mode
2. If mode is not Idle, validate inputs and call `DmcController::next_move(...)`.
3. Validate MV mode gates (Remote Cascade policy) and limits.
4. If engage mode, write first MV setpoint move.
5. Publish diagnostics:
   - predictions
   - future plan
   - solver status
   - heartbeat / execution timing

---

## Modes and write behavior

System operating mode is evaluated each scan.

- Mode `0` (Idle): no control action.
- Mode `1` (Monitor): compute/predict, but do not write MV setpoints.
- Mode `2` (Engage): compute and apply first MV move (subject to MV mode and safety checks).

Important implementation detail:
- In Monitor mode, the controller runs with `commit = false`, so internal prediction state is intentionally cleared each scan (cold-start behavior next scan).
- In Engage mode, the controller runs with `commit = true`, so prediction memory is retained and shifted forward scan-to-scan.

Per-MV write gating is also enforced:
- MPC setpoint writes are intended for Remote Cascade operation.
- Bumpless transfer setpoint is only sent in Engage mode when `ModeTarget == 3` but actual mode is not yet `3`.

Failure handling in runtime loop:
- Invalid inputs (NaN/Inf or out-of-range critical checks) skip the scan and set solver status to input error.
- If solver status is infeasible/timeout, MV writes are skipped for that scan.
- Repeated failures auto-demote OperatingMode (Engage → Monitor, then Monitor/Engage → Idle at higher failure count).

---

## Prediction horizon vs control horizon

- **Prediction horizon (`P`)**: how far ahead CVs are predicted.
- **Control horizon (`M`)**: how many future MV increments are optimized.
- Typical MPC pattern: $P \ge M$.

The engine stores/publishes both:
- First move used for real actuation.
- Future plan and CV predictions for operator/engineer visibility.

---

## Constraint and weighting behavior

The optimization includes practical bounds and penalties:
- MV absolute limits
- MV move-size/rate limits
- CV error weighting
- MV move suppression weighting

Live OPC values are treated as runtime truth for limits/tuning where configured, enabling online retuning without rebuilding binaries.

---

## Model types supported

`apc_engine` supports:
- `parametric`: step response generated from FOPDT-style parameters.
- `step_response`: explicit coefficient arrays from model data.

Both feed the same scan-by-scan receding-horizon optimization behavior.

---

## Common misconception

**“Why compute many moves if only one is applied?”**

Because the first move is only optimal relative to anticipated future behavior and constraints. The rest of the plan provides that context. On the next scan, the full optimization is recalculated with new measurements.

That is the core MPC mechanism.

---

## Related docs

- `apc_engine/DCS_ADAPTER_GUIDE.md`
- `SYSTEM_OVERVIEW.md`
- `apc_engine/src/dmc.rs`
- `apc_engine/src/main.rs`
