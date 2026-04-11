# Keyboard Capture Spec

## Overview

kmmon captures keyboard activity from Linux evdev devices with a **privacy-first design**: raw key identities are never logged or stored. Only the timing of keystrokes is retained, long enough to compute an approximate words-per-minute rate.

## Privacy Design

1. `EV_KEY` events are read from the device
2. **Key code is immediately discarded** — only the timestamp (`Instant::now()`) is kept
3. Timestamps older than 60 seconds are purged from memory
4. No key identity, no key sequence, no text reconstruction is possible from the output

## Device Discovery

A device is treated as a keyboard if it supports `EV_KEY` with at least standard letter keys (e.g. `KEY_A`). This excludes game controllers and media remotes that only have a handful of buttons.

## WPM Algorithm

The rolling-window approach:

```
keystrokes_per_minute = count of timestamps in the last 60 seconds
approx_wpm = keystrokes_per_minute / 5
```

The divisor of 5 is the standard assumption: an average English word is ~5 characters. This is the same formula used by most WPM tests (e.g. monkeytype, 10fastfingers).

Only key-down events (`value == 1`) are counted. Key-repeat events (`value == 2`) and key-up events (`value == 0`) are ignored.

## Output Topic

### `/keyboard/activity`

Emitted at **1 Hz** regardless of activity.

```json
{
  "keystrokes_per_minute": 250,
  "approx_wpm": 50.0,
  "active": true
}
```

| Field                   | Type   | Description                                           |
|-------------------------|--------|-------------------------------------------------------|
| `keystrokes_per_minute` | `u32`  | Number of key-down events in the past 60 seconds      |
| `approx_wpm`            | `f32`  | `keystrokes_per_minute / 5.0`                         |
| `active`                | `bool` | `true` if any keystroke occurred in the past 60 seconds |

JSON Schema encoding: `jsonschema`

## Limitations

- WPM is a rolling average, not instantaneous — it lags behind bursts
- Multiple keyboards on the same system are aggregated into one stream
- The approximation assumes typical English prose; code/symbol-heavy typing will read lower than actual WPM
