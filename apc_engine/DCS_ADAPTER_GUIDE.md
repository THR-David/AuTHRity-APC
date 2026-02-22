# DCS Adapter Guide for AuTHRity MPC System

This document describes the generic MPC mode protocol and required OPC UA nodes used by `apc_engine`.

---

## Generic MPC Protocol

The MPC system uses a **vendor-neutral mode protocol**. DCS-specific adapters must translate native modes to the values below.

### MPC Mode Values

| Value | Name | Description |
|-------|------|-------------|
| **0** | Operator Control | Manual, Initialization Manual, Out of Service |
| **1** | Local Auto | PID controls to local setpoint |
| **2** | Cascade | PID accepts setpoint from upstream controller |
| **3** | Remote Cascade | PID accepts setpoint from MPC ← **MPC writes here** |

### Required OPC UA Nodes (per MV)

For each Manipulated Variable, provide these nodes (names are examples):

```yaml
MVName:PV           # Process measurement (Double)
MVName:SP           # Setpoint input for MPC (Double)
MVName:OP           # Controller output to field (Double)
MVName:Mode         # Current mode 0-3 (Int32)
MVName:ModeTarget   # Requested mode 0-3 (Int32)
MVName:FuturePlan   # Control horizon visualization (Double[])
MVName:HighLimit    # MV high limit
MVName:LowLimit     # MV low limit
MVName:HighHighLimit# MV high-high limit (safety)
MVName:LowLowLimit  # MV low-low limit (safety)
```

---

## Bumpless Transfer Handshake

The MPC implements a standard bumpless transfer sequence:

1. **Operator Action**: Sets `ModeTarget = 3` (Remote Cascade)
2. **MPC Response**: Writes current `OP` value to `SP` (no bump)
3. **DCS Response**: Transitions `Mode → 3`
4. **MPC Control**: Begins sending calculated moves

**Timeline:**
```
Scan 1: Mode=1, Target=3  → MPC writes bumpless SP
Scan 2: Mode=3, Target=3  → MPC writes first controller move
Scan 3+: Normal operation
```

---

## OPC DA to OPC UA Bridge (Placeholder)

If your DCS does not include an OPC UA client (or licensing is costly), an OPC DA → OPC UA bridge can be used to expose tags over OPC UA.

Placeholder repo link: <ADD-OPCDA-OPCUA-BRIDGE-REPO-URL>
