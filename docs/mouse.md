# Mouse Capture Spec

## Overview

kmmon captures mouse position and scroll wheel events from Linux evdev devices and publishes them as MCAP-compatible messages over a Foxglove WebSocket.

## Device Discovery

Devices are discovered by scanning `/dev/input/event*` via `evdev::enumerate()`. A device is treated as a mouse/pointer if it supports:

- `EV_REL` with `REL_X` (standard USB/Bluetooth mouse), OR
- `EV_ABS` with `ABS_X` (touchpad, drawing tablet)

## Event Sources

| Source      | Event type | Codes                          |
|-------------|-----------|--------------------------------|
| Mouse move  | `EV_REL`  | `REL_X`, `REL_Y`               |
| Touchpad    | `EV_ABS`  | `ABS_X`, `ABS_Y`               |
| Scroll      | `EV_REL`  | `REL_WHEEL` (dy), `REL_HWHEEL` (dx) |

Relative moves are batched per `EV_SYN` sync frame so that X and Y deltas in the same frame are emitted together.

## Output Topics

### `/mouse/position`

Throttled at **20 Hz** (every 50 ms), only emitted when the position has changed.

```json
{
  "x": 1234,
  "y": 567
}
```

| Field | Type  | Description                                      |
|-------|-------|--------------------------------------------------|
| `x`   | `i32` | Absolute X coordinate (accumulated relative moves or direct ABS_X) |
| `y`   | `i32` | Absolute Y coordinate (accumulated relative moves or direct ABS_Y) |

JSON Schema encoding: `jsonschema`

### `/mouse/scroll`

Emitted per-event (not throttled).

```json
{
  "dx": 0,
  "dy": -3
}
```

| Field | Type  | Description              |
|-------|-------|--------------------------|
| `dx`  | `i32` | Horizontal scroll delta  |
| `dy`  | `i32` | Vertical scroll delta (negative = down on most systems) |

JSON Schema encoding: `jsonschema`

### `/mouse/activity`

Rolling-window aggregate designed to be directly comparable to
`/keyboard/activity` — both report a scalar "how busy is the user"
signal at 1 Hz.

Emitted at **1 Hz**. Suppressed while continuously idle; re-emitted on
state transitions (idle → active, active → idle) so the timeline
faithfully shows periods of no movement.

```json
{
  "pixels_per_second": 412.3,
  "active": true
}
```

| Field                | Type  | Description                                    |
|----------------------|-------|------------------------------------------------|
| `pixels_per_second`  | `f64` | Cursor distance travelled in the last 60 s, divided by 60. Euclidean distance (`sqrt(dx² + dy²)`) summed across every relative-move event in the window. |
| `active`             | `bool`| `true` while `pixels_per_second > 0`, i.e. there was at least one move event in the window. |

Window: **60 seconds**, matching `/keyboard/activity` so both signals
respond on the same timescale.

Only relative moves (`EV_REL`) contribute. Absolute pointer devices
(touchpads, tablets) currently do not drive this metric; adding them
would require computing step-wise deltas from consecutive `ABS_X`/
`ABS_Y` values and is left for a future change.

JSON Schema encoding: `jsonschema`

## Privacy

Mouse position and scroll data are geometric coordinates only — no application context, window titles, or UI element information is captured or logged.
